//! Food icons (milestone 16): berries, roasts, bowls of each dish, buckets,
//! nails, syrup and sugar. The icon rules of `icons.rs`: light from the top
//! left, a dark outline, chunky shapes that read at 16 x 16. Every dish is
//! the same wooden bowl, so a hotbar of meals reads as a set; what is in it
//! (its colour, broth or chunks) says which.

use crate::gathered::{blob, ramp};
use crate::icons::{Icon, P, seg_dist, speck};
use crate::materials::{plank_ramp, steel_ramp};
use crate::noise::{hash2, seed, unit};
use crate::tex::{Tex, c, grid, smoothstep};

/// Three berries on a stalk with a leaf: dewberries, raw or cooked.
fn berries(rgb: u32, cooked: bool, name: &str) -> Tex {
    let r = ramp(rgb, 0.3, 0.95);
    let leaf = ramp(if cooked { 0x6a6a30 } else { 0x4aa040 }, 0.3, 0.9);
    let salt = seed(name);
    let mut icon = Icon::new();
    // The stalk and a leaf behind the berries.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if seg_dist((16.0, 4.0), (16.5, 13.0), p) < 1.0 {
            return Some(leaf.at(1));
        }
        let (u, v) = (p.0 - 21.5, p.1 - 6.5);
        ((u / 5.5).hypot(v / 2.6) < 1.0).then(|| leaf.pick(0.55 - v * 0.05, x, y))
    });
    for (i, &(x, y, rad)) in [(10.5f32, 17.5f32, 6.0f32), (21.5, 18.0, 6.0), (16.0, 24.5, 6.2)]
        .iter()
        .enumerate()
    {
        blob(
            &mut icon,
            (x, y),
            (rad, rad),
            0.0,
            &r,
            0.0,
            !cooked,
            hash2(salt, i as u32),
        );
    }
    icon.finish()
}

/// A roast: a thick slab with grill marks; prime meat marbled, lean paler.
fn roast(rgb: u32, marbled: bool, name: &str) -> Tex {
    let r = ramp(rgb, 0.28, 0.92);
    let salt = seed(name);
    let mut icon = Icon::new();
    blob(&mut icon, (16.0, 17.0), (12.5, 9.0), -0.35, &r, 0.08, false, salt);
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (dx, dy) = (p.0 - 16.0, p.1 - 17.0);
        let (s, co) = (-0.35f32).sin_cos();
        let (u, v) = (dx * co + dy * s, -dx * s + dy * co);
        if (u / 12.5).hypot(v / 9.0) > 0.85 {
            return None;
        }
        // Grill marks across, then marbling.
        if (u + v * 0.9).rem_euclid(6.0) < 1.1 {
            return Some(r.at(0));
        }
        if marbled && unit(hash2(salt, ((u * 0.7) as i32 * 31 + (v * 1.3) as i32) as u32)) > 0.8 {
            return Some(r.at(r.len() - 1));
        }
        None
    });
    // A bone end on the left.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let bone = ramp(0xe8dcc0, 0.5, 0.98);
        (seg_dist((3.5, 21.5), (7.0, 19.5), p) < 1.8).then(|| bone.pick(0.6, x, y))
    });
    icon.finish()
}

/// A roasted tuber: the raw tuber's shape, darker, with charred patches.
fn roasted_tuber(rgb: u32, name: &str) -> Tex {
    let r = ramp(rgb, 0.25, 0.85);
    let salt = seed(name);
    let mut icon = Icon::new();
    blob(&mut icon, (16.5, 16.5), (12.5, 8.5), -0.55, &r, 0.06, false, salt);
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (dx, dy) = (p.0 - 16.5, p.1 - 16.5);
        let (s, co) = (-0.55f32).sin_cos();
        let (u, v) = (dx * co + dy * s, -dx * s + dy * co);
        if (u / 12.5).hypot(v / 8.5) > 0.88 {
            return None;
        }
        // A split along it, showing the soft inside, and char spots.
        if v.abs() < 1.0 && u.abs() < 7.0 {
            return Some(ramp(0xf0d890, 0.6, 1.0).pick(0.7, x, y));
        }
        (unit(hash2(salt, ((u * 0.5) as i32 * 17 + (v * 0.5) as i32) as u32)) > 0.86).then(|| r.at(0))
    });
    icon.finish()
}

