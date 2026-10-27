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

*Specced once `docs/research/brews-and-breath.md` is in.* Boundless's brews from its recipe data (mixer; five grades; brew containers at the workbench; restorative, boost, reduction and channelling oils; purified and fortified water and milk), one brew buff beside the food buff, and an air brew.

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

## Status

- [x] Part 1: water (2026-10-27). Breath and drowning on the air bar (`PlayerState::submerged`, `drowning()`, `atmosphere::DROWN_PERCENT`), Leaden's halved drain and refill (`breath_halvings`), X to swim down and to fly down in creative (`PlayerInput::descend`, protocol 39), neutral buoyancy, the Breath and Drowning tags on the bar, fog only for bad air. 5 core tests, 1 prediction test, the air bar test extended; 768 tests. No save change.
- Research in (`docs/research/brews-and-breath.md`): breath and swim timings are not online anywhere reachable, so they stay on the Measure list; players say Agility raised swim speed in Boundless (open).
- [ ] Part 2: brews (spec, then build)
- [ ] Nathan's play-test
