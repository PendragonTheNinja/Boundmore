# CLAUDE.md: Boundmore

Read this file in full before changing anything. If a request conflicts with it, say so instead of quietly complying.

## What this is

**Boundmore** is a spiritual successor to *Boundless* (Wonderstruck, 2018): a persistent online voxel universe of many small planets, built and traded on by its players. The goal is that Boundless players feel at home (same systems, same rhythm) in a game with its own art, names and world, that is finished and keeps being updated, and that runs better than the original.

Rules of the spirit:
- **Mechanics may follow Boundless closely; expression may not.** Never copy its art, textures, sounds, text, names (no "gleam", "Oortians", "Hunting Grounds") or code. Study it as a player does. Game data the community publishes (fan sites such as boundless.mayumi.fi, pfiffel.com/bl, boundlessinfo.com, including the data files those sites host, which may be fetched and decoded) may be used for mechanics and numbers, which should match as closely as possible (Nathan, 2026-10-19); take facts (stats, formulas, rules), never its names, text, art or code wholesale.
- **Feel numbers are calibration targets**, not guesses to defend: `docs/feel.md` lists every one and what to check it against in play. Nathan plays Boundless; his reports beat our assumptions.

Owner: Nathan (`PendragonTheNinja`). Solo project, multi-year, no deadline. Do it right over doing it fast.

## Architecture (ADRs in `docs/decisions/`)

| Crate | Role | Rule |
|---|---|---|
| `bm-core` | blocks, chunks, coordinates, planets, raycast, movement, light, day cycle | headless: no graphics, sockets or files |
| `bm-worldgen` | deterministic planet generation | server only (ADR-0003) |
| `bm-mesh` | greedy meshing with AO | headless |
| `bm-protocol` | wire messages, framing | pure data, no IO |
| `bm-net` | QUIC endpoints, certificates, framed streams | shared by server and client |
| `bm-server` | authoritative game loop, streaming, edits, saves | lib + `bm-server` binary |
| `bm-client` | window, renderer, prediction, input, HUD | `boundmore` binary |
| `tools/bm-texgen` | paints every block texture procedurally | `cargo run --release -p bm-texgen` rewrites `assets/textures/` |

Dependency direction: `bm-core` depends on nothing in the workspace; nothing headless depends on wgpu or winit.

### Invariants (each one is a bug if broken)

1. **The server is the authority** on world, players and every rule (ADR-0004). The client predicts its own movement and edits; the server's answer always wins. Never trust a number from the client: inputs are clamped, edits are checked for reach, rate, support, occupancy and catalogue flags (`check_edit` in `game.rs`).
2. **Movement is deterministic across machines** (`bm-core/src/movement.rs`): fixed 60 Hz step, quantised angles, `libm` for trigonometry (never `f64::sin`), plain `f64` arithmetic. If prediction corrections appear in steady play, a rule has been broken; the title bar counts them.
3. **Client and server answer `PhysicsWorld` identically**: below the floor is solid, unloaded chunks are solid, water is liquid. Change one, change both.
4. **Positions are canonical** (X, Z in `0..width`) on both sides (ADR-0002). Compare positions with `PlanetShape::delta`, interpolate with `nearest_lap`, render each chunk at the lap nearest the camera. Never subtract two raw positions that may straddle the seam.
5. **Chunks are 32³ cubes, palette-compressed**; never assume a bits-per-voxel or a column layout. Use `coords` helpers for chunk/local splits (`>>`/`&`, never `/`, `%`).
6. **Clients never generate terrain** (ADR-0003): ore is economy, and a seed on the client is an x-ray of the planet.
7. **Everything serialized is versioned**: `CHUNK_FORMAT_VERSION`, `PROTOCOL_VERSION`, `SAVE_VERSION`, `GENERATOR_VERSION`. Any change to generated terrain bumps `GENERATOR_VERSION` and re-pins `terrain_fingerprint_is_pinned` in the same commit; the server refuses a planet made by another version.
8. **Blocks are data** (`assets/blocks.toml`), sent to the client in the handshake. Code refers to blocks by name, resolved once at startup; a missing name is a startup error. `solid` is physical, `opaque` is visual; ask the one you mean.
9. **The game loop never awaits and the network never touches state.** Network tasks turn packets into `Event`s; the loop answers through `Outbox` (non-blocking, with a byte backlog the loop reads for backpressure).
10. **Saves are crash-safe and never silently lose a build**: a failed read is retried, never answered with fresh terrain; a failed write is retried; dirty chunks are saved before eviction.
11. **Reversed-Z, infinite far plane** in the renderer: clear depth to 0, test `Greater`. Build projections with `render::projection`, never a library helper with a different convention.
12. **Reliable messages share one ordered stream** so a block change can never overtake its chunk. Movement uses datagrams and is never resent.
13. **Light first, mesh once** (`bm-core/src/light.rs`). Light is computed only for complete columns; unlit or missing reads as dark (dark-then-brighten is safe, lit-then-stale is the bug). Every chunk insert, forget and block change goes through `LightMap`, and the chunks it returns are re-meshed. Sky and block light are separate vertex channels; day and night is a uniform (`sun_color.w`), never a re-mesh.
14. **Textures are data**: the catalogue names them, files live in `assets/textures/` (`docs/art-direction.md`), and a missing file falls back to the block colour with a warning in the title bar. Face table entries pack `first_layer | variants << 16`; decode before using one as a layer.

