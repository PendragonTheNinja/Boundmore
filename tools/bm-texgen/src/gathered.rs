//! Icons for the crafting chain's materials and what gathering objects drop
//! (milestone 11): stones, sap and glue, sackcloth, relic fragments, the
//! leaves, petals, beans and tubers of plants, the boulders' minerals, and
//! the fungi's lamellae and orbs.
//!
//! Same family as `icons`: a sprite on a clear ground with a dark outline,
//! light from the top left, chunky shapes that read at 16 x 16. Things of
//! one kind share a composition (three beans in a cluster, a lamella fan
//! from the bottom left, an orb in the middle) and differ by colour and a
//! mark of their own, so a pack of them reads as a set.

use crate::color::{HUE_BLUE, HUE_GOLD, HUE_PURPLE, Ramp, RampSpec, Rgba};
use crate::icons::{Icon, Lump, P, blade, inside_poly, seg_dist};
use crate::machines::{relic_light_ramp, relic_ramp};
use crate::materials::stone_ramp;
use crate::noise::{hash2, seed, unit, value};
use crate::tex::{Tex, c, grid};

pub(crate) fn ramp(rgb: u32, dark: f32, light: f32) -> Ramp {
    RampSpec::new(Rgba::hex(rgb), 7, dark, light)
        .cool(HUE_PURPLE, 0.3)
        .warm(HUE_GOLD, 0.25)
        .build()
}

/// A shaded ellipse turned by `turn`, lit from the top left, with a small
/// highlight. `bump` roughens its outline (tubers), `gloss` adds a sheen.
#[allow(clippy::too_many_arguments)]
pub(crate) fn blob(icon: &mut Icon, centre: P, r: P, turn: f32, ramp: &Ramp, bump: f32, gloss: bool, salt: u32) {
    let (s, co) = turn.sin_cos();
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - centre.0, c(y) - centre.1);
        let (u, v) = (dx * co + dy * s, -dx * s + dy * co);
        let a = v.atan2(u);
        let rough = 1.0 + bump * ((a * 3.0 + unit(salt) * 6.0).sin() * 0.6 + (a * 5.0).cos() * 0.4);
        let d = (u / r.0).hypot(v / r.1) / rough;
        if d > 1.0 {
            return None;
        }
        let lit = -(dx * 0.6 + dy * 0.8) / r.0.max(r.1);
        let mut t = 0.5 + lit * 0.42 - d * d * 0.16;
        if gloss {
            let (hx, hy) = (centre.0 - r.0 * 0.38, centre.1 - r.1 * 0.42);
            if (c(x) - hx).hypot(c(y) - hy) < r.0.min(r.1) * 0.28 {
                t = 1.0;
            }
        }
        Some(ramp.pick(t, x, y))
    });
}

// ---------------------------------------------------------------- materials

/// Stones: a little heap of four knapped cobbles.
fn stones() -> Tex {
    let salt = seed("stones");
    let r = stone_ramp();
    let mut icon = Icon::new();
    for (i, &(x, y, rad)) in [
        (10.5, 21.5, 6.6),
        (22.0, 22.0, 6.2),
        (16.0, 13.0, 6.0),
        (16.5, 25.5, 4.8),
    ]
    .iter()
    .enumerate()
    {
        Lump::new(hash2(salt, i as u32), x, y, rad, 6).draw(&mut icon, &r, 0.3, 0.96, false);
    }
    icon.finish()
}

/// Sap: a fat drop of amber, pointed at the top, a bright glint in it.
fn sap() -> Tex {
    let amber = ramp(0xd88a1c, 0.32, 0.95);
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 19.0);
        let r = if dy > 0.0 {
            9.0
        } else {
            9.0 * (1.0 + dy / 15.0).max(0.0)
        };
        if dx.hypot(dy.max(0.0)) > r || dy < -15.0 {
            return None;
        }
        // Lit inside on the lower right, as amber is; a darker rim.
        let edge = (dx.abs() / r.max(0.1)).min(1.0);
        let t = 0.42 + (dx + dy) / 18.0 * 0.3 - edge * 0.2 + 0.08;
        Some(amber.pick(t, x, y))
    });
    icon.layer(false, |x, y| {
        let (dx, dy) = (c(x) - 12.5, c(y) - 15.5);
        (dx.hypot(dy * 0.7) < 1.8).then(|| Rgba::hex(0xfff4d0))
    });
    icon.finish()
}

