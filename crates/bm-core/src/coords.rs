//! Block, chunk and in-chunk coordinates.
//!
//! Chunks are 32×32×32 cubes. A world block position maps to its chunk by an
//! arithmetic shift and to its in-chunk position by a mask, which is correct for
//! negative coordinates where `/ 32` and `% 32` are not. Never reimplement this
//! at a call site; use these helpers.

use serde::{Deserialize, Serialize};

pub const CHUNK_BITS: u32 = 5;
pub const CHUNK_SIZE: i32 = 1 << CHUNK_BITS;
pub const CHUNK_MASK: i64 = (CHUNK_SIZE - 1) as i64;
pub const CHUNK_VOLUME: usize = (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize;

/// A block in the world, in block units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockPos {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl BlockPos {
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }

    /// The block containing a point.
    pub fn containing(x: f64, y: f64, z: f64) -> Self {
        Self::new(x.floor() as i64, y.floor() as i64, z.floor() as i64)
    }

    pub fn chunk(self) -> ChunkPos {
        ChunkPos::new(
            (self.x >> CHUNK_BITS) as i32,
            (self.y >> CHUNK_BITS) as i32,
            (self.z >> CHUNK_BITS) as i32,
        )
    }

    pub fn local(self) -> LocalPos {
        LocalPos::new(
            (self.x & CHUNK_MASK) as u8,
            (self.y & CHUNK_MASK) as u8,
            (self.z & CHUNK_MASK) as u8,
        )
    }

    pub fn offset(self, dx: i64, dy: i64, dz: i64) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }
}

/// A chunk, in chunk units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChunkPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// The block at this chunk's minimum corner.
    pub fn origin(self) -> BlockPos {
        BlockPos::new(
            (self.x as i64) << CHUNK_BITS,
            (self.y as i64) << CHUNK_BITS,
            (self.z as i64) << CHUNK_BITS,
        )
    }

    pub fn block(self, local: LocalPos) -> BlockPos {
        self.origin().offset(local.x as i64, local.y as i64, local.z as i64)
    }

    pub fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }
}

/// A block inside a chunk, each axis in `0..32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalPos {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl LocalPos {
    pub const fn new(x: u8, y: u8, z: u8) -> Self {
        debug_assert!(x < 32 && y < 32 && z < 32);
        Self { x, y, z }
    }

    /// Index into a chunk's 32³ array. X varies fastest, then Z, then Y, so a
    /// horizontal slice is contiguous (worldgen fills by layer).
    pub const fn index(self) -> usize {
        self.x as usize | (self.z as usize) << 5 | (self.y as usize) << 10
    }

    pub const fn from_index(i: usize) -> Self {
        Self::new((i & 31) as u8, (i >> 10 & 31) as u8, (i >> 5 & 31) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_coordinates_split_correctly() {
        let p = BlockPos::new(-1, -33, 31);
        assert_eq!(p.chunk(), ChunkPos::new(-1, -2, 0));
        assert_eq!(p.local(), LocalPos::new(31, 31, 31));
        assert_eq!(p.chunk().block(p.local()), p);
    }

    #[test]
    fn split_round_trips_over_a_range() {
        for x in -70..70 {
            for y in [-65, -1, 0, 1, 64] {
                let p = BlockPos::new(x, y, -x * 3);
                assert_eq!(p.chunk().block(p.local()), p);
            }
        }
    }

    #[test]
    fn index_round_trips() {
        for i in 0..CHUNK_VOLUME {
            assert_eq!(LocalPos::from_index(i).index(), i);
        }
    }

    #[test]
    fn containing_floors_negative_points() {
        assert_eq!(BlockPos::containing(-0.5, 0.0, 2.99), BlockPos::new(-1, 0, 2));
    }
}
