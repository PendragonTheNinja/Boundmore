# Milestone 19: Living Worlds (the worldgen pass)

*Spec written 2026-10-25. Nathan (2026-10-25): "eventually we'll need to do another worldgen pass to make planets more interesting overall and the different tree types, different stone types, et cetera." The research is `docs/research/boundless-worldgen.md`, read from Boundless's own World Builder configs (CC BY 4.0: "Boundless World Builder configs, Wonderstruck and contributors"). Boundless first: the structure (biomes chosen by climate, a per-tier biome mix, three rock kinds with banded strata, soils under grass, slope limits, river beds, tree kinds, cave styles by tier) follows those configs; the shapes, names and art are Boundmore's. Boundless's names appear in brackets only to identify things.*

## Goal

Today a Boundmore planet is one landscape: one noise heightmap, one stone, one soil, one tree, one cave style, coloured by region. A Boundless world is a patchwork of fifteen biomes, each with its own land shape, ground, rock and trees, crossed by rivers, with striped cliffs and, on higher tiers, overhangs, floating islands and bigger caves. This milestone gives Boundmore's planets that variety:

- **Part 1, land and stone:** biomes chosen by climate, each with its own height shape, ground cover and base rock; a per-tier biome mix; rivers; three rock kinds with banded strata; three soils with their grasses, and mud; slope limits that bare the rock on steep ground; recipe groups ("any rock", "any trunk") so the new kinds work in every recipe.
- **Part 2, trees and props:** three trunk kinds and three leaf kinds; a dozen tree shapes (broad crowns, conifers, snowy conifers, palms, giant conifers, flat-tops, swamp trees, dead trees, fallen logs, bushes); rock outcrops; plants and boulders filtered by biome.
- **Part 3, height and depth:** vertical land (overhang cliffs, raised plateaus, floating islands) more common up the tiers; cave styles by tier (tunnels, flat caves, rooms, great halls, shafts); cave linings from tier 2.

## How Boundless did it (research sections in brackets)

- **Biomes by climate** [1, 3.3]: four smooth fields over the world (altitude, temperature, humidity, vegetation) laid out as irregular cells; each biome has a target for each and a weight; a place takes the biome nearest its climate, blended at the edges.
- **Biome mix** [2.2]: a world keeps 15 biomes, drawn from every tier's biome set weighted 16 / 8 / 4 / 2 / 1 toward its own tier's set, filtered by labels its type allows. So every world has some gentler and some wilder land, mostly its own.
- **Shapes** [2.3]: flat land just above the sea with low rolling hills, low mountains, mountains to the ceiling, plateaus (a flat table high up), seas and lakes; rivers carve through 292 of 363 biomes.
- **Rock** [4]: each biome is filled with one of three rock kinds (about a third each); above about 9 blocks under the sea every rock face shows many wavy horizontal bands of the other two kinds, 2 to 16 thick; below, pockets of rock, sand and gravel. Each kind gives its own trace fragment (sulphur, saltpetre, olivine) as a rare drop [tuning 4.4].
- **Ground** [3.2]: three soils, each under a grass; a biome patches two or three grass-and-soil pairs by noise; soils and grass stop at slope 1.25, so steeper ground shows bare rock; beaches of sand in a band at the sea; river and lake beds of gravel, clay and sand; mud in swamps, ash in wastelands, ice and snow caps on cold heights.
- **Trees** [5]: three trunk kinds, three leaf kinds; 80 tree models in about 17 shapes; each biome lists its trees with density, slope, height band and clearance.
- **Caves** [6]: tunnels and flat caves on tiers 1 and 2; rooms and crooked caves from tier 3; great flat halls, tall cathedral caves and deep rivers from tier 5; shafts and pillars on 6 and 7; carving stops about 19 blocks under the surface; from tier 2 cave walls get a 2-deep lining.

## Decided in this spec

