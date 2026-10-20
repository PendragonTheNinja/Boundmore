# Boundless tuning reference

The numbers Boundmore tunes to (CLAUDE.md: Boundless first, identity second; Nathan, 2026-10-19). One section per area, compiled from the community's published game data (boundlessinfo.com, boundless.mayumi.fi, pfiffel.com/bl, including the data files those sites host), the forum's developer posts and patch notes, and the fandom wiki. Every number carries its source, version and a confidence.

Boundless's item names appear only to say which number belongs to which item; Boundmore names its own items. How a number is used in Boundmore, and what is still a guess, is in `docs/feel.md`.

| Section | Area | Status |
|---|---|---|
| 1 | Grapples and player movement | compiled 2026-10-20 |
| 2 | Tools and mining: tool stats per tier, block health by world tier, the damage formula, boons, skills, drops | compiled 2026-10-20 |
| 3 | Weapons, combat, health, creatures: slingbows, bombs, damage formula, elements, creature stats and drops, meteors | compiled 2026-10-20 |
| 4 | Materials, worlds, crafting | in progress |
| 5 | Skills, attributes, forging | in progress |

Older notes: `boundless-mechanics.md` (how systems work, EA-era numbers) and `boundless-forum-feedback.md` (what players liked and disliked).

---

## 1. Grapples and player movement: tuning reference

Units: 1 block = 1 m; speeds in m/s. Item names below are Boundless's, used only to say which number belongs to which item; Boundmore must rename them.

