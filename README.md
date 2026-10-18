# Boundmore

A persistent online voxel universe in the spirit of *Boundless*: a world of planets that wrap around, built, shared and traded on by everyone in it. All art, names and code are Boundmore's own.

![Day](docs/media/m2-day.png)

**Status:** milestone 2 (light, day and night, painted textures, hold-to-break, hotbar) is built and awaiting a play-test; milestone 1 (foundation) is done. You can run a server, connect, walk, jump, swim, dig and build, see other players and have everything saved. See `docs/milestones/01-foundation.md` for what exists, and `docs/roadmap.md` for what comes next.

## Running it

You need [Rust](https://rustup.rs) (1.85 or newer).

```
cargo run --release -p bm-server
```

In a second terminal, from the same folder:

```
cargo run --release -p bm-client -- --name YourName
```

Controls: click to capture the mouse (Esc releases it), WASD to move, Space to jump (and to swim up; push against a shore with Space held to climb out), Shift to sprint. **Hold** the left button to break a block (harder blocks take longer; cracks show progress), right click to place, 1 to 0 or the mouse wheel to pick a block, F11 for fullscreen.

To play across machines, open UDP port 7777 on the server and give each player the server's `data/server.cert.der`:

```
cargo run --release -p bm-client -- --name YourName --server 203.0.113.5:7777 --cert server.cert.der
```

Load testing: `cargo run --release -p bm-server --example bots -- --count 50`.

## Updating

```
git pull
cargo run --release -p bm-server
cargo run --release -p bm-client -- --name YourName
```

Stop the running server (Ctrl+C in its window) and close the game before rebuilding: Windows will not let a running program be replaced, and the build fails with `failed to remove file ... bm-server.exe: Access is denied`. If a copy is still running somewhere, `taskkill /IM bm-server.exe /F` and `taskkill /IM boundmore.exe /F` stop them.

## Layout

| Crate | What |
|---|---|
| `bm-core` | blocks, chunks, coordinates, planets, movement shared by server and client |
| `bm-worldgen` | planet generation (server only) |
| `bm-mesh` | chunk meshing |
| `bm-protocol`, `bm-net` | the wire protocol and QUIC transport |
| `bm-server` | the authoritative server |
| `bm-client` | the game (`boundmore`) |

Design decisions are in `docs/decisions/`; the rules every change follows are in `CLAUDE.md`.
