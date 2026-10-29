//! Blocks players placed, per planet (Boundless's "dug up" mark): breaking
//! one gives back what it drops, but no XP, no bonus yield and no mining
//! feats, so placing and breaking the same block over and over earns
//! nothing (audit 2026-10-29; a macro on one block used to farm XP, coin,
//! endeavour and rare bonus drops).
//!
//! Server only: clients never need it. Kept per chunk as local indices,
//! saved in its own table (`planet_placed`); blocks placed before it existed
//! count as natural.

use std::collections::{HashMap, HashSet};

use bm_core::{BlockPos, ChunkPos, LocalPos};

#[derive(Default)]
pub struct Placed {
    by_chunk: HashMap<ChunkPos, HashSet<u16>>,
    dirty: HashSet<ChunkPos>,
}

impl Placed {
    pub fn load(saved: Vec<(ChunkPos, Vec<u16>)>) -> Self {
        let mut p = Self::default();
        for (c, cells) in saved {
            p.by_chunk.insert(c, cells.into_iter().collect());
        }
        p
    }

    fn cell(pos: BlockPos) -> u16 {
        pos.local().index() as u16
    }

    /// Marks `pos` (canonical) as placed by a player.
    pub fn mark(&mut self, pos: BlockPos) {
        if self.by_chunk.entry(pos.chunk()).or_default().insert(Self::cell(pos)) {
            self.dirty.insert(pos.chunk());
        }
    }

    /// Clears the mark at `pos`; whether it was placed.
    pub fn clear(&mut self, pos: BlockPos) -> bool {
        let Some(cells) = self.by_chunk.get_mut(&pos.chunk()) else {
            return false;
        };
        let was = cells.remove(&Self::cell(pos));
        if was {
            if cells.is_empty() {
                self.by_chunk.remove(&pos.chunk());
            }
            self.dirty.insert(pos.chunk());
        }
        was
    }

    pub fn contains(&self, pos: BlockPos) -> bool {
        self.by_chunk
            .get(&pos.chunk())
            .is_some_and(|c| c.contains(&Self::cell(pos)))
    }

    /// What changed since the last save: each chunk's marks, or none left.
    pub fn take_dirty(&mut self) -> Vec<(ChunkPos, Option<Vec<u16>>)> {
        let dirty = std::mem::take(&mut self.dirty);
        dirty
            .into_iter()
            .map(|c| {
                let cells = self.by_chunk.get(&c).map(|s| {
                    let mut v: Vec<u16> = s.iter().copied().collect();
                    v.sort_unstable();
                    v
                });
                (c, cells)
            })
            .collect()
    }
}

/// A chunk-local index back to its position (for tests and tools).
pub fn at(chunk: ChunkPos, cell: u16) -> BlockPos {
    chunk.block(LocalPos::from_index(cell as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_kept_cleared_and_saved_by_chunk() {
        let mut p = Placed::default();
        let a = BlockPos::new(-1, 40, 33);
        let b = BlockPos::new(5, 40, 33);
        p.mark(a);
        p.mark(b);
        assert!(p.contains(a) && p.contains(b));
        assert!(!p.contains(BlockPos::new(6, 40, 33)));
        let saved = p.take_dirty();
        assert_eq!(saved.len(), 2, "two chunks");
        assert!(p.take_dirty().is_empty(), "saved");
        assert!(p.clear(a));
        assert!(!p.clear(a), "only once");
        assert_eq!(p.take_dirty(), vec![(a.chunk(), None)], "the chunk has none left");
        let again = Placed::load(saved.into_iter().map(|(c, v)| (c, v.unwrap())).collect());
        assert!(again.contains(a) && again.contains(b));
        assert_eq!(at(b.chunk(), b.local().index() as u16), b);
    }
}
