# Milestone 13: Creatures and Combat

*Spec written 2026-10-21. Numbers from `docs/research/boundless-tuning.md` section 3 (weapons, creatures, damage, health, drops), 3.8 (recipes, healing, behaviour) and 5.4a (the game's own skill text), unless marked **guess**. Nathan's direction: "make this game as close to Boundless as humanly possible", Boundless first, identity second.*

## Goal

Planets get creatures, and players get health. You can hunt with slings and tools, and creatures can defeat you. Kills give XP and Boundless's creature parts. Bone makes real glue (1 sap + 1 bone), replacing the 2-sap stand-in from M11, and tallow makes the better torch. The combat skills that waited on the tree since M12 start working.

Built in two parts, as M7 was:

- **Part 1:** health, damage and defeat; slings; three creatures (the herd charger, the spitter, the runner); drops and the recipes they feed; combat skills.
- **Part 2:** the exploder (with bombs, which need its blood) and the flyer; exotic creatures.

Elemental creatures and their shards wait for gem worlds. Meteorites get their own milestone.

## Names (Boundmore's own; Nathan may rename)

| Boundless | Boundmore | What it is |
|---|---|---|
| Wildstock | **grazer** | herd beast; charges and headbutts when angered |
| Spitter | **pelter** | scurrying lizard; shoots from range |
| Roadrunner | **dasher** | lone bird; flees, and drops the rare portal fuel |
| Hopper | **popper** (part 2) | hops about; curls into a ball, then explodes |
| Cuttletrunk | **drifter** (part 2) | flying tentacled thing; bolts, beams, lifts you and drops you |
| Slingbow | **sling** | the ranged weapon (stone, quick copper, heavy iron) |
| Stout, Strong, Mighty, Elite | **Hardy, Tough, Grim, Elder** | creature tier words (tier 1 has none) |

The models are Boundmore's own: chunky stylised box models made in code (`docs/art-direction.md`), tinted from the planet's colours as Boundless tinted its creatures from the biome. They are never Boundless's shapes. The skills named "Slingbow Skill" in M12 are renamed "Sling Skill" (only the names change; ids stay).

## How Boundless did it (the live game, Release 248)

### Health and damage

- **Health** 600. Vitality raises it on Boundless's curve, `600 + ((vit - 10) / 90)^1.5 x 3400`, rounded to 10: 600 at 10, up to 4000 at 100. The health epic triples it.
- **Damage types**: *kinetic* (a single hit: shots, charges), *impulse* (an area: blasts, beams) and *falling*.
- **Armour** is subtracted from each hit. **Resistance** is a chance to take nothing. The skills give:
  - kinetic and impulse armour: +10 a rank (5 ranks);
  - kinetic and impulse resistance: +5% a rank (5 ranks);
  - falling armour: +20 a rank (5 ranks).
- **Outgoing damage**: `(item damage) x (1 + power x 0.05 x (damage epic ? 3 : 1) + mastery x 0.05 + (weapon epic ? 0.25 : 0))`, then crit, then minus the target's armour, never below 0. This is M12's mining formula, now aimed at creatures.
- **Tools hit creatures**: hand stone 200 (the totem), wood 300, stone and copper 400, iron 700.

### Slings

All are kinetic, break on contact and have infinite ammunition. Energy per shot is damage / 50.

| Sling | Damage | Action speed | Shot speed | Range | Crit | Crit effect | Durability | Energy |
|---|---|---|---|---|---|---|---|---|
| stone | 300 | 10 | 30 m/s | 15 m | 0 | 0.20 | 600 | 6 |
| quick copper | 300 | 30 | 55 m/s | 15 m | 0 | 0.20 | 500 | 6 |
| heavy iron | 600 | 10 | 35 m/s | 20 m | 0.20 | 0.20 | 900 | 12 |

- Shots arc and fly through the crosshair, as Boundless did from Release 184.
- Time between shots comes from a13o's action-speed curve (M9). With **Rapid Fire** (epic) the sling fires while the button is held. With **Draw** (epic, either/or with Rapid Fire) holding charges the shot for up to 2 s, to double damage at full charge (charge effect 1).
- Recipes:
  - stone sling: 4 stones, 4 sticks, 4 leaves, at the crafting table, 10 s, 4 XP, no gate;
  - quick copper sling: 3 copper bars, 4 sticks, 1 sackcloth, 1 glue, at the workbench, 60 s, 10 XP, needs Weaponsmithing 1;
  - heavy iron sling: as copper but 3 iron bars, 120 s, 14 XP, needs Weaponsmithing 1.

### Creatures (part 1)

Health, armour and XP by creature tier:

| | Tier 1 | Hardy (2) | Tough (3) | Grim (4) | Elder (5) |
|---|---|---|---|---|---|
| grazer | 1500 / 0 / 60 | 3500 / 0 / 140 | 9000 / 0 / 240 | 25000 / 0 / 360 | 60000 / 0 / 540 |
| pelter | 2000 / 0 / 140 | 4000 / 50 / 240 | 8000 / 200 / 360 | 16000 / 500 / 540 | 32000 / 1000 / 840 |
| dasher | 1000 / 0 / 180 | 2500 / 0 / 300 | 5000 / 0 / 480 | | |

- **Which tiers appear**: the highest creature tier is the world tier minus 1, never below 1 (Boundless's table: tier 3 worlds hold tiers 1 and 2). Our tier 1 and 2 worlds hold tier 1 creatures; the tier 3 world holds tiers 1 and 2.
- **Tier 1 worlds are passive**: nothing attacks unless attacked.
- **New player protection**: below level 5, creatures do not attack you unless you attack them (Boundless's "New Citizen").
- **Grazer**: herds, wanders, never starts a fight. When hurt, or when a weapon is fired near it, it rears and charges.
  - Charge speed 12 m/s, range up to 15 m, hit radius 1.8 m, knockback (Release 172 numbers).
  - Tier 1 charges once per run-up; higher tiers charge more times in a row.
  - Base hit 100 kinetic, x2.5 at Hardy.
- **Pelter**: runs, never walks; strafes around its target and shoots.
  - Tier 1: single shots. Hardy and up add 3-shot bursts and fans. Tough and up add a short-range spread with knockback.
  - Base hit 30 kinetic, x3 at Hardy. It gives up on a target beyond 30 m.
- **Dasher**: alone, rare, never attacks. It squawks and sprints away from any player it notices, can run across water, and hurts itself falling off cliffs. Grapple hooks stick in it.
- **Threat**: each creature keeps a list of who hurt it, weighted by damage, and attacks the top player.
- **Leash**: a creature that has not seen its target for a while gives up and wanders again.
- **Spawning**: creatures appear around players, out of sight, and vanish when nobody is near. A killed creature's spot stays empty for a while. Creatures are not saved.

### Drops

When a creature dies, its body stays for a short while. **Every nearby player hits the body for their own drops** (instanced, Boundless Release 156). Luck raises the counts, as it does for blocks.

| | Drops per kill | Pool (weight / most per roll) |
|---|---|---|
| grazer 1 | 1 to 5 | meat 10/2, bone 7/1, tallow 8/2 |
| grazer 2 | 1 to 6 | + hide 6/2, fresh blood 5/2, milk gland 4/1, eye 2/1 |
| pelter 1 | 1 to 5 | meat 23/2, tallow 19/2, bone 19/2, thick blood 11/2 |
| pelter 2 | 2 to 6 | + hide 4/2 |
| dasher 1 | 1 to 5 | lean meat 11/3, tallow 7/1, bone 7/1, hide 5/1, quick gland 5/2, fresh blood 4/1, rough wayshard **2/2** (Decided 4) |
| dasher 2 | 2 to 7 | + eye 7/2, feather trophy 1/1, rough wayshard 2/4 |

Higher tiers follow tuning 3.2.4 row by row. Elemental shards are left out until elemental creatures exist.

What the parts make now:

- **Glue**: 1 sap + 1 bone, furnace, 20 s, heat 100. This replaces the 2-sap stand-in, which is removed.
- **The better torch** (4 at a time): 1 stick, 1 flint, 1 tallow, 1 sackcloth, at the workbench, 20 s, needs Decorating 1.
- **Fresh vital essence**, from the extractor:
  - 5 bone, 5 tallow or 5 meat (1, 1 or 2 essence, 30 s);
  - 3 hide (2 essence, 60 s);
  - 2 fresh or thick blood (3 essence, 60 s).
  
  Essence is kept for later recipes.

Meat, hide, glands and eyes are kept for food, leather and brews in later milestones.

### Defeat

Boundless's live rules (Release 195):

- Nothing carried is lost or damaged.
- An XP debt of 10% of the XP the next level needs. While in debt you earn half XP, and the debt clears as you earn.
- You come back a short random distance from where you fell (Boundless's sanctum, then a warp back). Boundmore skips the sanctum and respawns you on safe ground nearby.

Kept exactly (Decided 1).

### Skills that start working

- **Toughness** (vitality): health on the curve above.
- **Sling Skill**: +5% sling damage and +2 action speed a rank.
- **Sharpshot** (sling epic): +25% damage and +10 action speed.
- **Rapid Fire** or **Draw**.
- **Might**: now also adds damage to creatures.
- **Hard Skin, Steady, Blast Guard, Braced, Soft Landing**: armour and resistance, per rank as above.
- **Iron Body**: health x3.
- **Mending**: +25 health per 5 s, also in combat.
- **Quiet Step**: creatures notice you at half the range (+50% stealth).
- **Lure**: twice the threat.
- **Second Chance**: half the defeat penalty.
- **Weaponsmithing**: gates the metal slings.

## Acceptance (part 1)

1. **Health.**
   - The HUD shows a health bar beside energy.
   - Damage shows as a red flash and a number.
   - Health is saved with the character (`SAVE_VERSION` 6; older saves start at full health).
   - Health is the same rule on client and server (`bm-core` pure functions).
2. **Damage rules** (`bm-core::combat`): the outgoing formula, crits, armour, resistance chance, kinetic, impulse and falling types, and the vitality curve, each tested against tuning 3.3.
3. **Slings.**
   - The three slings are crafted, held and fired. They cost energy and wear one durability a shot.
   - The server simulates every shot and decides every hit. The client draws its own shots at once, so firing feels instant.
   - **Hits are judged against where the shooter saw the creature** (the server rewinds creatures by the client's view delay, at most 200 ms), so a shot that hit on screen hits.
4. **Tools and the hand stone hit creatures** with their creature damage, on the swing timing from M9.
5. **Creatures.**
   - Grazers, pelters and dashers spawn on every planet, at the tiers above, around players.
   - They move with the planet's physics (they collide with blocks, swim, fall and wrap the seam) and behave as described.
   - They show a health bar with armour pips, and their name with its tier word.
   - Creatures travel to clients as unreliable snapshots, interpolated like other players. Spawning and removal travel on the reliable stream.
6. **Healing and falls.** The stand-in regen. Fall damage with the hook-attached rule and the 3 s grace after release, each tested in `bm-core`.
7. **Defeat.** At 0 health you are defeated. A screen offers "Return", and you respawn on safe ground near where you fell, never inside blocks or water. Then the XP debt applies (Decided 1).
8. **Drops and XP.**
   - Hitting a body gives that player their own roll, once.
   - The kill's XP goes to every player who damaged the creature.
   - Glue's real recipe replaces the stand-in, and the torch, extractor and sling recipes exist.
9. **Skills**: every skill listed above works, and its card stops saying "Comes later".
10. **Load**: 50 bots on one planet with creatures around each keep the 30-second report free of slow ticks.
11. **Protocol 18.** Creature kinds are data (`assets/creatures.toml`: health, armour, XP, speeds, attacks and drops per tier), sent in the handshake. Code refers to kinds by name.
12. **Tests**:
    - every formula against tuning 3.3;
    - drop rolls against the weights;
    - spawning limits;
    - AI state changes (calm, angry, giving up);
    - an end-to-end kill with drops and XP;
    - an end-to-end defeat and respawn;
    - the save upgrade (upgrade check from `main`).

## Acceptance (part 2)

1. **Popper** (tier 3 worlds and up). It hops about passively. When triggered it turns hostile, whistles for at least 1.5 s, then explodes on impact. The blast is impulse damage, base 400, x2.5 at Hardy, and does not pass through walls. It can curl into a ball that cannot be hurt but can be grappled and thrown, and the ball explodes if it hits hard. Hammers stun it. Drops include volatile blood.
2. **Bombs.**
   - The bomb charge: 2 saltpetre, 5 flint, 1 volatile blood.
   - Copper and iron bags, 200 throws each.
   - Hold to light the fuse; a tap drops the bomb at your feet. The blast is area damage, and blocks it destroys drop nothing.
   - The Bomb Skill and Blaster skills start working.
3. **Drifter** (tier 3 worlds and up). It rests on a dock and flies when angered. It strafes and fires bolts and beams. Attacks unlock as its health drops (bomb at 80%, homing at 60%). Higher tiers lift a player and drop them, with a 30 s cooldown.
4. **Exotic creatures**: marked with decals, carrying 1 or 2 buffs on low worlds. A health buff is +200% and an armour buff +100%. Damage and speed buffs are **guesses**. Exotics give more XP and drops.

## Decided (Nathan, 2026-10-21)

1. **Defeat penalty: Boundless exactly.**
   - Nothing carried is lost.
   - An XP debt of 10% of the next level, with half XP until it is paid.
   - Second Chance halves the debt.
   - You respawn on safe ground nearby.
2. **Healing: a stand-in until food exists.** Out of combat, health returns at Boundless's well-fed rate: a full 600 bar in about 7 minutes, which is 1.4 a second. "Out of combat" means 10 s since you last took or dealt damage (**guess**). When food arrives, being well fed takes this over.
3. **Fall damage: as Boundless or slightly gentler, and never a grapple's fault.** Nathan: "one of the main ways I died from fall damage was simply grappling around ... a single grapple let go too early while still in motion killed me so many times".
   - **Attached means safe.** While a hook is attached to anything, a landing does no fall damage, from any height. This is Boundless's own rule: hooking the ground at the end of a long fall saves you.
   - **Grace after letting go.** The first landing within **3 s** (**guess**, `fall_grace_after_release`) of releasing a hook takes no fall damage. A swing let go too early no longer kills. A fall that starts long after the release counts as normal.
   - **The curve.** Damage is a share of max health by impact speed, as Boundless did, softer on sand, soil, snow and leaves, and none in water. Boundless's curve is unpublished, so ours is a **guess**, set slightly gentler and tuned by Nathan in play. Soft Landing (falling armour) applies.
4. **Wayshards from dashers: yes**, at Boundless's weights, as one extra way to get portal fuel. Nathan: the main way players got portal fuel in Boundless was **meteorite hunts**. Meteors fall when you enter a new region, drop waves of creatures, and after the last wave every player loots the core; higher world tiers and more players make bigger meteors with better loot. "Hunts were always a lot of fun so it would be good to have." So meteorites, dropping wayshards, come soon after this milestone (roadmap).

## Measure in game (Nathan offered, 2026-10-21)

Not published anywhere found; every one is a **guess** until measured:

- **Movement**: grazer, pelter and dasher walk and run speeds. One way: time one across a counted stretch of blocks.
- **Ranges**: how far away a pelter notices you; how far it chases before giving up.
- **Damage per attack**: from a tier 1 grazer charge and pelter shot, on a character with no armour skills. The health bar's number before and after is enough.
- **Pelter rate**: shots per second.
- **Time to come back**: from defeat, and how much health you return with.
- **Fall damage**: health lost after falling 10, 20 and 30 blocks onto rock and onto sand, at full health.
- **Kill counts**: how many pelters and grazers you usually see within about 50 blocks on a tier 1 world.

## Non-goals

- Elemental creatures, shards and status effects.
- Meteorites and hunts.
- Augments, brews, oils, leather (refinery), food and hunger (Decided 2: a regen stand-in until food).
- Rage and focus.
- Revive and the reviver tools.
- Healing bombs.
- Fists (a reward-gated item in Boundless).
- Melee weapons and shields (Boundless's never-released Testing 249).
- PvP: Boundless had none.
- Taming and mounts.

## Build plan (part 1)

**Foundation (done first, shared, `bm-core` and `bm-protocol`):**
- `combat.rs`: the health curve, `taken` (armour and resistance), `dealt` (crits and creature armour), falls (`fall_damage`, `SOFT_LANDING`, the hook rule and `FALL_GRACE_TICKS`), regeneration (`CALM_REGEN` every 5 s after `CALM_TICKS`; `MENDING_REGEN`), `shot_step` (shots arc by `SHOT_GRAVITY` x fall-off), `charge_multiplier` (Draw).
- Health lives in `PlayerState` (`hurt`, `calm`, `beat`, `loose`, `down`); falls and healing are stepped in `movement::step`. The server's hits use `PlayerState::take`, and a respawn uses `revive`.
- `Body` gains `health_bonus`, `falling_armour` and `mending`. `PhysicsWorld::soft` (soft ground, `BlockFlags::soft`) is on both sides.
- `creatures.rs` and `assets/creatures.toml`:
  - kinds, tiers and drops;
  - `tiers_on(world tier)`, `hostile_world`, `PROTECTED_BELOW_LEVEL`;
  - `roll_drops` (luck scales the number of rolls, a guess);
  - `CreatureDef::move_config`: creatures move by `movement::step`, the players' own physics.
- `ItemKind::Sling(SlingStats)` and three slings.
- Skills:
  - `Effects` gains vitality, `defence()`, `skilled_sling`, `noticed_at` (Quiet Step), `threat` (Lure), `max_health`, and `crafting("weapon_crafting")`;
  - `Character::debt`, `skills::defeat_debt`, and `add_xp` halving while in debt.
- Protocol 18:
  - client messages `Shoot`, `Strike`, `Loot`, `Return`;
  - server messages `Creature`, `CreatureGone`, `Shot`, `ShotEnd`, `Hit`, `Defeated`;
  - `Welcome::creatures`;
  - snapshots carry `creatures` (27 players, or 40 creatures, per datagram; further snapshots of the same tick carry the rest).
- Recipes: glue is 1 sap + 1 bone; slings; the tallow torch (new blocks appended at the end of `blocks.toml`); vital essence at the extractor.

**Server (agent):**
- **Creatures**, `game/creatures.rs`: spawning, despawning, AI, physics by `movement::step`, threat lists, and position history for rewinding hits.
- **Combat:**
  - `Shoot` and `Strike` on the input timeline, as swings are: cadence, energy, wear, server-side shots;
  - hits judged against creatures rewound by the client's view delay;
  - creature attacks on players: `take`, `Hit`, then defeat with `Defeated` and the debt;
  - `Return`: safe ground nearby, `Teleport`, revive at half health (a guess);
  - XP for every player who damaged the creature.
- **Loot**: per-player rolls on a body.
- **Saves**: `SAVE_VERSION` 6 (`hurt`, `debt`), with the upgrade check from `main`.
- **Snapshots** and creature view sets.
- **Tests**: end to end.
- **Load**: the 50-bot run.

**Client (agent):**
- **Prediction**: health is server-led. A snapshot that disagrees only on health is adopted without counting a correction.
- **HUD**: the health bar, damage flash and numbers; the defeat screen with Return and the XP debt.
- **Creatures**: models (chunky boxes, Boundmore's own designs), animated by pose and interpolated like players. A health bar with armour pips and the name with its tier word.
- **Fighting**: aim at creatures (strike with tools; shoot with slings, with Rapid Fire and Draw); your own shots drawn at once and others' arcs from `Shot`; looting bodies.
- **Demo screens**: `--demo-ui creatures` and `--demo-ui defeat`.

**Art (agent):** icons for every new item (slings, creature parts, essence, trophies) and the tallow torch's texture, in `tools/bm-texgen`.

## Status

*Spec written and decided 2026-10-21.*

- [x] Research: tuning 3.1 to 3.7 (existing), 3.8 added (recipes, healing, behaviour; boundlexx has no creature data)
- [x] Spec
- [x] Nathan's decisions (2026-10-21): Boundless's defeat debt; a regen stand-in; fall damage gentler with grapple safety; dashers drop wayshards, meteorites next
- [x] Part 1 foundation (2026-10-21): `bm-core` combat, creatures, slings, skills, debt; protocol 18; recipes and the tallow torch (Build plan above)
- [ ] Part 1 server, client and art (agents)
- [ ] Part 2: popper and bombs; drifter; exotics
- [ ] Nathan's play-test
