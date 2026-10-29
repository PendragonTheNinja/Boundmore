# Audit: building and the world (2026-10-29)

Scope: placing and breaking for building, the block catalogue (M22), colour (M6), chisels (M7), claims and rights (M3, M25), lapse and ash, world generation (M19), far views (M20), light (M26), textures and icons, the atlas (M27), storage, and a builder's day. Read-only audit: no code was changed. Forum evidence is from the Discourse JSON API (topic ids given as t/NNNN). Anything marked **guess** is not checked.

## Summary

1. **Natural terrain is hard stair-steps.** Boundless's generator has placed slope shapes on terrain since release 177 (dev, t/9352); ours emits none (`bm-worldgen` never writes a `Shape`), although the roadmap promised "slopes in terrain too" under M7. This is the single biggest look difference on every planet (`docs/media/m19-verdance.jpg`: terraced hills).
2. **No creep (edge guard).** Boundless builders held the creep key to walk backwards off a ledge placing blocks without falling (t/26751: "You shouldn't fall unless you cross a chiseled block"). Ours has no edge guard; X only swims or flies down. Sky building is much harder than in Boundless.
3. **No block orientation.** Every cube faces one way; trunks cannot lie sideways and patterned blocks (carved, tiled, marble frieze) cannot be turned. Boundless had 4 placement orientations and players still begged for more (t/12751, t/15287, t/15128).
4. **Colour is narrow and has no painting.** Only 162 of the 255 palette colours appear on any permanent planet kind, a given universe sees far fewer, and there are no colour sprays, tinter or transformation chisel (M6 and M22 non-goals). Colour was Boundless's builder economy.
5. **The wild never heals.** Boundless regenerated unclaimed land and lapsed ash; ours keeps every hole, pillar and ash block forever (no regeneration anywhere in `bm-server`). Hubs will scar and lapsed builds stay as grey ruins.
6. **Item icons are the weakest art.** 32 px pixel-art with a hard 1 px outline (`tools/bm-texgen/src/icons.rs` `OUTLINE`), nearest-scaled to 64 and then linearly shrunk to 40 px in a 54 px slot (`menus.rs` `paint_stack`, inset 0.13), so pixels land unevenly and look soft. Blocks are painted with soft ramps; the two styles clash.
7. **Lighting is flat white sky light.** The terrain shader uses sky light as a grey scalar (`terrain.wgsl` `shade`: `sun_color.w` only), no warm sun or blue night tint, no glow or bloom on emitters. Boundless's look leaned heavily on tinted atmosphere and glowing gleam.
8. **Building quality of life we lack:** a plot view that hides clutter and draws plot lines strongly, drag-to-claim many plots, storage sort and "deposit all matching", Boundless's smart stacks.
9. **Where we already beat Boundless:** no beacon fuel; 6-month lapse with full reclaim; chisel fill-back built in (Shift) where Boundless needed a forged boon; colour passes through crafting with a chosen colour; doors swing as joined groups; signs join into 10 by 10 panels; atlas heat maps; coloured light. Keep them.

## Where Boundless did it better

### Terrain looked smooth
- Boundless: "the 150 slope patterns were also added to terrain generation in 177 so new worlds had smoother ground" (dev, t/9352, in `docs/research/boundless-mechanics.md` 1.4).
- Ours: `crates/bm-worldgen/src/lib.rs` computes `slope_at` only to choose cover blocks (`surface_block`, `biome_cover`); no block gets a slope shape. Terraces (`land.terrace`, line ~1343) make it worse: biomes with terraces read as Minecraft-like staircases. ADR-0008 already makes slopes walk as half steps, so slopes in terrain need no physics change.
- Cost: a `GENERATOR_VERSION` bump (new planets only, old pins kept), the far-view halving (`bm-core/src/far.rs`) should drop shapes, and the lapse (`lapse::ash_for` compares against generated state, so it keeps working if the generator is the reference).