/// What fills a bowl.
#[derive(Clone, Copy)]
enum Fill {
    /// A smooth broth, with a few bits floating.
    Broth,
    /// Chunks heaped over the rim.
    Chunks,
    /// A smooth domed porridge with a swirl and a few bits.
    Porridge,
    /// A dome of rice grains, each one showing, with bits through it.
    Risotto,
    /// Raw: pale grain on one side, the ingredients on the other, unmixed.
    Raw,
}

/// The shared bowl, with `fill` in `rgb` and, for some, a second colour of
/// bits (herbs, berries).
fn bowl(rgb: u32, fill: Fill, bits: u32, name: &str) -> Tex {
    bowl_icon(rgb, fill, bits, name).finish()
}

/// `bowl`, not yet outlined, so a garnish can be laid on top.
fn bowl_icon(rgb: u32, fill: Fill, bits: u32, name: &str) -> Icon {
    let wood = plank_ramp();
    let food = ramp(rgb, 0.3, 0.95);
    let bit = ramp(bits, 0.3, 0.95);
    let salt = seed(name);
    let mut icon = Icon::new();
    // The bowl: the lower half of an ellipse with a rim band.
    let (cx, top, w, h) = (16.0f32, 15.0f32, 14.0f32, 12.0f32);
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (u, v) = ((p.0 - cx) / w, (p.1 - top) / h);
        if v < 0.0 || u.hypot(v) > 1.0 {
            return None;
        }
        let t = 0.62 - u * 0.35 - v * 0.3;
        Some(if v < 0.14 {
            wood.pick(t + 0.15, x, y)
        } else {
            wood.pick(t, x, y)
        })
    });
    // A foot.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        ((p.0 - cx).abs() < 5.0 && (27.0..29.5).contains(&p.1)).then(|| wood.pick(0.35, x, y))
    });
    // The surface: an ellipse at the rim, or a heap.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let (u, v) = ((p.0 - cx) / (w - 1.5), (p.1 - top) / 3.4);
        let inside = match fill {
            Fill::Broth => u.hypot(v) < 1.0,
            Fill::Chunks => {
                let heap = (p.1 - (top + 1.0)) / 8.0;
                (u.hypot(v) < 1.0) || (heap > -1.0 && heap < 0.0 && u.hypot(heap) < 0.95)
            }
            Fill::Porridge | Fill::Risotto | Fill::Raw => {
                let heap = (p.1 - (top + 1.0)) / 6.0;
                (u.hypot(v) < 1.0) || (heap > -1.0 && heap < 0.0 && u.hypot(heap) < 0.98)
            }
        };
        if !inside {
            return None;
        }
        let lit = -(p.0 - cx) / w * 0.4 - (p.1 - top) / 8.0 * 0.3;
        // Bits: a scatter of dots in the second colour, chunks for stews.
        let cell = (((p.0 * 0.45) as i32) * 13 + (p.1 * 0.45) as i32) as u32;
        let h = unit(hash2(salt, cell));
        match fill {
            Fill::Broth if h > 0.82 => Some(bit.pick(0.6 + lit, x, y)),
            Fill::Chunks if h > 0.55 => Some(bit.pick(0.55 + lit, x, y)),
            Fill::Porridge => {
                if h > 0.86 {
                    return Some(bit.pick(0.6 + lit, x, y));
                }
                // A swirl of cream drawn round the dome's top.
                let (sx, sy) = ((p.0 - cx + 1.0) / 6.0, (p.1 - (top - 1.5)) / 2.6);
                let ring = (sx.hypot(sy) - 0.6).abs() < 0.12;
                Some(food.pick(0.55 + lit + if ring { 0.22 } else { 0.0 }, x, y))
            }
            Fill::Risotto => {
                if h > 0.72 {
                    return Some(bit.pick(0.55 + lit, x, y));
                }
                Some(food.pick(0.5 + lit + grains(salt, p), x, y))
            }
            Fill::Raw => {
                // The ingredients heaped on the right, the dry grain left.
                let split = cx + 1.5 + ((p.1 * 0.9).sin() * 1.2);
                if p.0 > split && h > 0.25 {
                    return Some(bit.pick(0.5 + lit, x, y));
                }
                Some(food.pick(0.56 + lit + grains(salt, p), x, y))
            }
            _ => Some(food.pick(0.55 + lit, x, y)),
        }
    });
    icon
}

