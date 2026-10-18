# Milestone 2: A World Worth Looking At

*Spec written 2026-10-18. In progress.*

## Goal

Turn the M1 test world into a place: light and darkness, day and night, painted textures, mining that takes effort, and a hotbar on screen. At the end, a screenshot should look like a game, not a tech demo.

## Acceptance criteria

1. **Light.** Sky light (15 under open sky, falling off by 1 per block sideways and under cover) and block light (from emitting blocks such as glow crystals, coloured later) propagate in true 3D, across chunk borders and across the planet seam. Caves without a light source are dark. Breaking or placing a block updates light incrementally, and the result equals a full recompute (tested on random edits).
2. **Smooth lighting.** Each vertex takes the light of the four cells in front of its corner (like AO), separate sky and block channels in the vertex, combined in the shader, so day and night never re-mesh anything.
3. **Day and night.** A day cycle driven by server time, the same for every player: sun and moon direction, sky gradient, fog colour, sky-light brightness. Nights are dark but playable under open sky.
4. **Textures.** The texture pipeline in `docs/art-direction.md`: catalogue `textures` field, 32 × 32 texture array with mipmaps, per-position variants, loud failure on a missing file. Every catalogue block has painted textures from `tools/bm-texgen`, reviewed via the contact sheet.
5. **Hold to break.** Breaking takes `hardness` seconds of holding; a crack overlay shows progress; letting go or looking away resets it. The server refuses a break that came sooner than the hardness allows (with a tolerance for latency).
6. **Hotbar.** A row of slots at the bottom of the screen with block icons and the selection marked; 1 to 9 and the wheel select.
7. **Whole columns stream.** Planets are bounded in height, so the server sends every chunk of each column in view; sky light is then exact from the top of the column down. Bandwidth stays near M1's (uniform chunks cost a dozen bytes).

## Non-goals

Coloured light, shadows from the sun, animated water, cut-out leaves, sound, inventory, tools affecting break time (M3).

## Plan and who does what

| Part | Owner | Where |
|---|---|---|
| Light engine and smooth-lit meshing | agent `light` | `bm-core/src/light.rs`, `bm-mesh` |
| Texture painter | agent `texgen` | `tools/bm-texgen`, `assets/textures/` |
| Column streaming, hold to break, day and night, texture array, hotbar, integration | main session | `bm-server`, `bm-protocol`, `bm-client` |

## Status

- [ ] Whole-column streaming
- [ ] Hold to break (protocol, server validation, client progress, crack overlay)
- [ ] Day and night (server time, sky, sun, fog, sky-light scale)
- [ ] Texture pipeline in the client (catalogue field, array, variants, mipmaps)
- [ ] Light engine (agent) merged and integrated in the client
- [ ] Painted textures (agent) merged and reviewed
- [ ] Hotbar
- [ ] Play-tested headlessly, screenshots in `docs/media/`, ready for Nathan
