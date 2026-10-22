# Milestone 16: Food

*Spec written 2026-10-22. Nathan picked food next (2026-10-22: "Food", over forging and gems). Boundless first: recipes, times, heat, XP and skill gates come from the game's own data as the community published it (boundlessinfo `compiled-recipes.json`, fetched 2026-10-22); food values and buffs from the developers' patch notes (Release 207, 2018-11; Release 223, 2019-05) and the players' food index (forum t/17271, 2018-09, medium); crop drops from pfiffel's `dataBlockDrops.js`. Tuning 3.9 holds the full tables. Numbers marked **guess** are on no site; the Measure list at the end is for Nathan.*

## Goal

Using tools, running and jumping make you **hungry**, and hunger eats into your energy bar. **Food** takes the hunger away. Cooked food beats raw, raw meat makes you **sick**, and dishes cooked at the furnace carry a **food buff** for 10 to 60 minutes: never hungry, more health, more energy, faster energy refill, more crits, less tool wear, or armour. Being **full** is what heals you out of combat, replacing M13's stand-in. Going hungry long enough **starves** you.

Part 1 is hunger, eating, buffs and every dish whose ingredients exist on Boundmore's worlds today. Part 2 is **farming**, which brings oats, rice and wheat (porridge, risotto, loaf, pie: ranks 3 to 5) and the berries of harder worlds.

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

## Part 2: farming (outline; detailed before it is built)

Wild crops on every world (berry vines under foliage; tuber, oat, rice and wheat plants), seeds from harvesting them, sowing on soil (tilled soil with a tiller grows more), growth ticks once a game day through up to 8 stages, fertiliser, yield chances from the drop data (seed 1 + 0.35, crop 0.2 + 0.1 for grains), the Farming skill. Then bags of oats, rice, flour and baking powder, butter at the mixer, and the rank 3 to 5 dishes, teaching foods, hearty and springy foods. Crop growth is saved world state: it needs a format decision (an ADR) before building, and a question for Nathan on the farming complaints in `research/boundless-forum-feedback.md` (random growth, tilled soil untilling itself, hand replanting, low XP).

## Non-goals

- Brews, bombs that heal, healing items (with brews); elemental shielding and atmosphere foods (with elements and atmospheres).
- Glossy and juicy berries from foliage (no tier 4+ world yet); Hearty and Springy dishes come with farming or higher tiers.
- Food decay; drinking water; thirst (Boundless had none).

## Measure in game (Nathan)

1. From a fresh meal (hunger shading gone), run on flat ground for one minute: how much of a 600 bar is shaded? Then sprint for a minute; then jump 20 times.
2. Mine 100 blocks with an iron hammer: how much is shaded?
3. Hungry to Full: does health come back only when the shading is almost gone? How fast?
4. At full hunger, how much health does Starving take, and how often?
5. After a defeat, how much is shaded?
6. A berry dish's Recovery buff: how long does it last?
7. Is a water block used up by filling a bucket?

## Status

*Spec written 2026-10-22. Part 1 not started.*
