//! Doors and trapdoors (milestone 22 part 2, `docs/milestones/22-building-blocks.md`).
//!
//! A door kind is two blocks, shut and open, each naming the other
//! (`BlockDef::toggle`). What a door looks like and what it stops is its
//! block's shape (ADR-0008's octants), so the mesher and the physics need
//! nothing new: a shut door is a solid half-thick slab on one side of its
//! block; an open door is a passable quarter-wide strip at its hinge, the
//! corner at that side's left end; a shut trapdoor is the solid lower half;
//! an open trapdoor a passable half standing against one side.

use crate::block::Shape;
use crate::shape::{ShapeFamily, octant};
use serde::{Deserialize, Serialize};

/// What a door block is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Swing {
    Door,
    Trapdoor,
}

/// A horizontal side of a block (north is -z, east +x, as `Mount`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    North,
    East,
    South,
    West,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::North, Side::East, Side::South, Side::West];

    /// Whether the octant at half `(x, z)` (each 0 or 1) lies on this side.
    fn holds(self, x: u8, z: u8) -> bool {
        match self {
            Side::North => z == 0,
            Side::East => x == 1,
            Side::South => z == 1,
            Side::West => x == 0,
        }
    }

    /// The side of a block nearest a point `(dx, dz)` from its centre.
    pub fn towards(dx: f64, dz: f64) -> Side {
        if dx.abs() > dz.abs() {
            if dx > 0.0 { Side::East } else { Side::West }
        } else if dz > 0.0 {
            Side::South
        } else {
            Side::North
        }
    }

    /// The hinge corner of a door shut on this side: the side's left end
    /// seen from outside (north's is west, east's north, south's east,
    /// west's south).
    fn hinge(self) -> (u8, u8) {
        match self {
            Side::North => (0, 0),
            Side::East => (1, 0),
            Side::South => (1, 1),
            Side::West => (0, 1),
        }
    }
}

/// The shape keeping exactly the octants `keep` says (square cuts).
fn keeping(keep: impl Fn(u8, u8, u8) -> bool) -> Shape {
    let mut removed = 0u8;
    for y in 0..2 {
        for z in 0..2 {
            for x in 0..2 {
                if !keep(x, y, z) {
                    removed |= 1 << octant(x, y, z);
                }
            }
        }
    }
    Shape::new(ShapeFamily::Square, removed).expect("a door keeps some of its block")
}

/// A shut door's shape: the half on `side`.
pub fn shut_door(side: Side) -> Shape {
    keeping(|x, _, z| side.holds(x, z))
}

/// An open door's shape: the quarter strip at the hinge of the door shut on
/// `side`.
pub fn open_door(side: Side) -> Shape {
    let (hx, hz) = side.hinge();
    keeping(|x, _, z| x == hx && z == hz)
}

/// A shut trapdoor's shape: the lower half.
pub fn shut_trapdoor() -> Shape {
    keeping(|_, y, _| y == 0)
}

/// An open trapdoor's shape: the half standing against `side`.
pub fn open_trapdoor(side: Side) -> Shape {
    keeping(|x, _, z| side.holds(x, z))
}

/// The side a door's shape stands on, shut or open; None if it is neither.
pub fn door_side(shape: Shape) -> Option<Side> {
    Side::ALL
        .into_iter()
        .find(|s| shape == shut_door(*s) || shape == open_door(*s))
}

/// What a door of `swing` in `shape` becomes when swung (`opener` is the
/// side the swinger stands on, for a trapdoor opening). The block id flips
/// to its `toggle`; this is its new shape.
pub fn swung(swing: Swing, shape: Shape, opening: bool, opener: Side) -> Shape {
    match swing {
        Swing::Door => {
            let side = door_side(shape).unwrap_or(opener);
            if opening { open_door(side) } else { shut_door(side) }
        }
        Swing::Trapdoor => {
            if opening {
                open_trapdoor(opener)
            } else {
                shut_trapdoor()
            }
        }
    }
}

/// The shape a door of `swing` is placed with, by someone standing at
/// `(dx, dz)` from the block's centre: a door on the side nearest them, a
/// trapdoor shut.
pub fn placed(swing: Swing, dx: f64, dz: f64) -> Shape {
    match swing {
        Swing::Door => shut_door(Side::towards(dx, dz)),
        Swing::Trapdoor => shut_trapdoor(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(s: Shape) -> u32 {
        8 - s.removed().count_ones()
    }

    #[test]
    fn shut_doors_are_half_slabs_and_open_ones_strips_at_their_hinge() {
        for side in Side::ALL {
            let shut = shut_door(side);
            let open = open_door(side);
            assert_eq!((kept(shut), kept(open)), (4, 2));
            // The hinge strip is part of the shut slab.
            assert_eq!(open.removed() | shut.removed(), open.removed());
            assert_eq!(door_side(shut), Some(side));
            assert_eq!(door_side(open), Some(side));
            assert_eq!(swung(Swing::Door, shut, true, Side::North), open);
            assert_eq!(
                swung(Swing::Door, open, false, Side::South),
                shut,
                "it shuts where it was"
            );
        }
        // Four sides, four different strips.
        let strips: std::collections::HashSet<_> = Side::ALL.iter().map(|s| open_door(*s)).collect();
        assert_eq!(strips.len(), 4);
    }

    #[test]
    fn a_door_is_placed_on_the_placers_side() {
        assert_eq!(placed(Swing::Door, 0.1, -2.0), shut_door(Side::North));
        assert_eq!(placed(Swing::Door, 3.0, 1.0), shut_door(Side::East));
        assert_eq!(placed(Swing::Door, -0.2, 0.9), shut_door(Side::South));
        assert_eq!(placed(Swing::Door, -1.5, 0.3), shut_door(Side::West));
        assert_eq!(placed(Swing::Trapdoor, 1.0, 0.0), shut_trapdoor());
        // A shut trapdoor's top is half way up: walked on, solid below.
        let boxes: Vec<_> = shut_trapdoor().boxes().collect();
        assert!(boxes.iter().all(|(lo, hi)| lo[1] == 0.0 && hi[1] <= 0.5));
    }

    #[test]
    fn trapdoors_open_against_the_swingers_side_and_shut_flat() {
        let open = swung(Swing::Trapdoor, shut_trapdoor(), true, Side::East);
        assert_eq!(open, open_trapdoor(Side::East));
        assert_eq!(kept(open), 4);
        assert_eq!(swung(Swing::Trapdoor, open, false, Side::West), shut_trapdoor());
    }
}
