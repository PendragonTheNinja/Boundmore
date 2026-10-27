//! Block ids and the block catalogue.
//!
//! Blocks are data (`assets/blocks.toml`), not code. The server parses the
//! catalogue and sends it to clients in the handshake (ADR-0003), so the only
//! copy that matters is the server's. Code refers to blocks by name, resolved
//! once to an id at startup; a missing name is a startup error, never a silent
//! fallback.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockId(pub u16);

/// An index into the colour palette (`assets/colours.toml`, ADR-0007). 0 is
/// natural: the block as its texture is painted. Items carry their colour, so
/// a block keeps it from the world to the pack and back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Colour(pub u8);

impl Colour {
    pub const NATURAL: Colour = Colour(0);
}

/// How a block is cut by chisels (ADR-0007). 0 is the full cube; the
/// encoding of cut shapes belongs to the chisel milestone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Shape(pub u16);

impl Shape {
    pub const FULL: Shape = Shape(0);
}

/// Everything about one voxel: its type, colour and shape. Chunks store
/// states in their palette, so colours and cuts cost no memory per voxel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockState {
    pub id: BlockId,
    pub colour: Colour,
    pub shape: Shape,
}

impl BlockState {
    pub const AIR: BlockState = BlockState::plain(BlockId::AIR);

    /// Natural colour, full cube.
    pub const fn plain(id: BlockId) -> Self {
        Self {
            id,
            colour: Colour::NATURAL,
            shape: Shape::FULL,
        }
    }

    pub const fn coloured(id: BlockId, colour: Colour) -> Self {
        Self {
            id,
            colour,
            shape: Shape::FULL,
        }
    }
}

impl BlockId {
    pub const AIR: BlockId = BlockId(0);

