//! Storage blocks: chests and anything else whose catalogue entry gives it
//! `storage` slots.
//!
//! Like a station, a storage block has data only while something is in it,
//! and that is what stops it being broken. Contents are kept per planet by
//! canonical block position, saved by item name in the same write batch as
//! every chunk and inventory, so moving a stack between a pack and a chest
//! can never be saved half done.
//!
//! Moves follow exactly the rules of `Inventory::move_items` (merge up to the
//! stack size, split onto an empty slot, swap whole stacks of different
//! items), wherever the two slots are: [`move_stack`] is the one rule, and a
//! test holds it to `move_items`.

use crate::store::{SavedSlots, load_slots, save_slots};
use bm_core::{BlockPos, Inventory, ItemRegistry, ItemStack};
use std::collections::{BTreeSet, HashMap};

pub use bm_core::inventory::move_stack;

/// Every storage block of one planet with something in it.
#[derive(Default)]
pub struct Storages {
    map: HashMap<BlockPos, Vec<Option<ItemStack>>>,
    /// Changed since the last save.
    dirty: BTreeSet<BlockPos>,
}

fn is_empty(slots: &[Option<ItemStack>]) -> bool {
    slots.iter().all(Option::is_none)
}

impl Storages {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds from the save. Items no longer in the catalogue, and stacks
    /// no longer possible, are dropped with a warning; a record left empty
    /// is deleted at the next save.
    pub fn load(saved: Vec<(BlockPos, SavedSlots)>, items: &ItemRegistry) -> Self {
        let mut s = Self::new();
        for (pos, slots) in saved {
            let what = format!("storage at {pos:?}");
            let slots: Vec<Option<ItemStack>> = load_slots(items, &slots, &what)
                .into_iter()
                .map(|st| st.filter(|st| st.count > 0 && items.get(st.item).is_some_and(|d| st.count <= d.stack)))
                .collect();
            if is_empty(&slots) {
                s.dirty.insert(pos);
            } else {
                s.map.insert(pos, slots);
            }
        }
        s
    }

    /// Whether anything is in the block at `pos` (so it cannot be broken).
    pub fn has_contents(&self, pos: BlockPos) -> bool {
        self.map.contains_key(&pos)
    }

    /// The slots of a block with `size` slots. Never shorter than `size`;
    /// longer only if the catalogue shrank the block since things were put
    /// in (nothing is ever cut off).
    pub fn slots(&self, pos: BlockPos, size: usize) -> Vec<Option<ItemStack>> {
        let mut v = self.map.get(&pos).cloned().unwrap_or_default();
        if v.len() < size {
            v.resize(size, None);
        }
        v
    }

    /// Replaces a block's slots (kept only while not empty).
    pub fn set(&mut self, pos: BlockPos, slots: Vec<Option<ItemStack>>) {
        if is_empty(&slots) {
            self.map.remove(&pos);
        } else {
            self.map.insert(pos, slots);
        }
        self.dirty.insert(pos);
    }

    /// Removes a block's contents, returning them (its claim lapsed).
    pub fn take(&mut self, pos: BlockPos) -> Vec<ItemStack> {
        match self.map.remove(&pos) {
            Some(slots) => {
                self.dirty.insert(pos);
                slots.into_iter().flatten().collect()
            }
            None => Vec::new(),
        }
    }

