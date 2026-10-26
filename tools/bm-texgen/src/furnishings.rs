//! Furnishings (milestone 22 part 3, `docs/milestones/22-building-blocks.md`):
//! poles and beams, glass panes, the plinth, table and chair, and storage
//! in five materials and three grades.
//!
//! Poles and beams are drawn as thin posts (a pole the middle 6/16 of its
//! block, a beam the middle 8/16) with the texture sampled by position, so
//! what shows on a long side is the centre strip of the face, running up
//! and down: every post is painted as that strip, its grain, flutes and
//! bands running along `v`, bevelled at the strip's edges. The faces
//! beyond the strip barely show and carry on the same material. Plain is
//! the bare material; stylish adds turned rings, carved bands or iron
//! collars; ornate a barley twist, a fluted shaft with a beaded capital,
//! or gold collars and a gold spiral inlay.
//!
//! Panes are thin sheets, so the whole face shows, translucent as the glass
//! blocks are. The plinth, table and chair are small boxes, so their faces
//! are an even, fine material with no motif. Storage blocks are whole
//! cubes: a chest front on the sides (a lid over a dark lid line, a hasp,
//! growing from plain boards to iron bands and studs to carved or gilded
//! panels), its lid on top and bottom.
//!
//! Materials as the doors have them (`crate::doors`): wood and stone are
//! colourable and painted through timber's and stone's ramps and grain;
//! metal is iron with darker fittings and gold inlay; crystal is the glow
//! crystal's cyan with silver fittings and gold inlay; wicker is woven
//! straw with leather fittings.

use std::f32::consts::{PI, TAU};

use crate::building::{Rock, Wood, face_inset, ridge, rise, sparkles};
use crate::color::{Ramp, Rgba};
use crate::doors::{Grade, board, inside, ring, stud};
use crate::materials::crystal_ramp;
use crate::noise::{hash2, seed, unit, value};
use crate::spark::dark_iron_ramp;
use crate::stations::iron_ramp;
use crate::tex::{Field, SIZE, SIZEF, Tex, c};
use crate::worlds::gold_alloy_ramp;

// ---------------------------------------------------------------- kinds

/// What a furnishing is made of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mat {
    Wood,
    Stone,
    Metal,
    Crystal,
    Wicker,
}

impl Mat {
    /// The materials posts come in.
    pub const POSTS: [Mat; 4] = [Mat::Wood, Mat::Stone, Mat::Metal, Mat::Crystal];
    /// The materials storage comes in.
    pub const STORAGE: [Mat; 5] = [Mat::Wood, Mat::Stone, Mat::Metal, Mat::Crystal, Mat::Wicker];

    pub fn name(self) -> &'static str {
        match self {
            Mat::Wood => "wood",
            Mat::Stone => "stone",
            Mat::Metal => "metal",
            Mat::Crystal => "crystal",
            Mat::Wicker => "wicker",
        }
    }

    fn body(self) -> Ramp {
        match self {
            Mat::Wood => Wood::Timber.ramp(),
            Mat::Stone => Rock::Stone.ramp(),
            Mat::Metal => iron_ramp(),
            Mat::Crystal => crystal_ramp(),
            Mat::Wicker => straw_ramp(),
        }
    }

    /// Bands, studs, hasps.
    fn fit(self) -> Ramp {
        match self {
            Mat::Wood | Mat::Stone | Mat::Metal => dark_iron_ramp(),
            Mat::Crystal => iron_ramp(),
            Mat::Wicker => leather_ramp(),
        }
    }

    fn base(self) -> f32 {
        match self {
            Mat::Wood => 0.56,
            Mat::Stone => 0.58,
            Mat::Metal => 0.46,
            Mat::Crystal => 0.5,
            Mat::Wicker => 0.6,
        }
    }

    /// Boards across a chest's body.
    fn boards(self) -> u32 {
        match self {
            Mat::Wood => 3,
            Mat::Stone => 2,
            Mat::Metal | Mat::Wicker => 1,
            Mat::Crystal => 3,
        }
    }

    /// Whether ornament is inlaid in gold (else carved or woven in).
    fn inlaid(self) -> bool {
        matches!(self, Mat::Metal | Mat::Crystal)
    }
}

/// Woven straw: warm, golden, a little green in the shade.
fn straw_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x3a2616, 0x5c3c1c, 0x825826, 0xa6762f, 0xc4933c, 0xdab050, 0xecca72, 0xf8e4a4,
    ])
}

/// Tanned leather for the basket's straps and hasp.
fn leather_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x1e0e10, 0x341a14, 0x4e2a1a, 0x6a3c22, 0x86522c, 0xa26a38, 0xbc864c, 0xd6a668,
    ])
}

// ---------------------------------------------------------------- parts

#[derive(Clone, Copy, PartialEq, Debug)]
enum Part {
    Gap,
    Body { piece: u32, across: bool, tone: f32 },
    Fit,
    Inlay,
}

#[derive(Clone, Copy, Debug)]
struct Px {
    h: f32,
    part: Part,
}

