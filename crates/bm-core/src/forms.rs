//! Furnishings' forms (milestone 22 part 3, `docs/milestones/22-building-blocks.md`).
//!
//! A form is a fixed model a furnishing block takes (a pole, a pane, a
//! table), turned to how it was placed. It lives in the block's [`Shape`],
//! like a chisel's cut (ADR-0008), so collision, targeting and the mesher
//! read it through [`Shape::boxes`] and the mesher's templates; nothing else
//! needs to know. Forms are made only by placing a furnishing, never by a
//! chisel, and a chisel never cuts one.
//!
//! Encoding (`Shape(u16)`, saved in chunks, never reinterpret): bit 11 set;
//! bits 0-3 the kind, 4-5 the style (plain, stylish, ornate), 6-8 the turn;
//! bits 9-10 and 12-15 are 0. What the turn means is the kind's:
//! an axis (0 x, 1 y, 2 z) for poles and beams, a facing (0 north, 1 east,
//! 2 south, 3 west; north is -z) for the rest, plus 4 for one hung on a wall
//! where a kind can be.
//!
//! Every size is in sixteenths of a block. What collides is a form's solid
//! boxes; what is drawn may add detail (feet, collars) inside or beside them.

use crate::block::{BlockDef, BlockState, Mount, Shape};
use crate::doors::Side;
use serde::{Deserialize, Serialize};

/// The bit that marks a shape as a form.
pub const FORM_BIT: u16 = 1 << 11;

/// What a furnishing is shaped as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormKind {
    /// A round-ish post through the block along an axis.
    Pole = 0,
    /// A square timber through the block along an axis.
    Beam = 1,
    /// A thin sheet standing against one side of the block.
    Pane = 2,
    /// A pedestal: a broad foot, a waist and a top.
    Plinth = 3,
    /// A top on legs.
    Table = 4,
    /// A seat with a back, facing away from its back.
    Chair = 5,
    /// A board on a post, or hung on a wall.
    Sign = 6,
}

impl FormKind {
    pub const ALL: [FormKind; 7] = [
        Self::Pole,
        Self::Beam,
        Self::Pane,
        Self::Plinth,
        Self::Table,
        Self::Chair,
        Self::Sign,
    ];

    fn from_bits(b: u16) -> Option<Self> {
        Self::ALL.get(b as usize).copied()
    }

    /// Whether the turn is an axis (else a facing).
    pub fn axial(self) -> bool {
        matches!(self, Self::Pole | Self::Beam)
    }

    /// The largest turn the kind takes.
    fn max_turn(self) -> u8 {
        match self {
            Self::Pole | Self::Beam => 2,
            Self::Plinth | Self::Table => 0,
            Self::Pane | Self::Chair => 3,
            Self::Sign => 7,
        }
    }
}

/// How fine a furnishing is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    #[default]
    Plain = 0,
    Stylish = 1,
    Ornate = 2,
}

impl Style {
    pub const ALL: [Style; 3] = [Self::Plain, Self::Stylish, Self::Ornate];
}

/// A form: kind, style and turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Form {
    pub kind: FormKind,
    pub style: Style,
    pub turn: u8,
}

impl Form {
    pub fn new(kind: FormKind, style: Style, turn: u8) -> Option<Form> {
        (turn <= kind.max_turn()).then_some(Form { kind, style, turn })
    }

    /// The shape that carries this form.
    pub fn shape(self) -> Shape {
        Shape(FORM_BIT | self.kind as u16 | (self.style as u16) << 4 | (self.turn as u16) << 6)
    }

    /// The form a shape carries, if it is a valid one.
    pub fn of(shape: Shape) -> Option<Form> {
        let s = shape.0;
        if s & FORM_BIT == 0 || s & !(FORM_BIT | 0x1ff) != 0 {
            return None;
        }
        let kind = FormKind::from_bits(s & 15)?;
        let style = *Style::ALL.get((s >> 4 & 3) as usize)?;
        Form::new(kind, style, (s >> 6 & 7) as u8)
    }