    /// Changed blocks for the save; None for one now empty.
    pub fn take_dirty(&mut self, items: &ItemRegistry) -> Vec<(BlockPos, Option<SavedSlots>)> {
        std::mem::take(&mut self.dirty)
            .into_iter()
            .map(|pos| {
                let saved = self
                    .map
                    .get(&pos)
                    .map(|slots| save_slots(items, &Inventory::row(slots.clone())));
                (pos, saved)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SavedStack;
    use bm_core::BlockRegistry;

    fn items() -> ItemRegistry {
        ItemRegistry::builtin(&BlockRegistry::builtin())
    }

    #[test]
    fn moves_merge_split_and_swap() {
        let items = items();
        let stone = items.id("stone").unwrap();
        let sand = items.id("sand").unwrap();
        let hammer = items.id("stone_hammer").unwrap();
        let st = |item, count| Some(ItemStack::new(item, count));

        // Split onto an empty slot; asking for more moves what there is.
        assert_eq!(
            move_stack(&items, st(stone, 10), None, 4),
            Some((st(stone, 6), st(stone, 4)))
        );
        assert_eq!(
            move_stack(&items, st(stone, 10), None, 400),
            Some((None, st(stone, 10)))
        );
        // Merge up to the stack size.
        assert_eq!(
            move_stack(&items, st(stone, 10), st(stone, 95), 10),
            Some((st(stone, 5), st(stone, 100)))
        );
        assert_eq!(move_stack(&items, st(stone, 10), st(stone, 100), 10), None, "full");
        // Different items swap only whole.
        assert_eq!(
            move_stack(&items, st(stone, 10), st(sand, 3), 10),
            Some((st(sand, 3), st(stone, 10)))
        );
        assert_eq!(move_stack(&items, st(stone, 10), st(sand, 3), 4), None);
        // Worn tools never merge.
        let worn = Some(ItemStack {
            wear: 5,
            ..ItemStack::new(hammer, 1)
        });
        assert_eq!(move_stack(&items, worn, st(hammer, 1), 1), Some((st(hammer, 1), worn)));
        // Nothing from nothing, nothing of nothing.
        assert_eq!(move_stack(&items, None, st(stone, 1), 1), None);
        assert_eq!(move_stack(&items, st(stone, 1), None, 0), None);
    }

    /// The one rule agrees with the inventory's own on every pair of slots.
    #[test]
    fn the_rule_is_the_inventorys() {
        let items = items();
        let stone = items.id("stone").unwrap();
        let sand = items.id("sand").unwrap();
        let hammer = items.id("stone_hammer").unwrap();
        let stacks = [
            None,
            Some(ItemStack::new(stone, 1)),
            Some(ItemStack::new(stone, 60)),
            Some(ItemStack::new(stone, 100)),
            Some(ItemStack::new(sand, 7)),
            Some(ItemStack::new(hammer, 1)),
            Some(ItemStack {
                wear: 3,
                ..ItemStack::new(hammer, 1)
            }),
            Some(ItemStack::coloured(stone, bm_core::Colour(40), 30)),
        ];
        for a in stacks {
            for b in stacks {
                for count in [0, 1, 7, 40, 60, 100, 500] {
                    let mut inv = Inventory::row(vec![a, b]);
                    inv.move_items(&items, 0, 1, count);
                    let after = move_stack(&items, a, b, count).unwrap_or((a, b));
                    assert_eq!((inv.slots[0], inv.slots[1]), after, "{a:?} -> {b:?} x{count}");
                }
            }
        }
    }

    #[test]
    fn storage_holds_data_only_while_not_empty_and_saves_by_name() {
        let items = items();
        let stone = items.id("stone").unwrap();
        let pos = BlockPos::new(-3, 40, 9);
        let mut s = Storages::new();
        assert!(!s.has_contents(pos));
        assert_eq!(s.slots(pos, 20), vec![None; 20]);
        let mut slots = s.slots(pos, 20);
        slots[19] = Some(ItemStack::new(stone, 12));
        s.set(pos, slots.clone());
        assert!(s.has_contents(pos));
        let saved = s.take_dirty(&items);
        assert_eq!(saved.len(), 1);
        let record = saved[0].1.clone().unwrap();
        assert_eq!(record[19].as_ref().unwrap().item, "stone");
        assert!(s.take_dirty(&items).is_empty(), "clean after taking");

        let t = Storages::load(vec![(pos, record)], &items);
        assert_eq!(t.slots(pos, 20), slots);
        // A shrunk block keeps what was beyond its new size.
        assert_eq!(t.slots(pos, 5).len(), 20);

        let mut u = Storages::new();
        u.set(pos, slots.clone());
        u.take_dirty(&items);
        assert_eq!(u.take(pos), vec![ItemStack::new(stone, 12)]);
        assert!(!u.has_contents(pos) && u.take(pos).is_empty());
        assert_eq!(u.take_dirty(&items), vec![(pos, None)]);

        s.set(pos, vec![None; 20]);
        assert!(!s.has_contents(pos));
        assert_eq!(s.take_dirty(&items), vec![(pos, None)], "an emptied block is deleted");

        // Impossible stacks from the save are dropped.
        let bad = vec![Some(SavedStack {
            item: "stone".into(),
            count: 5000,
            wear: 0,
        })];
        let mut t = Storages::load(vec![(pos, bad)], &items);
        assert!(!t.has_contents(pos));
        assert_eq!(t.take_dirty(&items), vec![(pos, None)]);
    }
}
