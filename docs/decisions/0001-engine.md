# ADR-0001: Custom engine in Rust on wgpu, server in the same language

- **Status:** Accepted, 2026-10-18

## Context

Boundmore is a persistent online voxel universe: many planets, every block editable, a player economy, an authoritative server. The two hardest constraints are (1) a server that can carry many players per machine for years, and (2) a client that renders far voxel views smoothly on ordinary PCs. Development is solo, assisted by Claude, with no deadline.

## Options

1. **Unity / Unreal.** Scene-graph engines fight voxel worlds (millions of dynamic faces, streaming, custom lighting), and neither gives a natural home for an authoritative voxel server. Rejected.
2. **Godot plus a voxel module.** Good editor and UI, but the voxel core, the server and the protocol would still be ours, split across two languages and two runtimes. Rejected for the split.
3. **Bevy.** Rust, but its renderer is not built for voxel-scale geometry and its API changes every few months. Rejected; its patterns stay available.
4. **Fork Luanti (Minetest).** Fastest to a playable world, but built around one flat world, an old renderer and a different game. Rejected.
5. **Custom engine in Rust, wgpu for graphics, winit for windows, QUIC for networking.** Accepted.

## Decision

- **One language end to end.** Server and client share `bm-core` (chunks, coordinates, movement), `bm-protocol` and `bm-net`. The movement code that predicts on the client is the same code that rules on the server (ADR-0004), byte for byte.
- **Rust** for speed without garbage-collection pauses, and memory safety for a server that must not crash.
- **wgpu** (Vulkan, DX12, Metal) for a modern GPU pipeline on every PC platform.
- **Libraries for generic problems**: winit, rayon, quinn (QUIC), rustls, redb, glam, postcard. Voxel-specific code (chunks, meshing, generation, streaming, netcode) is ours.

## Consequences

- No editor: content is data files and tools we build.
- UI must be built (an immediate-mode UI library is the likely choice when menus arrive).
- Every crate that does not need graphics or IO stays headless and fully testable in CI.
