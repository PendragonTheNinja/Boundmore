//! Signs (milestone 22 part 4, `docs/milestones/22-building-blocks.md`):
//! a standalone sign and a sign panel in six materials, and the ink every
//! sign takes.
//!
//! Players' text is drawn over these faces, so the middle of each is calm
//! and even: a smooth board, light enough under dark text in wood and stone
//! (text turns light on the darker signs, and glows on metal, crystal and
//! glowing glass). A standalone sign has a slim raised frame round the edge
//! of the texture and a plain field inside it. A panel has no frame at all:
//! panels join edge to edge into one big sign, so each tiles seamlessly,
//! every pattern in it periodic across the face.
//!
//! Wood and stone are colourable and painted through timber's and stone's
//! ramps and grain; metal is brushed iron with dark iron fittings; crystal
//! the glow crystal's cyan in silver; glass is translucent as the glass
//! blocks are (`crate::building`), glowing glass brighter and frosted, in
//! a gold frame.

use std::f32::consts::TAU;

use crate::building::{Rock, Wood, face_inset, ridge, sparkles};
use crate::color::{Ramp, Rgba};
use crate::icons::{Icon, seg_dist};
use crate::materials::crystal_ramp;
use crate::noise::{hash2, seed, unit, value};
use crate::spark::dark_iron_ramp;
use crate::stations::iron_ramp;
use crate::tex::{Field, SIZE, SIZEF, Tex, c};
use crate::worlds::gold_alloy_ramp;

/// What a sign is made of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mat {
    Wood,
    Stone,
    Metal,
    Crystal,
    Glass,
    GlowingGlass,
}

impl Mat {
    pub const ALL: [Mat; 6] = [
        Mat::Wood,
        Mat::Stone,
        Mat::Metal,
        Mat::Crystal,
        Mat::Glass,
        Mat::GlowingGlass,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Mat::Wood => "wood",
            Mat::Stone => "stone",
            Mat::Metal => "metal",
            Mat::Crystal => "crystal",
            Mat::Glass => "glass",
            Mat::GlowingGlass => "glowing_glass",
        }
    }

    fn glass(self) -> bool {
        matches!(self, Mat::Glass | Mat::GlowingGlass)
    }
}

// ---------------------------------------------------------------- frame

/// The standalone sign's frame, this wide from the texture's edge.
const FRAME: f32 = 4.0;

/// Height across the frame, `d` in from the edge: a dark arris, a raised
/// rounded moulding, then a fine groove before the field.
fn frame_height(d: f32) -> f32 {
    if d < 1.0 {
        0.35 + 0.25 * d
    } else if d < 3.0 {
        0.75 + 0.3 * ridge(d, 0.6, 3.4)
    } else {
        0.3
    }
}

/// Which frame member a point lies on, mitred at the corners: 0 left,
/// 1 right, 2 top, 3 bottom; and whether it is a stile (runs up and down).
fn member(fx: f32, fy: f32) -> (u32, bool) {
    let side = fx.min(SIZEF - fx);
    let end = fy.min(SIZEF - fy);
    if side < end {
        (if fx < 16.0 { 0 } else { 1 }, true)
    } else {
        (if fy < 16.0 { 2 } else { 3 }, false)
    }
}

/// The field's height: flat.
const FIELD: f32 = 0.55;

// ---------------------------------------------------------------- opaque

