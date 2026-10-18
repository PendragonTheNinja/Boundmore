# ADR-0003: The server owns the content; clients never generate terrain

- **Status:** Accepted, 2026-10-18

## Context

Two ways to get terrain to a client: send it, or send a seed and let the client generate it. Sending a seed saves bandwidth, but in a game whose economy is built on finding ore, a client that can generate the planet can see every vein on it.

## Decision

- **Chunks are generated on the server only** and streamed, compressed (palette + LZ4: a typical terrain chunk is a few hundred bytes, an air chunk a dozen).
- **The block catalogue comes from the server** in the handshake (`Welcome.blocks`). The client renders whatever the server says a block is and ships no copy that could disagree. The client still validates it (`BlockRegistry::from_defs`).
- The server caches each chunk's encoded network frame, so ten players in one area cost one encode.

## Consequences

- Joining costs about 3 MB of chunk data at the default view radius. Measured: 50 bots joined at once without slow ticks beyond a handful.
- Later bandwidth work stays compatible: hiding fully enclosed ore (anti-x-ray), a smaller radius for underground chunks, delta-compressed re-sends.
