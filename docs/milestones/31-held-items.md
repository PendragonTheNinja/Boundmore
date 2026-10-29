# Milestone 31: Held Items

*Spec drafted 2026-10-29.* Nathan, 2026-10-29 (polish batch 3, `docs/feel.md`): "Yes I would like handheld models for items held in first person". The feel audit ranked it the second largest feel gap after sound (`docs/audit/feel-and-controls.md`, ranked item 9): today only a sling and a bag of bombs are drawn in hand, as boxes, and nothing moves when you swing, place, eat or fire a grapple.

## Goal

Whatever each hand holds is drawn in first person, in that hand's corner of the view (milestone 28: the left hand's item at the lower left, the right's at the lower right), and moves with what the player does: a bob while walking, a sway that trails the camera, a motion for every use on the use's own cadence, and a lowered pose while sprinting or swimming. It must sit well beside the painterly icons and be lit like everything else in the world.

## What Boundless did (forum dev updates; the wiki has nothing on it)

- **Each hand showed its own item**, held by a first-person arm, the left hand's on the left and the right's on the right (two hands, topic 3195; the two-hand rules are in M28).
- **One shared default pose for every tool**, with core animations per action, "so all 1st person tool animations share the same default positioning, core animations and perspective for consistency" (Weekly Dev Update 2017-04-21, topic 7255). Tools had their own 3D models.
- **The held item kept its size on any screen shape**: "a neat system that scales the 1st person tools and weapons to compensate for changes in screen aspect ratio. This means the tools/weapons are now centralised" (2017-02-24, topic 6570).
- **Field of view was capped at 110 because the first-person animations and items distorted beyond it** (a developer, topic 3547). We avoid the cap's reason by drawing held items with their own fixed projection.
- **A swing was split into a hit and a miss**, matched to the sound and impact effect cues, "for a much more responsive 1st person experience" (topic 7255).
- **Consuming had its own motion** (food, brews), as did the throw of a bomb and the draw of a slingbow (topic 4274's animation collection).
- What players said: the chisel's swing was so fast it hurt to watch while chiselling for long (topic 30947, 2019); swing animation and sound drifted out of step (topic 5089, 2016); jumping while swinging played an extra swing (topic 4135). So: the motion follows the real cadence, never runs faster than the item really acts, and nothing but a real use starts one.

We take the behaviour (one consistent pose per hand, a motion per action, timing tied to the real cadence, size independent of the view), never the models or animations themselves.

## How Boundmore draws them

- **Block items:** a small cube with the block's own top, side and bottom textures, painted in the planet's tint or the stack's colour exactly as the inventory cube icons are (`icons.rs`, `colour::recolour`).
- **Every other item** (tools, grapples, slings, chisels, spanners, food, torches, brews, materials): the item's own 64 px painted icon (`assets/textures/items/<icon>.png`) extruded into a slab a few pixels thick. Each opaque pixel is a column; front and back faces show the icon itself; side faces take the colour of the edge pixel they close. Geometry is greedy-merged (rectangles of opaque pixels for the front and back, runs of exposed edges for the sides), and every face samples the icon with nearest filtering, so the painting keeps every pixel while the model stays a few hundred quads. An item with no picture is not drawn (its initials tile would look wrong in a hand).
- **Built once per item and colour and cached** (models and their textures go to the GPU once; a small budget of texture layers is reused oldest first).
- **Lit like the world's boxes:** sky and block light (in colour) where the player stands, held lights (our own torch lights our own tool), the sun on faces that meet it, and the shading every box gets. A bevel was considered: the edge faces' darker shading already gives the slab its rim, so none is added.
- **Its own fixed projection** (70 degrees, `render::projection`, reversed-Z): the field of view option and the sprint widening never stretch it, and its size is a fraction of the screen's height on any aspect ratio. **Drawn after the world** with depth cleared, so it never sinks into a wall; before the menus.

## Motion

- **Rest pose per hand:** grip at the lower outer corner of the view, tool heads up and leaning in towards the crosshair; a block cube held a little lower and nearer the centre.
- **Bob** while walking on the ground, with the distance walked (so it matches the stride at any speed), fading out when standing, in the air or in water.
- **Sway:** the item lags a turn of the camera a little and settles back (a spring on the change of yaw and pitch, clamped).
- **Uses**, each on the real cadence of that action and started only by a real use, in the hand that acted:
  - a tool (and the hand stone, a spanner): an arc down and in towards the crosshair over the tool's own swing time (`bm_core::mining::swing_ticks` through the client's `Swinger`), so a fast tool swings fast and a slow one never looks faster than it is;
  - a block or prop placed, a chisel's cut or fill: a short jab forward;
  - a sling: drawn back and released on its shot, on its cadence;
  - a bag of bombs: raised while lit, a throw on release (the lit bomb and its fuse ring stay, drawn in the hand);
  - food and brews: brought up to the mouth and tipped for the eating time (`PlayerState::chew`);
  - a grapple: a kick back as the hook fires.
