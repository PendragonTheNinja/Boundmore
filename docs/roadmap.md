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
| Gathering objects | Plants, flowers, boulders and fungi as models on every world; the main source of leaves, flint, food and forge materials | **Built** in M11 (Nathan, 2026-10-20: "the plants are really important"): 20 kinds, placement guessed |
| Plots and beacons | Claim land so builds are protected; costs upkeep | **Done** for claims (M3). **Changed on purpose:** no fuel; a claim lapses after 6 months without a visit and its contents go back to the owner (the research below) |
| Settlements | Plots near each other form ranked settlements | M8 |
| Portals and travel | Player-built portals link planets; warps; grapples | Warp shrines at each spawn (M4, free for now). Player-built portals need fuel, less punishing than Boundless (Nathan, 2026-10-19). Agreed (Nathan, 2026-10-19): a portal never disappears (dormant when empty, reopens when refuelled), uses no fuel while idle, and charges each player at most once per portal per cooldown (a tunable; a day is one option, shorter is fine), so back-and-forth trips cannot drain it. Decided (Nathan, 2026-10-19): the fuel is a new mined crystal (wayshards), the portal's own tank pays per trip, portals link with tokens as in Boundless, see-through windows come in a later milestone. Built as M8 (`docs/milestones/08-portals.md`, ADR-0009). Grapples: M7 part 2 (built 2026-10-19), one hook at a time; dual grapples later |
| Player economy | Shop stands and request baskets; almost nothing sold by NPCs | M8 |
| Hunting and creatures | Creatures per planet with drops; world bosses | M13 (spec written): Boundless's five creatures; meteorites and bosses later |
| Meteors and events | Meteor strikes with special drops; temporary worlds that appear for a while | **Built**: meteors in M14, temporary (wandering) worlds in M21 |
| Guilds | Shared plots, permissions, guild buffs | M8 |
| Sovereign worlds | Players rent whole planets | M10 |
| Character progression | Levels, skills, attributes | **Built** in M12: Boundless's live tree exactly, in Boundmore's names |
| Forging | Traits on gear from a risky session of ingredients | M17: Boundless's rules exactly; coils give the forge power from M18 |
| Higher worlds | Tier 4 to 8 worlds guarded by atmosphere; gems; machine power | **Built** to tier 6 in M18: atmospheres, gems, coils, lava, elements. Wandering worlds (exoworlds) and tier 7 in M21. Tier 8 never went live in Boundless |

## Milestones

**M1: Foundation (done).** Engine, one planet, authoritative server, multiplayer, edits, saves. See `milestones/01-foundation.md`.

**M2: A world worth looking at.** Light propagation (sky light and block light, dark caves, glowing blocks), day and night, a real texture pipeline (a texture atlas or array, our own art direction), the full block palette with colour variants, hold-to-break with hardness, a hotbar and minimal HUD, sound hooks. Ends with a planet that looks like a place.

**M3: Gather, craft, claim.** Accounts (real login, so names cannot be taken), inventory, item drops, tools, crafting machines that work over time, plots/beacons protecting builds. Accounts, inventories, stations and claims live in the same crash-safe redb save as the world, written in one batch so a block broken into an inventory is saved with it (built 2026-10-18).

**M4: Many worlds (built 2026-10-19).** Planet kinds (tiers, climates, block sets), a universe of planets in one process (ADR-0006), warp shrines between them, planets visible in the sky.

**M5: Keep and talk.** Storage chests and chat (local, planet, universe, whispers), ahead of the larger milestones because the forum research ranks broken chat among the reasons players left. See `milestones/05-keep-and-talk.md`.

**M6: Colour and ash.** Block states with colour (ADR-0007), a 255-colour palette, planet colours, coloured ash when a claim lapses, reclaim. See `milestones/06-colour-and-ash.md`.

**M7: Movement and shape.** Chisels and shaped blocks (slopes in terrain too), grapples, swimming improvements, far views (level-of-detail terrain; built as M20).

**M8: Portals (built 2026-10-20).** Player-built portals linked by tokens, fuelled per trip (ADR-0009). See `milestones/08-portals.md`.

**M9: Mining and light (built 2026-10-20).** Boundless's mining exactly (health by world tier, armour, swings at a rate set by action speed, the right tool, bonus yields by tier) and the basic torch. See `milestones/09-mining-and-light.md`.

**M10: Energy (built 2026-10-20).** Each swing, grapple shot and chisel cut costs energy from a bar of 600, as in Boundless. See `milestones/10-energy.md`.

**M11: The crafting chain** (built 2026-10-20, `milestones/11-crafting-chain.md`). Boundless's materials and machines up to iron tools: crafting table, stones, sticks, the two-part stone furnace with fuel and heat, an old-tech seam, sap and glue (2 sap until creatures bring bone), the extractor and sackcloth. Also the **gatherable plants, boulders and fungi** scattered over every world, which give fibrous leaves, flint, beans, petals, lamellae and orbs (tuning 4.4a).

**M12: Skills and progression (built 2026-10-21).** Boundless's levels, XP, skill tree, pages, respecs and marks; marks buy plots. See `milestones/12-skills-and-progression.md`.

**M13: Creatures and combat** (spec 2026-10-21, `milestones/13-creatures-and-combat.md`). Health, damage and defeat, slings, Boundless's creatures in our own names and models (grazer, pelter, dasher; then popper with bombs, and drifter), per-player drops, bone for the real glue, tallow for the better torch, and the combat skills.

