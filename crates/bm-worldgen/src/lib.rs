//! Planet generation.
//!
//! Every chunk is a pure function of `(seed, planet shape, config, chunk
//! position)`: any chunk can be generated alone, in any order, on any thread,
//! with no neighbours. Features that cross chunk borders (trees, caves) are
//! decided from world-aligned functions that every chunk evaluates the same
//! way, so a tree cut by a chunk border is still one tree.
//!
//! Generation runs on the **server only** (ADR-0003): ore placement is part of
//! the economy, and a client that could generate terrain could see every vein
//! on the planet.

pub mod biomes;
pub mod kinds;
pub mod noise;
pub mod trees;

pub use biomes::{BiomeCatalogue, BiomeChoice, BiomeSpec};
pub use kinds::{KindError, PlanetKind, PlanetKinds};

use bm_core::{
    BlockId, BlockPos, BlockRegistry, BlockState, CHUNK_SIZE, CHUNK_VOLUME, Chunk, ChunkPos, Colour, ColourPalette,
    LocalPos, PlanetShape, RegistryError,
};
use noise::{Fbm, hash01, hash3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Bump on ANY change to what the generator produces for a given seed, and
/// re-pin `terrain_fingerprint_is_pinned` in the same commit. A server refuses
/// to open a planet made by another version (its edited chunks were saved
/// against terrain that would no longer be there).
///
/// Version 2 (M4) added planet kinds. The default settings still produce
/// version 1's terrain block for block (`default_settings_reproduce_version_1`),
/// so a version 1 planet is a version 2 planet with default settings.
///
/// Version 3 (M6) colours natural blocks from the palette
/// ([`PlanetGenConfig::colours`]). Settings with no colours, which is what a
/// version 2 planet's saved settings read as, still produce version 2's
/// terrain block for block (`no_colours_reproduce_version_2`): see
/// [`opens_generator_version`].
///
/// Version 4 (M19) adds biomes ([`PlanetGenConfig::biomes`]). Settings with
/// no biomes, which is what every version 3 planet's saved settings read as,
/// still produce version 3's terrain block for block
/// (`no_biomes_reproduce_version_3`).
pub const GENERATOR_VERSION: u32 = 4;

/// Whether this generator reproduces, block for block, the terrain of a
/// planet saved by generator `version` from that planet's saved settings, so
/// the planet (and every edit saved against its terrain) can be opened.
///
/// Version 2's settings have no `colours` field, so they read as no colours,
/// and version 3 with no colours is version 2's terrain: a version 2 planet
/// opens as a version 3 planet that keeps natural colours. Version 3's have
/// no `biomes`, and version 4 with no biomes is version 3's terrain. Version 1 saved
/// its settings in another layout ([`PlanetGenConfigV1`]) and is upgraded by
/// the server's save conversion, not here.
pub fn opens_generator_version(version: u32) -> bool {
    version == GENERATOR_VERSION || version == 3 || version == 2
}

/// The most palette colours a planet uses for one block: a main colour and
/// up to three regional variants.
pub const MAX_BLOCK_COLOURS: usize = 4;

/// The shared colour palette (`assets/colours.toml`) that colour names in
/// planet settings resolve against. Ids are positions and only ever appended
/// (ADR-0007), so this is the palette every server and client holds.
pub fn palette() -> &'static ColourPalette {
    static PALETTE: OnceLock<ColourPalette> = OnceLock::new();
    PALETTE.get_or_init(ColourPalette::builtin)
}

/// Per-planet generation settings, saved with the planet. A planet kind in
/// `assets/planets.toml` supplies them (`[kind.generation]`, any field left
/// out keeps its default) when a planet is created. The defaults are the
/// temperate world.
///
/// Blocks are named, not numbered, so a saved planet does not depend on the
/// catalogue's order; [`PlanetGenConfig::check`] confirms every name exists.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanetGenConfig {
    pub seed: u64,
    /// Terrain is shaped around this height: shores, sea bed, snow line.
    pub sea_level: i64,
    /// Height above sea level where `ground` gives way to `snow`.
    pub snow_line: i64,
    /// Peak mountain height above the land base.
    pub mountain_height: f64,
    /// Fraction of suitable columns that grow a tree.
    pub tree_density: f64,
    /// Height above sea level above which no tree grows.
    pub tree_line: i64,
    /// Trunk height is `trunk_min` plus `0..trunk_extra` blocks.
    pub trunk_min: i64,
    pub trunk_extra: i64,
    /// Water fills hollows up to `sea_level + water_offset`; a negative offset
    /// leaves the shallow basins dry and only the deepest ones wet.
    pub water_offset: i64,
    /// How many of the topmost water layers are frozen to `ice`.
    pub ice_depth: i64,

    /// Top block of flat land below the snow line.
    pub ground: String,
    /// The few blocks under `ground` and `snow`.
    pub subsoil: String,
    /// Top block of flat land above the snow line.
    pub snow: String,
    /// Top block of dry land just above sea level.
    pub shore: String,
    /// Top block of the sea bed near the shore.
    pub shallows: String,
    /// Top block of the deep sea bed, picked per column from these.
    pub seabed: Vec<String>,
    /// Top block of steep slopes (rock shows through).
    pub cliff: String,
    pub water: String,
    pub ice: String,
    /// A rock layer under the topsoil (and under cliffs), `layer_depth`
    /// blocks thick plus up to three more; 0 for none.
    pub layer: String,
    pub layer_depth: i64,

    /// Chance per rock block under a cave's floor of a glowing crystal, below
    /// `glow_depth` under sea level.
    pub glow_chance: f64,
    pub glow_depth: i64,
    /// A lower sea (milestone 18 part 3): open cave cells at or below this
    /// height fill with this block (Boundless's lava, or water, at altitude
    /// 16). None: caves stay open.
    pub lower_sea: Option<LowerSea>,
    /// Ores in rock, tried in order; the first that matches a block wins.
    /// Each ore's veins come from its own noise, keyed by its place in this
    /// list, so add new ores at the end.
    pub ores: Vec<OreSpec>,
    /// Gatherable objects (M11: plants, boulders and fungi, floor props),
    /// tried in order at each candidate cell; the first that grows there
    /// wins. Empty (planets made before M11, and the default) is none; the
    /// server appends a kind's list to an older planet on opening, as it
    /// does new ores, so only chunks nobody saved gain them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub flora: Vec<FloraSpec>,

    /// Natural colours (ADR-0007): block name to palette colour names, the
    /// planet's main colour for that block first, then up to three regional
    /// variants that take over in patches hundreds of blocks across. Chosen
    /// once, when the planet is created, from its kind's choices
    /// ([`PlanetKind::generation`]). A block not listed keeps its natural
    /// colour; empty (planets made before version 3, and the default) is
    /// natural everywhere. Only colourable blocks may be listed.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub colours: BTreeMap<String, Vec<String>>,

    /// The planet's biomes (milestone 19, [`biomes`]): drawn from the
    /// catalogue when the planet is created and saved here. Empty (planets
    /// made before generator 4, and the default) is version 3's single
    /// landscape, block for block.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub biomes: Vec<BiomeSpec>,
    /// Biome cells are about this many blocks across (a **guess**: Boundless's
    /// were about 480 on worlds three times as wide).
    pub biome_size: f64,
    /// The planet's tier for its caves (milestone 19 part 3): 0 is version
    /// 3's caves; 1 and up add flat caves, from 2 linings, from 3 rooms and
    /// crooked tunnels, from 5 great halls and tall caves, from 6 shafts
    /// and pillars (Boundless's cave sets by tier).
    #[serde(skip_serializing_if = "is_zero")]
    pub cave_tier: u8,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

/// A lower sea: what open caves fill with, up to which height.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LowerSea {
    pub level: i64,
    pub block: String,
}

/// Why planet settings cannot be generated.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(transparent)]
    Block(#[from] RegistryError),
    #[error("colours for `{0}`: that block keeps its natural colour (not colourable)")]
    NotColourable(String),
    #[error("colours for `{block}`: there is no palette colour named `{colour}`")]
    UnknownColour { block: String, colour: String },
    #[error("colours for `{0}`: {1} colours listed; a kind lists at least 1 per block, a planet 1 to 4")]
    ColourCount(String, usize),
    #[error("flora `{0}`: a gatherable object must be a prop (not solid)")]
    FloraSolid(String),
    #[error("flora `{0}`: lists no ground (`on`) to grow on")]
    FloraGround(String),
    #[error("biome `{0}`: lists no cover")]
    NoCover(String),
}

/// One ore in a planet's rock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OreSpec {
    pub block: String,
    /// Approximate vein size in blocks.
    pub size: f64,
    /// Noise threshold: a vein is where the noise exceeds it. Lower is
    /// richer (0.52 is plentiful, 0.62 scarce).
    pub threshold: f64,
    /// World heights it occurs at, inclusive.
    pub min_y: i64,
    pub max_y: i64,
    /// How far under the surface it starts: never within `min_depth` blocks
    /// of the column's top.
    pub min_depth: i64,
    /// The deepest it goes under the column's top (milestone 18).
    pub max_depth: i64,
    /// Only under columns with this much water over them, inclusive
    /// (milestone 18: emerald under deep water, amethyst at shallow edges).
    pub water_depth: Option<[i64; 2]>,
    /// With `water_depth`: dry columns within this many blocks (at most 4)
    /// of such water count too (amethyst on the shore).
    pub near_water: i64,
    /// Only under columns whose top is this far above sea level, inclusive
    /// (milestone 18: sapphire in hills, topaz and diamond in mountains).
    pub land_height: Option<[i64; 2]>,
    /// Only under columns topped by one of these blocks (milestone 18: ruby
    /// under sand). Empty: any.
    pub under: Vec<String>,
}

impl Default for OreSpec {
    fn default() -> Self {
        Self {
            block: String::new(),
            size: 12.0,
            threshold: 0.6,
            min_y: 1,
            max_y: i64::MAX,
            min_depth: 0,
            max_depth: i64::MAX,
            water_depth: None,
            near_water: 0,
            land_height: None,
            under: Vec::new(),
        }
    }
}

/// One gatherable object a planet grows: a floor prop (plant, boulder or
/// fungus, `assets/blocks.toml`) standing in the air cell above a matching
/// ground block. Where each grows is Boundmore's guess (Boundless's
/// placement data is not published); every number here is a knob for
/// `assets/planets.toml`.
///
/// Each candidate cell rolls a hash of the seed, the object's name and the
/// cell, so placement is deterministic and an object's spots do not move
/// when others are added to the list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FloraSpec {
    pub block: String,
    /// Ground blocks it grows on (the block right under it).
    pub on: Vec<String>,
    /// Chance per matching ground cell (0.002 is about one per 500).
    pub density: f64,
    /// On cave floors underground instead of on the surface.
    pub cave: bool,
    /// For cave flora: how far below the column's surface the cell it stands
    /// in lies, inclusive. At least 1.
    pub min_depth: i64,
    pub max_depth: i64,
    /// World heights the cell it stands in may be at, inclusive.
    pub min_y: i64,
    pub max_y: i64,
    /// Only within [`NEAR_WATER`] columns of open water (shores).
    pub near_water: bool,
    /// Patches: when above 0, the size in blocks of a noise whose high
    /// ground alone grows it (above `patch_threshold`). 0 grows it
    /// everywhere its ground is.
    pub patch: f64,
    pub patch_threshold: f64,
}

impl Default for FloraSpec {
    fn default() -> Self {
        Self {
            block: String::new(),
            on: Vec::new(),
            density: 0.002,
            cave: false,
            min_depth: 1,
            max_depth: i64::MAX,
            min_y: 1,
            max_y: i64::MAX,
            near_water: false,
            patch: 0.0,
            patch_threshold: 0.0,
        }
    }
}

/// How many columns away open water may be for flora that grows by it.
pub const NEAR_WATER: i64 = 2;

impl Default for PlanetGenConfig {
    fn default() -> Self {
        Self {
            seed: 1,
            sea_level: 128,
            snow_line: 112,
            mountain_height: 130.0,
            tree_density: 0.012,
            tree_line: 100,
            trunk_min: 5,
            trunk_extra: 4,
            water_offset: 0,
            ice_depth: 0,
            ground: "turf".into(),
            subsoil: "soil".into(),
            snow: "snow".into(),
            shore: "sand".into(),
            shallows: "sand".into(),
            seabed: vec!["gravel".into(), "sand".into()],
            cliff: "stone".into(),
            water: "water".into(),
            ice: "ice".into(),
            layer: "sandstone".into(),
            layer_depth: 0,
            glow_chance: 0.02,
            glow_depth: 24,
            lower_sea: None,
            ores: vec![
                OreSpec {
                    block: "coal_seam".into(),
                    size: 14.0,
                    threshold: 0.52,
                    min_y: 13,
                    max_y: i64::MAX,
                    min_depth: 6,
                    ..Default::default()
                },
                OreSpec {
                    block: "copper_vein".into(),
                    size: 12.0,
                    threshold: 0.58,
                    min_y: 1,
                    max_y: 149,
                    min_depth: 0,
                    ..Default::default()
                },
            ],
            flora: Vec::new(),
            colours: BTreeMap::new(),
            biomes: Vec::new(),
            biome_size: 320.0,
            cave_tier: 0,
        }
    }
}

impl PlanetGenConfig {
    /// These settings without biomes: version 3's single landscape (the
    /// older tests check that path, which planets saved before generator 4
    /// still take).
    #[cfg(test)]
    fn single(self) -> Self {
        Self {
            biomes: Vec::new(),
            cave_tier: 0,
            ..self
        }
    }

    /// Every block the settings name exists, and every colour is a palette
    /// colour on a colourable block.
    pub fn check(&self, blocks: &BlockRegistry) -> Result<(), ConfigError> {
        Ids::resolve(self, blocks)?;
        for b in &self.biomes {
            for name in b.blocks() {
                blocks.id(name)?;
            }
            if b.cover.is_empty() {
                return Err(ConfigError::NoCover(b.name.clone()));
            }
        }
        Flora::resolve(self, blocks, 1)?;
        BlockColours::resolve(&self.colours, blocks).map(|_| ())
    }
}

/// [`PlanetGenConfig`] as generator version 1 saved it (postcard is
/// positional, so a save holding this layout must be read with it). Converts
/// to the version 2 settings that produce the same terrain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanetGenConfigV1 {
    pub seed: u64,
    pub sea_level: i64,
    pub snow_line: i64,
    pub mountain_height: f64,
    pub tree_density: f64,
}

impl From<PlanetGenConfigV1> for PlanetGenConfig {
    fn from(v: PlanetGenConfigV1) -> Self {
        Self {
            seed: v.seed,
            sea_level: v.sea_level,
            snow_line: v.snow_line,
            mountain_height: v.mountain_height,
            tree_density: v.tree_density,
            // Version 1 stopped trees 12 below the snow line.
            tree_line: v.snow_line - 12,
            ..Default::default()
        }
    }
}

struct Ore {
    block: BlockId,
    noise: Fbm,
    threshold: f64,
    min_y: i64,
    max_y: i64,
    min_depth: i64,
    max_depth: i64,
    water_depth: Option<[i64; 2]>,
    near_water: i64,
    land_height: Option<[i64; 2]>,
    under: Vec<BlockId>,
}

impl Ore {
    /// Whether the column (its top `h`, top block `surface`) may hold this
    /// ore; `height_at` reads neighbouring tops for the shore rule. Every
    /// rule is off by default, so ores without them pass everywhere.
    fn column_allows(
        &self,
        h: i64,
        surface: BlockId,
        sea: i64,
        water_level: i64,
        height_at: impl Fn(i64, i64) -> i64,
    ) -> bool {
        if let Some([lo, hi]) = self.land_height
            && !(lo..=hi).contains(&(h - sea))
        {
            return false;
        }
        if !self.under.is_empty() && !self.under.contains(&surface) {
            return false;
        }
        if let Some([lo, hi]) = self.water_depth {
            let wet = |h: i64| (lo..=hi).contains(&(water_level - h));
            if !wet(h) {
                let r = self.near_water.clamp(0, TREE_REACH + 1);
                if r == 0 || water_level - h > 0 {
                    return false;
                }
                let near = (-r..=r).step_by(2.max(r as usize / 2)).any(|dz| {
                    (-r..=r)
                        .step_by(2.max(r as usize / 2))
                        .any(|dx| (dx, dz) != (0, 0) && wet(height_at(dx, dz)))
                });
                if !near {
                    return false;
                }
            }
        }
        true
    }
}

/// A [`FloraSpec`] resolved to ids, with its patch noise.
struct Flora {
    block: BlockId,
    on: Vec<BlockId>,
    density: f64,
    cave: bool,
    min_depth: i64,
    max_depth: i64,
    min_y: i64,
    max_y: i64,
    near_water: bool,
    /// Hash key: the seed mixed with the object's name.
    key: u64,
    patch: Option<(Fbm, f64)>,
}

impl Flora {
    fn resolve(cfg: &PlanetGenConfig, r: &BlockRegistry, width: i64) -> Result<Vec<Self>, ConfigError> {
        cfg.flora
            .iter()
            .map(|f| {
                let block = r.id(&f.block)?;
                if r.get(block).solid {
                    return Err(ConfigError::FloraSolid(f.block.clone()));
                }
                if f.on.is_empty() {
                    return Err(ConfigError::FloraGround(f.block.clone()));
                }
                let key = cfg.seed ^ name_hash(&f.block) ^ 0xf104a;
                Ok(Self {
                    block,
                    on: f.on.iter().map(|n| r.id(n)).collect::<Result<_, _>>()?,
                    density: f.density,
                    cave: f.cave,
                    min_depth: f.min_depth.max(1),
                    max_depth: f.max_depth,
                    min_y: f.min_y,
                    max_y: f.max_y,
                    near_water: f.near_water,
                    key,
                    patch: (f.patch > 0.0).then(|| (Fbm::new(key, width, f.patch, 2, 0.5), f.patch_threshold)),
                })
            })
            .collect()
    }
}

struct Ids {
    air: BlockId,
    bedrock: BlockId,
    stone: BlockId,
    deep_stone: BlockId,
    trunk: BlockId,
    foliage: BlockId,
    glow: BlockId,
    ground: BlockId,
    subsoil: BlockId,
    snow: BlockId,
    shore: BlockId,
    shallows: BlockId,
    seabed: Vec<BlockId>,
    cliff: BlockId,
    water: BlockId,
    ice: BlockId,
    layer: BlockId,
    ores: Vec<BlockId>,
    /// The lower sea's height and block, resolved.
    lower: Option<(i64, BlockId)>,
    /// Each ore's `under` blocks, resolved.
    ore_under: Vec<Vec<BlockId>>,
}

impl Ids {
    fn resolve(cfg: &PlanetGenConfig, r: &BlockRegistry) -> Result<Self, RegistryError> {
        if cfg.seabed.is_empty() {
            return Err(RegistryError::Unknown("(empty seabed list)".into()));
        }
        Ok(Self {
            air: r.id("air")?,
            bedrock: r.id("bedrock")?,
            stone: r.id("stone")?,
            deep_stone: r.id("deep_stone")?,
            trunk: r.id("trunk")?,
            foliage: r.id("foliage")?,
            glow: r.id("glow_crystal")?,
            ground: r.id(&cfg.ground)?,
            subsoil: r.id(&cfg.subsoil)?,
            snow: r.id(&cfg.snow)?,
            shore: r.id(&cfg.shore)?,
            shallows: r.id(&cfg.shallows)?,
            seabed: cfg.seabed.iter().map(|n| r.id(n)).collect::<Result<_, _>>()?,
            cliff: r.id(&cfg.cliff)?,
            water: r.id(&cfg.water)?,
            ice: r.id(&cfg.ice)?,
            layer: r.id(&cfg.layer)?,
            ores: cfg.ores.iter().map(|o| r.id(&o.block)).collect::<Result<_, _>>()?,
            lower: cfg
                .lower_sea
                .as_ref()
                .map(|l| r.id(&l.block).map(|b| (l.level, b)))
                .transpose()?,
            ore_under: cfg
                .ores
                .iter()
                .map(|o| o.under.iter().map(|n| r.id(n)).collect::<Result<Vec<_>, _>>())
                .collect::<Result<_, _>>()?,
        })
    }
}

/// Spacing of the world-aligned lattice that caves and ores are sampled on and
/// interpolated from. Divides 32, so chunk corners land on lattice points and
/// the interpolated value is a pure function of world position.
const COARSE: i64 = 4;
const COARSE_N: usize = (CHUNK_SIZE as i64 / COARSE) as usize + 1;
const LATTICE: usize = COARSE_N * COARSE_N * COARSE_N;
/// How far outside a chunk a tree's trunk may stand and still reach into it.
const TREE_REACH: i64 = 3;
/// Least headroom kept for trees above the surface (taller trees raise it).
const TREE_MAX_HEIGHT: i64 = 11;

