# Milestone 21: Wandering Worlds

*Spec written 2026-10-26.* Boundless's exoworlds and its tier 7 worlds (roadmap, "Higher worlds": "Exoworlds and tiers 7 and 8 later"; the CLAUDE.md handoff lists it as work that needs no decision of Nathan's). Research: `docs/research/exoworlds.md` (the player archive of 419 exoworlds, 2021 to 2024, and the release notes), `boundless-tuning.md` for tier 7 gear, creatures and recipes. Nathan, 2020 (forum t/50855): "Player-made exoworlds that last longer. Such a fun idea."

## Goal

Every couple of days a new world drifts into the sky beside one of the permanent planets: fresh untouched ground in colours found nowhere else, at tiers 4 to 7, reached from its host's warp shrine. It stays about five days, and in its last half hour its air thickens until nobody can breathe there. Then it leaves, taking everything left on it. Some are tier 7 worlds of a new kind, the only home of three new ores and of the strongest creatures and gear in the game.

## Decided in this spec

Boundless's way unless said. Every number here is a tunable in `ServerConfig` or data.

1. **A steady stream, as the live game settled on.** A new wandering world about every 48 hours (Boundless: 281 of 418 gaps exactly two days); each stays **113 to 128 hours**, drawn per world (Boundless's measured range). So about two or three are up at once. Three **slots** run the stream, each arriving every 6 days, staggered 2 days apart; the first arrives when a universe is made, so a new universe has one at once. Time is the universe clock (server ticks, as crops; ADR-0010), so a world does not leave while the server is off.
2. **Tiers and kinds, by Boundless's live mix**: tier 4 10%, tier 5 31%, tier 6 29%, tier 7 30%. A tier 4 wanderer is one of the tier 1 to 3 kinds raised to tier 4 (temperate and lush with acid air, arid with burning, frozen with charged: Boundless raised lush, coal and metal worlds, one per air line); a tier 5 wanderer is one of the tier 4 and 5 kinds at tier 5, a tier 6 one any tier 4 to 6 kind at tier 6 (Boundless's tier 5 and 6 exos were the gem types and the coal and metal types about evenly); tier 7 is one of the three new kinds (part 2). Until part 2 lands, tier 7 draws a tier 6 kind raised to 7.
3. **A host, by tier**: tier 4 wanderers orbit a tier 1 or 2 planet, tier 5 a tier 3, tier 6 and 7 a tier 4 or 5 (the archive's hosts). Picked at random among the permanent planets of those tiers (nearest lower tiers if none). **It is reached only from its host's warp shrine** (the travel window lists it everywhere, greyed with "Reach it from Host's shrine"); from the wanderer, its shrine reaches any world. Warps stay free (Boundless charged coin by distance; Boundmore has no coin).
4. **Nothing can be claimed** on a wandering world, so nothing is protected and no portal can be built there (portals stand on claims, ADR-0009). Everything else works: building, mining, machines, storage, crops, meteors, regions with discovery XP.
5. **Its own colours.** Each wanderer gets a palette drawn from the whole 255-colour palette by a recipe (a base hue, its neighbours and complement, for rock, wood, foliage, soil and sand), not from its kind's lists, so it brings colours the permanent planets lack (Boundless: "unique colours to snag"). Its look (sky and how it is seen from afar) is tinted toward the base hue.
6. **Its last half hour**: from 30 minutes before it leaves, its air hazard climbs from its own to 10 (above any protection: ward 5 and the best food give 10 at most, so 10 is reached in the last minutes) in steady steps. Every player there is told at 24 hours, 1 hour, 30 minutes and 5 minutes before. A tier 4 to 6 wanderer has its line's air from the start; the climb uses the same line.
7. **When it leaves**, anyone still on it is carried to its host's shrine (Boundless let the air kill them; carrying them is gentler and loses nothing they carry: inventories are account-wide). Everything on the world is deleted from the save. A player who was offline on it arrives at the host's shrine when they next log in.
8. **Fresh ids.** Each wanderer is a new planet with a new planet id (counting up from 1000, never reused); its slot in the server's planet list is reused. Nothing saved about an old wanderer can ever be read as part of a new one (ADR-0011).
9. **In the sky**, a wanderer is drawn like any planet with a soft halo, so it stands out; the travel window shows how long it stays.
10. **Size**: 48 chunks around (1,536 blocks, the size of Boundmore's other tier 4 to 6 planets; Boundless's exoworlds were 3,072 m but so were many of its permanent worlds, and Boundmore's planets are half Boundless's throughout).

### Part 2: tier 7 (Boundless's "special" worlds)

11. **Three new planet kinds at tier 7**, each with an air line and a pair of elements: **auroral** worlds (charged air; shock and chill), **pyric** worlds (burning air; blast and burn), **gloaming** worlds (acid air; corrosion and toxin). Moods as Boundless's: cool and bright, warm and bright, dark and red. Tier 7: hazard 7, block health ×5, armour 800 (already in the tables).
12. **Three new ores**, the "astral" ores: **aurorite**, **pyrelith** and **gloamite** seams, one on each kind, rare (Boundless's player counts: 5 to 160 seams a world by kind), deep; mining XP 216; the tier bonus table of gem seams.
13. **Astral creatures**: on a tier 7 world some creatures are made of its ore: they carry both its elements (a hit lands as whichever of the two does most against the target), have +200% health, +100% damage and armour, drop three times and give +75% XP (Boundless's "special" world modifiers, `m18-data-check.md` 17). The chance follows `elemental_chance(7)`.
14. **The astral chain and gear**: compact (10 rough, compactor, 480 s, power 3000) and refined (refinery, 240 s, power 3000); hammers, shovels, axes, grapples, spanners and chisels of each (Boundless's lucent stats and recipes, tuning 1.x, 2.x, 4.10: 3 refined, 4 sticks, hard leather, the best bonding agent, workbench at power 3000, 3600 s); each kind's tools strike with its two elements. What Boundmore lacked of the recipe is added from Boundless's recipes (tuning 4.12): **hard leather** (furnace: 2 leather and 2 buckets of water, 120 s, heat 400) and the **supreme bonding agent** (mixer: 8 glue, 60 fossil essence, 30 vital essence and a shadow orb, 90 s, spark 400, power 1500). Grapples take a relic core, 3 relic components and 10 relic fragments as Boundless's lucent grapples did. Boundless gated the gear at crafting level 4; Boundmore's skill tree stops at 3 (two ranks and the epic), so level 3.
15. **The tier 7 kinds hold no gem seams** (a **guess**: the astral ore takes the gems' place) and draw biomes from set 6.

## Rules (part 1)

1. `ServerConfig`: `exo_slots` (3; 0 turns wandering worlds off), `exo_every_hours` (48), `exo_life_hours` ([113, 128]), `exo_width_chunks` (48), `exo_leaving_minutes` (30).
2. `bm-server/src/exo.rs`: the slot schedule and records (`ExoRecord { id, slot, kind, tier, seed, host, name, arrives, leaves, palette }`), choosing kind, tier, host and name from a seeded rng, the colour recipe, the air at a tick. Pure functions where possible, tested without a server.
3. The save: an `exo` table (slot -> current record) and the next exo id in `META`; a departed wanderer's rows (planet record, chunks, far copies, stations, storage, crops, sky) are deleted in one write. `SAVE_VERSION` stays 14 (new table only).
4. `Game`: planet ids map to list indices through one helper (no more `id as usize`); a hidden (not yet arrived) wanderer is not in the universe list, refuses warps and loads nothing. Each minute the game checks the slots: arrivals, warnings, the air's climb (`ServerMsg::Air { at, air }`, set at an agreed input like a skill change so prediction stays exact), departures.
5. Claims are refused on wanderers ("Nothing can be claimed on a wandering world").
6. Protocol: `PlanetSummary` gains `host: Option<u16>` and `leaves: Option<u64>` (tick); `ServerMsg::Air`. `PROTOCOL_VERSION` 34.
7. Client: the travel window lists wanderers with their host and time left and only lets you travel from the host's shrine; the sky draws a halo round wanderers; the HUD's air line follows `Air`.

## Acceptance (part 1)

1. A new universe has one wandering world at once, then one more every 48 hours of play; three at most (unit test on the schedule; end-to-end test with short times).
2. A wanderer is reachable from its host's shrine and refused from others; claims are refused on it (end-to-end).
3. In its last minutes the air climbs; a player there is told, then carried to the host's shrine when it leaves; its chunks and records are gone from the save; a new wanderer in the slot has a new id and fresh ground (end-to-end, with short times).
4. A player offline on a departed wanderer logs in at its host's shrine.
5. A wanderer's colours are not all drawn from its kind's lists (test), and its tier and air are its own (a tier 4 temperate wanderer has acid air 1).
6. Existing universes open unchanged and gain their first wanderer (upgrade check).
7. Prediction stays exact while the air climbs (no corrections in the end-to-end run).

## Non-goals

- Coin and warp prices; meteorite ichor and what it makes (portal fuel stays wayshards).
- Resin and oil pockets (the rare liquids of exoworlds): not materials in Boundmore yet.
- Tier 8 worlds (in Boundless's data, never live).
- Player-made or rented wandering worlds (that is sovereign worlds, waiting on Nathan).

## Measure in game (for Nathan)

1. Does the rhythm feel like Boundless's (one every two days, five days each)? Too many, too few?
2. Is carrying players home at the end right, or should the air finish them as in Boundless?
3. Do the wanderers' colours feel rare and worth collecting?
4. Reached only from the host: right, or should every shrine reach them?
5. Tier 7: are astral creatures as dangerous as Boundless's lucent ones felt? Do astral tools feel worth the chain?
6. How many astral seams does a tier 7 world hold against Boundless's (pyrelith rarest, gloamite commonest)?

## Status

- *Spec written 2026-10-26.* **Part 1 built the same day** (on branch `ccr-5b303ad7-6coo06`): `bm-server/src/exo.rs` (schedule, tier mix, hosts, names, the colour recipe, the leaving air), `planet::open_exo`, `open_exo_slots`, `next_exo`, `game/exo.rs` (arrivals, warnings, the air's climb as `ServerMsg::Air` at an agreed input, leaving), the `exo` and `exo_gone` tables and `WriteBatch::forget_planets` (ADR-0011), claims refused on wanderers, the travel window's host and time left, the sky's halo. Protocol 34; `SAVE_VERSION` still 14 (new tables only).
- **Tests:** the schedule (2 to 3 up at once from day 4), the tier mix within 3 to 4 points of Boundless's, hosts by tier, raised low kinds' air, the climb to 10, names, colours outside the kind's lists, the look; the store forgets every row under an id in one write (even with a late chunk save in the same batch) and never reuses an id; the predictor lands an air change where the server sets it with no correction; end to end (`wandering_worlds_come_are_reached_from_their_host_and_leave`): one up at once, refused from the wrong shrine, claims refused, the air climbs to 10, the player carried to the host, the list loses it, the next has a new id, nothing left in the save, a player who logged out on it comes back on the host after a restart.
- **Part 2 built the same day:** `bm_core::creatures::{Astral, Essence, strike_element}` (an essence is one element or an astral pair; health, armour, damage, drops and XP multiples by essence; an astral creature resists as the better of its pair; an astral strike lands as whichever of its pair the target resists least), `ItemDef::astral`, `PlanetKind::astral`, `Planet::essence`; the auroral, pyric and gloaming kinds in `assets/planets.toml` (copied from stormy, scorched and acrid, tier 7, biome set 6, their gem seam replaced by their astral seam); three seams, nine chain items, eighteen tools, hard leather and the supreme bonding agent in the catalogues; their textures and icons (`bm-texgen` draws the astral ores as gems lit from within: aurorite cyan, pyrelith amber, gloamite plum). New kinds were pinned without a generator bump: no existing kind's terrain changed.
- **Part 2 tests:** astral pairs, codes, resistance and strike choice; astral drops (both shards, x3); the special multiples; astral ores only on their own worlds and no gems there; every new texture and icon painted and distinct.
- **Seen in game** (software rendering, 2026-10-26): the travel window lists a wanderer with its host and time left (`docs/media/m21-travel.jpg`); a wanderer hangs in the sky with its halo (`docs/media/m21-halo.jpg`); the astral icons (`docs/media/m21-astral-icons.jpg`, before gloamite was moved from red to plum to stand apart from ruby).
- **Load test** (50 bots on a fresh universe with its first wanderer up, 2026-10-26): 10.4 to 13.4 ms a tick on average (M20 measured 14.5 to 16); the wanderers are looked over once a second and cost nothing measurable. One tick of 189 ms came 30 s in, at no wanderer event (none arrives or leaves then); not yet explained, worth watching in later load tests.
- **Upgrade check** (2026-10-26): a universe made by `main` (protocol 33, two bot players on two planets) opened with no warnings, gained its three wanderers (about 25 ms each to make) and kept its players where they were.
- **Fixed while building:** the delete opened a table still open in the same write, so the writer retried forever (caught by the end-to-end test).