/// Paints an opaque sign or panel: `field` gives the field's tone at a
/// point (before light), `frame_ramp` and `frame_tone` the frame's.
fn opaque(name: &str, m: Mat, framed: bool) -> Tex {
    let salt = seed(name);
    let height = Field::from_fn(|x, y| {
        let d = face_inset((c(x), c(y)));
        if framed && d < FRAME { frame_height(d) } else { FIELD }
    });
    let light = height.light();
    let stone = (m == Mat::Stone).then(|| Rock::Stone.grain(salt));
    let (ramp, frame_ramp): (Ramp, Ramp) = match m {
        Mat::Wood => (Wood::Timber.ramp(), Wood::Timber.ramp()),
        Mat::Stone => (Rock::Stone.ramp(), Rock::Stone.ramp()),
        Mat::Metal => (iron_ramp(), dark_iron_ramp()),
        _ => (crystal_ramp(), iron_ramp()),
    };
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let d = face_inset((fx, fy));
        let l = light.at(x, y);
        if framed && d < FRAME {
            let (k, stile) = member(fx, fy);
            let s = hash2(salt, 50 + k);
            let grain = match m {
                Mat::Wood => Wood::Timber.grain(s, fx, fy, !stile) * 0.7,
                Mat::Stone => stone.as_ref().map_or(0.0, |g| g.at(x, y)) * 0.5,
                _ => 0.0,
            };
            let base = match m {
                Mat::Wood => 0.42,
                Mat::Stone => 0.5,
                Mat::Metal => 0.5,
                _ => 0.55,
            };
            let off = (unit(s) - 0.5) * 0.06;
            return frame_ramp.pick(base + off + grain + l * 0.8 + (height.at(x, y) - 0.6) * 0.3, x, y);
        }
        let tone = field_tone(m, salt, stone.as_ref(), (x, y), framed);
        ramp.pick(tone + l * 0.6, x, y)
    });
    // A rivet in each corner of a metal frame, a sparkle or two in crystal.
    match m {
        Mat::Stone => {
            let mut flecked = tex.clone();
            Rock::Stone.flecks(&mut flecked, salt ^ 9);
            for i in 0..SIZE * SIZE {
                let (x, y) = (i % SIZE, i / SIZE);
                let on_frame = framed && face_inset((c(x), c(y))) < FRAME;
                // Flecks in full on the frame, faint in the field.
                let k = if on_frame { 1.0 } else { 0.35 };
                tex.px[i] = tex.px[i].mix(flecked.px[i], k);
            }
        }
        Mat::Metal if framed => {
            let dark = dark_iron_ramp();
            for (cx, cy) in [(2, 2), (29, 2), (2, 29), (29, 29)] {
                tex.set(cx, cy, dark.at(7));
                tex.set(cx + 1, cy, dark.at(5));
                tex.set(cx, cy + 1, dark.at(4));
                tex.set(cx + 1, cy + 1, dark.at(1));
            }
        }
        Mat::Crystal => {
            let at: &[(i32, i32)] = if framed {
                &[(7, 6), (25, 25)]
            } else {
                &[(6, 9), (22, 25)]
            };
            sparkles(&mut tex, at);
        }
        _ => {}
    }
    tex
}

