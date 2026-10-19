//! Palette-compressed 32³ chunk storage and its versioned wire/save format.
//!
//! A chunk stores the distinct block states it contains (the palette: a block
//! type with its colour and shape, ADR-0007) and, per voxel, an index into
//! that palette packed into `bits` bits. A chunk of one
//! block (air, solid stone) stores no indices at all. Indices never straddle a
//! `u64` word, so reads are one load, one shift, one mask.
//!
//! Never assume a fixed bits-per-voxel: it grows as the palette grows.

use crate::block::{BlockId, BlockState, Colour, Shape};
use crate::coords::{CHUNK_VOLUME, LocalPos};

/// Version of the serialized chunk format. Bump on any change to `encode`.
pub const CHUNK_FORMAT_VERSION: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    palette: Vec<BlockState>,
    bits: u8,
    words: Vec<u64>,
}

fn bits_for(palette_len: usize) -> u8 {
    if palette_len <= 1 {
        0
    } else {
        (usize::BITS - (palette_len - 1).leading_zeros()) as u8
    }
}

fn words_for(bits: u8) -> usize {
    if bits == 0 {
        0
    } else {
        let per_word = 64 / bits as usize;
        CHUNK_VOLUME.div_ceil(per_word)
    }
}

impl Chunk {
    /// A chunk filled with one block.
    pub fn filled(id: BlockId) -> Self {
        Self::filled_state(BlockState::plain(id))
    }

    /// A chunk filled with one block state.
    pub fn filled_state(state: BlockState) -> Self {
        Self {
            palette: vec![state],
            bits: 0,
            words: Vec::new(),
        }
    }

    pub fn empty() -> Self {
        Self::filled(BlockId::AIR)
    }

    /// Builds a chunk from a dense array of plain blocks (natural colour,
    /// full shape), choosing the smallest palette.
    pub fn from_dense(blocks: &[BlockId]) -> Self {
        assert_eq!(blocks.len(), CHUNK_VOLUME);
        let states: Vec<BlockState> = blocks.iter().map(|&b| BlockState::plain(b)).collect();
        Self::from_dense_states(&states)
    }

    /// Builds a chunk from a dense array of states, choosing the smallest
    /// palette.
    pub fn from_dense_states(blocks: &[BlockState]) -> Self {
        assert_eq!(blocks.len(), CHUNK_VOLUME);
        let mut palette: Vec<BlockState> = Vec::new();
        let mut indices = vec![0u16; CHUNK_VOLUME];
        // Most chunks have a handful of distinct blocks; a linear scan with a
        // last-hit cache beats hashing here.
        let mut last = (BlockState::plain(BlockId(u16::MAX)), 0u16);
        for (i, &b) in blocks.iter().enumerate() {
            if b != last.0 {
                let idx = match palette.iter().position(|&p| p == b) {
                    Some(p) => p,
                    None => {
                        palette.push(b);
                        palette.len() - 1
                    }
                };
                last = (b, idx as u16);
            }
            indices[i] = last.1;
        }
        let bits = bits_for(palette.len());
        let mut chunk = Self {
            palette,
            bits,
            words: vec![0; words_for(bits)],
        };
        if bits > 0 {
            for (i, &idx) in indices.iter().enumerate() {
                chunk.write_index(i, idx as u64);
            }
        }
        chunk
    }

    #[inline]
    fn read_index(&self, i: usize) -> usize {
        if self.bits == 0 {
            return 0;
        }
        let bits = self.bits as usize;
        let per_word = 64 / bits;
        let word = self.words[i / per_word];
        ((word >> ((i % per_word) * bits)) & ((1u64 << bits) - 1)) as usize
    }

    #[inline]
    fn write_index(&mut self, i: usize, v: u64) {
        let bits = self.bits as usize;
        let per_word = 64 / bits;
        let shift = (i % per_word) * bits;
        let mask = ((1u64 << bits) - 1) << shift;
        let w = &mut self.words[i / per_word];
        *w = (*w & !mask) | (v << shift);
    }

    /// The block type at `p` (what physics, light and tools ask about).
    #[inline]
    pub fn get(&self, p: LocalPos) -> BlockId {
        self.get_index(p.index())
    }

    #[inline]
    pub fn get_index(&self, i: usize) -> BlockId {
        self.palette[self.read_index(i)].id
    }

    /// The full state at `p`: type, colour and shape.
    #[inline]
    pub fn state(&self, p: LocalPos) -> BlockState {
        self.state_index(p.index())
    }

    #[inline]
    pub fn state_index(&self, i: usize) -> BlockState {
        self.palette[self.read_index(i)]
    }

