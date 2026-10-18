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

- [x] Whole-column streaming
- [x] Hold to break (protocol, server validation, client progress, crack overlay with built-in cracks until painted ones exist)
- [x] Day and night (saved planet clock, sky pass, sun, moon, stars, fog from the sky, night dimming). `BOUNDMORE_TIME=0.75` previews a time of day.
- [x] Texture pipeline in the client (catalogue field, array, variants, mipmaps; a missing texture falls back to the block colour and shows in the title bar)
- [x] Hotbar (isometric block icons, scrolling, selection)
- [x] Fixes from Nathan's first play (2026-10-18): climbing out of water (a pop up when pushing against a shore with jump held); breaking shows progress
- [x] Light engine (agent `light`) merged and integrated: lit columns, smooth per-vertex sky and block light, night dims only the sky. Timings (release): a column lights in about 1 ms, an edit in 0.015 ms on average.
- [x] Painted textures (agent `texgen`) merged: 43 block textures and 8 crack stages; contact sheet `docs/media/textures.png`, diorama `docs/media/texture-preview.png`
- [x] Play-tested headlessly: `docs/media/m2-day.png`, `m2-dusk.png`, `m2-glow-crystal-at-night.png`
- [ ] Nathan's play-test of M2

### Known gaps and next ideas

- Texture weak points the painter named: stone reads a little like cobblestone; water and snow repeat over large areas (no variants yet); glow crystal is regular; foliage is busy at 1:1. Re-run `bm-texgen` after tuning.
- Water is static and its surface is a flat sheet; animated water is a later milestone.
- Leaves are opaque cubes; cut-out leaves need an alpha-tested pass.
- Prediction corrections occasionally appear around one's own edits (the client applies an edit a round trip before the server does). Small, and smoothed; revisit if Nathan notices them.

