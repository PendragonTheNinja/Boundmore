//! Doors, trapdoors and the lock (milestone 22 part 2,
//! `docs/milestones/22-building-blocks.md`), and the pipe they are made with.
//!
//! The game draws a door as a half-thick slab a block tall and wide, and a
//! trapdoor as a half-height slab, with one texture on every face, so each
//! texture is painted as the face of the leaf: a door stands up (its boards
//! and panels run up and down, hinges on the left, a handle on the right),
//! a trapdoor is a hatch seen from above (boards across, braced, a ring to
//! lift it by). Three grades step up in richness: plain is boards in a
//! frame; stylish is panelled, with iron straps and studs; ornate is carved
//! or inlaid, with scrolled hinges and a ring pull.
//!
//! Four materials: wood and stone are colourable, painted through timber's
//! and stone's own ramps and grain so they recolour as those do; metal is
//! iron with darker fittings and gold inlay; crystal is the glow crystal's
//! cyan with silver fittings and gold inlay. Each design is a height field
//! and a map of parts (body, gap, fitting, inlay), lit from the top left.

use std::f32::consts::TAU;

use crate::building::{Rock, Wood, face_inset, ridge, rise, sparkles};
use crate::color::Ramp;
use crate::icons::{Icon, seg_dist};
use crate::materials::{crystal_ramp, steel_ramp};
use crate::noise::{hash2, seed, unit, value};
use crate::spark::dark_iron_ramp;
use crate::stations::iron_ramp;
use crate::tex::{Field, SIZE, SIZEF, Tex, c};
use crate::worlds::gold_alloy_ramp;

// ---------------------------------------------------------------- kinds

/// How rich a door is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grade {
    Plain,
    Stylish,
    Ornate,
}

impl Grade {
    pub const ALL: [Grade; 3] = [Grade::Plain, Grade::Stylish, Grade::Ornate];

    pub fn name(self) -> &'static str {
        match self {
            Grade::Plain => "plain",
            Grade::Stylish => "stylish",
            Grade::Ornate => "ornate",
        }
    }
}

/// What a door is made of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mat {
    Wood,
    Stone,
    Metal,
    Crystal,
}

impl Mat {
    pub const ALL: [Mat; 4] = [Mat::Wood, Mat::Stone, Mat::Metal, Mat::Crystal];

    pub fn name(self) -> &'static str {
        match self {
            Mat::Wood => "wood",
            Mat::Stone => "stone",
            Mat::Metal => "metal",
            Mat::Crystal => "crystal",
        }
    }

    fn body(self) -> Ramp {
        match self {
            Mat::Wood => Wood::Timber.ramp(),
            Mat::Stone => Rock::Stone.ramp(),
            Mat::Metal => iron_ramp(),
            Mat::Crystal => crystal_ramp(),
        }
    }

    /// Hinges, studs and handles.
    fn fit(self) -> Ramp {
        match self {
            Mat::Wood | Mat::Stone | Mat::Metal => dark_iron_ramp(),
            Mat::Crystal => iron_ramp(),
        }
    }

    /// The middle tone the body is painted at.
    fn base(self) -> f32 {
        match self {
            Mat::Wood => 0.56,
            Mat::Stone => 0.58,
            Mat::Metal => 0.46,
            Mat::Crystal => 0.5,
        }
    }

    /// Boards across a leaf.
    fn boards(self) -> u32 {
        match self {
            Mat::Wood => 4,
            Mat::Stone => 2,
            Mat::Metal => 1,
            Mat::Crystal => 3,
        }
    }

    /// Whether ornament is inlaid in gold (else carved in the body).
    fn inlaid(self) -> bool {
        matches!(self, Mat::Metal | Mat::Crystal)
    }
}

// ---------------------------------------------------------------- parts

/// What one point of a design is made of.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Part {
    /// A dark gap, groove or hole.
    Gap,
    /// The leaf's own material: `piece` picks its shade, `across` turns its
    /// grain, `tone` lightens or darkens it.
    Body { piece: u32, across: bool, tone: f32 },
    /// Iron fittings: hinges, straps, studs, handles.
    Fit,
    /// Gold inlay.
    Inlay,
}

/// A point of a design: its height and what it is.
#[derive(Clone, Copy, Debug)]
struct Px {
    h: f32,
    part: Part,
}

fn body(h: f32, piece: u32, across: bool) -> Px {
    Px {
        h,
        part: Part::Body {
            piece,
            across,
            tone: 0.0,
        },
    }
}

fn toned(h: f32, piece: u32, across: bool, tone: f32) -> Px {
    Px {
        h,
        part: Part::Body { piece, across, tone },
    }
}

fn gap(h: f32) -> Px {
    Px { h, part: Part::Gap }
}

fn fit(h: f32) -> Px {
    Px { h, part: Part::Fit }
}

