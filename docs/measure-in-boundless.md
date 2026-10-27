# Things to measure in Boundless

*Started 2026-10-27.* Every number below is one Boundmore needs but that no community site, data file, API or forum post has (searched: the forum, the wiki, boundlexx, boundlessinfo, Mayumi's data, pfiffel; see each milestone's research). Until Nathan measures it in the live game it stays a **guess** in `docs/feel.md`.

**How to answer:** whenever there is time, do a trip below and reply with the item's ID and what you saw ("A1: 22 s", "E4: about 150, my health went 1000 to 850"). Rough is fine; a stopwatch or counting seconds out loud is enough. Partial trips are useful too. Each answer goes into `feel.md` (or the spec) with the date, the code is changed to match, and the item is ticked here.

**Before any trip, note your character:** level, and the ranks in any skill the item names (Agility, Vitality, Zeal, Intelligence, the armour skills). Most items want an untrained or lightly trained character; a fresh alt is ideal for A, C, D and E.

**For Claude:** when a new guess is made that only the live game can answer, add it here in the right trip, with what Boundmore uses now. Tick items (`[x]`, with the answer and date) once recorded in `feel.md`.

Ordered by how much they matter; within a trip, top items first.

---

## A. A lake trip (tier 1 world, a deep lake; then one lava pool)

Boundmore now: one air bar for water and bad air, 30 s full to empty, about 8 s back to full, then drowning at 10% of max health every 5 s; swim 3.2 blocks a second up, down and across.

- [ ] **A1. Breath.** Float at the surface until the air bar is full, then dive with C and stay under. Seconds from going under until the bar is empty.
- [ ] **A2. Refill.** Surface with the bar empty: seconds until it is full again.
- [ ] **A3. Drowning.** Stay under after it empties: health lost each time it hits, and seconds between hits (note your max health). Does it hit harder the longer you stay? Is there a countdown on screen, and how long?
- [ ] **A4. Swim speeds.** Over a counted stretch of blocks (or between two pillars a known distance apart): seconds swimming across at the surface; seconds to swim straight down 10 blocks holding C; seconds to sink 10 blocks holding nothing; seconds to rise 10 blocks holding Space. Does sprint change surface swimming?
- [ ] **A5. With That Sinking Feeling** (any item with the quirk, held): repeat A1, A2 and the sink in A4. Can you rise at all with Space?
- [ ] **A6. Agility** (if you have a character with Agility ranks): repeat the surface swim in A4 and note the ranks. Players said Agility made swimming faster; our notes said it did not.
- [ ] **A7. Lava.** Step into lava at full health and step out after a few seconds: the first hit, then how often it hits while you stay in. (We take a third of max health at once, then every second.)

## B. An unprotected trip to a tier 4 to 6 world

Boundmore now: 30 s of air, then choking at 5% of max health every 5 s, the same however far short; air comes back over about 8 s; choking stops healing.

- [ ] **B1.** Arrive with no protection: seconds until the air bar is empty.
- [ ] **B2.** After it empties: is there a countdown, and how long? Then health lost per hit and seconds between hits. If you can, compare being 1 short (tier 4) with 3 or more short (tier 5 or 6).
- [ ] **B3.** Put on protection (eat the food, or warp out) with the bar empty: back to full at once, or over how many seconds?
- [ ] **B4.** While choking, does health still come back from being Full?

## C. Energy and hunger (fresh character, flat ground)

Boundmore now: energy refills at 300 a second (empty to full in 2 s); running 0.5 hunger a second, sprinting 1, a jump 1; swimming and standing cost nothing; defeat adds a quarter of the bar; starving takes 2% of max health every 5 s.

- [ ] **C1. Energy refill.** Empty the energy bar (swing a tool at rock until it stops), then seconds until it is full again, with no Zeal or regen skills.
- [ ] **C2. Hunger from moving.** From a fresh meal (no hunger shading): run on flat ground for one minute and note how much of the bar is shaded (a fraction is fine, e.g. "a tenth"). Then sprint one minute. Then jump 20 times on the spot.
- [ ] **C3. Hunger from work.** Mine 100 blocks with an iron hammer: how much is shaded?
- [ ] **C4. Swimming and standing.** Does swimming for a minute, or standing still for five, add any shading?
- [ ] **C5. Full.** From hungry, eat until Full: does health only start coming back when the shading is almost gone? How fast does it come back (health a second, or seconds from half to full)?
- [ ] **C6. Starving.** At full hunger: health lost per hit and how often.
- [ ] **C7. Defeat.** Right after a defeat, how much is shaded? And with how much health do you come back (a half, a quarter)?
- [ ] **C8.** A berry dish's Recovery buff: how long does it last?
- [ ] **C9.** Fill a bucket from a single water block: is the water block used up?

## D. Falls (fresh character, full health; note max health)

Boundmore now: a share of max health by impact speed, softer on soft blocks, a little gentler than Boundless on purpose.

- [ ] **D1.** Health lost falling exactly 10, 20 and 30 blocks onto rock. Then the same onto sand or soil.
- [ ] **D2.** From what height does a fall first hurt at all?

## E. Creatures (tier 1 world first; tier 3 if you can)

Boundmore now: grazer walks 1.4, charges 12 blocks a second over up to 15 blocks, 3 s between charges, notices at 16, gives up at 32; pelter walks 3, runs 5.5, shoots every 2 s from up to 24 blocks, notices at 20; dasher walks 1.6, sprints 9, notices at 14.

