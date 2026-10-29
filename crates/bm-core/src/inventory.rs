//! A player's inventory: fixed slots, the first ten being the hotbar.
//!
//! Pure data and rules, shared by server (the authority) and client (which
//! only displays it and asks for moves). Every operation is all-or-nothing:
//! a craft that cannot take every input takes none.
//!
//! Smart stacks (milestone 33, Boundless's): a player's slot is a *group* of
//! [`SMART`] sub-slots, each holding one stack, and a group holds one item
//! family (an item in any colour, or a wider family named in `items.toml`).
//! `slots` stays flat (group `g` is `slots[g * group..(g + 1) * group]`), so
//! counting, crafting and removing work over it unchanged. Storage rows and
//! station outputs are groups of one ([`Inventory::row`]).

use crate::block::Colour;
use crate::items::{ItemId, ItemRegistry, ItemStack};
use serde::{Deserialize, Serialize};

/// Visible slots in a player's inventory, and those of the hotbar.
pub const INVENTORY_SLOTS: usize = 40;
pub const HOTBAR_SLOTS: usize = 10;
/// Stacks in one smart stack (Boundless's 9).
pub const SMART: usize = 9;
/// Stored slots in a player's inventory.
pub const PACK_SLOTS: usize = INVENTORY_SLOTS * SMART;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
    /// Sub-slots per visible slot: [`SMART`] for a player, 1 for a row.
    pub group: usize,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: vec![None; PACK_SLOTS],
            group: SMART,
        }
    }
}

fn limit(items: &ItemRegistry, item: ItemId) -> u16 {
    items.get(item).map(|d| d.stack).unwrap_or(1)
}