/// Glue: a squat clay pot, amber glue welling over its lip and running
/// down one side.
fn glue() -> Tex {
    let clay = ramp(0xa8603a, 0.3, 0.82);
    let amber = ramp(0xe0a030, 0.4, 0.96);
    let mut icon = Icon::new();
    // The pot: a rounded body under a rim.
    icon.layer(true, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let (dx, dy) = (fx - 16.0, fy - 20.0);
        let body = (dx / 10.5).hypot(dy / 8.5) < 1.0 && fy > 13.0;
        let rim = (12.0..15.0).contains(&fy) && dx.abs() < 9.0;
        if !body && !rim {
            return None;
        }
        let t = if rim {
            0.62 - (fy - 12.0) * 0.12
        } else {
            0.55 - dx / 10.5 * 0.3 - dy / 8.5 * 0.12
        };
        Some(clay.pick(t, x, y))
    });
    // The glue: a dome in the mouth, a drip down the left.
    icon.layer(true, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let dome = ((fx - 16.0) / 8.0).hypot((fy - 12.5) / 4.5) < 1.0 && fy < 14.0;
        let drip = (fx - 9.5).abs() < 1.6 && (12.0..21.0).contains(&fy) || (fx - 9.5).hypot(fy - 21.0) < 2.0;
        if !dome && !drip {
            return None;
        }
        let t = 0.62 - (fx - 16.0) / 10.0 * 0.2 - (fy - 12.0) / 10.0 * 0.15;
        Some(amber.pick(t, x, y))
    });
    icon.layer(false, |x, y| {
        let (dx, dy) = (c(x) - 13.0, c(y) - 10.5);
        (dx.hypot(dy) < 1.2).then(|| Rgba::hex(0xfff6d8))
    });
    icon.finish()
}

/// A two-by-two basket weave: each thread rounded across its width, lit in
/// its middle and shaded where it dives under its neighbour. A tone offset
/// of up to `depth` either way.
fn weave(p: P, depth: f32) -> f32 {
    let (i, j) = ((p.0 / 2.0).floor() as i32, (p.1 / 2.0).floor() as i32);
    let (fx, fy) = (p.0 / 2.0 - i as f32, p.1 / 2.0 - j as f32);
    let along_x = (i + j).rem_euclid(2) == 0;
    let across = if along_x { fy } else { fx };
    let round = (across * std::f32::consts::PI).sin();
    depth * (round - 0.55) + if along_x { depth * 0.3 } else { -depth * 0.3 }
}

/// Sackcloth: a folded square of coarse woven cloth, its weave showing,
/// one corner turned back and the edge frayed.
fn sackcloth() -> Tex {
    let cloth = ramp(0xb89a6a, 0.32, 0.88);
    let quad: [P; 4] = [(5.0, 9.0), (25.0, 5.0), (28.0, 25.0), (7.0, 28.0)];
    let fold: [P; 3] = [(25.0, 5.0), (28.0, 25.0), (19.0, 13.0)];
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&quad, p.0, p.1) {
            return None;
        }
        let mut t = 0.56 + weave(p, 0.2) - (p.0 + p.1 - 30.0) / 40.0 * 0.2;
        if seg_dist(quad[0], quad[1], p) < 1.0 || seg_dist(quad[3], quad[0], p) < 1.0 {
            t += 0.15;
        }
        Some(cloth.pick(t, x, y))
    });
    // The turned-back corner, its underside in shade.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        inside_poly(&fold, p.0, p.1).then(|| cloth.pick(0.3 + weave(p, 0.12), x, y))
    });
    // Frayed threads along the bottom edge.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let near = seg_dist(quad[2], quad[3], p);
        (near < 1.6 && !inside_poly(&quad, p.0, p.1) && (grid(x) * 7 + grid(y) * 3) % 4 == 0).then(|| cloth.at(4))
    });
    icon.finish()
}