    /// Sets a plain block (natural colour, full shape). Returns the previous
    /// block type.
    pub fn set(&mut self, p: LocalPos, id: BlockId) -> BlockId {
        self.set_state(p, BlockState::plain(id)).id
    }

    /// Sets a block state, growing the palette (and repacking) if it is new.
    /// Returns the previous state.
    pub fn set_state(&mut self, p: LocalPos, state: BlockState) -> BlockState {
        let i = p.index();
        let old = self.state_index(i);
        if old == state {
            return old;
        }
        let idx = match self.palette.iter().position(|&b| b == state) {
            Some(idx) => idx,
            None => {
                self.palette.push(state);
                let needed = bits_for(self.palette.len());
                if needed > self.bits {
                    self.repack(needed);
                }
                self.palette.len() - 1
            }
        };
        self.write_index(i, idx as u64);
        old
    }

    fn repack(&mut self, bits: u8) {
        let old = std::mem::replace(
            self,
            Self {
                palette: Vec::new(),
                bits,
                words: vec![0; words_for(bits)],
            },
        );
        for i in 0..CHUNK_VOLUME {
            let v = old.read_index(i) as u64;
            self.write_index(i, v);
        }
        self.palette = old.palette;
    }

    /// The single block type this chunk is made of, if it is uniform in
    /// state. Exact only after [`Chunk::compact`] (dead palette entries hide
    /// uniformity).
    pub fn uniform(&self) -> Option<BlockId> {
        (self.palette.len() == 1).then(|| self.palette[0].id)
    }

    pub fn palette(&self) -> &[BlockState] {
        &self.palette
    }

    /// Whether any voxel may be of type `id` (palette entries can be dead
    /// until [`Chunk::compact`], so `true` is a "maybe").
    pub fn may_contain(&self, id: BlockId) -> bool {
        self.palette.iter().any(|s| s.id == id)
    }

    pub fn bits(&self) -> u8 {
        self.bits
    }

    /// Drops palette entries no voxel uses any more and shrinks `bits`.
    pub fn compact(&mut self) {
        if self.bits == 0 {
            return;
        }
        let mut used = vec![false; self.palette.len()];
        for i in 0..CHUNK_VOLUME {
            used[self.read_index(i)] = true;
        }
        if used.iter().all(|&u| u) {
            return;
        }
        let dense = self.to_dense_states();
        *self = Self::from_dense_states(&dense);
    }

    /// Every voxel's type, in `LocalPos::index` order.
    pub fn to_dense(&self) -> Vec<BlockId> {
        (0..CHUNK_VOLUME).map(|i| self.get_index(i)).collect()
    }

    /// Every voxel's state, in `LocalPos::index` order.
    pub fn to_dense_states(&self) -> Vec<BlockState> {
        (0..CHUNK_VOLUME).map(|i| self.state_index(i)).collect()
    }

    /// Serializes (after compacting a copy) and LZ4-compresses. Format v2:
    /// `version u8 | palette_len u16 | (id u16, colour u8, shape u16)* |
    /// bits u8 | words u64*`, little-endian, then LZ4 with the uncompressed
    /// size prepended. Version 1 (palette of bare `id u16`) still decodes, as
    /// natural colour and full shape.
    pub fn encode(&self) -> Vec<u8> {
        let mut c = self.clone();
        c.compact();
        let mut raw = Vec::with_capacity(4 + c.palette.len() * 5 + c.words.len() * 8);
        raw.push(CHUNK_FORMAT_VERSION);
        raw.extend_from_slice(&(c.palette.len() as u16).to_le_bytes());
        for b in &c.palette {
            raw.extend_from_slice(&b.id.0.to_le_bytes());
            raw.push(b.colour.0);
            raw.extend_from_slice(&b.shape.0.to_le_bytes());
        }
        raw.push(c.bits);
        for w in &c.words {
            raw.extend_from_slice(&w.to_le_bytes());
        }
        lz4_flex::compress_prepend_size(&raw)
    }

