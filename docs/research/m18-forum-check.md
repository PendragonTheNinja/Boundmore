# M18 forum check (2026-10-25)

Second pass over the sources that were blocked for tuning 4.13. This time the forum's Discourse JSON API, the fandom wiki's MediaWiki API and the fan sites' data files (boundless.mayumi.fi, boundlessinfo.com) answered; boundlexx answered only partly (see Not found). Facts and numbers only; every line has a source and a confidence (H high, M medium, L low). Forum links are `https://forum.playboundless.com/t/<topic>/<post>`. "Staff" means a Wonderstruck account (rossstephens, james, Steggs101 and so on).

## What this changes (summary)

| Current Boundmore value (feel.md, M18) | Finding | Verdict |
|---|---|---|
| Atmosphere food protects every line (guess, Measure 5) | One food per line (Caustic/Potent/Volatile chilli, biryani, pie) in the game data | **Wrong: make it per line** |
| Choking 5% of max health **per point short** every 5 s (guess) | "You suffocate at the same rate, no matter how many points you have" | Shortfall size should **not** scale the damage (M); rate and tick still unknown |
| Air 30 s, refill 8 s, fog to 30 blocks (guesses) | Bar refills when protected (R198); fog right up to you; food's last minute pulls fog to 30 m | Shape confirmed; the numbers stay guesses |
| Lava 20% of max a second (guess, Measure 6) | 30% (2017) and a third (2018) of max health **per tick**; periodic, one tick on contact; two blocks float you; no skill helps | Per-tick amount: use **a third of max**; tick length still unknown (20%/s is the same if the tick is 1.67 s) |
| Elemental 30% of spawns (guess, Measure 7) | Dev: 10% on the first elemental worlds rising to 50% on the hardest | **Lower it**: T5 about 10%, T6 about 23 to 30% (inference) |
| Exotic and elemental | They combine | Allow both on one creature |
| Power curve: early-access guide, floor 25% of time, spark following (`spark::power_scale`) | Since Release 198: one multiplier for time and spark, floor **0.4675**, measured table per 300 power | **Replace the curve** with the measured table (section 4a) |
| Coil wear 10,000 a coil, every coil by the recipe's wear (guess) | Coils fall by the same **percentage** as their machine; all coils on a machine wear alike; advanced about 20,000; wear can go negative | Keep "every coil wears"; scale each craft's wear to the machine's share; machines 2,000 to 10,000 |
| Spanners silver/gold/titanium 1200/30/13, 1100/30/14, 1500/30/20 (guess) | Game data: 1,100/70/10, 1,000/40/15, 1,500/50/30; gems 2,000/50/50 | **Use the data** |
| Gem spanners "most effective" on own machine (not built) | No per-machine number in the data | Leave flat 10,000 |
| Gem places | Sister gems in small amounts; emerald, diamond, topaz far commoner than their twins; amethyst rings the water's edge; sapphire 80 to 140 and topaz 110 to 220 altitude (sea 64) | Mostly agree; consider sister gems and twin rarity |
| M17 Effectiveness | New points: titanium 124% at 2 and 4 coils; diamond 96% at 2 coils; 35 to 200% range at 24 | Power share likely counts only above a per-item amount |

## 1. Atmosphere

