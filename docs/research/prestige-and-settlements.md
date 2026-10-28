# Prestige, settlements and footfall in Boundless (research)

*2026-10-28. Facts only (numbers, formulas, rules); names and text are Boundless's and are not to be copied into the game. Inferences are marked "inferred". Work in progress: sections B to D are being filled.*

## Sources

| Key | Source |
|---|---|
| R191 | Forum t/13087, "Testing 191" release notes (james, 2018-04-24) |
| R211 | Forum t/24546, "Release 211" (guilds) release notes (james, 2019-01) |
| R218 | Forum t/28046, "Testing 218" release notes (james, 2019-03-28): the prestige rework |
| R242 | Forum t/44553, "Release 242: Beacon Reclaims and Compactness" (james, 2020-04-23) |
| wikiS | Wiki page Settlement (MediaWiki API; frozen around 2020; its "how prestige works" text describes the pre-218 system) |
| wikiF | Wiki page Footfall |
| wikiB | Wiki page Beacon (version 244) |
| pf | pfiffel.com/bl/blocks/dataBlockStats.js: per-block `prestige`, `buildXP`, `mineXP` from the game files (446 block kinds; predates Release 233's building blocks) |
| bx | boundlexx API schema (`/api/v1/schema/`): fields the final game exposed (item `prestige`, beacon `prestige`, `compactness`, `num_plots`, settlement `level`) |

## A. Prestige

### A1. What counts

| Finding | Source | Conf. |
|---|---|---|
| Prestige is computed per beacon from the blocks inside its plots. A settlement's prestige is the sum of its beacons'; a guild's is the sum of its aligned and controlled beacons' | wikiS, R211 | H |
| Since Release 218 every block counts, including soil, rock, foliage and trunks, and every mesh (machines, doors, poles, beams, torches, storage, flowers). Only air, mantle (bedrock), water and lava give none | R218 | H |
| Release 191: open portal conduits count toward a beacon's prestige | R191 | H |
| Before 218, natural blocks gave nothing and only placed blocks counted (the old system "calculated prestige as the sum of all block prestige for placed blocks") | R218 | H |
| Seams (ore and gem veins in the rock), meteorites, creature-dropped decoration and many hanging decorations have 0 in the data | pf | H |

### A2. The per-block number in the data

| Finding | Source | Conf. |
|---|---|---|
| Each block kind carries one integer `prestige` in the game data. In the post-218 data the values run 0 to 26 (natural blocks 1; refined rock 4; bricks 9; decorative rock and wood 12; compact metals 16 to 22; machined titanium 24; decorative gems 22; tier 7 decorative 26; guild buff machines 23 to 26) | pf | H |
| Before 218 the wiki listed base values that ran to 150 (power coils 150 and 100, decorative gems 140, compact gems 100, spark generator 80, machined titanium 80, lantern 15, torch 5). Those are the pre-218 numbers and are not on the same scale as the data's 0 to 26 | wikiS | H (that they differ); M (which version each is) |
| A player who reads the data (Pfiffel, 2019-08) believed the post-218 number is a **bucket** (a rank) that the game maps to the real prestige, not prestige itself; the effective prestige of one block also varies with the beacon's mix | forum t/34403 (search blurb) | M |
| The final game's item records still carried a `prestige` integer (boundlexx item schema) | bx | H |

### A3. How the total is formed

| Finding | Source | Conf. |
|---|---|---|
| **Old system (to Release 217):** sum of the base prestige of placed blocks, then a percentage bonus for variety of blocks, other-world colours, chiselling and the share of built against unbuilt space. Wiki's reading of the files: colours from other worlds cap at x1.3 at 30% of blocks; chiselled blocks cap at x1.2 at 25% chiselled; built space caps at x1.5 when 10% or less of the beacon is built; all together capped at x2.5 | wikiS | M (a player's reading of files) |
| The old system was abused (a solid block of high-prestige blocks gave huge prestige) and could go down when a block was added, because the percentage bonuses shifted | R218 | H |
| **New system (Release 218 on):** prestige never decreases when a block is added, a block is chiselled or a plot is added. Spamming one *category* of block gives less and less, down to 0; evening out the mix later lets those blocks count again | R218 | H |
| The bonuses are no longer a percentage of the base; they are extra prestige added on their own, under the same never-decreasing rule | R218 | H |
| The chisel bonus takes the **variety of chisel shapes** into account: the maximum needs a decent spread of shapes, and one shape repeated gives little | R218 | H |
| Block values were rebalanced so that each is proportional to the effort of collecting and producing it | R218 | H |
| The other-world colour bonus: Release 218 fixed metal doors counting as other-worldly colours, so the colour bonus survived 218 in some form | R218 | M |
| A beacon that is uncompact (score below 0 on a -100 to 100 scale) gets **0 prestige** until fixed: no footfall, nothing to guild, settlement or world | R242 | H |

## Not found (so far)

- The bucket to prestige mapping, the per-category diminishing curve and the variety and chisel bonus formulas: Wonderstruck never published them (players called it "the secret formula").
