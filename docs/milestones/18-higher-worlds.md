# Milestone 18: Higher Worlds

*Spec written 2026-10-25. M17's handoff named this next ("gems, power coils and tier 5 and 6 worlds"), and Nathan asked for work to continue overnight. Boundless first: the atmosphere rules, world types per tier, seam depths, smelting, alloys, gear stats and recipes are Boundless's as the community published them (tuning 2.1, 4.1 to 4.9, 4.13, 3.9, 5.4). Numbers marked **guess** are on no site we could reach; the Measure list at the end is for Nathan. Boundless's own names are used below in brackets only to identify things.*

## Goal

Boundmore's worlds stop at tier 3. Boundless's best materials (gold, titanium, gems) and its power coils sit on tier 4 to 6 worlds, and those worlds are guarded by their **air**: each needs a level of protection against its kind of atmosphere, from skills and food, or you choke. This milestone opens those worlds and climbs the ladder they hold:

- **Part 1, the worlds:** atmospheres and protection; nine new planets at tiers 4 to 6, one of each kind Boundless had; gold, titanium, medium and hard coal; gold and titanium smelting and alloys; silver, gold and titanium tools, grapples and spanners, with bonding agents; atmosphere food.
- **Part 2, gems and power:** six gem seams with their places, rough, compact and refined gems; power coils and machine power (crafts above a recipe's power need run faster and cheaper); leather; gem tools, grapples, spanners and advanced chisels; the titanium furnace; power on the forge (M17's power share) and the forge recipes that waited for it.
- **Part 3 (later, its own spec section when started):** lava seas and lava below, elemental creatures on gem worlds.

## How Boundless did it

- **Tiers and types.** Each world is a tier plus a type. Tier 1 to 3 worlds have ordinary air. From tier 4 every world has an **atmosphere** of one of three lines, set by its type: **caustic** (Lush, Corrosive, Toxic), **potent** (Metal, Shock, Chill), **volatile** (Coal, Blast, Burn). The hazard is 1 on tier 4, 3 on tier 5, 5 on tier 6 (7 and 10 on 7 and 8). [boundlexx world code; R198; tuning 4.13.1]
- **Protection** is the skill points of the world's line (each line 5 ranks giving +1, +1, +1, +2, +2: 1, 2, 3, 5, 7) plus an atmosphere food's +1, +2 or +3. At or above the hazard you are safe for as long as you stay. Below it, a debuff shows the shortfall, fog closes in, an **air bar** drains, a countdown shows, and then you suffocate until defeated, indoors too. Protected again, the air comes back. [R198; t/16261; tuning 4.13.1]
- **Which world types at which tier** [official world-builder configs]: Lush up to tier 4; Metal and Coal from tier 3; Corrosive, Toxic, Shock, Chill, Blast and Burn from tier 5. Release 207 made sure every server region had a tier 5 or 6 world of each gem type. Metal and Burn worlds have lava seas; Blast worlds have their sea low (lava is part 3 here).
- **Ores up the tiers** [tuning 4.3]: gold on tier 4 and up, 10 to 20 above the floor and 25+ under the surface; titanium on tier 4 Metal worlds and every world from tier 5, 70+ under the surface; medium coal everywhere 20+ down; hard coal tier 3 and up, 50+ down. Bigger yields on higher tiers [tuning 4.4]. Block health ×2, ×3, ×4 and armour 200, 400, 600 on tiers 4, 5, 6 (M9 already has these).
- **The metal ladder** [tuning 4.8, 4.9]: gold bar 180 s and 400 heat; titanium bar 240 s and 600 heat; gold alloy = gold bar + copper bar (mixer, 50 s, 100 spark); titanium alloy = titanium bar + 2 silver or gold alloy (60 s, 200 spark). Silver and gold gear need cotton cloth and a **bonding agent**; titanium gear an **enriched bonding agent**.
- **Gems and coils** (part 2): gems from tier 5 and 6 worlds of their type; 10 rough to 1 compact (compactor) to 1 refined (refinery); each **power coil** needs a compact gem and powers one machine of its type; gem gear needs 1,500 power.

## Decided in this spec