### Creep for building at edges
- Boundless's creep key stopped you walking off a block edge; holding it while walking backwards and placing was how floors and bridges were built in the sky (t/26751, 2019). Ours: `movement.rs` `descend` (line ~125) only swims or flies down; nothing stops a fall at an edge on land.
- This must live in `movement::step` (invariant 2) as an input bit, like `descend`.

### Block orientation
- Boundless: four orientations from the placing direction (t/12751: "Top-Down facing N/S, E/W; Sideways facing N/S, E/W"), still a top complaint because some patterns could not be reached. Ours: no orientation for cubes at all. `Shape` bits 12 to 15 are free for chisel shapes (bit 11 marks forms), so 3 to 4 bits of orientation fit without a new state field, but every mesher path (greedy cubes, shaped, far) and the texture face table must honour it. Chunk format bump.
- Trunks are the clearest case: Boundless builders laid logs sideways; ours always stand.

### Colour supply
- Boundless: colours from every world, exoworld colours, gleambow meteor gleam, sovereign world choices, colour sprays from goo pigment at a pigment processor plus the spray tinter, the transformation chisel (rock to rock, keeping colour) (mechanics 4). Sprays were a whole trade (t/42575, t/55733).
- Ours: per kind 23 to 42 colours per block (stone 38, soil 23, sand 30), only 162 of 255 used by any permanent kind (`assets/planets.toml`), each planet picking 2 to 4 per block by seed. Exos draw from the whole wheel but are temporary, which is exactly forum complaint 8 (rare colours locked to timed events).

### The world heals
- Boundless: unclaimed edits and lapsed ash regenerated over hours; regen bombs existed (t/24273, t/44177). Ours: ADR-0005 stores every changed chunk forever; ash (`lapse.rs`) stays forever. Over months the land round shrines, portals and meteor sites fills with holes and floating towers, and the save grows. Needs Nathan's decision (it shapes what players invest in).

### Plot view
- Boundless's build mode (B): plot outlines in a chosen colour, others' plots in other colours, snow hidden (t/23335, t/44226). Ours: claim mode draws outlines (`app.rs` `claim_outlines`, ~3015), one plot per click (`claim_click`, ~1961). No drag or area claiming, no "show plots while building" toggle without entering claim mode (**guess** that Boundless allowed drag plotting; check with Nathan).

### Storage and stacks
- Boundless had smart stacks (one slot held several stacks of one item; shift-click merged into matching smart stacks in storage, t/20119). Players still asked for sort and deposit-all buttons (t/35868, t/17706, t/38403). Ours: `bm-client/src/storage.rs` has Shift-click to the other side only; no sort, no deposit matching, no search, 40-slot inventory with 100 stacks.

### Leaves and glass
- Foliage textures are fully opaque (alpha 255 everywhere), so trees are solid green cubes. Boundless's foliage had cut-out edges (art direction rule 9 already plans it). Glass is fine (alpha 36 to 242).

## Where we can beat Boundless

Already better (keep and advertise):
- No fuel, 183-day lapse, reclaim at any shrine, ash keeps colour and chisel shape (`lapse::ash_for`).
- Chisel undo: Shift-fill restores cut octants (`bm_core::chisel::apply` `restore`), no forged "unchisel" boon needed.
- Crafting lets the player pick the output colour (`Recipe::tint`, craft window swatches); Boundless took whatever colour came first.
- Doors swing as one joined group (`game/doors.rs`), locks per claim.
- Rights: Builder, Worker, Engineer and Farmhand per friend and per guild (`game/rights.rs`), checked by one function.
- Atlas heat maps from the generator, fogged per region.

