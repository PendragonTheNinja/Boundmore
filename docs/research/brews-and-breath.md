# Brews, breath and swimming (research, 2026-10-27)

What Boundless's brews did, tier by tier, how attributes scale them, how they stack, and how breath and swimming worked under water. Facts and numbers only; Boundless's item and status names below are lookup keys, never names for Boundmore. Inferences are marked **inference**; anything not found is listed at the end.

## Sources (fetched 2026-10-27, decoded in the scratchpad, not committed)

| Key | Source | What it holds |
|---|---|---|
| **[BI-R]** | `boundlessinfo.com/assets/data/compiled-recipes.json` + `item-names.json` | every brew recipe: machine, inputs, time, spark, power, XP, skill gate (870 recipes, latest patch) |
| **[MAY-I]** | `boundless.mayumi.fi/recipes/raw/compileditems.msgpack` | item table: each brew carries two modifier bundles (`Item Brew Initial` + `Item <Tier> <Kind> Brew Initial`); names only, **no values**. No Kraken or Halloween brews (older export) |
| **[BLX]** | `api.boundlexx.app/api/v1/skills/` | in-game skill text with resolved numbers |
| **[WIKI]** | fandom wiki item pages (`Minor_Fast_Brew` ... `Mega_Smoothie_Healing_Brew`, `Healing_Brew`, `Kraken_Brew`, `Reviver_Brew`, `Brew_Extension_Helix`) | infobox `description` = the game's own item text as of game version 244 (2020), with numbers; tables written by editors |
| **[R188]** | forum t/12306 (Release 188, james, 2018-03-12; same text as Testing 187, t/11861) | brews, buffs, Rage, Focus introduced; one food and one brew buff rule |
| **[R199]** | t/17621 (Release 199, Steggs101, 2018-09-26) | brew buffs 2 min to 10 min; healing brews made to out-heal instant ones |
| **[R209]** | t/22871 (Release 209, james, 2018-12-05; = t/22336, rossstephens) | That Sinking Feeling quirk: +50% breath |
| **[R223]** | t/29774 (Testing 222, james, 2019-05-10; shipped as Release 223, t/30194) | recovery brew rates raised |
| **[R248]** | t/52466 (Release 248, james, 2020-10-29) | Halloween brews: Kraken (double air), Zombie Lungs (+2 atmosphere protection), four weather brews |
| **[T249]** | t/53908 (Testing 249 continued, james, 2020-12-29) | Second Chance brew added (249.3) |
| **[IDX]** | t/17271 (willcrutchley, 2018-09-22, before R199) | player index of every brew with tables; durations there are the old 2 min |
| **[F:n]** | other forum topics, by id | players unless a dev is named |

## A. Every brew, every tier

### A.1 Durations

| Finding | Source | Conf. |
|---|---|---|
| Timed brew buffs (fast, strength, innocuous, luminous, reviver) last **10 min** base since Release 199 (were 2 min). | [R199]; player check "improved strength brew was 10 min" [F:17942] | H |
| Over-time brews (healing, recovery, healing part of smoothies) run **60 s**; they were not lengthened by R199. | [WIKI] item text ("over 60 s"); [F:18118] "Mega Recovery Brew still lasts 2 minutes" right after R199, i.e. its buff icon; tooltip bug about their duration field [F:20062, vdragon] | M (60 s text is H; the icon lasting 2 min in 2018 is a player report) |
| Halloween brews (weather brews, Kraken) last **20 min** base. | [WIKI] Mummy/Nessie/Yeti/Gravekeeper text "for 20 minutes"; Kraken 20 min to 30 with max Zeal [F:52526] | H for weather, M for Kraken |
| Rage brew shield showed 1 min 30 s (about 1:36 measured) with all Rage skills, 2:15 with max Zeal; Adrenaline Rush heal lasted 20 s, 30 s with Zeal (2018). | [F:20935] player stopwatch + vdragon confirming a display bug | L-M |
| Calming, aggravating, cleansing and instant brews are instant (no duration). | [IDX]; [WIKI] text | H |

### A.2 Healing

