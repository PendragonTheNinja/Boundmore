//! An open storage block, as the client sees it.
//!
//! The server is the authority (ADR-0004): nothing here changes what the
//! player sees. The window shows the last `Storage` and `Slots` the server
//! sent; these functions only decide which moves are worth asking for, using
//! the very rule the server runs (`bm_core::inventory::move_across`), so an
//! obviously refused drag is never sent. The offline demo answers with the
//! same rule.
//!
//! In the window, `SlotRef::Inventory(g)` names a *visible* slot, a smart
//! stack (milestone 33); messages name stored indices, so every request
//! turns the one into the other here.

use crate::items::{Items, half};
use bm_core::inventory::{Side, move_across};
use bm_core::{BlockPos, Inventory, ItemRegistry, ItemStack};
use bm_protocol::{ClientMsg, SlotRef};

/// A protocol slot reference (stored indices) as the inventory rules name
/// it.
pub fn side(r: SlotRef) -> Side {
    match r {
        SlotRef::Inventory(i) => Side::Pack(i as usize),
        SlotRef::Storage(i) => Side::Row(i as usize),
    }
}

/// Applies a move (stored indices) between the inventory and a chest, or
/// within either, the way the server does. Returns whether anything moved;
/// on false neither side changed.
pub fn apply_move(
    items: &ItemRegistry,
    inv: &mut Inventory,
    chest: &mut [Option<ItemStack>],
    from: SlotRef,
    to: SlotRef,
    count: u16,
) -> bool {
    move_across(items, inv, chest, side(from), side(to), count)
}

/// What a slot of either side shows: a visible inventory slot's active
/// stack, or a chest slot's.
pub fn stack_at(inv: &Inventory, chest: &[Option<ItemStack>], r: SlotRef) -> Option<ItemStack> {
    match r {
        SlotRef::Inventory(g) => inv.held(g as usize),
        SlotRef::Storage(i) => chest.get(i as usize).copied().flatten(),
    }
}

/// Where a stack (`src`, coming from `from`, a stored reference) dropped on
/// window slot `to` lands, as a stored reference: in a visible inventory
/// slot, a stack it tops up, else an empty sub-slot if the smart stack takes
/// its family, else the active stack (a whole-stack swap); a chest slot is
/// itself.
fn landing(items: &ItemRegistry, inv: &Inventory, from: SlotRef, src: ItemStack, to: SlotRef) -> Option<SlotRef> {
    let SlotRef::Inventory(g) = to else { return Some(to) };
    let g = g as usize;
    if g >= inv.groups() {
        return None;
    }
    let max = items.get(src.item).map_or(1, |d| d.stack);
    let range = inv.group_range(g);
    let own = |i: usize| from == SlotRef::Inventory(i as u16);
    let pick = range
        .clone()
        .find(|&i| !own(i) && inv.get(i).is_some_and(|d| d.stacks_with(&src) && d.count < max))
        .or_else(|| {
            inv.accepts(items, g, src.item, None)
                .then(|| range.clone().find(|&i| inv.slots[i].is_none()))
                .flatten()
        })
        .or_else(|| inv.active(g))?;
    Some(SlotRef::Inventory(pick as u16))
}

/// A window slot as a stored reference to its active stack (a visible
/// inventory slot's first filled sub-slot).
fn stored(inv: &Inventory, r: SlotRef) -> Option<SlotRef> {
    match r {
        SlotRef::Inventory(g) => inv.active(g as usize).map(|i| SlotRef::Inventory(i as u16)),
        other => Some(other),
    }
}

/// The message for dragging window slot `from` onto `to` (the whole stack
/// or half), or None when the server would refuse it. Within the inventory
/// it is the inventory's own request (whole smart stacks move together);
/// anything touching the chest is a `StorageMove` of the active stack.
pub fn move_request(
    items: &Items,
    chest: &[Option<ItemStack>],
    pos: BlockPos,
    from: SlotRef,
    to: SlotRef,
    half_stack: bool,
) -> Option<ClientMsg> {
    if let (SlotRef::Inventory(a), SlotRef::Inventory(b)) = (from, to) {
        return items.move_request(a as usize, b as usize, half_stack);
    }
    let inv = &items.inventory;
    let from = stored(inv, from)?;
    move_stored(&items.registry, inv, chest, pos, from, to, half_stack)
}