    pub fn is_air(self) -> bool {
        self == Self::AIR
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockDef {
    pub name: String,
    /// Side colour, and every face's unless overridden below.
    pub color: [u8; 3],
    #[serde(default)]
    pub top_color: Option<[u8; 3]>,
    #[serde(default)]
    pub bottom_color: Option<[u8; 3]>,
    /// Sides show a band of the top colour along their upper edge (turf).
    #[serde(default)]
    pub fringe: bool,
    /// Physical: collision, targeting, breaking.
    pub solid: bool,
    /// Visual: hides faces behind it and casts ambient occlusion.
    pub opaque: bool,
    /// Drawn in the blended pass.
    pub translucent: bool,
    pub breakable: bool,
    pub placeable: bool,
    /// Health category: with the planet's tier, sets how much damage breaking
    /// it takes (`mining::block_health`).
    #[serde(default)]
    pub health: Health,
    /// How opaque a translucent block draws (1 for everything else).
    #[serde(default = "full_opacity")]
    pub opacity: f32,
    /// Light the block gives off, 0 (none) to 15.
    #[serde(default)]
    pub light: u8,
    /// Texture names per face (`docs/art-direction.md`). Every block that
    /// renders must resolve all three faces.
    #[serde(default)]
    pub textures: BlockTextures,
    /// What it is made of: decides which tools break it faster.
    #[serde(default)]
    pub material: Material,
    /// The item breaking it yields. Unset: the block's own item, if it has
    /// one. Empty string: nothing.
    #[serde(default)]
    pub drops: Option<String>,
    /// A crafting station: looking at it and opening crafting offers its
    /// recipes, which then run over time inside it.
    #[serde(default)]
    pub station: Option<Station>,
    /// A liquid: swum in, not walked on (water, lava). Client and server
    /// answer `PhysicsWorld::liquid` from it (invariant 3).
    #[serde(default)]
    pub liquid: bool,
    /// Burns whoever is in it (lava, milestone 18 part 3).
    #[serde(default)]
    pub burns: bool,
    /// Hard (milestone 18, Boundless's hardness above 3): only advanced
    /// chisels shape it (gem seams, hard coal).
    #[serde(default)]
    pub hard: bool,
    /// A power coil (milestone 18): the kind of machine it powers, across
    /// one block of air.
    #[serde(default)]
    pub powers: Option<Station>,
    /// A coil's power (Boundless: 100, advanced 300).
    #[serde(default)]
    pub coil_power: u32,
    /// Slots of storage inside (0: none). A storage block with anything in
    /// it cannot be broken.
    #[serde(default)]
    pub storage: u8,
    /// Takes a palette colour (ADR-0007). Ores, machines and the like keep
    /// their painted look.
    #[serde(default = "yes")]
    pub colourable: bool,
    /// Which chisels may cut it (ADR-0008).
    #[serde(default)]
    pub chisel: Chiselling,
    /// A prop drawn as a small model (a torch), standing on the floor or
    /// hanging on a wall; it needs that neighbour to be solid. None: a block.
    #[serde(default)]
    pub mount: Option<Mount>,
    /// Items it yields beyond its drop, by planet tier (seam bonus ore, flint
    /// from rock). Rolled by the server only.
    #[serde(default)]
    pub bonus: Vec<Bonus>,
    /// How a machine's blocks must stand to work (milestone 11).
    #[serde(default)]
    pub assembly: Option<Assembly>,
    /// The model a prop is drawn as (default: a torch).
    #[serde(default)]
    pub model: Model,
    /// Heat it gives burnt in a furnace (its item; 0: not fuel).
    #[serde(default)]
    pub fuel: u16,
    /// A furnace base's heat multiplier: the share of a recipe's heat it
    /// burns (milestone 15: stone 1.0, iron 0.5, as in Boundless).
    #[serde(default = "full_heat")]
    pub heat_share: f32,
    /// XP for breaking one (milestone 12; gathering objects scale with the
    /// tier's health multiplier).
    #[serde(default)]
    pub mine_xp: u16,
    /// XP for placing one inside your own claim.
    #[serde(default)]
    pub build_xp: u16,
    /// A crop's stage block (milestone 16): 1 sown to 4 ripe, 0 for
    /// anything else (`crops`).
    #[serde(default)]
    pub crop: u8,
    /// A door or trapdoor (milestone 22 part 2): E swings it, turning it
    /// into its `toggle` block; its shape draws and collides it
    /// (`crate::doors`).
    #[serde(default)]
    pub swing: Option<crate::doors::Swing>,
    /// The block a door becomes when swung (its open or shut self).
    #[serde(default)]
    pub toggle: Option<String>,
    /// A furnishing's form (milestone 22 part 3): placed, the block takes
    /// this model, turned to how it was placed (`crate::forms`).
    #[serde(default)]
    pub form: Option<crate::forms::FormKind>,
    /// How fine its form is drawn.
    #[serde(default)]
    pub style: crate::forms::Style,
    /// A sign whose text glows (metal, crystal, glowing glass; milestone 22
    /// part 4).
    #[serde(default)]
    pub glow_text: bool,
}

/// A block's health category [tuning 2.2]: soft 800 (soil, sand, foliage),
/// hard 1200 (rock, gravel, wood, ice, crystal), seam 2400 (ore), plants
/// (gathering objects: plants, boulders, fungi) 800 that ignore armour, and
/// props that break in two swings of anything.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Soft,
    #[default]
    Hard,
    Seam,
    Prop,
    Plant,
}

/// Where a prop attaches: on top of the block below it, or against the side
/// of the neighbour in that direction (`North` hangs on the block at z - 1;
/// east +x, south +z, west -x).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mount {
    Floor,
    North,
    East,
    South,
    West,
}

impl Mount {
    /// The offset to the block it attaches to.
    pub fn support(self) -> (i64, i64, i64) {
        match self {
            Mount::Floor => (0, -1, 0),
            Mount::North => (0, 0, -1),
            Mount::East => (1, 0, 0),
            Mount::South => (0, 0, 1),
            Mount::West => (-1, 0, 0),
        }
    }