| Finding | Source | Conf. |
|---|---|---|
| R198 (2018-09-13) rules, verbatim in substance: protected when skill + food protection >= the world's atmosphere damage; when protected the fog rolls out and the **air bar refills**; unprotected, the fog comes right up to the portal; in the last minute of a food's protection the fog moves up to 30 m; you can eat on the planet to refresh it | Release 198 notes, t/16254/1 (staff, 2018-09-13) | H |
| Hazard by tier 0/0/0/1/3/5/7/10 for Placid to Brutal | rossstephens, t/16261/15 (staff, 2018-09-13); wiki World page table | H |
| The debuff's number is the shortfall ("the one signifies that you are 1 protection level short") | rossstephens, t/16261/10 (staff) | H |
| **Shortfall size does not change the rate**: "Too low, and you are blinded and you suffocate at the same rate, no matter how many points you have in it" | Setimu666, t/17271/12 (player, 2018-10-08, after R198) | M (one player, consistent with the all-or-nothing staff wording) |
| Ranks 4 and 5 of each protection skill give +2 each; atmosphere pies give +3 | rossstephens, t/16261/17 and /20 (staff) | H |
| Lush -> caustic, Metal -> potent, Coal -> volatile (R198 rolled the old "neutral hazard" into the three lines) | rossstephens, t/16261/20 (staff) | H |
| Unprotected arrival reads as "instant drowning" to a player whose old protection no longer counted; another "runs out of breath very quickly" on a T4 world | t/16254/37 and t/16261/19 (players, 2018-09-13); t/17012/1 (player, 2018-09-20) | L (impressions; the bar is short, well under a minute is implied) |
| Players run unprotected through foggy portal hubs on purpose ("I have to run thru at least 3 foggy hubs"), so the air lasts long enough to cross a hub | a13o, t/22761/2 (player, 2018-12-02) | M (a lower bound of some seconds, no number) |
| Drowning damage (water) is a percentage of max health, like fall and lava damage | Sero, t/17849/1 (player, 2018-09-28) | M |
| Once the bar is out the HUD shows a "drowning countdown timer in the centre of the screen" | wiki World page (`action=parse&page=World`, frozen ~2020) | M-H |
| Interaction bug report: on a hazardous world with 5 protection, after drowning in water and surfacing the player "started taking 300 damage from" the atmosphere (protection seemed lost) | Tagris, t/14805/106 (player, 2018-08-17, **before R198**) | L (pre-R198 system; 300 at a ~1,000 to 2,000 HP character hints at a percentage tick) |

### 1a. Does atmosphere food protect one line or all?

| Finding | Source | Conf. |
|---|---|---|
| **One line each.** The game's recipes have three of every atmosphere food: Caustic, Potent and Volatile Protecting Chilli (rank 1), Biryani (2) and Pie (3), each cooked from raw in the mixer; the caustic chilli takes a corrosion **or** toxin amalgam (the two elements of its line), so each line's food comes from its own gem worlds' shards | MAY `recipes.js` (`FOOD_PROTECTING_CHILLI_CAUSTIC_COOKED`, `..._POTENT_...`, `..._VOLATILE_...`; same for BIRYANI and PIE) | H (game data) |
| Elemental "shielding" foods (blast bread and so on) are a different buff and do not help with the air: a player fed blast bread still had the fog on a Blast world | Karko t/16254/179 (2018-09-25) | M |

## 2. Lava