/// An atmosphere chilli (milestone 18): a bowl of chilli tinted by its
/// line, thick with dark beans, a whole red chilli pepper laid across the
/// top.
fn chilli(rgb: u32, name: &str) -> Tex {
    let mut icon = bowl_icon(rgb, Fill::Broth, 0x4a1a0c, name);
    pepper(&mut icon);
    icon.finish()
}

/// Purified butter: a pale yellow block on a little board, its top lit and
/// one corner sliced off.
fn butter() -> Tex {
    let board = plank_ramp();
    let fat = ramp(0xf4dc78, 0.35, 1.0);
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        ((3.0..29.0).contains(&p.0) && (22.0..26.0).contains(&p.1)).then(|| board.pick(0.5 - (p.1 - 22.0) * 0.05, x, y))
    });
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        // A box seen from the front and above: the top face from y 9 to 13,
        // the front below it to 22; the cut corner at the right.
        if !(6.0..26.0).contains(&p.0) || !(9.0..22.0).contains(&p.1) {
            return None;
        }
        if p.0 - 20.0 > p.1 - 9.0 {
            return None;
        }
        let top = p.1 < 13.0;
        Some(fat.pick(if top { 0.85 } else { 0.6 - (p.0 - 6.0) * 0.008 }, x, y))
    });
    icon.finish()
}

/// The colour each line's dishes are tinted: acid green, charged blue,
/// burning orange-red.
fn line_rgb(line: &str) -> Option<u32> {
    Some(match line {
        "acid" => 0x7aa82a,
        "spark" => 0x3a7ad8,
        "fire" => 0xd8401a,
        _ => return None,
    })
}

/// The atmosphere dishes, raw and cooked, of every line.
fn line_dish(name: &str) -> Option<Tex> {
    let (raw, rest) = match name.strip_prefix("raw_") {
        Some(r) => (true, r),
        None => (false, name),
    };
    let (line, dish) = rest.split_once('_')?;
    let rgb = line_rgb(line)?;
    Some(match (dish, raw) {
        ("chilli", false) => chilli(rgb, name),
        ("chilli", true) => bowl(rgb, Fill::Raw, 0xf0e0c0, name),
        ("rice", false) => bowl(0xf0c058, Fill::Risotto, rgb, name),
        ("rice", true) => bowl(0xf6f2e6, Fill::Raw, rgb, name),
        ("pie", r) => pie(r, rgb, name),
        _ => return None,
    })
}