/// As [`move_request`], for a stack named by its stored reference (a
/// sub-slot of an opened smart stack, or a chest slot).
pub fn move_stored(
    items: &ItemRegistry,
    inv: &Inventory,
    chest: &[Option<ItemStack>],
    pos: BlockPos,
    from: SlotRef,
    to: SlotRef,
    half_stack: bool,
) -> Option<ClientMsg> {
    let src = match from {
        SlotRef::Inventory(i) => inv.get(i as usize),
        SlotRef::Storage(i) => chest.get(i as usize).copied().flatten(),
    }?;
    let count = if half_stack { half(src.count) } else { src.count };
    let to = landing(items, inv, from, src, to)?;
    let (mut i, mut c) = (inv.clone(), chest.to_vec());
    if !apply_move(items, &mut i, &mut c, from, to, count) {
        return None;
    }
    Some(match (from, to) {
        (SlotRef::Inventory(a), SlotRef::Inventory(b)) => ClientMsg::MoveItems { from: a, to: b, count },
        _ => ClientMsg::StorageMove { pos, from, to, count },
    })
}

/// The messages for Shift-clicking window slot `from`: everything in it
/// across to the other side (milestone 33: every stack of a smart stack,
/// each onto the first stack it can join with room, else the first empty
/// slot), worked out one after another as the server will apply them.
pub fn shift_requests(
    items: &ItemRegistry,
    inv: &Inventory,
    chest: &[Option<ItemStack>],
    pos: BlockPos,
    from: SlotRef,
) -> Vec<ClientMsg> {
    let sources: Vec<SlotRef> = match from {
        SlotRef::Inventory(g) => inv
            .group_range(g as usize)
            .filter(|&i| inv.slots[i].is_some())
            .map(|i| SlotRef::Inventory(i as u16))
            .collect(),
        other => vec![other],
    };
    let (mut i, mut c) = (inv.clone(), chest.to_vec());
    let mut out = Vec::new();
    for src_ref in sources {
        let Some(src) = (match src_ref {
            SlotRef::Inventory(k) => i.get(k as usize),
            SlotRef::Storage(k) => c.get(k as usize).copied().flatten(),
        }) else {
            continue;
        };
        let Some(to) = shift_target(items, &i, &c, src_ref, src) else {
            continue;
        };
        if apply_move(items, &mut i, &mut c, src_ref, to, src.count) {
            out.push(ClientMsg::StorageMove {
                pos,
                from: src_ref,
                to,
                count: src.count,
            });
        }
    }
    out
}

