//! One planet's atlas map (milestone 27, `bm_core::atlas`): tiles drawn on
//! the worker pool from the real world, held in memory, redrawn when blocks
//! in them change.
//!
//! A tile is drawn from snapshots of the resident chunks it covers (so an
//! edit shows before it is saved), else the saved chunks, else freshly
//! generated terrain (never saved). A saved chunk that cannot be read
//! leaves the tile undrawn rather than drawing fresh terrain over a build.
//! Each pixel also records its region, so the game can fog the regions a
//! player has not discovered before sending (`Tile::frame_for`).

use crate::store::Store;
use bm_core::atlas::{FOG, TILE_BLOCKS, TILE_PIXELS, map_pixels, pixel_column, shade};
use bm_core::regions::RegionMap;
use bm_core::{BlockRegistry, CHUNK_SIZE, Chunk, ChunkPos, ColourPalette, LocalPos, PlanetShape};
use bm_protocol::ServerMsg;
use bm_worldgen::Generator;
use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Water deeper than this reads as this deep.
const MAX_DEPTH: i64 = 24;

/// One drawn tile.
pub struct Tile {
    /// Pixels across and down (64, or fewer at the planet's edge).
    pub w: usize,
    pub h: usize,
    /// RGB by `pz * w + px`.
    pub rgb: Vec<[u8; 3]>,
    /// Each pixel's region (`RegionMap::locate`).
    pub region: Vec<u16>,
}

impl Tile {
    /// The tile as one player may see it: land in regions they have not
    /// discovered is fog. `None` when they have discovered none of it.
    pub fn frame_for(&self, planet: u16, tile: (u16, u16), discovered: &[u16]) -> Option<Vec<u8>> {
        if !self.region.iter().any(|r| discovered.contains(r)) {
            return None;
        }
        let mut bytes = Vec::with_capacity(self.rgb.len() * 3);
        for (c, r) in self.rgb.iter().zip(&self.region) {
            bytes.extend_from_slice(if discovered.contains(r) { c } else { &FOG });
        }
        let rgb = lz4_flex::compress_prepend_size(&bytes);
        Some(bm_protocol::encode_frame(&ServerMsg::AtlasTile { planet, tile, rgb }))
    }
}

/// What tile jobs need to colour a column.
struct Look {
    /// Each block's map colour (its top colour), by id.
    block: Vec<[u8; 3]>,
    liquid: Vec<bool>,
    /// Palette colours by id (0 natural: unused).
    palette: Vec<[u8; 3]>,
    sea: i64,
}

impl Look {
    fn colour(&self, s: bm_core::BlockState) -> [u8; 3] {
        if s.colour.0 != 0
            && let Some(c) = self.palette.get(s.colour.0 as usize)
        {
            return *c;
        }
        self.block.get(s.id.0 as usize).copied().unwrap_or([128; 3])
    }

    fn liquid(&self, s: bm_core::BlockState) -> bool {
        self.liquid.get(s.id.0 as usize).copied().unwrap_or(false)
    }
}

struct Built {
    tile: (u16, u16),
    version: u32,
    drawn: Option<Tile>,
}

pub struct AtlasStore {
    planet: u16,
    shape: PlanetShape,
    generator: Arc<Generator>,
    store: Store,
    look: Arc<Look>,
    tiles: HashMap<(u16, u16), Arc<Tile>>,
    in_flight: HashSet<(u16, u16)>,
    versions: HashMap<(u16, u16), u32>,
    /// Tiles whose blocks changed, with the tick they first did.
    stale: HashMap<(u16, u16), u64>,
    done_tx: Sender<Built>,
    done: Receiver<Built>,
    /// Tiles drawn again since `take_changed`.
    changed: Vec<(u16, u16)>,
    last_used: u64,
}

impl AtlasStore {
    pub fn new(planet: u16, generator: Arc<Generator>, store: Store, registry: &BlockRegistry) -> Self {
        let palette = ColourPalette::builtin();
        let look = Look {
            block: registry.defs().iter().map(|d| d.top_color.unwrap_or(d.color)).collect(),
            liquid: registry.defs().iter().map(|d| d.liquid).collect(),
            palette: (0..256)
                .map(|i| palette.get(bm_core::Colour(i as u8)).map_or([0; 3], |c| c.rgb))
                .collect(),
            sea: generator.config().sea_level,
        };
        let (done_tx, done) = crossbeam_channel::unbounded();
        Self {
            planet,
            shape: generator.shape(),
            generator,
            store,
            look: Arc::new(look),
            tiles: HashMap::new(),
            in_flight: HashSet::new(),
            versions: HashMap::new(),
            stale: HashMap::new(),
            done_tx,
            done,
            changed: Vec::new(),
            last_used: 0,
        }
    }