fn body(h: f32, piece: u32, across: bool) -> Px {
    toned(h, piece, across, 0.0)
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
/// wood and stone, dyed strands woven into wicker.
fn ornament(m: Mat, h: f32, across: bool) -> Px {
    match m {
        Mat::Metal | Mat::Crystal => Px {
            h: h + 0.05,
            part: Part::Inlay,
        },
        Mat::Wicker => toned(h, 40, across, -0.26),
        _ => toned(h + 0.6, 40, across, 0.14),
    }
}

/// Basket weave in 4-unit squares, the strands going over and under in a
/// checker: how high the strand is here (0 at its edges, 1 on its crown)
/// and whether it runs across.
fn weave(fx: f32, fy: f32) -> (f32, bool) {
    let (i, j) = ((fx / 4.0).floor() as i32, (fy / 4.0).floor() as i32);
    if (i + j).rem_euclid(2) == 0 {
        ((PI * fy.rem_euclid(4.0) / 4.0).sin(), true)
    } else {
        ((PI * fx.rem_euclid(4.0) / 4.0).sin(), false)
    }
}

// ---------------------------------------------------------------- render

/// Paints a design through its material. `posts` lays metal's sheen and
/// crystal's glow down the middle of the face, as on a post.
fn render(name: &str, m: Mat, posts: bool, design: impl Fn(f32, f32) -> Px) -> Tex {
    let salt = seed(name);
    let cells: Vec<Px> = (0..SIZE * SIZE).map(|i| design(c(i % SIZE), c(i / SIZE))).collect();
    let woven = |i: usize| m == Mat::Wicker && matches!(cells[i].part, Part::Body { .. });
    let height = Field::from_fn(|x, y| {
        let i = y * SIZE + x;
        cells[i].h + if woven(i) { weave(c(x), c(y)).0 * 0.5 } else { 0.0 }
    });
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
                let dx = fx - 16.0;
                let grain = match m {
                    // Timber's `across` names the streaks' run, not the board's.
                    Mat::Wood => Wood::Timber.grain(s, fx, fy, !across) * 0.75,
                    Mat::Stone => stone.as_ref().map_or(0.0, |g| g.at(x, y)) * 0.55,
                    Mat::Metal => {
                        let b = if across {
                            value(s, 2, 32, fx, fy)
                        } else {
                            value(s, 32, 2, fx, fy)
                        };
                        let glint = if posts {
                            // A bright line down the post, left of centre.
                            (-((dx + 2.5) / 1.5).powi(2)).exp() * 0.22 - (dx / 10.0).max(0.0) * 0.12
                        } else {
                            let d = (fx + fy).rem_euclid(SIZEF);
                            (-((d - 12.0) / 2.6).powi(2)).exp() * 0.22
                                + (-((d - 20.0) / 1.4).powi(2)).exp() * 0.12
                                + (0.5 - fy / SIZEF) * 0.1
                        };
                        (b - 0.5) * 0.12 + glint
                    }
                    Mat::Crystal => {
                        let g = if across {
                            value(s, 1, 8, fx, fy)
                        } else {
                            value(s, 8, 1, fx, fy)
                        };
                        let glow = if posts {
                            (1.0 - dx.abs() / 9.0).max(0.0) * 0.16
                        } else {
                            (1.0 - dx.hypot(fy - 16.0) / 18.0).max(0.0) * 0.18
                        };
                        (g - 0.5) * 0.18 + glow
                    }
                    Mat::Wicker => {
                        let (w, along) = weave(fx, fy);
                        let cell = ((fx / 4.0).floor() as u32) * 17 + (fy / 4.0).floor() as u32;
                        let k = hash2(s, cell);
                        let streak = if along {
                            value(k, 2, 32, fx, fy)
                        } else {
                            value(k, 32, 2, fx, fy)
                        };
                        (w - 0.6) * 0.32 + (streak - 0.5) * 0.14 + (unit(k) - 0.5) * 0.08
                    }
                };
                ramp.pick(base + tone + off + grain + l * 0.65 + (px.h - 0.75) * 0.15, x, y)
            }
        }
    });
    match m {
        Mat::Stone => {
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
                    let x = if posts {
                        11 + (unit(h) * 10.0) as i32
                    } else {
                        6 + (unit(h) * 20.0) as i32
                    };
                    (x, 4 + (unit(hash2(h, 1)) * 24.0) as i32)
                })
                .filter(|&(x, y)| matches!(cells[y as usize * SIZE + x as usize].part, Part::Body { .. }))
                .collect();
            sparkles(&mut tex, &at);
        }
        _ => {}
    }
    tex
}

// ---------------------------------------------------------------- posts

/// A post's surface: the strip `half` either side of the middle, bevelled
/// at its edges (mirrored beyond them, so the face outside carries on),
/// a pole a little rounded, a beam square.
fn post(m: Mat, g: Grade, beam: bool, fx: f32, fy: f32) -> Px {
    let half = if beam { 8.0 } else { 6.0 };
    let dx = fx - 16.0;
    let e = (half - dx.abs()).abs();
    // Across the strip, -1 at its left edge, 1 at its right.
    let t = (dx / half).clamp(-1.0, 1.0);
    let mut h = 0.55 + 0.35 * rise(e, if beam { 1.0 } else { 1.5 });
    if !beam {
        h += 0.15 * (1.0 - t * t);
    }
    match g {
        Grade::Plain => plain_post(m, beam, dx, half, h, fy),
        Grade::Stylish => stylish_post(m, beam, dx, half, h, fy),
        Grade::Ornate => ornate_post(m, beam, dx, half, h, fy),
    }
}

/// A raised round bead across a post at `yc`, `w` half wide (none off it).
fn bead(fy: f32, yc: f32, w: f32) -> Option<f32> {
    let d = (fy - yc).abs() / w;
    (d < 1.0).then(|| (1.0 - d * d).sqrt())
}