/// A raised ornament: gold inlay in metal and crystal, carved relief in
/// wood and stone.
fn ornament(m: Mat, h: f32) -> Px {
    if m.inlaid() {
        Px {
            h: h + 0.05,
            part: Part::Inlay,
        }
    } else {
        toned(h + 0.6, 40, false, 0.14)
    }
}

/// How far a point lies inside the rectangle (negative outside).
fn inside(p: (f32, f32), x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    (p.0 - x0).min(x1 - p.0).min(p.1 - y0).min(y1 - p.1)
}

/// A round stud's height above its plate (none outside it).
fn stud(fx: f32, fy: f32, cx: f32, cy: f32, r: f32) -> Option<f32> {
    let d = (fx - cx).hypot(fy - cy) / r;
    (d < 1.0).then(|| (1.0 - d * d).sqrt())
}

/// A ring's height (none off it): `r` its radius, `w` half its thickness.
fn ring(fx: f32, fy: f32, cx: f32, cy: f32, r: f32, w: f32) -> Option<f32> {
    let d = ((fx - cx).hypot(fy - cy) - r).abs() / w;
    (d < 1.0).then(|| (1.0 - d * d).sqrt())
}

/// Splits `v` in `lo..hi` into `n` boards on whole pixels: which board,
/// how far in from its start, and how far from its end.
fn board(v: f32, lo: f32, hi: f32, n: u32) -> (u32, f32, f32) {
    let w = (hi - lo) / n as f32;
    let edge = |k: u32| (lo + k as f32 * w).round();
    let k = (1..n).filter(|&k| v >= edge(k)).count() as u32;
    (k, v - edge(k), edge(k + 1) - v)
}

/// A frame `w` wide round the face, bevelled outside. Doors run their
/// stiles the full height; `rails_full` runs the rails the full width
/// instead (a hatch's frame).
fn frame(fx: f32, fy: f32, w: f32, rails_full: bool, groove: bool) -> Option<Px> {
    let d = face_inset((fx, fy));
    if d >= w {
        return None;
    }
    let stile = fx < w || fx > SIZEF - w;
    let rail = fy < w || fy > SIZEF - w;
    let is_stile = if rails_full { stile && !rail } else { stile };
    let mut h = 0.55 + 0.45 * rise(d, 1.3);
    if groove {
        h -= ridge(d, w * 0.45, w * 0.45 + 1.2) * 0.35;
    }
    let piece = match (is_stile, fx < 16.0, fy < 16.0) {
        (true, true, _) => 0,
        (true, false, _) => 1,
        (false, _, true) => 2,
        (false, _, false) => 3,
    };
    Some(body(h, piece, !is_stile))
}

/// Boards inside a frame: `along_y` boards stand up (a door), else lie
/// across (a hatch), each bevelled, with a dark joint between.
fn boards(m: Mat, fx: f32, fy: f32, lo: f32, hi: f32, along_y: bool) -> Px {
    let v = if along_y { fx } else { fy };
    let (k, a, b) = board(v, lo, hi, m.boards());
    // A dark joint on each board's leading edge (the frame's rebate before the first).
    if a < 1.0 {
        return gap(0.1);
    }
    body(0.45 + 0.35 * rise(a.min(b) - 0.5, 1.2), 10 + k, !along_y)
}

/// A raised panel in `x0..x1, y0..y1`: sunk at its edge, then rising in a
/// bevel to a flat field (none outside it).
fn panel(fx: f32, fy: f32, x0: f32, y0: f32, x1: f32, y1: f32, piece: u32) -> Option<Px> {
    let d = inside((fx, fy), x0, y0, x1, y1);
    (d > 0.0).then(|| body(0.15 + 0.65 * rise(d - 0.6, 1.8), piece, false))
}

// --------------------------------------------------------------- doors

/// The knob, plate or ring pull every door has on its right.
fn handle(g: Grade, fx: f32, fy: f32) -> Option<Px> {
    match g {
        Grade::Plain => {
            // A round knob on a small rose.
            if let Some(s) = stud(fx, fy, 25.5, 17.0, 1.8) {
                return Some(fit(1.2 + s * 0.6));
            }
            stud(fx, fy, 25.5, 17.0, 2.6).map(|s| fit(1.0 + s * 0.2))
        }
        Grade::Stylish => {
            // A long plate with a grip above a keyhole.
            let d = inside((fx, fy), 23.6, 11.5, 27.4, 22.5);
            if d <= 0.0 {
                return None;
            }
            let key = (fx - 25.5).hypot(fy - 19.2) < 0.9 || ((fx - 25.5).abs() < 0.45 && (19.2..21.0).contains(&fy));
            if key {
                return Some(gap(0.9));
            }
            if (fx - 25.5).abs() < 0.9 && (13.0..17.5).contains(&fy) {
                return Some(fit(1.75));
            }
            Some(fit(1.0 + 0.25 * rise(d, 0.8)))
        }
        Grade::Ornate => {
            // A round escutcheon with a boss and a ring hanging from it.
            if let Some(s) = ring(fx, fy, 25.5, 20.6, 2.2, 0.75) {
                return Some(fit(1.4 + s * 0.4));
            }
            if let Some(s) = stud(fx, fy, 25.5, 17.0, 1.1) {
                return Some(fit(1.45 + s * 0.4));
            }
            if (fx - 25.5).hypot(fy - 17.0) < 3.0 {
                let r = (fx - 25.5).hypot(fy - 17.0);
                return Some(fit(1.05 + ridge(r, 1.6, 3.0) * 0.3));
            }
            None
        }
    }
}

