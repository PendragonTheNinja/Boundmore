# ADR-0010: Crops grow by the clock, not by dice

- **Status:** Accepted, 2026-10-22.

## Context

Milestone 16 part 2 brings farming. Boundless's crops "tick" once a game day, and each tick rolls a chance to reach the next of up to 8 stages; fertiliser raised the chance and was itself rolled away a tick at a time. Crops near no player were not simulated; on return the server fast-forwarded them (Testing 224, forum t/31046 and t/31322). Players' main complaints (`research/boundless-forum-feedback.md`, forum t/31141, t/31239, t/43947, t/33042): growth was random (crops sown together ripened days apart), fertiliser vanished without a tick, tilled soil turned back to soil, every harvest had to be replanted by hand, and farming gave little XP. Nathan (2026-10-22): fix all of them; Boundless's farming was "never fully finished"; where Boundless's way is unknown, choose what is best in the long run.

What a crop needs to know is when it was sown and what helps it; its stage then follows. A planet holds many crops, most of them far from any player most of the time.

## Decision

1. **A crop's growth is a pure function of time.** Each crop kind has a base time to ripen (data, `assets/crops.toml`). Conditions known when it is sown or tended (tilled soil, water beside it, fertiliser) multiply that time; nothing is rolled. Its stage at any tick is `elapsed / ripen_time` split into its stages. Crops sown together under the same conditions ripen together. A ripe crop never withers.
2. **The server keeps one record per crop** (`Crop { kind, sown_at, fertilised_at, tilled, wet }`), per planet, keyed by block position, saved in its own table (`crops`, `SAVE_VERSION` 10). Universe ticks are the clock, so a crop nobody is near keeps growing and catches up the moment its chunk loads: no fast-forwarding, no per-tick work for crops out of view.
3. **What the player sees is a block.** Each crop kind has one block per visible stage (`oats_1` to `oats_4`), a prop like the gathering plants (blocks stay data, ADR-0003 and invariant 8). The server swaps the block when the stage changes, as any edit: chunks in memory carry a schedule of the next change per crop (a priority queue by tick); a chunk loaded later is brought up to date as it loads. A record whose block is gone (broken, lapsed to ash, edited away) is dropped; a crop block with no record is wild.
4. **Fertiliser lasts until harvest**, never rolled away. Tilled soil is its own block and stays tilled until broken. Harvesting a ripe crop replants it from its own seed, with its conditions kept. Harvests give XP by how long the crop took.
5. **Yields roll once, at harvest**, from Boundless's drop data (a seed always, a chance of more; the crop by kind), with the server's rng like every other drop. Rolled yields are the one place chance stays: a field gives a steady average.

## Consequences

- No ticking cost grows with the number of crops: work happens only when a stage changes in a loaded chunk, and on chunk load.
- Changing a crop's base time (`assets/crops.toml`) applies to crops already growing: their stage is recomputed from `sown_at`. Records store conditions, not results, so tuning is free.
- The client never needs crop records: it sees blocks and asks the server for a crop's details (time to ripe) when looking at one.
- Wild crops are worldgen flora using a crop's ripe block; breaking one drops its wild yield and nothing replants.
- Later crop conditions that change after sowing (light, nearby blocks) would need re-evaluation on neighbour edits; the record holds `sown_at`, so a change can be folded in by moving an equivalent sowing time. Not built now.