/// A red chilli pepper with a green stalk, curved across the top right.
fn pepper(icon: &mut Icon) {
    let red = ramp(0xe02a1a, 0.3, 0.98);
    let stalk = ramp(0x4aa040, 0.3, 0.9);
    let (a, b, k) = ((14.0f32, 8.0f32), (27.0f32, 13.5f32), (21.5f32, 8.0f32));
    // A quadratic curve from the stalk end `a` to the tip `b`, bowed by `k`.
    let at = |t: f32| {
        let s = 1.0 - t;
        (
            s * s * a.0 + 2.0 * s * t * k.0 + t * t * b.0,
            s * s * a.1 + 2.0 * s * t * k.1 + t * t * b.1,
        )
    };
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let mut best = (f32::MAX, 0.0f32);
        for i in 0..24 {
            let (t0, t1) = (i as f32 / 24.0, (i + 1) as f32 / 24.0);
            let d = seg_dist(at(t0), at(t1), p);
            if d < best.0 {
                best = (d, t0);
            }
        }
        let (d, t) = best;
        let half = 2.6 * (1.0 - t * 0.75);
        if d > half {
            return None;
        }
        // Lit along its upper side, a highlight streak near the stalk end.
        let up = p.1 < at(t).1;
        let mut tone = 0.55 + if up { 0.15 } else { -0.15 };
        if up && d > half * 0.35 && d < half * 0.75 && (0.1..0.5).contains(&t) {
            tone = 1.0;
        }
        Some(red.pick(tone, x, y))
    });
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        (seg_dist((10.5, 6.0), (14.5, 8.0), p) < 1.2 || (p.0 - 14.0).hypot(p.1 - 8.2) < 2.0)
            .then(|| stalk.pick(0.6 - (p.1 - 7.0) * 0.08, x, y))
    });
}

/// Hearty pie (milestone 18): a round pie in a tin dish, a crimped crust
/// round its edge and three slits showing the meat inside; baked golden
/// brown, or raw in pale dough.
fn pie(raw: bool, filling: u32, name: &str) -> Tex {
    let crust = if raw {
        ramp(0xf0dcb4, 0.55, 1.0)
    } else {
        ramp(0xd89a40, 0.3, 0.98)
    };
    let filling = ramp(filling, if raw { 0.45 } else { 0.25 }, 0.9);
    let tin = steel_ramp();
    let (cx, cy, rx, ry, wall) = (16.0f32, 14.5f32, 13.5f32, 7.5f32, 6.5f32);
    let mut icon = Icon::new();
    // The tin dish: a band below the rim ellipse, flaring to its foot.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let dy = p.1 - cy;
        if !(0.0..=wall + ry).contains(&dy) {
            return None;
        }
        let w = rx - (dy / (wall + ry)) * 1.8;
        let foot = ((p.0 - cx) / w).hypot((dy - wall) / ry);
        if (p.0 - cx).abs() > w || (dy > wall && foot > 1.0) {
            return None;
        }
        let t = 0.6 - (p.0 - cx) / rx * 0.3 - dy / (wall + ry) * 0.2;
        Some(tin.pick(if dy < 1.5 { t + 0.2 } else { t }, x, y))
    });
    // The crust's top: a domed ellipse with a scalloped, crimped edge.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let (u, v) = ((p.0 - cx) / rx, (p.1 - cy) / ry);
        let r = u.hypot(v);
        let ang = v.atan2(u);
        let scallop = 1.0 + (ang * 14.0).cos() * 0.035;
        if r > scallop {
            return None;
        }
        let lit = -u * 0.25 - v * 0.2;
        // The crimped rim: ridges round the edge.
        if r > 0.78 {
            let ridge = (ang * 14.0).cos();
            return Some(crust.pick(0.55 + lit + ridge * 0.14, x, y));
        }
        // Three slits across the middle, showing the filling.
        for k in -1..=1 {
            let ox = cx + k as f32 * 5.0;
            if seg_dist((ox - 1.5, cy - 2.5), (ox + 1.5, cy + 2.5), p) < 0.75 {
                return Some(filling.pick(0.45, x, y));
            }
        }
        // A glaze sheen on the dome, top left.
        let sheen = !raw && ((u + 0.35).hypot(v + 0.4) - 0.22).abs() < 0.07;
        let mut t = 0.62 + lit + (1.0 - r) * 0.12;
        if sheen {
            t += 0.2;
        }
        // Raw dough is dusted with flour in soft patches.
        if raw {
            let cell = (((p.0 * 0.4) as i32) * 11 + (p.1 * 0.4) as i32) as u32;
            if unit(hash2(seed(name), cell)) > 0.7 {
                t += 0.18;
            }
        }
        Some(crust.pick(t, x, y))
    });
    icon.finish()
}

