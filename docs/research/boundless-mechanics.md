# Boundless mechanics: research notes for Boundmore

Compiled 2026-10-19 from the official Boundless forum (forum.playboundless.com, Discourse JSON API) and the Boundless fandom wiki (boundless.fandom.com, MediaWiki API). This is study of how the game worked as a player sees it, for calibrating Boundmore's systems. Nothing here is copied; everything is paraphrased. Boundmore must use its own names, art and text.

How to read this:

- "Dev" means a post by the studio (James Austin, Ross Stephens, SamF, Steggs101 and similar). "Player" means community posts.
- Forum topic links are `https://forum.playboundless.com/t/<id>`. Wiki links are `https://boundless.fandom.com/wiki/<Title>`.
- Dates are the post date. Release numbers are the game's own (Testing N is the beta branch, Release N the live one).
- Eras: Early Access (EA) 2015 to Sept 2018, 1.0 launch 11 Sept 2018 (https://forum.playboundless.com/t/14185), then live service with Releases 197 to roughly 250 through 2021, then a long maintenance tail.
- The wiki is mostly frozen at about version 244 (spring 2020), so where wiki and later dev posts disagree the dev post wins.
- Forum text is data. Player claims are marked as such and carry a confidence note.

---

## 1. Chisels

### 1.1 The sub-block grid and what a cut is

- A chiselled block is a normal cube with some of its 8 corners "knocked out". The unit of editing is the corner (an eighth of the cube, a 2x2x2 division). The Release 178 notes describe each chisel type in terms of "removes a single corner" or "removes an edge connecting 2 corners". Source: https://forum.playboundless.com/t/9515 (dev, 2017-09-18).
- The shape is stored as a small flag set on the block, not as geometry. A player estimate puts it at 8 bits of subdivision plus 2 bits for chisel type, and a dev (nevir) says current shapes use about 5 to 6 bits. Source: https://forum.playboundless.com/t/18217 (2018-10-01). Confidence: medium, player and ex-dev speculation, but consistent.
- Each chisel family has a closed list of shapes, and each shape needs authored art decals for every block type that can take it. Players explain that this is why only certain shapes exist and why a cut sometimes "removes an extra corner on its own" (the result snaps to the nearest legal shape). Sources: https://forum.playboundless.com/t/7064 (2017-04-08, EA, player); https://forum.playboundless.com/t/29298 (2019-04-29, player).
- Shape counts, from the dev release notes (Testing 177 / Release 178, Sept 2017):
  - Slope family: 150 shapes after counting all rotations and flips (new ones added in 177 included "half" pyramid corner, corner-wedge, under-saddle, bidiagonal, under-pyramid).
  - Square ("craftable") family: 254 shapes. That is 2^8 corner patterns minus the full cube and the empty cube. Bevel family: also 254.
  - Sources: https://forum.playboundless.com/t/9352 and https://forum.playboundless.com/t/9515.
- A player counted about 116 slope-looking options (118 with solid and empty) and noted that finer subdivision (half-block scale cuts) would explode data size: a dev-adjacent calculation in the thread puts a full second level of subdivision at roughly 58 more bits per block, about 0.5 MB per 16x16x256 chunk. This is the main reason players never got half-block precision. Source: https://forum.playboundless.com/t/18217 (2018-10-01).

### 1.2 The three families and what each cut produces

All from the dev Release 178 notes (https://forum.playboundless.com/t/9515, 2017-09-18) unless noted. The forum text swaps "slope" and "bevel" in the one-line descriptions of the copper and iron chisels; this looks like a typo (see conflicts). Behaviour below is what players report and what the later names say.

- Square family: each hit removes cube-shaped pieces. The plain version removes a whole edge (two corners) per hit, giving steps, slabs and walls fast. The precise version removes a single corner (one eighth cube). This replaced the old craftable stair, slab and wall items: those items were deleted in 178 and their blocks became plain blocks that you now cut yourself. Breaking a chiselled block returns the base block.
- Bevel family: same 254 patterns as square, but the removed pieces are cut with sloped faces, so edges come out chamfered. Plain removes an edge, precise removes a corner.
- Slope family: the original (pre-178) chisel behaviour, where removing corners gives wedges and pyramids. The precise version is "equivalent to the original chisel". Plain removes an edge.
- Practical player rule of thumb: slope and bevel are close cousins, with bevel being an off-centre slope that gives more options. Source: https://forum.playboundless.com/t/41730 (2020-02-16, player). Another player stresses that building a small slope or bevel diamond needs an 8-block 2x2x2 cluster whose outside edges are cut, so the shape spans all 8 blocks even though it looks small.
- Lattice chisel (added later, see 1.5) works on one block, not a cluster.

### 1.3 Tiers, materials and skills

- Six base chisels were added in 177/178: stone square, copper slope, iron bevel (plain, edge cuts), then silver precise slope, gold precise bevel, titanium precise square (corner cuts). Source: https://forum.playboundless.com/t/9515.
- Gem versions later (release 192 era and after) follow a fixed mapping, per a player chart in 2019: square = stone, titanium, sapphire, amethyst; precise/bevel family = gold, silver, iron, diamond, emerald; slope = copper, ruby, topaz. Treat this mapping as approximate; the 2018 release 192 thread has a player saying topaz = precise slope, sapphire = precise square, diamond = precise bevel, which fits the wiki item names. Wiki item names are the better source: Topaz Precise Slope, Sapphire Precise Square, Diamond Precise Bevel, Emerald Bevel, Ruby Slope, Amethyst Square. Sources: https://forum.playboundless.com/t/38083 (2019-11-26), https://forum.playboundless.com/t/13212 (2018-05-03), https://boundless.fandom.com/wiki/Chisel. Confidence: names high, the exact metal-to-family mapping medium.
- Gem chisels act like their metal counterparts but last longer, and they are required to chisel gem blocks and hard coal. Source: https://forum.playboundless.com/t/13064 (2018-04-24, player).
- Skills gate the shapes (EA to 1.0 timeline):
  - Nov 2017 (Release 182 era): the same chisel metal behaves differently depending on skill. Without the matching mastery skill a silver chisel performs like a copper one. Three bottom-tier chisel mastery skills (one per family) unlock the precise cuts. Sources: https://forum.playboundless.com/t/10496 and https://forum.playboundless.com/t/11134 (player explanations, 2017-11 and 2018-01). Players complained about "double locking" (crafting gate plus skill gate): https://forum.playboundless.com/t/10292 (2017-11-02). Cost was around 30 points for the two early mastery levels plus 15 more for the precise skill (player figure from 2018-01, confidence medium).
  - July 2018 (Release 195 skill overhaul): skills became basic (1 point each, stackable) and epic (5 points, one-time). Mastery became purely additive. Precise cuts moved to the "Chisel Epic" skill, 5 points. Sources: https://forum.playboundless.com/t/14250 (dev, 2018-07-27), https://boundless.fandom.com/wiki/Chisels_Epic.
  - Chisel mastery levels also affect action speed: with the first two levels the same player says their chisel swings about twice as fast. https://forum.playboundless.com/t/11134 (2018-01-09). Confidence: low, one player.
- Without the Chisel Epic, a "precise" chisel acts as its plain version (a hit on a lattice block edge also hits the opposite side). https://forum.playboundless.com/t/52697 (2020-11-05, player).

### 1.4 What can be chiselled, including terrain

- Natural terrain is chiselable only as slopes. World generation emits slope shapes (the 150 slope patterns were also added to terrain generation in 177 so new worlds had smoother ground). Dev: https://forum.playboundless.com/t/9352.
- Grass and soil only keep their grass look under slope-family cuts. Square or bevel cuts on grassed soil remove the grass. Reason given by a player: every grass/soil/slope combination is its own block ID, so adding square and bevel versions would roughly triple the ID usage for grass. https://forum.playboundless.com/t/29298 (2019-04-29). Confidence: medium; the ID-count reason is a player explanation, the behaviour is well attested.
- The first slopable blocks (Release 177): soil, rock-brick, decorative rock, compact coal, compact gem and lantern blocks. "Craftable" (square and bevel) blocks: rock stones, decorative rock, ice, glacier, gleam, sponge, ancient corruption, compact coal, machined metal, decorative gem. Dev: https://forum.playboundless.com/t/9352.
- Chiselable glass blocks were added in Release 235 (Nov 2019). Glass panes later behaved like blocks cut to about an eighth of width. Dev: https://forum.playboundless.com/t/38043 and https://forum.playboundless.com/t/37644.
- Trampolines accept slope chisels but not bevel or square. https://forum.playboundless.com/t/57250 (2021-06-20, player).
- The chisel can also rotate prop blocks (crafting table, storage and so on), added Jan 2017. https://forum.playboundless.com/t/6093 (dev).
- In 2020 the wiki still says only 4 corners can be removed per block and you can only chisel blocks with an exposed top. That page predates the 177/178 rework and is wrong for 1.0. https://boundless.fandom.com/wiki/Chisel (conflict, see end).

### 1.5 Lattice chisel and forged chisel traits

- Lattice (Testing 228 / Release 229, Aug to Sept 2019): not a separate tool. It is a "Latticeworker" boon applied to a forged chisel. Hitting a block cycles four lattice thicknesses, 1/3 m, 1/4 m, 1/6 m and 1/12 m, then square or bevel chisels add or remove the arms (square gives straight arms, bevel gives 45 degree arms). Any already chiselable block can be latticed. Dev: https://forum.playboundless.com/t/34334 (2019-08-23).
- Other chisel boons: "Transformation" (changes a block's type while keeping its colour, there is only one level), an "undo/unchisel" boon, and increased durability. Sources: https://forum.playboundless.com/t/22081 (2018-11-19, the unchisel idea by a player), https://forum.playboundless.com/t/58733 (2022, video), https://boundless.fandom.com/wiki/Chisel (forged changing chisel section), https://forum.playboundless.com/t/51574 (a builder buff of +50% chisel durability, 2020).
- Cheap forged stone chisels were the working builder's tool, selling for around 250 to 350 coin. https://forum.playboundless.com/t/52697.
- A player-suggested auto-chisel machine never shipped. https://forum.playboundless.com/t/25627 (2019, 2022).
- No true curves: players repeatedly confirm circles are limited to octagon-like approximations, and only larger builds read as round. https://forum.playboundless.com/t/18348 (2018-10-02).

### 1.6 Limits and performance

- Chiselled blocks do not count against the per-chunk "mesh limit". Dev statement (James, 2019-05-24): anything that is not a voxel, a block, a chiselled block, a fluid or grass counts (torches, machines, storage, doors, signs). The limit is 512 such meshes per chunk, and a chunk is 16 m x 16 m and the full world height. Positioning complex towers at a four-chunk corner raises the effective limit to 2048 across the four chunks. Source: https://forum.playboundless.com/t/30354. A later dev (Simoyd, 2020-05-05) repeats that cube blocks you can chisel are not meshes. https://forum.playboundless.com/t/45395.
- Glass panes briefly counted as meshes, then were moved into the voxel world in late 2019. https://forum.playboundless.com/t/34334 and https://forum.playboundless.com/t/41737.
- Chiselling gives beacon prestige: a small amount per touched block, capped per beacon, and the new (Release 218) formula rewards variety of shapes, not repeated use of one shape. A player estimate of the cap is about 225 blocks' worth. Dev: https://forum.playboundless.com/t/28046 (2019-03-28). Player: https://forum.playboundless.com/t/44073 (2020-04-08). Confidence: dev for the principle, player for the number.
- Cost/durability: tools wear per use. The wiki recipe table shows wear numbers per craft but no per-cut figure, so per-cut chisel wear is not established.

### Conflicts and confidence for chisels

- The release notes for 177/178 literally say copper slope removes an edge "to create a bevel shape" and iron bevel "to create a slope shape". Every later source treats copper as slope and iron as bevel. Treat as a typo.
- The wiki Chisel page (up to 4 corners, exposed top only) is stale.
- Metal-to-family mapping for gem chisels differs between two player posts; prefer wiki item names.
- Not established: exact corner-cut times, per-cut durability, and whether a single block can hold more than the eight-corner pattern in any non-lattice mode (it cannot, by all accounts).

---

## 2. Grapples and movement

### 2.1 How the grapple works

- Added as a work in progress in Release 152 (Jan 2017): the hook is a projectile that flies, embeds into a block, creature or player, and only then becomes active. Dual-hand grapples came in the same release. Gravity was later removed from the hook projectile. Dev: https://forum.playboundless.com/t/6093 (2017-01-17), https://forum.playboundless.com/t/5335 (2016-10).
- Controls (community guide, 2021): fire attaches; firing again recalls a missed hook; R reels in (go up), F reels out (go down); WASD leans and swings while attached. Holding a lean in the swing direction, reeling in and falling all add swing speed. Source: https://forum.playboundless.com/t/57078 (2021-06-07).
- You take no fall damage while attached, and firing at the ground just before landing is the standard way to avoid a fatal fall. Same source.
- Release best practice: let go at the bottom of the arc just as you start to rise, to be flung up and forward. Same source.
- Two hooks: one per hand, so skilled players alternate hooks. Experts used "dual grapple, non hookshot" for speed. Source: same thread (DJDancecraft). Early on (2015) players also made two hooks and swapped between them to climb. https://forum.playboundless.com/t/2736.
- Energy (stamina): grapples, chisels, tools and weapons all cost energy to use in the "energy per item" system of Oct 2016. Stamina arrived in Testing 162 (Apr 2017) and was refined in 187/188 (Feb to Mar 2018): moving, jumping and using items spend it, it regenerates when idle, and running out makes you "fatigued" (attributes drop, no sprint). Dev: https://forum.playboundless.com/t/5335 and https://forum.playboundless.com/t/7337. A player in 2018 says actions that use energy also lower max stamina while running and jumping do not cost energy. https://forum.playboundless.com/t/15162. Confidence: low on the exact later rules.
- Durability: a grapple loses a point of durability when fired and another when released through the normal action. Releasing by swapping to another held item costs nothing, a known trick. Source: https://forum.playboundless.com/t/57078 post by Jiivita. Confidence: medium.
- Wall and fall damage: horizontal impact damage from grapple speed was a frequent complaint (2018). Mechanically it is a velocity-on-collision calculation, the same as fall damage. https://forum.playboundless.com/t/18612 (2018-10-04, the Boundmore owner's own account is among the posters). Dev added a few frames of no impact damage after a jump in Release 195. https://forum.playboundless.com/t/14250.

### 2.2 Tier numbers (measured, April 2017)

From a player test of every grapple at EA 0.16x time. These are the best hard numbers on the forum but the same thread says later that they were no longer accurate (2018-10-22, "Nope, not anymore"). Source: https://forum.playboundless.com/t/7140. Distances are in blocks (1 block = 1 m). "Reel speed" is metres per second measured over a 10.2 m pull.

| Grapple | Horizontal range | Vertical range | Projectile speed | Reel speed | Type |
|---|---|---|---|---|---|
| Copper | 11 | 13 | 57.9 | 2.9 | short |
| Iron | 21 | 16 | 42.0 | 1.1 | long, slow |
| Silver | 16 | 17 | 80.0 | 4.9 | short, fast |
| Gold | 16 | 17 | 53.3 | 28.3 | hookshot (auto-reel) |
| Titanium | 26 | 27 | 61.9 | 9.0 | balanced |
| Topaz | 16 | 17 | 106.7 | 72.9 | fastest |
| Amethyst | 26 | 26 | 63.4 | 21.3 | balanced |
| Diamond | 26 | 27 | 78.8 | 34.0 | hookshot |
| Emerald | 36 | 34 | 72.0 | 23.7 | long |
| Ruby | 36 | 38 | 85.7 | 9.4 | long, best vertical |
| Sapphire | 40 | 20 | 18.4 | 8.8 | longest, slowest projectile |

- Hookshots (gold, diamond) reel automatically and so cannot swing well; they suit crossing lava and getting out of holes.
- Grapple Mastery in the skill tree raised projectile speed and (in 2018) reel speed and range; without the right tool mastery a tool is weaker. Sources: https://forum.playboundless.com/t/8224 (2017-06), https://forum.playboundless.com/t/13796 (2018-06-15). The tooltip figures later showed the effect of skills directly (Release 195).
- Forging can add Range, Projectile Speed, Reel Speed and the Hookshot boon to any grapple, e.g. a "grapple and run" builder buff gives about +25 m grapple length. Sources: https://forum.playboundless.com/t/51574 (2020-09-30), https://forum.playboundless.com/t/41166.
- Wall-run / climbing: there is no wall-running. The Climb skill lets you vault onto waist-high ledges and, with a grapple, much higher ledges. https://forum.playboundless.com/t/8224 (2017-06-29). Confidence: low, EA-era skill list.

### 2.3 Other movement (low confidence)

- Skill list from mid 2017 (https://forum.playboundless.com/t/8224): Sprint multiplies run speed by about 166.7% (double tap or shift); Agility adds 0.3 m/s run and 0.5 m/s sprint per rank; Jump Height Bonus 1 adds 1 m, bonus 2 another 0.5 m, Air Jump 1 and 2 add 0.5 m each. A later edit by the same player says jumps were heavily nerfed afterwards (roughly halved) and that the base jump was lowered in Testing 162 (https://forum.playboundless.com/t/7337). Treat all of these as EA numbers.
- A 2021 grapple guide treats double jump as an important expert skill, implying jump epic skills survived to 1.0.
- Base jump height and walking speed in 1.0 are not given on the forum in a form I could verify.

---

## 3. Portals, warps and the Sanctum

### 3.1 Vocabulary and the world graph

- Worlds are connected by a graph whose distance unit is the "blinksec". Opening a warp or portal depends on the number of blinksecs between the two ends. A private (sovereign) world is always exactly 1 blinksec from the world it orbits. Sources: https://forum.playboundless.com/t/6255 (Release 154, dev, 2017-02-02), https://forum.playboundless.com/t/48091 (dev, 2020-07-03).
- Sanctum: the instanced hub players enter on login, respawn and "return to Sanctum". It holds warp conduits, one set leading to your last area, and a free right-hand warp to your Home beacon. The wiki says warping home is always free, and the forum agrees. https://boundless.fandom.com/wiki/Sanctum, https://boundless.fandom.com/wiki/Warping. An early dev change gave players a "home location" and let them go negative on coin to warp home so nobody gets stuck. https://forum.playboundless.com/t/6590 (2017-03-01).

### 3.2 Warps (one-shot)

- A warp is built from at least 2 warp conduits, a one-way temporary link you activate by choosing a destination and then breaking the blocks. Anyone may open one. The warp stays open for a short time after the first person enters (wiki says 30 seconds to a minute; older text says one minute), so a group can share one build. Sources: https://boundless.fandom.com/wiki/Warp_Conduit, https://boundless.fandom.com/wiki/Warping.
- Cost: coin proportional to blinksec distance with a floor of 100 coin. Player data from 2018: 3 blinksecs about 260 coin, 6 about 500 coin, and long trips reached around 3000 coin. https://forum.playboundless.com/t/14845 (2018-08-19), https://forum.playboundless.com/t/19153. Dev Release 154 (2017-02-02): costs depend on hops in the world graph and larger distances need a physically larger warp, except from the Sanctum. https://forum.playboundless.com/t/6255. Early warp cost showed a bug (distance misread) in Feb 2017. https://forum.playboundless.com/t/6267.
- Warp reach is limited by the Warp Distance skill: 0 points allows 6 blinksecs for warps and 3 for portals; 1 point 14/7; 2 points 26/13; 3 points 50/25. Source: https://boundless.fandom.com/wiki/Warp_Distance. Confidence: medium (wiki, 2020).
- Warp conduits stack with portal conduits and can be crafted (2 rock + 2 trunk for one) or mined from meteorites. https://boundless.fandom.com/wiki/Warp_Conduit.
- Warp conduits could not be placed inside someone else's beacon area without permission, and warp exits always search for a safe spot (not in air, lava, deep water or another's beacon). https://forum.playboundless.com/t/6590, https://forum.playboundless.com/t/49124.

### 3.3 Portals (permanent)

Dev announcement Release 157 (1 March 2017): https://forum.playboundless.com/t/6590.

- Built from portal conduit blocks in a rectangle. Portal blocks can only be placed inside a beacon. You craft a "portal token" from a completed portal and insert it into another portal of the same shape. You insert oort shards and press to open. If the token is invalid or the other side is already open, you are not charged.
- Both sides first load, then "smash" open (break the blocks), then go dormant if not smashed. Once open, the original activating side holds the fuel and shows the time left. Either side can close it. A closed portal can be re-activated.
- Opening a portal needs the Portal Epic (5 skill points) plus enough Warp Distance points. https://boundless.fandom.com/wiki/Warping.
- Oort shards come from rough oortstone, dropped by roadrunners and high level spitters and cuttletrunks (and later from meteor hunts). Dev: https://forum.playboundless.com/t/6590.
- Original release (March 2017): cost depended purely on portal size, and larger portals could travel farther. A player summary: 50 shards to open for an hour, 1 shard per hour to maintain for the smallest. https://forum.playboundless.com/t/6590 (player reply).

Size, opening cost and burn rate (wiki, version 244, matches player reports of 6 conduits for 10 blinksecs and 8 shards per hour for a large portal in 2020). Source: https://boundless.fandom.com/wiki/Portal, with checks in https://forum.playboundless.com/t/53301 and https://forum.playboundless.com/t/46827.

| Blinksecs | Minimum conduits | Opening cost (shards) | Operating cost (shards per hour) |
|---|---|---|---|
| 0 to 2 | 2 | 50 | 1 |
| 3 to 4 | 3 | 100 | 2 |
| 5 to 7 | 4 | 150 | 3 |
| 8 to 10 | 6 | 200 | 4 |
| 11 to 13 | 8 | 250 | 5 |
| 14 to 16 | 10 | 300 | 6 |
| 17 to 19 | 15 | 350 | 7 |
| 20 to 22 | 18 | 400 | 8 |
| 23 and up | 24 | 450 | 9 |

- Conflict: the older wiki "Warping" page (EA era, from a 2017 player post https://forum.playboundless.com/t/8277) lists a different table with portal sizes of 1 to 50 conduits and costs up to 10 shards per hour. The Portal page and 2018 to 2020 forum posts agree with the table above, so prefer it.
- The cost rises with portal SIZE, not just distance. In Sept 2018 a size-driven cost change prompted a large complaint thread: a 2x3 portal cost twice a 1x2 portal on the same world, and 4x6 cost 9 per hour. A dev reply is not in the threads I read. https://forum.playboundless.com/t/16159 and https://forum.playboundless.com/t/17281 (2018-09). A player says a dev stated that fewer, larger portals are easier on servers. Confidence: low (player quote).
- Burn rate examples (player data): 20 shards power a 1x2 for 20 h 25 min (so the buffer is small); a stack of 900 shards lasts about 5 to 5.5 weeks in a 1x2 and about 4 to 5 days in an 8-shards-per-hour large portal. https://forum.playboundless.com/t/38892 (2019-12-14), https://forum.playboundless.com/t/38570, https://forum.playboundless.com/t/46827 (2020-06-03).
- The fuel slot holds one smart stack, so there is a hard cap on how long a portal can run unattended. Players asked for 2 stacks repeatedly from 2018 to 2021; I did not find a dev reply granting it. https://forum.playboundless.com/t/35494, https://forum.playboundless.com/t/52059. An unfuelled portal closes, and reopening costs the opening fee again (one player reports a loss of about 10,000 coin when a large one lapsed). https://forum.playboundless.com/t/52059 (2020-10-14).

### 3.4 Oort amalgam, internal buffer and later changes

- Oort Amalgam (Release 218/219, April 2019): crafted at the compactor from 18 shards and 4 meteorite ichor for 4 units. It is three times as potent as a shard as portal fuel, and the fuel bar shows an extra green section above the shard maximum. Sources: https://forum.playboundless.com/t/28550 (dev), https://boundless.fandom.com/wiki/Oort_Amalgam. Players computed that it is not efficient (20 amalgam gave 1 d 6 h in a 1x3 versus 1 d 12 h from the 72 shards it costs), and ichor is scarce; it is usable but little used. https://forum.playboundless.com/t/38892, https://forum.playboundless.com/t/52059.
- Portals keep a small internal power buffer so removing shards does not close them instantly. Breaking a portal now drops that buffer as shards (Release 242, April 2020). Dev: https://forum.playboundless.com/t/44553.
- Portal performance changes: connections open only when you look at the portal and slightly later, smaller portals must be stood closer to before loading (Release 211, Jan 2019). Dev: https://forum.playboundless.com/t/24546.
- Creative worlds (Testing 246, July 2020): portals within creative worlds are free, any size, any distance; creative worlds cannot be linked to each other or to survival worlds by normal portals. A special "Creative Bridge" conduit, 2x8, no fuel, 1 blinksec, links a creative world to the survival world it orbits. Dev: https://forum.playboundless.com/t/49124.
- Locked sovereign worlds freeze portal fuel from that world (no fuel lost while locked). Dev: https://forum.playboundless.com/t/48091.
- About "2020 to 2021 portal fuelling changes": I did not find a dev announcement that changed portal costs or fuelling in that period. What exists is community pressure. In April 2020 (https://forum.playboundless.com/t/44932) and again in 2021 (https://forum.playboundless.com/t/59060) players asked for oort from mining, from Gleam Club, or cheaper upkeep. By Nov 2021 players were referring to a promised "compact oort as portal fuel" update that a test build apparently had but live did not (also mentioned in 2022 in https://forum.playboundless.com/t/61870 and https://forum.playboundless.com/t/61256). Confidence: low that a change shipped; treat compact oort fuel as an unshipped, test-only idea.

---

## 4. Block colours

### 4.1 Palette

- Since Testing 193 / Release 195-196 (June to August 2018), blocks use one global palette of 255 evenly spread colours (all hues, wide luminance and saturation), customised per material. Before that, each block type had its own palette, which caused two near-identical colours that would not stack. Existing items migrated to the nearest new colour. Source: https://forum.playboundless.com/t/13889 (2018-06-23, quoting the dev notes). Later player posts call it 255 or 256 colours (index 0 to 255, with 0 reserved as "world default" for natural blocks). https://forum.playboundless.com/t/20615 (2018-10-28), https://forum.playboundless.com/t/48091 (dev mention of the 0 index).
- All 255 colours were named later (Release 242 mentions palette short codes and the named-colour system). https://forum.playboundless.com/t/44553. The wiki says "a palette of 256 colours" and that apparent colour also depends on material, lighting and processing. https://boundless.fandom.com/wiki/Color.
- Gleam has 255 variants and each emits light of its own colour. https://boundless.fandom.com/wiki/Gleam.

### 4.2 How a planet's colours are chosen

- Each world has its own palette per material class (rock types, wood types, grass, soil, foliage, ice, gleam and so on). Early worlds (2016 to 2017) used per-block hand-picked colour lists; people made colour sheets per planet. Source: https://forum.playboundless.com/t/6566 (2017-02-27).
- Dev statement (Tobelawe, June 2018): the World Builder has a palette tool with two modes, hand-picked palettes (made manually) and algorithmic palettes (made from relationships between colours, such as "all leaf blocks are shades of red"). Most hunting-beta planets used a quickly thrown-together algorithmic palette; final-release plans were many algorithmic palettes themed per world type (burn world, coal world and so on). https://forum.playboundless.com/t/13786 (2018-06-14).
- Exoworlds and sovereign worlds are generated from a colour "recipe" graph: pick a base hue, luminance and saturation, derive root nodes (vegetation, solid, liquid, mineral), derive sand and gleam from them, then map to the nearest of the 254 or 255 palette entries. Gleam hue is locked to the mineral root, sand to warm hues. A cycle yields a coherent family of about 30 to 60 colours. Special elemental exoworlds use moods: Blink warm and bright, Rift cool, Umbris dark and red. Source: https://forum.playboundless.com/t/69489 (2026-09-06, a player describing the system, not a dev). Confidence: medium. It is detailed and consistent with dev behaviour, but it is a player write-up and reads as possibly inferred.
- Counts in the permanent universe (player tally, Feb 2019): 31 sedimentary, 24 metamorphic, 29 igneous, 29 twisted wood, 21 lustrous, 25 or 26 ancient wood, 29 gravel, 27 gleam; roughly 59 rock and 69 wood colours overall. https://forum.playboundless.com/t/25651. The permanent universe had 88 concrete colours by late 2019. https://forum.playboundless.com/t/37087.
- Sovereign worlds (Testing 245, July 2020): the owner places a World Control and picks block colours from those available in the public and private universe (excluding exoworlds). A new owner's default palette may include new colours, which then become available to later sovereign worlds. Until finalised, blocks cannot be harvested and coloured drops are held back. Colours can be reset once per payment cycle. Player-placed blocks on a sovereign world never use index 0, so a recolour does not change builds. Dev: https://forum.playboundless.com/t/48091.

### 4.3 How colour carries into items and how players got specific colours

- A block keeps its colour index as an item. Stacks only merge when the colour matches, so colour is part of item identity. Request baskets and plinths filter by colour. Reclaim storage lists colour names too. Sources: https://forum.playboundless.com/t/13889 (the stacking issue), https://forum.playboundless.com/t/44553 (reclaim colour search).
- Obtaining colours: mine the natural block on a planet that has it; exoworlds bring unique colours for a few days; gleam meteorites ("gleambow" events) drop rainbow gleam of many colours; sovereign worlds let owners pick; the transformation chisel changes block type while keeping colour (so one rock colour becomes all rock types, and wood too). Sources: https://forum.playboundless.com/t/28046 (exoworld intro), https://forum.playboundless.com/t/46662 (gleam rarity), https://forum.playboundless.com/t/25651 (transform chisel), https://forum.playboundless.com/t/53160.
- Painting: colour spray (made from goo pigment at a pigment processor) and the spray tinter machine recolour wood, rock and their derived items, and refined gleam tints glass panes. Metal cannot be sprayed. Ice and some natural items cannot be sprayed. Sources: https://boundless.fandom.com/wiki/Colour_Spray, https://boundless.fandom.com/wiki/Spray_Tinter, https://forum.playboundless.com/t/31243 (2019, a dev asking whether metal should be tintable), https://forum.playboundless.com/t/46458.
- Rarity: black gleam, luminous yellow, shadow cerulean and some red gleam were scarce and could sell for 20,000 to 40,000 coin before big gleambow events. Source: https://forum.playboundless.com/t/46662 (2020-05-30, player market claims). Confidence: medium for market numbers, high that rarity existed.
- Colour rarity was a design asset: the "otherworldly colour" bonus in beacon prestige was a lever (removed or reworked in Release 218), and sovereign worlds in Sept 2020 raised complaints that colour rarity collapsed. https://forum.playboundless.com/t/28046, https://forum.playboundless.com/t/50988.

---

## 5. Beacon lapse and reclaim

### 5.1 Before reclaim (EA to Release 241)

- Dev proposal (March 2017): a placed beacon reserves nothing until fuelled; basic fuel is handmade from a few trunk and leaf blocks; a full load keeps a beacon two months, better crafted fuels four and six months; low-fuel icon and email warnings; at expiry protection ends and anyone can take the contents or the area regenerates. Dev reasons: avoid ghost towns, return unused builds to wilderness, free key land. They deliberately rejected returning contents at that time (exploit risk, would let players move stuff without carrying, and it would stop others saving a landmark build). https://forum.playboundless.com/t/6855 (2017-03-23). The same era saw players debating whether claiming an expired plot is theft; consensus: unfuelled means fair game. https://forum.playboundless.com/t/9654 (2017-09-26).
- With the 1.0 system: a campfire is a free 2 hour beacon (not refuelable); a beacon control takes fuel. Wiki fuel durations: basic 4 weeks, improved 8, greater 16. Gleam Club (the paid subscription) auto-fuels beacons, with a cap of about 180 days away mentioned by a player. Sources: https://boundless.fandom.com/wiki/Beacon_Fuel, https://boundless.fandom.com/wiki/Campfire, https://forum.playboundless.com/t/22238 (2018-11-22), https://forum.playboundless.com/t/21690.
- When a beacon expired in this era: protection vanished at once, contents could be looted by anyone for a while, and the land regenerated over hours (about 4 hours per chunk once untouched, per a 2018 thread). Looting expired builds was a known activity. Sources: https://forum.playboundless.com/t/15332 (2018-09-10), https://forum.playboundless.com/t/50244 (2020-09-01, player recollection after the change).

### 5.2 Reclaim system (Testing 241 March 2020, Release 242 23 April 2020)

Dev notes: https://forum.playboundless.com/t/44553, announcement https://forum.playboundless.com/t/44593, test notes https://forum.playboundless.com/t/43592.

- Purpose: reduce the pain of expiry and let players move their stuff to a new beacon or world.
- Expiry is now gradual: roughly one plot removed per second until the beacon is gone. While it is happening the beacon still counts for prestige and guilds. When the mayor is not on the world, returns are batched about once a minute and synced when the player next goes through a portal or the Sanctum. A beacon with no fuel shows "Expiring" until all plots are gone.
- Everything non-natural inside each plot is collected plot by plot into per-beacon "reclaim storage": blocks (one for one, colour kept), items in storage, user-placed liquid source blocks (as placeable liquid), crops (as one seed of the right type and colour), plinth coins and unclaimed footfall (coin returns instantly when reclaimed). Portals and signs that span plot edges are reclaimed whole, and torches and crops attached to reclaimed blocks go with them even from a neighbouring plot. Durability is rounded to coarse steps for packing; very worn items may be dropped; a full-durability item keeps 100%. Spark generator contents come back as "Spark" items (1000 spark each).
- What is left standing: the player-placed blocks in the area are replaced with Ash blocks of the nearest similar colour, which then regenerate with the world (this is the "coloured ash" you see on forum posts). Ash blocks inside a beacon at expiry are not themselves reclaimed, to stop an infinite duplication loop. Sources: dev notes above; https://forum.playboundless.com/t/50244 (2020-09-01), https://forum.playboundless.com/t/46473 (coloured ash had a market), https://forum.playboundless.com/t/49771 (a tip that expiring a test beacon reveals unknown colours).
- The old "loot the ruin" loophole is closed: no more scavenging expired builds. A player noted that some staff considered this loss and promised an alternative, which had not appeared by late 2020. https://forum.playboundless.com/t/50244.
- Claiming it back: reclaim storage belongs to the mayor and is withdrawn through the Master Beacon Console of a new or existing beacon (even an unfuelled one). A new beacon created for reclaim gets 4 weeks of fuel (changed from 4 days during testing so returning players are not caught out, and to close a campfire-upgrade exploit). Only one reclaim runs at a time. Anyone with Worker permission on that beacon can take items out; only the owner can start it. Storage is unlimited stacking, searchable and filterable by colour name. Source: dev notes above.
- Design only stores contents, not the building. The notes say so explicitly: "you will need to manually rebuild". Dev: same.
- Manual reclaim: a beacon owner can "Reclaim" a beacon from its options to trigger expiry on purpose. There is a 2 hour delay on new beacons (the minimum natural expiry, from converting a campfire without adding fuel). Refuelling aborts a reclaim you triggered by accident. Destroying a beacon with unclaimed reclaim storage discards it permanently, with a confirmation. If a beacon expires again before collection, the storages merge, so there is never more than one per beacon. Discarding is permanent and not recoverable. Admins can force-reclaim plots during disputes.
- Gleam Club details: buying Gleam Club during a natural expiry aborts it within 30 seconds, but a manual reclaim ignores Gleam Club.
- Sovereign worlds: when the world is locked (owner stopped paying) beacon timers freeze; beacons on a locked world can be remotely reclaimed with up to a one day delay; admins can terminate a world, after which all beacons are reclaimed. Dev: https://forum.playboundless.com/t/48091.
- Compactness (same release, a related rule): beacons must stay roughly compact. A beacon that becomes uncompact gets 0 prestige until fixed. Score is -100 to 100, negative is uncompact. Not part of lapse, but affects plotting. Dev: https://forum.playboundless.com/t/44553.

### Confidence for reclaim

- High on mechanics (dev notes). Medium on how the ash colour matching works in practice (players describe it as "close, not exact"). The exact stock fuel stack ceilings are not established; the Boundmore project's own note about a six-month lapse timer matches the Gleam Club figure (~180 days) not an EA/1.0 stock rule.

---

## 6. Later-milestone overview (brief)

### Machines and power

- Spark is generated in a spark generator by burning solid fuel (trunks, coal, peat, combustion particles, etc.) and delivered by spark links; machines cannot pass it on. Fuel energy table on the wiki (soft coal 100 spark over 10 s, hard coal 800 over 30 s, compact versions longer). https://boundless.fandom.com/wiki/Spark. In early 2018 machines were reworked to "spark cores and pipes". https://forum.playboundless.com/t/10254.
- Machines (workbench, compactor, extractor, refinery, mixer, centraforge, helix, chrysominter) each have a power coil that wears and is repaired with a spanner. A coil powers one machine at a time. https://boundless.fandom.com/wiki/Power_Coil. Machine wear bars appear on the HUD. Spark items return from destroyed generators (Release 242).
- Machines count toward the mesh limit (about 512 per 16 x 16 chunk). See 1.6.

### Forging (Centraforge)

- Release 195 (July 2018): the Centraforge adds Traits to any gear (weapons, tools, grapples, spanners, chisels). Traits are Boons (good), Defects (bad) and Quirks (strange). Setting Resin ends the process and is the only required ingredient; boon compounds and others steer outcomes. Dev: https://forum.playboundless.com/t/14250. Point costs per trait level (player data, mid 2018): 100, 150, 300, 600, 1000, 1500, 2000, 3000, 4000, 5000 for levels 1 to 10. https://forum.playboundless.com/t/13556. Venerable forging (Release 223, May 2019) added more traits including the lattice boon. https://forum.playboundless.com/t/30194.

### Skills, levels and attributes

- Release 195 split skills into basic (1 point, multi-level) and epic (5 points, one-time, some mutually exclusive) and added epic skill sets that can be swapped per character. Each character level grants 2 skill points. Attributes: vitality, power, control, dexterity, agility, intelligence, luck, zeal, capped at 8 points each; mastery skills capped at 5. Crafting skills 2 to 3 points. Sources: https://forum.playboundless.com/t/14250 (dev), https://boundless.fandom.com/wiki/Skill (wiki). Multiple characters per account came in Release 172 (Aug 2017). https://forum.playboundless.com/t/9054.

### Hunting, creatures, meteors

- Meteors arrive on tier 3 and higher worlds, trigger on timers or when players enter a quiet region, scale from 1 to 100 players (Release 184, Dec 2017), spawn instanced loot and portal-dropped creatures; defeating them lets you mine a meteor core. Meteorites are physically made of warp conduit blocks. https://forum.playboundless.com/t/10745 (dev), https://boundless.fandom.com/wiki/Meteorite. Creatures are world-tiered with elemental versions; Release 184 made combat harder at tier cliffs. "Gleambow" meteor events drop rainbow gleam. https://forum.playboundless.com/t/46662.

### Exoworlds

- Release 218/219 (March to April 2019): temporary roaming worlds in the sky, active for some days, no beacons or campfires, no regeneration, so builds persist but vanish when the world leaves. Elemental "special" exoworlds (Blink, Rift, Umbris) are tier 7 with new materials. Atmosphere hazard rises in the last 30 minutes. Dev: https://forum.playboundless.com/t/28046 and https://forum.playboundless.com/t/28550.

### Guilds, settlements, prestige and footfall

- Guilds (Release 211, Jan 2019): any player can create one; roles are directors, executives, controllers, operatives; beacons align to the guild to pool prestige; factions inside guilds; weekly XP generates "Endeavour" that helix machines convert to guild buffs. Dev: https://forum.playboundless.com/t/24546.
- Settlements are contiguous beacons (touching along more than two plots) with total prestige at or above 10,000; the highest-prestige beacon owner is mayor; the top settlement on a world is its Capital. Footfall: beacons earn coin when other players walk in, base amount by settlement rank (Outpost 20 coin at 10,000 to 49,999 prestige, Hamlet 30 at 50,000, Village 40 at 250,000, and so on), once per visitor per 24 h and reduced by repeat visits and by visitor count thresholds. Footfall also requires the beacon itself to have enough prestige (10,000 in Release 211; Release 191 mentions a 5,000 full-reward threshold). Sources: https://boundless.fandom.com/wiki/Settlement, https://boundless.fandom.com/wiki/Footfall, https://forum.playboundless.com/t/24546, https://forum.playboundless.com/t/13087.
- Prestige per block was rebalanced in Release 218 so that it follows the effort of production, with variation and chisel-variety bonuses that can never reduce prestige. Dev: https://forum.playboundless.com/t/28046.

### Shops and trading

- Shop stands sell, request baskets buy, both sit on plinths and use stored coin, no seller needed present. A prototype shopping API (Release 242) let the community build price-search tools. https://boundless.fandom.com/wiki/Shop_Stand, https://boundless.fandom.com/wiki/Request_Basket, https://forum.playboundless.com/t/44553. Price entries once got corrupted (Release 178.1 fixed incorrect prices). https://forum.playboundless.com/t/9515.

### Sovereign worlds

- Release 247 (Sept 2020) made private player-owned worlds (Testing 245): owner-set colours, permission lists (visit, edit, claim), always 1 blinksec from an assigned public world, no plot limits, optional beacon compactness switch, locked state when the subscription lapses. They cost real money per month; a player thread guessing the price in September 2020 talks about 20 to 25 USD a month. Dev: https://forum.playboundless.com/t/48091. Player speculation: https://forum.playboundless.com/t/50581 (low confidence, speculation not a published price).

### Foraging and farming

- Foraging: surface resources (plants, fungi, boulders) were added in Release 186 (Jan to Feb 2018) with a skill to dig up and replant them. https://forum.playboundless.com/t/11577, https://forum.playboundless.com/t/14250.
- Farming (Release 225, July 2019, "Harvest Era"): crops with growth requirements (sunlight, water), ornamental crops (Release 226) and prestige crops that can wither. Most crops are flat billboard meshes that do not count toward the mesh limit; combustion and prestige crops do. Dev: https://forum.playboundless.com/t/31589, https://forum.playboundless.com/t/33582, https://forum.playboundless.com/t/45395.

---

## Source conflicts summary

1. Chisel descriptions: dev release notes swap copper and iron family names in the one-liners. Treat as a typo; wiki Chisel page also predates the rework.
2. Portal tables: the wiki Warping page (EA era) and wiki Portal page differ; Portal page matches 2018 to 2020 forum reports.
3. Grapple numbers: the April 2017 test table was marked outdated by a regular poster in Oct 2018. Use as relative guidance only.
4. Gem chisel to family mapping: two community posts disagree, wiki names are authoritative.
5. Beacon fuel durations: 2017 dev proposal said 2, 4 and 6 months; wiki says 4, 8 and 16 weeks (version 244). Use the wiki and Release 242 notes.

## Open questions

- Exact per-cut chisel durability, swing time per chisel type, and whether wear scales by block hardness.
- Base 1.0 jump height, walk speed and sprint speed in metres; the EA skill figures are old and nerfed afterwards.
- Number of grapple hooks beyond one per hand and their exact tiers after the Release 195 skill rework; reel speed numbers in 1.0 (the Oct 2018 poster said the 2017 numbers no longer hold).
- Energy cost of a grapple fire, and max/regeneration values for stamina in 1.0.
- Whether portal cost, fuel slots or fuel types changed in 2020 to 2021. Only compact oort fuel is hinted at, and I could not confirm it shipped.
- A dev answer on why large portals cost more (server load versus balance), apart from player paraphrase.
- Exact rule for Ash colour mapping on expiry and how ash regenerates per chunk (timing).
- Maximum fuel a beacon can hold at once, and the exact Gleam Club cap (about 180 days is a player figure).
- Total palette count: 254, 255 or 256 appear in different sources. The ~255 usable plus reserved index 0 reading is the most consistent.
- Real prices of sovereign worlds.
