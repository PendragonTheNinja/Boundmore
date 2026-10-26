//! Chiselled shapes (ADR-0008). A block is divided into eight octants (a
//! 2×2×2 grid of half-blocks); a shape is a family and the set of octants cut
//! away. `Shape(0)` is the full cube in every family.
//!
//! Encoding (`Shape(u16)`, saved in chunks, never reinterpret): bits 0-7 are
//! the removed octants, octant `i = x | z << 1 | y << 2` with each of x, y, z
//! 0 (low half) or 1 (high half); bits 8-10 are the family; bits 11-15 are 0.
//! A shape with bit 11 set is a furnishing's form instead (`crate::forms`):
//! no family, never chiselled, its boxes the form's.
//! How each family draws its cuts (square: cubes; slope: planes through the
//! block's corners; bevel: 45 degree cuts; round: curves) is the mesher's;
//! what is solid is the same for all: the kept octants.

use crate::block::Shape;
use serde::{Deserialize, Serialize};

/// How a family draws a cut. Families 4 to 7 are reserved (lattice is planned).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeFamily {
    /// Cubic cuts: the kept octants as half-block cubes.
    Square = 0,
    /// Sloped surfaces through the block's corners (the only family natural
    /// terrain takes).
    Slope = 1,
    /// Cuts at 45 degrees across each removed octant.
    Bevel = 2,
    /// Rounded cuts (experimental, Nathan 2026-10-19).
    Round = 3,
}

impl ShapeFamily {
    pub const ALL: [ShapeFamily; 4] = [Self::Square, Self::Slope, Self::Bevel, Self::Round];

    fn from_bits(b: u16) -> Option<Self> {
        Self::ALL.get(b as usize).copied()
    }
}

/// Octant index of the half-block at `(x, y, z)`, each 0 or 1.
pub const fn octant(x: u8, y: u8, z: u8) -> u8 {
    (x & 1) | (z & 1) << 1 | (y & 1) << 2
}

/// The six edge pairs of octants along each axis: an ordinary chisel removes
/// one pair (two octants sharing an edge of the block); a precise one removes
/// a single octant.
pub fn edge_pairs() -> impl Iterator<Item = u8> {
    // Two octants differing in exactly one coordinate.
    (0u8..8).flat_map(|a| {
        [1u8, 2, 4]
            .into_iter()
            .filter(move |&d| a & d == 0)
            .map(move |d| (1 << a) | (1 << (a | d)))
    })
}

impl Shape {
    /// A shape of `family` with the octants in `removed` cut away. All eight
    /// removed is not a shape (it is air): `None`.
    pub fn new(family: ShapeFamily, removed: u8) -> Option<Shape> {
        match removed {
            0 => Some(Shape::FULL),
            0xff => None,
            m => Some(Shape(m as u16 | (family as u16) << 8)),
        }
    }

    pub fn is_full(self) -> bool {
        !self.is_form() && self.removed() == 0
    }

    /// Whether this is a furnishing's form (`crate::forms`), not a cut.
    pub fn is_form(self) -> bool {
        self.0 & crate::forms::FORM_BIT != 0
    }

    /// Octants cut away.
    pub fn removed(self) -> u8 {
        self.0 as u8
    }

    /// The family, or `None` for the full cube (which belongs to all).
    pub fn family(self) -> Option<ShapeFamily> {
        if self.is_full() || self.is_form() {
            None
        } else {
            ShapeFamily::from_bits(self.0 >> 8 & 7)
        }
    }

    /// Whether this is a shape the game can make: a known family, at least
    /// one octant kept, no stray bits. Anything from disk or the network is
    /// checked with this before use.
    pub fn is_valid(self) -> bool {
        self.is_full()
            || crate::forms::Form::of(self).is_some()
            || (self.0 >> 11 == 0 && self.removed() != 0xff && ShapeFamily::from_bits(self.0 >> 8 & 7).is_some())
    }

    /// Whether octant `i` is solid.
    pub fn keeps(self, i: u8) -> bool {
        self.removed() & (1 << i) == 0
    }

    /// The solid boxes, in block units within `0..=1`: one per kept octant
    /// (collision and targeting use these for every family, ADR-0008).
    pub fn boxes(self) -> impl Iterator<Item = ([f64; 3], [f64; 3])> {
        let form = crate::forms::Form::of(self).map(|f| f.solid());
        let cut = !self.is_form();
        let octants = (0u8..8).filter(move |&i| cut && self.keeps(i)).map(|i| {
            let (x, z, y) = ((i & 1) as f64, (i >> 1 & 1) as f64, (i >> 2 & 1) as f64);
            (
                [x * 0.5, y * 0.5, z * 0.5],
                [x * 0.5 + 0.5, y * 0.5 + 0.5, z * 0.5 + 0.5],
            )
        });
        octants.chain(form.into_iter().flatten())
    }

    /// This shape with `cut` also removed, if the result is still a shape and
    /// the families agree (a full block takes the chisel's family).
    pub fn chiselled(self, family: ShapeFamily, cut: u8) -> Option<Shape> {
        if self.is_form() || self.family().is_some_and(|f| f != family) {
            return None;
        }
        Shape::new(family, self.removed() | cut)
    }

    /// This shape with `fill` put back (`Shape::FULL` once nothing is removed).
    pub fn restored(self, fill: u8) -> Shape {
        if self.is_form() {
            return self;
        }
        match self.family() {
            None => self,
            Some(f) => Shape::new(f, self.removed() & !fill).unwrap_or(Shape::FULL),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_cube_is_zero_and_every_family_round_trips() {
        assert!(Shape::FULL.is_full() && Shape::FULL.is_valid() && Shape::FULL.family().is_none());
        assert_eq!(Shape::FULL.boxes().count(), 8);
        for f in ShapeFamily::ALL {
            for m in 1u8..0xff {
                let s = Shape::new(f, m).unwrap();
                assert!(s.is_valid());
                assert_eq!((s.family(), s.removed()), (Some(f), m));
                assert_eq!(s.boxes().count(), 8 - m.count_ones() as usize);
            }
            assert_eq!(Shape::new(f, 0xff), None, "all removed is air");
            assert_eq!(Shape::new(f, 0), Some(Shape::FULL));
        }
        assert!(!Shape(0xff | 1 << 8).is_valid(), "nothing kept");
        assert!(!Shape(1 | 5 << 8).is_valid(), "reserved family");
        assert!(!Shape(1 | 1 << 12).is_valid(), "stray bits");
    }

    #[test]
    fn chisels_keep_to_one_family_and_restore_back_to_full() {
        let top = (0..8).filter(|i| i & 4 != 0).fold(0u8, |m, i| m | 1 << i);
        let slab = Shape::FULL.chiselled(ShapeFamily::Square, top).unwrap();
        assert_eq!(slab.boxes().count(), 4, "a half slab");
        assert!(slab.boxes().all(|(lo, hi)| lo[1] == 0.0 && hi[1] == 0.5));
        assert_eq!(slab.chiselled(ShapeFamily::Bevel, 1), None, "families do not mix");
        assert_eq!(
            slab.chiselled(ShapeFamily::Square, !top),
            None,
            "cannot cut it all away"
        );
        assert_eq!(slab.restored(top), Shape::FULL);
        assert_eq!(edge_pairs().count(), 12, "a cube has twelve edges");
        assert!(edge_pairs().all(|m| m.count_ones() == 2));
        assert_eq!(octant(1, 1, 1), 7);
    }
}