/// A strap hinge on the left: a band at height `yc` out to `len`,
/// studded; an ornate one ends in a spearhead with a stud in it.
fn hinge(fx: f32, fy: f32, yc: f32, len: f32, spear: bool) -> Option<Px> {
    let studs: &[f32] = if spear {
        &[2.5, len + 1.6]
    } else {
        &[2.5, len * 0.5 + 1.0, len - 1.5]
    };
    for &sx in studs {
        if let Some(s) = stud(fx, fy, sx, yc, 0.95) {
            return Some(fit(1.35 + s * 0.4));
        }
    }
    let dy = (fy - yc).abs();
    if spear && fx >= len {
        // A diamond head: widening to 2.4, then coming to a point.
        let t = fx - len;
        let half = if t < 1.6 { 1.2 + t * 0.75 } else { 2.4 - (t - 1.6) * 1.2 };
        return (dy < half).then(|| fit(1.0 + 0.25 * rise(half - dy, 0.7)));
    }
    let half = if !spear && fx > len - 1.0 {
        1.3 * (1.0 - (fx - len + 1.0) / 2.0).max(0.0) + 0.6
    } else {
        1.3
    };
    (fx < len + 0.9 && dy < half).then(|| fit(1.0 + 0.2 * rise(half - dy, 0.6)))
}

fn plain_door(m: Mat, fx: f32, fy: f32) -> Px {
    if let Some(p) = handle(Grade::Plain, fx, fy) {
        return p;
    }
    if let Some(p) = frame(fx, fy, 3.5, false, false) {
        // Metal: rivets down the stiles and along the rails.
        if m == Mat::Metal {
            let near = |v: f32| ((v - 1.75).rem_euclid(5.0) - 2.5).abs();
            let on_stile = !(3.5..=SIZEF - 3.5).contains(&fx);
            let (cx, along) = if on_stile {
                (if fx < 16.0 { 1.75 } else { SIZEF - 1.75 }, near(fy))
            } else {
                (if fy < 16.0 { 1.75 } else { SIZEF - 1.75 }, near(fx))
            };
            let across = if on_stile { fx - cx } else { fy - cx };
            if let Some(s) = stud(along, across, 0.0, 0.0, 0.85) {
                return fit(1.2 + s * 0.4);
            }
        }
        return p;
    }
    // Nails at the top and bottom of each wooden board.
    if m == Mat::Wood {
        let (k, _, _) = board(fx, 3.5, 28.5, 4);
        let cx = 3.5 + 25.0 / 4.0 * (k as f32 + 0.5);
        for ny in [6.5, 25.5] {
            if let Some(s) = stud(fx, fy, cx, ny, 0.8) {
                return fit(0.9 + s * 0.3);
            }
        }
    }
    let mut p = boards(m, fx, fy, 3.5, 28.5, true);
    // A crystal door's prisms glow up the middle of each.
    if m == Mat::Crystal
        && let Part::Body { piece, across, .. } = p.part
    {
        let (_, a, b) = board(fx, 3.5, 28.5, 3);
        let d = a.min(b);
        p.part = Part::Body {
            piece,
            across,
            tone: (d / 4.2).min(1.0) * 0.12,
        };
    }
    p
}

fn stylish_door(_m: Mat, fx: f32, fy: f32) -> Px {
    if let Some(p) = handle(Grade::Stylish, fx, fy) {
        return p;
    }
    for yc in [7.5, 24.5] {
        if let Some(p) = hinge(fx, fy, yc, 12.0, false) {
            return p;
        }
    }
    for (sx, sy) in [(2.0, 2.0), (30.0, 2.0), (2.0, 30.0), (30.0, 30.0)] {
        if let Some(s) = stud(fx, fy, sx, sy, 1.1) {
            return fit(1.2 + s * 0.4);
        }
    }
    if let Some(p) = frame(fx, fy, 4.0, false, false) {
        return p;
    }
    // The middle rail.
    if (14.5..17.5).contains(&fy) {
        let d = (fy - 14.5).min(17.5 - fy);
        return body(0.75 + 0.25 * rise(d, 0.8), 6, true);
    }
    let piece = if fy < 16.0 { 20 } else { 21 };
    let (y0, y1) = if fy < 16.0 { (4.0, 14.5) } else { (17.5, 28.0) };
    panel(fx, fy, 4.0, y0, 28.0, y1, piece).unwrap_or_else(|| gap(0.2))
}

