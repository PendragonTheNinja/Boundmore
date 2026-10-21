//! Icons for what creatures drop and what hunts them (milestone 13): meat,
//! bone, tallow and hide, bloods in stoppered vials, glands and sacs, the
//! mantle, one eye per creature, the trophies, vital essence, and the three
//! slings.
//!
//! Same family as `icons` and `gathered`: a sprite on a clear ground with a
//! dark outline, light from the top left, chunky shapes that read at
//! 16 x 16. Things of one kind share a composition (bloods in one vial, eyes
//! as one ball, trophies with a gold band, slings along the tool diagonal)
//! and differ by colour and a mark of their own.

use std::f32::consts::{FRAC_1_SQRT_2, PI};

use crate::color::{HUE_BLUE, HUE_GOLD, HUE_PURPLE, Ramp, RampSpec, Rgba};
use crate::gathered::{blob, ramp};
use crate::icons::{Icon, Lump, P, cord_ramp, inside_poly, leather_ramp, seg_dist, vine_ramp};
use crate::materials::{copper_ramp, end_grain_ramp, plank_ramp, steel_ramp, stone_ramp};
use crate::noise::{hash2, seed, unit, value};
use crate::tex::{Tex, c};

/// How much a point at offset (dx, dy) from a round shape's centre faces
/// the top-left light, -1..1 at radius `r`.
fn lit(dx: f32, dy: f32, r: f32) -> f32 {
    -(dx * 0.6 + dy * 0.8) / r
}

fn gold_ramp() -> Ramp {
    Ramp::from_hex(&[0x5a2e14, 0x8a5418, 0xc08a22, 0xe8b838, 0xfad86a, 0xfff2b8])
}

fn glass_ramp() -> Ramp {
    Ramp::from_hex(&[0x3a4a64, 0x5a7090, 0x8aa6c0, 0xb8d0e0, 0xe0eef6, 0xffffff])
}

// ------------------------------------------------------------------ strokes

/// A path sampled along `s` in 0..1, drawn as a tube of varying width.
struct Stroke {
    pts: Vec<P>,
}

impl Stroke {
    fn new(n: usize, f: impl Fn(f32) -> P) -> Self {
        Self {
            pts: (0..=n).map(|i| f(i as f32 / n as f32)).collect(),
        }
    }

    /// The nearest point on the path: where along it (0..1), how far, and
    /// how much the offset faces the light (-1..1).
    fn near(&self, p: P) -> (f32, f32, f32) {
        let n = self.pts.len() - 1;
        let mut best = (0.0, f32::MAX, 0.0);
        for i in 0..n {
            let (a, b) = (self.pts[i], self.pts[i + 1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len2 = (dx * dx + dy * dy).max(1e-6);
            let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0);
            let (ox, oy) = (p.0 - a.0 - dx * t, p.1 - a.1 - dy * t);
            let d = ox.hypot(oy);
            if d < best.1 {
                let l = if d > 1e-4 { lit(ox, oy, d) } else { 0.0 };
                best = ((i as f32 + t) / n as f32, d, l);
            }
        }
        best
    }

    /// Draws the tube: `paint(s, k, x, y)` with `k` -1..1 across it, +1 on
    /// its lit edge.
    fn draw(
        &self,
        icon: &mut Icon,
        contact: bool,
        width: impl Fn(f32) -> f32,
        paint: impl Fn(f32, f32, usize, usize) -> Option<Rgba>,
    ) {
        icon.layer(contact, |x, y| {
            let (s, d, l) = self.near((c(x), c(y)));
            let w = width(s);
            if d > w {
                return None;
            }
            paint(s, l * d / w.max(0.5), x, y)
        });
    }
}

// --------------------------------------------------------------------- meat

/// Raw meat: a thick steak seen from above, red with white marbling, a cap
/// of fat along its far edge and a round of bone in it.
fn raw_meat() -> Tex {
    steak(false)
}

/// Lean meat: a narrower, darker cut with no fat or bone, its grain
/// running across it and a thin silver skin along one edge.
fn lean_meat() -> Tex {
    steak(true)
}

fn steak(lean: bool) -> Tex {
    let salt = seed(if lean { "lean_meat" } else { "raw_meat" });
    let red = if lean {
        RampSpec::new(Rgba::hex(0x96283a), 8, 0.2, 0.72)
            .cool(HUE_PURPLE, 0.4)
            .warm(HUE_GOLD, 0.12)
            .build()
    } else {
        ramp(0xd03a44, 0.28, 0.86)
    };
    let fat = ramp(0xf4e2cc, 0.6, 0.98);
    let silver = ramp(0xd8c8dc, 0.55, 0.96);
    let bone = ramp(0xe8dcb8, 0.5, 0.96);
    let (cx, cy, rx, ry, turn, depth) = if lean {
        (16.0f32, 14.5f32, 13.0f32, 6.8f32, -0.62f32, 3)
    } else {
        (16.0, 14.5, 12.5, 8.5, -0.3, 4)
    };
    let (s, co) = turn.sin_cos();
    let local = move |px: f32, py: f32| {
        let (dx, dy) = (px - cx, py - cy);
        let (u, v) = (dx * co + dy * s, -dx * s + dy * co);
        let a = v.atan2(u);
        let rough = 1.0 + 0.05 * (a * 3.0 + 1.3).sin() + 0.03 * (a * 5.0).cos();
        ((u / rx).hypot(v / ry) / rough, u, v)
    };
    let mut icon = Icon::new();
    // The cut side: the same outline dropped a few pixels, streaked with
    // fat on the raw cut.
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        let on = (0..=depth).any(|k| local(px, py - k as f32).0 <= 1.0);
        if !on || local(px, py).0 <= 1.0 {
            return None;
        }
        let (_, u, _) = local(px, py - 2.0);
        if !lean && (u * 0.7 + (py * 1.3).sin()).rem_euclid(5.0) < 1.0 {
            return Some(fat.pick(0.35, x, y));
        }
        Some(red.pick(0.22 - u / rx * 0.08, x, y))
    });
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        let (d, u, v) = local(px, py);
        if d > 1.0 {
            return None;
        }
        // The far (upper) rim: a fat cap on the raw cut, a silver skin on
        // the lean one.
        if d > 0.78 && v < -1.0 {
            return Some(if lean {
                silver.pick(0.5 + lit(u, v, rx) * 0.3, x, y)
            } else {
                fat.pick(0.62 + lit(u, v, rx) * 0.3, x, y)
            });
        }
        let mut t = 0.55 + lit(px - cx, py - cy, rx) * 0.3 - d * d * 0.12;
        if lean {
            // Grain: fine lines across the cut, wavering a little.
            let g = u + (v * 0.7).sin() * 0.8;
            if g.rem_euclid(3.0) < 0.9 {
                t -= 0.16;
            }
            return Some(red.pick(t, x, y));
        }
        // Marbling: thin wandering white veins.
        let m = value(salt, 4, 4, px * 1.3, py * 1.3);
        if (m - 0.5).abs() < 0.05 {
            return Some(fat.pick(0.7, x, y));
        }
        if (m - 0.5).abs() < 0.11 {
            t += 0.16;
        }
        // The round of bone, near the left.
        let (bx, by) = (u + 6.0, v - 1.0);
        let r = bx.hypot(by);
        if r < 2.8 {
            return Some(if r < 1.2 {
                Rgba::hex(0x8a3a3a)
            } else {
                bone.pick(0.6 + lit(bx, by, 2.8) * 0.3, x, y)
            });
        }
        Some(red.pick(t, x, y))
    });
    icon.finish()
}

