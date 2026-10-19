# Roadmap: from Boundless to Boundmore

*Written 2026-10-18 at the end of milestone 1. A plan, not a promise: each milestone gets its own spec, agreed with Nathan, before it is built.*

## The systems that make Boundless itself

What a Boundless player expects to find, and where each stands. "Ours" is the Boundmore take; every name and asset will be our own.

| Boundless system | What it is to players | Boundmore status |
|---|---|---|
| Many small planets | Distinct worlds by tier, climate, hazards, resources; seen hanging in each other's skies | **Done** for four kinds in one universe, visible in each other's skies (M4). More kinds and hazards later |
| Worlds that wrap | No edge; walk round the world | **Done** (ADR-0002) |
| Freeform building | Every block editable, huge palette, colours | **Done** for placing and breaking; palette and colours: M2 |
| Shaped blocks (chisel) | Slopes, bevels, cuts on any block | M7. Decided (Nathan, 2026-10-19): Boundless's square, slope and bevel families on the 2x2x2 corner grid, precise versions as better-tier tools, plus a round family (rounded edges and corners); lattice frames later; room in the shape encoding for 8 families. Terrain chisels as slopes only |
| Mining by hand and tool | Hold to break; harder blocks, better tools; AoE tools later | **Done**: hold-to-break by hardness (M2), stone and copper tools that wear (M3). AoE tools later |
| Light and darkness | Dark caves, torches, glowing ores, day and night | **Done** (M2) |
| Inventory and crafting | Gather, craft at machines that run over time, upgrade with power | **Done** for inventory, hand crafting, workbench and furnace (M3). Power and more machines later |
| Plots and beacons | Claim land so builds are protected; costs upkeep | **Done** for claims (M3). **Changed on purpose:** no fuel; a claim lapses after 6 months without a visit and its contents go back to the owner (the research below) |
| Settlements | Plots near each other form ranked settlements | M8 |
| Portals and travel | Player-built portals link planets; warps; grapples | Warp shrines at each spawn (M4, free for now). Player-built portals need fuel, less punishing than Boundless (Nathan, 2026-10-19). Proposed, awaiting Nathan's yes before building: a portal never disappears (dormant when empty), uses no fuel while idle, and charges each player at most once per portal per day, so back-and-forth trips cannot drain it. Grapples: M7 |
| Player economy | Shop stands and request baskets; almost nothing sold by NPCs | M8 |
| Hunting and creatures | Creatures per planet with drops; world bosses | M9 |
| Meteors and events | Meteor strikes with special drops; temporary worlds that appear for a while | M9 |
| Guilds | Shared plots, permissions, guild buffs | M8 |
| Sovereign worlds | Players rent whole planets | M10 |
| Character progression | Levels, skills, attributes | M3 onward |

## Milestones

**M1: Foundation (done).** Engine, one planet, authoritative server, multiplayer, edits, saves. See `milestones/01-foundation.md`.

**M2: A world worth looking at.** Light propagation (sky light and block light, dark caves, glowing blocks), day and night, a real texture pipeline (a texture atlas or array, our own art direction), the full block palette with colour variants, hold-to-break with hardness, a hotbar and minimal HUD, sound hooks. Ends with a planet that looks like a place.

**M3: Gather, craft, claim.** Accounts (real login, so names cannot be taken), inventory, item drops, tools, crafting machines that work over time, plots/beacons protecting builds. Accounts, inventories, stations and claims live in the same crash-safe redb save as the world, written in one batch so a block broken into an inventory is saved with it (built 2026-10-18).

**M4: Many worlds (built 2026-10-19).** Planet kinds (tiers, climates, block sets), a universe of planets in one process (ADR-0006), warp shrines between them, planets visible in the sky.

**M5: Keep and talk.** Storage chests and chat (local, planet, universe, whispers), ahead of the larger milestones because the forum research ranks broken chat among the reasons players left. See `milestones/05-keep-and-talk.md`.

**M6: Colour and ash.** Block states with colour (ADR-0007), a 255-colour palette, planet colours, coloured ash when a claim lapses, reclaim. See `milestones/06-colour-and-ash.md`.

**M7: Movement and shape.** Chisels and shaped blocks (slopes in terrain too), grapples, swimming improvements, far views (level-of-detail terrain).

**M8: Society.** Shop stands, request baskets, settlements and their ranking, guilds, mail.

**M9: Life.** Creatures, combat, hunting, meteors, temporary worlds.

**M10: Ownership.** Rentable worlds, world customisation, and the operations work for running it as a service.

## Player research

`research/boundless-forum-feedback.md` ranks what Boundless players liked, disliked and wished for, with ten candidate principles. Decided so far: no beacon fuel (claims lapse after 6 months away, nothing destroyed). Open: portal fuel (Nathan is more open to it than to beacon fuel), whether to move PvE earlier (the most-wished-for content), schematics that save a lapsed build so it can be placed again.

## Later art passes

- **Larger textures** (Nathan asked, 2026-10-19): make the procedural painters resolution-aware so the same recipes paint 64×64 with real added detail; offer a texture-quality setting (32 or 64). Above 64, hand-made or AI-made drop-in replacements fit better than procedural paint.
