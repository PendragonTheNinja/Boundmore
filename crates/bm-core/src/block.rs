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
    pub hardness: f32,
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
    /// Slots of storage inside (0: none). A storage block with anything in
    /// it cannot be broken.
    #[serde(default)]
    pub storage: u8,
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
#[serde(rename_all = "lowercase")]
pub enum Station {
    /// In the hands, instantly.
    Hand,
    Workbench,
    Furnace,
}

fn full_opacity() -> f32 {
    1.0
}

impl BlockDef {
    /// Whether the block produces geometry at all.
    pub fn renders(&self) -> bool {
        self.opaque || self.translucent
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
            if by_name.insert(d.name.clone(), BlockId(i as u16)).is_some() {
                return Err(RegistryError::Duplicate(d.name.clone()));
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
}

impl BlockFlags {
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