/// The field's tone at a point, before light. Every term is periodic over
/// the face, so a panel tiles.
fn field_tone(m: Mat, salt: u32, stone: Option<&Field>, (x, y): (usize, usize), framed: bool) -> f32 {
    let (fx, fy) = (c(x), c(y));
    match m {
        Mat::Wood => {
            // Grain: now and then a row carries a fine darker streak a
            // third to two thirds of the way across.
            let h = hash2(salt, 7 + y as u32);
            let start = unit(hash2(h, 1)) * SIZEF;
            let len = 10.0 + unit(hash2(h, 2)) * 12.0;
            let streak = if unit(h) < 0.2 && (fx - start).rem_euclid(SIZEF) < len {
                -0.12
            } else {
                0.0
            };
            if framed {
                // One planed board, grain running across.
                (0.875 + Wood::Timber.grain(salt, fx, fy, false) * 0.45).min(0.89) + streak
            } else {
                // Four planed boards across, close-jointed: a faint line
                // where two meet, each board its own shade.
                let k = (fy / 8.0).floor() as u32;
                let s = hash2(salt, k);
                let ly = fy.rem_euclid(8.0);
                let joint = if ly < 1.0 { -0.17 } else { 0.0 };
                let tone = (0.87 + (unit(s) - 0.5) * 0.03 + Wood::Timber.grain(s, fx, fy, false) * 0.45).min(0.89);
                tone + if ly < 1.0 { joint } else { streak }
            }
        }
        Mat::Stone => {
            // Dressed smooth: faint tooling across, the rock's grain under it.
            let tool = (fy * TAU / 4.0 + (fx * TAU / 32.0).sin() * 0.6).sin() * 0.015;
            0.73 + stone.map_or(0.0, |g| g.at(x, y)) * 0.2 + tool
        }
        Mat::Metal => {
            // Brushed along x: each row its own faint shade, broken now and
            // then along its length; on the panel, one faint diamond of
            // bright hairlines a block, its corners at the edges' middles,
            // so joined panels carry a lattice.
            let h = hash2(salt, y as u32);
            let lit = unit(h) < 0.22;
            // A lit row runs most of the way across, broken at one place.
            let start = unit(hash2(h, 1)) * SIZEF;
            let gap = (fx - start).rem_euclid(SIZEF) < 6.0;
            let brush = if lit && !gap { 0.143 } else { 0.0 };
            let lattice = if framed {
                0.0
            } else {
                let on = (x + y) % 32 == 15 || (x + 32 - y) % 32 == 16;
                if on { 0.12 } else { 0.0 }
            };
            0.464 + brush + lattice
        }
        _ => {
            // Glow crystal: even light and a gentle diagonal glint.
            let s = (fx + fy).rem_euclid(SIZEF);
            let glint = (-((s - 9.0) / 1.6).powi(2)).exp() * 0.1;
            0.6 + glint
        }
    }
}

// ---------------------------------------------------------------- glass

/// Paints a glass sign or panel. Clear glass is a pale, faintly tinted
/// sheet with diagonal streaks of reflection; glowing glass a brighter,
/// frosted sheet. A sign frames it in iron (gold for glowing glass).
fn glass(name: &str, m: Mat, framed: bool) -> Tex {
    let salt = seed(name);
    let glow = m == Mat::GlowingGlass;
    let frame_ramp = if glow { gold_alloy_ramp() } else { iron_ramp() };
    let height = Field::from_fn(|x, y| {
        let d = face_inset((c(x), c(y)));
        if framed && d < FRAME { frame_height(d) } else { FIELD }
    });
    let light = height.light();
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let d = face_inset((fx, fy));
        if framed && d < FRAME {
            if d >= 3.0 {
                // A fine dark line where the glass meets the frame.
                return frame_ramp.pick(if glow { 0.3 } else { 0.18 }, x, y).with_alpha(0.7);
            }
            let t = 0.5 + light.at(x, y) * 0.8 + (height.at(x, y) - 0.6) * 0.3;
            return frame_ramp.pick(t, x, y);
        }
        // Streaks along the diagonal, periodic over the face.
        let s = (fx + fy).rem_euclid(SIZEF);
        let streak = if (9.0..11.0).contains(&s) {
            0.3
        } else if (14.0..15.0).contains(&s) {
            0.18
        } else {
            0.0
        };
        if glow {
            let frost = (value(salt, 8, 8, fx, fy) - 0.5) * 0.12 + (value(salt ^ 3, 16, 16, fx, fy) - 0.5) * 0.08;
            let base = Rgba::hex(0xe2f6f8).mix(Rgba::hex(0xffffff), (0.4 + frost * 2.0).clamp(0.0, 1.0));
            base.mix(Rgba::hex(0xffffff), streak)
                .with_alpha(0.52 + frost + streak * 0.4)
        } else {
            let tint = (value(salt, 2, 2, fx, fy) - 0.5) * 0.04;
            Rgba::hex(0xdcf0f6)
                .mix(Rgba::hex(0xffffff), streak)
                .with_alpha(0.2 + tint + streak)
        }
    })
}

