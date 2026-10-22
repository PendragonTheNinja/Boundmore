//! One planet's growing crops (milestone 16 part 2, ADR-0010): a record per
//! crop, grouped by chunk, a queue of when each crop in a loaded chunk next
//! changes stage, and what to save.
//!
//! The records are the truth about growth; the blocks are what players see.
//! `Game` keeps the two together: it swaps a crop's block when the queue
//! says its stage changed (in loaded chunks) and when a chunk loads (the
//! crops in it catch up), and drops a record whose block went.

use std::collections::{BTreeSet, HashMap, HashSet};

use bm_core::crops::{Crop, CropKinds};
use bm_core::{BlockPos, ChunkPos};

use crate::store::SavedCrop;

#[derive(Default)]
pub struct Farm {
    by_chunk: HashMap<ChunkPos, HashMap<BlockPos, Crop>>,
    /// (tick, crop): when a crop in a loaded chunk next changes stage. An
    /// entry may be stale (the crop replanted or gone); it is checked when
    /// it comes due.
    due: BTreeSet<(u64, BlockPos)>,
    dirty: HashSet<BlockPos>,
}

impl Farm {
    /// The saved crops of a planet; one of a kind no longer in the catalogue
    /// is dropped (its block reads as wild).
    pub fn load(saved: Vec<(BlockPos, SavedCrop)>, kinds: &CropKinds) -> Self {
        let mut farm = Self::default();
        for (pos, s) in saved {
            let Some(kind) = kinds.by_name(&s.crop) else {
                tracing::warn!("a crop of kind `{}` at {pos:?} is no longer known; dropped", s.crop);
                farm.dirty.insert(pos);
                continue;
            };
            farm.by_chunk.entry(pos.chunk()).or_default().insert(
                pos,
                Crop {
                    kind,
                    sown_at: s.sown_at,
                    fertilised_at: s.fertilised_at,
                    tilled: s.tilled,
                    wet: s.wet,
                    epic: s.epic,
                },
            );
        }
        farm
    }

    pub fn get(&self, pos: BlockPos) -> Option<Crop> {
        self.by_chunk.get(&pos.chunk())?.get(&pos).copied()
    }

    pub fn len(&self) -> usize {
        self.by_chunk.values().map(HashMap::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Adds or replaces the record at `pos` and queues its next change.
    pub fn put(&mut self, pos: BlockPos, crop: Crop, now: u64, kinds: &CropKinds) {
        self.by_chunk.entry(pos.chunk()).or_default().insert(pos, crop);
        self.dirty.insert(pos);
        self.schedule(pos, crop, now, kinds);
    }

    pub fn remove(&mut self, pos: BlockPos) -> Option<Crop> {
        let chunk = self.by_chunk.get_mut(&pos.chunk())?;
        let c = chunk.remove(&pos)?;
        if chunk.is_empty() {
            self.by_chunk.remove(&pos.chunk());
        }
        self.dirty.insert(pos);
        Some(c)
    }

    /// Queues the crop's next stage change, if it has one.
    pub fn schedule(&mut self, pos: BlockPos, crop: Crop, now: u64, kinds: &CropKinds) {
        if let Some(at) = kinds.get(crop.kind).and_then(|d| crop.next_change(d, now)) {
            self.due.insert((at, pos));
        }
    }

    /// The crops in `chunk`, for catching up as it loads.
    pub fn in_chunk(&self, chunk: ChunkPos) -> Vec<(BlockPos, Crop)> {
        self.by_chunk
            .get(&chunk)
            .map(|m| m.iter().map(|(p, c)| (*p, *c)).collect())
            .unwrap_or_default()
    }

    /// Crops whose queued change has come by `now` (each once; the caller
    /// checks the record and the block, and queues the next change).
    pub fn take_due(&mut self, now: u64) -> Vec<BlockPos> {
        let mut out = Vec::new();
        while let Some(&(at, pos)) = self.due.first() {
            if at > now {
                break;
            }
            self.due.pop_first();
            out.push(pos);
        }
        out
    }

    /// Records changed since the last save, as saved (None: gone).
    pub fn take_dirty(&mut self, kinds: &CropKinds) -> Vec<(BlockPos, Option<SavedCrop>)> {
        let dirty: Vec<BlockPos> = self.dirty.drain().collect();
        dirty
            .into_iter()
            .map(|pos| {
                let saved = self.get(pos).and_then(|c| {
                    Some(SavedCrop {
                        crop: kinds.get(c.kind)?.name.clone(),
                        sown_at: c.sown_at,
                        fertilised_at: c.fertilised_at,
                        tilled: c.tilled,
                        wet: c.wet,
                        epic: c.epic,
                    })
                });
                (pos, saved)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{BlockRegistry, ItemRegistry};

    #[test]
    fn records_save_load_and_come_due_in_order() {
        let blocks = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let kinds = CropKinds::builtin(&blocks, &items);
        let tuber = kinds.by_name("tuber").unwrap();
        let hour = 3600 * bm_core::TICK_HZ as u64;
        let crop = |sown_at| Crop {
            kind: tuber,
            sown_at,
            fertilised_at: None,
            tilled: false,
            wet: false,
            epic: false,
        };
        let mut farm = Farm::default();
        let (a, b) = (BlockPos::new(1, 40, 1), BlockPos::new(40, 40, 2));
        farm.put(a, crop(0), 0, &kinds);
        farm.put(b, crop(hour / 2), hour / 2, &kinds);
        assert_eq!(farm.len(), 2);
        assert!(farm.take_due(hour - 1).is_empty());
        assert_eq!(farm.take_due(hour), vec![a]);
        assert_eq!(farm.take_due(2 * hour), vec![b]);
        assert_eq!(farm.in_chunk(a.chunk()), vec![(a, crop(0))]);

        let saved: Vec<_> = farm
            .take_dirty(&kinds)
            .into_iter()
            .filter_map(|(p, s)| Some((p, s?)))
            .collect();
        assert_eq!(saved.len(), 2);
        let mut back = Farm::load(saved, &kinds);
        assert_eq!(back.get(a), Some(crop(0)));
        assert_eq!(back.remove(a), Some(crop(0)));
        assert_eq!(back.take_dirty(&kinds), vec![(a, None)]);
        assert!(back.get(a).is_none());
    }
}
