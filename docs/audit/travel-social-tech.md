# Audit: travel, universe, social systems and the technical foundation

*Written 2026-10-29 on branch `ccr-b3c63c22-hfkvc9` (protocol 57), read-only: no code was built or run. Line numbers are from that commit. Confidence is marked on every finding; "guess" marks anything not checked in code or in play.*

## Summary

1. **Portals are not windows.** Boundless's portals showed the far side and you walked through a picture of it; players named this among the best things in the game (`boundless-forum-feedback.md`, "portals as windows to elsewhere"). Ours are a flat surface block. This is the biggest feel gap in travel.
2. **Travel has no shape.** Shrine travel is free and instant from any shrine to any planet (`game.rs:2672`, M4 left the cost "for now"). There is no home warp, no warp to a friend, no warp conduit, no distance. Portals therefore have little reason to exist between planets, and hubs (Boundless's social centres) cannot form. Needs Nathan's decision.
3. **A player who loses the connection is thrown to the login screen and must type the password again** (`app.rs:1578`). No automatic reconnect, no "server restarting" screen, and raw transport errors are shown (`net.rs`, "connection lost: {e:#}").
4. **Settings are two strings** (`settings.rs`: name and server). No mouse sensitivity (`MOUSE_SENSITIVITY` const, `app.rs:112`), FOV (`FOV_Y`, `app.rs:120`), view or far distance, key binds, vsync or frame cap, UI scale. Every player will ask for sensitivity and FOV on day one.
5. **Far views draw one call per far chunk** (`render.rs:940-1125`): a separate vertex buffer and `draw_indexed` per chunk, per pass, near and far alike. At far radius 32 that is thousands of draws a frame; likely Nathan's frame-rate ceiling (guess until measured on his machine).
6. **No moderation or social safety**: no kick, ban, mute, ignore or Do Not Disturb (the forum's DND request, #2354). Fine for friends; needed before any public server.
7. **The server has no flood limit on reliable messages**: one tick drains every queued event (`game.rs:984`) from an unbounded channel (`lib.rs:209`), so a misbehaving or buggy client can stall the 60 Hz loop. Only chat, edits and inputs are rate limited.
8. **Saves are sound but silent when they fail**: the writer retries forever (`store.rs:2794`), which is right, but nothing reaches the 30-second report or players, and a permanently failing batch blocks every later save and hangs shutdown. On Windows, closing the server's console window (not Ctrl+C) skips the shutdown save (guess, high confidence), and the README advises `taskkill /F`.
9. **Joining friends is hard**: every player must be handed `server.cert.der` and pass `--cert` (`bm-net/src/lib.rs`, README). Trust on first use would make a friend's server one address to type.

## Where Boundless did it better

### Travel and the universe

- **Seeing through portals.** Boundless rendered the destination inside the portal frame and loaded it as you approached; stepping through was seamless. Ours: `portal_surface` is an opaque-looking block, travel is a `Teleport` (same planet) or the full `Travel` journey with a "Travelling to" screen (M8 Status). Effort L (a second view of a remote area: the server must stream a small window of the far side's chunks; the client renders them with a stencil or a render-to-texture). Worth a milestone of its own.
- **Travel with geography.** Boundless had the blinksec world graph, warps whose cost rose with distance (100 coin floor; 3 blinksecs about 260, long trips about 3000; `boundless-mechanics.md` 3.2), a Warp Distance skill, and portal size rising with distance. Warps cost coin, so portals (fuel) were worth building, and portal hubs on capitals became the meeting places of the universe. Ours: any shrine to any planet, free (`game.rs:2672 warp`). With M24 coin now in game, a coin cost by distance is buildable. Needs Nathan.
- **Sanctum and home.** Boundless had a free warp home to your beacon from the Sanctum, and "return to last area". We have nothing like it (no home travel in `game.rs` or the protocol). A free warp to one chosen claim from any shrine is cheap to add and keeps free travel from mattering less.
- **Warp conduits from meteorites**: Boundless meteorites were made of warp conduit blocks, so hunts paid in travel. We give every fourth shell block as a portal conduit (M14 Status), close to this.
- **Warp exits looked for a safe spot** (not air, lava, deep water or another's beacon; `boundless-mechanics.md` 3.2). Our `Planet::arrival` is the shrine plaza, safe by construction; portal arrivals use the far portal's front. Worth a test that a portal whose far side was walled in or flooded does not put a player inside blocks (not checked; guess).

### Social

- **Mail and offline messages**: Boundless had none, so we are ahead (M25 part 2 keeps whispers to friends and guildmates, `player_mail`, up to 50). But we lack the parts players miss most after chat itself: **Do Not Disturb**, **ignore**, chat kept across restarts (`chat::HISTORY` 50 lines, memory only, universe channel only), and a **friends list that shows where friends are** in the travel window (the friends window shows it, `menus` O; travel does not let you go to them).
- **Guilds** match Boundless's first release well (M25). Missing: guild renames, controlled claims' warp settings, eight helixes (numbers not online, Measure M1).

### Technical

- **Boundless streamed planets as you approached through portals**, hiding loads. Our cross-planet journey shows a loading screen; fine, but the time it takes is unmeasured on Nathan's machine (Measure list item to add).
- **Boundless had graphics options** (draw distance, LOD distance, shadows, frame cap, FOV, sensitivity, key binds). We have F11 and `--no-vsync` only.

## Where we can beat Boundless

| Boundless complaint (forum feedback) | Where we stand |
|---|---|
| Rubber-banding into lava, deaths from lag (#1089) | **Fixed by design**: deterministic `movement::step`, same code on both sides, quantised angles, input redundancy of 6 (ADR-0004). Keep the title-bar correction counter and ask Nathan for it after long play. |
| Chat delayed, out of order, lost (#2665) | **Fixed**: one reliable ordered stream (invariant 12). Chat history should survive a restart (small table) to finish the job. |
| No mail, no global chat | **Fixed**: universe channel, kept whispers. Add DND and ignore. |
| Portal fuel treadmill (#3004) | **Fixed**: tanks pay per trip, never burn idle (ADR-0009). |
| Empty universe, ghost hubs | Not fixed by code. Ideas: the planet list shows players per planet already; add **where people are gathered** (settlements with players now, from M24 footfall), and a "busiest settlement" line in the travel window. Exoworld arrival lines already go to chat (`game/exo.rs`). |
| Finding shops took hours (#2003) | M24 decided: shops are found physically. The atlas (M27) could mark settlements with shops (data already exists, `planet_shops`). Needs Nathan (he chose physical finding). |
| Server crashes and long downtimes | Saves every 5 s (`SAVE_EVERY = 300` ticks) in one batch and a crash-safe database (ADR-0005) are better than most. Add a watchdog: if the game thread dies, the process exits with an error code so a wrapper script restarts it (see Bugs 1). |
| Unwanted warps and visits | We have no warp-to-player, so none arise yet. If warping to friends is added, build DND with it. |

Further ideas that suit our design:
- **Exoworld events with a reason to gather**: every wanderer could arrive with a guaranteed meteor wave on its first day (meteors already exist; `game/meteors.rs` triggers are region centres). Low effort, makes "a world arrived" an event. Needs Nathan (economy).
- **Auto-reconnect that puts you back where you were** is easy for us: the server already replaces a lingering session on re-login (`game.rs:1239`) and keeps `departed` state.

## Bugs, crash risks and rough edges found in our code

Ordered by severity. "Confidence" is how sure I am it happens as described.

1. **A panic in the game thread stops the world but not the process cleanly.** `Game::run` (`game.rs:863`) has no `catch_unwind`; on a panic the thread ends without `shutdown()` (no final save, so up to 5 s of edits since the last save are lost) and without telling players. `main.rs` notices `is_running()` is false within 0.5 s and stops, exiting with status 0 (it logs "stopped"), so a restart script cannot tell a crash from a stop. Fix: wrap `step()` in `catch_unwind`, on a panic log it, save what can be saved (the dirty set may be inconsistent; save players and chunks, not half-applied state), close every outbox with "the server hit an error and is restarting", exit non-zero. Confidence high (the mechanism); how often panics happen is unknown. The game code has about 70 `expect`s outside tests; the ones I checked (`meteors.rs:859`, `shops.rs:369`, `guild.rs:30`, `signs.rs:58`, `exo.rs:174`) are all guarded just above. None found reachable from client input.
2. **The store writer retries a failing batch forever and blocks everything behind it** (`store.rs:2794-2814`). Right for a transient error (disk briefly full, file locked by an antivirus scan on Windows), wrong for a deterministic one: M21 already hit this once ("the writer retried forever", fixed). Effects: all later batches queue in an unbounded channel (memory grows by every chunk saved), `shutdown()` gives up after 30 s (`game.rs:3629`) and then `RunningServer::shutdown` joins the writer, which never ends, so the server hangs on Ctrl+C. Players see nothing. Fix: after N attempts, write the batch to a side file (`data/failed-batch-<time>.bin`) and say so loudly in the 30-second report and to players ("saving is failing; the server will stop"); never drop it. Confidence high for the hang, medium for how likely a permanent failure is.
3. **Save failures are invisible in the 30-second report** (`game.rs:893 report`). It shows held-for-saving counts (`+Ns`) but not failed attempts or the writer's queue length. Add both. Confidence high.
4. **Closing the server's console window on Windows skips the save.** `main.rs` waits on `tokio::signal::ctrl_c()`, which does not see Windows' close, logoff or shutdown events (tokio has `tokio::signal::windows::ctrl_close` and `ctrl_shutdown` for those). Clicking the window's X gives about 5 s before Windows kills the process; the save never starts. The README's `taskkill /IM bm-server.exe /F` does the same. Loses up to 5 s of the world; redb itself stays consistent (commits are atomic). Fix: listen for `ctrl_close` and `ctrl_shutdown` too, and change the README to plain `taskkill /IM bm-server.exe` (no `/F`), which sends a close. Confidence high on the mechanism, medium on Windows' exact grace period.
5. **No rate limit on reliable client messages; one tick drains them all.** `step()` loops `while let Ok(ev) = self.events.try_recv()` (`game.rs:984`) over an unbounded crossbeam channel (`lib.rs:209`), and the connection reader forwards every frame (`net.rs` serve loop). Chat, edits and queued actions are limited, but `Craft`, `MoveItems`, `StorageMove`, `OpenAtlas`, `AtlasLoad`, `Friend`, `Command` and others are not. A buggy client (or a key held with repeat) can make one tick take seconds. Fix: a per-connection token bucket in `net.rs` (for example 200 messages a second, burst 400), and cap events handled per tick (carry the rest). Confidence high that the code allows it; low that a normal client triggers it.
6. **The server never checks a frame's size when it encodes.** `encode_frame` (`bm-protocol/src/lib.rs:1334`) writes any size; the client's `read_msg` refuses anything over `MAX_FRAME` (1 MiB) and drops the connection with "frame of N bytes exceeds the limit". `Welcome` has a size test (under 128 KiB); other big messages (atlas tiles, universe list with many exoworlds, chat history, shop and storage views, the friends and guild views, far copies) have none I found. A big guild or shop could one day disconnect everyone who opens it, every time. Fix: `debug_assert` and a logged error on the server for frames over half of `MAX_FRAME`, and a test per large message at its stated maximum. Confidence medium.
7. **Disconnect means log in again from scratch** (`app.rs:1578-1596`): the session is dropped, menus closed, chunks cleared, the login screen shown with the reason, and the password must be typed again (it is never saved, by design). After a server restart or a Wi-Fi blip this is the worst moment of the game. Fix: keep the password in memory for the session and retry with backoff (1, 2, 4, 8 s, up to a minute) behind a "Reconnecting..." overlay, unless the refusal was a wrong password, an update, or "logged in from somewhere else". Confidence high.
8. **Raw error text reaches players.** `net.rs` shows `connection lost: {e:#}` (quinn's "timed out", "aborted by peer: ..."), and the server's closes say "bye" or "server stopped". Map the common cases to plain words: "The server stopped", "The connection was lost (no answer for 15 seconds)", "Could not reach the server at HOST: check the address and that UDP port 7777 is open". Confidence high.
9. **A wrong first message from a client is closed in silence** (`net.rs` `serve`, `_ => return Ok(())`), and a client that connects but never says hello holds a task for 5 s: fine. But the client itself waits for the welcome with no timeout of its own beyond QUIC's 15 s idle (`net.rs` client `session`); a server stuck hashing (2 at a time, `CONCURRENT_HASHES`) under a burst of logins leaves the login button busy with no message. Add "Waiting for the server..." after 3 s. Confidence medium.
10. **There is no connection cap before login.** `max_players` (200) is checked only after authentication (`game.rs:1251`), so a full server still hashes the password (tens of ms, about 19 MiB each) before saying "this server is full". Check fullness before hashing (the network side can read an atomic player count). Minor. Confidence high.
11. **Synchronous database reads on the game thread**: `store.player`, `store.brews`, `store.ledger` on join (`game.rs:1270-1320`), `store.ledger` in `pay_account` and `note_away` (`shops.rs:362, 384`), `store.account` in `rights.rs:133`, `store.discoveries` (`game.rs:1503`), and opening a whole new exoworld in `next_exo_planet` (`game/exo.rs`; about 25 ms each per M21's upgrade check). Each is small, but a burst of 50 joins does 150 reads in one tick, and the unexplained 189 ms tick in M21's load test may be one of these (guess). Move join loading into `net.rs` before `Event::Join` (the login task already runs there) and exoworld creation to the loader pool. Confidence medium.
12. **`pay_account` loses coin if the ledger cannot be read** (`shops.rs:384-388`): it logs "could not be paid" and returns. The comment says the coin is "kept in the log", which means the text log. A shop sale paid to an away owner during a disk hiccup is gone. Queue it (a `pending_payouts` map retried each minute). Confidence high, rare.
13. **Client CPU while minimised or in the background.** `ControlFlow::Poll` with `request_redraw` in `about_to_wait` (`app.rs:4014, 4218`). While minimised on Windows the surface has zero size: `resize` ignores it (`render.rs:809`), and `render` (`render.rs:899`) reconfigures and returns at once when the frame is `Outdated`, or returns on any other failure, so nothing blocks on vsync and the loop spins a core at 100% (medium confidence; confirm in Task Manager). Unfocused, it renders at full rate. Add a background frame cap (for example 15 frames a second; the game still ticks at 60 Hz from the accumulator, which `MAX_TICKS_PER_FRAME = 8` allows up to 133 ms frames).
14. **Relative paths on Windows.** The client defaults to `assets` and `data/server.cert.der` relative to the working directory (`main.rs`). Double-clicking `target\release\boundmore.exe` runs in `target\release`, so textures fall back to colours (with a title-bar warning) and the certificate is missing. Resolve both relative to the executable's folder (and its parents up to the repo root) when not found in the working directory. Confidence high.
15. **README is out of date at the top** ("milestones 4 to 6 are built and awaiting a play-test"). It is the first page a friend of Nathan's reads. Confidence high.
16. **Exoworld slot hidden until restart on a creation error** (`game/exo.rs` around line 170): logged, the slot stays empty with `arrives = u64::MAX`. Acceptable; retry on the next `EXO_EVERY` instead. Confidence high, rare.

## Performance and robustness

### Client

- **Draw calls.** `Renderer::render` (`render.rs:940-1125`) culls by distance and frustum (good), then issues one `set_vertex_buffer` and one draw per chunk per pipeline: opaque, shaped opaque, translucent, shaped translucent. Far meshes are kept per `ChunkPos` (`far_chunks`, `render.rs:828`) and drawn the same way. With far radius 32 the far set is a 65 by 65 column area (M20 measured 38,000 far copies on a whole planet, held by the client), so a frame can submit several thousand draws. On DX12 or Vulkan through wgpu, each costs a few microseconds of CPU; 4,000 to 8,000 draws is 10 to 40 ms (guess). Options, cheapest first: (a) skip empty meshes earlier and count `chunks_drawn` in the title bar so Nathan can report it; (b) merge far meshes into regions of 4 by 4 columns (one buffer, one draw each; far chunks change rarely); (c) one big buffer with `multi_draw_indexed_indirect` where the adapter supports it. Effort M for (b).
- **Buffer churn.** Every remesh calls `create_buffer_init` (`render.rs:854`), allocating a new GPU buffer; edits near the player do this many times a second. A pooled allocator (sub-allocate from large buffers) avoids driver stalls on Windows. Effort M. Priority after draw calls.
- **Translucent sorting** is per chunk, far to near (`render.rs:1111`), not per face: water seen through glass through water will show wrong order in places. Known trade-off; leave.
- **Settings to expose first:** view radius (the server already caps it; let the client ask for less), far radius (client side: drop far meshes past a distance), FOV, mouse sensitivity and invert Y, vsync and a frame cap, UI scale (egui `pixels_per_point`), key binds (stored by `KeyCode` name in `client.toml`). The settings file already round-trips safely (`settings.rs`, write then rename). Sound volume when sound exists.

### Server

- **The 30-second report** (`game.rs:893`) is good: average and worst tick, far time, per planet players and chunks, pending loads, held saves, creatures. Add: writer queue length and failed attempts (Bugs 2 and 3), the slowest phase of the worst tick (time `step()`'s parts as `creature_time` already is), each player's send backlog maximum (players held back by the 768 KiB rule), and the events handled per tick maximum.
- **Backpressure** works as ADR-0004 says: `Outbox::backlog` holds chunks back. The writer side frees `queued` only after `write_all` returns, so the number is honest. One gap: if the writer task ends (`write_all` error), `queued` stops falling and the loop keeps encoding frames for a dead player until `Leave` arrives (at most the 15 s idle timeout). Harmless.
- **The game thread loop** skips ahead after falling 250 ms behind (`game.rs:881`) and warns; good. The rayon pool leaves one core for the loop (`lib.rs:103`); on Nathan's machine with many cores this is fine, on a 2-core host it leaves one loader.
- **Meteors under load are unmeasured** (M14 follow-up: bots never reach region centres). A bot mode that walks to region centres would cover meteors, waves and the creature cost together.
- **Windows specifics:** console close (Bugs 4); antivirus locking `universe.redb` during a write would show as retried save failures (Bugs 2 handles it); working directory (Bugs 14); the client's `%APPDATA%\Boundmore\client.toml` is right. The server's private key is written without owner-only permissions on Windows (`write_private` non-unix branch); low risk on a home PC.
- **Security for a public server (later):** clients pin the self-signed certificate (good), passwords are hashed with a limiter (good). Missing: an admin role (`/kick`, `/ban`, `/mute`, `/stop` with a countdown), and on a `--creative` server every player may `/give` (`game/creative.rs:42`), fine only for testing.

### Robustness the player sees

| Failure | What the player sees now | What it should be |
|---|---|---|
| Server stops (Ctrl+C) | "the server is shutting down", login screen | Same, plus automatic retry: "The server is restarting. Reconnecting..." |
| Server crashes | "connection lost: timed out" after up to 15 s | Immediate notice if the panic is caught; otherwise the reconnect overlay |
| Wi-Fi drops 5 s | Nothing (QUIC survives), then rubber band if over 8 ticks of bank | Fine as is |
| Wi-Fi drops 20 s | Login screen, password again | Reconnect overlay, back in place |
| Server full | "this server is full", after a password hash | Same text, before the hash |
| Wrong version | "this server speaks protocol 57, your game speaks 56; update" | Good. Add "run git pull" for Nathan's friends |
| Missing certificate | Long message naming the path | Good; with TOFU it would not happen |

## Ranked polish list

Ranked by value to a player for the effort. Effort: S (under a day), M (a few days), L (a milestone).

| # | Item | Why it matters | Effort | Needs Nathan? |
|---|---|---|---|---|
| 1 | Mouse sensitivity, FOV, invert Y, view and far distance, vsync and frame cap in a settings window, saved in `client.toml` | Felt in the first minute; no other way to change them | S to M | No (defaults stay as now) |
| 2 | Automatic reconnect with the password kept in memory, plain-words error messages | Every restart or blip now costs a full login | S | No |
| 3 | Catch a game-thread panic: notify players, final save, non-zero exit; Windows console close and shutdown save too; README without `/F` | Save loss and silent crashes are the worst bugs | S | No |
| 4 | Writer: bounded retries then a side file, failures in the 30-second report and to players | A failing disk now hangs the server silently | S | No |
| 5 | Merge far meshes into regions (one draw per 4 by 4 columns), draw counts in the title bar | Likely the frame-rate ceiling with far views on (guess until measured) | M | Measure: frame rate with `--far 0` against default |
| 6 | Per-connection message rate limit and a cap on events handled per tick | One bad client can stall everyone | S | No |
| 7 | Key binds in settings | Expected of any PC game; left-handed and AZERTY players (physical keys help, but not all) | M | No |
| 8 | Ignore and Do Not Disturb (whispers and kept mail refused; friend asks held) | Forum asked for DND repeatedly (#2354) | S | No |
| 9 | Free warp home to one chosen claim from any shrine | Boundless's Sanctum home warp; a reason to have a base | S | Yes: which claim, cost (Boundless: free) |
| 10 | Shrine travel cost by distance in coin, portals as the cheaper way | Gives portals and hubs their purpose; makes the universe have geography | M | **Yes**: the economy of travel (M4 left it open) |
| 11 | Portals you can see through | The best-loved Boundless portal trait | L | Yes: worth a milestone now? |
| 12 | Chat history kept across restarts, per channel (universe, guild) | Finishes "chat that works" | S | No |
| 13 | Executable-relative `assets` and certificate paths on Windows | Double-clicking the exe should just work | S | No |
| 14 | Trust on first use for server certificates (pin on first connect, warn if it changes) | Friends join with one address, no file to copy | S | Yes: acceptable security for him |
| 15 | Join reads and exoworld creation off the game thread | Removes the remaining slow-tick spikes (guess: the 189 ms one) | M | No |
| 16 | Admin commands: kick, ban, mute, timed stop with a warning | Needed before any server beyond friends | S to M | Yes: who is admin (config list) |
| 17 | Frame size guard and tests for the large messages | Prevents a guild or shop that disconnects whoever opens it | S | No |
| 18 | Queue unpaid coin instead of dropping it (`pay_account`) | Rare, but lost coin is lost trust | S | No |
| 19 | Background and minimised frame cap | Laptop fans and battery while alt-tabbed | S | No |
| 20 | Travel window: players per settlement, and travel to a friend's planet from the friends window | Fights the empty-universe feeling | S | No |
| 21 | Exoworld arrivals bring a first-day meteor wave | Makes an arrival an event players gather for | S | Yes: reward economy |
| 22 | Pooled GPU buffers for chunk meshes | Fewer driver stalls while building | M | No |
| 23 | Bots that seek region centres (load test meteors) | Meteors under load are unmeasured | S | No |
| 24 | Update the README's status and Windows instructions | First impression for anyone Nathan invites | S | No |

### For Nathan's measure list (suggested additions)

- Frame rate standing at Verdance's shrine at noon: with defaults, with `--far 16`, with `--far 0` (tells whether item 5 matters).
- Seconds from choosing a planet at a shrine to standing on it.
- Whether the game uses a whole CPU core while minimised (Task Manager).
- In Boundless, the coin cost of a warp from one planet to its neighbour and to the far side of the universe, if he still has a character (for item 10).