/// Relic fragment: a broken plate of old bronze, a line of faint cyan
/// light cut across it and verdigris in its corner.
fn relic_fragment() -> Tex {
    let bronze = relic_ramp();
    let light = relic_light_ramp();
    let shard: [P; 6] = [
        (6.0, 12.0),
        (17.0, 4.5),
        (27.0, 9.0),
        (25.0, 20.0),
        (15.0, 28.0),
        (8.0, 22.0),
    ];
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&shard, p.0, p.1) {
            return None;
        }
        let mut t = 0.55 - (p.0 + p.1 - 32.0) / 40.0 * 0.4;
        let near_lit = seg_dist(shard[0], shard[1], p) < 1.2 || seg_dist(shard[1], shard[2], p) < 1.2;
        if near_lit {
            t += 0.28;
        }
        if seg_dist(shard[3], shard[4], p) < 1.2 {
            t -= 0.25;
        }
        // A bevelled groove with the light in it, and two studs.
        let line = seg_dist((8.0, 17.0), (25.0, 12.0), p);
        if line < 1.1 {
            return Some(light.pick(0.95 - line * 0.3, x, y));
        }
        if line < 1.9 {
            t -= 0.3;
        }
        for s in [(13.0, 9.5), (19.0, 22.0)] {
            let d = (p.0 - s.0).hypot(p.1 - s.1);
            if d < 1.6 {
                t = if p.0 + p.1 < s.0 + s.1 { 0.95 } else { 0.35 };
            }
        }
        let mut col = bronze.pick(t, x, y);
        if value(seed("relic_fragment"), 4, 4, p.0, p.1) > 0.66 && line > 2.0 {
            col = col.mix(Rgba::hex(0x5aa08a), 0.6);
        }
        Some(col)
    });
    icon.finish()
}

// ------------------------------------------------------------------ leaves

/// A leaf from its stalk at the bottom left to its tip at the top right,
/// `width` at its widest; `fibres` draws long parallel lines, else veins
/// branch from the midrib.
fn leaf(icon: &mut Icon, ramp: &Ramp, width: f32, round: f32, fibres: bool) {
    let (foot, tip) = ((5.0f32, 27.0f32), (27.0f32, 5.0f32));
    let (ax, ay) = ((tip.0 - foot.0) / 31.1, (tip.1 - foot.1) / 31.1);
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - foot.0, c(y) - foot.1);
        let u = (dx * ax + dy * ay) / 31.1;
        let v = -dx * ay + dy * ax;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let w = width * (std::f32::consts::PI * u.powf(round)).sin().max(0.0);
        if v.abs() > w || w < 0.6 {
            return None;
        }
        // The upper-left half faces the light.
        let mut t = 0.5 - v / w.max(1.0) * 0.22 + u * 0.1;
        if v.abs() < 0.6 {
            t += 0.24;
        } else if fibres {
            if (v * 1.4).fract().abs() < 0.3 {
                t += 0.12;
            }
        } else {
            let vein = ((u * 31.1 - v.abs() * 0.9) / 4.0).fract();
            if vein < 0.16 {
                t += 0.14;
            }
        }
        if v.abs() > w - 0.9 {
            t -= 0.12;
        }
        Some(ramp.pick(t, x, y))
    });
    // A short stalk.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        (seg_dist((3.0, 29.0), (6.5, 25.5), p) < 1.1).then(|| ramp.pick(0.3, x, y))
    });
}

fn fibrous_leaf() -> Tex {
    let mut icon = Icon::new();
    leaf(&mut icon, &ramp(0xa8b45a, 0.32, 0.9), 4.2, 0.9, true);
    icon.finish()
}