fn plain_post(m: Mat, beam: bool, dx: f32, half: f32, h: f32, fy: f32) -> Px {
    match m {
        // A knot in the timber, low on the post.
        Mat::Wood => {
            let r = ((dx + 1.5) * 1.4).hypot((fy - 22.0) * 0.7);
            if r < 1.0 {
                return toned(h - 0.1, 0, false, -0.25);
            }
            if r < 2.6 && ((r * 2.0) as i32) % 2 == 0 {
                return toned(h, 0, false, -0.1);
            }
            body(h, 0, false)
        }
        // Dressed stone in two lengths, a joint across the post.
        Mat::Stone => {
            let jy = if beam { 20.0 } else { 12.0 };
            if (fy - jy).abs() < 0.6 {
                return gap(0.3);
            }
            body(h - 0.12 * ridge((fy - jy).abs(), 0.6, 2.0), u32::from(fy > jy), false)
        }
        Mat::Metal => body(h, 0, false),
        // A prism: a crisp ridge down the middle, facets either side.
        Mat::Crystal => {
            let f = 1.0 - (dx.abs() / half).min(1.0);
            toned(h + f * 0.35, 0, false, if dx < 0.0 { 0.06 } else { -0.04 })
        }
        Mat::Wicker => body(h, 0, false),
    }
}

fn stylish_post(m: Mat, beam: bool, dx: f32, half: f32, h: f32, fy: f32) -> Px {
    // Ring pairs near both ends, so stacked posts meet between them.
    let rings: &[f32] = if beam { &[3.0, 29.0] } else { &[3.0, 6.5, 25.5, 29.0] };
    match m {
        Mat::Wood | Mat::Stone => {
            // Turned beads, a cove either side, on a plain shaft.
            for &yc in rings {
                if let Some(b) = bead(fy, yc, 1.3) {
                    return toned(h + 0.3 * b, 3, true, 0.08);
                }
                let d = (fy - yc).abs();
                if d < 2.0 {
                    return gap(h - 0.2);
                }
            }
            // A sunk band round the middle, carved with a line of beads.
            if (14.0..18.0).contains(&fy) && (m == Mat::Stone || !beam) {
                let pitch = 3.0;
                let k = ((dx + 0.5 * pitch) / pitch).floor();
                let cx = k * pitch;
                if let Some(s) = stud(dx, fy, cx, 16.0, 1.15) {
                    return toned(h - 0.1 + s * 0.3, 4, false, 0.05);
                }
                return toned(h - 0.3, 4, false, -0.08);
            }
            body(h, 0, false)
        }
        Mat::Metal | Mat::Crystal => {
            // Collars of darker metal (silver on crystal), riveted.
            for (y0, y1) in [(1.5, 6.0), (26.0, 30.5)] {
                if (y0..y1).contains(&fy) {
                    if let Some(s) = stud(dx, fy, 0.0, (y0 + y1) * 0.5, 1.0) {
                        return fit(1.5 + s * 0.4);
                    }
                    let d = (fy - y0).min(y1 - fy);
                    return fit(1.1 + 0.25 * rise(d, 0.8) + (h - 0.7) * 0.6);
                }
            }
            if m == Mat::Crystal {
                // Facets: two ridges down the shaft.
                let f = ((dx.abs() - half * 0.45).abs() / (half * 0.45)).min(1.0);
                return toned(h + (1.0 - f) * 0.25, 0, false, 0.0);
            }
            // Grooves down the shaft.
            if (dx.abs() - half * 0.5).abs() < 0.6 {
                return gap(h - 0.1);
            }
            body(h, 0, false)
        }
        Mat::Wicker => body(h, 0, false),
    }
}

fn ornate_post(m: Mat, beam: bool, dx: f32, half: f32, h: f32, fy: f32) -> Px {
    // Collars at both ends: moulded, carved or gilded.
    for (y0, y1) in [(0.0, 4.5), (27.5, 32.0)] {
        if (y0..y1).contains(&fy) {
            let d = (fy - y0).min(y1 - fy);
            let hh = h + 0.3 + 0.2 * rise(d, 1.0);
            if m.inlaid() {
                // A gold band with a dark line through it.
                if (fy - (y0 + y1) * 0.5).abs() < 0.5 {
                    return fit(1.2);
                }
                return ornament(m, hh, true);
            }
            // Beads along the collar.
            let pitch = 2.5;
            let k = ((dx + 0.5 * pitch) / pitch).floor();
            if let Some(s) = stud(dx, fy, k * pitch, (y0 + y1) * 0.5, 1.05) {
                return ornament(m, hh - 0.5 + s * 0.25, false);
            }
            return toned(hh - 0.15, 5, true, -0.05);
        }
        if (fy - y0).abs() < 1.2 || (fy - y1).abs() < 1.2 {
            return gap(h - 0.25);
        }
    }
    match m {
        // A barley twist: grooves winding round the shaft.
        Mat::Wood => {
            let pitch = if beam { 7.0 } else { 6.0 };
            let ph = (fy + dx * 1.1).rem_euclid(pitch) / pitch;
            let groove = (ph * TAU).cos() * 0.5 + 0.5;
            toned(h + 0.4 * (1.0 - groove), 6, false, -0.12 * groove)
        }
        // Flutes down the shaft, each with a fillet between.
        Mat::Stone => {
            let n = if beam { 4.0 } else { 3.0 };
            let w = half * 2.0 / n;
            let u = (dx + half).rem_euclid(w) / w;
            let flute = (u * PI).sin();
            toned(h - 0.5 * flute, 7, false, -0.2 * flute + 0.04)
        }
        // A gold spiral inlaid down the shaft; crystal also gets a facet.
        Mat::Metal | Mat::Crystal | Mat::Wicker => {
            let ph = (fy + dx * 1.3).rem_euclid(8.0);
            if !(4.5..27.5).contains(&fy) {
                return body(h, 0, false);
            }
            if ph < 1.1 {
                return ornament(m, h + 0.1, false);
            }
            if m == Mat::Crystal {
                let f = 1.0 - (dx.abs() / half).min(1.0);
                return toned(h + f * 0.3, 0, false, if dx < 0.0 { 0.05 } else { -0.03 });
            }
            body(h, 0, false)
        }
    }
}

