//! The atlas (milestone 27): a map of one world, drawn by the server in
//! square tiles, and the rules both sides share.
//!
//! The map is one pixel for [`BLOCKS_PER_PIXEL`] blocks along each side,
//! taken from the column at the pixel's centre, and wraps as the planet
//! does. The server sends tiles of [`TILE_PIXELS`] pixels a side as RGB,
//! with [`FOG`] wherever the player has not discovered the region (never
//! the land under it: no x-ray).
//!
//! An atlas stack carries its world and the resource loaded in it in its
//! `tag` ([`pack`]): no save format changes.

use crate::items::ItemId;
use crate::planet::PlanetShape;

/// Blocks along each side of one map pixel.
pub const BLOCKS_PER_PIXEL: i64 = 4;
/// Pixels along each side of one tile.
pub const TILE_PIXELS: usize = 64;
/// Blocks along each side of one tile.
pub const TILE_BLOCKS: i64 = TILE_PIXELS as i64 * BLOCKS_PER_PIXEL;
/// A pixel of land not discovered: drawn as fog. Real land never takes
/// this value ([`shade`] keeps every channel at 1 or more).
pub const FOG: [u8; 3] = [0, 0, 0];

/// Pixels along each side of the map of a planet.
pub fn map_pixels(shape: PlanetShape) -> usize {
    (shape.width_blocks() / BLOCKS_PER_PIXEL) as usize
}

/// Tiles along each side; the last may be cut short by the planet's edge.
pub fn tiles_per_side(shape: PlanetShape) -> u16 {
    map_pixels(shape).div_ceil(TILE_PIXELS) as u16
}

/// The tile holding canonical block column `(x, z)`.
pub fn tile_of(x: i64, z: i64) -> (u16, u16) {
    ((x / TILE_BLOCKS) as u16, (z / TILE_BLOCKS) as u16)
}

/// The block column a tile's pixel `(px, pz)` is taken from, if the pixel
/// lies on the planet (the last tile may be cut short).
pub fn pixel_column(shape: PlanetShape, tile: (u16, u16), px: usize, pz: usize) -> Option<(i64, i64)> {
    let n = map_pixels(shape);
    let (gx, gz) = (tile.0 as usize * TILE_PIXELS + px, tile.1 as usize * TILE_PIXELS + pz);
    (gx < n && gz < n).then(|| {
        let half = BLOCKS_PER_PIXEL / 2;
        (gx as i64 * BLOCKS_PER_PIXEL + half, gz as i64 * BLOCKS_PER_PIXEL + half)
    })
}

/// Packs an atlas stack's `tag`: the world it is bound to (`None`: not
/// yet) and the resource loaded in it.
pub fn pack(planet: Option<u16>, loaded: Option<ItemId>) -> u32 {
    planet.map_or(0, |p| p as u32 + 1) | loaded.map_or(0, |i| (i.0 as u32 + 1) << 16)
}

/// The world an atlas stack's `tag` is bound to.
pub fn bound(tag: u32) -> Option<u16> {
    match tag & 0xffff {
        0 => None,
        p => Some((p - 1) as u16),
    }
}

/// The resource loaded in an atlas stack's `tag`.
pub fn loaded(tag: u32) -> Option<ItemId> {
    match tag >> 16 {
        0 => None,
        i => Some(ItemId((i - 1) as u16)),
    }
}

/// One map pixel's colour: its top block's colour, lit by the slope (from
/// the heights west and north of it) and lifted by altitude; water darker
/// with depth. Never [`FOG`].
pub fn shade(colour: [u8; 3], h: i64, west: i64, north: i64, sea: i64, water_depth: Option<i64>) -> [u8; 3] {
    let k = match water_depth {
        Some(d) => (0.95 - d as f64 * 0.025).clamp(0.45, 0.95),
        None => {
            let light = (1.0 + ((h - west) + (h - north)) as f64 * 0.08).clamp(0.6, 1.4);
            let lift = 0.85 + ((h - sea) as f64 / 300.0).clamp(-0.2, 0.3);
            light * lift
        }
    };
    colour.map(|c| (c as f64 * k).round().clamp(1.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_round_trip() {
        assert_eq!(bound(0), None);
        assert_eq!(loaded(0), None);
        let t = pack(Some(1003), Some(ItemId(417)));
        assert_eq!(bound(t), Some(1003));
        assert_eq!(loaded(t), Some(ItemId(417)));
        let t = pack(Some(0), None);
        assert_eq!((bound(t), loaded(t)), (Some(0), None));
        assert_ne!(t, 0, "bound to planet 0 is still bound");
    }

    #[test]
    fn tiles_cover_the_planet_and_wrap() {
        let shape = PlanetShape::new(48, 12);
        assert_eq!(map_pixels(shape), 384);
        assert_eq!(tiles_per_side(shape), 6);
        assert_eq!(tile_of(0, 255), (0, 0));
        assert_eq!(tile_of(256, 1535), (1, 5));
        assert_eq!(pixel_column(shape, (5, 5), 63, 63), Some((1534, 1534)));
        // A planet not a whole number of tiles wide: the last tile is short.
        let shape = PlanetShape::new(20, 12);
        assert_eq!(tiles_per_side(shape), 3);
        assert_eq!(pixel_column(shape, (2, 0), 31, 0), Some((638, 2)));
        assert_eq!(pixel_column(shape, (2, 0), 32, 0), None);
    }

    #[test]
    fn land_is_never_fog() {
        assert_ne!(shade([0, 0, 0], 50, 60, 60, 64, None), FOG);
        assert_ne!(shade([0, 0, 0], 50, 50, 50, 64, Some(40)), FOG);
        let lit = shade([100, 100, 100], 70, 60, 60, 64, None);
        let shaded = shade([100, 100, 100], 60, 70, 70, 64, None);
        assert!(lit[0] > shaded[0], "slopes facing the light are brighter");
    }
}
