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
| `bm-core` | blocks, chunks, coordinates, planets, raycast, movement, light, day cycle | headless: no graphics, sockets or files |
| `bm-worldgen` | deterministic planet generation | server only (ADR-0003) |
| `bm-mesh` | greedy meshing with AO | headless |
| `bm-protocol` | wire messages, framing | pure data, no IO |
| `bm-net` | QUIC endpoints, certificates, framed streams | shared by server and client |
| `bm-server` | authoritative game loop, streaming, edits, saves | lib + `bm-server` binary |
| `bm-client` | window, renderer, prediction, input, HUD | `boundmore` binary |
| `tools/bm-texgen` | paints every block texture procedurally | `cargo run --release -p bm-texgen` rewrites `assets/textures/` |

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
13. **Light first, mesh once** (`bm-core/src/light.rs`). Light is computed only for complete columns; unlit or missing reads as dark (dark-then-brighten is safe, lit-then-stale is the bug). Every chunk insert, forget and block change goes through `LightMap`, and the chunks it returns are re-meshed. Sky and block light are separate vertex channels; day and night is a uniform (`sun_color.w`), never a re-mesh.
14. **Textures are data**: the catalogue names them, files live in `assets/textures/` (`docs/art-direction.md`), and a missing file falls back to the block colour with a warning in the title bar. Face table entries pack `first_layer | variants << 16`; decode before using one as a layer.

## Verification (run before every commit)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected test counts (M7 part 2, 2026-10-19): bm-core 91, bm-worldgen 32, bm-mesh 22, bm-protocol 6, bm-server 42 unit + 21 end-to-end, bm-client 80, bm-texgen 5 (299 in all). CI (`.github/workflows/ci.yml`) runs the same on Linux and builds on Windows.

**Upgrade check (any change to a saved or versioned format):** build the server from `main` in a separate worktree and target dir, let it create a universe (`bots --count 2 --planets 2` to make players), stop it, then open that data dir with the new server and read its log. Unit tests built from the new code missed exactly this once (2026-10-19: chunk format 2 refused every older universe).

**Load test:** run a server, then `cargo run --release -p bm-server --example bots -- --count 50 --prefix Load`. Watch the server's 30-second report for slow ticks.

