# Boundless tuning reference

The numbers Boundmore tunes to (CLAUDE.md: Boundless first, identity second; Nathan, 2026-10-19). One section per area, compiled from the community's published game data (boundlessinfo.com, boundless.mayumi.fi, pfiffel.com/bl, including the data files those sites host), the forum's developer posts and patch notes, and the fandom wiki. Every number carries its source, version and a confidence.

Boundless's item names appear only to say which number belongs to which item; Boundmore names its own items. How a number is used in Boundmore, and what is still a guess, is in `docs/feel.md`.

| Section | Area | Status |
|---|---|---|
| 1 | Grapples and player movement | compiled 2026-10-20 |
| 2 | Tools and mining | in progress |
| 3 | Weapons, combat, creatures | in progress |
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
