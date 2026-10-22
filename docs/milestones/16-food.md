# Milestone 16: Food

*Spec written 2026-10-22. Nathan picked food next (2026-10-22: "Food", over forging and gems). Boundless first: recipes, times, heat, XP and skill gates come from the game's own data as the community published it (boundlessinfo `compiled-recipes.json`, fetched 2026-10-22); food values and buffs from the developers' patch notes (Release 207, 2018-11; Release 223, 2019-05) and the players' food index (forum t/17271, 2018-09, medium); crop drops from pfiffel's `dataBlockDrops.js`. Tuning 3.9 holds the full tables. Numbers marked **guess** are on no site; the Measure list at the end is for Nathan.*

## Goal

Using tools, running and jumping make you **hungry**, and hunger eats into your energy bar. **Food** takes the hunger away. Cooked food beats raw, raw meat makes you **sick**, and dishes cooked at the furnace carry a **food buff** for 10 to 60 minutes: never hungry, more health, more energy, faster energy refill, more crits, less tool wear, or armour. Being **full** is what heals you out of combat, replacing M13's stand-in. Going hungry long enough **starves** you.

Part 1 is hunger, eating, buffs and every dish whose ingredients exist on Boundmore's worlds today. Part 2 is **farming**, which brings oats, rice and wheat (porridge, risotto and loaf: ranks 3 and 4; pies wait for power) and fixes Boundless's farming complaints.

## How Boundless did it

### Hunger (Release 187, 2018-02; reworked in Release 207, 2018-11)

- "The total energy is reduced by the character's current hunger level." Hunger is a number of points; your energy bar's usable size is max energy less hunger, drawn as a darker end of the bar. "Players should eat food to increase their max energy." [Release 187 notes, t/11861]
- Hunger grows with item use, moving, jumping and sprinting. Each item has a **hunger drain per use**: tools, totems and spanners exactly 1/100 of their energy cost, grapples and slingbows 1/500 (boundlessinfo `item-attributes.json`, tuning 1 and 2). [tuning 1.7]
- Release 207: "Hunger depletes at an equal rate of points for all players. Meaning higher energy players can run for longer, but eating food will allow for the same time of running." The rate itself is published nowhere.
- **Full Up**: while hunger is under 10% of max energy, health slowly returns. The food index says 4 every 5 s (2018); a player timed a full 600 bar back in about 7 minutes out of combat (7 every 5 s, t/16766). M13's stand-in uses the second.
- **Starving**: past full hunger a status deals damage until you eat; players were defeated by it (t/14148, t/15359).
- Defeat adds "a hunger penalty" (Release 195). Its size is not published.

### Eating

- Food is held and used like any item; one use eats one. "Only one food buff and one brew buff can be active at one time ... Consuming a second piece of food will remove any existing food buff." [Release 187] Plain food with no buff leaves the current buff alone; that was the point of tuber dishes after Release 223.
- Raw meats (and raw unbaked dishes) make you **Sick**: 5 damage every 5 s for 40 s, and no other healing meanwhile. [t/17271]

### Foods (hunger taken away; buff)

Single items, raw / cooked: berry 100 / 200; tuber 50 / 250; waxy tuber 50 / 350; exotic tuber 50 / 350; meat 100 (sick) / 400; lean and prime meat 150 (sick) / 500; edible lamella 50 raw. [t/17271]

Dishes, rank 1 to 5 (soup or broth, stew or casserole, porridge or risotto, loaf, pie):

