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

*(Rules written when part 1 is built; outline here so part 1 leaves room.)*

- **Gem seams** on their worlds (amethyst on Acrid, emerald on Blighted, sapphire on Glacial, topaz on Stormy, diamond on Scorched, ruby on Ember), placed by Boundless's intent: amethyst along the edges of shallow water, emerald under deep water, sapphire inside hills, topaz inside mountains, diamond deep under mountains, ruby under sandy ground. Generation gains the knobs these need (deepest depth, water above, height above sea).
- **Gem items**: rough → compact (compactor 240 s, 800 spark) → refined (refinery 120 s, 800 spark). Gem seams and hard coal need an advanced chisel to shape.
- **Power coils**: one per machine type, each needing a compact gem of its machine's gem (diamond workbench, ruby compactor, sapphire extractor, topaz refinery, emerald mixer, amethyst forge); +100 power each, 24 at most, one block's gap from the machine; they wear and are mended with spanners.
- **Machine power**: a recipe may need a minimum power; power above it cuts time (the early-access formula: the first 100 above halves it, each further 100 adds 100 / (8 × requirement) percent, never below 25%) and spark (**guess**: the same curve).
- **Gem gear** (1,500 power): tools, grapples, spanners and advanced chisels, Boundless's stats; leather (refinery, 500 power).
- **The titanium furnace**; the forge's power share (M17's Effectiveness formula, refitted); the powered forge ingredients get recipes.
- Advanced coils (+300) need titanium furnace bases and relic devices, which need large fossils and deep relics: decided in part 2.

## Non-goals

- Lava, elemental creatures and elemental damage (part 3); exoworlds and tier 7 and 8 worlds; lucents and oortstone; world-specific creature lists (Boundless's were never published); breath under water.

## Measure in game (for Nathan, on a Boundless tier 4 world with no protection)

1. Seconds from arriving to the choking countdown (our air: 30 s).
2. The countdown's length, and health lost per tick after it, at 1 short and at 3 short (ours: 5% of max health per point every 5 s).
3. Whether air refills at once or over time when protected again (ours: about 8 s).
4. Whether choking stops health regeneration.
5. Whether atmosphere food (chilli) protects on every atmosphere line or only one.

## Status

*Spec written 2026-10-25. Part 1 in progress.*

- [ ] Core: `atmosphere.rs` (lines, hazard, protection, air, choking) in `PlayerState` and `movement::step`; unit tests
- [ ] Data: nine kinds, four seams, appended coal on old kinds, items, recipes, ward skills real, atmosphere food
- [ ] Server: shortfall set before each step; planet list carries air; arrival notices; saves keep air
- [ ] Client: shortfall set before prediction; air bar, shortfall icon, fog, travel window air; `--demo-ui air`
- [ ] Art: four seams, new item icons, screenshots of each world
- [ ] Verified: tests, upgrade check from 11, load test