**Seeing the game from a Linux sandbox** (no GPU): software Vulkan and a virtual display.
```
apt-get install -y xvfb mesa-vulkan-drivers libvulkan1 libxkbcommon-x11-0 xdotool imagemagick
Xvfb :99 -screen 0 1280x720x24 &
DISPLAY=:99 VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json WGPU_BACKEND=vulkan target/release/boundmore --name Tester --password testerpass1 --register --cert data/server.cert.der &
DISPLAY=:99 import -window root shot.png
```
`--name` with `--password` skips the login screen (drop `--register` once the account exists). `--demo-ui login|hud|inventory|crafting|station|claims|shrine|travel|travelling|sky|storage|chat|colours|coloured-icons|reclaim|shapes|chisel|grapple` opens a menu on an offline test patch, no server needed.
Drive it with `xdotool` (`windowfocus --sync`, then plain `keydown`/`click`, not `--window`; to break, `mousedown 1`, wait the block's hardness, `mouseup 1`). `BOUNDMORE_TIME=0.77` (0 midnight, 0.5 noon) previews a time of day; `--assets DIR` points at another texture set. Frame rates there mean nothing; correctness and pictures do. Nathan's machine is the authority on performance and feel. Stop processes with `pkill -x boundmore`, never `pkill -f` (it matches your own shell); when parallel agents run servers, give each its own `--bind` port and data dir and stop processes by PID, or one agent kills another's server.

## Starting a session (do this first, every time)

1. `git -C <clone> fetch origin && git -C <clone> status`; work from `origin/main`. If what Nathan describes is not on `main`, ask before building on a guess.
2. Read, in order: this file, `docs/README.md` (the docs index), the milestone spec marked *in progress* in `docs/milestones/`, and its **Status** section, which lists exactly what is done and what is next.
3. Run the verification commands below and confirm the expected test count before changing anything.
4. Check CI on the last commit (`https://api.github.com/repos/PendragonTheNinja/Boundmore/actions/runs?per_page=1`); a red `main` is the first job.

## Working with Nathan (agreed 2026-10-18)

- **Work in large batches, not one step at a time.** Nathan tests when a major piece lands, not after every change. Keep building, verify everything you can yourself (tests, the headless run below, screenshots), push in task-sized commits, and tell him when something is worth playing. Ask only when a decision is genuinely his (feel, art, what players want).
- **Keep going between milestones** (Nathan, 2026-10-19): when one milestone is built, start the next without waiting, unless the next needs his decision.
- **Stop and ask first** when a decision is genuinely his AND building it the wrong way would cost a lot of work or time to change (data formats players will save into, how a system feels or is paid for, anything that shapes what players invest in). Build around it or pick other work meanwhile; never guess on those.
- **Parallel agents are welcome** where work splits cleanly (separate crates or modules, each in its own git worktree). Merge, re-verify, push.
- **He plays Boundless; his reports about how it felt are ground truth.** Record each such decision in `docs/feel.md` or the relevant spec with the date.
- **Art direction:** Boundless's bright, painterly high fantasy, crossed with Hytale's chunky stylised charm, in Boundmore's own textures (`docs/art-direction.md`). Nathan is not an artist: Claude produces the textures (procedurally, `tools/bm-texgen`), and the pipeline must also accept hand-made or AI-generated replacements dropped into `assets/textures/`.
- **Player size:** two blocks tall; nothing fits under less than two blocks (2026-10-18).

## Workflow

- Nathan works on **native Windows (PowerShell)**: give commands that work there. How to run the game is in `README.md`.
- Specs before code: each milestone gets `docs/milestones/NN-name.md` (goal, acceptance, non-goals) before it is built, and a status section kept current.
- Significant decisions get an ADR in `docs/decisions/`; accepted ADRs are not edited, they are superseded.
- A fix ships with a test that failed before it.
- Commit and push at every task boundary. Commit subjects are one line.
- Player-facing text (block names shown in game, messages, UI): no em dashes; plain, concrete words.

## Status

**Where we are (handoff, 2026-10-19):** milestones 1 to 7 are on `main` (M7: chisels, then grapples); Nathan has play-tested up to M3. Next: player-built portals with the agreed fuel rule (roadmap; spec first, as milestone 8), then a skills proposal for Nathan before building any progression. Open follow-ups: share the chisel rules between client and server in bm-core (the client copies the server's refusal texts today); the rope is drawn with 1-pixel lines (a few strands side by side), a real rope mesh would look better; the rest of the roadmap's M7 line (swimming improvements, far views) is not specced yet.

- **Milestone 1, Foundation: done** (2026-10-18). `docs/milestones/01-foundation.md`.
- **Milestone 2, A World Worth Looking At: built** (2026-10-18), play-tested by Nathan (breaking, water fixed). `docs/milestones/02-a-world-worth-looking-at.md`.
- **Milestone 3, Gather, Craft, Claim: built** (2026-10-18); Nathan's first test found the block-after-block break refusal (fixed, protocol 6). `docs/milestones/03-gather-craft-claim.md`.
- **Milestone 4, Many Worlds: built, awaiting Nathan's play-test.** `docs/milestones/04-many-worlds.md`. One process hosts the universe (ADR-0006), one save file `data/universe.redb`.
- **Milestone 5, Keep and Talk: built, awaiting Nathan's play-test.** `docs/milestones/05-keep-and-talk.md`: storage chests and chat. Inserted ahead of Movement and Shape; the roadmap was renumbered.
- **Milestone 6, Colour and Ash: built, awaiting Nathan's play-test.** `docs/milestones/06-colour-and-ash.md`; block states with colour and shape (ADR-0007), 255-colour palette, planet colours (generator 3), claims lapse to coloured ash, reclaim at warp shrines.
- **Milestone 7, Movement and Shape: built (chisels, then grapples), awaiting Nathan's play-test.** `docs/milestones/07-movement-and-shape.md`; shapes are a family plus eight octants (ADR-0008); the grapple hook lives in `PlayerState` and runs inside `movement::step` (protocol 11); saves keep the pre-grapple body layout (`store::SavedBody`).
- **Design research:** `docs/research/boundless-forum-feedback.md` (what Boundless players liked and disliked). Read it before designing any Boundless system. Decided from it so far: no beacon fuel; a claim lapses only after 6 months without a visit, and its contents go back to the owner (2026-10-18); on a lapse the build turns to ash blocks that keep each block's colour, as in Boundless (2026-10-19). Player-built portals will need fuel, but less punishing than Boundless's (2026-10-19).
- **Mechanics research:** `docs/research/boundless-mechanics.md` (how Boundless's systems worked, from the forum and wiki). Sources reachable from the sandbox: the forum's Discourse JSON API (`https://forum.playboundless.com/search.json?q=...`, `/t/<id>.json`) and the fandom wiki's MediaWiki API only (`https://boundless.fandom.com/api.php?action=parse&page=<Title>&prop=wikitext&format=json`; page URLs return 403). The wiki is frozen around spring 2020: prefer later dev posts and patch notes. Read the relevant section before designing a system, and do the research yourself rather than asking Nathan for facts the forum holds.
- Update this section and the milestone's Status section at the end of every working session.
