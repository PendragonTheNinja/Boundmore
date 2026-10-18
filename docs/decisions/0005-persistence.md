# ADR-0005: Persistence: one embedded database per planet, edited chunks only

- **Status:** Accepted, 2026-10-18

## Decision

- Each planet is one **redb** file (pure Rust, ACID, crash-safe, MVCC: loader threads read while the writer writes).
- **Only chunks players have changed are stored.** Everything else regenerates from the seed and `GENERATOR_VERSION`. The planet's metadata (name, shape, generation settings, format versions) is stored once and wins over the config file afterwards.
- Players' positions are stored by name on leaving and with every save.
- **Writes are batched**: every 5 seconds the game loop hands every dirty chunk and player to one writer thread, which commits them in one transaction and retries on failure. Chunks are saved before eviction. Shutdown waits for the final save.
- **Reads are retried, never replaced**: if a saved chunk cannot be read, the loader retries rather than serving fresh terrain that a later edit would save over the player's build.
- The server refuses a planet whose generator, chunk format or save version differs from its own.

## Consequences

- Up to 5 seconds of edits can be lost on a power cut (not on a clean stop or a crash of the game thread, since the writer is separate). Acceptable for now; the interval is one constant.
- Accounts, inventories and the economy will need a relational store (Postgres) with transactions across players. That is a separate store with its own ADR; voxels stay here.