/// The ornate door's upper panel: a round arch over a rectangle.
fn arch_inset(fx: f32, fy: f32) -> f32 {
    let (x0, x1, spring, bottom) = (10.0, 24.0, 12.0, 16.0);
    let r = (x1 - x0) * 0.5;
    let side = (fx - x0).min(x1 - fx).min(bottom - fy);
    if fy < spring {
        side.min(r - (fx - 17.0).hypot(fy - spring))
    } else {
        side
    }
}

/// A moulding round a panel: a rounded rib just outside its edge.
fn moulding(d: f32) -> Option<Px> {
    (d > -1.4 && d <= 0.0).then(|| toned(1.0 + ridge(d, -1.4, 0.0) * 0.35, 7, false, 0.06))
}

fn ornate_door(m: Mat, fx: f32, fy: f32) -> Px {
    if let Some(p) = handle(Grade::Ornate, fx, fy) {
        return p;
    }
    for yc in [7.5, 24.5] {
        if let Some(p) = hinge(fx, fy, yc, 5.5, true) {
            return p;
        }
    }
    // Studs along the top and bottom rails.
    for sx in [7.0, 12.0, 17.0, 22.0, 27.0] {
        for sy in [2.2, SIZEF - 2.2] {
            if let Some(s) = stud(fx, fy, sx, sy, 0.95) {
                return fit(1.25 + s * 0.4);
            }
        }
    }
    if let Some(p) = frame(fx, fy, 4.5, false, true) {
        return p;
    }
    // The arched upper panel, sunk in a moulding, a ringed boss in it.
    let a = arch_inset(fx, fy);
    if let Some(p) = moulding(a) {
        return p;
    }
    if a > 0.0 {
        let h = 0.25 + 0.3 * rise(a - 0.4, 1.4);
        if let Some(s) = stud(fx, fy, 17.0, 11.0, 1.5) {
            return ornament(m, h + s * 0.2);
        }
        if ring(fx, fy, 17.0, 11.0, 3.6, 0.7).is_some() {
            return ornament(m, h);
        }
        // Four leaves round the ring, along the diagonals.
        let (dx, dy) = (fx - 17.0, fy - 11.0);
        let r = dx.hypot(dy);
        if (dx.abs() - dy.abs()).abs() < 0.8 * (6.4 - r).max(0.0) && r > 4.6 && fy < 14.6 {
            return ornament(m, h);
        }
        return toned(h, 22, false, -0.06);
    }
    // The lower panel: a raised lozenge, its heart inlaid or carved.
    let low = inside((fx, fy), 10.0, 19.5, 24.0, 27.5);
    if let Some(p) = moulding(low) {
        return p;
    }
    if low > 0.0 {
        let (dx, dy) = ((fx - 17.0) / 1.6, fy - 23.5);
        let dia = 3.6 - (dx.abs() + dy.abs());
        let h = 0.25 + 0.3 * rise(low - 0.4, 1.4);
        if dia > 1.9 {
            return ornament(m, h + 0.3);
        }
        if dia > 0.0 {
            return body(h + 0.35 * rise(dia, 1.0), 24, true);
        }
        return toned(h, 23, false, -0.06);
    }
    // The rest: the inner frame round the panels, a rail between them.
    body(0.95, 8, (16.0..19.5).contains(&fy))
}

// ------------------------------------------------------------ trapdoors

/// A ring to lift a hatch by, on a staple plate, at `(cx, cy)`.
fn lift_ring(fx: f32, fy: f32, cx: f32, cy: f32, r: f32) -> Option<Px> {
    if let Some(s) = ring(fx, fy, cx, cy, r, 0.75) {
        return Some(fit(1.4 + s * 0.4));
    }
    let plate = inside((fx, fy), cx - 1.6, cy - r - 1.6, cx + 1.6, cy - r + 0.8);
    (plate > 0.0).then(|| fit(1.15 + 0.15 * rise(plate, 0.6)))
}

