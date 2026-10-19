# ADR-0009: Portals are conduit rectangles linked by tokens, paid per trip

- **Status:** Accepted, 2026-10-19.

## Context

Boundless's portals (research: `docs/research/boundless-mechanics.md` §3.3, §3.4) were rectangles of portal conduits inside a beacon, linked by a token crafted at one portal and inserted at another of the same shape, opened with oort shards and then burning shards every hour whether used or not. The forum research (`boundless-forum-feedback.md`) ranks that hourly burn among the most disliked chores ("I'm sick and tired of fueling portals"), and an unfuelled portal closed and charged its opening fee again. Nathan (2026-10-19): keep fuel but make it less punishing; a portal never disappears, burns nothing idle, and charges each player at most once per portal per cooldown; the fuel is a new mined crystal; the portal's tank pays; tokens link portals; see-through windows come later.

## Decision

- **A portal is a filled, upright rectangle of `portal_conduit` blocks**, 1 to 6 wide and 2 to 6 tall, in an X-Y or Z-Y plane, on its owner's claim. The server finds the rectangle from any of its conduits (`bm-core/src/portal.rs`); nothing about the shape is trusted from the client.
- **The server keeps a record per portal** (planet, lower corner, axis, width, height, owner, link, tank, opening side), saved with the planet in its own table in the same write batch as everything else (ADR-0005). Portal numbers are unique across the universe and never reused.
- **Open is a block state, not a renderer feature.** An open portal's conduits are replaced by `portal_surface` blocks (not solid, translucent, glowing); a dormant or unlinked one shows conduits. Clients need no portal logic to draw it, and walking into the surface is an ordinary physics question (invariant 3 holds: both sides read the same blocks).
- **Tokens are items bound to a portal by `ItemStack::tag`.** A tag is a number a single stack carries (0: none; stacks merge only with equal tags). Saves write it after an `@` in the item name, as colours use `#` (ADR-0007), so the save layout and `SAVE_VERSION` are unchanged.
- **Fuel is paid per trip from the portal's own tank**, at most once per player per portal per cooldown, never while idle. A tank that cannot pay makes its side dormant, never unlinks or removes anything. A portal with fuel inside cannot be broken; a lapsing claim returns the fuel through reclaim.
- **Travel uses the existing paths:** another planet goes through the M4 journey (`Travel`); the same planet uses a lightweight `Teleport` the predictor adopts as a jump, not a correction.
- **The new ore reaches existing planets** by appending a kind's new ores to a planet's saved generator settings on opening (only ever appending). Earlier ores keep their places because ores are tried in order; only never-saved chunks can change, and only from plain rock to the new ore.

## Consequences

- No hourly burn and no lost portals: a hub is a stock of wayshards that drains with distinct visitors, not with time.
- See-through windows need the far side's chunks near the portal on the client and a second render pass; nothing here prevents it.
- `ItemStack::tag` is general: later bound items (keys, named maps, deeds) use it the same way.
- Appending ores is safe only for ores at the end of a list; reordering or removing a kind's ores still needs a generator version bump.
