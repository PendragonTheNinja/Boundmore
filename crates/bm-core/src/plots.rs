//! Plots: the 8×8×8 cubes land is claimed in.

use crate::coords::BlockPos;
use crate::planet::PlanetShape;
use serde::{Deserialize, Serialize};

pub const PLOT_BITS: u32 = 3;
pub const PLOT_SIZE: i64 = 1 << PLOT_BITS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlotPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl PlotPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// The plot holding a block. Wraps with the planet.
    pub fn of(p: BlockPos, shape: PlanetShape) -> Self {
        let p = shape.wrap_block(p);
        Self::new(
            (p.x >> PLOT_BITS) as i32,
            (p.y >> PLOT_BITS) as i32,
            (p.z >> PLOT_BITS) as i32,
        )
    }

    pub fn wrap(self, shape: PlanetShape) -> Self {
        let w = (shape.width_blocks() >> PLOT_BITS) as i32;
        Self::new(self.x.rem_euclid(w), self.y, self.z.rem_euclid(w))
    }

    /// The minimum corner block.
    pub fn origin(self) -> BlockPos {
        BlockPos::new(
            (self.x as i64) << PLOT_BITS,
            (self.y as i64) << PLOT_BITS,
            (self.z as i64) << PLOT_BITS,
        )
    }

    pub fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }

    /// The six face neighbours, wrapped; a claim grows only through faces.
    pub fn neighbours(self, shape: PlanetShape) -> [PlotPos; 6] {
        [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
            .map(|(dx, dy, dz)| self.offset(dx, dy, dz).wrap(shape))
    }

    pub fn in_planet(self, shape: PlanetShape) -> bool {
        self.y >= 0 && (self.y as i64) < (shape.height_blocks() >> PLOT_BITS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_map_to_plots_across_the_seam() {
        let shape = PlanetShape::new(8, 2); // 256 blocks: 32 plots around
        assert_eq!(PlotPos::of(BlockPos::new(-1, 9, 255), shape), PlotPos::new(31, 1, 31));
        assert_eq!(PlotPos::of(BlockPos::new(7, 7, 8), shape), PlotPos::new(0, 0, 1));
        assert_eq!(PlotPos::new(0, 0, 0).neighbours(shape)[1], PlotPos::new(31, 0, 0));
        assert_eq!(PlotPos::new(2, 3, 4).origin(), BlockPos::new(16, 24, 32));
        assert!(!PlotPos::new(0, 8, 0).in_planet(shape));
        assert!(PlotPos::new(0, 7, 0).in_planet(shape));
    }
}