fn plain_trapdoor(m: Mat, fx: f32, fy: f32) -> Px {
    if let Some(p) = lift_ring(fx, fy, 16.0, 24.0, 2.4) {
        return p;
    }
    if let Some(p) = frame(fx, fy, 2.5, true, false) {
        return p;
    }
    // Two battens across the boards: wood for a wooden hatch, iron bands
    // on the others, nailed where they cross each board.
    for (i, bx) in [8.0f32, 24.0].into_iter().enumerate() {
        let half = if m == Mat::Wood { 2.0 } else { 1.5 };
        let d = half - (fx - bx).abs();
        if d > 0.0 {
            let n = m.boards().max(2);
            let (k, _, _) = board(fy, 2.5, 29.5, n);
            let ny = 2.5 + 27.0 / n as f32 * (k as f32 + 0.5);
            if let Some(s) = stud(fx, fy, bx, ny, 0.8) {
                return fit(1.25 + s * 0.3);
            }
            if m == Mat::Wood {
                return body(0.85 + 0.25 * rise(d, 0.9), 30 + i as u32, false);
            }
            return fit(1.0 + 0.15 * rise(d, 0.6));
        }
    }
    boards(m, fx, fy, 2.5, 29.5, false)
}

fn stylish_trapdoor(m: Mat, fx: f32, fy: f32) -> Px {
    if let Some(p) = lift_ring(fx, fy, 16.0, 25.0, 2.4) {
        return p;
    }
    for (sx, sy) in [(1.8, 1.8), (30.2, 1.8), (1.8, 30.2), (30.2, 30.2)] {
        if let Some(s) = stud(fx, fy, sx, sy, 1.1) {
            return fit(1.2 + s * 0.4);
        }
    }
    if let Some(p) = frame(fx, fy, 3.0, true, false) {
        return p;
    }
    // A boss where the straps cross.
    if let Some(s) = stud(fx, fy, 16.0, 16.0, 2.4) {
        return fit(1.35 + s * 0.45);
    }
    // Iron straps corner to corner, studded along their length.
    let p = (fx, fy);
    for (a, b) in [((3.0, 3.0), (29.0, 29.0)), ((29.0, 3.0), (3.0, 29.0))] {
        let d = seg_dist(a, b, p);
        if d < 1.6 {
            for t in [0.12, 0.3, 0.7, 0.88] {
                let (sx, sy) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                if let Some(s) = stud(fx, fy, sx, sy, 0.9) {
                    return fit(1.35 + s * 0.35);
                }
            }
            return fit(1.05 + 0.15 * rise(1.6 - d, 0.6));
        }
    }
    let mut px = boards(m, fx, fy, 3.0, 29.0, false);
    // A sunk band inside the frame, so the boards sit in a rebate.
    if inside(p, 3.0, 3.0, 29.0, 29.0) < 0.8 {
        px = gap(0.25);
    }
    px
}

fn ornate_trapdoor(m: Mat, fx: f32, fy: f32) -> Px {
    let p = (fx, fy);
    let (dx, dy) = (fx - 16.0, fy - 16.0);
    let r = dx.hypot(dy);
    // The ring pull in the medallion's heart.
    if let Some(s) = ring(fx, fy, 16.0, 16.0, 2.3, 0.75) {
        return fit(1.55 + s * 0.4);
    }
    if let Some(s) = stud(fx, fy, 16.0, 16.0, 1.0) {
        return fit(1.4 + s * 0.3);
    }
    // Corner brackets: a quarter-round plate with a stud.
    for (cx, cy) in [(0.0, 0.0), (SIZEF, 0.0), (0.0, SIZEF), (SIZEF, SIZEF)] {
        let d = (fx - cx).hypot(fy - cy);
        if d < 7.5 {
            let (sx, sy) = (cx + (16.0 - cx).signum() * 3.0, cy + (16.0 - cy).signum() * 3.0);
            if let Some(s) = stud(fx, fy, sx, sy, 1.1) {
                return fit(1.45 + s * 0.4);
            }
            if d > 6.2 {
                return fit(1.1);
            }
            return fit(1.2 + 0.1 * rise(7.5 - d, 1.0));
        }
    }
    // Studs along the frame between the brackets.
    for t in [11.0, 16.0, 21.0] {
        for (sx, sy) in [(t, 2.0), (t, SIZEF - 2.0), (2.0, t), (SIZEF - 2.0, t)] {
            if let Some(s) = stud(fx, fy, sx, sy, 0.95) {
                return fit(1.25 + s * 0.4);
            }
        }
    }
    if let Some(px) = frame(fx, fy, 4.0, true, true) {
        return px;
    }
    // The medallion: a moulded ring round an eight-pointed star.
    if r < 9.5 {
        if r > 8.0 {
            return toned(1.0 + ridge(r, 8.0, 9.5) * 0.25, 7, false, 0.06);
        }
        let ang = dy.atan2(dx);
        let spike = ((ang / TAU * 8.0).rem_euclid(1.0) - 0.5).abs() * 2.0;
        let star = 3.4 + (1.0 - spike) * 3.9;
        let h = 0.45;
        if r < star && r > 3.2 {
            return ornament(m, h);
        }
        return toned(h, 25, false, -0.06);
    }
    // Four panels round it, boards running across, sunk at their edges.
    let d = inside(p, 4.0, 4.0, 28.0, 28.0);
    let mid = (fx - 16.0).abs().min((fy - 16.0).abs());
    if mid < 1.0 || d < 0.7 {
        return gap(0.25);
    }
    // Diagonal fillets of ornament reaching for the corners.
    let diag = (dx.abs() - dy.abs()).abs();
    if diag < 0.9 && (10.5..15.5).contains(&r) {
        return ornament(m, 0.6);
    }
    let mut px = boards(m, fx, fy, 4.0, 28.0, false);
    px.h += 0.1 * rise(mid - 1.0, 1.0).min(rise(d - 0.7, 1.0));
    px
}

