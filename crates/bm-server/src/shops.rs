//! Shop stands' and request baskets' prices and tills, per planet
//! (milestone 24 part 2). Their items are kept as storage (`storage.rs`).

use bm_core::BlockPos;
use bm_core::trade::ShopRecord;
use std::collections::{BTreeSet, HashMap};

#[derive(Default)]
pub struct Shops {
    map: HashMap<BlockPos, ShopRecord>,
    /// Changed since the last save.
    dirty: BTreeSet<BlockPos>,
}

impl Shops {
    pub fn load(saved: Vec<(BlockPos, ShopRecord)>) -> Self {
        Shops {
            map: saved.into_iter().filter(|(_, r)| !r.is_empty()).collect(),
            dirty: BTreeSet::new(),
        }
    }

    pub fn get(&self, pos: BlockPos) -> ShopRecord {
        self.map.get(&pos).cloned().unwrap_or_default()
    }

    /// Replaces a stand's record (kept only while it holds something).
    pub fn set(&mut self, pos: BlockPos, r: ShopRecord) {
        if r.is_empty() {
            self.map.remove(&pos);
        } else {
            self.map.insert(pos, r);
        }
        self.dirty.insert(pos);
    }

    /// Coin in the till (so it cannot be broken while there is some).
    pub fn till(&self, pos: BlockPos) -> u64 {
        self.map.get(&pos).map_or(0, |r| r.till)
    }

    /// Removes a stand's record, returning its till (its claim lapsed, or
    /// the empty block was broken).
    pub fn take(&mut self, pos: BlockPos) -> u64 {
        match self.map.remove(&pos) {
            Some(r) => {
                self.dirty.insert(pos);
                r.till
            }
            None => 0,
        }
    }

    /// Changed records for the save; None for one now gone.
    pub fn take_dirty(&mut self) -> Vec<(BlockPos, Option<ShopRecord>)> {
        std::mem::take(&mut self.dirty)
            .into_iter()
            .map(|pos| (pos, self.map.get(&pos).cloned()))
            .collect()
    }
}