| Brew | Effect (base, before Intelligence) | Source | Conf. |
|---|---|---|---|
| Instant healing minor / improved / greater / super / mega | **750 / 1500 / 3000 / 6000 / 12000** HP at once | [WIKI] item text (Minor "Restores 750", Improved "1500", Super "6000", Mega "12000"; Greater is a bundle reference) + [IDX] table | H |
| Healing (over time), current game | heals every 5 s for 60 s (12 ticks). Item text: minor **600 total**, improved **150 per 5 s** (1800 total), super "4800 over 60 s", mega "9600 over 60 s"; greater is a bundle reference | [WIKI] item text | M: the text disagrees with itself (see next line) |
| Healing (over time), resolved | R199: "healing brews now heal more than instant healing brews of the same level". 150 per 5 s for improved (two players, [F:18296] "improved heal 150 every time they heal") and 1200 per 5 s for mega ([F:52455], 2020) fit a doubling ladder **75 / 150 / 300 / 600 / 1200 HP per 5 s**, totals **900 / 1800 / 3600 / 7200 / 14400**, each 1.2 times the instant brew of its tier. The wiki's 600 / 4800 / 9600 and its 10..160 HP/s table are the pre-R199 values (= [IDX], 2018-09). | **inference** from [R199] + two player reports; wiki table is stale | M |
| Smoothie (instant + over time), minor / improved / greater / super / mega | instant **375 / 750 / 1500 / 3000 / 6000** (half the instant brew), then over 60 s: minor "480" total, improved "15" (per s, read as 15 HP/s = 900), greater **150 / 5 s**, super **300 / 5 s**, mega **600 / 5 s** (1800 / 3600 / 7200 total) | [WIKI] item text (game v244) | H for instant part and greater to mega; M for minor and improved (odd units in the text) |
| Smoothies are made from raw earthyam and raw starberry (10/15/20/30/40 of each per batch of 4), restorative oil, a container; no bucket. | [BI-R] | H |
| All healing (brews, bombs) is multiplied by **(90 + Intelligence) / 100**; Intelligence starts at 10 and each skill level adds 10 points = **+10% healing per level** (8 levels). Healing Epic adds **+100%** healing modifier. | [WIKI] Healing Brew page; [BLX] "Intelligence ... increases Healing Potency by 10% per level"; [BLX] Healing Epic | H |
| A 2018 player found Intelligence did not change healing ticks (75 a tick); a dev logged it. Treat as a fixed bug. | [F:20935] | L |

### A.3 Energy

| Brew | Effect | Source | Conf. |
|---|---|---|---|
| Instant recovery minor / improved / greater / super / mega | **600 / 1200 / 2400 / 4800 / 9600** energy at once | [WIKI] item text (all five) + [IDX] | H |
| Recovery (over time), current | **30 / 60 / 120 / 360 / 900 energy per s for 60 s** (totals 1800 / 3600 / 7200 / 21600 / 54000) | [R223] dev notes ("Minor Recovery Brew now restores 30 Energy per second, up from 20" ... "Mega ... 900, up from 600") | H |
| The wiki's odd numbers resolved: minor "Recovers Energy by **20** over 60 s" is the pre-R223 rate **20 per s** (the text drops "/s"); improved "2400 over 60 s" is the old 40/s total; greater "120 points/s" and mega "**54000** over 60 s" (= 900 × 60) are post-R223. The wiki mixes item texts from before and after R223. | [WIKI] vs [R223] vs [IDX] (20/40/80/240/600 per s in 2018) | H |

### A.4 Speed, strength, stealth, light, threat, cleansing

