//! Far copies of chunks (milestone 20): the planet past the view radius,
//! drawn as coarser voxels, as Boundless did.
//!
//! A level 1 copy halves a chunk on every axis (16³ cells of 2×2×2 blocks), a
//! level 2 copy halves it again (8³ cells of 4×4×4). Each halving takes eight
//! parts to one cell by the same rule:
//!
//! - **solid** when at least [`SOLID_PARTS`] of the eight are solid, so thin
//!   walls and trunks survive a level. The cell takes the commonest state
//!   among its solid parts that face open space (what a player would see),
//!   else among all its solid parts. A buried ore is never what a cell
//!   shows unless it is most of it, so a far copy is no x-ray (ADR-0003).
//! - else **liquid** when its liquid parts outnumber its open ones;
//! - else **open** (air).
//!
//! Models (plants, torches, machines) count as open; cut blocks count as
//! their block, uncut. Ties go to the smallest state, so a copy depends only
//! on its chunk.

use crate::{BlockFlags, BlockId, BlockState, CHUNK_SIZE, CHUNK_VOLUME, Chunk, Colour, Shape};

/// Bumped whenever the encoding or the halving rule changes; saved copies
/// of another version are remade from their chunks.
pub const FAR_FORMAT_VERSION: u8 = 1;

/// Solid parts (of eight) that make a solid cell.
pub const SOLID_PARTS: u8 = 3;

