# Prestige, settlements and footfall in Boundless (research)

*2026-10-28. Facts only (numbers, formulas, rules); names and text are Boundless's and are not to be copied into the game. Inferences are marked "inferred". Forum topics are cited as t/id or t/id/post; dev posts (james, lucadeltodecso, rossstephens, olliepurkiss, luke-turbulenz) weigh most.*

## Sources

| Key | Source |
|---|---|
| R161 | t/7129, "Testing 161" (2017-04): settlements, capitals and footfall first added |
| R191 | t/13087, "Testing 191" (james, 2018-04-24) |
| R211 | t/24546 and t/23287, "Release 211" / "Testing 210" (guilds, footfall rework; james, 2018-12 to 2019-01) |
| R213 | t/25390, "Release 213" (2019-01-31) |
| R218 | t/28046, "Testing 218" (james, 2019-03-28): the prestige rework |
| P218 | t/27485/1, "Prestige Balance in Testing 218" (rossstephens, 2019-03-15): the design of the new formula |
| R242 | t/44553, "Release 242: Beacon Reclaims and Compactness" (james, 2020-04-23) |
| R245 | t/48091/1, "Testing 245: Sovereign Worlds" (2020-07) |
| R246 | t/49124/1, "Testing 246: Creative Worlds" (2020-07-24) |
| SET | t/24636, "Explaining Beacons and Settlements post Release 211" (james devlog and lucadeltodecso, 2019-01-16) |
| FF | t/24941, "Explaining Footfall post Release 211" (james devlog 2019-01-22, lucadeltodecso, players' tests) |
| PR1 | t/21311/1, "Help! Give us your opinion on refactoring prestige" (james, 2018-11-07) |
| PR2 | t/39950/23, /39, /41 (james and lucadeltodecso, 2020-01) |
| PR3 | t/18202 (lucadeltodecso, 2018-10): the pre-218 system |
| BKT | t/34403 (Pfiffel and others, 2019-08): the data value is a bucket |
| wikiS, wikiF, wikiB | Wiki pages Settlement, Footfall, Beacon (MediaWiki API; frozen around version 244, 2020) |
| pf | pfiffel.com/bl/blocks/dataBlockStats.js: per-block `prestige`, `buildXP`, `mineXP` from the game files (446 block kinds; data from about mid 2019, before Release 233's building blocks) |
| bx | boundlexx API schema (`/api/v1/schema/`): what the final game exposed per item, beacon and settlement (the data endpoints return errors now) |

## A. Prestige

### A1. What counts

| Finding | Source | Conf. |
|---|---|---|
| Prestige belongs to a beacon and is computed from the blocks inside its plots. The game does not track prestige per plot, only for the whole beacon | SET (lucadeltodecso #36) | H |
| A settlement's prestige is the sum of its beacons'. A guild's is the sum of its aligned and controlled beacons', across the whole universe (they need not touch) | R161, R211, t/24515/33 | H |
| A plot is 8 x 8 x 8 blocks; a world chunk column is 16 x 16 (2 x 2 plot columns) | SET | H |
| Since Release 218 every block kind gives some prestige, natural kinds included (soil, rock, foliage, trunks: "a tiny amount"), and every model (machines, doors, poles, beams, torches, storage, flowers). Only air, bedrock, water and lava give none | R218, P218 | H |
| Open portals count (since Release 191) | R191 | H |
| Ornamental (prestige) crops give prestige only when mature (actual values 50, 80, 100), none while growing | t/33582/120 (dev reply to Pfiffel) | H |
| **Origin:** before 218, blocks generated with the world gave 0 ("a beacon off in the wilderness will always have 0 prestige ... if you haven't modified the world"); only player-placed blocks counted | PR3 (lucadeltodecso #12, #18) | H |
| After 218 players still reported that untouched wild blocks give nothing and placed or changed ones do; an empty one-plot beacon shows about 10 prestige on its own | BKT (#4, #11), t/62906/2 | M (inferred: the origin rule survived 218) |
| Seams (ore, gem, fossil, relic veins), meteorites and creature-themed hangings are 0 in the data | pf | H |
| An uncompact beacon (score below 0 on a -100 to 100 scale) gets **0 prestige** until fixed: no footfall, nothing to its guild, settlement or world. Plotting that would make a beacon uncompact is refused. A square is perfectly compact; neighbouring beacons count, so roads inside a settlement are fine. Sovereign world owners can switch the rule off | R242, R245 | H |

### A2. The per-block number

| Finding | Source | Conf. |
|---|---|---|
| Design rule: a block's prestige is proportional to the effort to gather and craft it, from an internal "crafting value" per item. "Prestige isn't about beauty, it's about cost" | PR1, P218 | H |
| The data's `prestige` integer (0 to 26 after 218) is a **bucket index**, not the prestige added. Blocks are grouped into these buckets for the histogram; the real base values are larger and hidden (removed from the debug view) | P218 ("each prestige value ... falls into a bucket"), BKT | H |
| Scale of the hidden values: an advanced machine coil measured about 280 in game where the data says 24; ornamental crops are 50/80/100 where the data says 10/13/17. Effective prestige of one block can swing "by hundreds of points" with the rest of the build | BKT #9, #17; t/33582/120 | M |
| Pre-218 base values (wiki table): advanced power coils 150, other coils 100, decorative gems 140, compact gems 100, spark generator 80, machined titanium 80, refined titanium 70, machined silver and gold 70, machined iron 60, refined iron 50, machines 50, compact iron 40, gleam lantern 40, workbench 30, portal 30, compact copper 30, lantern 15, decorative rock and wood 12, bricks 10, refined gleam 10, gem torches 10, shop stand 8, glass 6, refined rock and wood 6, compact soil 6, gleam 6, torch 5, storage 5, plinth 5, chair and table 5, plants and boulders 4, timber and stones 2 | wikiS | H (that these are pre-218); they were rebalanced, not kept |
| The full post-218 bucket table for all 446 kinds is in `dataBlockStats.js` on pfiffel.com; the final game's item records also carried one `prestige` integer (boundlexx) | pf, bx | H |

**Bucket per block kind, mapped to Boundmore's catalogue** (pf; Boundless kind names kept only to cite the data). Use these as relative ranks, not as prestige added.

| Boundmore blocks | Boundless kind (data) | Bucket |
|---|---|---|
| soil, clay, peat; turf, meadow_turf, moss_turf | SOIL_*_BASE, GRASS_* | 1 |
| stone, basalt, limestone, sandstone | ROCK_*_BASE | 1 |
| sand, gravel, ice, snow, mud, ash | SAND, GRAVEL, ICE, MUD, ASH | 1 |
| trunks, foliage (all kinds) | WOOD_*_TRUNK, WOOD_*_LEAVES | 1 |
| timber (three) | WOOD_*_TIMBER | 1 |
| tilled_soil | SOIL_*_TILLED | 2 |
| glow_crystal | CRYSTAL_GLEAM_BASE | 3 |
| refined_glow_crystal (faceted: not in this data) | CRYSTAL_GLEAM_REFINED | 5 |
| brick, stone_bricks, basalt_bricks, limestone_bricks | ROCK_*_BRICK | 9 |
| refined_stone, refined_basalt, refined_limestone | ROCK_*_REFINED | 4 |
| carved_* and tiled_* rock | ROCK_*_DECORATIVE1/2 | 12 |
| refined timber (three) | WOOD_*_REFINED | 4 |
| carved timber (three) | WOOD_*_DECORATIVE | 12 |
| compact_soil, compact_clay, compact_peat | SOIL_*_COMPACT | 10 |
| compact_gravel | COMPACT_GRAVEL | 19 |
| marble (bordered, frieze: not in this data) | MARBLE | 19 |
| clear_glass (framed, ornate: not in this data) | GLASS_DEFAULT_BASE | 7 |
| lantern | LANTERN_DEFAULT_BASE | 12 |
| compact / refined / machined copper | METAL_COPPER_* | 16 / 17 / 18 |
| compact / refined / machined iron | METAL_IRON_* | 17 / 18 / 19 |
| compact / refined / machined silver alloy | METAL_SILVER_* | 19 / 20 / 21 |
| compact / refined / machined gold alloy | METAL_GOLD_* | 19 / 20 / 21 |
| compact / refined / machined titanium alloy | METAL_TITANIUM_* | 22 / 23 / 24 |
| circuit, filigree, grid metal blocks | not in this data | ? |
| compact_<gem> (six) / polished_<gem> (six) | GEM_*_COMPACT / GEM_*_DECORATIVE | 21 / 22 |
| compact / polished aurorite, pyrelith, gloamite | SPECIAL_*_COMPACT / *_DECORATIVE (refined 24) | 22 / 26 |
| compact coal soft / medium / hard (as blocks) | COAL_*_COMPACT | 15 / 18 / 20 |
| torch / tallow_torch | LIGHT_TORCH_BASE / _IMPROVED | 2 / 5 |
| crafting_table / workbench | MACHINE_CRAFTINGTABLE / WORKBENCH | 4 / 11 |
| furnace, furnace_crucible (stone) | MACHINE_FIREPIT / CRUCIBLE_STONE | 6 / 4 |
| iron_furnace, iron_furnace_crucible | ..._IRON | 17 / 16 |
| titanium_furnace, titanium_furnace_crucible | ..._TITANIUM | 21 / 21 |
| extractor / compactor / mixer / refinery / forge | MACHINE_*_BASE | 11 / 15 / 13 / 15 / 15 |
| spark_generator / spark_link | MACHINE_POWERCORE / PIPE | 19 / 5 |
| <machine>_coil / advanced_<machine>_coil | MACHINE_ENGINE_* / *ADVANCED | 22 / 24 |
| portal_conduit, portal_surface | PORTAL_CONDUIT_* | 15 |
| warp_shrine (warp conduit for comparison) | WARP_CONDUIT_* | 3 |
| claim_stone (beacon control) | CONTROL_BEACON_BASE | 5 |
| storage_chest, storage blocks | CONTAINER_STORAGE_BASE / SHELF | 8 |
| stone_plinth, wood_table, wood_chair | FURNITURE_PLINTH / TABLE / CHAIR | 13 |
| shop_stand, request_basket | SHOP_PLINTH_SELLING / BUYING | 16 |
| lock | LOCK_DEFAULT_BASE | 15 |
| gathered plants, flowers, fungi, boulders | FLORA_*, PLANT_*, FUNGUS_*, ROCK_BOULDER etc. | 4 |
| crops (grains, berries, tubers) | CROP_* | 1 (waxy tuber 2, exotic tuber 3) |
| signs: wood, stone / metal / crystal (both kinds) | SIGN_* | 14 / 16 / 17 |
| doors and trapdoors, plain: wood, stone, metal, crystal | DOOR_PLAIN_*, TRAP_PLAIN_* | 3, 5, 13, 17 |
| doors and trapdoors, stylish | DOOR_STYLISH_* | 11, 13, 17, 18 |
| doors and trapdoors, ornate | DOOR_ORNATE_* | 14, 16, 20, 20 |
| poles and beams, plain: wood, stone, metal, crystal | PLAIN_*_POLE / _BEAM | 2, 3, 8, 12 |
| poles and beams, stylish | STYLISH_*_POLE / _BEAM | 8, 11, 13, 15 |
| poles and beams, ornate | ORNATE_*_POLE / _BEAM | 11, 13, 15, 16 |
| seams of any kind, relic caches, meteor shell and heart | *_SEAM, METEORITE_* | 0 |

### A3. How the total is formed (Release 218 on)

| Finding | Source | Conf. |
|---|---|---|
| The score is built from **histograms**, not a sum of blocks: a histogram of blocks per prestige bucket, a histogram of chiselled blocks per chisel shape type, the share of foreign-coloured blocks, and building density (built against air). "Blocks are not scored individually"; the system is nonlinear | P218, PR2 (james #23) | H |
| **Bucket spread:** a good spread across buckets leaves the score whole; a very uneven spread (thousands of one high bucket and little else) is penalised "on a curve of how uneven the spread is" | P218 | H |
| **Weighting:** the ideal spread favours large amounts in the low buckets (wood, stone) over a castle of the top bucket | P218 | H |
| **Buildable space:** a beacon packed solid with blocks and few air gaps is penalised; explicitly aimed at plots filled 8 x 8 x 8 with high blocks. An 11 x 11 x 5 pit was said to be unaffected | P218, t/27485/59 (james) | H |
| **Colour:** reward for a good mix of blocks in the world's own colours and colours from other worlds (the aim is to make players gather from other worlds). On sovereign worlds this part is off (player blocks never use the world default colour), so prestige there is a little lower | P218, t/27485/17, /61 (james), R245 | H |
| **Chiselling:** histogram by shape type; the maximum needs a decent variety of shapes; one shape repeated gives little. Lattice shapes overlap the other shape types, so they add little on top of other chiselling | R218, t/44073/2 (lucadeltodecso) | H |
| Chiselling is a remove plus an add of a different block, so its effect can be up or down | PR2 (lucadeltodecso #39) | H |
| Players estimate chisel credit caps at around 225 chiselled blocks per beacon | t/44073/5 (Krollbar) | L |
| **Never decreases:** adding a block, chiselling or adding a plot never lowers prestige; it may add 0, a little, or a lot (adding a little dirt can be worth 0, 10 or 100,000 depending on the rest). Removing never raises it. **Order does not matter**: the same contents give the same score | R218, PR2 (lucadeltodecso #39, #41) | H |
| In practice players still saw small drops when a block was *replaced* (tilling 15,000 soil lost about 100,000 in a beacon full of one-colour bricks; adding 4 stacks of mud there added 600,000) | t/62906 | M |
| Old bonuses as a % of base are gone; bonuses are extra prestige computed on their own | R218 | H |
| **Same category:** blocks with different buckets count as different for variety; players believe each distinct block kind and colour helps (copper and iron differ). Not confirmed by a dev beyond "different bucket values count as varied" | t/37218, BKT #18 | M |
| **Old system (to Release 217), for contrast:** sum of base prestige of placed blocks times a multiplier for variety, other-world colours (to x1.3 at 30%), chiselled blocks (to x1.2 at 25%), built space (to x1.5 at 10% or less built); combined cap x2.5. Adding a block could lower the total | wikiS, PR3 | M (a player's reading of the files) |
| **Recalculation:** beacon prestige is "effectively synchronous", no delayed adjustment. The 218 rebalance recalculated every beacon at once | PR2 (lucadeltodecso #39), t/24085/1 | H |
| No cap on prestige per plot was ever stated; the density and spread penalties do that job | (absence in all sources) | M |
| Beacons do not need any prestige to exist; **10,000 prestige per beacon** is needed for footfall (Release 211), and a settlement needs 10,000 total | R211, SET #36 | H |

## B. Footfall

### B1. Rules

| Finding | Source | Conf. |
|---|---|---|
| Purpose: reward players for making useful or beautiful places worth visiting; "a function of prestige, player activity and time". Not meant as a daily income tap (feats are that) | FF (james #1), t/26330/42 | H |
| Paid only to beacons **in a settlement** with **at least 10,000 prestige of their own** (Release 211; before that, 191 to 210, pay scaled with the beacon's prestige up to full at 5,000). Below 10,000: nothing | R191, R211, t/24677/62 | H |
| The base amount comes from the **settlement's** rank (the greater settlement, not a guild settlement, to avoid counting twice): Outpost 10,000+ 20, Hamlet 50,000+ 30, Village 250,000+ 40, Town 1,250,000+ 60, City 6,500,000+ 80, Great City 32,000,000+ 100 coin | wikiS, SET #36 | H |
| **Trigger:** a character entering a beacon (being inside its plots, not just above them; head or feet) | SET #41, t/43991/20, R211 | H |
| **Once per owner per settlement:** a visitor pays only for the first beacon of a given owner they enter in a settlement; other beacons of the same owner in that settlement pay nothing for that visit. Beacons of the same owner in another settlement pay again | SET #36, t/24685/229 | H |
| **Who does not pay:** a visitor with any permission on any of the owner's beacons in that settlement (they do not count toward the traffic count either). Being friends or guildmates does not by itself stop payment | R211, t/26330/42, FF #165 | H |
| Counted per character, not per account | t/21072/6 (player) | M |
| **Cooldown:** a visitor pays at most once per 24 hours per owner and settlement, timed from the last **payment** (from Release 211 hotfix; before it was from the last visit, so daily commuters paid nothing) | R211, FF #216 | H |
| **Repeat visits:** a visit within 5 days of the last payment pays a reduced amount; full after 5 full days. Players' measured steps, as a share of the fresh-visit amount: 1 day 20%, 2 days 40%, 3 days 60%, 4 days 80%, 5 days 100% | R211, t/42190/6, t/44657/12, t/52213/11, FF #164 | M (measured, not published) |
| **Fresh visits pay double the base** (a fresh visitor in a Town measured 120 coin = 2 x 60) | FF #164, t/52213/11 | M |
| **Traffic:** pay is scaled down by how many visitors the owner's beacons in that settlement have paid for in the last 24 hours, stepwise (the wiki: 100% under a threshold, 80% above it, and so on). Measured: fresh visits fell from 120 to about 84 (70%) as visitors came in; a quiet beacon recovers its bonus | R211, wikiF, FF #164 | M (step sizes unknown) |
| Owner absence (Release 191): 7 days away with no reduction, then a linear daily decline; visiting the beacon or its settlement counts. Still reported in 2021 | R191, t/55599/2 | M |
| Wiki's worked example: base 20 x 25% (second day in a row) x 2 (fewer than 5 recent visitors) = 10 coin | wikiS | L (pre-dates later tuning) |
| Settlement rank changes the base only; beacon prestige above 10,000 adds nothing more to footfall directly | t/30582/1, SET #36 | H |
| No daily cap per beacon was published; the traffic steps act as one | (absence) | M |

### B2. Collecting, guilds and later changes

| Finding | Source | Conf. |
|---|---|---|
| Coin accumulates in the beacon and is collected by hand at its beacon control, by the owner only | wikiF | H |
| Release 213: a guild-controlled beacon can have its footfall owned by the guild, withdrawn only by guild leadership | R213 | H |
| Release 211 bug fixes: any permission (not only builder) stops payment; a multi-beacon placement trick for extra coin was closed | R211 | H |
| Release 213.1: balance pass raising footfall to higher-traffic beacons and settlements; median footfall per player then higher than before 211 | R213, t/25328/54 (james) | H |
| Release 239.3: a beacon at 0,0,0 no longer gets footfall from players entering the world | t/41409/150 | H |
| Release 242: a deleted beacon drops its uncollected footfall as coin; a reclaimed one returns it with the reclaim. An uncompact beacon gets none | R242 | H |
| Release 246: creative worlds generate no footfall coin | R246 | H |
| Footfall was the second-largest coin source after feats (2018), both "way above expectations" | t/13087/43, /45 (luke-turbulenz) | H |

## C. Settlements

| Finding | Source | Conf. |
|---|---|---|
| A settlement is a set of beacons whose **plot columns** touch (since Release 211; vertical distance ignored, so a sky plot can touch an underground one), with total prestige at least 10,000. Before 211 it was computed by 16 x 16 chunks, so gaps of 1 or 2 plots could still join | SET, R211 | H |
| **Roads:** a plot column is a "road" if it plus its 8 neighbours hold **fewer than 6 plots** (211 shipped with 9, cut to 6 in 211.2). Road plots do not join settlements; the settlement is the contiguous non-road plots, then **dilated by 1 plot** round its edge (not into unbeaconed plots, nor into other guilds' plots) | SET #17, #25, #35, t/25328/54 | H |
| A beacon split by roads into two non-road pieces that fall in two settlements "bridges" them and is dropped from both (a single beacon can even do this to itself) | SET #25 | H |
| **Guild settlements:** touching beacons aligned to one guild always form their own named settlement, which still shows inside any larger (greater) settlement it touches. The guild counts as one owner | R211, t/24085/1 | H |
| **Warden:** the player (or guild) with the most prestige in the settlement; ranking is by a player's total prestige across their beacons there, not by single beacon. The warden names the greater settlement; a guild with control names the guild settlement; owners name beacons. The beacon control shows the top 5 contributors | R161, R211, t/24772/1 | H |
| Settlements too small get no rank and no name | R161, wikiS | H |
| **Capital:** the highest-prestige settlement on a world; marked on the compass and seen in the sky when holding a totem; warping to a world without a location lands at its capital (Release 245) | R161, wikiS, R245 | H |
| **Viceroy:** the title of the capital's warden (a trophy existed for it; reachable on sovereign worlds, not on creative ones; a guild can hold it) | t/24085/1, t/55308 | M (inferred from use) |
| Exoworlds allow no beacons, so no settlements | R218 | H |
| The warden title gives no powers; players resisted giving it any, since many are under a warden involuntarily | t/34049/1, t/22783/12 (a13o) | H |

**What players liked and disliked**

| Finding | Source | Conf. |
|---|---|---|
| Liked: prestige as "XP for your build" and a score to climb; footfall as a reward for useful places (shops, hubs, malls) | PR1, t/18411 | H |
| Disliked: losing settlement identity when a neighbour merged in, and a stranger becoming warden; the cause of guild settlements | t/24085/1, t/22895, t/34049 | H |
| Disliked: "prestige bombing" (refined gleam towers, walls of packed high blocks) to take capitals; capitals held by old hub owners; capital names that meant nothing to visitors | PR1, P218, t/17032, t/22783 | H |
| Disliked: the formula being secret and nonlinear ("don't look at prestige until the build is done"); unexplained drops | t/37218, t/49287, t/45420 | H |
| Disliked: footfall swings and its 2018 to 2019 bugs (24 days instead of 24 hours; reset by server restarts) | FF, t/22293 | H |
| Gamed: one-plot empty beacons in cities (closed by the 10,000 rule), strings of beacons along roads and slides to collect from many owners, "footfall farms" where players walked each other's plots (a terms of service breach), alts walked through shops | R211, t/30582/1, t/60537, FF #133 | H |
| Disliked: the road rule breaking bridges and roads, needing extra plots or a separate aligned beacon | t/25328/54 | H |
| Asked for: opt-out of settlement merging, footfall that needs upkeep, a guide to the best builds instead of a warden | t/39495, t/30582/1, t/22783/19 | M |

## D. Fairness and cost of computing

| Finding | Source | Conf. |
|---|---|---|
| Prestige is kept per beacon as histograms (bucket counts, chisel-shape counts, colour counts, built against air), so a block change updates a few counts and the score is recomputed from the histograms; order-independent, synchronous (inferred from "histograms", "order is not important", "effectively synchronous") | P218, PR2 | M (inferred) |
| Settlements are recomputed in the background over many server frames, because a full recompute is too expensive in real time; renames and footfall during a rebuild are applied to the in-progress result | t/23287/226, SET | H |
| Plot-column resolution cost 4x the chunk version; spreading it over frames kept server load unchanged | SET | H |
| Footfall needs only, per (owner, settlement, visitor): the time of the last payment; and per owner and settlement, a count of payments in the last 24 hours; saved with the world so restarts do not reset it | R211, FF #216 | H |
| Fairness levers Boundless found it needed: per-beacon minimum (10,000), once per owner per settlement, no pay from anyone with permissions, cooldown by payment not visit, diminishing traffic steps, compactness | R211, R242 | H |

## Not found

- **The hidden real prestige per bucket** and the curves (bucket spread penalty, weighting, density, colour, chisel): never published; removed from the debug view. Looked in: release notes 191 to 246, t/27485, t/21311, t/34403, t/39950, t/44073, the wiki, pfiffel's and mayumi's data (mayumi's item msgpack has no prestige field; its keys include `coinValue`, not prestige), boundlexx (item `prestige` field exists but `/items/` and `/blocks/` return server errors).
- **Exact footfall steps for recent traffic** and the owner-absence decline: only players' measurements. The players' spreadsheet (Google Sheets `1zb_qsNqc6Twyv-KIQcSuJcx7Fr6p31SKmhrD_RZtixU`) is blocked from the sandbox (403).
- **Release 249 or later footfall or prestige changes:** none found in searches of dev posts after 2020-03.
- Buckets for Release 233+ blocks (bordered and frieze marble, faceted crystal, framed and ornate glass, circuit, filigree and grid metal, panes, the 15 storage kinds): not in pfiffel's data; boundlessinfo's JSON files carry no prestige.