/// Spacing of the world-aligned lattice the colour regions are sampled on
/// and interpolated from (per column, not per voxel: colour costs nothing
/// below the surface).
const REGION_STEP: i64 = 4;
/// Lattice points per chunk side: one cell of margin on each side so trees
/// whose trunks stand outside the chunk find their column's colour.
const REGION_N: usize = (CHUNK_SIZE as i64 / REGION_STEP) as usize + 3;
/// Size of the largest colour patches, in blocks.
const REGION_FEATURE: f64 = 320.0;
/// Ragged edges: each column's region value is nudged by up to half this, so
/// a border between two colours frays over a few blocks instead of tracing a
/// smooth curve.
const REGION_FRAY: f64 = 0.03;
/// Size of the medium-scale wobble added to both region fields, so a border
/// between two colours wanders and breaks into islands instead of running as
/// a long smooth curve (which reads as a straight line up close).
const REGION_WOBBLE_FEATURE: f64 = 28.0;
/// Its strength, in units of the region fields' spread.
const REGION_WOBBLE: f64 = 0.15;
/// Standard deviation of the two-octave region noise (measured by
/// `region_noise_spread_is_as_assumed`); thresholds below are in units of it.
const REGION_SIGMA: f64 = 0.22;
/// Where a block's main colour gives way to its variants: the normal
/// quantile of 0.6, so the main colour covers about 60% of the planet.
const MAIN_SHARE_Z: f64 = 0.2533;
/// Normal quantiles splitting the variant patches evenly between one, two or
/// three variants.
const VARIANT_SPLITS: [&[f64]; 3] = [&[], &[0.0], &[-0.4307, 0.4307]];

/// One coloured block's palette colours and how its regions are laid out.
#[derive(Clone, Debug)]
struct BlockColour {
    /// Main colour first.
    colours: Vec<Colour>,
    /// A direction mixing the two region fields: each block gets its own, so
    /// turf and stone change colour in different places.
    dir: (f64, f64),
    main_below: f64,
    splits: Vec<f64>,
}

impl BlockColour {
    /// The colour for a column whose region fields read `(a, b)`.
    #[inline]
    fn pick(&self, a: f64, b: f64) -> Colour {
        let (c, s) = self.dir;
        let u = a * c + b * s;
        if u < self.main_below || self.colours.len() == 1 {
            return self.colours[0];
        }
        let w = b * c - a * s;
        let v = self.splits.iter().take_while(|&&t| w >= t).count();
        self.colours[1 + v]
    }
}

/// A planet's natural colours, resolved to ids.
#[derive(Clone, Debug, Default)]
struct BlockColours {
    /// Per block id: index into `blocks`, or `NO_SLOT`.
    slot: Vec<u8>,
    blocks: Vec<BlockColour>,
}

const NO_SLOT: u8 = u8::MAX;

