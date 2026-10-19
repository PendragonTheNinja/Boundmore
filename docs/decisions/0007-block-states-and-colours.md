# ADR-0007: Block states carry colour and shape; a 255-colour palette

- **Status:** Accepted, 2026-10-19.

## Context

Boundless gave every block a colour from one shared palette (research: `docs/research/boundless-mechanics.md` §4): each planet colours its natural blocks from that palette, a gathered block keeps its colour in the pack, and colour is part of an item's identity. Colour rarity drove trade and exploration. A lapsed build turns to ash that keeps each block's colour (Nathan, 2026-10-19). Chisels (next milestones) give blocks shapes. Until now a voxel was only a `BlockId`.

## Decision

- **A voxel is a `BlockState { id, colour, shape }`.** `Colour(u8)`: 0 is natural (the block as its texture is painted), 1 to 255 index the palette. `Shape(u16)`: 0 is the full cube; the encoding of cut shapes belongs to the chisel milestone, but the slot exists now so chunks change format once.
- **Chunks store states in their palette.** Per-voxel cost is unchanged (indices into the palette); a chunk of one colour of stone still stores one entry. `Chunk::get` keeps returning the type, which is all physics, light, tools and generation ask about; `state`/`set_state` carry the rest. `CHUNK_FORMAT_VERSION` 2 adds colour and shape to each palette entry; version 1 still decodes, as natural colour and full shape, so every existing save opens unchanged.
- **The palette is data** (`assets/colours.toml`), 255 colours of Boundmore's own (16 hues and greys, 15 shades each, generated in OKLCH so shades step evenly), sent in the welcome. Ids are positions: append, never reorder.
- **Colour belongs to the item.** `ItemStack::colour`; stacks of different colours never merge; a placed block takes its stack's colour, a broken block that yields itself keeps its colour. Blocks flagged `colourable = false` (ores, machines, water, shrines) always stay natural. Saved stacks write a colour as `name#id` so older saves read as natural with no new save version.
- **Rendering:** the colour travels in the top 8 bits of the vertex (`Vertex::b`), so a colour change re-meshes one chunk and light is untouched. The greedy mesher never merges faces of different colours. The shader recolours a texture by keeping its light and shade pattern and replacing its hue with the palette colour, so the painted textures need no greyscale copies.
- **Fingerprints hash the blocks, not the encoding**, so a chunk format change never looks like a terrain change.

## Consequences

- Natural terrain colours per planet are generation output: adding them bumps `GENERATOR_VERSION`, and planets made before keep their natural colours.
- Recipes take inputs of any colour. Which crafts keep a colour (timber from a coloured trunk, glass from coloured sand) is a later, data-driven decision.
- A voxel state could later grow (rotation, water level) by widening the palette entry and bumping the chunk format again; per-voxel memory stays index-sized.
