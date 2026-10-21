# Proposal: 128 x 128 textures

*Written 2026-10-21. Status: **trial agreed, not started.** Nathan chose the Gemini API route; the trial runs once milestone 13 part 2 is merged.*

## Why

Boundless painted its blocks at 256 x 256: smooth and painterly. Ours are 32 x 32 by choice (`docs/art-direction.md`). Nathan asked whether we could go to 128 (2026-10-21).

## What the quick test showed (2026-10-21)

The test set `tools/bm-texgen`'s `SIZE` to 128 in a scratch copy; nothing was committed.

- **57 of 66 block textures** paint at 128 unchanged. The 9 that crash place pixels by hand: timber, and the crafting table, workbench, furnace and extractor faces.
- **Painters size features in pixels.** At 128 that gives four times as many small stones, not the same stones finer. Scaling site spacing by `SIZE / 32` (a one-line change in `Ctx::sites_stretched`) fixes the big shapes:
  - **Better:** stone, bedrock, deep stone, bark, glow crystal, gravel, brick, sand, water and ice look better.
  - **Too empty:** small details were not multiplied: foliage leaves, ore pockets, pebbles and grass tufts.
  - **Shrunk into a corner:** plants, torches, fungi, boulders and the station and shrine emblems are drawn at their old pixel size, small in one corner.
- **Nathan's verdict:** many of the 128s "don't look nearly as good".
- **Engine side, small:**
  - the client assumes 32 in one constant (`textures.rs` `SIZE`), plus the mip count and the crack overlay;
  - 128 for every block is about 9 MB on the GPU.

## Decided (Nathan, 2026-10-21)

**Try AI-generated textures through the Gemini API** (Nano Banana, Google's image model), with a cleanup script, against a proper procedural 128 pass.

**Access.** Nathan stores the API key as a network secret in each Claude account's cloud environment:
- header `x-goog-api-key`, no prefix;
- allowed host `generativelanguage.googleapis.com`.

The key is added to requests on the way out and never appears in the session. Never put a key in the repo or ask for it in chat.

**Billing.** Nathan's Google AI Pro subscription covers the Gemini app, not the API. If the image model refuses on the free tier, say so and ask Nathan to enable billing on the key's project (with a spending cap). Nothing is mocked.

## The trial

1. **Check access first.** List the models: `curl https://generativelanguage.googleapis.com/v1beta/models`, which should answer 200 once the secret is in. Then find the image model's current name in that list.
2. **Pick six textures:** stone, turf top, soil, trunk bark, copper ore and foliage.
3. **Make each two ways:**
   - **AI:** one style prompt shared by all six (from `docs/art-direction.md`: light from the top left, colour shifts instead of plain darkening, clumped detail, Boundmore's own look, never Boundless's), generated large.
   - **Procedural:** a real 128 pass of the same six painters, where big shapes scale and fine detail multiplies.
4. **Clean up the AI images** with a script under `tools/` that:
   - crops and downsizes to 128;
   - makes the image tile seamlessly;
   - nudges its colours toward our palette;
   - cuts 3 variants from one larger image;
   - checks the image still reads when the game recolours it by brightness (ADR-0007).
5. **Show Nathan the results in the game**, not on flat sheets: today's 32, the procedural 128 and the AI 128, side by side on a demo patch, in screenshots. He picks the direction.
6. **Then plan the full pass:**
   - a size setting (32, 64 or 128) in both the generator and the client (recommended: a player setting, not a replacement);
   - all 66 painters, or the AI pipeline for natural surfaces with procedural art kept for anything with an exact layout (machines, shrines, torches, plants, icons);
   - the 9 hand-placed painters.

The generator also writes the comparison sheets (`docs/media/`). Each painted image, prompt and result goes in the repo, so any session on either account can continue.