| Finding | Source | Conf. |
|---|---|---|
| Lava "does a third of your max health per tic": at 600 base health 200 damage; at 12,000, 4,000; even "that fraction of a second touching the lava" costs one tick | Sero, t/17849/1 (player, 2018-09-28) | M (one player's careful post; tick length not given) |
| Lava damage is a percentage of max health, so health skills do not help; full-heal and regen epics are flat, so big health pools take longer to recover | same thread, /1 and /13 | M |
| Lava "will damage the Character on contact very quickly making it almost impossible to survive once fallen into it"; mitigated only by the guild's Lava Protection buff (a guild helix, 2020+) | wiki Lava page (version 244) | M-H |
| Measured in early access: lava damage per tick 360 at 1,200 max health, 540 at 1,800 (30% of max a tick); the player guessed "twice a second?" for the tick but did not time it | AzureHelios, t/8220/1 and /11 (player, 2017-06-28, EA) | M for 30%; L for the tick length |
| Live 2018: "a third of your max health per tic" (200 of 600). So 30 to 33% a tick across 2017 to 2018, about **3 ticks to defeat** from full | t/17849/1 (2018-09-28) | M |
| **Damage is periodic, not per block**: grappling through 1 or 10 layers costs the same if you leave before the next tick; lava-gap-lava does not stack; touching lava at all costs one tick at once (falling through a 1 m sheet: 1 tick; walking onto it: 2 ticks; dropping into 2 m: float and keep taking ticks until defeated) | Alwin, t/47490/1 (2020-06-19); Ovis, t/47490/8 | M-H (careful player tests) |
| At 40 m/s on a grapple swing a player sometimes crossed 1 block of lava with no damage, so the game checks lava "N times a second" (a tick shorter than the crossing time would never miss; a crossing at 40 m/s takes about 0.025 s, so it says little) | Alwin, t/47490/16 | L |
| Two blocks of lava make you **float** (buoyant, like water); one block you fall through. Early access let you fall through lava "like air" (a bug players complained of) | t/47490/1 and /4 (2020); t/8220/6 and /15 (2017) | M |
| With your head in a water source you did not burn while your feet touched lava | Tmmk, t/47490/7 (player) | L |
| Health skills (Vitality) do not help: damage scales with max health. No skill reduces lava damage; the only mitigation is the guild **Lava Protection** buff (Release 211, 2019-01-15: "some resistance to Lava Damage", Minor, Improved and Super); one guild poll text calls it 75% reduction (unofficial). No food gives lava protection | Release 211 notes t/24546/1 (staff); Hashmalash t/27183/1, t/52455/2 (players) | H that skills do not help; L for 75% |
| You can swim in lava in the Sanctum without damage | t/47490/15 | M |
| How players got out: grapple out (t/22875/7, "forgot I still have my grapple equipped and could've easily pulled myself back out"); bump warp moves a stuck player out of lava (t/11856/2, moderator Havok40k, 2018) | as cited | M |
| Swim speed in lava: not re-found beyond Release 195 (already in M18 rule 45) | | |
| Lava sits "in caves on the lowest levels of some Worlds"; collected only with a forged liquid-breaker tool; burns combustible blocks below it down to bedrock; becomes rock where it meets water | wiki Lava page (v244) | M-H |

## 3. Elemental creatures

| Finding | Source | Conf. |
|---|---|---|
| **Chance a creature spawns elemental**: "Elemental worlds start off by having an elemental creature percentage of **10%**. As you travel further out into the universe and reach harder planets this percentage increases, currently maxing out at **50%**." Creatures only spawn with the world's primary element | Tobelawe (forum title "Developer", Wonderstruck) t/14255/30 (2018-07-29, two weeks before launch) | H that it is a dev statement; M that it held after launch |
| Before that change, a player's estimate on elemental worlds: "maybe every tenth creature was elemental" | boundmore t/14255/26 (2018-07-29) | L-M |
| Which tier gets 50% is not stated. Elemental worlds then ran T5 to T7 (the dev wrote "tiers 4 to 6" counting from 0, t/14255/49 and /52); a straight line gives about T5 10%, T6 30%, T7 50%; if 50% is T8 (Brutal, added later) it gives T5 10%, T6 23% | inference from the above | L |
| Elemental and exotic **combine**: "Exotic Elemental Mighty Cuttletrunks with 4 buffs" on T5 worlds; "elite, exotic, elemental wildstock" on hunts | Aridhol t/15485/1 (2018-09-04); Nightstar t/28658/4 (2019-04-12) | H |
| The wiki: creatures have 6 power tiers (none, Stout, Strong, Mighty, Elite, Hyper) and "in addition may sport Elemental or Exotic progression"; an elemental creature has a special attack of its element and drops that element's shards; exotics have extra health, damage, armour and speed buffs | wiki Creature page (frozen ~2020) | M-H |
| Release 199 (2018-09-26): meteorites on non-elemental worlds may spawn elemental creatures (so shards exist off gem worlds); a few elemental creatures roam lower tiers too | R199 notes t/17621/1 (staff); Hazir t/20013/1; Elrood t/18900/6 | H |
| Resistance is a **chance to block the whole hit**, crit or not ("when a creature resists your attack it doesn't get any damage, no matter if your attack is normal or crit"); elemental creatures resist physical (metal weapons) 40% of the time and their own element 80% (the in-game resistance panel shows the 40%) | boundmore t/30640/4 (2019-06-01); Venom t/21705/13 (2018-11-13); Trundamere t/22405/3 | M-H; agrees with M18 rule 50 |
| Conflict: one fist player thought crits always land on elementals | Rumplypigskin t/38230/15 (2019-11-29) | L (outweighed by the above) |
| Stat multipliers (+100% health, +50% damage and armour): not re-found on the forum; tuning 3.2.2 stays the source | | |

## 4. Power coils

### 4a. Power above the need (post-launch; this replaces the early-access curve)

| Finding | Source | Conf. |
|---|---|---|
| **Release 198 (2018-09) changed coils, undocumented**: maximum time cut went from 75% to about 53%; power now also cuts **spark**, by the same multiplier; one coil no longer gives the full cut on a no-power recipe | Cakengrad t/18018/47 (player, 2018-09-30); james (staff) t/18018/50 accepts the notes lacked detail; Simoyd t/12444/2 (2018-09-25) | H that it changed; the early-access guide (t/12444/1, used for tuning 4.13.2 and `spark::power_scale`) is **out of date** |
| One multiplier applies to both time and spark. **Best multiplier 0.4675** (46.75% of base time and spark). Each coil above the need takes about 5% of what remains, about 4% after 1,000 power over, 3% after 2,000; a no-power recipe's first 100 power takes it to about 0.82; ~2,300 over gives 0.50 and ~4,600 over reaches 0.4675 for needs of 0, 500 and 1,000; a 1,500-need recipe is a little slower (0.52 at 2,300 over) | Simoyd t/12444/2 (player measurement, 2018-09-25) | M-H |
| Measured table, refined titanium alloy mass craft (refinery, need 0; base 3,000 s and 20,000 spark, which the game data confirms), advanced coils (300 each) -> multiplier: 0 1.0000; 1 0.7662; 2 0.6969; 3 0.6405; 4 0.5951; 5 0.5594; 6 0.5317; 7 0.5108; 8 0.4955; 9 0.4847; 10 0.4773; 11 0.4726; 12 0.4699; 13 0.4684; 14 0.4678; 15 0.4676; 16+ 0.4675. Time and spark move together. "For things that require power, read the first column as coils over the requirement" | Arkhainn t/20845/17 and /20 (player, 2018-10-31; repeated t/25585/3, 2019-02-05); base checked against MAY `recipes.js` (`METAL_TITANIUM_REFINED` 3,000 s, 20,000 spark, power 0) | H (a full measured table, consistent with the data) |
| Cross-check: marble (refinery, need 800, mass base 5,000 spark per MAY `recipes.js`) measured 4,780 / 3,328 / 2,409 / 2,341 / 2,338 at 100 / 1,000 / 2,800 / 4,000 / 6,400 over = 0.956 / 0.666 / 0.482 / 0.468 / 0.4676. The floor is exactly 0.4675 and the curve from about 2,000 over matches the no-power table; below that, recipes with a need start slower (100 over: 0.956, not 0.82) | t/52603 (from tuning 4.13.2) against MAY `recipes.js` `ROCK_MARBLE_REFINED` | H for the floor; M for the start |
| Another fully coiled machine: 10,000 spark mass crafts cost 4,675 (0.4675) | Shadesmar t/20845/10 (2018-10-31) | M-H |
| Spark-reduction skills and the guild spark buff apply on top: fully coiled marble 2,338 without the guild "spark and wear" buff, 1,338 with it (a flat 1,000 off at that level, as seen) | PrincessMaude t/42020/91 (2020-02-22) | M |
| Spark reduction "is calculated with the extra power" (power over the need), confirmed by comparing compact coal (need 2,000) and compact diamond (need 0) at 10 coils | Mayumichi t/38602/2 (2019-12-08; runs boundless.mayumi.fi) | M-H |
| Players' advice: no-power recipes want about 4 advanced coils (1,200), up to 8 still worth it; power recipes count coils over the need | boundmore t/47637/12 (2020-06-22) | M |
| The Centraforge does not get faster or cheaper with power; coils raise Effectiveness instead (see 6) | Mayumichi and Arkhainn t/25585/2 and /5 (2019-02-05) | M-H |

### 4b. Coil wear

| Finding | Source | Conf. |
|---|---|---|
| **Coils lose wear by the same percentage as their machine** (the bars fall together), though coils have far more durability, so they take more spanner hits to mend | fidach t/26799/1 (2019-02-28); boundmore t/26799/2 ("coils have way more durability than machines"); Prome3us t/20119/171 (2018-11-23: "wear % on coils correlates with wear % on machines") | M-H (three players agree) |
| Every coil on a machine wears at the same rate, extra coils included (no sharing of the wear) | SilberB t/12363/53 (2018-10-24); Nightstar t/34798/1 complains helix coils all wear "more than needed" (2019-09) | M |
| Basic and advanced coils lose durability at the same rate; advanced have more total. "All coils have the same total (repair) hp, believe it's around 20k for advanced"; "20+ cranks with an iron spanner" (700 a hit, so 14,000+) to mend a worn one | Bryethewizard t/21452/5 (2018-11-10); Prome3us t/20119/171; Alienfish t/20119/168 | L-M for 20,000 |
| Machine durability (from the debug view, 2019-11): workbench 2,000; extractor 2,000; mixer 4,000; spark generator 8,000; refinery 10,000; compactor 10,000; centraforge 10,000 | NerArth t/37957/1 (player reading the debug screen, 2019-11-24) | M-H |
| Minting coils (a later kind, not advanced) wear out before their machine (about half its life), so not every coil matches its machine | t/34188 (2019-08), t/33156 (2019-07), t/37957/3 | M |
| **Wear can go negative, by design** (a dev): a coil gives full power while it has any durability left, whatever the recipe's wear, and goes into wear debt instead of clamping at 0 so nobody gains by crafting on nearly worn coils; debt must be mended back up (the 189,000 repair in t/27900 was this debt) | lucadeltodecso (Wonderstruck) t/19157/2 and /4 (2018-10-11), t/34188/8 (2019-08-20); Touch122 t/27900/7 | H |
| Worn coils keep wearing while any coil on the machine still powers it | ctrl-64 t/27900/8 | M |
| Release 235 (Testing 234, 2019-11): one bar shows the whole machine-and-coils network; hitting the machine with a spanner mends machine and coils together, "biasing to repair the engines more than the Machine as the engines have more durability", so all mend at the same rate | Testing 234 notes t/37644/1 (staff); Release 235 t/38043 | H |
| Any spanner mends coils, even stone; it is just slow | Lorgar t/27900/4 | M |
| Creatures that attack a coil wear it ("engines lose durability when zapping someone") | Prome3us t/12363/24 (2018) | L |
| Machine wear per craft = the recipe's `wear` (single/bulk/mass) in the game data, e.g. the lava helix 110/160/200 on the workbench. Coil wear per craft: no number found; by the equal-percentage rule above it is the recipe's wear scaled by coil capacity / machine capacity (inference) | boundlessinfo `compiled-recipes.json`; wiki Lava Protection Helix page | H for the field; L for the coil rule |

### 4c. Machine levels, spanners

| Finding | Source | Conf. |
|---|---|---|
| `machineLevel` is a per-recipe label in the game's recipe data (a 2018 data diff shows `TOTEM_AMMO_RESURRECTION/machineLevel: "Overdriven" -> "Powered"` with its spark cut); Release 199 fixed costs "showing max Overdrive reduced costs when not enough power is supplied". Its thresholds were not found; it looks like a label of the recipe's power tier, not a separate rule | james (staff) t/20197/2 (2018-10-23); Release 199 t/17621/1 | M |
| Spanner stats (game data): stone 600 durability, speed 40, energy 4, 100 a hit; copper 500/70/6, 400; iron 900/30/8, 700; **silver 1,100/70/10, 1,500; gold 1,000/40/15, 2,000; titanium 1,500/50/30, 6,000**; every gem spanner 2,000 durability, speed 50, energy 50, 10,000 a hit, each with its gem's element (amethyst corrosion, diamond blast, emerald toxin, topaz shock, ruby burn, sapphire chill). Columns: Durability, Action Speed, Burst Stamina Drain, Wear Removed Per Hit | boundlessinfo `item-attributes.json` (fan site data file) | H |
| Gem spanners' "most effective on its machine": the data gives every gem spanner the same 10,000 a hit and no per-machine field; nothing found that measures a difference | same; forum searches | M that there is no numeric bonus in the data |

## 5. Gems

Boundless altitudes run 0 to 255 with the sea at 64 (world configs, tuning 4.13.4); Boundmore's sea is at 128, so read altitudes against the sea.

| Finding | Source | Conf. |
|---|---|---|
| The intent (dev): amethyst shallow water areas, emerald deep water areas, sapphire inside hills, topaz inside mountains, diamond deep below mountains, ruby below sandy deserts. "Most of the resources do not have hard rules": common in their band, rare outside it | Tobelawe (Developer) t/14255/6 and /17 (2018-07-27/28) | H (already in M18) |
| **Sister gems**: each gem world also has its pair's gem "in much lower quantities", e.g. a little diamond on a burn world. Pairs: amethyst and emerald (caustic), diamond and ruby (volatile), sapphire and topaz (potent) | Tobelawe t/14255/17 and /19 (2018-07-28) | H for the intent; confirmed by players below |
| Gems only on tier 5 to 7 (1-based) worlds ("tiers 4-6" counting from 0), each on its own world type | Tobelawe t/14255/49 and /52 (2018-08) | H |
| Altitudes players mined at (launch universe, 2018-10): amethyst under shallow water along coasts, altitude 5 to 40; emerald under deep water (oceans), 5 to 40; ruby under deserts and close to lava, 5 to 40; sapphire in mountains, 40 to 110; topaz in mountains, 110 to 150 | Bryethewizard t/18769/26 (2018-10-06; same t/19145/3) | M |
| Mining guide (2019-08): emerald 5 to 25 (Shedu Tier, Toxic); amethyst 5 to 25 (Norkyna); diamond and ruby 5 to 20 (Malurialakrib, Blast), diamond "slightly above 20" too; sapphire roughly 80 to 140 (best 90 to 120, Houchus I, Chill); topaz roughly 110 to 210, "all the way up to 220" (Galan, Shock); 120 to 160 gives mostly topaz with a little sapphire | Dhusk t/34506/1 (2019-08-27) | M-H (careful guide; "recommended, not the limits") |
| Emerald "spawns liberally under water"; **amethyst forms strict lines around the perimeter of water** at that depth ("a narrow ring around emerald deposits due to the required depth of the water above") | Dhusk t/34506/1; Shadesmar t/22345/7 (2018-11-24) | M-H |
| Diamond "spawns liberally deep under mountains" and is mostly **mutually exclusive with ruby** (where you see ruby or silver, no diamond) | Dhusk t/34506/1 | M |
| **Rarity**: emerald, diamond and topaz come in "MUCH higher quantities" than amethyst, ruby and sapphire; players mine the plentiful one and convert with a gem-changer chisel. Topaz: about 2,800 rough from one 2,600-durability hammer on Galan (T6) | Dhusk t/34506/1 | M |
| Other reports: diamonds from about depth 30 and below (t/21442/4, 2018-11); "around depth 10, under mountains (elevation 120+)" (t/14849/13, 2018-08); sapphire and topaz around altitude 115 on Alnitans (t/15136/6) and 105 to 175 (t/19516/1); topaz under ice and floating mountains on a sovereign (t/56645/4, 2021) | as cited | L-M |
| Per-world gem counts (boundlexx `/items/{id}/resource-counts/`): the endpoint exists but boundlexx's database endpoints returned HTTP 500 on 2026-10-25 (only `/skills/` and `/metals/` answered) | api.boundlexx.app | - |

**Against Boundmore's current rules** (feel.md "Gem places"): amethyst "within 4 of water", emerald "under 10+ of water", ruby "under sand or gravel" agree with the intent. Diamond "70+ down" agrees in spirit (Boundless: altitude 5 to 20, 44 to 59 under its sea, under mountains). Topaz "80+ above sea" starts higher than Boundless (110 to 220, so 46 to 156 above its sea, overlapping sapphire at 110 to 140). Sapphire "20 to 80 above sea" fits the measured 80 to 140 (16 to 76 above Boundless's sea at 64). Emerald and amethyst in Boundless sit at altitude 5 to 40, far below the sea (24 to 59 under it), i.e. deep under the seabed, not just under the water.