/// Tone offset for a heap of grains: three-by-two pixel grains in staggered
/// rows, each a little lighter or darker, with a dark gap at its left end.
fn grains(salt: u32, p: P) -> f32 {
    let row = (p.1 / 2.0).floor() as i32;
    let shift = if row % 2 == 0 { 0.0 } else { 1.5 };
    let col = ((p.0 + shift) / 3.0).floor() as i32;
    let h = unit(hash2(salt ^ 0x5eed, (col * 7 + row * 131) as u32));
    let gap = (p.0 + shift).rem_euclid(3.0) < 1.0;
    (h - 0.5) * 0.2 + if gap { -0.14 } else { 0.08 }
}

/// A wooden bucket with two iron bands; `top` the colour of what is in it.
fn bucket(top: Option<u32>) -> Tex {
    bucket_icon(top).finish()
}

/// The bucket, unfinished, so others can draw over it (`brews`).
pub(crate) fn bucket_icon(top: Option<u32>) -> Icon {
    let wood = plank_ramp();
    let iron = steel_ramp();
    let mut icon = Icon::new();
    // The body: a trapezoid, wider at the top.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !(9.0..29.0).contains(&p.1) {
            return None;
        }
        let half = 11.0 - (p.1 - 9.0) * 0.16;
        if (p.0 - 16.0).abs() > half {
            return None;
        }
        let band = (12.0..14.0).contains(&p.1) || (24.0..26.0).contains(&p.1);
        let t = 0.6 - (p.0 - 16.0) / half * 0.3;
        // Staves.
        let stave = ((p.0 - 16.0 + 30.0) * 0.5) as i32 % 2 == 0;
        Some(if band {
            iron.pick(t, x, y)
        } else {
            wood.pick(t + if stave { 0.06 } else { -0.06 }, x, y)
        })
    });
    // The opening.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let (u, v) = ((p.0 - 16.0) / 10.0, (p.1 - 9.0) / 2.5);
        if u.hypot(v) > 1.0 {
            return None;
        }
        Some(match top {
            Some(rgb) => ramp(rgb, 0.4, 1.0).pick(0.6 - u * 0.2, x, y),
            None => wood.at(0),
        })
    });
    // The handle.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (u, v) = ((p.0 - 16.0) / 11.0, (p.1 - 9.0) / 7.5);
        (v < 0.0 && (u.hypot(v) - 1.0).abs() < 0.09).then(|| iron.pick(0.7, x, y))
    });
    icon
}

/// Three nails, crossed.
fn nails() -> Tex {
    let iron = steel_ramp();
    let mut icon = Icon::new();
    for (a, b) in [
        ((7.0f32, 8.0f32), (22.0f32, 26.0f32)),
        ((12.0, 25.0), (25.0, 7.0)),
        ((5.0, 18.0), (27.0, 17.0)),
    ] {
        icon.layer(true, |x, y| {
            let p: P = (c(x), c(y));
            if (p.0 - a.0).hypot(p.1 - a.1) < 2.6 {
                return Some(iron.pick(0.85, x, y));
            }
            (seg_dist(a, b, p) < 1.1).then(|| iron.pick(0.55, x, y))
        });
    }
    icon.finish()
}

/// A stoppered jar of amber syrup.
fn syrup() -> Tex {
    let glass = ramp(0xd88a2a, 0.3, 1.0);
    let cork = ramp(0xa87850, 0.4, 0.95);
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (u, v) = ((p.0 - 16.0) / 9.5, (p.1 - 19.0) / 10.0);
        if u.hypot(v) > 1.0 && !((p.0 - 16.0).abs() < 4.5 && (7.0..12.0).contains(&p.1)) {
            return None;
        }
        let t = 0.5 - u * 0.3 - v * 0.2;
        let shine = (p.0 - 12.0).abs() < 1.0 && (14.0..22.0).contains(&p.1);
        Some(if shine {
            glass.at(glass.len() - 1)
        } else {
            glass.pick(t, x, y)
        })
    });
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        ((p.0 - 16.0).abs() < 5.0 && (3.5..8.0).contains(&p.1)).then(|| cork.pick(0.6, x, y))
    });
    icon.finish()
}