    pub fn shape(&self) -> PlanetShape {
        self.shape
    }

    pub fn get(&self, tile: (u16, u16)) -> Option<&Arc<Tile>> {
        self.tiles.get(&tile)
    }

    pub fn is_building(&self, tile: (u16, u16)) -> bool {
        self.in_flight.contains(&tile)
    }

    pub fn running(&self) -> usize {
        self.in_flight.len()
    }

    /// The chunks a tile's columns cover.
    pub fn tile_chunks(&self, tile: (u16, u16)) -> Vec<ChunkPos> {
        let cs = CHUNK_SIZE as i64;
        let w = self.shape.width_chunks as i64;
        let (x0, z0) = (tile.0 as i64 * TILE_BLOCKS / cs, tile.1 as i64 * TILE_BLOCKS / cs);
        let per = TILE_BLOCKS / cs;
        let mut out = Vec::new();
        for cz in z0..(z0 + per).min(w) {
            for cx in x0..(x0 + per).min(w) {
                for cy in 0..self.shape.height_chunks as i32 {
                    out.push(ChunkPos::new(cx as i32, cy, cz as i32));
                }
            }
        }
        out
    }

    /// Draws `tile` on the worker pool, from `resident` (snapshots of the
    /// resident chunks it covers) and the disk; `regions` names each pixel's
    /// region. A tile already drawing is drawn again after (its version).
    pub fn request(&mut self, tile: (u16, u16), resident: HashMap<ChunkPos, Chunk>, regions: RegionMap) {
        if self.in_flight.contains(&tile) {
            return;
        }
        self.stale.remove(&tile);
        let version = *self.versions.entry(tile).and_modify(|v| *v += 1).or_insert(0);
        self.in_flight.insert(tile);
        let (planet, shape, g, store, look, tx) = (
            self.planet,
            self.shape,
            self.generator.clone(),
            self.store.clone(),
            self.look.clone(),
            self.done_tx.clone(),
        );
        rayon::spawn(move || {
            let drawn = draw(planet, shape, tile, &resident, &regions, &g, &store, &look);
            let _ = tx.send(Built { tile, version, drawn });
        });
    }

    /// Takes finished tiles in.
    pub fn receive(&mut self) {
        while let Ok(b) = self.done.try_recv() {
            self.in_flight.remove(&b.tile);
            if b.version != self.versions.get(&b.tile).copied().unwrap_or(0) {
                continue;
            }
            let Some(t) = b.drawn else { continue };
            self.tiles.insert(b.tile, Arc::new(t));
            self.changed.push(b.tile);
        }
    }

    /// Tiles drawn (first or again) since the last call.
    pub fn take_changed(&mut self) -> Vec<(u16, u16)> {
        std::mem::take(&mut self.changed)
    }

    /// A block in column `(x, z)` (canonical) changed.
    pub fn edited(&mut self, x: i64, z: i64, tick: u64) {
        let tile = bm_core::atlas::tile_of(x, z);
        if self.tiles.contains_key(&tile) {
            self.stale.entry(tile).or_insert(tick);
        }
    }

    /// Stale tiles that have waited `after` ticks: the caller draws them
    /// again if anyone is looking, else they are dropped (drawn afresh when
    /// next wanted).
    pub fn take_stale(&mut self, tick: u64, after: u64) -> Vec<(u16, u16)> {
        let due: Vec<_> = self
            .stale
            .iter()
            .filter(|&(_, &t)| tick.saturating_sub(t) >= after)
            .map(|(&k, _)| k)
            .collect();
        for k in &due {
            self.stale.remove(k);
        }
        due
    }

    /// Forgets a tile (nobody is looking at it).
    pub fn drop_tile(&mut self, tile: (u16, u16)) {
        self.tiles.remove(&tile);
    }

    /// Notes whether anyone has this planet's atlas open; after `idle_ticks`
    /// with nobody, the tiles leave memory.
    pub fn tend(&mut self, tick: u64, viewers: usize, idle_ticks: u64) {
        if viewers > 0 {
            self.last_used = tick;
        } else if !self.tiles.is_empty() && tick.saturating_sub(self.last_used) > idle_ticks {
            self.tiles.clear();
            self.stale.clear();
        }
    }

    pub fn held(&self) -> usize {
        self.tiles.len()
    }
}