fn inky_leaf() -> Tex {
    let r = RampSpec::new(Rgba::hex(0x3a2a5a), 7, 0.18, 0.7)
        .cool(HUE_BLUE, 0.4)
        .warm(HUE_PURPLE, 0.3)
        .build();
    let mut icon = Icon::new();
    leaf(&mut icon, &r, 8.0, 0.75, false);
    icon.finish()
}

/// Fleshy leaf: a thick wedge cut from a succulent, its cut end showing
/// pale wet flesh, pale flecks on its skin.
fn fleshy_leaf() -> Tex {
    let skin = ramp(0x6ea078, 0.3, 0.86);
    let flesh = ramp(0xc8e4b0, 0.6, 0.97);
    let mut icon = Icon::new();
    let wedge: [P; 4] = [(4.0, 20.0), (27.0, 4.0), (24.0, 14.0), (8.0, 28.0)];
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&wedge, p.0, p.1) {
            return None;
        }
        let mut t = 0.52 - (p.0 + p.1 - 30.0) / 40.0 * 0.2;
        if seg_dist(wedge[0], wedge[1], p) < 1.4 {
            t += 0.25;
        }
        if seg_dist(wedge[2], wedge[3], p) < 1.2 {
            t -= 0.2;
        }
        if unit(hash2(seed("fleshy_leaf"), (grid(x) / 2 + grid(y) / 2 * 16) as u32)) > 0.86 {
            t += 0.3;
        }
        Some(skin.pick(t, x, y))
    });
    // The cut end, an oval at the foot.
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - 6.5, c(y) - 24.0);
        let (u, v) = (dx * 0.8 + dy * 0.6, -dx * 0.6 + dy * 0.8);
        ((u / 2.6).hypot(v / 4.8) < 1.0).then(|| flesh.pick(0.6 - u * 0.08, x, y))
    });
    icon.finish()
}

/// Petal: one broad rounded petal, pink deepening to its base, veins fanning
/// up from it.
fn petal() -> Tex {
    let r = ramp(0xe07aa8, 0.42, 0.95);
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 27.0);
        let up = -dy;
        if !(0.0..24.0).contains(&up) {
            return None;
        }
        let k = up / 24.0;
        let w = 11.0 * k.powf(0.9).min(1.0) * (1.0 - k.powi(5)).max(0.0).sqrt() + 1.2;
        if dx.abs() > w {
            return None;
        }
        let mut t = 0.38 + up / 24.0 * 0.4 - dx / 12.0 * 0.15;
        let fan = (dx / up.max(1.0) * 6.0).fract().abs();
        if fan < 0.12 && up > 3.0 {
            t -= 0.12;
        }
        if dx.abs() > w - 1.0 {
            t -= 0.1;
        }
        // A notch at the tip.
        if up > 21.5 && dx.abs() < 1.2 {
            return None;
        }
        Some(r.pick(t, x, y))
    });
    icon.finish()
}

// ------------------------------------------------------- beans and tubers

/// Three beans in a cluster, each a kidney shape with a pale eye.
fn beans(rgb: u32, eye: u32, dark: f32, name: &str) -> Tex {
    let r = ramp(rgb, dark, 0.94);
    let salt = seed(name);
    let mut icon = Icon::new();
    for (i, &(x, y, turn)) in [(11.0f32, 12.0f32, 0.6f32), (21.5, 14.0, -0.5), (15.5, 23.0, 0.1)]
        .iter()
        .enumerate()
    {
        let (s, co) = turn.sin_cos();
        icon.layer(true, |px, py| {
            let (dx, dy) = (c(px) - x, c(py) - y);
            let (u, v) = (dx * co + dy * s, -dx * s + dy * co);
            // A kidney: an ellipse with a dent in its inner side.
            let d = (u / 7.2).hypot(v / 4.6);
            let dent = (-(u / 2.4).powi(2)).exp() * 1.6;
            if d > 1.0 || v > 4.6 - dent - 0.6 && v > 0.0 && d > 0.55 {
                return None;
            }
            let lit = -(dx * 0.6 + dy * 0.8) / 7.0;
            let t = 0.5 + lit * 0.4 - d * d * 0.12 + (unit(hash2(salt, i as u32)) - 0.5) * 0.08;
            if (u.abs() < 1.2) && (v - (4.6 - dent - 1.4)).abs() < 0.8 {
                return Some(Rgba::hex(eye));
            }
            if (u + 3.0).hypot(v + 2.0) < 1.1 {
                return Some(r.at(6));
            }
            Some(r.pick(t, px, py))
        });
    }
    icon.finish()
}

