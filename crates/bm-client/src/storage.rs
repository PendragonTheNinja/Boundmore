//! An open storage block, as the client sees it.
//!
//! The server is the authority (ADR-0004): nothing here changes what the
//! player sees. The window shows the last `Storage` and `Slots` the server
//! sent; these functions only decide which moves are worth asking for, using
//! the very rules the server runs (`Inventory::move_items` over the
//! inventory and the chest as one row of slots), so an obviously refused
//! drag is never sent. The offline demo answers with the same rules.

use crate::items::half;
use bm_core::{BlockPos, Inventory, ItemRegistry, ItemStack};
use bm_protocol::{ClientMsg, SlotRef};

/// Applies a move between the inventory and a chest (or within either) the
/// way the server does: merge, split, or a whole-stack swap. Returns whether
/// anything moved; on false neither side changed.
pub fn apply_move(
    items: &ItemRegistry,
    inv: &mut [Option<ItemStack>],
    chest: &mut [Option<ItemStack>],
    from: SlotRef,
    to: SlotRef,
    count: u16,
) -> bool {
    let n = inv.len();
    let index = |r: SlotRef| match r {
        SlotRef::Inventory(i) => ((i as usize) < n).then_some(i as usize),
        SlotRef::Storage(i) => ((i as usize) < chest.len()).then_some(n + i as usize),
    };
    let (Some(a), Some(b)) = (index(from), index(to)) else {
        return false;
    };
    let mut all = Inventory {
        slots: inv.iter().chain(chest.iter()).copied().collect(),
    };
    if all.move_items(items, a, b, count).is_none() {
        return false;
    }
    let (left, right) = all.slots.split_at(n);
    inv.copy_from_slice(left);
    chest.copy_from_slice(right);
    true
}

/// What is in a slot of either side.
pub fn stack_at(inv: &[Option<ItemStack>], chest: &[Option<ItemStack>], r: SlotRef) -> Option<ItemStack> {
    match r {
        SlotRef::Inventory(i) => inv.get(i as usize).copied().flatten(),
        SlotRef::Storage(i) => chest.get(i as usize).copied().flatten(),
    }
}

/// The message for dragging `from` onto `to` (the whole stack or half), or
/// None when the server would refuse it. A move within the inventory is an
/// ordinary `MoveItems`; anything touching the chest is a `StorageMove`.
pub fn move_request(
    items: &ItemRegistry,
    inv: &[Option<ItemStack>],
    chest: &[Option<ItemStack>],
    pos: BlockPos,
    from: SlotRef,
    to: SlotRef,
    half_stack: bool,
) -> Option<ClientMsg> {
    let src = stack_at(inv, chest, from)?;
    let count = if half_stack { half(src.count) } else { src.count };
    let (mut i, mut c) = (inv.to_vec(), chest.to_vec());
    if !apply_move(items, &mut i, &mut c, from, to, count) {
        return None;
    }
    Some(match (from, to) {
        (SlotRef::Inventory(a), SlotRef::Inventory(b)) => ClientMsg::MoveItems { from: a, to: b, count },
        _ => ClientMsg::StorageMove { pos, from, to, count },
    })
}

/// Where Shift-click sends a stack: across to the other side, onto the
/// first stack it can join (same item and colour, unworn) with room, else
/// the first empty slot.
pub fn shift_target(
    items: &ItemRegistry,
    inv: &[Option<ItemStack>],
    chest: &[Option<ItemStack>],
    from: SlotRef,
) -> Option<SlotRef> {
    let src = stack_at(inv, chest, from)?;
    let (other, across) = match from {
        SlotRef::Inventory(_) => (chest, true),
        SlotRef::Storage(_) => (inv, false),
    };
    let limit = items.get(src.item).map(|d| d.stack).unwrap_or(1);
    let merge = other
        .iter()
        .position(|s| s.is_some_and(|d| d.stacks_with(&src) && d.count < limit));
    let slot = merge.or_else(|| other.iter().position(|s| s.is_none()))?;
    let slot = u8::try_from(slot).ok()?;
    Some(if across {
        SlotRef::Storage(slot)
    } else {
        SlotRef::Inventory(slot)
    })
}