/// A tied sack with a white heap at its mouth.
fn sugar() -> Tex {
    let sack = ramp(0xc8a878, 0.35, 0.95);
    let white = ramp(0xf4f0ea, 0.65, 1.0);
    let mut icon = Icon::new();
    blob(
        &mut icon,
        (16.0, 20.0),
        (11.0, 9.5),
        0.0,
        &sack,
        0.05,
        false,
        seed("sugar"),
    );
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let (u, v) = ((p.0 - 16.0) / 7.0, (p.1 - 10.5) / 3.5);
        (u.hypot(v) < 1.0).then(|| white.pick(0.6 - u * 0.2, x, y))
    });
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        ((p.0 - 16.0).abs() < 7.5 && (12.5..14.5).contains(&p.1)).then(|| sack.at(1))
    });
    icon.finish()
}

/// What a sack holds, as seen at its mouth.
#[derive(Clone, Copy)]
enum Heap {
    /// Rolled flakes: light and dark flecks.
    Flakes,
    /// Separate grains: little dashes.
    Grains,
    /// A fine powder, smooth.
    Powder,
}

/// A tied sack (like `sugar`) in `cloth`, a heap of `fill` at its mouth and,
/// for some, a coloured stripe round its belly.
fn sack(cloth: u32, fill: u32, heap: Heap, stripe: Option<u32>, name: &str) -> Tex {
    let sackr = ramp(cloth, 0.35, 0.95);
    let content = ramp(fill, 0.5, 1.0);
    let salt = seed(name);
    let mut icon = Icon::new();
    blob(&mut icon, (16.0, 20.0), (11.0, 9.5), 0.0, &sackr, 0.05, false, salt);
    if let Some(rgb) = stripe {
        let band = ramp(rgb, 0.3, 0.9);
        icon.layer(false, |x, y| {
            let p = (c(x), c(y));
            let (u, v) = ((p.0 - 16.0) / 11.0, (p.1 - 20.0) / 9.5);
            (u.hypot(v) < 0.93 && (p.1 - 21.0 - u * u * 2.0).abs() < 1.3).then(|| band.pick(0.6 - u * 0.3, x, y))
        });
    }
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let (u, v) = ((p.0 - 16.0) / 7.0, (p.1 - 10.5) / 3.5);
        if u.hypot(v) >= 1.0 {
            return None;
        }
        let t = 0.6 - u * 0.2 - v * 0.1;
        let h = unit(hash2(salt, (grid(x) / 2 + grid(y) * 16) as u32));
        Some(match heap {
            Heap::Flakes => content.pick(
                t + if h > 0.6 {
                    0.15
                } else if h < 0.3 {
                    -0.2
                } else {
                    0.0
                },
                x,
                y,
            ),
            Heap::Grains => content.pick(t + if (grid(x) / 2 + grid(y)) % 2 == 0 { 0.12 } else { -0.14 }, x, y),
            Heap::Powder => content.pick(t, x, y),
        })
    });
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        ((p.0 - 16.0).abs() < 7.5 && (12.5..14.5).contains(&p.1)).then(|| sackr.at(1))
    });
    icon.finish()
}