1. **Nine new planets join the default universe**, after the four that exist (the server's planet list only ever grows): one tier 4 world of each atmosphere line, and one tier 5 or 6 world of each gem type, as Release 207 gave every region. Names and looks are Boundmore's. Nathan can change the list before it is merged; once players save on a planet it stays.
2. **Atmosphere numbers are guesses** where Boundless's are unpublished (the air bar's length, the damage once it is empty): see rule 5.
3. **Atmosphere food protects against all three lines** (Boundless's recipe data names one line of foods per rank and none per atmosphere: **guess** that it covers every line).
4. **Lava waits for part 3.** The Metal-like and Burn-like worlds get dry basins (water held low) instead of lava seas until lava exists.
5. **Chisels stay Boundmore's** (families made from iron, M7). Silver, gold and titanium give no new chisels; gem chisels arrive in part 2 as the **advanced** chisels that can cut gem blocks and hard coal.

## Names (Boundless's in brackets, for identification only)

- Atmospheres: **acid air** [caustic], **charged air** [potent], **burning air** [volatile]. The ward skills already exist: **Acid Ward**, **Spark Ward**, **Fire Ward**.
- The air bar is **air**. Out of air you are **choking**.
- Planet kinds (internal name, shown name, tier, line, Boundless type):

| Kind | Shown | Tier | Air | Boundless type | Gem (part 2) |
|---|---|---|---|---|---|
| `overgrown` | Overgrown | 4 | acid | Lush | none |
| `rustland` | Rustland | 4 | charged | Metal | none |
| `cinderland` | Cinderland | 4 | burning | Coal | none |
| `acrid` | Acrid | 5 | acid | Corrosive | amethyst |
| `blighted` | Blighted | 5 | acid | Toxic | emerald |
| `glacial` | Glacial | 5 | charged | Chill | sapphire |
| `stormy` | Stormy | 6 | charged | Shock | topaz |
| `scorched` | Scorched | 6 | burning | Blast | diamond |
| `ember` | Ember | 6 | burning | Burn | ruby |

- New items: gold ore, gold bar, gold alloy, titanium ore, titanium bar, titanium alloy, medium coal, hard coal; compact, refined and machined gold alloy and titanium alloy; bonding agent, strong bonding agent [enriched]; silver, gold and titanium hammers, shovels, axes, grapples and spanners; spicy chilli, spiced rice [biryani], hearty pie [atmosphere pie] (Boundmore's words for the three atmosphere dishes).

## Rules, part 1

### Atmosphere

1. **A planet kind has an air line** (`air = "acid" | "charged" | "burning"`, none by default) and its **hazard** comes from its tier: 0 up to tier 3, then 1, 3, 5, 7, 10 for tiers 4 to 8 (`bm-core/src/atmosphere.rs`). The client learns each planet's line and hazard in the handshake with the planet list.
2. **Protection** against a line = the ward skill of that line (ranks give +1, +1, +1, +2, +2, so 1, 2, 3, 5, 7) + the atmosphere food buff (+1, +2, +3, against every line). The **shortfall** is hazard − protection, at least 0.
3. **The body carries it**: `PlayerState` gains `air` (the planet's line and hazard, set by the server on arrival) and `air_used` (ticks of air used, 0 full); `Body` gains `wards` (protection points per line, from the skills, set at an agreed input as every skill is). The shortfall is worked out inside `movement::step` from those and the food buff, so server and client agree and prediction stays exact.
4. **Air**: full is **30 s** (**guess**). Each tick with a shortfall, air falls by 1; with none, it refills at 4 a tick (**guess**: about 8 s from empty). A player who is down keeps their air as it is. Arriving on a planet (warp, portal, revive) keeps the current air, so hopping between hazardous worlds does not reset it; reviving after defeat fills it.
5. **Choking**: with air at 0 and a shortfall, every 5 s (the existing regen beat) the player takes **5% of max health per point of shortfall** (**guess**: one short dies in about 100 s after the air runs out; five short in 20 s), at least 1. Choking and an empty air bar stop out-of-combat healing (**guess**, as sickness does).
6. **Everywhere on the planet**: underground, indoors, in water (Boundless: "even indoors"). Water's own breath is not modelled yet.
7. **What players see**: an **air bar** above the energy bar while air is below full or there is a shortfall; on arrival a notice in plain words ("The air here is acid. You need Acid Ward 3 to breathe it; you have 1.") and the shortfall as a status icon with its number; fog drawing in toward the player as air runs out (the sky's fog distance scales from normal at full air down to 30 blocks at empty, **guess**). The planet list (travel window) shows each world's tier and air.
8. **Defeat** by choking is ordinary defeat (M13: XP debt, revive on safe ground). Safe ground on a hazardous world is still hazardous; the revived player has full air (about 30 s) to reach a shrine or portal.
9. **Ward skills become real**: Acid Ward, Spark Ward, Fire Ward (5 ranks each, 1 point a rank, Exploration), with Boundless's per-rank values. Their text names the points ("+1 protection against acid air" per rank, totals shown).

### The worlds

10. **Nine new kinds** in `assets/planets.toml` (the table above), each with its own sky, planet colour, block tints and palette choices from existing blocks (no new terrain blocks in part 1), and generation settings in the spirit of its Boundless type: Overgrown denser and taller than Lush; Rustland bare and rocky, dry basins; Cinderland dark ground and rich coal; Acrid pale and bleached; Blighted sickly greens and still water; Glacial frozen with ice seas; Stormy high mountains under a dark sky; Scorched a low sea and deep dry canyons; Ember dark ash ground, dry basins. The looks are Boundmore's and are **for Nathan to judge**.
11. **Default universe**: the nine join after Wildmere, in the table's order, each 48 chunks wide (**guess**, as Emberdune and Rimehold), with seeds 20261025 to 20261033. Every world gets a warp shrine; the travel window lists them.
12. **Creatures, meteors, regions** follow the tier rules already in place (creature tiers by world tier from M13, meteors on tier 3+ from M14, regions everywhere).
13. **Flora** on the new kinds: each takes the flora list of its nearest existing kind (Overgrown and Blighted the Lush list, Rustland, Cinderland, Scorched and Ember the Arid list, Glacial the Frozen list, Acrid and Stormy the Temperate list), so gathering works everywhere; bonus drops already follow world tier.

### Ores

14. **New seams** (blocks, `health = "seam"`, hammer material rock):
    - **Gold seam**: tier 4+ worlds; 10 to 20 above the floor, 25+ under the surface; drops gold ore with Boundless's bonus by tier (1.0 to tier 3, 1.5 on 4 and 5, 1.7 on 6 and 7, 2.05 on 8, tuning 4.4).
    - **Titanium seam**: Rustland and every tier 5 and 6 world; any height, 70+ under the surface; titanium ore, bonus 1.5 on tier 5, 1.8 on 6 and 7, 2.2 on 8.
    - **Medium coal seam**: every world, 20+ under the surface, densest 25 to 45 above the floor; medium coal, bonus by tier as tuning 4.4.
    - **Hard coal seam**: tier 3 and up, 50+ under the surface, 0 to 20 above the floor; hard coal, bonus as titanium.
15. **Existing planets** gain medium coal (all four) and hard coal (Wildmere, tier 3) appended to their ores, in chunks nobody has saved, as wayshards, relics and fossils were. Terrain already saved never changes. `terrain_fingerprint_is_pinned` is re-pinned for the appended kinds only if a fingerprint changes; nothing else about generation changes, so `GENERATOR_VERSION` stays.
16. **Fuel**: medium coal 250 spark over 20 s, hard coal 800 over 30 s in generators (M15's table already has them); in furnaces they give heat in proportion to coal's (**guess**: 2.5× and 8×).

### Metals

17. **Smelting** (furnace): gold ore → gold bar, 180 s, 400 heat; titanium ore → titanium bar, 240 s, 600 heat (Boundless's). XP and wear follow the iron and silver lines (**guess** where not in the data).
18. **Alloys** (mixer): gold alloy = 1 gold bar + 1 copper bar, 50 s, 100 spark; titanium alloy = 1 titanium bar + 2 silver alloy (or 2 gold alloy), 60 s, 200 spark.
19. **Compact, refined, machined** gold and titanium alloys (the M15 chain): compactor 10 → 1, 110 / 120 s, 400 spark; refinery 1 → 1, 50 / 60 s, 400 spark; workbench 1 → 1, 50 / 60 s, 400 spark.
20. **Bonding agents** (mixer): bonding agent = 2 glue + 2 fossil essence + 1 vital essence + 1 pulsating orb, 30 s; strong bonding agent = 4 glue + 10 fossil essence + 5 vital essence + 1 shimmering orb, 60 s, 200 spark (Boundless's ancient essence is our fossil essence; its fresh vital essence our vital essence).

### Gear

21. **Tools** (hammer, shovel, axe; workbench; Boundless's stats, tuning 2.1):

| Material | Damage | Speed | Crit chance / effect | Durability | Energy | Flex | Rank | Recipe (each) | Time, spark |
|---|---|---|---|---|---|---|---|---|---|
| Silver alloy | 650 | 30 | 0 / +20% | 1200 | 13 | 100 | 50 | 3 silver alloy, 4 sticks, 1 cotton cloth, 1 bonding agent | 240 s, 800 |
| Gold alloy | 700 | 10 | 50% / +75% | 1100 | 14 | 90 | 50 | 3 gold alloy, 4 sticks, 1 cotton cloth, 1 bonding agent | 480 s, 800 |
| Titanium alloy | 1000 | 25 | 20% / +40% | 1500 | 20 | 90 | 80 | 3 titanium alloy, 4 sticks, 1 cotton cloth, 1 strong bonding agent | 1200 s, 1500 |

22. **Grapples** (workbench; sticks swapped for relics: silver and gold 5 relic fragments, titanium 1 relic component + 5 fragments): silver alloy 1000 durability, 100 energy, shot 50, length 18, reel in 10, out 20; gold alloy 1000, 100, 30, 25, 10, 20; titanium alloy 1200, 120, 40, 20, 40, 40.
23. **Spanners** (workbench; as the tools plus 4 cogs): repair a hit silver 1500, gold 2000, titanium 6000 (Boundless's). Durability, speed and energy are **guesses** following the copper and iron spanners' pattern: silver 1200 / 30 / 13, gold 1100 / 30 / 14, titanium 1500 / 30 / 20.
24. Skill gates and XP follow M12's pattern (tool and tech crafting levels 2 for silver and gold, 3 for titanium, **guess** where the data is not to hand); wear on the workbench 14 (silver, gold) and 22 (titanium) (**guess**).
25. Every new gear item has its Flexibility and rank (above) so the forge (M17) takes it.

### Atmosphere food

26. **Spicy chilli** (rank 1, +1 protection, 1,500 hunger taken), **spiced rice** (rank 2, +2, 2,500), **hearty pie** (rank 3, +3, 5,000), each a 30-minute buff that replaces any other food buff (Boundless's numbers). Recipes (**guess**, Boundless's inputs are prime meat, spicy beans and an amalgam we do not have): chilli = 1 cooked prime meat + 4 spicy beans + 1 water bucket (furnace, 60 s, 100 heat, Cooking 2); spiced rice = 1 cooked prime meat + 6 spicy beans + 1 rice bag + 1 water bucket (furnace, 120 s, 200 heat, Cooking 3); hearty pie = raw pie (mixer: 2 cooked prime meat, 10 spicy beans, 1 flour, 1 baking powder, 1 milk bucket; Cooking 4) baked at the furnace (480 s, 300 heat).

### Formats

27. Protocol 26: `PlanetInfo` and `PlanetSummary` carry the planet's `air`; `PlayerState` carries `air` and `air_used`, `Body` carries `wards`. `SAVE_VERSION` 12: player records keep `air_used`, so leaving does not refill it (version 11 characters start with full air). Planet records gain their appended ores on opening (no format change).
28. **Fixed on the way**: a warp used to rebuild the traveller's body from nothing but their skills, so it healed them fully and cleared hunger, food buffs and sickness. Travel now moves the body and keeps everything else (portals already did).

## Acceptance, part 1

- On a tier 4 acid world with no Acid Ward, air drains in 30 s and then health falls every 5 s until defeat; with Acid Ward 1, nothing happens; eating spicy chilli covers the gap for 30 minutes. Server and client agree (no prediction corrections while choking).
- On a tier 6 world, Ward 3 (protection 3) still chokes, at 2 shortfall; Ward 4 (protection 5) is safe.
- Air comes back when protected and survives a warp unchanged; defeat refills it.
- The nine worlds exist in a new universe, each reachable by warp, each with its air shown in the travel window; existing universes gain them on opening (upgrade check).
- Gold, titanium, medium and hard coal appear at their depths on the new worlds; medium and hard coal appear in new chunks of old worlds.
- The chain from gold and titanium ore to a titanium hammer works through furnace, mixer, compactor-free alloys, extractor and workbench, and the hammer breaks tier 5 rock in Boundless's hit count.
- Grapples and spanners of each new metal work with their numbers; the forge accepts all new gear.
- HUD: the air bar, the shortfall icon, the arrival notice, fog closing in. `--demo-ui air` shows a choking player; screenshots of each new world.
- Upgrade check from save version 11; load test.

## Part 2: gems and power

*Rules written 2026-10-25, after part 1 was built.*

### Gems in the ground

29. **Six gem seams** (`health = "seam"`, mining XP a **guess** at 120), each on the tier 5 and 6 world of its type only (Boundless: gems are native to their world type, tuning 4.2), with Boundless's yields (1.0 to tier 4, 1.5 on 5, 1.8 on 6 and 7, 2.2 on 8; tuning 4.4) and its places (tuning 4.3, the developers' intent: Tobelawe, t/14255). Boundmore's sea sits at 128 rather than 64, so "above the floor" heights stay Boundless's but the water and hill rules are read against Boundmore's own sea level:
    - **Amethyst** (Violetmoor): along the edges of shallow water: columns under 1 to 8 blocks of water, or dry columns within 4 of such water (**guess** at "shallow"), 5 to 40 under the surface.
    - **Emerald** (Mirefen): under deep water: columns under 10 or more blocks of water, 2 to 60 under the bed.
    - **Sapphire** (Frostspire): inside hills: columns 20 to 80 above sea, 10 to 75 under the surface.
    - **Topaz** (Stormcrown): inside mountains: columns 80 or more above sea, 10+ under the surface.
    - **Diamond** (Sunscar): deep under mountains: columns 60+ above sea, 70+ under the surface.
    - **Ruby** (Ashfall): under sandy or gravel ground (Boundless: below sandy deserts; Ashfall's ground is gravel), 50 to 70 under the surface.
30. **Generation knobs** for these, each off by default so every existing ore keeps its terrain exactly: `max_depth` (deepest below the surface), `water_depth = [min, max]` (water over the column), `near_water` (blocks to the nearest such column, for the dry shore), `land_height = [min, max]` (the column's surface above sea), `under = [blocks]` (the column's top block). `GENERATOR_VERSION` stays: old ores read the defaults; the pins check it.
31. **Gem items**: rough gem (the seam's drop) → compact gem (compactor, 10 → 1, 240 s, 800 spark) → refined gem (refinery, 1 → 1, 120 s, 800 spark), Boundless's. Gem blocks for building wait for decoration.
32. **Hard blocks**: gem seams and hard coal have Boundless's hardness 10; the chisels Boundmore has (iron, M7) cut up to 3, so they are refused on them; part 2's **advanced chisels** cut anything.

### Power

33. **Power coils** (one block each, workbench; Boundless's recipe: 1 iron furnace base, 3 relic components, 3 machined copper, iron or gold alloy, 1 compact gem; 900 s, 800 spark; Machine Crafting 2). Which gem names the machine it powers: diamond workbench, ruby compactor, sapphire extractor, topaz refinery, emerald mixer, amethyst forge.
34. **Placing**: a coil powers one machine of its type that it faces across exactly one block of air (Boundless's 1 m gap), straight or diagonal in the horizontal plane, at the same height or one up or down (**guess** at the diagonal rule); a coil that could serve two machines serves the one with the lowest key (Boundless picked by direction; here by key, so it is stable). At most 24 coils count per machine. The station view shows the machine's power and coil count.
35. **Power** = 100 a working coil (advanced coils +300, below). Coils **wear**: each craft wears every coil that powers it by the recipe's wear (**guess**; Boundless's capacity is unknown, here 10,000 like a machine's), a worn-out coil gives no power until a spanner mends it, and spanners mend coils as they mend machines (a gem spanner mends its own machine's coils and that machine twice as fast, **guess** at Boundless's "most effective").
36. **Recipes may need power** (`power` in `recipes.toml`): a machine short of it shows "Needs N power" and waits. Boundless's thresholds where known: gem gear 1,500, advanced coils 1,500, compact coal 500 / 1,000 / 2,000 (soft, medium, hard), large fossil essence 500, purified and fortified buckets 1,500 and the powered dishes and forge ingredients as their data says.
37. **Power above the need** cuts the craft's time and spark, by the early-access formula (tuning 4.13.2, low-medium): the first 100 above cuts 50%; each further 100 adds 100 / (8 × need) percent (with a need of 0, read as 100); never below 25% of the base. Spark follows the same curve (**guess**; one player's series sits a few points above it).
38. **The forge**: its power share in M17's Effectiveness becomes real (the forge's power, from amethyst coils); the formula stays M17's (Measure 1 there).
39. **Advanced coils** (+300 power; workbench, 1,500 power): 1 titanium furnace base, 3 machined titanium alloy (or gold or silver alloy), 2 refined gems, 6 relic components, 1 relic core; 1,800 s, 2,000 spark. The **relic core** [ancient tech device] comes from a new **relic core seam** on every world, 75+ above the floor and 5+ under the surface (Boundless), appended to old planets' ores.
40. **The titanium furnace** (base and crucible, workbench, Machine Crafting 2): Boundless's recipe (6 titanium alloy and 16 flint; 6 titanium alloy), 180 s and 400 spark each; heat multiplier 0.25 (a smelt burns a quarter of the heat).

### Gem gear

41. **Leather** (refinery, 4 hide + 4 bark, 60 s, 200 spark, 500 power). **Bark** drops from trunks on tier 3+ worlds (3%, 4% from tier 6; Boundless).
42. **Gem tools** (hammer, shovel, axe), **grapples**, **spanners** and **advanced chisels**: Boundless's stats (tuning 2.1, 4.9), recipe 3 refined gem + 1 leather + 1 strong bonding agent (+ 4 sticks for tools and spanners, + 6 cogs for spanners, + 1 relic component and 5 fragments for grapples, topaz and sapphire 1 fragment), 1,800 s, 2,000 spark, 1,500 power, Tool or Tech Crafting 3. Advanced chisels: speed 80, durability 2,000, energy 20; the six cut kinds follow Boundless's wiki names onto Boundmore's families (amethyst square precise, diamond bevel, emerald bevel precise, sapphire square, ruby slope, topaz slope precise).
43. **Gem spanners** mend 10,000 a hit; each is "most effective" on its machine (rule 35).

### Formats, part 2

44. Protocol 27: the station view carries power and coils; recipes carry `power`. `SAVE_VERSION` 13: stations save their coils' wear (coils are blocks; their wear lives with the machine they power, keyed by the coil's block). Planet records gain the relic core seam on opening.

### Acceptance, part 2

- Each gem shows up on its own world in its place, and nowhere else; the old planets' terrain is unchanged except for relic cores in unsaved chunks.
- A workbench with 15 diamond coils (1,500 power) crafts a diamond hammer; with 14 it waits and says why; with 24 it is quicker and cheaper by the formula.
- Coils wear with crafts, stop at zero and come back with a spanner; the station view shows power and coils.
- A diamond hammer breaks tier 6 rock in Boundless's hit count; an iron chisel is refused on a gem seam and an advanced one cuts it.
- The forge's Effectiveness rises with amethyst coils.
- Upgrade check from save version 12; load test.

## Non-goals

- Lava, elemental creatures and elemental damage (part 3); exoworlds and tier 7 and 8 worlds; lucents and oortstone; world-specific creature lists (Boundless's were never published); breath under water.

## Measure in game (for Nathan, on a Boundless tier 4 world with no protection)

1. Seconds from arriving to the choking countdown (our air: 30 s).
2. The countdown's length, and health lost per tick after it, at 1 short and at 3 short (ours: 5% of max health per point every 5 s).
3. Whether air refills at once or over time when protected again (ours: about 8 s).
4. Whether choking stops health regeneration.
5. Whether atmosphere food (chilli) protects on every atmosphere line or only one.

## Status

*Part 1 built 2026-10-25; awaiting Nathan's play-test.* Protocol 26, `SAVE_VERSION` 12.

- [x] Core: `atmosphere.rs` (lines, hazard, wards, air, choking, the arrival warning, the fog distance) in `PlayerState` and `movement::step`; the Breathing food buff; ward skills real; 8 unit tests
- [x] Data: nine kinds (`assets/planets.toml`), four seams, medium coal appended to every old kind and hard coal to lush (re-pinned, `appending_m18_only_turns_rock_into_coal`); items, recipes, atmosphere food; the gravel worlds' plants also grow on gravel; Mirefen takes the temperate plant list (its beaches are flooded)
- [x] Server: the planet's air on the body at login and on every arrival; travel keeps the body (the warp fix); arrival and login warnings; saves keep air used (upgrade test from version 11)
- [x] Client: the air bar and its tag, fog closing in, each world's air in the travel window, `--demo-ui air`
- [x] Art (bm-texgen `worlds.rs`): four seams, 35 icons; `docs/media/m18-icons.png`
- [x] Verified: 662 tests (`acid_air_runs_down_as_predicted_and_a_ward_stops_it` end to end, with the client's replay matching the server exactly); upgrade check from save version 11 ("upgraded the universe from save version 11 to 12 (2 characters with full air; stations as they were)", each old planet "medium_coal_seam added to its ores", Wildmere also hard coal); a fresh character on Thornreach chokes in the real client (`docs/media/m18-thornreach.jpg`; the nine worlds `docs/media/m18-worlds.jpg`)

Open: the guesses (Measure list); load test with 50 bots on the new worlds; whether the new worlds' looks suit Nathan (`docs/media/m18-worlds.jpg`; the plaza round each shrine shows plain green turf even where the world colours its turf).

### Part 2

*Built 2026-10-25 (art in progress); awaiting Nathan's play-test.* Protocol 27, `SAVE_VERSION` 13.

- [x] Generation: four ore knobs (`max_depth`, `water_depth` with `near_water`, `land_height`, `under`), off by default; six gem seams on their worlds by Boundless's intent (`gems_grow_on_their_own_worlds_by_their_rules`); relic core seams on every world, appended to old planets' ores (re-pinned; `appending_m18_only_turns_rock_into_coal` covers them)
- [x] Core: `spark::power_scale` (the early-access curve, fitted to the guide's example and the measured marble series; a need of 0 read as 2,400, a guess), coil constants; `power` on recipes; `powers` and `coil_power` on blocks; `hard` blocks and `advanced` chisels (`hard_blocks_take_only_advanced_chisels`)
- [x] Server: `machines::machine_coils` (coils across one block of air, straight or diagonal, one level up or down; a coil serves the lowest-keyed machine it faces), stations hear their coils, wait for power ("Needs N power: add power coils"), scale time and spark by power and save the pace, wear coils with each craft; spanners mend coils; the forge's power feeds Effectiveness; the station view shows power and coils
- [x] Data: twelve coil blocks and their recipes (Boundless's: iron furnace base, components, machined metal, a compact gem; advanced: titanium furnace base, refined gems, a relic core, 1,500 power), the titanium furnace, bark from trunks on tier 3+, leather, compact coal (soft, medium, hard; 500, 1,000, 2,000 power), rough, compact and refined gems, gem tools, grapples, spanners and advanced chisels (1,500 power)
- [x] Client: the power line in the machine window; hard blocks refused to ordinary chisels in prediction
- [x] Verified: game tests pass (`coils_power_a_compactor_past_its_need` end to end: 4 coils wait, a fifth starts it, a spanner mends a coil); store upgrade test from version 12; upgrade check (a version 12 universe opened as 13: "0 stations rewritten with no power coils"; every planet gained relic cores, the gem worlds their gems); load test, 50 bots on four worlds, sandbox shared with a compiling agent: the M17 server averaged 0.5, 1.0 and 1.4 ms a tick over its three reports, this one 0.6, 1.2 and 1.3 (a first sample read 0.8, 1.2, 1.9): within the sandbox's noise
- [ ] Art: gem seams, coils, titanium furnace, relic core seam, gem items and gear (an agent is painting them; until then they fall back to block colours)

Open: every guess in part 2 (coil wear capacity and wear per craft, the curve for unpowered recipes, gem placement widths, mining XP); gem spanners are not yet "most effective" on their own machine (a flat 10,000 everywhere); the gem worlds' looks.