/// Bone: a long bone along the diagonal, a double knob at each end.
fn bone() -> Tex {
    let ivory = RampSpec::new(Rgba::hex(0xe8dcb4), 7, 0.4, 0.98)
        .cool(HUE_BLUE, 0.3)
        .warm(HUE_GOLD, 0.2)
        .build();
    let (a, b) = ((8.5f32, 23.5f32), (23.5f32, 8.5f32));
    let mut icon = Icon::new();
    let knobs = [
        (a.0 - 3.0, a.1 - 1.0),
        (a.0 + 1.0, a.1 + 3.0),
        (b.0 - 1.0, b.1 - 3.0),
        (b.0 + 3.0, b.1 + 1.0),
    ];
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let shaft = seg_dist(a, b, p);
        let knob = knobs
            .iter()
            .map(|k| ((p.0 - k.0).hypot(p.1 - k.1), *k))
            .min_by(|l, r| l.0.total_cmp(&r.0))
            .unwrap();
        if shaft <= 2.6 {
            // Round in section: lit along its top-left side.
            let side = ((p.1 - p.0) - (a.1 - a.0)) * FRAC_1_SQRT_2;
            let mut t = 0.55 - side / 2.6 * 0.35;
            if (p.0 + p.1 - 32.0).abs() < 1.0 {
                t -= 0.06;
            }
            return Some(ivory.pick(t, x, y));
        }
        if knob.0 <= 3.4 {
            let (dx, dy) = (p.0 - knob.1.0, p.1 - knob.1.1);
            return Some(ivory.pick(0.55 + lit(dx, dy, 3.4) * 0.4, x, y));
        }
        None
    });
    icon.finish()
}

/// Tallow: a waxy pale cake in three-quarter view, a thumb dent in its top
/// and soft wax rolling over its front edge.
fn tallow() -> Tex {
    let wax = RampSpec::new(Rgba::hex(0xf4e6bc), 8, 0.4, 0.99)
        .cool(HUE_PURPLE, 0.25)
        .warm(HUE_GOLD, 0.3)
        .build();
    let (len, depth, height) = (17.0f32, 12.0f32, 10.5f32);
    let (ox, oy) = (3.5f32, 26.0f32);
    let proj = |x: f32, y: f32, z: f32| (ox + x + z * 0.72, oy - y - z * 0.62);
    let top = [
        proj(0.0, height, 0.0),
        proj(len, height, 0.0),
        proj(len, height, depth),
        proj(0.0, height, depth),
    ];
    let front = [proj(0.0, 0.0, 0.0), proj(len, 0.0, 0.0), top[1], top[0]];
    let end = [proj(len, 0.0, 0.0), proj(len, 0.0, depth), top[2], top[1]];
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        inside_poly(&end, p.0, p.1)
            .then(|| wax.pick(0.3 + if seg_dist(end[3], end[2], p) < 0.9 { 0.14 } else { 0.0 }, x, y))
    });
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&front, p.0, p.1) {
            return None;
        }
        let mut t = 0.58 - (p.0 - ox) / len * 0.1;
        // Soft melted wax rolling over the top edge in a wavy lip.
        let lip = oy - height + 1.6 + (p.0 * 0.8).sin() * 1.0 + if (p.0 - 9.0).abs() < 1.3 { 3.0 } else { 0.0 };
        if p.1 < lip {
            t += 0.24;
        } else if p.1 < lip + 0.9 {
            t -= 0.06;
        }
        // A waxy sheen low on the face.
        if (p.0 - p.1 * 0.3 - 8.0).abs() < 0.8 && p.1 > lip + 2.0 {
            t += 0.16;
        }
        if seg_dist(front[0], front[1], p) < 0.9 {
            t -= 0.14;
        }
        Some(wax.pick(t, x, y))
    });
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&top, p.0, p.1) {
            return None;
        }
        let mut t = 0.8 - (p.0 - ox) / len * 0.08;
        // The thumb dent: shaded on its near-left wall, lit on its far one.
        let (dx, dy) = (p.0 - 15.5, p.1 - 11.0);
        let d = (dx / 4.2).hypot(dy / 2.4);
        if d < 1.0 {
            t = 0.58 - lit(dx, dy, 4.0) * 0.18;
        }
        Some(wax.pick(t, x, y))
    });
    icon.finish()
}