/// A rounded loaf with three slashes across its top: raw is pale dough, baked
/// a golden-brown crust; both tinted towards their line, flecked with it.
fn loaf(line: u32, raw: bool, name: &str) -> Tex {
    use crate::farming::mix_hex;
    let crust = if raw {
        ramp(mix_hex(0xf0dcb8, line, 0.2), 0.55, 1.0)
    } else {
        ramp(mix_hex(0xc0803a, line, 0.28), 0.28, 0.95)
    };
    let crumb = ramp(if raw { mix_hex(0xe0c8a0, line, 0.2) } else { 0xf0d8a0 }, 0.45, 1.0);
    let fleck = ramp(line, 0.3, 0.9);
    let salt = seed(name);
    let (cx, cy, w) = (16.0f32, 20.0f32, 13.5f32);
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let dx = (p.0 - cx) / w;
        let dy = p.1 - cy;
        let h = if dy < 0.0 { 10.0 } else { 7.0 };
        if dx.abs().powi(4) + (dy / h).powi(2) > 1.0 {
            return None;
        }
        let lit = -(dx * 0.5 + dy / 10.0 * 0.8);
        let mut t = 0.5 + lit * 0.4;
        if dy > 3.0 {
            t -= 0.18;
        }
        // The slashes, showing the crumb.
        for k in 0..3 {
            let x0 = 8.5 + k as f32 * 6.0;
            if seg_dist((x0, 13.5), (x0 + 4.0, 18.5), p) < 0.8 {
                return Some(crumb.pick(0.6, x, y));
            }
        }
        if speck(salt, p, 2.6, 0.9, 0.22) > 0.0 {
            return Some(fleck.pick(0.5 + lit * 0.3, x, y));
        }
        // Raw dough has a dusting of flour on top, in soft patches.
        if raw && dy < -3.0 {
            let flour = crate::noise::value(salt ^ 3, 8, 8, p.0, p.1);
            t += smoothstep(0.5, 0.7, flour) * 0.25 * smoothstep(-3.0, -5.0, dy);
        }
        Some(crust.pick(t, x, y))
    });
    icon.finish()
}

/// Each dish line's colour and the colour of its bits.
fn line_colours(key: &str) -> Option<(u32, u32)> {
    Some(match key {
        "dewberry" => (0xd03a50, 0xf0b0c0),
        "tuber" => (0xc89a50, 0x9a6a40),
        "meat" => (0xa0603a, 0x7a3a24),
        "lively" => (0xf0c840, 0xb88a30),
        "keen" => (0xe07040, 0xf0c070),
        "gentle" => (0x8a60b0, 0xd0b0f0),
        "guarding" => (0x9a4030, 0x5aa040),
        "wise" => (0x4a90c0, 0xd0f0f8),
        _ => return None,
    })
}

/// The rank 3 and 4 dishes of milestone 16 part 2: `[raw_]<line>_porridge`,
/// `[raw_]<line>_risotto` and `[raw_]<line>_loaf`.
fn farm_dish(name: &str) -> Option<Tex> {
    use crate::farming::mix_hex;
    let (raw, rest) = match name.strip_prefix("raw_") {
        Some(rest) => (true, rest),
        None => (false, name),
    };
    let (key, kind) = rest.rsplit_once('_')?;
    let (line, bits) = line_colours(key)?;
    Some(match (kind, raw) {
        ("porridge", false) => bowl(mix_hex(0xe8d8b0, line, 0.55), Fill::Porridge, bits, name),
        ("risotto", false) => bowl(mix_hex(0xf0ead8, line, 0.45), Fill::Risotto, bits, name),
        ("porridge", true) => bowl(0xe0d0a8, Fill::Raw, line, name),
        ("risotto", true) => bowl(0xf0ecdc, Fill::Raw, line, name),
        ("loaf", raw) => loaf(line, raw, name),
        _ => return None,
    })
}