- [ ] **E1. Damage.** On a character with no armour skills: health lost from one grazer (wildstock) charge and one pelter (spitter) shot, on tier 1. If you get the chance, the same on tier 3 and higher (we only have tiers 1 and 2 measured).
- [ ] **E2. Noticing.** Walk slowly toward a calm pelter: roughly how many blocks away does it turn on you? The same for a grazer you have not hit.
- [ ] **E3. Giving up.** Once one chases you, run straight away: about how far before it gives up?
- [ ] **E4. Speeds.** Time each kind over a counted stretch: walking, and running or charging.
- [ ] **E5. Pelter rate.** Shots a minute (or seconds between volleys); how many shots in a volley on tiers 1, 2 and 3.
- [ ] **E6. Numbers.** About how many creatures are usually within 50 blocks of you on a tier 1 world? How far away do they appear?
- [ ] **E7. Elemental creatures.** On a tier 5 and a tier 6 gem world: roughly how many in ten creatures are elemental? (Ours: 1 in 10 and about 1 in 4.)

## F. Mining, light and blocks

- [ ] **F1.** Hit a block a few times and stop: how long until its damage is gone, and does it heal gradually or all at once? (Ours: all at once after 5 s.)
- [ ] **F2.** At night in an open field: how far does a basic torch light (blocks from it that you can still read the ground), next to a glowing crystal (gleam) and a Glow 3 tool held? Which lights furthest?
- [ ] **F3.** Grapples, by feel only: swinging on a rope and letting go, does the fling carry long, or fade quickly? (No numbers needed; "carries further than ours" is enough.)

## G. Regions and meteors (tier 3 world)

- [ ] **G1.** Walk out from a region's centre: at what distance does the HUD change from the name to "North of ..."?
- [ ] **G2.** A meteor you trigger alone: its level, how many waves, roughly how many creatures a wave, and how many rough oortstone the crate gave.
- [ ] **G3.** How long an untouched meteor takes to go dormant.

## H. Farming

- [ ] **H1.** For each crop (starberry, tuber, waxy tuber, exotic tuber, oats, wheat, rice), sown on tilled soil in good light with no fertiliser: the crop HUD's "Expected time to maturity". Rice in water and out of it.
- [ ] **H2.** Each seed's tooltip: which Crop Sowing level it needs.
- [ ] **H3.** The same crop on untilled soil: the expected time.

## I. Forging (a Centraforge)

- [ ] **I1. Effectiveness.** Note your Intelligence ranks, the Attributes Bonus, the Forging Epic, and how many coils are on the forge. Put in each of a wood, stone, copper or iron, and silver tool, a chisel and a grapple, and read the Effectiveness in the forge screen's circle. If you can take the coils off (or use a forge without them), the same again.
- [ ] **I2.** Stability and Vigour at the start of a session, with your Vitality and Power ranks.
- [ ] **I3.** Queue time after Setting Resin for a low-rank item (a wood or stone tool with one boon) and a high one.
- [ ] **I4.** When Stability falls to 0 or below: is the item destroyed at once, or can you still seal it?
- [ ] **I5.** Can Venerable boons (liquid breaker, lattice) appear without a Venerable Gum?
- [ ] **I6. Quirks, held** (any you have): Slippery Feet, how far you slide after letting go of forward at a run, against without it; Bouncy Feet, from what height a landing bounces and how high; One Jump Ahead, jumps a minute standing still; Wonky Trigger, misfires a minute; Nutritionally Challenged, standing still from fed, how long until hunger shading first shows; Bleeding Edge, health lost a hit and how often, when not Full.

## J. Brews and their tooltips

Mostly reading item tooltips (the wiki's copies of these were broken or out of date). Note your Intelligence and Zeal ranks.

- [ ] **J1. Tooltips to read:** Improved and Super Innocuous Brew (how much the creature attack range drops); Improved and Super Luminous Brew (light radius); Mega Cleansing Brew (cleansing points); Kraken Brew and Zombie Lungs Brew (duration, and Zombie Lungs' effect); Brew Extension Helix (what it adds).
- [ ] **J2. Healing over time.** Drink any Healing (not Instant) Brew while hurt: health per tick and seconds between ticks, with the brew's grade. (We use 75, 150, 300, 600, 1200 every 5 s for a minute; the wiki's table was older.)
- [ ] **J3. Reviver.** Defeated with a Reviver Brew active: with how much health and energy do you get up?
- [ ] **J4.** Drink a Fast Brew, then a Healing (over time) Brew: does the Fast one go? Then an Instant Healing one: does the Healing one stay?
- [ ] **J5.** Durations on the buff icon right after drinking: a Mega Fast Brew and a Mega Recovery Brew, with your Zeal ranks.

## K. Building

- [ ] **K1.** Poles, beams and panes: how thick are they (in sixteenths of a block, or "about a third of a block")? Do beams meeting a pole look joined? Do poles show caps at their ends? (Ours: pole 6/16, beam 8/16, pane 2/16.)
- [ ] **K2.** Do chairs face the player who placed them? Can you walk through panes or tables?
- [ ] **K3.** Signs: how long can the further ("additional") text be? Can a standalone sign stand on a floor, and how is it turned?
- [ ] **K5.** How many slots does a shop stand have, and a request basket? (Ours: 16 each.)
- [ ] **K4.** Machines: when you break a damaged machine and place it again, does it keep its wear?

## L. Worlds, by eye (no numbers needed)

- [ ] **L1.** How many blocks across does a biome usually run? How wide are rivers, and how often do you meet one?
- [ ] **L2.** On a tier 7 exoworld: roughly how many seams of each lucent ore (pyrelith rarest, gloamite commonest?).
