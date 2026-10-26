# Milestone 22: Building Blocks

*Spec written 2026-10-26.* Nathan, 2026-10-26: "Let's continue on to the next milestone!" The economy systems still wait on his decisions; building needs none of them, and it is what Boundless players spend most of their time doing ("Freeform building: every block editable, huge palette, colours", roadmap). Today Boundmore builds with what the ground gives plus timber, brick and glass. Boundless gave builders a whole craft of blocks: bricks, refined and carved stone in each rock and each world's colours, planed and carved wood, marble, metal and gem blocks, glass in three grades, glowing lanterns, then doors, poles, beams, signs and furniture. Research: `docs/research/building-blocks.md` (the game's own recipe data, every building recipe with its inputs, machine, time, spark, power and **which input's colour the output takes**).

## Goal

A player mines red basalt on one world and green limestone on another, and builds a house of red basalt bricks with carved green limestone trim, planed timber floors, clear glass windows tinted by the glow crystal they used, and lanterns. Colour, collected from the worlds, flows through every craft into the build.

## Decided in this spec

1. **Colour passes through crafting, as in Boundless.** A recipe may name one input whose colour the output takes (`tint`). Crafting it, the player picks that colour from the colours of that input they carry (the craft window lists them); the input is taken only from stacks of that colour and every output stack has it. Recipes without `tint` make natural-coloured output, as now. Stations keep each job's colour across restarts (`SAVE_VERSION` 15).
2. **Three stones and three timbers.** Each rock kind breaks into its own stones (`stones` stays the stone rock's, so carried stones are kept; `basalt_stones`, `limestone_stones`), and each trunk into its own timber (`timber` stays the plain trunk's; `pale_timber`, `gnarled_timber`), each taking the rock's or trunk's colour. Recipes that took "stones" or "timber" take "Any Stones" or "Any Timber". Tools and machines made from them stay natural, as in Boundless.
3. **Part 1, the blocks** (Boundless's recipes and numbers; Boundmore's names; the colour from the input Boundless's data names):
   - **Rock, in each of the three rocks:** brick mix (mixer: 6 stones, 1 compact clay, 4 ash, 4 mud), bricks (furnace: 1 mix → 4), refined (refinery: 8 stones → 1), carved and tiled (workbench: 1 refined and 1 wax each). **Marble** from refined stone: plain, bordered and frieze (refinery: 5 refined stone, a bonding agent of rising grade, 2 refined glow crystal → 4). The old clay `brick` stays.
   - **Wood, in each of the three timbers:** refined (refinery: 8 timber → 1) and carved (workbench: 1 refined and 1 wax).
   - **Compact soil, clay and peat** (compactor 10 → 1); **compact gravel** (compactor: 10 gravel and a bonding agent → 4).
   - **Glow crystal** (Boundless's gleam): refined (refinery 1 → 1) and faceted (2 refined, a bonding agent and an ingot); both glow.
   - **Glass:** clear, framed and ornate (refinery: glass with refined glow crystal, framed adds base metal, ornate base alloy). Boundless coloured them by the gleam; Boundmore's glow crystal has no colours yet, so they are natural until it does.
   - **Metal blocks:** the compact, refined and machined copper, iron, silver, gold and titanium already made become placeable blocks; with circuit, filigree and grid blocks of each (refinery or compactor: refined metal with relic fragments, relic components or cogs). One look each, no colour (Boundless's).
   - **Gem blocks:** compact gems become placeable, with a polished block of each gem and astral ore (workbench: 1 refined).
   - **Lantern:** a glowing block of a tallow torch in glass (refinery).
   - **Wax** (extractor: 1 waxy tuber → 4), the carved blocks' polish.
4. **Every new block is chiselable** and colourable where its Boundless counterpart takes a colour (rock, wood, soil, gravel, glow crystal and glass families; not metal or gem blocks).
5. **Part 2: doors and trapdoors** (plain, stylish and ornate in wood, stone, metal and glow crystal). Metal ones are natural until metals carry colours (Boundless coloured them by their metal). **Part 3: furnishings**: poles, beams, glass panes, signs, plinths, tables, chairs and storage variants. Each gets its section here before it is built.

## Part 2: doors and trapdoors

From Boundless (forum, 2016 to 2021): a door is a **single block**; door blocks of one kind side by side or stacked **join into one door** and open together (never across a claim border); **anyone may open a door** unless a **lock** stands beside it inside a claim, when only those with access may; trapdoors are the floor's doors. Recipes from the game's data (`research/building-blocks.md`): plain doors at the crafting table (wood, stone) or refinery (metal, crystal), stylish and ornate at the refinery with power; trapdoors the same at the refinery.

Decided:

6. **Each door kind is two blocks**: shut and open, each naming the other (`BlockDef::swing` and `toggle`). Their state is drawn and felt through the block's shape (ADR-0008's octants), so nothing new is needed to draw or collide them: a **shut door** is a solid half-thick slab standing on the side of its block nearest whoever placed it; an **open door** is a quarter-wide strip at its hinge, passable (not solid). A **shut trapdoor** is a solid half-height slab (the lower half, or the upper when placed against a ceiling); an **open trapdoor** stands up as a passable half slab against one side. They cannot be chiselled.
7. **E opens and shuts.** `ClientMsg::Swing { pos }`: in reach, the server flips the whole joined door (same kind, same orientation, touching by faces, in the same claim or none; at most 64 blocks) and sends the block changes. A door does not shut on a player standing in it.
8. **Locks.** A lock block beside any block of a door on a claim keeps it shut to everyone but the claim's owner.
9. Ingredients Boundmore lacked: **pipe** (workbench: 6 base metal and a relic fragment → 10) and the **lock** (refinery: 3 base metal and 6 cogs).

Rules: `BlockDef::{swing, toggle}`; the shape of a placed door from the placer's side (server, `game::doors`); `ClientMsg::Swing` (protocol 36); 24 door and trapdoor kinds, 48 blocks, their textures; the open block drops the shut one.

Acceptance: a door opens and shuts with E and joins its neighbours; a locked door on a claim opens only for its owner; a player walks through an open door and not a shut one (end to end).

## Rules (part 1)

1. `Recipe::tint: Option<u8>` (an input index) in `assets/recipes.toml` (`tint = 0`); `Recipe::plan` takes the chosen colour for that input; `Inventory` counts and removes by item and colour.
2. Protocol: `ClientMsg::Craft` gains `colour` (`PROTOCOL_VERSION` 35). The server refuses a colour the player does not carry enough of, and ignores it for an untinted recipe.
3. Stations: `Job::colour`; saved beside the jobs (`SavedStation::job_colours`, `SAVE_VERSION` 15; older stations' jobs are natural).
4. Client: the craft window shows a tinted recipe's colour choices as swatches (the colours of that input in the pack, most plentiful first) and the output in the chosen colour; counts follow the chosen colour.
5. Blocks and items in the catalogues; textures painted by `bm-texgen` (each rock's and wood's family recognisably its rock or wood, each family recognisably bricks, refined, carved, tiled, as Boundless's looked related within a family); recipes in `assets/recipes.toml`.

## Acceptance (part 1)

1. Breaking coloured basalt and crafting it through stones, refined and carved gives carved basalt in that colour; two colours of basalt in the pack give a choice of two (end-to-end test).
2. A tinted craft at a station keeps its colour across a restart (end-to-end).
3. Every new block has a texture, a recipe, and places, breaks and chisels like any block.
4. Existing universes open: carried stones and timber are kept, queued jobs finish natural, and items that became blocks (compact copper and the rest) are kept (upgrade check).

## Non-goals

- Colour sprays and the tinter (recolouring), a later milestone.
- Boundless's seasonal, event and prestige-only blocks; mosaics (they need a creature drop Boundmore lacks), concrete (needs placeable water), decorative ice (needs glacier ice).
- Sovereign colour choices.

## Measure in game (for Nathan)

1. Does the colour choice in the craft window feel like Boundless's?
2. Do the families read as Boundless's did: which ones you miss, which look wrong?
3. Recipe costs (Boundless's): any that feel off for Boundmore's pace?

## Status

- *Spec written 2026-10-26.* **Part 1 built the same day** (branch `ccr-5b303ad7-6coo06`): `Recipe::tint`, `Recipe::{plan_in, max_crafts_in, colour_choices, input_of}`, `Inventory::{count_coloured, colours_of, remove_coloured}`; the server's `craft_colour`, `take_inputs_keeping_in`, `craft_by_hand_in`, `Job::colour`, the tinted input refunded in its colour; `ClientMsg::Craft::colour` (protocol 35); `SavedStation::job_colours` (`SAVE_VERSION` 15, a version 14 layout read for the upgrade); the craft window's colour swatches (the choice kept per recipe, the counts and the output shown in it). The catalogue: three stones, three timbers, the `any_stones` and `any_timber` groups (every recipe that took stones or timber), 87 new blocks and 6 items with Boundless's recipes; compact, refined and machined metals and compact gems are now placeable blocks (saved items keep their names). Textures and icons painted by `bm-texgen/src/building.rs` (a texture agent; every existing texture unchanged).
- **Tests:** colour choices, a tinted hand craft refused in a colour not carried and made in the one asked, the most plentiful colour by default, a station job keeping its colour through a save and giving its input back in it when taken; a version 14 universe's queued jobs kept, natural; end to end (`crafting_passes_the_chosen_colour_on`); the crafting-table test moved to basalt stones and pale timber.
- **Upgrade check** (2026-10-26): a version 14 universe (made by `main`, opened by M21) opened as version 15 with no warnings.
- **Known limits:** inputs other than the tinted one are taken in whatever colour comes first and come back natural if a station is broken mid-job (as before this milestone).
