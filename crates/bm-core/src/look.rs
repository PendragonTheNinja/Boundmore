//! How a planet looks: its daytime sky, the colour it shows in other planets'
//! skies, and tints on its natural blocks. Data from `assets/planets.toml`
//! (each planet kind has one), sent to clients when they arrive on a planet.

use crate::block::{BlockId, BlockRegistry, RegistryError};
use serde::{Deserialize, Serialize};

/// sRGB 0-255 white: the block's texture as painted.
pub const NO_TINT: [u8; 3] = [255, 255, 255];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanetLook {
    /// Daytime sky overhead and at the horizon, linear RGB. Dusk and night
    /// are shared by every planet.
    pub day_zenith: [f32; 3],
    pub day_horizon: [f32; 3],
    /// The planet's colour seen from elsewhere, linear RGB.
    pub color: [f32; 3],
    /// Multiplies a block's texture, by block name; sRGB 0-255, white is none.
    #[serde(default)]
    pub tints: Vec<(String, [u8; 3])>,
}

impl Default for PlanetLook {
    fn default() -> Self {
        Self {
            day_zenith: [0.08, 0.26, 0.78],
            day_horizon: [0.42, 0.62, 0.90],
            color: [0.30, 0.55, 0.35],
            tints: Vec::new(),
        }
    }
}

impl PlanetLook {
    /// The tint of every block, indexed by `BlockId`. A name that is not a
    /// block is an error, caught when the kinds are loaded.
    pub fn resolve_tints(&self, blocks: &BlockRegistry) -> Result<Vec<[u8; 3]>, RegistryError> {
        let mut out = vec![NO_TINT; blocks.len()];
        for (name, rgb) in &self.tints {
            let BlockId(i) = blocks.id(name)?;
            out[i as usize] = *rgb;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tints_resolve_by_name_and_refuse_unknown_blocks() {
        let blocks = BlockRegistry::builtin();
        let look = PlanetLook {
            tints: vec![("turf".into(), [200, 120, 80])],
            ..Default::default()
        };
        let t = look.resolve_tints(&blocks).unwrap();
        assert_eq!(t.len(), blocks.len());
        assert_eq!(t[blocks.id("turf").unwrap().0 as usize], [200, 120, 80]);
        assert_eq!(t[blocks.id("stone").unwrap().0 as usize], NO_TINT);
        let bad = PlanetLook {
            tints: vec![("unobtainium".into(), [0, 0, 0])],
            ..Default::default()
        };
        assert!(bad.resolve_tints(&blocks).is_err());
    }
}