/// Paints the food icon called `name`, or `None` if it is not one.
pub fn paint(name: &str) -> Option<Tex> {
    Some(match name {
        "dewberry" => berries(0xd0304a, false, name),
        "cooked_dewberry" => berries(0x8a2a36, true, name),
        "cooked_meat" => roast(0x8a4a2a, false, name),
        "cooked_lean_meat" => roast(0xa86040, false, name),
        "cooked_prime_meat" => roast(0x7a3424, true, name),
        "cooked_tuber" => roasted_tuber(0x8a5a30, name),
        "cooked_waxy_tuber" => roasted_tuber(0xb88a30, name),
        "cooked_exotic_tuber" => roasted_tuber(0x6a3a7a, name),
        "dewberry_soup" => bowl(0xd8506a, Fill::Broth, 0xf0b0c0, name),
        "stewed_dewberries" => bowl(0x9a2438, Fill::Chunks, 0xd03a50, name),
        "tuber_soup" => bowl(0xd8b060, Fill::Broth, 0x9a6a40, name),
        "tuber_stew" => bowl(0xb08048, Fill::Chunks, 0xe8d0a0, name),
        "meat_broth" => bowl(0xc88a3a, Fill::Broth, 0x7a3a24, name),
        "meat_casserole" => bowl(0x8a4a2a, Fill::Chunks, 0xe0c8a0, name),
        "lively_soup" => bowl(0xf0c840, Fill::Broth, 0xb88a30, name),
        "lively_stew" => bowl(0xd0a030, Fill::Chunks, 0xf8e070, name),
        "keen_soup" => bowl(0xe07040, Fill::Broth, 0xf0c070, name),
        "keen_casserole" => bowl(0xb05030, Fill::Chunks, 0xf09050, name),
        "gentle_broth" => bowl(0x8a60b0, Fill::Broth, 0xd0b0f0, name),
        "gentle_stew" => bowl(0x6a3a8a, Fill::Chunks, 0xb080d8, name),
        "guarding_broth" => bowl(0x9a4030, Fill::Broth, 0x5aa040, name),
        "guarding_casserole" => bowl(0x7a3020, Fill::Chunks, 0x4a9a3a, name),
        "bucket" => bucket(None),
        "water_bucket" => bucket(Some(0x3a7ad0)),
        "milk_bucket" => bucket(Some(0xf2f0e8)),
        "nail" => nails(),
        "syrup" => syrup(),
        "sugar" => sugar(),
        "oat_bag" => sack(0xc8a878, 0xe4cc90, Heap::Flakes, None, name),
        "rice_bag" => sack(0xa8b47a, 0xf6f2e6, Heap::Grains, None, name),
        "flour" => sack(0xe4dccc, 0xfaf6f0, Heap::Powder, Some(0x4a6ac0), name),
        "baking_powder" => sack(0x98aac4, 0xf2f6fc, Heap::Powder, Some(0xc84a3a), name),
        // Milestone 18: purified milk and butter, then the atmosphere dishes.
        "purified_milk" => bucket(Some(0xe6f2ff)),
        "purified_butter" => butter(),
        _ => return line_dish(name).or_else(|| farm_dish(name)),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_food_icon_paints_something() {
        for name in [
            "dewberry",
            "meat_broth",
            "keen_casserole",
            "water_bucket",
            "nail",
            "syrup",
            "sugar",
        ] {
            let t = super::paint(name).unwrap();
            assert!(t.mean().a > 0.15, "{name} is mostly empty");
        }
    }

    #[test]
    fn every_farm_dish_and_bag_paints_something() {
        let mut names: Vec<String> = ["oat_bag", "rice_bag", "flour", "baking_powder"]
            .map(String::from)
            .to_vec();
        for key in [
            "dewberry", "tuber", "meat", "lively", "keen", "gentle", "guarding", "wise",
        ] {
            let bowl = if matches!(key, "dewberry" | "wise") {
                "porridge"
            } else {
                "risotto"
            };
            for dish in [bowl, "loaf"] {
                names.push(format!("raw_{key}_{dish}"));
                names.push(format!("{key}_{dish}"));
            }
        }
        for name in &names {
            let t = super::paint(name).unwrap_or_else(|| panic!("{name} has no painter"));
            assert!(t.mean().a > 0.15, "{name} is mostly empty");
        }
        // Raw and cooked look different.
        assert_ne!(super::paint("raw_keen_loaf"), super::paint("keen_loaf"));
    }
}