/// Hide: a folded pelt, its fur in short tufts, a corner turned back to
/// show the pale leather underneath.
fn hide() -> Tex {
    let salt = seed("hide");
    let fur = ramp(0xa8683a, 0.28, 0.86);
    let suede = ramp(0xe0b88a, 0.5, 0.94);
    let pelt: [P; 14] = [
        (3.5, 5.0),
        (10.0, 8.0),
        (16.0, 6.0),
        (22.0, 8.0),
        (28.5, 4.5),
        (25.0, 12.0),
        (26.5, 18.0),
        (29.0, 27.0),
        (22.0, 24.5),
        (16.0, 27.5),
        (10.0, 24.5),
        (3.0, 28.0),
        (7.0, 19.0),
        (5.5, 12.0),
    ];
    let fold: [P; 3] = [(26.0, 19.0), (29.0, 27.0), (20.0, 25.5)];
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let edge = (0..pelt.len())
            .map(|i| seg_dist(pelt[i], pelt[(i + 1) % pelt.len()], p))
            .fold(f32::MAX, f32::min);
        // A ragged fringe of fur round the edge.
        let fringe = edge < 1.2 && unit(hash2(salt ^ 0x33, (x + y * 32) as u32)) > 0.5;
        if !inside_poly(&pelt, p.0, p.1) && !fringe {
            return None;
        }
        // Tufts: short diagonal strokes of lighter and darker fur.
        let cell = hash2(salt, ((x + y) / 3 + (y / 2) * 16) as u32);
        let mut t = 0.52 - (p.0 + p.1 - 32.0) / 40.0 * 0.22 + (unit(cell) - 0.5) * 0.24;
        // A darker stripe down the back.
        if (p.0 - p.1 * 0.05 - 15.0).abs() < 2.0 {
            t -= 0.16;
        }
        let stroke = hash2(salt ^ 0x9, ((x + 2 * y) / 4 + (y / 3) * 13) as u32);
        if (x + 2 * y) % 4 == 0 && unit(stroke) > 0.4 {
            t += 0.2;
        } else if (x + 2 * y) % 4 == 2 && unit(stroke) > 0.55 {
            t -= 0.16;
        }
        if edge < 1.2 {
            t += if p.0 + p.1 < 32.0 { 0.18 } else { -0.12 };
        }
        Some(fur.pick(t, x, y))
    });
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&fold, p.0, p.1) {
            return None;
        }
        let mut t = 0.62 + (p.0 - 24.0) * 0.02;
        if seg_dist(fold[0], fold[2], p) < 1.0 {
            t += 0.2;
        }
        Some(suede.pick(t, x, y))
    });
    icon.finish()
}

// -------------------------------------------------------------------- vials

/// A stoppered glass vial of a liquid: a cork, a lipped neck, a rounded
/// body filled from `level` down, a long glint up its left side. `thick`
/// leaves a smear on the glass above the liquid and a slow bubble in it.
fn vial(liquid: &Ramp, thick: bool) -> Tex {
    let glass = glass_ramp();
    let cork = end_grain_ramp();
    let level = 14.0f32;
    let mut icon = Icon::new();
    // The body and neck.
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        let body = {
            let (qx, qy) = ((px - 16.0).abs() - 3.0, (py - 20.5).abs() - 4.5);
            qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) < 4.0
        };
        let neck = (px - 16.0).abs() < 3.0 && (8.0..13.0).contains(&py);
        let lip = (px - 16.0).abs() < 4.0 && (8.0..10.0).contains(&py);
        if !body && !neck && !lip {
            return None;
        }
        let dx = px - 16.0;
        let rim = !lip && (dx.abs() > if body && py > 12.5 { 6.0 } else { 2.0 } || py > 27.6);
        if lip {
            return Some(glass.pick(0.7 - (py - 8.0) * 0.12 - dx * 0.03, x, y));
        }
        // The glint up the left side.
        if body && (dx + 4.5).abs() < 0.6 && (13.5..25.0).contains(&py) {
            return Some(glass.lightest());
        }
        let meniscus = level + (dx / 6.0).powi(2) * -0.8;
        if body && py > meniscus && !rim {
            let mut t = 0.58 - dx / 7.0 * 0.32 - (py - level) / 15.0 * 0.12;
            if py < meniscus + 1.0 {
                t += 0.22;
            }
            if thick && (px - 18.0).hypot(py - 22.5) < 1.6 {
                t += 0.32;
            }
            return Some(liquid.pick(t, x, y));
        }
        if thick && body && !rim && py > meniscus - 3.0 && (x * 3 + y) % 5 < 2 {
            return Some(liquid.pick(0.35, x, y));
        }
        let t = if rim { 0.35 - dx * 0.02 } else { 0.62 - dx / 7.0 * 0.2 };
        Some(glass.pick(t, x, y))
    });
    // The cork.
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        if (px - 16.0).abs() > 2.6 || !(3.0..9.0).contains(&py) {
            return None;
        }
        let mut t = 0.6 - (px - 16.0) / 2.6 * 0.3;
        if py < 4.2 {
            t += 0.2;
        }
        Some(cork.pick(t, x, y))
    });
    icon.finish()
}

fn fresh_blood() -> Tex {
    let red = RampSpec::new(Rgba::hex(0xe02030), 7, 0.28, 0.82)
        .cool(HUE_PURPLE, 0.25)
        .warm(HUE_GOLD, 0.15)
        .build();
    vial(&red, false)
}

fn thick_blood() -> Tex {
    let dark = RampSpec::new(Rgba::hex(0x6a1438), 7, 0.14, 0.6)
        .cool(HUE_PURPLE, 0.45)
        .warm(HUE_PURPLE, 0.2)
        .build();
    vial(&dark, true)
}