/// A tuber: a lumpy root with eyes, a root hair at one end.
fn tuber(rgb: u32, dark: f32, gloss: bool, stripes: bool, name: &str) -> Tex {
    let r = ramp(rgb, dark, 0.92);
    let salt = seed(name);
    let mut icon = Icon::new();
    blob(&mut icon, (16.5, 16.5), (12.5, 8.5), -0.55, &r, 0.06, gloss, salt);
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (dx, dy) = (p.0 - 16.5, p.1 - 16.5);
        let (s, co) = (-0.55f32).sin_cos();
        let (u, v) = (dx * co + dy * s, -dx * s + dy * co);
        if (u / 12.5).hypot(v / 8.5) > 0.9 {
            return None;
        }
        if stripes && (u * 0.5).rem_euclid(2.0) < 0.6 {
            return Some(r.at(1));
        }
        for e in [(-5.0f32, -2.0f32), (1.0, 3.0), (6.0, -1.5), (-1.5, -4.0)] {
            if (u - e.0).hypot(v - e.1) < 0.9 {
                return Some(r.at(1));
            }
        }
        None
    });
    // A root hair off the far end.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        (seg_dist((26.0, 9.5), (29.0, 4.5), p) < 0.8).then(|| r.at(2))
    });
    icon.finish()
}

// ---------------------------------------------------------------- minerals

/// Opal: a polished oval with flashes of colour across a milky ground.
fn opal() -> Tex {
    let milk = ramp(0xd8dcec, 0.55, 0.98);
    let flashes = [0x5ae0c8u32, 0xf07ab4, 0x7aa8ff, 0xf0d060];
    let salt = seed("opal");
    let mut icon = Icon::new();
    blob(&mut icon, (16.0, 16.5), (12.0, 9.5), -0.4, &milk, 0.0, false, salt);
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (dx, dy) = (p.0 - 16.0, p.1 - 16.5);
        let (s, co) = (-0.4f32).sin_cos();
        let (u, v) = (dx * co + dy * s, -dx * s + dy * co);
        let d = (u / 12.0).hypot(v / 9.5);
        if d > 0.86 {
            return None;
        }
        if (u + 4.5).hypot(v + 4.0) < 1.6 {
            return Some(Rgba::hex(0xffffff));
        }
        let n = value(salt, 4, 4, p.0, p.1);
        let k = ((n * 4.0) as usize).min(3);
        let band = (n * 4.0).fract();
        (band > 0.45 && band < 0.8).then(|| Rgba::hex(flashes[k]).mix(milk.at(6), d * 0.4))
    });
    icon.finish()
}

/// Isometric cubes of crystal, for rock salt.
fn cubes(rgb: u32) -> Tex {
    let r = ramp(rgb, 0.45, 0.97);
    let mut icon = Icon::new();
    for &(cx, cy, s) in &[(11.0f32, 20.0f32, 6.5f32), (21.0, 21.0, 5.5), (16.0, 11.5, 6.0)] {
        // Top, left and right faces of a cube seen from above and the left.
        let top = [(cx, cy - s), (cx + s, cy - s * 0.5), (cx, cy), (cx - s, cy - s * 0.5)];
        let left = [(cx - s, cy - s * 0.5), (cx, cy), (cx, cy + s), (cx - s, cy + s * 0.5)];
        let right = [(cx, cy), (cx + s, cy - s * 0.5), (cx + s, cy + s * 0.5), (cx, cy + s)];
        icon.layer(true, |x, y| {
            let p = (c(x), c(y));
            if inside_poly(&top, p.0, p.1) {
                Some(r.pick(0.9, x, y))
            } else if inside_poly(&left, p.0, p.1) {
                Some(r.pick(0.62, x, y))
            } else if inside_poly(&right, p.0, p.1) {
                Some(r.pick(0.34, x, y))
            } else {
                None
            }
        });
    }
    icon.finish()
}

