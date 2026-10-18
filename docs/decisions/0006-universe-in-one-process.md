# ADR-0006: Many planets in one server process, one save file per universe

- **Status:** Accepted, 2026-10-18. Supersedes ADR-0005's "one database per planet"; the rest of ADR-0005 stands.

## Context

Boundless ran each planet as its own server on cloud machines; hosting cost was a recurring worry for its players and for Nathan. Boundmore has to run many worlds cheaply, and the first deployment will be one rented machine. Inventories are account-wide, so a player carrying a block from one planet to another, or breaking a block into their pack, must never be saved half on one planet's file and half on another's.

## Decision

- **One server process hosts a universe of planets.** The planet list is configuration (`ServerConfig::planets`: name, kind, seed, size). One game thread steps every planet each tick; a player is on exactly one planet at a time. Accounts, logins, inventories and the travel between planets are universe-wide.
- **An empty planet costs almost nothing.** With no players on it a planet holds no chunks (they are saved and dropped as players leave, as today) and its tick does no work beyond station jobs, which are already timestamp-driven. Nothing scans every planet per tick.
- **One clock for the universe.** Every planet shares the server tick; each has its own `DayCycle` phase and length, so days differ between planets without separate clocks.
- **One redb file per universe** (`data/universe.redb`), tables keyed by planet id where they are per planet (chunks, claims, stations, planet meta) and by account where they are per account (accounts, player data with the planet they are on and their position on it). One write batch covers everything, so the M3 rule holds: a block broken into an inventory is saved with that inventory, on any planet.
- **Planet kinds are data** (`assets/planets.toml`): generation settings and look. A planet copies its kind's generation settings when created and keeps them, so editing the file never changes existing terrain (ADR-0003, `GENERATOR_VERSION` still guards the code).
- **Travel resets the client's world.** `ServerMsg::Travel` arrives on the reliable stream, so everything after it belongs to the new planet; snapshots carry their planet id so a stale datagram is dropped.

## Consequences

- A single thread caps how many busy planets one process can run. When measurements say so, planets step in parallel (they share no mutable state except the account table, which only travel and login touch) or move to separate processes behind a gateway. The save layout already allows splitting the file by planet later.
- Old saves (one planet, `SAVE_VERSION` 2) are upgraded in place into the universe file as its first planet, keeping its name and seed.
