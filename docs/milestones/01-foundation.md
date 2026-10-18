# Milestone 1: Foundation

*Built and verified 2026-10-18.*

## Goal

The base the whole game stands on: a wrapping voxel planet streamed from an authoritative server to any number of players, who see each other move and build, with every change saved.

## Acceptance criteria and how each was verified

| # | Criterion | Verified by |
|---|---|---|
| 1 | Palette-compressed 32³ chunks with a versioned, validated format | `bm-core` chunk tests: model comparison through palette growth to 9 bits, round trips, 2 000 corrupted inputs decoded without a panic |
| 2 | Deterministic planet generation that wraps without a seam | `bm-worldgen` tests: identical chunks per seed, chunks equal across laps, pinned terrain fingerprint, sea/land/mountain distribution, trees across chunk borders |
| 3 | Greedy meshing with ambient occlusion, opaque and translucent | `bm-mesh` tests: greedy output covers exactly the naive faces (with AO) on random chunks; winding; water surfaces |
| 4 | Authoritative server over QUIC: inputs, snapshots, chunk streaming with backpressure | End-to-end test with two real clients over QUIC (`bm-server/tests/two_players.rs`) |
| 5 | Edits validated by the server and seen by others | Same test: accepted break seen by the second player; bedrock, out-of-reach and into-a-player edits refused |
| 6 | No moving faster than real time by flooding inputs | Same test: 600 inputs sent at once, fewer than 40 applied in 200 ms |
| 7 | Edits and positions survive a restart | `edits_and_positions_survive_a_restart` |
| 8 | Client prediction with no corrections in normal play | Unit tests for agreement, correction and replay, out-of-order snapshots; in play-testing, 0 corrections while walking, jumping and crossing the seam |
| 9 | Playable client: render, walk, jump, dig, place, see others | Run under a virtual display with software Vulkan and driven by simulated input; screenshots in `docs/media/` |
| 10 | Load: 50 players on one server | `bots` example: 50 bots plus one player, 7 slow ticks in 3 600, sharing 4 CPU cores with the bots and a software-rendered client |

## What shipped

- `bm-core`: coordinates, `PlanetShape`, block catalogue from `assets/blocks.toml`, chunk storage and format v1, voxel raycast, deterministic movement with swimming.
- `bm-worldgen`: periodic noise; continents, hills, ridged mountains; beaches, sea floor, snow line, cliffs; caverns and tunnels; coal, copper and glowing crystals; trees.
- `bm-mesh`: padded-neighbourhood greedy mesher, AO with the anisotropy fix, 8-byte vertices, uniform-chunk shortcuts.
- `bm-protocol`, `bm-net`: messages, framing, QUIC endpoints, self-signed certificate pinned by the client.
- `bm-server`: 60 Hz game loop, input banking, view-radius streaming nearest-first with forget hysteresis, cached chunk frames, edit validation and rate limiting, redb saves, graceful shutdown, same-name login replacing a stale session, 30-second reports.
- `bm-client` (`boundmore`): wgpu renderer (reversed-Z, floating origin, frustum culling, front-to-back opaque, back-to-front water), per-face block colours with a turf fringe, block highlight, crosshair, other players as boxes, prediction, reconciliation with smoothing, interpolation, predicted edits, hotbar keys 1 to 9 and the mouse wheel.
- `bots` example for load testing.

## Known gaps (deliberate, scheduled in `docs/roadmap.md`)

- **No light propagation**: caves are as bright as the surface. M2.
- **Placeholder look**: flat colours with procedural grain, no textures, no sky beyond a colour. M2.
- **Instant breaking**, no inventory, no crafting: building is creative-mode. M2 and M3.
- **No accounts**: a name is all it takes to log in, and logging in under a name replaces whoever has it. The certificate must be copied to clients by hand. M3.
- **No UI** beyond the title bar (fps, ping, position, chunk and quad counts, prediction corrections, selected block).
- **One planet.** M4.
- **Unloaded chunks are solid** to physics on both sides, so a player who outruns streaming stops at the edge until the chunk arrives. Correct, but streaming should stay ahead of sprinting; watch for it.
- **Not yet run on a real GPU or on Windows.** Everything above was verified on Linux with a software renderer. The first native run is Nathan's.

## How to run (PowerShell)

```
cargo run --release -p bm-server
# in a second terminal, from the same folder:
cargo run --release -p bm-client -- --name Pendragon
```

The server writes its certificate to `data\server.cert.der`, which the client reads by default. For a second machine, copy that file over and pass `--server HOST:7777 --cert path\to\server.cert.der`. The server listens on UDP port 7777.

Controls: click to capture the mouse, Esc to release. WASD, Space to jump, Shift to sprint, left click to break, right click to place, 1 to 9 or the mouse wheel to choose a block, F11 for fullscreen.
