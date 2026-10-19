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

pub mod kinds;
pub mod noise;

pub use kinds::{KindError, PlanetKind, PlanetKinds};

use bm_core::{
    BlockId, BlockPos, BlockRegistry, CHUNK_SIZE, CHUNK_VOLUME, Chunk, ChunkPos, LocalPos, PlanetShape, RegistryError,
};
use noise::{Fbm, hash01};
use serde::{Deserialize, Serialize};

/// Bump on ANY change to what the generator produces for a given seed, and
/// re-pin `terrain_fingerprint_is_pinned` in the same commit. A server refuses
/// to open a planet made by another version (its edited chunks were saved
/// against terrain that would no longer be there).
///
/// Version 2 (M4) added planet kinds. The default settings still produce
/// version 1's terrain block for block (`default_settings_reproduce_version_1`),
/// so a version 1 planet is a version 2 planet with default settings.
pub const GENERATOR_VERSION: u32 = 2;

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
    /// Ores in rock, tried in order; the first that matches a block wins.
    /// Each ore's veins come from its own noise, keyed by its place in this
    /// list, so add new ores at the end.
    pub ores: Vec<OreSpec>,
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
        }
    }
}

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
            ores: vec![
                OreSpec {
                    block: "coal_seam".into(),
                    size: 14.0,
                    threshold: 0.52,
                    min_y: 13,
                    max_y: i64::MAX,
                    min_depth: 6,
                },
                OreSpec {
                    block: "copper_vein".into(),
                    size: 12.0,
                    threshold: 0.58,
                    min_y: 1,
                    max_y: 149,
                    min_depth: 0,
                },
            ],
        }
    }
}