Fixable Boundless complaints:
- **Diagonal or "place against air" placement** (t/14013: hundreds of helper blocks placed and removed). A held "placement plane": while holding the place key, a ghost shows the cell in the aimed direction on the current floor plane (needs only the client choosing a cell plus the server's support rule relaxed to "a solid block within one cell, including edges"). Cheap and beloved.
- **3x3 placement** (t/26751 poll): a forged boon or skill that places a row of 3 from one click, as AoE tools break 3x3. Needs Nathan.
- **Rotation tool** (t/15287 "a wooden chisel that only rotates"): once orientation exists, a chisel hit with no cut rotates it.
- **Mesh limit:** Boundless capped 512 meshes per chunk (t/12780, at 1300 hours a builder hit it). Our forms live in `Shape` and are meshed as blocks; state plainly that there is no limit, and keep it so (load-test a chunk full of poles).
- **Snow over colours** (forum feedback "smaller but recurring"): we have no weather; when weather comes, make it a setting.
- **Colour availability:** guarantee each of the 255 colours on at least one permanent world per universe, per material family (rock, wood, soil, crystal), so rarity is distance not timing (forum principle 3).
- **Blueprints / schematics:** Nathan already said "a schematic that saves the build so it can be placed again is wanted if it proves feasible" (M3 spec). The lapse already walks a claim plot by plot; a "save claim as blueprint" item that records block states, then lets the owner place a ghost to fill from inventory, would be a first for the genre-likes of Boundless (t/25327, t/45386).

## Bugs and rough edges found in our code

| Where | What is wrong | How it shows in play | Confidence |
|---|---|---|---|
| `bm-client/src/app.rs` ~2553 to 2590 (place) vs `bm-server/src/game.rs` 2514 to 2523 | The client predicts a place without the server's "a solid face neighbour" rule or the claim rights check (`claims::may`) | Clicking a block against a plant, torch or in a friend's claim without Builder: the block appears, then vanishes a moment later with a refusal | High (logic read; rollback works) |
| `game.rs` 2489 | Placing needs the cell to be air or liquid, so a flower, fern or grass prop must be broken first | Building on a meadow means breaking every flower first; Boundless replaced foliage (**guess** on Boundless) | Medium |
| `game/rights.rs` `may_on` 21 to 43 | A guild member who is not a friend and lacks the right gets "This land belongs to X" instead of "has not given you Builder rights" | Confusing message for guild members | High |
| `bm-client/src/menus.rs` 123 to 130, 399 to 401; `icons.rs` `load_item_png` | 32 px icon art nearest-scaled to 64, then linear to about 40 px | Soft, uneven pixel sizes on every non-block icon; worse at Windows display scales of 125% and 150% | High |
| `terrain.wgsl` `shade` ~266 | Sky light is a scalar; `sun_color.xyz` unused for terrain | Dawn, dusk and night have no colour on the ground, only darker; planets' sky colours do not reach the land | High (read) |
| `bm-client/src/render.rs` ~1111 | Translucent chunks sorted far to near, faces inside a chunk not sorted | Glass behind water or glass behind glass in one chunk can blend in the wrong order | Medium |
| M22 known limit | Placing a pane or pole where the player stands is refused as a whole block | Cannot place a pane at your feet in a doorway | High (spec says so) |
| M22 known limit | Beams stop at their block edge, a gap where they meet a pole; no pole caps | Furniture looks unjoined | High |
| M22 known limit | Sign text drawn over the world with a coarse line-of-sight test | Signs show through panes and poles | High |
| M20 known limit | Far/near seams; torches give no far light | Small gaps at ring edges; settlements dark from afar at night | High |
| `lapse.rs` `ash_for` | Any edit differing from generated terrain turns to ash, including a chiselled natural hillside or a dug-and-refilled patch | Lapse leaves ash on landscaping the owner meant to look natural | Medium |
| `assets/blocks.toml` | Trunks and foliage are `chisel = none` | Cannot shape logs into posts or trim; Boundless allowed some natural chiselling (**guess**) | Low |
| M27 known limit | The atlas of another world shows no claims; heat maps show generated ore, not what is left | A mined-out area still glows on the heat map | High |

## Visual quality

What looks worst now, in order:
1. **Stair-step terrain and terraces** (`m19-verdance.jpg`): sloped terrain shapes would change every first impression.
2. **Item icons** (`docs/media/item-icons.png`): hard outlines, flat pixel art, blurry scaling, next to painterly blocks. Lift: paint icons at 64 px natively with the block ramps and soft edges (no outline, or a soft dark halo), show them at an integer scale (32 or 64 at the UI scale), and use linear filtering only when downscaling from 64+. Tools of each tier are recolours of one sprite; give the top tiers (gems, astral) distinct heads.
3. **Lighting**: tint sky light by `sun_color.xyz` and the planet's `look` colours, warm by day, blue by night; add a cheap bloom or emissive boost for `emission` blocks (lume, lava, lanterns). Boundless's worlds glowed.
4. **Close-up turf** reads as per-pixel noise at a few blocks (the screenshot's foreground), against art rule 3. Turf top and meadow turf need larger clumps (2 to 6 px) and fewer one-pixel speckles.
5. **Trees**: crowns are opaque spheres of leaf cubes; cut-out leaf edges plus a second, darker inner leaf colour would add depth.
6. **Furnishings** (`m22-furnishings.jpg`) read well; storage crates are busy (X braces on every face). Fine for now.

Textures at 32 px are sound; the Flow 128 px trial (`docs/proposals/textures-128.md`) is the right path for larger art.

## Ranked polish list

| # | Item | Why it matters | Effort | Nathan's decision? |
|---|---|---|---|---|
| 1 | Creep: an edge guard while a key is held, in `movement::step` | Core Boundless building technique; sky building is painful without it | S | Yes: which key (C is crafting, X is swim down; X doubling as creep on land is natural) |
| 2 | Slope shapes in generated terrain (new planets, generator bump) | Every planet looks smoother and more Boundless at once | M | Yes: confirm new planets only, and terraces kept or softened |
| 3 | Item icons at 64 px, integer scale, soft painterly style | The art Nathan called out; seen every minute | M (texgen) | No (style already decided); Flow icons optional |
| 4 | Client place prediction mirrors the server's support and rights rules | Removes flickering blocks | S | No |
| 5 | Tinted sky light and emissive glow | Atmosphere per planet, night interiors that glow | S to M | No (art direction covers it) |
| 6 | Block orientation (4 to 6 ways) for trunks and patterned blocks | Long-running Boundless complaint; logs sideways | M to L (chunk format, mesher) | Yes: which blocks turn, and how (placing face, or a rotate chisel) |
| 7 | Storage sort, deposit matching, search | Daily builder chore | S | No |
| 8 | Plot view toggle and drag-to-claim | Claiming 64 plots one click at a time | S | No |
| 9 | Every palette colour on some permanent world per material | Builders' palette; forum complaint 8 | S (data) to M | Yes: rarity design |
| 10 | Colour sprays and tinter | Boundless's colour economy | M to L | Yes: pigment source and cost |
| 11 | Place-in-front ghost (diagonal or air placement on the floor plane) | Removes helper blocks, beats Boundless | M | Yes: feel |
| 12 | Wild land and ash regeneration | Keeps worlds beautiful; frees land | L | Yes: whether, how fast, what counts as wild |
| 13 | Cut-out foliage and less noisy turf | Trees and ground up close | S to M | No |
| 14 | Beams joining poles, pole caps, panes at your feet | M22's own known limits | S each | No (Measure 5 answers thickness) |
| 15 | Smart stacks or larger block stacks | Inventory pressure while building | M | Yes: changes the economy of carrying |
| 16 | Blueprints of a claim | Wanted by Nathan "if feasible"; a feature Boundless never shipped | L | Yes: rules and cost |
| 17 | Transformation chisel (change rock or wood kind keeping colour) | Lets one colour cover all families | M | Yes |
| 18 | Guild-member rights message | Clarity | S | No |
