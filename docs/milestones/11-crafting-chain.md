# Milestone 11: The Crafting Chain

*Spec written 2026-10-20. Decided by Nathan the same day: energy first (M10, built), then this; glue is **2 sap** until creatures bring bone; no tier 4 world is needed (fibrous leaves come from plants on every tier); "the plants are really important".*

## Goal

The road from bare ground to iron tools works the way it did in Boundless. You gather materials:
- sticks and timber from trunks;
- stones from rock;
- flint from boulders;
- fibrous leaves from desert plants;
- sap from trees;
- old relic fragments from a seam.

You craft them at Boundless's machines:
- the crafting table;
- the four-block workbench and extractor;
- the stone furnace (a crucible on a base), which burns fuel for heat.

And you use Boundless's recipes, item for item. Every world is scattered with Boundless's kinds of **gatherable objects**, as models rather than blocks: plants and flowers, boulders, and fungi in caves. Each drops what Boundless's did.

Numbers come from `docs/research/boundless-tuning.md` 4.4 to 4.10 and the decoded recipe data (mayumi `recipes.msgpack`, decoded 2026-10-20) unless marked **guess**. Names are Boundmore's own; the right-hand columns say which Boundless thing each one copies.

## What Boundless did

- **Hand and crafting table.** A few recipes are made in the hand: timber, sticks, the crafting table, wood tools and the basic torch. Everything else needs a machine.
  - The crafting table is a single block made from 10 timber, and it crafts over time. Stones take 10 s, stone tools 10 s, a furnace part 15 s.
- **Four-block machines.** The workbench and the extractor (and later the refinery, mixer, compactor and forge) only work when **4 blocks of the same machine touch by their faces**, in any tetromino shape.
  - One recipe makes 4 blocks.
  - Opening any block of the group opens the machine.
- **The furnace** is two blocks: a base with a crucible on top. Either alone is decoration.
  - Recipes use **heat** (copper bar 100, iron bar 150, glass 100, glue 100), drawn from fuel in the furnace's fuel slot.
  - Fuel's value per item: soft coal 100, trunk 30, peaty soil 20, timber 15, stick 7. The stone furnace's heat multiplier is 1.0.
- **Recipes up to iron**, single craft:

  | Makes | At | Inputs | Time |
  |---|---|---|---|
  | 2 timber | hand or table | 1 trunk | instant |
  | 4 sticks | hand or table | 1 trunk | instant |
  | crafting table | hand or table | 10 timber | instant |
  | 2 stones | table | 1 rock | 10 s |
  | wood tool (each) | hand or table | 4 timber, 4 sticks, 4 foliage | instant |
  | stone tool or chisel (each) | table | 4 stones, 4 sticks, 4 foliage | 10 s |
  | 4 torches | hand or table | 1 stick, 1 flint | instant |
  | furnace base | table | 10 stones, 1 flint | 15 s |
  | furnace crucible | table | 10 stones | 15 s |
  | 4 workbench blocks | table | 10 timber, 3 copper or iron bars, 1 relic fragment | 60 s |
  | 4 extractor blocks | workbench | 10 stones, 3 copper or iron bars, 3 relic fragments | 180 s |
  | copper bar | furnace | 1 copper ore, heat 100 | 30 s |
  | iron bar | furnace | 1 iron ore, heat 150 | 60 s |
  | glass | furnace | 2 sand, 2 soil, heat 100 | 30 s |
  | glue | furnace | **2 sap** (Boundless: 1 sap + 1 bone), heat 100 | 20 s |
  | sackcloth | extractor | 2 fibrous leaves | 30 s |
  | copper tool or chisel (each) | workbench | 3 copper bars, 4 sticks, 1 sackcloth, 1 glue | 60 s |
  | iron tool or chisel (each) | workbench | 3 iron bars, 4 sticks, 1 sackcloth, 1 glue | 120 s |
  | copper grapple | workbench | 3 copper bars, 1 sackcloth, 1 glue, 3 relic fragments | 60 s |
  | iron grapple | workbench | 3 iron bars, 1 sackcloth, 1 glue, 3 relic fragments | 120 s |
  | storage chest | table | 6 timber, 1 glue | 60 s |