**M14: Regions and meteorites (built 2026-10-22, `milestones/14-meteorites.md`).** Named regions with discovery XP; meteorite hunts (Nathan, 2026-10-21): meteors fall as you enter a new region, drop waves of creatures, and every player loots the core afterwards; bigger with world tier and player count. In Boundless they were the main source of portal fuel and "a lot of fun", so they drop wayshards here.

**M15: Spark (built 2026-10-22, `milestones/15-spark.md`).** Spark generators, spark links, the mixer, refinery and compactor, machine wear and spanners, the iron furnace, silver and silver alloy, the compact, refined and machined metal chain. Power coils wait for gems.

**M16: Food** (part 1 built 2026-10-22, `milestones/16-food.md`; Nathan picked it 2026-10-22). Hunger that eats into the energy bar, eating, cooked dishes with Boundless's food buffs, sickness, starving, being full as the way to heal, buckets; part 2 is farming (crops, seeds, tilling, growth), which brings grains and the rank 3 to 5 dishes.

**M17: Forging** (spec 2026-10-22, `milestones/17-forging.md`; Nathan picked it 2026-10-22). The forge, Boundless's session rules exactly (pools, rolls, every ingredient), boons, quirks and flaws on tools, slings, bombs, grapples, spanners and chisels, fossils for the sealing wax. Power coils wait for gems: the ingredients that need power have their rules but no recipe yet.

**M18: Higher worlds** (built 2026-10-25, `milestones/18-higher-worlds.md`). Atmospheres and wards, nine tier 4 to 6 worlds, gold, titanium and coal grades; gems placed by Boundless's intent, power coils and machine power, gem gear, the titanium furnace; lava and elemental creatures.

**M19: Living worlds** (built 2026-10-25, `milestones/19-living-worlds.md`). Biomes, three rocks and soils, rivers, trees, lifted land, caves by tier.

**M20: Far views** (built 2026-10-26, `milestones/20-far-views.md`). The planet drawn to 1024 blocks in coarser voxel copies, as Boundless's LODs did.

**M21: Wandering worlds** (built 2026-10-26, `milestones/21-wandering-worlds.md`, ADR-0011). Boundless's exoworlds: temporary planets every two days for about five days, reached from a host's shrine, unclaimable, leaving with everything on them; tier 7 worlds with three astral ores, their creatures and gear.

**M22: Building blocks** (part 1 built 2026-10-26, `milestones/22-building-blocks.md`). Colour passes through crafting; Boundless's crafted building families (bricks, refined, carved and tiled rock, marble, planed and carved wood, metal and gem blocks, glass, lanterns); then doors and trapdoors, then furnishings.

*After that, still Boundless first (Nathan, 2026-10-19 and 2026-10-20): creatures and combat (bone, tallow and hide, then glue's real recipe), food and hunger, forging. Each gets its own spec and number when it starts.*

**Society.** Shop stands, request baskets, settlements and their ranking, guilds, mail.

**Life.** Creatures, combat, hunting, meteors, temporary worlds.

**Ownership.** Rentable worlds, world customisation, and the operations work for running it as a service.

## Player research

`research/boundless-forum-feedback.md` ranks what Boundless players liked, disliked and wished for, with ten candidate principles. Decided so far: no beacon fuel (claims lapse after 6 months away, nothing destroyed). Open: portal fuel (Nathan is more open to it than to beacon fuel), whether to move PvE earlier (the most-wished-for content), schematics that save a lapsed build so it can be placed again.

## A worldgen pass (Nathan, 2026-10-25)

**Now Milestone 19, Living Worlds** (`docs/milestones/19-living-worlds.md`): part 1 (biomes, rocks, soils, rivers) built 2026-10-25; parts 2 (trees, props) and 3 (vertical land, cave styles) follow. Old planets keep version 3 terrain.

"Eventually we'll need to do another worldgen pass to make planets more interesting overall, and the different tree types, different stone types, et cetera." Boundless's worlds have three rock kinds (igneous, sedimentary, metamorphic) each with its trace ore, three wood and leaf kinds, several soils (silty, clay, peaty), mud, gravel and sand kinds, biomes per world type, cave networks, floating islands, lower seas, and per-type surface features (the official world-builder configs in `monumentalco/boundless.worlds`, CC BY 4.0, hold the shapes: biomes, noise, caves, strata; tuning 4.13.4). It changes generated terrain, so it bumps `GENERATOR_VERSION`, and the server must keep the old generator for planets already made (as it keeps version 2): a spec decides that first.

## Later art passes

- **Three kinds of rock** (Nathan, 2026-10-22). Boundless has three rock kinds (igneous, sedimentary, metamorphic), on every world, each recoloured per world and each with its own trace ore (tuning 4: sulphur, saltpetre, olivine). For now Boundmore keeps one stone texture; when planet generation grows, that texture becomes **one** of the three, and the other two get their own textures and placement.

- **Larger textures** (Nathan asked, 2026-10-19). Boundless's block textures were 256x256 (a Wonderstruck developer on the forum, 2015, topic 2404; confirmed by players in 2020), smooth and painterly; ours are 32x32 by choice. Players coming from Boundless may expect more detail. Plan: make the procedural painters resolution-aware so the same recipes paint 64×64 with real added detail; offer a texture-quality setting (32 or 64). Above 64, hand-made or AI-made drop-in replacements fit better than procedural paint.