Source keys used throughout:
- **[BI]** boundlessinfo.com, data file `https://boundlessinfo.com/assets/data/item-attributes.json` (fan site; game attribute bundles "Item ... Initial", the final live values, post-2020 since it lists the Oortstone grapple). Confidence high for the numbers, medium on exact game version.
- **[MY]** boundless.mayumi.fi recipes data: `recipes/recipes.js` (recipes) and `recipes/raw/compileditems.msgpack` (decoded with `recipes/javascript.js`'s key mapping). Late-game data.
- **[WIKI-G]** fandom wiki `Grapple` page table (added 2018-03-06 for Testing 187, updated 2018-09-28 for the 1.0 launch; metal rows and Diamond updated, other gem rows still EA). https://boundless.fandom.com/api.php?action=parse&page=Grapple&prop=wikitext&format=json
- **[T204]** dev patch notes Testing 204 (2018-11-01), https://forum.playboundless.com/t/20937
- **[FT]** Forge trait mega-thread, Release 203 era (2018-10-25), https://forum.playboundless.com/t/20169 (copies the in-game trait tables).
- **[LUCA]** dev lucadeltodecso, 2020-08-10, https://forum.playboundless.com/t/49679 (post 7, 9, 16).

### 1.1 Grapple stats per item (final live values, [BI])

"Energy" = Burst Stamina Drain, the energy spent per shot. "Hunger drain" = Hunger Stamina Drain attribute. Action speed is 5 base for every grapple seen in tooltips. Rank = Default Rank (forge rank of an unforged item). Flexibility = forging budget.

| Tier | Item (Boundless name, for identification) | Max length (m) | Shot speed (m/s) | Reel in (m/s) | Reel out (m/s) | Hookshot | Durability | Energy / shot | Hunger drain | Rank | Flexibility |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Metal | Copper | 10 | 45 | 5 | 15 | no | 500 | 60 | 0.12 | 30 | 110 |
| Metal | Iron "Long-Range" | 20 | 30 | 2 | 10 | no | 1000 | 80 | 0.16 | 30 | 110 |
| Alloy | Silver "Swift-Firing" | 18 | 50 | 10 | 20 | no | 1000 | 100 | 0.20 | 50 | 100 |
| Alloy | Gold "Long-Range" | 25 | 30 | 10 | 20 | no | 1000 | 100 | 0.20 | 50 | 90 |
| Alloy | Titanium "Balanced" | 20 | 40 | 40 | 40 | no | 1200 | 120 | 0.24 | 80 | 90 |
| Gem | Amethyst "Swift-Firing" | 15 | 55 | 40 | 30 | no | 2000 | 150 | 0.30 | 100 | 70 |
| Gem | Topaz "Swift-Reeling" | 15 | 40 | 55 | 25 | no | 2000 | 150 | 0.30 | 100 | 70 |
| Gem | Diamond "Long-Range" | 30 | 35 | 30 | 40 | no | 2000 | 150 | 0.30 | 100 | 80 |
| Gem | Emerald "Long-Range" | 30 | 35 | 40 | 30 | no | 2000 | 150 | 0.30 | 100 | 80 |
| Gem | Ruby "Balanced" | 20 | 45 | 45 | 35 | no | 2000 | 150 | 0.30 | 100 | 75 |
| Gem | Sapphire "Balanced" | 20 | 45 | 35 | 45 | no | 2000 | 150 | 0.30 | 100 | 75 |
| Lucent (exo) | Rift "Long-Range" | 40 | 40 | 45 | 45 | no | 2000 | 250 | 0.50 | 180 | 10 |
| Lucent (exo) | Umbris "Balanced" | 30 | 50 | 55 | 55 | no | 2000 | 250 | 0.50 | 180 | 10 |
| Lucent (exo) | Blink "Swift" | 20 | 70 | 70 | 40 | no | 2000 | 250 | 0.50 | 180 | 10 |
| Special | Oortstone | 35 | 45 | 60 | 50 | no | 1000 | 250 | 0.50 | 200 | 10 |

Cross-checks (all agree with [BI]):
- Blink tooltip screenshot 2019-05-27: shot 70, range 20, energy 250, reel in 70, reel out 40, hookshot no, durability 2000, action speed 5, flexibility 10, rank 180. https://forum.playboundless.com/t/30476 (high).
- Emerald tooltip screenshots 2021-02-05 (base values in brackets): shot 35, range 30, energy 150, reel in 40, reel out 30, durability 2000, rank 100, flexibility 80. https://forum.playboundless.com/t/54569 post 9 and 11 (high).
- A player 2021: "rift starts at 40m"; "40 m/s reel speed is base stat for emerald". Same thread (high).
- [T204] Silver: durability 500 to 1000, length 15 to 18; Gold durability 800 to 1000; Titanium 1000 to 1500 (but [BI] shows 1200, so it was changed again later; medium).
- [WIKI-G] metal rows at 1.0 launch (2018-09): Copper 10 m/45/reel-out 15/reel-in 5; Iron 20/30/10/2; Silver 15/50/20/10 (pre-204); Gold 25/30/20/10; Titanium 20/40/40/40; Diamond 30/35/40(out)/30(in). All equal to [BI] apart from the T204 changes, so the metal and Diamond numbers have been stable since 1.0 (high).
- No base grapple is a hookshot in the final data. Gold and Diamond were hookshots in EA (Release 155 to about 1.0; https://forum.playboundless.com/t/7148, 2017-04-15) and the wiki still had "Gold Hookshot"/"Diamond Hookshot" rows in 2018-03. At 1.0 they became Long-Range and hookshot moved to a forge boon (section 3).
- Wiki stamina-drain column (2018-09) for metals matches [BI] energy (60, 80, 100, 100, 120; gems 150 to 156).

Rarity / gating: lucent grapples need an exo planet's T7 ore and a powered workbench (power 3000); gem grapples power 1500.

### 1.2 Crafting (all at the Workbench, skill Tech Crafting) [MY], confirmed by wiki item pages

Times are for a single craft (bulk = 3 items, mass = 10 items take 3x and 10x). "Remnant/Component/Device" are the three tiers of salvaged ancient-tech parts.

| Item | Ingredients (single craft) | Time | Spark | Power | Tech Crafting level |
|---|---|---|---|---|---|
| Copper | 3 copper bar, 1 sackcloth, 1 glue, 3 tech remnant | 60 s | 0 | 0 | 1 |
| Iron | 3 iron bar, 1 sackcloth, 1 glue, 3 remnant | 120 s | 0 | 0 | 1 |
| Silver | 3 silver alloy, 1 cotton cloth, 1 bonding agent, 5 remnant | 240 s | 800 | 0 | 2 |
| Gold | 3 gold alloy, 1 cotton cloth, 1 bonding agent, 5 remnant | 480 s | 800 | 0 | 2 |
| Titanium | 3 titanium alloy, 1 cotton cloth, 1 enriched bonding agent, 1 component, 5 remnant | 1200 s | 1500 | 0 | 2 |
| Gems (each) | 3 refined gem, 1 leather, 1 enriched bonding agent, 1 component, 5 remnant (Topaz and Sapphire: 1 remnant) | 1800 s | 2000 | 1500 | 3 |
| Lucents (each) | 3 refined lucent, 1 hard leather, 1 super-enriched bonding agent, 1 device, 3 component, 10 remnant | 3600 s | 4000 | 3000 | 4 |

Wiki item pages also give craft XP (copper 10, iron 14, silver 40, gold 56, titanium 100, gems 160, blink 150, rift/umbris 320) and coin value (3, 5, 40, 40, 70, 200 gems, 500 lucents). Levels from wiki `{{RecipeBox}}` (Version 244), e.g. https://boundless.fandom.com/api.php?action=parse&page=Copper_Grapple&prop=wikitext&format=json. The Oortstone grapple has no workbench recipe in [MY] (source unknown: likely an event or shop reward; low).

### 1.3 Modifiers: forge boons, skill, guild buff

| Modifier | Effect per level | Max | Source, confidence |
|---|---|---|---|
| Far Reaching boon (length) | +3 m | lvl 10 = +30 m | [FT]; 2020 and 2022 players quote "Level 10 Range (+30m)" (https://forum.playboundless.com/t/62866, /t/39949); tooltip +30 m (/t/54569). High |
| Fly Fast boon (shot speed) | +5 m/s | lvl 10 = +50 m/s | [FT]; tooltips show +35 and +45 (lvl 7, 9). High |
| Reel Speedster boon | +5 m/s reel in and +5 m/s reel out | lvl 10 = +50/+50 | [FT]; Alwin 2020 "5m/s per reel speed level" (/t/49679 post 18). High |
| Hookshot boon (single level) | grapple auto-reels once attached; +20 m/s reel in | 1 | [FT] (rank +20). A 2023 player says it also adds reel out (https://forum.playboundless.com/t/63986); low on reel out |
| Heavy-duty boon | +200 durability | +2000 | [FT]; tooltip +2000 (/t/54569). High |
| Energy Saver boon | -2 energy per use | -20 | [FT]. High |
| Glow boon | light radius 6/9/12, intensity 1/1.8/2.4 | 3 levels | [FT]. High |
| Defects (Limited Reach, Sluggish Shot, Reel Time, Fragile, Energy Expender) | mirror the boons, negative | 10 | [FT]; defect value multiplier -0.5 in pfiffel `forge.json` (applies to the forging points, not obviously to the effect; low) |
| Movement quirks a held grapple can carry | On Icy Ground (every block slides like ice; from Release 209 also +40 falling armour), Bouncy Feet (every surface acts like a trampoline), That Sinking Feeling (sink in water), Slow-Mo (from Release 209: walk/run speed -25%, sprint bonus +50%), One Jump Ahead (random jumps, grants a double jump) | | [FT]; Release 209 notes https://forum.playboundless.com/t/22871; /t/22943. Medium |
| Grapple Mastery skill (5 ranks, 1 point each) | +5 m length and +5 m/s shot speed per rank | +25 m, +25 m/s | tooltip text quoted 2020 and 2022 (https://forum.playboundless.com/t/39949, /t/63086); the bonus is not shown on the item tooltip but works (DJDancecraft, 2022). Pre-195 mastery also had a "difficulty": without enough mastery a grapple got range and shot speed penalties (wiki; /t/10744, 2017-12). Removed in Release 195 ("Mastery streamlined, no more difficulty, purely additive", https://forum.playboundless.com/t/14250). Mastery also raised fist damage (tooltip; reported bugged 2018). High for the +5/+5 |
| Grapple and Run guild buff | Minor: run +0.2 m/s, length +5 m; Greater: +0.4 m/s, +15 m; Mega: +0.6 m/s, +25 m | | wiki pages Minor/Greater/Mega Grapple and Run (Version 244); Ovis 2022 shows it also raises sprint speed (https://forum.playboundless.com/t/61352). High |

So a fully built grapple length is base + 30 (boon) + 25 (mastery) + 25 (guild), e.g. Rift 40 to 120 m; players quoted a "max range (70m) Rift" (base + boon, 2020, /t/46219) and "55m" rift (/t/63074). Boons are capped at 3 per item.

### 1.4 Speed caps and how reel works

- Player and creature speed is capped at **40 m/s** (blocks/s); projectiles up to about **100 m/s** "from memory". A reel speed above 40 does not raise top speed but accelerates you to the cap faster and fights air drag better ([LUCA] post 7). The cap exists because faster clients outrun chunk download and meshing ([LUCA] post 16). Terminal falling speed shown in the debug menu is 40.00 (same thread, post 1). High.
- Player measurement (Alwin, same thread post 12): reeling up, both 75 and 40 m/s reel took about 40 m of travel to reach 40 m/s. Reeling down (reel out): 65 m/s reel reached -21 m/s after 20 m, max -28 m/s after 80 m; 40 m/s reel: -21 after 30 m, max -23.5 after 57 m; 30 m/s reel: -21 after 40 m. Medium (eyeballed from debug menu). This implies reel is a force/acceleration toward a target rope speed, not a fixed rope speed.
- A 2021 forger: "reel speed physically capped at 40 m/s, slightly higher for hookshots" (https://forum.playboundless.com/t/54569). Low on the hookshot part.
- Creative-mode flight is 40 m/s; max run with all skills about 10 m/s, about 15 with a mega speed brew (debug velocity, Soju-VB, /t/49679 post 17). Medium.

Engine grapple parameters in [MY] `compileditems` (identical for every grapple, so they are tuning constants of the rope model, meaning partly inferred from names; medium): `swingSpeed` 0.05, `climbSpeed` 2, `friction` 0.01, `controlFriction` 5 (3 on the claw projectile), `stopLerp` 5, `dropLerp` 5, `disipateLengthMultiplier` 1.1 (rope breaks when stretched past 1.1x its max length?), `hookshotMinDistance` 0 (5 on the claw), `hookshotStrength` 1.5, `hookshotDrag` 0.01, `initialBoost` 0, `subsequentBoost` 0, `equipTime` 0.2 s, claw projectile `breaksOnContactWithGround` true, `range` 5. The item-level `reelInSpeed` field (copper 5, iron 2, silver 10, gold 30, titanium 20, amethyst 35, diamond/emerald/topaz 50, ruby/sapphire 20, rift 70, umbris 30, blink 80, oortstone 60) does not match the attribute bundles for most items and looks like a stale default overridden by the attributes; treat as unused (low).

### 1.5 Grapple behaviour (mechanics notes)

- The hook is a projectile with no gravity ("Projectile Fall Off" 0 for all, [BI]); it flies at shot speed to max length and attaches to blocks, creatures, drops and (friends only, from 2017-10) players (james, https://forum.playboundless.com/t/10235, /t/11560). History: Release 152 (2017-01) dual-hand grapples and projectile hook; Release 155 (2017-02-08) first per-material variation of max range and reel speeds and hookshots; impact damage was disabled while grappling "as a temporary workaround" (https://forum.playboundless.com/t/6337). High.
- Controls: fire attaches, fire again releases; R reels in, F reels out (dev luke-turbulenz 2017, https://forum.playboundless.com/t/6343). PS4: circle detaches the oldest grapple (Testing 202). Edge climbing is disabled while attached (Release 182).
- Swing: Release 197.1 (2018-08) changed the grapple so pumping back and forth builds a larger and larger swing, and long grapples can nearly restore the height of the down-swing while you keep pushing forward (https://forum.playboundless.com/t/14809). Release 196 "reworked grapple loop". High.
- Hookshot: reels in automatically on attach and does not stop until you reach the target (Release 155 notes). Gem grapples "retract automatically when hanging" (Testing 162 known issue).
- Stamina: from Release 192 (2018-05) the grapple detaches when stamina (energy) hits 0, intended by devs (https://forum.playboundless.com/t/13212). From Testing 187, grapples cost energy per use, shown on the item; you cannot fire if the cost exceeds current energy (https://forum.playboundless.com/t/11861). High.
- Fall/impact protection: you take no fall damage while attached; releasing before landing removes the protection (james 2018-09-22, https://forum.playboundless.com/t/17209). Impact damage applies to horizontal speed too (swinging into walls), the most-complained grapple danger (Nathan's own thread https://forum.playboundless.com/t/18612, 2018-10; james 2018-12-11 "on the TODO", /t/23224). Fast reel lucent grapples were called "a defeat penalty a dozen times" (/t/36742). High.
- Durability: one point per shot and one per normal release; releasing by switching items costs none (player, https://forum.playboundless.com/t/57078). Medium.
- Underwater grapples were fixed to attach in Release 211 (/t/23287); the hook flies slower underwater (player 2023, /t/65939). Medium.
- No grappling of non-friends to stop grief-pulling into lava (james 2017).

### 1.6 Player movement

| Quantity | Value | Source, date, confidence |
|---|---|---|
| Run speed, base (no Agility) | 3.0 m/s | wiki Agility Bonus page (Version ~244), medium (one row of the table is inconsistent: 3.6 to 4.2) |
| Sprint speed, base | 5.0 m/s | same, medium |
| Agility Bonus skill (8 ranks, 1 point each) | +10 Agility, +0.3 m/s run, +0.6 m/s sprint, +2% crit per rank; rank 8: 5.7 run / 9.8 sprint | https://boundless.fandom.com/api.php?action=parse&page=Agility_Bonus&prop=wikitext&format=json, medium-high |
| Attributes Bonus skill (5 ranks) | +2 to all attributes per rank (wiki "Attribute") | low on exact value |
| Max run with all skills | about 10 m/s (debug menu, 2020); ~15 with mega speed brew | /t/49679 post 17, medium |
| Guild buff run | +0.2 / +0.4 / +0.6 m/s (also sprint) | section 3 |
| Player and creature speed cap | 40 m/s; terminal fall 40 m/s | [LUCA], high |
| Lava swimming | about half water swim speed | Release 195 notes, high |
| Jump height, base | about 1 block (cannot clear 2-block ridges without a skill) | players 2018-08, https://forum.playboundless.com/t/15305, medium |
| Jump Distance (high jump) Epic, 5 points | "jump 2 blocks higher" (3 m total per a player); hold Space for a higher jump | /t/15305 posts 4, 7; Release 195 report /t/14248 post 114; medium |
| Double Jump Epic, 5 points | second jump in mid-air, about half a block if combined with high jump; fails when falling fast; press twice at apex | /t/27577, /t/27590 (2019), luca 2017 /t/7337 post 24; medium |
| Floating foods | add jump height, double jump and falling armour | /t/17271 (2018), with high and double jump epics a player clears ~6 blocks (/t/27590 post 18); low on numbers |
| Edge climb (EA skill) | climb a nearby ledge after a jump or grapple | Release 176 tooltip (/t/9218), EA only, low |
| Swim | slow; hold jump to float; Agility does not affect swim | players 2018 to 2020, low on numbers |

EA history (do not use for 1.0): Testing 162 (2017-04) lowered base jump and raised jump skills; Testing 168 (2017-07) cut jump skills again (players: "~4 blocks with all skills"). 2017 skill list: sprint about 166.7% of run; Agility +0.3 run, +0.5 sprint per rank (https://forum.playboundless.com/t/8224).

### 1.7 Energy (stamina) and hunger

- Energy (the "stamina" bar after Testing 187): base max 600, raised by Zeal (wiki Citizen page). Using a held item costs its Burst Stamina Drain (grapple 60 to 250, section 1); energy quickly recovers after a short pause; Energy Regeneration epic raises regen; max energy is reduced by hunger, which grows with item use, moving, jumping and sprinting (Testing 187 notes /t/11861; wiki). Movement itself does not cost energy after 187 (player 2018, /t/15162). Medium-high.
- Hunger Stamina Drain per item (grapples 0.12 to 0.5, [BI]) is presumably the hunger added per use. Low on meaning.
- From Testing 249 (2020-12) shields also drain stamina and block its regeneration while raised.
- Regen starts only after a pause of about 3 to 4 s since the last action (timed by a player, 2018-10-01, https://forum.playboundless.com/t/18199); since Release 199 (2018-09) holding the use button at zero energy keeps resetting that delay (/t/17741). The Energy Regeneration epic's tooltip says 900 energy per second (same thread; Testing 194 player quote "Energy 900/s for 5 skill points"), so once regen starts the bar refills almost at once. Medium.
- A 2018 player says running consumed a percentage of energy rather than a fixed amount (/t/18199 post 4); not confirmed elsewhere. Low.
- Zeal scaling of max energy: not found.

### 1.8 Fall and impact damage

- Damage depends on impact velocity against the block hit (falls and sideways hits alike) and is a percentage of max health, so extra Vitality does not help (players 2019-2024, /t/26625, /t/41874, /t/66868; dev rossstephens 2019-02-25 "still reviewing"; never changed through 2024). High on the shape, no formula published.
- Release 197 (2018-08) made fall damage "more realistic" and made the landing block matter: soft blocks (sponge, foliage, sand, gold) hurt less than rock, gem and metal (notes /t/14809; player tests 2024 /t/66868). Landing on slopes at a shallow angle, and slippery blocks (ice, slide), avoid or reduce damage (Release 195 notes).
- Falling Armour (Endurance, 5 points; about 300 armour points at max per a 2018-09 player, /t/16029, low) is a flat absorption, so it protects low-health builds well (one player says 32 blocks with 1/9 HP) and does little at high health (4400-5000 to 4200-4600 damage in a 2024 test). /t/27577, /t/66868. Medium.
- A few frames of no impact damage after a jump (Release 195). Building Safety guild buff Mega negates 100% fall damage (2024 player). Kinetic/Impulse armour skills exist (wiki skill list).

### 1.9 Conflicts between sources

- 2017 measured table (boundless-mechanics.md 2.2, /t/7140) is EA and differs wholesale from 1.0 (e.g. Sapphire 40 m there, 20 m now; Gold and Diamond hookshots then, not now). Use section 1.
- Wiki Grapple table rows for Amethyst, Emerald, Topaz, Ruby, Sapphire are Testing-187 EA values with old names (e.g. "Emerald Swift-Reeling 35 m", "Sapphire Long-Range 45 m"); [BI] and 2019-2021 tooltips disagree. Use [BI].
- Titanium durability: T204 notes 1500, [BI] 1200. Prefer [BI] (later), medium.
- Wiki table column order puts reel-out before reel-in; [BI] confirms copper reel in 5, reel out 15 (reel out is faster than reel in on metal grapples).
- Hookshot boon reel out bonus: [FT] says reel in only, a 2023 player says both.

### 1.10 Open questions

- Exact rope physics: how reel speed maps to force (Alwin's data suggests acceleration toward a target speed), air drag constant, swing input force (`swingSpeed` 0.05?), what `stopLerp`/`dropLerp` do.
- Base jump height in metres and gravity value; double-jump impulse; walk vs run vs creep speeds (creep exists, slower, stops you falling off edges).
- Swim speed and breath time; energy regen per second; Zeal scaling of max energy.
- Fall damage formula (velocity threshold, percentage curve, per-block hardness factor) and Falling Armour's absorption per rank.
- Whether the Oortstone grapple was craftable and where it came from.
- Agility 3.6 to 4.2 jump in the wiki table is probably a typo (expected 3.9).

---

## 2. Tools and mining (tuning reference)

Scope: hammers, shovels, axes, totems, chisels (stats only), hoes, block health and armour per world tier, the damage formula, hits to break, skills, forge boons, drops. Numbers come first from community sites that publish data taken from the game files (boundlessinfo.com, pfiffel.com/bl, boundless.mayumi.fi), cross-checked against forum posts and dev release notes. Boundless names appear only to identify which item a number belongs to.

Source keys used below:
- **[BI-attr]** boundlessinfo.com `assets/data/item-attributes.json` (per-item attribute table, final game version, fetched 2026-10-19). Confidence high: exported game data, matches dev notes.
- **[BI-calc]** boundlessinfo.com `common.bf51c4152c1e09a1.js` (forge auto/manual calculator: tool base damage, world-tier block health multiplier and armour, block categories, damage formula). High.
- **[BI-forge]** boundlessinfo.com chunk `47.8a6dfb8ba62ce5c2.js` (forge trait table with per-level values and point costs). High.
- **[BI-rec]** boundlessinfo.com `assets/data/compiled-recipes.json` + `item-names.json` (recipes). High.
- **[PF-dmg]** pfiffel.com/bl/dmg/`damageCalculator.js` (same formulas, independent author; block health table, AoE multipliers). High.
- **[PF-drops]** pfiffel.com/bl/drops/`dataBlockDrops.js`, `dropsBlocks.js` (drop chances per world tier and luck formula). High.
- **[PF-blocks]** pfiffel.com/bl/blocks/`dataBlockStats.js` (mine XP per block). High.
- **[R204]** Testing 204 / Release 205 notes, dev (james / Steggs101), 2018-11-01 / 11-06: https://forum.playboundless.com/t/20937 and https://forum.playboundless.com/t/21207
- Other forum threads cited inline by topic id (`t/<id>` means `https://forum.playboundless.com/t/<id>`).

Version note: the data-site numbers are the final live values (the game stopped receiving updates around 2021 to 2022). They match the November 2018 balance pass (R204) exactly, so tool base stats were stable from Release 205 (Nov 2018) to the end. Earlier values (EA, 1.0 launch) are listed only under Conflicts.

---

### 2.1 Tool table (one row per material; hammer, shovel and axe of a material share stats)

All three tool types of one material have identical stats; the type only decides which blocks take full damage (2.4). From [BI-attr] unless noted; "type" is the recipe name prefix [BI-rec]. Energy = energy (stamina) spent per use ("burst stamina drain"); the attribute table also lists a per-use hunger drain of exactly energy/100. "Hardness" = the item's "can damage hardness" value (10 = can damage every natural block; see 2.3). Flex = forge flexibility (affects forging only). Rank = default gear rank shown in the tooltip.

| Material | Recipe name type | Damage | Action speed | Crit chance | Crit effect | Durability | Energy per use | Hardness | Flex | Rank |
|---|---|---|---|---|---|---|---|---|---|---|
| Totem (basic) | Totem | 200 | 20 | 0% | +20% | 400 | 4 | 10 | 35 | 0 |
| Wood | (plain) | 300 | 20 | 0% | +20% | 600 | 6 | 10 | 150 | 10 |
| Stone | Heavy | 400 | 15 | 0% | +20% | 600 | 8 | 10 | 120 | 20 |
| Copper | Swift | 400 | 30 | 0% | +20% | 700 | 8 | 10 | 110 | 30 |
| Iron | Heavy | 700 | 10 | 20% | +20% | 900 | 14 | 10 | 110 | 30 |
| Silver (alloy) | Swift | 650 | 30 | 0% | +20% | 1200 | 13 | 10 | 100 | 50 |
| Gold (alloy) | Heavy | 700 | 10 | 50% | +75% | 1100 | 14 | 10 | 90 | 50 |
| Titanium (alloy) | Balanced | 1000 | 25 | 20% | +40% | 1500 | 20 | 10 | 90 | 80 |
| Amethyst | Swift | 1000 | 38 | 20% | +20% | 2400 | 20 | 10 | 70 | 100 |
| Topaz | Swift | 1100 | 38 | 0% | +20% | 2400 | 22 | 10 | 70 | 100 |
| Sapphire | Balanced | 1300 | 30 | 20% | +20% | 2200 | 26 | 10 | 75 | 100 |
| Ruby | Balanced | 1400 | 30 | 0% | +20% | 2200 | 28 | 10 | 75 | 100 |
| Emerald | Heavy | 1800 | 15 | 20% | +20% | 2000 | 36 | 10 | 80 | 100 |
| Diamond | Heavy | 1900 | 15 | 0% | +20% | 2000 | 38 | 10 | 80 | 100 |
| Blink (exo, T7) | Swift | 1350 | 45 | 0% | +20% | 3800 | 27 | 10 | 10 | 180 |
| Umbris / dark matter (exo, T7) | Balanced | 2000 | 30 | 10% | +20% | 3200 | 40 | 10 | 10 | 180 |
| Rift (exo, T7) | Heavy | 3400 | 5 | 0% | +20% | 3000 | 68 | 10 | 10 | 180 |
| Oort stone (in data, never craftable as far as known) | Balanced | 2500 | 20 | 20% | +20% | 3000 | 50 | 10 | 10 | 200 |
| Founders' totem (backer reward) | | 1000 | 25 | 0% | +20% | 1500 | 20 | 10 | 35 | 80 |
| Rainbow totem (event) | | 750 | 25 | 0% | +20% | 1500 | 15 | 10 | 35 | 75 |
| Candy totem (event) | | 600 | 25 | 0% | +20% | 750 | 12 | 10 | 35 | 50 |

Notes on the table:
- Pattern worth copying: energy per use = damage / 50 for every tool (and totems), so damage per energy is flat across tiers; heavier tools only cost more energy per swing. Exact for every row. [BI-attr]
- "Heavy / Swift / Balanced" is only a naming label for the material's damage vs speed trade-off; it is not a separate stat. Heavy = low action speed, high damage; Swift = high speed, low damage.
- Gem tools carry an element in their weapon role; for blocks it does not matter (no elemental block resistances found).
- Damage, crit and durability values cross-checked: R204 (dev) lists the Nov 2018 changes that produce exactly these numbers (silver 650/1200, gold 700/1100, titanium 1000, amethyst 1000 dmg 38 speed 2400 dur, topaz 1100/38/2400, ruby 1400/2200, sapphire 1300/2200). [PF-dmg] and [BI-calc] tool tables give the same damage and crit for every material. Confidence high.
- Exo tools (rift, blink, umbris) arrived with Exoworlds, Testing 218 / Release 219 (March to April 2019): t/28046, t/28550.
- The founders' totem is described by the dev as "as effective as similar-material hammers, axes and shovels, one tool instead of three" (Release 195, 2018-07-27, t/14250). Data says it equals a titanium tool without crit. Totems ignore tool mastery and tool epic skills [BI-calc formula].

Other tools for context [BI-attr]:

| Item | Action speed | Durability | Energy per use | Hardness | Notes |
|---|---|---|---|---|---|
| Basic chisel (all stone/metal/alloy chisels) | 50 | 800 | 10 | 3 | cannot affect blocks above hardness 3 (gem, hard coal etc.) |
| Advanced chisel (gem chisels) | 80 | 2000 | 20 | 10 | |
| Wood hoe | 20 | 500 | 6 | | farming |
| Metal hoe | 30 | 700 | 8 | | farming |
| Spanners (stone to gem) | 30 to 70 | 500 to 2000 | 4 to 50 | | repair 100 (stone) to 10000 (gem) machine wear per hit |

Chisel durability per cut is 1 durability per hit like other tools (not measured separately; medium). The data has only two chisel stat groups, so within a group (e.g. copper slope vs titanium precise square) the stats are equal and only the cut type differs. Confidence medium (the export may merge item variants).

### 2.2 World tier: block health and armour

Block health = category base health × tier multiplier. Armour is a flat subtraction from every hit. From [BI-calc] and [PF-dmg] (identical tables), confirmed by player measurement (Brynjar 2018-11-13, t/21316: rock 1200/1200/1800/2400/3600/4800 and armour 0/0/100/200/400/600 for T1 to T6; rossstephens, dev, 2018-11-22, t/22228: armour values "not altered since August" 2018).

| World tier (player-facing) | Name | Health multiplier | Block armour | Soil / sand / foliage / flora (800 base) | Rock, wood, gravel etc. (1200 base) | Seams (2400 base) |
|---|---|---|---|---|---|---|
| T1 | Placid | 1 | 0 | 800 | 1200 | 2400 |
| T2 | Temperate | 1 | 0 | 800 | 1200 | 2400 |
| T3 | Rugged | 1.5 | 100 | 1200 | 1800 | 3600 |
| T4 | Inhospitable | 2 | 200 | 1600 | 2400 | 4800 |
| T5 | Turbulent | 3 | 400 | 2400 | 3600 | 7200 |
| T6 | Fierce | 4 | 600 | 3200 | 4800 | 9600 |
| T7 | Savage | 5 | 800 | 4000 | 6000 | 12000 |
| T8 | Brutal | 6 | 1000 | 4800 | 7200 | 14400 |

Block categories and base health [BI-calc]:
- hard (1200): rock, ice, glacier, dark glass, gleam (the glowing crystal block), titan heart
- soft (1200): gravel, mud, ash, sponge, growth, ancient corruption, titan flesh
- natural (1200): wood trunks, tangle, thorns, mould, titan skin
- soil (800): soil, sand
- flora (800): foliage, flowers, plants, fungus
- surface rock (800): boulders
- seam (2400): every resource seam (ores, coal, gems, fossils, tech remnants)
- Plants, fungus and boulders (mesh objects) ignore block armour [BI-calc, PF-dmg].

Notes:
- Same table for every world tier is also the creature tier ladder; T8 never shipped as a public world type but exists in the data (pfiffel offers "Show Tier 8").
- Blocks a player places inherit the armour and health of the world they sit on (t/17380, Havok40k 2018-09-26; t/22321, 2018-11). There is no depth scaling: a block's health depends only on its category and the world tier. (No source found for any depth or altitude effect; the dev design post t/8444, 2017-07-12, describes scaling "per world" only. Medium.)
- Atmosphere level per tier, same table [BI-calc]: 0,0,0,1,3,5,7,10 (T1 to T8); this is the hazard that atmosphere-protection skills/foods counter, unrelated to mining.

### 2.3 Damage formula and hits to break

Per hit, per block [BI-calc `calculateDamage`, PF-dmg]:

```
base      = tool base damage (2.1)
e         = base × aoe_mult + forge_flat             (forge only; see 2.6)
              forge damage boon level L adds base × ceil(L/2) × 0.10 + 50 × L
              day/night quirk adds ±(0.10 × base + 100)
skill_mult = 1 + (0.05 × power + brew) × (3 if damage epic else 1)
               + 0.05 × tool_mastery + (0.25 if tool epic)          (mastery/epic not for totems)
dmg       = ceil(e × skill_mult)
if crit (chance roll):  dmg × (1 + crit_effect)
dealt     = max(dmg − block_armour, 0)    × 0.25 if wrong tool type (2.4)
hits      = ceil(block_health / dealt)
```

- power = Power attribute points, 0 to 9 (8 from the Power skill plus 1 from the all-attributes skill at 5/5). Maximum skill_mult without brew = 1 + 0.45×3 + 0.25 + 0.25 = **2.85**. Check: max-skill diamond 3×3 AoE hammer with damage boon 8 shows 5472 damage in game = 1920 × 2.85 (toastkid 2018-12-22, t/23665; Nightstar's 2023 combat log in t/63432 shows the same 5472 on all nine blocks). High.
- brew = strength brew 0.1 per tier (minor 0.1 to mega 0.5); strength bomb buff adds 0.25 to the same term; both are multiplied by the damage epic's ×3 [BI-calc, PF-dmg].
- Crit chance = tool crit + 0.05 per crit boon level + agility: 0.2 × (agility/9) (so 9 agility = +20%) [BI-calc]. Crit damage multiplier = 1 + crit effect. Crit is rolled per hit before armour (Brynjar t/21316). The wiki Agility page gives +2% per point (8 points = 16%; the 9th comes from the all-attributes skill): https://boundless.fandom.com/wiki/Agility_Bonus. High.
- Armour is subtracted per hit, so it hurts multi-hit tools much more than one-hit tools (Prome3us t/23442, 2018-12-18; Crete: "each armour icon = −100 damage per hit"). The HUD shows armour as small shield icons under the block health bar (Release 196 dev note, 2018-08-08, t/14494), one icon per 100.
- Partial damage: a hit block stays damaged for a short time and then heals back to full (players race "before their health is restored", t/18295, 2018-10-01). The heal delay is not documented (open question).
- One-shot convention used by the whole player economy: "a T6 tool" = one-shots rock (1200-base blocks) on T6 with max skills. Needed damage after skills = health + armour: T3 1900, T4 2600, T5 4000, T6 5400, T7 6800 (6800 confirmed for T7 by Mayumichi t/41899 2020-02-20 and scrufola t/40855 2020-01-29).

Derived hits to break one 1200-base block (rock, trunk, gravel) with an unforged tool, no crit, no brew, right tool type (computed from 2.1 and 2.2; "–" = cannot damage):

Untrained character (skill_mult 1):

| Tool | Hit | T1-2 | T3 | T4 | T5 | T6 | T7 |
|---|---|---|---|---|---|---|---|
| Totem | 200 | 6 | 18 | – | – | – | – |
| Wood | 300 | 4 | 9 | 24 | – | – | – |
| Stone / Copper | 400 | 3 | 6 | 12 | – | – | – |
| Iron / Gold | 700 | 2 | 3 | 5 | 12 | 48 | – |
| Silver | 650 | 2 | 4 | 6 | 15 | 96 | – |
| Titanium / Amethyst | 1000 | 2 | 2 | 3 | 6 | 12 | 30 |
| Topaz | 1100 | 2 | 2 | 3 | 6 | 10 | 20 |
| Sapphire | 1300 | 1 | 2 | 3 | 4 | 7 | 12 |
| Ruby | 1400 | 1 | 2 | 2 | 4 | 6 | 10 |
| Emerald | 1800 | 1 | 2 | 2 | 3 | 4 | 6 |
| Diamond | 1900 | 1 | 1 | 2 | 3 | 4 | 6 |
| Blink | 1350 | 1 | 2 | 3 | 4 | 7 | 11 |
| Umbris | 2000 | 1 | 1 | 2 | 3 | 4 | 5 |
| Rift | 3400 | 1 | 1 | 1 | 2 | 2 | 3 |

Maximum mining skills (skill_mult 2.85; totem 2.35):

| Tool | Hit | T1-2 | T3 | T4 | T5 | T6 | T7 |
|---|---|---|---|---|---|---|---|
| Totem | 470 | 3 | 5 | 9 | 52 | – | – |
| Wood | 855 | 2 | 3 | 4 | 8 | 19 | 110 |
| Stone / Copper | 1140 | 2 | 2 | 3 | 5 | 9 | 18 |
| Iron / Gold | 1995 | 1 | 1 | 2 | 3 | 4 | 6 |
| Silver | 1853 | 1 | 2 | 2 | 3 | 4 | 6 |
| Titanium / Amethyst | 2850 | 1 | 1 | 1 | 2 | 3 | 3 |
| Topaz | 3135 | 1 | 1 | 1 | 2 | 2 | 3 |
| Sapphire | 3705 | 1 | 1 | 1 | 2 | 2 | 3 |
| Ruby | 3990 | 1 | 1 | 1 | 2 | 2 | 2 |
| Emerald | 5130 | 1 | 1 | 1 | 1 | 2 | 2 |
| Diamond | 5415 | 1 | 1 | 1 | 1 | 1 | 2 |
| Umbris | 5700 | 1 | 1 | 1 | 1 | 1 | 2 |
| Rift | 9690 | 1 | 1 | 1 | 1 | 1 | 1 |

Seams take twice the hits' worth of health; soil/sand/foliage take two thirds. Crits shorten these on average (gold's 50% chance / +75% effect is its whole point). Cross-check: Brynjar's simulated "rocks per unforged tool" tables (t/21316, patch 205) agree with these breakpoints. A player diary of the 2017 design target (dev, t/8444): "level 1 world: 3 hits wood hammer, 2 stone, 1 metal", which the final numbers roughly keep for a skilled character.

### 2.4 Tool type vs block type ("right tool" rule)

- Hammer: the hard category (rock, ice, glacier, gleam, dark glass) and all seams/ores. Shovel: soil, sand, gravel, mud, ash, growth and other soft blocks. Axe: wood trunks, foliage, plants, flowers, fungus. Sources: wiki Tool page (https://boundless.fandom.com/wiki/Tool, "hammer: rock and seams; axe: trunks, timber, plants; shovel: soils, sand, gravel"); willcrutchley t/19853 (2018-10-19): the three tools deal different damage types and blocks have different resistances. Medium-high (exact category to tool map is not in the published data; the forum agrees with this split).
- Wrong tool: hit deals 25% of its post-armour damage. Alienfish (t/23442, 2018-12-18), Cuetzpalomitl (t/50140, 2020-08-27), scrufola (t/54870, 2021-02-19: wrong tool = (weapon damage − block armour)/4). cuzzea in the same thread measured roughly "total × 0.25 − base × 0.13", i.e. not exactly a quarter; treat 25% as the rule (medium).
- A guild buff machine ("hammer versatility", Release 221, 2019-04-25, t/29139) raises a hammer's wrong-block damage: levels 1/2/3 multiply the quarter-damage by 1.25/1.5/1.75 (scrufola t/54870). Players reported it did not help on gravel/sand; scope uncertain (low).
- Totem: damages wood, rock, ice, sand, soil (wiki https://boundless.fandom.com/wiki/Totem, version 244). Since Release 195 (2018-07-27, t/14250) totems, hammers, axes and shovels can damage all blocks: the earlier material "hardness" lock (e.g. needing iron to mine a gem seam) was removed in favour of world armour; dev reasoning in t/16711 (james, 2018-09-17): a heavily forged wood hammer still could not touch high-level seams, and the lock was arbitrary and confusing. Chisels kept a hardness lock (basic chisels hardness 3, gem chisels 10) [BI-attr].

### 2.5 Action speed, energy, durability

- Action speed is a stat, not seconds. Character skills add up to +40 to a tool (dexterity 2 per point, 9 points with the all-attributes skill = 18; mastery and the tool epic supply the other 22: Heilos t/19616 expected +22 from hammer skills). Forge "busy bee" adds +5 per level (2.6). Fast brews multiply the total (mega fast ×2). Sources: wakeNbake/whitelet t/47769 and t/53011 (2020): "max action speed for tools is 120; maxed skills give +40; sapphire (30 base) + 40 = 70, +50 forge = 120; diamond 15 + 40 = 55, ×2 with mega fast brew = 110". The wiki's Dexterity page: +2 action speed per point (https://boundless.fandom.com/wiki/Dexterity_Bonus); the wiki's Hammer Mastery page says +2 per point and +10% damage per point (that page predates the launch halving to 5%, see Conflicts). Medium-high.
- Cap: hits per second stop rising at a total action speed of about 120 (players) with a hard cap at 150 and a soft cap near 125 (a13o's sheet, quoted by Pfiffel t/30132 2019-05-20; Nightstar t/63432 2023). High that a cap exists near 120 to 125.
- Measured hits per second vs action speed (Mayumichi, metronome method, t/30132 2019-05-21, chart https://forum.playboundless.com/uploads/default/original/3X/e/2/e2e194cff393cc3fc1d05c32769e92122e2e0cd6.png). The rate climbs in steps:

| Total action speed | 10 | 20 | 30 | 40 | 50 | 60 | 80 | 90 | 100 | 110 | 120+ |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Hits per second | 1.07 | 1.33 | 1.6 | 2.0 | 2.3 | 2.66 | 4.0 | 5.3 | 5.3 | 5.3 | 8.2 |

(Values read off the chart's four series by total speed; base 5 = 1.0 hit/s, 70 ≈ 3.2. Steps suggest the game rounds the swing cooldown to whole animation frames. Confidence medium: hand-timed, but consistent across series.) For Boundmore: a curve from ~1 hit/s at speed 5 to ~8 hits/s at 120 is the target; the AoE thread quotes an older cap of 20 actions per second (a13o t/23156, 2018-12), superseded.
- Testing 200 (2018-10-08, t/18904, dev): cooldowns were reworked so the same action speed means the same rate on every tool and weapon.
- Energy: each swing costs the tool's energy per use (2.1); energy refills over time (energy regeneration epic: empty to full in about 3 s, Dhusk t/34506 2019-08-27). Forge "energy saver" −2 per level (2.6). Chisel energy halved in Release 196 (2018-08-08, t/14494).
- Durability: one point per swing, regardless of how many blocks an AoE swing breaks (a13o t/23156: a 3×3 is "as good as a 9× multiplier on durability"; Dhusk t/34506: one 2600-durability T6 AoE hammer yielded 2802 topaz). Wear reducers: persisting food −5/−10/−15/−30/−40% durability drain (soup, stew, risotto, loaf, pie; willcrutchley t/17271, 2018-09-22); a durability-drain epic skill (value not found). Buffs and skills multiply rather than add (Nightstar t/63432). Totems got durability in Release 195 to stop macro use ("no tool should last forever", t/14250).

### 2.6 Forging (Centraforge) boons on tools

From [BI-forge] (game trait table; costs are forge points per level). A tool can carry several boons; total points depend on the forge session (see section 6 of the mechanics notes).

| Trait | Effect per level | Levels | Point cost by level |
|---|---|---|---|
| Damage boon ("devastating") | +10% base per two levels (ceil(L/2)×10%) and +50 flat per level; L10 = +50% and +500 | 10 | 100, 150, 300, 600, 1000, 1500, 2000, 3000, 4000, 5000 |
| Action speed ("busy bee") | +5 action speed per level (L10 +50) | 10 | same |
| Durability ("heavy-duty") | +200 durability per level (L10 +2000) | 10 | same |
| Energy saver | −2 energy per use per level (L10 −20) | 10 | same |
| Crit chance | +5% per level | 10 | same |
| Crit effect ("hard hitter") | +10% per level | 10 | same |
| Glow | tool emits light | 3 | 300, 1000, 2000 |
| Drop attraction ("magnetic collector") | pulls drops from +5 / +7 / +9 m | 3 | 300, 1000, 2000 |
| Area of effect (one trait, 5 levels) | L1 3 blocks in a horizontal row at 40% damage; L2 2 blocks (hit + one below) at 60%; L3 plus-shape of 5 at 40%; L4 3×3 of 9 at 40%; L5 full damage to the hit block plus a random number of neighbours | 5 | 100, 300, 1000, 2000, 4000 |
| Liquid breaker (venerable forging) | lets a hammer, shovel or axe break and collect liquid blocks | 1 | |
| Solidifier (totem only) | hit turns lava to rock, water to ice | 1 | |
| Defects (mirror of each boon) | same magnitudes, negative | | |
| Quirks | day/night damage ±(10% base + 100); action speed at high/low health; energy at full/empty hunger; random noises; weather; "wonky trigger" (+durability, random swings, +400 per a guide quoted in t/63432); etc. | 1 | 600 |

- AoE math: the AoE multiplier replaces the 100% base (e = base × 0.4 + damage boon), it is not a separate penalty applied after. So a 3×3 diamond with damage 8 = 1900 × (0.4 + 0.4) + 400 = 1920 [BI-calc, PF-dmg]. Players describe it as "you lose 60% of base damage for 3×3" (Unagi4me t/23156). Every block in the AoE pattern takes the same damage (Nightstar t/63432, combat log 2023).
- AoE L5 (random) breaks about 1 to 4 extra blocks per swing at full damage (NerArth t/29288, 2019-04-28; pfiffel models it as 6). Unpopular because it eats durability.
- Economic weight: a13o (t/23156, 2018-12-10) shows the 3×3 boon gives ~3.6× DPS and ~9× blocks per durability; most end-game mining used 3×3 hammers with damage + speed or durability. Gem 3×3 hammers needed damage boon 8 (diamond) to one-shot T6 rock with max skills (Dhusk t/34506; toastkid t/23665). Players called this the "gem wall" problem.
- Trait point costs per level match the earlier player figures (100 to 5000) in the mechanics notes (t/13556).

### 2.7 Skills that change mining (post-launch values, Release 197 on)

Formula terms in 2.3. Values from the calculators [BI-calc, PF-dmg] plus the skill calculator author's launch changelog (Logandi, t/14749, 2018-08-18: "power 10% → 5%, mastery 10% → 5%, tool epic 50% → 25%; damage epic now triples").

| Skill | Cost | Effect on tools |
|---|---|---|
| Power (attribute) | 1 pt per level, max 8 (+1 from all-attributes bonus) | +5% damage per point (max +45%) |
| Damage epic (epic 3) | 5 pts | triples the power and strength-brew term (max +135% instead of +45%) |
| Hammer / shovel / axe mastery | 1 pt per level, max 5 | +5% damage per point for that tool type (max +25%) and part of the +22 action speed |
| Hammer epic; shovel-and-axe epic | 5 pts | +25% damage for that tool type and action speed |
| Dexterity (attribute) | max 8 (+1) | +2 action speed per point (and block placement speed) |
| Agility (attribute) | max 8 (+1) | crit chance 0.2 × points/9 (about +2.2% per point, 20% at 9; wiki says +2% per point) |
| Luck (attribute) | max 8 (+1) | more bonus drops, see 2.8 |
| Gathering epic | 5 pts | doubles bonus drop chance on gathering blocks (plants, fungus, boulders) and lets you pick up and replant them (Release 195) |
| Energy, energy regeneration epics | | bigger energy pool, faster refill (~3 s empty to full, Dhusk) |
| Durability drain epic | 5 pts | less tool wear (size unknown) |
| Light source epic | 5 pts | character light, frees a hand from torches |
| Chisels epic | 5 pts | precise (corner) cuts; see section 1 |

Typical miner build (Dhusk t/34506, 2019; CreativeWorlds t/17712, 2018): 8/8 power, 5/5 all attributes, 5/5 hammer mastery, hammer epic, damage epic, energy epic, energy regeneration epic, durability drain epic, luck 8/8.

### 2.8 Drops, yields and luck

- Breaking a block always drops the block itself (or its ore item for seams), probability 1. Extra drops are separate entries with a chance per world tier [PF-drops].
- Luck: every non-guaranteed chance is multiplied by (1 + luck/9 × 1.8), i.e. up to ×2.8 at 9 luck; gathering epic adds +1 to that multiplier on gathering blocks; each entry is capped at 100% [PF-drops formula]. Release 197 (2018-08-16, t/14809) "doubled the effect of luck on rarer drops" (this is the 0.9 × 2 factor). Bonus drop replaces the self-drop on blocks that drop a bonus item (Release 195 anti-macro change).
- Seam bonus yield (expected extra ore per seam, before luck) [PF-drops]:

| World tier | T1 | T2 | T3 | T4 | T5 | T6 | T7 |
|---|---|---|---|---|---|---|---|
| Copper / coal (common seams) | +0.5 | +0.5 | +0.7 | +0.7 | +1.05 | +1.05 | +1.05 |
| Gem seams (diamond etc.) | – | – | – | – | +0.5 | +0.8 | +0.8 (+1.2 at T8) |

  So with max luck a gem seam on T5 to T7 drops 2 gems (1 + min(1, 0.5 × 2.8)). Dhusk's yield rule of thumb "gems ≈ hammer durability + 10%" for a 3×3 matches this.
- Rare side drops from ordinary blocks [PF-drops], e.g. rock: 1.5 to 2% a fragment of flint on T1 to T4, 1% rock salt from T4, 0.1% opal or sulphur from T4; gravel 5% flint; trunks 8 to 12% sap, 3 to 4% bark from T3; sand 0.2 to 0.5% olivine.
- Mining XP per block [PF-blocks]: soil/sand/gravel/rock/trunk 2; gleam 8; copper/soft coal/small fossil seam 24; iron 40; silver/coal/medium fossil 64; gold 80; hard coal/large fossil 104; titanium 136; gem seams 176; exo seams (rift, blink, umbris) 216.

### 2.9 Crafting recipes and machines (tools) [BI-rec]

Per single craft (bulk and mass crafts give 3 and 10 for more inputs and time). Wear = machine wear. All tools of a material share the recipe; the chisel of that material uses the same recipe.

| Material | Machine | Skill | Inputs | Time | Spark | Wear |
|---|---|---|---|---|---|---|
| Totem | handcrafted | – | none | instant | – | – |
| Wood | crafting table | – | 2 timber, 1 stick, 1 foliage | instant | – | 1 |
| Stone | crafting table | – | 4 stones, 4 sticks, 4 foliage | 10 s | – | 1 |
| Copper | workbench | tool crafting 1 | 3 copper bars, 4 sticks, 1 sackcloth, 1 glue | 60 s | – | 7 |
| Iron | workbench | tool crafting 1 | 3 iron bars, 4 sticks, 1 sackcloth, 1 glue | 120 s | – | 11 |
| Silver | workbench | tool crafting 2 | 3 silver alloy, 4 sticks, 1 cotton cloth, 1 bonding agent | 240 s | 800 | 17 |
| Gold | workbench | tool crafting 2 | 3 gold alloy, same | 480 s | 800 | 30 |
| Titanium | workbench | tool crafting 2 | 3 titanium alloy, 4 sticks, 1 cotton cloth, 1 enriched bonding agent | 1200 s | 1500 | 70 |
| Any gem | workbench (needs power 1500) | tool crafting 3 | 3 refined gem, 4 sticks, 1 leather, 1 enriched bonding agent | 1800 s | 2000 | 100 |
| Rift / blink / umbris | workbench (power 3000) | tool crafting 4 | 3 refined exo gem, 4 sticks, 1 hard leather, 1 super-enriched bonding agent | 3600 s | 4000 | 110 |
| Founders' totem | workbench | – | 3 titanium alloy, 4 sticks, cloth, enriched agent, 4 tech remnants | 1200 s | – | 70 |

Chisels use "tech crafting" levels 1 to 3 in place of tool crafting. Tool crafting 1 and 2 are basic skill points; level 3+ comes from the gear/tool crafting epics (wiki https://boundless.fandom.com/wiki/Tool_Crafting).

### 2.10 Bombs (block clearing, for comparison)

Bombs damage blocks too, through "impulse" damage: player-derived formula (willcrutchley 2018-08-25, t/15154; Nightstar t/50140): rock takes (damage − impulse armour) × 0.5 × 0.75, seams × 0.5 × 1.1, so bombs break seams before the rock around them. Measured one-shot bomb damage for rock: T1/2 3750, T3 5500, T4 7200, T5 10725, T6 14190 (wakeNbake t/50140, 2020-08-28). Bomb mining was nerfed in Testing 199 (Sept 2018) to stop it beating hammers. Low-medium (formula incomplete at T1).

---

### Conflicts

- **Wiki Hammer / Shovel / Axe / Tool pages** list different numbers (e.g. wood durability 300, copper durability 500, sapphire damage 1800, gem action speeds 75/85, "energy drain" 0 for gems, "rank" and "flexibility" rows). They mix EA, launch and later values and some columns are shifted. Superseded by [BI-attr] and R204. Do not use.
- **Power and mastery per point**: wiki Power Bonus page says +10% per point up to 80%; wiki Hammer Mastery says +10% damage per point. Launch (Release 197, Aug 2018) halved both to 5% and the tool epic from 50% to 25% (Logandi t/14749, calculators). Use 5%.
- **Block armour at T6/T7**: players wrote 400 (Prome3us 2018-12) or 800 (Nightstar 2023) for T6; data says 600 at T6 and 800 at T7. The 800 in Nightstar's post fits T7. Use the data table.
- **Wrong-tool penalty**: 25% (most posts) vs one measurement suggesting an extra subtraction (cuzzea 2021). Use 25% of post-armour damage.
- **Action speed cap**: 120 (sellers), 125 soft / 150 hard (a13o via Pfiffel), "20 actions per second" (a13o 2018, older). The measured hit rate stops at ~8.2 hits/s.
- **Wood tool durability**: 600 in data, 300 on the wiki; recipes on the wiki also show larger stick and foliage counts for wood tools than the data (4/4/4 vs 2/1/1).
- **Totem damage scope**: wiki lists only some block types; Release 195 says all tools can damage all blocks.

### Open questions

1. How long a partially damaged block waits before healing back to full, and whether it heals gradually or all at once. No figure found.
2. Exact action speed → swing cooldown formula (only a hand-timed chart exists; a13o's sheet holds a formula but Google Sheets is unreachable: https://docs.google.com/spreadsheets/d/1L5_AifEeMMnHyeEcsXjmSNKKSLdDGcMlY-qGvTMuE2o).
3. Split of the +22 skill action speed between tool mastery (per point) and the tool epic; the action speed bonus of hammer epic.
4. Exact category → tool type map for soft blocks (gravel and mud are "soft" for health, and players break them with shovels; confirm ash, growth, sponge).
5. Durability drain epic size; energy pool size and regeneration rate per skill.
6. The full per-shape chisel speed and whether precise cuts cost more durability (see section 1).
7. Sickles: no such tool exists in Boundless (plants are broken with axes or by hand; the gathering epic lets you pick them up). Hoes exist for farming (2.1).

---

## 3. Weapons, combat, health and creatures

Scope: everything a Boundless player fights with or against, as the game stood at its last live version (Release 248, Oct 2020; servers kept running after that without combat changes). Item names below are Boundless's and are used only to say which item a number belongs to; Boundmore must rename everything.

**Source key** (cited by tag in the tables; every number carries a tag and a confidence):

| Tag | Source | Date / version | Notes |
|---|---|---|---|
| [BI] | boundlessinfo.com, `assets/data/item-attributes.json` (fan site publishing the game's item attributes) | data matches live 24x (Lucent items present, melee/shields from Testing 249 also present) | primary source for per-item weapon stats |
| [BI-drops] | boundlessinfo.com Angular chunk `c883.*.js` (creature drop tables) | same | drop weights per creature tier |
| [PF] | pfiffel.com/bl/dmg/ `damageCalculator.js` and `damageCalculatorCreatures.js` | 2019-2020 (cited by players in April 2020, t/44307) | creature HP, armour, XP; full damage formula; boon values |
| [PF-forge] | pfiffel.com/bl/forge/ `forge.json` | 2019+ | trait list, point costs, which gear each trait applies to |
| [BI-forge] | boundlessinfo.com Angular chunk `c47.*.js` (forge trait texts with per-level values) | same as [BI] | exact boon values per level |
| [MAY] | boundless.mayumi.fi/recipes/ `recipes.js`, `items.js`, `itemIds.js` | 2019-2020 | recipes, machines, craft times, spark, power |
| [WIKI] | boundless.fandom.com MediaWiki API | pages tagged version 244 (mid 2020) or older | |
| [F:id] | forum.playboundless.com topic id (`https://forum.playboundless.com/t/<id>`) | date given per row | dev posts by `james` are release notes |

Confidence: **high** = game data published by a fan site and cross-checked by a forum screenshot or dev note; **medium** = single data source or careful player testing; **low** = single player observation, old, or inferred.

---

### 3.1 Weapons by tier (tables first)

#### 3.1.1 Slingbows (the main ranged weapon)

All values [BI], confidence high; cross-checked against the wiki slingbow table [WIKI Slingbow, marked Outdated but identical numbers] and two in-game tooltips posted 2020-04-14 (Gold Heavy and Rift Heavy, [F:44307]: damage 540 / 2400, projectile speed 35 m/s, range 20 / 30 m, crit 50% / 0%, crit effect 75% / 75%, energy drain 11 / 48, durability 950 / 2000, flexibility 90 / 10, projectile weight 30 g). Damage type for all player slingbows is "Impulse", source "Direct".

| Material (style) | Rank | Damage | Element | Action speed | Proj. speed (m/s) | Range (m) | Fall-off | Crit chance | Crit effect | Durability | Energy per shot | Flexibility |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Stone | 20 | 300 | none | 10 | 30 | 15 | 0.5 | 0 | 0.20 | 600 | 6 | 120 |
| Copper (swift) | 30 | 300 | none | 30 | 55 | 15 | 0.5 | 0 | 0.20 | 500 | 6 | 110 |
| Iron (heavy) | 30 | 600 | none | 10 | 35 | 20 | 0.6 | 0.20 | 0.20 | 900 | 12 | 110 |
| Silver (swift) | 50 | 500 | none | 30 | 55 | 20 | 0.5 | 0 | 0.20 | 1000 | 10 | 100 |
| Gold (heavy) | 50 | 540 | none | 15 | 35 | 20 | 0.6 | 0.50 | 0.75 | 950 | 11 | 90 |
| Titanium (balanced) | 80 | 800 | none | 20 | 45 | 20 | 0.5 | 0.20 | 0.30 | 1200 | 16 | 90 |
| Amethyst (swift) | 100 | 650 | Corrosion | 40 | 55 | 25 | 0.5 | 0 | 0.20 | 2000 | 13 | 70 |
| Topaz (swift) | 100 | 650 | Shock | 40 | 55 | 25 | 0.5 | 0 | 0.20 | 2000 | 13 | 70 |
| Ruby (balanced) | 100 | 900 | Burn | 30 | 45 | 25 | 0.5 | 0.20 | 0.20 | 1800 | 18 | 75 |
| Sapphire (balanced) | 100 | 900 | Chill | 30 | 45 | 26 | 0.5 | 0 | 0.20 | 1800 | 18 | 75 |
| Diamond (heavy) | 100 | 1200 | Blast | 10 | 35 | 25 | 0.6 | 0.20 | 0.20 | 1500 | 24 | 80 |
| Emerald (heavy) | 100 | 1200 | Toxin | 10 | 35 | 25 | 0.6 | 0 | 0.20 | 1500 | 24 | 80 |
| Blink (swift, Lucent) | 180 | 950 | Blink (Blast+Burn) | 50 | 55 | 30 | 0.5 | 0 | 0.20 | 2500 | 19 | 10 |
| Umbris (balanced, Lucent) | 180 | 1200 | Umbris (Corrosion+Toxin) | 35 | 45 | 30 | 0.5 | 0.20 | 0.20 | 2200 | 24 | 10 |
| Rift (heavy, Lucent) | 180 | 2400 | Rift (Shock+Chill) | 10 | 35 | 30 | 0.6 | 0 | 0.75 | 2000 | 48 | 10 |
| Oortstone (balanced) | 200 | 1700 | none | 20 | 45 | 30 | 0.5 | 0.20 | 0.20 | 2000 | 34 | 10 |

Patterns worth copying (derived from the table, high confidence):
- Three "styles": **swift** (action speed 30 to 50, projectile 55 m/s, low damage), **balanced** (20 to 35, 45 m/s), **heavy** (10 to 15, 35 m/s, fall-off 0.6, highest damage). Energy per shot is exactly damage / 50 for every slingbow.
- Range steps 15 (stone/copper), 20 (metal, alloy), 25 (gem; sapphire 26), 30 (Lucent, Oortstone).
- All slingbows: charge time 2 s, charge effect 1, knockback 0, break on contact; hunger drain per shot = energy / 500 [BI].
- Projectiles have weight (30 g on heavy, tooltip [F:44307]) and arc; Release 184 made projectiles fly "through the crosshair" [F:10745]. The heavy-projectile boon adds weight and slows the projectile (3.4).
- Infinite basic ammo since Release 149 (Dec 2016); arrows removed; damage crystals ("augments") loaded into the bow give a temporary buff [F:5731]. Augment charges: 20 shots, raised to **125 shots** in Release 203 (Oct 2018) [F:20150]. High.
- Rapid fire and charged shot are skills, not weapon types, since Release 195 (Jul 2018): buying Rapid Fire makes every slingbow auto-fire while held; Charging lets you hold to charge for more damage [F:14250]. High. Before 195 (Release 162, Apr 2017) some bows rapid fired and some charged [F:7337].
- "Action speed" is a unitless stat; skills add to it (+2 per Slingbow Mastery point [WIKI Slingbow Mastery], +24 total from skills on a maxed character in the tooltip [F:44307]). Fast Brew gives +20% to +100% action speed and players report a cap around +60% [F:17271]. **The conversion from action speed to seconds per shot was not found** (open question).

#### 3.1.2 Fists (short-range "punch" projectile)

[BI], high. Only two exist and both were reward-gated recipes (backer / deluxe edition), but tradeable [WIKI Explorer's Fist, Gold Fist].

| Item | Rank | Damage | Type | Action speed | Action time multiplier | Proj. speed | Range (m) | Crit | Crit effect | Durability | Energy | Flex |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Iron fist | 30 | 1360 | Fist | 3 | 2.5 | 45 m/s | 11 | 0.20 | 0.30 | 1000 | 28 | 110 |
| Gold fist | 50 | 1410 | Fist | 4 | 2.5 | 45 m/s | 11 | 0.50 | 0.75 | 700 | 29 | 90 |

Rules: **fists ignore creature armour** (high; Pfiffel 2019-06-01 [F:30640], and the calculator sets armour to 0 for fists [PF]); fists never get the elemental bonus (calculator sets elemental chance 0 for fists) and still suffer the 40% physical resist of elemental creatures (medium). Fists use **Grapple Mastery** for their damage bonus, not a weapon mastery [PF]. Cooldown about 1.5 s between punches (player, 2019-06-01 [F:30640]; low). Fists can mine blocks (can-damage-hardness 10) [BI].

#### 3.1.3 Bombs (bags of bombs; thrown, area damage)

[BI], high. All bombs: action speed 50, crit 10%, crit effect 20%, **durability 200 throws per bag**, fuse/charge time 3.85 s, projectile speed 25 m/s, max throw 100 m, fall-off 0.5, damage source "Area", damage type Impulse, energy per throw = damage / 10 [BI].

| Bag | Rank | Damage | AoE (radius, m, assumed) | Knockback | Element | Energy | Flex |
|---|---|---|---|---|---|---|---|
| Copper | 30 | 900 | 3 | 10 | none | 90 | 110 |
| Iron | 30 | 1500 | 2 | 10 | none | 150 | 110 |
| Silver | 50 | 1350 | 3 | 15 | none | 135 | 100 |
| Gold | 50 | 1800 | 2 | 15 | none | 180 | 90 |
| Titanium | 80 | 2000 | 3 | 15 | none | 200 | 90 |
| Amethyst | 100 | 1500 | 4 | 20 | Corrosion | 150 | 70 |
| Topaz | 100 | 1500 | 4 | 20 | Shock | 150 | 70 |
| Ruby | 100 | 2000 | 3 | 20 | Burn | 200 | 75 |
| Sapphire | 100 | 2000 | 3 | 20 | Chill | 200 | 75 |
| Diamond | 100 | 2500 | 2 | 20 | Blast | 250 | 80 |
| Emerald | 100 | 2500 | 2 | 20 | Toxin | 250 | 80 |
| Blink | 180 | 2500 | 5 | 25 | Blink | 250 | 10 |
| Umbris | 180 | 3400 | 4 | 25 | Umbris | 340 | 10 |
| Rift | 180 | 4300 | 3 | 25 | Rift | 430 | 10 |
| Oortstone | 200 | 3000 | 4 | 25 | (special) | 300 | 10 (never obtainable in practice [WIKI]) |

Behaviour (Release 188, Mar 2018 [F:12306], high): hold to light the fuse; a tap drops the bomb at your feet, a longer hold throws further; explodes on impact with a target or when the fuse runs out; the bomb is always thrown before it explodes. Damage bombs hurt terrain less than before and destroy ore seams; blocks destroyed by bombs drop nothing. Some bombs heal, restore energy or cleanse instead (now forge boons, 3.4). Players complain bombs have too little lifetime value because of the fixed 200 durability (2019-05 [F:29597]).

Bomb charge recipe (every bag needs 1 to 6): 5 flint + 2 saltpetre fragment + 1 volatile blood (a hopper drop), workbench, 90 s [MAY][WIKI Bomb Charge]. High.

#### 3.1.4 Melee weapons and shields (Testing only; never reached Live)

Added in **Testing 249 (2020-12-28)** with the local-universe work [F:53886] and further tuned in 249.2/249.3 [F:53908]; the follow-up live release never shipped (players still waiting in 2022, [F:62037]; "Release 250 waiting in the wings"). Treat as design intent, not as what players played. Stats [BI], medium (Testing data).

Melee: cone attack ("swing radius" m and "swing degrees"), hits up to "max hit" targets, targets toward the edge take a glancing hit at 50% damage, damage type Kinetic.

| Melee | Damage | Action speed | Radius m | Degrees | Max targets | Crit | Crit eff. | Durability | Energy |
|---|---|---|---|---|---|---|---|---|---|
| Wood sword | 280 | 20 | 4 | 50 | 2 | 0 | 0.2 | 500 | 6 |
| Stone mace | 380 | (n/a) | 4 | 50 | 2 | 0 | 0.2 | 600 | 8 |
| Copper axe (swift) | 380 | 30 | 4.5 | 70 | 2 | 0 | 0.2 | 500 | 8 |
| Iron axe (heavy) | 750 | 10 | 4 | 50 | 2 | 0.2 | 0.2 | 900 | 15 |
| Silver mace (swift) | 630 | 30 | 4.5 | 70 | 3 | 0 | 0.2 | 1000 | 13 |
| Gold mace (heavy) | 670 | 15 | 4 | 50 | 2 | 0.5 | 0.75 | 950 | 14 |
| Titanium mace (balanced) | 1000 | 20 | 4 | 90 | 2 | 0.2 | 0.3 | 1200 | 20 |
| Amethyst sword | 800 | 40 | 4 | 50 | 3 | 0 | 0.2 | 2000 | 16 |
| Topaz sword | 820 | 40 | 6 | 70 | 2 | 0 | 0.2 | 2000 | 17 |
| Ruby sword | 1150 | 30 | 4 | 70 | 2 | 0.2 | 0.2 | 1800 | 23 |
| Sapphire sword | 1050 | 30 | 4 | 90 | 3 | 0 | 0.2 | 1800 | 21 |
| Diamond sword | 1500 | 10 | 5 | 90 | 2 | 0.2 | 0.2 | 1500 | 30 |
| Emerald sword | 1350 | 10 | 4 | 70 | 3 | 0 | 0.2 | 1500 | 27 |
| Rift sword | 3000 | 10 | 4.5 | 70 | 2 | 0 | 0.75 | 2000 | 60 |
| Blink glaive | 1450 | 42 | 4 | 90 | 2 | 0 | 0.2 | 2500 | 29 |

Testing 249.2 increased all melee reach by 1 to 1.5 m and made the Blink weapon slower but harder hitting [F:53908]; the [BI] values may be pre- or post-change (low certainty which).

Shields: hold the action button to raise; adds armour against attacks arriving inside the "degrees of protection" cone (including bombs and falls); no stamina while raised, but each blocked hit costs stamina and stamina does not regenerate while raised; shield drops when stamina runs low; crit chance gives "critical blocks" (more armour); gem shields carry the same elemental resistances as the matching creature element [F:53886][F:53908]. [BI] shield armour (kinetic = impulse): wood 35, stone 60, copper 110, iron 150, silver 330, gold 400, titanium 560, sapphire 1020, topaz 1200, amethyst/ruby 1280, emerald 1500, diamond 1620, Blink 2300, Umbris 2600, Rift 3000; cones 120 to 240 degrees; energy per block 10 to 275. Medium.

Testing-only skills: Sword Mastery 5/5 (melee damage and speed), Shield Mastery 5/5 (shield armour), Sword Epic [F:53886].

#### 3.1.5 Other damage sources a player can use

- Tools hit creatures: totem 200, wood 300, stone/copper 400, silver 650, iron/gold 700, titanium/amethyst/Wayfarer 1000 ... diamond 1900, Rift 3400 (tool base damage [PF]); "early tools now do more damage to creatures" (Release 195 [F:14250]). Hammers stun hoppers (Release 188 [F:12306]).
- Knockback into terrain: concussive bombs and wildstock knockback make creatures (and players) take fall/impact damage; players used bomb pits as a kill method (2019-06 [F:30640]). Lava kills creatures (2021 [F:55530]). Low to medium.

---

### 3.2 Creatures

#### 3.2.1 Tier names and where tiers appear

Creature power tiers and adjectives: 1 (none), 2 Stout, 3 Strong, 4 Mighty, 5 Elite, 6 Hyper (Hyper never seen in play) [WIKI Creature][F:50248]. Names replaced numbers in Release 184 [F:10745]. High.

World tiers: 1 Placid, 2 Temperate, 3 Rugged, 4 Inhospitable, 5 Turbulent, 6 Fierce, 7 Savage, 8 Brutal [PF calculator][WIKI Hopper]. Hopper table on the wiki: Rugged worlds hold tiers 1-2, Inhospitable 1-3, Turbulent 1-4, Fierce and above 1-5 [WIKI Hopper]; medium (only stated for hoppers, likely general). Roadrunners exist only at tiers 1-3 [PF][F:50248]. Hoppers only on tier 3+ worlds [WIKI Hopper]; spitters and wildstock on all worlds; tier 1 world creatures are passive [WIKI Spitter], and new characters wear "New Citizen" (not attacked unless they attack) until level 5 or tutorial done (Release 184, refined Testing 249.3) [F:10745][F:53908]. High.

#### 3.2.2 Creature base stats by tier

[PF] `damageCalculatorCreatures.js`, values as (HP / armour / XP). Medium-high (data-derived by a well-regarded player; the wiki's wildstock HP 1500 matches; spitter XP differs, see conflicts).

| Creature | T1 | T2 Stout | T3 Strong | T4 Mighty | T5 Elite |
|---|---|---|---|---|---|
| Wildstock | 1500 / 0 / 60 | 3500 / 0 / 140 | 9000 / 0 / 240 | 25000 / 0 / 360 | 60000 / 0 / 540 |
| Spitter | 2000 / 0 / 140 | 4000 / 50 / 240 | 8000 / 200 / 360 | 16000 / 500 / 540 | 32000 / 1000 / 840 |
| Cuttletrunk | 1500 / 100 / 240 | 3000 / 350 / 360 | 5000 / 750 / 540 | 10000 / 1300 / 840 | 20000 / 2000 / 1200 |
| Hopper | 1000 / 0 / 140 | 2500 / 100 / 240 | 4500 / 200 / 360 | 7000 / 300 / 540 | 10000 / 400 / 840 |
| Roadrunner | 1000 / 0 / 180 | 2500 / 0 / 300 | 5000 / 0 / 480 | (none) | (none) |

Modifiers on top [PF], confirmed by a player-posted table (Vansten 2019-09-04 [F:34744], image) and dev notes:

| Kind | HP | Damage | Armour | Drops | XP |
|---|---|---|---|---|---|
| Elemental (on the six gem-element worlds) | +100% | +50% | +50% | +100% | +50% |
| Lucent / "special" (Blink, Umbris, Rift worlds, tier 7) | +200% | +100% | +100% | +200% | +75% |
| Exotic health buff | +200% | | | | |
| Exotic armour buff | | | +100% | | |

Exotic creatures (visible decals) carry extra buffs from health, damage, armour and speed, and have different attack sets; since Release 195 any creature can be exotic [F:14250]; 20% chance for a spitter [WIKI Spitter] (medium); since Testing 218 exotics are rarer on low worlds, carry 1-2 buffs on low worlds and 3-4 on high worlds [F:28046]; they give extra XP and more drops [WIKI Spitter]. Exotic and elemental stack (Testing 218: "exotic elemental creatures incredibly potent") [F:28046]. Release 159 (Mar 2017) scaled all health and damage down by 10x, so pre-2017 numbers are 10x larger [F:6851].

Creature damage (base hit before tier multiplier), player measurements 2023-07 [F:64706], low-medium:

| Creature | Base hit | Tier multipliers seen |
|---|---|---|
| Wildstock | 100 | Stout x2.5 |
| Spitter | 30 | Stout x3 on its normal shot |
| Cuttletrunk | 100 | Strong attacks up to x6 |
| Hopper (explosion) | 400 | Stout x2.5, Strong x5 |

Other data points: a hit above about 450 goes through a maxed armour build (2019-01 [F:25040]); high tier cuttletrunks can reach about 4000 armour on tier 7 (2019-06 [F:30640]).

Health bar colours (presentation): one green segment = 500 HP, blue = 2500, purple = 12500, orange beyond 62500 (geometric steps of x5) [PF][F:25040]; introduced as "Unified Entity Health Bars 2.0" in Release 184 [F:10745]. Armour shown as shield icons under the bar since Release 196 [F:14494]. High.

#### 3.2.3 Creature types and behaviour

- **Wildstock** (goat-like, passive): attacks only when hurt or when it hears weapon fire nearby; meteor-spawned ones are always aggressive [WIKI Wildstock]. Charge headbutt with knockback; Release 172 (Aug 2017) tuning: charge hit radius 0.5 to 1.8 m, charges in a row 3 to 1, charge speed 6 to 12 m/s, charge range 10 to 15 m, repeat damage only after 0.5 s [F:9054]. Release 184: multiple charges with a short pause, low tiers charge slower, high tiers faster [F:10745]; Testing 219: lowest tier charges only once [F:28046]. Higher-tier wildstock have a random chance to be aggressive (Release 172). Charges break blocks (Release 195 fix). Players say the knockback into terrain (fall damage) is the real killer [F:26625]. High.
- **Spitter** (lizard-like, ranged; hostile except on tier 1 worlds): single shots, 3-shot rapid bursts (tier 1 needs an exotic), fans/spread, shotgun with knockback (tier 3+ since Release 195), lobbed bombs with area damage [WIKI Spitter][F:10745][F:14250]. Low tiers move slower, high tiers faster. Target break range 30 m (Release 156) [F:6519]. High.
- **Cuttletrunk** (flying tentacled): ranged energy bolts, homing missiles (kinetic damage although they explode, [F:25040]), beams (sweep or direct), bombs and cluster bombs; higher tiers grab and throw the player up for fall damage ("drag attack": picks an area, if you are still in it after 1 s you are lifted and thrown, Release 159 [F:6851]); throw cooldown 50 s, lowered to 30 s (Release 188 [F:12306]). Attacks unlock as its health drops (observed: basic attack at full, bomb at 80%, homing and arcing at 60%, [F:25040]). High armour, so fists and heavy bows are preferred. High.
- **Hopper** (tier 3+ worlds): wanders passive, becomes hostile, whistles for at least 1.5 s then explodes on impact (impulse damage, explosion size by tier) [F:10745][F:12306]; when triggered it can curl into an invulnerable ball that can be grappled and thrown and that explodes if it hits a surface hard [WIKI Hopper]. Does not explode through walls (Release 184). Stunned by hammers. High.
- **Roadrunner** (bird, never aggressive, never from meteors): squawks and sprints away when a player is near, runs on water, hurts itself on cliffs and falls; can be grappled; the only creature that drops rough oortstone outside meteors [WIKI Road Runner][F:10745]. High.
- **Event creatures**: Snow spitter (Oortmas snowball meteors on tier 1-2 worlds; only snowballs hurt it; stacking "embarrassment" debuff defeats you without the defeat penalty) [F:22594]; Gleamtrunk (cuttletrunk variant at Gleambow meteors; drops event sac) [F:46111].
- Planned but never shipped: Hunter, Ground Basher, Stone Thrower, titans [WIKI Creature]. The Hunter had placeholder stats in the calculator: 600/800/1000/1200/1400 HP, XP 140/200/300/450/700 [PF, commented out]. Low.

Creature generation and AI (Release 159, 172, 184 [F:6851][F:9054][F:10745], high):
- Two predetermined variants per creature race per world (different health, damage, speed, element, abilities).
- Each world assigns creatures a random bundle of attacks; each power tier has its own behaviours and weapons; exotics have different attacks.
- Each attack has its own cooldown and unlock conditions (number of attackers, creature health, weapon used); some attacks have a tell animation; creatures can buff themselves.
- Elemental spawning is a fixed percentage chance.
- Creatures give up on targets they have not seen for a while ("prowl"); detection range in all directions, then a "threatening" period before attacking, during which running away avoids combat; projectile use near a creature alerts it.
- Threat: since Release 196 each creature keeps a hate list scored by average DPS, scaled up by the Threat Epic; healers join the hate list of creatures the healed player damaged; creatures attack the most hated player; the list survives portals; Mega Calming Brew clears you from every hate list within 16 m, Mega Aggravating Brew adds 10,000 hate within 16 m [F:14494]. Player testing in 2023 suggests in practice nearest player gets first aggro and the first to hit keeps it unless someone else lands more hits [F:64706] (low).
- Spawning: spawn ahead of travelling players, despawn far ones quickly; killed creatures do not respawn for a while (Release 159, 170) [F:6851][F:8695]. Testing 249.2 (never live) added biome rules: wildstock and spitters not in mountains, cuttletrunks not in forests [F:53908].
- Shadow Effect skill makes you harder to detect (multiplier since Release 184).

#### 3.2.4 Drops by creature and tier

[BI-drops], high. Format: item (weight, max per roll). Drops per kill = min to max rolls (average in brackets). "Elemental shard" = the shard of the world's element (only elemental creatures drop it [WIKI Creature]). Every tier also has an event-only seasonal drop. Drops are per player (instanced): everyone near gets their own set by hitting the corpse (Release 156) or with a loot-magnet tool [F:6519][F:54181]. Luck raises drop counts (Release 172, Release 197 doubled luck on blocks) [F:9054][F:14809].

| Creature | Tier | Drops per kill | Pool (weight / max) |
|---|---|---|---|
| Wildstock | 1 | 1-5 (2) | meat 10/2, bone 7/1, tallow 8/2, shard 10/3 |
| | 2 | 1-6 (2) | + hide 6/2, fresh blood 5/2, milk gland 4/1, eye 2/1 |
| | 3 | 2-7 (3) | meat 8/4, bone 6/2, shard 8/5, rest as T2 |
| | 4 | 2-8 (3) | meat 8/4, bone 6/2, shard 8/5, tallow, hide, blood 5/4, eye 3/2, horn trophy 1/1 |
| | 5 | 3-9 (3) | meat 7/6, bone 6/3, shard 7/7, trophy 1/1 |
| Spitter | 1 | 1-5 (2) | meat 23/2, tallow 19/2, bone 19/2, viscous blood 11/2, shard 23/2 |
| | 2 | 2-6 (2) | + hide 4/2 |
| | 3 | 2-7 (3) | meat 15/4, tallow 12/4, bone 12/4, blood 10/4, mantle 8/2, spark sac 5/1, eye 3/1, shard 15/5 |
| | 4 | 3-8 (3) | + scale trophy 2/1, spark sac 5/2, eye 4/2 |
| | 5 | 3-9 (3) | max per roll 6 on common parts, shard 17/7, spark sac 5/3 |
| Cuttletrunk | 1 | 1-5 (2) | prime meat 23/2, tallow 19/2, bone 19/1, shard 23/3 |
| | 2 | 2-6 (2) | + hide 11/2, viscous blood 4/2 |
| | 3 | 2-7 (3) | + mantle 5/1, floatation sac 4/1 |
| | 4 | 3-8 (3) | + tentacle trophy 2/1 |
| | 5 | 3-9 (3) | maxima 3 to 7 |
| Hopper | 1 | 1-5 (2) | prime meat 10/2, tallow 8/2, bone 7/1, shard 10/3, volatile blood 8/2 |
| | 2 | 2-6 (2) | + hide 4/1, mantle 1/1 |
| | 3 | 2-7 (3) | + thermal sac 3/1, eye 1/1 |
| | 4 | 3-8 (3) | + thermal core trophy 1/1 |
| | 5 | 3-9 (3) | maxima up to 6 |
| Roadrunner | 1 | 1-5 (2) | lean meat 11/3, tallow 7/1, bone 7/1, hide 5/1, adrenal gland 5/2, fresh blood 4/1, **rough oortstone 2/2**, shard 11/3 |
| | 2 | 2-7 (3) | + eye 7/2, feather trophy 1/1, oortstone 2/4 |
| | 3 | 3-9 (3) | oortstone 3/6, shard 15/7 |

Wiki drop counts for spitters (older): 1-5, 2-6, 2-7, 3-8, 3-9 per tier, same as [BI-drops] [WIKI Spitter]. High.

What the drops are for (economy hooks): volatile blood makes bomb charges; viscous blood makes strength and rage brews and the damage slingbow augment; adrenal glands make fast brews and speed augments; spark, thermal and floatation sacs make augments [MAY]; shards (elemental) are needed for every gem weapon and gem bomb (4 per craft) [MAY]; trophies and eyes feed forge ingredients [F:19578]. High.

#### 3.2.5 Meteorites and hunts

- Introduced Release 184 (Dec 2017) as "Conduit Meteorites" [F:10745]. They fall on tier 3+ worlds; some on a timer, others when a player enters an empty named region; not within 64 m of a beacon [F:10745][WIKI Meteorite]. Release 244 (May 2020) fixed region-triggered sizing to count the players around the one entering [F:46111]. High.
- On impact a few portals open in the air and drop waves of creatures that always hunt players (even wildstock). Kill all waves, then mine the meteorite (made of warp conduit blocks) to reach the core loot [WIKI Meteorite][F:10745]. High.
- Size scales from 1 to 100 players present at the start; larger means more and stronger creatures and better loot; solo meteors were made easier and their drops raised (Release 184, 197) [F:10745][F:14809]. 2000 wave variants (Release 196); meteors can spawn exotics [F:14494]. Medium-high.
- **Meteorite life** (Release 196, Aug 2018): once the first creature dies, every player who joins gets one life; returning to the sanctum loses it; revived players keep it; leaving the zone gives 10 s to return; if no living holders remain the meteor fails and pays only dormant loot; creatures periodically target life holders [F:14494]. High.
- An unfinished meteor becomes **dormant** after a while with reduced loot [WIKI Meteorite].
- Loot: rough oortstone (the portal-fuel economy), gems, fossils, ancient tech, metal ores; tier 5-6 add a rare prestige crop seed; tier 7 adds Lucent ore and meteorite ichor (portal fuel ingredient) [WIKI Meteorite][F:28046][F:54181]. Lower tier worlds yield more oortstone per hour because meteors clear faster [F:54181]. Release 184 removed oortstone from creatures except roadrunners [F:10745]. High.
- Group "hunts" are a community ritual: running hunts (grapple from region to region) and platform hunts (warp between pre-built platforms per region) [F:54181].
- Gleambow meteorites (event and natural on lush tier 3+): timed; timer per world and per character shrinks when completed, grows on failure, capped (Release 244) [F:46111]; players saw 20 s to 2 min timers [F:63394].

---

### 3.3 Mechanics and formulas

#### 3.3.1 Player outgoing damage

From [PF] `damageCalculator.js` (reverse-engineered from game data, high; it reproduces the in-game tooltips in [F:44307] exactly: gold 540+770+400 = 1710 x 1.95 = 3335; rift (2400+1020) x 1.95 = 6669):

```
base      = item damage + damage augment (slingbow +100/+200/+400; bomb +200/+400/+800)
forge     = base_item * (multishot_mult - 1) + base_item * (devastating_pct + heavy_proj_pct + daynight_pct) + devastating_flat + daynight_flat
skillMult = 1 + (power_pts * 0.05 + brew + strength_bomb) * (damage_epic ? 3 : 1) + mastery_pts * 0.05 + (weapon_epic ? 0.25 : 0)
damage    = (base + forge) * skillMult                      # per projectile
vs target = max(0, round(damage * mult) - target_armour) * projectiles_hitting   # mult = crit and/or element
```
- power_pts runs 0..9 (8 Power Bonus points plus 1 from 5/5 Attribute Bonus, which adds +10 to every attribute); each Power point is +5%, tripled by Damage Epic [PF]. Brew: Strength +10/20/30/40/50% (minor to mega); strength buff bomb +25% [PF][F:17271]. Both are inside the Damage Epic tripling (so a mega brew gives +150% with the epic).
- Weapon mastery: +5% per point, max 5 [PF][WIKI Slingbow Mastery]; slingbow and bomb epics +25%; fists use Grapple Mastery and have no weapon epic [PF].
- Multishot ("number of shots" boon): each projectile's damage is reduced by 50% (2 shots) or 60% (3 to 6 shots) [BI-forge], i.e. each does 50% or 40% of normal [PF]. High.
- Criticals: chance = item crit + boon + (agility_pts / 9) * 0.20 * (1 + criticaling food 0.2..1.0); capped at 100%. Crit multiplier = 1 + item crit effect + Hard Hitter boon + crit-effect augment (5/10/20%) [PF]. Character crit from skills is not shown on the weapon tooltip (2023 [F:64706]).
- Element: a correct element has a **30% chance to deal double damage** (the "-0.3" entries in 3.3.3); a resisted element has that % chance to deal nothing [F:30640][PF]. Elemental and critical can stack (x2 x crit) [F:30640]. Element boost applies before armour (Release 196) [F:14494].
- Armour is a flat subtraction per hit, so low per-hit weapons can do 0 to armoured cuttletrunks; fists bypass it [F:30640].
- Damage bleed (forge "damage over time") ignores armour [F:64706].

#### 3.3.2 Incoming damage, armour and resistance (players)

- Damage types since Release 184: **Kinetic** (single-target: spitter shots, wildstock, cuttletrunk bolts including homing), **Impulse** (area: bombs, beams, hopper blasts), **Falling** (impact) [F:10745][F:17271]. Elemental damage types are separate (status effects).
- Armour: flat reduction per hit. Resistance: chance to take zero from the hit [F:16743][F:25040]. Maxed armour + resistance skills + Protection Epic: 50% resistance chance [F:25040] (medium). Max-HP does not change damage taken [F:25040].
- Shielding foods (kinetic and impulse): +10/20/40/70/100 armour and +5/10/15/20/25% resistance, 30 min; elemental shielding foods give the same numbers against one element [F:17271] (2018, medium; the post itself warns values may have changed).
- Rage brew "thick skin" (needs Rage skills): stacks 3/4/5/7/8/9/10 give armour 60/100/150/310/420/550/700, resistance 20/25/30/40/45/50/60%, heal per tick 90/140/220/450/600/780/1000; Protection Epic triples the armour and doubles the resistance [F:64706] (low, single tester 2023).

#### 3.3.3 Elemental resistance table (creature world element vs damage element)

From Vansten's table (2019-09-04, image, [F:34744]); the [BI] gem-shield resistances repeat exactly the same rows for the six gem elements, so high. Positive = chance to resist completely; negative = chance of double damage.

| Creature \ attack | Physical | Amethyst (corr.) | Ruby (burn) | Diamond (blast) | Topaz (shock) | Sapphire (chill) | Emerald (toxin) | Umbris | Blink | Rift |
|---|---|---|---|---|---|---|---|---|---|---|
| Corrosion | 0.4 | 0.8 | 0.3 | 0 | **-0.3** | 0 | 0.6 | 0.6 | 0 | -0.3 |
| Burn | 0.4 | 0.3 | 0.8 | 0.6 | 0 | **-0.3** | 0 | 0 | 0.6 | -0.3 |
| Blast | 0.4 | 0 | 0.6 | 0.8 | 0.3 | 0 | **-0.3** | -0.3 | 0.6 | 0 |
| Shock | 0.4 | **-0.3** | 0 | 0.3 | 0.8 | 0.6 | 0 | -0.3 | 0 | 0.6 |
| Chill | 0.4 | 0 | **-0.3** | 0 | 0.6 | 0.8 | 0.3 | 0 | -0.3 | 0.6 |
| Toxin | 0.4 | 0.6 | 0 | **-0.3** | 0 | 0.3 | 0.8 | 0.6 | -0.3 | 0 |
| Umbris | 0.4 | 0.8 | 0.6 | -0.3 | -0.3 | 0.6 | 0.8 | 0.8 | -0.3 | -0.3 |
| Blink | 0.4 | 0.3 | 0.8 | 0.8 | 0.3 | -0.3 | -0.3 | -0.3 | 0.8 | -0.3 |
| Rift | 0.4 | -0.3 | -0.3 | 0.3 | 0.8 | 0.8 | 0.3 | -0.3 | -0.3 | 0.8 |

Structure: three opposed pairs (corrosion vs shock, burn vs chill, blast vs toxin): each element is weak to its opposite, strongly resists itself (0.8), resists its two neighbours 0.6/0.3. Every elemental creature resists physical damage 40% of the time. Lucent elements each channel two gem elements (Rift = shock + chill, Blink = blast + burn, Umbris = corrosion + toxin) and use whichever does more damage (Testing 218 [F:28046]); Lucent creatures are weak to the other Lucents. Normal (non-elemental) creatures have no resistances.

Elemental status effects applied by elemental creatures (2018 player descriptions [F:17271], low-medium): burn reduces max HP ("scar"), chill slows, toxin is damage over time, corrosion lowers defences, blast heavily slows action and movement, shock (not described). Cleansing brews cut debuff time by 500/1000/1500/2000/2500 s-equivalents [F:17271].

#### 3.3.4 Health

- Base 600 HP [WIKI Character]. Formula (Pfiffel, from game data, 2019-05-13 [F:29895], high):
  `maxHP = 600 + ((Vitality - 10) / 90)^1.5 * (4000 - 600)`, rounded to 10. Vitality starts at 10, +10 per Vitality Bonus point (max 8) and +10 from 5/5 Attribute Bonus, so 10..100.
  Table: vit 10 600, 20 730, 30 960, 40 1250, 50 1610, 60 2010, 70 2450, 80 2930, 90 3450, 100 4000.
  **Health Epic x3** (max 12,000); **Invigorating food +20/40/60/80/100%** (pie doubles: 24,000) [F:29895][F:17271]. Release 184 made vitality "start slower then accelerate" (this curve) [F:10745].
- Regeneration: no passive regen except **Full Up** (hunger below 10%): 4 HP every 5 s [F:17271]; Health Regeneration Epic heals about 45 per 5 s (player quote 2018 [F:26625], low). Healing brews heal over 60 s: 10/20/40/80/160 HP per s-tick (600 to 9600 total); instant healing brews 750/1500/3000/6000/12000 [F:17271] (2018, medium). Intelligence +10% healing per point, Healing Epic +100% (bombs [PF]). Mega healing brew reported as 1200 per 5 s in 2020 [F:52455] (conflict with 160/s).
- Starberries heal (+100 extra raw/cooked since Release 184); raw meat gives "Sick" (5 damage per 5 s for 40 s, blocks regen) [F:17271][F:10745].
- Healing bombs (forge boon "Hot on their heels" levels I-III): heal everyone in the blast, top level also restores energy and cleanses; detonate instantly on any player or creature; cheaper than brews [F:26188][F:54181]. Titanium heal bomb about 3000 HP (2020 [F:46099], low).
- Lava, drowning and fall damage take a percentage of max HP (lava about one third of max per tick) while healing is flat, so big HP pools suffer more [F:26625][F:17849]. Medium.

#### 3.3.5 Fall damage

- Based on impact velocity, not height (dev-confirmed via player [F:26625]); a percentage of max HP [F:26625] (medium).
- Release 197 (Aug 2018): made "more realistic" (no more surviving a plane jump at 15%) and **surface material matters** (sand softer than igneous rock) [F:14809]. Release 195: sliding at shallow angles on slippery blocks (slide blocks, ice) avoids impact damage; no impact damage in the first frames after a jump [F:14250]. High.
- Falling Armour skill and Floating foods (+50/75/100/150/200 falling armour, jump +0.5..2 m) [F:17271]; guild Building Safety helix reduces fall damage (Release 217) [F:26819]. Players found falling armour nearly useless because armour is flat and fall damage is %-based [F:26625]. No fall damage while grappled to the ground [F:54181].
- Never reworked after 2019 despite dev saying it was "on the TODO" (Dec 2018 [F:23224], Feb 2019 rossstephens in [F:26625]). Player feedback: the single most common killer. Design hint for Boundmore.

#### 3.3.6 Energy, hunger and stamina

- Energy (stamina) base 600 [WIKI Character]; every weapon costs energy per use (table above); actions blocked when energy is below cost; energy recovers quickly after a short pause; max energy is cut by hunger; Zeal raises max energy and buff duration (Release 188 [F:12306][WIKI Attribute]).
- Running out ("fatigued") dropped attributes to base and stopped sprinting (Release 162 [F:7337]); starvation status deals damage (Release 195 [F:14250]).
- Energy recovery: recovery brews 30/60/120/360/900 energy per s (Release 223) [F:29774]; starberry foods 10/20/40/120/300 per s recharge buff [F:29774]; Focus brew restores energy.

#### 3.3.7 Death, defeat and revive

- Final rules (Release 195, Jul 2018 [F:14250], high): items are **not** damaged on defeat; defeat adds an **XP debt of 10% of the XP needed for the next level**; while in debt you earn **50% XP**; debt clears as you earn; a hunger penalty applies; the default return from the sanctum opens a warp a short random distance from where you fell (away from the meteor). Reviving skips the penalty.
- Earlier rules for history: Release 159 smart-stack items wore on death; Release 188 "Death Sickness" (-25% attributes for 90 s) [F:6851][F:12306].
- Revive: totem with a Revive Augment (needs the Revive skill; crafted 3 per batch from 3 alloy + 1 vital orb, compactor, Release 203 removed the skill gate on crafting) revives others; reviving gives the reviver a 5-minute cooldown debuff removable by heal bombs [F:12306][F:20150][F:54181]. Reviver brew: self-revive once within 10 min (longer with Zeal) [F:17271][F:54181]. Testing 249.3 added a brew to remove the defeat penalty (never live) [F:53908].
- Revived players keep their meteorite life [F:14494].

#### 3.3.8 Buffs and consumables (combat relevant)

One food buff, one brew buff and the reviver buff at a time; a new one replaces the old [F:12306][F:54181]. Food buffs last 30 min (well-fed 10-60 min) [F:17271]. Values (2018-09 index [F:17271], medium; check before use):

| Consumable (5 ranks) | Effect per rank |
|---|---|
| Strength brew | +10/20/30/40/50% damage, 2 min |
| Fast brew | +0.5/0.75/1/1.5/2 m/s run, +20..100% action speed, 2 min |
| Invigorating food | +20..100% max HP, 30 min |
| Energising food | +20..100% max energy |
| Criticaling food | +20..100% (multiplies agility crit, [PF]) |
| Persisting food | -5/10/15/30/40% wear |
| Shielding food | +10/20/40/70/100 armour, +5..25% resist |
| Innocuous brew | +20/40/70% stealth (3 ranks) |
| Luminous brew | light radius 6/9/12 m |
| Buff bombs (forged, Release 223) | armour, speed or strength (+25% damage) to players hit [F:29774][PF] |

Recipes [MAY], high: brews are mixer recipes, 4 per batch, 60 s (minor) to 600 s (mega), spark 0 to 800, power 0 to 2000, from brew containers + water or milk bucket + an oil + a creature part (viscous blood for strength, adrenal gland for fast, fresh blood for cleansing) or bark (healing). Shielding foods are furnace recipes from prime meat plus element amalgams.

---

### 3.4 Skills and forge boons

#### 3.4.1 Skills (combat relevant), live system Release 195 to 248

Basic skills cost 1 point per rank; epics cost 5 and some exclude each other; 2 skill points per level (4 at level 2), 100 points per skill page; skill pages cost cubits [F:14250][WIKI Skills]. Attributes up to 8 points, masteries up to 5 [WIKI Skills].

| Skill | Effect per rank | Source, confidence |
|---|---|---|
| Vitality Bonus (8) | +10 vitality per point (curve in 3.3.4) | [F:29895] high |
| Power Bonus (8) | +5% damage per point (x3 with Damage Epic) | [PF] high |
| Agility Bonus (8) | movement speed; +20%/9 crit chance per point (20% at 9 effective) | [PF] high |
| Intelligence Bonus (8) | +10% healing per point | [PF] medium |
| Zeal Bonus (8) | max energy, buff duration up, debuff duration down | [WIKI Attribute] medium |
| Attributes Bonus (5) | +2 to every attribute per point (+10 at 5/5, worth one point of each) | [WIKI][F:29895] high |
| Slingbow Mastery (5) | +5% slingbow damage, +2 action speed per point | [WIKI][PF] high |
| Bomb Mastery (5) | +5% bomb damage per point | [PF] high |
| Grapple Mastery (5) | also +5% fist damage per point | [PF] high |
| Slingbow Epic / Bomb Epic | +25% damage | [PF] high |
| Damage Epic (epic 3) | triples the Power and brew bonus | [PF] high |
| Health Epic | x3 max HP | [F:29895] high |
| Rapid Fire Epic / Charging Epic | hold-to-autofire / hold-to-charge on all slingbows (exclusive pair) | [F:14250] high |
| Kinetic / Impulse Armour and Resistance (5 each), Falling Armour | flat armour; resist chance (5 + Protection Epic = 50%) | [F:25040] medium; per-point values unknown |
| Protection Epic | boosts armour and resistance (triples rage armour, doubles rage resist) | [F:25040][F:64706] low |
| Healing Epic | +100% healing | [PF] medium |
| Health / Stamina Regeneration Epic | regen over time (about 45 HP per 5 s) | [F:26625] low |
| Threat Epic | multiplies hate generated | [F:14494] high |
| Shadow Effect / Stealth Epic | detection range multiplier | [F:10745] medium |
| Rage (and upgrades: Power Surge AoE, Adrenaline Rush HoT, Thick Skin armour) | up to 10 stacks from damage taken, spent by a Rage brew | [F:12306] high |
| Focus (upgrades: Precise Strikes damage, Quickening speed, Petrify slow) | up to 10 stacks from damage dealt and healing, spent by a Focus brew; 10 stacks about +300% damage | [F:12306][F:64706] medium |
| Revive Epics | stronger revive, shorter cooldown | [F:12306] medium |
| Death Penalty Epic | reduces defeat penalty | [WIKI Skills] low |

Testing 249 (never live) removed most epic exclusions except DPS/tank/healer links, raised the cap to 359 then 367 points, gave 2 points per level to 180, and made every page get the full points [F:53886][F:53908].

#### 3.4.2 Forge (Centraforge) traits for weapons

Trait list and point costs [PF-forge] (high). Boons have up to 10 levels costing 100, 150, 300, 600, 1000, 1500, 2000, 3000, 4000, 5000 points (matches [F:13556]); "special" boons have 3 or 5 levels (100/300/1000/... or 300/1000/2000); defects are worth -0.5 and quirks -0.3 of a boon's points. A gear item holds up to 3 boons [F:26188].

| Boon (Boundless name, for identification) | Gear | Levels | Value per level |
|---|---|---|---|
| Devastating damage | slingbow, fist, bomb, tool | 10 | +50 flat per level, plus +10/10/20/20/30/30/40/40/50/50% (lvl 1-10): lvl 10 = +500 and +50% [PF]; confirmed by the gold-bow tooltip (+770 on 540) [F:44307]. High |
| Critically lucky | slingbow, fist, tool | 10 | +5% crit chance per level (50% at 10) [PF] high |
| Hard hitter | slingbow, fist, tool | 10 | +10% crit effect per level (100% at 10) [PF] high |
| Heavyweight projectile | slingbow | 5 | +10% damage per level, -10 m/s projectile speed, +5 g weight per level [PF] medium |
| Number of shots (multishot) | slingbow | 5 | 2 to 6 projectiles; each does 50% (2) or 40% (3-6) [PF] medium |
| Range ("long range") | slingbow, fist | 10 | +3 m per level (30 m at 10) [BI-forge] high |
| Projectile speed ("fly fast") | slingbow, grapple | 10 | +5 m/s per level [BI-forge] high |
| Light projectile | slingbow | 5 | +10/12/15/20/30 m/s, flatter arc [BI-forge] high |
| Durability ("heavy duty") | slingbow, fist, tools | 10 | +200 per level (+2000 at 10) [BI-forge] high |
| Action speed ("busy bee") | slingbow, fist, bomb, tools | 10 | +5 action speed per level [BI-forge] high |
| Energy saver | all gear | 10 | -2 energy per use per level [BI-forge] high |
| Bomb AoE ("bigger boom") | bombs | 10 | +0.3 m radius per level (+3 m at 10) [BI-forge] high |
| Bomb knockback | bombs | 10 | +2 per level [BI-forge] high |
| Magnetic collector | tools, fist | 3 | 5 / 7 / 9 m pickup [BI-forge] high |
| Damage over time (bleed) | slingbow | 5 | about 100/200/300/400/500 per tick over 30 s, ignores armour [F:64706] low |
| Apply weak | slingbow | 5 | target deals 10..50% less of its base damage, 60 s [F:64706] low |
| Apply slow | slingbow | 5 | slows target, 20 s [F:64706] low |
| Hard as nails (armour) | slingbow, fist | 3 | +20 / 50 / 100 kinetic and impulse armour while held [BI-forge]; item loses durability when you are hit (Release 223) [F:29774] |
| Bomb AoE, knockback | bombs | 10 | values not found |
| Healing bomb, regenerate world | bombs | 3 / 1 | heal values not in the published text |
| Buff bombs | bombs | 1 | strength +25% damage; armour +30 kinetic and impulse armour and +5% resistance each; speed +40% action speed, +0.75 m/s run and sprint [BI-forge] high |
| Defects (mirror of boons) | | 10 | same steps negative (e.g. -200 durability per level) [BI-forge] |
| Fist attraction | fist | 1 | pulls drops to you (Release 223) |
| Glow, drop attraction | many | 3 | light, loot magnet |
| Quirks: day-only / night-only damage | all | 1 | +100 and +10% in the favoured period, -100 and -10% in the other (Release 209) [F:22594][PF] |
| Testing 249 only: Cleaver (more melee targets), Slugger (melee radius), Fortress (shield cone), Damage reflect | melee/shield | | [F:53886] |

Flexibility caps forging: 120 (stone) down to 10 (Lucent) [BI]; copper and gold raised to 60 and 40 in Release 209 for tools [F:22594] (the [BI] weapon values are higher, so the scale differs between versions; low).

#### 3.4.3 Weapon crafting

[MAY], high. All slingbows: 3 bars/alloy/refined gem + 4 sticks + cloth or leather + glue/bonding agent. Stone slingbow at the crafting table (10 s); copper/iron at the workbench 60/120 s, no spark; silver 240 s, gold 480 s (800 spark); titanium 1200 s (1500 spark); gem 1800 s (2000 spark, needs a machine with 1500 power, plus 4 elemental shards of the bow's element); Lucent 3600 s (4000 spark, 3000 power, hard leather, super-enriched bonding agent, elemental fusion). Fists: iron = slingbow recipe with iron (120 s), gold 480 s, 800 spark. Bombs: same tiers, 1 to 6 bomb charges. Augments (compactor): 3 tiers, 60/120/240 s, 100/200/400 spark, power 0/500/1500, ingredients rock + a creature part (viscous blood = damage, adrenal = speed, thermal sac = crit, floatation sac = range, spark sac = projectile speed) + a fragment (sulphur/olivine/cobalt). Weapon Crafting skill level gates the higher tiers [WIKI Explorer's Fist].

---

### 3.5 Exoworlds (combat only)

Temporary worlds since Release 218/219 (Mar-Apr 2019): no beacons, no regen, atmosphere hazard rises in the last 30 minutes; the special tier 7 ones (Blink, Umbris, Rift) carry Lucent creatures (x3 HP, x2 damage and armour, 3.2.2) and drop Lucent materials and meteorite ichor; atmospheres: Rift potent, Blink volatile, Umbris caustic [F:28046]. Players recommend max damage brew, damage augments and a maxed forged bow for tier 7 hunts [F:34744].

---

### 3.6 Conflicts between sources

1. **Spitter XP**: wiki 120/200/300/450/700 [WIKI Spitter] vs [PF] 140/240/360/540/840. The wiki numbers match [PF]'s commented-out Hunter row, so the wiki is probably older or mislabelled. Use [PF].
2. **Multishot per-projectile damage**: a 2021 player says each projectile "does 60%" [F:54135]; the boon text says damage is *reduced by* 60% for 3-6 shots [BI-forge] and [PF] uses 40%. Resolved: 40% per projectile (50% for 2 shots).
3. **Healing brew rate**: 160 HP per s for mega (2018 index [F:17271]) vs "1200 every 5 s" (2020 player [F:52455]). The 2018 index warns it may be stale.
4. **Fall damage**: % of max HP (players 2019) vs dev intent "realistic" with material softness (Release 197). Both can be true; exact formula unknown.
5. **Melee weapons**: present in [BI] data and Testing 249 notes, but never on Live. Do not treat as Boundless-as-played.
6. **Action speed numbers**: wiki slingbow table and [BI] agree; tooltip shows skill +24 at 5/5 mastery + epic, while the wiki says +2 per mastery point (+10), so the epic or Dexterity adds the rest (unresolved).
7. **Devastating damage on the Rift tooltip** (+1020 on 2400) equals level 6 by the [PF] table (300 + 30%), not level 10 as the poster implied; consistent with [PF].
8. Pre-Release-159 numbers are 10x larger (health and damage rescaled, Mar 2017).

### 3.7 Open questions and gaps

- **Action speed to time**: seconds per shot/swing for a given action speed value (biggest feel gap for weapons). Measure in a Boundless video or ask Nathan.
- **Creature damage per attack and per tier** beyond the 2023 player sample; creature move speeds, detection ranges, attack cooldowns (only the cuttletrunk throw 30 s and the wildstock charge numbers are known).
- **Per-point values** of kinetic/impulse armour and resistance skills, Falling Armour, Regeneration epics, Zeal.
- **Fall damage formula** (velocity threshold, % per m/s, material multipliers).
- **Exotic damage and speed buff sizes** (only health +200% and armour +100% are known) and the exact exotic chance per world tier.
- **Elemental creature spawn chance** (fixed percentage, value not found) and spawn rates/counts per chunk, day/night effects (none found; Boundless spawning did not seem time-of-day based).
- **Meteor timers** (how often timer meteors fall per world, region cooldowns), wave counts per meteor size, and core loot tables. The boundlessinfo bundle may contain meteorite drop tables (only creature tables were parsed here).
- Bleed, slow and weakening effect strengths per level (only the player estimates in 3.4.2), and healing-bomb heal amounts (the community "Forge Trait List" Google sheet `1AaTC0QzceyVGgAIFiqqTgYhdWK-iXRzREnj3mkksi98` and Forge Materials sheet `1nx8aENb98HmC3Ku4F0hvGIk8HuZm6Azs4k-mRajM6W8` are not reachable from the sandbox; boundlessskill.com is also unreachable).
- Unit of bomb "Area of Effect" (2 to 5): the AoE boon adds "0.3 m" per level [BI-forge], so the base value is metres (radius assumed).
- Skill per-rank values (armour/resist points, regen epics, Zeal, Protection Epic multipliers): the game's own tooltip strings are templates filled from progression data that none of the fan sites publish (checked mayumi `raw/english.json` and the decoded `compileditems.msgpack`, which holds item records but only references to attribute bundles). Needs in-game observation or a video.
- Decoded mayumi `compileditems.msgpack` gives small extras: item equip time 0.2 s for weapons; bomb projectile fuse 3 s (vs 3.85 s "charge time" on the bag [BI]); creature attacks are modelled as items (wildstock charge, hopper explosion, spitter base/lobbed/sticky/shotgun/cluster projectiles, cuttletrunk base/homing slow/homing rapid/lobbed timed/sticky/cluster projectiles and a damage beam), but their damage numbers live in attribute bundles that are not published.
