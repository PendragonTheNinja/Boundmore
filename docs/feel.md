# Feel: numbers to calibrate against Boundless

Every value here was chosen as a sensible starting point, not measured from Boundless. Nathan (who plays it) checks each one in play and reports what is off; the fix is a one-line change.

## Movement (`bm-core/src/movement.rs`, `MoveConfig::default`)

| Value | Now | Check in Boundless |
|---|---|---|
| Walk speed | 3.0 blocks/s | **Nathan, 2026-10-20:** Boundless's untrained run, 3.0 m/s (tuning 1.6); Agility will add 0.3 a point |
| Sprint speed | 5.0 blocks/s | **Nathan, 2026-10-20:** Boundless's untrained sprint, 5.0 m/s; Agility will add 0.6 a point |
| Jump height | about 1.45 blocks (velocity 8.7, gravity 26) | Can you jump onto a 1-block ledge? A 2-block one? |
| Gravity | 26 blocks/s² | Does falling feel floaty or heavy? |
| Ground acceleration | 55 blocks/s² (full speed in about 0.14 s) | Is starting and stopping instant or eased? |
| Air control | 14 blocks/s² | How much can you steer mid-jump? |
| Swim speed | 3.2 blocks/s | |
| Player size | 0.6 wide, collides 1.9 tall, drawn 2.0, eyes at 1.75 | **Set by Nathan (2026-10-18):** two blocks tall; nobody fits under less than two blocks. Height sliders in Boundless are visual only |
| Reach | 4 blocks (`ServerConfig::reach`) | **Nathan, 2026-10-20:** Boundless's untrained reach, 4 m; Control will add 1 a point |

## Grapples (`assets/items.toml`; rope feel in `bm-core/src/movement.rs`)

Matched to Boundless's final numbers (Nathan, 2026-10-19: Boundless first; `docs/research/boundless-tuning.md` 1.1 to 1.4, from boundlessinfo.com's item data, checked against in-game tooltips). Ranges in blocks, speeds in blocks per second.

| Value | Now | Source, or guess |
|---|---|---|
| Copper grapple | range 10, shot 45, reel in 5, reel out 15, durability 500 | Boundless copper, exact |
| Iron grapple | range 20, shot 30, reel in 2, reel out 10, durability 1000 | Boundless iron ("long range"), exact |
| Wear | one per shot and one per release by firing again; putting it away is free | Boundless, medium |
| Hookshot | no base grapple; the ability waits for forging (a boon there, +20 reel in) | Boundless 1.0+ |
| Speed cap | 40 for everything; terminal fall 40 (was 55) | Boundless dev, high |
| Reel gathering | 20 blocks/s² up to the reel speed (`REEL_ACCEL`) | derived from one player's measurement, medium |
| Swing lean | 9 blocks/s² (`swing_accel`) | guess: Boundless's rope constants are known but not what they mean |
| Fling fade | 0.4 of the speed per second (`fling_drag`) | guess |
| Shortest rope | 1 block (`ROPE_MIN`) | guess |

**Nathan, 2026-10-20 (first play):** copper and iron grapples "feel really nice for their tier" and "the physics seem pretty sound". He is used to fully forged rift grapples (40 base range, up to about 120 with boons, mastery and guild buffs, tuning 1.3), which wait for exotic tiers and forging.

Not built yet, from Boundless: energy per shot (copper 60, iron 80, of a 600 bar), grapple mastery (+5 range and +5 shot speed per rank, 5 ranks), forge boons (+3 range, +5 shot, +5 reel per level), no fall damage while attached.

## Portals (`ServerConfig`, `assets/planets.toml`; M8, ADR-0009)

Boundless burned shards every hour whether a portal was used or not (1 to 9 an hour by size, 50 to 450 to open). Boundmore charges per trip instead (Nathan, 2026-10-19), so these are not comparable one for one: check how long a stocked hub lasts in practice.