## Verification (run before every commit)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected test counts (M16 part 1 built, 2026-10-22): bm-core 164, bm-worldgen 39, bm-mesh 34, bm-protocol 6, bm-server 112 unit + 51 end-to-end, bm-client 170, bm-texgen 9 (585 in all). `a_meteor_falls_fights_and_pays_each_player_once` has twice failed ("the fight did not end") under a loaded full run and passed alone (32 s); watch it. CI (`.github/workflows/ci.yml`) runs the same on Linux and builds on Windows.

**Upgrade check (any change to a saved or versioned format):** build the server from `main` in a separate worktree and target dir, let it create a universe (`bots --count 2 --planets 2` to make players), stop it, then open that data dir with the new server and read its log. Unit tests built from the new code missed exactly this once (2026-10-19: chunk format 2 refused every older universe).

**Load test:** run a server, then `cargo run --release -p bm-server --example bots -- --count 50 --prefix Load`. Watch the server's 30-second report for slow ticks.

**Seeing the game from a Linux sandbox** (no GPU): software Vulkan and a virtual display.
```
apt-get install -y xvfb mesa-vulkan-drivers libvulkan1 libxkbcommon-x11-0 xdotool imagemagick
Xvfb :99 -screen 0 1280x720x24 &
DISPLAY=:99 VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json WGPU_BACKEND=vulkan target/release/boundmore --name Tester --password testerpass1 --register --cert data/server.cert.der &
DISPLAY=:99 import -window root shot.png
```
`--name` with `--password` skips the login screen (drop `--register` once the account exists). `--demo-ui login|hud|inventory|crafting|station|claims|shrine|travel|travelling|sky|storage|chat|colours|coloured-icons|reclaim|shapes|chisel|grapple|portal|mining|torches|table|furnace|extractor|flora|textures|meteor|skills|creatures|defeat|bombs|spark|food` opens a menu on an offline test patch, no server needed.
Drive it with `xdotool` (`windowfocus --sync`, then plain `keydown`/`click`, not `--window`; to break, hold `mousedown 1` with a tool selected (swings repeat at the tool's rate), then `mouseup 1`). `BOUNDMORE_TIME=0.77` (0 midnight, 0.5 noon) previews a time of day; `--assets DIR` points at another texture set. Frame rates there mean nothing; correctness and pictures do. Nathan's machine is the authority on performance and feel. Stop processes with `pkill -x boundmore`, never `pkill -f` (it matches your own shell); when parallel agents run servers, give each its own `--bind` port and data dir and stop processes by PID, or one agent kills another's server.

## Starting a session (do this first, every time)

1. `git -C <clone> fetch origin && git -C <clone> status`; work from `origin/main`. If what Nathan describes is not on `main`, ask before building on a guess. Until Nathan merges it, the newest work (M9 mining and light, M10 energy, M11 the crafting chain, M12 skills and progression) is on branch `claude/amazing-pascal-lwttor`: if `origin/main` lacks `docs/milestones/12-skills-and-progression.md`, start from that branch instead.
2. Read, in order: this file, `docs/README.md` (the docs index), the milestone spec marked *in progress* in `docs/milestones/`, and its **Status** section, which lists exactly what is done and what is next.
3. Run the verification commands below and confirm the expected test count before changing anything.
4. Check CI on the last commit (`https://api.github.com/repos/PendragonTheNinja/Boundmore/actions/runs?per_page=1`); a red `main` is the first job.

## Working with Nathan (agreed 2026-10-18)

- **Work in large batches, not one step at a time.** Nathan tests when a major piece lands, not after every change. Keep building, verify everything you can yourself (tests, the headless run below, screenshots), push in task-sized commits, and tell him when something is worth playing. Ask only when a decision is genuinely his (feel, art, what players want).
- **Keep going between milestones** (Nathan, 2026-10-19): when one milestone is built, start the next without waiting, unless the next needs his decision.
- **Stop and ask first** when a decision is genuinely his AND building it the wrong way would cost a lot of work or time to change (data formats players will save into, how a system feels or is paid for, anything that shapes what players invest in). Build around it or pick other work meanwhile; never guess on those.
- **Use every tool available** (Nathan, 2026-10-19): subagents, parallel agents, web research, screenshots, whatever does the job best. Parallel agents are welcome where work splits cleanly (separate crates or modules, each in its own git worktree; research split by topic). Merge, re-verify, push. Keep to about four agents at once: seven in parallel exhausted the usage limit twice on 2026-10-20 and stopped every one of them mid-task. Give agents a self-contained brief (files to read, scope, verification), tell them to save progress early, and check their worktrees after any interruption. **Agents push** (2026-10-21): a cloud container can be replaced at any time (after a usage-limit pause it was), and with it every worktree; two agents' unpushed work was lost that way. Each agent commits only when its checks pass and the workspace builds, then `git pull --rebase` and pushes to the session's branch; give parallel agents separate crates so rebases stay clean.
- **Boundless first, identity second** (Nathan, 2026-10-19): make the game function and feel as much like Boundless as possible first (systems, per-item stats such as each grapple's range, shot and reel speeds; tools; weapons), then give it its own identity. Tune from `docs/research/boundless-tuning.md`, the per-item numbers reference; research before inventing a number, and mark any number that is a guess.
- **He plays Boundless; his reports about how it felt are ground truth.** Record each such decision in `docs/feel.md` or the relevant spec with the date.
- **Art direction:** Boundless's bright, painterly high fantasy, crossed with Hytale's chunky stylised charm, in Boundmore's own textures (`docs/art-direction.md`). Nathan is not an artist: Claude produces the textures (procedurally, `tools/bm-texgen`), and the pipeline must also accept hand-made or AI-generated replacements dropped into `assets/textures/`.
- **Player size:** two blocks tall; nothing fits under less than two blocks (2026-10-18).

## Workflow

- Nathan works on **native Windows (PowerShell)**: give commands that work there. How to run the game is in `README.md`.
- Specs before code: each milestone gets `docs/milestones/NN-name.md` (goal, acceptance, non-goals) before it is built, and a status section kept current.
- Significant decisions get an ADR in `docs/decisions/`; accepted ADRs are not edited, they are superseded.
- A fix ships with a test that failed before it.
- Commit and push at every task boundary. Commit subjects are one line.
- Player-facing text (block names shown in game, messages, UI): no em dashes; plain, concrete words.

## Status

**Newest (handoff, 2026-10-22, later):** Nathan picked **food** next (2026-10-22). **Milestone 16, Food, part 1 is built** on branch `ccr-f92470d6-0kkbof` (which carries everything on `claude/focused-darwin-5u8znd` too): hunger that eats into the energy bar, eating, Boundless's food buffs, sickness, starving, being Full as the way to heal (M13's stand-in is gone), buckets, and the rank 1 and 2 dishes (`docs/milestones/16-food.md`, tuning 3.9). Next: part 2, farming, which needs an ADR on crop growth and Nathan's view on Boundless's farming complaints first (its Status); meanwhile Nathan's Measure list there, M15's open questions, and his Flow textures.

**Earlier (handoff, 2026-10-22):** branch `claude/focused-darwin-5u8znd` carries everything below up to M13, plus the texture trial (Nathan is making AI textures in Google Flow from `tools/texture-ai/flow-prompts.md`; one image per texture, no variants; stone prompt made smoother), **M14 Regions and Meteorites** and **M15 Spark** (both built, awaiting play-test). Next: Nathan's pick of what follows (the roadmap: food and hunger, forging, or gems and power coils), his answer on M15's open questions (its Status section), and installing his Flow textures as they arrive.

**Where we are (handoff, 2026-10-20, later session):** milestones 1 to 8 are on `main` (Nathan merged M7 part 2 and M8; he has played the grapples: "feel really nice for their tier"). **Milestone 9, Mining and Light, is built on branch `claude/amazing-pascal-lwttor`** (2026-10-20; not yet merged to `main`; awaiting Nathan's play-test): Boundless's mining exactly (block health by world tier, armour, swings timed by a13o's action-speed curve, damage, crits, wrong tool a quarter, area-of-effect rule for future forging, bonus ore by tier, flint) and the basic torch (placed on floors and walls, held for light). Read its spec and Status first. Nathan's direction (2026-10-20): "make this game as close to Boundless as humanly possible"; tune from `docs/research/boundless-tuning.md` (check the raw community data before trusting a summary line: a wrong "fibrous leaves only on tier 4+" claim was caught by Nathan and corrected in 4.4a). **Decided 2026-10-20** (recorded in the docs named): a bare hand cannot break blocks, everyone carries the hand stone (`feel.md`); base movement and reach are an untrained Boundless character's (run 3.0, sprint 5.0, reach 4; `feel.md`; applied); skills and progression exactly as Boundless's live game (`docs/proposals/skills-and-progression.md`, Decided section); after M9 came **M10 energy** (built, `docs/milestones/10-energy.md`), then **M11 the crafting chain** (built; it covers crafting table, stones, sticks, timber 2 per trunk, the two-part stone furnace with fuel and heat, an old-tech seam, sap, glue as **2 sap** until creatures bring bone, the extractor and sackcloth, Boundless's tool, chisel and grapple recipes, and **gatherable plants, boulders and fungi** as models scattered by worldgen: Nathan, "the plants are really important"; tuning 4.4a lists what each drops; no tier 4 world is needed). Then **M12 skills and progression** (built). **M13 creatures and combat is built** (2026-10-22, on branch `claude/boundmore-creatures-combat-bx3sq2`, which also carries M9 to M12; `docs/milestones/13-creatures-and-combat.md`, see its Decided section; part 1 health, slings, grazer, pelter, dasher, drops and real glue; part 2 popper, bombs, drifter, exotics). Then the powered machines (spark, coils, refinery, mixer, compactor, forge). Open follow-ups: M8's listed in its Status; the rope is drawn with 1-pixel strands; swimming improvements and far views are not specced; flint now comes mainly from boulders (M11). a13o's sheet is in tuning 2.5a; more sheets can come as Nathan's screenshots (do not commit them); forge sheets are in `docs/research/sources/`.

- **Milestone 1, Foundation: done** (2026-10-18). `docs/milestones/01-foundation.md`.
- **Milestone 2, A World Worth Looking At: built** (2026-10-18), play-tested by Nathan (breaking, water fixed). `docs/milestones/02-a-world-worth-looking-at.md`.
- **Milestone 3, Gather, Craft, Claim: built** (2026-10-18); Nathan's first test found the block-after-block break refusal (fixed, protocol 6). `docs/milestones/03-gather-craft-claim.md`.
- **Milestone 4, Many Worlds: built, awaiting Nathan's play-test.** `docs/milestones/04-many-worlds.md`. One process hosts the universe (ADR-0006), one save file `data/universe.redb`.
- **Milestone 5, Keep and Talk: built, awaiting Nathan's play-test.** `docs/milestones/05-keep-and-talk.md`: storage chests and chat. Inserted ahead of Movement and Shape; the roadmap was renumbered.
- **Milestone 6, Colour and Ash: built, awaiting Nathan's play-test.** `docs/milestones/06-colour-and-ash.md`; block states with colour and shape (ADR-0007), 255-colour palette, planet colours (generator 3), claims lapse to coloured ash, reclaim at warp shrines.
- **Milestone 7, Movement and Shape: built (chisels, then grapples), awaiting Nathan's play-test.** `docs/milestones/07-movement-and-shape.md`; shapes are a family plus eight octants (ADR-0008); the grapple hook lives in `PlayerState` and runs inside `movement::step` (protocol 11); saves keep the pre-grapple body layout (`store::SavedBody`).
- **Milestone 8, Portals: built (2026-10-20), on `main`, awaiting Nathan's play-test.** `docs/milestones/08-portals.md`, ADR-0009: conduit rectangles linked by tokens, each side's tank pays per trip (wayshards, a new deep ore), never burns idle; protocol 12.
- **Milestone 9, Mining and Light: built (2026-10-20), awaiting merge and Nathan's play-test,** on branch `claude/amazing-pascal-lwttor`. `docs/milestones/09-mining-and-light.md`; protocol 13 (swings replace hold-to-break).
- **Milestone 10, Energy: built (2026-10-20), awaiting merge and Nathan's play-test,** on the same branch. `docs/milestones/10-energy.md`; protocol 14.
- **Milestone 11, the crafting chain: built (2026-10-20), awaiting merge and Nathan's play-test,** on the same branch. `docs/milestones/11-crafting-chain.md`; protocol 15, `SAVE_VERSION` 4 (version 3 universes upgrade on opening).
- **Milestone 12, Skills and Progression: built (2026-10-21), awaiting merge and Nathan's play-test,** on the same branch. `docs/milestones/12-skills-and-progression.md`; protocol 17, `SAVE_VERSION` 5. Skill numbers checked against the game's own skill text (boundlexx API, tuning 5.4a). Decided (Nathan, 2026-10-21): marks only by levelling (no real money), marks buy plots at 30 each, the portal epic is required (`ServerConfig::portal_epic_required`).
- **Milestone 13, Creatures and Combat: built (parts 1 and 2, 2026-10-22), awaiting Nathan's play-test.** Part 1: health, falls, defeat, slings, grazer, pelter, dasher, drops, real glue. Part 2: popper, drifter, bombs, exotics. Protocol 19, `SAVE_VERSION` 6; on branch `claude/boundmore-creatures-combat-bx3sq2`. Decided (Nathan): Boundless's defeat XP debt; out-of-combat regen as a stand-in until food; fall damage slightly gentler, never while hooked and not on the first landing within 3 s of letting go of a grapple; dashers drop wayshards, but meteorite hunts should be the main portal fuel source (a milestone soon after). `docs/milestones/13-creatures-and-combat.md`; research in tuning 3 and 3.8.
- **Milestone 14, Regions and Meteorites: built (2026-10-22), awaiting Nathan's play-test,** on branch `claude/focused-darwin-5u8znd` (which carries M9 to M13 and the texture trial). `docs/milestones/14-meteorites.md`: named regions with discovery XP on every planet; meteors on tier 3+ (lush) brought by fresh region centres, waves through portals, meteor lives, each player's own share of the heart (wayshards first); protocol 20, `SAVE_VERSION` 7 (version 6 opens unchanged). Its guesses are in `feel.md`; its Measure-in-game list is for Nathan.
- **Milestone 15, Spark: built (2026-10-22), awaiting Nathan's play-test,** on branch `claude/focused-darwin-5u8znd`. `docs/milestones/15-spark.md`: spark generators and links, mixer, refinery, compactor, machine wear and spanners, iron furnace, silver alloy and the metal chain; protocol 21, `SAVE_VERSION` 8 (version 7 opens with stations fully repaired). Power coils wait for gems. Open questions in its Status.
- **Milestone 16, Food: part 1 built (2026-10-22), awaiting Nathan's play-test,** on branch `ccr-f92470d6-0kkbof`. `docs/milestones/16-food.md`: hunger in `PlayerState` (`bm-core/src/food.rs`), eating at an agreed input (`ClientMsg::Eat`), nine buffs, dishes, buckets that come back from recipes one fill more worn; protocol 22, `SAVE_VERSION` 9 (version 8 opens unhungry). Its guesses are in `feel.md`; its Measure list is for Nathan. Part 2 (farming) is outlined.
- **Three rock kinds later** (Nathan, 2026-10-22): one stone texture now; it becomes one of Boundless's three rock kinds when planet generation grows (`docs/roadmap.md`).
- **Proposals:** `docs/proposals/skills-and-progression.md` (decided: exactly Boundless). `docs/proposals/textures-128.md` (**trial run 2026-10-22** on branch `claude/focused-darwin-5u8znd`, built on the M13 branch; read its Trial results and Decided sections. The client takes 16 to 256 pixel textures mixed with 32s; `--demo-ui textures` shows the trial blocks. The API route was too costly (and is blocked by Google billing); **decided: Nathan generates in Google Flow** from `tools/texture-ai/flow-prompts.md` (regenerate it with `make_flow_prompts.py` after editing `prompts.json`), soft painterly look, saves into `art/texture-trial/ai-raw/` and pushes; Claude runs `cleanup.py`, screenshots in game, and says what needs another try).
- **Design research:** `docs/research/boundless-forum-feedback.md` (what Boundless players liked and disliked). Read it before designing any Boundless system. Decided from it so far: no beacon fuel; a claim lapses only after 6 months without a visit, and its contents go back to the owner (2026-10-18); on a lapse the build turns to ash blocks that keep each block's colour, as in Boundless (2026-10-19). Player-built portals will need fuel, but less punishing than Boundless's (2026-10-19).
- **Mechanics research:** `docs/research/boundless-mechanics.md` (how Boundless's systems worked, from the forum and wiki). Sources reachable from the sandbox: the forum's Discourse JSON API (`https://forum.playboundless.com/search.json?q=...`, `/t/<id>.json`) and the fandom wiki's MediaWiki API only (`https://boundless.fandom.com/api.php?action=parse&page=<Title>&prop=wikitext&format=json`; page URLs return 403). The wiki is frozen around spring 2020: prefer later dev posts and patch notes. Read the relevant section before designing a system, and do the research yourself rather than asking Nathan for facts the forum holds. **The boundlexx API** (`https://api.boundlexx.app/api/v1/`, `?format=json`) serves the final game's own data; `/skills/?limit=200` gives every skill's in-game text with exact numbers (tuning 5.4a), and its `/recipes/`, `/metals/`, `/worlds/` may hold more (`/items/` and `/blocks/` returned errors on 2026-10-21). Check it before guessing a number. **Nathan can look things up in the live game** (offered 2026-10-21): when a fact or number is not on any website or API, list exactly what to measure or check (the item, the screen, what to read off) and ask him, rather than guessing silently; until he answers, mark the number as a guess.
- Update this section and the milestone's Status section at the end of every working session.
