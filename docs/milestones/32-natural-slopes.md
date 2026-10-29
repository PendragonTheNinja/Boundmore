# Milestone 32: Natural Slopes

*Spec drafted 2026-10-29.* Nathan, 2026-10-29 (polish batch 3, `docs/feel.md`): "Yes Boundless had natural slopes, it wasn't all blocky (I don't know how it was calculated)". New planets only (a generator bump). The building audit ranked it the biggest look difference on every planet (`docs/audit/building-and-world.md`, finding 1 and ranked item 2): our generator writes only full cubes, so every hill is a staircase.

## Goal

New planets' ground reads as land, not stairs: where the surface steps down by a block, the top block is a slope; outer corners are bevelled down, inner corners are filled in, and a player walks up a one-block rise without jumping. Old planets open unchanged, block for block.

## What Boundless did (forum, Discourse JSON API)

- **Slopes in generated terrain from early on.** Dev update 2016-06-03 (t/4479): "The World Builder first analyses the environment to work out which blocks are candidates for automatic sloping", with a screenshot of the candidates coloured by slope type and a scene before and after. "The settings are per block type to allow variation so areas can still look more faceted or smooth." Testing 14 (t/4504, 2016-06-10): new worlds with inverted slopes.
- **Inverted slopes under overhangs and cave roofs** (t/4479, t/4504): "the underside of caves can now be smoothed out"; players: "Caves do look a lot better now and feel more natural". Someone found water misbehaving against the slopes in a cave (t/4504).
- **All 150 slope shapes in terrain from release 177** (t/9352, 2017-09-08): "The new slope shapes are used for natural terrain generated in the World Builder now meaning new worlds will have 'even smoother' terrain in some cases." Square and bevel shapes were never generated ("These shapes will not be created by the World Builder", dev update t/8826).
- **Why slopes instead of auto-climbing:** "that's what slopes are for, in most cases terrain is sensible enough that there are sloped paths to where you want to go rather than having to constantly jump" (dev, t/1394, 2015). A player in the same thread: "leaves, stones, and other non sloping materials don't make for a fun time because they can't slope".
- **Which blocks:** natural terrain takes slope chisels only (mechanics 1.4); grass keeps its look only on full and slope shapes ("you'll never see grass on a block that's not a full square or slope shape", t/32224). Ores, leaves, trunks, ice were not sloped (not slopeable at all). No evidence that water was ever shaped; it stood beside slopes.
- **Things on slopes:** surface resources (boulders) and flowers grew on sloped ground and were drawn "offset-down onto sloped surfaces" (Testing 212, t/25109); the cell above still counted as taken (t/41784, 2020).
- **Not found:** the exact rule the World Builder used (its analysis is not described anywhere public); our rule below is our own, chosen to give the look in the dev's before and after pictures.

## The method

A pass inside `Generator::generate`, from the column height field the generator already has (with its margin, so chunk borders agree), only when the planet's settings say `slopes` (new planets):

- **Candidates:** the top block of a column (`wy == h`), still the column's cover block after caves and ores, whose block allows the slope family (`chisel = "slope"` or `"any"` in the catalogue: soils, turfs, sand, gravel, snow, clay, peat, mud, the rocks), standing above the water line (`h > water_level`, so no water ever touches a cut), with no lifted land in its own or a neighbour's cell beside or above it.
- **The cut:** only the top four octants (the block keeps its whole lower half, so it is always at least a slab). For each top corner of the block, look at the two neighbours sharing that corner's edges and the one across the corner:
  - cut it if either edge neighbour is lower (a ramp down to that side; two lower sides give an outer corner);
  - or cut it if only the diagonal neighbour is lower and neither edge neighbour is higher (an inner corner: the corner slopes down between two ramps, so the ramps meet without a notch).
  The slope family draws the convex hull of the kept corners (ADR-0008), so one side cut is a ramp, three corners cut is a pyramid corner, one corner cut fills an inner corner, and all four is a half slab (a lone bump or a one-wide ridge).
- **Things that stand on the ground keep a full block under them:** a tree's trunk or root foot restores the block it stands on (in whichever chunk that block lies), plants and boulders do not grow on cut ground, and anything else the chunk puts on a cut block (leaves, a low canopy) restores it. Boundless drew plants offset down onto slopes; that needs a client change and is a follow-up.
- **Walking:** what collides is the kept octants (ADR-0008): a ramp is a half step then another, both under `movement::STEP_HEIGHT`, so a one-block rise is walked up without a jump. No physics change.
- **Terraces:** kept. Only mesa (9 blocks) and badlands (6) terrace, and they are cliff biomes by design; their treads' edges now round off and the bottom of each riser's ease becomes ramps, so they read as eroded benches rather than stairs. Ordinary hills had no terraces; their stair-steps were single-block steps of the height field, which this pass makes ramps.
- **Steep ground:** a drop of two or more still gets its top block's cut (a rounded cliff edge); the faces below stay blocks. Boundless's cliffs looked the same.
- **Determinism and the server:** the pass is a pure function of the height field and the column's settings, run on the server only (ADR-0003). The shrine's plaza is cut flat after generation as before (`ensure_shrine` writes full blocks), so the spawn stays level.