- **Lowered** while sprinting (on the ground) or swimming, eased in and out.
- Nothing is drawn while down (defeated).

## Acceptance

1. Every item in either hand is drawn in its hand's corner: blocks as textured cubes in their colour, all else as its icon's slab; the left hand's on the left and the right's on the right (a test checks each side).
2. The extrusion: each opaque pixel gives front and back faces, transparent pixels nothing, side faces only where a pixel meets transparency or the edge, and greedy merging keeps a solid square to 6 faces and a real icon well under one quad per pixel (tests).
3. A tool's swing pose runs over exactly its swing time and is back at rest at the next swing (a test with a slow and a fast tool).
4. Changing the field of view or sprinting does not change the held item's size or place on screen.
5. Held items never clip into walls and are lit like the world: dark in a cave, coloured by nearby lights, lit by a held torch.
6. `--demo-ui held` shows a tool in one hand and a block in the other, swinging; `docs/media/m31-held.jpg`.
7. Checks green, no slower frames (two models of a few hundred quads each, cached).

## Non-goals

First-person arms (they come with the character models of M29), third-person held items (part 2), hit-or-miss variants of the swing, per-tool-class animations beyond the arc, items' own sounds (the sound milestone hooks into the same events), hand-made 3D models of items.

## Part 2 (later)

- **Others' held items:** each remote player's two held items drawn at their hands (needs both hands' items in `RemotePlayer`, a protocol change) and their swing seen by others (from the swing events already sent). Best done with M29's models, at the hand bones.
- Arms holding the items, with M29's race models.

## Decisions for Nathan

1. **Arms:** items only for now (as built), or wait for arms with the character models. Recommended: items now, arms with M29.
2. **Bob and sway strength**, and whether to offer an option to turn the bob off (some players get motion-sick; Boundless had no such option that we found). Built: a gentle bob, on; an option is cheap.
3. **Slab thickness:** built at 4 icon pixels (as thick, for its size, as a 16 px item one pixel deep). Thinner reads like paper, thicker like a toy.
4. **Hit and miss swings** as Boundless did (a recoil on a hit, a follow-through on a miss): later, with the sound milestone's cues, if he wants it.

## Status

- [x] Research and spec (2026-10-29)
- [x] Models (2026-10-29): `bm-client/src/held.rs`. Icons extruded into 4-pixel slabs (front and back as greedy rectangles of opaque pixels, sides as runs of exposed edges, every face sampling the icon with nearest filtering; the iron hammer is about 300 quads for some 1,100 pixels), block items as cubes painted as their icons are (tint or colour), the left hand's model the mirror image of the right's. Built once per item, colour and tint; the renderer keeps 24 texture layers and gives the least recently drawn model's up.
- [x] Renderer (2026-10-29): `HeldGpu` in `render.rs`, `shaders/held.wgsl`: its own pass after the world with depth cleared (reversed-Z, `render::projection` at a fixed 70 degrees, so the field of view option and the sprint widening never touch it), lit as boxes are (sky and coloured block light where we stand, held lights, the sun, face shading).
- [x] Motion (2026-10-29): rest poses per kind (tool, block, sling, bombs, grapple, other), a bob with the stride on the ground, a sway that trails the view, a tool's swing over its own swing time (`Swinger::last`), a jab for a place or a chisel cut, a sling's draw and a bomb's throw on their cadence (drawn over at most 0.6 s), eating over the eating time, a grapple's kick as a hook flies, lowered while sprinting or swimming; nothing while down.
- [x] Old box sling and bag replaced by their icons' slabs (`held_sling`, `effects::held_bombs` gone); the lit bomb and its fuse are drawn above the bag in the same pass.
- [x] `--demo-ui held` (2026-10-29): an iron hammer in the left hand swinging at a stone block on its own cadence, a marble block in the right; `BOUNDMORE_HELD=left,right` names other items. `docs/media/m31-held.jpg`: at rest, mid-strike, a sling and a grapple, and an axe and a torch at midnight (the torch lighting both). Tests: `held::tests` (the extrusion, merging bounds, winding, each hand on its side, the swing on the cadence, uses only from real actions, the bob and lowering, the lit bomb's boxes).
- Known limits: the motion is tuned from screenshots only (Nathan's eye decides); a slow sling's draw and a bomb's throw are drawn over at most 0.6 s; glass blocks are held opaque; an item with no icon picture is not drawn; the jab is the same for a place and a chisel cut.
- [ ] Nathan's play-test and answers
