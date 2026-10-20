# Roadmap: from Boundless to Boundmore

*Written 2026-10-18 at the end of milestone 1. A plan, not a promise: each milestone gets its own spec, agreed with Nathan, before it is built.*

## The systems that make Boundless itself

What a Boundless player expects to find, and where each stands. "Ours" is the Boundmore take; every name and asset will be our own.

| Boundless system | What it is to players | Boundmore status |
|---|---|---|
| Many small planets | Distinct worlds by tier, climate, hazards, resources; seen hanging in each other's skies | **Done** for four kinds in one universe, visible in each other's skies (M4). More kinds and hazards later |
| Worlds that wrap | No edge; walk round the world | **Done** (ADR-0002) |
| Freeform building | Every block editable, huge palette, colours | **Done** for placing and breaking; palette and colours: M2 |
| Shaped blocks (chisel) | Slopes, bevels, cuts on any block | M7. Decided (Nathan, 2026-10-19): Boundless's square, slope and bevel families on the 2x2x2 corner grid, precise versions as better-tier tools, plus a round family (rounded edges and corners; Nathan: try it and see how it works); lattice frames later; room in the shape encoding for 8 families. Terrain chisels as slopes only |
| Mining by hand and tool | Swings that deal damage against block health; world tiers add health and armour; the right tool; more ore on harder worlds; AoE from forging | M9 (`milestones/09-mining-and-light.md`): Boundless's numbers exactly (Nathan, 2026-10-20). Energy, skills and forging later |
| Light and darkness | Dark caves, torches, glowing ores, day and night | **Done** for light, dark and glowing blocks (M2). Torches, placed and held: M9. Gem torches, lanterns and tinted light later |
| Inventory and crafting | Gather, craft at machines that run over time, upgrade with power | **Done** for inventory, hand crafting, workbench and furnace (M3). Boundless's own chain and machines: M11. Power and spark later |
| Gathering objects | Plants, flowers, boulders and fungi as models on every world; the main source of leaves, flint, food and forge materials | M11 (Nathan, 2026-10-20: "the plants are really important") |
| Plots and beacons | Claim land so builds are protected; costs upkeep | **Done** for claims (M3). **Changed on purpose:** no fuel; a claim lapses after 6 months without a visit and its contents go back to the owner (the research below) |
| Settlements | Plots near each other form ranked settlements | M8 |
| Portals and travel | Player-built portals link planets; warps; grapples | Warp shrines at each spawn (M4, free for now). Player-built portals need fuel, less punishing than Boundless (Nathan, 2026-10-19). Agreed (Nathan, 2026-10-19): a portal never disappears (dormant when empty, reopens when refuelled), uses no fuel while idle, and charges each player at most once per portal per cooldown (a tunable; a day is one option, shorter is fine), so back-and-forth trips cannot drain it. Decided (Nathan, 2026-10-19): the fuel is a new mined crystal (wayshards), the portal's own tank pays per trip, portals link with tokens as in Boundless, see-through windows come in a later milestone. Built as M8 (`docs/milestones/08-portals.md`, ADR-0009). Grapples: M7 part 2 (built 2026-10-19), one hook at a time; dual grapples later |
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

**M8: Portals (built 2026-10-20).** Player-built portals linked by tokens, fuelled per trip (ADR-0009). See `milestones/08-portals.md`.

**M9: Mining and light.** Boundless's mining exactly (health by world tier, armour, swings at a rate set by action speed, the right tool, bonus yields by tier) and the basic torch. See `milestones/09-mining-and-light.md`.

**M10: Energy.** Each swing, grapple shot and chisel cut costs energy from a bar of 600, as in Boundless. See `milestones/10-energy.md`.

**M11: The crafting chain** (decided 2026-10-20, spec to write). Boundless's materials and machines up to iron tools: crafting table, stones, sticks, the two-part stone furnace with fuel and heat, an old-tech seam, sap and glue (2 sap until creatures bring bone), the extractor and sackcloth. Also the **gatherable plants, boulders and fungi** scattered over every world, which give fibrous leaves, flint, beans, petals, lamellae and orbs (tuning 4.4a).

*After that, still Boundless first (Nathan, 2026-10-19 and 2026-10-20): skills and progression exactly as Boundless's live game (`proposals/skills-and-progression.md`), creatures and combat (bone, tallow and hide, then glue's real recipe), food and hunger, forging. Each gets its own spec and number when it starts.*

**Society.** Shop stands, request baskets, settlements and their ranking, guilds, mail.

**Life.** Creatures, combat, hunting, meteors, temporary worlds.

**Ownership.** Rentable worlds, world customisation, and the operations work for running it as a service.

## Player research

`research/boundless-forum-feedback.md` ranks what Boundless players liked, disliked and wished for, with ten candidate principles. Decided so far: no beacon fuel (claims lapse after 6 months away, nothing destroyed). Open: portal fuel (Nathan is more open to it than to beacon fuel), whether to move PvE earlier (the most-wished-for content), schematics that save a lapsed build so it can be placed again.

## Later art passes

- **Larger textures** (Nathan asked, 2026-10-19). Boundless's block textures were 256x256 (a Wonderstruck developer on the forum, 2015, topic 2404; confirmed by players in 2020), smooth and painterly; ours are 32x32 by choice. Players coming from Boundless may expect more detail. Plan: make the procedural painters resolution-aware so the same recipes paint 64×64 with real added detail; offer a texture-quality setting (32 or 64). Above 64, hand-made or AI-made drop-in replacements fit better than procedural paint.
