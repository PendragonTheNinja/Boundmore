# Proposal: 128 x 128 textures

*Written 2026-10-21. Status: **trial run (2026-10-22), awaiting Nathan's pick of direction.** Results in "Trial results" at the end.*

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

## Trial results (2026-10-22)

**Access works.** The key is injected on the way out; `gemini-3-pro-image` (Nano Banana Pro) and `gemini-3.1-flash-image` both answer, with billing on. The pro model is often busy (HTTP 502 or 503); `generate.py` retries with a growing wait.

**What was built** (all on the trial branch, nothing in `assets/textures/` replaced):

- **The client takes any square power-of-two texture from 16 to 256, mixed** (`bm-client/src/textures.rs`). The array uses the largest size; smaller files scale up by whole pixels. Blocks magnify nearest-neighbour and mips are 2x2 boxes, so a 32 beside a 128 looks exactly as it does today (tested). This is the engine half of any size setting, and lets AI art cover natural surfaces while machines keep their painted 32s.
- **`--demo-ui textures`**: one view of turf, bare soil, a turf step, a stone wall with copper ore, and a tree.
- **Procedural 128** (`bm-texgen -- --size 128 --out DIR`): painters work in 32 units at any pixel count, so big shapes keep their place and come out smooth; the six trial painters add finer detail at 128 (more octaves, chips in stone, crumbs in soil, four times the grass blades, pebbles, specks and leaves at half size). Output at 32 is byte-identical to before.
- **The AI pipeline** (`tools/texture-ai/`, Python with Pillow and numpy):
  - `prompts.json`: one shared style prompt from this file's rules, one line per texture. Copper ore is painted into the AI stone and turf side from the AI soil and grass, so they match.
  - `generate.py`: calls the API, saves each image with its prompt in `art/texture-trial/ai-raw/` (this trial's are now in `api-raw/`, cleaned into `api/blocks/`). Varied textures (stone, soil, turf top, foliage) also get three more patches of the same material, painted with the first as reference.
  - `cleanup.py`: the whole image becomes one face; it is made seamless by joining the strip past each edge into the opposite edge along the cut where the two agree best (image quilting: the cut runs along the dark joints, where a cross-fade ghosted); downsized to 128; the extra patches are set into the base along four such cuts in the edge band, so every variant has the base's edges; and each face is checked against the game's recolouring (ADR-0007). Writes `art/texture-trial/ai/blocks/`, `ai-sheet.png` and `ai-report.json` (this trial's: `api/`, `api-sheet.png`, `api-report.json`).

**Nathan's call so far (2026-10-22):** the procedural 128s, upscaled from the 32 painters, are not the way; Gemini should make the textures from scratch. So the AI images use Gemini's own design and colours (the colour nudge toward our palette is off; `--nudge` turns it on).

**What the trial taught:**

- **Scale has to be asked for.** A first full set packed a whole picture into each face (eight stones per block): in game the stone read as small bricks and the foliage as a printed pattern. Asking for bold, readable forms fixed most of it; "big shapes" asked too bluntly turned grass into moss balls.
- **Gemini draws masonry when asked for stone.** "Slabs" gave walls of bricks even with "not bricks" in the prompt; the prompt now asks for fractured bedrock and lists what not to draw.
- **Gemini's "seamless" images are not seamless**, and it will not repaint a seam when asked (it returned the shifted image almost unchanged). The quilting cut in `cleanup.py` handles it.
- **One face per texture repeats visibly** across a field of turf; the variants are needed, and the cut keeps them seamless against each other.
- **Recolouring holds up:** every face keeps a light-and-shade pattern at least as strong as the painted 32's. A few bright highlights pass the shader's cap (bark and foliage, 3 to 5% of texels); they look fine in the recolour previews.

- **Gemini lights the whole image, not each shape**: bright and warm top left, cool bottom right. Every block repeats it, and a field shows a grid. `cleanup.py` flattens drift across the face in OKLab (lightness and hue); it helps, but not enough for grass.
- **Grass is the hard one.** Gemini draws grass as a few big radial rosettes; four per face in a regular layout repeat visibly across a field however the variants are cut, and the field reads darker and duller than today's 32 (`docs/media/texture-trial-32-vs-ai.png`: 32 on top, AI below). Foliage has the same rosette habit. The next prompt (in `prompts.json`, not yet run) asks for an even, low-contrast carpet of blades and random leaves with no rosettes.

**Where it stopped (2026-10-22):** after about 50 images the API began refusing every call, text included: `403 PERMISSION_DENIED, "Lightning dunning decision is deny for project 90537199338"`. That is Google's billing system denying the project (credit, spending cap or a failed payment); Nathan needs to check billing on the key's project in Google AI Studio or Cloud Console. Then: `python tools/texture-ai/generate.py turf_top foliage turf_side`, `python tools/texture-ai/cleanup.py`, and new screenshots.

**In game** (`docs/media/texture-trial-32-vs-ai.png`, sheet `docs/media/texture-trial-ai-sheet.png`): stone, soil, bark, turf side and copper ore are clearly better than the 32s up close: painterly, chunky, readable. Grass and foliage are not there yet (above).

**Cost:** about 50 pro images were made over the trial, at roughly US$0.13 to 0.24 each (Google's list price for 1K to 2K images); a full natural set of about 25 textures with variants is about 100 images.

**Next, if Nathan picks the AI direction:** extend the prompts to every natural surface (deep stone, bedrock, sand, gravel, snow, ice, sandstone, ash, the ores, water, trunk top), keep machines, shrines, torches, plants and icons as painted art, and decide whether 128 replaces 32 or becomes a player setting.

## Decided: Google Flow by hand (Nathan, 2026-10-22)

The API route costs money per image on top of Nathan's Google AI Pro subscription, so **Nathan generates the textures himself in Google Flow** (Nano Banana Pro, included in AI Pro). Flow makes four images per prompt, takes a square shape and adds no visible watermark (the Gemini app puts a mark in a corner). Flow has no Gems, so every prompt carries the whole style text.

**The look** (Nathan, 2026-10-22): the soft, painterly look of Boundless's textures, in our own art. The style prompt asks for blended brushwork, soft edges and no dark outlines (the API trial's images were outlined and harder), and never names Boundless (CLAUDE.md: expression may not be copied, and naming it invites imitation).

**The workflow:**

1. `tools/texture-ai/flow-prompts.md` (written by `make_flow_prompts.py` from `prompts.json`) lists all 24 textures in working order, grass and foliage first, each as one copy-paste prompt with its file name and any reference images (ores are painted into Nathan's chosen stone; turf side from his soil and grass), plus the variant prompt for the 8 varied textures.
2. Nathan keeps the best of each four, saves it in `art/texture-trial/ai-raw/` under the given name, and pushes.
3. Claude runs `cleanup.py` (seams, variants, lighting drift, 128, recolour check), puts the set in the game with `--demo-ui textures`, sends screenshots and says which need another try.

`generate.py` stays for the API, should it be wanted again.

**One stone for now** (Nathan, 2026-10-22): the stone made in this pass is one texture; it will later be one of Boundless's three rock kinds (`docs/roadmap.md`, Later art passes).