/// Vital essence: a round flask glowing green-gold from inside, a bright
/// crystal droplet floating at its heart.
fn vital_essence() -> Tex {
    let glass = glass_ramp();
    let cork = end_grain_ramp();
    let glow = RampSpec::new(Rgba::hex(0xb4dc3c), 8, 0.42, 0.99)
        .cool(HUE_BLUE, 0.2)
        .warm(HUE_GOLD, 0.45)
        .build();
    let (cx, cy, r) = (16.0f32, 20.0f32, 9.5f32);
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        let (dx, dy) = (px - cx, py - cy);
        let d = dx.hypot(dy);
        let ball = d < r;
        let neck = dx.abs() < 2.6 && (7.0..12.0).contains(&py);
        let lip = dx.abs() < 3.6 && (7.0..9.0).contains(&py);
        if !ball && !neck && !lip {
            return None;
        }
        if lip || neck && !ball {
            return Some(glass.pick(if lip { 0.7 } else { 0.55 - dx * 0.08 }, x, y));
        }
        if d > r - 1.2 {
            return Some(glass.pick(0.45 + lit(dx, dy, r) * 0.3, x, y));
        }
        if (dx + 5.0).hypot(dy + 4.0) < 1.3 {
            return Some(glass.lightest());
        }
        // Lit from inside: brightest round the droplet.
        let heat = 1.0 - dx.hypot(dy - 0.5) / r;
        if py < cy - 4.5 {
            return Some(glass.pick(0.62 - dx * 0.03, x, y));
        }
        Some(glow.pick(0.25 + heat * 0.7, x, y))
    });
    // The droplet: a cut gem pointed at the top.
    icon.layer(false, |x, y| {
        let (dx, dy) = (c(x) - cx, c(y) - (cy + 0.5));
        let w = if dy < 0.0 {
            2.6 * (1.0 + dy / 4.0)
        } else {
            (2.6f32.powi(2) - dy * dy).max(0.0).sqrt()
        };
        if dx.abs() > w || dy < -4.0 {
            return None;
        }
        let t = if dx < 0.0 { 1.0 } else { 0.86 };
        Some(
            glow.pick(t, x, y)
                .mix(Rgba::hex(0xffffff), if dx < -0.5 && dy < 0.0 { 0.6 } else { 0.2 }),
        )
    });
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        if (px - 16.0).abs() > 2.4 || !(2.5..8.0).contains(&py) {
            return None;
        }
        let t = 0.6 - (px - 16.0) / 2.4 * 0.3 + if py < 3.8 { 0.2 } else { 0.0 };
        Some(cork.pick(t, x, y))
    });
    icon.finish()
}

// ------------------------------------------------------------- glands, sacs

/// Milk gland: a cluster of three pale fleshy lobes laced with pink veins,
/// a bead of milk hanging from the lowest.
fn milk_gland() -> Tex {
    let salt = seed("milk_gland");
    let flesh = ramp(0xf0d4cc, 0.45, 0.97);
    let lobes = [
        ((11.0f32, 11.0f32), (7.5f32, 6.5f32), -0.4f32),
        ((21.0, 10.5), (7.0, 6.2), 0.3),
        ((16.0, 18.0), (9.5, 7.5), 0.0),
    ];
    let mut icon = Icon::new();
    for (i, &(centre, r, turn)) in lobes.iter().enumerate() {
        blob(&mut icon, centre, r, turn, &flesh, 0.05, i == 2, hash2(salt, i as u32));
    }
    // Veins: thin pink lines wandering over them.
    icon.layer(false, |x, y| {
        let (px, py) = (c(x), c(y));
        let on = lobes
            .iter()
            .any(|&(cn, r, _)| ((px - cn.0) / (r.0 - 1.5)).hypot((py - cn.1) / (r.1 - 1.5)) < 1.0);
        if !on {
            return None;
        }
        let m = value(salt, 4, 4, px * 1.2, py * 1.2);
        ((m - 0.5).abs() < 0.035).then(|| Rgba::hex(0xd88a9a))
    });
    // The bead of milk.
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - 16.5, c(y) - 26.8);
        let r = if dy < 0.0 { 2.4 * (1.0 + dy / 4.0) } else { 2.4 };
        if dx.abs() > r || dy < -4.0 || dx.hypot(dy.max(0.0)) > 2.4 {
            return None;
        }
        Some(if dx < 0.0 && dy < 0.5 {
            Rgba::hex(0xffffff)
        } else {
            Rgba::hex(0xe4e8f0)
        })
    });
    icon.finish()
}

/// Quick gland: a lively green-yellow pear, glossy, ringed in segments and
/// freckled with bright spots.
fn quick_gland() -> Tex {
    let salt = seed("quick_gland");
    let lime = RampSpec::new(Rgba::hex(0xb4d03a), 8, 0.3, 0.97)
        .cool(HUE_BLUE, 0.35)
        .warm(HUE_GOLD, 0.35)
        .build();
    let body = Stroke::new(40, |s| (9.0 + s * 13.0, 24.0 - s * 14.0));
    let width = |s: f32| 3.0 + 5.8 * (PI * (0.15 + s * 0.85)).sin().max(0.0).powf(0.7) * (1.0 - s * 0.35);
    let mut icon = Icon::new();
    body.draw(&mut icon, true, width, |s, k, x, y| {
        let mut t = 0.5 + k * 0.36;
        // Segment rings across it.
        if (s * 5.0).fract() < 0.12 && s > 0.1 && s < 0.95 {
            t -= 0.24;
        }
        // Freckles.
        if unit(hash2(salt, (x / 2 + (y / 2) * 16) as u32)) > 0.86 {
            t += 0.3;
        }
        // The gloss.
        if (k - 0.62).abs() < 0.1 && (0.3..0.75).contains(&s) {
            t = 1.0;
        }
        Some(lime.pick(t, x, y))
    });
    // A little stalk where it was cut free.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        (seg_dist((24.5, 7.5), (27.0, 4.5), p) < 1.1).then(|| lime.pick(0.3, x, y))
    });
    icon.finish()
}