- **The relic seam** (Boundless's ancient tech remnant): every tier, from 20 blocks above the floor up, at least 3 blocks below the surface. It yields like copper (+0.5 bonus on tiers 1 and 2).
- **Sap**: trunks drop it 8% (tiers 1 to 4) or 12% (5+).
- **Gatherable objects** (tuning 4.4a): health 800 times the tier, never armoured; they break with any tool (axe for plants and fungi, hammer for boulders) and drop:

  | Boundmore | Copies | Drops (expected each) |
  |---|---|---|
  | spikeleaf | desert spike plant (a yucca) | fibrous leaf 1.3, spicy bean 0.17, tuber 1.3 |
  | hornroot | Trumpet Root | spicy bean 1.3, sweet bean 0.15, bitter bean 0.02, waxy tuber 1.3 |
  | stiltfrond | Traveller's Perch | fleshy leaf 1.3, fibrous leaf 0.15, inky leaf 0.05, exotic tuber 1.3 |
  | nightlily | a lotus | inky leaf 1.3, petal 0.05, tuber 1.3 |
  | ridge cactus | a cactus | petal 1.4, exotic tuber 1.3 |
  | knot aloe | an aloe | fleshy leaf 1.35, waxy tuber 1.3 |
  | frostbell | a snowdrop | bitter bean 1.2, petal 0.3, tuber 1.3 |
  | curlfern | a fiddlehead | sweet bean 1.3, fleshy leaf 0.12, tuber 1.3 |
  | boulder | basic boulder | flint 1.3, opal 0.55 |
  | stalk boulder | beanstalk boulder | rock salt 1.3, olivine 1.35 |
  | stacked boulder | boulder tower | flint 1.3, sulphur 1.4, opal 0.2 |
  | ring boulder | boulder ring | rock salt 1.3, cobalt 1.4 |
  | chip boulder | boulder chip | flint 1.3, cobalt 1.3, opal 0.9 |
  | tapered boulder | tapered boulder | rock salt 1.3, saltpetre 1.4 |
  | tarspot cap | mottled tar spot fungus | reactive lamella 1.25, vital orb 0.4 |
  | tongue bracket | clustered tongue fungus | edible lamella 1.5, pulsating orb 1.75 |
  | branch coral | branch funnel fungus | reactive lamella 1.5, vital orb 1.4 |
  | burst cap | tinted-burst fungus | edible lamella 1.25, pulsating orb 0.6 |
  | weeping cap | weeping waxcap fungus | edible lamella 1.25, shadow orb 0.6 |
  | glowcap | glow cap fungus | glowing lamella 1.25, shimmering orb 0.6 (gives light 6, **guess**) |

  Seeds (Boundless's spooky seed and the like) wait for farming.

## Acceptance

1. **Gathering objects grow.** Plants and boulders are scattered on every planet's surface, fungi in caves, as models (props standing on the floor, not solid, no AO). Where each grows is **a guess** (Boundless's placement data is not published):
   - spikeleaf on sand and dry ground, plentiful on arid worlds and present on the starter world's beaches and dry patches, so fibrous leaves are within reach of a new player as Boundless's home deserts made them;
   - frostbell on snow;
   - nightlily by water;
   - the rest on turf;
   - boulders on stone and turf everywhere;
   - fungi on cave floors.
   
   Each planet kind lists its flora in `planets.toml` (block, ground, density, depth). Existing planets gain flora in chunks nobody has saved yet, as wayshards were added in M8; `GENERATOR_VERSION` does not change.
2. **Breaking them** gives their drops (expected counts above, rolled by the server), at the Health category `plant`: 800 times the tier, ignoring armour.
3. **The crafting table** (10 timber, by hand) is a station that crafts over time. Opening crafting in the hand shows only hand recipes; at a table, table recipes too.
4. **Four-block machines.** Workbench and extractor recipes make 4 blocks. A machine works only when 4 of its blocks touch by faces. Three alone say so ("A workbench needs 4 blocks touching"). Its queue and output belong to the group: breaking any of its blocks is refused while it has contents.
5. **The furnace** is a crucible on a base: the base alone or the crucible alone does nothing ("A furnace needs its crucible on top"). It has a fuel slot. A recipe starts when there is heat for it, burning fuel items as needed (a soft coal gives 100 heat).
6. **Recipes** as in the table. Existing machines are upgraded in place:
   - a placed `workbench` block is one workbench block;
   - a placed `furnace` is a base.
   
   Both need their partners before they work again (noted in the play-test notes).
7. **The relic seam** generates on new planets and in unsaved chunks of existing ones. Its fragments drop with the usual bonus.
8. **Sap** drops from trunks.
9. **Saves**: new blocks and items are appended; station contents keep their format or are versioned with an upgrade path. The upgrade check from `main` passes.
10. **Tests** for each rule; screenshots of plants, boulders and fungi, the four-block workbench and the furnace.

## Shared contract (built first, by the main session)

- **bm-core**: `Station::CraftingTable`, `Station::Extractor` (appended; serde names `crafting_table`, `extractor`); `Assembly { Four, FurnaceBase, FurnaceCrucible }` and `BlockDef::assembly`; `Model { Torch, Plant, Boulder, Fungus }` and `BlockDef::model`; `Health::Plant` (800, never armoured: `mining`); `BlockDef::fuel` and `ItemDef::fuel` (heat when burnt); `Recipe::heat`.
- **Data**: every block, item and recipe above, in `assets/`. Flora are floor props with `drops = ""` and their drops as `bonus`; they are not placeable.
- **bm-protocol 15**:
  - `StationView` gains `idle` (why it does not work), `fuel` and `heat`; its `pos` is the machine's first block.
  - New `ClientMsg::StationFuel { pos, slot, count, into }`.

## Plan and who does what

| Part | Owner |
|---|---|
| Spec, contract, data | main session |
| Server: four-block machines and furnaces (assembly checks, a group's one queue), the crafting table, fuel and heat, `StationFuel`, saves for fuel and heat, the crafting tests | agent (bm-server) |
| Worldgen: flora placement (`planets.toml` lists per kind), the relic seam in every kind's ores, appended to existing planets as wayshards were | agent (bm-worldgen, planets.toml) |
| Client and art: plant, boulder and fungus models (bm-mesh props), textures and icons (bm-texgen), the crafting table and machine screens, the furnace fuel slot and heat, hand crafting showing hand recipes | agent (bm-client, bm-mesh, bm-texgen) |
| Integration, headless play-test, docs | main session |

## Non-goals

Spark, power coils, machine wear and spanners (a later machines milestone); the refinery, mixer, compactor and forge; bone and the real glue recipe (creatures); food and farming (the beans, tubers and petals are kept for later); seeds; the gathering epic's dig-up-and-replant; the iron and titanium furnaces; Boundless's other chisel materials.

## Status

*Built 2026-10-20 on branch `claude/amazing-pascal-lwttor`; awaiting merge and Nathan's play-test.*

- [x] Spec, contract and data (protocol 15)
- [x] Server (`crates/bm-server/src/machines.rs`, `game/stations.rs`):
  - The crafting table queues its own recipes. Hand recipes stay instant everywhere, a table included; Boundless queued them at a table.
  - Four-block workbenches and extractors keep one record at the group's first block. A fifth touching block is refused ("Only 4 Workbench blocks can touch"); idle messages explain an incomplete machine.
  - The furnace is a crucible on its base, with one fuel slot. Heat is drawn as each craft starts, and a craft waits without fuel. `StationFuel` moves fuel in and out.
  - Fuel, jobs or output stop a furnace being broken; leftover heat alone does not (it rarely matches a recipe, so it would never be breakable), and it is lost when the furnace breaks.
  - A plant falls with its ground and gives its drops to the breaker (a guess).
  - `SAVE_VERSION` 4: a version 3 universe upgrades its station rows on opening (checked against one made by `main`).
- [x] Worldgen (`bm-worldgen` `FloraSpec`, `planets.toml`):
  - The relic seam is last in every kind's ores.
  - Each kind has a flora list: spikeleaf on sand and in dry patches of the starter world, the other plants on turf by kind, frostbell on snow, nightlily by water, boulders everywhere, and six fungi on cave floors with the glowcap deepest.
  - Placement is deterministic, never in water or solids, and a prop's ground is in its own chunk.
  - Existing planets gain the seam and flora in chunks never saved ("flora added"); `GENERATOR_VERSION` unchanged; kind pins re-pinned, with the M8 pins kept and a test that old settings still give them.
  - Starter world: about 20 plants and 5 boulders a minute's walk.
- [x] Client and art:
  - Plant models are crossed quads with cut-out textures; boulders are irregular rocks, varied per cell; fungi are a stem and cap.
  - Textures for all 20 objects, the three machines and the seam; icons for all 28 new items.
  - Crafting screens: hand recipes only in the hand, table recipes at a table, idle messages, and the furnace's fuel slot, heat and heat per recipe.
  - `--demo-ui table`, `furnace`, `extractor` and `flora`; screenshots `docs/media/m11-flora.png`, `docs/media/m11-furnace.png`.
- [x] Integrated: 402 tests, fmt and clippy clean; a live run against a real server showed the generated world with boulders, the starter hand stone, and no warnings or slow ticks.
- [ ] Nathan's play-test. To judge:
  - Where things grow and how many: all guesses, tunable in `planets.toml`.
  - Machine and furnace behaviour against Boundless.
  - Dragging fuel with a real mouse.
- Follow-ups:
  - Art: knot aloe looks thin in game; hornroot's flowers are boxy up close.
  - Existing single workbench and furnace blocks need their partners (3 more workbench blocks; a crucible on the furnace) before they work again.
  - Spark, coils, wear and spanners; the refinery, mixer, compactor and forge; bone and the real glue recipe; food and seeds: later milestones.
