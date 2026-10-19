# Milestone 5: Keep and Talk

*Spec written 2026-10-19. Built 2026-10-19; awaiting Nathan's play-test.*

## Why now

Two gaps show as soon as several people play: 40 slots fill in an evening of digging, and there is no way to talk. The forum research (`docs/research/boundless-forum-feedback.md`, dislike 6) ranks broken chat among the reasons Boundless players left: delayed, out-of-order or lost messages, no persistent chat box, everyone moved to Discord. Both are small, decided by that research, and need nothing from Nathan, so they come before the larger Movement and Shape milestone (renumbered M6; later milestones move down one).

## Acceptance criteria

1. **Storage chests.** `storage_chest` (workbench, 8 timber) holds 20 stacks. Blocks are data: a block's `storage` field gives its slots, so later storage blocks are rows in `blocks.toml`.
2. **Using a chest.** E on a chest within reach opens it beside the inventory. Drag between the two, or Shift-click to send a stack across; the server moves items (`StorageMove`, same merge, split and swap rules as `MoveItems`) and every viewer sees the change at once. Walking out of reach closes it.
3. **Rules.** A chest with anything in it cannot be broken ("Empty the Storage Chest before breaking it"). On someone else's claim a chest cannot be opened, like a station. Contents are saved with the planet in the same write batch as everything else, so nothing is lost or duplicated by a crash.
4. **Chat.** Enter or T opens a chat line; the box shows the last lines and fades when idle, and the history scrolls while open. Channels: local (players within 96 blocks), planet, universe (everyone), whisper (`/w name text`). `/l`, `/p`, `/u` switch the channel; the box shows which. Lines arrive in order (the one reliable stream), are cut at 240 characters, have control characters removed, and are rate-limited per player (a short burst, then about one a second) with a notice when limited.
5. **History.** A player joining receives the last 50 universe lines. The server keeps them in memory (not saved across restarts in this milestone).
6. **Join and leave lines.** "Wren arrived on Verdance" and "Wren left" appear in the box for players on the same planet.

## Shared contract (built first)

- `bm-core`: `BlockDef::storage`.
- `bm-protocol` v8: `ClientMsg::OpenStorage`, `StorageMove`, `Chat`; `SlotRef`; `Channel`; `ServerMsg::Storage(StorageView)`, `ServerMsg::Chat { lines }`; `MAX_CHAT_LEN`, `LOCAL_CHAT_RANGE`.
- Data: `storage_chest` at the end of `blocks.toml`, its recipe.

## Plan and who does what

| Part | Owner |
|---|---|
| Spec, contract, data | main session |
| Server: chests, their saves, chat routing, rate limit, history | agent (bm-server) |
| Client: chest window, Shift-click, E on chests, chat box and commands | agent (bm-client) |
| Texture: storage chest | agent (bm-texgen) |
| Integration, headless play-test, docs | main session |

## Non-goals

Mail, Do Not Disturb, chat saved across restarts, guild chat, profanity filtering, sorting buttons, bigger storage tiers, shop stands (M7 economy), claim lapse returns (a later task once storage exists).

## Status

- [x] Contract, spec
- [x] Server: storage blocks (`planet_storage` table, same write batch, `SAVE_VERSION` unchanged), shared access rules with stations, chat channels, cleaning, rate limit, 50-line universe history, arrival and leave lines. 30 unit and 15 end-to-end tests.
- [x] Client: chest window beside the inventory (drag, Shift-click, half-drag), chat box with fading lines, history, `/l /p /u /w /r`, channel colours. Screenshots `docs/media/m5-*.png`.
- [x] Art: storage chest faces.
- [x] Integrated: 215 tests, fmt and clippy clean; a real client spoke on the universe channel and saw it back.
- [ ] Nathan's play-test. Not yet seen on a real server: a chest placed and used by hand, a two-player whisper (both covered by end-to-end tests).
