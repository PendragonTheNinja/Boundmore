# Milestone 27: The Atlas

*Spec 2026-10-28.* Deferred from M14 ("the atlas (map item), region outlines on a map"). Every system left on the roadmap that shapes the economy waits on Nathan; this one is a tool, Boundless's own, and needs no decision of his (the defaults below are cheap to change). A Boundless player found their way, their regions and their ore with an atlas from their first day.

## What Boundless did (wiki "Atlas", forum)

- **The atlas** is gear: workbench, 2 copper bars and 4 glass, 30 s, 8 XP (wiki, version 244).
- Crafted unbound; **the first time it is used on a world it is bound to that world for good**, and from then on shows that world.
- Held and used, it shows **the land around you**; **regions you have not discovered are under fog**.
- **A resource put into it** (an ore, a rough gem, a plant drop) shows **a heat map of where that resource lies**: its relative density projected onto the surface, never its depth. A warp augment showed plots. Drop (X) takes the resource out again.
- The server recomputed the heat maps about every 30 minutes (forum, topic 19685, 2018).
- What players asked for (forum 2018 to 2020): **real zoom** ("I just wish I could zoom in closer"), **region names** and the names of settlements on it, and not to sink while reading it in water (M23 already floats you).

## The rules

1. **The atlas item**, Boundless's recipe. It carries its world in the stack's `tag` (as portal tokens do), and the resource loaded in it, so **no save format changes**: `tag = planet + 1 | resource item << 16`; 0 is unbound.
2. **Using it** (the use press with it in hand, as food is eaten) binds an unbound atlas to the world you stand on, and opens **the map window**. A bound atlas opened on another world says which world it shows and still shows it.
3. **The map** is the whole planet seen from above, one pixel for four blocks at its widest, shaded by height, water by depth (`planet_map`'s look). It wraps as the planet does, so it pans forever. It opens centred on you, with mouse-wheel zoom (four steps, closest one pixel a block) and drag to pan (**players' wish**). It shows:
   - fog over every region you have not discovered on that world (M14's discoveries);
   - your position and facing, when you are on that world;
   - **the names of discovered regions** at their centres (**players' wish**);
   - claims: yours in one colour, others' in another; **settlements by name and rank** (M24) and the capital;
   - warp shrines.
4. **The land is the server's**, never generated on the client (invariant 6). The server draws each tile of the map (64 by 64 pixels, 256 blocks a side) from the real world where chunks exist and from the generator's surface where none do yet, and **sends only tiles of regions the player has discovered** (fogged land is never sent: no x-ray). A tile is redrawn when blocks in it change and resent to whoever has it open.
5. **Heat maps:** put a resource in the atlas from the map window (any ore, gem, wayshard, fossil or plant drop you carry) or take it out. The server answers with that resource's density for the planet: for each map cell of 16 by 16 blocks, how many blocks dropping it lie under that cell at any depth, sampled from the generator, scaled to the planet's own highest, in four bands (**Boundmore's colours**: dark violet, cyan, amber, white). Computed once per planet and resource on a worker thread, kept in memory. Like the land, only discovered regions are sent.
6. A heat map shows where a resource **was generated**. Mined-out seams still show (Boundless's was refreshed every 30 minutes from the world; ours from the generator alone is a known limit until a mining tally exists).

## Numbers

- Recipe and XP: Boundless's (wiki). Wear: Boundless's 6.
- Map scale (4 blocks a pixel), tile size, cell size, the four bands: Boundmore's choices.

## Acceptance

- An atlas crafted at a workbench, bound on first use, opens a map of its world centred on the player; fogged regions show no land, discovered ones do; region names, claims, settlements and shrines are drawn; it pans across the seam and zooms.
- A tile in a region the player has not discovered is never sent (end-to-end test).
- Loading copper ore shows copper's heat map, highest where the generator puts the most copper.
- Breaking or placing blocks changes the open map within a second.
- `--demo-ui atlas` shows it; a screenshot.

## Non-goals

Boundless's 3D globe held in hand (a wrapping world is a flat map that wraps; a window reads better and zooms); other players on the map; markers and waypoints; a heat map of prestige (Boundless's warp augment): claims and settlements are drawn instead.

## Status

- [x] Research and spec (2026-10-28)
- [x] Part 1 (2026-10-28): the atlas item and Boundless's recipe (`ItemKind::Atlas`, `items.toml`); `bm_core::atlas` (scale, tiles, the stack tag, shading); `ClientMsg::OpenAtlas`, `ServerMsg::{Atlas, AtlasTile}` (protocol 54); the server's tiles per planet (`bm-server/src/atlas.rs`: drawn on the worker pool from snapshots of resident chunks, else saved chunks, else fresh terrain never saved; a saved chunk that cannot be read leaves the tile undrawn), each pixel's region kept so tiles are fogged per player before sending and tiles with none of their land never sent (`game/atlas.rs`); binding on first open; tiles near discovered regions sent four a tick; edited columns mark tiles stale, drawn again half a second later if someone has them, else forgotten; a discovery while open resends with the new land; tiles leave memory five minutes after the last atlas there closes. Client: the map window (`bm-client/src/atlas.rs`: wraps across the seam, zoom by wheel, drag to pan, centre on me, fog, our arrow, region names, claims (ours gold), settlements by name and rank, the shrine), the atlas icon. Tests: 3 core, 1 server, 2 client, 1 end to end (unbound until opened, bound on opening, every pixel fogged exactly where the region is undiscovered, a dig redraws the tile). Screenshot `docs/media/m27-atlas.jpg` (`--demo-ui atlas`: a fixture landscape, three of nine regions discovered).
- Part 1's known limits: the atlas of another world shows no claims or settlements (the client holds only its own world's); a tile's slope shading stops at its edge; heat maps are part 2.
- [x] Part 2, heat maps (2026-10-28): `Generator::density` (each matching ore's noise sampled every 3 blocks down its depth band where its column rules allow, plus each matching surface plant's chance where its ground and patch allow; no chunk generated); the server's heat map per world and resource on the worker pool (cells of 16 blocks, four columns each, banded 1 to 4 against the world's richest cell), hidden per player outside discovered regions; `ClientMsg::AtlasLoad` puts one of a stack in (giving back what it held) or takes it out, refused for coloured stacks and for what no seam or plant drops (`atlas::finds`); `ServerMsg::AtlasHeat` (protocol 55). Client: the heat overlay in four bands with its legend, "Put in to find" buttons for what is carried, "Take it out". Tests: 1 worldgen (the estimate's richest chunk columns hold more copper than its poorest), the end to end extended (copper in: its heat map, hidden exactly outside her region, copper somewhere in it; taken out: the ore comes back, the map clears, the atlas stays bound). Screenshot `docs/media/m27-atlas.jpg` now shows copper's heat map.
- Part 2's known limits: a heat map shows where the generator put a resource, not what is left (Boundless's was refreshed from the world every 30 minutes); cave plants are not counted; resources from bonus drops (flint from rock) cannot be loaded.
- [x] Demo, screenshot, docs (2026-10-28)
- [ ] Nathan's play-test