/// Spark sac: a dark blue-violet sac with lightning crackling inside it
/// and sparks jumping off its top.
fn spark_sac() -> Tex {
    let salt = seed("spark_sac");
    let sac = RampSpec::new(Rgba::hex(0x3a48b8), 8, 0.2, 0.9)
        .cool(HUE_PURPLE, 0.4)
        .warm(200.0, 0.4)
        .build();
    let bolt: [P; 6] = [
        (8.0, 15.0),
        (13.0, 19.5),
        (15.5, 15.0),
        (19.0, 21.0),
        (21.5, 16.5),
        (25.0, 20.0),
    ];
    let bolt_d = move |p: P| {
        (0..5)
            .map(|i| seg_dist(bolt[i], bolt[i + 1], p))
            .fold(f32::MAX, f32::min)
    };
    let mut icon = Icon::new();
    blob(&mut icon, (16.0, 18.5), (12.5, 9.5), 0.0, &sac, 0.06, false, salt);
    icon.layer(false, |x, y| {
        let (px, py) = (c(x), c(y));
        if ((px - 16.0) / 11.5).hypot((py - 18.5) / 8.5) > 1.0 {
            return None;
        }
        let d = bolt_d((px, py));
        if d < 0.7 {
            return Some(Rgba::hex(0xf0ffff));
        }
        if d < 1.6 {
            return Some(Rgba::hex(0x6ae0ff));
        }
        if d < 3.0 {
            return Some(sac.pick(0.72, x, y));
        }
        None
    });
    // A gloss and the sparks.
    icon.layer(false, |x, y| {
        let (px, py) = (c(x), c(y));
        if (px - 10.0).hypot(py - 13.0) < 1.4 {
            return Some(Rgba::hex(0xdce8ff));
        }
        for (sx, sy, r) in [(24.0f32, 5.5f32, 2.2f32), (9.0, 5.0, 1.6), (17.0, 3.5, 1.4)] {
            let (ex, ey) = ((px - sx).abs(), (py - sy).abs());
            if (ex < 0.6 && ey < r) || (ey < 0.6 && ex < r) {
                return Some(if ex < 0.6 && ey < 0.6 {
                    Rgba::hex(0xffffff)
                } else {
                    Rgba::hex(0x7ae8ff)
                });
            }
        }
        None
    });
    icon.finish()
}

/// Mantle: a ruffled leathery membrane hanging from a thick rim, pleated
/// in folds that end in a frilled edge.
fn mantle() -> Tex {
    let skin = RampSpec::new(Rgba::hex(0x9a5a78), 8, 0.22, 0.88)
        .cool(HUE_BLUE, 0.3)
        .warm(HUE_GOLD, 0.25)
        .build();
    let apex = (16.0f32, -6.0f32);
    let pleat = 34.0f32;
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - apex.0, c(y) - apex.1);
        let r = dx.hypot(dy);
        let a = dx.atan2(dy);
        if a.abs() > 0.46 {
            return None;
        }
        let wave = (a * pleat).cos();
        let edge = 28.5 + wave * 1.8;
        if !(13.5..=edge).contains(&r) {
            return None;
        }
        // Folds: crests lit, troughs in shade, brighter towards the left.
        let mut t = 0.48 + wave * 0.2 - a * 0.25;
        // The rim along the top: thick and darker, with a lit lip.
        if r < 16.5 {
            t = if r < 14.5 { 0.62 } else { 0.32 } - a * 0.2;
        } else if edge - r < 1.3 {
            t += 0.18;
        }
        // Leathery mottling.
        if r > 17.0 && value(seed("mantle"), 4, 4, c(x) * 1.4, c(y) * 1.4) > 0.68 {
            t -= 0.1;
        }
        // Thin veins running down the crests.
        if r > 17.0 && (a * pleat).sin().abs() < 0.12 {
            t -= 0.12;
        }
        Some(skin.pick(t, x, y))
    });
    icon.finish()
}

// --------------------------------------------------------------------- eyes

#[derive(Clone, Copy)]
enum Pupil {
    /// A grazer's: a wide horizontal bar.
    Bar,
    /// A pelter's: a narrow vertical slit.
    Slit,
    /// A dasher's: round.
    Round,
}