/// The coarsest level (cells of 4×4×4 blocks).
pub const MAX_LEVEL: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FarChunk {
    level: u8,
    /// The states in the copy, in the order cells first use them.
    palette: Vec<BlockState>,
    /// Each cell's palette index, indexed `x | z << s | y << 2s` like
    /// `LocalPos::index`; empty when the palette has one state.
    cells: Vec<u16>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FarDecodeError {
    #[error("far copy truncated")]
    Truncated,
    #[error("far copy too large")]
    TooLarge,
    #[error("far copy is not lz4")]
    Compression,
    #[error("far copy format {0} unknown")]
    Version(u8),
    #[error("far copy level {0} unknown")]
    Level(u8),
    #[error("far copy palette invalid")]
    Palette,
    #[error("far copy index out of range")]
    Index,
}

/// Cells along one axis at a level.
pub const fn cells_across(level: u8) -> usize {
    CHUNK_SIZE as usize >> level
}

impl FarChunk {
    fn from_dense(level: u8, dense: &[BlockState]) -> Self {
        let mut palette: Vec<BlockState> = Vec::new();
        let mut cells = Vec::with_capacity(dense.len());
        for c in dense {
            let i = match palette.iter().position(|p| p == c) {
                Some(i) => i,
                None => {
                    palette.push(*c);
                    palette.len() - 1
                }
            };
            cells.push(i as u16);
        }
        if palette.len() == 1 {
            cells = Vec::new();
        }
        Self { level, palette, cells }
    }

    fn dense(&self) -> Vec<BlockState> {
        let count = self.across().pow(3);
        if self.cells.is_empty() {
            vec![self.palette[0]; count]
        } else {
            self.cells.iter().map(|i| self.palette[*i as usize]).collect()
        }
    }

    /// The level 1 copy of a chunk.
    pub fn from_chunk(chunk: &Chunk, flags: &BlockFlags) -> Self {
        let dense = match chunk.uniform_state() {
            Some(s) => vec![s; CHUNK_VOLUME],
            None => chunk.to_dense_states(),
        };
        Self::from_dense(1, &halve(CHUNK_SIZE as usize, &dense, flags))
    }

    /// The next level's copy (a level 2 copy of a level 1 one). The coarsest
    /// level stays as it is.
    pub fn coarser(&self, flags: &BlockFlags) -> Self {
        if self.level >= MAX_LEVEL {
            return self.clone();
        }
        Self::from_dense(self.level + 1, &halve(self.across(), &self.dense(), flags))
    }

    /// Every state the copy holds (each once).
    pub fn states(&self) -> &[BlockState] {
        &self.palette
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    /// Cells along one axis.
    pub fn across(&self) -> usize {
        cells_across(self.level)
    }

    /// The cell at `(x, y, z)`, each in `0..across()`.
    pub fn cell(&self, x: usize, y: usize, z: usize) -> BlockState {
        if self.cells.is_empty() {
            return self.palette[0];
        }
        let s = self.across().trailing_zeros();
        self.palette[self.cells[x | z << s | y << (2 * s)] as usize]
    }

    /// The one state every cell holds, if so.
    pub fn uniform(&self) -> Option<BlockState> {
        (self.palette.len() == 1).then_some(self.palette[0])
    }

    /// The copy as a full-size chunk, each cell filling its blocks, for the
    /// near terrain's mesher.
    pub fn expand(&self) -> Chunk {
        if let Some(s) = self.uniform() {
            return Chunk::filled_state(s);
        }
        let n = CHUNK_SIZE as usize;
        let shift = self.level;
        let mut dense = vec![BlockState::AIR; CHUNK_VOLUME];
        for y in 0..n {
            for z in 0..n {
                for x in 0..n {
                    dense[x | z << 5 | y << 10] = self.cell(x >> shift, y >> shift, z >> shift);
                }
            }
        }
        Chunk::from_dense_states(&dense)
    }

    pub fn encode(&self) -> Vec<u8> {
        let (palette, index) = (&self.palette, &self.cells);
        let mut raw = Vec::with_capacity(4 + palette.len() * 3 + index.len() * 2);
        raw.push(FAR_FORMAT_VERSION);
        raw.push(self.level);
        raw.extend_from_slice(&(palette.len() as u16).to_le_bytes());
        for p in palette {
            raw.extend_from_slice(&p.id.0.to_le_bytes());
            raw.push(p.colour.0);
        }
        if palette.len() > 1 {
            let wide = palette.len() > 256;
            for &i in index {
                if wide {
                    raw.extend_from_slice(&i.to_le_bytes());
                } else {
                    raw.push(i as u8);
                }
            }
        }
        lz4_flex::compress_prepend_size(&raw)
    }

    /// Decodes and validates bytes from the disk or the network; never
    /// panics on malformed input.
    pub fn decode(bytes: &[u8]) -> Result<Self, FarDecodeError> {
        let cells = cells_across(1).pow(3);
        let max_raw = 4 + cells * 3 + cells * 2;
        if bytes.len() < 4 {
            return Err(FarDecodeError::Truncated);
        }
        let claimed = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if claimed > max_raw {
            return Err(FarDecodeError::TooLarge);
        }
        let raw = lz4_flex::decompress_size_prepended(bytes).map_err(|_| FarDecodeError::Compression)?;
        let mut at = 0usize;
        let mut take = |n: usize| -> Result<&[u8], FarDecodeError> {
            let s = raw.get(at..at + n).ok_or(FarDecodeError::Truncated)?;
            at += n;
            Ok(s)
        };
        let version = take(1)?[0];
        if version != FAR_FORMAT_VERSION {
            return Err(FarDecodeError::Version(version));
        }
        let level = take(1)?[0];
        if !(1..=MAX_LEVEL).contains(&level) {
            return Err(FarDecodeError::Level(level));
        }
        let count = cells_across(level).pow(3);
        let b = take(2)?;
        let len = u16::from_le_bytes([b[0], b[1]]) as usize;
        if len == 0 || len > count {
            return Err(FarDecodeError::Palette);
        }
        let mut palette = Vec::with_capacity(len);
        for _ in 0..len {
            let b = take(3)?;
            palette.push(BlockState {
                id: BlockId(u16::from_le_bytes([b[0], b[1]])),
                colour: Colour(b[2]),
                shape: Shape::FULL,
            });
        }
        let dense = if len == 1 {
            vec![palette[0]; count]
        } else {
            let wide = len > 256;
            let b = take(count * if wide { 2 } else { 1 })?;
            let mut cells = Vec::with_capacity(count);
            for k in 0..count {
                let i = if wide {
                    u16::from_le_bytes([b[2 * k], b[2 * k + 1]]) as usize
                } else {
                    b[k] as usize
                };
                cells.push(*palette.get(i).ok_or(FarDecodeError::Index)?);
            }
            cells
        };
        if at != raw.len() {
            return Err(FarDecodeError::Truncated);
        }
        // Rebuilt from the cells, so a palette listing a state twice or out
        // of first-use order still decodes to the one canonical copy.
        Ok(Self::from_dense(level, &dense))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Open,
    Liquid,
    Solid,
}

fn kind(flags: &BlockFlags, s: BlockState) -> Kind {
    if flags.mount(s.id).is_some() {
        Kind::Open
    } else if flags.liquid(s.id) {
        Kind::Liquid
    } else if flags.solid(s.id) {
        Kind::Solid
    } else {
        Kind::Open
    }
}

/// Halves a dense `n³` volume (indexed like `LocalPos::index` at side `n`)
/// by the module's rule.
fn halve(n: usize, src: &[BlockState], flags: &BlockFlags) -> Vec<BlockState> {
    let s = n.trailing_zeros();
    let at = |x: usize, y: usize, z: usize| src[x | z << s | y << (2 * s)];
    // A part faces open space when a neighbour inside the volume does not
    // hide it (the volume's edge counts as hidden: the neighbour chunk is
    // not known here, and a buried edge must not leak what it holds).
    let hides = |x: usize, y: usize, z: usize| {
        let b = at(x, y, z);
        flags.opaque(b.id) && b.shape.is_full() && flags.mount(b.id).is_none()
    };
    let exposed = |x: usize, y: usize, z: usize| {
        (x > 0 && !hides(x - 1, y, z))
            || (x + 1 < n && !hides(x + 1, y, z))
            || (y > 0 && !hides(x, y - 1, z))
            || (y + 1 < n && !hides(x, y + 1, z))
            || (z > 0 && !hides(x, y, z - 1))
            || (z + 1 < n && !hides(x, y, z + 1))
    };
    let h = n / 2;
    let hs = h.trailing_zeros();
    let mut out = vec![BlockState::AIR; h * h * h];
    for cy in 0..h {
        for cz in 0..h {
            for cx in 0..h {
                let mut solid: [(BlockState, bool); 8] = [(BlockState::AIR, false); 8];
                let (mut n_solid, mut n_liquid, mut n_open) = (0usize, 0usize, 0usize);
                let mut liquids: [BlockState; 8] = [BlockState::AIR; 8];
                for k in 0..8 {
                    let (x, y, z) = (cx * 2 + (k & 1), cy * 2 + (k >> 2 & 1), cz * 2 + (k >> 1 & 1));
                    let b = at(x, y, z);
                    match kind(flags, b) {
                        Kind::Solid => {
                            let plain = BlockState {
                                id: b.id,
                                colour: b.colour,
                                shape: Shape::FULL,
                            };
                            solid[n_solid] = (plain, exposed(x, y, z));
                            n_solid += 1;
                        }
                        Kind::Liquid => {
                            liquids[n_liquid] = BlockState::plain(b.id);
                            n_liquid += 1;
                        }
                        Kind::Open => n_open += 1,
                    }
                }
                let cell = if n_solid >= SOLID_PARTS as usize {
                    let parts = &solid[..n_solid];
                    let seen: Vec<BlockState> = parts.iter().filter(|p| p.1).map(|p| p.0).collect();
                    if seen.is_empty() {
                        commonest(&parts.iter().map(|p| p.0).collect::<Vec<_>>())
                    } else {
                        commonest(&seen)
                    }
                } else if n_liquid > n_open {
                    commonest(&liquids[..n_liquid])
                } else {
                    BlockState::AIR
                };
                out[cx | cz << hs | cy << (2 * hs)] = cell;
            }
        }
    }
    out
}

/// The commonest state of a non-empty list, ties to the smallest.
fn commonest(list: &[BlockState]) -> BlockState {
    let key = |s: &BlockState| (s.id.0, s.colour.0);
    let mut best = list[0];
    let mut best_n = 0;
    for a in list {
        let n = list.iter().filter(|b| *b == a).count();
        if n > best_n || (n == best_n && key(a) < key(&best)) {
            best = *a;
            best_n = n;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockRegistry, LocalPos};

    fn reg() -> BlockRegistry {
        BlockRegistry::builtin()
    }

    fn id(r: &BlockRegistry, name: &str) -> BlockId {
        r.id(name).unwrap()
    }

    /// Stone below 16, a turf layer, air above; a flower on the turf.
    fn hillside(r: &BlockRegistry) -> Chunk {
        let mut c = Chunk::empty();
        for y in 0..32u8 {
            for z in 0..32u8 {
                for x in 0..32u8 {
                    let b = match y {
                        0..=13 => id(r, "stone"),
                        14..=15 => id(r, "turf"),
                        _ => BlockId::AIR,
                    };
                    c.set(LocalPos::new(x, y, z), b);
                }
            }
        }
        c
    }

    #[test]
    fn a_far_copy_keeps_the_ground_where_it_is() {
        let r = reg();
        let f = FarChunk::from_chunk(&hillside(&r), &r.flags());
        assert_eq!(f.across(), 16);
        assert_eq!(f.cell(3, 6, 3).id, id(&r, "stone"));
        assert_eq!(f.cell(3, 7, 3).id, id(&r, "turf"), "the top face shows the turf");
        assert_eq!(f.cell(3, 8, 3), BlockState::AIR);
        let g = f.coarser(&r.flags());
        assert_eq!(g.level(), 2);
        assert_eq!(g.cell(1, 3, 1).id, id(&r, "turf"));
        assert_eq!(g.cell(1, 4, 1), BlockState::AIR);
        // Expanded back, the ground's top is where it was.
        let e = f.expand();
        assert_eq!(e.get(LocalPos::new(5, 15, 5)), id(&r, "turf"));
        assert_eq!(e.get(LocalPos::new(5, 16, 5)), BlockId::AIR);
    }

    #[test]
    fn buried_ore_never_shows() {
        let r = reg();
        let mut c = Chunk::filled(id(&r, "stone"));
        let ore = r
            .iter()
            .find(|(_, d)| d.name.contains("ore"))
            .map(|(i, _)| i)
            .expect("an ore");
        // Ore everywhere but on a stone skin: never what a cell shows.
        for y in 2..30u8 {
            for z in 2..30u8 {
                for x in 2..30u8 {
                    if (x + y + z) % 3 == 0 {
                        c.set(LocalPos::new(x, y, z), ore);
                    }
                }
            }
        }
        // Open the top so the surface faces air.
        for z in 0..32u8 {
            for x in 0..32u8 {
                c.set(LocalPos::new(x, 31, z), BlockId::AIR);
                c.set(LocalPos::new(x, 30, z), BlockId::AIR);
            }
        }
        let f = FarChunk::from_chunk(&c, &r.flags());
        for g in [f.clone(), f.coarser(&r.flags())] {
            let n = g.across();
            for y in 0..n {
                for z in 0..n {
                    for x in 0..n {
                        assert_ne!(g.cell(x, y, z).id, ore, "ore at {x} {y} {z} level {}", g.level());
                    }
                }
            }
        }
    }

    #[test]
    fn thin_walls_survive_and_models_vanish() {
        let r = reg();
        let mut c = Chunk::empty();
        // A wall one block thick, eight tall.
        for y in 0..8u8 {
            for x in 0..32u8 {
                c.set(LocalPos::new(x, y, 10), id(&r, "stone"));
            }
        }
        let torch = r.iter().find(|(_, d)| d.is_prop()).map(|(i, _)| i).expect("a prop");
        c.set(LocalPos::new(3, 20, 3), torch);
        // A pillar one block across, eight tall.
        for y in 0..8u8 {
            c.set(LocalPos::new(20, y, 20), id(&r, "stone"));
        }
        let f = FarChunk::from_chunk(&c, &r.flags());
        // A wall one block thick fills four of a cell's eight parts: solid,
        // and still solid a level up (four of eight again).
        assert_eq!(f.cell(2, 1, 5).id, id(&r, "stone"));
        assert_eq!(f.coarser(&r.flags()).cell(1, 0, 2).id, id(&r, "stone"));
        // A pillar fills two: too thin to see from afar.
        assert_eq!(f.cell(10, 1, 10), BlockState::AIR);
        assert_eq!(f.cell(1, 10, 1), BlockState::AIR, "a torch is not terrain");
    }

    #[test]
    fn water_wins_over_air_only_when_there_is_more_of_it() {
        let r = reg();
        let water = id(&r, "water");
        let mut c = Chunk::empty();
        for y in 0..17u8 {
            for z in 0..32u8 {
                for x in 0..32u8 {
                    c.set(LocalPos::new(x, y, z), water);
                }
            }
        }
        let f = FarChunk::from_chunk(&c, &r.flags());
        assert_eq!(f.cell(0, 7, 0).id, water);
        // Cell 8 holds y 16 (water) and 17 (air): a tie stays open.
        assert_eq!(f.cell(0, 8, 0), BlockState::AIR);
    }

    #[test]
    fn copies_round_trip_and_bad_bytes_are_refused() {
        let r = reg();
        let f = FarChunk::from_chunk(&hillside(&r), &r.flags());
        for g in [f.clone(), f.coarser(&r.flags())] {
            assert_eq!(FarChunk::decode(&g.encode()).unwrap(), g);
        }
        let air = FarChunk::from_chunk(&Chunk::empty(), &r.flags());
        assert!(air.encode().len() < 16);
        assert_eq!(FarChunk::decode(&air.encode()).unwrap(), air);
        assert!(FarChunk::decode(&[]).is_err());
        assert!(FarChunk::decode(&[255, 255, 255, 255, 0]).is_err());
        let mut raw = vec![FAR_FORMAT_VERSION, 1, 2, 0, 1, 0, 0, 2, 0, 0];
        raw.extend(std::iter::repeat_n(7u8, 4096));
        assert_eq!(
            FarChunk::decode(&lz4_flex::compress_prepend_size(&raw)),
            Err(FarDecodeError::Index)
        );
        let raw = [FAR_FORMAT_VERSION, 3, 1, 0, 0, 0, 0];
        assert_eq!(
            FarChunk::decode(&lz4_flex::compress_prepend_size(&raw)),
            Err(FarDecodeError::Level(3))
        );
    }

    #[test]
    fn many_kinds_take_wide_indices() {
        let r = reg();
        let mut cells = Vec::new();
        for i in 0..4096u32 {
            cells.push(BlockState {
                id: BlockId((i % 300) as u16 + 1),
                colour: Colour(0),
                shape: Shape::FULL,
            });
        }
        let f = FarChunk::from_dense(1, &cells);
        assert_eq!(FarChunk::decode(&f.encode()).unwrap(), f);
        let _ = r;
    }
}
