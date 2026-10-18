# Art direction

*Set 2026-10-18 with Nathan: "very, very similar to Boundless, but its own textures; maybe a mix of that and Hytale."*

## The look in one paragraph

Bright, saturated high fantasy. Every block reads clearly from a distance and rewards a close look. Textures are **painted, not photographed and not noisy**: soft shapes, clustered detail, a deliberate light direction baked in (light from the top left), colour shifts instead of plain darkening (shadows lean blue or purple, highlights lean warm). From Boundless: the vivid palette, the jewel-like ores and crystals, worlds that each have a strong colour identity. From Hytale: chunky readable shapes, a hand-crafted feel, a little cartoon warmth. Neither game's actual textures, colours or motifs are copied.

## Rules for block textures

1. **32 × 32 pixels per face**, seamless on all four edges, sRGB PNG. The renderer builds mipmaps, so detail must survive being shrunk to 8 × 8: big shapes first, then medium, then a little fine detail.
2. **Readable at distance.** Every block keeps a clear average colour; neighbouring blocks in the catalogue must be distinguishable at 4 × 4.
3. **No per-pixel random noise.** Detail comes in clumps of 2 to 6 pixels (stones, cracks, grain, blades).
4. **Light from the top left** in every texture: a highlight on the top-left edge of each shape and a shade on the bottom-right edge.
5. **Shift hue in shadows and highlights.** Never just multiply by black.
6. **Saturation goes up, not down,** compared with real materials: grass is a rich green, sand a warm gold, stone a cool blue-grey.
7. **Variants against tiling.** A block may have up to 4 variants of a face; the renderer picks one per block position, deterministically.
8. **Ores and crystals glow in their colour**: the vein is a jewel set into its host stone, with a bright rim and a darker core.
9. **Transparency only where the block is translucent** (water, glass) or, later, cut-out (leaves). Translucent textures carry their own alpha.

## Where textures live and how they are found

```
assets/textures/blocks/<texture>.png        32×32, sRGB
assets/textures/blocks/<texture>_<n>.png    optional variants, n = 1..3
```

Each block in `assets/blocks.toml` names its textures:

```toml
textures = { all = "stone" }
textures = { top = "turf_top", side = "turf_side", bottom = "soil" }
```

`top`, `side` and `bottom` each fall back to `all`. A missing file is a loud error at startup, never a silent pink square. Any PNG of the right size dropped into the folder replaces the generated one: that is how hand-painted or AI-generated art enters the game.

## Who makes them

- **Now:** `tools/bm-texgen` paints every texture procedurally (deterministic, so the same command makes the same files), writes them to `assets/textures/blocks/`, and writes a contact sheet to `docs/media/textures.png` for review.
- **Later:** for hero art (creatures, items, UI frames, icons), the plan is AI image generation run by Nathan with prompts Claude writes, then cleaned up and sized by a script. The pipeline above already accepts drop-in files.
