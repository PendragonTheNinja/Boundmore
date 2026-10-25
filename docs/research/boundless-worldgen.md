# Boundless world generation (research for the worldgen pass)

Research for the roadmap's "A worldgen pass" (Nathan, 2026-10-25). Facts and numbers for a reimplementation: no Boundless names, text, art or code go into Boundmore. File names below are cited only as sources.

**Source:** the official Boundless World Builder node configs, `github.com/monumentalco/boundless.worlds` (last commit 2021-01-22), © Wonderstruck and community contributors, **CC BY 4.0** (attribution: "Boundless World Builder configs, Wonderstruck and contributors, CC BY 4.0"). Paths are relative to its `config/`. Read with a small resolver that follows `reference` nodes and applies their `links` overrides, and a decoder for prefab block data (run-length triples `[id lo, id hi, run-1]`, trailing air omitted; ids matched against pfiffel.com/bl/blocks `dataBlockStats.js`; decoded non-air counts equal each prefab's `nonAirCount`, so the decoding is right). Scripts stayed in the sandbox scratchpad.

**Confidence:** *high* = read directly from a config value; *medium* = inferred from several configs agreeing; *low* = inferred from one case or from node names. The World Builder's own manual is not in the repo (its `docs/biomes`, `docs/noise2d` folders are empty), and forum guides (t/46421 2020, t/3691 2016, t/4627, t/4375) explain nodes visually but give no units.

**What the repo does not hold:** ores, gems, fossils, the gatherable plants, boulders and fungi, creatures and atmosphere. Each world config names a server-side `profile` (e.g. `worlds/release/lush_world_0.json` → `lush_world_profile_tier_0`) that is not published. The block palette in every biome lists the plant, boulder and fungus kinds (`PLANT_*`, `ROCK_BOULDER`…, `FUNGUS_*`) but no placement rule in the repo places them (high). Where they grow stays an open question (tuning 4.4a).

---

## 1. How a world is built

Every config is a node graph. Top-level node kinds (`@configType`): WORLD, BIOME, STRATA, SUBSTRATE, DECORATIONS, INSTANCE, NOISE2D, NOISE3D, WORLDCOLORS.

| Stage | Node | What it does | Main parameters (typical) | Source | Conf. |
|---|---|---|---|---|---|
| World | WORLD | size, seas, biome selection, colour recipe | `worldSize` 288 chunks of 16 (4608 m; some T6-T8 192); `sea` level 64; `lowerSea` level 16; `biomeSize` 30; `palette` → worldcolors | `worlds/release/*.json` | high |
| Climate | `generator.distribution` | four 2D fields: altitude, temperature, humidity, vegetation | each a Voronoi-cell field (cell value from a perlin sample, edges blurred by interpolation and distortion); 1 in 12 worlds per field gets straight triangle-wave bands along X or Z instead | `noise2d/misc/attribute_distribution.json` (weights 1, 0.1, 0.1) | medium |
| Biome choice | `select` / `search` | picks the world's biome list from tier sets filtered by labels | see 3.1 | `biomes/release/biome_sets/`, `filters/` | high (lists), medium (exact pick rule) |
| Biome placement | biome climate values | each biome has target altitude, temperature, humidity, vegetation (0..1) and a weight (0.5..1); a place takes the biome nearest its climate values, blended at edges | e.g. grassland alt 0.41 temp 0.45 hum 0.60 veg 0.60 w 0.7 | `biomes/release/tier 1/grassland.json` | medium; dev lucadeltodecso (t/381, 2015): biomes are generated as an immediate blend, not separately |
| Terrain | `part.heightmap` (2D) | column height | value v maps to height y = 128 (v + 1): v = -0.5 is sea level 64 | inferred: flat grassland floors at v = -0.49 (y 65), the plateau biome is the constant v = 0.012 (y 130), mountains reach v ≈ 1.5 (clamped at 256) | medium |
| Terrain | `part.threeD` (3D) | overhangs, arches, floating islands | `threeDMinY/MaxY`, `threeDSurfaceMinY/MaxY` (band relative to the surface), `threeDThresholdBias` | 33 biomes; see 2.3 | high (params), low (exact semantics) |
| Terrain | `part.heightmapBlockType` | the biome's solid fill | one rock kind per biome: igneous 111, metamorphic 103, sedimentary 101 of 339; AIR in 24 (floating-only biomes) | all biome files | high |
| Rivers | `noise2d/watersystems/water_system_1.json` | `min(heightmap, rivers)` carves winding channels | used by 292 of 363 biomes; 6 river variants picked by weight 0.48/0.24/0.24/0.08… | biome heightmaps | high |
| Strata | `part.strata` | rock bands through the fill | see 4 | `strata/release/strata.json` (all biomes) | high (values), medium (stacking) |
| Surface | `surface_substrate` | top layers by height and slope | grass-over-soil, sand, gravel, ash, ice…; `averageDepth` 3 ± 3; `maxGradient` 1.25 for soils (steeper shows bare rock) | see 3.3 | high |
| Caves | `part.caves` (3D noise) + `caves_substrate` | cave carving and cave linings | per-tier cave sets; carving held under a 2D ceiling about 19 blocks below the surface | see 6 | high (sets), medium (ceiling) |
| Decorations | `surface_decorations`, `caves_decorations`, `threed_decorations` | trees, bushes, props, single flower blocks, liquid pockets | per rule: instance, `placementType`, `density` (-1..1), `threshold`, `constraintType` (ground blocks), `minY/maxY`, `min/maxGradient`, `min/maxSize`, `clearance` (8), `depth` | see 5 | high (values), low (density units) |
| Colour | WORLDCOLORS | picks palette indices per block family | hue/sat/lum offsets per family root (vegetation, solid, liquid, mineral, sand) | `docs/worldcolors.md`, `worldcolors/release/*.json` | high |

Node vocabulary (2D and 3D): perlin, simplex, worley, voronoi, cell_distance, cell_samples, gabor, wave (sine/triangle along an axis), grid, hexagonal, triangular, image; combinators sum (terms with multiplier and power), min, max, product, lerp, remap (min/max, clamp), absolute, distort, fractal (octaves), interpolated, layered (3D by height), shifting, constant. A parameter given as `{min, max}` is drawn per world from the seed (high: e.g. `frequency {0.2, 0.5}`). Frequencies have no stated unit (low: smaller is larger features).

`join` and `select`: a `join` with `count` below its number of components draws `count` of them by weight (`w`), e.g. cave linings `join count 1` over weights 0.9 / 0.1; a `join` whose count equals its components keeps all. `select` picks biomes; `search` collects biomes by labels (high for what each holds; medium for the draw rule).

## 2. Terrain shape

### 2.1 Seas per world type (high; `worlds/release/<type>_world_<tier-1>.json`)

| Type | Tiers | Sea (64 unless noted) | Lower sea (16) | Colour recipe | Biome filter (labels a biome needs, any of) |
|---|---|---|---|---|---|
| Lush | 1-4 | water | water | lush-0..3 | T1-2: no ice, no hazard blocks (`starter_filter`: excludes ice, tangle, sponge, mould, growth, thorns, corruption, dark glass, light crystal); T3-4: any |
| Metal | 3-7 | **lava** | lava | metal-2..5 | any |
| Coal | 3-7 | water | lava | coal-2..5 | any |
| Corrosive | 5-7 | water | lava | gem-4/5 | lush, swamp, islands, liquid |
| Toxic | 5-7 | water | lava | gem-4/5 | lush, swamp, liquid |
| Shock | 5-7 | water | lava | gem-4/5 | ice, wasteland, desert, mountains, hills |
| Chill | 5-7 | water | **water** | gem-4/5 | ice, lush, cool, cold |
| Blast | 5-7 | water at **32** | lava | gem-4/5 | desert, wasteland, rocky, mountains |
| Burn | 5-7 | **lava** | lava | gem-4/5 | desert, wasteland, rocky, crater; no ice |
| Umbris (dark) | 7-8 | lava | water | special-umbris | any |
| Rift, Blink | 7-8 | water | lava | special-rift/blink | any |

Lush T1-2 worlds also add 2 sea biomes from `biome_sets/water.json` (12 lakes and oceans from every tier).

### 2.2 How tiers mix (high; same files)

A world draws biomes from every biome-tier set, weighted to one home set: count 16 from the home set, 8 from each neighbour, 4 two away, 2 three away, 1 beyond. The home set is **biome folder N for a world of tier N+1** (T3 to T8), and folder 1 for T1 and T2 worlds: e.g. a T5 gem world (`blast_world_4.json`) draws folder 1 ×2, 2 ×4, 3 ×8, **4 ×16**, 5 ×8, 6 ×4; a T1 world (`lush_world_0.json`) folder 1 ×16, 2 ×8, 3 ×4 plus 2 sea biomes; a T2 world (`lush_world_1.json`) 1 ×14, 2 ×8, 3 ×6. So "tier N" biome folders below mean that set, one world tier lower. The top-level `select` then keeps **15** biomes for the world (medium: `count 15`). So every world has some gentler and some wilder biomes, mostly its own. Exoworlds: 10 normal biomes plus 5 from a rare-liquid set (section 7).

Biome sizes: `biomeSize` 30 on worlds (24 in biome files' test settings). If its unit is chunks, a biome cell is about 480 blocks across, so a 4608 m world holds roughly 90 cells of its 15 biomes (low: unit not stated).

### 2.3 Shapes by biome label (high for labels and Y bands; medium for heights)

Labels per biome (`labels` in each biome file; 363 files): lush 173, warm 138, flat 123, cool 112, liquid 91, forest 90, hills 89, mountains 71, rocky 62, wasteland 52, hot 50, crater 50, desert 46, cold 39, floating 34, bushland 33, grassland 32, swamp 22, ruins 17, sea 11, islands 9, lake 4.

| Shape | Heights (y = 128 (v+1)) | Example source | Conf. |
|---|---|---|---|
| Flat land | floor v ≈ -0.49 (y 65), low rolling hills on top (≤ 0.12 v, ~15 blocks), rivers cut through | `tier 1/grassland.json` | medium |
| Low mountains | floor y ≈ 68, peaks from a 0.47..0.6 × noise term | `tier 1/low_mountains.json` | medium |
| Mountains | 1.27 × noise remapped -0.38..1.2, plus bumps: y 65 to the 256 ceiling | `tier 3/mountains.json` | medium |
| Plateau | flat table at v 0.012 (y ≈ 130), only rivers cut it | `tier 1/plateau.json` | medium |
| Sea, lakes | heightmap below v -0.5 with a sea-bed substrate (rock, sand, gravel; sand band at y 59-70) | `tier 1/deep_sea.json`, `substrate/sea_bed.json` | high (substrate) |
| Raised plateau (3D) | 3D band y 98-154 | `tier 4/raised_plateau.json` | high |
| Floating islands (3D) | bands y 105-148, 110-160, 116-148, 90-131, 181-219 (sky layer); whole-column chaos -64..256 | `tier 4/snake_islands`, `tier 5/alien_invasion`, `tier 6/circle_islands`, `tier 5/island_trees`, `tier 7/sky_pattern`, `tier 6/floating_chaos` | high |
| Floating-only biomes | `heightmapBlockType` AIR: no ground, only 3D land (sea fills below) | `tier 3/round floating islands`, `tier 4/icebergs`, `tier 6/floating_chaos`, `tier 7/stone_balls` | high |
| Overhang cliffs | 3D surface band from -64 below to above the surface | `tier 5/forest_cliffs` (y 0-160) | high |

3D noise by biome folder (biomes using it / biomes): T1 0/40, T2 2/38, T3 4/40, T4 5/40, T5 6/36, T6 6/33, T7 9/30; "floating" labels: T4 2, T5 3, T6 5, T7 10 (high; `biomes/release/tier N/`). Higher tiers are steadily more vertical and broken.

## 3. Biomes

### 3.1 Lists per tier (high; full table in the appendix)

`biomes/release/tier 1` to `tier 7` hold 40, 38, 40, 40, 36, 33, 30 biomes. Element labels show which world family a biome was drawn for (T1-3: lush, metal, coal; T4-5: the six gem types; T6-7: rift, blink, umbris), but the filters choose by shape labels, not element (high). `rare_liquid_oil/` (38) and `rare_liquid_resin/` (42) are copies of tier biomes with a pocket rule added, labelled `exo_oil` / `exo_resin`.

| Tier | Mostly | Base rock (ign / met / sed / air) | Notable |
|---|---|---|---|
| 1 | flat 25, hills 14, forest 13, grassland 12, liquid 12 | 14 / 10 / 16 / 0 | no 3D; forests, meadows, lakes, a few deserts and wastelands |
| 2 | flat 21, forest 16, hills 12, desert 8, cold 8 | 9 / 16 / 12 / 2 | snow forests, dunes, salt flats, jungles, first ice |
| 3 | hills 15, liquid 15, forest 12, mountains 9, desert 8, swamp 6 | 15 / 11 / 14 / 2 | tall forests, swamps, mesas, first floating islands |
| 4 | liquid 15, hills 14, crater 9, cold 9, wasteland 8 | 16 / 8 / 13 / 4 | canyons, sinkholes, volcanoes, plateaus, icebergs |
| 5 | forest 10, hills 11, crater 7, mountains 7 | 11 / 11 / 10 / 4 | spires, overhangs, cave-riddled surfaces, lava fields |
| 6 | mountains 12, rocky 8, floating 5, crater 5 | 11 / 12 / 8 / 2 | chasms, gorges, floating islands, mould seas |
| 7 | mountains 11, rocky 11, floating 10 | 9 / 13 / 7 / 1 | shrapnel fields, sky layers, giant spikes |

### 3.2 Surface blocks (high; `surface_substrate` in every biome)

The game's natural surface families, as named in the configs (generic words here):

| Family | Kinds | Where (most common) |
|---|---|---|
| Soil | silty, clay, peaty (three) | under grass; clay also bare in river and lake beds |
| Grass top | three grass kinds, each can sit on any of the three soils (9 combinations, block `GRASS_x/SOIL_y`) | grass1/silty most common (T1 24 biomes), grass2/peaty, grass1/peaty, grass2/clay |
| Sand | one | beaches y 58-67 (298 biomes), deserts, sea beds |
| Gravel | one | river beds (333 biomes), deserts, wastelands |
| Mud | one | swamps, bogs, mud flats (12+ biomes) |
| Ash | one | wastelands, volcanic and burnt biomes |
| Ice, glacier | two | cold biomes; some only above y 110 (snow caps) |
| Hazard covers | thorns, tangle, growth, mould, sponge | T2+ biomes and cave linings |
| Bare rock | igneous / metamorphic / sedimentary | slopes steeper than the cover's `maxGradient` |

Rules (high):
- Each substrate layer: `averageDepth` 3, `depthNoiseScale` ±3, a Y band, and a slope band (`minGradient`..`maxGradient`). Soils and grass stop at **gradient 1.25** (a few 1.5); sand, gravel, ash and ice usually go to 5 (all slopes).
- Every lowland biome joins two substrates: a **river bed** (`substrate/river_bed.json`: gravel 3, clay 3, sand 3 only at y 58-67) and its own cover. Lakes: `substrate/lake_bed.json` (gravel, sand, clay; sand at 59-67). Sea: `substrate/sea_bed.json` (rock, sand, gravel up to 64; sand at 59-70).
- A biome's cover picks among 2-3 grass/soil pairs by noise, giving patchwork ground (e.g. grassland: grass1/silty, grass2/clay, grass3/peaty; `tier 1/grassland.json`).
- Single **flower blocks** (four flower kinds, `FLORA_FLOWER_1..4`) are placed one block on grass in most green biomes (density -0.53..0 for grassland).

### 3.3 Layout (medium)

Biomes are not placed by tier bands or latitude: each place takes the world biome whose four climate targets best match the local climate fields (2.1); fields are Voronoi cells, so biomes come as irregular patches, blurred at the edges. Rivers (min with river noise) and the beach band cross all biomes. Weights (0.5..1) favour some biomes.

## 4. Strata: rock kinds and layers (high for values; `strata/release/`)

Three rock kinds: igneous, metamorphic, sedimentary. Each biome is **filled with one of them** (its `heightmapBlockType`, about a third each), and every biome uses the same strata graph `strata.json` = `standard_strata` + `standard_fill`:

| Layer set | Bands (kind: thickness ± deviation) | Height band | Wobble (`yNoiseScale`) | Count | Source |
|---|---|---|---|---|---|
| Thin bands | metamorphic 4±2, sedimentary 4±2, igneous 2±1, metamorphic 2±1, sedimentary 2±1, or none (2 empty slots) | centre drawn in y 55-256 | 6 | 6 slots × 8 draws | `strata_1.json` → `strata_selection_1.json` → `random_strata_2.json` |
| Thick bands | metamorphic or sedimentary 12±4 (igneous weight 0) | y 55-256 | 8 | 8 × 3 | `strata_2.json`, `strata_selection_2.json` |
| Mid bands | metamorphic or sedimentary 6±3 (igneous weight 0) | y 55-256 | 20 | 7 × 3 | `strata_3.json`, `strata_selection_3.json` |
| Deep fill pockets | igneous, metamorphic, sedimentary, sand or gravel, 1±4 | y 0-64 | 64 (blobs, not bands) | 2 draws of 5 | `standard_fill.json`, `random_fill.json` |

Reading: above about y 55 every rock face shows many horizontal bands of the other two kinds (2 to 16 thick, wavy), igneous only in thin bands; below the sea, irregular pockets of rock, sand and gravel. This is the striped-cliff look Boundless players know (medium). Unused alternatives `strata_4`, `strata_selection_4/5`, `random_strata*` exist with similar values.

Trace ores per rock kind (sulphur in igneous, saltpetre in sedimentary, olivine in metamorphic) are **drops**, not placed blocks; they are in tuning 4.4, not in these configs (high).

## 5. Trees and plants (high for shapes and rules; low for density units)

### 5.1 Wood and leaf blocks

Three trunk kinds and three leaf kinds (`WOOD_ANCIENT/LUSTROUS/TWISTED_TRUNK`, `WOOD_LUSH/EXOTIC/WAXY_LEAVES`); each world recolours them. A tree prefab fixes which trunk and leaf it uses, and many mix two leaf kinds. Trunks have 3 orientation ids each (`id`, `id+1`, `id+2`).

### 5.2 Tree kinds (decoded prefabs in `instances/trees/`; 80 files)

Heights are the prefab's bounding height including roots (blocks). Leaf/trunk counts are blocks per variant (average).

| Shape (file) | Variants | Height | Width | Leaves / trunk | Biomes using it (by tier) |
|---|---|---|---|---|---|
| broad round crown (`oak`) | 3 | 15-22 | 18-32 | waxy 458 / twisted 256 | T1 2, T2 2, T3 1, T6 2 |
| small fruit tree (`olive_tree`, `apple_tree`, `peach_tree`) | 1 each | 9-10 | 8-10 | lush ~70 / twisted or lustrous ~25 | olive in every tier (19 biomes) |
| conifer (`pine`) | 10 | 18-40 | 9-16 | exotic 202 / twisted 131 | T1, T4-6 |
| snowy conifer (`pine_snow`) | 10 | 20-44 | 10-17 | exotic 201 + ice 176 / twisted | T2, T4, T6-7 |
| spruce (`spruce_tree`, `spruce_large`) | 1, 2 | 11; 14-34 | 5; 6-15 | exotic 50 or waxy 198 / lustrous | T1-5 |
| palm (`palm`, `palm_big`, `palm_small`, `palm_bent`, `curly_palm`) | 10, 3, 5, 2, 1 | 12-37 | 9-20 | exotic ~110-175 / twisted | T1-5 deserts, islands |
| giant conifer (`redwood`, `_hollow`, `_w_hole`) | 3, 1, 1 | 54-87 | 22-25 | exotic 773-895 / lustrous 830-1520 | T3 redwood forest |
| cypress (`cypress_tree`) | 2 | 35-45 | 20-24 | exotic 366 / lustrous 562 | T5 |
| broadleaf giants (`colourful_tree`, `cottonwood`, `wide_canopy_tree`, `socotra_tree`) | 1-2 | 24-41 | 21-29 | lush or waxy 650-870 / 265-530 | T1-7 |
| jungle (`jungle`, `jungle_tree`, `jungle_vine_tree`, `banana_tree`) | 1-4 | 17-30 | 11-29 | lush or exotic / twisted or lustrous | T1-7 |
| willow, birch, elm, sycamore, cherry | 1-3 | 13-20 | 10-18 | mixed | T1-7 |
| savanna flat-top (`arcacia`) | 1 | 21 | 28 | waxy 483 / twisted 381 | T1-4, T7 |
| swamp (`swamp_tree`, `small_swamp_tree`, `swamp_shrub`) | 1-2 | 10-20 | 13-19 | lush / twisted | T1-3, T5-7 |
| dead trees (`death_tree_set_a..d`, `pine_death`, `joshua_death`) | 1-11 | 7-62 | 6-22 | trunk only | wastelands T1-6 |
| hazard trees (`poisonous`, `dangerous`, `jungle_fruit_tree`) | 1-2 | 13-28 | 8-29 | waxy + growth, thorns or crystal | T3-7 |
| fallen logs, stumps (`fallen_tree`, `tree_stumps*`) | 1-4 | 3-8 | 3-23 | trunk only | T3, T5 |
| odd shapes (`spiral_tree`, `upside_down_tree`, `cube_tree`, `umbrella_tree`) | 1 | 9-26 | 5-17 | | T1-2 |

Procedural shapes (inline `bush` and `basicTree` nodes, 128 and 34 uses; high): a blob of leaves (or rock, sponge, thorns, mould, crystal) with `height` 1-6, `radius` 1-6, `taper` 0.7-1, optional stem kind; a basic tree adds `trunkHeight` and scale 1-2 (e.g. leaf height 11, radius 6.2; `tier 1/bamboo_forest.json` uses a 7-tall, radius-1 one).

### 5.3 Placement rules (high for values)

| Field | Typical | Meaning |
|---|---|---|
| `constraintType` | the biome's grass kinds (trees), sand and clay (seaweed) | ground blocks it may stand on |
| `density` | trees median -0.82 (range -0.99..1); bushes -0.57; plants -0.75; rock props -0.87; flower blocks -0.62 | spawn rate on a -1..1 scale; -1 none, 1 everywhere (low: curve unknown) |
| `threshold` + `noise` | 0, or a noise for clumps (willows follow river noise) | placement only where noise > threshold (forum t/4627 #19) |
| `clearance` | 8 (trees), 0-32 | free space needed around it |
| `minGradient..maxGradient` | 0..5 (most trees), 0..1.5, 0..0.5 for ruins and liquid pockets | slope it may stand on |
| `minY..maxY` | 0..256; seaweed 0..54; beach props 58..69 | height band |
| `minSize..maxSize` + `sizeNoise` | 0..1 or 0.5 | size scaling (forum: 0.5 to 2 × the prefab) |

Other props (high, decoded): bushes 1-9 tall (`instances/bushes/`); rock outcrops of sedimentary rock 5-15 tall, 3-15 wide (`instances/rocks/rock_*`), tundra slabs, glacier hexagon columns, termite mounds of ash; mushrooms as giant mould-and-sponge caps 3-18 tall (`instances/mushrooms/`); bones of metamorphic rock 13-25 long on beaches and deserts; seaweed (lush leaves) on sea and lake beds below y 54 in 312 biomes; icebergs and ice caps; lily pads of peaty soil and leaves; reeds; bamboo; cacti built of tangle and thorns; ruins, portals, huts (`instances/remanants/`), rare (density -0.97..-0.99, flat ground only).

## 6. Caves (high for sets; medium for depth)

| Tier | Cave styles drawn (weight) | Source |
|---|---|---|
| 1-2 | standard, standard 2, flat (0.5 each) | `noise3d/caves/release/tier_1_caves.json`, `tier_2` |
| 3 | + crooked, medium round, cube, large simple, round rooms (0.36) | `tier_3_caves.json` |
| 4 | + small, tube, big, cube, deep (0.36); earlier 0.15 | `tier_4_caves.json` |
| 5 | + giant flat (2), cathedral, deep river, Voronoi cells, ladder (0.9) | `tier_5_caves.json` |
| 6-7 | + shafts, pillars, large smooth, huge crooked (0.9) | `tier_6_caves.json`, `tier_7` |

- Shapes from their nodes: standard = |lerp of perlins| below a threshold (tunnels); flat = stretched simplex (frequency X/Z 0.37, Y 1.17: wide, low); giant flat = simplex X/Z 0.163, Y 1.99 (huge flat halls); cathedral and deep river = Worley cells stretched vertically (Y frequency 0.375 or 0.147: tall); cube = grid cell samples (boxy rooms); Voronoi = cell walls. (`noise3d/caves/release/beginner|moderate|expert/`; high for nodes, low for sizes in blocks.)
- Depth: carving runs from the bottom up to a 2D ceiling (`noise3d/misc/cave_height_3d.json` → `noise2d/misc/cave_height.json`: the terrain height with rivers, minus 0.15 v ≈ **19 blocks below the surface**); above it nothing is carved. Most biomes set `caveThresholdBias` -1 (normal caves); 6 set 0 and 3 set 0.65-0.95 (fewer or no caves) (medium).
- Linings (`substrate/caves/`): T1 none; T2+ a 2-deep lining on cave walls, 90 % one of the three rocks or glacier, 10 % an exotic lining (sponge, thorns, ice, glacier, sand, ash, or one of the three leaf blocks) (high).
- Lower sea: below y 16 open space fills with the world's lower-sea liquid (2.1).
- Cave decorations are rare in the configs: crystal shards on ceilings in a few light-crystal biomes, cave trees in 2. Light crystal appears through biome props (`underground/gleam_pieces`, `crystal/*`) in light-crystal-labelled biomes (high).

## 7. Rare liquid pockets and crystals (high)

- Resin and oil pockets exist **only on exoworlds**: each exo config adds a 5-biome selection from `biome_sets/rare_liquid_resin_set.json` (lush, metal, corrosive, toxic, shock, chill, blast, burn exos) or `rare_liquid_oil_set.json` (coal, rift, blink, umbris exos) (`worlds/release/*_exo.json`).
- The pocket is a small prefab, 3-12 tall, 3-9 wide (resin, 9 variants) or 2-6 by 3-6 (oil, 12 variants) of rock, soil or sand around the liquid (`instances/rare_liquids/`). Rule: density -0.97..-0.98 (very rare), flat ground only (gradient ≤ 0.5), y ≥ 63 (some 37-59), on grass, soil, mud, hazard covers (resin) or sand, ash, gravel, rock, grass (oil); a few oil pockets in caves y ≥ 3 (`biomes/release/rare_liquid_*/`).
- Small surface water and lava pools (`liquid_water`, `liquid_lava`, 1-3 wide) on rock in some 3D biomes, y 59-89 and up.
- Crystal props: angled crystal shards of ice, light-crystal clusters, light-crystal-in-rock outcrops 3-10 tall, ceiling light crystal, dark-glass crystals, glacier crystals (`instances/crystal/`).

## 8. Mapping onto Boundmore's generator

Boundmore today (`crates/bm-worldgen/src/lib.rs`, `assets/planets.toml`): one noise heightmap with mountains, sea, snow line, one tree kind (trunk + foliage), caves, a lower sea, ores with depth/water/height/ground knobs, flora props, and per-planet colours. Boundless heights fit Boundmore by shifting (Boundless sea 64 of 256; Boundmore's default sea is 128).

Ordered by how much each would make planets more interesting for the work:

| # | Feature | What Boundmore lacks | Work | New blocks (generic) |
|---|---|---|---|---|
| 1 | **Biomes by climate fields** | one landscape per planet | climate fields (4 Voronoi-cell noises), a per-kind biome list with targets and weights, blending of height and cover at edges; each biome sets heightmap, cover, base rock, decorations | none by itself |
| 2 | **Three rock kinds and strata bands** | one stone (plus an optional sandstone band) | base rock per biome; banded strata above ~55 levels over sea (thin 2-4, mid 6, thick 12, wobbling); deep pockets of rock, sand, gravel | two more rocks (stone becomes one of three); each with its own trace drop |
| 3 | **Tree kinds** | one tree | 10-15 shapes first: broad crown, small fruit tree, conifer, snowy conifer, palm, giant conifer, jungle, savanna flat-top, swamp, dead tree, bushes, fallen log; procedural blob bushes; per-biome lists with density, slope, clearance | two more trunk kinds and two more leaf kinds |
| 4 | **Surface cover rules** | turf/soil/sand/snow by height | grass-over-soil pairs patched by noise, slope limit 1.25 (bare rock on steeper), river beds of gravel and clay, beach band, mud, ash, ice caps | two more soils (three: fine, clay, peat-like), two more grass tops (or grass tints over each soil), mud |
| 5 | **Rivers** | water only in hollows | `min(height, river noise)` carving winding channels to sea level, with bed cover | none |
| 6 | **Vertical shapes by tier** | 2D heightmap only | 3D-noise bands: plateaus (y band), overhang cliffs, floating islands at a height band, more on higher tiers (biome folder 1 none, folder 7 nearly a third) | none |
| 7 | **Cave styles by tier** | one cave style | tunnels and flat caves on T1-2; rooms, cubes, crooked on T3-4; giant halls, cathedrals, shafts, pillars on T5+; 2-deep linings from T2 | none (linings reuse rocks, ice, leaves, sand, ash) |
| 8 | **Props** | flora props only | rock outcrops, bones, giant fungus caps, ice spikes, crystal clusters, seaweed, ruins (rare) as small prefabs | perhaps a glowing crystal kind per planet colour (Boundmore has one glow crystal) |
| 9 | **Biome mix by tier** | one look per planet kind | each planet draws 15 biomes, weighted to its home set (16 / 8 / 4 / 2 / 1), so every world has some gentler and some wilder land | none |
| 10 | Rare liquid pockets | none | only if resin and oil become materials: tiny pockets on flat ground of temporary worlds | two solidified liquids if added |

Notes for the spec:
- Changing terrain bumps `GENERATOR_VERSION` and keeps the old generator for saved planets (roadmap).
- Prefab trees fit the "world-aligned function" rule if each tree's origin and variant come from a lattice hash, as Boundmore's single tree does now.
- Plants, boulders and fungi placement is not in the source: Boundmore's `FloraSpec` knobs stay guesses, but could gain a biome filter once biomes exist.
- Density units (-1..1) and frequency units are unknown: calibrate by eye against screenshots, and list them in `docs/feel.md` as guesses.

---

## Appendix: every tier biome (from the resolved configs)

Columns: tier folder; biome file (source only, never a name for Boundmore); shape labels (climate words and tier/element labels dropped; "+3D" = uses 3D noise); base rock (ign, met, sed, air); surface cover besides the shared river, lake and beach beds (grass1-3 = the three grass kinds, silty/clay/peaty = soils, leaf1-3 = lush/exotic/waxy leaves); trees and props (prefab file stems; `blob(...)` and `ptree(...)` = procedural shapes with height h and radius r; `[flowerN]` = single flower blocks). Source: `biomes/release/tier N/<file>.json`.

| Tier | Biome file | Shape labels | Base rock | Surface cover | Trees and props |
|---|---|---|---|---|---|
| 1 | `bamboo_forest` | forest | met | grass1/silty, grass1/peaty | [flower2], [flower4], ptree(leaf1+trunk1 h7 r1) |
| 1 | `bog` | swamp flat forest liquid | ign | grass2/silty, clay, grass3/peaty | jungle_tree, swamp_tree, cypress_swamp_root |
| 1 | `boreal` | hills forest | met | grass1/silty, grass1/peaty | pine, atlas_cedar |
| 1 | `bushlands` | hills bushland mountains | ign | grass1/silty, grass3/peaty | blob(leaf1+trunk2 h2.5 r3.2), blob(leaf2+trunk3 h3 r3.8), blob(leaf3+trunk1 h2.2 r3) |
| 1 | `carved_path` | flat rocky grassland | sed | grass1/silty, grass1/peaty | bush_lush, [flower2] |
| 1 | `coniferous_forest` | hills forest | met | grass2/clay, grass3/peaty, grass1/silty | spruce_tree, spruce_large, [flower3] |
| 1 | `crop_cricles` | flat grassland | ign | grass1/silty, grass2/clay | bush_waxy, bush_lush, sapling, [flower2] |
| 1 | `curley_desert` | desert flat | sed | sand, gravel | mamoth_bones, tiny_bush, curly_palm |
| 1 | `dead_coniferous_forest` | wasteland flat forest | sed | gravel, ash, grass1/silty | pine_death |
| 1 | `deep_sea` | sea | sed | ign, sand, gravel | ptree(mould h1.5 r4.5), ptree(sponge h1.5 r4.5), blob(growth h3.8 r4) |
| 1 | `deforest` | wasteland flat hills | sed | ash, gravel | tree_stumps, tree_stumps_big |
| 1 | `farm_fields` | flat ruins | ign | grass3/peaty, grass2/clay, grass1/silty | apple_tree, birch, bush_lush, [flower3] |
| 1 | `flower_field` | flat grassland | ign | grass1/silty, grass1/peaty | [flower1], [flower2], [flower3], [flower4] |
| 1 | `forest_ponds` | flat forest liquid | met | grass1/silty, grass1/clay, gravel | cottonwood, [flower2] |
| 1 | `fun_mountain` | mountains | met | grass1/silty | spruce_tree, [flower4], [flower3], liquid_water |
| 1 | `garden_lawn` | flat grassland | sed | grass1/silty, grass2/peaty |  |
| 1 | `golf_course` | flat forest | sed | sand, grass2/clay, grass1/silty, grass3/silty | oak, willow, [flower3], flag_pole |
| 1 | `grassland` | flat grassland | ign | grass1/silty, grass2/clay, grass3/peaty | [flower1], [flower4] |
| 1 | `grassy_dunes` | hills grassland | met | sand, grass1/silty | [flower3], [leaf2], blob(sed h3 r3), palm |
| 1 | `grid_platforms` | wasteland flat forest | sed | grass2/clay, grass3/silty | cube_tree, [flower4] |
| 1 | `lake` | lake liquid | ign |  |  |
| 1 | `low_mountains` | hills mountains | sed | grass1/silty, grass3/clay | willow, round_bush |
| 1 | `lowlands` | flat grassland liquid | sed | grass1/silty, grass2/peaty, mud | [flower1], blob(leaf1 h2 r3.8), umbrella_tree, blob(leaf2 h3.8 r2.5), olive_tree, reeds |
| 1 | `mass_rivers` | flat forest liquid | ign | grass1/clay, grass2/peaty, gravel | arcacia, bush_lush |
| 1 | `mud_flats` | wasteland flat liquid | sed | mud | [flower4] |
| 1 | `orchard` | hills forest | ign | grass1/silty | apple_tree |
| 1 | `plateau` | hills | met | grass2/peaty, grass3/clay | bush_waxy, [flower4] |
| 1 | `pleasant_swamp` | swamp forest flat liquid | sed | grass3/clay, grass2/peaty, grass2/silty | blob(leaf1 h2 r3), olive_tree, blob(leaf3 h1.8 r3.2), small_swamp_tree, swamp_shrub |
| 1 | `praire` | grassland liquid flat rocky | ign | grass1/silty, grass1/peaty | [flower2], [flower3], blob(leaf1 h3 r2), rock_small |
| 1 | `rock_pond` | grassland liquid flat rocky | ign | grass1/silty, grass1/peaty, gravel | [flower2], [flower3], blob(leaf1 h3 r2), rock_small, willow |
| 1 | `rolling_hills` | hills grassland | sed | grass1/silty, grass2/peaty, grass3/clay | bush_waxy, [flower4] |
| 1 | `savana` | desert flat liquid | sed | sand, grass1/silty | socotra_tree, blob(leaf3 h2 r5), termite_mound |
| 1 | `shrubland` | bushland flat hills | met | grass3/silty, peaty | blob(leaf1 h2 r2), blob(leaf2 h3.5 r2), tiny_bush, bush_lush |
| 1 | `small_lakes` | flat forest liquid hills | sed | grass1/silty, grass1/peaty | rock_small, sapling, [flower2] |
| 1 | `stonehenge` | bushland ruins flat | ign | grass3/silty, grass2/clay | blob(leaf1 h2 r2), blob(leaf2 h3.5 r2), tiny_bush, bush_lush, stonehenge |
| 1 | `sunflower_field` | hills grassland | sed | grass1/silty, grass1/peaty | [flower2], [flower3], sunflower |
| 1 | `temperate_forest` | forest hills | ign | grass2/peaty, grass2/clay | oak, willow, [flower3] |
| 1 | `tiny_triangles` | flat | met | grass3/silty, grass2/peaty | blob(leaf1+trunk1 h2 r2), apple_tree |
| 1 | `wave_hills` | hills | met | grass2/peaty, grass3/clay | bush_exotic, bush_lush |
| 1 | `wetlands` | swamp flat grassland liquid | ign | peaty, clay, mud | reeds |
| 2 | `algea_lake` | lake liquid +3D | met |  |  |
| 2 | `basket_trees` | forest flat | sed | grass2/peaty, grass3/silty | colourful_tree, banana_tree, arcacia, flower_small, large_leaf_plant, fruit_tree, [flower3], [flower1], basket_circus |
| 2 | `cold_desert` | desert hills | sed | sand, ice | cactus_traditional_thorns, cactus_round, tundra_rock_large |
| 2 | `crater_field` | crater flat forest | ign | grass1/silty, gravel, ash | oak, bush_lush, [flower2], gleam_pieces |
| 2 | `dry_river` | wasteland flat | met | grass1/silty, sand, ash | death_tree_set_b, bush_death |
| 2 | `flat_desert` | desert flat | sed | sand, gravel | cactus_traditional_thorns, cactus_traditional, mamoth_bones, tiny_bush |
| 2 | `flower_field_2` | hills liquid grassland | ign | grass1/clay, grass1/peaty | [flower2], [flower3] |
| 2 | `hand_ocean` | sea liquid | met | ign, sand, gravel | hand_totem_b |
| 2 | `ice_forest` | hills forest | sed | ice | pine_snow, [flower3] |
| 2 | `ice_hex_mountain` | mountains | met | ice, glacier | pine_snow |
| 2 | `ice_ridge` | hills | ign | glacier, ice | [glacier] |
| 2 | `icy_pools` | flat liquid forest | ign | ice | spruce_large, spruce_tree, iglo, [flower2] |
| 2 | `jungle_canopy` | flat forest | sed | grass3/silty, grass2/peaty | wide_canopy_tree, [flower2], [flower3] |
| 2 | `jungle_gleam` | rocky flat | ign | grass2/peaty, mud, silty | rock_jungle_small, rock_jungle_set, rock_jungle_medium, rock_jungle_large, angled_gleam_rock_in_a_rock |
| 2 | `meadows` | forest | met | grass1/silty, grass1/peaty | [flower2], [flower3], [flower1], [flower4], blob(leaf1 h3 r2), bush_lush, oak |
| 2 | `natural_wells` | crater flat forest liquid | met | grass1/peaty, grass2/silty | cherry_blossom, [flower1], [flower2] |
| 2 | `oasis` | desert flat liquid | air,sed | sand, gravel, grass3/peaty, grass1/silty, clay | mamoth_bones, tiny_bush, banana_tree, curly_palm, fruit_tree, gum_tree, palm_big, [flower4], large_leaf_plant, [flower3] |
| 2 | `palm_desert` | desert flat forest | met | sand, grass1/silty | palm, palm_big, palm_bent, palm_small |
| 2 | `peat_swamp` | swamp flat forest liquid | met | peaty, mud, grass2/peaty | jungle, swamp_tree, [flower1], large_leaf_plant, cypress_swamp_root |
| 2 | `pleasant_forest` | flat forest | met | grass1/clay, grass1/peaty | [flower2], [sponge] |
| 2 | `rice_fields` | hills mountains bushland liquid | sed | grass3/silty, grass2/peaty | colourful_tree, banana_tree, arcacia, flower_small, large_leaf_plant, fruit_tree, [flower3], [flower1], liquid_water |
| 2 | `rock_blades` | flat rocky grassland | met | grass2/peaty, grass2/silty, met | tiny_bush, [flower3], [flower2], gleam_pieces |
| 2 | `rock_desert` | rocky desert hills crater | sed | sand, ash | rock_jungle_large, rock_jungle_medium, rock_jungle_set, gaint_staligmite |
| 2 | `salt_flat` | wasteland flat | sed | ash |  |
| 2 | `sand_dunes` | desert hills | sed | sand |  |
| 2 | `semi_arid` | desert rocky hills bushland | sed | sand, grass1/silty | olive_tree, tiny_bush, [flower1], rock_large |
| 2 | `shallow_sea` | sea | met | ign, sand, gravel | sea_shells |
| 2 | `shattered_desert` | desert flat crater | ign | sand, peaty, gravel | desert_rose |
| 2 | `snowy_hills` | hills | met | ice, grass1/silty | regular_pine, pine_snow, [flower1] |
| 2 | `soiled` | wasteland flat | met | silty, clay, peaty | [thorns], bush_death |
| 2 | `spiral_tree_plain` | forest liquid flat swamp +3D | ign | grass2/silty, clay | spiral_tree, [flower3], reeds |
| 2 | `strata_slice` | wasteland flat | sed |  |  |
| 2 | `tiered_lake` | hills bushland | ign | grass2/clay, grass2/peaty | olive_tree, sapling, small_portal_g, liquid_water |
| 2 | `tundra_forest` | hills forest | met | grass2/silty, grass2/peaty | spruce_tree, spruce_large, [flower4], tundra_rock_small, tundra_rock_large |
| 2 | `tundra_forest_icy` | hills forest | met | grass2/silty, glacier | spruce_tree, spruce_large, [flower4], tundra_rock_small, tundra_rock_large |
| 2 | `twisted_forest` | flat forest liquid | ign | grass2/silty, mud, clay | standing_tree, [flower3] |
| 2 | `upside_down` | forest flat | met | leaf1, leaf2 | upside_down_tree |
| 2 | `water_forest` | swamp forest flat | air | grass1/silty, grass1/peaty | wide_canopy_tree, cypress_swamp_root, swamp_tree, blob(leaf1 h4.8 r1.8), blob(leaf1 h1 r6) |
| 3 | `ancient_cliffs` | hills mountains forest | sed | grass2/silty, grass2/peaty | blob(leaf1 h1 r6), blob(crystal h1 r6), [flower3], [flower1], jungle, bush_exotic |
| 3 | `ancient_structure` | bushland ruins flat | ign | grass1/silty | bamboo, [flower1], blob(leaf1 h2 r2) |
| 3 | `anti-grav forest` | swamp liquid forest | ign | grass2/peaty, clay | fruit_tree, oak, peach_tree |
| 3 | `buttes` | desert rocky hills mountains | sed | sand, ash, gravel | thorn_bushes, bush_waxy |
| 3 | `carved_desert` | desert rocky hills | met | sand, gravel | blob(leaf3 h2 r1.8), olive_tree |
| 3 | `chocolate_hills` | hills forest | met | grass3/silty, grass2/peaty | colourful_tree, banana_tree, arcacia, flower_small, large_leaf_plant, fruit_tree, [flower3], [flower1] |
| 3 | `cliff_islands` | islands | ign | ign, sand, gravel, grass2/silty | palm_big, palm_small, [flower1], [flower2], [flower3], [flower4] |
| 3 | `desert_cliffs` | desert mountains hills | ign | sand, ash, gravel | cactus_traditional, cactus_round, cactus_big, rock_jungle_medium |
| 3 | `elephant_graveyard` | wasteland flat | met | ash, gravel | mamoth_bones, animal_skeleton, fallen_tree_stump, fallen_tree, blob(thorns h1.5 r1.5) |
| 3 | `gleam_lake` | lake liquid +3D | ign |  |  |
| 3 | `hazzard_sea` | sea liquid | ign | ign, sand, gravel | blob(mould h1.5 r4), blob(sponge h2.5 r3.5), blob(growth h6 r2) |
| 3 | `ice_crack` | flat ruins | ign | ice | iglo |
| 3 | `ice_gleam` | flat | ign | ice | angled_gleam_rock, ice_rock |
| 3 | `ice_lake` | lake liquid +3D | sed |  |  |
| 3 | `ice_spike_hills` | hills liquid | met,sed | ice, glacier | angled_gleam |
| 3 | `layer_hills` | hills mountains | sed | grass1/silty, grass3/peaty | cherry_blossom, peach_tree, liquid_water |
| 3 | `layered_mushroom_swamp` | swamp hills liquid | met | grass3/peaty, clay, peaty, mud | mushroom_small, willow, mushroom_medium, sycamore |
| 3 | `mangroves` | swamp liquid flat | sed | clay, mud, peaty | swamp_tree, flat_bush, blob(leaf1 h3.5 r1) |
| 3 | `moat_mounds` | mountains liquid | sed | grass1/clay, clay, grass2/peaty | blob(leaf1 h3 r2), sycamore, olive_tree, bush_exotic, [flower1] |
| 3 | `mountains` | mountains | ign | glacier, ice, grass1/silty, grass1/peaty | spruce_tree, [flower4], [flower3] |
| 3 | `no_mans_land` | wasteland flat | met | silty, ash, mud | tree_stumps, death_tree_set_a, fallen_tree, fence |
| 3 | `old_villages` | ruins flat forest | sed | grass2/silty, grass1/peaty | floating_relic, large_leaf_plant, [flower2], [flower3] |
| 3 | `open_mines` | desert wasteland crater | ign | gravel | blob(ign h2.5 r2.5), blob(met h2.5 r2.5), blob(leaf3 h2.8 r1), gleam_pieces |
| 3 | `pancake_mountain` | desert mountains | sed | gravel, ash, sand | oasis, thorn_bushes, angled_gleam_rock |
| 3 | `poison_forest_plateau` | forest hills | ign | grass1/silty, clay, grass2/peaty | poisonous, poisonous_ng, gleam_pieces, [flower3] |
| 3 | `rainbow_hills` | rocky hills desert | met |  |  |
| 3 | `redwood_forest` | forest hills | ign | grass2/clay, grass2/peaty | redwood, redwood_hollow, redwood_w_hole |
| 3 | `ring_islands` | desert islands liquid | sed | sand, silty, mud | palm, palm_small, palm_big |
| 3 | `rocky_desert` | desert rocky hills crater | sed | sand, gravel | bush_lush |
| 3 | `round floating islands` | flat bushland +3D | air | grass2/clay, gravel | large_leaf_plant, [flower4], [flower2], leaf_flower |
| 3 | `sawtooth_mountain` | mountains grassland | met | grass1/silty, grass1/peaty | [flower1], [flower4] |
| 3 | `scenic_mountains` | rocky forest liquid hills | met | grass1/clay, grass2/peaty | spruce_tree, [flower1], [flower2], liquid_water |
| 3 | `sheltered_lakes` | forest liquid | air,sed | grass2/silty, grass3/peaty | [flower1], leaf_flower, cave:angled_gleam |
| 3 | `spike_mountains` | wasteland mountains | ign | ash, silty | death_tree_set_a, bush_death, [thorns] |
| 3 | `spike_valley` | hills | met | grass3/silty, grass1/silty, gravel, silty, met, mud, clay | ptree(leaf1+trunk3 h5 r5.2), [flower2], blob(leaf1+trunk3 h2 r2), [flower1] |
| 3 | `sponge_swamp_forest` | flat forest swamp liquid +3D | sed | grass3/clay, grass3/peaty, mud | apple_tree, colourful_tree, [flower1], [flower2] |
| 3 | `tier_mountains` | hills forest | met | grass1/silty, grass1/peaty | ptree(leaf2+trunk1 h9.5 r12.0), blob(leaf3 h1.0 r6.0), liquid_water |
| 3 | `tier_swamp_forest` | swamp forest liquid flat | sed | peaty, silty, mud | ptree(leaf1+trunk2 h11 r6.2), [flower1], [leaf2], reeds |
| 3 | `toxic_wasteland` | wasteland flat liquid | ign | clay, grass2/peaty | [growth], [mould], mushroom_small |
| 3 | `water_swamp` | swamp forest flat liquid | ign | peaty, clay, mud | swamp_tree, jungle_tree, jungle_fruit_tree, animal_skeleton |
| 4 | `ancient_temples` | rocky ruins liquid | sed | sed |  |
| 4 | `beaches` | desert liquid flat | sed | sand, grass1/clay | palm, palm_big, palm_small, beach_chair |
| 4 | `boreal_mountain` | mountains forest | ign | grass1/silty, grass1/peaty | pine, atlas_cedar, tundra_rock_large |
| 4 | `castle_mountains` | mountains | met | grass1/silty, grass1/peaty | sycamore, arcacia, blob(leaf1 h2.0 r2.0), liquid_lava |
| 4 | `colourful_mountain` | mountains | met | grass1/silty, grass2/clay, grass3/peaty, silty, ign, sed | willow, sycamore, poisonous |
| 4 | `cube_hills` | mountains hills | met | glacier, ice | pine_snow, cave:small_portal_j |
| 4 | `desert_canyon` | desert crater rocky | sed | sand, ash, gravel | olive_tree, blob(sed h3.8 r4.0), liquid_lava |
| 4 | `desert_pools` | desert flat liquid | ign | sand, ash, gravel | palm, cactus_big, [thorns] |
| 4 | `floating_mountain` | mountains floating forest | met | glacier, ice, grass1/clay, grass2/peaty | mushroom_medium, [flower2], [flower1] |
| 4 | `fooks_world` | hills liquid forest | ign | grass1/clay, grass2/peaty | ptree(leaf1+trunk2 h16 r10.5), [flower3], [flower2] |
| 4 | `geyser-ice` | wasteland liquid hills crater | ign | ice, grass2/peaty | thorn_bushes, fungus_stump_b |
| 4 | `geyser` | wasteland liquid hills crater | ign | ash, grass2/peaty | thorn_bushes, fungus_stump_b |
| 4 | `gleam_ice_forest` | forest liquid flat | met | ice, glacier, grass1/silty | crystal_gleam, light_mushroom_small |
| 4 | `graveyard` | ruins forest flat | air | grass2/clay, grass3/silty, grass1/peaty | gravestones, blob(leaf1 h1.5 r1.2), death_tree_set_b, ruins_1 |
| 4 | `hex_columns` | mountains grassland | ign | grass1/clay, grass2/peaty | sycamore, twisting_tree, liquid_lava |
| 4 | `ice_levels` | crater forest +3D | sed | ice, glacier | pine_snow, spruce_tree, icicles, gleam_pieces |
| 4 | `icebergs` | liquid sea +3D | air |  | iceberg |
| 4 | `jungle_cliffs` | hills forest | sed | grass3/clay, grass2/peaty | colourful_tree, banana_tree, arcacia, flower_small, large_leaf_plant, fruit_tree, [flower3], [flower1], basket_circus, liquid_lava |
| 4 | `jungle_temple` | ruins forest flat | air,ign | grass3/silty, grass2/peaty, sed | colourful_tree, banana_tree, arcacia, flower_small, large_leaf_plant, fruit_tree, [flower3], [flower1] |
| 4 | `lake_hills` | hills liquid | sed | grass1/silty, peaty | fungus_stump_a, [flower3], tiny_bush, olive_tree, liquid_water |
| 4 | `large_meteor_strike` | desert crater flat | ign | ash, sand, grass1/peaty, grass2/silty | rock_jungle_small, tiny_bush, palm, bush_exotic |
| 4 | `lush_cliffs` | hills forest | met | grass1/clay, grass1/peaty | ptree(leaf1+trunk2 h11 r6.2), [flower3] |
| 4 | `man_made_islands` | islands liquid | ign | leaf1 | palm_big |
| 4 | `maze` | flat ruins | sed | glacier |  |
| 4 | `ocean` | sea liquid | sed | ign, sand, gravel | blob(sed h3.8 r4.0), blob(sed h6.0 r1.8) |
| 4 | `raised_plateau` | desert rocky mountains +3D | sed | sand | stepping_stones, [flower1], liquid_lava, vine_single, gleam_pieces |
| 4 | `raised_rivers` | flat bushland liquid | sed | grass1/silty, clay, grass2/peaty | jungle, large_leaf_plant, [flower2], tiny_bush |
| 4 | `regular_pools` | wasteland rocky flat liquid | ign | ign |  |
| 4 | `shafts` | wasteland crater | ign | sand, ash | blob(crystal h1.0 r6.0), olive_tree, liquid_lava |
| 4 | `shelf_cliffs` | hills | ign | grass1/silty, grass1/peaty | rock_small, sapling, [flower2] |
| 4 | `sink_holes` | desert hills crater | met | sand, ash | joshua, blob(leaf1 h1.0 r6.0), liquid_water |
| 4 | `small_volcanos` | hills crater | ign | grass3/peaty, ash | [leaf1], bush_waxy, bush_death, [flower4], liquid_lava |
| 4 | `snake_islands` | floating bushland liquid flat +3D | met | grass1/silty, grass1/peaty | twisting_tree, [flower3], flat_bush, bush_exotic, reeds, cave_lights |
| 4 | `snowy_redwoods` | hills forest | sed | grass1/peaty, ice | death_tree_set_d |
| 4 | `sunken_canyon` | crater desert rocky hills | ign | sand, ash | cactus_traditional_thorns, rock_jungle_small, desert_rose, tiny_bush, liquid_lava |
| 4 | `supported_hills` | desert wasteland hills ruins | sed | sand, ash, gravel, silty, grass1/silty, sed | olive_tree, poisonous, liquid_lava, cave:[tangle] |
| 4 | `thorny_forest` | wasteland forest flat | ign | silty, peaty | death_tree_set_a, death_tree_set_b, bush_death, thorn_bushes, [thorns] |
| 4 | `twined_hills` | hills bushland +3D | air | grass2/peaty, peaty | [flower4], liquid_water, vine_single |
| 4 | `wasteland` | wasteland flat | ign | gravel, sand | death_tree, death_tree_set_a, rock_jungle_small, blob(thorns h2 r6) |
| 4 | `water_grid_2` | wasteland rocky flat liquid swamp | sed | ign | mushroom_small, reeds |
| 5 | `alien_invasion` | forest floating +3D | air | grass3/peaty, grass2/silty, gravel | birch, blob(leaf1 h2 r2.5), fallen_tree, [flower1] |
| 5 | `bushy_hills` | hills bushland | met | grass1/silty, grass1/clay | round_bush |
| 5 | `castles` | rocky ruins hills | sed | sed |  |
| 5 | `clam_ocean` | sea liquid | ign | ign, sand, gravel | blob(sed h3.8 r4.0), sea_shells |
| 5 | `coral` | sea liquid | met | gravel, sand, clay, ign | blob(sponge h2 r5.8), ptree(tangle h4.2 r7.8), blob(mould h6 r2), blob(crystal h3.2 r2.8), blob(growth h1 r1.2), coral |
| 5 | `desert_overhangs` | hills desert | sed | sand, gravel | cactus_traditional_thorns, cactus_traditional, mamoth_bones, tiny_bush |
| 5 | `desert_spires` | desert hills | ign | sand, ash, gravel | [thorns], [crystal], liquid_lava |
| 5 | `dome_field` | bushland +3D | met | grass2/clay, grass3/peaty | ptree(leaf1 h10 r1), [flower1], [flower3] |
| 5 | `drains` | wasteland rocky flat crater liquid | ign | ign | small_portal_j, stonehenge_ruin_extra, liquid_water |
| 5 | `flat_top` | hills forest | ign | grass2/peaty, grass1/silty | colourful_tree, fruit_tree, jungle_tree, large_leaf_plant, pasque_flower, [flower3], [flower4] |
| 5 | `flying_foliage` | floating hills grassland +3D | met | sand, ash, grass3/silty | rock_large, rock_set, cactus_round, cactus_traditional_thorns |
| 5 | `forest_cliffs` | hills forest +3D | sed | grass1/silty, grass1/peaty | birch, [flower4] |
| 5 | `fort` | flat | met | glacier, ice | cottonwood |
| 5 | `giant_rock_outcrop` | wasteland liquid mountains | ign | grass1/clay, grass1/peaty | [flower2], tiny_bush |
| 5 | `gleam_cavern` | crater rocky wasteland | met | ash | cactus_round, cave:gleam_pieces |
| 5 | `hanoi` | floating grassland +3D | sed | grass3/silty, grass1/silty, grass2/peaty | olive_tree, liquid_lava, [flower3] |
| 5 | `ice_spikes` | mountains | met | ice, glacier |  |
| 5 | `island_trees` | forest flat +3D | sed | grass2/silty, trunk2 | [flower1], bush_lush |
| 5 | `kong_island` | islands forest mountains liquid | ign | ign, sand, gravel, grass1/silty, grass2/peaty | palm, palm_big, palm_small, jungle, banana_tree, swamp_tree, [flower1], [flower2], liquid_lava |
| 5 | `lantern_field` | swamp liquid flat | ign | ash | dark_lantern |
| 5 | `lava_field` | wasteland crater | ign | ign, met | death_tree_set_a, [thorns], liquid_lava |
| 5 | `maze_2` | bushland flat | sed | silty |  |
| 5 | `mountain_roads` | mountains | met | grass1/silty, grass2/clay, met | spruce_tree, [flower1], [flower2] |
| 5 | `mushroom_forest` | wasteland hills | sed | grass3/peaty, grass2/clay | mushroom_large, mushroom_medium, mushroom_small |
| 5 | `overhang_cliffs` | mountains forest | met | grass1/silty, grass2/peaty | cherry_blossom, [flower2], [flower4] |
| 5 | `pillar_forest` | liquid forest crater | sed | grass1/clay | birch, sycamore, [flower1], liquid_water |
| 5 | `sea_trench` | sea liquid | air | ign, sand, gravel | ptree(crystal h8.5 r4.8), blob(leaf1 h6 r1), blob(leaf2 h3.5 r1) |
| 5 | `smooth_caves` | flat forest crater | air | grass1/silty, grass2/peaty | cypress_tree, big_square_bush, liquid_lava, cave:liquid_lava |
| 5 | `stone_pillars` | desert rocky flat | air | ign, gravel |  |
| 5 | `stone_walls` | swamp liquid flat | ign | grass1/silty, clay | apple_tree, bush_lush, [flower2], liquid_water |
| 5 | `sunken_forest` | crater | met | grass1/silty, grass2/peaty | wide_canopy_tree, egg, liquid_lava |
| 5 | `surface_caves` | hills forest | met |  | cave:pine, cave:atlas_cedar |
| 5 | `tiered_rock_outcrops` | desert rocky mountains hills | sed | sand, silty, ash | flat_bush, liquid_lava |
| 5 | `volcanos` | wasteland mountains crater forest | ign | grass3/silty, grass2/peaty, ash | pine, pine_death, [flower1], bush_waxy, gleam_pieces, liquid_lava |
| 5 | `wall_caves` | desert hills | sed | gravel, sand | joshua, desert_rose, liquid_lava |
| 5 | `water_entrances` | islands liquid bushland | ign | grass3/peaty, ign, sand, gravel | [leaf1], bush_waxy, bush_death, [flower4] |
| 6 | `alien_inselberg` | rocky mountains grassland | met | grass1/silty, grass1/peaty | [flower2], [flower3], blob(leaf1 h3.0 r2.0), elm, olive_tree, liquid_lava |
| 6 | `ancient_village` | flat ruins liquid | sed | grass1/silty, grass3/peaty | cherry_blossom, eastern_building_a, eastern_building_b, portal_orient_broken |
| 6 | `angular_mountains` | mountains | ign | mud, sand, grass2/silty, ash | bamboo, [flower3] |
| 6 | `bridge_islands` | wasteland islands liquid | met | ash, silty, mud, sand, clay | death_tree_set_b, death_tree_set_a, small_portal_a |
| 6 | `circle_islands` | floating flat +3D | met | ice, grass1/peaty | pine_snow, hexagonal_rocks_big |
| 6 | `cliff_islands` | islands mountains liquid | ign | grass1/clay, ign, sand, gravel | oak, cherry_blossom, apple_tree, liquid_water |
| 6 | `corrupt_trees` | flat forest | sed | grass2/clay, grass1/peaty, grass3/silty | tree_relic, flat_bush, [flower2] |
| 6 | `cube_field` | wasteland flat | sed | mud | dark_cube_l, dark_cube_m, dark_cube_s |
| 6 | `dagger_mountain` | mountains | met | ice, glacier |  |
| 6 | `dark_field` | swamp flat liquid | ign | gravel, ash | ptree(thorns h1.0 r5.0), liquid_water |
| 6 | `floating_chaos` | floating +3D | air | grass1/silty, grass1/peaty | pine, atlas_cedar |
| 6 | `floating_island_paradise` | floating liquid +3D | ign |  | tree_roots, apple_tree, [flower2], liquid_lava |
| 6 | `giant_chasm` | rocky crater | met |  | gleam_pieces |
| 6 | `giant_slice` | flat grassland | sed | grass1/silty, grass1/peaty | olive_tree |
| 6 | `gleam_encrusted_plain` | wasteland flat rocky +3D | sed | crystal |  |
| 6 | `gleam_spiral` | wasteland rocky flat | air | met | crystal_dark_glass, gleam_pieces |
| 6 | `grand_gorge` | mountains crater rocky | ign |  | rock_large, rock_medium, rock_set, rock_small, liquid_lava |
| 6 | `light_mushrooms` | flat liquid forest | met | ice, grass2/silty | light_mushroom_small, light_mushroom_large |
| 6 | `mouldy_sea` | sea liquid | ign | ign, sand, gravel | [mould] |
| 6 | `mount_doom` | mountains grassland | ign | grass2/peaty | rem_underground_f |
| 6 | `nests` | crater flat forest | met | grass2/silty, grass1/peaty, tangle | egg2, elm, pasque_flower, [flower2] |
| 6 | `palace_mountain` | mountains ruins forest | met | grass1/silty, grass3/peaty | cherry_blossom, peach_tree, [flower1], [flower4] |
| 6 | `piller_field` | wasteland mountains | ign | ign |  |
| 6 | `piller_islands` | rocky floating flat +3D | met | peaty, clay, mud | swamp_tree, jungle_tree, jungle_fruit_tree, animal_skeleton |
| 6 | `pitfall_field` | rocky flat crater | met | met |  |
| 6 | `poison_cliffs` | flat hills | ign | grass3/silty, grass2/peaty | poisonous, poisonous_ng, [mould], [growth], liquid_water |
| 6 | `ribbon_mountain` | mountains desert | sed | sand, ash | desert_rose, [thorns], liquid_lava |
| 6 | `sliced_mountain` | mountains bushland | met | grass1/clay, grass1/peaty | olive_tree |
| 6 | `stunted_ice` | flat | ign | ice, glacier | stunted_ice_snow |
| 6 | `supports` | mountains rocky | ign | grass1/clay | cave:olive_tree, cave:[flower1], cave:cottonwood |
| 6 | `tiny_islands` | islands liquid flat | sed | grass1/silty, grass2/peaty, clay, sand | oak, cherry_blossom, apple_tree |
| 6 | `torn_canyon` | crater floating bushland +3D | met | grass3/silty, clay, grass1/peaty | cherry_blossom, olive_tree, [flower4], liquid_lava, angled_gleam_rock |
| 6 | `wave_wall` | mountains bushland | sed | grass1/silty, grass1/peaty | bush_lush, bush_exotic, [flower4], [flower3], liquid_water |
| 7 | `angled_spikes` | mountains forest | met | grass1/clay, grass1/peaty | bush_lush, olive_tree |
| 7 | `array` | floating rocky flat +3D | ign | ign, sed |  |
| 7 | `bubble_mushroom_mountain` | mountains liquid swamp | ign | grass2/silty, grass3/clay | mushroom_small, mushroom_medium, mushroom_large, liquid_water |
| 7 | `crazy_circles` | floating | met | grass1/clay, grass1/silty, ice, glacier | light_mushroom_large, light_mushroom_small |
| 7 | `crystal_mountain` | mountains rocky | ign | ice, glacier | angled_gleam, angled_gleam_rock, pine_snow |
| 7 | `cube_sample` | hills bushland | sed | leaf2, leaf1 |  |
| 7 | `filled_valley` | floating flat +3D | met | ice | pine_snow, [flower3] |
| 7 | `floating_shrapnel` | rocky floating liquid +3D | sed | ign, sand, gravel | liquid_water |
| 7 | `floating_structures` | rocky floating +3D | ign | ign, grass2/peaty |  |
| 7 | `gleam_rain` | floating flat bushland +3D | met | grass3/silty, grass2/peaty | [flower3], bush_exotic |
| 7 | `growth_field` | wasteland rocky swamp | met | grass2/silty, grass3/peaty | [growth], swamp_tree, cypress_swamp_root |
| 7 | `ice_walkway` | crater liquid | ign | glacier, ice | iglo |
| 7 | `jungle_twist` | forest mountains | sed | grass3/silty, grass2/peaty | colourful_tree, banana_tree, arcacia, flower_small, large_leaf_plant, fruit_tree, [flower3], [flower1] |
| 7 | `lumpy` | hills grassland | met | grass3/silty, grass2/peaty | [flower4], [flower2] |
| 7 | `olympus` | mountains wasteland rocky | met | ice, ign |  |
| 7 | `reverse_hanoi` | desert bushland floating rocky +3D | ign | sand, ash, grass1/silty | olive_tree, [flower3], liquid_lava, gleam_pieces |
| 7 | `river_mountains` | mountains bushland | met | grass1/silty, grass2/peaty | gleam_gems, apple_tree, cherry_blossom |
| 7 | `seaweed_mountain` | mountains forest | met | gravel, sand |  |
| 7 | `sky_pattern` | floating forest +3D | sed | grass3/silty, grass2/peaty | colourful_tree, banana_tree, arcacia, flower_small, large_leaf_plant, fruit_tree, [flower3], [flower1] |
| 7 | `sliced` | wasteland rocky mountains | ign | ice |  |
| 7 | `sliced_islands` | rocky grassland floating +3D | met | grass1/silty, grass2/clay | [flower4] |
| 7 | `spiral_forest` | forest hills | met | grass1/silty, grass2/peaty | flower_totem, poisonous, poisonous_ng, [flower4], bush_lush |
| 7 | `square_caves` | crater rocky | sed |  |  |
| 7 | `standing_river` | sea liquid | met | ign, sand, gravel |  |
| 7 | `stone_balls` | rocky floating +3D | air | ign, sand, gravel | dark_glass_pieces, colourful_tree, banana_tree, arcacia, flower_small, large_leaf_plant, fruit_tree, [flower3], [flower1] |
| 7 | `sword_field` | wasteland crater liquid | ign | thorns, grass2/clay | [flower1], [flower2], sword_totem |
| 7 | `tenticle_outcrop` | swamp mountains liquid | sed |  | birch |
| 7 | `warped_mountains` | mountains forest | sed | glacier, ice, grass3/clay, grass3/peaty | artic_willow, [flower1], [flower2], liquid_lava |
| 7 | `water_grid` | crater liquid | ign | ice, glacier |  |
| 7 | `web_supports` | mountains | met | grass1/clay | cave:olive_tree, cave:[flower1], cave:cottonwood, cave:gleam_pieces |