impl PlanetGenConfig {
    /// Every block the settings name exists.
    pub fn check(&self, blocks: &BlockRegistry) -> Result<(), RegistryError> {
        Ids::resolve(self, blocks).map(|_| ())
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

pub struct Generator {
    shape: PlanetShape,
    cfg: PlanetGenConfig,
    ids: Ids,
    ores: Vec<Ore>,
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
}

/// Cave values at one lattice point.
#[derive(Clone, Copy, Default)]
struct Cave {
    cavern: f64,
    tunnel: f64,
}

impl Cave {
    fn open(self) -> bool {
        self.cavern > 0.42 || self.tunnel < 0.055
    }
}

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
    pub fn new(shape: PlanetShape, cfg: PlanetGenConfig, registry: &BlockRegistry) -> Result<Self, RegistryError> {
        let w = shape.width_blocks();
        let s = cfg.seed;
        let k = |n: u64| s.wrapping_mul(0x2545_f491_4f6c_dd1d).wrapping_add(n << 12);
        let ids = Ids::resolve(&cfg, registry)?;
        let ores = cfg
            .ores
            .iter()
            .zip(&ids.ores)
            .enumerate()
            .map(|(i, (o, &block))| Ore {
                block,
                noise: Fbm::new(k(9 + i as u64), w, o.size, 1, 0.5),
                threshold: o.threshold,
                min_y: o.min_y,
                max_y: o.max_y,
                min_depth: o.min_depth,
            })
            .collect();
        Ok(Self {
            shape,
            ids,
            ores,
            width: w as f64,
            tree_top: (cfg.trunk_min + cfg.trunk_extra.max(1) + 1).max(TREE_MAX_HEIGHT),
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

    /// Y of the topmost terrain block in a column (before caves and trees).
    pub fn surface_height(&self, x: i64, z: i64) -> i64 {
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

    fn cave_at(&self, x: i64, y: i64, z: i64) -> Cave {
        let (fx, fy, fz) = (x as f64, y as f64, z as f64);
        let w = self.width;
        let a = self.tunnel_a.sample3(fx, fy * 1.4, fz, w);
        let b = self.tunnel_b.sample3(fx, fy * 1.4, fz, w);
        Cave {
            cavern: self.caverns.sample3(fx, fy * 1.6, fz, w),
            tunnel: a.abs().max(b.abs()),
        }
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
        let mut max_h = i64::MIN;
        for dz in 0..span {
            for dx in 0..span {
                let h = self.surface_height(origin.x + dx as i64 - m, origin.z + dz as i64 - m);
                heights[dz * span + dx] = h;
                max_h = max_h.max(h);
            }
        }
        let height_at = |lx: i64, lz: i64| heights[((lz + m) as usize) * span + (lx + m) as usize];

        // Sky fast path: most chunks on a planet are empty air.
        if origin.y > max_h + self.tree_top && origin.y > water_level {
            return Chunk::empty();
        }

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
        let cave_in = |cell: &Cell| Cave {
            cavern: cell.lerp(|i| caves[i].cavern),
            tunnel: cell.lerp(|i| caves[i].tunnel),
        };

        let seed = cfg.seed;
        let top = origin.y + cs - 1;
        let mut blocks = vec![ids.air; CHUNK_VOLUME];
        for lz in 0..cs {
            for lx in 0..cs {
                let h = height_at(lx, lz);
                let (wx, wz) = (origin.x + lx, origin.z + lz);
                let slope = slope_at(&height_at, lx, lz);
                let surface = self.surface_block(h, slope, wx, wz);
                // Under the top: a few blocks of soil (or more of the surface
                // block), then the planet's layer rock, then stone.
                let (fill, soil_bottom) = if surface == ids.cliff {
                    (surface, h)
                } else if surface == ids.ground || surface == ids.snow {
                    (ids.subsoil, h - 3)
                } else {
                    (surface, h - 3)
                };
                let layer_bottom = if cfg.layer_depth > 0 {
                    soil_bottom - cfg.layer_depth - (hash01(seed ^ 0x5a2d, wx, 0, wz) * 4.0) as i64
                } else {
                    soil_bottom
                };
                let deep_line = 60 + (hash01(seed ^ 0xdee9, wx, 0, wz) * 5.0) as i64;
                let y_end = top.min(h.max(water_level));
                for wy in origin.y..=y_end {
                    let ly = wy - origin.y;
                    let mut b = if wy == 0 || (wy <= 2 && hash01(seed ^ 0xbed, wx, wy, wz) < 0.6 / wy as f64) {
                        ids.bedrock
                    } else if wy > h {
                        // y_end stops this at the water line.
                        if wy > water_level - cfg.ice_depth {
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
                        ids.stone
                    };
                    // Never open a cave into the sea floor: static water would
                    // stand as a wall over the hole.
                    let carveable =
                        b != ids.bedrock && b != ids.water && b != ids.ice && (h >= water_level || wy < h - 6);
                    if underground && carveable {
                        let cell = Cell::at(lx, ly, lz);
                        if cave_in(&cell).open() {
                            b = ids.air;
                        } else if b == ids.stone || b == ids.deep_stone {
                            let mut ore_hit = None;
                            for (o, ore) in self.ores.iter().enumerate() {
                                if wy >= ore.min_y
                                    && wy <= ore.max_y
                                    && wy <= h - ore.min_depth
                                    && cell.lerp(|i| ore_lattice[i * n_ores + o]) > ore.threshold
                                {
                                    ore_hit = Some(ore.block);
                                    break;
                                }
                            }
                            if let Some(ore) = ore_hit {
                                b = ore;
                            } else if wy < sea - cfg.glow_depth
                                && ly < cs - 1
                                && cave_in(&Cell::at(lx, ly + 1, lz)).open()
                                && hash01(seed ^ 0x610, wx, wy, wz) < cfg.glow_chance
                            {
                                b = ids.glow;
                            }
                        }
                    }
                    blocks[LocalPos::new(lx as u8, ly as u8, lz as u8).index()] = b;
                }
            }
        }

        self.place_trees(pos, &mut blocks, &height_at);
        Chunk::from_dense(&blocks)
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

    /// Whether a tree grows from canonical column `(x, z)`, and its trunk height.
    fn tree_at(&self, x: i64, z: i64, h: i64) -> Option<i64> {
        let sea = self.cfg.sea_level;
        if self.cfg.tree_density <= 0.0 || h < sea.max(self.water_level) + 2 || h >= sea + self.cfg.tree_line {
            return None;
        }
        let forest = smoothstep(-0.2, 0.4, self.forest.sample2(x as f64, z as f64, self.width));
        let density = self.cfg.tree_density * (0.15 + 2.0 * forest);
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

    fn place_trees(&self, pos: ChunkPos, blocks: &mut [BlockId], height_at: &impl Fn(i64, i64) -> i64) {
        if self.cfg.tree_density <= 0.0 {
            return;
        }
        let ids = &self.ids;
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
                let Some(trunk) = self.tree_at(wx, wz, h) else { continue };
                let base = h + 1;
                let crown = base + trunk - 1;
                let mut put = |bx: i64, by: i64, bz: i64, id: BlockId, replace: bool| {
                    let (dx, dy, dz) = (bx - origin.x, by - origin.y, bz - origin.z);
                    if (0..cs).contains(&dx) && (0..cs).contains(&dy) && (0..cs).contains(&dz) {
                        let i = LocalPos::new(dx as u8, dy as u8, dz as u8).index();
                        if replace || blocks[i] == ids.air {
                            blocks[i] = id;
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
                            put(bx + ox, crown + oy, bz + oz, ids.foliage, false);
                        }
                    }
                }
                for y in base..=crown {
                    put(bx, y, bz, ids.trunk, true);
                }
            }
        }
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
        for dz in -TREE_REACH..=TREE_REACH {
            for dx in -TREE_REACH..=TREE_REACH {
                let (nx, nz) = at(dx, dz);
                if self.tree_at(nx, nz, self.surface_height(nx, nz)).is_some() {
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

    fn kind(name: &str) -> Generator {
        let blocks = BlockRegistry::builtin();
        let cfg = PlanetKinds::builtin(&blocks).get(name).unwrap().generation(42);
        Generator::new(PlanetShape::new(64, 12), cfg, &blocks).unwrap()
    }

    fn all_kinds() -> Vec<(String, u8, Generator)> {
        let blocks = BlockRegistry::builtin();
        PlanetKinds::builtin(&blocks)
            .kinds
            .iter()
            .map(|k| {
                let g = Generator::new(PlanetShape::new(64, 12), k.generation(42), &blocks).unwrap();
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

    fn fingerprint(g: &Generator) -> u64 {
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
        assert_eq!(fingerprint(&generator()), 0x4d00_41c8_06c3_f24c);
        assert_eq!(fingerprint(&kind("temperate")), 0x4d00_41c8_06c3_f24c);
        let v1 = PlanetGenConfigV1 {
            seed: 42,
            sea_level: 128,
            snow_line: 112,
            mountain_height: 130.0,
            tree_density: 0.012,
        };
        assert_eq!(PlanetGenConfig::from(v1), generator().config().clone());
    }

    /// Pins what the generator produces for every built-in kind. If this
    /// fails the terrain changed: bump GENERATOR_VERSION and re-pin, in the
    /// same commit.
    #[test]
    fn terrain_fingerprint_is_pinned() {
        let got: Vec<(String, u64)> = all_kinds()
            .iter()
            .map(|(n, _, g)| (n.clone(), fingerprint(g)))
            .collect();
        assert_eq!(GENERATOR_VERSION, 2);
        let want: Vec<(String, u64)> = PINNED.iter().map(|(n, h)| (n.to_string(), *h)).collect();
        assert_eq!(got, want, "terrain changed");
    }
    const PINNED: [(&str, u64); 4] = [
        ("temperate", 0x4d00_41c8_06c3_f24c),
        ("arid", 0x73a4_2ab0_3f53_7644),
        ("frozen", 0xdb2c_2f7d_dce3_2c63),
        ("lush", 0xbdf8_252a_fa20_5411),
    ];

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
        for (name, _, g) in all_kinds() {
            let s = g.spawn_point();
            assert!(s.y > g.config().sea_level + 1, "{name}: spawn at {s:?}");
            let below = block_at(&g, s.offset(0, -1, 0));
            assert!(
                blocks.get(below).solid && below != id("water") && below != id("ice"),
                "{name}: standing on {}",
                blocks.get(below).name
            );
            for dy in 0..3 {
                assert_eq!(
                    block_at(&g, s.offset(0, dy, 0)),
                    BlockId::AIR,
                    "{name}: {dy} above the ground"
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
            let cfg = k.generation(9);
            let text = toml::to_string(&cfg).unwrap();
            assert_eq!(toml::from_str::<PlanetGenConfig>(&text).unwrap(), cfg);
        }
        let kinds = PlanetKinds::from_toml("[[kind]]\nname = \"x\"\ntier = 1\n", &blocks).unwrap();
        assert_eq!(kinds.kinds[0].generation(1), PlanetGenConfig::default());
    }
}