// ---------------------------------------------------------------- render

/// Paints a design through its material.
fn render(name: &str, m: Mat, design: impl Fn(Mat, f32, f32) -> Px) -> Tex {
    let salt = seed(name);
    let cells: Vec<Px> = (0..SIZE * SIZE).map(|i| design(m, c(i % SIZE), c(i / SIZE))).collect();
    let height = Field::from_fn(|x, y| cells[y * SIZE + x].h);
    let light = height.light();
    let ramp = m.body();
    let fitr = m.fit();
    let inlay = gold_alloy_ramp();
    let stone = (m == Mat::Stone).then(|| Rock::Stone.grain(salt));
    let base = m.base();
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let px = cells[y * SIZE + x];
        let l = light.at(x, y);
        match px.part {
            Part::Gap => ramp.pick(0.06 + px.h * 0.3 + l * 0.2, x, y),
            Part::Fit => fitr.pick(0.42 + l * 0.75 + (px.h - 1.2) * 0.25, x, y),
            Part::Inlay => inlay.pick(0.62 + l * 0.7, x, y),
            Part::Body { piece, across, tone } => {
                let s = hash2(salt, piece);
                let off = (unit(s) - 0.5) * 0.12;
                let grain = match m {
                    Mat::Wood => Wood::Timber.grain(s, fx, fy, across) * 0.75,
                    Mat::Stone => stone.as_ref().map_or(0.0, |g| g.at(x, y)) * 0.55,
                    Mat::Metal => {
                        let b = if across {
                            value(s, 2, 32, fx, fy)
                        } else {
                            value(s, 32, 2, fx, fy)
                        };
                        // Two broad glints across the plate, as refined metal has.
                        let d = (fx + fy).rem_euclid(SIZEF);
                        let glint =
                            (-((d - 12.0) / 2.6).powi(2)).exp() * 0.22 + (-((d - 20.0) / 1.4).powi(2)).exp() * 0.12;
                        (b - 0.5) * 0.12 + (0.5 - fy / SIZEF) * 0.1 + glint
                    }
                    Mat::Crystal => {
                        let g = if across {
                            value(s, 1, 8, fx, fy)
                        } else {
                            value(s, 8, 1, fx, fy)
                        };
                        let glow = (1.0 - (fx - 16.0).hypot(fy - 16.0) / 18.0).max(0.0) * 0.18;
                        (g - 0.5) * 0.18 + glow
                    }
                };
                ramp.pick(base + tone + off + grain + l * 0.65 + (px.h - 0.75) * 0.15, x, y)
            }
        }
    });
    match m {
        Mat::Stone => {
            // The rock's flecks, on the stone only.
            let mut flecked = tex.clone();
            Rock::Stone.flecks(&mut flecked, salt ^ 9);
            for (i, px) in cells.iter().enumerate() {
                if matches!(px.part, Part::Body { .. }) {
                    tex.px[i] = flecked.px[i];
                }
            }
        }
        Mat::Crystal => {
            let at: Vec<(i32, i32)> = (0..3)
                .map(|k| {
                    let h = hash2(salt, 100 + k);
                    (6 + (unit(h) * 20.0) as i32, 6 + (unit(hash2(h, 1)) * 20.0) as i32)
                })
                .filter(|&(x, y)| matches!(cells[y as usize * SIZE + x as usize].part, Part::Body { .. }))
                .collect();
            sparkles(&mut tex, &at);
        }
        _ => {}
    }
    tex
}

// ------------------------------------------------------------------ lock

