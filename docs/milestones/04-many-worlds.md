# Milestone 4: Many Worlds

*Spec written 2026-10-18. In progress.*

## Goal

One server, several different planets: each with its own climate, look and resources, visible in each other's skies, and a way to travel between them. Show that idle planets cost next to nothing, so one rented machine can host a whole universe (ADR-0006).

## Acceptance criteria

1. **A universe per server.** The server hosts the planets in its config (default four, one of each kind), in one process and one save file. New players start on the tier 1 planet.
2. **Planet kinds** (`assets/planets.toml`): temperate (tier 1), arid (2), frozen (2), lush (3). They differ in terrain (mountain height, trees, sand, snow and ice, sandstone layers), in ore (iron only on tier 2 and up, richer on tier 3; coal and copper everywhere), and in look (sky colours, block tints). `GENERATOR_VERSION` is bumped and the fingerprint re-pinned.
3. **Iron.** Iron veins, iron ore, iron ingots (furnace), iron hammer, shovel and axe (faster and longer-lasting than copper). Sandstone crafted from sand.
4. **Warp shrines and travel.** Every planet has a warp shrine at its spawn, placed by the server, unbreakable, and protected like a claim. Interacting with it (E) opens a travel menu listing every planet: name, kind, tier, players there. Choosing one takes you to that planet's shrine. Travel is free for now (see decisions).
5. **Arriving.** The client shows "Travelling to <planet>" until the ground around you has arrived, then plays as before: prediction, light, textures, claims of that planet. No stale chunks, players or claims carry over.
6. **Planets in the sky.** The other planets of the universe hang in the sky, in their colours, sized by width, fixed to the planet's sky (they rise and set with the stars).
7. **Saves.** A `SAVE_VERSION` 2 planet opens as the first planet of a new universe with everything kept (builds, accounts, inventories, claims, stations). Each player is saved with the planet they are on.
8. **Cost.** With four planets and 50 bots spread over two of them, the 30-second report shows no slow ticks, and an empty planet holds no chunks. The report shows players and resident chunks per planet.

## Shared contract (built first)

- `bm-core`: `PlanetLook` (`look.rs`).
- `bm-worldgen`: `PlanetKind`, `PlanetKinds` (`kinds.rs`); `PlanetGenConfig` gains `#[serde(default)]` and `check`.
- `bm-protocol` v7: `PlanetInfo`, `PlanetSummary`, `Travel`, `ServerMsg::Universe`, `ClientMsg::Warp`, `planet` on snapshots; `Welcome` carries `planet: PlanetInfo` and `universe`.
- Data: `assets/planets.toml`; new blocks at the end of `blocks.toml` (iron_vein, sandstone, ice, warp_shrine); iron items and recipes.

## Plan and who does what

| Part | Owner |
|---|---|
| Spec, ADR-0006, contract, data | main session |
| Planet kinds in generation, iron, ice, sandstone, `GENERATOR_VERSION` | agent (bm-worldgen) |
| Universe, save v3, shrines, travel, per-planet report, bots across planets | agent (bm-server) |
| Travel screen and menu, per-planet sky and tints, planets in the sky | agent (bm-client) |
| Textures: iron vein, sandstone, ice, warp shrine, iron items | agent (bm-texgen) |
| Integration, headless play-test, docs | main session |

## Non-goals

Player-built portals and portal fuel (Nathan wants to discuss them), block colours carried by items, temporary worlds, more than four kinds, creatures, death and respawn, parallel planet stepping.

## Decisions for Nathan

- **Travel cost.** Free shrine travel for now. Boundless charged for warps; the forum research disliked footfall-driven warp costs but liked travel meaning something. Decide with portals.
- **Player-built portals and their fuel.** Next to design together.

## Status

- [ ] Contract, ADR, spec
- [ ] Worldgen (agent)
- [ ] Server (agent)
- [ ] Client (agent)
- [ ] Art (agent)
- [ ] Integrated, play-tested headlessly
- [ ] Nathan's play-test