// ---------------------------------------------------------------- panes

/// An opaque frame pixel: lit on the top and left, shaded bottom and right.
fn frame_tone(fx: f32, fy: f32, w: f32) -> f32 {
    if fx.min(fy) < 1.0 {
        0.85
    } else if (SIZEF - fx).min(SIZEF - fy) < 1.0 {
        0.25
    } else if fx.min(fy) < w && fx < SIZEF - w && fy < SIZEF - w {
        0.6
    } else {
        0.45
    }
}

/// Plain pane: clear glass in a thin iron frame, with two
/// streaks of reflection.
fn plain_pane() -> Tex {
    let metal = iron_ramp();
    let pane = Rgba::hex(0xe4f6fc).with_alpha(0.06);
    let streak = Rgba::hex(0xffffff).with_alpha(0.34);
    let soft = Rgba::hex(0xffffff).with_alpha(0.18);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let d = face_inset((fx, fy));
        if d < 2.0 {
            return metal.pick(frame_tone(fx, fy, 2.0), x, y);
        }
        // A fine dark line where the glass meets the frame.
        if d < 2.6 {
            return metal.pick(0.2, x, y).with_alpha(0.55);
        }
        let d = fx + fy;
        if (11.0..13.0).contains(&d) || (16.5..17.5).contains(&d) {
            return streak;
        }
        if (38.0..40.0).contains(&d) {
            return soft;
        }
        pane
    })
}

/// Stylish pane: an iron frame and cross bar, angle brackets in the
/// corners and a riveted boss where the bars cross.
fn stylish_pane() -> Tex {
    let metal = iron_ramp();
    let pane = Rgba::hex(0xd0eaf4).with_alpha(0.1);
    let streak = Rgba::hex(0xffffff).with_alpha(0.45);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let p = (fx, fy);
        if face_inset(p) < 2.0 {
            return metal.pick(frame_tone(fx, fy, 2.0), x, y);
        }
        // The boss.
        let r = (fx - 16.0).hypot(fy - 16.0);
        if r < 2.6 {
            let lit = (16.0 - fx + 16.0 - fy) / 5.2;
            return metal.pick(0.55 + lit * 0.3, x, y);
        }
        let (bx, by) = ((fx - 16.0).abs(), (fy - 16.0).abs());
        if bx < 1.0 || by < 1.0 {
            let t = if (bx < 1.0 && fx < 16.0) || (by < 1.0 && fy < 16.0) {
                0.7
            } else {
                0.38
            };
            return metal.pick(t, x, y);
        }
        // Angle brackets: a triangle in each corner of the frame.
        for (cx, cy) in [(2.0, 2.0), (30.0, 2.0), (2.0, 30.0), (30.0, 30.0)] {
            if (fx - cx).abs() + (fy - cy).abs() < 4.5 {
                let rivet = (fx - cx).abs() < 1.6 && (fy - cy).abs() < 1.6 && x % 2 == 1 && y % 2 == 1;
                return metal.pick(if rivet { 0.85 } else { 0.5 }, x, y);
            }
        }
        // Each quarter's own streak.
        let (qx, qy) = (fx % 16.0, fy % 16.0);
        if ((qx + qy) - 10.0).abs() < 0.8 || ((qx + qy) - 13.0).abs() < 0.4 {
            return streak;
        }
        pane
    })
}

/// Ornate pane: a gilded frame round leaded glass, a ring of amber panes
/// in the middle with spokes to the frame and diagonals to the corners,
/// the outer panes in two pale tints, a gold boss in the heart.
fn ornate_pane() -> Tex {
    let lead = dark_iron_ramp();
    let gold = gold_alloy_ramp();
    let amber = Rgba::hex(0xf8dc90).with_alpha(0.36);
    let pale = Rgba::hex(0xdcf2f8).with_alpha(0.22);
    let rose = Rgba::hex(0xf2dce6).with_alpha(0.26);
    let streak = Rgba::hex(0xffffff).with_alpha(0.5);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let p = (fx, fy);
        let d = face_inset(p);
        if d < 2.0 {
            let t = frame_tone(fx, fy, 2.0);
            return gold.pick(0.25 + t * 0.6, x, y);
        }
        let (dx, dy) = (fx - 16.0, fy - 16.0);
        let r = dx.hypot(dy);
        if r < 1.8 {
            return gold.pick(0.75 - (dx + dy) * 0.08, x, y);
        }
        // The ring and the inner spokes.
        if (r - 7.5).abs() < 0.75 {
            return lead.pick(if dx + dy < 0.0 { 0.7 } else { 0.45 }, x, y);
        }
        if r < 7.5 {
            if dx.abs() < 0.6 || dy.abs() < 0.6 {
                return lead.pick(0.55, x, y);
            }
            if (dx + dy + 3.0).abs() < 0.6 && dx < 0.0 {
                return streak;
            }
            return amber;
        }
        // Outside: spokes on the axes, leads to the corners.
        if dx.abs() < 0.6 || dy.abs() < 0.6 || (dx.abs() - dy.abs()).abs() < 0.8 {
            return lead.pick(0.5, x, y);
        }
        // A lead line at a constant inset, parallel to the frame.
        if (d - 4.5).abs() < 0.5 {
            return lead.pick(0.45, x, y);
        }
        let side = (dx.abs() > dy.abs()) as i32;
        if d > 5.0 && ((fx + fy) - 15.0).abs() < 0.7 && fx < 16.0 {
            return streak;
        }
        if (side + i32::from(dx * dy > 0.0)) % 2 == 0 {
            pale
        } else {
            rose
        }
    })
}

