# CLAUDE.md: Boundmore

Read this file in full before changing anything. If a request conflicts with it, say so instead of quietly complying.

## What this is

**Boundmore** is a spiritual successor to *Boundless* (Wonderstruck, 2018): a persistent online voxel universe of many small planets, built and traded on by its players. The goal is that Boundless players feel at home (same systems, same rhythm) in a game with its own art, names and world, that is finished and keeps being updated, and that runs better than the original.

Rules of the spirit:
- **Mechanics may follow Boundless closely; expression may not.** Never copy its art, textures, sounds, text, names (no "gleam", "Oortians", "Hunting Grounds") or code. Do not decompile or read its client. Study it as a player does.
- **Feel numbers are calibration targets**, not guesses to defend: `docs/feel.md` lists every one and what to check it against in play. Nathan plays Boundless; his reports beat our assumptions.

Owner: Nathan (`PendragonTheNinja`). Solo project, multi-year, no deadline. Do it right over doing it fast.

## Architecture (ADRs in `docs/decisions/`)

| Crate | Role | Rule |
|---|---|---|
| `bm-core` | blocks, chunks, coordinates, planets, raycast, movement | headless: no graphics, sockets or files |
| `bm-worldgen` | deterministic planet generation | server only (ADR-0003) |
| `bm-mesh` | greedy meshing with AO | headless |
| `bm-protocol` | wire messages, framing | pure data, no IO |
| `bm-net` | QUIC endpoints, certificates, framed streams | shared by server and client |
| `bm-server` | authoritative game loop, streaming, edits, saves | lib + `bm-server` binary |
| `bm-client` | window, renderer, prediction, input | `boundmore` binary |

Dependency direction: `bm-core` depends on nothing in the workspace; nothing headless depends on wgpu or winit.

### Invariants (each one is a bug if broken)

1. **The server is the authority** on world, players and every rule (ADR-0004). The client predicts its own movement and edits; the server's answer always wins. Never trust a number from the client: inputs are clamped, edits are checked for reach, rate, support, occupancy and catalogue flags (`check_edit` in `game.rs`).
2. **Movement is deterministic across machines** (`bm-core/src/movement.rs`): fixed 60 Hz step, quantised angles, `libm` for trigonometry (never `f64::sin`), plain `f64` arithmetic. If prediction corrections appear in steady play, a rule has been broken; the title bar counts them.
3. **Client and server answer `PhysicsWorld` identically**: below the floor is solid, unloaded chunks are solid, water is liquid. Change one, change both.
4. **Positions are canonical** (X, Z in `0..width`) on both sides (ADR-0002). Compare positions with `PlanetShape::delta`, interpolate with `nearest_lap`, render each chunk at the lap nearest the camera. Never subtract two raw positions that may straddle the seam.
5. **Chunks are 32³ cubes, palette-compressed**; never assume a bits-per-voxel or a column layout. Use `coords` helpers for chunk/local splits (`>>`/`&`, never `/`, `%`).
6. **Clients never generate terrain** (ADR-0003): ore is economy, and a seed on the client is an x-ray of the planet.
7. **Everything serialized is versioned**: `CHUNK_FORMAT_VERSION`, `PROTOCOL_VERSION`, `SAVE_VERSION`, `GENERATOR_VERSION`. Any change to generated terrain bumps `GENERATOR_VERSION` and re-pins `terrain_fingerprint_is_pinned` in the same commit; the server refuses a planet made by another version.
8. **Blocks are data** (`assets/blocks.toml`), sent to the client in the handshake. Code refers to blocks by name, resolved once at startup; a missing name is a startup error. `solid` is physical, `opaque` is visual; ask the one you mean.
9. **The game loop never awaits and the network never touches state.** Network tasks turn packets into `Event`s; the loop answers through `Outbox` (non-blocking, with a byte backlog the loop reads for backpressure).
10. **Saves are crash-safe and never silently lose a build**: a failed read is retried, never answered with fresh terrain; a failed write is retried; dirty chunks are saved before eviction.
11. **Reversed-Z, infinite far plane** in the renderer: clear depth to 0, test `Greater`. Build projections with `render::projection`, never a library helper with a different convention.
12. **Reliable messages share one ordered stream** so a block change can never overtake its chunk. Movement uses datagrams and is never resent.

## Verification (run before every commit)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected test counts at `M1`: bm-core 32, bm-worldgen 13, bm-mesh 8, bm-protocol 5, bm-server 2 unit + 2 end-to-end, bm-client 11 (73 in all). CI (`.github/workflows/ci.yml`) runs the same on Linux and builds on Windows.

**Load test:** run a server, then `cargo run --release -p bm-server --example bots -- --count 50 --prefix Load`. Watch the server's 30-second report for slow ticks.

**Seeing the game from a Linux sandbox** (no GPU): software Vulkan and a virtual display.
```
apt-get install -y xvfb mesa-vulkan-drivers libvulkan1 libxkbcommon-x11-0 xdotool imagemagick
Xvfb :99 -screen 0 1280x720x24 &
DISPLAY=:99 VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json WGPU_BACKEND=vulkan target/release/boundmore --name Tester --cert data/server.cert.der &
DISPLAY=:99 import -window root shot.png
```
Drive it with `xdotool` (`windowfocus --sync`, then plain `keydown`/`click`, not `--window`). Frame rates there mean nothing; correctness and pictures do. Nathan's machine is the authority on performance and feel. Stop processes with `pkill -x boundmore`, never `pkill -f` (it matches your own shell).

## Workflow

- Nathan works on **native Windows (PowerShell)**: give commands that work there.
- Specs before code: each milestone gets `docs/milestones/NN-name.md` (goal, acceptance, non-goals) before it is built, and a status section kept current.
- Significant decisions get an ADR in `docs/decisions/`; accepted ADRs are not edited, they are superseded.
- A fix ships with a test that failed before it.
- Commit and push at every task boundary. Commit subjects are one line.
- Player-facing text (block names shown in game, messages, UI): no em dashes; plain, concrete words.

## Status

- **Milestone 1, Foundation: done** (2026-10-18). See `docs/milestones/01-foundation.md` for what shipped, what was verified and how, and the known gaps.
- **Next:** milestone 2, the first Boundless loop, outlined in `docs/roadmap.md`. Its spec has not been written; write it with Nathan first.