| Line (Boundless) | Hunger taken | Buff | Per rank | Lasts |
|---|---|---|---|---|
| Meat (meaty broth...) | 1200, 1600, 2400, 3200, 4000 | **Well Fed**: no hunger gained | | 10, 20, 30, 40, 60 min |
| Tuber (earthyam soup...) | 2000, 2500, 3500, 4500, 5500 (Release 223) | none | | |
| Berry (starberry soup...) | 600, 800, 1200, 1600, 2000 | **Recovery**: energy refills faster | +10, 20, 40, 120, 300 a second (Release 223) | **guess** 30 min |
| Waxy tuber (energising) | 1000, 1400, 2100, 2800, 3500 | max energy | +20, 40, 60, 80, 100% | 30 min |
| Lean meat (criticaling) | 1500, 2000, 3000, 4000, 5000 | crit chance | x1.2, 1.4, 1.6, 1.8, 2.0 | 30 min |
| Exotic tuber (persisting) | 1000, 1400, 2100, 2800, 3500 | tool wear | -5, 10, 15, 30, 40% | 30 min |
| Prime meat (shielding) | 1500, 2000, 3000, 4000, 5000 | armour and resistance | +10, 20, 40, 70, 100 and +5, 10, 15, 20, 25% | 30 min |
| Glossy berry (invigorating) | 900, 1200, 1800, 2400, 3000 | max health | +20, 40, 60, 80, 100% | 30 min |
| Juicy berry (floating) | 900, 1200, 1800, 2400, 3000 | jump and falling armour | +0.5, 0.8, 1.1, 1.5, 2.0 m; +50, 75, 100, 150, 200 | 30 min |
| Sweet bean (teaching) | 1000, 1500, 2000, 2500 (4 ranks) | XP | +10, 20, 50, 100% | 30 min |

Elemental shielding and atmosphere protection foods wait for elements and atmospheres.

### Where ingredients come from

- **Berries** drop from foliage: plain on tier 1 to 3 (10% a block), glossy on tier 4 and 5 (3%), juicy on tier 5 to 8 (3%). Every tier also has berry vines hanging under foliage (part 2). [pfiffel drops]
- **Tubers** drop from soils: 1.5% a block on tier 1 to 4 (1% more on tier 3), waxy from tier 4. [pfiffel drops] Boundmore's plants already drop them (M11).
- **Meat** from creatures (M13). **Edible lamella** from fungi (M11). **Rock salt** from boulders (M11).
- **Buckets** (workbench: 1 timber, 6 nails; nails 4 from 1 iron bar) are filled at water and hold 10 fills; a machine that uses a full bucket hands the empty one back. [WIKI Bucket, data]
- **Syrup** (extractor: 2 sap, or 4 berries), **milk** (extractor: milk gland + bucket), **sugar** (refinery: 4 syrup + sackcloth, cooking 2). [data]
- Bags of oats, rice and flour need grain from crops; butter needs spark and power. Part 2.

## Rules (part 1)