// ------------------------------------------------------------ furniture

/// The plinth: dressed stone with fine tooling marks, a fillet and a
/// cove at the top and bottom.
fn stone_plinth() -> Tex {
    let design = |fx: f32, fy: f32| -> Px {
        let e = fy.min(SIZEF - fy);
        // A fillet then a cove at each end.
        if e < 2.0 {
            return body(0.9 + 0.1 * rise(e, 0.8), 1, true);
        }
        if e < 3.4 {
            return toned(0.5 + 0.3 * ridge(e, 2.0, 4.8), 2, true, -0.04);
        }
        // Tooling: faint drafted lines across.
        let tool = ((fy + (fx * 0.31).sin() * 0.4).rem_euclid(3.0) - 1.5).abs();
        toned(0.75 - 0.08 * (1.0 - tool).max(0.0), 0, true, 0.0)
    };
    render("stone_plinth", Mat::Stone, false, design)
}

/// The table: planed boards lying across, close-jointed, fine grain.
fn wood_table() -> Tex {
    let design = |_fx: f32, fy: f32| -> Px {
        let (k, a, b) = board(fy, 0.0, SIZEF, 4);
        if a < 0.8 {
            return gap(0.35);
        }
        body(0.6 + 0.2 * rise(a.min(b) - 0.8, 1.0), k, true)
    };
    render("wood_table", Mat::Wood, false, design)
}

/// The chair: narrower boards running up and down, a little darker, a
/// small bead along each.
fn wood_chair() -> Tex {
    let design = |fx: f32, _fy: f32| -> Px {
        let (k, a, b) = board(fx, 0.0, SIZEF, 5);
        if a < 0.7 {
            return gap(0.3);
        }
        let bead = ridge(a, 0.8, 2.2) * 0.15;
        toned(0.6 + 0.2 * rise(a.min(b) - 0.7, 1.0) + bead, 10 + k, false, -0.06)
    };
    render("wood_chair", Mat::Wood, false, design)
}

// -------------------------------------------------------------- storage

/// Where the lid line runs across a chest's front.
const LID: f32 = 10.0;

/// A chest's corner posts and rails, `w` wide, bevelled; wicker gets a
/// wrapped cane rim instead.
fn rim(m: Mat, fx: f32, fy: f32, w: f32, groove: bool) -> Option<Px> {
    let d = face_inset((fx, fy));
    if d >= w {
        return None;
    }
    if m == Mat::Wicker {
        // Cane wrapped round: ridges on a slant.
        let s = ((fx - fy).rem_euclid(2.5) / 2.5 * PI).sin();
        return Some(toned(
            0.75 + 0.3 * s + 0.2 * rise(d, 1.0),
            2,
            false,
            0.04 - 0.1 * (1.0 - s),
        ));
    }
    let stile = fx < w || fx > SIZEF - w;
    let mut h = 0.6 + 0.4 * rise(d, 1.2);
    if groove {
        h -= ridge(d, w * 0.5, w * 0.5 + 1.2) * 0.35;
    }
    let piece = match (stile, fx < 16.0, fy < 16.0) {
        (true, true, _) => 0,
        (true, false, _) => 1,
        (false, _, true) => 2,
        (false, _, false) => 3,
    };
    Some(body(h, piece, !stile))
}

/// Boards across `lo..hi`, bevelled, a dark joint between (wicker is one
/// weave).
fn chest_boards(m: Mat, fy: f32, lo: f32, hi: f32, n: u32, piece: u32) -> Px {
    if m == Mat::Wicker {
        return body(0.55, piece, true);
    }
    let (k, a, b) = board(fy, lo, hi, n);
    if k > 0 && a < 1.0 {
        return gap(0.1);
    }
    body(0.45 + 0.35 * rise(a.min(b) - 0.5, 1.2), piece + k, true)
}

/// The front's lid and body, whatever the grade: the lid's lip rounding
/// over a dark lid line, boards below.
fn front_field(m: Mat, fy: f32, top: f32, bottom: f32) -> Px {
    if fy < LID {
        let lip = rise(LID - fy, 1.8);
        let mut p = chest_boards(m, fy, top, LID, 1, 30);
        p.h += 0.25 * lip + 0.1;
        return p;
    }
    if fy < LID + 1.2 {
        return gap(0.0);
    }
    let mut p = chest_boards(m, fy, LID + 1.2, bottom, m.boards(), 10);
    // A shadow under the lip.
    p.h -= 0.25 * (1.0 - rise(fy - LID - 1.2, 2.0));
    p
}

/// A hasp hanging over the lid line, its staple below.
fn hasp(fx: f32, fy: f32, w: f32, top: f32, bottom: f32) -> Option<Px> {
    if let Some(s) = ring(fx, fy, 16.0, bottom - 0.6, 1.2, 0.55) {
        return Some(fit(1.5 + s * 0.3));
    }
    let d = inside((fx, fy), 16.0 - w, top, 16.0 + w, bottom - 1.3);
    if d <= 0.0 {
        return None;
    }
    if let Some(s) = stud(fx, fy, 16.0, top + 1.6, 0.8) {
        return Some(fit(1.4 + s * 0.3));
    }
    Some(fit(1.1 + 0.2 * rise(d, 0.6)))
}