| Value | Now | Check |
|---|---|---|
| Trip cost | 1 wayshard on the same planet, 2 between planets (`portal_trip_cost_same_planet`, `portal_trip_cost_other_planet`) | Does a busy hub drain too fast, or never? |
| Charge cooldown | 1 hour per player per portal (`portal_charge_cooldown`, seconds) | Back-and-forth trips should feel free |
| Tank | 500 wayshards (`portal_tank_capacity`) | How often does an owner restock? |
| Portal sizes | 1 to 6 wide, 2 to 6 tall | Boundless went up to 24 conduits for its longest links |
| Wayshard rarity | vein size 6, threshold 0.64, below y 68 and 24 under the surface: about 9 blocks per 32x32 column, heights 1 to 68 (median 30) under a surface near 147, about 30 times rarer than copper (measured 2026-10-20) | Nathan, 2026-10-20: found none in his first play, but keep them where they are (he will dig for them) |
| Conduit recipe | 4 stone, 1 copper ingot, 1 wayshard for 2 | A 1 by 2 portal costs one wayshard |

## Mining (`assets/items.toml`, `assets/blocks.toml`, `bm-core/src/mining.rs`; M9)

Matched to Boundless's final numbers (Nathan, 2026-10-20: "as close to Boundless as humanly possible"; `docs/research/boundless-tuning.md` 2.1 to 2.5a, 4.4). Exact unless marked guess.

| Value | Now | Source, or guess |
|---|---|---|
| Block health | soft 800 (soil, sand, snow, foliage), hard 1200 (rock, gravel, ash, wood, ice, crystal, glass, machines), seam 2400 (ores), x1, 1, 1.5, 2, 3, 4, 5, 6 by tier | Boundless, exact. Snow as soft, glass and machines as hard: **guesses** (no source) |
| Armour | 0, 0, 100, 200, 400, 600, 800, 1000 by tier, off every hit | Boundless, exact |
| Time between swings | a13o's curve: 1 s at action speed 5 to 0.05 s at 150 (copper 0.59 s, stone 0.82 s, iron 0.91 s) | Boundless, exact (a13o's sheet) |
| Tools | wood 300 damage / speed 20 / 600 uses; stone 400 / 15 / 600; copper 400 / 30 / 700; iron 700 / 10 / 900, 20% crits at +20% | Boundless, exact |
| Hand stone | 200 / 20 / 400, full damage on everything, crafted from nothing | Boundless's starter tool, exact |
| Bare hand | cannot break blocks; everyone carries the hand stone (crafted from nothing) | **Nathan, 2026-10-20:** in Boundless a bare hand cannot break blocks |
| Wrong tool | a quarter of the damage after armour | Boundless, medium (players' measurements) |
| Healing | a block left alone 5 s heals to full at once | **Guess.** How long did a damaged block stay damaged? |
| Bonus ore | copper, iron, coal +0.5 per seam on tiers 1 and 2, +0.7 on 3 and 4, +1.05 from 5; wayshards (ours) +0.5 from tier 5 | Boundless, exact for its ores; wayshards modelled on its gems |
| Flint | rock 1.5% (tiers 1, 2), 2% (3, 4); gravel 5% | Boundless, exact |

Not built yet, from Boundless: energy per swing (damage / 50), skills (power, mastery, epics, dexterity, luck), forging (the only source of area of effect), mining XP.

## Energy (`bm-core/src/energy.rs`, `assets/items.toml`; M10)

| Value | Now | Source, or guess |
|---|---|---|
| Bar | 600 | Boundless base (Zeal raises it later) |
| Costs | tools damage / 50 (hand stone 4, wood 6, stone and copper 8, iron 14); copper grapple 60, iron 80 a shot; chisels 10 a cut | Boundless, exact |
| Pause before refill | 3.5 s after the last use; a tired use restarts it | players timed 3 to 4 s; the restart is Release 199 |
| Refill | 300 a second (empty to full in 2 s) | **Guess.** How fast did an empty bar refill with no skills? |

## Light (`assets/blocks.toml`, M9)

| Value | Now | Check |
|---|---|---|
| Torch | light 10 (reaches 9 blocks), placed on floors and walls or held | **Guess.** How far did a basic torch light, next to a glowing crystal? |
| Glow crystal | light 12 | Boundless gleam was the reference for "fully lit" |
| Torch recipe | 1 stick + 1 flint makes 4, by hand; 1 trunk makes 4 sticks | Boundless, exact |

Not built yet, from Boundless: the better torch (tallow and sackcloth, at a workbench), gem torches (tinted light, brighter), lanterns (refinery: torch and glass), the glow forge boon (6, 9 or 12 m), the light-source skill.

## Gathering and crafting (`assets/planets.toml` flora, `assets/recipes.toml`; M11)

| Value | Now | Source, or guess |
|---|---|---|
| What each plant, boulder and fungus drops | tuning 4.4a (e.g. spikeleaf 1.3 fibrous leaves, a boulder 1.3 flint) | Boundless, exact |
| Where each grows, and how thick | spikeleaf on sand and dry patches (starter world too), frostbell on snow, nightlily by water, the rest on turf; boulders everywhere; fungi in caves, the glowcap deepest; about 20 plants and 5 boulders a minute's walk on the starter world | **Guess** (Boundless's placement is not published). Does it feel like a Boundless world? |
| Recipes, times, heat | Boundless's, item for item, up to iron (spec table) | Boundless, exact |
| Glue | 2 sap | **Stand-in** until creatures bring bone (Nathan, 2026-10-20); M13 replaces it with 1 sap + 1 bone |
| Relic seam | as common as copper about 0.7 to 1 | **Guess** |
| A plant whose ground breaks | falls, its drops to the breaker | **Guess** |

