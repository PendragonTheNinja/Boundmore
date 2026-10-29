# Polish plan (2026-10-29)

Nathan, 2026-10-29: "Do a deep dive of both games and figure out areas where ours could be better made. We can start polishing the actual features now since most are in the game."

Four audits, each comparing Boundmore with Boundless (and with what Boundless players complained about) and reading our code for bugs:

- [feel-and-controls.md](feel-and-controls.md): movement, grapples, mining, combat, camera, controls, HUD, the first minutes.
- [building-and-world.md](building-and-world.md): building, blocks, colour, chisels, claims, world generation, light, textures, the atlas.
- [economy-and-progression.md](economy-and-progression.md): crafting, machines, the forge, skills, food, coin, shops, guilds, the crafting and inventory windows.
- [travel-social-tech.md](travel-social-tech.md): shrines, portals, meteors, exoworlds, chat and social, networking, saves, performance, settings, Windows.

Each has its evidence, the bugs found (file and line) and a ranked list. This page orders the work across all four.

## What we already do better than Boundless (keep)

No prediction rubber-banding (deterministic movement); chat on one ordered stream; no beacon fuel, a 6-month lapse and reclaim; chisel fill-back; colour chosen when crafting; doors that swing together; rights per friend and guild; atlas heat maps; no wall impact damage, a safe drop and no fall damage while hooked; floating in water.

## Batch 1: integrity (no decision; do first)

Exploits, save loss and crash risks.

| # | Item | Source |
|---|---|---|
| 1 | "Dug up" mark: a placed natural block gives no XP, bonus drops, feats or endeavour when broken (Boundless's rule) | economy 1 |
| 2 | `Buy` and `Sell` carry the expected price and item; refused if they changed | economy 2 |
| 3 | Shop stands and request baskets open as storage only for their owner (M24's rule, broken by M25's rights) | economy 3 |
| 4 | The Max craft button over 100 | economy 4 |
| 5 | Unused Spark accepted back by a generator | economy 7 |
| 6 | A forge session can be given up; the owner ends a stale one | economy 8 |
| 7 | A guild's treasury is not lost when its last member leaves | economy 19 |
| 8 | A game-thread panic: final save, notice to players, non-zero exit; closing the Windows console saves too | travel 3 |
| 9 | The save writer: bounded retries, failures reported | travel 4 |
| 10 | Per-connection message limits and a cap on events a tick | travel 6 |
| 11 | Frame size guard for large messages; unpaid coin queued, not dropped | travel 17, 18 |

## Batch 2: feel and comfort (no decision)

| # | Item | Source |
|---|---|---|
| 1 | A new character's first click works (hand stone in hand) and the no-tool notice names the key | feel 1 |
| 2 | Settings window: mouse sensitivity, field of view, invert Y, view and far distance, vsync and frame cap, hide HUD; then key binds | feel 7, travel 1, 7 |
| 3 | Automatic reconnect, plain error messages | travel 2 |
| 4 | Combat readability: damage direction, hit marker, a spark where shots land; defeat names its cause | feel 4, 5 |
| 5 | E hints for every usable block; the right hand's held sling and bombs drawn; Twitchy's hand | feel 2, 3 |
| 6 | Place prediction mirrors the server's support and rights rules (no flickering blocks) | building 4 |
| 7 | Craft window search, categories, "can make"; cancel and move-up for queued jobs | economy 5, 6 |
| 8 | Inventory and storage sort, deposit matching | economy 9, building 7 |
| 9 | Plot view toggle and drag-to-claim | building 8 |
| 10 | Break particles, pickup lines, bar labels, HUD tidiness, grapple range cue, thicker rope | feel 12, 16, 19, 20, 23 |
| 11 | Ignore and Do Not Disturb; chat history kept across restarts | travel 8, 12 |
| 12 | Windows: paths relative to the executable; a frame cap when minimised | travel 13, 19 |
| 13 | Tinted sky light (dawn, dusk, night colour) and glowing emitters | building 5 |
| 14 | Item icons at 64 px in a painted style (Nathan's own complaint) | building 3 |
| 15 | Far meshes merged into regions (fewer draw calls), once Nathan measures his frame rate | travel 5 |

## Batch 3: needs Nathan's decision

| Decision | What it unlocks | Source |
|---|---|---|
| Creep (no walking off edges while building): which key, its speed | builders' main movement tool | feel 10, building 1 |
| Sound: direction and sources (made by us, procedural or free libraries) | the largest feel gap | feel 8 |
| First-person held items and swing animation: the look | the second largest feel gap | feel 9 |
| First-minutes guide: a checklist panel or a tutorial island; XP for it | onboarding | feel 6 |
| Slopes in generated terrain (new planets only; terraces softened or kept) | planets that look like Boundless's | building 2 |
| Inventory capacity: Boundless's smart stacks or bigger stacks | builders and gatherers | economy 11, building 15 |
| Alts minting coin: limits on starting coin and footfall | economy health | economy 10 |
| Travel: free home warp; shrine travel priced by distance; see-through portals | the universe's geography, portals' purpose | travel 9 to 11 |
| Block orientation (logs sideways, patterned blocks turned) | building | building 6 |
| Colour: every palette colour on some world; sprays and a tinter | the builders' colour trade | building 9, 10 |
| Wild land and ash regenerating | worlds that stay beautiful | building 12 |
| Machine wear kept on a picked-up machine | spanners stay meaningful | economy 14 |
| Crafting from nearby storage | base layouts | economy 18 |
| Shop directory at shrines | finding sellers (M24 chose physical only) | economy 17 |
| Falling armour as a percentage; sprint in water; a respawn choice | small rule changes away from Boundless | feel 13, 18, 21 |
| Admins (kick, ban, mute); trust-on-first-use certificates | servers beyond friends | travel 14, 16 |

## Status

- [x] Audits (2026-10-29)
- [x] Batch 1 (2026-10-29): all eleven, each with a test; the dug-up mark needed a new save table (`planet_placed`, upgrade check from `main` done); protocol 58
- [ ] Batch 2 (2026-10-29): items 1 to 13 built, each pushed with tests and screenshots (`docs/media/polish-*.jpg`): options window (Esc twice), F1 hides the HUD, reconnect, hit marker and hurt direction, E hints, place prediction, craft search and job controls, sorting, drag to claim, break chips, pickup lines, bar labels, grapple range cue, a solid lit rope, `/ignore`, `/dnd`, chat kept across restarts, Windows paths and a minimised frame cap, tinted sky light. Also fixed: box light was interpolated (long boxes speckled white), worn tools read as pickups, a load-dependent demo test. Left: Twitchy's hand (a known gap), a spark where shots land, item 14 (icons, in progress), item 15 (waits on Nathan's frame rate)
- [ ] Nathan's answers on batch 3
