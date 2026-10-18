# ADR-0004: Netcode: QUIC, authoritative server, client prediction

- **Status:** Accepted, 2026-10-18

## Decision

**Transport: QUIC** (quinn, rustls). One UDP port; encryption is built in; a reliable bidirectional stream and unreliable datagrams share one congestion-controlled connection.

- *Reliable, ordered stream:* handshake, chunks, block changes, edit results, players joining and leaving. One stream, so a block change can never arrive before the chunk it belongs to. The server coalesces frames into one write per wakeup and holds chunks back when a player's send backlog passes 768 KiB, so a slow connection never balloons server memory.
- *Datagrams:* client inputs at 60 Hz, server snapshots at 20 Hz. A lost datagram is never resent; the next supersedes it.

**Simulation: authoritative server, fixed 60 Hz.** The server steps each player only by that player's inputs, one per tick of real time on average. A small bank (8 ticks) lets a late client catch up but caps any burst, so flooding inputs cannot make anyone faster than real time (tested). A missing input is replaced by the previous one.

**Prediction.** The client runs the same `movement::step` on the same inputs and draws itself immediately. Each snapshot carries the last input the server applied and the resulting state. If the client's record for that input differs, it adopts the server's state and replays the unacknowledged inputs; the visible jump is smoothed over about 150 ms. Determinism (fixed step, quantised angles, `libm`) makes corrections rare: none were seen in play-testing on flat ground, slopes or across the seam.

**Redundancy.** Every input datagram carries the last 6 inputs, so up to 5 consecutive lost packets cost nothing.

**Other players** are drawn 7 ticks (about 115 ms) behind the newest snapshot and interpolated, wrap-aware. The client estimates the server clock from snapshot arrivals, trusting the least-delayed one.

**Edits** are predicted: the block changes on screen at once, the server validates (reach, rate limit, chunk sent to that player, breakable or placeable, support, no player in the way) and answers with the block's true state, which the client adopts.

## Consequences

- A player who sends no inputs is not simulated (frozen, not falling). The client keeps ticking while minimised so this only happens to a disconnected client, which the 15 s idle timeout removes.
- Logging in again under the same name replaces the old session (until accounts exist, this means anyone can take a name; see the roadmap).