| Brew | Effect | Source | Conf. |
|---|---|---|---|
| Fast minor / improved / greater / super / mega | run speed **+0.5 / +0.75 / +1 / +1.5 / +2 m/s** and action speed **+20 / 40 / 60 / 80 / 100%**; [IDX] says sprint gets the same m/s | [WIKI] item text, every tier | H (sprint part M) |
| Fast brew action-speed bonus seemed to cap near +60% in 2018 (player). | [IDX] | L |
| Strength minor / improved / greater / super / mega | damage per hit **+10 / 20 / 30 / 40 / 50%**; it adds to the Power term of the damage formula and is tripled by the Damage Epic (see `boundless-tuning.md` 2.x) | [WIKI] item text every tier; tuning doc | H |
| Innocuous minor / improved / greater / super / mega | creature attack range reduced **20% / "1" / 40% / 70% / 70%** (improved's text prints "1", super and mega both print 70%); [IDX] 2018 had only minor 20, greater 40, mega 70 | [WIKI] item text | M: improved is probably 30% and super maybe 55% (**inference**, unfilled text) |
| Innocuous + stealth skills once made wildstock turn aggressive (fixed in 249). | [T249] | H |
| Luminous minor / greater / mega | light radius **6 / 9 / 12 m**, intensity **1 / 1.8 / 2.4**; improved and super added later (no numbers found; **inference** 7.5 m and 10.5 m) | [IDX]; a player sign "adds 10m of light for 10 minutes (base)" [F:19908] | M |
| Calming (mega only) | removes **10000 threat** at once | [IDX] | M |
| Aggravating (mega only) | adds **10000 threat** at once | [IDX] | M |
| Cleansing minor / improved / greater / super / mega | removes **500 / 1000 / 1500 / 2000 / 2500 cleansing points** of debuff; [IDX] read a point as one second of debuff duration; "instantly remove or significantly reduce the duration"; how much is taken depends on the debuff's level | [WIKI] item text (minor to super; mega text missing) + [IDX]; [R188] | H for points, M for points = seconds |

### A.5 Defeat, Rage, Focus

| Brew | Effect | Source | Conf. |
|---|---|---|---|
| Reviver | drink before danger; if defeated within the buff (10 min) you revive on the spot, **no defeat penalty**, meteor life kept, but with a health and energy deficit; cannot be drunk while defeated; coexists with other brews | [WIKI] Reviver page; [F:37076]; [IDX] ("revive level 1 on yourself") | H |
| Second Chance (249.3, one per batch, Brew Crafting 5, power 3000, from meteorite ichor, purified milk, bitter beans, restorative oil, container 5) | "allows **direct removal of a Defeat Penalty**", i.e. drunk after defeat to clear the debt (unlike the reviver, which must be drunk before) | [T249] dev note; [BI-R] recipe | H for what, M for "after" (**inference** from the wording) |
| Rage (Rage Epic) | Rage stacks build **each time you are hit by a creature**, **up to 10 stacks**, based on damage taken; drinking a Rage brew spends them. Rage Epic: Thick Skin (armour and resistance) scaled by stacks; Rage Focus Epic One: +50% rage gain and Adrenaline Rush (heal over time); Epic Two: +50% gain and Power Surge (area attack on nearby creatures). Stacks cannot build while a rage effect is active. | [BLX] Rage Epic, Rage Focus Epic One/Two; [R188] | H |
| Rage numbers by stacks (3/4/5/7/8/9/10): armour 60/100/150/310/420/550/700, resistance 20/25/30/40/45/50/60%, heal per tick 90/140/220/450/600/780/1000 | `boundless-tuning.md` 3.x from [F:64706] | L-M |
| Focus (Focus Epic) | Focus stacks build **each time you hit a creature** (2018: also from health recovered), up to 10; a Focus brew spends them for Precise Strikes (damage up). Epic One: +50% gain and Quickening (action speed); Epic Two: +50% gain and Petrify (slows nearby creatures). 2018 base Focus also restored some energy. About +300% damage at 10 stacks (player). | [BLX] Focus Epic etc.; [R188]; [F:64706] | H (rules), L (numbers) |
| Rage and Focus epics are either/or (one or the other). | [BLX] text "If you have Rage Epic ... If you have Focus Epic" + tuning doc | M |
| Rage and Focus brews: 4 per batch, Brew Crafting 2, 600 s, power 500; R199 "greatly reduced" their cost. | [BI-R]; [R199] | H |

### A.6 Other brews

| Brew | Effect | Source | Conf. |
|---|---|---|---|
| Kraken (Halloween, 1 per batch, no skill) | "**Halves your Air depletion rate while underwater**" (item text); release notes: "double the air" | [WIKI] item text v249; [R248] | H |
| Zombie Lungs (Halloween) | **+2 atmosphere protection to all atmospheres**; not in the recipe or item exports | [R248] | M |
| Mummy / Yeti / Nessie / Gravekeeper | weather around you set to clear / snow / rain / fog for 20 min | [WIKI]; [R248] | H |
| Sticky Brew, Mystery Brew | in the item list but never craftable (players asked for sticky in 2019) | [BI-R] (no recipe); [F:37708] | H (not obtainable) |
| Brew Extension Helix | a **guild buff machine** (prestige 26, workbench, Machine Crafting 3, 5 per batch): "gives a bonus to any active Brew Buffs for Guild or Faction Members" while active (inside a guild-aligned beacon). Levels exist; a 2020 player saw Super and Mega with the same stats. Its exact bonus (longer brews, by the name) was not found. | [WIKI] item text; R217 notes t/26819; [F:49124] | H (what), L (amount) |

### A.7 Containers, liquids, oils (verified)

| Finding | Source | Conf. |
|---|---|---|
| Brew Container 1..5: workbench, 4 per batch, 30/60/90/120/240 s, spark 0/100/200/400/800, power 0/0/500/1000/2000, from 2 clay soil + ancient vital essence 1/1/2/4/10 + base metal 1 / base metal 2 / base alloy 1 / base alloy 2 / titanium alloy 1; Brew Crafting 1..5. | [BI-R] | H |
| Brews: mixer, 4 per batch, tier time 60/120/180/300/600 s, spark 0/100/200/400/800, power 0/0/500/1000/2000, Brew Crafting 1..5; liquid by tier: water or milk (minor, improved), purified (greater, super), fortified (mega; super fast and super strength already fortified). Per-kind extra: bark (healing), fleshy leaf (recovery), pulsating orb (instant), adrenal gland + syrup (fast), viscous blood + syrup (strength), fresh blood (cleansing), petal (innocuous, calming, aggravating), glowing lamella (luminous). | [BI-R] | H |
| Exceptions: improved/greater innocuous and luminous run 60 s / 180 s at lower XP; super innocuous and super luminous need fortified milk/water at 600 s and power 2000; reviver uses container 3 but needs Brew Crafting 4 and power 2000. | [BI-R] | H |

## B. Scaling, stacking, cooldowns

| Finding | Source | Conf. |
|---|---|---|
| **Zeal** (Energy attribute): each level adds 10 Zeal points and **+6% (de)buff duration** ("making Buffs last longer and Debuffs last for less time"), 8 levels. | [BLX] Energy skill text | H |
| Max Zeal (8/8 plus the 5/5 all-attributes skill, "Zeal 100") makes food and brew buffs last about **+50%** (mega brew 10 to 15 min, Kraken 20 to 30 min, pie 30 to 45 min). One player read +44%; one said "tripled" (wrong). **Inference:** duration × (1 + 0.006 × (Zeal − 10)), giving +48% at 8/8 and about +50% with the all-attributes bonus. | [F:58557] (almund, KArios), [F:52526], [F:53908 #974] | M |
| Zeal lengthened the Rage shield and Adrenaline Rush too (2018). | [F:20935] | L-M |
| **Intelligence** scales healing amounts only (brews and bombs), not durations: (90 + Int) / 100. | [WIKI]; [BLX] | H |
| Brew Crafting skill (3 levels) plus the Consumable Crafting Epic and the Brew Crafting Epic (+1 level each, epic also −20% brew spark) gate the 5 tiers; Efficient Crafting Epic −30% spark. Crafting skill does not change a brew's strength. | [BLX] | H |
| **One brew buff at a time**: "Only one food buff and one brew buff can be active at one time. Consuming a second brew will remove any existing brew buff." No cooldown on drinking. | [R188] dev note | H |
| Exceptions (players, consistent across 2019 to 2023): **instant** brews (instant healing, instant recovery, cleansing, calming, aggravating) do not touch the brew slot; **reviver** has its own slot; **Rage and Focus** brews stack separately. **Over-time** healing and recovery brews (and smoothies, **inference**) do take the brew slot and will replace a speed or strength brew. | [F:64235] (Nightstar, georgegroeg, Hazel1558), [F:45515], [F:40908], [F:35138] | M-H |
| Bomb buffs and forge buffs stack with food and brew buffs; guild buffs stack too. | [F:40908], [F:35138], [F:52316] | M |

## C. Breath under water and swimming

| Finding | Source | Conf. |
|---|---|---|
| Air is its own bar, separate from energy since 2017; under water it drains, at zero a **Drowning** debuff ticks damage until you surface. | Testing 162 notes t/7337 and dev update t/8826 (james, 2017); status list t/53218 | H |
| Drowning damage is a **share of max health** per tick, like fall and lava (lava about one third of max per tick). Exact drowning share and tick rate not found. | [F:17849], [F:26625] players | M (share), not found (number) |
| The same air bar serves hazardous atmospheres: unprotected on an atmosphere world the air bar drains and a drowning countdown shows. In R197 drowning under water while protected dropped atmosphere protection (bug, fixed). | `boundless-tuning.md` 4.13.1; [F:14805] | M |
| Air refills quickly once the head is above water ("the breath gauge started to rapidly fill"). | [F:44095] | M (no number) |
| That Sinking Feeling quirk (forged gear, held): you **sink in water**, and since R209 you **hold your breath 50% longer**; the bar also takes longer to refill, which players read as the larger bar filling at the same rate (unequip it to breathe faster). | [R209] dev note; [F:37488], [F:44095] | H (+50%), M (refill) |
| **Kraken + That Sinking Feeling = air never drops** for the brew's duration (30 min with max Zeal); a dev (rossstephens) confirmed it is intended. **Inference:** both are −50% to the drain rate and they add, 1 − 0.5 − 0.5 = 0. | [F:52526], [F:54092] | H (behaviour), inference (formula) |
| No skill, epic or food raises breath; players asked for one for years (a swimming epic, breath epic, gills, scuba, water-breathing food). | [F:16718], [F:17318] (Nathan's own 2018 thread), [F:27099], [F:45335] | H |
| Air pockets work: breaking the bottom layer or clearing water with a liquid breaker gives air; a client-side prediction quirk once made players drown with a full bar while clearing water. | [F:43115], [F:44095], [F:32293] (vdragon logged it) | M |
| Controls: Space swims up, **C (creep) swims down**; with neither you **sink slowly**; you must hold Space to stay at the surface (no floating). Creep can be set to a toggle. | wiki Guide:Controls(PC); [F:42447], [F:19936], [F:26479] | H |
| Swim speed is slow, slower than walking; diving with C is "very slow"; Agility (run speed) raises swim speed: a max-agility character "moves as fast in water as my other characters move on land" (a13o). No m/s value found. | [F:27590] a13o; [F:16718], [F:16875], [F:42447] | M (Agility applies; corrects tuning doc line "Agility does not affect swim"), not found (m/s) |
| Lava swimming since R195, at about **half** water swim speed. | R195 notes t/14250 | H |
| Swimming costs no energy in any source read (no mention in notes or complaints). | absence across the threads above | L |
| Devs wanted faster swimming under water "at some point" (dave, relayed by Jiivita, 2020); never shipped. Release 182 tweaked swim physics for responsiveness. | [F:45335]; t/10254 | M |
| Under water a grapple flies slower; underwater grapples attach since R211. | `boundless-tuning.md` | M |

## D. What players disliked

| Finding | Source | Conf. |
|---|---|---|
| Swimming was among the most disliked things: slow, must hold Space to float, you sink while in a menu or checking the atlas, slow to dive, nothing to see; players walked around lakes rather than swim. Most-asked fixes: float by default and press to dive, swim where you look, swim at run speed. | [F:16875], [F:19936], [F:26479], [F:42447], [F:45315], [F:27590] | H |
| Breath too short to reach deep resources (deep fungi, gems under seas); underwater building meant scaffolds and repeated drowning. The Kraken + quirk combo was welcomed as "scuba" and devs kept it. | [F:17318], [F:30707]; [F:52526] | H |
| Percent-of-max-health environmental damage (drowning, lava, falls) punishes high-health builds while healing is flat. | [F:17849], [F:16029] | H |
| One brew slot: players lost brews by drinking a second timed one by mistake, and wanted stacking (some for a better chef economy); others accepted it. The silent replacement was the complaint. | [F:18937], [F:45515], [F:64235], [F:35307] | M |
| Over-time healing taking the brew slot surprised players ("it replaced a speed brew"). | [F:64235] | M |
| Rage and Focus felt too alike and Focus "kinda useless" next to a one-shot bow; skills spread too thin. | [F:12464], [F:40908], [F:11861] | M |
| Wiki and tooltip numbers went stale after patches (index authors warned their tables were wrong after each patch; duration field bug). Lesson: put the real number in the item text from data. | [IDX]; [F:20062] | M |

## Not found

- **Air bar length** for an untrained character (seconds), its **refill rate**, and the **drowning share of max health and tick interval**. Looked: forum search ("air bar", "air meter", "breath", "breath bar", "drown", "drowning damage", "seconds of air", "underwater time", dev-filtered `@james`, `@vdragon`, `@dave`, `@olliepurkiss`), the wiki (full-text "breath", "drowning", "underwater", "swim"; Water page), mayumi and boundlessinfo data files (no player constants), boundlexx (only skills answers; items 500). **Ask Nathan to time in game:** full air bar to empty while still under water; empty to full at the surface; damage per drowning tick and ticks per second with a known max health; the same with a That Sinking Feeling item held.
- **Swim speed in m/s** (surface, under water, diving with C, sink rate with no key). Same places. Ask Nathan to time 20 blocks of surface swimming, a 10-block dive with C and a 10-block passive sink, untrained and with Agility levels.
- **Healing brew greater tier** and the minor and improved smoothie units: game text is a bundle reference or ambiguous; the 75..1200 per 5 s ladder is an inference.
- **Innocuous improved and super, luminous improved and super** values (item text missing or unfilled).
- **Brew Extension Helix** bonus per level.
- **Kraken brew's base duration** in data (20 min is a player-derived figure: 30 min at max Zeal ÷ 1.5).
- **Zombie Lungs** recipe (not in either recipe export).
- **Status effect description strings** with numbers (the `GUI_STATUS_EFFECT_*` list in t/53218 carries names only; no localisation file is hosted on mayumi or boundlessinfo: checked every data path their scripts and lazy chunks reference).