    /// The mount for a prop placed against the face of a block at offset
    /// `d` from the new prop (the support), if a prop can hang there.
    pub fn from_support(d: (i64, i64, i64)) -> Option<Mount> {
        [Mount::Floor, Mount::North, Mount::East, Mount::South, Mount::West]
            .into_iter()
            .find(|m| m.support() == d)
    }
}

/// Extra yield of `item`: the expected count on each tier 1 to 8 (the whole
/// part always, the rest as a chance; `mining::bonus_count`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bonus {
    pub item: String,
    pub per_tier: [f32; 8],
}

/// Which chisels may cut a block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chiselling {
    /// None: machines, ores, liquids, plants.
    #[default]
    None,
    /// Only slopes: natural terrain, as in Boundless.
    Slope,
    /// Every family: building blocks.
    Any,
}

impl Chiselling {
    pub fn allows(self, family: crate::shape::ShapeFamily) -> bool {
        match self {
            Chiselling::None => false,
            Chiselling::Slope => family == crate::shape::ShapeFamily::Slope,
            Chiselling::Any => true,
        }
    }
}

fn yes() -> bool {
    true
}

/// What a block is made of, for tools (`ToolClass::suits`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Material {
    #[default]
    None,
    Rock,
    Soil,
    Wood,
    Plant,
    Crystal,
}

/// Where a recipe is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Station {
    /// In the hands, instantly; a crafting table offers these too.
    Hand,
    /// Four blocks touching (`Assembly::Four`).
    Workbench,
    /// A crucible on a base (`Assembly::FurnaceBase`); burns fuel for heat.
    Furnace,
    /// One block, crafting over time (milestone 11). Appended: saved stations
    /// keep their kind.
    CraftingTable,
    /// Four blocks touching.
    Extractor,
    /// Four blocks touching; runs on spark (milestone 15). Appended, as
    /// every new kind is: saved stations keep their kind.
    Mixer,
    /// Four blocks touching; runs on spark.
    Refinery,
    /// Four blocks touching; runs on spark.
    Compactor,
    /// One block; burns fuel into spark and stores it (`spark`). Joined
    /// generators share one store.
    SparkGenerator,
    /// Four blocks touching; forges traits onto gear (milestone 17,
    /// `forge`). Draws spark for its queue.
    Forge,
}

/// How a machine's blocks must stand for it to work (milestone 11, as in
/// Boundless).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Assembly {
    /// Four blocks of this machine touching by faces, in any shape.
    Four,
    /// A furnace's base: works with its crucible on top.
    FurnaceBase,
    /// A furnace's crucible: sits on its base; does nothing alone.
    FurnaceCrucible,
}

/// The small model a prop is drawn as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Model {
    /// A stick with a flame (on floors and walls).
    #[default]
    Torch,
    /// Crossed leaves and stems.
    Plant,
    /// A rock on the ground.
    Boulder,
    /// A cap on a stem.
    Fungus,
}

fn full_heat() -> f32 {
    1.0
}

fn full_opacity() -> f32 {
    1.0
}

impl BlockDef {
    /// Whether the block produces geometry at all.
    pub fn renders(&self) -> bool {
        self.opaque || self.translucent || self.mount.is_some()
    }

    /// A small model (a torch) rather than a block.
    pub fn is_prop(&self) -> bool {
        self.mount.is_some()
    }
}

/// Texture names for a block's faces; `top`, `side` and `bottom` each fall
/// back to `all`. Names are file stems under `assets/textures/blocks/`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockTextures {
    #[serde(default)]
    pub all: Option<String>,
    #[serde(default)]
    pub top: Option<String>,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(default)]
    pub bottom: Option<String>,
}

impl BlockTextures {
    pub fn top(&self) -> Option<&str> {
        self.top.as_deref().or(self.all.as_deref())
    }