## Skills and progression (`bm-core/src/skills.rs`, `assets/skills.toml`; M12)

Boundless's live tree and tables exactly (Nathan, 2026-10-20), in Boundmore's names. Since 2026-10-21 the skill numbers come from the game's own skill text (boundlexx, tuning 5.4a), which replaced most of the earlier guesses.

| Value | Now | Source, or guess |
|---|---|---|
| Levels, points, marks, cleanses, pages (600), resets (800), plots (30) | Boundless's tables | Boundless, exact |
| XP per block, per craft | Boundless's per-block and per-recipe data | exact; wayshard mining 64 and gathering objects scaled by tier are **guesses** |
| Stamina (Zeal) | max energy 600 + 1400 x (level / 9)^1.5: +50, 100, 120, 140, 170, 180, 200, 210 a rank, 2,000 at 9 | Boundless's skill text, exact (the curve fits every rank) |
| Deep Reserves (energy epic) | max energy x3 | Boundless's skill text, exact |
| Second Wind (energy regeneration epic) | +900 a second | Boundless's skill text, exact |
| Hammer and shovel-and-axe epics | +25% damage, +10 action speed | Boundless's skill text, exact |
| Grasp (Control) | reach +0.75 a level (4 to 10.75) | Boundless's skill text (the wiki said +1; the game's text wins). Does trained reach feel right? |
| Grapple Skill (mastery) | +5 blocks of rope and +5 a second shot speed a rank | Boundless's skill text, exact |
| Careful Hands (durability epic) | 30% less wear (as 30% of swings not wearing) | Boundless's skill text, exact |
| Base energy refill | 300 a second after the 3.5 s pause | **Guess** (not in any data found) |
| Inner Light (light epic) | light 10 around you, off outdoors in daylight | **Guess** at the radius (the text gives none) |
| Spring (high jump) | 2 blocks higher | Boundless's skill text ("hold jump to rise higher": we give the full 2 on any jump) |
| Second Leap (double jump) | 0.7 of a jump's speed in mid-air | **Guess** (the text gives no height) |
| Swiftness crit | 0.2 x level / 9 (20% at 9) | Boundless's damage calculator; the skill text says 2% a level (18% at 9) |

