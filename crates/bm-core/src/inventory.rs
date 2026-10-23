//! A player's inventory: fixed slots, the first ten being the hotbar.
//!
//! Pure data and rules, shared by server (the authority) and client (which
//! only displays it and asks for moves). Every operation is all-or-nothing:
//! a craft that cannot take every input takes none.

use crate::block::Colour;
use crate::items::{ItemId, ItemRegistry, ItemStack};
use serde::{Deserialize, Serialize};

pub const INVENTORY_SLOTS: usize = 40;
pub const HOTBAR_SLOTS: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: vec![None; INVENTORY_SLOTS],
        }
    }
}

fn limit(items: &ItemRegistry, item: ItemId) -> u16 {
    items.get(item).map(|d| d.stack).unwrap_or(1)
}

impl Inventory {
    pub fn get(&self, slot: usize) -> Option<ItemStack> {
        self.slots.get(slot).copied().flatten()
    }

    /// Slots changed by an operation, so the server can tell the client.
    fn changed(before: &Inventory, after: &Inventory) -> Vec<usize> {
        (0..after.slots.len())
            .filter(|&i| before.slots.get(i) != after.slots.get(i))
            .collect()
    }

    pub fn count(&self, item: ItemId) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|s| s.item == item)
            .map(|s| s.count as u32)
            .sum()
    }

    /// Whether `count` of `item` (natural colour) would fit.
    pub fn can_fit(&self, items: &ItemRegistry, item: ItemId, count: u16) -> bool {
        self.can_fit_coloured(items, item, Colour::NATURAL, count)
    }

    pub fn can_fit_coloured(&self, items: &ItemRegistry, item: ItemId, colour: Colour, count: u16) -> bool {
        let mut c = self.clone();
        c.add_coloured(items, item, colour, count) == 0
    }

    /// Adds as much as fits, in natural colour. Returns what did not fit.
    pub fn add(&mut self, items: &ItemRegistry, item: ItemId, count: u16) -> u16 {
        self.add_coloured(items, item, Colour::NATURAL, count)
    }

    /// Adds as much as fits: topping up stacks of the same item and colour
    /// first, then empty slots (hotbar first). Returns what did not fit.
    pub fn add_coloured(&mut self, items: &ItemRegistry, item: ItemId, colour: Colour, count: u16) -> u16 {
        let max = limit(items, item);
        let want = ItemStack::coloured(item, colour, 0);
        let mut left = count;
        for s in self.slots.iter_mut().flatten() {
            if left == 0 {
                break;
            }
            if s.stacks_with(&want) && s.count < max {
                let n = left.min(max - s.count);
                s.count += n;
                left -= n;
            }
        }
        for slot in self.slots.iter_mut() {
            if left == 0 {
                break;
            }
            if slot.is_none() {
                let n = left.min(max);
                *slot = Some(ItemStack::coloured(item, colour, n));
                left -= n;
            }
        }
        left
    }

    /// Adds as much of `stack` as fits, keeping everything it carries
    /// (colour, tag, wear, forged traits): onto stacks it stacks with first
    /// (never a worn one), then empty slots. Returns what did not fit.
    pub fn add_stack(&mut self, items: &ItemRegistry, stack: ItemStack) -> u16 {
        let max = limit(items, stack.item);
        let mut left = stack.count;
        for s in self.slots.iter_mut().flatten() {
            if left == 0 {
                break;
            }
            if s.stacks_with(&stack) && s.count < max {
                let n = left.min(max - s.count);
                s.count += n;
                left -= n;
            }
        }
        for slot in self.slots.iter_mut() {
            if left == 0 {
                break;
            }
            if slot.is_none() {
                let n = left.min(max);
                *slot = Some(ItemStack { count: n, ..stack });
                left -= n;
            }
        }
        left
    }

    /// Adds, all or nothing. Returns the changed slots, or None if it does
    /// not fit (and nothing changed).
    pub fn add_all(&mut self, items: &ItemRegistry, item: ItemId, count: u16) -> Option<Vec<usize>> {
        self.add_all_coloured(items, item, Colour::NATURAL, count)
    }

    pub fn add_all_coloured(
        &mut self,
        items: &ItemRegistry,
        item: ItemId,
        colour: Colour,
        count: u16,
    ) -> Option<Vec<usize>> {
        let mut c = self.clone();
        if c.add_coloured(items, item, colour, count) != 0 {
            return None;
        }
        let changed = Self::changed(self, &c);
        *self = c;
        Some(changed)
    }

    /// Removes `count` of `item` from wherever it is (fullest stacks last, so
    /// the hotbar empties last). All or nothing.
    pub fn remove(&mut self, item: ItemId, count: u16) -> Option<Vec<usize>> {
        if self.count(item) < count as u32 {
            return None;
        }
        let before = self.clone();
        let mut left = count;
        for slot in self.slots.iter_mut().rev() {
            if left == 0 {
                break;
            }
            if let Some(s) = slot
                && s.item == item
            {
                let n = left.min(s.count);
                s.count -= n;
                left -= n;
                if s.count == 0 {
                    *slot = None;
                }
            }
        }
        Some(Self::changed(&before, self))
    }

    /// Takes one from a specific slot (placing a block, using an item).
    pub fn take_one(&mut self, slot: usize) -> Option<ItemStack> {
        let s = self.slots.get_mut(slot)?.as_mut()?;
        let taken = ItemStack { count: 1, ..*s };
        s.count -= 1;
        if s.count == 0 {
            self.slots[slot] = None;
        }
        Some(taken)
    }

    /// Moves up to `count` from one slot to another: onto an empty slot,
    /// onto the same item (up to its stack size), or swapping whole stacks of
    /// different items. Returns the changed slots, or None if nothing moved.
    pub fn move_items(&mut self, items: &ItemRegistry, from: usize, to: usize, count: u16) -> Option<Vec<usize>> {
        if from == to || from >= self.slots.len() || to >= self.slots.len() || count == 0 {
            return None;
        }
        let src = self.slots[from]?;
        let n = count.min(src.count);
        let before = self.clone();
        match self.slots[to] {
            None => {
                self.slots[to] = Some(ItemStack { count: n, ..src });
                self.reduce(from, n);
            }
            Some(dst) if dst.stacks_with(&src) => {
                let room = limit(items, src.item).saturating_sub(dst.count);
                let n = n.min(room);
                if n == 0 {
                    return None;
                }
                self.slots[to] = Some(ItemStack {
                    count: dst.count + n,
                    ..dst
                });
                self.reduce(from, n);
            }
            Some(dst) => {
                // Different items: only a whole stack can swap.
                if n != src.count {
                    return None;
                }
                self.slots[to] = Some(src);
                self.slots[from] = Some(dst);
            }
        }
        Some(Self::changed(&before, self))
    }

    fn reduce(&mut self, slot: usize, n: u16) {
        if let Some(s) = &mut self.slots[slot] {
            s.count -= n;
            if s.count == 0 {
                self.slots[slot] = None;
            }
        }
    }

    /// Removes a whole slot (throwing away).
    pub fn clear(&mut self, slot: usize) -> Option<ItemStack> {
        self.slots.get_mut(slot)?.take()
    }

    /// Validates an inventory that came from storage or the network.
    pub fn sanitize(&mut self, items: &ItemRegistry) {
        self.slots.resize(INVENTORY_SLOTS, None);
        for slot in self.slots.iter_mut() {
            if let Some(s) = slot
                && (!items.contains(s.item) || s.count == 0 || s.count > limit(items, s.item))
            {
                *slot = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockRegistry;

    fn items() -> ItemRegistry {
        ItemRegistry::builtin(&BlockRegistry::builtin())
    }

    #[test]
    fn adding_a_stack_keeps_its_traits_wear_and_tag() {
        let i = items();
        let stone = i.id("stone").unwrap();
        let mut inv = Inventory::default();
        let mut forged = ItemStack::new(stone, 1);
        forged.forge.done = true;
        forged.wear = 3;
        assert_eq!(inv.add_stack(&i, forged), 0);
        assert_eq!(inv.get(0), Some(forged));
        let tagged = ItemStack::tagged(stone, 7);
        assert_eq!(inv.add_stack(&i, tagged), 0);
        assert_eq!(inv.get(1), Some(tagged));
        assert_eq!(inv.add_stack(&i, ItemStack::new(stone, 5)), 0);
        assert_eq!(inv.get(2).unwrap().count, 5);
        assert_eq!(inv.add_stack(&i, ItemStack::new(stone, 5)), 0);
        assert_eq!(inv.get(2).unwrap().count, 10);
    }

    #[test]
    fn adding_fills_stacks_then_empty_slots() {
        let i = items();
        let stone = i.id("stone").unwrap();
        let mut inv = Inventory::default();
        assert_eq!(inv.add(&i, stone, 150), 0);
        assert_eq!(inv.get(0).unwrap().count, 100);
        assert_eq!(inv.get(1).unwrap().count, 50);
        assert_eq!(inv.add(&i, stone, 60), 0);
        assert_eq!(inv.get(1).unwrap().count, 100);
        assert_eq!(inv.get(2).unwrap().count, 10);
        assert_eq!(inv.count(stone), 210);
    }

    #[test]
    fn a_full_inventory_refuses_whole_adds_without_changing() {
        let i = items();
        let stone = i.id("stone").unwrap();
        let soil = i.id("soil").unwrap();
        let mut inv = Inventory::default();
        assert_eq!(inv.add(&i, stone, 100 * INVENTORY_SLOTS as u16), 0);
        let before = inv.clone();
        assert!(inv.add_all(&i, soil, 1).is_none());
        assert_eq!(inv, before);
        assert!(!inv.can_fit(&i, soil, 1));
        assert!(inv.can_fit(&i, soil, 0));
    }

    #[test]
    fn removing_is_all_or_nothing_and_spares_the_hotbar() {
        let i = items();
        let stone = i.id("stone").unwrap();
        let mut inv = Inventory::default();
        inv.slots[0] = Some(ItemStack::new(stone, 5));
        inv.slots[20] = Some(ItemStack::new(stone, 5));
        assert!(inv.remove(stone, 11).is_none());
        assert_eq!(inv.count(stone), 10);
        let changed = inv.remove(stone, 6).unwrap();
        assert_eq!(changed, vec![0, 20]);
        assert_eq!(inv.get(20), None);
        assert_eq!(inv.get(0).unwrap().count, 4);
    }

    #[test]
    fn moves_merge_split_and_swap() {
        let i = items();
        let (stone, soil) = (i.id("stone").unwrap(), i.id("soil").unwrap());
        let mut inv = Inventory::default();
        inv.slots[0] = Some(ItemStack::new(stone, 80));
        inv.slots[1] = Some(ItemStack::new(stone, 50));
        inv.slots[2] = Some(ItemStack::new(soil, 3));
        // Merge up to the stack limit.
        inv.move_items(&i, 1, 0, 50).unwrap();
        assert_eq!((inv.get(0).unwrap().count, inv.get(1).unwrap().count), (100, 30));
        // Split onto an empty slot.
        inv.move_items(&i, 1, 9, 10).unwrap();
        assert_eq!((inv.get(1).unwrap().count, inv.get(9).unwrap().count), (20, 10));
        // Swap different items, whole stacks only.
        assert!(inv.move_items(&i, 2, 9, 1).is_none());
        inv.move_items(&i, 2, 9, 3).unwrap();
        assert_eq!(inv.get(9).unwrap().item, soil);
        assert_eq!(inv.get(2).unwrap().item, stone);
        assert!(inv.move_items(&i, 5, 6, 1).is_none(), "empty source");
        assert!(inv.move_items(&i, 0, 99, 1).is_none(), "bad slot");
    }

    #[test]
    fn colours_never_share_a_stack() {
        let i = items();
        let stone = i.id("stone").unwrap();
        let mut inv = Inventory::default();
        assert_eq!(inv.add(&i, stone, 10), 0);
        assert_eq!(inv.add_coloured(&i, stone, Colour(5), 10), 0);
        assert_eq!(inv.get(0).unwrap().colour, Colour::NATURAL);
        assert_eq!(inv.get(1).unwrap().colour, Colour(5));
        assert_eq!(inv.count(stone), 20, "counting takes any colour");
        assert!(
            inv.move_items(&i, 1, 0, 5).is_none(),
            "no merge across colours (part of a stack)"
        );
        inv.move_items(&i, 1, 0, 10).unwrap();
        assert_eq!(inv.get(0).unwrap().colour, Colour(5), "whole stacks swap");
        assert_eq!(inv.add_coloured(&i, stone, Colour(5), 3), 0);
        assert_eq!(inv.get(0).unwrap().count, 13, "same colour tops up");
    }

    #[test]
    fn tools_never_stack() {
        let i = items();
        let hammer = i.id("stone_hammer").unwrap();
        let mut inv = Inventory::default();
        assert_eq!(inv.add(&i, hammer, 2), 0);
        assert_eq!(inv.get(0).unwrap().count, 1);
        assert_eq!(inv.get(1).unwrap().count, 1);
    }

    #[test]
    fn sanitize_drops_impossible_stacks() {
        let i = items();
        let mut inv = Inventory { slots: vec![None; 3] };
        inv.slots[0] = Some(ItemStack::new(ItemId(60000), 1));
        inv.slots[1] = Some(ItemStack::new(i.id("stone").unwrap(), 500));
        inv.slots[2] = Some(ItemStack::new(i.id("stone").unwrap(), 5));
        inv.sanitize(&i);
        assert_eq!(inv.slots.len(), INVENTORY_SLOTS);
        assert_eq!(inv.get(0), None);
        assert_eq!(inv.get(1), None);
        assert_eq!(inv.get(2).unwrap().count, 5);
    }
}
