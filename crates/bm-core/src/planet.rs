//! Planet shape: a world that wraps horizontally and is bounded vertically.
//!
//! Like the worlds of the game this succeeds, a planet has no edge: walk far
//! enough east and you arrive back where you started (X and Z wrap). Y is
//! bounded: a floor of bedrock and a sky above which nothing is stored.
//!
//! **Positions are canonical** (X and Z in `0..width`) on server and client
//! alike (ADR-0002). Anything that compares two positions uses
//! [`PlanetShape::delta`]; rendering places each chunk at the lap nearest the
//! camera. That keeps the seam out of physics and streaming entirely.

use crate::coords::{BlockPos, CHUNK_SIZE, ChunkPos};
use glam::DVec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanetShape {
    /// Chunks around the planet along X and along Z (the planet is square).
    pub width_chunks: u32,
    /// Chunks from the floor (y = 0) to the sky limit.
    pub height_chunks: u32,
}

impl PlanetShape {
    pub fn new(width_chunks: u32, height_chunks: u32) -> Self {
        assert!(width_chunks >= 4, "a planet narrower than 4 chunks would see itself");
        assert!(height_chunks >= 1);
        Self {
            width_chunks,
            height_chunks,
        }
    }

    pub fn width_blocks(self) -> i64 {
        self.width_chunks as i64 * CHUNK_SIZE as i64
    }

    pub fn height_blocks(self) -> i64 {
        self.height_chunks as i64 * CHUNK_SIZE as i64
    }

    pub fn contains_chunk_y(self, cy: i32) -> bool {
        cy >= 0 && (cy as u32) < self.height_chunks
    }

    pub fn contains_y(self, y: i64) -> bool {
        y >= 0 && y < self.height_blocks()
    }

    pub fn wrap_block(self, p: BlockPos) -> BlockPos {
        let w = self.width_blocks();
        BlockPos::new(p.x.rem_euclid(w), p.y, p.z.rem_euclid(w))
    }

    pub fn wrap_chunk(self, c: ChunkPos) -> ChunkPos {
        let w = self.width_chunks as i32;
        ChunkPos::new(c.x.rem_euclid(w), c.y, c.z.rem_euclid(w))
    }

    pub fn wrap_point(self, p: DVec3) -> DVec3 {
        let w = self.width_blocks() as f64;
        DVec3::new(wrap_f64(p.x, w), p.y, wrap_f64(p.z, w))
    }

    /// Shortest signed horizontal difference `to - from` on the loop.
    pub fn delta(self, from: DVec3, to: DVec3) -> DVec3 {
        let w = self.width_blocks() as f64;
        DVec3::new(
            shortest_f64(to.x - from.x, w),
            to.y - from.y,
            shortest_f64(to.z - from.z, w),
        )
    }

    /// Shortest signed chunk difference `to - from` on the loop.
    pub fn chunk_delta(self, from: ChunkPos, to: ChunkPos) -> (i32, i32, i32) {
        let w = self.width_chunks as i32;
        (
            shortest_i32(to.x - from.x, w),
            to.y - from.y,
            shortest_i32(to.z - from.z, w),
        )
    }

    /// `value` moved to the lap nearest `reference`, so the two can be
    /// subtracted or interpolated without crossing the seam.
    pub fn nearest_lap(self, reference: DVec3, value: DVec3) -> DVec3 {
        reference + self.delta(reference, value)
    }
}

fn wrap_f64(v: f64, w: f64) -> f64 {
    let r = v.rem_euclid(w);
    // rem_euclid can return exactly `w` for tiny negative inputs.
    if r >= w { 0.0 } else { r }
}

fn shortest_f64(d: f64, w: f64) -> f64 {
    let r = d.rem_euclid(w);
    if r > w * 0.5 { r - w } else { r }
}

fn shortest_i32(d: i32, w: i32) -> i32 {
    let r = d.rem_euclid(w);
    if r > w / 2 { r - w } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape() -> PlanetShape {
        PlanetShape::new(8, 4) // 256 blocks around
    }

    #[test]
    fn wraps_both_ways() {
        let s = shape();
        assert_eq!(s.wrap_block(BlockPos::new(-1, 5, 256)), BlockPos::new(255, 5, 0));
        assert_eq!(s.wrap_chunk(ChunkPos::new(-1, 2, 8)), ChunkPos::new(7, 2, 0));
        assert_eq!(
            s.wrap_point(DVec3::new(-0.25, 1.0, 256.5)),
            DVec3::new(255.75, 1.0, 0.5)
        );
        assert_eq!(s.wrap_point(DVec3::new(-1e-20, 0.0, 0.0)).x, 0.0);
    }

    #[test]
    fn delta_takes_the_short_way_across_the_seam() {
        let s = shape();
        let d = s.delta(DVec3::new(255.0, 0.0, 1.0), DVec3::new(1.0, 3.0, 255.0));
        assert_eq!(d, DVec3::new(2.0, 3.0, -2.0));
        assert_eq!(
            s.chunk_delta(ChunkPos::new(7, 0, 0), ChunkPos::new(0, 1, 6)),
            (1, 1, -2)
        );
    }

    #[test]
    fn nearest_lap_lands_beside_the_reference() {
        let s = shape();
        let r = DVec3::new(255.5, 10.0, 10.0);
        let v = s.nearest_lap(r, DVec3::new(0.5, 10.0, 10.0));
        assert_eq!(v, DVec3::new(256.5, 10.0, 10.0));
    }
}
