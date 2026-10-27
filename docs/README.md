# Documentation index

Start with `../CLAUDE.md` (rules and invariants), then the milestone marked in progress.

| Document | What it is | When to read |
|---|---|---|
| `roadmap.md` | Every Boundless system, where Boundmore stands on it, milestone order | Planning anything new |
| `milestones/NN-*.md` | One spec per milestone: goal, acceptance, non-goals, live **Status** | Before working on that milestone |
| `decisions/NNNN-*.md` | ADRs: why the architecture is the way it is. Immutable once accepted; superseded by new ones | Before changing anything they cover |
| `measure-in-boundless.md` | Every number only the live game can answer, grouped into trips for Nathan, with IDs he answers by | Making a guess no site has (add it there); recording his answers |
| `feel.md` | Every gameplay number and what to calibrate it against, plus Nathan's verdicts | Touching movement, reach, world size, timings |
| `art-direction.md` | The look: palette, texture rules, what Boundless and Hytale contribute | Making or changing any texture, colour or model |
| `research/boundless-forum-feedback.md` | What Boundless players liked, disliked and asked for (3,153 forum posts, weighted to 2020+), with candidate design principles | Before designing any Boundless system (upkeep, crafting, PvE, economy, social) |
| `research/boundless-tuning.md` | Boundless's numbers per item and tier (grapples, tools, weapons, creatures, materials, recipes, skills, forging), from community game data, with sources and confidence | Before setting any gameplay number (Boundless first, identity second) |
| `research/boundless-mechanics.md` | How Boundless's systems worked (chisels, grapples, portals and warps, colours, reclaim, machines, forging...), from the forum and wiki, with sources and confidence | Before designing any system that copies a Boundless mechanic |
| `research/exoworlds.md` | Boundless's exoworlds from the player archive of 419 of them (rhythm, stays, tiers, hosts) and the release notes | Touching wandering worlds or tier 7 |
| `proposals/*.md` | Designs waiting on Nathan's decision, with recommendations (skills and progression) | Before building the system they cover |
| `media/` | Screenshots per milestone | Releases, comparisons |

## Crate map

| Crate | Read first |
|---|---|
| `bm-core` | `movement.rs` (determinism rules), `planet.rs` (seam rules), `chunk.rs` (format), `light.rs` (light rules and the LightMap call order), `time.rs` (day cycle) |
| `bm-worldgen` | `lib.rs` header (purity rules), `GENERATOR_VERSION` |
| `bm-mesh` | `Vertex` packing, `mesh_chunk` |
| `bm-protocol` | module header (channels), `PROTOCOL_VERSION` |
| `bm-server` | `game.rs` (`step`, `check_edit`), `net.rs` (Event/Outbox boundary), `exo.rs` and `game/exo.rs` (wandering worlds, ADR-0011) |
| `bm-client` | `app.rs` (`update`, `render`), `predict.rs`, `render.rs`, `world.rs` (light and mesh scheduling), `textures.rs`, `sky.rs`, `ui.rs` |
| `tools/bm-texgen` | `materials.rs` (one painter per material) |
