# Milestone 26: Coloured Light

*Spec 2026-10-28.* Every system left on the roadmap (sovereign worlds, more economy) waits on Nathan's decisions; this one needs none. M2 left coloured light out, M9 noted that gem torches need it, and the roadmap lists "gem torches, lanterns and tinted light" as later. A Boundless builder lights a build with glow crystals in the build's own colours; today every light in Boundmore is the same warm white.

## What Boundless did (forum, `research/boundless-tuning.md`, boundlessinfo recipe data)

- **Light has colour.** Since the 2019 lighting update (Testing 224/225, topic 32334) block light is RGB: a gleam lights in its own colour, and **its reach depends on the colour's intensity: light colours go further than dark ones** (Mayumichi, 2019-07-17). Gem torches "lit the area with the corresponding colour" (topic 23405, 2018).
- What players disliked in that update: dark blocks going flat black and a sharp drop-off near the light (topic 32334). Boundmore's light falls off one level per block over 15 levels with a soft curve, and has a small ambient floor, so neither applies; Nathan's eye decides.
- **Lanterns, gleam lanterns and gem torches give more light than gleam** (topic 32334: "gleam as a block that glows slightly").
- **Gem torches** (six: amethyst, diamond, emerald, topaz, ruby, sapphire): workbench, 1 stick + 1 rough gem makes 2, 60 s, spark 800, 20 XP, Decoration Crafting 3.
- **Gleam lantern:** refinery, 1 gleam + 2 of any base metal + 1 glowing lamella, 60 s, spark 400, 18 XP, Decoration Crafting 3; it takes its colour from the gleam.
- Already in Boundmore: the basic torch, the better (tallow) torch, the lantern (torch and glass), the glow boon, the light-source epic.
- Not published anywhere: any light's level or radius (tuning, "still not published"). Every level below is a **guess** for Measure F2 and F4.

## The rules

1. **Block light is three channels**, red, green and blue, 0 to 15 each, beside sky light. Each channel spreads exactly as block light does now (one less per block, opaque cells hold only their own emission), independently of the others. A cell is therefore a two-byte value (sky, red, green, blue). Nothing about sky light changes.
2. **A light's emission per channel is its level times its colour** (sRGB, 0 to 255): `round(level x c / 255)`. White light 12 is (12, 12, 12); a ruby torch's red reaches 14 blocks and its blue 4. So light colours reach further than dark ones, as in Boundless, and a light's colour shifts towards its strongest channel at the edge of its reach.
3. **Each emitting block names its light colour** (`light_colour` in `assets/blocks.toml`, default white). **Blocks that take a colour light in it** (`light_tint = true`: glow crystals, refined and faceted glow crystal, the crystal furnishings and the crystal lantern): painted, they light in the palette colour; natural, in their own `light_colour`. Glow crystal on a world is generated in the world's colours, so each world's crystal lights its caves in its own colour.
4. **Held light has colour too**: a held torch in the torch's colour, a gem torch in its gem's, the glow boon and the light-source epic white. A held light still lights its surroundings through the shader (no re-mesh), per channel by the same rule.
5. Glass and water do not filter colour (a non-goal; Boundless did not).
6. Light is computed on the client only (the server never reads it), saved nowhere and sent nowhere, so no save or chunk format changes. The catalogue carries the new fields, so the protocol version is bumped.

## New blocks and items

- **Six gem torches** with Boundless's recipe, placed on floors and walls like torches (a floor block and four wall blocks each), level 14 (**guess**), in their gem's colour; held, they light like torches.
- **The crystal lantern** (Boundless's gleam lantern), refinery with Boundless's recipe, its colour from the glow crystal (`Recipe::tint`), level 14 (**guess**), lighting in its colour.
- Light colours for the existing emitters, chosen by eye (**guesses**): torches warm, furnaces and lava fire-orange, portals, wayshards and shrines violet, glowcaps teal, machines a cool white.

## Acceptance

- A painted glow crystal lights a dark room in its colour; two different colours mix where their light meets; a dark colour lights a smaller area than a light one.
- Light stays exact: the light map's existing property tests (random edits against a from-scratch recompute) pass with coloured emitters, including removal and refill of one colour beside another.
- Meshing carries all three channels to the shader; a cube vertex stays 8 bytes.
- The light benchmark (`bm-mesh/examples/light_bench.rs`) costs no more than about one and a half times what it did, and memory for dense light no more than twice.
- Gem torches and the crystal lantern can be crafted, placed, held, and seen in a screenshot (`--demo-ui torches`).

## Non-goals

Coloured glass or water filtering light; crops growing by light (ADR-0010: crops grow by the clock); shadows from the sun; light from creatures or effects.

## Status

- [x] Research and spec (2026-10-28)
- [x] Part 1, RGB light (2026-10-28): `Light` is two bytes (sky, red, green, blue); the light map runs four independent channels (`CHANNELS`), emission per channel from each block's `light_colour` and, for `light_tint` blocks, their palette colour (`LightMap::with_colours`, given the server's palette by the client); `BlockFlags::emission`. The cube vertex keeps 8 bytes (green and blue in the spare bits of `a`); the cut-block vertex gained a fifth word (20 bytes). Shaders take block light as three channels; held lights carry a level per channel (`ItemDef::held_colour`, `held_rgb_of`; `RemotePlayer::light` is `[u8; 3]`, protocol 52), blasts flash warm, a meteor fight glows orange; boxes (players, creatures) take coloured light too. Tests: the light map's random-edit, load-order and forgetting tests now run with painted crystals and torches beside each other against a four-channel reference solver; mesh, client and end-to-end tests updated.
- [x] Glow crystals in their worlds' colours (2026-10-28): each kind lists bright glow crystal colours (`assets/planets.toml`), wandering worlds draw theirs from anywhere on the colour wheel (`exo::Role::Glow`); the crystal family (raw, refined, faceted, doors, trapdoors, poles, beams, storage, signs) and the clear glasses and panes are now colourable and their Boundless recipes carry the crystal's colour (`tint`). Planets saved before keep natural crystals and their terrain (`PINNED_M25`, `PINNED_V4_M25`); the kinds were re-pinned without a generator bump (kind data only).
- [x] Part 2 (2026-10-28): light colours for every emitter (guesses by eye); the six gem torches (30 blocks: floor and four walls each) and the crystal lantern with Boundless's recipes; their textures and icons (`tools/bm-texgen/src/lights.rs`).
- Cost (`light_bench`, release, against the base commit): lighting a column 1.18 ms (was 0.83, 1.4x); a block edit 20 us on average (was 12, 1.7x: each emitter now spreads three channels); dense light 64 KB a chunk (was 32). Meshing unchanged.
- [x] Renamed in game (Nathan, 2026-10-28, "Lume for now"): the glow crystal family is shown as lume (`BlockDef::display`, `display_name`; 27 blocks; the mining feat "Lume Cutter"). Internal names and saves are unchanged. Protocol 53.
- [x] Screenshot (2026-10-28): `docs/media/m26-lights.jpg` (`--demo-ui lights`: the torch hall with six gem torches, seven lume colours down the floor, a lume lantern and an amethyst torch in hand). Test counts in CLAUDE.md.
- [x] Far views light in colour too (2026-10-28): far cells that give off light spread it, coloured, over the far copy's coarse cells (`bm-client/src/far.rs`, `far_glow`); a neighbourhood with no emitter (checked on each copy's palette) costs nothing. Test: a far lume lump lights the cells round it red, fading out of reach.
- [ ] Nathan's play-test; Measure F4 and F5