fn plain_side(m: Mat, fx: f32, fy: f32) -> Px {
    if let Some(p) = hasp(fx, fy, 1.6, 7.0, 15.5) {
        return p;
    }
    if let Some(p) = rim(m, fx, fy, 2.5, false) {
        return p;
    }
    front_field(m, fy, 2.5, 29.5)
}

/// Iron bands up the front, studded; cut by the lid line.
fn band(fx: f32, fy: f32, x0: f32) -> Option<Px> {
    let d = inside((fx, fy), x0, -1.0, x0 + 3.0, SIZEF + 1.0);
    if d <= 0.0 {
        return None;
    }
    if (LID..LID + 1.2).contains(&fy) {
        return Some(gap(0.0));
    }
    for sy in [2.0, 6.5, 14.0, 20.0, 26.0, 30.0] {
        if let Some(s) = stud(fx, fy, x0 + 1.5, sy, 0.9) {
            return Some(fit(1.4 + s * 0.35));
        }
    }
    Some(fit(1.05 + 0.2 * rise(d, 0.7)))
}

fn stylish_side(m: Mat, fx: f32, fy: f32) -> Px {
    // A lock plate with a keyhole, over the lid line.
    let d = inside((fx, fy), 12.5, 7.0, 19.5, 17.5);
    if d > 0.0 {
        let key = (fx - 16.0).hypot(fy - 12.0) < 1.0 || ((fx - 16.0).abs() < 0.5 && (12.0..14.6).contains(&fy));
        if key {
            return gap(0.6);
        }
        for (sx, sy) in [(13.9, 8.4), (18.1, 8.4), (13.9, 16.1), (18.1, 16.1)] {
            if let Some(s) = stud(fx, fy, sx, sy, 0.7) {
                return fit(1.45 + s * 0.3);
            }
        }
        return fit(1.1 + 0.25 * rise(d, 0.8));
    }
    for x0 in [4.0, 25.0] {
        if let Some(p) = band(fx, fy, x0) {
            return p;
        }
    }
    // Corner caps.
    for (cx, cy) in [(0.0, 0.0), (SIZEF, 0.0), (0.0, SIZEF), (SIZEF, SIZEF)] {
        if (fx - cx).abs() < 3.5 && (fy - cy).abs() < 3.5 {
            return fit(1.15 + 0.15 * rise(3.5 - (fx - cx).abs().max((fy - cy).abs()), 0.6));
        }
    }
    if let Some(p) = rim(m, fx, fy, 3.0, false) {
        return p;
    }
    front_field(m, fy, 3.0, 29.0)
}

/// A moulding just outside a panel's edge.
fn moulding(d: f32) -> Option<Px> {
    (d > -1.4 && d <= 0.0).then(|| toned(1.0 + ridge(d, -1.4, 0.0) * 0.35, 7, true, 0.06))
}

/// A quatrefoil of ornament round `(cx, cy)`.
fn quatrefoil(m: Mat, fx: f32, fy: f32, cx: f32, cy: f32, h: f32) -> Option<Px> {
    let (dx, dy) = (fx - cx, fy - cy);
    if dx.hypot(dy) < 0.9 {
        return Some(ornament(m, h + 0.2, true));
    }
    for (px, py) in [(0.0, -1.9), (1.9, 0.0), (0.0, 1.9), (-1.9, 0.0)] {
        if (dx - px).hypot(dy - py) < 1.25 {
            return Some(ornament(m, h, true));
        }
    }
    None
}

fn ornate_side(m: Mat, fx: f32, fy: f32) -> Px {
    // An escutcheon: a round plate, a boss and a ring hanging from it.
    if let Some(s) = ring(fx, fy, 16.0, 15.0, 2.0, 0.7) {
        return fit(1.6 + s * 0.4);
    }
    if let Some(s) = stud(fx, fy, 16.0, 11.5, 1.1) {
        return fit(1.55 + s * 0.4);
    }
    let r = (fx - 16.0).hypot(fy - 11.5);
    if r < 3.4 {
        return ornament(m, 1.0 + ridge(r, 1.6, 3.4) * 0.3, true);
    }
    // Corner rosettes on the posts.
    for (cx, cy) in [(2.0, 2.0), (30.0, 2.0), (2.0, 30.0), (30.0, 30.0)] {
        if let Some(s) = stud(fx, fy, cx, cy, 1.5) {
            return ornament(m, 1.2 + s * 0.3, true);
        }
    }
    if let Some(p) = rim(m, fx, fy, 4.0, true) {
        return p;
    }
    if fy < LID {
        // The lid: a band of ornament along it, studs at its ends.
        let d = inside((fx, fy), 6.0, 5.0, 26.0, 7.6);
        if d > 0.0 {
            if m == Mat::Wicker {
                // A dyed zigzag woven in.
                let z = ((fx * 0.5).rem_euclid(2.0) - 1.0).abs() * 2.6 + 5.0;
                if (fy - z).abs() < 0.9 {
                    return ornament(m, 0.8, true);
                }
            } else if (fy - 6.3).abs() < 0.7
                || stud(fx, fy, 7.0, 6.3, 1.2).is_some()
                || stud(fx, fy, 25.0, 6.3, 1.2).is_some()
            {
                return ornament(m, 0.8, true);
            }
        }
        return front_field(m, fy, 4.0, 28.0);
    }
    // Two panels in the body, each with a quatrefoil.
    for (x0, x1) in [(6.0, 13.0), (19.0, 26.0)] {
        let d = inside((fx, fy), x0, 14.0, x1, 26.0);
        if let Some(p) = moulding(d) {
            return p;
        }
        if d > 0.0 {
            let h = 0.3 + 0.3 * rise(d - 0.4, 1.4);
            if let Some(p) = quatrefoil(m, fx, fy, (x0 + x1) * 0.5, 20.0, h + 0.15) {
                return p;
            }
            return toned(h, 22, false, -0.06);
        }
    }
    let mut p = front_field(m, fy, 4.0, 28.0);
    p.h += 0.15;
    p
}