/// An eye: a ball, the iris turned a little to the top left, its pupil, and a bright catchlight.
fn eye(sclera: u32, iris: &Ramp, iris_r: f32, pupil: Pupil, name: &str) -> Tex {
    let salt = seed(name);
    let white = ramp(sclera, 0.42, 0.98);
    let (cx, cy, r) = (16.0f32, 16.0f32, 12.0f32);
    let (ix, iy) = (cx - 1.0, cy - 1.0);
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        let (dx, dy) = (px - cx, py - cy);
        let d = dx.hypot(dy);
        if d > r {
            return None;
        }
        let (ex, ey) = (px - ix, py - iy);
        let e = ex.hypot(ey);
        if e > iris_r {
            let mut t = 0.7 + lit(dx, dy, r) * 0.26 - (d / r).powi(3) * 0.2;
            // A few red veins near the rim.
            let a = dy.atan2(dx);
            if d > r * 0.72
                && (unit(hash2(salt, ((a * 5.0).floor() as i32 + 20) as u32)) > 0.6)
                && (a * 23.0).sin().abs() < 0.15
            {
                return Some(Rgba::hex(0xd8707a));
            }
            if e < iris_r + 0.9 {
                t -= 0.2;
            }
            return Some(white.pick(t, x, y));
        }
        // The pupil.
        let inside = match pupil {
            Pupil::Bar => ((ex / 4.4).powi(4) + (ey / 1.7).powi(4)) < 1.0,
            Pupil::Slit => ex.abs() < 1.3 * (1.0 - (ey / (iris_r - 0.6)).powi(2)).max(0.0).sqrt(),
            Pupil::Round => e < iris_r * 0.42,
        };
        if inside {
            return Some(Rgba::hex(0x120c18));
        }
        // Iris: dark limbal ring, radial streaks, brighter towards the
        // lower right where light gathers.
        let a = ey.atan2(ex);
        let streak = (unit(hash2(salt, ((a * 3.2).floor() as i32 + 40) as u32)) - 0.5) * 0.18;
        let mut t = 0.42 + streak - lit(ex, ey, iris_r) * 0.25 + (1.0 - e / iris_r) * 0.12;
        if e > iris_r - 1.1 {
            t = 0.08;
        }
        Some(iris.pick(t, x, y))
    });
    // The catchlight.
    icon.layer(false, |x, y| {
        let (dx, dy) = (c(x) - (ix - iris_r * 0.4), c(y) - (iy - iris_r * 0.45));
        (dx.hypot(dy) < 1.5).then(|| Rgba::hex(0xffffff))
    });
    icon.finish()
}

fn grazer_eye() -> Tex {
    let amber = RampSpec::new(Rgba::hex(0xd88a28), 8, 0.2, 0.9)
        .cool(HUE_PURPLE, 0.3)
        .warm(HUE_GOLD, 0.3)
        .build();
    eye(0xfaf2e6, &amber, 6.8, Pupil::Bar, "grazer_eye")
}

fn pelter_eye() -> Tex {
    let green = RampSpec::new(Rgba::hex(0x6ab83a), 8, 0.16, 0.92)
        .cool(HUE_BLUE, 0.35)
        .warm(HUE_GOLD, 0.45)
        .build();
    eye(0xd8d8a0, &green, 9.2, Pupil::Slit, "pelter_eye")
}

fn dasher_eye() -> Tex {
    let yellow = RampSpec::new(Rgba::hex(0xf6cc20), 8, 0.3, 0.97)
        .cool(HUE_PURPLE, 0.25)
        .warm(HUE_GOLD, 0.1)
        .build();
    eye(0x2c2c44, &yellow, 9.6, Pupil::Round, "dasher_eye")
}

// ----------------------------------------------------------------- trophies

/// A gold band round a tube at `s0..s1` of a stroke, for every trophy.
fn gold_band(s: f32, k: f32, s0: f32, s1: f32, x: usize, y: usize) -> Option<Rgba> {
    (s0..s1).contains(&s).then(|| {
        gold_ramp().pick(
            0.5 + k * 0.4 + if (s - s0) < 0.25 * (s1 - s0) { 0.15 } else { 0.0 },
            x,
            y,
        )
    })
}

/// Horn trophy: a ridged horn curling round on itself, ivory at its gold
/// banded base darkening to its tip.
fn horn_trophy() -> Tex {
    let horn = RampSpec::new(Rgba::hex(0xe4cc98), 8, 0.3, 0.97)
        .cool(HUE_PURPLE, 0.3)
        .warm(HUE_GOLD, 0.2)
        .build();
    let tip = Rgba::hex(0x3a2420);
    let (cx, cy) = (16.5f32, 15.5f32);
    let path = Stroke::new(140, |s| {
        let a = 2.35 + s * 4.9;
        let r = 11.5 - s * 7.0;
        (cx + a.cos() * r, cy + a.sin() * r)
    });
    let mut icon = Icon::new();
    path.draw(
        &mut icon,
        true,
        |s| 5.2 * (1.0 - s) + 0.6,
        |s, k, x, y| {
            if let Some(g) = gold_band(s, k, 0.0, 0.07, x, y) {
                return Some(g);
            }
            let mut t = 0.52 + k * 0.34;
            // Growth ridges across it.
            if (s * 16.0).fract() < 0.22 {
                t -= 0.18;
            }
            let col = horn.pick(t, x, y);
            Some(col.mix(tip, ((s - 0.45) / 0.55).clamp(0.0, 1.0).powf(1.3) * 0.8))
        },
    );
    icon.finish()
}

/// Scale trophy: one large scale, iridescent from violet through teal to
/// gold, ridged in growth lines round a lit keel, hung from a gold clasp.
fn scale_trophy() -> Tex {
    // Iridescent: violet in the shade, teal through the middle, gold where
    // the light catches it.
    let shimmer = Ramp::from_hex(&[
        0x221448, 0x3a2484, 0x3a48b0, 0x2a7eb4, 0x28aaa8, 0x52cc9c, 0xa8e294, 0xf4f0b4,
    ]);
    let gold = gold_ramp();
    let (cx, top, bottom) = (16.0f32, 4.0f32, 29.0f32);
    let mid = 17.0f32;
    let half = 11.5f32;
    let width = move |py: f32| {
        if py < mid {
            half * ((py - top) / (mid - top)).clamp(0.0, 1.0).powf(0.7)
        } else {
            half * (1.0 - ((py - mid) / (bottom - mid)).powi(2)).max(0.0).sqrt()
        }
    };
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        let w = width(py);
        let dx = px - cx;
        if dx.abs() > w || py < top || py > bottom {
            return None;
        }
        // Two faces either side of the keel: left in the light.
        let mut t = 0.55 - dx / half * 0.25 - (py - mid) / 25.0 * 0.15;
        if dx.abs() < 0.8 {
            t += 0.25;
        }
        // Growth lines: arcs round the attached top.
        let g = (dx * 0.8).hypot(py - top);
        if (g / 4.0).fract() < 0.18 && g > 5.0 {
            t -= 0.16;
        }
        if dx.abs() > w - 1.0 {
            t += if dx < 0.0 { 0.15 } else { -0.15 };
        }
        // A sheen sweeping across it diagonally.
        if ((dx + py * 0.6) - 11.0).abs() < 1.3 {
            t += 0.22;
        }
        Some(shimmer.pick(t, x, y))
    });
    // The gold clasp at the top.
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - cx, c(y) - 5.0);
        let r = (dx / 3.4).hypot(dy / 2.6);
        (r < 1.0).then(|| gold.pick(0.55 + lit(dx, dy, 3.0) * 0.4, x, y))
    });
    icon.finish()
}

