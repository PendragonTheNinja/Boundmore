# Furnishings: research (2026-10-26)

How Boundless's furnishings behaved in the final game: poles and beams, glass panes, plinth, table and chair, storage, signs, trellis, trampoline and slide. The recipes themselves are in `building-blocks.md`; this file is about behaviour. Facts only, paraphrased; Boundmore names and draws its own.

Sources: the studio's release notes on the official forum (Discourse JSON API, `https://forum.playboundless.com/t/<id>.json`; "dev" below means a post by the studio, almost always the lead programmer's release notes), player threads on the same forum (marked "player"), the fandom wiki through its MediaWiki API (pages tagged version 244 to 249, spring 2020; the infobox descriptions are the game's own item text), and the game's item data as boundless.mayumi.fi publishes it (`recipes/raw/compileditems.msgpack`, decoded with the key map in `recipes/javascript.js`; late build, about Release 249). The item data holds stack sizes, categories and tint rules but **no block shapes, collision or storage sizes**: those live in data no fan site publishes, so they come from release notes and player reports. boundlexx's `/items/` returned a server error on 2026-10-26.

## Shared rules

- **Meshes and the per-chunk limit.** Everything that is not a voxel block (cube, chiselled cube, fluid, grass) is a "mesh": torches, machines, storage, doors, signs, poles, beams, plinths, furniture. A chunk (16 x 16 m, full world height) holds at most 512 meshes. High: dev, https://forum.playboundless.com/t/30354 (2019-05-24). Poles and beams were stated to count when added (https://forum.playboundless.com/t/25390). Glass panes started as meshes and were moved into the voxel renderer in Testing 234, so they stopped counting (below).
- **Meshes cannot be chiselled** into shapes; the chisel only rotates some of them. High: dev, https://forum.playboundless.com/t/27052 (post 23, 2019-03-04).
- **Colour.** Every furnishing takes its colour from one ingredient (`tintTakenFrom` in the recipe data; see `building-blocks.md`): wood pieces from the timber, stone from the stones, metal from the metal bar (copper, iron, silver, gold, titanium are five tints), gleam and glass from the (refined) gleam. High.
- **Stacks.** All of these items stack to 100 in an inventory slot (`maxStackSize` 100 in the item data). High: mayumi item data.
- **Tiers.** Doors, poles, beams and storage share one matrix: 4 materials (wood, stone, metal, gleam) x 3 designs (plain, stylish, ornate); storage adds a fifth material, farmed (wicker). High: dev, https://forum.playboundless.com/t/25109 (post 73), https://forum.playboundless.com/t/35785.

## 1. Poles and beams

Added Testing 212 / Release 213 (Jan 2019). Release notes: https://forum.playboundless.com/t/25109, https://forum.playboundless.com/t/25390.

| Question | Answer | Confidence | Source |
|---|---|---|---|
| Materials and styles | Wood, stone, metal, gleam, each plain ("standard"), stylish, ornate: 12 poles and 12 beams. | High | t/25390 (dev) |
| Light | Gleam poles and beams give off a little light; the others none. | High | t/25390 (dev); wiki infobox "A softly glowing Ornate Pole" |
| Orientation on placement | Both can stand vertical or lie horizontal; the orientation comes from the face clicked (placed against a side face, it lies along that face's normal; against a top or bottom, it stands). | High | t/25390 (dev) |
| Following neighbours | A pole placed against a pole, or a beam against a beam, takes the same orientation as the one it touches, so runs build straight. | High | t/25390 (dev) |
| Pole meets beam | "Beams extend into poles": a beam that meets a pole at right angles stretches into the pole's voxel so the two touch. The pole is unchanged. | High | t/25390 (dev) |
| Pieces change with neighbours | Poles look at their neighbours to choose an orientation and a "bend" piece (a player who reverse-reads the mesh choice says so; the launch picture shows a capped top piece on each pole, plain shaft pieces below, and beam end pieces where a beam meets a pole). So a column shows a cap on its top piece and middle pieces below; a beam run shows end pieces against poles. Exact rule set not published. | Medium | https://forum.playboundless.com/t/25109 post 56 (player a13o, 2019-01-26); launch image in t/25390 |
| Position in the voxel | Centred in the voxel (not on an edge like doors and signs); players asked for edge-aligned versions, which never came. | High | t/25109 posts 79 to 85 (dev and players) |
| Thickness | Not published. From the launch image: a pole looks about half a block across (wider at its cap), a beam about a third to half a block deep. A visual estimate only. | Low | launch image, t/25390 |
| Chisel | The chisel rotates a pole or beam about its long axis; nothing else (they are meshes). | High | t/25390 (dev) |
| Collision | Solid to players and creatures: a one-high fence of poles and beams stops wildlife walking through, but creatures hop over it because it is one block high; players planned jump-from-pole-to-pole courses. Whether the collision box is the thin mesh or the whole voxel is not stated. | Medium | t/25109 posts 81, 86, 109 (players, 2019-01-28/29); marketing text "keep the wildstock from wandering into your land", t/25009 (dev) |
| Count against mesh limit | Yes. | High | t/25390 (dev) |
| Smart stacks | Mixed poles share one smart stack slot. | Medium | t/25109 post 26 (player bug report) |

## 2. Glass panes

Added Testing 228 / Release 229 (Aug to Sep 2019); made voxel in Testing 234 / Release 235 (Nov 2019). Notes: https://forum.playboundless.com/t/34334, https://forum.playboundless.com/t/34672, https://forum.playboundless.com/t/37644, https://forum.playboundless.com/t/38043.

| Question | Answer | Confidence | Source |
|---|---|---|---|
| Kinds | Plain, stylish, ornate; tint from the refined gleam in the recipe. Matching chiselable full glass blocks (plain, stylish, ornate, same recipes) came in 234. | High | t/34334, t/37644 (dev) |
| Thickness and position | A thin plate on the inside edge of its own voxel, like a door or trapdoor (doors sit on the voxel's inner edge so they open into their own voxel). Exact thickness not published. | High (position), Low (thickness) | t/34334 post 125 (dev) |
| Placement | Like doors, not like signs: no backing wall needed. Aim at a face and the pane stands upright on the edge of that face nearest the cursor, always perpendicular to the face clicked, never flat on it; to lay one flat, aim at the side of a neighbouring block. Placing against an existing pane's edge extends it. Panes need not form rectangles. | High | t/34334 (dev); https://forum.playboundless.com/t/38918 posts 2 and 3 (players, 2019-12-15) |
| Connecting | The ornate pane picks its texture from its neighbouring panes (a "smart texture": border pieces change so a group reads as one window); from 234 the ornate glass block does the same per face. Panes do not join walls like fences. A pane against a glass block hides the frame lines on that side. | High (ornate smart texture), Medium (frame hiding) | t/37644 (dev); https://forum.playboundless.com/t/49771 post 2 (player) |
| Chisel | Panes cannot be chiselled (glass blocks can). | High | https://forum.playboundless.com/t/38063 post 29 (dev, 2019-11-27) |
| Transparency and light | Transparent; tinted. After the voxel move the glass texture's opacity was halved because both faces now draw. Liquids can share a pane's voxel (fixed placing liquid against a pane's back). Panes reduce water's see-through like glass blocks. Whether they pass sunlight to the light map is not stated; transparent, so assume yes. | High (transparency), Low (light) | t/34672, t/37644 (dev) |
| Weather | Panes did not reliably keep out rain or snow (players; behaviour changed around 2020 and depended on the pane's position in its block). | Low | https://forum.playboundless.com/t/47493 (players) |
| Collision | Not stated. Doors and signs have door-like thin physics (see signs), and panes place like doors; most likely the same thin solid plate. | Low | inference from t/10254, t/34334 |
| Mesh limit | Counted while meshes (228 to 233); not after 234. | High | t/37644 (dev) |

## 3. Plinth, table, chair

| Question | Answer | Confidence | Source |
|---|---|---|---|
| Plinth | A solid display-stand mesh with no function of its own: decorative, and an ingredient of the request basket and shop stand. Players repeatedly asked for it to display an item; it never did. | High | wiki `Plinth` (game text: "A nice solid Plinth. Use decoratively or Craft into a Request Basket or Shop Stand."); https://forum.playboundless.com/t/37566, t/33563 (players, 2019) |
| Table | Purely decorative. Items cannot be placed on it. | High | wiki `Table`; https://forum.playboundless.com/t/34914 post 4 (player, 2019-09) |
| Chair | Purely decorative; you cannot sit on it (stated for Release 209 on the wiki, and players still asked for sittable chairs in 2021 and 2022). | High | wiki `Chair`; https://forum.playboundless.com/t/34914, t/61774 post 25 (players) |
| Size, orientation, collision | Not published. Each is a one-voxel mesh. Facing on placement is not documented. | Low | none |

## 4. Storage

Open storage (items shown on the block) since early access; closed storage (items shown only when opened) from Testing 232 / Release 233 (Oct 2019, wood and stone) and Testing 234 / Release 235 (Nov 2019, metal, gleam, farmed). Notes: https://forum.playboundless.com/t/35785, https://forum.playboundless.com/t/36277, https://forum.playboundless.com/t/37644.

### Capacity (slots)

| Block | Slots | Confidence | Source |
|---|---|---|---|
| Storage block (stone, open) | 4 | High | wiki `Storage Block` (game text "Stores up to 4 Items") |
| Shelf (wood, open) | 4 | High | wiki `Storage Shelf` (game text) |
| Plain wood, plain stone, plain farmed | 16 | High (wood), Medium (stone, farmed) | wiki `Plain Wood Chest` (16); players below |
| Stylish wood, stone, farmed | 24 | High (wood), Medium | wiki `Stylish Wood Barrel` (24); players |
| Ornate wood, stone, farmed | 32 | High (wood), Medium | wiki `Ornate Wood Box` (32); players |
| Plain metal, plain gleam | 24 | High | wiki `Plain Metal Crate`, `Plain Gleam Cabinet` (24 each) |
| Stylish metal, stylish gleam | 32 | High (metal), Medium (gleam) | wiki `Stylish Metal Cabinet` (32); players |
| Ornate metal, ornate gleam | 40 | High (metal), Medium (gleam) | wiki `Ornate Metal Chest` (40); players |

Player tables agreeing: wood, stone and farmed 16/24/32 and metal and gleam 24/32/40, https://forum.playboundless.com/t/42205 post 2 (2020-02-26); same list and "stone and wood have 8 fewer than metal and gleam, farmed the same as wood", https://forum.playboundless.com/t/46788 (2020-06). The dev notes for 232 say closed storage holds 8, 16, 24, 32 or 40 items "depending on the storage setup", the stylish and ornate designs holding more; no closed storage of 8 appears in any list, so 8 was likely dropped before release.

So **both material and style set capacity**: style adds 8 slots per step, and metal and gleam add 8 over wood, stone and farmed. Slots are laid out in rows of 8.

### Other rules

- **Open vs closed.** Open storage (storage block, shelf) shows its items on the block to everyone, which costs network and rendering; closed storage sends its contents only when someone opens it, then caches them. High: dev, t/35785.
- **Joining.** Open storage blocks touching side by side present one combined window when opened. High: wiki `Storage`. Whether closed storage joins its neighbours is not stated; no source says it does.
- **Slot contents.** A slot holds a normal stack (up to 100) or a smart stack (several kinds of one family in one slot). Medium: item data `maxStackSize` 100; Smart Stacks in Releases 151 and 244 (https://forum.playboundless.com/t/6000, t/46111).
- **Light.** Stylish and ornate gleam storage light their surroundings; the plain gleam cabinet "emits its own light"; the stylish metal cabinet has "a gentle glow" in its text (light level not given). High: t/37644 fix list (dev); wiki `Plain Gleam Cabinet`.
- **Breaking.** Breaking a storage block drops its contents. High: t/25390 (fix about gatherer feats from "items dropped from destroyed storage containers").
- **Locks.** Storage, doors and signs can take door locks and advanced locks (beacon permissions). Medium: https://forum.playboundless.com/t/26819 (Release 217, Advanced Locks), t/46952.
- **Mesh limit.** Every storage counts. High: t/30354.

## 5. Signs

Added Testing 181 / Release 182 (Oct 2017, stone); wood, metal, gleam signs in Testing 204 (Nov 2018); glass signs in Testing 241 / Release 242 (Apr 2020). Notes: https://forum.playboundless.com/t/10254, https://forum.playboundless.com/t/20937, https://forum.playboundless.com/t/44553.

| Question | Answer | Confidence | Source |
|---|---|---|---|
| Two kinds | A **standalone** sign: a fixed sub-voxel placard that can be rotated. A **modular** sign ("sign module"): always 1 m square, joins neighbouring modules into one big sign. | High | t/10254 (dev) |
| How modules join | Modules of the same block type facing the same way join into rectangles up to **10 x 10**. While loose they show as plain "crates"; as soon as they form a valid rectangle they turn into sign pieces that can be written on. Once text is written the sign stops joining, keeps its size, and breaks as a whole (rewriting at another size means breaking and replacing it). Since Testing 232 modules join even when their colours differ (the type must still match; players found iron and titanium metal modules joining). | High | t/10254, t/35785 (dev); https://forum.playboundless.com/t/33632 (players); t/51123 (Nathan, 2020-09) |
| Orientation | Modules keep the direction they were placed from (not visible until joined); only same-facing modules join. After text is added the text can be rotated. Signs are placed against a block face; a backing surface is needed (panes were later made "unlike signs" for that reason). | High | t/10254 (dev), t/33632 (player), t/34334 (dev) |
| Text | Two texts: the **display text** shown on the sign, and a longer **additional text** anyone can read by interacting, whatever their permissions. Display text: regular, bold or italic font, several lines, left or right aligned, **auto-sized to fit the sign** (no size setting). | High | t/10254 (dev); https://forum.playboundless.com/t/10019 post 8 (dev, 2017) |
| Length | Display text holds **128 characters** however big the sign; colour codes count against the limit. | Medium-high | https://forum.playboundless.com/t/41357 (player, 2020-02), t/27685 (players and support, 2019-03) |
| Additional text length | Not found. | Unknown | none |
| Colour | Coloured text with codes such as `:#ff1133:`, short `:#f13:`, or a 2-digit palette code (`:#ff:` for palette colour 255; colour names convert to these). Only on signs in a beacon whose mayor has the paid membership. | High | t/20937, t/44553 (dev) |
| Glyphs | Emoji codes like `:sad:` or a hex code point render as vector emoji on signs (from 242, same membership rule). 2x2 and 3x3 glyphs and item icons were promised next; not confirmed shipped. | High (emoji), Low (glyphs) | t/44553 (dev); https://forum.playboundless.com/t/44493 post 14 (dev) |
| Materials | Stone (first), wood, metal, gleam (204); glass and glowing ("refined") glass (242). Metal and gleam sign text glows; text turns light automatically on dark-tinted signs. Glass signs look the same as glass pane voxels and sit seamlessly in a pane wall, with plain or glowing text. | High | t/20937, t/44553 (dev) |
| Collision | Signs have fixed thin physics like a door piece (but thinner): you collide with them and can stand on them. Standalone signs use the same physics box as modular ones. Their selection box was later shrunk so standalone signs on shop stands or storage do not get in the way. | High | t/10254, t/35785 (dev) |
| Who edits | Editing a sign needs build permission in the beacon (no separate "edit signs" permission existed; players complained). Locked signs break as a whole; unlocked loose modules break one by one. | Medium | https://forum.playboundless.com/t/12760 (player, 2018), t/37644 post 160 and t/17656 (players); t/10254 (dev) |
| Ink | 5 inky leaves (from the inky plant) in the extractor make 1 ink (60 s); every sign takes 1 ink, plus 5 of its material and 2 nails (glass signs: glue instead of nails). | High | wiki `Ink`; `building-blocks.md` (recipe data) |
| Mesh limit | Signs count. | High | t/30354 |

## 6. Trellis, trampoline, slide

| Item | Behaviour | Confidence | Source |
|---|---|---|---|
| Trellis | A voxel block (not a mesh): a horizontal lattice of timber, fully chiselable (it has the four chisel variants in the item data), gaps let liquids flow through. Foliage and mould trellises are the same block dressed with leaves or mould and take their colour from them (not spray-tintable). Some seasonal crops grow on trellis. It cannot be rotated: its grain always runs one way. Players report it blocks rain and snow despite its gaps. | High (block, liquids, tint), Medium (crops, orientation, weather) | t/35785 (dev, 232 and 232.1); wiki `Trellis`; https://forum.playboundless.com/t/56514, t/47493 (players) |
| Trampoline | A voxel block that bounces you: game text "a springy block for increasing jump height". The bounce scales with the speed you land at (falling higher, or grappling into it, bounces higher), so there is no fixed height; landing on it can still hurt from a great height. Accepts only the slope chisel. | High (bounce), Medium (scaling, damage) | wiki `Trampoline`; https://forum.playboundless.com/t/41165 posts 2 and 14 (players, 2020-02); t/14248 post 250 (player bug report, 2018); t/57250 (player, 2021) |
| Slide | A slippery block: players measured it the same as ice for speed and distance. Differences from ice: it does not melt or burn in lava, and roadrunners cannot run up it. Fully chiselable. | Medium | wiki `Slide` ("a slippery block"); https://forum.playboundless.com/t/41185 (players, 2020); t/57250 post 5 (player) |
| Fall damage by block | Landing hurts differently by surface: sponge least (none from moderate heights), then sand, rock, metal. | Medium | https://forum.playboundless.com/t/45611 post 4 (player) |

## Unknown, to ask Nathan to check in game

1. **Pole and beam thickness**: how wide a plain pole and a plain beam are, in sixteenths of a block (stand beside one and compare with a 1-block edge), and whether stylish and ornate differ.
2. **Pole and beam collision**: does the player collide with the thin mesh (can you squeeze past a pole in its own voxel, or stand on a horizontal beam's top), or with the whole block?
3. **Pole end pieces**: does a vertical column show a cap only on its top piece (and a foot on its bottom)? Does a lone pole show both? Do beams show end pieces on a free end, or only where they meet a pole?
4. **Pane thickness and collision**: how thick is a pane, and can you walk through one?
5. **Panes and light**: does sunlight pass through a pane (or a glass block) into a sealed room?
6. **Chair, table, plinth**: do they turn to face the player when placed, and do you collide with the whole voxel or the shape?
7. **Closed storage joining**: do two chests side by side open as one window, as open storage blocks do?
8. **Storage of 8**: is there any storage with 8 slots (the 232 notes mention it), and do stylish and ornate gleam hold 32 and 40?
9. **Sign additional text**: how many characters does the longer text hold?
10. **Who may edit a sign**: is beacon build permission enough, or does a lock on the sign change it?
11. **Standalone sign placement**: can a standalone sign go on a floor or ceiling, or only walls, and how is it rotated?
12. **Trampoline numbers**: how high a bounce from a standing jump and from a fall of, say, 10 blocks, and whether a bounce ever cancels fall damage.