/// The lock: a brass padlock hung on a riveted dark iron plate.
fn lock() -> Tex {
    let design = |fx: f32, fy: f32| -> (f32, u8) {
        // 0 plate, 1 shackle, 2 lock body, 3 keyhole, 4 rivet.
        let (dx, dy) = (fx - 16.0, fy - 12.5);
        // The shackle: the top of a ring, its legs running down into the body.
        let rr = dx.hypot(dy);
        let band = (5.2 - rr).abs();
        if (band < 1.4 && fy < 12.5) || ((12.5..16.0).contains(&fy) && (dx.abs() - 5.2).abs() < 1.4) {
            return (1.3 + (1.0 - band.min((dx.abs() - 5.2).abs()) / 1.4) * 0.4, 1);
        }
        let b = inside((fx, fy), 8.0, 15.0, 24.0, 28.0);
        if b > 0.0 {
            // A rounded bottom.
            let corner = (fx - 16.0).abs() > 5.0 && fy > 25.0;
            let cr = ((fx - 16.0).abs() - 5.0).hypot(fy - 25.0);
            if !corner || cr < 3.0 {
                let key = (fx - 16.0).hypot(fy - 20.0) < 1.6
                    || ((fx - 16.0).abs() < 0.6 + (fy - 20.0) * 0.18 && (20.0..24.5).contains(&fy));
                if key {
                    return (1.0, 3);
                }
                if let Some(s) = [(10.0, 17.0), (22.0, 17.0)]
                    .into_iter()
                    .find_map(|(sx, sy)| stud(fx, fy, sx, sy, 0.9))
                {
                    return (1.7 + s * 0.3, 2);
                }
                let e = if corner { 3.0 - cr } else { b };
                return (1.2 + 0.4 * rise(e, 1.4), 2);
            }
        }
        for (sx, sy) in [
            (3.0, 3.0),
            (29.0, 3.0),
            (3.0, 29.0),
            (29.0, 29.0),
            (16.0, 3.0),
            (3.0, 16.0),
            (29.0, 16.0),
            (16.0, 29.0),
        ] {
            if let Some(s) = stud(fx, fy, sx, sy, 1.2) {
                return (0.9 + s * 0.4, 4);
            }
        }
        let d = face_inset((fx, fy));
        (0.3 + 0.5 * rise(d, 1.5) - ridge(d, 4.5, 5.7) * 0.25, 0)
    };
    let cells: Vec<(f32, u8)> = (0..SIZE * SIZE).map(|i| design(c(i % SIZE), c(i / SIZE))).collect();
    let height = Field::from_fn(|x, y| cells[y * SIZE + x].0);
    let light = height.light();
    let plate = dark_iron_ramp();
    let steel = iron_ramp();
    let brass = gold_alloy_ramp();
    let salt = seed("lock");
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let (h, part) = cells[y * SIZE + x];
        let l = light.at(x, y);
        match part {
            1 => steel.pick(0.45 + l * 0.8 + (h - 1.4) * 0.3, x, y),
            2 => brass.pick(0.5 + l * 0.7 + (0.5 - (fx + fy) / 64.0) * 0.2 + (h - 1.4) * 0.2, x, y),
            3 => plate.pick(0.02, x, y),
            4 => plate.pick(0.55 + l * 0.8, x, y),
            _ => {
                let b = (value(salt, 2, 32, fx, fy) - 0.5) * 0.08;
                plate.pick(0.4 + b + l * 0.6 + (h - 0.8) * 0.15, x, y)
            }
        }
    })
}

// ------------------------------------------------------------------ pipe

/// The pipe: a short length of steel pipe lying from the bottom left to
/// the top right, a collar at each end, its open bore facing up the right.
fn pipe() -> Tex {
    let ramp = steel_ramp();
    let (a, b) = ((7.5f32, 24.5f32), (23.0f32, 9.0f32));
    let len = (b.0 - a.0).hypot(b.1 - a.1);
    let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
    // Across the pipe, towards the bottom right.
    let (nx, ny) = (-uy, ux);
    let r = 4.6;
    let mut icon = Icon::new();
    // The body and its collars, shaded round the cylinder.
    icon.layer(false, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let (px, py) = (fx - a.0, fy - a.1);
        let t = px * ux + py * uy;
        let s = px * nx + py * ny;
        let collar = (-0.5..2.2).contains(&t) || (len - 2.6..len).contains(&t);
        let rad = if collar { r + 0.9 } else { r };
        if !(-0.5..=len).contains(&t) || s.abs() > rad {
            return None;
        }
        let k = s / rad;
        let mut tone = 0.58 - k * 0.3 + (-((k + 0.45) / 0.18).powi(2)).exp() * 0.32;
        if collar {
            tone += 0.06;
            // A dark line where each collar meets the pipe.
            if (t - 2.2).abs() < 0.5 || (t - (len - 2.6)).abs() < 0.5 {
                tone -= 0.25;
            }
        }
        Some(ramp.pick(tone, x, y))
    });
    // The open end: a rim round a dark bore, seen at a slant.
    icon.layer(false, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let (px, py) = (fx - b.0, fy - b.1);
        let t = px * ux + py * uy;
        let s = px * nx + py * ny;
        let rr = (s / (r + 0.9)).powi(2) + (t / 2.4).powi(2);
        if rr > 1.0 {
            return None;
        }
        let bore = (s / (r - 0.6)).powi(2) + (t / 1.6).powi(2);
        if bore < 1.0 {
            // Deeper towards the far side.
            return Some(ramp.pick(0.06 + (s / r).max(0.0) * 0.12, x, y));
        }
        Some(ramp.pick(0.82 - s / r * 0.2, x, y))
    });
    icon.finish()
}

