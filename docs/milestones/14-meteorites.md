# Milestone 14: Regions and Meteorites

*Spec written 2026-10-22. Facts from `docs/research/boundless-tuning.md` 3.2.5 (and the forum threads cited there and below); numbers marked **guess** are ours, to tune in play. Nathan's direction (2026-10-21): meteorite hunts come soon after M13; meteors fall as you enter a new region, drop waves of creatures, every player loots the core afterwards, bigger with world tier and player count; in Boundless they were the main source of portal fuel and "a lot of fun", so here they drop wayshards.*

## Goal

Planets get **named regions** that players discover (with Boundless's discovery XP), and tier 3 and higher planets get **meteorites**: entering a region nobody has visited for a while brings one down nearby. Creature portals open over the impact and drop waves of creatures that always attack. Clear every wave and the meteorite opens; each player then loots their own share of its core: wayshards first, then ores, relics and creature parts. Lose every life and it goes dormant, paying a little.

## How Boundless did it

### Named regions

- Each world was divided into procedurally named, themed regions (dev update, Sept 2016, [F:4931]); about 50 on a world (players' count of the discovery XP: a full 50-region world gives about 63,750 XP, tuning 5.5).
- The HUD names where you are: just the name near a region's centre, "North of ..." (or any direction) farther out [F:61674].
- **Discovery XP:** the n-th region you discover on a world gives 50 x n XP (R218/219, tuning 5.5).

### Meteorites

- Fall on **tier 3 and higher** worlds, **never within 64 m of a beacon** [F:10745][WIKI Meteorite].
- **Two triggers** [F:26037][F:61674][F:27475]:
  - **region centre** (the main one, since 2020): reaching the centre of a named region that is not on cooldown brings a meteor at once; the region then rests for **2 to 3 hours**;
  - **an older, slower one**: entering a 300 x 300 block square not visited for a while (players guessed about 4 hours; "might not be 100%").
- **Size** from the players present when it starts, 1 to 100, counted within about 80 m [F:61674]; bigger means more and stronger creatures and better loot; solo ones were made easier [F:10745][F:14809]. Players speak of level 1 to 6 meteorites [F:18655][F:27475].
- **On impact** a few portals open in the air and drop **waves** of creatures that always hunt players, even the docile kinds; they can be exotic; waves vary a lot (2000 variants) [F:10745][F:14494].
- **Meteorite life** (R196): once the first creature dies, every player who joins gets one life; going back to the sanctum loses it; a revived player keeps it; leaving the zone gives 10 s to return; with no life holders left the meteor fails and pays only dormant loot; creatures periodically target life holders [F:14494].
- **Cleared:** the meteorite (made of warp conduit blocks, which can be mined) can be broken open to reach the **core**; its loot is **instanced**: each player gets their own [WIKI Meteorite].
- **Dormant:** a meteor left unfinished goes dormant after a while; its loot is much reduced [WIKI Meteorite].
- **Loot:** rough oortstone (the portal fuel economy), gems, fossils, ancient tech, metal ores [WIKI Meteorite]. Lower tier worlds yield more fuel per hour because meteors clear faster [F:54181].

## Names (Boundmore's own; Nathan may rename)

| Boundless | Boundmore |
|---|---|
| Meteorite | **meteorite** (a plain word, kept) |
| Warp conduit (its shell) | **star iron shell** (`meteor_shell`) |
| Meteor core / crate | **meteor heart** (`meteor_heart`) |
| Meteorite life | **meteor life** |
| Rough oortstone | wayshards (Nathan, 2026-10-21) |

Region names come from our own word lists, themed by the land they lie in (for example "Amber Hollows", "the Cinder Reach"); never Boundless's names.

## Rules

### Regions

1. **Layout.** A planet's regions are the cells of a jittered grid, spacing **288 blocks** (about 50 on a 2048-block planet; **guess** from Boundless's count), wrapping with the planet. The layout comes from the planet's id and size, not its terrain seed, so the client may know it (ADR-0003 is about terrain).
2. **Names** are made on the server from the planet's kind and the land at the region's centre (sea, shore, plain, hills, mountain, snow), from word lists in `assets/regions.toml`, deterministic, and never repeated on one planet.
3. **The HUD** shows the region's name within **40 blocks** of its centre (**guess**), else "North of ...", "Southeast of ...", and so on (eight directions).
4. **Discovery:** entering a region's cell for the first time gives **50 x n XP**, where n counts the regions you have discovered on that planet, with a short notice ("Discovered: Amber Hollows"). Saved per character.

### Meteorites

5. **Where:** planets of tier 3 and higher (today only lush; temperate is tier 1, arid and frozen 2). Never within 64 m of a claim or a warp shrine, never on water (**our rule**: Boundless's could land in water, which players disliked).
6. **Triggers** (server, each tick a player moves into a new cell):
   - **region centre**: a player within 40 blocks of a centre whose region is off cooldown starts a meteor; the region then rests **2 to 3 hours** (random);
   - **wandering**: a player entering a 300 x 300 square not entered for **4 hours** starts one with chance **0.25** (**guess** for both).
   Planet cap: at most **3** active meteors per planet (**guess**), so a running group cannot stack a dozen.
7. **Size.** Count players within 80 m of the trigger: `level = 1 + floor(log2(players))`, capped at 6 (1 player is level 1, 2 is 2, 4 is 3, 8 is 4, 16 is 5, 32 and more is 6) (**guess**, shaped to Boundless's 1 to 100 players and levels 1 to 6).
8. **The fall.** The meteor appears high in the sky and falls for **6 seconds** (**guess**) to an impact point **60 to 140 blocks** from the trigger (**guess**), on dry open ground outside claims; every player on the planet sees the streak, and a light beam marks the site for 30 minutes.
9. **Impact.** A crater of radius 4 is cut (no drops), and a rough ball of **meteor shell** (radius 2) sits in it with the **heart** at its centre. While the meteor is active, its blocks cannot be broken.
10. **Waves.** On impact, **3 portals** open 12 blocks above the site; waves drop through them, each when the last is cleared:
    - `waves = 2 + level` (**guess**);
    - each wave has `2 + level` creatures (**guess**), kinds from the planet's own list, tiers from the planet's range, the last wave biased to the top tier;
    - they are always angry, target the meteor's life holders, and are exotic at the normal chance;
    - they do not despawn while the meteor is active, and do not count toward the planet's creature cap.
11. **Meteor life.** As Boundless: from the first kill, each player who comes within the **zone (48 blocks, guess)** gets one life; defeat or Return loses it; leaving the zone starts a 10-second countdown. No holders left: the meteor **fails** and goes dormant.
12. **Cleared.** When the last wave dies the shell becomes breakable (it drops **portal conduits**, one per four blocks broken (**guess**), as Boundless's dropped warp conduits) and the heart can be **used** (the use key). Each player who held a life, or was in the zone at the end, gets their own loot once.
13. **Dormant.** A meteor goes dormant when it fails, or **20 minutes** after impact if not cleared (**guess**). Its remaining creatures leave through the portals; the heart pays anyone once, from the dormant table.
14. **Afterwards.** A heart stays usable for **30 minutes** (**guess**), then turns to shell. The shell and crater stay as ordinary blocks.
15. **Loot** (`assets/meteors.toml`, data like creature drops), per player:
    - wayshards: `4 + 3 x level`, plus the world tier bonus (+25% a tier above 3) (**guess**; a trip costs 1 or 2, a dasher drops up to 6);
    - one to three rolls on a table of coal, copper and iron ore, relic pieces and creature parts (**guess**);
    - dormant: a quarter of the wayshards and one roll.
16. **Persistence.** Meteors, region cooldowns, square timers and discoveries are saved. A meteor that was active when the server stopped wakes dormant.

## Acceptance

- Every planet has named regions; the HUD shows "Name" or "Direction of Name"; discovering one gives 50 x n XP once and says so.
- On lush, reaching a fresh region centre brings a meteor down 60 to 140 blocks away, visible to everyone on the planet; the same region does not do it again for 2 hours.
- No meteor lands within 64 m of a claim or shrine, on water, or on a tier 1 or 2 planet.
- Waves drop through portals one after another; solo level 1 is beatable with copper tools and a sling (checked in the demo and a bot fight).
- Meteor lives work as rule 11; losing them all turns the meteor dormant.
- Clearing every wave makes the shell breakable and lets each participant take their own loot from the heart, once; the wayshard count matches rule 15.
- A server restart mid-fight leaves a dormant meteor, nothing lost or duplicated.
- The 50-bot load test with meteors falling shows no more slow ticks than M13.
- `--demo-ui meteor` shows a meteor mid-fight with portals, a wave and the meteor bar; screenshots in `docs/media/`.

## Measure in game (for Nathan, when convenient)

Numbers no site has; until measured they stay guesses:

1. On a tier 3 world with an atlas: **how many regions**, and roughly how far apart their centres are.
2. Walking out from a region's centre: **at what distance** the HUD changes from the name to "North of ...".
3. A solo meteor on a tier 3 world: its **level**, **how many waves**, roughly **how many creatures per wave**, and **how many rough oortstone** the crate gave.
4. How long an untouched meteor takes to go **dormant**.

## Non-goals

- The atlas (map item), region outlines on a map, and region feats.
- Gleambow meteorites and other events; temporary worlds; Lucent loot.
- Meteors on tier 1 and 2 planets (Boundless had none; revisit if lush is too far for new players).
- Meteorites made of a mineable conduit that portals can use directly (ours drop conduits).
- Gems and fossils (no such items yet); the table grows when they exist.

## Build plan

1. **`bm-core`:** `regions` (layout, nearest region, centre distance, direction words, discovery XP) and `meteors` (level from players, waves, wave sizes, loot rolls), pure and tested.
2. **Data:** `assets/regions.toml` (word lists), `assets/meteors.toml` (loot tables, numbers); blocks `meteor_shell` and `meteor_heart`; textures painted in `bm-texgen`.
3. **Protocol 20:** regions in the planet info (centre and name each); `MeteorFall`, `MeteorState` (level, wave, creatures left, lives, phase), `MeteorLife`; discovery notice.
4. **Server:** regions per planet and naming; discovery and XP; triggers and cooldowns; the fall, crater and shell; waves through creatures (forced angry, exempt from despawn and caps); lives; clear, dormant, heart loot; `SAVE_VERSION` 7 (discoveries, cooldowns, meteors) with the upgrade check; end-to-end tests.
5. **Client:** HUD region line and discovery notice; the falling streak and beam; portals in the air; the meteor bar and the life marker; using the heart; `--demo-ui meteor`.
6. **Verify:** tests, load test, screenshots; Status.

## Status

*Spec written 2026-10-22. Built 2026-10-22; awaiting Nathan's play-test.*

- [x] Research: this spec; tuning 3.2.5 extended (region-centre and square triggers, cooldowns, levels)
- [x] `bm-core`: `regions` (layout, lookup across the seam, HUD wording, discovery XP, names from `assets/regions.toml`) and `meteors` (levels, waves, tiers, loot from `assets/meteors.toml`)
- [x] Blocks `meteor_shell` and `meteor_heart`, painted by `bm-texgen`
- [x] Protocol 20: `Regions`, `Discovered`, `Meteor`, `MeteorGone`, `MeteorLife`; `LootHeart`
- [x] Server (`crates/bm-server/src/game/meteors.rs`):
  - regions named from the land at each centre; discovery with 50 x n XP (`ServerConfig::discovery_xp`, off in the XP-counting tests);
  - the region-centre and square triggers, rests, the planet cap, landing sites clear of claims, shrines and water;
  - the fall, crater, shell and heart; the shell guarded while it fights (swings and blasts);
  - waves through three portals, always angry at life holders, kept while the meteor fights, outside the planet's creature cap; stuck creatures drop back in;
  - meteor lives (zone, 10 s grace, lost on Return or travel), failure and the 20-minute timeout to dormant;
  - the heart's full share for those who fought, dormant share for the rest, once each; every fourth shell block a portal conduit;
  - `SAVE_VERSION` 7: new tables `planet_skies` and `discoveries`; a meteor active at shutdown wakes dormant. Upgrade check on a real M13 universe (bots, 2 planets): "upgraded from save version 6 to 7", the bots rejoined where they were.
- [x] Client (`crates/bm-client/src/meteors.rs`): the region line at the top, discovery notices, the falling meteor and its trail, the beam over a site (from afar), the portals, the meteor bar and life marker, E on an open heart; `--demo-ui meteor`; screenshot `docs/media/m14-meteor.png`.
- [x] Tests: 547 in all (`regions_are_named_and_discovered_once_for_xp`, `a_meteor_falls_fights_and_pays_each_player_once` end to end, store round trips and the version 6 upgrade, unit tests of the rules and the HUD).
- [x] Load test: 50 bots over the four planets, 9 slow ticks in the first minute (M13 noted none worse than M12 on the same machine; this sandbox's numbers mean little). **No meteor fell during it**: the bots do not walk to region centres, so meteors under load are not yet measured.
- Follow-ups:
  - meteors under load (bots that seek region centres);
  - Boundless's revive does not exist yet, so a life is lost on Return;
  - creatures fall from the portals rather than being carried down;
  - no sound; the beam is a plain glowing shaft;
  - the region outline map (the atlas) is a later milestone.