impl BlockColours {
    fn resolve(colours: &BTreeMap<String, Vec<String>>, r: &BlockRegistry) -> Result<Self, ConfigError> {
        let palette = palette();
        let mut out = Self {
            slot: vec![NO_SLOT; r.len()],
            blocks: Vec::new(),
        };
        for (block, names) in colours {
            let id = r.id(block)?;
            if !r.get(id).colourable {
                return Err(ConfigError::NotColourable(block.clone()));
            }
            if names.is_empty() || names.len() > MAX_BLOCK_COLOURS {
                return Err(ConfigError::ColourCount(block.clone(), names.len()));
            }
            let ids = names
                .iter()
                .map(|n| {
                    palette.id(n).ok_or_else(|| ConfigError::UnknownColour {
                        block: block.clone(),
                        colour: n.clone(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            // A direction from the block's name, so the layout does not move
            // when other blocks are added. sqrt is exact in IEEE arithmetic,
            // so this is the same on every machine (no trigonometry).
            let h = name_hash(block);
            let (x, y) = (
                (h & 0xffff) as f64 / 32768.0 - 1.0,
                ((h >> 16) & 0xffff) as f64 / 32768.0 - 1.0,
            );
            let len = (x * x + y * y).sqrt().max(1e-6);
            out.slot[id.0 as usize] = out.blocks.len() as u8;
            out.blocks.push(BlockColour {
                dir: (x / len, y / len),
                main_below: MAIN_SHARE_Z * REGION_SIGMA,
                splits: VARIANT_SPLITS[ids.len().saturating_sub(2).min(2)]
                    .iter()
                    .map(|z| z * REGION_SIGMA)
                    .collect(),
                colours: ids,
            });
        }
        Ok(out)
    }

    fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    #[inline]
    fn slot(&self, id: BlockId) -> Option<usize> {
        match self.slot.get(id.0 as usize) {
            Some(&s) if s != NO_SLOT => Some(s as usize),
            _ => None,
        }
    }
}

/// FNV-1a of a name, then mixed: a stable key that does not depend on any
/// list's order.
fn name_hash(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    hash3(h, 0, 0, 0)
}

/// Picks a new planet's colours from its kind's choices (block name to
/// palette colour names): for each block, a main colour and one to three
/// regional variants, all different, chosen by the planet's seed. A block
/// with one choice gets that colour alone.
pub fn pick_colours(choices: &BTreeMap<String, Vec<String>>, seed: u64) -> BTreeMap<String, Vec<String>> {
    choices
        .iter()
        .filter(|(_, names)| !names.is_empty())
        .map(|(block, names)| {
            let key = name_hash(block) as i64;
            let r = |n: i64| hash3(seed ^ 0xc0_10c4, key, n, 0);
            let want = (2 + (r(0) % 3) as usize).min(names.len());
            // A partial Fisher-Yates shuffle: the first `want` are the picks.
            let mut pool = names.clone();
            for i in 0..want {
                let j = i + (r(1 + i as i64) % (pool.len() - i) as u64) as usize;
                pool.swap(i, j);
            }
            pool.truncate(want);
            (block.clone(), pool)
        })
        .collect()
}

pub struct Generator {
    shape: PlanetShape,
    cfg: PlanetGenConfig,
    ids: Ids,
    ores: Vec<Ore>,
    flora: Vec<Flora>,
    colours: BlockColours,
    region_a: Fbm,
    region_b: Fbm,
    region_wobble: Fbm,
    width: f64,
    /// Highest a tree reaches above its footing.
    tree_top: i64,
    water_level: i64,
    continent: Fbm,
    hills: Fbm,
    ridges: Fbm,
    mountain_mask: Fbm,
    forest: Fbm,
    caverns: Fbm,
    tunnel_a: Fbm,
    tunnel_b: Fbm,
    /// The planet's biomes (milestone 19); None: version 3's one landscape.
    biome: Option<BiomeWorld>,
    /// Cave styles by tier (milestone 19 part 3; 0: version 3's caves).
    cave_tier: u8,
    flat_caves: Fbm,
    halls: Fbm,
    tall_caves: Fbm,
    shafts: Fbm,
    pillars: Fbm,
    /// What lines cave walls from tier 2: the three rocks, then the rarer
    /// linings (ice, sand, ash, gravel).
    linings: Vec<BlockId>,
    /// Tree sites already decided, by lattice square: neighbouring chunks
    /// ask about most of the same squares. Only a cache: every answer is a
    /// pure function of the square.
    tree_sites: Cache<Option<TreeHit>>,
    /// Each chunk column's land (the chunk and its margin), shared by the
    /// chunks stacked in it. Only a cache, like `tree_sites`.
    spans: Cache<std::sync::Arc<Vec<Column>>>,
}

/// Chunk columns' land the cache holds before it starts again (each about
/// 35 KB).
const SPAN_CACHE: usize = 512;

/// A tree site: its column, ground height, biome and tree kind (indices),
/// and the seed it grows from.
#[derive(Clone, Copy, Debug)]
struct TreeHit {
    x: i64,
    z: i64,
    h: i64,
    biome: u8,
    tree: u8,
    seed: u64,
}

/// A cache shared by the threads generating a planet, keyed by a square or
/// chunk column.
type Cache<T> = std::sync::Mutex<std::collections::HashMap<(i64, i64), T>>;

/// Tree sites the cache holds before it starts again.
const TREE_SITE_CACHE: usize = 200_000;

/// A biome's land and blocks, resolved for generating.
struct BiomeLand {
    base: f64,
    hills: f64,
    mountains: f64,
    terrace: f64,
    rivers: f64,
    hill: Fbm,
    ridge: Fbm,
    /// (top, under, whether it covers steep ground).
    cover: Vec<(BlockId, BlockId, bool)>,
    rock: BlockId,
    cliff: BlockId,
    beach: Option<BlockId>,
    snow_above: Option<i64>,
    tree_density: f64,
    /// Its trees, resolved, and their weights' total.
    trees: Vec<LandTree>,
    tree_weight: f64,
    lift: Option<biomes::Lift>,
}

/// A tree kind a biome grows, its wood resolved.
struct LandTree {
    shape: trees::Shape,
    weight: f64,
    size: f64,
    /// Trunk, leaves, second leaves.
    wood: [BlockId; 3],
}

/// Trees grow from one hashed column in each square this wide, so a tree
/// is a function of the world and crosses chunk borders whole. Divides 32,
/// so every planet width holds whole squares.
const TREE_CELL: i64 = 4;

/// What a biome planet adds to the generator.
struct BiomeWorld {
    layout: biomes::Layout,
    lands: Vec<BiomeLand>,
    rivers: Fbm,
    patch: Fbm,
    wobble: Fbm,
    /// The rock band crossing each height (None: the biome's own rock).
    strata: Vec<Option<BlockId>>,
    /// Every block ores and glow crystals may replace.
    rocks: Vec<BlockId>,
    river_bed: [BlockId; 3],
    any_trees: bool,
    /// The furthest any of its trees reaches sideways, and up.
    tree_reach: i64,
    tree_up: i64,
    /// Lifted land's noise, and whether any biome lifts.
    lift: Fbm,
    any_lift: bool,
}

/// One column of a biome planet: its height, the biome it belongs to and how
/// much a river cuts it (0 to 1).
#[derive(Clone, Copy, Debug, Default)]
struct Column {
    h: i64,
    biome: u8,
    river: f64,
    /// Lifted land: the band (empty when `lift_hi <= lift_lo`) and how much
    /// of it is solid, faded toward the biome's border.
    lift_lo: i64,
    lift_hi: i64,
    lift_fill: f64,
}

/// Rows between the samples `Generator::density` takes of an ore's noise.
pub const DENSITY_STEP: i64 = 3;

/// Lifted land fades this far into its band's top and bottom (blocks).
const LIFT_FADE: f64 = 8.0;

/// Height over which neighbouring biomes' land blends (a **guess**).
const BIOME_BLEND: f64 = 48.0;
/// A river channel where the river noise is within this of zero (a **guess**:
/// about 15 to 25 blocks wide).
const RIVER_WIDTH: f64 = 0.022;
/// Strata bands start this far under the sea (Boundless: 55 against 64).
const STRATA_BELOW_SEA: i64 = 9;

/// Blocks that grass and soil cannot hold on steep ground (Boundless's
/// `maxGradient` 1.25): everything else covers any slope.
const SOFT_COVER: [&str; 7] = ["turf", "meadow_turf", "moss_turf", "soil", "clay", "peat", "mud"];

impl BiomeWorld {
    fn new(cfg: &PlanetGenConfig, r: &BlockRegistry, width: i64, k: impl Fn(u64) -> u64) -> Result<Self, ConfigError> {
        let id = |n: &str| r.id(n);
        let mut lands = Vec::with_capacity(cfg.biomes.len());
        let mut rocks = vec![id("stone")?, id("deep_stone")?];
        for (i, b) in cfg.biomes.iter().enumerate() {
            let rock = id(&b.rock)?;
            if !rocks.contains(&rock) {
                rocks.push(rock);
            }
            let cover = b
                .cover
                .iter()
                .map(|[top, under]| Ok((id(top)?, id(under)?, !SOFT_COVER.contains(&top.as_str()))))
                .collect::<Result<Vec<_>, RegistryError>>()?;
            let mut trees = Vec::new();
            let specs = if b.trees.is_empty() && b.tree_density > 0.0 {
                vec![biomes::TreeSpec::default()]
            } else {
                b.trees.clone()
            };
            for t in &specs {
                let [tr, lv, l2] = t.wood();
                // Rock shapes are of the biome's rock unless they name one.
                let named = |n: &str| if n.is_empty() { Ok(rock) } else { id(n) };
                trees.push(LandTree {
                    shape: t.shape,
                    weight: t.weight.max(0.0),
                    size: t.size.clamp(0.0, 1.0),
                    wood: [named(tr)?, named(lv)?, named(l2)?],
                });
            }
            let tree_weight = trees.iter().map(|t| t.weight).sum();
            lands.push(BiomeLand {
                trees,
                tree_weight,
                lift: b.lift.clone(),
                base: b.base,
                hills: b.hills,
                mountains: b.mountains,
                terrace: b.terrace,
                rivers: if b.rivers { 1.0 } else { 0.0 },
                hill: Fbm::new(k(2001 + 2 * i as u64), width, b.hill_size.max(8.0), 3, 0.5),
                ridge: Fbm::new(k(2002 + 2 * i as u64), width, b.mountain_size.max(16.0), 4, 0.5),
                cover,
                rock,
                cliff: if b.cliff.is_empty() { rock } else { id(&b.cliff)? },
                beach: if b.beach.is_empty() { None } else { Some(id(&b.beach)?) },
                snow_above: b.snow_above,
                tree_density: b.tree_density,
            });
        }
        // The three rock kinds band every cliff (Boundless's strata), when
        // the catalogue has them.
        let kinds: Vec<BlockId> = ["stone", "basalt", "limestone"]
            .iter()
            .filter_map(|n| r.id(n).ok())
            .collect();
        for b in &kinds {
            if !rocks.contains(b) {
                rocks.push(*b);
            }
        }
        let strata = Self::strata(cfg, &kinds, 512);
        let tree_reach = lands
            .iter()
            .filter(|l| l.tree_density > 0.0)
            .flat_map(|l| &l.trees)
            .map(|t| t.shape.reach().0 as i64)
            .max()
            .unwrap_or(0);
        let tree_up = lands
            .iter()
            .filter(|l| l.tree_density > 0.0)
            .flat_map(|l| &l.trees)
            .map(|t| t.shape.reach().1 as i64)
            .max()
            .unwrap_or(0);
        Ok(Self {
            tree_reach,
            tree_up,
            lift: Fbm::new(k(2904), width, 34.0, 3, 0.5),
            any_lift: cfg.biomes.iter().any(|b| b.lift.is_some()),
            layout: biomes::Layout::new(&cfg.biomes, width, cfg.biome_size, cfg.seed),
            any_trees: lands.iter().any(|l| l.tree_density > 0.0 && l.tree_weight > 0.0),
            lands,
            rivers: Fbm::new(k(2901), width, 520.0, 2, 0.5),
            patch: Fbm::new(k(2902), width, 96.0, 2, 0.5),
            wobble: Fbm::new(k(2903), width, 110.0, 2, 0.5),
            strata,
            rocks,
            river_bed: [id("gravel")?, id("clay").unwrap_or(id("gravel")?), id("sand")?],
        })
    }

    /// The band crossing each height up to `height`: from a little under the
    /// sea up, thick, middle and thin bands of the rock kinds at hashed
    /// heights, thinner ones laid over thicker (Boundless's strata: thick 12
    /// ± 4 and middle 6 ± 3 of two kinds, thin 2 to 4 of any).
    fn strata(cfg: &PlanetGenConfig, kinds: &[BlockId], height: usize) -> Vec<Option<BlockId>> {
        let mut out = vec![None; height];
        if kinds.is_empty() {
            return out;
        }
        let lo = (cfg.sea_level - STRATA_BELOW_SEA).max(0);
        let span = (height as i64 - lo).max(1);
        let mut n = 0i64;
        let draw = |n: &mut i64| {
            *n += 1;
            hash01(cfg.seed ^ 0x57a7_a000, *n, 0, 0)
        };
        // (how many, thickness, spread, which kinds may band)
        for (count, thick, spread, thin_kinds) in [(4, 12.0, 4.0, false), (6, 6.0, 3.0, false), (12, 3.0, 1.0, true)] {
            for _ in 0..count {
                let centre = lo + (draw(&mut n) * span as f64) as i64;
                let t = (thick + (draw(&mut n) * 2.0 - 1.0) * spread).max(2.0) as i64;
                let pool: Vec<BlockId> = if thin_kinds || kinds.len() < 3 {
                    kinds.to_vec()
                } else {
                    // Thick and middle bands are never the dark kind.
                    vec![kinds[0], kinds[2]]
                };
                let rock = pool[((draw(&mut n) * pool.len() as f64) as usize).min(pool.len() - 1)];
                for y in centre - t / 2..centre - t / 2 + t {
                    if (0..height as i64).contains(&y) {
                        out[y as usize] = Some(rock);
                    }
                }
            }
        }
        out
    }
}

/// Cave values at one lattice point. `flat` and `hall` are milestone 19's
/// cave styles, read only on planets with a cave tier.
#[derive(Clone, Copy, Default)]
struct Cave {
    cavern: f64,
    tunnel: f64,
    flat: f64,
    hall: f64,
    /// The planet's cave tier (0: version 3's caves).
    tier: u8,
}

impl Cave {
    /// Rooms open wider and tunnels wind thicker from tier 3.
    fn thresholds(self) -> (f64, f64) {
        if self.tier >= 3 { (0.39, 0.068) } else { (0.42, 0.055) }
    }

    fn open(self) -> bool {
        let (cavern, tunnel) = self.thresholds();
        self.cavern > cavern
            || self.tunnel < tunnel
            || (self.tier >= 1 && self.flat > FLAT_OPEN)
            || (self.tier >= 5 && self.hall > HALL_OPEN)
    }

    /// Close to a cave's wall (inside rock): where a lining goes.
    fn near_wall(self) -> bool {
        let (cavern, tunnel) = self.thresholds();
        self.cavern > cavern - 0.06
            || self.tunnel < tunnel + 0.03
            || (self.tier >= 1 && self.flat > FLAT_OPEN - 0.06)
            || (self.tier >= 5 && self.hall > HALL_OPEN - 0.06)
    }
}

/// Flat caves open where their stretched noise passes this (a **guess**).
const FLAT_OPEN: f64 = 0.5;
/// Great halls and tall caves open where theirs passes this (a **guess**).
const HALL_OPEN: f64 = 0.48;
/// Great halls and tall caves stay this far under the sea.
const HALL_BELOW_SEA: i64 = 24;
/// Shafts (tier 6 and up): round pits, one at a hashed spot in some squares
/// `SHAFT_CELL` across, open from `SHAFT_TOP` under the surface down
/// `SHAFT_DEPTH` (sizes **guesses**).
const SHAFT_CELL: i64 = 32;
const SHAFT_CHANCE: f64 = 0.35;
const SHAFT_TOP: i64 = 10;
const SHAFT_DEPTH: i64 = 70;

/// Where a block sits in the chunk's lattice: the flat indices of the eight
/// surrounding lattice points (`[x][y][z]` order) and the fractions between.
#[derive(Clone, Copy)]
struct Cell {
    idx: [usize; 8],
    t: [f64; 3],
}

impl Cell {
    fn at(lx: i64, ly: i64, lz: i64) -> Self {
        let (i, j, k) = ((lx / COARSE) as usize, (ly / COARSE) as usize, (lz / COARSE) as usize);
        let g = |di: usize, dj: usize, dk: usize| ((j + dj) * COARSE_N + k + dk) * COARSE_N + i + di;
        let t = |v: i64| (v % COARSE) as f64 / COARSE as f64;
        Self {
            idx: [
                g(0, 0, 0),
                g(0, 0, 1),
                g(0, 1, 0),
                g(0, 1, 1),
                g(1, 0, 0),
                g(1, 0, 1),
                g(1, 1, 0),
                g(1, 1, 1),
            ],
            t: [t(lx), t(ly), t(lz)],
        }
    }

    /// Trilinear interpolation of one field.
    #[inline]
    fn lerp(&self, v: impl Fn(usize) -> f64) -> f64 {
        let c = self.idx.map(v);
        trilerp(c, self.t[0], self.t[1], self.t[2])
    }
}

impl Generator {
    pub fn new(shape: PlanetShape, cfg: PlanetGenConfig, registry: &BlockRegistry) -> Result<Self, ConfigError> {
        let w = shape.width_blocks();
        let s = cfg.seed;
        let k = |n: u64| s.wrapping_mul(0x2545_f491_4f6c_dd1d).wrapping_add(n << 12);
        let ids = Ids::resolve(&cfg, registry)?;
        let colours = BlockColours::resolve(&cfg.colours, registry)?;
        let ores = cfg
            .ores
            .iter()
            .zip(&ids.ores)
            .zip(&ids.ore_under)
            .enumerate()
            .map(|(i, ((o, &block), under))| Ore {
                block,
                noise: Fbm::new(k(9 + i as u64), w, o.size, 1, 0.5),
                threshold: o.threshold,
                min_y: o.min_y,
                max_y: o.max_y,
                min_depth: o.min_depth,
                max_depth: o.max_depth,
                water_depth: o.water_depth,
                near_water: o.near_water,
                land_height: o.land_height,
                under: under.clone(),
            })
            .collect();
        let flora = Flora::resolve(&cfg, registry, w)?;
        let biome = if cfg.biomes.is_empty() {
            None
        } else {
            Some(BiomeWorld::new(&cfg, registry, w, k)?)
        };
        let biome_tree_up = biome.as_ref().map_or(0, |b| b.tree_up + 2);
        let linings = if cfg.cave_tier >= 2 {
            ["stone", "basalt", "limestone", "ice", "sand", "ash", "gravel"]
                .iter()
                .filter_map(|n| registry.id(n).ok())
                .collect()
        } else {
            Vec::new()
        };
        Ok(Self {
            biome,
            cave_tier: cfg.cave_tier,
            flat_caves: Fbm::new(k(3001), w, 90.0, 2, 0.5),
            halls: Fbm::new(k(3002), w, 150.0, 2, 0.5),
            tall_caves: Fbm::new(k(3003), w, 48.0, 2, 0.5),
            shafts: Fbm::new(k(3004), w, 70.0, 1, 0.5),
            pillars: Fbm::new(k(3005), w, 14.0, 1, 0.5),
            linings,
            tree_sites: std::sync::Mutex::new(std::collections::HashMap::new()),
            spans: std::sync::Mutex::new(std::collections::HashMap::new()),
            shape,
            ids,
            ores,
            flora,
            colours,
            // Keys 1 to 8 are the terrain's and 9 up the ores'; these stay
            // clear of any ore list a kind could hold.
            region_a: Fbm::new(k(1001), w, REGION_FEATURE, 2, 0.5),
            region_b: Fbm::new(k(1002), w, REGION_FEATURE, 2, 0.5),
            region_wobble: Fbm::new(k(1003), w, REGION_WOBBLE_FEATURE, 2, 0.5),
            width: w as f64,
            tree_top: (cfg.trunk_min + cfg.trunk_extra.max(1) + 1)
                .max(TREE_MAX_HEIGHT)
                .max(biome_tree_up),
            water_level: cfg.sea_level + cfg.water_offset,
            continent: Fbm::new(k(1), w, 700.0, 3, 0.5),
            hills: Fbm::new(k(2), w, 160.0, 4, 0.5),
            ridges: Fbm::new(k(3), w, 420.0, 5, 0.5),
            mountain_mask: Fbm::new(k(4), w, 900.0, 2, 0.5),
            forest: Fbm::new(k(5), w, 250.0, 2, 0.5),
            caverns: Fbm::new(k(6), w, 64.0, 2, 0.5),
            tunnel_a: Fbm::new(k(7), w, 96.0, 1, 0.5),
            tunnel_b: Fbm::new(k(8), w, 96.0, 1, 0.5),
            cfg,
        })
    }

    pub fn shape(&self) -> PlanetShape {
        self.shape
    }

    pub fn config(&self) -> &PlanetGenConfig {
        &self.cfg
    }

    /// How much of `blocks` the generator puts in column `(x, z)`, roughly:
    /// ore blocks (its noise sampled every [`DENSITY_STEP`] blocks down its
    /// depth band, where the column's rules allow it), plus the chance of a
    /// surface plant of those kinds. For the atlas's heat maps (milestone
    /// 27): relative, never the depth, and blind to caves and to what was
    /// mined since.
    pub fn density(&self, blocks: &[BlockId], x: i64, z: i64) -> f64 {
        let (h, top) = match &self.biome {
            Some(bw) => {
                let c = self.column(bw, x, z);
                (c.h, self.biome_cover(bw, c, 0, false, x, z).0)
            }
            None => {
                let h = self.surface_height(x, z);
                (h, self.surface_block(h, 0, x, z))
            }
        };
        let sea = self.cfg.sea_level;
        let mut total = 0.0;
        for ore in self.ores.iter().filter(|o| blocks.contains(&o.block)) {
            if !ore.column_allows(h, top, sea, self.water_level, |dx, dz| {
                self.surface_height(x + dx, z + dz)
            }) {
                continue;
            }
            let lo = ore.min_y.max(h.saturating_sub(ore.max_depth)).max(1);
            let hi = ore.max_y.min(h - ore.min_depth);
            let mut y = lo;
            while y <= hi {
                if ore.noise.sample3(x as f64, y as f64, z as f64, self.width) > ore.threshold {
                    total += DENSITY_STEP as f64;
                }
                y += DENSITY_STEP;
            }
        }
        for f in self.flora.iter().filter(|f| blocks.contains(&f.block) && !f.cave) {
            let y = h + 1;
            if y < f.min_y || y > f.max_y || !f.on.contains(&top) {
                continue;
            }
            if let Some((noise, above)) = &f.patch
                && noise.sample2(x as f64, z as f64, self.width) <= *above
            {
                continue;
            }
            total += f.density;
        }
        total
    }

    /// Y of the topmost terrain block in a column (before caves and trees).
    pub fn surface_height(&self, x: i64, z: i64) -> i64 {
        if let Some(bw) = &self.biome {
            return self.column(bw, x, z).h;
        }
        let (x, z) = (x as f64, z as f64);
        let w = self.width;
        let c = self.continent.sample2(x, z, w);
        let land = smoothstep(-0.30, 0.05, c);
        let hills = self.hills.sample2(x, z, w);
        let ridge = self.ridges.ridged2(x, z, w);
        let mask = smoothstep(-0.10, 0.30, self.mountain_mask.sample2(x, z, w));
        let sea = self.cfg.sea_level as f64;
        let h = sea - 24.0
            + land * 30.0
            + hills * 14.0 * (0.4 + land)
            + mask * land * ridge * ridge * self.cfg.mountain_height;
        (h.floor() as i64).clamp(4, self.shape.height_blocks() - self.tree_top - 4)
    }

    /// The columns of the chunk column at `origin` and `m` around it,
    /// `[dz][dx]`, from the cache or computed.
    fn span_columns(&self, bw: &BiomeWorld, origin: BlockPos, span: usize, m: i64) -> std::sync::Arc<Vec<Column>> {
        let key = (origin.x, origin.z);
        if let Some(c) = self.spans.lock().ok().and_then(|s| s.get(&key).cloned()) {
            return c;
        }
        let mut columns = Vec::with_capacity(span * span);
        for dz in 0..span {
            for dx in 0..span {
                columns.push(self.column(bw, origin.x + dx as i64 - m, origin.z + dz as i64 - m));
            }
        }
        let columns = std::sync::Arc::new(columns);
        if let Ok(mut s) = self.spans.lock() {
            if s.len() >= SPAN_CACHE {
                s.clear();
            }
            s.insert(key, columns.clone());
        }
        columns
    }

    /// A column of a biome planet: the blend of the nearby cells' land, cut
    /// by rivers, and the biome it belongs to.
    fn column(&self, bw: &BiomeWorld, x: i64, z: i64) -> Column {
        let (fx, fz) = (x as f64, z as f64);
        let w = self.width;
        let sea = self.cfg.sea_level as f64;
        let (near, n) = bw.layout.near(fx, fz);
        let d0 = near[0].0;
        let (mut sum, mut weights, mut rivers) = (0.0, 0.0, 0.0);
        // Each biome counted once, at its nearest cell.
        let mut seen = [false; 256];
        for &(d, b) in &near[..n] {
            let t = 1.0 - (d - d0) / BIOME_BLEND;
            if t <= 0.0 || seen[b as usize] {
                continue;
            }
            seen[b as usize] = true;
            let wt = t * t;
            let land = &bw.lands[b as usize];
            let mut h = sea + land.base + land.hill.sample2(fx, fz, w) * 1.4 * land.hills;
            if land.mountains > 0.0 {
                let r = land.ridge.ridged2(fx, fz, w);
                h += land.mountains * r * r;
            }
            if land.terrace > 0.0 {
                let k = (h - sea) / land.terrace;
                h = sea + (k.floor() + smoothstep(0.7, 1.0, k - k.floor())) * land.terrace;
            }
            sum += wt * h;
            rivers += wt * land.rivers;
            weights += wt;
        }
        let mut h = sum / weights;
        // A river cuts to two under the water line, its banks easing out.
        let r = bw.rivers.sample2(fx, fz, w).abs();
        let river = smoothstep(RIVER_WIDTH, RIVER_WIDTH * 0.35, r) * (rivers / weights);
        let bottom = (self.water_level - 2) as f64;
        if river > 0.0 && h > bottom {
            h -= (h - bottom) * river;
        }
        // A little jitter in which cell a border column belongs to, so biome
        // edges are not ruled lines.
        let jitter = (hash01(self.cfg.seed ^ 0xb0d3, x, 0, z) - 0.5) * 8.0;
        let biome = if n > 1 && near[1].0 - d0 < jitter {
            near[1].1
        } else {
            near[0].1
        };
        let h = (h.floor() as i64).clamp(4, self.shape.height_blocks() - self.tree_top - 4);
        let (mut lift_lo, mut lift_hi, mut lift_fill) = (0, 0, 0.0);
        if let Some(l) = &bw.lands[biome as usize].lift {
            let base = if l.from_ground { h } else { self.cfg.sea_level };
            lift_lo = base + l.lo as i64;
            lift_hi = (base + l.hi as i64).min(self.shape.height_blocks() - 8);
            // Thinner toward other biomes, so islands end before the border.
            let own = near[..n]
                .iter()
                .filter(|c| c.1 == biome)
                .map(|c| c.0)
                .fold(f64::MAX, f64::min);
            let other = near[..n]
                .iter()
                .filter(|c| c.1 != biome)
                .map(|c| c.0)
                .fold(f64::MAX, f64::min);
            let edge = ((other - own) / (BIOME_BLEND * 2.0)).clamp(0.0, 1.0);
            lift_fill = l.fill - (1.0 - edge) * 0.7;
        }
        Column {
            h,
            biome,
            river,
            lift_lo,
            lift_hi,
            lift_fill,
        }
    }

    fn cave_at(&self, x: i64, y: i64, z: i64) -> Cave {
        let (fx, fy, fz) = (x as f64, y as f64, z as f64);
        let w = self.width;
        let a = self.tunnel_a.sample3(fx, fy * 1.4, fz, w);
        let b = self.tunnel_b.sample3(fx, fy * 1.4, fz, w);
        let tier = self.cave_tier;
        let flat = if tier >= 1 {
            self.flat_caves.sample3(fx, fy * 3.0, fz, w)
        } else {
            -1.0
        };
        let hall = if tier >= 5 && y < self.cfg.sea_level - HALL_BELOW_SEA {
            let wide = self.halls.sample3(fx, fy * 4.0, fz, w);
            let tall = self.tall_caves.sample3(fx, fy * 0.4, fz, w) - 0.04;
            // Pillars hold the great halls up (tier 6 and up).
            if tier >= 6 && self.pillars.sample2(fx, fz, w) > 0.42 {
                -1.0
            } else {
                wide.max(tall)
            }
        } else {
            -1.0
        };
        Cave {
            cavern: self.caverns.sample3(fx, fy * 1.6, fz, w),
            tunnel: a.abs().max(b.abs()),
            flat,
            hall,
            tier,
        }
    }

    /// Whether a shaft opens at canonical `(x, y, z)` under a surface at `h`
    /// (tier 6 and up).
    fn shaft_at(&self, x: i64, y: i64, z: i64, h: i64) -> bool {
        if self.cave_tier < 6 || y > h - SHAFT_TOP || y <= h - SHAFT_TOP - SHAFT_DEPTH {
            return false;
        }
        let (ci, cj) = (x.div_euclid(SHAFT_CELL), z.div_euclid(SHAFT_CELL));
        let seed = self.cfg.seed ^ 0x5_4af7;
        if hash01(seed, ci, 0, cj) >= SHAFT_CHANCE {
            return false;
        }
        let at = |k: i64| 6 + (hash01(seed, ci, k, cj) * (SHAFT_CELL - 12) as f64) as i64;
        let (sx, sz) = (ci * SHAFT_CELL + at(1), cj * SHAFT_CELL + at(2));
        // A little wobble with depth so the pit is not a ruled tube.
        let r = 1.5 + hash01(seed, ci, 3, cj) * 2.0 + self.shafts.sample2(x as f64, y as f64, self.width) * 0.8;
        let (dx, dz) = ((x - sx) as f64, (z - sz) as f64);
        dx * dx + dz * dz <= r * r
    }

    /// Interpolated cave values at any block, from the world-aligned lattice.
    /// Used for single lookups (tree footings, spawn); chunk generation
    /// computes the same values in bulk.
    fn cave_interp(&self, x: i64, y: i64, z: i64) -> Cave {
        let snap = |v: i64| v.div_euclid(COARSE) * COARSE;
        let (x0, y0, z0) = (snap(x), snap(y), snap(z));
        let mut c = [Cave::default(); 8];
        for (n, v) in c.iter_mut().enumerate() {
            let (i, j, k) = ((n >> 2) as i64, ((n >> 1) & 1) as i64, (n & 1) as i64);
            *v = self.cave_at(x0 + i * COARSE, y0 + j * COARSE, z0 + k * COARSE);
        }
        let t = |v: i64, v0: i64| (v - v0) as f64 / COARSE as f64;
        let (tx, ty, tz) = (t(x, x0), t(y, y0), t(z, z0));
        Cave {
            cavern: trilerp(c.map(|c| c.cavern), tx, ty, tz),
            tunnel: trilerp(c.map(|c| c.tunnel), tx, ty, tz),
            flat: trilerp(c.map(|c| c.flat), tx, ty, tz),
            hall: trilerp(c.map(|c| c.hall), tx, ty, tz),
            tier: self.cave_tier,
        }
    }

    fn is_cave_at(&self, x: i64, y: i64, z: i64) -> bool {
        self.cave_interp(x, y, z).open()
    }

    /// Generates one chunk. Positions off the canonical lap are wrapped first.
    pub fn generate(&self, pos: ChunkPos) -> Chunk {
        let pos = self.shape.wrap_chunk(pos);
        if !self.shape.contains_chunk_y(pos.y) {
            return Chunk::empty();
        }
        let origin = pos.origin();
        let ids = &self.ids;
        let cfg = &self.cfg;
        let sea = cfg.sea_level;
        let water_level = self.water_level;
        let cs = CHUNK_SIZE as i64;

        // Column heights for the chunk plus a margin: the margin feeds slope
        // tests at the border and trees whose canopy reaches in.
        let m = TREE_REACH + 1;
        let span = (cs + 2 * m) as usize;
        let mut heights = vec![0i64; span * span];
        let columns = match &self.biome {
            Some(bw) => self.span_columns(bw, origin, span, m),
            None => std::sync::Arc::new(Vec::new()),
        };
        let mut max_h = i64::MIN;
        for dz in 0..span {
            for dx in 0..span {
                let h = match &self.biome {
                    Some(_) => columns[dz * span + dx].h,
                    None => self.surface_height(origin.x + dx as i64 - m, origin.z + dz as i64 - m),
                };
                heights[dz * span + dx] = h;
                max_h = max_h.max(h);
            }
        }
        let column_at = |lx: i64, lz: i64| columns[((lz + m) as usize) * span + (lx + m) as usize];
        let height_at = |lx: i64, lz: i64| heights[((lz + m) as usize) * span + (lx + m) as usize];

        // Lifted land reaches above the ground: the highest band in reach.
        let lift_top = columns
            .iter()
            .filter(|c| c.lift_hi > c.lift_lo)
            .map(|c| c.lift_hi)
            .max();
        // Sky fast path: most chunks on a planet are empty air.
        if origin.y > max_h.max(lift_top.unwrap_or(i64::MIN)) + self.tree_top && origin.y > water_level {
            return Chunk::empty();
        }
        // The lifted land's noise on the chunk's lattice, when a band
        // crosses the chunk.
        let top_y = origin.y + cs - 1;
        let lifting = self.biome.as_ref().is_some_and(|bw| bw.any_lift)
            && columns
                .iter()
                .any(|c| c.lift_hi > c.lift_lo && c.lift_lo <= top_y + 3 && c.lift_hi >= origin.y);
        let mut lift_lattice = Vec::new();
        if let (true, Some(bw)) = (lifting, &self.biome) {
            lift_lattice.reserve_exact(LATTICE);
            for j in 0..COARSE_N {
                for k in 0..COARSE_N {
                    for i in 0..COARSE_N {
                        let (x, y, z) = (
                            (origin.x + i as i64 * COARSE) as f64,
                            (origin.y + j as i64 * COARSE) as f64,
                            (origin.z + k as i64 * COARSE) as f64,
                        );
                        lift_lattice.push(bw.lift.sample3(x, y * 1.3, z, self.width));
                    }
                }
            }
        }
        // Whether lifted land is solid at `wy` in column `c` (chunk column
        // `(lx, lz)`): the lattice inside the chunk, the noise above it.
        let lifted = |c: &Column, lx: i64, wy: i64, lz: i64, wx: i64, wz: i64| -> bool {
            if c.lift_hi <= c.lift_lo || wy < c.lift_lo || wy > c.lift_hi {
                return false;
            }
            let n = if wy <= top_y && !lift_lattice.is_empty() {
                Cell::at(lx, wy - origin.y, lz).lerp(|i| lift_lattice[i])
            } else {
                match &self.biome {
                    Some(bw) => bw.lift.sample3(wx as f64, wy as f64 * 1.3, wz as f64, self.width),
                    None => return false,
                }
            };
            let fade = (((wy - c.lift_lo) as f64).min((c.lift_hi - wy) as f64) / LIFT_FADE).clamp(0.0, 1.0);
            n + c.lift_fill - (1.0 - fade) * 0.8 > 0.0
        };

        // Cave and ore lattice, only if some of the chunk is underground.
        let underground = origin.y <= max_h;
        let n_ores = self.ores.len();
        let mut caves = Vec::new();
        let mut ore_lattice = Vec::new();
        if underground {
            caves.reserve_exact(LATTICE);
            ore_lattice.reserve_exact(LATTICE * n_ores);
            for j in 0..COARSE_N {
                for k in 0..COARSE_N {
                    for i in 0..COARSE_N {
                        let (x, y, z) = (
                            origin.x + i as i64 * COARSE,
                            origin.y + j as i64 * COARSE,
                            origin.z + k as i64 * COARSE,
                        );
                        caves.push(self.cave_at(x, y, z));
                        let (fx, fy, fz) = (x as f64, y as f64, z as f64);
                        for ore in &self.ores {
                            ore_lattice.push(ore.noise.sample3(fx, fy, fz, self.width));
                        }
                    }
                }
            }
        }
        let tier = self.cave_tier;
        let cave_in = |cell: &Cell| Cave {
            cavern: cell.lerp(|i| caves[i].cavern),
            tunnel: cell.lerp(|i| caves[i].tunnel),
            flat: if tier >= 1 { cell.lerp(|i| caves[i].flat) } else { -1.0 },
            hall: if tier >= 5 { cell.lerp(|i| caves[i].hall) } else { -1.0 },
            tier,
        };

        // Colour regions, decided per column: every coloured block in a
        // column takes the colour its region gives it.
        let regions = self.region_lattice(origin);
        let mut column_colours = vec![Colour::NATURAL; self.colours.blocks.len()];
        let mut ore_column = vec![true; self.ores.len()];

        let seed = cfg.seed;
        let top = top_y;
        let mut blocks = vec![BlockState::plain(ids.air); CHUNK_VOLUME];
        for lz in 0..cs {
            for lx in 0..cs {
                let h = height_at(lx, lz);
                let (wx, wz) = (origin.x + lx, origin.z + lz);
                if !regions.is_empty() {
                    let (a, b) = self.region_at(&regions, lx, lz, wx, wz);
                    for (c, bc) in column_colours.iter_mut().zip(&self.colours.blocks) {
                        *c = bc.pick(a, b);
                    }
                }
                let slope = slope_at(&height_at, lx, lz);
                // The rock under the cover: the planet's stone, or on a biome
                // planet the biome's rock crossed by strata bands.
                let mut rock = ids.stone;
                let (surface, fill, soil_bottom) = match &self.biome {
                    None => {
                        let surface = self.surface_block(h, slope, wx, wz);
                        // Under the top: a few blocks of soil (or more of the
                        // surface block), then the planet's layer rock, then
                        // stone.
                        if surface == ids.cliff {
                            (surface, surface, h)
                        } else if surface == ids.ground || surface == ids.snow {
                            (surface, ids.subsoil, h - 3)
                        } else {
                            (surface, surface, h - 3)
                        }
                    }
                    Some(bw) => {
                        let c = column_at(lx, lz);
                        rock = bw.lands[c.biome as usize].rock;
                        let shore =
                            (-3..=3i64).any(|dz| (-3..=3i64).any(|dx| height_at(lx + dx, lz + dz) < self.water_level));
                        let (top, under) = self.biome_cover(bw, c, slope, shore, wx, wz);
                        let depth = 2 + (hash01(seed ^ 0x5a2e, wx, 0, wz) * 4.0) as i64;
                        (top, under, if under == rock { h } else { h - depth })
                    }
                };
                let layer_bottom = if cfg.layer_depth > 0 && self.biome.is_none() {
                    soil_bottom - cfg.layer_depth - (hash01(seed ^ 0x5a2d, wx, 0, wz) * 4.0) as i64
                } else {
                    soil_bottom
                };
                let wobble = match &self.biome {
                    Some(bw) => (bw.wobble.sample2(wx as f64, wz as f64, self.width) * 9.0) as i64,
                    None => 0,
                };
                let rock_at = |wy: i64| match &self.biome {
                    Some(bw) if wy >= sea - STRATA_BELOW_SEA => {
                        let band = (wy + wobble).clamp(0, bw.strata.len() as i64 - 1) as usize;
                        bw.strata[band].unwrap_or(rock)
                    }
                    _ => rock,
                };
                let deep_line = 60 + (hash01(seed ^ 0xdee9, wx, 0, wz) * 5.0) as i64;
                // Which ores this column may hold (milestone 18's rules; every
                // older ore passes).
                for (ok, ore) in ore_column.iter_mut().zip(&self.ores) {
                    *ok = ore.column_allows(h, surface, cfg.sea_level, water_level, |dx, dz| {
                        height_at(lx + dx, lz + dz)
                    });
                }
                let col = if lifting { column_at(lx, lz) } else { Column::default() };
                let y_end = top.min(h.max(water_level).max(if lifting { col.lift_hi } else { 0 }));
                for wy in origin.y..=y_end {
                    let ly = wy - origin.y;
                    let mut b = if wy == 0 || (wy <= 2 && hash01(seed ^ 0xbed, wx, wy, wz) < 0.6 / wy as f64) {
                        ids.bedrock
                    } else if wy > h {
                        if lifting && lifted(&col, lx, wy, lz, wx, wz) {
                            // Lifted land: its biome's cover on top, the
                            // under block below that, then rock.
                            let land = &self.biome.as_ref().expect("lifting").lands[col.biome as usize];
                            if !lifted(&col, lx, wy + 1, lz, wx, wz) {
                                match land.snow_above {
                                    Some(above) if wy >= sea + above => ids.snow,
                                    _ => land.cover[0].0,
                                }
                            } else if !lifted(&col, lx, wy + 3, lz, wx, wz) {
                                land.cover[0].1
                            } else {
                                rock_at(wy)
                            }
                        } else if wy > water_level {
                            ids.air
                        } else if wy > water_level - cfg.ice_depth {
                            ids.ice
                        } else {
                            ids.water
                        }
                    } else if wy == h {
                        surface
                    } else if wy >= soil_bottom {
                        fill
                    } else if wy >= layer_bottom {
                        ids.layer
                    } else if wy < deep_line {
                        ids.deep_stone
                    } else {
                        rock_at(wy)
                    };
                    // Never open a cave into the sea floor: static water would
                    // stand as a wall over the hole.
                    let carveable =
                        b != ids.bedrock && b != ids.water && b != ids.ice && (h >= water_level || wy < h - 6);
                    if underground && carveable {
                        let cell = Cell::at(lx, ly, lz);
                        let cave = cave_in(&cell);
                        if cave.open() || self.shaft_at(wx, wy, wz, h) {
                            b = match ids.lower {
                                Some((level, sea)) if wy <= level => sea,
                                _ => ids.air,
                            };
                        } else if self.is_rock(b) {
                            let mut ore_hit = None;
                            for (o, ore) in self.ores.iter().enumerate() {
                                if ore_column[o]
                                    && wy >= ore.min_y
                                    && wy <= ore.max_y
                                    && wy <= h - ore.min_depth
                                    && wy >= h.saturating_sub(ore.max_depth)
                                    && cell.lerp(|i| ore_lattice[i * n_ores + o]) > ore.threshold
                                {
                                    ore_hit = Some(ore.block);
                                    break;
                                }
                            }
                            if let Some(ore) = ore_hit {
                                b = ore;
                            } else if !self.linings.is_empty() && wy < h - 2 && cave.near_wall() {
                                // A lining (tier 2 and up): one kind per
                                // stretch of cave, most often a rock.
                                let pick = hash01(seed ^ 0x11_1e, wx >> 4, wy >> 4, wz >> 4);
                                let n = self.linings.len();
                                let i = if pick < 0.9 {
                                    (pick / 0.9 * 3.0) as usize % 3
                                } else {
                                    3 + ((pick - 0.9) / 0.1 * (n - 3) as f64) as usize % (n - 3)
                                };
                                b = self.linings[i.min(n - 1)];
                            } else if wy < sea - cfg.glow_depth
                                && ly < cs - 1
                                && cave_in(&Cell::at(lx, ly + 1, lz)).open()
                                && hash01(seed ^ 0x610, wx, wy, wz) < cfg.glow_chance
                            {
                                b = ids.glow;
                            }
                        }
                    }
                    blocks[LocalPos::new(lx as u8, ly as u8, lz as u8).index()] = match self.colours.slot(b) {
                        Some(s) => BlockState::coloured(b, column_colours[s]),
                        None => BlockState::plain(b),
                    };
                }
            }
        }

        match &self.biome {
            Some(bw) => self.place_biome_trees(bw, origin, &mut blocks, max_h),
            None => {
                let density_at = |_: i64, _: i64| cfg.tree_density;
                self.place_trees(pos, &mut blocks, &height_at, &density_at, &regions);
            }
        }
        self.place_flora(origin, &mut blocks, &height_at);
        Chunk::from_dense_states(&blocks)
    }

    /// The two colour-region fields on the chunk's lattice, `[z][x]`, from one
    /// lattice step outside the chunk to one past it. Empty when the planet
    /// has no colours.
    fn region_lattice(&self, origin: BlockPos) -> Vec<(f64, f64)> {
        if self.colours.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(REGION_N * REGION_N);
        for j in 0..REGION_N as i64 {
            for i in 0..REGION_N as i64 {
                let x = (origin.x + (i - 1) * REGION_STEP) as f64;
                let z = (origin.z + (j - 1) * REGION_STEP) as f64;
                // One wobble moves both fields the same way (opposite sign
                // for b), so variants and the main colour meander together.
                let w = self.region_wobble.sample2(x, z, self.width) * REGION_WOBBLE;
                out.push((
                    self.region_a.sample2(x, z, self.width) + w,
                    self.region_b.sample2(x, z, self.width) - w,
                ));
            }
        }
        out
    }

    /// The region fields at column `(lx, lz)` of the chunk (`-4..36`), whose
    /// canonical world column is `(wx, wz)`, frayed a little per column.
    #[inline]
    fn region_at(&self, lattice: &[(f64, f64)], lx: i64, lz: i64, wx: i64, wz: i64) -> (f64, f64) {
        let (gx, gz) = (lx + REGION_STEP, lz + REGION_STEP);
        let (i, j) = ((gx / REGION_STEP) as usize, (gz / REGION_STEP) as usize);
        let t = |g: i64| (g % REGION_STEP) as f64 / REGION_STEP as f64;
        let (tx, tz) = (t(gx), t(gz));
        let at = |di: usize, dj: usize| lattice[(j + dj) * REGION_N + i + di];
        let (p00, p10, p01, p11) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
        let bilerp = |a: f64, b: f64, c: f64, d: f64| {
            let top = a + (b - a) * tx;
            top + (c + (d - c) * tx - top) * tz
        };
        let h = hash3(self.cfg.seed ^ 0xf4a7_c010, wx, 0, wz);
        let fray = |bits: u64| ((bits & 0xffff) as f64 / 65536.0 - 0.5) * REGION_FRAY;
        (
            bilerp(p00.0, p10.0, p01.0, p11.0) + fray(h),
            bilerp(p00.1, p10.1, p01.1, p11.1) + fray(h >> 32),
        )
    }

    fn surface_block(&self, h: i64, slope: i64, wx: i64, wz: i64) -> BlockId {
        let ids = &self.ids;
        let sea = self.cfg.sea_level;
        if h < sea - 3 {
            let n = ids.seabed.len();
            let i = ((hash01(self.cfg.seed ^ 0x9a7e1, wx, 0, wz) * n as f64) as usize).min(n - 1);
            ids.seabed[i]
        } else if h < sea {
            ids.shallows
        } else if h <= sea + 1 {
            ids.shore
        } else if slope >= 3 {
            ids.cliff
        } else if h >= sea + self.cfg.snow_line {
            ids.snow
        } else {
            ids.ground
        }
    }

    /// Rock that ores and glow crystals may replace: stone and deep stone,
    /// and on a biome planet every rock kind.
    #[inline]
    fn is_rock(&self, b: BlockId) -> bool {
        match &self.biome {
            Some(bw) => bw.rocks.contains(&b),
            None => b == self.ids.stone || b == self.ids.deep_stone,
        }
    }

    /// A biome column's top block and the block under it: sea beds and
    /// river beds under water, beaches at the shore (`shore`: open water
    /// within three columns), snow above the biome's
    /// snow line, its cover patched by noise elsewhere, and bare rock (its
    /// cliff) where the ground is too steep for the cover to hold.
    fn biome_cover(&self, bw: &BiomeWorld, c: Column, slope: i64, shore: bool, wx: i64, wz: i64) -> (BlockId, BlockId) {
        let ids = &self.ids;
        let land = &bw.lands[c.biome as usize];
        let sea = self.cfg.sea_level;
        let seed = self.cfg.seed;
        if c.river > 0.25 && c.h < self.water_level {
            let i = ((hash01(seed ^ 0x81fe, wx, 0, wz) * 3.0) as usize).min(2);
            return (bw.river_bed[i], bw.river_bed[i]);
        }
        if c.h < sea - 6 {
            let n = ids.seabed.len();
            let i = ((hash01(seed ^ 0x9a7e1, wx, 0, wz) * n as f64) as usize).min(n - 1);
            return (ids.seabed[i], ids.seabed[i]);
        }
        if let Some(beach) = land.beach
            && c.h <= sea + 2
            && shore
            && slope < 4
        {
            return (beach, beach);
        }
        if slope >= 4 {
            return (land.cliff, land.rock);
        }
        if let Some(above) = land.snow_above
            && c.h >= sea + above
        {
            return (ids.snow, land.cover[0].1);
        }
        let n = land.cover.len();
        let p = bw.patch.sample2(wx as f64, wz as f64, self.width);
        let i = (((p * 1.4 + 0.5).clamp(0.0, 0.999)) * n as f64) as usize;
        let (top, under, steep) = land.cover[i.min(n - 1)];
        if slope >= 2 && !steep {
            return (land.cliff, land.rock);
        }
        (top, under)
    }

    /// Whether a tree grows from canonical column `(x, z)`, and its trunk height.
    /// `density` is the planet's tree density, or on a biome planet the
    /// column's biome's ([`Self::tree_density`]).
    fn tree_at(&self, x: i64, z: i64, h: i64, density: f64) -> Option<i64> {
        let sea = self.cfg.sea_level;
        if density <= 0.0 || h < sea.max(self.water_level) + 2 || h >= sea + self.cfg.tree_line {
            return None;
        }
        let forest = smoothstep(-0.2, 0.4, self.forest.sample2(x as f64, z as f64, self.width));
        let density = density * (0.15 + 2.0 * forest);
        if hash01(self.cfg.seed ^ 0x7ee, x, 0, z) >= density {
            return None;
        }
        // The footing must be the surface itself, not the roof of a cave mouth.
        if self.is_cave_at(x, h, z) || self.is_cave_at(x, h - 1, z) {
            return None;
        }
        let extra = self.cfg.trunk_extra.max(1);
        Some(self.cfg.trunk_min.max(1) + (hash01(self.cfg.seed ^ 0x7ee2, x, 0, z) * extra as f64) as i64)
    }

    /// The tree that grows from the lattice square `(ci, cj)` of a biome
    /// planet (canonical squares), if any: its column, the column's height,
    /// the kind and the seed it grows from. Trees stand on dry, gentle
    /// ground of a biome that grows them, as thick as its density and the
    /// forest noise allow, never on a river or over a cave mouth.
    fn biome_tree_uncached(&self, bw: &BiomeWorld, ci: i64, cj: i64) -> Option<TreeHit> {
        let seed = self.cfg.seed;
        let h0 = hash3(seed ^ 0x07ee_5eed, ci, 0, cj);
        let x = ci * TREE_CELL + (h0 & 3) as i64;
        let z = cj * TREE_CELL + ((h0 >> 2) & 3) as i64;
        // Cheap tests first: the dice, then the land.
        let roll = ((h0 >> 11) as f64) / (1u64 << 53) as f64;
        let land = &bw.lands[bw.layout.biome_at(x as f64, z as f64) as usize];
        if land.tree_weight <= 0.0 || land.tree_density <= 0.0 {
            return None;
        }
        let forest = smoothstep(-0.2, 0.4, self.forest.sample2(x as f64, z as f64, self.width));
        let chance = land.tree_density * (0.15 + 2.0 * forest) * (TREE_CELL * TREE_CELL) as f64;
        if roll >= chance {
            return None;
        }
        let c = self.column(bw, x, z);
        let land = &bw.lands[c.biome as usize];
        if land.tree_weight <= 0.0 || c.river > 0.05 {
            return None;
        }
        let sea = self.cfg.sea_level;
        if c.h < sea.max(self.water_level) + 2 || c.h >= sea + self.cfg.tree_line {
            return None;
        }
        let near = |dx: i64, dz: i64| self.column(bw, x + dx, z + dz).h;
        let slope = (near(1, 0) - near(-1, 0)).abs().max((near(0, 1) - near(0, -1)).abs());
        if slope >= 2 || self.is_cave_at(x, c.h, z) || self.is_cave_at(x, c.h - 1, z) {
            return None;
        }
        let mut pick = hash01(seed ^ 0x07ee_5a1c, x, 0, z) * land.tree_weight;
        let tree = land
            .trees
            .iter()
            .position(|t| {
                pick -= t.weight;
                pick < 0.0
            })
            .unwrap_or(0);
        Some(TreeHit {
            x,
            z,
            h: c.h,
            biome: c.biome,
            tree: tree as u8,
            seed: hash3(seed ^ 0x07ee_59a0, x, 0, z),
        })
    }

    /// [`Self::biome_tree_uncached`], remembered.
    fn biome_tree(&self, bw: &BiomeWorld, ci: i64, cj: i64) -> Option<TreeHit> {
        if let Some(hit) = self.tree_sites.lock().ok().and_then(|m| m.get(&(ci, cj)).copied()) {
            return hit;
        }
        let hit = self.biome_tree_uncached(bw, ci, cj);
        if let Ok(mut m) = self.tree_sites.lock() {
            if m.len() >= TREE_SITE_CACHE {
                m.clear();
            }
            m.insert((ci, cj), hit);
        }
        hit
    }

    /// Plants a biome planet's trees in the chunk at `origin`: every tree
    /// whose lattice square lies within reach, whole where it overlaps.
    fn place_biome_trees(&self, bw: &BiomeWorld, origin: BlockPos, blocks: &mut [BlockState], max_h: i64) {
        if !bw.any_trees || origin.y > max_h + self.tree_top {
            return;
        }
        let cs = CHUNK_SIZE as i64;
        let w = self.shape.width_blocks();
        let r = bw.tree_reach + 1;
        let air = self.ids.air;
        let state = |id: BlockId, wx: i64, wz: i64| match self.colours.slot(id) {
            Some(slot) => {
                let (a, b) = self.region_point(wx, wz);
                BlockState::coloured(id, self.colours.blocks[slot].pick(a, b))
            }
            None => BlockState::plain(id),
        };
        let c0 = (origin.x - r).div_euclid(TREE_CELL);
        let c1 = (origin.x + cs + r).div_euclid(TREE_CELL);
        let k0 = (origin.z - r).div_euclid(TREE_CELL);
        let k1 = (origin.z + cs + r).div_euclid(TREE_CELL);
        let n = w / TREE_CELL;
        for cj in k0..=k1 {
            for ci in c0..=c1 {
                let Some(hit) = self.biome_tree(bw, ci.rem_euclid(n), cj.rem_euclid(n)) else {
                    continue;
                };
                let (tx, tz, h, seed) = (hit.x, hit.z, hit.h, hit.seed);
                let tree = &bw.lands[hit.biome as usize].trees[hit.tree as usize];
                // Where it stands as seen from this chunk (its lap).
                let bx = tx + (ci - ci.rem_euclid(n)) * TREE_CELL;
                let bz = tz + (cj - cj.rem_euclid(n)) * TREE_CELL;
                let (_, up) = tree.shape.reach();
                if origin.y > h + 1 + up as i64 || origin.y + cs <= h - 6 {
                    continue;
                }
                let size = (tree.size + (hash01(seed, 1, 0, 0) - 0.5) * 0.6).clamp(0.0, 1.0);
                let grown = tree.shape.grow(seed, size);
                let parts = [
                    state(tree.wood[0], tx, tz),
                    state(tree.wood[1], tx, tz),
                    state(tree.wood[2], tx, tz),
                ];
                let leafy = |id: BlockId| id == air || id == tree.wood[1] || id == tree.wood[2];
                let mut put = |x: i64, y: i64, z: i64, st: BlockState, trunk: bool| {
                    let (dx, dy, dz) = (x - origin.x, y - origin.y, z - origin.z);
                    if (0..cs).contains(&dx) && (0..cs).contains(&dy) && (0..cs).contains(&dz) {
                        let i = LocalPos::new(dx as u8, dy as u8, dz as u8).index();
                        let here = blocks[i].id;
                        if here == air || (trunk && leafy(here)) {
                            blocks[i] = st;
                        }
                    }
                };
                for &(dx, dy, dz, part) in &grown.blocks {
                    let (x, y, z) = (bx + dx as i64, h + 1 + dy as i64, bz + dz as i64);
                    let (st, trunk) = match part {
                        trees::Part::Trunk => (parts[0], true),
                        trees::Part::Leaves => (parts[1], false),
                        trees::Part::Leaves2 => (parts[2], false),
                    };
                    put(x, y, z, st, trunk);
                    // A trunk's foot reaches down to the ground under it, so
                    // wide trunks and roots never stand on air.
                    if trunk && dy == 0 && (dx, dz) != (0, 0) {
                        let ground = self.column(bw, x.rem_euclid(w), z.rem_euclid(w)).h;
                        for fy in (ground + 1).max(h - 5)..y {
                            put(x, fy, z, st, true);
                        }
                    }
                }
            }
        }
    }

    /// The colour-region fields at canonical column `(wx, wz)`, sampled
    /// directly (trees reach past the chunk's region lattice).
    fn region_point(&self, wx: i64, wz: i64) -> (f64, f64) {
        if self.colours.is_empty() {
            return (0.0, 0.0);
        }
        let (x, z) = (wx as f64, wz as f64);
        let w = self.region_wobble.sample2(x, z, self.width) * REGION_WOBBLE;
        (
            self.region_a.sample2(x, z, self.width) + w,
            self.region_b.sample2(x, z, self.width) - w,
        )
    }

    /// The tree density of canonical column `(x, z)`.
    fn tree_density(&self, x: i64, z: i64) -> f64 {
        match &self.biome {
            Some(bw) => bw.lands[bw.layout.biome_at(x as f64, z as f64) as usize].tree_density,
            None => self.cfg.tree_density,
        }
    }

    fn place_trees(
        &self,
        pos: ChunkPos,
        blocks: &mut [BlockState],
        height_at: &impl Fn(i64, i64) -> i64,
        density_at: &impl Fn(i64, i64) -> f64,
        regions: &[(f64, f64)],
    ) {
        let any = match &self.biome {
            Some(bw) => bw.any_trees,
            None => self.cfg.tree_density > 0.0,
        };
        if !any {
            return;
        }
        let ids = &self.ids;
        // A whole tree takes the colours of the column it grows from, so a
        // canopy is one colour even across a region border.
        let tree_states = |a: f64, b: f64| {
            let state = |id: BlockId| match self.colours.slot(id) {
                Some(s) => BlockState::coloured(id, self.colours.blocks[s].pick(a, b)),
                None => BlockState::plain(id),
            };
            (state(ids.trunk), state(ids.foliage))
        };
        let origin = pos.origin();
        let w = self.shape.width_blocks();
        let cs = CHUNK_SIZE as i64;
        let seed = self.cfg.seed;
        for lz in -TREE_REACH..cs + TREE_REACH {
            for lx in -TREE_REACH..cs + TREE_REACH {
                let h = height_at(lx, lz);
                if origin.y > h + self.tree_top || origin.y + cs <= h - 2 {
                    continue;
                }
                if slope_at(height_at, lx, lz) >= 3 {
                    continue;
                }
                let (wx, wz) = ((origin.x + lx).rem_euclid(w), (origin.z + lz).rem_euclid(w));
                let Some(trunk) = self.tree_at(wx, wz, h, density_at(lx, lz)) else {
                    continue;
                };
                let (trunk_state, foliage_state) = if regions.is_empty() {
                    (BlockState::plain(ids.trunk), BlockState::plain(ids.foliage))
                } else {
                    let (a, b) = self.region_at(regions, lx, lz, wx, wz);
                    tree_states(a, b)
                };
                let base = h + 1;
                let crown = base + trunk - 1;
                let mut put = |bx: i64, by: i64, bz: i64, state: BlockState, replace: bool| {
                    let (dx, dy, dz) = (bx - origin.x, by - origin.y, bz - origin.z);
                    if (0..cs).contains(&dx) && (0..cs).contains(&dy) && (0..cs).contains(&dz) {
                        let i = LocalPos::new(dx as u8, dy as u8, dz as u8).index();
                        if replace || blocks[i].id == ids.air {
                            blocks[i] = state;
                        }
                    }
                };
                let (bx, bz) = (origin.x + lx, origin.z + lz);
                for oy in -2i64..=2 {
                    let r: i64 = if oy >= 1 { 1 } else { 2 };
                    for oz in -r..=r {
                        for ox in -r..=r {
                            let corner = ox.abs() == r && oz.abs() == r;
                            if corner && (oy == 2 || hash01(seed ^ 0x1eaf, wx + ox, crown + oy, wz + oz) < 0.5) {
                                continue;
                            }
                            put(bx + ox, crown + oy, bz + oz, foliage_state, false);
                        }
                    }
                }
                for y in base..=crown {
                    put(bx, y, bz, trunk_state, true);
                }
            }
        }
    }

    /// Gatherable objects, after the trees so nothing grows inside a trunk.
    /// A prop stands in an air cell (never water, ice or a solid) whose
    /// ground, the block right under it, is in the same chunk: what grows in
    /// a chunk then depends only on that chunk's own blocks, so a prop never
    /// floats over ground a player dug out of a saved chunk below. (Cells at
    /// the bottom of a chunk grow nothing; that costs a thirty-second of the
    /// spots.) One prop per cell: the first in the list to grow there.
    fn place_flora(&self, origin: BlockPos, blocks: &mut [BlockState], height_at: &impl Fn(i64, i64) -> i64) {
        if self.flora.is_empty() {
            return;
        }
        let cs = CHUNK_SIZE as i64;
        let air = self.ids.air;
        let surface = self.flora.iter().any(|f| !f.cave);
        // The shallowest any cave flora grows.
        let cave_depth = self.flora.iter().filter(|f| f.cave).map(|f| f.min_depth).min();
        let index = |lx: i64, ly: i64, lz: i64| LocalPos::new(lx as u8, ly as u8, lz as u8).index();
        for lz in 0..cs {
            for lx in 0..cs {
                let h = height_at(lx, lz);
                let (wx, wz) = (origin.x + lx, origin.z + lz);
                let grow = |ly: i64, cave: bool, blocks: &mut [BlockState]| {
                    let cell = index(lx, ly, lz);
                    if blocks[cell].id != air {
                        return;
                    }
                    let ground = blocks[index(lx, ly - 1, lz)].id;
                    let wy = origin.y + ly;
                    let depth = h - wy;
                    for f in &self.flora {
                        if f.cave != cave
                            || wy < f.min_y
                            || wy > f.max_y
                            || (cave && (depth < f.min_depth || depth > f.max_depth))
                            || !f.on.contains(&ground)
                            || hash01(f.key, wx, wy, wz) >= f.density
                        {
                            continue;
                        }
                        if let Some((noise, above)) = &f.patch
                            && noise.sample2(wx as f64, wz as f64, self.width) <= *above
                        {
                            continue;
                        }
                        if f.near_water && !self.near_water(height_at, lx, lz) {
                            continue;
                        }
                        blocks[cell] = BlockState::plain(f.block);
                        return;
                    }
                };
                if surface {
                    let ly = h + 1 - origin.y;
                    if (1..cs).contains(&ly) {
                        grow(ly, false, blocks);
                    }
                }
                if let Some(min_depth) = cave_depth {
                    let top = (h - min_depth - origin.y).min(cs - 1);
                    for ly in 1..=top {
                        grow(ly, true, blocks);
                    }
                }
            }
        }
    }

    /// Whether a column within [`NEAR_WATER`] of chunk column `(lx, lz)`
    /// holds open water (its surface lies under the water line).
    fn near_water(&self, height_at: &impl Fn(i64, i64) -> i64, lx: i64, lz: i64) -> bool {
        (-NEAR_WATER..=NEAR_WATER)
            .any(|dz| (-NEAR_WATER..=NEAR_WATER).any(|dx| height_at(lx + dx, lz + dz) < self.water_level))
    }

    /// A safe place to stand, and for the server's warp shrine: dry, nearly
    /// flat land near the planet's origin, not over a cave, with no tree
    /// close enough to reach over it. Returns the first air block above the
    /// ground.
    pub fn spawn_point(&self) -> BlockPos {
        let dry = self.cfg.sea_level.max(self.water_level);
        let w = self.shape.width_blocks();
        for r in 0..w / 16 {
            for i in -r..=r {
                for (x, z) in [(i, -r), (i, r), (-r, i), (r, i)] {
                    let (x, z) = ((x * 8).rem_euclid(w), (z * 8).rem_euclid(w));
                    let h = self.surface_height(x, z);
                    if h > dry + 2 && h < dry + 40 && self.good_spawn(x, z, h) {
                        return BlockPos::new(x, h + 1, z);
                    }
                }
            }
        }
        BlockPos::new(0, self.shape.height_blocks() - self.tree_top, 0)
    }

    fn good_spawn(&self, x: i64, z: i64, h: i64) -> bool {
        let w = self.shape.width_blocks();
        let at = |dx: i64, dz: i64| ((x + dx).rem_euclid(w), (z + dz).rem_euclid(w));
        // Flat: every neighbour within one block, so the ground is the surface
        // block (not a cliff) and the shrine sits level.
        for dz in -1..=1 {
            for dx in -1..=1 {
                let (nx, nz) = at(dx, dz);
                if (self.surface_height(nx, nz) - h).abs() > 1 {
                    return false;
                }
            }
        }
        if self.is_cave_at(x, h, z) || self.is_cave_at(x, h - 1, z) {
            return false;
        }
        // No trunk or canopy over the spot.
        if let Some(bw) = &self.biome {
            let r = bw.tree_reach + 1;
            let n = w / TREE_CELL;
            for cj in (z - r).div_euclid(TREE_CELL)..=(z + r).div_euclid(TREE_CELL) {
                for ci in (x - r).div_euclid(TREE_CELL)..=(x + r).div_euclid(TREE_CELL) {
                    if let Some(hit) = self.biome_tree(bw, ci.rem_euclid(n), cj.rem_euclid(n)) {
                        let (tx, tz) = (hit.x, hit.z);
                        let tree = &bw.lands[hit.biome as usize].trees[hit.tree as usize];
                        let reach = tree.shape.reach().0 as i64;
                        // Wrapped distances: the planet is a torus.
                        let wrap = |a: i64, b: i64| {
                            let d = (a - b).rem_euclid(w);
                            d.min(w - d)
                        };
                        if wrap(tx, x) <= reach + 1 && wrap(tz, z) <= reach + 1 {
                            return false;
                        }
                    }
                }
            }
            return true;
        }
        for dz in -TREE_REACH..=TREE_REACH {
            for dx in -TREE_REACH..=TREE_REACH {
                let (nx, nz) = at(dx, dz);
                if self
                    .tree_at(nx, nz, self.surface_height(nx, nz), self.tree_density(nx, nz))
                    .is_some()
                {
                    return false;
                }
            }
        }
        true
    }
}

fn slope_at(height_at: &impl Fn(i64, i64) -> i64, lx: i64, lz: i64) -> i64 {
    (height_at(lx + 1, lz) - height_at(lx - 1, lz))
        .abs()
        .max((height_at(lx, lz + 1) - height_at(lx, lz - 1)).abs())
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Trilinear interpolation; corners in `[x][y][z]` order (index `x*4+y*2+z`).
#[inline]
fn trilerp(c: [f64; 8], tx: f64, ty: f64, tz: f64) -> f64 {
    let l = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = l(c[0], c[4], tx);
    let x01 = l(c[1], c[5], tx);
    let x10 = l(c[2], c[6], tx);
    let x11 = l(c[3], c[7], tx);
    l(l(x00, x10, ty), l(x01, x11, ty), tz)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generator() -> Generator {
        let cfg = PlanetGenConfig {
            seed: 42,
            ..Default::default()
        };
        Generator::new(PlanetShape::new(64, 12), cfg, &BlockRegistry::builtin()).unwrap()
    }

    /// Milestone 27: the atlas's density estimate tracks the terrain: the
    /// chunk columns it rates richest in copper hold more copper than the
    /// ones it rates poorest, and a block the planet never makes reads 0.
    #[test]
    fn density_follows_the_ore_it_estimates() {
        let g = kind("temperate");
        let copper = id("copper_vein");
        let mut cols: Vec<(f64, usize)> = Vec::new();
        for cz in 0..5 {
            for cx in 0..5 {
                let mut est = 0.0;
                for z in (0..32).step_by(4) {
                    for x in (0..32).step_by(4) {
                        est += g.density(&[copper], cx * 32 + x, cz * 32 + z);
                    }
                }
                let mut real = 0;
                for cy in 0..12 {
                    let c = g.generate(ChunkPos::new(cx as i32, cy, cz as i32));
                    real += c.to_dense_states().iter().filter(|s| s.id == copper).count();
                }
                cols.push((est, real));
            }
        }
        cols.sort_by(|a, b| a.0.total_cmp(&b.0));
        let third = cols.len() / 3;
        let low: usize = cols[..third].iter().map(|c| c.1).sum();
        let high: usize = cols[cols.len() - third..].iter().map(|c| c.1).sum();
        assert!(high > low, "richest {high} against poorest {low}: {cols:?}");
        assert_eq!(g.density(&[id("gold_seam")], 40, 40), 0.0, "no gold on a tier 1 world");
    }

    fn kind(name: &str) -> Generator {
        let blocks = BlockRegistry::builtin();
        let cfg = PlanetKinds::builtin(&blocks).get(name).unwrap().generation(42).single();
        Generator::new(PlanetShape::new(64, 12), cfg, &blocks).unwrap()
    }

    /// A kind's settings as planets made before M8 saved them: the same, but
    /// without the wayshard ore appended to every kind's list.
    fn before_wayshards(cfg: PlanetGenConfig) -> PlanetGenConfig {
        let mut cfg = before_m11(cfg);
        assert_eq!(cfg.ores.last().map(|o| o.block.as_str()), Some("wayshard_vein"));
        cfg.ores.pop();
        cfg
    }

    /// A kind's settings as planets made before M15 saved them: without the
    /// relic cache (every kind) and silver (tier 3) appended to the ores.
    fn before_m15(cfg: PlanetGenConfig) -> PlanetGenConfig {
        let mut cfg = before_m16(cfg);
        if cfg.ores.last().map(|o| o.block.as_str()) == Some("silver_seam") {
            cfg.ores.pop();
        }
        assert_eq!(cfg.ores.last().map(|o| o.block.as_str()), Some("relic_cache"));
        cfg.ores.pop();
        cfg
    }

    /// A kind's settings as planets made before M16 part 2 saved them:
    /// without the wild crops appended to the flora.
    fn before_m16(cfg: PlanetGenConfig) -> PlanetGenConfig {
        let mut cfg = before_m17(cfg);
        let n = cfg
            .flora
            .iter()
            .rposition(|f| !CROP_FLORA.contains(&f.block.as_str()))
            .map_or(0, |i| i + 1);
        assert!(n < cfg.flora.len(), "wild crops appended");
        cfg.flora.truncate(n);
        cfg
    }

    /// A kind's settings as planets made before M17 saved them: without the
    /// two fossil seams at the end of the ores.
    fn before_m17(cfg: PlanetGenConfig) -> PlanetGenConfig {
        let mut cfg = before_m18(cfg);
        for seam in ["medium_fossil_seam", "small_fossil_seam"] {
            assert_eq!(cfg.ores.last().map(|o| o.block.as_str()), Some(seam));
            cfg.ores.pop();
        }
        cfg
    }

    /// A kind's settings as planets made before M18 saved them: without the
    /// seams appended to the ores (medium coal everywhere, hard coal on tier
    /// 3, relic cores everywhere). Only the four kinds that existed before M18 have such
    /// planets.
    fn before_m18(mut cfg: PlanetGenConfig) -> PlanetGenConfig {
        cfg = before_m26(cfg);
        assert_eq!(cfg.ores.last().map(|o| o.block.as_str()), Some("relic_core_seam"));
        cfg.ores.pop();
        if cfg.ores.last().map(|o| o.block.as_str()) == Some("hard_coal_seam") {
            cfg.ores.pop();
        }
        assert_eq!(cfg.ores.last().map(|o| o.block.as_str()), Some("medium_coal_seam"));
        cfg.ores.pop();
        cfg
    }

    /// A kind's settings as planets made before M26 saved them: glow
    /// crystals natural (no colour choices).
    fn before_m26(mut cfg: PlanetGenConfig) -> PlanetGenConfig {
        cfg.colours.remove("glow_crystal");
        cfg
    }

    /// The kinds that existed before milestone 18: the only ones with older
    /// planets to keep.
    const ORIGINAL_KINDS: [&str; 4] = ["temperate", "arid", "frozen", "lush"];

    fn original_kinds(kinds: &PlanetKinds) -> impl Iterator<Item = &crate::PlanetKind> {
        kinds.kinds.iter().filter(|k| ORIGINAL_KINDS.contains(&k.name.as_str()))
    }

    const CROP_FLORA: [&str; 7] = [
        "tuber_plant_4",
        "dewberry_bush_4",
        "oats_4",
        "rice_4",
        "waxy_tuber_plant_4",
        "wheat_4",
        "exotic_tuber_plant_4",
    ];

    /// A kind's settings as planets made before M11 saved them: without the
    /// relic seam at the end of the ores, and with no flora.
    fn before_m11(cfg: PlanetGenConfig) -> PlanetGenConfig {
        let mut cfg = before_m15(cfg);
        assert_eq!(cfg.ores.last().map(|o| o.block.as_str()), Some("relic_seam"));
        assert!(!cfg.flora.is_empty());
        cfg.ores.pop();
        cfg.flora.clear();
        cfg
    }

    fn all_kinds() -> Vec<(String, u8, Generator)> {
        let blocks = BlockRegistry::builtin();
        PlanetKinds::builtin(&blocks)
            .kinds
            .iter()
            .map(|k| {
                let g = Generator::new(PlanetShape::new(64, 12), k.generation(42).single(), &blocks).unwrap();
                (k.name.clone(), k.tier, g)
            })
            .collect()
    }

    fn id(name: &str) -> BlockId {
        BlockRegistry::builtin().id(name).unwrap()
    }

    fn block_at(g: &Generator, p: BlockPos) -> BlockId {
        g.generate(p.chunk()).get(p.local())
    }

    fn fnv(bytes: &[u8], mut h: u64) -> u64 {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h
    }

    const FINGERPRINT_CHUNKS: [(i32, i32, i32); 5] = [(0, 3, 0), (0, 4, 0), (10, 3, 20), (40, 5, 7), (63, 2, 1)];

    /// Block types only, as version 2 pinned them: proves a change left the
    /// shape of the terrain alone.
    fn fingerprint_ids(g: &Generator) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325;
        for (x, y, z) in FINGERPRINT_CHUNKS {
            // The blocks themselves, not their encoding: a new chunk format
            // must not look like new terrain.
            let bytes: Vec<u8> = g
                .generate(ChunkPos::new(x, y, z))
                .to_dense()
                .iter()
                .flat_map(|b| b.0.to_le_bytes())
                .collect();
            h = fnv(&bytes, h);
        }
        h
    }

    /// Everything about every block: type, colour and shape.
    fn fingerprint(g: &Generator) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325;
        for (x, y, z) in FINGERPRINT_CHUNKS {
            let bytes: Vec<u8> = g
                .generate(ChunkPos::new(x, y, z))
                .to_dense_states()
                .iter()
                .flat_map(|b| {
                    let [i0, i1] = b.id.0.to_le_bytes();
                    let [s0, s1] = b.shape.0.to_le_bytes();
                    [i0, i1, b.colour.0, s0, s1]
                })
                .collect();
            h = fnv(&bytes, h);
        }
        h
    }

    #[test]
    fn same_seed_gives_identical_chunks() {
        for (name, _, a) in all_kinds() {
            let b = kind(&name);
            for p in [
                ChunkPos::new(0, 3, 0),
                ChunkPos::new(17, 4, -5),
                ChunkPos::new(63, 2, 63),
            ] {
                assert_eq!(a.generate(p).encode(), b.generate(p).encode(), "{name}");
            }
        }
    }

    #[test]
    fn chunks_do_not_depend_on_generation_order() {
        // A fresh generator per order: nothing generated earlier may leak
        // into a later chunk.
        let ps: Vec<ChunkPos> = (0..6).map(|i| ChunkPos::new(i * 3, 2 + i % 3, 5 - i)).collect();
        for (name, _, forward) in all_kinds() {
            let a: Vec<_> = ps.iter().map(|&p| forward.generate(p)).collect();
            let backward = kind(&name);
            let mut b: Vec<_> = ps.iter().rev().map(|&p| backward.generate(p)).collect();
            b.reverse();
            assert_eq!(a, b, "{name}");
        }
    }

    #[test]
    fn chunks_wrap_with_the_planet() {
        let g = generator();
        assert_eq!(
            g.generate(ChunkPos::new(3, 4, 5)),
            g.generate(ChunkPos::new(3 + 64, 4, 5 - 128))
        );
    }

    /// Version 1's pin. The default settings (the temperate kind) still make
    /// exactly that terrain, so a version 1 planet opens unchanged as a
    /// version 2 planet with default settings.
    #[test]
    fn default_settings_reproduce_version_1() {
        // 0x7d6bf6bb16c76ecf when fingerprints hashed the chunk encoding;
        // re-pinned on 2026-10-19 when they began hashing the blocks
        // themselves, after checking main and the new chunk format give the
        // same value (the terrain did not change).
        assert_eq!(fingerprint_ids(&generator()), 0x4d00_41c8_06c3_f24c);
        let blocks = BlockRegistry::builtin();
        let temperate = before_wayshards(
            PlanetKinds::builtin(&blocks)
                .get("temperate")
                .unwrap()
                .generation(42)
                .single(),
        );
        assert_eq!(
            temperate.ores,
            generator().config().ores,
            "temperate lists the default ores first"
        );
        let temperate = Generator::new(PlanetShape::new(64, 12), temperate, &blocks).unwrap();
        assert_eq!(fingerprint_ids(&temperate), 0x4d00_41c8_06c3_f24c);
        let v1 = PlanetGenConfigV1 {
            seed: 42,
            sea_level: 128,
            snow_line: 112,
            mountain_height: 130.0,
            tree_density: 0.012,
        };
        assert_eq!(PlanetGenConfig::from(v1), generator().config().clone());
    }

    /// Version 3 coloured the blocks and left their types alone: each kind
    /// with no colours makes exactly version 2's terrain (these are version
    /// 2's pins), and so does each kind with its colours, block type for
    /// block type. A version 2 planet, whose settings read as no colours,
    /// therefore opens unchanged and stays natural.
    #[test]
    fn no_colours_reproduce_version_2() {
        let blocks = BlockRegistry::builtin();
        for (name, pinned) in PINNED_V2 {
            let k = PlanetKinds::builtin(&blocks);
            let with = before_wayshards(k.get(name).unwrap().generation(42).single());
            assert!(!with.colours.is_empty(), "{name} has no colours");
            let without = PlanetGenConfig {
                colours: BTreeMap::new(),
                ..with.clone()
            };
            let g = Generator::new(PlanetShape::new(64, 12), without.clone(), &blocks).unwrap();
            assert_eq!(fingerprint_ids(&g), pinned, "{name} without colours");
            for (x, y, z) in FINGERPRINT_CHUNKS {
                assert!(
                    g.generate(ChunkPos::new(x, y, z))
                        .palette()
                        .iter()
                        .all(|s| s.colour == Colour::NATURAL)
                );
            }
            // Saved as version 2 saved it (no `colours` key), it reads back
            // as no colours.
            let text = toml::to_string(&without).unwrap();
            assert!(!text.contains("colours"), "{text}");
            assert_eq!(toml::from_str::<PlanetGenConfig>(&text).unwrap(), without);
            let g = Generator::new(PlanetShape::new(64, 12), with, &blocks).unwrap();
            assert_eq!(fingerprint_ids(&g), pinned, "{name} with colours changed block types");
        }
        assert!(opens_generator_version(2) && opens_generator_version(GENERATOR_VERSION));
        assert!(!opens_generator_version(1));
    }
    const PINNED_V2: [(&str, u64); 4] = [
        ("temperate", 0x4d00_41c8_06c3_f24c),
        ("arid", 0x73a4_2ab0_3f53_7644),
        ("frozen", 0xdb2c_2f7d_dce3_2c63),
        ("lush", 0xbdf8_252a_fa20_5411),
    ];

    /// M8 appended wayshards to every kind's ores (ADR-0009). Planets made
    /// before keep their saved ore lists and so their terrain exactly (these
    /// are generator 3's pins from before M8); the server appends the new ore
    /// to them on opening, which may only turn rock into wayshard, never
    /// move or remove anything else.
    #[test]
    fn appending_wayshards_only_turns_rock_into_wayshard() {
        let blocks = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&blocks);
        let rock = ["stone", "deep_stone", "sandstone"].map(id);
        let wayshard = id("wayshard_vein");
        for (name, pinned) in PINNED_M7 {
            // As planets made between M8 and M11 have it, and before M8.
            let full = kinds.get(name).unwrap().generation(42).single();
            let with = before_m11(full.clone());
            let without = before_wayshards(full);
            let shape = PlanetShape::new(64, 12);
            let old = Generator::new(shape, without, &blocks).unwrap();
            let new = Generator::new(shape, with, &blocks).unwrap();
            assert_eq!(fingerprint(&old), pinned, "{name} before wayshards");
            let mut found = 0;
            for x in 0..8 {
                for y in 0..3 {
                    let pos = ChunkPos::new(x * 7, y, x * 13);
                    let (a, b) = (old.generate(pos).to_dense_states(), new.generate(pos).to_dense_states());
                    for (a, b) in a.iter().zip(&b) {
                        if a != b {
                            assert!(rock.contains(&a.id) && b.id == wayshard, "{name}: {a:?} became {b:?}");
                            found += 1;
                        }
                    }
                }
            }
            assert!(found > 0, "{name} has no wayshards in its deep rock");
        }
    }
    const PINNED_M7: [(&str, u64); 4] = [
        ("temperate", 0xdcf6_1868_0cb9_14df),
        ("arid", 0xc1f7_d350_fed3_af65),
        ("frozen", 0x04e5_7bc8_b505_2127),
        ("lush", 0xc4ba_2a84_be17_4d24),
    ];

    /// Pins what the generator produces for every built-in kind, colours
    /// included. If this fails the terrain changed: bump GENERATOR_VERSION
    /// and re-pin, in the same commit (a change to a kind's data alone, such
    /// as M8's appended ore, re-pins without a bump only if planets saved
    /// with the old data still make their old terrain).
    /// A biome planet: chunks are pure functions of their place (any order,
    /// either side of the seam), and its land holds several biomes' covers,
    /// rivers below the water line, and bands of all three rocks.
    #[test]
    fn biome_planets_are_varied_and_seamless() {
        let blocks = BlockRegistry::builtin();
        let cfg = PlanetKinds::builtin(&blocks).get("temperate").unwrap().generation(7);
        assert_eq!(cfg.biomes.len(), 15);
        let shape = PlanetShape::new(48, 12);
        let g = Generator::new(shape, cfg.clone(), &blocks).unwrap();
        let again = Generator::new(shape, cfg, &blocks).unwrap();
        let w = shape.width_blocks() as i32 / 32;
        for p in [
            ChunkPos::new(0, 4, 0),
            ChunkPos::new(w - 1, 4, 3),
            ChunkPos::new(20, 3, 31),
        ] {
            assert_eq!(g.generate(p).encode(), again.generate(p).encode());
            let lap = ChunkPos::new(p.x + w, p.y, p.z - w);
            assert_eq!(
                g.generate(p).encode(),
                g.generate(lap).encode(),
                "{p:?} across the seam"
            );
        }
        // Tops across the planet: several covers, and every rock kind in
        // the walls under them.
        let mut tops = std::collections::BTreeSet::new();
        let mut rocks = std::collections::BTreeSet::new();
        let rock_ids = [id("stone"), id("basalt"), id("limestone")];
        for cz in (0..w).step_by(3) {
            for cx in (0..w).step_by(3) {
                let h = g.surface_height(cx as i64 * 32 + 5, cz as i64 * 32 + 9);
                let c = g.generate(ChunkPos::new(cx, (h / 32) as i32, cz));
                tops.insert(c.get(LocalPos::new(5, (h % 32) as u8, 9)));
                for (i, b) in c.to_dense().iter().enumerate() {
                    if rock_ids.contains(b) && i % 7 == 0 {
                        rocks.insert(*b);
                    }
                }
            }
        }
        assert!(tops.len() >= 4, "{tops:?}");
        assert_eq!(rocks.len(), 3, "{rocks:?}");
    }

    /// Biome trees are whole across chunk borders: every trunk block stands
    /// on something (trunk, ground or a root), looked up in whichever chunk
    /// holds the block under it, and every kind of trunk grows somewhere.
    #[test]
    fn biome_trees_stand_on_something_across_chunks() {
        let blocks = BlockRegistry::builtin();
        let cfg = PlanetKinds::builtin(&blocks).get("temperate").unwrap().generation(7);
        let g = Generator::new(PlanetShape::new(48, 12), cfg, &blocks).unwrap();
        let trunks = [id("trunk"), id("pale_trunk"), id("gnarled_trunk")];
        let air = id("air");
        let mut cache: std::collections::HashMap<ChunkPos, Chunk> = std::collections::HashMap::new();
        let mut at = |p: BlockPos| {
            let c = cache.entry(p.chunk()).or_insert_with(|| g.generate(p.chunk()));
            c.get(p.local())
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut checked = 0;
        for cz in 16..21 {
            for cx in 16..21 {
                for cy in 4..6 {
                    let c = g.generate(ChunkPos::new(cx, cy, cz));
                    if c.uniform().is_some() {
                        continue;
                    }
                    let o = ChunkPos::new(cx, cy, cz).origin();
                    for (i, b) in c.to_dense().iter().enumerate() {
                        if !trunks.contains(b) {
                            continue;
                        }
                        seen.insert(*b);
                        let (lx, ly, lz) = ((i % 32) as i64, (i / 1024) as i64, ((i / 32) % 32) as i64);
                        let below = at(BlockPos::new(o.x + lx, o.y + ly - 1, o.z + lz));
                        // Branches reach sideways: only trunk blocks with a
                        // trunk or air beside them at the same height and
                        // nothing over air below count as hanging.
                        if below == air {
                            let side = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| {
                                trunks.contains(&at(BlockPos::new(o.x + lx + dx, o.y + ly, o.z + lz + dz)))
                            });
                            assert!(side, "a lone trunk over air at {:?}", (o.x + lx, o.y + ly, o.z + lz));
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 50, "{checked} trunk blocks");
        assert!(seen.len() >= 2, "{seen:?}");
    }

    /// Part 3: a planet of only floating isles holds land well clear of the
    /// ground, topped with its cover; the same planet without the lift has
    /// none, and caves by tier add shafts on tier 6 and linings from tier 2.
    #[test]
    fn lifted_land_floats_and_caves_follow_the_tier() {
        let blocks = BlockRegistry::builtin();
        let cat = crate::BiomeCatalogue::builtin();
        let isles = cat
            .biomes
            .iter()
            .find(|b| b.spec.name == "sky_isles")
            .unwrap()
            .spec
            .clone();
        let cfg = PlanetGenConfig {
            seed: 3,
            biomes: vec![isles.clone()],
            ..Default::default()
        };
        let shape = PlanetShape::new(16, 12);
        let g = Generator::new(shape, cfg.clone(), &blocks).unwrap();
        let flat = Generator::new(
            shape,
            PlanetGenConfig {
                biomes: vec![crate::BiomeSpec { lift: None, ..isles }],
                ..cfg
            },
            &blocks,
        )
        .unwrap();
        let (limestone, air) = (id("limestone"), id("air"));
        let count = |g: &Generator| {
            let mut n = 0;
            let mut tops = std::collections::BTreeSet::new();
            for cz in 0..16 {
                for cx in 0..16 {
                    for cy in 5..8 {
                        let c = g.generate(ChunkPos::new(cx, cy, cz));
                        let o = ChunkPos::new(cx, cy, cz).origin();
                        for lz in (0..32).step_by(4) {
                            for lx in (0..32).step_by(4) {
                                let ground = g.surface_height(o.x + lx, o.z + lz);
                                for ly in 0..32 {
                                    let b = c.get(LocalPos::new(lx as u8, ly as u8, lz as u8));
                                    if o.y + ly as i64 > ground + 20 && b == limestone {
                                        n += 1;
                                        if c.get(LocalPos::new(lx as u8, (ly + 1).min(31) as u8, lz as u8)) != air {
                                            tops.insert(c.get(LocalPos::new(
                                                lx as u8,
                                                (ly + 1).min(31) as u8,
                                                lz as u8,
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            (n, tops)
        };
        let (lifted, tops) = count(&g);
        assert!(lifted > 200, "{lifted} lifted rock blocks");
        assert!(tops.contains(&id("turf")) || tops.contains(&id("soil")), "{tops:?}");
        assert_eq!(count(&flat).0, 0);

        // Shafts only from tier 6; linings from tier 2.
        let tiered = |tier: u8| {
            Generator::new(
                shape,
                PlanetGenConfig {
                    seed: 5,
                    biomes: g.cfg.biomes.clone(),
                    cave_tier: tier,
                    ..Default::default()
                },
                &blocks,
            )
            .unwrap()
        };
        let (t1, t6) = (tiered(1), tiered(6));
        let shafts = |g: &Generator| {
            (0..512)
                .step_by(2)
                .flat_map(|x| (0..512).step_by(2).map(move |z| (x, z)))
                .filter(|&(x, z)| g.shaft_at(x, 100, z, 140))
                .count()
        };
        assert_eq!(shafts(&t1), 0);
        assert!(shafts(&t6) > 10, "{}", shafts(&t6));
        assert!(t1.linings.is_empty() && !tiered(2).linings.is_empty());
    }

    /// Steep ground shows bare rock: no turf where a column rises 4 or more
    /// over its neighbours.
    #[test]
    fn turf_never_holds_on_cliffs() {
        let blocks = BlockRegistry::builtin();
        let cfg = PlanetKinds::builtin(&blocks).get("stormy").unwrap().generation(11);
        let g = Generator::new(PlanetShape::new(48, 12), cfg, &blocks).unwrap();
        let soft = [id("turf"), id("meadow_turf"), id("moss_turf")];
        let mut cliffs = 0;
        for cz in 0..12 {
            for cx in 0..12 {
                for (lx, lz) in [(4i64, 4i64), (20, 13)] {
                    let (x, z) = (cx * 32 + lx, cz * 32 + lz);
                    let h = g.surface_height(x, z);
                    let slope = (g.surface_height(x + 1, z) - g.surface_height(x - 1, z))
                        .abs()
                        .max((g.surface_height(x, z + 1) - g.surface_height(x, z - 1)).abs());
                    if slope < 4 {
                        continue;
                    }
                    cliffs += 1;
                    let c = g.generate(ChunkPos::new(cx as i32, (h / 32) as i32, cz as i32));
                    let top = c.get(LocalPos::new(lx as u8, (h % 32) as u8, lz as u8));
                    assert!(!soft.contains(&top), "turf on a cliff at {x},{h},{z}");
                }
            }
        }
        assert!(cliffs > 0, "no cliffs to check");
    }

    #[test]
    fn no_biomes_reproduce_version_3() {
        let mut got: Vec<(String, u64)> = vec![("default".into(), fingerprint(&generator()))];
        got.extend(all_kinds().iter().map(|(n, _, g)| (n.clone(), fingerprint(g))));
        let want: Vec<(String, u64)> = PINNED.iter().map(|(n, h)| (n.to_string(), *h)).collect();
        assert_eq!(got, want, "version 3 terrain changed");
        assert!(opens_generator_version(3));
        // Planets saved before M26 (no glow crystal colours) keep theirs.
        let blocks = BlockRegistry::builtin();
        for k in &PlanetKinds::builtin(&blocks).kinds {
            let g = Generator::new(PlanetShape::new(64, 12), before_m26(k.generation(42).single()), &blocks).unwrap();
            let pin = PINNED_M25.iter().find(|(n, _)| *n == k.name).expect("pinned").1;
            assert_eq!(fingerprint(&g), pin, "{} saved before M26 keeps its terrain", k.name);
        }
    }

    /// Pins what generator 4 makes for every built-in kind with its biomes.
    /// If this fails the terrain changed: bump GENERATOR_VERSION and re-pin,
    /// in the same commit.
    #[test]
    fn terrain_fingerprint_is_pinned() {
        let blocks = BlockRegistry::builtin();
        let got: Vec<(String, u64)> = PlanetKinds::builtin(&blocks)
            .kinds
            .iter()
            .map(|k| {
                let g = Generator::new(PlanetShape::new(64, 12), k.generation(42), &blocks).unwrap();
                (k.name.clone(), fingerprint(&g))
            })
            .collect();
        assert_eq!(GENERATOR_VERSION, 4);
        let want: Vec<(String, u64)> = PINNED_V4.iter().map(|(n, h)| (n.to_string(), *h)).collect();
        assert_eq!(got, want, "terrain changed");
        // Planets saved before M26 (no glow crystal colours) keep theirs.
        for k in &PlanetKinds::builtin(&blocks).kinds {
            let g = Generator::new(PlanetShape::new(64, 12), before_m26(k.generation(42)), &blocks).unwrap();
            let pin = PINNED_V4_M25.iter().find(|(n, _)| *n == k.name).expect("pinned").1;
            assert_eq!(fingerprint(&g), pin, "{} saved before M26 keeps its terrain", k.name);
        }
    }
    // Re-pinned for M19 part 2's trees and outcrops and part 3's lifted land
    // and caves (2026-10-25) without a bump:
    // generator 4 had not left the branch, so no planet held the old pins.
    // Re-pinned for M26's glow crystal colours (kind data only) without a
    // version bump: planets saved before keep theirs (`PINNED_V4_M25`).
    const PINNED_V4: [(&str, u64); 16] = [
        ("temperate", 0xebbb_b6ad_2a2d_4428),
        ("arid", 0xf87d_9467_37d9_33a9),
        ("frozen", 0xc9ee_c353_8b86_8989),
        ("lush", 0xf8ac_414c_a1b1_e0e3),
        ("overgrown", 0xcaf5_a93b_71fe_63f2),
        ("rustland", 0x339d_5b75_3ef9_fb6f),
        ("cinderland", 0x8815_3bae_6647_234e),
        ("acrid", 0x7583_155b_9b3d_d99a),
        ("blighted", 0x99d5_27dd_6f4e_7d50),
        ("glacial", 0xca3b_e5e4_bdf2_4821),
        ("stormy", 0x04cd_101d_e331_2a41),
        ("scorched", 0x1b0d_ef2b_2a2e_6956),
        ("ember", 0x5ae9_9f35_c4a7_7236),
        // Milestone 21's tier 7 kinds (new kinds: nothing older changed).
        ("auroral", 0x03f5_05a4_2c79_0069),
        ("pyric", 0xda05_d528_4a70_1bbc),
        ("gloaming", 0x2419_609f_400c_f58e),
    ];
    // Before M26 gave glow crystals their worlds' colours (kind data only,
    // no generator change): what planets saved before still make.
    const PINNED_V4_M25: [(&str, u64); 16] = [
        ("temperate", 0x1c45_7b1d_cfba_bc48),
        ("arid", 0xf87d_9467_37d9_33a9),
        ("frozen", 0xc9ee_c353_8b86_8989),
        ("lush", 0xf8ac_414c_a1b1_e0e3),
        ("overgrown", 0xcaf5_a93b_71fe_63f2),
        ("rustland", 0x339d_5b75_3ef9_fb6f),
        ("cinderland", 0x8815_3bae_6647_234e),
        ("acrid", 0x7583_155b_9b3d_d99a),
        ("blighted", 0x99d5_27dd_6f4e_7d50),
        ("glacial", 0xca3b_e5e4_bdf2_4821),
        ("stormy", 0x04cd_101d_e331_2a41),
        ("scorched", 0x1b0d_ef2b_2a2e_6956),
        ("ember", 0x5ae9_9f35_c4a7_7236),
        // Milestone 21's tier 7 kinds (new kinds: nothing older changed).
        ("auroral", 0x03f5_05a4_2c79_0069),
        ("pyric", 0xda05_d528_4a70_1bbc),
        ("gloaming", 0x2419_609f_400c_f58e),
    ];
    // The version 3 pins: every kind's settings without biomes (what planets
    // saved before generator 4 hold) still make them.
    // Generator 3 was re-pinned before release (2026-10-19) when colour
    // borders got their wobble and the temperate greens were retuned; no
    // planet had been made by the earlier version 3 outside tests. The kinds
    // were re-pinned again for M8's wayshards (2026-10-19), with the old pins
    // kept in `PINNED_M7` for planets made before, and for M11's relic seam
    // and flora (2026-10-20), with the old pins kept in `PINNED_M8`. Both
    // were changes to the kinds' data alone: the default settings (no flora)
    // keep their pin, and a planet's saved settings still make its old
    // terrain (`appending_m11_only_adds_relic_and_props`).
    // Re-pinned for M15's appended ores (relic caches, silver) without a
    // version bump: planets saved before keep their terrain (`PINNED_M14`,
    // checked in `appending_m15_only_turns_rock_into_relic_caches_and_silver`).
    // Re-pinned for M16 part 2's wild crops (appended flora) without a
    // version bump: planets saved before keep their terrain (`PINNED_M15`,
    // checked in `appending_m16_only_grows_crops_on_their_ground`).
    // Re-pinned for M17's fossil seams (appended ores) without a version
    // bump: planets saved before keep their terrain (`PINNED_M16`, checked in
    // `appending_m17_only_turns_rock_into_fossils`).
    // Re-pinned for M18's coal seams (appended ores), its nine new kinds and
    // their gems and lava
    // without a version bump: planets saved before keep their terrain
    // (`PINNED_M17`, checked in `appending_m18_only_turns_rock_into_coal`).
    // Re-pinned for M26's glow crystal colours (kind data only) without a
    // version bump: planets saved before keep theirs (`PINNED_M25`).
    const PINNED: [(&str, u64); 17] = [
        ("default", 0x6bc9_e59d_8877_7736),
        ("temperate", 0x667d_b72f_1f1b_60cb),
        ("arid", 0x157e_220c_2614_6ab8),
        ("frozen", 0xd5e0_f0d6_62fa_8d4a),
        ("lush", 0x6045_df1f_3bec_6f16),
        ("overgrown", 0x5d60_fdc0_4971_0c72),
        ("rustland", 0xee61_8199_3a77_6e62),
        ("cinderland", 0xe11a_e129_c18d_a1d1),
        ("acrid", 0xb510_c490_03f4_f397),
        ("blighted", 0x95cc_bdcf_77cc_eded),
        ("glacial", 0x3bdd_53c0_5161_9ba9),
        ("stormy", 0xf0d9_2e65_ad14_b9d1),
        ("scorched", 0xb4d3_67f8_2bb5_eefc),
        ("ember", 0x2714_019c_6485_d237),
        // Milestone 21's tier 7 kinds (new kinds: nothing older changed).
        ("auroral", 0x7ebe_da1d_e029_5f96),
        ("pyric", 0xea60_e765_e561_9d81),
        ("gloaming", 0x22e1_bd1b_b247_0c79),
    ];
    // Version 3 pins before M26's glow crystal colours (kind data only).
    const PINNED_M25: [(&str, u64); 17] = [
        ("default", 0x6bc9_e59d_8877_7736),
        ("temperate", 0xaae5_c5bd_f326_8a63),
        ("arid", 0x1e58_8500_d8a3_d120),
        ("frozen", 0x8d82_1743_4246_6964),
        ("lush", 0x4878_da2d_016c_ec27),
        ("overgrown", 0x8831_bcaa_b529_9f85),
        ("rustland", 0xaa38_4fa4_295a_f88c),
        ("cinderland", 0xb1c5_f24e_e00d_6755),
        ("acrid", 0x2a22_4a1c_33b4_5015),
        ("blighted", 0x8ae4_33e0_5ea0_2841),
        ("glacial", 0xc357_442f_7e20_db2b),
        ("stormy", 0xda7e_6340_af03_6d5d),
        ("scorched", 0x021e_f8ca_e477_3cac),
        ("ember", 0x93c2_6bbb_86b5_8913),
        // Milestone 21's tier 7 kinds (new kinds: nothing older changed).
        ("auroral", 0x78e0_7c40_3eea_6038),
        ("pyric", 0xc394_31eb_c46c_e425),
        ("gloaming", 0x8c15_15de_cbb3_69db),
    ];
    const PINNED_M17: [(&str, u64); 4] = [
        ("temperate", 0xa10a_36aa_5d69_57f1),
        ("arid", 0x8ef1_bc6b_29ce_448c),
        ("frozen", 0x9068_f146_8540_363b),
        ("lush", 0xb328_fb3d_cdc9_94a1),
    ];
    const PINNED_M16: [(&str, u64); 4] = [
        ("temperate", 0x22cb_b64b_0855_1daa),
        ("arid", 0x31a0_b625_c37a_056f),
        ("frozen", 0xb412_fff4_a47a_bd70),
        ("lush", 0xb327_5465_e360_86ec),
    ];

    /// M17 appended small and medium fossil seams to every kind's ores: an
    /// existing planet's unsaved chunks may only turn rock into fossil.
    #[test]
    fn appending_m17_only_turns_rock_into_fossils() {
        let blocks = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&blocks);
        let rock = ["stone", "deep_stone", "sandstone"].map(id);
        let fossils = [id("small_fossil_seam"), id("medium_fossil_seam")];
        for k in original_kinds(&kinds) {
            let with = before_m26(k.generation(42).single());
            let shape = PlanetShape::new(64, 12);
            let old = Generator::new(shape, before_m17(with.clone()), &blocks).unwrap();
            let new = Generator::new(shape, before_m18(with), &blocks).unwrap();
            let pinned = PINNED_M16.iter().find(|(n, _)| *n == k.name).expect("pinned").1;
            assert_eq!(fingerprint(&old), pinned, "{} before M17 keeps its terrain", k.name);
            let mut found = [0; 2];
            for x in 0..12 {
                for y in 0..5 {
                    let pos = ChunkPos::new(x * 5, y, x * 11);
                    let (a, b) = (old.generate(pos).to_dense_states(), new.generate(pos).to_dense_states());
                    for (a, b) in a.iter().zip(&b) {
                        if a == b {
                            continue;
                        }
                        assert!(rock.contains(&a.id), "{}: {a:?} became {b:?}", k.name);
                        let i = fossils.iter().position(|f| *f == b.id).expect("only fossils appear");
                        found[i] += 1;
                    }
                }
            }
            assert!(found[0] > 0 && found[1] > 0, "{}: fossils {found:?}", k.name);
            assert!(found[0] > found[1], "{}: medium fossils are rarer {found:?}", k.name);
        }
    }

    /// M18 appended medium coal and relic cores to every kind's ores (and
    /// hard coal to the lush kind's): an existing planet's unsaved chunks may
    /// only turn rock into those.
    #[test]
    fn appending_m18_only_turns_rock_into_coal() {
        let blocks = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&blocks);
        // Glow crystals sit in rock under cave floors; coal takes that rock
        // first, as any ore does.
        let rock = ["stone", "deep_stone", "sandstone", "glow_crystal"].map(id);
        let coal = [id("medium_coal_seam"), id("hard_coal_seam"), id("relic_core_seam")];
        for k in original_kinds(&kinds) {
            let with = before_m26(k.generation(42).single());
            // A cave fungus whose rock floor became coal no longer grows.
            let props: Vec<_> = with.flora.iter().map(|f| id(&f.block)).collect();
            let shape = PlanetShape::new(64, 12);
            let old = Generator::new(shape, before_m18(with.clone()), &blocks).unwrap();
            let new = Generator::new(shape, with, &blocks).unwrap();
            let pinned = PINNED_M17.iter().find(|(n, _)| *n == k.name).expect("pinned").1;
            assert_eq!(fingerprint(&old), pinned, "{} before M18 keeps its terrain", k.name);
            let mut found = [0; 3];
            for x in 0..12 {
                for y in 0..5 {
                    let pos = ChunkPos::new(x * 5, y, x * 11);
                    let (a, b) = (old.generate(pos).to_dense_states(), new.generate(pos).to_dense_states());
                    for (a, b) in a.iter().zip(&b) {
                        if a == b {
                            continue;
                        }
                        if props.contains(&a.id) && b.id == BlockId::AIR {
                            continue;
                        }
                        assert!(rock.contains(&a.id), "{}: {a:?} became {b:?}", k.name);
                        let i = coal.iter().position(|c| *c == b.id).expect("only coal appears");
                        found[i] += 1;
                    }
                }
            }
            assert!(
                found[0] > 0 && found[2] > 0,
                "{}: medium coal and relic cores {found:?}",
                k.name
            );
            assert_eq!(
                found[1] > 0,
                k.name == "lush",
                "{}: hard coal only on tier 3 {found:?}",
                k.name
            );
        }
    }

    const PINNED_M15: [(&str, u64); 4] = [
        ("temperate", 0xa6ab_8a36_dcb2_7da0),
        ("arid", 0x31a0_b625_c37a_056f),
        ("frozen", 0xb412_fff4_a47a_bd70),
        ("lush", 0xab67_cc12_6f1d_84d9),
    ];

    /// M16 part 2 appended wild crops to every kind's flora: an existing
    /// planet's unsaved chunks may only turn air into a ripe crop.
    #[test]
    fn appending_m16_only_grows_crops_on_their_ground() {
        let blocks = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&blocks);
        let crops: Vec<BlockId> = CROP_FLORA.iter().map(|n| id(n)).collect();
        let mut grown = 0;
        for k in original_kinds(&kinds) {
            let with = before_m26(k.generation(42).single());
            let shape = PlanetShape::new(64, 12);
            let old = Generator::new(shape, before_m16(with.clone()), &blocks).unwrap();
            let new = Generator::new(shape, before_m17(with), &blocks).unwrap();
            let pinned = PINNED_M15.iter().find(|(n, _)| *n == k.name).expect("pinned").1;
            assert_eq!(fingerprint(&old), pinned, "{} before M16 keeps its terrain", k.name);
            for x in 0..16 {
                for y in 0..6 {
                    let pos = ChunkPos::new(x * 3, y, x * 7);
                    let (a, b) = (old.generate(pos).to_dense_states(), new.generate(pos).to_dense_states());
                    for (a, b) in a.iter().zip(&b) {
                        if a != b {
                            assert!(a.id.is_air() && crops.contains(&b.id), "{}: {a:?} became {b:?}", k.name);
                            grown += 1;
                        }
                    }
                }
            }
        }
        assert!(grown > 0, "some wild crops grew");
    }
    const PINNED_M14: [(&str, u64); 4] = [
        ("temperate", 0xb7f4_90e8_ceaf_2736),
        ("arid", 0x143e_1e07_0a9f_c1e7),
        ("frozen", 0xdcae_5ff1_56e3_06b5),
        ("lush", 0x25eb_0f8a_8283_a981),
    ];

    /// M11 appended the relic seam to every kind's ores and gave every kind
    /// flora. Planets made before keep their saved settings and so their
    /// terrain exactly (these are the pins from before M11); the server
    /// appends the new entries on opening, which may only turn rock into
    /// relic seam and air into a prop standing on its ground.
    #[test]
    fn appending_m11_only_adds_relic_and_props() {
        let blocks = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&blocks);
        let rock = ["stone", "deep_stone", "sandstone"].map(id);
        let relic = id("relic_seam");
        for (name, pinned) in PINNED_M8 {
            let with = before_m26(kinds.get(name).unwrap().generation(42).single());
            let props: Vec<BlockId> = with.flora.iter().map(|f| id(&f.block)).collect();
            let shape = PlanetShape::new(64, 12);
            let old = Generator::new(shape, before_m11(with.clone()), &blocks).unwrap();
            let new = Generator::new(shape, before_m15(with), &blocks).unwrap();
            assert_eq!(fingerprint(&old), pinned, "{name} before M11");
            let (mut relics, mut grown) = (0, 0);
            for x in 0..10 {
                for y in 1..8 {
                    let pos = ChunkPos::new(x * 7, y, x * 13);
                    let (a, b) = (old.generate(pos).to_dense_states(), new.generate(pos).to_dense_states());
                    for (a, b) in a.iter().zip(&b) {
                        if a == b {
                            continue;
                        }
                        if b.id == relic {
                            assert!(rock.contains(&a.id), "{name}: {a:?} became relic");
                            relics += 1;
                        } else {
                            assert!(a.id.is_air() && props.contains(&b.id), "{name}: {a:?} became {b:?}");
                            grown += 1;
                        }
                    }
                }
            }
            assert!(relics > 0 && grown > 0, "{name}: {relics} relic, {grown} props");
        }
    }
    /// M15 appended the relic cache to every kind's ores and silver to tier
    /// 3: on opening, an existing planet's unsaved chunks may only turn rock
    /// into those ores; silver lies only on tier 3 worlds.
    #[test]
    fn appending_m15_only_turns_rock_into_relic_caches_and_silver() {
        let blocks = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&blocks);
        let rock = ["stone", "deep_stone", "sandstone"].map(id);
        let (cache, silver) = (id("relic_cache"), id("silver_seam"));
        for k in original_kinds(&kinds) {
            let with = before_m26(k.generation(42).single());
            let shape = PlanetShape::new(64, 12);
            let old = Generator::new(shape, before_m15(with.clone()), &blocks).unwrap();
            let new = Generator::new(shape, before_m16(with), &blocks).unwrap();
            let pinned = PINNED_M14.iter().find(|(n, _)| *n == k.name).expect("pinned").1;
            assert_eq!(fingerprint(&old), pinned, "{} before M15 keeps its terrain", k.name);
            let (mut caches, mut silvers) = (0, 0);
            for x in 0..12 {
                for y in 0..5 {
                    let pos = ChunkPos::new(x * 5, y, x * 11);
                    let (a, b) = (old.generate(pos).to_dense_states(), new.generate(pos).to_dense_states());
                    for (a, b) in a.iter().zip(&b) {
                        if a == b {
                            continue;
                        }
                        assert!(rock.contains(&a.id), "{}: {a:?} became {b:?}", k.name);
                        if b.id == cache {
                            caches += 1;
                        } else {
                            assert_eq!(b.id, silver, "{}", k.name);
                            silvers += 1;
                        }
                    }
                }
            }
            assert!(caches > 0, "{}: no relic caches", k.name);
            assert_eq!(
                silvers > 0,
                k.tier >= 3,
                "{}: {silvers} silver on tier {}",
                k.name,
                k.tier
            );
        }
    }

    const PINNED_M8: [(&str, u64); 4] = [
        ("temperate", 0x3608_579d_2236_4d0b),
        ("arid", 0x5876_767f_47f6_8745),
        ("frozen", 0x9a68_3b2f_8daa_0e57),
        ("lush", 0x3f72_0658_6120_3994),
    ];

    /// A prop found in a sample: (block, its world position, the ground
    /// under it).
    type Prop = (BlockId, BlockPos, BlockId);

    /// Every prop in a sample of chunks, and the cells a prop could stand
    /// in, by (ground under it, whether in a cave): open cells (air, or a
    /// prop) over a block in the same chunk, on the surface or under it.
    fn scan(g: &Generator, chunks: impl Iterator<Item = ChunkPos>) -> (Vec<Prop>, BTreeMap<(BlockId, bool), usize>) {
        let blocks = BlockRegistry::builtin();
        let flora: Vec<BlockId> = g.flora.iter().map(|f| f.block).collect();
        let cs = CHUNK_SIZE as usize;
        let (mut props, mut spots) = (Vec::new(), BTreeMap::new());
        for pos in chunks {
            let c = g.generate(pos);
            if c.uniform().is_some() {
                continue;
            }
            let dense = c.to_dense();
            let o = pos.origin();
            let heights: Vec<i64> = (0..cs * cs)
                .map(|i| g.surface_height(o.x + (i % cs) as i64, o.z + (i / cs) as i64))
                .collect();
            for (i, &b) in dense.iter().enumerate() {
                let l = LocalPos::from_index(i);
                let is_prop = flora.contains(&b);
                if is_prop {
                    assert!(l.y > 0, "a prop at the bottom of its chunk");
                    assert!(!blocks.get(b).solid);
                }
                if l.y == 0 || !(b.is_air() || is_prop) {
                    continue;
                }
                let (x, y, z) = (o.x + l.x as i64, o.y + l.y as i64, o.z + l.z as i64);
                let h = heights[l.z as usize * cs + l.x as usize];
                let ground = dense[LocalPos::new(l.x, l.y - 1, l.z).index()];
                if is_prop {
                    props.push((b, BlockPos::new(x, y, z), ground));
                }
                let cave = if y == h + 1 {
                    false
                } else if y < h {
                    true
                } else {
                    continue;
                };
                *spots.entry((ground, cave)).or_default() += 1;
            }
        }
        (props, spots)
    }

    fn sample_chunks() -> impl Iterator<Item = ChunkPos> {
        (0..12).flat_map(|cz| (0..12).flat_map(move |cx| (1..9).map(move |cy| ChunkPos::new(cx * 5, cy, cz * 5))))
    }

    /// Each prop stands where some entry of its kind's list allows: on its
    /// ground, on the surface or a cave floor at its depth, in its heights,
    /// by water if it must be, in its patches. Every listed object grows
    /// somewhere, and fungi only underground.
    #[test]
    fn flora_grows_only_where_its_rules_allow() {
        let names = BlockRegistry::builtin();
        for (name, _, g) in all_kinds() {
            let (found, spots) = scan(&g, sample_chunks());
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for &(b, p, ground) in &found {
                let h = g.surface_height(p.x, p.z);
                let def = names.get(b);
                *counts.entry(def.name.clone()).or_default() += 1;
                let ok = g.flora.iter().any(|f| {
                    let depth = h - p.y;
                    f.block == b
                        && f.on.contains(&ground)
                        && (p.y >= f.min_y && p.y <= f.max_y)
                        && if f.cave {
                            depth >= f.min_depth && depth <= f.max_depth
                        } else {
                            p.y == h + 1
                        }
                        && (!f.near_water
                            || (-NEAR_WATER..=NEAR_WATER).any(|dz| {
                                (-NEAR_WATER..=NEAR_WATER)
                                    .any(|dx| g.surface_height(p.x + dx, p.z + dz) < g.water_level)
                            }))
                        && f.patch
                            .as_ref()
                            .is_none_or(|(n, t)| n.sample2(p.x as f64, p.z as f64, g.width) > *t)
                });
                assert!(ok, "{name}: {} at {p:?} on {}", def.name, names.get(ground).name);
                if def.model == bm_core::Model::Fungus {
                    assert!(p.y < h, "{name}: {} above ground at {p:?}", def.name);
                }
            }
            eprintln!("{name}: {counts:?}");
            // Every object whose ground the sample offers often enough grew.
            for f in &g.flora {
                let n: usize =
                    f.on.iter()
                        .map(|b| spots.get(&(*b, f.cave)).copied().unwrap_or(0))
                        .sum();
                let expected = n as f64 * f.density;
                let block = &names.get(f.block).name;
                if expected >= 10.0 && f.patch.is_none() && !f.near_water {
                    assert!(counts.get(block).copied().unwrap_or(0) > 0, "{name}: no {block} grew");
                }
            }
            for f in &g.cfg.flora {
                if ["spikeleaf", "boulder", "glowcap", "tarspot_cap"].contains(&f.block.as_str()) {
                    assert!(counts.contains_key(&f.block), "{name}: no {} grew", f.block);
                }
            }
        }
    }

    /// The starter world's fibrous leaves: spikeleaf grows on its beaches
    /// and in dry patches of its turf, within reach of a new player.
    #[test]
    fn spikeleaf_grows_on_the_starter_worlds_sand_and_turf() {
        let g = kind("temperate");
        let spike = id("spikeleaf");
        let on: std::collections::HashSet<BlockId> = scan(&g, sample_chunks())
            .0
            .into_iter()
            .filter(|(b, ..)| *b == spike)
            .map(|(.., ground)| ground)
            .collect();
        assert!(on.contains(&id("sand")) && on.contains(&id("turf")), "{on:?}");
    }

    /// The relic seam lies 20 or more above the floor and 3 or more under
    /// the surface on every kind, about as common as copper.
    #[test]
    fn relic_seam_keeps_its_depths() {
        let (relic, copper) = (id("relic_seam"), id("copper_vein"));
        for (name, _, g) in all_kinds() {
            let (mut relics, mut coppers) = (0, 0);
            for cz in 0..8 {
                for cx in 0..8 {
                    for cy in 0..6 {
                        let p = ChunkPos::new(cx * 3, cy, cz * 3);
                        let c = g.generate(p);
                        if !c.may_contain(relic) && !c.may_contain(copper) {
                            continue;
                        }
                        for (i, b) in c.to_dense().into_iter().enumerate() {
                            let l = LocalPos::from_index(i);
                            let w = BlockPos::new(
                                p.origin().x + l.x as i64,
                                p.origin().y + l.y as i64,
                                p.origin().z + l.z as i64,
                            );
                            if b == relic {
                                assert!(w.y >= 20, "{name}: relic at {w:?}");
                                assert!(w.y <= g.surface_height(w.x, w.z) - 3, "{name}: shallow relic at {w:?}");
                                relics += 1;
                            } else if b == copper {
                                coppers += 1;
                            }
                        }
                    }
                }
            }
            eprintln!("{name}: relic {relics}, copper {coppers}");
            assert!(
                relics * 3 > coppers && relics < coppers * 3,
                "{name}: relic {relics}, copper {coppers}"
            );
        }
    }

    /// Milestone 18 part 2: each gem grows on its own world only, and only
    /// where its rule allows (depth, water over it, height of the land).
    /// Milestone 21: each astral ore grows only on its own tier 7 kind,
    /// deep, and the tier 7 kinds hold no gem seams.
    #[test]
    fn astral_ores_grow_only_on_their_own_worlds() {
        let ores = [
            ("auroral", "aurorite_seam"),
            ("pyric", "pyrelith_seam"),
            ("gloaming", "gloamite_seam"),
        ];
        let ids = ores.map(|(_, o)| id(o));
        let gems = ["amethyst", "emerald", "sapphire", "topaz", "diamond", "ruby"].map(|g| id(&format!("{g}_seam")));
        let blocks = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&blocks);
        for kind in &kinds.kinds {
            let own = ores.iter().position(|(k, _)| *k == kind.name);
            if kind.tier < 5 && own.is_none() {
                continue;
            }
            let g = Generator::new(PlanetShape::new(64, 12), kind.generation(42), &blocks).unwrap();
            let mut found = 0;
            for cz in 0..6 {
                for cx in 0..6 {
                    for cy in 0..4 {
                        let p = ChunkPos::new(cx * 10, cy, cz * 10 + cx);
                        let c = g.generate(p);
                        for b in c.to_dense() {
                            if let Some(which) = ids.iter().position(|i| *i == b) {
                                assert_eq!(Some(which), own, "{}: {} out of place", kind.name, ores[which].1);
                                found += 1;
                            }
                            if own.is_some() {
                                assert!(!gems.contains(&b), "{}: a gem seam on a tier 7 world", kind.name);
                            }
                        }
                    }
                }
            }
            if let Some(own) = own {
                assert!(found > 0, "{}: no {} found", kind.name, ores[own].1);
                assert_eq!(kind.tier, 7);
                assert!(kind.astral.is_some());
            }
        }
    }

    #[test]
    fn gems_grow_on_their_own_worlds_by_their_rules() {
        let gems = [
            ("acrid", "amethyst_seam"),
            ("blighted", "emerald_seam"),
            ("glacial", "sapphire_seam"),
            ("stormy", "topaz_seam"),
            ("scorched", "diamond_seam"),
            ("ember", "ruby_seam"),
        ];
        let ids = gems.map(|(_, g)| id(g));
        for (name, _, g) in all_kinds() {
            let own = gems.iter().position(|(k, _)| *k == name);
            let sea = g.cfg.sea_level;
            let mut found = 0;
            for cz in 0..8 {
                for cx in 0..8 {
                    for cy in 0..6 {
                        let p = ChunkPos::new(cx * 8, cy, cz * 8 + cx);
                        let c = g.generate(p);
                        if !ids.iter().any(|i| c.may_contain(*i)) {
                            continue;
                        }
                        for (i, b) in c.to_dense().into_iter().enumerate() {
                            let Some(which) = ids.iter().position(|g| *g == b) else {
                                continue;
                            };
                            assert_eq!(Some(which), own, "{name}: {} out of place", gems[which].1);
                            let l = LocalPos::from_index(i);
                            let (x, y, z) = (
                                p.origin().x + l.x as i64,
                                p.origin().y + l.y as i64,
                                p.origin().z + l.z as i64,
                            );
                            let h = g.surface_height(x, z);
                            let (depth, land, water) = (h - y, h - sea, g.water_level - h);
                            let ok = match name.as_str() {
                                "acrid" => (5..=40).contains(&depth) && water <= 8,
                                "blighted" => (2..=60).contains(&depth) && water >= 10,
                                "glacial" => (10..=75).contains(&depth) && (20..=80).contains(&land),
                                "stormy" => depth >= 10 && land >= 80,
                                "scorched" => depth >= 70 && land >= 60,
                                _ => (50..=70).contains(&depth),
                            };
                            assert!(ok, "{name}: gem at depth {depth}, land {land}, water {water}");
                            found += 1;
                        }
                    }
                }
            }
            eprintln!("{name}: {found} gem blocks");
            if own.is_some() {
                assert!(found > 0, "{name}: no gems found");
            }
        }
    }

    /// Milestone 18 part 3: lower seas of lava fill low caves on their
    /// worlds, never above altitude 16; Ironmarch's kind has lava seas; old
    /// kinds have no lava at all.
    #[test]
    fn lava_fills_low_caves_and_the_lava_worlds_seas() {
        let lava = id("lava");
        for (name, _, g) in all_kinds() {
            let (mut low, mut high) = (0, 0);
            for cz in 0..6 {
                for cx in 0..6 {
                    for cy in 0..6 {
                        let p = ChunkPos::new(cx * 10, cy, cz * 10 + cx);
                        let c = g.generate(p);
                        if !c.may_contain(lava) {
                            continue;
                        }
                        for (i, b) in c.to_dense().into_iter().enumerate() {
                            if b == lava {
                                if p.origin().y + LocalPos::from_index(i).y as i64 <= 16 {
                                    low += 1;
                                } else {
                                    high += 1;
                                }
                            }
                        }
                    }
                }
            }
            eprintln!("{name}: lava {low} low, {high} high");
            match name.as_str() {
                "temperate" | "arid" | "frozen" | "lush" | "overgrown" | "glacial" => {
                    assert_eq!(low + high, 0, "{name}: no lava")
                }
                "rustland" | "ember" => assert!(high > 0, "{name}: a lava sea"),
                _ => assert!(low > 0 && high == 0, "{name}: lava only low ({low}, {high})"),
            }
        }
    }

    #[test]
    fn bad_flora_is_refused() {
        let blocks = BlockRegistry::builtin();
        let with = |block: &str, on: &[&str]| PlanetGenConfig {
            flora: vec![FloraSpec {
                block: block.into(),
                on: on.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(with("boulder", &["stone"]).check(&blocks).is_ok());
        assert!(matches!(
            with("stone", &["stone"]).check(&blocks),
            Err(ConfigError::FloraSolid(_))
        ));
        assert!(matches!(
            with("boulder", &[]).check(&blocks),
            Err(ConfigError::FloraGround(_))
        ));
        assert!(matches!(
            with("nope", &["stone"]).check(&blocks),
            Err(ConfigError::Block(_))
        ));
        assert!(matches!(
            with("boulder", &["nope"]).check(&blocks),
            Err(ConfigError::Block(_))
        ));
        assert!(Generator::new(PlanetShape::new(8, 4), with("stone", &["stone"]), &blocks).is_err());
    }

    #[test]
    fn interpolated_lookup_matches_bulk_generation() {
        // Tree footings use cave_interp; chunks use the bulk lattice. They
        // must agree or a tree could stand over a cave the chunk carved.
        let g = generator();
        let pos = ChunkPos::new(9, 3, 4);
        let chunk = g.generate(pos);
        let o = pos.origin();
        let mut checked = 0;
        for i in 0..CHUNK_VOLUME {
            let l = LocalPos::from_index(i);
            if i % 97 != 0 {
                continue;
            }
            let (wx, wy, wz) = (o.x + l.x as i64, o.y + l.y as i64, o.z + l.z as i64);
            let h = g.surface_height(wx, wz);
            if wy < h - 6 && wy > 3 && chunk.get(l) == BlockId::AIR {
                assert!(g.is_cave_at(wx, wy, wz));
                checked += 1;
            }
        }
        assert!(checked > 0, "sample hit no cave air; pick another chunk");
    }

    #[test]
    fn surface_tiles_across_the_seam() {
        let g = generator();
        let w = g.shape().width_blocks();
        for i in 0..200 {
            assert_eq!(g.surface_height(i * 7, i), g.surface_height(i * 7 + w, i - w));
        }
    }

    #[test]
    fn world_has_sea_land_and_mountains() {
        let g = generator();
        let w = g.shape().width_blocks();
        let sea = g.config().sea_level;
        let (mut water, mut land, mut high) = (0, 0, 0);
        for z in (0..w).step_by(16) {
            for x in (0..w).step_by(16) {
                let h = g.surface_height(x, z);
                if h < sea {
                    water += 1
                } else {
                    land += 1
                }
                if h > sea + 60 {
                    high += 1;
                }
            }
        }
        let total = (water + land) as f64;
        assert!(water as f64 / total > 0.1, "water {water} land {land}");
        assert!(land as f64 / total > 0.3, "water {water} land {land}");
        assert!(high > 0, "no mountains");
    }

    #[test]
    fn lush_mountains_are_taller() {
        let (t, l) = (kind("temperate"), kind("lush"));
        let w = t.shape().width_blocks();
        let peak = |g: &Generator| {
            (0..w)
                .step_by(8)
                .flat_map(|z| (0..w).step_by(8).map(move |x| (x, z)))
                .map(|(x, z)| g.surface_height(x, z))
                .max()
                .unwrap()
        };
        assert!(peak(&l) > peak(&t) + 20, "lush {} temperate {}", peak(&l), peak(&t));
    }

    #[test]
    fn floor_is_bedrock_and_sky_is_air() {
        for (name, _, g) in all_kinds() {
            let bedrock = id("bedrock");
            let floor = g.generate(ChunkPos::new(5, 0, 5));
            for x in 0..32 {
                for z in 0..32 {
                    assert_eq!(floor.get(LocalPos::new(x, 0, z)), bedrock, "{name}");
                }
            }
            assert_eq!(
                g.generate(ChunkPos::new(5, 11, 5)).uniform(),
                Some(BlockId::AIR),
                "{name}"
            );
            assert_eq!(
                g.generate(ChunkPos::new(5, -1, 5)).uniform(),
                Some(BlockId::AIR),
                "{name}"
            );
        }
    }

    /// Counts of each block over a 12×12-chunk area, heights 2..8 (y 64 to 255).
    fn census(g: &Generator) -> std::collections::HashMap<BlockId, usize> {
        let mut n = std::collections::HashMap::new();
        for cz in 0..12 {
            for cx in 0..12 {
                for cy in 2..8 {
                    let c = g.generate(ChunkPos::new(cx, cy, cz));
                    if let Some(b) = c.uniform() {
                        *n.entry(b).or_default() += CHUNK_VOLUME;
                        continue;
                    }
                    for b in c.to_dense() {
                        *n.entry(b).or_default() += 1;
                    }
                }
            }
        }
        n
    }

    #[test]
    fn kinds_differ_where_they_should() {
        let count = |n: &std::collections::HashMap<BlockId, usize>, b: &str| n.get(&id(b)).copied().unwrap_or(0);
        let t = census(&kind("temperate"));
        let a = census(&kind("arid"));
        let f = census(&kind("frozen"));
        let l = census(&kind("lush"));
        // Arid: no trees, sand and sandstone, little water.
        assert_eq!(count(&a, "trunk") + count(&a, "foliage"), 0);
        assert!(count(&a, "sandstone") > 10_000, "sandstone {}", count(&a, "sandstone"));
        assert_eq!(count(&a, "turf"), 0);
        assert!(
            count(&a, "water") * 4 < count(&t, "water"),
            "arid water {}",
            count(&a, "water")
        );
        // Frozen: snow instead of turf, an iced sea, a few trees.
        assert_eq!(count(&f, "turf"), 0);
        assert!(count(&f, "snow") > count(&t, "snow") * 4);
        assert!(count(&f, "ice") > 1000, "ice {}", count(&f, "ice"));
        assert_eq!(count(&t, "ice"), 0);
        assert!(count(&f, "trunk") > 0 && count(&f, "trunk") * 2 < count(&t, "trunk"));
        // Lush: denser trees, more glow.
        assert!(
            count(&l, "trunk") > count(&t, "trunk") * 2,
            "lush trees {}",
            count(&l, "trunk")
        );
        assert!(count(&l, "glow_crystal") > count(&t, "glow_crystal"));
        // Coal and copper everywhere.
        for (k, n) in [("temperate", &t), ("arid", &a), ("frozen", &f), ("lush", &l)] {
            assert!(count(n, "coal_seam") > 0 && count(n, "copper_vein") > 0, "{k}");
        }
    }

    #[test]
    fn frozen_has_ice_where_temperate_has_water() {
        let (t, f) = (kind("temperate"), kind("frozen"));
        let (water, ice) = (id("water"), id("ice"));
        let sea = t.config().sea_level;
        let mut checked = 0;
        // The sea surface and the block under it, over a strip of the planet.
        for cx in 0..64 {
            let at = |y: i64| (ChunkPos::new(cx, (y >> 5) as i32, 7), (y & 31) as u8);
            let ((p, ly), (p_under, ly_under)) = (at(sea), at(sea - 1));
            let (ct, cf) = (t.generate(p), f.generate(p));
            let (ct_under, cf_under) = (t.generate(p_under), f.generate(p_under));
            for lx in 0..32 {
                for lz in 0..32 {
                    if ct.get(LocalPos::new(lx, ly, lz)) == water {
                        assert_eq!(cf.get(LocalPos::new(lx, ly, lz)), ice);
                        // One layer of ice; below it the sea is as deep as ever.
                        let under = LocalPos::new(lx, ly_under, lz);
                        if ct_under.get(under) == water {
                            assert_eq!(cf_under.get(under), water);
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 1000, "only {checked} sea columns");
    }

    #[test]
    fn iron_only_on_tier_two_and_up_and_deeper_than_copper() {
        let (iron, copper) = (id("iron_vein"), id("copper_vein"));
        let mut iron_count = Vec::new();
        for (name, tier, g) in all_kinds() {
            let (mut n, mut iron_top, mut copper_top) = (0, i64::MIN, i64::MIN);
            for cz in 0..8 {
                for cx in 0..8 {
                    for cy in 0..5 {
                        let p = ChunkPos::new(cx, cy, cz);
                        let c = g.generate(p);
                        if !c.may_contain(iron) && !c.may_contain(copper) {
                            continue;
                        }
                        for (i, b) in c.to_dense().into_iter().enumerate() {
                            let y = p.origin().y + LocalPos::from_index(i).y as i64;
                            if b == iron {
                                n += 1;
                                iron_top = iron_top.max(y);
                            } else if b == copper {
                                copper_top = copper_top.max(y);
                            }
                        }
                    }
                }
            }
            if tier < 2 {
                assert_eq!(n, 0, "{name} has iron");
            } else {
                assert!(n > 0, "{name} has no iron");
                assert!(
                    iron_top < copper_top,
                    "{name}: iron to {iron_top}, copper to {copper_top}"
                );
            }
            iron_count.push((name, n));
        }
        let get = |k: &str| iron_count.iter().find(|(n, _)| n == k).unwrap().1;
        assert!(
            get("lush") > get("arid") && get("lush") > get("frozen"),
            "{iron_count:?}"
        );
    }

    #[test]
    fn trees_grow_and_survive_chunk_borders() {
        let g = generator();
        let trunk = id("trunk");
        let foliage = id("foliage");
        let mut trees = 0;
        let mut canopy_only = 0;
        for cz in 0..12 {
            for cx in 0..12 {
                for cy in 3..8 {
                    let c = g.generate(ChunkPos::new(cx, cy, cz));
                    let (t, f) = (c.may_contain(trunk), c.may_contain(foliage));
                    if t && f {
                        trees += 1;
                    } else if f {
                        canopy_only += 1;
                    }
                }
            }
        }
        assert!(trees > 0, "no trees in a 12x12 chunk area");
        // Canopy reaching over a border without its trunk proves cross-chunk
        // placement happens; it must not be the common case.
        assert!(canopy_only < trees * 3, "trees {trees} canopy-only {canopy_only}");
    }

    #[test]
    fn spawn_is_on_dry_solid_standable_ground() {
        let blocks = BlockRegistry::builtin();
        let (water, ice) = (id("water"), id("ice"));
        for (name, _, g) in all_kinds() {
            let s = g.spawn_point();
            assert!(s.y > g.config().sea_level + 1, "{name}: spawn at {s:?}");
            let below = block_at(&g, s.offset(0, -1, 0));
            assert!(
                blocks.get(below).solid && below != water && below != ice,
                "{name}: standing on {}",
                blocks.get(below).name
            );
            for dy in 0..3 {
                // A plant or boulder may stand on the spot itself (the
                // server's plaza clears it); nothing else.
                let b = block_at(&g, s.offset(0, dy, 0));
                let prop = dy == 0 && g.cfg.flora.iter().any(|f| blocks.id(&f.block).ok() == Some(b));
                assert!(
                    b == BlockId::AIR || prop,
                    "{name}: {} {dy} above the ground",
                    blocks.get(b).name
                );
            }
            // Level ground around it for the shrine.
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let p = s.offset(dx, 0, dz);
                let ground = (-2..=1).any(|dy| blocks.get(block_at(&g, p.offset(0, dy, 0))).solid);
                assert!(ground, "{name}: no ground beside the spawn at {p:?}");
            }
        }
    }

    #[test]
    fn region_noise_spread_is_as_assumed() {
        let g = kind("temperate");
        let w = g.shape().width_blocks();
        let (mut n, mut sa, mut sb) = (0.0, 0.0, 0.0);
        for z in (0..w).step_by(8) {
            for x in (0..w).step_by(8) {
                let (fx, fz) = (x as f64, z as f64);
                sa += g.region_a.sample2(fx, fz, g.width).powi(2);
                sb += g.region_b.sample2(fx, fz, g.width).powi(2);
                n += 1.0;
            }
        }
        let (a, b) = ((sa / n).sqrt(), (sb / n).sqrt());
        eprintln!("region sigma {a:.4} {b:.4}");
        assert!(
            (a - REGION_SIGMA).abs() < 0.05 && (b - REGION_SIGMA).abs() < 0.05,
            "{a} {b}"
        );
    }

    /// The colour each coloured block gets in column `(wx, wz)`, computed the
    /// way chunk generation does (without generating terrain).
    fn column_colours(g: &Generator, wx: i64, wz: i64) -> Vec<Colour> {
        let origin = BlockPos::new(wx & !31, 0, wz & !31);
        let lattice = g.region_lattice(origin);
        let (a, b) = g.region_at(&lattice, wx & 31, wz & 31, wx, wz);
        g.colours.blocks.iter().map(|c| c.pick(a, b)).collect()
    }

    #[test]
    fn colours_vary_in_large_patches() {
        let g = kind("temperate");
        let w = g.shape().width_blocks();
        let n_blocks = g.colours.blocks.len();
        assert!(n_blocks >= 5);
        for slot in 0..n_blocks {
            let colours = &g.colours.blocks[slot].colours;
            let mut count = vec![0usize; colours.len()];
            let (mut same, mut pairs) = (0, 0);
            for z in (0..w).step_by(32) {
                let mut prev: Option<Colour> = None;
                for x in (0..w).step_by(16) {
                    let c = column_colours(&g, x, z)[slot];
                    count[colours.iter().position(|&k| k == c).unwrap()] += 1;
                    if let Some(p) = prev {
                        pairs += 1;
                        same += (p == c) as usize;
                    }
                    prev = Some(c);
                }
            }
            let total: usize = count.iter().sum();
            let main = count[0] as f64 / total as f64;
            // The main colour leads, every variant turns up somewhere, and
            // colours come in patches: columns 16 apart nearly always match.
            assert!((0.4..0.85).contains(&main), "slot {slot}: {count:?}");
            assert!(count.iter().all(|&n| n > 0), "slot {slot}: {count:?}");
            assert!(same as f64 / pairs as f64 > 0.85, "slot {slot}: {same}/{pairs}");
        }
        // Blocks change colour in different places: their layouts differ.
        let (mut differ, mut n) = (0, 0);
        for z in (0..w).step_by(64) {
            for x in (0..w).step_by(64) {
                let c = column_colours(&g, x, z);
                let main = |slot: usize| c[slot] == g.colours.blocks[slot].colours[0];
                differ += (main(0) != main(1)) as usize;
                n += 1;
            }
        }
        assert!(differ * 8 > n, "{differ} of {n}");
    }

    #[test]
    fn colours_tile_across_the_seam() {
        let g = kind("lush");
        let w = g.shape().width_blocks();
        // A column just west of the seam, seen from its own chunk and from
        // the margin of the chunk east of the seam (where a tree standing
        // in it would be coloured from): the same colours.
        let last = g.region_lattice(BlockPos::new(w - 32, 0, 0));
        let first = g.region_lattice(BlockPos::new(0, 0, 0));
        for lx in [29, 30, 31] {
            for lz in [0, 7, 31] {
                let (wx, wz) = (w - 32 + lx, lz);
                let (a0, b0) = g.region_at(&last, lx, lz, wx, wz);
                let (a1, b1) = g.region_at(&first, lx - 32, lz, wx, wz);
                assert!((a0 - a1).abs() < 1e-9 && (b0 - b1).abs() < 1e-9, "{lx} {lz}");
            }
        }
        // Chunks across the seam generate the same, colours included.
        for (x, z) in [(63, 0), (0, 63), (63, 63)] {
            assert_eq!(
                g.generate(ChunkPos::new(x, 4, z)),
                g.generate(ChunkPos::new(x - 64, 4, z + 64))
            );
        }
    }

    /// Every state in sampled surface chunks across the planet.
    fn surface_states(g: &Generator) -> Vec<BlockState> {
        let mut out = Vec::new();
        for cz in (0..64).step_by(9) {
            for cx in (0..64).step_by(9) {
                let h = g.surface_height(cx as i64 * 32 + 16, cz as i64 * 32 + 16);
                let cy = (h >> 5) as i32;
                for y in [cy - 1, cy, cy + 1] {
                    out.extend(g.generate(ChunkPos::new(cx, y, cz)).to_dense_states());
                }
            }
        }
        out
    }

    #[test]
    fn only_listed_colourable_blocks_take_colours() {
        let blocks = BlockRegistry::builtin();
        for (name, _, g) in all_kinds() {
            let states = surface_states(&g);
            let mut coloured = std::collections::HashSet::new();
            for s in &states {
                let def = blocks.get(s.id);
                assert_eq!(s.shape, bm_core::Shape::FULL);
                match g.cfg.colours.get(&def.name) {
                    Some(names) => {
                        assert!(def.colourable, "{name}: {}", def.name);
                        let palette = palette();
                        assert!(
                            names.iter().any(|n| palette.id(n) == Some(s.colour)),
                            "{name}: {} in {}",
                            def.name,
                            palette.name(s.colour)
                        );
                        coloured.insert(def.name.clone());
                    }
                    None => assert_eq!(s.colour, Colour::NATURAL, "{name}: {}", def.name),
                }
            }
            // Ores, water and bedrock are never listed, so stay natural; the
            // common ground blocks did get coloured (and glow crystals take
            // their world's colours, milestone 26).
            for b in ["coal_seam", "copper_vein", "iron_vein", "water", "bedrock"] {
                assert!(!g.cfg.colours.contains_key(b), "{name} colours {b}");
            }
            for b in ["stone", "gravel"] {
                assert!(coloured.contains(b), "{name}: no coloured {b} in {coloured:?}");
            }
        }
    }

    #[test]
    fn trees_are_one_colour_top_to_bottom() {
        let g = kind("lush");
        let (trunk, foliage) = (id("trunk"), id("foliage"));
        let mut columns: std::collections::HashMap<(i64, i64), Colour> = Default::default();
        let mut trunks = 0;
        for cz in 0..4 {
            for cx in 0..4 {
                for cy in 3..8 {
                    let p = ChunkPos::new(cx, cy, cz);
                    let c = g.generate(p);
                    if !c.may_contain(trunk) {
                        continue;
                    }
                    for (i, s) in c.to_dense_states().into_iter().enumerate() {
                        if s.id == trunk {
                            let l = LocalPos::from_index(i);
                            let key = (p.origin().x + l.x as i64, p.origin().z + l.z as i64);
                            assert_eq!(*columns.entry(key).or_insert(s.colour), s.colour);
                            assert_ne!(s.colour, Colour::NATURAL);
                            trunks += 1;
                        } else if s.id == foliage {
                            assert_ne!(s.colour, Colour::NATURAL);
                        }
                    }
                }
            }
        }
        assert!(trunks > 50, "{trunks}");
    }

    #[test]
    fn default_settings_are_natural_everywhere() {
        let g = generator();
        assert!(g.config().colours.is_empty());
        assert!(surface_states(&g).iter().all(|s| s.colour == Colour::NATURAL));
    }

    #[test]
    fn planets_pick_their_own_colours_by_seed() {
        let blocks = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&blocks);
        let k = kinds.get("temperate").unwrap();
        assert_eq!(k.generation(5).single(), k.generation(5).single());
        let mut mains = std::collections::HashSet::new();
        for seed in 0..40 {
            let cfg = k.generation(seed).single();
            assert!(cfg.check(&blocks).is_ok());
            assert_eq!(
                cfg.colours.keys().collect::<Vec<_>>(),
                k.colours.keys().collect::<Vec<_>>()
            );
            for (block, picked) in &cfg.colours {
                let choices = &k.colours[block];
                assert!((2..=MAX_BLOCK_COLOURS).contains(&picked.len()), "{block}: {picked:?}");
                assert!(picked.iter().all(|c| choices.contains(c)));
                let distinct: std::collections::HashSet<_> = picked.iter().collect();
                assert_eq!(distinct.len(), picked.len(), "{block}: {picked:?}");
            }
            mains.insert(cfg.colours["turf"][0].clone());
        }
        assert!(mains.len() >= 4, "turf mains over 40 seeds: {mains:?}");
        // One choice: that colour alone.
        let one = BTreeMap::from([("stone".to_string(), vec!["Stone".to_string()])]);
        assert_eq!(pick_colours(&one, 9), one);
    }

    #[test]
    fn bad_colours_are_refused() {
        let blocks = BlockRegistry::builtin();
        let with = |block: &str, names: &[&str]| PlanetGenConfig {
            colours: BTreeMap::from([(block.to_string(), names.iter().map(|n| n.to_string()).collect())]),
            ..Default::default()
        };
        assert!(with("stone", &["Stone", "Iron"]).check(&blocks).is_ok());
        assert!(matches!(
            with("stone", &["Stone", "Mauve"]).check(&blocks),
            Err(ConfigError::UnknownColour { .. })
        ));
        assert!(matches!(
            with("coal_seam", &["Stone"]).check(&blocks),
            Err(ConfigError::NotColourable(_))
        ));
        assert!(matches!(
            with("water", &["True Azure"]).check(&blocks),
            Err(ConfigError::NotColourable(_))
        ));
        assert!(matches!(
            with("unobtainium", &["Stone"]).check(&blocks),
            Err(ConfigError::Block(_))
        ));
        assert!(matches!(
            with("stone", &[]).check(&blocks),
            Err(ConfigError::ColourCount(..))
        ));
        assert!(matches!(
            with("stone", &["Stone", "Iron", "Fog", "Ash", "Smoke"]).check(&blocks),
            Err(ConfigError::ColourCount(..))
        ));
        assert!(Generator::new(PlanetShape::new(8, 4), with("stone", &["Mauve"]), &blocks).is_err());
        // In a kind's choices too.
        let kind = |colours: &str| format!("[[kind]]\nname = \"x\"\ntier = 1\n[kind.colours]\n{colours}\n");
        assert!(PlanetKinds::from_toml(&kind("stone = [\"Stone\", \"Fog\"]"), &blocks).is_ok());
        for bad in [
            "stone = [\"Stone\", \"Mauve\"]",
            "copper_vein = [\"Stone\"]",
            "stone = []",
            "nope = [\"Stone\"]",
        ] {
            assert!(
                matches!(
                    PlanetKinds::from_toml(&kind(bad), &blocks),
                    Err(KindError::Generation(..))
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn bad_block_names_are_refused() {
        let blocks = BlockRegistry::builtin();
        assert!(PlanetGenConfig::default().check(&blocks).is_ok());
        let mut cfg = PlanetGenConfig::default();
        cfg.ores[1].block = "unobtainium".into();
        assert!(cfg.check(&blocks).is_err());
        let cfg = PlanetGenConfig {
            ground: "nope".into(),
            ..Default::default()
        };
        assert!(cfg.check(&blocks).is_err());
        assert!(Generator::new(PlanetShape::new(8, 4), cfg, &blocks).is_err());
        let cfg = PlanetGenConfig {
            seabed: vec![],
            ..Default::default()
        };
        assert!(cfg.check(&blocks).is_err());
    }

    #[test]
    fn settings_round_trip_and_unnamed_fields_keep_defaults() {
        let blocks = BlockRegistry::builtin();
        for k in &PlanetKinds::builtin(&blocks).kinds {
            let cfg = k.generation(9).single();
            let text = toml::to_string(&cfg).unwrap();
            assert_eq!(toml::from_str::<PlanetGenConfig>(&text).unwrap(), cfg);
        }
        let kinds = PlanetKinds::from_toml("[[kind]]\nname = \"x\"\ntier = 1\n", &blocks).unwrap();
        assert_eq!(kinds.kinds[0].generation(1).single(), PlanetGenConfig::default());
    }
}
