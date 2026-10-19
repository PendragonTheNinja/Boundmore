# Milestone 7: Movement and Shape

*Spec written 2026-10-19. Part 1 (chisels) built 2026-10-19; part 2 (grapples) next.*

## Goal

Building gets its shape tools and travel gets its grapple, as Boundless players remember them: chisels in square, slope and bevel (with precise versions) plus a round family to try, and a grappling hook to swing and climb with.

## Part 1: Chisels (acceptance)

1. **Shapes** (ADR-0008): a block can be cut into any of 254 shapes per family on a 2×2×2 grid; cut blocks save, stream and render like any other.
2. **Chisels:** square (stone), slope (copper), bevel and round (iron), precise square and precise slope (iron). Left click cuts where you aim (one eighth with a precise chisel, an edge's two otherwise); right click with the chisel fills back. Each cut uses durability.
3. **Rules:** natural terrain takes slopes only; building blocks take every family; machines, ores, plants and liquids take none; a block keeps the family of its first cut; claims protect against chisels like any edit.
4. **Looks:** square cuts are half-block cubes; slopes are planes through the block's corners; bevels are 45 degree cuts; rounds are curves. Textures and colours wrap the cut faces; light and shading look right; no cracks or holes between neighbours.
5. **Feel:** a preview outline shows what a click will cut. Standing on and walking up cut blocks is smooth (half-block steps).
6. **Gathering:** breaking a cut block gives the whole block back (as in Boundless).

## Part 2: Grapples (acceptance, spec to be detailed after part 1)

Fire a hook that sticks where it lands, swing on the rope, reel in and out; no fall damage while attached; tiers with longer range and faster reel. Deterministic on client and server (invariant 2). Numbers are calibration targets for Nathan (`docs/feel.md`), seeded from the research.

## Non-goals

Lattice and other forged chisel effects, undo history, chisel skills, smooth ramp collision.

## Status

- [x] Part 1 contract: shape encoding, chisel items and recipes, catalogue flags, protocol 10
- [x] Shaped geometry, meshing and rendering: per-block templates per family and mask, a 16-byte shaped vertex (1/256 block positions, normals), its own pipelines; aiming with `raycast_shaped`; cut preview (yellow) and fill preview (blue); screenshots `docs/media/m7-*.png`
- [x] Core and server: kept-octant collision, a 0.55 step-up (half blocks only, never a full block), shaped raycast, light through cut blocks, the chisel rules and wear, cut blocks give back the whole block, lapse ash keeps the cut shape
- [x] Chisel icons
- [x] Integrated: 287 tests, fmt and clippy clean
- [ ] Follow-up: move the chisel rules into bm-core so client and server share one copy
- [ ] Part 2: grapples
- [ ] Nathan's play-test