/// Whether two items share a smart stack: the same item (any colour, wear
/// or forging), or the same named family (`smart` in `items.toml`).
pub fn same_family(items: &ItemRegistry, a: ItemId, b: ItemId) -> bool {
    if a == b {
        return true;
    }
    match (
        items.get(a).and_then(|d| d.smart.as_ref()),
        items.get(b).and_then(|d| d.smart.as_ref()),
    ) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Whether a stored index is in the hotbar of a player's inventory.
pub fn in_hotbar(slot: usize) -> bool {
    slot < HOTBAR_SLOTS * SMART
}

impl Inventory {
    /// A plain row of slots (a storage block, a station's output).
    pub fn row(slots: Vec<Option<ItemStack>>) -> Self {
        Self { slots, group: 1 }
    }

    /// A player's inventory from slots saved before smart stacks (one stack
    /// a slot): slot `i` becomes group `i`'s first stack.
    pub fn from_plain(old: &[Option<ItemStack>]) -> Self {
        let mut inv = Self::default();
        for (i, s) in old.iter().enumerate().take(INVENTORY_SLOTS) {
            inv.slots[i * SMART] = *s;
        }
        inv
    }

    pub fn get(&self, slot: usize) -> Option<ItemStack> {
        self.slots.get(slot).copied().flatten()
    }

    /// How many visible slots (groups).
    pub fn groups(&self) -> usize {
        self.slots.len() / self.group.max(1)
    }

    /// The stored indices of group `g`.
    pub fn group_range(&self, g: usize) -> std::ops::Range<usize> {
        let n = self.group.max(1);
        (g * n).min(self.slots.len())..((g + 1) * n).min(self.slots.len())
    }

    /// The group a stored index is in.
    pub fn group_of(&self, slot: usize) -> usize {
        slot / self.group.max(1)
    }

    /// Group `g`'s active stack: its first filled sub-slot, the one a hand
    /// holding the group uses. None for an empty group.
    pub fn active(&self, g: usize) -> Option<usize> {
        self.group_range(g).find(|&i| self.slots[i].is_some())
    }

    /// What a hand holding group `g` holds.
    pub fn held(&self, g: usize) -> Option<ItemStack> {
        self.active(g).and_then(|i| self.get(i))
    }

    /// The stacks in group `g`, in order.
    pub fn group_stacks(&self, g: usize) -> Vec<ItemStack> {
        self.group_range(g).filter_map(|i| self.get(i)).collect()
    }

    /// Everything in group `g`, counted together.
    pub fn group_count(&self, g: usize) -> u32 {
        self.group_stacks(g).iter().map(|s| s.count as u32).sum()
    }

    /// Whether `item` may go into group `g`: every stack there (but the one
    /// at `except`, which is leaving) is of its family.
    pub fn accepts(&self, items: &ItemRegistry, g: usize, item: ItemId, except: Option<usize>) -> bool {
        self.group_range(g)
            .filter(|&i| Some(i) != except)
            .filter_map(|i| self.get(i))
            .all(|s| same_family(items, s.item, item))
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

    /// Adds as much as fits (see [`Self::add_stack`]). Returns what did not
    /// fit.
    pub fn add_coloured(&mut self, items: &ItemRegistry, item: ItemId, colour: Colour, count: u16) -> u16 {
        self.add_stack(items, ItemStack::coloured(item, colour, count))
    }

    /// Adds as much of `stack` as fits, keeping everything it carries
    /// (colour, tag, wear, forged traits): onto stacks it stacks with first
    /// (never a worn one), then empty sub-slots of smart stacks of its
    /// family, then empty slots (hotbar first). Returns what did not fit.
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
        // Empty sub-slots: in groups already of this family, then in empty
        // groups (a group of one is never both filled and empty).
        for wanted_filled in [true, false] {
            for g in 0..self.groups() {
                if left == 0 {
                    return 0;
                }
                let filled = self.active(g).is_some();
                if filled != wanted_filled || (filled && !self.accepts(items, g, stack.item, None)) {
                    continue;
                }
                for i in self.group_range(g) {
                    if left == 0 {
                        break;
                    }
                    if self.slots[i].is_none() {
                        let n = left.min(max);
                        self.slots[i] = Some(ItemStack { count: n, ..stack });
                        left -= n;
                    }
                }
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

    /// How many of `item` in `colour` (milestone 22: a tinted recipe's
    /// input counts only its chosen colour).
    pub fn count_coloured(&self, item: ItemId, colour: Colour) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|s| s.item == item && s.colour == colour)
            .map(|s| s.count as u32)
            .sum()
    }

    /// The colours `item` is carried in, with how many of each.
    pub fn colours_of(&self, item: ItemId) -> Vec<(Colour, u32)> {
        let mut out: Vec<(Colour, u32)> = Vec::new();
        for s in self.slots.iter().flatten().filter(|s| s.item == item) {
            match out.iter_mut().find(|(c, _)| *c == s.colour) {
                Some((_, n)) => *n += s.count as u32,
                None => out.push((s.colour, s.count as u32)),
            }
        }
        out
    }

    /// Removes `count` of `item` in `colour` only, as [`Self::remove`] does.
    pub fn remove_coloured(&mut self, item: ItemId, colour: Colour, count: u16) -> Option<Vec<usize>> {
        if self.count_coloured(item, colour) < count as u32 {
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
                && s.colour == colour
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
    /// different items. A smart stack only ever takes its own family.
    /// Returns the changed slots, or None if nothing moved.
    pub fn move_items(&mut self, items: &ItemRegistry, from: usize, to: usize, count: u16) -> Option<Vec<usize>> {
        if from == to || from >= self.slots.len() || to >= self.slots.len() || count == 0 {
            return None;
        }
        let src = self.slots[from]?;
        let n = count.min(src.count);
        let before = self.clone();
        let (gf, gt) = (self.group_of(from), self.group_of(to));
        match self.slots[to] {
            None => {
                if !self.accepts(items, gt, src.item, (n == src.count).then_some(from)) {
                    return None;
                }
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
                // Different items: only a whole stack can swap, and each
                // group must take the other's.
                if n != src.count
                    || !self.accepts(items, gt, src.item, Some(to))
                    || !self.accepts(items, gf, dst.item, Some(from))
                {
                    return None;
                }
                self.slots[to] = Some(src);
                self.slots[from] = Some(dst);
            }
        }
        Some(Self::changed(&before, self))
    }

    /// Where a drop of `count` from stored slot `from` onto group `g` lands:
    /// a stack it tops up, else an empty sub-slot if the group takes its
    /// family, else (a whole stack onto a different group of one) the slot
    /// itself, to swap. None when it cannot go there.
    pub fn drop_target(&self, items: &ItemRegistry, from: usize, g: usize, count: u16) -> Option<usize> {
        let src = self.get(from)?;
        let max = limit(items, src.item);
        let range = self.group_range(g);
        if range.contains(&from) {
            return None;
        }
        if let Some(i) = range
            .clone()
            .find(|&i| self.get(i).is_some_and(|d| d.stacks_with(&src) && d.count < max))
        {
            return Some(i);
        }
        if self.accepts(items, g, src.item, None)
            && let Some(i) = range.clone().find(|&i| self.slots[i].is_none())
        {
            return Some(i);
        }
        // A whole stack onto another slot of one: swap.
        (range.len() == 1 && count >= src.count).then_some(range.start)
    }

    /// Moves group `from` onto group `to` whole: into an empty slot, merged
    /// into a smart stack of the same family (what fits), or swapped with a
    /// different one. Returns the changed slots, or None if nothing moved.
    pub fn move_group(&mut self, items: &ItemRegistry, from: usize, to: usize) -> Option<Vec<usize>> {
        if from == to || from >= self.groups() || to >= self.groups() || self.active(from).is_none() {
            return None;
        }
        let before = self.clone();
        let family = self.held(from)?.item;
        let same = self.active(to).is_some() && self.accepts(items, to, family, None);
        if self.active(to).is_none() || same {
            let mut dest = Inventory::row(self.group_range(to).map(|i| self.slots[i]).collect());
            for i in self.group_range(from) {
                let Some(s) = self.slots[i] else { continue };
                let left = dest.add_stack(items, s);
                self.slots[i] = (left > 0).then_some(ItemStack { count: left, ..s });
            }
            for (k, i) in self.group_range(to).enumerate() {
                self.slots[i] = dest.slots[k];
            }
        } else {
            for (a, b) in self.group_range(from).zip(self.group_range(to)) {
                self.slots.swap(a, b);
            }
        }
        let changed = Self::changed(&before, self);
        (!changed.is_empty()).then_some(changed)
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

    /// Sorts the groups from `first` on (the pack beyond the hotbar):
    /// stacks merged, families gathered into as few smart stacks as they
    /// fill, ordered by item and colour, empty slots last. Nothing is lost.
    pub fn sort_from(&mut self, items: &ItemRegistry, first: usize) {
        let start = (first * self.group).min(self.slots.len());
        if self.group <= 1 {
            return sort_slots(items, &mut self.slots[start..]);
        }
        let mut held: Vec<ItemStack> = self.slots[start..].iter_mut().filter_map(Option::take).collect();
        held.sort_by_key(|s| (family_key(items, s.item), s.item, s.colour.0, s.tag, s.wear));
        let row = merge_sorted(items, held);
        // Lay the merged stacks out, a new group whenever the family changes
        // or the group is full.
        let mut g = first;
        let mut k = 0;
        let mut last: Option<ItemId> = None;
        for s in row {
            if last.is_some_and(|l| !same_family(items, l, s.item)) || k == self.group {
                if k > 0 {
                    g += 1;
                }
                k = 0;
            }
            let Some(i) = self.group_range(g).nth(k) else { break };
            self.slots[i] = Some(s);
            k += 1;
            last = Some(s.item);
        }
    }

    /// Validates an inventory that came from storage or the network: a
    /// player's has [`PACK_SLOTS`]; impossible stacks go, and a stack in a
    /// smart stack of another family moves to where it fits (or goes).
    pub fn sanitize(&mut self, items: &ItemRegistry) {
        if self.group == SMART {
            self.slots.resize(PACK_SLOTS, None);
        }
        for slot in self.slots.iter_mut() {
            if let Some(s) = slot
                && (!items.contains(s.item) || s.count == 0 || s.count > limit(items, s.item))
            {
                *slot = None;
            }
        }
        let mut strays = Vec::new();
        for g in 0..self.groups() {
            let Some(first) = self.held(g) else { continue };
            for i in self.group_range(g) {
                if let Some(s) = self.slots[i]
                    && !same_family(items, s.item, first.item)
                {
                    strays.push(s);
                    self.slots[i] = None;
                }
            }
        }
        for s in strays {
            self.add_stack(items, s);
        }
    }
}

/// Merges neighbouring stacks that stack together (up to full stacks),
/// keeping the order.
fn merge_sorted(items: &ItemRegistry, held: Vec<ItemStack>) -> Vec<ItemStack> {
    let mut merged: Vec<ItemStack> = Vec::new();
    for s in held {
        let max = limit(items, s.item);
        let mut left = s.count;
        if let Some(last) = merged.last_mut().filter(|l| l.stacks_with(&s) && l.count < max) {
            let n = left.min(max - last.count);
            last.count += n;
            left -= n;
        }
        if left > 0 {
            merged.push(ItemStack { count: left, ..s });
        }
    }
    merged
}

/// Orders families together (an item's family name, else the item).
fn family_key(items: &ItemRegistry, item: ItemId) -> (String, ItemId) {
    match items.get(item).and_then(|d| d.smart.clone()) {
        Some(f) => (f, ItemId(0)),
        None => (String::new(), item),
    }
}

/// What a move leaves in its two slots, or None if nothing would move.
/// `src` and `dst` are different slots (in one container or two).
pub fn move_stack(
    items: &ItemRegistry,
    src: Option<ItemStack>,
    dst: Option<ItemStack>,
    count: u16,
) -> Option<(Option<ItemStack>, Option<ItemStack>)> {
    let src = src?;
    if count == 0 {
        return None;
    }
    let n = count.min(src.count);
    let less = |n: u16| {
        (src.count > n).then_some(ItemStack {
            count: src.count - n,
            ..src
        })
    };
    match dst {
        None => Some((less(n), Some(ItemStack { count: n, ..src }))),
        Some(d) if d.stacks_with(&src) => {
            let max = items.get(src.item).map_or(1, |def| def.stack);
            let n = n.min(max.saturating_sub(d.count));
            if n == 0 {
                return None;
            }
            Some((
                less(n),
                Some(ItemStack {
                    count: d.count + n,
                    ..d
                }),
            ))
        }
        // Different items (or a worn tool): only a whole stack swaps.
        Some(d) => (n == src.count).then_some((Some(d), Some(src))),
    }
}

/// One side of a move between a player's pack and a row (a storage block).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Pack(usize),
    Row(usize),
}

/// Moves between two slots of a pack and a row (either may be on either
/// side), with the rules of [`move_stack`]; a pack slot only ever holds its
/// smart stack's family. Every index is checked; all or nothing. Returns
/// whether anything changed. The server rules with it and the client
/// predicts with it.
pub fn move_across(
    items: &ItemRegistry,
    pack: &mut Inventory,
    row: &mut [Option<ItemStack>],
    from: Side,
    to: Side,
    count: u16,
) -> bool {
    if from == to {
        return false;
    }
    let get = |pack: &Inventory, row: &[Option<ItemStack>], s: Side| match s {
        Side::Pack(i) => pack.slots.get(i).copied(),
        Side::Row(i) => row.get(i).copied(),
    };
    let (Some(src), Some(dst)) = (get(pack, row, from), get(pack, row, to)) else {
        return false;
    };
    let Some((a, b)) = move_stack(items, src, dst, count) else {
        return false;
    };
    if (a, b) == (src, dst) {
        return false;
    }
    let fits = |pack: &Inventory, s: Side, v: Option<ItemStack>| match (s, v) {
        (Side::Pack(i), Some(st)) => pack.accepts(items, pack.group_of(i), st.item, Some(i)),
        _ => true,
    };
    // A swap inside one smart stack is checked as it will stand.
    let mut trial = pack.clone();
    for (s, v) in [(from, a), (to, b)] {
        if let Side::Pack(i) = s {
            trial.slots[i] = v;
        }
    }
    if !fits(&trial, from, a) || !fits(&trial, to, b) {
        return false;
    }
    for (s, v) in [(from, a), (to, b)] {
        match s {
            Side::Pack(i) => pack.slots[i] = v,
            Side::Row(i) => row[i] = v,
        }
    }
    true
}

/// Sorts a row of slots in place (a storage block, or the pack beyond the
/// hotbar): stacks that stack together are merged (up to a full stack),
/// then ordered by item and colour, empty slots last. Nothing is lost: the
/// row holds the same items after.
pub fn sort_slots(items: &ItemRegistry, slots: &mut [Option<ItemStack>]) {
    let mut held: Vec<ItemStack> = slots.iter_mut().filter_map(Option::take).collect();
    held.sort_by_key(|s| (s.item, s.colour.0, s.tag, s.wear));
    let merged = merge_sorted(items, held);
    for (slot, s) in slots
        .iter_mut()
        .zip(merged.into_iter().map(Some).chain(std::iter::repeat(None)))
    {
        *slot = s;
    }
}

/// Moves every stack in `from` whose kind `into` already holds into `into`
/// (onto those stacks first, then empty slots): Boundless's "deposit
/// matching". Returns whether anything moved.
pub fn deposit_matching(items: &ItemRegistry, from: &mut [Option<ItemStack>], into: &mut [Option<ItemStack>]) -> bool {
    let mut moved = false;
    for slot in from.iter_mut() {
        let Some(stack) = *slot else { continue };
        if !into.iter().flatten().any(|s| s.stacks_with(&stack)) {
            continue;
        }
        let mut row = Inventory::row(into.to_vec());
        let left = row.add_stack(items, stack);
        if left < stack.count {
            into.copy_from_slice(&row.slots);
            *slot = (left > 0).then_some(ItemStack { count: left, ..stack });
            moved = true;
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockRegistry;

    fn items() -> ItemRegistry {
        ItemRegistry::builtin(&BlockRegistry::builtin())
    }

    #[test]
    fn sorting_merges_orders_and_keeps_everything() {
        let i = items();
        let (stone, trunk) = (i.id("stone").unwrap(), i.id("trunk").unwrap());
        let mut row = vec![
            None,
            Some(ItemStack::new(trunk, 3)),
            Some(ItemStack::new(stone, 60)),
            None,
            Some(ItemStack::new(stone, 70)),
        ];
        sort_slots(&i, &mut row);
        let max = limit(&i, stone);
        let stone_first = stone < trunk;
        let stones: Vec<u16> = row
            .iter()
            .flatten()
            .filter(|s| s.item == stone)
            .map(|s| s.count)
            .collect();
        assert_eq!(stones.iter().sum::<u16>(), 130, "nothing lost");
        assert_eq!(stones[0], max.min(130), "merged into full stacks first");
        assert_eq!(
            row.iter()
                .flatten()
                .filter(|s| s.item == trunk)
                .map(|s| s.count)
                .sum::<u16>(),
            3
        );
        assert_eq!(row[0].unwrap().item, if stone_first { stone } else { trunk });
        assert!(
            row.iter().rev().take_while(|s| s.is_none()).count() >= 2,
            "empties last"
        );
    }

    #[test]
    fn deposit_matching_moves_only_what_the_box_already_holds() {
        let i = items();
        let (stone, trunk) = (i.id("stone").unwrap(), i.id("trunk").unwrap());
        let mut pack = vec![Some(ItemStack::new(stone, 20)), Some(ItemStack::new(trunk, 4))];
        let mut chest = vec![Some(ItemStack::new(stone, 5)), None];
        assert!(deposit_matching(&i, &mut pack, &mut chest));
        assert_eq!(pack, vec![None, Some(ItemStack::new(trunk, 4))]);
        assert_eq!(chest[0], Some(ItemStack::new(stone, 25)));
        assert!(!deposit_matching(&i, &mut pack, &mut chest), "nothing else matches");
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
    fn removing_is_all_or_nothing_and_spares_the_hotbar() {
        let i = items();
        let stone = i.id("stone").unwrap();
        let mut inv = Inventory::default();
        inv.slots[0] = Some(ItemStack::new(stone, 5));
        inv.slots[200] = Some(ItemStack::new(stone, 5));
        assert!(inv.remove(stone, 11).is_none());
        assert_eq!(inv.count(stone), 10);
        let changed = inv.remove(stone, 6).unwrap();
        assert_eq!(changed, vec![0, 200]);
        assert_eq!(inv.get(200), None);
        assert_eq!(inv.get(0).unwrap().count, 4);
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
    fn a_full_inventory_refuses_whole_adds_without_changing() {
        let i = items();
        let stone = i.id("stone").unwrap();
        let soil = i.id("soil").unwrap();
        let mut inv = Inventory::default();
        // Every slot a smart stack of stone in nine colours.
        for c in 0..PACK_SLOTS {
            inv.slots[c] = Some(ItemStack::coloured(stone, Colour((c % SMART) as u8 + 1), 100));
        }
        let before = inv.clone();
        assert!(inv.add_all(&i, soil, 1).is_none());
        assert_eq!(inv, before);
        assert!(!inv.can_fit(&i, soil, 1));
        assert!(inv.can_fit(&i, soil, 0));
    }

    #[test]
    fn a_smart_stack_holds_nine_stacks_of_one_family() {
        let i = items();
        let (stone, soil) = (i.id("stone").unwrap(), i.id("soil").unwrap());
        let hammer = i.id("stone_hammer").unwrap();
        let mut inv = Inventory::default();
        for c in 1..=9 {
            assert_eq!(inv.add_coloured(&i, stone, Colour(c), 100), 0);
        }
        assert_eq!(inv.group_stacks(0).len(), 9, "nine colours in the first slot");
        assert_eq!(inv.group_count(0), 900);
        assert_eq!(inv.add_coloured(&i, stone, Colour(10), 5), 0);
        assert_eq!(inv.held(1).unwrap().colour, Colour(10), "a tenth goes to the next slot");
        assert_eq!(inv.add(&i, soil, 3), 0);
        assert_eq!(inv.held(2).unwrap().item, soil, "another item, another slot");
        assert_eq!(inv.add(&i, stone, 1), 0);
        assert_eq!(
            inv.group_stacks(1).len(),
            2,
            "natural stone joins the stone slot with room"
        );
        // Tools: nine of a kind to a slot.
        assert_eq!(inv.add(&i, hammer, 10), 0);
        assert_eq!(inv.group_stacks(3).len(), 9);
        assert_eq!(inv.held(4).unwrap().item, hammer);
        // The hand uses the first stack; taking it all shows the next.
        assert_eq!(inv.held(0).unwrap().colour, Colour(1));
        inv.slots[0] = None;
        assert_eq!(inv.held(0).unwrap().colour, Colour(2));
        assert_eq!(inv.active(0), Some(1));
        assert!(in_hotbar(89) && !in_hotbar(90));
    }

    #[test]
    fn moves_merge_split_swap_and_keep_families_apart() {
        let i = items();
        let (stone, soil) = (i.id("stone").unwrap(), i.id("soil").unwrap());
        let mut inv = Inventory::default();
        inv.slots[0] = Some(ItemStack::new(stone, 80));
        inv.slots[9] = Some(ItemStack::new(stone, 50));
        inv.slots[18] = Some(ItemStack::new(soil, 3));
        // Merge up to the stack limit.
        inv.move_items(&i, 9, 0, 50).unwrap();
        assert_eq!((inv.get(0).unwrap().count, inv.get(9).unwrap().count), (100, 30));
        // Split into the same smart stack, or onto an empty slot.
        inv.move_items(&i, 9, 1, 10).unwrap();
        assert_eq!(inv.get(1).unwrap().count, 10);
        inv.move_items(&i, 9, 27, 10).unwrap();
        assert_eq!(inv.get(27).unwrap().count, 10);
        // Soil never enters a stone smart stack.
        assert!(inv.move_items(&i, 18, 2, 3).is_none());
        // Whole stacks swap between slots that take each other.
        inv.move_items(&i, 18, 9, 3).unwrap();
        assert_eq!(inv.get(9).unwrap().item, soil);
        assert_eq!(inv.get(18).unwrap().item, stone);
        assert!(inv.move_items(&i, 5, 6, 1).is_none(), "empty source");
        assert!(inv.move_items(&i, 0, 9999, 1).is_none(), "bad slot");
        // Where a drop on a slot lands.
        assert_eq!(inv.drop_target(&i, 27, 0, 10), Some(1), "tops up a stack");
        inv.slots[1] = Some(ItemStack::new(stone, 100));
        assert_eq!(inv.drop_target(&i, 27, 0, 10), Some(2), "else an empty sub-slot");
        assert_eq!(inv.drop_target(&i, 9, 0, 3), None, "soil cannot go in");
    }

    #[test]
    fn whole_smart_stacks_move_merge_and_swap() {
        let i = items();
        let (stone, soil) = (i.id("stone").unwrap(), i.id("soil").unwrap());
        let mut inv = Inventory::default();
        inv.slots[0] = Some(ItemStack::coloured(stone, Colour(1), 10));
        inv.slots[1] = Some(ItemStack::coloured(stone, Colour(2), 10));
        inv.slots[9] = Some(ItemStack::coloured(stone, Colour(3), 10));
        inv.slots[18] = Some(ItemStack::new(soil, 4));
        // Merged into a smart stack of the same family.
        inv.move_group(&i, 0, 1).unwrap();
        assert_eq!(inv.group_stacks(1).len(), 3);
        assert!(inv.held(0).is_none());
        // Swapped with a different one.
        inv.move_group(&i, 2, 1).unwrap();
        assert_eq!(inv.held(1).unwrap().item, soil);
        assert_eq!(inv.group_stacks(2).len(), 3);
        // Moved into an empty slot.
        inv.move_group(&i, 2, 30).unwrap();
        assert_eq!(inv.group_stacks(30).len(), 3);
        assert!(inv.move_group(&i, 2, 3).is_none(), "nothing to move");
    }

    #[test]
    fn sorting_the_pack_gathers_families() {
        let i = items();
        let (stone, soil) = (i.id("stone").unwrap(), i.id("soil").unwrap());
        let mut inv = Inventory::default();
        inv.slots[90] = Some(ItemStack::new(soil, 5));
        inv.slots[99] = Some(ItemStack::coloured(stone, Colour(4), 30));
        inv.slots[108] = Some(ItemStack::new(soil, 7));
        inv.slots[117] = Some(ItemStack::coloured(stone, Colour(4), 30));
        inv.slots[3] = Some(ItemStack::new(soil, 1));
        inv.sort_from(&i, HOTBAR_SLOTS);
        assert_eq!(inv.get(3).unwrap().count, 1, "the hotbar is left alone");
        let soils: Vec<_> = (10..40)
            .filter(|&g| inv.held(g).is_some_and(|s| s.item == soil))
            .collect();
        let stones: Vec<_> = (10..40)
            .filter(|&g| inv.held(g).is_some_and(|s| s.item == stone))
            .collect();
        assert_eq!((soils.len(), stones.len()), (1, 1));
        assert_eq!(inv.group_count(soils[0]), 12);
        assert_eq!(inv.group_count(stones[0]), 60);
        assert_eq!(inv.count(stone) + inv.count(soil), 73, "nothing lost");
    }

    #[test]
    fn old_saves_and_sanitizing() {
        let i = items();
        let (stone, soil) = (i.id("stone").unwrap(), i.id("soil").unwrap());
        let mut old = vec![None; INVENTORY_SLOTS];
        old[0] = Some(ItemStack::new(stone, 5));
        old[39] = Some(ItemStack::new(soil, 2));
        let inv = Inventory::from_plain(&old);
        assert_eq!((inv.held(0).unwrap().count, inv.held(39).unwrap().count), (5, 2));
        assert_eq!(inv.slots.len(), PACK_SLOTS);
        // Impossible stacks go; a stray of another family moves out.
        let mut bad = Inventory::default();
        bad.slots[0] = Some(ItemStack::new(ItemId(60000), 1));
        bad.slots[1] = Some(ItemStack::new(stone, 500));
        bad.slots[2] = Some(ItemStack::new(stone, 5));
        bad.slots[3] = Some(ItemStack::new(soil, 4));
        bad.slots.truncate(40);
        bad.sanitize(&i);
        assert_eq!(bad.slots.len(), PACK_SLOTS);
        assert_eq!(bad.group_stacks(0), vec![ItemStack::new(stone, 5)]);
        assert_eq!(bad.held(1), Some(ItemStack::new(soil, 4)));
        // A row keeps its length.
        let mut row = Inventory::row(vec![None; 3]);
        row.sanitize(&i);
        assert_eq!(row.slots.len(), 3);
    }
}
