# Milestone 33: Creep and Smart Stacks

*Spec 2026-10-29.* Two of the builders' tools Boundless had and we lack, both decided by Nathan on 2026-10-29 (`docs/feel.md`, "Polish batch 3 decisions"):

- **Creep:** "Yes creep please, and toggleable."
- **Smart stacks:** "Yes they called them smart stacks, up to 9 stacks of the same block TYPE could stack together."

## Part 1: Creep (built 2026-10-29)

What Boundless did: C crept. You moved slower and could not fall off a ledge ("You shouldn't fall unless you cross a chiseled block", t/26751). Builders walked backwards off ledges placing floors and bridges (t/45812: "I spent 5 hours creep-building floors").

Ours:
- **V** switches creep on and off (C is our crafting key). Options: "Hold V to creep" instead.
- A "Creeping (V)" tag over the health bar shows while it is on. It stays on through chat and menus.
- Inside `movement::step`, so prediction stays exact (invariant 2), via `PlayerInput::creep` (protocol 63):
  - On the ground, out of water, not flying and not hanging on a rope, a sideways move that would leave nothing under the body within `CREEP_DROP` (0.55, so a half-block step down is still taken) is undone on that axis.
  - Part of the body may hang over the edge, as in Boundless.
  - Off the ground (a jump), creep does nothing.
- Speed on foot while creeping: `CREEP_SPEED` 1.5 blocks a second, half an untrained walk. **Guess**: Boundless's creep speed is unmeasured. Measure A6: walk 10 blocks creeping in Boundless and time it.

Acceptance (tests in `movement.rs`):
- creeping east never leaves a platform (it stops with the body hanging over the edge), while walking the same path falls;
- creep speed is `CREEP_SPEED`;
- a half-block step down is taken;
- a jump still leaves the ground.

## Part 2: Smart stacks

What Boundless did (wiki "Smart Stack"; forum release notes):
- A smart stack is one inventory slot holding up to **9 stacks**. Each holds up to the item's stack size of **identical** items (same item and colour; a tool is a stack of one).
- Items share a smart stack when they belong to the same **smart stack category**: the same block in any colour (Nathan: "the same block TYPE"), and families of related items.
- Right click (PC) opens a smart stack to look inside.
- Shift-click on a smart stack in the inventory splits and sorts its stacks into the matching cubbies of the storage open beside it.
- Items picked up join a matching smart stack first.

Ours:

1. **Families.** Two items share a smart stack when they have the same family: by default the item itself (all its colours, wear and forging). An item may name a wider family in `assets/items.toml` (`smart = "hammers"`). Families wider than one item follow Boundless's categories as research finds them (`docs/research/boundless-mechanics.md`; the game-data API was down on 2026-10-29). Until then, only the item itself.
2. **Layout.** A visible slot is a group of `SMART` (9) sub-slots. The inventory's stored slots are `INVENTORY_SLOTS` (40) × 9. Add, remove, count and craft keep working over all stored slots, plus one rule: **a group holds one family**, set by whatever is in it.
   - Placing or adding into a group of another family is refused or goes elsewhere.
   - A group's **active** item, the one a hand holds and uses, is its first filled sub-slot. Reorder inside the open smart stack to choose it.
3. **Storage** blocks and station outputs keep plain slots for now (a group of 1). Whether Boundless's storage cubbies held smart stacks is checked in research. If they did, storage can use 9-groups later through the same code.
4. **Picking up** (breaking, crafting, looting, buying): into a group of the same family with room first, then an empty group.
5. **The window:**
   - A smart stack shows its active item's icon with a small 3×3 mark, and the total count.
   - Right click opens it as a 3×3 grid beside the slot: drag sub-stacks out, in or within it.
   - Shift-click on it with storage open sorts each sub-stack into storage: matching stacks first, then empty slots.
   - "Sort" (inventory) gathers families into as few groups as possible.
6. **Hands.** `PlayerInput::slot` and `off` stay hotbar indices (0 to 9). The server and the prediction read the group's active item. Messages naming an inventory slot use the stored index (`u16` where it can pass 255).
7. **Saves.** `SAVE_VERSION` 16. A saved inventory of 40 slots (older) loads as slot *i* into group *i*'s first sub-slot, so nothing moves visibly. Do the CLAUDE.md upgrade check.

Acceptance:
- 9 stacks of stone in nine colours fill one slot;
- a tenth colour goes to a new slot;
- tools of one kind sit nine to a slot;
- crafting draws from inside smart stacks;
- the hand uses the active item;
- shift-click into storage sorts;
- an old universe opens with every item where it was;
- the client's prediction of a held grapple or quirk reads the active item as the server does;
- no new prediction corrections.

## Non-goals

Smart stacks in storage (until research says Boundless had them), auto-sorting on pickup beyond the family rule, and the related-item families (until their data is found).

## Decisions for Nathan

1. **Creep speed** (Part 1 guess, Measure A6).
2. **The active item in a smart stack:** the first sub-stack, chosen by reordering, is the plan. Say if Boundless let you pick it another way (the mouse wheel inside a stack, say).

## Status

- [x] Spec (2026-10-29)
- [x] Part 1, creep (2026-10-29; protocol 63)
- [x] Part 2, smart stacks (2026-10-29; protocol 65, `SAVE_VERSION` 16): `bm_core::inventory` (groups of `SMART`, `same_family`, `held`/`active`, `move_group`, `drop_target`, `sort_from`, `move_across` shared by the server and the client's storage window, `from_plain` for old saves). The window shows a smart stack's total with a 3 by 3 mark; right click opens its stacks in a row to drag out; Shift-click sends every stack across to storage, each onto its match. Hands use the active stack (the first). Messages name stored slots (`u16`); `PlayerInput::slot` stays the hotbar slot.
- [ ] Wider families (Boundless's categories) once their data is found
