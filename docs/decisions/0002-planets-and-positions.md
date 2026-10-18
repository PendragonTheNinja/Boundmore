# ADR-0002: Planets wrap horizontally; positions are canonical

- **Status:** Accepted, 2026-10-18

## Context

Boundless worlds have no edge: walk far enough and you come back round. A voxel grid can do that by wrapping X and Z (a torus), shown from orbit as a sphere. Wrapping creates a seam where coordinate `width - 1` meets `0`, and seams are where bugs live.

## Decision

- A planet is `PlanetShape { width_chunks, height_chunks }`: square, wrapping in X and Z, bounded in Y (bedrock floor, sky limit).
- **Every stored and transmitted position is canonical**: X and Z in `0..width`. The server wraps a player after every movement step; the client does the same in prediction, so both agree.
- Code never subtracts two positions directly. `PlanetShape::delta` gives the shortest signed difference; `nearest_lap(reference, value)` moves a position to the lap nearest a reference, for interpolation and rendering.
- Storage is keyed by canonical chunk position. Queries wrap on the way in, so physics works on unwrapped inputs near the seam.
- Terrain noise is periodic in the planet's width (`bm-worldgen::noise`), so the seam is invisible.
- The renderer draws each chunk at its lap nearest the camera.

## Consequences

- A planet must be at least twice the view diameter wide, or a player would see the same chunk twice (`ServerConfig::validate`).
- Tests cross the seam explicitly: movement, deltas, interpolation, worldgen tiling. In play-testing, a player walked from x 2040 to x 4 on a 2048-wide planet with zero prediction corrections.