1. **Hunger** lives in `PlayerState` as hundredths of a point, stepped in `movement::step` on both sides, and is saved with the character. The usable bar is `max_energy - hunger` (never below 0); energy spent beyond it is lost. Hunger never exceeds max energy.
2. **What makes you hungry**: each use adds its item's hunger drain (tools, chisels and spanners: energy cost / 100; grapples, slingbows and bombs: energy / 500, Boundless's data). Moving at run speed on the ground adds **0.5 a second**, sprinting **1 a second**, each jump **1** (all three **guesses**). Standing, swimming and hanging on a rope add nothing (**guess**). Defeat adds **25% of max energy** (**guess**).
3. **Eating**: with food selected, the use button eats one (`ClientMsg::Eat { at, slot }`), at most one a second (**guess**). Both sides apply it right after input `at` (as skill changes are), so prediction agrees. Eating takes its hunger value off, and sets its buff if it has one, replacing the old buff. Food with no buff leaves the buff alone. Eating at zero hunger with no buff to gain is refused ("You are full").
4. **Buffs**: one food buff at a time, with rank and ticks left, in `PlayerState`, saved. While it lasts: **Sated** (Boundless's Well Fed) stops hunger growing; **Refreshed** (Recovery) adds its energy a second to the refill; **Lively** multiplies max energy; **Keen** multiplies crit chance; **Gentle** gives that chance for a use to cost no durability; **Guarded** adds kinetic and impulse armour and resistance; **Hearty** multiplies max health; **Springy** adds jump height and falling armour; **Studious** multiplies XP earned.
5. **Full and healing**: M13's calm regeneration (7 every 5 s after 10 s out of combat) now happens only while hunger is under 10% of max energy ("Full"). Mending is unchanged.
6. **Sick**: eating raw meat, lean or prime meat sets 40 s of sickness: 5 damage every 5 s, and no calm regeneration or Mending meanwhile. Sickness does not replace the food buff.
7. **Starving**: at full hunger, 2% of max health every 5 s until you eat (**guess**); it can defeat you.
8. **Recipes** as in tuning 3.9 (Boundless's single craft, times, heat, XP, cooking level), for every dish whose ingredients exist: cooked singles; berry soup and stew; tuber, meat, waxy tuber, lean meat, exotic tuber and prime meat soups/broths and stews/casseroles; nails, buckets, syrup, milk and sugar. A recipe input that is a full bucket gives its bucket back with one more fill used; at 10 it is gone.
9. **Buckets** fill from water: use an empty bucket on a water block (it stays: Boundless's water was not used up, **guess**). Water buckets stack with others of the same wear.
10. **Berries** drop from foliage (plain, 0.1 a block on tier 1 to 3) and **tubers** from soil and grass (0.015 a block on tier 1 to 3, plus 0.01 more on tier 3), as bonus drops; no terrain change.
11. **HUD**: the energy bar shows hunger as a darker end; the buff shows as an icon with time left, Full, Sick and Starving as status icons; food tooltips show hunger taken and the buff.
12. **Formats**: protocol 22 (state, `Eat`, food data in the item catalogue), `SAVE_VERSION` 9 (hunger and buff saved; version 8 opens with no hunger and no buff).

## Acceptance (part 1)

- Mining, running and jumping fill hunger; the bar's usable part shrinks; eating cooked meat gives 400 back.
- Raw meat makes you sick: damage ticks, no healing, for 40 s.
- Out of combat you heal only while Full; starving damages until you eat.
- A meat broth stops hunger for 10 minutes; a new dish replaces its buff; a tuber soup does not.
- Each buff does what it says, each with a unit test (max energy and health, refill, crit, wear, armour, XP).
- Buckets fill at water, are used by soup, come back worn, break at 10.
- No prediction corrections while eating, running or mining with buffs (bots and the title bar count).
- `--demo-ui food` shows the HUD with hunger, a buff and the food tooltips; screenshots.
- Upgrade check from save version 8.

## Part 2: farming

*Detailed 2026-10-22. Nathan (2026-10-22): fix every farming complaint ("farming was implemented but never fully finished"); where Boundless's way is unknown, do what is best in the long run. How crops grow and are saved: ADR-0010.*

### How Boundless did it (Testing 224, forum t/31046, t/31322; tuning 3.9)

- Crops grow from seeds sown on (or, for vines, under) blocks; wild crops spawn on worlds and drop seeds and some yield. Organic crops: berry vines, tuber plants (three kinds), oats, rice, wheat; inorganic and decorative crops besides.
- A crop ticks once a game day through up to 8 stages, each tick a dice roll; expected times run from about 2 to 4 hours (tubers) to days. Fertiliser raises the chance and is itself rolled away. Rice grows very slowly out of water. Tilled soil (hand tiller) improves growth and yield, but reverts. Harvesting a ripe crop drops its seed (always one, a chance of more) and its yield (a chance); an unripe one gives its seed back. Crop sowing levels (the Farming skill) gate the better crops.
- Complaints (`research/boundless-forum-feedback.md`): random growth, fertiliser lost without effect, tilled soil untilling itself, replanting every harvest by hand, low XP.

### Rules (part 2)

13. **Crops** (`assets/crops.toml`, data): dewberry bush, tuber, waxy tuber, exotic tuber, oats, rice, wheat. Each has a seed item, a yield item, four stage blocks (`<crop>_1` to `<crop>_4`, props like the gathering plants; `_4` is ripe), a base time to ripen, the ground it is sown on, and the Farming level it needs. Boundless's berry vines hang under foliage; ours stand on soil until props can hang from a ceiling (a later change, noted in Non-goals).
14. **Growth by the clock** (ADR-0010): stage = time since sowing over the time to ripen, split into four. Nothing is rolled. Times to ripen (**guesses**, from players' reports of 2 to 4 hours for tubers and "hours to days" for the rest): dewberry 3 h, tuber 3 h, waxy tuber 4 h, oats 4 h, exotic tuber 6 h, wheat 6 h, rice 6 h. Tilled soil takes a quarter off; rice not beside water takes four times as long (Boundless's rice rule, factor a **guess**). A ripe crop never withers.
15. **Tilling**: a hand tiller (wood: crafting table, 2 timber, 1 stick, 1 foliage; metal: workbench, 3 copper or iron bars, 4 sticks, 1 cotton cloth, 200 spark, Toolmaking 1; Boundless's recipes and stats) swung at soil or turf with air above turns it into **tilled soil**, which stays tilled until broken. A tiller swung at a crop harvests it.
16. **Sowing**: placing a seed on the top of soil, turf or tilled soil (rice also beside water counts) plants its crop at stage 1 above it. Turf becomes soil under a crop (Boundless). Needs the crop's Farming level. 1 XP a seed.
17. **Harvesting**: breaking a crop. Unripe: its seed back. Ripe: its seed and a 35% chance of a second, its yield (dewberry and tuber 1 plus a 10% chance of another; waxy and exotic tubers and grains a 20% chance plus a 10% chance of another; Boundless's drop data), half again with tilled soil (**guess**); then it **replants itself** from one seed, keeping its conditions. XP: 4 a ripen hour (**guess**: a tuber 12, wheat 24). Wild crops (no record) drop the same ripe yield and do not replant.
18. **Fertiliser**, when it arrives (it needs resins, a later milestone), halves the time left and lasts until harvest. The Farming Epic doubles its effect. Built into the record now; no recipe yet.
19. **Wild crops** grow on new and existing planets' unsaved chunks (flora appended, as M11's were): tuber, dewberry and oats on turf everywhere; wheat on turf on tier 2 and 3; rice by water; waxy tuber on tier 2 and 3; exotic tuber on tier 3. Densities **guesses**.
20. **Looking at a crop** shows its name, stage and time to ripe (asked of the server: `ClientMsg::LookCrop`, answered `ServerMsg::CropInfo`).
21. **Foods unlocked by grain** (Boundless's recipes, tuning 3.9): bags of oats, rice and flour (extractor, 2 grain, Cooking 2, 3, 4); baking powder (extractor, 2 rock salt, 1 sackcloth, Cooking 4); butter (mixer, a bucket of milk, 100 spark, Cooking 5); the rank 3 dishes (porridge or risotto: mixed raw at the mixer, cooked at the furnace) and rank 4 loaves for every line in part 1, and teaching porridge and loaf (Studious). Raw unbaked dishes are food too (200 or 300, sick). Pies need power (power coils, with gems) and wait; so do dishes needing purified milk (charcoal is not in the game yet).
22. **Formats**: `SAVE_VERSION` 10 (the `crops` table; version 9 opens with no crops), protocol 23.

### Acceptance (part 2)

- A tuber sown on tilled soil ripens in 2 h 15 min of universe time, whether or not anyone was near; two sown together ripen together.
- Harvesting it gives a seed, its yield and XP, and it is growing again at stage 1.
- Tilled soil stays tilled; a crop with no record is wild and does not replant.
- A crop's stage survives a restart and catches up; breaking its block drops the record.
- A bag of oats, a porridge and a loaf can be made from farmed grain.
- `--demo-ui farm` shows a field at every stage and the crop info; screenshots.
- Upgrade check from save version 9.

## Non-goals

- Brews, bombs that heal, healing items (with brews); elemental shielding and atmosphere foods (with elements and atmospheres).
- Glossy and juicy berries from foliage (no tier 4+ world yet); Hearty and Springy dishes come with farming or higher tiers.
- Food decay; drinking water; thirst (Boundless had none).
- Hanging crops (vines under foliage), inorganic and decorative crops, light as a crop condition, neighbour-block conditions, fertiliser's recipe (it needs resins).

## Measure in game (Nathan)

1. From a fresh meal (hunger shading gone), run on flat ground for one minute: how much of a 600 bar is shaded? Then sprint for a minute; then jump 20 times.
2. Mine 100 blocks with an iron hammer: how much is shaded?
3. Hungry to Full: does health come back only when the shading is almost gone? How fast?
4. At full hunger, how much health does Starving take, and how often?
5. After a defeat, how much is shaded?
6. A berry dish's Recovery buff: how long does it last?
7. Is a water block used up by filling a bucket?
8. Farming: Boundless's crop HUD showed an "Expected time to maturity". For each crop (starberry vine, tuber plant, waxy and exotic tuber plants, oats, wheat, rice), sown on tilled soil with good light and no fertiliser, what does it say? Rice in and out of water?
9. Which crops needed which Crop Sowing level (the seed's tooltip, "Requires crop sowing")?

## Status

*Part 2 (farming) built 2026-10-22; awaiting Nathan's play-test.* ADR-0010; protocol 23, `SAVE_VERSION` 10 (the crops table; version 9 opens with none).

- [x] Core (`bm-core/src/crops.rs`): crop kinds from `assets/crops.toml`, growth as a pure function of time with tilled, water and fertiliser, harvest rolls from Boundless's drop data, the Farming Epic
- [x] Data: 7 crops with 4 stage blocks each (the first is the seed), tilled soil, wood and metal tillers, grains, bags of oats, rice and flour, baking powder, rank 3 and 4 dishes of every line, Wise porridge and loaf (Studious), raw dishes; Farming and the Farming Epic are real skills; wild crops appended to every kind (existing planets gain them in unsaved chunks)
- [x] Server (`farm.rs`, `game/farm.rs`): a record per crop, saved; sowing checks (ground, Farming level), turf under a crop becomes soil; stage changes in loaded chunks once a second; chunks catch up as they load; ripe harvests replant themselves and give XP; tilling; `LookCrop`/`CropInfo`
- [x] Client and art: the crop line under the crosshair ("Oats: ripe in 1 h 20 min, tilled"), tillers predict no damage on soil, 28 crop stage textures, tilled soil, 52 icons, `--demo-ui farm`; screenshot `docs/media/m16-farm.png`
- [x] Verified: 598 tests; upgrade check from version 9 ("upgraded the universe from save version 9 to 10"); load test (50 bots, no slow ticks, worst 1.4 ms)
- [x] Tests: crop maths (clock, tilled, rice, fertiliser, harvest rolls), farm records, the round trip in the save, worldgen pins (`appending_m16_only_grows_crops_on_their_ground`), and end to end: till, sow, catch up across a restart with nobody near, harvest and replant, the Farming gate, the record gone with its block

Open: fertiliser has its rules but no recipe (resins come later); berry vines stand on soil rather than hanging under foliage; the crop times and Farming levels are guesses (Measure 8 and 9).

### Part 1


*Part 1 built 2026-10-22; awaiting Nathan's play-test.* Protocol 22, `SAVE_VERSION` 9 (version 8 universes open with everyone unhungry, no buff, and stations' jobs holding fresh buckets).

- [x] Core (`bm-core/src/food.rs`): hunger in 1/600 points, the usable bar, drains per use (`Drain`), running, sprinting, jumping, Sated, Full healing (replacing M13's stand-in), sickness, starving, defeat hunger, nine buffs and their effects (Lively, Hearty, Refreshed, Keen, Gentle, Guarded, Springy, Studious, Sated)
- [x] Data: food on raw meats, tubers and edible lamella; dewberries and cooked singles; rank 1 and 2 dishes of six lines; nails, buckets (10 fills), buckets of water and milk, syrup, sugar; Cooking (and its two epics) gates the dishes; dewberries from foliage, tubers from soil and turf
- [x] Server: `ClientMsg::Eat` at its input, `FillBucket`; full buckets ride with a station's job and come back one fill more worn (`Job::containers`, saved); buffs on XP, crits (mining and strikes), tool wear and armour; defeat hunger
- [x] Client: eating predicted at its input (no corrections, tested), hunger shading on the energy bar, the buff and Full, Sick or Starving as tags, food and bucket tooltips, 28 new icons, `--demo-ui food`
- [x] Verified: tests (counts in CLAUDE.md), the upgrade check from version 8 ("upgraded the universe from save version 8 to 9 (0 stations and 2 characters ...)"), load test (50 bots: worst tick 9.8 ms while they joined, the same as M15's server on this sandbox); screenshots `docs/media/m16-food-hud.png`, `m16-food-tooltips.png`, `m16-food-icons.png`, `m16-after-eating.png`

Not covered by an automated test: filling a bucket at water (refused away from water is tested; the success path is a few lines, and the test worlds have no reachable water). Try it in game.

Open (for Nathan):
- Eating raw meat with no hunger is refused like any plain food ("You are full"). Boundless's rule here is not found.
- The rates marked guess in `docs/feel.md` (running, sprinting, jumping, starving, defeat) wait for the Measure list above.
- Part 2 followed (above).
