# Milestone 6: Colour and Ash

*Spec written 2026-10-19. Built 2026-10-19; awaiting Nathan's play-test.*

## Goal

Blocks have colours, as in Boundless: each planet colours its natural blocks from one shared palette, what you gather keeps its colour, and you build with it. Then the claim lapse Nathan decided: after 6 months without a visit the build turns to ash that keeps each block's colour, and what was stored goes back to the owner.

## Acceptance criteria

1. **States** (ADR-0007): chunks, saves and the network carry type, colour and shape per block; old saves open unchanged.
2. **Palette:** 255 named colours of our own, shown by name in item tooltips.
3. **Rendering:** a coloured block keeps its texture's light and detail in the palette colour; natural blocks look exactly as before. Hotbar and inventory icons show the colour.
4. **Gathering and building:** breaking a coloured block gives an item of that colour; stacks of different colours stay apart; placing uses the stack's colour; ores, machines and shrines never take colours.
5. **Planets:** each new planet picks colours for its natural blocks from its kind's choices (`assets/planets.toml`), by seed, with some variation across the planet so exploring finds other colours. `GENERATOR_VERSION` 3; planets created earlier keep natural colours.
6. **Ash and lapse:** a claim whose owner has not visited for `claim_keep_days` (183) lapses, a few plots per second so nothing stalls: every block the owner placed (anything that differs from the generated terrain and is not natural terrain) turns to ash of the nearest palette colour to the block (its own colour if coloured, else its painted colour); the claim is removed; contents of stations and storage blocks in it go to the owner's reclaim. A notice and a chat line say what happened.
7. **Reclaim:** an owner's reclaimed items wait for them; at any warp shrine a Reclaim tab lists them and moves what fits into the inventory.
8. **Testable:** the lapse can be forced on a test server with a short `claim_keep_days`.

## Shared contract (built first, main session)

`BlockState`, `Colour`, `Shape`, `ColourPalette` (bm-core); chunk format 2; `ItemStack::colour`; `BlockDef::colourable`; protocol 9 (`BlockChanged`/`EditResult` carry states, `Welcome.colours`); colour plumbing through server edits and client prediction; `ash` block; mesher colour in the vertex.

## Plan

| Part | Owner |
|---|---|
| Contract, ADR-0007, spec | main session |
| Client: palette upload, recolouring shader, coloured icons, colour names in tooltips | agent (bm-client) |
| Natural colours per planet kind, `GENERATOR_VERSION` 3 | agent (bm-worldgen) |
| Lapse, ash, reclaim (server) and the Reclaim tab (client side through the protocol) | agent (bm-server), then client |
| Ash texture | agent (bm-texgen) |

## Non-goals

Spray paint, the transformation chisel, colour inherited through crafting, colour-rare temporary worlds, chisels.

## Status

- [x] Contract, ADR-0007, spec
- [x] Client: palette uniform, recolouring shader (keeps a texture's light and shade, takes the palette hue; coloured voxels skip the planet tint), coloured icons, "Stone, Rich Azure" tooltips, Reclaim tab and hint. Screenshots `docs/media/m6-*.png`.
- [x] Planet colours: per-kind choices in `planets.toml`, 2 to 4 colours per block picked by seed, large patches with wandering borders, trees one colour; generator 3 (planets made by 2 open unchanged and stay natural). Temperate greens retuned after a look in play.
- [x] Lapse, ash, reclaim: gradual, never loses or duplicates, owner returning stops it, reclaim collected at any shrine.
- [x] Ash texture.
- [x] Integrated: 252 tests, fmt and clippy clean; a universe saved by the M5 server opens (found and fixed: chunk format 1 was refused); new planets render their colours (`m6-temperate-colours.png`).
- [ ] Nathan's play-test. Not seen headlessly: a lapse forced on a real server and collected through the tab (covered end to end in tests).
