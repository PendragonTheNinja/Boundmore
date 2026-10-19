# ADR-0008: Chiselled shapes are a family and eight octants

- **Status:** Accepted, 2026-10-19.

## Context

Boundless's chisels (research: `docs/research/boundless-mechanics.md` §1) cut a block on a 2×2×2 grid: square cuts make cubes, slope cuts make planes, bevel cuts make 45 degree faces; ordinary chisels cut an edge (two eighths) at a time, precise ones a single eighth; natural terrain takes only slopes. Nathan (2026-10-19): the same families, precise chisels as better tools, plus an experimental round family; lattice frames later. ADR-0007 reserved a `Shape(u16)` in every block state.

## Decision

- **A shape is a family and the set of removed octants** (`bm-core/src/shape.rs`): bits 0-7 the removed octants (`i = x | z << 1 | y << 2`), bits 8-10 the family (square 0, slope 1, bevel 2, round 3; 4-7 reserved, lattice planned), the rest 0. `Shape(0)` is the full cube. All octants removed is air, never a shape. Anything read from disk or the network is checked with `Shape::is_valid`. The encoding is saved in chunks and must never be reinterpreted; new families take reserved numbers.
- **What is solid is the kept octants, for every family.** Collision, targeting and support use those half-block boxes; families differ only in how the cut is drawn. A slope therefore climbs like half-block steps (the movement's step-up handles it) while looking like a ramp. This keeps movement deterministic and identical on client and server (invariant 2) with no per-family physics.
- **A block keeps one family once cut**; a full block takes the family of the first chisel used on it; filling every octant back makes it full again.
- **Catalogue:** `chisel = none | slope | any` per block (natural terrain is `slope`).
- **Chisels are items** (`ItemKind::Chisel { family, precise, durability }`), one use per cut. `EditAction::Chisel { slot, octants, restore }`: the client chooses the octants from where the player aims; the server checks the tool, the family rule, that a precise chisel moves one octant and an ordinary one an edge pair, reach, claims and rate as for any edit.
- **Light and faces:** a cut block is not opaque (light passes, neighbours draw their faces against it); shapes are meshed per block, never greedily merged.

## Consequences

- Climbing a slope feels like half steps rather than a smooth ramp. If play shows that matters, a later ADR can give the slope family its own collision; the saved encoding does not change.
- Up to 254 shapes per family. Finer cuts than eighths (Boundless never had them) would need a new encoding in a reserved family.