1. **Old planets keep their land.** `GENERATOR_VERSION` becomes 4. Every new setting is empty by default, and settings with none of them produce version 3's terrain block for block (a pinned test, as versions 2 and 3 did), so a saved planet opens unchanged and its builds stay on the ground they were built on. Only planets created after this milestone (a new universe, or a planet added to an existing one) get biomes. Nathan sees the new worlds by starting a fresh universe.
2. **Biomes are data**: `assets/biomes.toml` holds the catalogue (each biome's climate targets, weight, tier set, labels, land shape, cover, rock, cliff, beach, trees). A planet kind names its home tier and the labels it allows; when a planet is created its 15 biomes are drawn (16 / 8 / 4 / 2 / 1 by tier distance, Boundless's) from the catalogue **and copied into its saved settings**, as colours are picked once at creation. A saved planet never depends on the catalogue again, so the catalogue can grow freely.
3. **Heights against Boundmore's sea.** Boundless's heights are read against its sea at 64 of 256; Boundmore's sea is at 128 of its height, so Boundless altitude a becomes Boundmore 128 + (a - 64). Biome land shapes are given in blocks above or below the sea.
4. **Three rocks**: `stone` (the grey stone every planet has now; Boundless's metamorphic role, olivine), `basalt` (dark; igneous, sulphur), `limestone` (pale; sedimentary, saltpetre). `deep_stone` stays as the deep layer under all three. Each drops itself; flint, rock salt, opal and its trace fragment as Boundless's rare drops (tuning 4.4).
5. **Three soils and their grasses**: `soil` under `turf` (as now), `clay` under `meadow_turf`, `peat` under `moss_turf`; and `mud`. Boundless let any of its three grasses sit on any soil (nine blocks); Boundmore pairs each grass with one soil (three blocks), which keeps the look of patchwork ground at a third of the blocks. Each turf drops its own soil, as `turf` drops `soil` now.
6. **Three trunks and three leaves** (part 2): `trunk`, `pale_trunk`, `gnarled_trunk`; `foliage`, `fronds`, `glossy_foliage`. Trees are drawn by code (shapes with hashed sizes), not from Boundless's models.
7. **Recipe groups**: a recipe input may name a group (`any_rock`, `any_trunk`, `any_leaves`, `any_soil`, `any_base_alloy`, a line's pastes); any member item satisfies it, taken in a fixed order. This is how Boundless's recipes read ("Any Rock", "Any Timber") and it replaces the duplicated recipes M18 needed.
8. **Units are calibrated by eye.** Boundless's noise frequencies and density scale have no stated units (research 5.3); Boundmore's biome sizes, hill sizes and tree densities are tuned against screenshots and listed in `docs/feel.md` as guesses.

## Rules

### Part 1: land and stone

1. **Climate**: four 2D fields over each planet (altitude, temperature, humidity, vegetation), each 0 to 1, from cells about `biome_size` blocks across (default 384, a guess: Boundless's `biomeSize` 30 read as chunks of 16 is 480; Boundmore's planets are smaller), jittered and warped so edges wander. Each cell takes the biome whose four targets are nearest its own climate (distance divided by the biome's weight).
2. **Blending**: a column's height is the blend of the heights of the biomes of the cells near it, weighted by how close the column is to each cell (a smooth falloff over `blend` blocks, default 48, a guess); its cover, rock and trees come from the biome that weighs most there (with a little noise at the border so edges are not ruled lines).
3. **Land shapes** (per biome, blocks relative to the sea): `base` (the floor), `hills` (amplitude) with `hill_size`, `mountains` (ridged amplitude) with `mountain_size`, `plateau` (a flat table: the land is cut flat at this height above the floor where it would rise above it), `rivers` (on or off). The planet's `mountain_height` scales nothing in a biome world; each biome says its own.
4. **Rivers**: a winding river noise; where its value is near zero a channel is cut to two blocks under the water line, banks easing out over `river_width` blocks; river beds are gravel, clay and sand. Rivers cut every biome that allows them, never plateaus and floating land.
5. **Cover**: each biome lists one to three (top, under) pairs, picked per column by a patch noise; `under` is three blocks deep give or take three (Boundless's `averageDepth` 3, `depthNoiseScale` 3). Soils and grass stop at slope 1.25 (steeper columns show the biome's `cliff`, usually its rock); sand, gravel, ash and snow cover any slope. A beach of the biome's `beach` block (sand by default) runs from 6 below the sea to 3 above it (Boundless 58 to 67 against 64). Above the biome's `snow_above` (if set) the top is snow.
6. **Rock and strata**: under the cover, the biome's rock fills down to the deep layer. From 9 below the sea up (Boundless 55 against 64), bands of the other two rocks cross it: thin bands 2 to 4 thick, middle bands about 6, thick bands about 12 (Boundless: thin 2 to 4, middle 6 ± 3, thick 12 ± 4, igneous only in thin bands), each band's height wobbling with a slow 2D noise so the stripes wave across cliffs. Below the sea, small pockets of the other rocks, sand and gravel (Boundless's deep fill).
7. **Ores, caves, flora, lower seas and colours** work as now on biome worlds: ores replace any of the three rocks; colours apply to every new block (the kinds' colour choices name them).
8. **The biome catalogue** (part 1): about 24 biomes in tier sets 1 to 4, from Boundless's commonest shapes: meadows, rolling hills, forest floors, lakes and seas, low mountains, mountains, plateaus, mesas, dunes and deserts, salt flats, wastelands, swamps and bogs, snowfields and tundra, ash plains, canyons. Each kind's list draws from them by tier and labels; the nine M18 kinds and the four older ones all get biomes (new planets of every kind change).
9. **Spawn and shrines** stay on dry, flat land near the planet's origin (the existing search, over the new heights).

### Part 2: trees and props

10. **Tree shapes** (`bm-worldgen` `trees.rs`, pure functions of a seed and a size): broad round crown, small fruit tree, conifer, snowy conifer, palm (straight and bent), giant conifer (40 to 70 tall, rare), flat-top, swamp tree with drooping leaves, jungle tree, dead tree (trunk and bare branches), fallen log, stump, bush (a leaf blob 1 to 4 tall). Each shape names its trunk and leaf kinds.
11. **Placement**: each biome lists its trees (shape, density, slope limit, height band); a tree grows from a hashed lattice point, as now, so a tree is a world-aligned function and crosses chunk borders whole. The tree reach grows to the widest crown (the generator's margin grows with it).
12. **Props**: rock outcrops (of the biome's rock), boulder piles, fallen logs and stumps as small drawn shapes; the gatherable plants, boulders and fungi of M11 gain an optional biome filter.

### Part 3: height and depth

13. **Vertical land**: a 3D density added in a height band, per biome: raised plateaus (a band 30 to 90 above the sea), overhang cliffs (from the surface up), floating islands (a band 40 to 100 above the sea, over open land or sea). Boundless's share of biomes using it by tier set: 0 of 40 on tier 1, about a third on tier 7.
14. **Cave styles by tier**: tunnels and flat caves (tiers 1 and 2); add crooked caves and rooms (3 and 4); great flat halls and tall caves (5); shafts and pillars (6 and up). Carving stops about 19 blocks under the surface (Boundless).
15. **Cave linings** from tier 2: a 2-deep lining on cave walls, mostly a rock kind or ice, now and then sand, ash or leaves.

## Acceptance

- Part 1: a planet made by any kind has several biomes a player can walk between in a few minutes, each recognisably different in shape and ground; rivers wind to the sea; cliffs show stripes of the three rocks; steep ground is bare rock; beaches are sand; a planet saved before this milestone opens with every block where it was (pinned test, and the upgrade check on a version 3 planet); `cargo test` passes; screenshots of each kind in `docs/media/`.
- Part 2: forests of several tree shapes; conifers on cold land, palms on warm shores, dead trees in wastelands; nothing floats when a chunk next to it is saved first.
- Part 3: overhangs and floating islands on higher-tier worlds; caves differ by tier.

## Non-goals

- Boundless's exact biomes, models and noise graphs (the shapes are ours).
- Exoworlds and their liquid pockets (no exoworlds yet).
- Climbing plants, hazard covers (thorns, tangle, mould), and the light crystal props of Boundless (later, with the blocks that need them).
- Changing existing planets' land (rule 1).

## Measure (for Nathan, in game)

1. Do the biomes feel the size of Boundless's? (Ours: about 384 blocks across, a guess.)
2. Tree density in a forest against a Boundless tier 1 forest.
3. River width and how often a player meets one.
4. Whether the striped cliffs read like Boundless's.

## Status

*Spec written 2026-10-25; part 1 in progress.*

- [x] Recipe groups (rule 7 of Decided): `[[group]]` tables in `assets/recipes.toml` (`any_rock`, `any_trunk`, `any_leaves`, `any_soil`, `any_base_metal` for the copper or iron machine recipes, `any_base_alloy`, and each atmosphere line's pastes); a group input takes its members in the group's order (then `Inventory::remove`'s slot order), mixed members allowed. A station job keeps the items each craft took, saves them as runs of real items (the save format is unchanged; old jobs match the merged recipes) and refunds exactly those. The duplicated recipes (titanium alloy, the paste dishes, the copper or iron machines) are one each now. The crafting screen shows "Any Rock 3/4" with its members on hover. Protocol 31.