## Decided in this spec (needs no decision of Nathan's)

- New planets only: `PlanetGenConfig::slopes`, set for every new biome planet; absent (false) in every saved planet's settings, so generator 5 makes version 4's terrain exactly. `GENERATOR_VERSION` 5; 4, 3 and 2 open unchanged.
- Never shaped: ores, crystals, ice, water, lava, trunks, leaves, plants, built blocks, anything not slopeable; blocks at or below the water line; blocks with something standing on them.
- Cave floors, cave roofs (Boundless's inverted slopes) and lifted land's tops stay blocky for now (see Part 2).
- Far views (M20) draw cut blocks as their block, uncut, as they already did for chiselled builds; a far cell is 2 or 4 blocks, coarser than any cut. The halving now also counts a cut block as hiding what is under it (far format 2, protocol 64), so a sloped hill's far copy is the same cell for cell as it was without slopes (turf seen, not the soil under a ramp).

## Acceptance

1. A new planet's hills have ramps, bevelled outer corners and filled inner corners; a test finds slopes on a new planet's surface and none on an old one's.
2. Generator 5 with `slopes` off makes generator 4's terrain block for block (the old pin, kept); the fingerprint is re-pinned for new planets.
3. A player walks up a one-block natural rise without jumping (a test on generated terrain).
4. No ore, water, ice or lava block is ever shaped, and no shaped block has water beside or above it (a test over many chunks).
5. Nothing grows or stands on a cut block (a test).
6. An older universe opens unchanged (the upgrade check).
7. `docs/media/m32-slopes.jpg` shows a new world before and after.

## Non-goals

Inverted slopes under overhangs and cave roofs; slopes on cave floors and lifted land; plants drawn offset onto slopes; sloped water; smoothing existing planets; changing how slopes collide (ADR-0008 keeps half steps).

## Part 2 (later, needs no decision)

- **Inverted slopes** on overhang and cave-roof undersides, and slopes on cave floors and lifted land's tops, from a 3D neighbour test (needs each chunk's neighbours' cells; the generator would compute a one-block shell).
- **Plants and boulders on slopes**, drawn offset down onto the slope as Boundless did (client mesher).

## Decisions for Nathan

1. **How it feels to walk on:** a natural slope climbs as two half steps (ADR-0008). If it feels steppy, a later ADR can give slopes ramp collision.
2. **How much is sloped:** every one-block step gets a ramp. Boundless's per-block-type settings let some ground stay faceted; if some biome should stay blocky (badlands, crags), say which.

## Status

- 2026-10-29: spec written, then **built**: `PlanetGenConfig::slopes` (set by `PlanetKind::generation` for biome planets; the server's no-biome test planets clear it), `Generator::cut_slopes` and `slope_cut` in `bm-worldgen`, trees restoring their footing (`place_biome_trees`'s `put`), flora skipping cut ground, `keep_footings` for anything else. `GENERATOR_VERSION` 5 (4, 3, 2 open unchanged), `FAR_FORMAT_VERSION` 2, protocol 64. Tests: `slope_cuts_make_ramps_corners_and_inner_corners`, `new_planets_slope_their_ground_and_never_ores_or_water` (every kind: 12 to 40% of dry columns cut; never an ore, never at or under the water line, nothing standing on a cut, no liquid beside one), `a_one_block_natural_rise_is_walked_up_without_a_jump` (real generated rises, `movement::step`), `no_slopes_reproduce_version_4` (the old pins kept), `slopes_leave_far_copies_unchanged`, and the re-pinned `terrain_fingerprint_is_pinned`.
- **Known limits:** plants and boulders do not grow on cut ground (Boundless drew them offset down onto it: a client follow-up); cave floors, cave roofs and lifted land stay blocky (Part 2); outside a chunk the lifted-land test samples the noise directly rather than the chunk's lattice, so a rare slope may sit beside a lifted block; the shrine plaza writes full blocks, and terrain just outside it keeps the cuts it had against the old ground.
- **Next:** Nathan's look and walk on a fresh universe (his two decisions above).
