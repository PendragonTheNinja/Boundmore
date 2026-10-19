# Milestone 7: Movement and Shape

*Spec written 2026-10-19. Part 1 (chisels) built 2026-10-19; part 2 (grapples) built 2026-10-19.*

## Goal

Building gets its shape tools and travel gets its grapple, as Boundless players remember them: chisels in square, slope and bevel (with precise versions) plus a round family to try, and a grappling hook to swing and climb with.

## Part 1: Chisels (acceptance)

1. **Shapes** (ADR-0008): a block can be cut into any of 254 shapes per family on a 2×2×2 grid; cut blocks save, stream and render like any other.
2. **Chisels:** square (stone), slope (copper), bevel and round (iron), precise square and precise slope (iron). Left click cuts where you aim (one eighth with a precise chisel, an edge's two otherwise); right click with the chisel fills back. Each cut uses durability.
3. **Rules:** natural terrain takes slopes only; building blocks take every family; machines, ores, plants and liquids take none; a block keeps the family of its first cut; claims protect against chisels like any edit.
4. **Looks:** square cuts are half-block cubes; slopes are planes through the block's corners; bevels are 45 degree cuts; rounds are curves. Textures and colours wrap the cut faces; light and shading look right; no cracks or holes between neighbours.
5. **Feel:** a preview outline shows what a click will cut. Standing on and walking up cut blocks is smooth (half-block steps).
6. **Gathering:** breaking a cut block gives the whole block back (as in Boundless).

## Part 2: Grapples (acceptance)

Fire a hook that sticks where it lands, swing on the rope, reel in and out; tiers with longer range and faster reel. Deterministic on client and server (invariant 2). Numbers are calibration targets for Nathan (`docs/feel.md`), seeded from the research (`docs/research/boundless-mechanics.md` §2).

1. **The hook is part of movement.** `PlayerState` carries the hook; `movement::step` flies it, sticks it and pulls on the rope, so the client predicts a swing exactly as the server runs it. The input names the hotbar slot in hand; each side looks the grapple up in its own copy of the inventory (the server never takes the client's word for what it holds).
2. **Firing:** left click with a grapple in hand fires the hook from the eye where you aim. It flies straight (no gravity, as in Boundless after its early releases) at the grapple's shot speed and sticks in the first solid block (or kept octant of a cut block) it meets. One that flies its whole range without hitting anything is lost. Clicking again lets go (or recalls a hook in flight). Putting the grapple away lets go. Each shot uses one point of durability.
3. **The rope** holds you at its length from the hook and never pushes: once taut it removes speed away from the hook, so gravity turns into swing. **R** reels in (taking up any slack at once, so it always pulls), **F** lets out, both at the grapple's reel speed, between 1 block and the grapple's range. A **hookshot** reels in by itself.
4. **Swinging:** WASD leans while hanging in the air (adds speed, no cap, no braking). Letting go above sprint speed keeps the fling's speed, fading slowly, instead of the usual air braking; ordinary jumps are unchanged.
5. **Breaking the block** the hook is in lets go. A player who leaves, travels or reconnects has no hook (it is never saved; saves keep their old layout, no save version change).
6. **Others see it:** snapshots carry each player's hook (protocol 11), drawn as a rope from their hands to a small hook.
7. **Tiers** (`assets/items.toml`, workbench): copper grapple (range 16, reel 4), iron grapple (range 26, reel 7), iron hookshot (range 20, reel 16, reels itself).

Not in part 2: two hooks at once (one per hand), hooking players or creatures, grapple skills and forged traits, stamina. There is no fall damage in Boundmore yet, so "no fall damage while attached" waits for damage.

## Non-goals

Lattice and other forged chisel effects, undo history, chisel skills, smooth ramp collision.

## Status

- [x] Part 1 contract: shape encoding, chisel items and recipes, catalogue flags, protocol 10
- [x] Shaped geometry, meshing and rendering: per-block templates per family and mask, a 16-byte shaped vertex (1/256 block positions, normals), its own pipelines; aiming with `raycast_shaped`; cut preview (yellow) and fill preview (blue); screenshots `docs/media/m7-*.png`
- [x] Core and server: kept-octant collision, a 0.55 step-up (half blocks only, never a full block), shaped raycast, light through cut blocks, the chisel rules and wear, cut blocks give back the whole block, lapse ash keeps the cut shape
- [x] Chisel icons
- [x] Integrated: 287 tests, fmt and clippy clean
- [x] Follow-up: move the chisel rules into bm-core so client and server share one copy (`bm_core::chisel::apply`, 2026-10-19)
- [x] Part 2 core: hook flight, sticking, rope constraint, reel, hookshot, lean and fling in `movement::step`; grapple items, recipes and icons; protocol 11 (input slot, fire, reel; hooks in snapshots); the server wears grapples per shot; saved bodies keep the pre-grapple layout (upgrade check from `main`: players kept)
- [x] Part 2 client: prediction replays with the grapple each input held, R and F reel, click-to-fire latched so no shot is lost between frames, ropes and hooks drawn for everyone, tooltips, `--demo-ui grapple`; screenshot `docs/media/m7-grapple.png`
- [x] Integrated: 299 tests, fmt and clippy clean, 50-bot load test the same as `main`
- [ ] Nathan's play-test (chisels and grapples)