/// Draws one tile. `None` when a saved chunk could not be read.
#[allow(clippy::too_many_arguments)]
fn draw(
    planet: u16,
    shape: PlanetShape,
    tile: (u16, u16),
    resident: &HashMap<ChunkPos, Chunk>,
    regions: &RegionMap,
    g: &Generator,
    store: &Store,
    look: &Look,
) -> Option<Tile> {
    let n = map_pixels(shape);
    let w = TILE_PIXELS.min(n - tile.0 as usize * TILE_PIXELS);
    let h = TILE_PIXELS.min(n - tile.1 as usize * TILE_PIXELS);
    // Each pixel's column: (top height, top block, water depth).
    let mut tops: Vec<(i64, [u8; 3], Option<i64>)> = vec![(0, [128; 3], None); w * h];
    let mut chunks: HashMap<ChunkPos, Arc<Chunk>> = HashMap::new();
    let mut chunk = |pos: ChunkPos| -> Option<Arc<Chunk>> {
        if let Some(c) = chunks.get(&pos) {
            return Some(c.clone());
        }
        let c = if let Some(c) = resident.get(&pos) {
            c.clone()
        } else {
            match store.chunk(planet, pos) {
                Ok(Some(bytes)) => match Chunk::decode(&bytes) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!("atlas tile {tile:?} of planet {planet}: chunk {pos:?} unreadable ({e})");
                        return None;
                    }
                },
                Ok(None) => g.generate(pos),
                Err(e) => {
                    tracing::warn!("atlas tile {tile:?} of planet {planet}: reading chunk {pos:?} failed: {e:#}");
                    return None;
                }
            }
        };
        let c = Arc::new(c);
        chunks.insert(pos, c.clone());
        Some(c)
    };
    let top_y = shape.height_blocks() - 1;
    for pz in 0..h {
        for px in 0..w {
            let (x, z) = pixel_column(shape, tile, px, pz).expect("inside the tile");
            // The top block; under water, down to the floor for its depth.
            let mut water: Option<(i64, bm_core::BlockState)> = None;
            let mut top = None;
            let mut y = top_y;
            while y >= 0 {
                let c = chunk(bm_core::BlockPos::new(x, y, z).chunk())?;
                if water.is_none() && c.uniform() == Some(bm_core::BlockId::AIR) {
                    // A whole empty chunk.
                    y = (y >> 5 << 5) - 1;
                    continue;
                }
                let s = c.state(LocalPos::new((x & 31) as u8, (y & 31) as u8, (z & 31) as u8));
                match water {
                    Some((wy, ws)) => {
                        if (!s.id.is_air() && !look.liquid(s)) || wy - y >= MAX_DEPTH {
                            top = Some((wy, look.colour(ws), Some(wy - y)));
                            break;
                        }
                    }
                    None if s.id.is_air() => {}
                    None if look.liquid(s) => water = Some((y, s)),
                    None => {
                        top = Some((y, look.colour(s), None));
                        break;
                    }
                }
                y -= 1;
            }
            let top = top.or(water.map(|(wy, ws)| (wy, look.colour(ws), Some(wy + 1))));
            tops[pz * w + px] = top.unwrap_or((0, [40; 3], None));
        }
    }
    let mut rgb = Vec::with_capacity(w * h);
    let mut region = Vec::with_capacity(w * h);
    for pz in 0..h {
        for px in 0..w {
            let (top, colour, depth) = tops[pz * w + px];
            let west = if px > 0 { tops[pz * w + px - 1].0 } else { top };
            let north = if pz > 0 { tops[(pz - 1) * w + px].0 } else { top };
            rgb.push(shade(colour, top, west, north, look.sea, depth));
            let (x, z) = pixel_column(shape, tile, px, pz).expect("inside the tile");
            region.push(regions.locate(x as f64, z as f64) as u16);
        }
    }
    Some(Tile { w, h, rgb, region })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(regions: Vec<u16>) -> Tile {
        Tile {
            w: 2,
            h: 2,
            rgb: vec![[10, 20, 30]; 4],
            region: regions,
        }
    }

    fn pixels(frame: &[u8]) -> Vec<u8> {
        let msg: ServerMsg = bm_protocol::decode(&frame[4..]).expect("a frame");
        let ServerMsg::AtlasTile { rgb, .. } = msg else {
            panic!("{msg:?}")
        };
        lz4_flex::decompress_size_prepended(&rgb).unwrap()
    }

    #[test]
    fn undiscovered_land_is_fog_and_unseen_tiles_are_not_sent() {
        let t = tile(vec![1, 1, 2, 2]);
        assert!(t.frame_for(0, (0, 0), &[3]).is_none(), "nothing of it discovered");
        let px = pixels(&t.frame_for(0, (0, 0), &[1]).unwrap());
        assert_eq!(&px[..6], &[10, 20, 30, 10, 20, 30]);
        assert_eq!(&px[6..], &[0; 6], "region 2's land is never sent");
    }
}
