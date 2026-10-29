# Milestone 29: Models

*Spec drafted 2026-10-29.* Players and creatures are drawn as coloured boxes (`BoxInstance` in `bm-client/src/render.rs`). Nathan wants smooth, sculpted characters like Boundless's (2026-10-29), in several playable races, after unused concept art he likes (his picks 2, 14 and 22, followed in spirit, never copied). The scripted races (`tools/bm-modelgen/`, `docs/media/m29-races.jpg`) proved the pipeline but not the look ("the quality just isn't there yet", Nathan, 2026-10-29), so the final models will come from elsewhere: AI concept images in Flow turned into 3D by an image-to-3D tool (trial: `tools/model-ai/raptor-trial.md`), or an artist. This milestone builds everything that does not depend on where the art comes from.

## What Boundless did (forum dev updates, 2017)

- A character is assembled from parts: a library of faces and head pieces (horns, crests), body type and scale, body colour, eye texture and colour, and body markings in three levels of elaboration with three colour areas and a strength slider (Weekly Dev Update, 2017-05-26, topic 7731; topic 7472).
- Appearance could be changed later (free for the subscription, kits for cubits otherwise; topic 41072). Races had no gameplay differences.
- Tinting: one colour, or three with the subscription ("advanced tinting").

## The pipeline (built in part 1)

1. **Source:** any sculpted model (AI or artist) is brought into Blender, fitted to the shared skeleton (`tools/bm-modelgen/kit.py`: the bone names every race shares, so one set of animations plays on all), reduced to about 10,000 triangles, its detail baked into a normal map, and its four colour areas painted into a mask texture (red belly, green markings, blue claws and scales; alpha the painted shading). The scripted races already come out this way.
2. **Files:** `assets/models/<race>.glb` (mesh, UVs, skin weights, skeleton, animation clips), `<race>.paint.png`, `<race>.normal.png`. Missing files fall back to the box (as a missing texture falls back to a colour, invariant 14).
3. **Clips:** idle, walk, run, jump, fall, swim, swing (per tool class later), grapple hang, defeat. The scripted kit has idle and walk; the rest are added to the kit and retargeted onto each source model.

## Part 1: models in the game (no decision needed)

- Load glTF 2.0 skinned meshes in the client (the `gltf` crate), with their textures; upload once per race.
- **GPU skinning**: a skinned-mesh pipeline beside the box pipeline (reversed-Z, the same lighting as boxes: sky and coloured block light at the body's position, held lights), up to 64 bones a model, bone matrices per instance in a storage buffer.
- **Animation**: sample clips on the client from what a player is doing, with no new network data: speed and ground contact from the interpolated positions, swimming from the liquid at the chest, hanging from the hook offsets, swings from the swing events already sent, defeat from the snapshot. Blend between clips over about 0.2 s. Feet keep their stride to the ground speed.
- **Tint**: four colours and an eye colour per player, applied in the shader from the paint texture (`tint = mix(skin, belly, r); mix(.., mark, g); mix(.., claw, b); * a`).
- **Other players** drawn as their race's model, facing their yaw, head turned by their pitch; held items attached to the hand bones later (part 3).
- **Distance**: fade to a simpler copy (about 2,000 triangles, made by the same bake) past about 48 blocks, and to a box past the creature draw distance; a budget on skinned instances a frame.
- **Our own body**: first person stays as it is (no body drawn); a third-person camera toggle (F5) shows our own model, useful for screenshots and the character creator.
- Placeholder art: the scripted raptor until the trial models arrive.
- Tests: glTF loading of a kit model (bone count, clip names, weights summing to one), skinning matrices against Blender's pose for a known frame, clip choice from movement samples, the fallback to a box when files are missing; `--demo-ui models` shows every race walking and idling.

## Part 2: choosing a race and colours (decisions below)

- Appearance in the player record: race id, four colours from the 255-colour palette, eye colour, saved (a new `player_looks` table; no save version change) and sent in the handshake and in a new `ServerMsg::Looks` when someone appears (not in every snapshot).
- A character creator at first login (and a way to change it later): race, colours, eyes, shown on the turning model.

## Part 3: later

Creatures as models (the same pipeline: each creature kind its own skeleton and clips); held items and tools in hand, seen by others; first-person arms; Boundless-like head pieces and markings as swappable parts.

## Decisions for Nathan

1. **Races are looks only** (as in Boundless): no stat differences. Recommended.
2. **Changing appearance later**: free at any time, or at a cost (Boundless charged without the subscription). Recommended: free.
3. **Colours**: four tinted areas plus eyes from the full palette for everyone (Boundless gated three-colour tinting behind its subscription). Recommended: everything for everyone.
4. **Art source**: decided after the raptor trial (Flow and image-to-3D, or an artist).

## Non-goals

Facial animation, cloth physics, inverse kinematics on slopes (later polish), armour.

## Status

- [x] Research and spec (2026-10-29)
- [x] Scripted pipeline: the kit, three races, the bake to game models (2026-10-29; `tools/bm-modelgen/`)
- [ ] Raptor trial with an AI-made model (Nathan, `tools/model-ai/raptor-trial.md`)
- [ ] Part 1, models in the game
- [ ] Nathan's answers to Decisions 1 to 3
- [ ] Part 2, choosing a race and colours
