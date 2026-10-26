# ADR-0011: Temporary planets live in slots and never reuse an id

- **Status:** Accepted, 2026-10-26.

## Context

Milestone 21 brings wandering worlds (Boundless's exoworlds): planets that arrive, stay about five days and leave for good, taking everything on them. Until now the universe's planets were fixed by configuration (ADR-0006): planet `i` of `ServerConfig::planets` has id `i`, the game indexes its planet list (and the per-planet lists beside it: creature herds, block damage, skies) by that id, and the save keys chunks, far copies, stations, storage, crops and skies by planet id.

A temporary planet breaks two of those assumptions. Its rows must be deleted when it leaves, while chunk saves and loads for it may still be in flight on the writer and loader threads; and players saved on it (online or offline) must not later find themselves on whatever planet replaces it. Config may also grow new permanent planets at the end of the list at any time.

## Decision

1. **Slots.** The game's planet list is the permanent planets (ids `0..n` as before) followed by `exo_slots` slots. A slot always holds one planet: the wanderer that is up, or the next one, made in advance and hidden until it arrives. The per-planet lists beside the planet list keep their indices; a slot's entries are reset when its planet changes.
2. **Ids count up from 1000 and are never reused.** Each wanderer gets the next id (kept in the save's `meta`). A permanent planet's id is its config index, as before, so the two never meet (configs stay far below 1000 planets; validated). Planet ids map to list indices through one function; no code turns an id into an index by casting.
3. **Leaving deletes by id.** When a wanderer leaves, its players are moved off, its chunks are dropped from memory unsaved, and one write deletes every row under its id (planet record, chunks, far copies, stations, storage, crops, sky). Because the id is never reused, a late load or a queued save for it can only touch rows nobody will read again, and a stale row can never appear in a new world. A late save that lands after the delete leaves a few orphaned rows, which the next start's sweep deletes (rows under an id that is neither permanent nor a slot's current wanderer).
4. **Records.** The `exo` table keeps each slot's current wanderer (`ExoRecord`); departed wanderers' host is kept by id (`exo_gone`, small) so a player who logs in on one is sent to its host.

## Consequences

- Wanderers cost what a planet costs while they are up, and nothing after.
- Portal, claim and player records may name only planets that exist; claims and portals are refused on wanderers, so only player positions ever name a departed one, and login handles that.
- Raising `exo_slots` adds slots at the end of the list; lowering it sends the extra slots' wanderers away at the next start (their players arrive at the host).