/// The message for Shift-clicking `from`: the whole stack across.
pub fn shift_request(
    items: &ItemRegistry,
    inv: &[Option<ItemStack>],
    chest: &[Option<ItemStack>],
    pos: BlockPos,
    from: SlotRef,
) -> Option<ClientMsg> {
    let to = shift_target(items, inv, chest, from)?;
    move_request(items, inv, chest, pos, from, to, false)
}

/// A storage view from the server, made safe to index and draw: unknown
/// items and empty stacks become empty slots, and no more slots than a
/// `SlotRef` can name.
pub fn sanitize(items: &ItemRegistry, slots: Vec<Option<ItemStack>>) -> Vec<Option<ItemStack>> {
    slots
        .into_iter()
        .take(u8::MAX as usize + 1)
        .map(|s| s.filter(|s| items.contains(s.item) && s.count > 0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{BlockRegistry, INVENTORY_SLOTS};

    fn reg() -> ItemRegistry {
        ItemRegistry::builtin(&BlockRegistry::builtin())
    }

    fn st(r: &ItemRegistry, name: &str, n: u16) -> Option<ItemStack> {
        Some(ItemStack::new(r.id(name).unwrap(), n))
    }

    const POS: BlockPos = BlockPos { x: 1, y: 2, z: 3 };

    #[test]
    fn moves_between_the_sides_follow_the_inventory_rules() {
        let r = reg();
        let mut inv = vec![None; INVENTORY_SLOTS];
        let mut chest = vec![None; 20];
        inv[0] = st(&r, "stone", 80);
        chest[0] = st(&r, "stone", 50);
        chest[1] = st(&r, "soil", 3);
        // Merge into the inventory up to the stack limit.
        assert!(apply_move(
            &r,
            &mut inv,
            &mut chest,
            SlotRef::Storage(0),
            SlotRef::Inventory(0),
            50
        ));
        assert_eq!(inv[0].unwrap().count, 100);
        assert_eq!(chest[0].unwrap().count, 30);
        // Split into an empty chest slot.
        assert!(apply_move(
            &r,
            &mut inv,
            &mut chest,
            SlotRef::Inventory(0),
            SlotRef::Storage(5),
            10
        ));
        assert_eq!((inv[0].unwrap().count, chest[5].unwrap().count), (90, 10));
        // Different items swap whole stacks only.
        let before = (inv.clone(), chest.clone());
        assert!(!apply_move(
            &r,
            &mut inv,
            &mut chest,
            SlotRef::Storage(1),
            SlotRef::Inventory(0),
            1
        ));
        assert_eq!((inv.clone(), chest.clone()), before, "a refused move changes nothing");
        assert!(apply_move(
            &r,
            &mut inv,
            &mut chest,
            SlotRef::Storage(1),
            SlotRef::Inventory(0),
            3
        ));
        assert_eq!(inv[0].unwrap().item, r.id("soil").unwrap());
        assert_eq!(chest[1].unwrap().count, 90);
        // Off either end.
        assert!(!apply_move(
            &r,
            &mut inv,
            &mut chest,
            SlotRef::Storage(20),
            SlotRef::Inventory(1),
            1
        ));
        assert!(!apply_move(
            &r,
            &mut inv,
            &mut chest,
            SlotRef::Storage(1),
            SlotRef::Inventory(40),
            1
        ));
    }

    #[test]
    fn drags_are_checked_before_they_are_sent() {
        let r = reg();
        let mut inv = vec![None; INVENTORY_SLOTS];
        let mut chest = vec![None; 20];
        inv[0] = st(&r, "stone", 9);
        inv[1] = st(&r, "stone", 100);
        chest[0] = st(&r, "soil", 4);
        chest[1] = st(&r, "stone", 100);
        let sm = |from, to, count| {
            Some(ClientMsg::StorageMove {
                pos: POS,
                from,
                to,
                count,
            })
        };
        let (i, s) = (SlotRef::Inventory, SlotRef::Storage);
        // Whole and half stacks into an empty chest slot.
        assert_eq!(
            move_request(&r, &inv, &chest, POS, i(0), s(4), false),
            sm(i(0), s(4), 9)
        );
        assert_eq!(move_request(&r, &inv, &chest, POS, i(0), s(4), true), sm(i(0), s(4), 5));
        // Whole stacks of different items swap; half stacks are never sent.
        assert_eq!(
            move_request(&r, &inv, &chest, POS, i(0), s(0), false),
            sm(i(0), s(0), 9)
        );
        assert_eq!(move_request(&r, &inv, &chest, POS, i(0), s(0), true), None);
        // Onto a full stack, from an empty slot, onto itself, off the end.
        assert_eq!(move_request(&r, &inv, &chest, POS, i(0), s(1), false), None);
        assert_eq!(move_request(&r, &inv, &chest, POS, s(9), i(9), false), None);
        assert_eq!(move_request(&r, &inv, &chest, POS, s(0), s(0), false), None);
        assert_eq!(move_request(&r, &inv, &chest, POS, s(0), s(25), false), None);
        // Within the chest is a StorageMove; within the inventory, MoveItems.
        assert_eq!(move_request(&r, &inv, &chest, POS, s(0), s(7), true), sm(s(0), s(7), 2));
        assert_eq!(
            move_request(&r, &inv, &chest, POS, i(0), i(7), false),
            Some(ClientMsg::MoveItems {
                from: 0,
                to: 7,
                count: 9
            })
        );
    }

    #[test]
    fn shift_click_prefers_a_stack_with_room_then_an_empty_slot() {
        let r = reg();
        let mut inv = vec![None; INVENTORY_SLOTS];
        let mut chest = vec![None; 20];
        inv[3] = st(&r, "stone", 30);
        chest[0] = st(&r, "soil", 5);
        chest[2] = st(&r, "stone", 100);
        chest[6] = st(&r, "stone", 60);
        let (i, s) = (SlotRef::Inventory, SlotRef::Storage);
        // The full stack in slot 2 is passed over for the one with room.
        assert_eq!(shift_target(&r, &inv, &chest, i(3)), Some(s(6)));
        assert_eq!(
            shift_request(&r, &inv, &chest, POS, i(3)),
            Some(ClientMsg::StorageMove {
                pos: POS,
                from: i(3),
                to: s(6),
                count: 30
            })
        );
        // No soil in the inventory: the first empty slot (the hotbar's first).
        assert_eq!(shift_target(&r, &inv, &chest, s(0)), Some(i(0)));
        // A full chest with no room for the item sends nothing.
        let full: Vec<_> = (0..20).map(|_| st(&r, "soil", 100)).collect();
        assert_eq!(shift_target(&r, &inv, &full, i(3)), None);
        assert_eq!(shift_request(&r, &inv, &full, POS, i(3)), None);
        // Nothing to send from an empty slot.
        assert_eq!(shift_target(&r, &inv, &chest, i(0)), None);
        // Worn items never merge, whatever their stack size: an empty slot.
        let mut worn = st(&r, "stone", 10).unwrap();
        worn.wear = 5;
        inv[4] = Some(worn);
        assert_eq!(shift_target(&r, &inv, &chest, i(4)), Some(s(1)));
    }

    #[test]
    fn shift_click_joins_only_a_stack_of_the_same_colour() {
        use bm_core::Colour;
        let r = reg();
        let stone = r.id("stone").unwrap();
        let mut inv = vec![None; INVENTORY_SLOTS];
        let mut chest = vec![None; 20];
        inv[3] = Some(ItemStack::coloured(stone, Colour(12), 10));
        chest[0] = Some(ItemStack::new(stone, 20));
        chest[1] = Some(ItemStack::coloured(stone, Colour(40), 20));
        chest[4] = Some(ItemStack::coloured(stone, Colour(12), 20));
        let (i, s) = (SlotRef::Inventory, SlotRef::Storage);
        // Natural and another colour are passed over for the matching stack.
        assert_eq!(shift_target(&r, &inv, &chest, i(3)), Some(s(4)));
        // With no stack of its colour, the first empty slot.
        chest[4] = None;
        assert_eq!(shift_target(&r, &inv, &chest, i(3)), Some(s(2)));
        assert!(shift_request(&r, &inv, &chest, POS, i(3)).is_some());
    }

    #[test]
    fn views_from_the_server_are_sanitized() {
        let r = reg();
        let slots = vec![
            st(&r, "stone", 5),
            Some(ItemStack::new(bm_core::ItemId(60000), 1)),
            st(&r, "stone", 0),
            None,
        ];
        let clean = sanitize(&r, slots);
        assert_eq!(clean.len(), 4);
        assert!(clean[0].is_some() && clean[1].is_none() && clean[2].is_none());
        assert_eq!(sanitize(&r, vec![None; 1000]).len(), 256);
    }
}