/// Feather trophy: a long plume curving from the bottom left, white down at
/// its foot, teal vanes and a crest of flame orange at its tip, its quill
/// bound in gold.
fn feather_trophy() -> Tex {
    let teal = RampSpec::new(Rgba::hex(0x2a9ab8), 8, 0.2, 0.92)
        .cool(HUE_PURPLE, 0.3)
        .warm(HUE_GOLD, 0.3)
        .build();
    let crest = RampSpec::new(Rgba::hex(0xf06a28), 8, 0.3, 0.96)
        .cool(HUE_PURPLE, 0.35)
        .warm(HUE_GOLD, 0.35)
        .build();
    let down = ramp(0xeef0f4, 0.6, 0.99);
    let quill = ramp(0xf0e4c8, 0.5, 0.97);
    let path = Stroke::new(80, |s| {
        let (x0, y0, x1, y1, kx, ky) = (4.5f32, 28.0f32, 27.5f32, 4.0f32, 10.0f32, 6.0f32);
        let i = 1.0 - s;
        (
            i * i * x0 + 2.0 * i * s * kx + s * s * x1,
            i * i * y0 + 2.0 * i * s * ky + s * s * y1,
        )
    });
    let mut icon = Icon::new();
    // The vanes.
    path.draw(
        &mut icon,
        true,
        |s| {
            if s < 0.1 {
                0.9
            } else {
                1.2 + 5.2 * (PI * ((s - 0.1) / 0.92).powf(0.8)).sin().max(0.0)
            }
        },
        |s, k, x, y| {
            let barb = ((s * 50.0 + k.abs() * 3.0).fract()) < 0.3;
            let mut t = 0.52 + k * 0.28 + if barb { -0.14 } else { 0.0 };
            if s < 0.24 {
                return Some(down.pick(t + 0.1, x, y));
            }
            if s > 0.72 {
                // The crest: an eye of flame orange ringed in dark.
                let ring = (s - 0.72).abs() < 0.035 || k.abs() > 0.8 && s > 0.72;
                if ring {
                    return Some(teal.pick(0.12, x, y));
                }
                t += 0.1;
                return Some(crest.pick(t, x, y));
            }
            Some(teal.pick(t, x, y))
        },
    );
    // The quill down the middle, bound in gold at its foot.
    path.draw(
        &mut icon,
        false,
        |s| if s < 0.16 { 1.3 } else { 0.7 },
        |s, k, x, y| {
            if let Some(g) = gold_band(s, k, 0.06, 0.16, x, y) {
                return Some(g);
            }
            Some(quill.pick(0.7 + k * 0.2, x, y))
        },
    );
    icon.finish()
}

// -------------------------------------------------------------------- slings

#[derive(Clone, Copy, PartialEq)]
enum Frame {
    Stone,
    Copper,
    Iron,
}

