# Milestone 23: Water and Brews

*Spec written 2026-10-27.* Nathan is play-testing M17 to M22; the economy systems still wait on his decisions. "Swimming improvements" has been open since M7, Boundless's breath and drowning were never built ("breath under water" is a non-goal of M18; Leaden's +50% breath waits for it, M17), and brews are the last of Boundless's consumables (the Brewing skill already says "comes with brews"; M16 left healing brews to them). None of these shape what players invest in, so they need no decision of his before they are built; every feel number is on the Measure list.

Research: `docs/research/brews-and-breath.md` (forum, wiki, the game's recipe data), the forum threads cited below.

## Goal

A player dives into a lake for the glowing lamellae on its floor: X takes them down, the air bar runs while their head is under, they come up for breath before it runs out (or drown slowly if they misjudge it), and they float at the surface without holding a key. Later they brew a Kraken-style air brew and stay down twice as long, and carry healing and speed brews into a meteor fight.

## What Boundless did (forum and wiki)

- **One air bar.** The bar that runs down in bad air also runs down under water; "it is possible for the player to drown when completely submerged" (wiki, Water). Once it is empty a **Drowning** debuff takes health, a share of max health like falls and lava (t/17849). It refills at the surface.
- **Controls.** Space swims up, **C swims down** (creep), and with neither the swimmer **sinks slowly** (t/14997, t/42447, t/29682). Swimming is slow and agility does not change it (tuning 1.6).
- **Breath modifiers.** That Sinking Feeling (Boundmore's Leaden): sink, and **+50% breath** under water; it also makes breathing back in much slower (t/44095). The Kraken Brew (Release 248) **halves the air used under water**; with a Sinking Feeling item too, **the bar never drains** (t/52526, t/54092: "working as intended"). So both halve the drain and the halves add up.
- **What players hated** (t/14997, t/16875, t/42447, t/26479): holding Space for minutes to stay at the surface ("my thumb hurts"; "neutral buoyancy would be a welcome change"), diving "at the speed of smell", and drowning while away from the keyboard for a moment ("not enough time to get a glass of water").

## Decided in this spec (part 1: water)

1. **Breath.** While the eyes are in a liquid the air bar runs down, one tick of air a tick, from the same `air_used` the atmosphere uses (one bar, as in Boundless). Out of liquid it refills at the atmosphere's refill rate, unless the planet's air is also short of protection (then it keeps running down). A full bar is the atmosphere's 30 s (**guess**, Measure 1).
2. **Drowning.** With the bar empty and the eyes still under, every regen beat (5 s) takes **10% of max health** (**guess**, Measure 2; Boundless's was a share of max health, players found it fast). A defeated swimmer keeps their air; creative mode never runs out.
3. **Leaden halves the air used under water** (Boundless's +50% breath read as halving the drain, so that with an air brew it is zero, as players found) and **halves the refill** (Boundless: much slower to breathe in). The halves skip every other tick of the step clock, which both sides count alike.
4. **Swimming down.** A new input, **descend** (X; `PlayerInput::descend`), swims down at swim speed. It also flies down in creative mode. Space swims up at swim speed. C stays the crafting key in Boundmore.
5. **Neutral buoyancy** (a departure from Boundless, its most complained-of swimming rule, cheap to change back): with neither key held a swimmer slows to a stop and stays at their depth; one whose head is out of the water floats there. Leaden still sinks and cannot swim up. Climbing out against a wall with Space is kept.
6. **What players see.** The air bar shows while air is below full, as it does for bad air; under water it is labelled **Breath**, and **Drowning** in warning colours once empty. The fog closes in only for bad air, not under water (water has its own fog).

## Part 2: brews

From Boundless (`docs/research/brews-and-breath.md`, the game's recipe data for every recipe): brews are made in the **mixer** in five grades from a **brew flask** of the same grade (workbench: clay, fossil essence and metal), a liquid (water or milk for grades 1 and 2, purified for 3 and 4, fortified for 5), an **oil** (mixer: a trace fragment, clay and tallow) and the brew's own ingredient. One **brew buff** is active at a time beside the food buff; drinking a second timed brew replaces the first. Instant brews never touch it, and the reviver has its own slot.

Decided:

7. **The brews** (Boundmore's names; grades shown as I to V; Boundless's numbers; healing is multiplied by (100 + 10 per Insight rank)%, Boundless's (90 + Intelligence) / 100):

   | Brew (Boundless's) | Grades | Effect | Lasts |
   |---|---|---|---|
   | Restoring (instant healing) | I to V | 750 / 1500 / 3000 / 6000 / 12000 health at once | instant |
   | Mending (healing) | I to V | 75 / 150 / 300 / 600 / 1200 health every 5 s (Release 199's "more than instant", an inference in the research) | 60 s |
   | Rousing (instant recovery) | I to V | 600 / 1200 / 2400 / 4800 / 9600 energy at once | instant |
   | Steady (recovery) | I to V | 30 / 60 / 120 / 360 / 900 energy a second (Release 223) | 60 s |
   | Swift (fast) | I to V | run and sprint +0.5 / 0.75 / 1 / 1.5 / 2 blocks a second; action speed x1.2 / 1.4 / 1.6 / 1.8 / 2 | 10 min |
   | Brawn (strength) | I to V | +10 / 20 / 30 / 40 / 50% damage, added to Power's share (so the damage epic triples it) | 10 min |
   | Hushed (innocuous) | II to IV (as in the data) | creatures notice you 30 / 40 / 55% nearer (II and IV are inferences; the item texts are broken) | 10 min |
   | Glowing (luminous) | II to IV | carried light of level 10 / 11 / 12 (Boundless 6 to 12 m; the levels a **guess**) | 10 min |
   | Clearing (cleansing) | I to V | 500 to 2500 points off sickness, a point a second | instant |
   | Soothing (calming), Taunting (aggravating) | V | 10000 threat taken from, or added to, every creature near you that knows you | instant |
   | Lifesaving (reviver) | one | if defeated while it lasts, back on your feet at once with a quarter of your health and half your energy (**guess**), no defeat penalty | 10 min, its own slot |
   | Deepwater (Kraken) | one | halves the air used under water; with Leaden none at all | 20 min |

8. **Recipes are Boundless's** (single craft: 4 brews, times, spark, power, XP, wear, Brewing level). Boundmore's ingredients stand in by role: thick blood for viscous blood, quick gland for adrenal gland, fossil essence for ancient vital essence. The Kraken brew was a Halloween recipe of spooky orbs; Deepwater takes a grade I flask, 3 rice grain, a float sac and a bucket of water instead (**guess**, no skill needed, as Kraken).
9. **Brewing levels**: Brewing (3 ranks), Provisioner and Master Brewer (+1 each), as Boundless's Brew Crafting 1 to 5. Insight's text gains "+10% healing".
10. **Brews are carried in `PlayerState`** (`brew`, `revive`) and stepped in `movement::step`; drinking is eating (`ClientMsg::Eat` at an agreed input), so prediction stays exact. They are saved in a new `player_brews` table (no save version change) and travel with the player.
11. **Not built**: Rage and Focus brews (their epics are not built), the weather brews (no weather), Zombie Lungs (not in the game's data), Second Chance (needs meteorite ichor), smoothies (their crops are not Boundmore's), Zeal lengthening buffs (food does not yet either), the brew-extending guild machine (no guilds).

Rules (part 2): `bm_core::brew` (kinds, numbers, `PlayerState::drink`), `FoodStats::brew`, `PlayerState::{brew, revive}`, `Effects::brewed`, `Body::heal_bonus`; 3 oils, 3 liquids, 5 flasks, 45 brews in `items.toml`; their recipes; the `player_brews` table; protocol 40.

Acceptance (part 2): every brew does what the table says (unit tests); a second timed brew replaces the first and an instant one leaves it (unit test); Lifesaving stands a defeated player up without the penalty (end to end); a brew survives leaving and coming back (end to end); Swift predicts with no correction (prediction test).

## Rules (part 1)

- `PlayerInput::descend` (protocol 39). `PlayerState::submerged` (eyes in liquid this tick; never saved).
- Breath and drowning in `atmosphere.rs` beside choking; `PlayerState::drowning()`. Everything inside `movement::step`, so prediction stays exact.
- No save change: `air_used` is already saved.

## Acceptance (part 1)

- A swimmer whose head stays under runs out of air in 30 s and then loses 10% of max health every 5 s; surfacing refills the bar (unit tests).
- Leaden halves the drain and the refill (unit test).
- X swims down at swim speed, Space up; with neither the swimmer keeps their depth and floats at the surface (unit tests). Creative flight descends on X.
- Prediction agrees with the server through a dive (the client's prediction test).

## Non-goals

- Boats, flowing water, liquid breaking tools, creatures that drown.
- Swim speed from skills (Boundless had none).

## Measure in game (for Nathan)

1. How long a breath lasts against Boundless's (dive in a lake on a tier 1 world and count).
2. How fast drowning hurts.
3. Neutral buoyancy: better than Boundless's sinking, or should it sink slowly as Boundless did?
4. Swim speed up, down and across (3.2 blocks a second).
5. Brews: do Swift, Brawn and Mending feel like Boundless's at each grade? Is Lifesaving's quarter health right? Is Deepwater's recipe sensible?

## Status

- [x] Part 1: water (2026-10-27). Breath and drowning on the air bar (`PlayerState::submerged`, `drowning()`, `atmosphere::DROWN_PERCENT`), Leaden's halved drain and refill (`breath_halvings`), X to swim down and to fly down in creative (`PlayerInput::descend`, protocol 39), neutral buoyancy, the Breath and Drowning tags on the bar, fog only for bad air. 5 core tests, 1 prediction test, the air bar test extended; 768 tests. No save change.
- Research in (`docs/research/brews-and-breath.md`): breath and swim timings are not online anywhere reachable, so they stay on the Measure list; players say Agility raised swim speed in Boundless (open).
- [x] Part 2 specced (2026-10-27)
- [x] Part 2: brews built (2026-10-27). `bm_core::brew` (13 kinds, Boundless's numbers, drunk through `PlayerState::eat`, `brew_tick` in the step, Lifesaving inside `take`), `Effects::brewed` for damage, action speed and notice range on the server and in the client's swings, Insight's healing (`Body::heal_bonus`), the Brewing levels, Soothing and Taunting moving creatures' threat (`game/food.rs`), Glowing lighting the drinker for them and for others; 3 liquids, 3 oils, 5 flasks, 45 brews and their recipes; icons (`bm-texgen/src/brews.rs`); the HUD tag and tooltips; the `player_brews` table. Protocol 40; players per snapshot 25 (our own state grew). 10 core tests, a prediction test, an end-to-end test (kept across a restart; Lifesaving from a killing blow), the store round trip; 782 tests.
- Upgrade check: a universe made by `main` (protocol 38, two bots on two planets) opened with no warnings, and its players logged back in with their characters (no brews table yet: none).
- Lifesaving tells the player "Your Lifesaving Brew saved you" (2026-10-27, `game/food.rs`).
- Known limits: the Glowing level is a guess; Deepwater's recipe is a guess.
- [ ] Nathan's play-test