fn plain_top(m: Mat, fx: f32, fy: f32) -> Px {
    // The hasp's tab at the front edge.
    let d = inside((fx, fy), 14.4, 27.5, 17.6, SIZEF + 1.0);
    if d > 0.0 {
        return fit(1.1 + 0.2 * rise(d, 0.6));
    }
    if let Some(p) = rim(m, fx, fy, 2.5, false) {
        return p;
    }
    let mut p = chest_boards(m, fy, 2.5, 29.5, m.boards().max(2) + 1, 10);
    if m == Mat::Wicker {
        p.h += 0.1;
    }
    p
}

fn stylish_top(m: Mat, fx: f32, fy: f32) -> Px {
    // The bands come up the front and run over the lid.
    for x0 in [4.0, 25.0] {
        let d = inside((fx, fy), x0, -1.0, x0 + 3.0, SIZEF + 1.0);
        if d > 0.0 {
            for sy in [3.0, 9.5, 16.0, 22.5, 29.0] {
                if let Some(s) = stud(fx, fy, x0 + 1.5, sy, 0.9) {
                    return fit(1.4 + s * 0.35);
                }
            }
            return fit(1.05 + 0.2 * rise(d, 0.7));
        }
    }
    for (cx, cy) in [(0.0, 0.0), (SIZEF, 0.0), (0.0, SIZEF), (SIZEF, SIZEF)] {
        if (fx - cx).abs() < 3.5 && (fy - cy).abs() < 3.5 {
            return fit(1.15 + 0.15 * rise(3.5 - (fx - cx).abs().max((fy - cy).abs()), 0.6));
        }
    }
    // A handle across the middle: a bar on two lugs.
    if inside((fx, fy), 11.0, 15.0, 21.0, 17.0) > 0.0 {
        return fit(1.6 - (fy - 15.6).abs() * 0.15);
    }
    if let Some(p) = rim(m, fx, fy, 3.0, false) {
        return p;
    }
    // A raised field between the bands.
    let d = inside((fx, fy), 9.0, 5.0, 23.0, 27.0);
    let mut p = chest_boards(m, fy, 3.0, 29.0, m.boards().max(2), 10);
    p.h += 0.15 * rise(d, 1.2);
    if d > -0.6 && d < 0.4 {
        return gap(0.3);
    }
    p
}

fn ornate_top(m: Mat, fx: f32, fy: f32) -> Px {
    let (dx, dy) = (fx - 16.0, fy - 16.0);
    let r = dx.hypot(dy);
    for (cx, cy) in [(2.2, 2.2), (29.8, 2.2), (2.2, 29.8), (29.8, 29.8)] {
        if let Some(s) = stud(fx, fy, cx, cy, 1.6) {
            return ornament(m, 1.2 + s * 0.3, true);
        }
    }
    if let Some(p) = rim(m, fx, fy, 4.0, true) {
        return p;
    }
    // A raised panel with a moulding round it.
    let d = inside((fx, fy), 6.5, 6.5, 25.5, 25.5);
    if let Some(p) = moulding(d) {
        return p;
    }
    if d > 0.0 {
        let h = 0.35 + 0.3 * rise(d - 0.4, 1.4);
        // The medallion: a ring, a boss, four leaves on the diagonals.
        if let Some(s) = stud(fx, fy, 16.0, 16.0, 1.7) {
            return ornament(m, h + s * 0.25, true);
        }
        if ring(fx, fy, 16.0, 16.0, 4.2, 0.75).is_some() {
            return ornament(m, h, true);
        }
        if (dx.abs() - dy.abs()).abs() < 0.9 * (8.0 - r).max(0.0) && r > 5.3 {
            return ornament(m, h, true);
        }
        if m == Mat::Wicker {
            return toned(h, 22, true, 0.02);
        }
        return toned(h, 22, (fy as i32 / 4) % 2 == 0, -0.04);
    }
    let mut p = chest_boards(m, fy, 4.0, 28.0, 1, 10);
    p.h += 0.1;
    p
}

// ---------------------------------------------------------------- lookups

/// Every block texture this module paints, in catalogue order.
pub fn block_names() -> Vec<String> {
    let mut names = Vec::new();
    for kind in ["pole", "beam"] {
        for g in Grade::ALL {
            for m in Mat::POSTS {
                names.push(format!("{}_{}_{kind}", g.name(), m.name()));
            }
        }
    }
    for g in Grade::ALL {
        names.push(format!("{}_glass_pane", g.name()));
    }
    for n in ["stone_plinth", "wood_table", "wood_chair"] {
        names.push(n.to_string());
    }
    for g in Grade::ALL {
        for m in Mat::STORAGE {
            for face in ["top", "side"] {
                names.push(format!("{}_{}_storage_{face}", g.name(), m.name()));
            }
        }
    }
    names
}