    /// Every form there is (tests, the mesher's tables).
    pub fn all() -> impl Iterator<Item = Form> {
        FormKind::ALL.into_iter().flat_map(|k| {
            Style::ALL
                .into_iter()
                .flat_map(move |s| (0..=k.max_turn()).filter_map(move |t| Form::new(k, s, t)))
        })
    }

    /// The boxes that collide, in block units.
    pub fn solid(self) -> Vec<([f64; 3], [f64; 3])> {
        self.turned(&solid_base(self.kind, self.style, self.turn & 4 != 0))
    }

    /// The boxes that are drawn, in sixteenths, turned.
    pub fn drawn(self) -> Vec<([u8; 3], [u8; 3])> {
        drawn_base(self.kind, self.style, self.turn & 4 != 0)
            .iter()
            .map(|b| turn_box(self.kind, self.turn, *b))
            .collect()
    }

    fn turned(self, base: &[([u8; 3], [u8; 3])]) -> Vec<([f64; 3], [f64; 3])> {
        base.iter()
            .map(|b| {
                let (lo, hi) = turn_box(self.kind, self.turn, *b);
                (lo.map(|v| v as f64 / 16.0), hi.map(|v| v as f64 / 16.0))
            })
            .collect()
    }
}

/// The shape a block of `def` takes when placed: a door on the side
/// nearest the placer, a furnishing's form turned as Boundless's were, else
/// the full block. `mount` is the face it was put against (from the clicked
/// face; None for under a ceiling or unknown), `support` the block there,
/// `(dx, dz)` the placer from the block's centre. Client prediction and the
/// server both use this, so they agree.
pub fn placed_shape(def: &BlockDef, mount: Option<Mount>, support: Option<BlockState>, dx: f64, dz: f64) -> Shape {
    if let Some(swing) = def.swing {
        return crate::doors::placed(swing, dx, dz);
    }
    let Some(kind) = def.form else {
        return Shape::FULL;
    };
    let facing = Side::ALL.iter().position(|s| *s == Side::towards(dx, dz)).unwrap_or(0) as u8;
    let turn = match kind {
        // Along the face clicked; against one of its own kind, as that one.
        FormKind::Pole | FormKind::Beam => {
            let along = support
                .and_then(|b| Form::of(b.shape))
                .filter(|f| f.kind == kind)
                .map(|f| f.turn);
            along.unwrap_or(match mount {
                Some(Mount::North | Mount::South) => 2,
                Some(Mount::East | Mount::West) => 0,
                _ => 1,
            })
        }
        FormKind::Pane | FormKind::Chair | FormKind::Sign => facing,
        FormKind::Plinth | FormKind::Table => 0,
    };
    Form::new(kind, def.style, turn).map_or(Shape::FULL, Form::shape)
}

/// A box in sixteenths: low and high corners.
type B = ([u8; 3], [u8; 3]);

const fn bx(x0: u8, y0: u8, z0: u8, x1: u8, y1: u8, z1: u8) -> B {
    ([x0, y0, z0], [x1, y1, z1])
}

/// Turns a base box (modelled along y for axial kinds, facing north for the
/// rest) to the form's turn.
fn turn_box(kind: FormKind, turn: u8, (lo, hi): B) -> B {
    if kind.axial() {
        // Base along y; axis 0 swaps y and x, axis 2 swaps y and z.
        let swap = |v: [u8; 3]| match turn {
            0 => [v[1], v[0], v[2]],
            2 => [v[0], v[2], v[1]],
            _ => v,
        };
        return (swap(lo), swap(hi));
    }
    // Wall-hung (turn & 4) is the base model's own business; then quarter
    // turns about y.
    let (mut lo, mut hi) = (lo, hi);
    for _ in 0..(turn & 3) {
        // A quarter turn clockwise seen from above: north to east,
        // (x, z) -> (16 - z, x).
        let (l, h) = (lo, hi);
        lo = [16 - h[2], l[1], l[0]];
        hi = [16 - l[2], h[1], h[0]];
    }
    (lo, hi)
}

