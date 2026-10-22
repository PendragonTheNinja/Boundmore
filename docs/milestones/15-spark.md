# Milestone 15: Spark

*Spec written 2026-10-22. Nathan's direction: Boundless first; after creatures and meteorites, the powered machines (roadmap). Recipes, times, spark costs and spanner stats come from the game's own data as the community published it: Mayumi's `recipes.js` (decoded 2026-10-22), boundlessinfo's `item-attributes.json`, the wiki's Spark, Spark Generator, Spark Link, Machine and recipe pages (v244); tuning 4.6 to 4.8. Numbers marked **guess** are on no site.*

## Goal

Machines past the workbench run on **spark**. You burn fuel in a **spark generator**, run **spark links** from it to each machine, and the **mixer**, **refinery** and **compactor** draw spark for their recipes. Machines **wear** as they craft and stop when worn out until you **repair them with a spanner**. The **iron furnace** halves the heat a smelt needs. **Silver** appears on tier 3 worlds and leads to **silver alloy**, the first alloy, which the refinery and compactor are built from. The metal chain runs bar, compact, refined, machined, as in Boundless.

**Power coils wait for gems**: every coil needs a compact gem, and no world has gems yet. Recipes that need power are left out until then.

## How Boundless did it

### Spark

- A **spark generator** (Boundless's spark core) burns solid fuel into spark and stores it. Generators joined by a face pool their spark into one store with one window. Breaking one that holds spark drops the spark as an item. [WIKI Spark Generator]
- Fuel gives spark over a burn time (tuning 4.7): stick 7 over 1 s, timber 15 over 3.5 s, peaty soil 20 over 7 s, trunk 30 over 7 s, soft coal 100 over 10 s. [MAY `fuelData`]
- **Spark links** carry spark from a generator to machines. Every machine needs its own path of links to a generator; machines never pass spark on. Spark goes through at most **100 links**. [WIKI Spark Link]
- A recipe costs its spark when it runs. [MAY]

### Machines and wear

- The **mixer**, **refinery** and **compactor** are four-block machines like the workbench. [MAY recipes make 4]
- Machines **wear** as they craft: each recipe has a wear cost (cog 6, silver alloy 12, cotton cloth 35, a machine 29). At 0% the queue stops until the machine is repaired. [WIKI Machine, recipe pages]
- A **spanner** swung at a machine removes wear: stone 100 a hit, copper 400, iron 700. [BI]
- The **iron furnace** is a crucible on a base, like the stone one, with heat multiplier 0.5. [WIKI Furnace]

### Recipes (single craft) [MAY]

| Makes | At | Inputs | Time | Spark |
|---|---|---|---|---|
| spark generator | workbench | 1 iron furnace base, 3 copper or iron bars, 3 relic components, 15 cogs | 450 s | |
| 10 spark links | workbench | 6 copper or iron bars, 1 relic fragment | 60 s | |
| cog | workbench | 1 copper bar | 30 s | |
| iron furnace base | workbench | 6 iron bars, 4 flint | 90 s | |
| iron furnace crucible | workbench | 6 iron bars | 90 s | |
| 4 mixer blocks | workbench | 10 stones, 3 copper or iron bars, 6 relic fragments | 450 s | |
| 4 refinery blocks | workbench | 10 stones, 3 silver alloy, 6 relic fragments, 1 relic component | 450 s | 400 |
| 4 compactor blocks | workbench | 10 stones, 3 silver alloy, 6 relic fragments, 1 relic component | 450 s | 400 |
| silver bar | furnace | 1 silver ore, heat 300 | 120 s | |
| silver alloy | mixer | 1 silver bar, 1 iron bar | 40 s | 100 |
| compact copper / iron / silver alloy | compactor | 10 bars or alloy | 50 / 60 / 100 s | 100 / 100 / 400 |
| refined copper / iron / silver alloy | refinery | 1 compact | 20 / 30 / 40 s | 0 / 0 / 400 |
| machined copper / iron / silver alloy | workbench | 1 refined | 20 / 30 / 40 s | 200 / 200 / 400 |
| cotton cloth | refinery | 2 sackcloth | 60 s | |
| stone spanner | crafting table | 4 stones, 4 sticks, 4 foliage | 10 s | |
| copper spanner | workbench | 3 copper bars, 4 sticks, 1 sackcloth, 1 glue, 2 cogs | 60 s | |
| iron spanner | workbench | 3 iron bars, 4 sticks, 1 sackcloth, 1 glue, 2 cogs | 120 s | |

### Ores

- **Silver**: tier 3 and up, 10 to 40 blocks above the world floor (densest 20 to 30), 20 to 70 below the surface (densest 30 to 60). [tuning 4.3]
- **Relic components** (Boundless's ancient tech component): every tier, 40+ above the floor, 5+ below the surface; rarer than fragments. [tuning 4.3]

## Rules

1. **Generators** (`spark_generator`, a single block, a station with a fuel slot): a stack of fuel in the slot burns one item at a time, adding its spark over its burn time (as heat burns in a furnace). Each generator block stores up to **5,000 spark** (**guess**); face-joined generators share one store and one window (the group's first block keys it, as four-block machines are keyed). Breaking a generator with spark refunds it as `spark` items of 1,000 (rounded down; Boundless's "Unused Spark").
2. **Links** (`spark_link`, a solid block): a machine is **on the grid** when one of its blocks touches a link face to face, and that link reaches a generator block through at most 100 links touching face to face. A machine's spark comes from every generator group its links reach, fullest first.
3. **Spark costs**: a recipe with spark starts only when the grid can pay it all, and takes it at the start. Off the grid or short of spark, the machine waits and says why ("No spark: link this machine to a spark generator", "Not enough spark (needs 400)").
4. **Machines and recipes** as in the table. The mixer, refinery and compactor are four-block machines; the iron furnace is a crucible on a base and its heat costs are halved (multiplier 0.5).
5. **Wear**: the workbench, extractor, mixer, refinery and compactor wear by each recipe's wear cost (from the wiki where it has one, else **guess**, `assets/recipes.toml` `wear`) and hold **10,000** (**guess**). At 0 the queue stops ("Worn out: repair it with a spanner"). The crafting table and furnaces do not wear (Boundless's did not). The machine window shows its wear.
6. **Spanners** (`stone_spanner`, `copper_spanner`, `iron_spanner`): swinging one at any block of a machine removes its wear removed per hit (100, 400, 700, plus Spanner Mastery's +10% a rank), at its action speed, with its energy and durability (Boundless's stats, `assets/items.toml`); a spanner does no damage to blocks.
7. **Silver** (`silver_seam`, on tier 3+ planets) and **relic components** (`relic_cache`, every planet) generate in chunks nobody has saved, appended to each kind's ore list as wayshards were; `GENERATOR_VERSION` does not change.
8. **Saves**: stations gain wear and stored spark; `SAVE_VERSION` 8 with the upgrade check from M14.

## Acceptance

- A generator burns coal into spark; joined generators show one store; breaking one gives spark items.
- Links carry spark to a mixer; without links, or past 100 links, the mixer waits and says why.
- Silver ore from a lush world smelts to bars; a mixer makes silver alloy with spark; refinery and compactor are built from it; the compact, refined, machined chain works.
- The iron furnace smelts copper for 50 heat.
- Machines wear by recipe and stop at 0; spanners repair them by their stats.
- `--demo-ui spark` shows a generator, links and a running mixer; screenshots.
- Tests for each rule; end-to-end: fuel to spark to an alloy; wear to repair; the upgrade check.

## Non-goals

- Power coils, advanced coils and recipes that need power (with gems).
- Gold, titanium, gems and their alloys; silver tools and spanners (they need bonding agents, which need essences).
- Generator wear from fuel; the centraforge (forging); the chrysominter; refined rock and wood blocks; spark item use beyond refunds.

## Status

*Built 2026-10-22; awaiting Nathan's play-test.* Protocol 21, `SAVE_VERSION` 8 (version 7 universes open with every station fully repaired and no spark).

- [x] Data and core: blocks, items, recipes (spark, wear), seams; `Station` kinds; spark rules in `bm-core/src/spark.rs`
- [x] Protocol 21: `StationView.wear` and `.spark`, `ServerMsg::MachineWear`
- [x] Server: generators (fuel slot, even burn, joined groups, spark refund on break), links (`grid_generators`, 100 limit), spark costs with wait reasons, wear, spanners, iron furnace heat, saves (version 8)
- [x] Worldgen: silver and relic components appended (generator 3 unchanged)
- [x] Client and art: textures and icons, machine window wear and spark, generator window, recipe spark costs, wear bar after spanner hits, spanner tooltips, `--demo-ui spark`
- [x] Verified: 568 tests, upgrade check from version 7 (log: "upgraded the universe from save version 7 to 8"), load test (50 bots, worst tick 4.7 ms); screenshots `docs/media/m15-spark.png`, `m15-machines.png`, `m15-wear-bar.png`

Open (for Nathan):
- **Breaking a machine resets its wear**: picking a machine up and placing it again restores full wear. Boundless's rule is not found; keeping wear on the item would be a format change.
- Joining two generator groups that both hold fuel is refused ("take the fuel out of one first").
- The generator spark refund is unit-tested only (1,000 spark takes about 100 s of coal).
- The tooltip shows a spanner's base repair, not the Spanner Mastery figure.

Measure in game (Nathan): how much spark one generator block holds; how many spanner hits a fully worn machine took; whether a machine kept its wear when picked up.