    /// Decodes and fully validates bytes that came from disk or the network.
    /// Never panics on malformed input.
    pub fn decode(bytes: &[u8]) -> Result<Self, ChunkDecodeError> {
        // Largest legal payload: a palette entry per voxel at 16 bits.
        const MAX_RAW: usize = 4 + CHUNK_VOLUME * 5 + CHUNK_VOLUME * 8;
        if bytes.len() < 4 {
            return Err(ChunkDecodeError::Truncated);
        }
        let claimed = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if claimed > MAX_RAW {
            return Err(ChunkDecodeError::TooLarge);
        }
        let raw = lz4_flex::decompress_size_prepended(bytes).map_err(|_| ChunkDecodeError::Compression)?;
        let mut r = Reader { b: &raw, at: 0 };
        let version = r.u8()?;
        if version != 1 && version != CHUNK_FORMAT_VERSION {
            return Err(ChunkDecodeError::Version(version));
        }
        let len = r.u16()? as usize;
        if len == 0 || len > CHUNK_VOLUME {
            return Err(ChunkDecodeError::Palette);
        }
        let mut palette = Vec::with_capacity(len);
        for _ in 0..len {
            let id = BlockId(r.u16()?);
            palette.push(if version == 1 {
                BlockState::plain(id)
            } else {
                BlockState {
                    id,
                    colour: Colour(r.u8()?),
                    shape: Shape(r.u16()?),
                }
            });
        }
        let bits = r.u8()?;
        if bits != bits_for(len) {
            return Err(ChunkDecodeError::Bits);
        }
        let n = words_for(bits);
        let mut words = Vec::with_capacity(n);
        for _ in 0..n {
            words.push(r.u64()?);
        }
        if r.at != raw.len() {
            return Err(ChunkDecodeError::TrailingBytes);
        }
        let chunk = Self { palette, bits, words };
        if bits > 0 {
            for i in 0..CHUNK_VOLUME {
                if chunk.read_index(i) >= len {
                    return Err(ChunkDecodeError::IndexOutOfPalette);
                }
            }
        }
        Ok(chunk)
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], ChunkDecodeError> {
        let s = self.b.get(self.at..self.at + N).ok_or(ChunkDecodeError::Truncated)?;
        self.at += N;
        Ok(s.try_into().expect("slice has length N"))
    }
    fn u8(&mut self) -> Result<u8, ChunkDecodeError> {
        Ok(self.take::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16, ChunkDecodeError> {
        Ok(u16::from_le_bytes(self.take()?))
    }
    fn u64(&mut self) -> Result<u64, ChunkDecodeError> {
        Ok(u64::from_le_bytes(self.take()?))
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ChunkDecodeError {
    #[error("chunk data is truncated")]
    Truncated,
    #[error("chunk data claims an impossible size")]
    TooLarge,
    #[error("chunk data is not valid LZ4")]
    Compression,
    #[error("chunk format version {0} is not supported")]
    Version(u8),
    #[error("chunk palette length is invalid")]
    Palette,
    #[error("chunk bits-per-voxel does not match its palette")]
    Bits,
    #[error("chunk data has trailing bytes")]
    TrailingBytes,
    #[error("chunk voxel indexes outside its palette")]
    IndexOutOfPalette,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng, rngs::StdRng};

    fn random_dense(rng: &mut StdRng, kinds: u16) -> Vec<BlockId> {
        (0..CHUNK_VOLUME).map(|_| BlockId(rng.random_range(0..kinds))).collect()
    }

    #[test]
    fn uniform_chunk_stores_no_indices() {
        let c = Chunk::filled(BlockId(3));
        assert_eq!(c.bits(), 0);
        assert_eq!(c.get(LocalPos::new(5, 6, 7)), BlockId(3));
        assert_eq!(c.uniform(), Some(BlockId(3)));
    }

    #[test]
    fn set_matches_a_dense_model_across_palette_growth() {
        let mut rng = StdRng::seed_from_u64(1);
        let mut chunk = Chunk::empty();
        let mut model = vec![BlockId::AIR; CHUNK_VOLUME];
        // Grows through 1, 2, 3, ... 9 bits.
        for _ in 0..60_000 {
            let i = rng.random_range(0..CHUNK_VOLUME);
            let id = BlockId(rng.random_range(0..300));
            let old = chunk.set(LocalPos::from_index(i), id);
            assert_eq!(old, model[i]);
            model[i] = id;
        }
        assert_eq!(chunk.to_dense(), model);
        assert!(chunk.bits() >= 9);
    }

    #[test]
    fn from_dense_round_trips() {
        let mut rng = StdRng::seed_from_u64(2);
        for kinds in [1, 2, 3, 5, 17, 1000] {
            let dense = random_dense(&mut rng, kinds);
            assert_eq!(Chunk::from_dense(&dense).to_dense(), dense);
        }
    }

    #[test]
    fn compact_drops_dead_entries() {
        let mut c = Chunk::empty();
        c.set(LocalPos::new(1, 1, 1), BlockId(7));
        c.set(LocalPos::new(1, 1, 1), BlockId::AIR);
        assert_eq!(c.palette().len(), 2);
        c.compact();
        assert_eq!(c.uniform(), Some(BlockId::AIR));
    }

    #[test]
    fn encode_decode_round_trips() {
        let mut rng = StdRng::seed_from_u64(3);
        for kinds in [1, 2, 4, 33, 4000] {
            let c = Chunk::from_dense(&random_dense(&mut rng, kinds));
            let back = Chunk::decode(&c.encode()).unwrap();
            assert_eq!(back.to_dense(), c.to_dense());
        }
    }

    #[test]
    fn typical_chunks_compress_small() {
        // A terrain-like chunk: stone below, soil, turf, air above.
        let mut dense = vec![BlockId::AIR; CHUNK_VOLUME];
        for (i, b) in dense.iter_mut().enumerate() {
            let y = LocalPos::from_index(i).y;
            *b = match y {
                0..=12 => BlockId(2),
                13..=15 => BlockId(4),
                16 => BlockId(5),
                _ => BlockId::AIR,
            };
        }
        let bytes = Chunk::from_dense(&dense).encode();
        assert!(bytes.len() < 400, "{} bytes", bytes.len());
        assert!(Chunk::filled(BlockId::AIR).encode().len() < 16);
    }

    #[test]
    fn decode_rejects_garbage_without_panicking() {
        let mut rng = StdRng::seed_from_u64(4);
        let good = Chunk::from_dense(&random_dense(&mut rng, 6)).encode();
        for _ in 0..2000 {
            let mut bytes = good.clone();
            let n = rng.random_range(1..8);
            for _ in 0..n {
                let i = rng.random_range(0..bytes.len());
                bytes[i] = rng.random();
            }
            bytes.truncate(rng.random_range(0..=bytes.len()));
            let _ = Chunk::decode(&bytes);
        }
        assert_eq!(Chunk::decode(&[]), Err(ChunkDecodeError::Truncated));
        assert_eq!(Chunk::decode(&[255, 255, 255, 255, 0]), Err(ChunkDecodeError::TooLarge));
    }

    #[test]
    fn colours_and_shapes_are_separate_palette_entries_and_round_trip() {
        let mut c = Chunk::filled(BlockId(2));
        let red = BlockState {
            id: BlockId(2),
            colour: Colour(17),
            shape: Shape::FULL,
        };
        let cut = BlockState {
            id: BlockId(2),
            colour: Colour(17),
            shape: Shape(0x1_05),
        };
        c.set_state(LocalPos::new(1, 2, 3), red);
        c.set_state(LocalPos::new(4, 5, 6), cut);
        assert_eq!(c.palette().len(), 3);
        assert_eq!(c.get(LocalPos::new(1, 2, 3)), BlockId(2), "the type ignores colour");
        assert_eq!(c.state(LocalPos::new(1, 2, 3)), red);
        let back = Chunk::decode(&c.encode()).unwrap();
        assert_eq!(back.to_dense_states(), c.to_dense_states());
        // A plain set clears colour and shape.
        assert_eq!(c.set(LocalPos::new(1, 2, 3), BlockId(2)), BlockId(2));
        assert_eq!(c.state(LocalPos::new(1, 2, 3)), BlockState::plain(BlockId(2)));
    }

    #[test]
    fn version_1_chunks_still_decode_as_plain_blocks() {
        let mut raw = vec![1u8];
        raw.extend_from_slice(&2u16.to_le_bytes());
        for id in [0u16, 7] {
            raw.extend_from_slice(&id.to_le_bytes());
        }
        raw.push(1);
        for i in 0..words_for(1) {
            raw.extend_from_slice(&(if i == 0 { 1u64 } else { 0 }).to_le_bytes());
        }
        let c = Chunk::decode(&lz4_flex::compress_prepend_size(&raw)).unwrap();
        assert_eq!(c.state_index(0), BlockState::plain(BlockId(7)));
        assert_eq!(c.get_index(1), BlockId::AIR);
    }

    #[test]
    fn decode_rejects_index_outside_palette() {
        // Palette of 3 at 2 bits: index 3 is representable but invalid.
        let mut raw = vec![CHUNK_FORMAT_VERSION];
        raw.extend_from_slice(&3u16.to_le_bytes());
        for id in [0u16, 1, 2] {
            raw.extend_from_slice(&id.to_le_bytes());
            raw.push(0);
            raw.extend_from_slice(&0u16.to_le_bytes());
        }
        raw.push(2);
        for _ in 0..words_for(2) {
            raw.extend_from_slice(&u64::MAX.to_le_bytes());
        }
        let bytes = lz4_flex::compress_prepend_size(&raw);
        assert_eq!(Chunk::decode(&bytes), Err(ChunkDecodeError::IndexOutOfPalette));
    }
}