fn grade_mat(rest: &str, mats: &[Mat]) -> Option<(Grade, Mat)> {
    let (g, m) = rest.split_once('_')?;
    let g = Grade::ALL.into_iter().find(|k| k.name() == g)?;
    let m = mats.iter().copied().find(|k| k.name() == m)?;
    Some((g, m))
}

/// Paints a furnishing's texture, or `None`.
pub fn paint(name: &str) -> Option<Tex> {
    match name {
        "plain_glass_pane" => return Some(plain_pane()),
        "stylish_glass_pane" => return Some(stylish_pane()),
        "ornate_glass_pane" => return Some(ornate_pane()),
        "stone_plinth" => return Some(stone_plinth()),
        "wood_table" => return Some(wood_table()),
        "wood_chair" => return Some(wood_chair()),
        _ => {}
    }
    for (suffix, beam) in [("_pole", false), ("_beam", true)] {
        if let Some(rest) = name.strip_suffix(suffix) {
            let (g, m) = grade_mat(rest, &Mat::POSTS)?;
            return Some(render(name, m, true, move |fx, fy| post(m, g, beam, fx, fy)));
        }
    }
    for (suffix, top) in [("_storage_top", true), ("_storage_side", false)] {
        if let Some(rest) = name.strip_suffix(suffix) {
            let (g, m) = grade_mat(rest, &Mat::STORAGE)?;
            let design: fn(Mat, f32, f32) -> Px = match (top, g) {
                (false, Grade::Plain) => plain_side,
                (false, Grade::Stylish) => stylish_side,
                (false, Grade::Ornate) => ornate_side,
                (true, Grade::Plain) => plain_top,
                (true, Grade::Stylish) => stylish_top,
                (true, Grade::Ornate) => ornate_top,
            };
            return Some(render(name, m, false, move |fx, fy| design(m, fx, fy)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(p: Rgba) -> f32 {
        0.2126 * p.r + 0.7152 * p.g + 0.0722 * p.b
    }

    /// Mean difference in light and shade between two faces, each about
    /// its own average (what survives the game's recolouring), counting
    /// only the columns `x0..x1`.
    fn pattern_difference(a: &Tex, b: &Tex, x0: usize, x1: usize) -> f32 {
        let cols = |t: &Tex| -> Vec<f32> {
            (0..SIZE)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .map(|(x, y)| luma(t.get(x, y)))
                .collect()
        };
        let (va, vb) = (cols(a), cols(b));
        let ma = va.iter().sum::<f32>() / va.len() as f32;
        let mb = vb.iter().sum::<f32>() / vb.len() as f32;
        va.iter()
            .zip(&vb)
            .map(|(p, q)| ((p - ma) - (q - mb)).abs())
            .sum::<f32>()
            / va.len() as f32
    }

    /// Every furnishing texture is painted through the crate's lookups,
    /// 32 x 32, opaque but for the panes, and no two alike.
    #[test]
    fn every_furnishing_texture_is_painted_and_distinct() {
        let names = block_names();
        assert_eq!(names.len(), 24 + 3 + 3 + 30);
        let texs: Vec<Tex> = names
            .iter()
            .map(|n| crate::materials::paint_named(n, 0).unwrap_or_else(|| panic!("no painter for `{n}`")))
            .collect();
        for (n, t) in names.iter().zip(&texs) {
            assert_eq!(t.n, 32, "{n}");
            assert_eq!(crate::to_image(t).dimensions(), (32, 32), "{n}");
            if n.ends_with("_pane") {
                assert!(t.px.iter().any(|p| p.a < 0.5), "{n} has no clear glass");
            } else {
                assert!(t.px.iter().all(|p| p.a >= 1.0), "{n} is not opaque");
            }
        }
        for i in 0..texs.len() {
            for j in i + 1..texs.len() {
                assert_ne!(texs[i], texs[j], "`{}` and `{}` look the same", names[i], names[j]);
            }
        }
        // None is the old storage chest.
        let chest = crate::materials::paint_named("storage_chest_side", 0).unwrap();
        for (n, t) in names.iter().zip(&texs).filter(|(n, _)| n.ends_with("_side")) {
            assert!(
                pattern_difference(t, &chest, 0, SIZE) > 0.03,
                "{n} looks like the old chest"
            );
        }
    }

    /// Within a material, the grades differ in shape, not just shade: for
    /// posts in the strip that shows, for storage across the face.
    #[test]
    fn grades_differ_in_shape() {
        let check = |names: Vec<String>, x0: usize, x1: usize| {
            let texs: Vec<Tex> = names.iter().map(|n| paint(n).unwrap()).collect();
            for i in 0..texs.len() {
                for j in i + 1..texs.len() {
                    let d = pattern_difference(&texs[i], &texs[j], x0, x1);
                    assert!(d > 0.03, "{} vs {}: {d}", names[i], names[j]);
                }
            }
        };
        for m in Mat::POSTS {
            check(
                Grade::ALL.map(|g| format!("{}_{}_pole", g.name(), m.name())).to_vec(),
                10,
                22,
            );
            check(
                Grade::ALL.map(|g| format!("{}_{}_beam", g.name(), m.name())).to_vec(),
                8,
                24,
            );
        }
        for m in Mat::STORAGE {
            for face in ["top", "side"] {
                check(
                    Grade::ALL
                        .map(|g| format!("{}_{}_storage_{face}", g.name(), m.name()))
                        .to_vec(),
                    0,
                    SIZE,
                );
            }
        }
        check(Grade::ALL.map(|g| format!("{}_glass_pane", g.name())).to_vec(), 0, SIZE);
    }
}