    pub fn side(&self) -> Option<&str> {
        self.side.as_deref().or(self.all.as_deref())
    }

    pub fn bottom(&self) -> Option<&str> {
        self.bottom.as_deref().or(self.all.as_deref())
    }

    /// Top, side and bottom, if all three resolve.
    pub fn faces(&self) -> Option<[&str; 3]> {
        Some([self.top()?, self.side()?, self.bottom()?])
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("block catalogue is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("the first block must be `air`")]
    AirNotFirst,
    #[error("block `{0}` is listed twice")]
    Duplicate(String),
    #[error("block `{0}` is both opaque and translucent")]
    OpaqueAndTranslucent(String),
    #[error("too many blocks ({0}); ids are u16")]
    TooMany(usize),
    #[error("no block named `{0}`")]
    Unknown(String),
    #[error("block `{0}` renders but does not name a texture for every face")]
    MissingTextures(String),
    #[error("block `{0}` gives off light {1}; the most is 15")]
    TooBright(String, u8),
    #[error("prop `{0}` must be neither solid nor opaque, with health `prop` or `plant`")]
    BadProp(String),
    #[error("door `{0}` must toggle to a door of the same swing that toggles back")]
    BadDoor(String),
    #[error("door or furnishing `{0}` is drawn by its shape, so it must be opaque or translucent")]
    Undrawn(String),
    #[error("block `{0}` has a bonus with a negative or non-finite count")]
    BadBonus(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockRegistry {
    defs: Vec<BlockDef>,
    by_name: HashMap<String, BlockId>,
}

#[derive(Deserialize)]
struct CatalogueFile {
    block: Vec<BlockDef>,
}

impl BlockRegistry {
    pub fn from_toml(source: &str) -> Result<Self, RegistryError> {
        let file: CatalogueFile = toml::from_str(source)?;
        Self::from_defs(file.block)
    }

    /// Builds and validates a registry. Used for the server's file and for
    /// the copy a client receives, so a hostile server cannot hand a client a
    /// catalogue that breaks its invariants either.
    pub fn from_defs(defs: Vec<BlockDef>) -> Result<Self, RegistryError> {
        if defs.len() > u16::MAX as usize {
            return Err(RegistryError::TooMany(defs.len()));
        }
        match defs.first() {
            Some(d) if d.name == "air" && !d.solid && !d.renders() => {}
            _ => return Err(RegistryError::AirNotFirst),
        }
        let mut by_name = HashMap::with_capacity(defs.len());
        for (i, d) in defs.iter().enumerate() {
            if d.opaque && d.translucent {
                return Err(RegistryError::OpaqueAndTranslucent(d.name.clone()));
            }
            if d.renders() && d.textures.faces().is_none() {
                return Err(RegistryError::MissingTextures(d.name.clone()));
            }
            if d.light > 15 {
                return Err(RegistryError::TooBright(d.name.clone(), d.light));
            }
            if d.is_prop() && (d.solid || d.opaque || !matches!(d.health, Health::Prop | Health::Plant)) {
                return Err(RegistryError::BadProp(d.name.clone()));
            }
            // The shaped mesher draws only opaque and translucent blocks.
            if (d.swing.is_some() || d.form.is_some()) && !(d.opaque || d.translucent) {
                return Err(RegistryError::Undrawn(d.name.clone()));
            }
            if d.bonus
                .iter()
                .any(|b| b.per_tier.iter().any(|c| !(c.is_finite() && *c >= 0.0)))
            {
                return Err(RegistryError::BadBonus(d.name.clone()));
            }
            if by_name.insert(d.name.clone(), BlockId(i as u16)).is_some() {
                return Err(RegistryError::Duplicate(d.name.clone()));
            }
        }
        // A door's toggle is a door of the same swing that toggles back.
        for d in &defs {
            if d.swing.is_some() || d.toggle.is_some() {
                let back = d
                    .toggle
                    .as_ref()
                    .and_then(|t| by_name.get(t))
                    .map(|id| &defs[id.0 as usize]);
                if !back.is_some_and(|b| b.swing == d.swing && b.toggle.as_deref() == Some(d.name.as_str())) {
                    return Err(RegistryError::BadDoor(d.name.clone()));
                }
            }
        }
        Ok(Self { defs, by_name })
    }

    /// The catalogue shipped in `assets/blocks.toml`.
    pub fn builtin() -> Self {
        Self::from_toml(include_str!("../../../assets/blocks.toml")).expect("assets/blocks.toml is valid")
    }

    pub fn id(&self, name: &str) -> Result<BlockId, RegistryError> {
        self.by_name
            .get(name)
            .copied()
            .ok_or_else(|| RegistryError::Unknown(name.to_string()))
    }

    /// The definition for an id. Unknown ids read as air, so corrupt data can
    /// never index out of bounds; validation at the edges (chunk decode, edit
    /// requests) rejects them before they get this far.
    pub fn get(&self, id: BlockId) -> &BlockDef {
        self.defs.get(id.0 as usize).unwrap_or(&self.defs[0])
    }

    pub fn contains(&self, id: BlockId) -> bool {
        (id.0 as usize) < self.defs.len()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn defs(&self) -> &[BlockDef] {
        &self.defs
    }

    pub fn iter(&self) -> impl Iterator<Item = (BlockId, &BlockDef)> {
        self.defs.iter().enumerate().map(|(i, d)| (BlockId(i as u16), d))
    }

    /// Per-id flag tables for hot loops (meshing, physics) that should not
    /// chase pointers per voxel.
    pub fn flags(&self) -> BlockFlags {
        BlockFlags {
            solid: self.defs.iter().map(|d| d.solid).collect(),
            opaque: self.defs.iter().map(|d| d.opaque).collect(),
            translucent: self.defs.iter().map(|d| d.translucent).collect(),
            light: self.defs.iter().map(|d| d.light).collect(),
            mount: self.defs.iter().map(|d| d.mount).collect(),
            model: self.defs.iter().map(|d| d.model).collect(),
            soft: self.defs.iter().map(|d| d.health == Health::Soft).collect(),
            liquid: self.defs.iter().map(|d| d.liquid).collect(),
            burns: self.defs.iter().map(|d| d.burns).collect(),
        }
    }
}

/// Dense per-id property tables. Out-of-range ids read as air.
#[derive(Clone, Debug)]
pub struct BlockFlags {
    solid: Vec<bool>,
    opaque: Vec<bool>,
    translucent: Vec<bool>,
    light: Vec<u8>,
    mount: Vec<Option<Mount>>,
    model: Vec<Model>,
    soft: Vec<bool>,
    liquid: Vec<bool>,
    burns: Vec<bool>,
}

impl BlockFlags {
    /// A liquid (water, lava).
    #[inline]
    pub fn liquid(&self, id: BlockId) -> bool {
        self.liquid.get(id.0 as usize).copied().unwrap_or(false)
    }

    /// Burns whoever is in it (lava).
    #[inline]
    pub fn burns(&self, id: BlockId) -> bool {
        self.burns.get(id.0 as usize).copied().unwrap_or(false)
    }

    /// Soft ground (soil, sand, snow, leaves: `Health::Soft`) softens falls.
    #[inline]
    pub fn soft(&self, id: BlockId) -> bool {
        self.soft.get(id.0 as usize).copied().unwrap_or(false)
    }

    #[inline]
    pub fn solid(&self, id: BlockId) -> bool {
        self.solid.get(id.0 as usize).copied().unwrap_or(false)
    }

    #[inline]
    pub fn opaque(&self, id: BlockId) -> bool {
        self.opaque.get(id.0 as usize).copied().unwrap_or(false)
    }

    #[inline]
    pub fn translucent(&self, id: BlockId) -> bool {
        self.translucent.get(id.0 as usize).copied().unwrap_or(false)
    }

    /// Light the block gives off, 0 to 15.
    #[inline]
    pub fn light(&self, id: BlockId) -> u8 {
        self.light.get(id.0 as usize).copied().unwrap_or(0)
    }

    /// Where a prop attaches (`None` for every block that is not a prop).
    #[inline]
    pub fn mount(&self, id: BlockId) -> Option<Mount> {
        self.mount.get(id.0 as usize).copied().flatten()
    }

    /// The model a prop is drawn as (meaningless for blocks that are not
    /// props).
    #[inline]
    pub fn model(&self, id: BlockId) -> Model {
        self.model.get(id.0 as usize).copied().unwrap_or_default()
    }

    /// Number of ids in the catalogue the tables were built from.
    pub fn len(&self) -> usize {
        self.solid.len()
    }

    pub fn is_empty(&self) -> bool {
        self.solid.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalogue_loads() {
        let r = BlockRegistry::builtin();
        assert_eq!(r.id("air").unwrap(), BlockId::AIR);
        assert!(r.get(r.id("stone").unwrap()).solid);
        assert!(r.get(r.id("water").unwrap()).translucent);
        assert!(!r.get(r.id("bedrock").unwrap()).breakable);
        let turf = r.get(r.id("turf").unwrap());
        assert_eq!(turf.textures.faces(), Some(["turf_top", "turf_side", "soil"]));
        assert!(r.get(r.id("glow_crystal").unwrap()).light > 0);
    }

    #[test]
    fn rendering_blocks_need_every_face_textured() {
        let air = BlockRegistry::builtin().defs()[0].clone();
        let mut stone = BlockRegistry::builtin().get(BlockId(2)).clone();
        stone.textures = BlockTextures {
            top: Some("x".into()),
            ..Default::default()
        };
        assert!(matches!(
            BlockRegistry::from_defs(vec![air, stone]),
            Err(RegistryError::MissingTextures(_))
        ));
    }

    #[test]
    fn doors_and_furnishings_must_be_drawn() {
        // Every door was once neither opaque nor translucent, and the shaped
        // mesher skipped them all: invisible in game.
        let r = BlockRegistry::builtin();
        let shaped: Vec<_> = r
            .defs()
            .iter()
            .filter(|d| d.swing.is_some() || d.form.is_some())
            .collect();
        assert!(shaped.len() >= 48);
        assert!(shaped.iter().all(|d| d.opaque || d.translucent));
        let air = r.defs()[0].clone();
        let mut door = r.get(r.id("plain_wood_door").unwrap()).clone();
        let mut open = r.get(r.id("plain_wood_door_open").unwrap()).clone();
        door.opaque = false;
        open.opaque = false;
        assert!(matches!(
            BlockRegistry::from_defs(vec![air, door, open]),
            Err(RegistryError::Undrawn(_))
        ));
    }

    #[test]
    fn unknown_ids_read_as_air() {
        let r = BlockRegistry::builtin();
        assert_eq!(r.get(BlockId(60000)).name, "air");
        assert!(!r.flags().solid(BlockId(60000)));
    }

    #[test]
    fn rejects_bad_catalogues() {
        let air = BlockRegistry::builtin().defs()[0].clone();
        let mut stone = BlockRegistry::builtin().get(BlockId(2)).clone();
        assert!(matches!(
            BlockRegistry::from_defs(vec![stone.clone()]),
            Err(RegistryError::AirNotFirst)
        ));
        assert!(matches!(
            BlockRegistry::from_defs(vec![air.clone(), stone.clone(), stone.clone()]),
            Err(RegistryError::Duplicate(_))
        ));
        stone.translucent = true;
        assert!(matches!(
            BlockRegistry::from_defs(vec![air, stone]),
            Err(RegistryError::OpaqueAndTranslucent(_))
        ));
    }
}
