# Milestone 9: Mining and Light

*Spec written 2026-10-20 (Nathan, 2026-10-20: "the main goal right now is to make this game as close to Boundless as humanly possible").*

## Goal

Breaking blocks works the way it did in Boundless: a tool swings at a rate set by its action speed, each swing deals damage, the world's tier sets each block's health and armour, the right tool matters, and harder worlds give more ore per seam. Players can make light to dig by: the basic torch, placed on floors and walls or held in the hand.

Every number below comes from `docs/research/boundless-tuning.md` (section numbers in brackets) unless marked **guess**. Guesses are listed in `docs/feel.md` for Nathan to check.

## How Boundless did it (the rules this milestone copies)

- **Block health** = category base x tier multiplier [2.2]. Categories: soft blocks (soil, sand, foliage) 800; hard, soft and natural blocks (rock, ice, the glowing crystal, gravel, ash, trunks, timber) 1200; seams (every ore) 2400. Tier multipliers 1, 1, 1.5, 2, 3, 4, 5, 6 for tiers 1 to 8; armour 0, 0, 100, 200, 400, 600, 800, 1000. Tier 2 is x1.0 (Nathan, 2026-10-20).
- **Damage per swing** [2.3, 2.5a]: `max(damage - armour, 0)`, then x0.25 with the wrong tool [2.4], then x the area multiplier when a swing hits several blocks [2.5a]. A critical hit (rolled per swing, chance from the tool) multiplies the damage by `1 + crit effect` before armour [2.3]. No character skills yet, so skill multiplier 1.
- **Seconds per swing** from action speed (a13o's curve, [2.5a]): `0.05 + 0.95 * (1 - (s - 5) / 145)^3`, `s` clamped to 5..150. Rounded to whole 60 Hz ticks.
- **Tool stats** [2.1], per material, shared by hammer, shovel and axe:

  | Material | Damage | Action speed | Crit chance | Crit effect | Durability |
  |---|---|---|---|---|---|
  | Wood | 300 | 20 | 0 | 0.2 | 600 |
  | Stone | 400 | 15 | 0 | 0.2 | 600 |
  | Copper | 400 | 30 | 0 | 0.2 | 700 |
  | Iron | 700 | 10 | 0.2 | 0.2 | 900 |

- **Right tool** [2.4]: hammer for rock, ice, the glowing crystal and every seam; shovel for soil, sand, gravel, snow, ash; axe for trunks, timber, foliage. A wrong tool still works at a quarter of the damage.
- **Durability** [2.5]: one point per swing, whatever it hits (so an area swing is cheaper per block).
- **Partial damage heals**: a block left alone heals back to full. The delay is not documented anywhere [2, open question 1]: **guess 5 seconds** without a hit, then full health at once.
- **The starter tool**: Boundless gave every player a tool crafted from nothing (200 damage, action speed 20, durability 400, full damage on every block) [2.1, wiki Totem]. Boundmore's is the **hand stone**.
- **Area of effect** [2.5a, 2.6]: the shapes 1x2 (the hit block and the one below, 60%), 3x1 (a row across the view, 40%), cross (5, 40%), 3x3 (9, 40%), and random (the hit block plus 1 to 4 random neighbours, 100%). In Boundless these come only from forging, which Boundmore does not have yet; this milestone builds the rule and a data field so forging can switch it on later.
- **Yields by tier** [4.4, 2.8]: breaking a seam always gives one ore; harder worlds add bonus ore. Copper, iron and coal: +0.5 on tiers 1 and 2, +0.7 on tiers 3 and 4, +1.05 on tiers 5 to 8. Side drops: rock gives **flint** at 1.5% (tiers 1, 2) and 2% (tiers 3, 4), gravel 5% on every tier.
- **Torches** (wiki Basic Torch, Torch; forum t/9768, t/20672, t/38365): the basic torch is 1 stick + 1 flint, crafted by hand, making 4. Sticks are 1 trunk, by hand, making 4. A torch stands on floors or hangs on walls, breaks in two hits with anything, and lights the area around you when held.

## Acceptance

1. **Health and armour.** Each block in `blocks.toml` names a health category (`health = "soft" | "hard" | "seam" | "prop"`, replacing `hardness`). A block's health and armour come from its category and its planet's tier. Looking at a block shows its health bar, with one small shield per 100 armour (as Boundless's HUD did).
2. **Swings.** Holding break swings the held tool (or the bare hand) once at once and then every `seconds per swing`; each swing damages the block under the cursor. Swings have their own cooldown on the player, so switching blocks does not reset it. A block reaching zero health breaks. A damaged block left alone for 5 s heals to full.
3. **Tools.** The stone, copper and iron hammer, shovel and axe have the stats above; wood tools exist (2 timber, 1 stick, 1 foliage, by hand, as in Boundless). A hand stone is crafted from nothing, by hand. The right tool deals full damage, the wrong one a quarter. Every swing costs one durability.
4. **Hits match Boundless.** On a tier 1 world: copper breaks stone in 3 swings (0.59 s each), iron in 2 (0.91 s each), a stone hammer in 3 (0.82 s each); copper takes 6 swings on a copper vein. On a tier 3 world (armour 100), copper takes 6 on stone. These are unit tests.
5. **Critical hits** roll per swing from the tool's chance, the same on client and server (a hash of the player, the swing's input and the block, so prediction never disagrees).
6. **Area of effect** is a tool field (`aoe = "1x2" | "3x1" | "cross" | "3x3" | "random"`), applied by the server and predicted by the client; no craftable tool carries it yet. Every block in the shape takes the same damage; protected blocks (another player's claim, a shrine's ground, a stocked portal) are skipped, never refused.
7. **Yields.** Seams give bonus ore by tier, rock and gravel give flint, as above. Bonus drops are rolled by the server only.
8. **Cheating is refused.** The server checks every swing: reach, the item in the slot, and the cadence on the input timeline (a swing sooner than the tool allows after the last is refused and its block's true health sent back).
9. **Torches.** `stick` (1 trunk makes 4, by hand), `flint` (drop), `torch` (1 stick + 1 flint makes 4, by hand). A torch places on top of a block or against its side, needs that face to be solid, is not solid itself, draws as a small stick with a flame, gives light 10 (**guess**, calibrate against Boundless), and breaks in two swings with anything. Breaking the block it hangs on drops the torch.
10. **Held light.** Holding a torch in the selected hotbar slot lights the area around you as if a torch stood at your hand; other players see your torch's light too.
11. **Versions.** `PROTOCOL_VERSION` 13. Saves keep their format (no `SAVE_VERSION` change): new blocks and items are appended, tools keep their names and wear. The upgrade check from `main` passes.

## Shared contract (built first, by the main session)

- **bm-core `mining.rs`**: block health and armour by category and tier; `seconds_per_hit`, `swing_ticks`; `swing_damage` (armour, wrong tool, area multiplier, crit); `is_crit` hash; `aoe_blocks` (the shape's blocks from the hit block and the look direction); `heal` rule constants. Pure functions, tested.
- **Catalogue**: `BlockDef::health` (category) replaces `hardness`; `ToolStats { class, damage, action_speed, crit_chance, crit_effect, durability, aoe }`; the bare hand's stats (`ItemRegistry::hand`); `BlockDef::bonus` (extra drops by tier).
- **Torch placement**: `EditAction::Place { slot, mount }` (the clicked face for a prop), `ItemRegistry::prop_block` (the variant block for an item and a mount: `torch`, `torch_north`, ...).
- **bm-protocol 13**: `ClientMsg::Swing { seq, at, pos, slot }` replaces `StartBreak` and `EditAction::Break`; `ServerMsg::BlockHealth { pos, damage }` (damage taken so far, sent to everyone watching that chunk, 0 when healed or reset); broken blocks answer with `EditResult` (the swinger, one per block) and `BlockChanged` (others), as edits do; `PlanetInfo::tier`; `RemotePlayer::light` (held light level).

## Plan and who does what

| Part | Owner |
|---|---|
| Spec, contract, data | main session |
| Server: swings, cadence, damage map and healing, AoE, drops by tier, torch rules | agent (bm-server, worktree) |
| Client: swing loop and prediction, health bar and shields, crack stages from damage | agent (bm-client, worktree) |
| Torches: prop mesh (floor and wall), held light in the renderer, textures and icons | agent (bm-mesh, bm-client renderer, bm-texgen, worktree) |
| Integration, headless play-test, docs | main session |

## Non-goals (each waits for its own milestone)

- **Energy and hunger** (each swing costs energy in Boundless [2.1]); **skills** (power, mastery, epics, dexterity, luck [2.7]); **forging** (the only source of area of effect and of the glow boon); **mining XP**.
- **Boundless's full crafting chain**: copper and iron tools keep their present recipes until the materials milestone brings sackcloth, glue, tallow and the crafting table.
- **Better lights**: the torch with tallow and sackcloth, gem torches (they tint their light: needs coloured light), lanterns (refinery and glass), the light-source skill.
- Bombs, totem augments, tool rank.

## Also decided while building (Nathan, 2026-10-20)

- Base movement and reach match an untrained Boundless character: run 3.0, sprint 5.0 blocks a second, reach 4 (were 4.6, 7.6 and 7). Skills will raise them (`docs/proposals/skills-and-progression.md`).

## Questions for Nathan (built with the default meanwhile)

1. **Bare hands.** Answered (Nathan, 2026-10-20): a bare hand **cannot** break blocks in Boundless. A swing needs a tool; every player starts with a hand stone and can craft another from nothing.
2. **Healing.** How long did a damaged block stay damaged before healing, and did it heal all at once? Default: 5 s, all at once (**guess**).
3. **Torch brightness.** How far did a basic torch light, next to a glowing crystal block? Default: light 10 against the crystal's 12 (**guess**).

## Status

- [x] Spec (this file), contract, data (2026-10-20)
- [ ] Server
- [ ] Client
- [ ] Torches
- [ ] Integrated: tests, fmt, clippy, upgrade check, screenshots
- [ ] Nathan's play-test