// ---------------------------------------------------------------- ink

/// Blue-black ink with a cold sheen.
fn ink_ramp() -> Ramp {
    Ramp::from_hex(&[0x06081a, 0x0e122c, 0x161e42, 0x22305e, 0x34487e, 0x5068a4, 0x8aa2d4])
}

/// The pot's thick glass.
fn pot_glass_ramp() -> Ramp {
    Ramp::from_hex(&[0x1c2a3c, 0x34506a, 0x58809a, 0x88b0c6, 0xbcdce8, 0xeaf8fc])
}

/// A pale quill feather.
fn quill_ramp() -> Ramp {
    Ramp::from_hex(&[0x5a4e62, 0x857a8c, 0xb2a8b4, 0xd8d0d2, 0xf0eae2, 0xfffbf2])
}

/// Ink: a squat glass ink pot, nearly full of blue-black ink, a quill
/// standing in it and leaning to the right.
fn ink() -> Tex {
    let ink = ink_ramp();
    let glass = pot_glass_ramp();
    let quill = quill_ramp();
    let (cx, top, bottom) = (13.0f32, 15.0f32, 29.5f32);
    // The pot's half width at height `py`, or `None` outside it.
    let pot = |py: f32| -> Option<f32> {
        if (top..bottom).contains(&py) {
            // Rounded shoulders and foot.
            let sh = (top + 3.0 - py).max(0.0);
            let ft = (py - (bottom - 2.5)).max(0.0);
            Some(10.0 - sh * sh * 0.45 - ft * ft * 0.6)
        } else if (top - 3.0..top).contains(&py) {
            Some(5.0)
        } else if (top - 5.0..top - 3.0).contains(&py) {
            Some(6.2)
        } else {
            None
        }
    };
    let (a, b) = ((15.5f32, 17.0f32), (28.5f32, 1.5f32));
    let len = (b.0 - a.0).hypot(b.1 - a.1);
    let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
    let mut icon = Icon::new();
    // The quill first, so the pot stands in front of its foot.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (px, py) = (p.0 - a.0, p.1 - a.1);
        let t = (px * ux + py * uy) / len;
        let s = px * -uy + py * ux;
        if !(0.0..=1.0).contains(&t) {
            return None;
        }
        // The shaft.
        if seg_dist(a, b, p) < 0.8 {
            return Some(quill.pick(0.55 + (0.5 - t) * 0.1, x, y));
        }
        // The vane: the upper two thirds, wider on its left side.
        if t > 0.32 {
            let k = ((t - 0.32) / 0.68 * std::f32::consts::PI).sin().powf(0.6);
            let (wl, wr) = (4.2 * k, 2.6 * k);
            if (s < 0.0 && -s < wl) || (s > 0.0 && s < wr) {
                // Barbs: faint lines slanting back from the shaft.
                let barb = ((t * len * 1.2 + s.abs() * 0.7).rem_euclid(2.0) < 0.6) as i32 as f32;
                let edge = if s < 0.0 { -s / wl } else { s / wr };
                let tone = 0.85 - edge * 0.25 - barb * 0.15 - if s > 0.0 { 0.15 } else { 0.0 };
                return Some(quill.pick(tone, x, y));
            }
        }
        None
    });
    // The pot: thick glass, the ink filling it to the shoulders.
    icon.layer(true, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let half = pot(fy)?;
        let dx = fx - cx;
        if dx.abs() >= half {
            return None;
        }
        let k = dx / half;
        if fy < top - 3.0 {
            // The lip, lit on top.
            let t = if fy < top - 4.0 { 0.85 } else { 0.55 } - k * 0.2;
            return Some(glass.pick(t, x, y));
        }
        if fy < top {
            // The neck: dark ink seen down it.
            if dx.abs() < 3.6 {
                return Some(ink.pick(0.1 + (fy - top + 3.0) * 0.05, x, y));
            }
            return Some(glass.pick(0.6 - k * 0.3, x, y));
        }
        let wall = dx.abs() > half - 1.2 || fy > bottom - 1.2;
        if wall {
            return Some(glass.pick(0.62 - k * 0.3, x, y));
        }
        // A highlight down the left of the body.
        if (dx + 6.0).abs() < 0.7 && (top + 2.5..bottom - 3.0).contains(&fy) {
            return Some(glass.pick(0.98, x, y));
        }
        let mut t = 0.32 - k * 0.15 - (fy - top) / 30.0;
        // The ink's surface just under the shoulder.
        if fy < top + 1.5 {
            t += 0.35;
        }
        Some(ink.pick(t, x, y))
    });
    // A drip of ink down the lip on the right.
    icon.layer(false, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let on = ((fx - (cx + 5.0)).abs() < 0.6 && (top - 4.0..top + 1.0).contains(&fy))
            || (fx - (cx + 5.0)).hypot(fy - (top + 1.2)) < 1.0;
        on.then(|| ink.pick(0.25, x, y))
    });
    icon.finish()
}