/// A cluster of cut gem lumps with glints (olivine, sulphur, cobalt).
fn gems(rgb: u32, dark: f32, name: &str, sides: usize) -> Tex {
    let r = RampSpec::new(Rgba::hex(rgb), 8, dark, 0.97)
        .cool(HUE_PURPLE, 0.25)
        .warm(HUE_GOLD, 0.3)
        .build();
    let salt = seed(name);
    let mut icon = Icon::new();
    for (i, &(x, y, rad)) in [(12.0, 13.0, 7.5), (21.5, 19.5, 7.0), (11.5, 23.5, 5.2)]
        .iter()
        .enumerate()
    {
        Lump::new(hash2(salt, i as u32), x, y, rad, sides).draw(&mut icon, &r, 0.2, 0.98, true);
    }
    icon.finish()
}

/// Saltpetre: a spray of white needle crystals from a crust at their foot.
fn saltpetre() -> Tex {
    let r = RampSpec::new(Rgba::hex(0xeceae0), 7, 0.55, 0.99)
        .cool(HUE_BLUE, 0.5)
        .build();
    let up = -std::f32::consts::FRAC_PI_2;
    let mut icon = Icon::new();
    for (foot, ang, len, half) in [
        ((11.0, 27.0), up - 0.75, 15.0, 2.4),
        ((20.0, 27.0), up + 0.7, 15.0, 2.4),
        ((14.0, 28.0), up - 0.28, 22.0, 3.0),
        ((18.0, 28.0), up + 0.22, 20.0, 2.8),
        ((16.0, 28.5), up, 25.0, 3.0),
    ] {
        blade(&mut icon, foot, ang, len, half, &r);
    }
    let crust = ramp(0xb8b0a0, 0.4, 0.85);
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 28.5);
        ((dx / 10.0).hypot(dy / 3.0) < 1.0).then(|| crust.pick(0.6 - dy * 0.1, x, y))
    });
    icon.finish()
}

// ------------------------------------------------------------------ fungi

/// A lamella: a fan-shaped slice of gills from the bottom left, ridged, its
/// edge frilled.
fn lamella(rgb: u32, glow: bool, name: &str) -> Tex {
    let r = ramp(rgb, if glow { 0.45 } else { 0.32 }, 0.96);
    let salt = seed(name);
    let mut icon = Icon::new();
    let origin = (5.0f32, 27.0f32);
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - origin.0, c(y) - origin.1);
        let d = dx.hypot(dy);
        let a = (-dy).atan2(dx);
        if !(-0.1..1.67).contains(&a) {
            return None;
        }
        let frill = 22.0 + (a * 14.0).sin() * 1.2;
        if d > frill || d < 3.0 {
            return None;
        }
        let ridge = ((a * 9.0).sin() + 1.0) * 0.5;
        let mut t = 0.42 + d / frill * 0.3 + (ridge - 0.5) * 0.36;
        t += (unit(hash2(salt, (a * 9.0) as u32)) - 0.5) * 0.06;
        if d > frill - 1.4 {
            t += 0.18;
        }
        Some(r.pick(t, x, y))
    });
    if glow {
        icon.layer(false, |x, y| {
            let (dx, dy) = (c(x) - origin.0, c(y) - origin.1);
            let d = dx.hypot(dy);
            let a = (-dy).atan2(dx);
            let on = (-0.1..1.67).contains(&a) && d < 21.0 && d > 4.0;
            (on && (grid(x) * 5 + grid(y) * 3) % 11 == 0).then(|| Rgba::hex(0xf0fffc))
        });
    }
    icon.finish()
}

/// An orb: a sphere of its colour with a bright heart, a highlight on the
/// top left and a mark of its own.
#[derive(Clone, Copy, PartialEq)]
enum OrbMark {
    /// A bright red core, warm and alive.
    Core,
    /// Rings round the middle, as if beating.
    Rings,
    /// Dark, with a dim violet heart.
    Dim,
    /// Sparkles across it.
    Sparkle,
}