/// What collides, before turning. Tops sit where a player can stand on them
/// without catching (a whole or half block), as ADR-0008's cuts.
fn solid_base(kind: FormKind, style: Style, wall: bool) -> Vec<B> {
    match kind {
        FormKind::Pole => vec![bx(5, 0, 5, 11, 16, 11)],
        FormKind::Beam => vec![bx(4, 0, 4, 12, 16, 12)],
        FormKind::Pane => vec![bx(0, 0, 0, 16, 16, 2)],
        FormKind::Plinth => vec![bx(2, 0, 2, 14, 16, 14)],
        FormKind::Table => vec![bx(0, 8, 0, 16, 16, 16), bx(6, 0, 6, 10, 8, 10)],
        FormKind::Chair => vec![bx(2, 0, 2, 14, 8, 14), bx(2, 8, 12, 14, 16, 14)],
        // A sign faces north, read from the north; hung, it is against the
        // wall on its south side.
        FormKind::Sign if wall => vec![bx(1, 3, 14, 15, 13, 16)],
        FormKind::Sign => {
            let _ = style;
            vec![bx(1, 6, 7, 15, 16, 9)]
        }
    }
}

/// What is drawn, before turning.
fn drawn_base(kind: FormKind, style: Style, wall: bool) -> Vec<B> {
    use Style::*;
    match (kind, style) {
        (FormKind::Pole, Plain) => vec![bx(5, 0, 5, 11, 16, 11)],
        (FormKind::Pole, Stylish) => vec![
            bx(5, 0, 5, 11, 16, 11),
            bx(4, 0, 4, 12, 2, 12),
            bx(4, 14, 4, 12, 16, 12),
        ],
        (FormKind::Pole, Ornate) => vec![
            bx(5, 0, 5, 11, 16, 11),
            bx(3, 0, 3, 13, 2, 13),
            bx(4, 7, 4, 12, 9, 12),
            bx(3, 14, 3, 13, 16, 13),
        ],
        (FormKind::Beam, Plain) => vec![bx(4, 0, 4, 12, 16, 12)],
        (FormKind::Beam, Stylish) => vec![bx(4, 0, 4, 12, 16, 12), bx(3, 7, 3, 13, 9, 13)],
        (FormKind::Beam, Ornate) => vec![
            bx(4, 0, 4, 12, 16, 12),
            bx(3, 1, 3, 13, 3, 13),
            bx(3, 13, 3, 13, 15, 13),
        ],
        // A pane stands against the north side of its block, as a door.
        (FormKind::Pane, Plain) => vec![bx(0, 0, 0, 16, 16, 2)],
        (FormKind::Pane, Stylish) => vec![bx(0, 0, 0, 16, 16, 2), bx(0, 7, 0, 16, 9, 3)],
        (FormKind::Pane, Ornate) => vec![
            bx(0, 0, 0, 16, 16, 2),
            bx(0, 0, 0, 16, 1, 3),
            bx(0, 15, 0, 16, 16, 3),
            bx(7, 0, 0, 9, 16, 3),
        ],
        (FormKind::Plinth, s) => {
            let mut v = vec![
                bx(2, 0, 2, 14, 3, 14),
                bx(4, 3, 4, 12, 13, 12),
                bx(2, 13, 2, 14, 16, 14),
            ];
            if s != Plain {
                v.push(bx(3, 3, 3, 13, 4, 13));
                v.push(bx(3, 12, 3, 13, 13, 13));
            }
            if s == Ornate {
                v.push(bx(1, 0, 1, 15, 1, 15));
            }
            v
        }
        (FormKind::Table, s) => {
            let top = if s == Plain { 14 } else { 13 };
            let mut v = vec![
                bx(0, top, 0, 16, 16, 16),
                bx(1, 0, 1, 3, top, 3),
                bx(13, 0, 1, 15, top, 3),
                bx(1, 0, 13, 3, top, 15),
                bx(13, 0, 13, 15, top, 15),
            ];
            if s != Plain {
                v.push(bx(1, top - 2, 1, 15, top, 15));
            }
            if s == Ornate {
                v.push(bx(1, 3, 3, 3, 4, 13));
                v.push(bx(13, 3, 3, 15, 4, 13));
            }
            v
        }
        (FormKind::Chair, s) => {
            let mut v = vec![
                bx(2, 6, 2, 14, 8, 14),
                bx(2, 0, 2, 4, 6, 4),
                bx(12, 0, 2, 14, 6, 4),
                bx(2, 0, 12, 4, 6, 14),
                bx(12, 0, 12, 14, 6, 14),
                bx(2, 8, 12, 14, if s == Plain { 16 } else { 15 }, 14),
            ];
            if s != Plain {
                v.push(bx(1, 15, 12, 15, 16, 14));
            }
            if s == Ornate {
                v.push(bx(2, 8, 2, 4, 11, 12));
                v.push(bx(12, 8, 2, 14, 11, 12));
            }
            v
        }
        (FormKind::Sign, _) if wall => vec![bx(1, 3, 14, 15, 13, 16)],
        (FormKind::Sign, _) => vec![bx(1, 6, 7, 15, 16, 9), bx(7, 0, 7, 9, 6, 9)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_form_round_trips_and_stays_inside_its_block() {
        let all: Vec<Form> = Form::all().collect();
        assert!(all.len() > 40);
        for f in &all {
            let s = f.shape();
            assert_eq!(Form::of(s), Some(*f), "{f:?}");
            assert!(s.is_valid() && !s.is_full(), "{f:?}");
            assert!(s.family().is_none(), "no chisel family: {f:?}");
            for (lo, hi) in f.solid() {
                assert!((0..3).all(|a| 0.0 <= lo[a] && lo[a] < hi[a] && hi[a] <= 1.0), "{f:?}");
            }
            for (lo, hi) in f.drawn() {
                assert!((0..3).all(|a| lo[a] < hi[a] && hi[a] <= 16), "{f:?}");
            }
        }
        assert_eq!(Form::of(Shape(FORM_BIT | 15)), None, "unknown kind");
        assert_eq!(Form::of(Shape(FORM_BIT | 3 << 4)), None, "unknown style");
        assert_eq!(Form::of(Shape(FORM_BIT | 3 << 6)), None, "a pole has three axes");
        assert_eq!(Form::of(Shape(FORM_BIT | 1 << 12)), None, "stray bits");
    }

    #[test]
    fn turns_put_poles_along_their_axis_and_turn_facings_clockwise() {
        let pole = |t| Form::new(FormKind::Pole, Style::Plain, t).unwrap().solid()[0];
        assert_eq!(pole(1).1[1], 1.0, "upright pole is a block tall");
        assert_eq!((pole(0).0[0], pole(0).1[0]), (0.0, 1.0), "axis x runs along x");
        assert_eq!((pole(2).0[2], pole(2).1[2]), (0.0, 1.0), "axis z runs along z");
        // A chair's back is on the south side facing north; turned east its
        // back is on the west.
        let back = |t| Form::new(FormKind::Chair, Style::Plain, t).unwrap().solid()[1];
        assert_eq!(back(0).0[2], 12.0 / 16.0);
        assert_eq!((back(1).0[0], back(1).1[0]), (2.0 / 16.0, 4.0 / 16.0));
        let pane = |t| Form::new(FormKind::Pane, Style::Plain, t).unwrap().solid()[0];
        assert_eq!(pane(0).1[2], 2.0 / 16.0, "against the north side");
        assert_eq!(
            (pane(1).0[0], pane(1).0[2], pane(1).1[2]),
            (14.0 / 16.0, 0.0, 1.0),
            "east side, along z"
        );
    }
}
