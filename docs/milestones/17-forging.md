# Milestone 17: Forging

*Spec written 2026-10-22. Nathan picked forging next (2026-10-22). Boundless first: the session rules, every ingredient's costs and effects, the trait table and level thresholds come from the game's own data as the community published it (mayumi's forge simulator: `raw/forge.json`, `ingredientConfig.js`, `ingredientScripts/*.js`, `main.js`; fetched and read 2026-10-22). Recipes, craft XP, wear and skill gates come from boundlessinfo `compiled-recipes.json` (fetched 2026-10-22); each gear item's Flexibility and base rank from boundlessinfo `item-attributes.json`. Tuning 5.7 and 5.8 hold the tables. Numbers marked **guess** are on no site; the Measure list at the end is for Nathan.*

## Goal

A **forge** (four blocks, like a workbench) gives a piece of gear **traits**: **boons** (good), **flaws** (bad) and **quirks** (odd). A forging **session** is a short game of risk: you load a **deck** of ingredients, then use them one a round. Each tincture pours points into three pools; each round every pool is rolled, and a hit moves points from the pool onto a random trait of that kind. Glazes lean the dice toward a kind of boon, salves and spurs bend the rules for a few rounds, washes take a trait back off. Every ingredient costs **Stability** (run out and the gear is destroyed) and some cost **Vigour** (run out and you can only use the free ones). A **sealing wax** ends the session and the gear goes into the forge's queue; an **unmaking wax** abandons it and returns some of what you used.

How good you are at it is your character: Vitality gives Stability, Power gives Vigour, Insight (Intelligence) gives **Effectiveness**, which multiplies boon points. Softer materials (high **Flexibility**) take traits more readily than hard ones.

## How Boundless did it

(Tuning 5.7 has the session model step by step; this is the short version.)

- **The Centraforge** is 4 blocks that must touch; crafted at a workbench (Machine Crafting 2). Gear goes in, the deck (6 slots, 10 with the Forging Epic, up to 100 of each) is built, and the session starts. Stats are snapshotted at the start: Stability from Vitality, Vigour from Power, Effectiveness from Intelligence, the item's Flexibility and the forge's power from coils. [R195; wiki Centraforge]
- **A round**: one ingredient. Its Stability cost is multiplied by a random 1.00 to 1.10. A tincture adds points to the boon, quirk and flaw pools (each capped at 1,000; boon points first multiplied by Effectiveness and by any boost), then each pool is rolled: draw r in 0..1; if r ≤ pool / 1000 it hits and floor(r × 1000) points leave the pool for one random eligible trait of that kind. [simulator `main.js`]
- **Eligible traits**: those the gear can carry, of that kind, not excluded by a trait already on the item (a boon and its flaw exclude each other; some specials exclude each other), not already at their top level; a new trait only while the item has fewer than 3 of that kind. Every eligible trait weighs 1; each glaze of a tag adds 10 to the boons of that tag. [simulator]
- **Levels**: a trait's points are cumulative; scalar traits have 10 levels at 100, 150, 300, 600, 1,000, 1,500, 2,000, 3,000, 4,000, 5,000; specials 5 levels (100, 300, 1,000, 2,000, 4,000) or 3 (300, 1,000, 2,000) or 1 (1,000); quirks one level at 600. [`forge.json`]
- **End**: setting resin queues the item in the forge for a time set by its value (2 minutes up to an hour, with spark and machine wear); deconstruction resin returns each ingredient type used with a 25, 50 or 75% chance and the item unforged (from R209). Stability at 0 destroys the item and what was used. Gear is forged once. [`forge.json` levels; R209]
- **Power coils** on the Centraforge raise Effectiveness: each coil's text says excess power over what the item needs raises "forge momentum"; up to 24 coils; Effectiveness is capped at 200%. A diamond hammer with a full forging build rose 6% a coil and reached 200% at 18 advanced coils (t/30053, 2019-05). Every coil needs a compact gem. [wiki; t/30053]

## Decided in this spec

1. **Power coils come later, with gems** (CLAUDE.md asked this spec to decide). Every coil needs a compact gem, Boundless's gems come from tier 5 and 6 worlds, and Boundmore's worlds go up to tier 3. Building coils now would mean building higher-tier worlds first. So: the forge works now with **no power**; Effectiveness comes from Insight and the gear's Flexibility, which is how a Boundless player without coils forged. The power term is in the formula and reads 0; coils, gems and tier 5 and 6 worlds become their own milestone. The ingredients whose recipes need power (tier 2 and 3 tinctures, most salves, spurs and washes, unmaking wax 2 and 3) are built with their full rules but have no recipe until then.
2. **Boundmore's own names.** Boundless's ingredient and trait names are its own; Boundmore uses plain words (below). The mechanics and numbers are Boundless's.
3. **Smart stacks** (forging a stack at once) do not apply: every piece of gear in Boundmore stacks to one.

## Names (Boundless's in brackets, for identification only)

- The **forge** [Centraforge]. **Boons**, **flaws** [defects], **quirks**.
- **Leanings** (a boon's tag): Thrift [Ease], Swift [Lightness], Odd [Special], Force [Effect], Lasting [Longevity], Glow, Elder [Venerable].
- **Tinctures** [compounds]: pure, rough [imperfect], wild [unstable], sapping [draining], tainted [corrupted]; mending [defect reduction], wild mending, calming [quirk reduction], rough calming. Ranks 1 to 3 (or 1 to 2).
- **Glazes** [gums]: thrift, swift, odd, force, lasting, glow, elder; keeping [trait reduction] (leans to traits already on), seeking [trait expansion] (leans to new ones).
- **Salves** [pastes]: shielding [protection], boosting [boon boost], flaw guard, quirk guard, vigour [envigoration], steadying [stabilisation], fate, turnabout [fate reversal], gamble.
- **Spurs** [catalysts]: flaw turning [defect reversal], vigour, boosting [boon boost], points [boon points].
- **Washes** [solvents]: boon, flaw and quirk washes [removal], and shifting washes [transmute] of each kind.
- **Waxes** [resins]: sealing wax [setting resin], unmaking wax [deconstruction resin] 1 to 3.
- **Bases**: tincture, glaze, salve, spur, wash and wax bases. **Fossil essence** [ancient vital essence] from **fossils**; **charcoal**.

## Rules

### The machine

1. **The forge**: 4 `forge` blocks touching by faces (`Assembly::Four`, `Station::Forge`), as the workbench. Recipe (Boundless's): workbench, 10 stones, 3 silver alloy, 6 relic fragments, 1 relic component; 400 spark, 15 minutes, Machine Crafting 2, wear 55, 46 XP; makes 4 blocks. It wears like the other spark machines (M15), and is repaired with a spanner.
2. **One session at a time** per forge, owned by the player who started it; only they can continue, seal or unmake it. It lives in the forge's station record and is **saved** (a restart, a disconnect or the player walking off keeps it exactly). The forge's blocks cannot be broken while it holds a session, a queued item or ingredients (as with any station with contents).

### Starting

3. **Gear that can be forged**: hammers, shovels and axes, chisels, grapples, slings, bags of bombs, spanners. Not the hand stone, not tillers (Boundless's hoes had no Flexibility). Each forges **once**: a forged item (any trait, or forged with none) is refused.
4. **Flexibility and rank** per item (Boundless's data): wood 150 (rank 10), stone 120 (20), copper and iron 110 (30), silver alloy 100 (50); chisels 100 (50, precise 100). Held in the catalogue (`flex`, `rank`).
5. **The deck**: up to **6** ingredient stacks (**10** with Forgemaster), up to 100 of each, chosen from the inventory and moved into the forge at the start. What is left in the deck when the session ends comes back.
6. **Session stats**, fixed at the start from the forger's skills (attribute ranks count Wholeness's fifth of a rank, as everywhere else):
   - **Stability** = 900 + 100 per Vitality rank, at most 1,800 (wiki Vitality Bonus table).
   - **Vigour** = 200 per Power rank, at most 1,800 (game skill text; wiki).
   - **Intelligence** = 10 + 10 per Insight rank (wiki Intelligence Bonus table).
   - **Flexibility** = the item's, +25 with Forgemaster (game skill text), at most 150.
   - **Effectiveness** (**guess**, see below), at most 200%.

### Effectiveness (a guess built from the game's constants)

No site publishes the formula. The game's forge data has its constants (momentum base 0.2 and high 1, intelligence cap 30 to 135 with curve 1, power cap 1,000 to 14,500, flexibility proportion 0.5 to 5 with curve 2, flexibility max 150), and players measured one line (a diamond hammer, full forging build: 6% a coil, 200% at 18 advanced coils of 300 power). Boundmore's formula uses those constants in their plain reading and is scaled to that line. With f = Flexibility / 150:

- Insight share I = 0.2 + 0.8 × min(1, Intelligence / (30 + 105 × (1 − f))).
- Flexibility share F = 0.34 × (0.5 + 4.5 × f²) (the 0.34 puts the diamond line at 92% with no coils, its measured slope extended to 0).
- Power share P = power / (1,000 + 13,500 × (1 − f)) (6% a 300-power coil for diamond with Forgemaster; power is 0 until coils).
- **Effectiveness = min(2, F × I + P)**.

What it gives with no coils, for a new character and for a full forger (8 Insight and 5 Wholeness): wood 79% and 170%; stone 41% and 115% (160% with Forgemaster); copper or iron 34% and 99% (141%); silver alloy and chisels 27% and 85% (123%). With coils it reproduces the diamond line (193% at 17 coils, 199% at 18; measured 194 and 200), but it gives special materials 87% at 24 coils where players saw 35 to 42%: the power share needs refitting when coils arrive. It is on Nathan's Measure list.

### A round

7. Using an ingredient from the deck is one round, in Boundless's order (the simulator's `useItem`, `endRound`): its effect (points to add, buffs, a wash's change); then any **corruption** adds its flaw points; the points are added to the pools and the pools clamped to 0..1,000; each pool marked to roll is rolled (a hit moves floor(r × 1000) points to one weighted random eligible trait); round-end buffs act; every timed buff loses a round; and the ingredient's cost is paid: Vigour as listed, Stability × a random 1.00 to 1.10, rounded.
8. **Refused**: an ingredient whose Vigour cost is more than the Vigour left.
9. **Destroyed**: when Stability is 0 or below at the end of a round, the gear and every ingredient used are lost; the unused deck comes back. The player is told in plain words before each round how much Stability the ingredient will cost.
10. **Ingredients** do exactly what Boundless's do (tuning 5.7's table; costs are Vigour / Stability; points boon / quirk / flaw, boon points before Effectiveness):
    - Pure tincture 1/2/3: 0/50/100 vigour, 100 stability; boon 100/200/400.
    - Rough tincture 1/2/3: boon 200/400/1,000, flaw 80/160/400.
    - Wild tincture 1/2/3: boon 200/400/1,000, quirk 100/200/500, flaw 50/100/200.
    - Sapping tincture 1/2: 50/100 vigour; boon 400/1,000, quirk 160/400; then 3 rounds of −20/−50 vigour.
    - Tainted tincture 1/2: 50/100 vigour; boon 400/1,000; then 3 rounds of +80/+180 flaw points.
    - Mending tincture 1/2: boon and flaw −150/−400. Wild mending 1/2: boon −100/−400, quirk +50/+150, flaw −150/−250. Calming 1/2: boon and quirk −150/−400. Rough calming 1/2: boon −100/−250, quirk −150/−400, flaw +50/+150.
    - Glazes (50 vigour, no stability): a lasting buff: +10 weight to every boon of its leaning per glaze used; keeping glaze to traits already on the item, seeking glaze to traits not on it.
    - Salves (50/100 vigour, 100 stability): shielding (stability cost ×0.5) 2/4 rounds; boosting (boon points +50%) 2/4; flaw guard and quirk guard (those points −50%) 2/4; vigour (+15/+50 vigour a round) 5; steadying (−35/−50 stability cost a round) 5; fate (up to 2 rerolls of a boon miss or a quirk or flaw hit) 3/5; turnabout (200 vigour: flaw points added become fate rounds, one per 100) 5; gamble (40/60% chance an ingredient costs no stability) 3.
    - Spurs (no stability): vigour 1/2 (100/200 vigour) ends every buff for +30/+50 vigour per round it had left; points 1/2 (100/200) turns fate rounds into +100/+200 boon points each; boosting (100) turns fate rounds into boost rounds; flaw turning (200) moves half the flaw pool to the boon pool.
    - Washes (no stability): boon wash 1/2 (100/200 vigour) takes the newest boon's points back into the pool (50%/100% of them, as far as the pool has room) and rolls the boon pool; flaw and quirk washes 1/2 the same for flaws and quirks (100%/50%); shifting washes (100 vigour) move the newest trait's points onto a different trait of its kind.
    - Sealing wax: ends the session, the gear goes into the queue. Unmaking wax 1/2/3 (0/50/200 vigour): ends it, gives the gear back unforged, and each kind of ingredient used comes back with a 25/50/75% chance (all of that kind, or none).
11. **The dice** are the server's (a seeded generator saved with the session, so a restart resumes the same session; clients never roll).

### Traits

12. **Boons and their flaws** (Boundless's values, tuning 5.7; only on the gear listed). Per level unless said:

| Boon / flaw | Leaning | Gear | Effect |
|---|---|---|---|
| Thrifty / Tiring | Thrift | all | energy a use −2 / +2 (never below 0) |
| Quick / Sluggish | Swift | tools, slings, bombs, spanners, chisels | action speed +5 / −5 |
| Lucky / Unlucky | Thrift | tools, slings | crit chance +5% / −5% |
| Fierce / Mild | Force | tools, slings | crit effect +10% / −10% |
| Mighty / Feeble | Force | tools, slings, bombs | damage +50 and +10% of base every two levels / the reverse |
| Sturdy / Brittle | Lasting | tools, slings, grapples, spanners, chisels | durability +200 / −200 |
| Pushing / Soft | Force | bombs | knockback +2 / −2 |
| Wide Blast / Small Blast | Lasting | bombs | radius +0.3 / −0.3 |
| Fast Shot / Slow Shot | Swift | slings, grapples | shot speed +5 / −5 |
| Quick Reel / Slow Reel | Force | grapples | reel in and out +5 / −5 |
| Long Rope / Short Rope | Lasting | grapples | range +3 / −3 |
| Far Shot / Short Shot | Lasting | slings | range +3 / −3 |
| Mender / Poor Mender | Force | spanners | wear mended a hit +100 and +10% every two levels / the reverse |
| Wide Swing (5 levels) | Odd | tools | 1: a row of three at 40%; 2: the block below too at 60%; 3: a cross of five at 40%; 4: all nine at 40%; 5: the hit block and 1 to 4 random neighbours at full damage (M9's `Aoe`) |
| Glowing (3 levels) | Glow | all | held light 8 / 11 / 14 (Boundless: radius 6 / 9 / 12; the light levels are a **guess**) |
| Hookshot (1 level) | Odd | grapples | reels itself in, reel in +20 |
| Heavy Shot (5 levels) | Force | slings | damage +10% a level, shot speed −10, falloff +0.1 a level |
| Light Shot (5 levels) | Swift | slings | shot speed +10/12/15/20/30, falloff −10% a level |
| Many Shots (5 levels) | Odd | slings | 2 to 6 shots at a time, each at 50% damage (2) or 40% (3 to 6), spread 0.15 rad |

Heavy and Light Shot exclude each other. Stats never go below sensible floors (damage 1, action speed 1, durability 1, speeds 1).

13. **Quirks** (one level; values from Release 209): **Night Owl** (+100 and +10% damage at night, −100 and −10% by day), **Early Bird** (the reverse), **Bold** (+10 action speed at 55% health or more), **Desperate** (+10 action speed at 45% health or less), **Sated** (energy a use −10 while hunger is under 45% of the bar), **Ravenous** (energy a use −10 while hunger is over 55%), **Loud** (creatures notice you from half as far again, **guess** on how far). Night Owl and Early Bird exclude each other, as do Bold and Desperate, Sated and Ravenous. They apply while the item is held.
14. **Rank** (shown on the item; sets queue time): the item's base rank + 10 a level for scalar boons, +10, 30, 50, 70, 90 for five-level specials, +30, 50, 70 for three-level, +20 for hookshot; flaws count −0.5 and quirks −0.3 of a boon's (Boundless's multipliers), a quirk counting as 40 (**guess**).
15. **The queue** after sealing: by rank, Boundless's table: up to 200: 2 min, 90 spark, wear 4, 5 XP; 500: 4 min, 180, 8, 10; 1,000: 10 min, 360, 12, 20; 2,000: 20 min, 720, 24, 50; 5,000: 40 min, 1,440, 40, 75; above: 60 min, 2,880, 60, 100 (`forge.json` levels; reading its "value" as rank is a **guess**). Spark comes from the forge's grid as for any machine. The XP goes to whoever collects.
16. **Where traits act**: every place a stat is read from an item reads it through its traits, on the server and the client alike (prediction of swing cadence, energy and grapples stays exact). Quirks that depend on time, health or hunger use what the client can see; a mismatch at a threshold is corrected like any other.

### Ingredients and their sources

17. **Recipes** (Boundless's, extractor unless said; craft XP, wear and Forge Stocks level from the data). Boundless's materials map to Boundmore's: reactive lamella, syrup, fleshy and inky leaves, beans, olivine, sulphur, saltpetre, cobalt, opal, flint, sap, petals, orbs, glands, bloods, eyes, mantles, hides and trophies are already in the game (grazer for wildstock, pelter for spitter, dasher for roadrunner, popper for hopper, drifter for cuttletrunk; thick blood for viscous, quick gland for adrenal, raw tuber for earthyam).
    - **Charcoal**: furnace, 1 trunk, 120 s, 100 heat, 4 XP.
    - **Fossils**: small fossil seams in rock on every world, 3 to 40 blocks down (Boundless); medium fossils deeper (**guess**: 5 to 50, rarer); large wait for power. Breaking one gives a fossil. **Fossil essence**: 1 from a small fossil (60 s), 4 from a medium (240 s, 100 spark). Existing planets gain fossils in chunks nobody has saved, as wayshards, relic seams and crops were.
    - **Bases**: tincture (1 glass, 1 reactive lamella, 1 charcoal), glaze (1 glass, 1 syrup, 1 fleshy leaf), wax (1 glass, 1 sap, 1 fossil essence): Forge Stocks 1, 30 s, 4 XP. Salve (1 glass, 2 spicy beans, 1 bitter bean, 200 spark, Forge Stocks 1), spur (1 glass, 2 inky leaves, 1 cobalt, 200 spark, Forge Stocks 2), wash (1 glass, 1 thick blood, 1 olivine, 200 spark, Forge Stocks 2): 30 s, 8 XP.
    - Made without power now: pure, rough and wild tincture 1 (1 tincture base, 2 vital essence; 4 of them); every glaze (1 glaze base and 1 fresh blood (thrift), float sac (swift), thick blood (odd), heat sac (force), mantle (lasting), glowing lamella (glow), 20 raw tubers (elder); keeping: 2 bases and 1 olivine; seeking: 2 bases and 1 sulphur; 4 of each); sealing wax (1 wax base, 1 flint; 4); unmaking wax 1 (1 wax base, 1 volatile blood; 4); boon wash 1 (2 wash bases, 1 horn trophy, 1 shimmering orb, 200 spark; 4); flaw and quirk shifting washes (the same, 90 s).
    - The rest need power and wait for coils (their rules are built and tested).
18. **Skills become real**: **Insight** (Intelligence: +10 Intelligence a rank for forging; its healing part waits for healing), **Forge Stocks** (ingredient crafting 1 to 3, gating the recipes above), **Forgemaster** (forging epic: +25 Flexibility, 4 more deck slots).

### Formats

19. A forged stack carries its traits (`ItemStack::forge`: up to 9 traits, each a trait and a level, and whether it was forged); stacks only merge when their traits match. Saved after a `!` in the stack's saved name (`iron_hammer!mighty5,quick3`; `iron_hammer!` for forged with none), so every older save reads unchanged.
20. `SAVE_VERSION` 11: a forge's station record saves its session (gear, deck, used ingredients, stats, pools, traits, buffs, the dice) and its queued item; version 10 universes open unchanged (no forges existed). Protocol 24.

## Acceptance

- Four forge blocks make a forge; a fifth is refused; three do nothing and say so.
- A full session: put an iron hammer and a deck in, use pure tinctures until a boon lands, seal it; after the queue the hammer comes out with the boon, and swings faster (Quick) or hits harder (Mighty) on the server and in the client's prediction (no corrections).
- The session survives a restart and a disconnect, mid-way, with the same pools, traits, buffs and dice.
- Stability running out destroys the gear and the used ingredients and returns the rest; a vigour-costing ingredient with too little vigour is refused.
- Unmaking wax returns the gear unforged and some ingredients; a forged item is refused by the forge.
- Every ingredient's effect matches the simulator in unit tests (pools, buffs, rerolls, washes, spurs).
- A Wide Swing 4 hammer breaks nine blocks a swing; a Long Rope grapple reaches 3 m further a level; a Glowing tool lights the hand.
- Fossils appear in new chunks of existing planets; the recipe chain from fossil to sealing wax works.
- The forge screen shows the gear, the deck, Effectiveness, Stability, Vigour, the three pools with the last roll, traits with their levels, buffs with rounds left; item tooltips list traits. `--demo-ui forge`; screenshots.
- Upgrade check from save version 10; load test.

## Part 2: the body quirks

*Built 2026-10-23 (Nathan: "Continue on part 2!"). Values from Release 209's notes (forum t/22336, t/22594) and the trait list (tuning 5.8); how often and how much, where no source says, are **guesses** on the Measure list.*

21. Eight more quirks, one level at 600 points, on every kind of gear, acting while the item is in hand (appended to the trait ids):
    - **Slippery** [slippery feet]: the ground grips at 15% (**guess**); +40 falling armour (Release 209). Excludes Bouncing.
    - **Bouncing** [bouncy feet]: a landing faster than 6 blocks a second springs back up at half the speed (**guesses**). The landing hurts as any landing does, but the bounces do not: in Boundless the quirk dealt fall damage on every bounce, a bug players called a death quirk (t/23147), so Boundmore leaves the bug out.
    - **Leaden** [that sinking feeling]: you sink in water, three times the pull, and swimming up does nothing (**guess**); pushing against a wall still climbs out. Its +50% breath waits for breath (there is no drowning yet).
    - **Sprinter** [slow-mo]: walking and running 25% slower, the sprint's bonus 50% bigger (Release 209).
    - **Jumpy** [one jump ahead]: jumps by itself on the ground about every 4 s (**guess**; players saw holders "jumping up and down while running"), and gives a double jump without the epic (Release 209).
    - **Twitchy** [wonky trigger]: goes off by itself about every 10 s (**guess**; players found it too frequent); +400 durability (Release 209). A grapple fires inside the step; a tool swings, a sling shoots and a bag throws as if clicked.
    - **Gnawing** [nutritionally challenged]: hunger as fast as sprinting, all the time (**guess**; players: it "drains energy" fast); +30% max energy (Release 209).
    - **Draining** [bleeding edge]: 5 health every 5 s (**guess**: a little under what being Full heals, as players found "all you need is Full Up"); +30% max health. It never interrupts the calm that lets being Full heal, and it can defeat you (players were). Boundmore counts health lost, not health left, so the +30% shows at once on picking the item up; Boundless's players complained that it did not (t/28136).
22. **Odd noises** waits for sound.
23. **Deterministic on both sides**: the held item's quirks ride in `PlayerState::quirks`, set by server and client from the slot each input names before its step; the random ones roll from `PlayerState::clock`, counted by the step itself, so client and server roll the same dice and prediction needs no messages. Protocol 25 (the state on the wire gained both fields; saves keep the body layout of `SavedBody`, so no save change).

### Acceptance (part 2)

- Each quirk's rule in a unit test (speeds, grip and falling armour, bounces that settle and do not hurt twice, Jumpy's rate and double jump, identical on two copies, Twitchy's rate, the drains and the +30% maxima, sinking).
- A Jumpy and Sprinter item held for part of a run predicts with no corrections across reconciliations.
- End to end: a Gnawing hammer in hand makes the server's player hungry standing still and raises the bar to 780; holding the stone, nothing.

## Non-goals (later)

- Power coils, gems, tier 5 and 6 worlds, and the ingredient recipes that need power (the milestone after this, or with gems).
- Traits whose systems do not exist yet: the loot magnet (no dropped items: it would loot nearby bodies), bleed, slow and weaken (creature debuffs), healing, regenerating and buff bombs, changing, unshaping and lattice chisels, door spanners, liquid breakers, totems and fists. Until they exist they are not in the dice, so the dice choose among fewer traits than Boundless's.
- Odd noises (no sound yet) and Leaden's breath (no drowning yet).
- Forge academy, recipe planner, trading forged gear (shops come with Society).

## Measure in game (Nathan)

Part 2's guesses (any quirk you have on an item):

7. Slippery Feet: how far do you slide after letting go of forward at a run, compared with without it?
8. Bouncy Feet: from what height does a landing bounce, and how high (about half the fall)?
9. One Jump Ahead: about how many jumps a minute standing still? Wonky Trigger: how many misfires a minute?
10. Nutritionally Challenged: standing still from fed, how long until hunger shading first shows? Bleeding Edge: how much health a tick and how often, when not Full?
11. That Sinking Feeling: in water, do you sink like a stone, or slowly? Can you swim up at all?

Part 1:

1. **Effectiveness**: on your forging character, note your Intelligence ranks (and Attributes Bonus), whether you have the Forging Epic, and how many coils are on the Centraforge. Put in a wood, stone, copper or iron, and silver tool (any you have), a chisel and a grapple, and read the Effectiveness in the forge screen's circle. If you can take the coils off (or use a forge without them), the same with none. This calibrates the guessed formula.
2. **Stability and Vigour** at the start of a session, with your Vitality and Power ranks.
3. **Queue time** after Setting Resin for a low-rank item (a wood or stone tool with one boon) and a high one: how long the timer says.
4. When Stability falls to 0 or below, is the item destroyed at once, or can you still use Setting Resin?
5. Can Venerable boons (liquid breaker, lattice) appear without a Venerable Gum?
6. A Glow 3 tool next to a torch at night: which lights further?

## Status

*Part 1 built 2026-10-23; awaiting Nathan's play-test.* Protocol 24, `SAVE_VERSION` 11 (version 10 universes open unchanged: no forges existed).

- [x] Core (`bm-core/src/forge.rs`): 39 traits with Boundless's levels, exclusions and values; stats through traits (`ItemDef::tool_of` and friends, `Held::of` for quirk conditions on both sides); the session engine following the simulator round by round (pools, rolls, fate, every buff, every ingredient: 66); Effectiveness (the guess), Stability, Vigour, Intelligence; rank and the queue table; the dice saved with the session. 23 unit tests, one per rule
- [x] Data: forge blocks (workbench, Machine Crafting 2), 66 ingredients, 6 bases, charcoal, small and medium fossils and fossil essence, recipes for everything that needs no power (Forge Stocks levels, XP and wear from Boundless's data), Flexibility and rank on all 28 gear items; Insight, Forge Stocks and Forgemaster are real skills; fossil seams on every kind, appended to existing planets (re-pinned, `appending_m17_only_turns_rock_into_fossils`)
- [x] Server (`forging.rs`, `game/forge.rs`): the forge station (wears, draws spark for its queue), `ForgeBegin`/`ForgeUse`, sessions owned by their forger and saved, the queue and collect with XP, forged stats wherever the server reads gear (swings, Wide Swing areas, slings with Many Shots, bombs, grapples, spanners, chisels, Loud)
- [x] Client: the forge window (set-up with predicted Effectiveness, the session in three columns, the queue), forged tooltips (traits by kind, rank, forged stats), ingredient tooltips, prediction through traits, `--demo-ui forge` and `forge-queue` running a real session; screenshots `docs/media/m17-forge.png`, `m17-forged-tooltip.png`
- [x] Art: forge top and side, two fossil seams, icons for all 76 new items by family; `docs/media/m17-forge-icons.png`
- [x] Verified: 640 tests; upgrade check from `main` ("upgraded the universe from save version 10 to 11 (0 stations rewritten with no forge sessions; characters as they were)", and each planet: "small_fossil_seam, medium_fossil_seam added to its ores"); load test, 50 bots: 14 slow ticks while they joined, worst 5.2 ms in steady play, against 41 and 58.8 ms for `main`'s server on the same sandbox the same hour (the sandbox, not the change)
- [x] End to end: a hammer forged Quick swings faster after the queue; a session survives a restart and only its forger can play it; Stability running out destroys the gear and returns the rest; unmaking returns the gear unforged; a forged item is refused; a forge with a session cannot be broken

### Part 2

*Built 2026-10-23; awaiting Nathan's play-test.* Protocol 25.

- [x] Core: eight body quirks (`forge::Quirks`, `PlayerState::quirks` and `clock`), their rules in `movement::step`, the drains and maxima in `forge`, `food`; 7 unit tests
- [x] Server and client set the held item's quirks before each step; replays use each input's own; Twitchy uses a held tool, sling or bag as a click would
- [x] Tests: prediction with no corrections (`quirks_in_hand_predict_with_no_correction`), end to end (`a_gnawing_hammer_in_hand_makes_you_hungry`)

Open: the Effectiveness formula and every guess (Measure list); Night Owl and Early Bird may correct once at dusk (client and server judge night by their own tick); Twitchy on a tool, sling or bag is the client's click, so a modified client could skip its own drawback (the server cannot tell a misfire from a click); power coils, gems and the powered recipes; odd noises and breath.
