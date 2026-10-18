# Roadmap: from Boundless to Boundmore

*Written 2026-10-18 at the end of milestone 1. A plan, not a promise: each milestone gets its own spec, agreed with Nathan, before it is built.*

## The systems that make Boundless itself

What a Boundless player expects to find, and where each stands. "Ours" is the Boundmore take; every name and asset will be our own.

| Boundless system | What it is to players | Boundmore status |
|---|---|---|
| Many small planets | Distinct worlds by tier, climate, hazards, resources; seen hanging in each other's skies | One planet (M1). Planet types and many planets: M4 |
| Worlds that wrap | No edge; walk round the world | **Done** (ADR-0002) |
| Freeform building | Every block editable, huge palette, colours | **Done** for placing and breaking; palette and colours: M2 |
| Shaped blocks (chisel) | Slopes, bevels, cuts on any block | M5 |
| Mining by hand and tool | Hold to break; harder blocks, better tools; AoE tools later | Instant break today. Hold-to-break by hardness: M2 |
| Light and darkness | Dark caves, torches, glowing ores, day and night | Not yet. M2 |
| Inventory and crafting | Gather, craft at machines that run over time, upgrade with power | M3 |
| Plots and beacons | Claim land so builds are protected; costs upkeep | M3 |
| Settlements | Plots near each other form ranked settlements | M6 |
| Portals and travel | Player-built portals link planets; warps; grapples | Portals: M4. Grapples: M5 |
| Player economy | Shop stands and request baskets; almost nothing sold by NPCs | M6 |
| Hunting and creatures | Creatures per planet with drops; world bosses | M7 |
| Meteors and events | Meteor strikes with special drops; temporary worlds that appear for a while | M7 |
| Guilds | Shared plots, permissions, guild buffs | M6 |
| Sovereign worlds | Players rent whole planets | M8 |
| Character progression | Levels, skills, attributes | M3 onward |

## Milestones

**M1: Foundation (done).** Engine, one planet, authoritative server, multiplayer, edits, saves. See `milestones/01-foundation.md`.

**M2: A world worth looking at.** Light propagation (sky light and block light, dark caves, glowing blocks), day and night, a real texture pipeline (a texture atlas or array, our own art direction), the full block palette with colour variants, hold-to-break with hardness, a hotbar and minimal HUD, sound hooks. Ends with a planet that looks like a place.

**M3: Gather, craft, claim.** Accounts (real login, so names cannot be taken), inventory, item drops, tools, crafting machines that work over time, plots/beacons protecting builds. Introduces the relational store (Postgres) for accounts and items, with transactions for every item movement.

**M4: Many worlds.** Planet types (tiers, climates, block sets), several planets per server cluster, portals between them, planets visible in the sky. Each planet its own process; a gateway routes players.

**M5: Movement and shape.** Chisels and shaped blocks (slopes in terrain too), grapples, swimming improvements, far views (level-of-detail terrain).

**M6: Society.** Shop stands, request baskets, settlements and their ranking, guilds, chat.

**M7: Life.** Creatures, combat, hunting, meteors, temporary worlds.

**M8: Ownership.** Rentable worlds, world customisation, and the operations work for running it as a service.

## Things to decide with Nathan before M2

1. **Block scale and player size.** How many blocks tall is a player in Boundless, and how high can they jump? This sets everything from cave size to building feel (`docs/feel.md`).
2. **Art direction.** Boundless has a distinctive bright, painterly look. What is Boundmore's?
3. **Which systems players miss most.** The order above is engineering order; if the community's top wish is, say, the economy, it can move earlier.