## World (`bm-server/src/config.rs`, `bm-worldgen`)

| Value | Now | Check |
|---|---|---|
| Planet width | 2 048 blocks around | How long to walk all the way round a small world? |
| Planet height | 384 blocks, sea level 128 | Deepest caves, tallest mountains |
| View radius | 10 chunks (320 blocks) | How far can you see terrain detail? |
| Edit rate | 12 per second, burst 24 | Fast building feel |

## Planets (`bm-server/src/config.rs`, `assets/planets.toml`)

| Value | Now | Check |
|---|---|---|
| Planet widths | Verdance and Wildmere 2 048 blocks, Emberdune and Rimehold 1 536 | Do small worlds feel small enough to know, big enough to explore? |
| Day length | Verdance 20 min; the others 15, 30 and 25 min | Boundless days varied by planet; do these feel right? |
| Shrine guard | 4 blocks around each warp shrine | Enough to keep it reachable without wasting the spawn |

## Not built yet, but to measure while playing

- Day length.
- How dark caves are without light.

## Combat (M13, `docs/milestones/13-creatures-and-combat.md`)

| Value | Now | Source, or guess |
|---|---|---|
| Health | 600, Vitality curve to 4000, Iron Body x3 | Boundless, exact |
| Defeat | XP debt 10% of the next level, half XP until paid; nothing lost | **Nathan, 2026-10-21:** Boundless exactly |
| Regen (stand-in) | 1.4 a second after 10 s out of combat (600 in about 7 min) | **Nathan, 2026-10-21:** stand-in until food. Rate from player reports; the 10 s is a guess |
| Fall damage | share of max health by impact speed, softer on soft blocks, slightly gentler than Boundless | **Nathan, 2026-10-21.** Curve is a guess: measure 10, 20, 30 block falls in Boundless |
| Hooked landing | no fall damage while a hook is attached | Boundless, and Nathan |
| Grace after letting go | the first landing within 3 s of releasing a hook takes no fall damage | **Nathan, 2026-10-21** (momentum deaths from early releases); 3 s is a guess |
| Health, armour, XP and drops by creature tier | `assets/creatures.toml` | Boundless, exact (tuning 3.2.2, 3.2.4) |
| Grazer | walk 1.4, charge 12 blocks/s over up to 15, hit radius 1.8, wind-up 0.8 s, 3 s between charges; notices at 16, gives up at 32 or 12 s unseen | charge speed, range, radius: Boundless (Release 172). The rest: guesses |
| Pelter | walk 3.0, run 5.5; shots 22 blocks/s from up to 24 blocks, a volley every 2 s (1 shot, 3 at tier 2, 5 at tier 3+, 8 degrees apart); notices at 20, gives up at 30 | give-up 30: Boundless (Release 156). The rest: guesses |
| Dasher | walk 1.6, sprint 9.0; notices at 14 | guesses |
| Creature hits | grazer 100, 250, 600, 1500, 3600; pelter 30, 90, 180, 360, 720 (tiers 1 to 5) | tiers 1 and 2: players' measurements (2023). Higher tiers: guesses |
| Luck on creature drops | multiplies the number of rolls | guess |
| Sling shot drop | gravity 20 x fall-off (a stone sling's shot drops about a block over its 15) | guess |
| Draw | holding charges over 2 s to double damage | Boundless (charge time 2, charge effect 1) |
| Respawn health | half of max | guess (a player complained of "half", 2018) |
| Spawning | 24 to 48 blocks from a player, out of sight; at most 8 within 48 blocks of a player, 600 a planet; gone 10 s after no player is within 64; a kill's 32x32 column stays empty 5 minutes; bodies last 60 s | guesses (`crates/bm-server/src/game/creatures.rs`, `ServerConfig`) |
| Grazer charges | 1 per run-up at tier 1, 2 at tier 2, 3 from tier 3; knockback lifts 5 blocks a second | guesses (Boundless: tier 1 once, more at higher tiers) |
| Pelter distance | keeps 7 to 14 blocks from its target, aims 0.4 s before a volley | guesses |
| Hit rewind | shots and strikes judged against creatures where you saw them, up to 12 ticks (0.2 s) back | not a feel number: lag fairness |

## Regions and meteorites (M14, `docs/milestones/14-meteorites.md`)

| Number | Value | Source |
|---|---|---|
| Region spacing | centres 512 blocks apart (16 on a 2048-block planet) | Boundless: 32 to 34 regions on 3 km worlds, up to 64 on 4 km ones (forum world announcements) |
| Region centre | the HUD shows the name alone, and a meteor may be called, within 40 blocks | guess |
| Discovery XP | 50 x n for the n-th region discovered on a planet | Boundless (tuning 5.5) |
| Region rest | 2 to 3 hours after a meteor | Boundless (players' reports) |
| Wandering trigger | entering a 300 x 300 square not entered for 4 hours: a 1 in 4 chance | square: Boundless (players); rest and chance: guesses |
| Meteors a planet | at most 3 active at once | guess |
| Level | 1 + log2(players within 80 blocks), 1 to 6 | guess shaped to Boundless's 1 to 100 players and levels 1 to 6 |
| Waves | 2 + level waves of 2 + level creatures; the last wave leans to the top tier | guess |
| Fall | 6 s, landing 60 to 140 blocks from the trigger | guess |
| Zone and lives | 48 blocks; 10 s to come back | zone: guess; 10 s: Boundless |
| Dormant | 20 minutes after impact if not beaten, or when no life is left | timeout: guess; lives: Boundless |
| Heart | open 30 minutes; 4 + 3 x level wayshards (+25% a tier above 3), 1 to 3 rolls of ores, relics and creature parts; dormant: a quarter of the wayshards, 1 roll | guess (`assets/meteors.toml`) |
| Shell | every fourth block broken gives a portal conduit | guess (Boundless's meteorites were made of conduits) |
| Stuck creatures | a wave creature that moves under half a block in 8 s with nobody within 4 blocks drops back in through a portal | our rule (a creature in a pit by the shell made a meteor unbeatable) |

## Spark and machines (M15, `docs/milestones/15-spark.md`)

| Number | Value | Source |
|---|---|---|
| Fuel | spark = heat value; burns stick 1 s, timber 3.5, trunk 7, soft coal 10 | Boundless `fuelData` (tuning 4.7) |
| Generator store | 5,000 spark a block, joined blocks add up | guess (not published) |
| Link limit | 100 links between generator and machine | Boundless (wiki Spark Link) |
| Machine wear | 10,000 when repaired; recipes wear copper 7, iron 11, machines 29, cog 6, silver alloy 12, cotton 35, sackcloth 6, extractor 14 | capacity: guess (players: 10 to 15 gem spanner hits for a machine and its coils); wear: wiki recipe boxes, the rest follow the pattern (guesses: compact 10 to 14, refined 8 to 12, machined 8 to 12, tools 7 and 11) |
| Spanners | stone 100 wear a hit (speed 40, 600 uses, 4 energy), copper 400 (70, 500, 6), iron 700 (30, 900, 8); Spanner Mastery +10% and +2 speed a rank | Boundless item attributes (boundlessinfo) |
| Iron furnace | half the heat | Boundless (wiki Furnace) |
| Silver | tier 3+, 10 to 40 above the floor, 20+ below the surface; one ore a block to tier 3 | Boundless (tuning 4.3, 4.4) |
| Relic components | every tier, 40+ above the floor, 5+ below; rarer than fragments (size 9, threshold 0.63) | placement: Boundless; richness: guess |