// ---------------------------------------------------------------- lookups

/// Every block texture this module paints, in catalogue order.
pub fn block_names() -> Vec<String> {
    let mut names = Vec::new();
    for kind in ["door", "trapdoor"] {
        for g in Grade::ALL {
            for m in Mat::ALL {
                names.push(format!("{}_{}_{kind}", g.name(), m.name()));
            }
        }
    }
    names.push("lock".to_string());
    names
}

/// The item icons this module paints.
pub const ICONS: [&str; 1] = ["pipe"];

/// Paints a door, trapdoor or the lock, or `None`.
pub fn paint(name: &str) -> Option<Tex> {
    if name == "lock" {
        return Some(lock());
    }
    let (rest, trap) = if let Some(r) = name.strip_suffix("_trapdoor") {
        (r, true)
    } else {
        (name.strip_suffix("_door")?, false)
    };
    let (g, m) = rest.split_once('_')?;
    let g = Grade::ALL.into_iter().find(|k| k.name() == g)?;
    let m = Mat::ALL.into_iter().find(|k| k.name() == m)?;
    let design: fn(Mat, f32, f32) -> Px = match (trap, g) {
        (false, Grade::Plain) => plain_door,
        (false, Grade::Stylish) => stylish_door,
        (false, Grade::Ornate) => ornate_door,
        (true, Grade::Plain) => plain_trapdoor,
        (true, Grade::Stylish) => stylish_trapdoor,
        (true, Grade::Ornate) => ornate_trapdoor,
    };
    Some(render(name, m, design))
}

/// Paints one of this module's icons, or `None`.
pub fn paint_icon(name: &str) -> Option<Tex> {
    (name == "pipe").then(pipe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    fn luma(p: Rgba) -> f32 {
        0.2126 * p.r + 0.7152 * p.g + 0.0722 * p.b
    }

    /// Mean difference in light and shade between two faces, each about
    /// its own average (what survives the game's recolouring).
    fn pattern_difference(a: &Tex, b: &Tex) -> f32 {
        let ma = a.px.iter().map(|p| luma(*p)).sum::<f32>() / a.px.len() as f32;
        let mb = b.px.iter().map(|p| luma(*p)).sum::<f32>() / b.px.len() as f32;
        a.px.iter()
            .zip(&b.px)
            .map(|(p, q)| ((luma(*p) - ma) - (luma(*q) - mb)).abs())
            .sum::<f32>()
            / a.px.len() as f32
    }

    /// Every door, trapdoor, the lock and the pipe are painted through the
    /// crate's lookups, 32 x 32, opaque, and no two alike.
    #[test]
    fn every_door_texture_and_icon_is_painted_and_distinct() {
        let names = block_names();
        assert_eq!(names.len(), 25);
        let texs: Vec<Tex> = names
            .iter()
            .map(|n| crate::materials::paint_named(n, 0).unwrap_or_else(|| panic!("no painter for `{n}`")))
            .collect();
        for (n, t) in names.iter().zip(&texs) {
            assert_eq!(t.n, 32, "{n}");
            assert!(t.px.iter().all(|p| p.a >= 1.0), "{n} is not opaque");
        }
        for i in 0..texs.len() {
            for j in i + 1..texs.len() {
                assert_ne!(texs[i], texs[j], "`{}` and `{}` look the same", names[i], names[j]);
            }
        }
        for n in ICONS {
            let icon = crate::icons::paint_icon(n).unwrap_or_else(|| panic!("no icon for `{n}`"));
            assert_eq!(icon, paint_icon(n).unwrap());
            assert!(icon.px.iter().filter(|p| p.a > 0.0).count() > 150, "{n} is too small");
        }
    }

    /// Within a material, the six designs differ in shape, not just shade.
    #[test]
    fn designs_differ_in_shape() {
        for m in Mat::ALL {
            let names: Vec<String> = ["door", "trapdoor"]
                .iter()
                .flat_map(|k| Grade::ALL.map(|g| format!("{}_{}_{k}", g.name(), m.name())))
                .collect();
            let texs: Vec<Tex> = names.iter().map(|n| paint(n).unwrap()).collect();
            for i in 0..texs.len() {
                for j in i + 1..texs.len() {
                    let d = pattern_difference(&texs[i], &texs[j]);
                    assert!(d > 0.03, "{} vs {}: {d}", names[i], names[j]);
                }
            }
        }
    }
}