/// Where Shift-click sends a stack: across to the other side, onto the
/// first stack it can join (same item and colour, unworn) with room, else
/// (into the pack) where adding it would put it, else the first empty slot.
fn shift_target(
    items: &ItemRegistry,
    inv: &Inventory,
    chest: &[Option<ItemStack>],
    from: SlotRef,
    src: ItemStack,
) -> Option<SlotRef> {
    let limit = items.get(src.item).map(|d| d.stack).unwrap_or(1);
    match from {
        SlotRef::Inventory(_) => {
            let merge = chest
                .iter()
                .position(|s| s.is_some_and(|d| d.stacks_with(&src) && d.count < limit));
            let slot = merge.or_else(|| chest.iter().position(|s| s.is_none()))?;
            Some(SlotRef::Storage(u8::try_from(slot).ok()?))
        }
        SlotRef::Storage(_) => {
            let mut trial = inv.clone();
            trial.add_stack(items, src);
            let slot = (0..trial.slots.len()).find(|&i| trial.slots[i] != inv.slots[i])?;
            Some(SlotRef::Inventory(slot as u16))
        }
    }
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
    use bm_core::inventory::SMART;
    use bm_core::{BlockRegistry, Colour};

    fn reg() -> ItemRegistry {
        ItemRegistry::builtin(&BlockRegistry::builtin())
    }

    fn st(r: &ItemRegistry, name: &str, n: u16) -> Option<ItemStack> {
        Some(ItemStack::new(r.id(name).unwrap(), n))
    }

    fn items_with(inv: Inventory) -> Items {
        let blocks = BlockRegistry::builtin();
        let reg = reg();
        Items::new(
            reg.defs().to_vec(),
            vec![],
            inv,
            bm_core::ColourPalette::builtin(),
            &blocks,
        )
        .unwrap()
    }

    #[test]
    fn moves_between_the_sides_follow_the_inventory_rules() {
        let r = reg();
        let mut inv = Inventory::default();
        inv.slots[0] = st(&r, "stone", 30);
        let mut chest = vec![None; 4];
        let (i, s) = (SlotRef::Inventory, SlotRef::Storage);
        assert!(apply_move(&r, &mut inv, &mut chest, i(0), s(1), 10));
        assert_eq!((inv.slots[0], chest[1]), (st(&r, "stone", 20), st(&r, "stone", 10)));
        // Soil never joins the stone smart stack.
        chest[2] = st(&r, "soil", 3);
        assert!(!apply_move(&r, &mut inv, &mut chest, s(2), i(1), 3));
        assert!(apply_move(&r, &mut inv, &mut chest, s(2), i(SMART as u16), 3));
        assert_eq!(inv.held(1), st(&r, "soil", 3));
    }

    #[test]
    fn drags_are_checked_before_they_are_sent() {
        let r = reg();
        let mut inv = Inventory::default();
        inv.slots[0] = st(&r, "stone", 30);
        inv.slots[SMART] = st(&r, "soil", 5);
        let items = items_with(inv);
        let chest = vec![None, st(&r, "stone", 95), st(&r, "sand", 2)];
        let pos = BlockPos::new(1, 2, 3);
        let (i, s) = (SlotRef::Inventory, SlotRef::Storage);
        let mv = |from, to, count| Some(ClientMsg::StorageMove { pos, from, to, count });
        // Into the chest: onto an empty slot, or topping up a stack.
        assert_eq!(move_request(&items, &chest, pos, i(0), s(0), false), mv(i(0), s(0), 30));
        assert_eq!(move_request(&items, &chest, pos, i(0), s(1), true), mv(i(0), s(1), 15));
        // Out of the chest onto a visible slot: into its smart stack when
        // it is of that family, never into another's (sand onto soil swaps
        // whole stacks, which the soil's smart stack refuses no more than
        // the stone's would).
        assert_eq!(move_request(&items, &chest, pos, s(1), i(0), false), mv(s(1), i(0), 95));
        assert_eq!(
            move_request(&items, &chest, pos, s(1), i(5), false),
            mv(s(1), i(5 * SMART as u16), 95)
        );
        assert_eq!(
            move_request(&items, &chest, pos, s(2), i(1), false),
            mv(s(2), i(SMART as u16), 2)
        );
        // Within the inventory, the inventory's own requests.
        assert_eq!(
            move_request(&items, &chest, pos, i(0), i(1), false),
            Some(ClientMsg::MoveGroup { from: 0, to: 1 })
        );
        // Nothing from nothing.
        assert_eq!(move_request(&items, &chest, pos, s(0), i(3), false), None);
    }

    #[test]
    fn shift_click_sends_a_whole_smart_stack_across_and_sorts_it() {
        let r = reg();
        let stone = r.id("stone").unwrap();
        let mut inv = Inventory::default();
        inv.slots[0] = Some(ItemStack::coloured(stone, Colour(3), 10));
        inv.slots[1] = Some(ItemStack::coloured(stone, Colour(5), 7));
        inv.slots[2] = Some(ItemStack::new(stone, 4));
        let chest = vec![None, Some(ItemStack::coloured(stone, Colour(5), 90)), None, None];
        let pos = BlockPos::new(0, 0, 0);
        let msgs = shift_requests(&r, &inv, &chest, pos, SlotRef::Inventory(0));
        let to = |m: &ClientMsg| match m {
            ClientMsg::StorageMove { to, .. } => *to,
            _ => unreachable!(),
        };
        assert_eq!(msgs.len(), 3, "every stack goes");
        // Colour 3 to the first empty slot, colour 5 onto its stack, natural
        // to the next empty one.
        assert_eq!(
            msgs.iter().map(to).collect::<Vec<_>>(),
            [SlotRef::Storage(0), SlotRef::Storage(1), SlotRef::Storage(2)]
        );
        // And back: into the pack where adding would put it.
        let msgs = shift_requests(&r, &inv, &chest, pos, SlotRef::Storage(1));
        assert_eq!(to(&msgs[0]), SlotRef::Inventory(1), "onto the matching stack");
    }

    #[test]
    fn views_from_the_server_are_sanitized() {
        let r = reg();
        let slots = vec![
            Some(ItemStack::new(bm_core::ItemId(60000), 1)),
            Some(ItemStack::new(r.id("stone").unwrap(), 0)),
            st(&r, "stone", 3),
        ];
        let clean = sanitize(&r, slots);
        assert_eq!(clean, vec![None, None, st(&r, "stone", 3)]);
        assert_eq!(sanitize(&r, vec![None; 400]).len(), 256);
    }
}
