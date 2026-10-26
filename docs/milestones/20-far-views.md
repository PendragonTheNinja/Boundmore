# Milestone 20: Far Views

*Spec written 2026-10-26. Planned since M7 ("far views (level-of-detail terrain)") and never specced. Nathan, 2026-10-26: "Continue onward": M19 was built, the economy systems wait on his decisions, and far views need none of them (it is rendering and streaming, it shapes nothing players invest in). Boundless first: Boundless drew terrain past its full-detail chunks as coarser voxel copies ("the Level of Detail (LOD) system allows the game to have longer draw distances by reducing the # of voxels drawn", james, Wonderstruck, forum t/1258, 2015); its default and largest draw distance was 1024 blocks (players, t/12681, 2018), and far LODs were "lower cost" than near terrain (michaelb, t/4866, 2016).*

## Goal

Today the world ends at the view radius (10 chunks, 320 blocks) in fog. A Boundless player stands on a hill and sees the far side of the valley, the mountains beyond, floating islands on the horizon and the builds of a city a kilometre away. This milestone draws the planet out to 1024 blocks (the whole of a 64-chunk planet) at reduced detail, without raising the cost of near terrain.

## Decided in this spec

1. **Coarse voxels, not a height map.** Far terrain is the real voxels at a coarser grain (Boundless's way), so overhangs, floating land, trees and builds show from afar; a height map would flatten them.
2. **Two levels.** Level 1 cells are 2×2×2 blocks (a chunk is 16³ cells); level 2 cells are 4×4×4 (8³). Full chunks out to the view radius (10 chunks), level 1 out to twice it (20 chunks, 640 blocks), level 2 out to the far radius (default 32 chunks, 1024 blocks, Boundless's; `far_radius` in the server settings, 0 turns far views off). At those distances a cell is about the size on screen of a block at the ring's inner edge.
3. **Made on the server, from the real chunk** (ADR-0003 holds: the client never generates). A chunk's far copy is computed from the saved chunk when there is one, so builds and digging show; else from a freshly generated chunk, which is then dropped (not saved). Far copies are saved in their own table, keyed by planet and chunk, with a format version, and remade when their chunk is saved after an edit.
4. **No x-ray.** A cell takes the commonest block among its blocks that face open space; a buried cell takes its commonest block. Ore is never the commonest block of a buried cell in practice, and an ore that wins a cell facing air is visible in game anyway. Models (plants, torches, machines) are dropped; cut blocks count as their block.
5. **A cell is solid** when at least 3 of its 8 parts (blocks or level 1 cells) are solid, so thin walls and trunks survive one level and most of the next. Else it is liquid when its liquid parts outnumber its open ones, else open.
6. **The client draws far copies with the near terrain's mesher and textures**: a far chunk is expanded to a 32³ chunk of its cells and meshed as any chunk (its colours, textures and ambient occlusion come for free); far light is simple (full sky above a column's top solid cell, dim below), computed on the client from the far copies. A chunk is drawn from its full mesh when it has one, else from its far copy, so far copies also fill the gaps while near chunks stream in (after a warp the land is there at once, coarse, then sharpens).
7. **Far copies are sent for every chunk within the far radius**, near ones included (they are small), each at the level of its ring; a chunk crossing a ring is resent at its new level. Fog moves out to the far radius (atmosphere fog still closes in as air runs out).
8. **Built in the background, nearest first.** A planet's far copies are made on demand for the players on it, a few at a time on the loader pool and never ahead of real chunk loads; the first visit to a planet fills its horizon over a minute or two (the planet's whole far set is about 49,000 chunks at 5 to 12 ms each), later visits read them from the save.

## Rules

1. `bm-core/src/far.rs`: `FarChunk { level, cells, palette }`, `FarChunk::from_chunk(&Chunk, level 1)`, `FarChunk::coarser()` (level 1 to 2), `expand() -> Chunk`, `encode`/`decode` with `FAR_FORMAT_VERSION` (validated, never panics on bad input).
2. Protocol: `ServerMsg::Far { pos, data }` (a far copy, any level) and `ServerMsg::ForgetFar { pos }`; the welcome carries `far_radius`. `PROTOCOL_VERSION` 33. Far copies ride the reliable stream (a far copy is replaced, never patched).
3. Server (`bm-server/src/far.rs`): per planet, a far store (encoded level 1 copies, made on demand, with level 2 frames derived and cached), a background builder on the loader pool with a small budget in flight, the far table in the save (`far`), remade from any saved dirty chunk; per player, a far view list (chunks within `far_radius` and their ring level) sent within a byte budget per tick after the near chunks have priority, forgotten beyond `far_radius + 2`.
4. Client (`bm-client/src/far.rs`): the far copies held, a column top map for far light, meshing jobs nearest first with their own budget, `Renderer` meshes keyed apart from near meshes; a chunk draws its near mesh if present, else its far mesh.
5. A far mesh treats a missing neighbour as solid (no faces towards it), as near chunks treat unloaded ones.

## Acceptance

1. Standing on high ground on a fresh biome planet, the land is visible to 1024 blocks: mountains, trees, floating islands, seas; screenshots in `docs/media/`.
2. Far copies are bit-identical however they are reached: from a chunk, from a saved copy, after a round trip through the wire (tests).
3. An edit far from everyone changes that chunk's far copy once its chunk is saved; a player in range is sent the new copy (end-to-end test).
4. No ore that is buried shows in a far copy (test: a chunk of stone with buried ore gives a far copy without ore).
5. The near terrain is unchanged: same meshes, same light, same tests; the near chunk budget is not slowed by far work (load test: 50 bots, ticks as before).
6. Existing universes open: the far table is created empty and filled as players look around (upgrade check).

## Non-goals

- Far creatures, players and models (those stay at the near radius).
- Smooth blending between levels (Boundless's LODs also popped; fog and distance hide it).
- Far views through portals (portals show nothing yet).
- Client-side caching of far copies between sessions.

## Measure in game (for Nathan)

1. Does the horizon look like Boundless's at its largest draw distance? Is 1024 enough, or should it reach further on bigger planets?
2. Do the ring edges (320 and 640 blocks) pop noticeably when walking?
3. Frame rate with far views on and off (`far_radius = 0` in `server.toml`) on your machine.

## Status

- *Spec written 2026-10-26.* Building.