fn orb(rgb: u32, mark: OrbMark) -> Tex {
    let dark = if mark == OrbMark::Dim { 0.14 } else { 0.3 };
    let r = RampSpec::new(Rgba::hex(rgb), 8, dark, 0.97)
        .cool(HUE_PURPLE, 0.3)
        .warm(HUE_GOLD, 0.2)
        .build();
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 16.5);
        let d = dx.hypot(dy) / 12.5;
        if d > 1.0 {
            return None;
        }
        // Lit from inside: brightest a little below the middle.
        let heart = 1.0 - (dx.hypot(dy - 1.5) / 12.5);
        let mut t = 0.2 + heart * 0.62 - (dx + dy) / 25.0 * 0.12;
        match mark {
            OrbMark::Rings => {
                if ((d * 10.0).fract() - 0.5).abs() < 0.12 && d > 0.25 {
                    t += 0.2;
                }
            }
            OrbMark::Dim => t *= 0.7,
            OrbMark::Core => {
                if d < 0.3 {
                    t = 0.98;
                }
            }
            OrbMark::Sparkle => {}
        }
        Some(r.pick(t, x, y))
    });
    icon.layer(false, |x, y| {
        let (dx, dy) = (c(x) - 11.5, c(y) - 11.5);
        let d = dx.hypot(dy);
        if d < 1.8 {
            return Some(Rgba::hex(0xffffff));
        }
        if mark == OrbMark::Sparkle {
            for (sx, sy) in [(20.0f32, 13.0f32), (14.0, 21.5), (22.0, 22.0)] {
                let (ex, ey) = ((c(x) - sx).abs(), (c(y) - sy).abs());
                if (ex < 0.6 && ey < 2.1) || (ey < 0.6 && ex < 2.1) {
                    return Some(Rgba::hex(0xf4ffff));
                }
            }
        }
        None
    });
    icon.finish()
}

/// Paints the gathered material's icon called `name`, if it is one.
pub fn paint(name: &str) -> Option<Tex> {
    Some(match name {
        "stones" => stones(),
        "sap" => sap(),
        "glue" => glue(),
        "sackcloth" => sackcloth(),
        "relic_fragment" => relic_fragment(),
        "fibrous_leaf" => fibrous_leaf(),
        "inky_leaf" => inky_leaf(),
        "fleshy_leaf" => fleshy_leaf(),
        "petal" => petal(),
        "spicy_bean" => beans(0xc8382a, 0xf0d8a0, 0.3, name),
        "sweet_bean" => beans(0xf0d478, 0x8a5a30, 0.55, name),
        "bitter_bean" => beans(0x5a6a34, 0xd8d0a0, 0.28, name),
        "tuber" => tuber(0x9a6a40, 0.3, false, false, name),
        "waxy_tuber" => tuber(0xf0d890, 0.58, true, false, name),
        "exotic_tuber" => tuber(0x9a3a6a, 0.3, false, true, name),
        "opal" => opal(),
        "rock_salt" => cubes(0xf0c8c8),
        "olivine" => gems(0x8ab43a, 0.3, name, 6),
        "sulphur" => gems(0xf0dc40, 0.55, name, 5),
        "cobalt" => gems(0x3a5ac8, 0.22, name, 7),
        "saltpetre" => saltpetre(),
        "reactive_lamella" => lamella(0xe07a3a, false, name),
        "edible_lamella" => lamella(0xe4cca0, false, name),
        "glowing_lamella" => lamella(0x5ad8cc, true, name),
        "vital_orb" => orb(0xe0405a, OrbMark::Core),
        "pulsating_orb" => orb(0xf0a030, OrbMark::Rings),
        "shadow_orb" => orb(0x4a2a6a, OrbMark::Dim),
        "shimmering_orb" => orb(0x8ae8f0, OrbMark::Sparkle),
        _ => return None,
    })
}
