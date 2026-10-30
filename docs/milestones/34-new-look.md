# Milestone 34: New Look

*Spec drafted 2026-10-30.* Nathan, 2026-10-30: "I was hoping for an overall texture redesign" (`docs/feel.md`, "Held items and the look"). Today's blocks are 32 px faces painted in clumps of pixels (`docs/art-direction.md`); on the ground they read as a fine, busy grid of identical squares (`docs/media/m32-slopes.jpg`), far from the soft, painterly, bright worlds of Boundless. Part 1 is a **trial on about ten key blocks**, for Nathan's eye, before the whole catalogue is touched.

## Goal

The land reads as painted land, not as a grid: large soft shapes, gentle gradients, colour that drifts across a field, grass that softens over its edge, ores that still sparkle as jewels in their stone, and every block still readable from a distance. Part 1 proves the method on a few blocks, shows it side by side with today's look, and costs out the full pass.

## What Boundless did (forum, Discourse JSON API)

Studied as a player and from the developers' own posts; no texture or art of theirs is used or copied.

- **Patterns spanning several blocks.** A Boundless artist (t/13159, 2018-05-01): "The base version of sedimentary rock has two different tiling materials: one applied to the top and bottom of the block, and one applied to the sides. Both of these are 3x3 tiles, which means they have nine individual textures that go together to make a three metre-by-three metre square that tiles with the next three-by-three square... Most of the natural base materials like rock and grass work in this way; a larger tile size means you minimise obviously repeating details." A player relaying a developer (t/12734 #29): "we also have 3x3 tilings of textures for many blocks, which requires the worlds to be multiples of 3 in size".
- **High resolution, painted from sculpted surfaces.** The same post: each texture is 256 x 256 per block face, with a colour map, a greyscale gradient map and a mask (the tintable parts), a normal map ("tells the engine how to light the texture") and a specular map. Release 102 (t/973, 2015): "over 4x the pixel density of the current textures. Each block is created as a highly detailed 3D surface, which is then baked into" colour, normal, specular, emissive, alpha and reflection maps; the trunk had four side textures and a top.
- **Blending between blocks.** t/973: "Grass blends and creeps onto stone, sand scatters over timber. As well as that, we have a shaded blend that makes the blocks look solidly embedded into the environment." Texture blending for newer blocks arrived in the C++ client (t/4453), and players noticed how much it changed the look ("Wow, it's amazing how much blending changes it!").
- **Lit, not baked, detail.** t/973: "Every bit of light information in the current textures is essentially fake... The new textures look weighty, solid", because the normal maps catch the moving sun.
- **Texture level of detail** with distance (t/12681), and players asked for texture packs (t/13159; Nathan was in that thread).

What we take: the **mechanics of the look** (spans of several blocks for natural materials, higher pixel density, soft painted forms, colour that varies over the land, grass softened onto its sides), never the textures, colours or motifs.

## The method (part 1, the trial)

Three independent pieces, each switchable, so Nathan can judge each one:

1. **Painterly textures at 128 px** (`tools/bm-texgen/src/painterly.rs`, `cargo run --release -p bm-texgen -- --look painterly`). Each material is built as a height field of large shapes (rocks, clods, clumps of blades, planks, bricks, bark ridges), lit softly from the top left, coloured through a hue-shifting ramp (cool shadows, warm lights), then softened; little per-pixel noise. Written to a separate texture set, `assets/looks/painterly/blocks/`, so the default game is untouched.
2. **Spans: one texture across several blocks.** A texture's metadata may say `span = 2` or `4`: its picture covers 2 x 2 or 4 x 4 blocks, sampled by the block's position in the world, so the repeat is that many blocks apart and the block grid stops showing. Boundless used 3 x 3; our planets are whole chunks of 32 blocks around (ADR-0002), so a span must divide 32 (1, 2, 4, 8, 16). The shader takes the face coordinate from the chunk-local position divided by the span; chunk origins are multiples of 32, so the pattern is continuous across chunks and across the planet's seam with no lap logic (invariant 4), and cut and sloped shapes take the same coordinate as cubes. Variants (art direction rule 7) are picked per span cell, so a span texture's variants must share their edges, as today's do.
3. **Colour drifting over the land.** A texture may say `vary = 1..3`: the shader shifts its brightness and warmth by a smooth value noise of the world position (lattice of 32 and 16 blocks), wrapped at the planet's width so the seam is continuous, the same on a block's top and sides. It is mirrored on the CPU (`bm-client/src/look.rs`) with tests for the seam and for chunk borders.

Plus, inside the textures: the turf side's grass hangs over the top edge in soft rounded tufts with a shaded lip, so the edge between a grass top and its side is not a hard line; ores are painted into their host stone's picture (the same stone, with jewels in each block's cell), so an ore block continues the wall around it.

**Texture metadata** lives with the textures (invariant 14, textures are data): an optional `look.toml` in a texture folder,

```toml
fallback = "../../textures"     # files not here are read from there
[textures.turf_top]
span = 2
vary = 2
```

No catalogue or protocol change: the server never knows. The client opens a look with `--look painterly` (`assets/looks/painterly/`), reading each file from the look first and from the normal texture folder after it. Without `--look` nothing changes.

**Filtering.** Today's 32 px blocks are magnified nearest-neighbour (crisp pixels). When the texture array is larger than 32 (a look with 128 px files), magnification is linear, so painted faces stay soft up close; the 32 px files in such a set are first scaled up by whole pixels, so they still look crisp (each of their pixels becomes a 4 x 4 square, softened only at its rim).

## Acceptance (part 1)

- About ten trial textures painted at 128 px: grass top and side, soil, stone, sand, a trunk's side and top, its leaves, planks (timber), one brick, water, one ore; a contact sheet `docs/media/m34-new-look-sheet.png`.
- Spans and land colour in the terrain shader, behind texture metadata; the default texture set has none, so the default game is pixel for pixel unchanged.
- Before and after screenshots of the same views (a meadow on a fresh server, a close-up of a wall, a far view) in `docs/media/m34-before-after.jpg`.
- Tests: the metadata parses and packs; the span coordinate is continuous across chunk borders and the seam; the land colour noise is continuous across the seam and chunk borders and stays in range; the painterly textures tile, are 128 px and deterministic.
- `cargo fmt`, `clippy -D warnings` and the whole test suite pass.

## Non-goals (part 1)

- Converting the whole catalogue (about 400 textures). Nathan judges the trial first.
- Normal and specular maps, and blending between neighbouring blocks (grass creeping onto stone, the shaded blend). Both are what made Boundless's look; both need more than the texture pass (a second texture array and a lighting term; neighbour data from the mesher). They are candidates for part 3 below.
- Item icons: block icons are drawn from the block's layers and follow the look automatically; painted item icons are the audit's separate item.
- AI textures (Flow). The pipeline still accepts them: a look folder is just files, and `look.toml` can give an AI texture a span.

## Decisions for Nathan

1. **Direction:** this procedural painterly set, his Flow AI textures in the same pipeline, or a mix (procedural for anything with an exact layout, AI for natural surfaces).
2. **Pixel density:** 64 px per block (a 128 px texture over 2 x 2 blocks; what the trial uses for most natural blocks) or 128 per block. The array is one size for every layer: all at 128 is about 35 MB on the GPU with mips, all at 256 about 140 MB.
3. **Span per material:** 2 x 2 or 4 x 4 (Boundless's 3 x 3 does not divide our planets).
4. **Soft or crisp up close** (linear or nearest magnification).
5. **How strong the land colour drift is**, and whether built blocks get any (the trial: natural blocks only).
6. **Default or a setting:** whether the new look replaces the 32s outright, or the 32 px "classic" set stays as an option.

## Rolling it out (part 2, after Nathan's verdict)

What converting all about 400 block textures takes, from the trial:

- **Natural surfaces, about 60 textures** (the three rocks, deep stone, bedrock, soils, the 15 biomes' turfs, sands, gravel, snow, ice, ash, mud, clay, peat, the ores and gems, trunks and leaves of each tree kind, water and lava): one painter each in the painterly module, spans and land colour set in `look.toml`. Most share a painter with parameters (one rock painter, one turf painter, one ore painter), so about 15 painters. The biggest single piece of work.
- **Building blocks, about 200** (bricks, refined, carved and tiled in each rock, woods, metals, gems, glass, doors): painted from a handful of pattern painters per family, as `building.rs` does today; span 1 (a brick course is a block), 128 px.
- **Machines, stations, shrines, signs, furnishings, about 100**: laid out by hand today in 32 px units; repainted at 128 with the same layouts, or kept at 32 and scaled up crisp (they read well as they are).
- **Props (plants, torches, fungi)**: carry their own texture coordinates; repaint at 128 or keep.
- **Then** make the new set the default (or a setting), retire `--look`, and update `docs/art-direction.md` (rule 1's 32 px becomes 128, rule 3's clumps become brush shapes).
- Rough size: two to three sessions of painters and screenshots for natural surfaces, two for building blocks, one for machines; Nathan's eye after each.

## Possible part 3

Normal maps from each painter's own height field (the painter already has it), lit by the sun in the terrain shader; a grass fringe that creeps onto a neighbouring block's side; a darker contact shade where a block meets the ground. Each is its own decision.

## Status

**In progress (2026-10-30), part 1.** Spec written.
