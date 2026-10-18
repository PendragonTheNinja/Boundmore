# Milestone 3: Gather, Craft, Claim

*Spec written 2026-10-18. Built 2026-10-18; awaiting Nathan's play-test.*

## Goal

Turn the creative-mode sandbox into the first real Boundless loop: you log in to an account, break blocks into your inventory, craft tools that break faster, build machines that craft over time, and claim land so your build is yours.

## Acceptance criteria

1. **Accounts.** Register and log in with a name and password (Argon2id, never stored or logged in clear; sent only inside QUIC's TLS). Logging in again replaces the old session. Login attempts are rate-limited per address. A name belongs to one account forever.
2. **Inventory.** 40 slots per account; slots 0 to 9 are the hotbar. Stacks up to each item's limit. Server-authoritative: the client asks, the server moves items and tells it the result. Saved with the account.
3. **Breaking gathers.** A broken block goes straight into the inventory (its `drops` item, by default the block itself). With no room, the break is refused with a notice; nothing is ever silently destroyed.
4. **Placing spends.** Placing takes one of the selected hotbar item; the server checks the slot holds it.
5. **Tools.** Hammers (rock, ore, crystal), shovels (soil, sand, gravel, snow) and axes (wood, leaves) in stone and copper. Break time is `hardness / speed`, where speed is the tool's on its own material class and 1 otherwise. Each block broken with the matching tool costs one durability; a tool at zero is gone.
6. **Crafting.** Recipes are data (`assets/recipes.toml`). Hand recipes complete at once. Station recipes (workbench, furnace) queue at the station and finish over time on the server, whether or not anyone is watching; the output waits in the station until collected. Stations with contents cannot be broken.
7. **Claims.** A claim stone claims the 8×8×8 plot it sits in. Its owner adds and removes adjacent plots in claim mode, up to an allowance (64 plots per account for now). Nobody else can edit inside a claim. Breaking the claim stone (owner only) releases the claim.
8. **Menus.** A login screen; an inventory screen with drag and drop; a crafting screen (hand, or the station being looked at); a station screen with its queue, progress and output; the hotbar shows the first ten slots with counts; notices appear briefly on screen.
9. **Saves.** Old planets open: the save layout is upgraded in place (`SAVE_VERSION` 1 to 2), never refused.

## Shared contract (built first, frozen while the parallel work runs)

- `bm-core`: `items.rs` (`ItemId`, `ItemDef`, `ItemKind`, `ToolClass`, `ItemStack`, `ItemRegistry`: every placeable block is also an item of the same name), `inventory.rs` (`Inventory`), `recipes.rs` (`Recipe`, `RecipeBook`, `Station`), `plots.rs` (`PlotPos`, `PLOT_SIZE`), and on `BlockDef`: `material`, `drops`.
- `bm-protocol` v5: the messages in `lib.rs`, documented there.
- Data: `assets/items.toml`, `assets/recipes.toml`, new blocks in `assets/blocks.toml` (workbench, furnace, claim stone).

## Plan and who does what

| Part | Owner |
|---|---|
| Spec, shared contract, data files | main session |
| Server: accounts, inventory, gathering, tools, stations, claims, saves, end-to-end tests | agent `server-m3` (bm-server only) |
| Client: egui integration, login, inventory, crafting, station and claim screens, hotbar counts, notices | agent `client-m3` (bm-client only) |
| Integration, headless play-test, docs | main session |

## Non-goals

Item entities on the ground, trading and shops, guilds, claim permissions for friends, beacon upkeep, power for machines, recipe discovery. (Several wait on the forum research: Nathan asked to weigh what Boundless players liked and disliked before copying systems such as upkeep.)

## Decisions for Nathan

- **Plot size** 8×8×8. Confirmed by Nathan, 2026-10-18.
- **Inventory size** 40 slots, stacks of 100 for blocks. Confirmed by Nathan, 2026-10-18.
- **Stations on someone else's claim** cannot be used (craft, watch or collect), so output can't be taken. Proposed by the server work; awaiting Nathan.
- **Land upkeep:** decided by Nathan, 2026-10-18. No fuel. A claim lapses after 6 months (`claim_keep_days` = 183) without its owner visiting, and its contents go back to the owner, never destroyed; players must be told (a notice when a claim is placed now, the tutorial later). Each player's `last_seen` is saved from M3 on; lapsing itself is a later task. A schematic that saves the build so it can be placed again is wanted if it proves feasible.

## Status

- [x] Shared contract and data files
- [x] Server: accounts (Argon2id, login rate limit), inventory, gathering, tools with wear, hand and station crafting that runs unattended and across restarts, claims, save v1 to v2 upgrade. 15 unit and 8 end-to-end tests.
- [x] Client: egui menus (login, inventory with drag and drop, crafting, station queue and output, claim mode with outlines), hotbar icons and counts, notices, saved settings. Screenshots `docs/media/m3-*.png`.
- [x] Art: workbench, furnace and claim stone faces; item icons (`assets/textures/items/`).
- [x] Integrated: fmt, clippy and all 156 tests clean on the merged tree; headless run against the real server (login refusal, gathering, live crafting counts).
- [ ] Nathan's play-test. Not yet confirmed against the real server headlessly: placing and station crafting by aiming (covered by the end-to-end tests and the offline demo).