// ---------------------------------------------------------------- lookups

/// Every block texture this module paints, in catalogue order.
pub fn block_names() -> Vec<String> {
    let mut names = Vec::new();
    for m in Mat::ALL {
        names.push(format!("{}_sign", m.name()));
        names.push(format!("{}_sign_panel", m.name()));
    }
    names
}

/// The item icons this module paints.
pub const ICONS: [&str; 1] = ["ink"];

/// Paints a sign or sign panel, or `None`.
pub fn paint(name: &str) -> Option<Tex> {
    let (rest, framed) = if let Some(r) = name.strip_suffix("_sign_panel") {
        (r, false)
    } else {
        (name.strip_suffix("_sign")?, true)
    };
    let m = Mat::ALL.into_iter().find(|k| k.name() == rest)?;
    Some(if m.glass() {
        glass(name, m, framed)
    } else {
        opaque(name, m, framed)
    })
}

/// Paints one of this module's icons, or `None`.
pub fn paint_icon(name: &str) -> Option<Tex> {
    (name == "ink").then(ink)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(p: Rgba) -> f32 {
        (0.2126 * p.r + 0.7152 * p.g + 0.0722 * p.b) * p.a
    }

    fn step(a: Rgba, b: Rgba) -> f32 {
        (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs() + (a.a - b.a).abs()
    }

    fn col_mean(t: &Tex, x: usize) -> f32 {
        (0..SIZE).map(|y| luma(t.get(x, y))).sum::<f32>() / SIZE as f32
    }

    fn row_mean(t: &Tex, y: usize) -> f32 {
        (0..SIZE).map(|x| luma(t.get(x, y))).sum::<f32>() / SIZE as f32
    }

    /// Luma mean and spread over `x0..x1, y0..y1`.
    fn stats(t: &Tex, x0: usize, x1: usize, y0: usize, y1: usize) -> (f32, f32) {
        let v: Vec<f32> = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .map(|(x, y)| luma(t.get(x, y)))
            .collect();
        let m = v.iter().sum::<f32>() / v.len() as f32;
        let var = v.iter().map(|q| (q - m).powi(2)).sum::<f32>() / v.len() as f32;
        (m, var.sqrt())
    }

    /// Every sign texture and the ink are painted through the crate's
    /// lookups, 32 x 32, opaque but for glass, and no two alike.
    #[test]
    fn every_sign_texture_and_icon_is_painted_and_distinct() {
        let names = block_names();
        assert_eq!(names.len(), 12);
        let texs: Vec<Tex> = names
            .iter()
            .map(|n| crate::materials::paint_named(n, 0).unwrap_or_else(|| panic!("no painter for `{n}`")))
            .collect();
        for (n, t) in names.iter().zip(&texs) {
            assert_eq!(t.n, 32, "{n}");
            assert_eq!(crate::to_image(t).dimensions(), (32, 32), "{n}");
            if n.contains("glass") {
                assert!(t.px.iter().any(|p| p.a < 0.7), "{n} is not see-through");
            } else {
                assert!(t.px.iter().all(|p| p.a >= 1.0), "{n} is not opaque");
            }
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

    /// A sign has a frame round a calm field; wood and stone fields are
    /// light enough for dark text.
    #[test]
    fn signs_are_framed_round_a_calm_field() {
        for m in Mat::ALL {
            let n = format!("{}_sign", m.name());
            let t = paint(&n).unwrap();
            let (field, spread) = stats(&t, 8, 24, 8, 24);
            let calm = if m.glass() { 0.12 } else { 0.06 };
            assert!(spread < calm, "{n}: field spread {spread}");
            // The frame differs from the field, in shade or in alpha.
            let frame: Vec<Rgba> = (0..SIZE)
                .map(|i| t.get(i, 2))
                .chain((0..SIZE).map(|i| t.get(2, i)))
                .collect();
            let fl = frame.iter().map(|p| luma(*p)).sum::<f32>() / frame.len() as f32;
            let fa = frame.iter().map(|p| p.a).sum::<f32>() / frame.len() as f32;
            let ma = t.get(16, 16).a;
            assert!((fl - field).abs() > 0.04 || (fa - ma).abs() > 0.3, "{n}: no frame");
            if matches!(m, Mat::Wood | Mat::Stone) {
                assert!(field > 0.5, "{n}: field too dark for dark text ({field})");
            }
        }
    }

    /// A panel has no border, and tiles: its edges look like its middle
    /// and the step across each wrap is like a step inside.
    #[test]
    fn panels_have_no_border_and_tile() {
        for m in Mat::ALL {
            let n = format!("{}_sign_panel", m.name());
            let t = paint(&n).unwrap();
            let (_, spread) = stats(&t, 0, SIZE, 0, SIZE);
            // Glass carries its streaks of reflection; the rest is calm.
            let calm = if m.glass() { 0.12 } else { 0.07 };
            assert!(spread < calm, "{n}: spread {spread}");
            // Each edge column and row lies within the range of the inner
            // ones: a border would stand out of it.
            for (what, f) in [("column", &col_mean as &dyn Fn(&Tex, usize) -> f32), ("row", &row_mean)] {
                let inner: Vec<f32> = (2..SIZE - 2).map(|i| f(&t, i)).collect();
                let lo = inner.iter().cloned().fold(f32::MAX, f32::min) - 0.03;
                let hi = inner.iter().cloned().fold(f32::MIN, f32::max) + 0.03;
                for edge in [0, 1, SIZE - 2, SIZE - 1] {
                    let v = f(&t, edge);
                    assert!(
                        (lo..=hi).contains(&v),
                        "{n}: {what} {edge} ({v}) stands out of {lo}..{hi}"
                    );
                }
            }
            let mean = |f: &dyn Fn(usize) -> f32| (0..SIZE).map(f).sum::<f32>() / SIZE as f32;
            // Patterns repeat every 8 pixels or a divisor of 32, so the wrap
            // is compared with the steps at 8, 16 and 24, the worst of them.
            let worst =
                |f: &dyn Fn(usize, usize) -> f32| [8, 16, 24].map(|k| f(k - 1, k)).into_iter().fold(0.0, f32::max);
            let cols = |a: usize, b: usize| mean(&|i| step(t.get(a, i), t.get(b, i)));
            let rows = |a: usize, b: usize| mean(&|i| step(t.get(i, a), t.get(i, b)));
            for (what, f) in [("columns", &cols as &dyn Fn(usize, usize) -> f32), ("rows", &rows)] {
                let (across, inside) = (f(31, 0), worst(f));
                assert!(across < inside * 1.25 + 0.03, "{n}: {what} seam {across} vs {inside}");
            }
        }
    }
}