/// A sling along the tool diagonal: a grip at the bottom left, a fork
/// opening to the top right, a band drawn back from its tips to a leather
/// pouch holding a pebble. Stone: a forked stick with a stone wedge lashed
/// in the crotch with green withies and leaves. Copper: a slim cast frame
/// with curled tips, quick. Iron: a thick riveted frame, heavy.
fn sling(frame: Frame) -> Tex {
    let salt = seed(match frame {
        Frame::Stone => "stone_sling",
        Frame::Copper => "copper_sling",
        Frame::Iron => "iron_sling",
    });
    let (ox, oy) = (6.5f32, 25.5f32);
    let xy = move |u: f32, v: f32| (ox + (u + v) * FRAC_1_SQRT_2, oy + (v - u) * FRAC_1_SQRT_2);
    let uv = move |px: f32, py: f32| {
        let (dx, dy) = (px - ox, py - oy);
        ((dx - dy) * FRAC_1_SQRT_2, (dx + dy) * FRAC_1_SQRT_2)
    };
    let (body, arm_w, grip_w) = match frame {
        Frame::Stone => (plank_ramp(), 1.5f32, 1.6f32),
        Frame::Copper => (copper_ramp(), 1.1, 1.3),
        Frame::Iron => (steel_ramp(), 1.9, 2.0),
    };
    let spread = if frame == Frame::Iron { 6.0 } else { 6.5 };
    let crotch = 12.0f32;
    let tip_u = 25.0f32;
    let leather = leather_ramp();
    let band = match frame {
        Frame::Stone => cord_ramp(),
        _ => Ramp::from_hex(&[0x2a1418, 0x4a2024, 0x6a3030, 0x8a4a40, 0xa86650]),
    };
    let mut icon = Icon::new();
    // The far band, from the upper tip back to the pouch.
    let pouch_u = 19.0f32;
    let tips = [(tip_u, -spread), (tip_u, spread)];
    for tip in tips {
        let (a, b) = (xy(tip.0, tip.1), xy(pouch_u + 1.0, 0.0));
        icon.layer(false, |x, y| {
            (seg_dist(a, b, (c(x), c(y))) < 0.75).then(|| band.pick(if (x + y) % 3 == 0 { 0.7 } else { 0.45 }, x, y))
        });
    }
    // The grip and handle.
    icon.layer(true, |x, y| {
        let (u, v) = uv(c(x), c(y));
        let end = if frame == Frame::Iron { -1.5 } else { -0.5 };
        if u < end || u > crotch + 1.0 || v.abs() > grip_w {
            return None;
        }
        let side = -v / grip_w;
        if frame == Frame::Iron && u < 1.0 {
            // A heavy pommel.
            return Some(body.pick(0.5 + side * 0.3 + 0.1, x, y));
        }
        if frame != Frame::Stone && (1.0..9.5).contains(&u) {
            let wrap = ((u * FRAC_1_SQRT_2 * 2.0).round() as i32) % 3 == 2;
            return Some(leather.pick(0.5 + side * 0.32 - if wrap { 0.24 } else { 0.0 }, x, y));
        }
        let clump = unit(hash2(salt, (u * 0.6).floor() as i32 as u32)) - 0.5;
        let grain = if frame == Frame::Stone { clump * 0.14 } else { 0.0 };
        Some(body.pick(0.52 + side * 0.32 + grain, x, y))
    });
    // The two arms of the fork.
    for sign in [-1.0f32, 1.0] {
        let arm = Stroke::new(30, |s| {
            let u = crotch + s * (tip_u - crotch);
            let k = (s * 1.4).min(1.0);
            let v = sign * spread * (k * (2.0 - k));
            xy(u, v)
        });
        let curl = if frame == Frame::Copper { 0.6 } else { 0.0 };
        let w = move |s: f32| arm_w + if s > 0.92 { curl } else { 0.0 };
        arm.draw(&mut icon, true, w, |s, k, x, y| {
            let mut t = 0.5 + k * 0.34;
            match frame {
                Frame::Stone => t += (unit(hash2(salt, (s * 8.0) as u32 + 50)) - 0.5) * 0.14,
                Frame::Copper | Frame::Iron => {
                    if k > 0.35 && (0.15..0.85).contains(&s) {
                        t += 0.2;
                    }
                }
            }
            if s > 0.9 {
                t += 0.12;
            }
            Some(body.pick(t, x, y))
        });
    }
    match frame {
        Frame::Stone => {
            // A knapped stone wedge in the crotch, lashed with withies, and
            // two leaves tied in.
            let (sx, sy) = xy(crotch + 1.0, 0.0);
            Lump::new(salt, sx, sy, 3.6, 5).draw(&mut icon, &stone_ramp(), 0.3, 0.92, false);
            let vine = vine_ramp();
            icon.layer(true, |x, y| {
                let (u, v) = uv(c(x), c(y));
                if (u - (crotch - 1.5)).abs() > 1.6 || v.abs() > 2.2 {
                    return None;
                }
                let turn = ((x + y) / 2) % 2 == 0;
                Some(vine.pick(0.55 - v / 2.2 * 0.25 + if turn { 0.15 } else { -0.2 }, x, y))
            });
            for (lu, lv, dir) in [(crotch - 2.5, 2.2, 1.0f32), (crotch - 4.0, -2.0, -1.0)] {
                let leaf = Stroke::new(12, |s| xy(lu - s * 3.5, lv + dir * s * 4.0));
                leaf.draw(
                    &mut icon,
                    true,
                    |s| 0.6 + 1.6 * (PI * s).sin(),
                    |_, k, x, y| Some(vine.pick(0.55 + k * 0.3, x, y)),
                );
            }
        }
        Frame::Copper => {}
        Frame::Iron => {
            // A riveted plate across the crotch.
            let steel = steel_ramp();
            icon.layer(true, |x, y| {
                let (u, v) = uv(c(x), c(y));
                if (u - crotch).abs() > 1.8 || v.abs() > 3.2 {
                    return None;
                }
                if (u - crotch).abs() < 0.6 && (v.abs() - 1.8).abs() < 0.6 {
                    return Some(steel.lightest());
                }
                Some(steel.pick(0.5 - v / 3.2 * 0.3, x, y))
            });
        }
    }
    // The near band and the pouch with its pebble.
    let (pa, pb) = (xy(tips[1].0, tips[1].1), xy(pouch_u + 1.0, 0.0));
    icon.layer(false, |x, y| {
        (seg_dist(pa, pb, (c(x), c(y))) < 0.75).then(|| band.pick(if (x + y) % 3 == 0 { 0.75 } else { 0.5 }, x, y))
    });
    icon.layer(true, |x, y| {
        let (u, v) = uv(c(x), c(y));
        let d = ((u - pouch_u) / 2.2).hypot(v / 3.2);
        if d > 1.0 {
            return None;
        }
        let pebble = (u - (pouch_u + 0.6)).hypot(v) < 1.6;
        if pebble {
            return Some(stone_ramp().pick(0.7 - v * 0.15, x, y));
        }
        Some(leather.pick(0.55 - v / 3.2 * 0.3, x, y))
    });
    icon.finish()
}

/// Paints the creature part's or sling's icon called `name`, if it is one.
pub fn paint(name: &str) -> Option<Tex> {
    Some(match name {
        "raw_meat" => raw_meat(),
        "lean_meat" => lean_meat(),
        "bone" => bone(),
        "tallow" => tallow(),
        "hide" => hide(),
        "fresh_blood" => fresh_blood(),
        "thick_blood" => thick_blood(),
        "milk_gland" => milk_gland(),
        "quick_gland" => quick_gland(),
        "spark_sac" => spark_sac(),
        "mantle" => mantle(),
        "grazer_eye" => grazer_eye(),
        "pelter_eye" => pelter_eye(),
        "dasher_eye" => dasher_eye(),
        "horn_trophy" => horn_trophy(),
        "scale_trophy" => scale_trophy(),
        "feather_trophy" => feather_trophy(),
        "vital_essence" => vital_essence(),
        "stone_sling" => sling(Frame::Stone),
        "copper_sling" => sling(Frame::Copper),
        "iron_sling" => sling(Frame::Iron),
        _ => return None,
    })
}
