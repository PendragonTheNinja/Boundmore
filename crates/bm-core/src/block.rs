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
    pub color: [u8; 3],
    /// Physical: collision, targeting, breaking.
    pub solid: bool,
    /// Visual: hides faces behind it and casts ambient occlusion.
    pub opaque: bool,
    /// Drawn in the blended pass.
    pub translucent: bool,
    pub breakable: bool,
    pub placeable: bool,
    pub hardness: f32,
}

impl BlockDef {
    /// Whether the block produces geometry at all.
    pub fn renders(&self) -> bool {
        self.opaque || self.translucent
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
        }
    }
}

/// Dense per-id property tables. Out-of-range ids read as air.
#[derive(Clone, Debug)]
pub struct BlockFlags {
    solid: Vec<bool>,
    opaque: Vec<bool>,
    translucent: Vec<bool>,
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