## 6. Forge Effectiveness

No formula was found; these are the measured points.

| Finding | Source | Conf. |
|---|---|---|
| Effectiveness is shown in the forge's top-left circle; range with 24 coils is about 35% to 200%, lucent (rift, blink, umbris) gear and totems at the low end; capped at 200% | Tynker, Mayumichi t/30053/15 and /16 (2019-05-17); bregma t/30053/12 | M-H |
| Diamond heavy hammer, full forging build: 200% at 18 advanced coils; 194% at 17; each coil fewer takes another 6% | Kurokaze t/30053/7 (2019-05-17) (already used to scale M17's guess) | M-H |
| "For any material under gem (titanium etc.), 17 coils give 200%"; a sapphire shovel tops out at 197% even with 24 | redlegdaddy t/30053/14; t/30053/9 | M |
| Maxed forging skills (test server, 2019-01): **titanium hammer 124% with 4 advanced coils and still 124% with 2**; diamond hammer **96% with 2 coils**, 144% with "a bunch more" | Sterbehilfe t/24160/3 and /7 (2019-01-06) | M (one tester; the flat 2-to-4 coil result suggests coils only count above some threshold of the item, as the coil text's "excess power" says) |
| No power: titanium tools at **68%** for one forger; "with a high flexibility tool and no forge coils you could get like 86%"; iron forges well without coils on a maxed forger | pofs t/19100/13 (2018-10-10); Prome3us t/19100/21; Shadesmar t/21407/30 | L-M (skill builds not given) |
| Effectiveness multiplies the boon points that reach the bar (100 points at 86% gives 86); a boon, once rolled, is full strength | Prome3us t/19100/21 | M-H |
| "With 24 coils every material gets 200%" (contradicted by the sapphire shovel and lucent reports above) | Arkhainn t/25585/5 (2019-02-05) | L |
| Pre-launch (Testing 193, 2018-05): max 100%, from coils, flexibility and intelligence; the 200% cap came later | willcrutchley t/13556/18 (2018-05-26) | M (history only) |

Against M17's guess (Effectiveness = min(2, F x I + P)): it gives diamond 92% at no coils plus 6% a coil (104% at 2 coils; measured 96%, so the power share is probably not linear from zero). The titanium 124% at 2 and at 4 coils is the strongest hint that only power above some per-item amount counts. M17's 87% for special materials at 24 coils is still far above the 35 to 42% seen.

## Not found

- The air bar's length in seconds, the choking countdown's length, damage per tick once out of air, and whether choking stops regeneration (only impressions: "instant drowning", "run out of breath very quickly", yet hubs can be crossed unprotected). Still Measure 1 to 4.
- Any altitude or underground rule for atmospheres (none mentioned anywhere; nobody reports shelter underground or indoors helping).
- Lava's tick length (a third of max health a tick is the measured part); swim speed beyond Release 195.
- Which tier gets the 50% elemental rate; whether the 10% to 50% plan survived launch unchanged.
- Exact coil durability (only "around 20k for advanced"), basic coil durability, and coil wear per craft as a number.
- The thresholds behind Standard / Powered / Overdriven / Supercharged.
- A numeric "most effective" bonus for a gem spanner on its own machine (the data shows none).
- Per-world gem counts (boundlexx's database endpoints return 500).
- A forge Effectiveness formula.
- Sources tried: forum Discourse API (fine, but rate-limits `?print=true`), fandom wiki API (fine; no Atmosphere or Elemental page), boundlexx (`/skills/`, `/metals/`, `/schema/` only; worlds, recipes, items give 500), boundless.mayumi.fi `recipes/recipes.js` and boundlessinfo `compiled-recipes.json` and `item-attributes.json` (fine).
