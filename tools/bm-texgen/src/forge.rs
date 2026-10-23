//! Forging (milestone 17): the forge machine, the fossil seams, and the
//! icons of every forging material, base and ingredient.
//!
//! The forge is drawn like the spark machines (`spark`): a stone rim in
//! quarters on top, a banded side with the spark socket at its foot, here
//! in a violet-grey stone round a crucible of violet fire.
//!
//! The ingredients come in six families, and each family has one shape so a
//! forge queue reads at a glance: tinctures are round flasks, glazes squat
//! glazed jars, salves round tins with a mark on the lid, spurs crystal
//! shards growing from a rock, washes tall bottles, waxes discs. Within a
//! family the colour says what it does; ranks are gold pips in a corner
//! (none for an unranked ingredient). Each family's base is its container in
//! plain stuff: clear water, bare clay, a grey tin, pale quartz, milky wash,
//! raw wax.

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_4, TAU};

use crate::color::{Lab, Ramp, Rgba};
use crate::gathered::ramp;
use crate::icons::{Icon, Lump, P, blade, seg_dist};
use crate::materials::{plank_ramp, steel_ramp, stone, stone_ramp};
use crate::noise::{hash2, seed, unit, value};
use crate::spark::{machine_side, quarters};
use crate::stations::{Mask, SOOT, cast_shadow, iron_ramp, rivet};
use crate::tex::{Ctx, SIZE, Tex, c};

// ---------------------------------------------------------------- palettes

/// The forge's stone: a violet grey.
fn forge_stone() -> Ramp {
    Ramp::from_hex(&[
        0x1c1626, 0x2c2238, 0x40344e, 0x564866, 0x6e6080, 0x8a7c9c, 0xaa9eba, 0xd0c8dc,
    ])
}

/// The forge's fire: deep violet through lilac to a white heart.
fn violet_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x1e0a30, 0x3a1460, 0x5e2490, 0x8a3cc0, 0xb064e8, 0xd09cff, 0xecd4ff, 0xfff8ff,
    ])
}

/// Fossil bone and shell: shadow brown to warm ivory.
fn bone_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x2e2228, 0x54443e, 0x7e6c5c, 0xa8967c, 0xcab89a, 0xe2d4b8, 0xf4ecd8, 0xfffcf0,
    ])
}

/// Glass, seen against a dark slot: dim blue-grey to a white glint.
fn glass_ramp() -> Ramp {
    Ramp::from_hex(&[0x1c2a3c, 0x34506a, 0x58809a, 0x88b0c6, 0xbcdce8, 0xeaf8fc])
}

/// Charcoal: near black with a warm brown sheen.
fn char_ramp() -> Ramp {
    Ramp::from_hex(&[0x0e0a10, 0x1c1418, 0x2c2024, 0x3e3030, 0x54443e, 0x6e5c50, 0x8a7868])
}

/// The rank pips' gold.
fn pip_ramp() -> Ramp {
    ramp(0xf0d060, 0.4, 0.98)
}

/// A ramp round `rgb` that keeps its own lightness in the middle, so a
/// dark liquid stays dark and a pale one pale.
fn tint(rgb: u32) -> Ramp {
    let l = Lab::from_rgb(Rgba::hex(rgb)).l;
    ramp(rgb, (l - 0.32).max(0.12), (l + 0.26).min(0.99))
}

/// A near-white liquid: kept bright all through, so it reads apart from
/// the grey-blue ones.
fn bright(rgb: u32) -> Ramp {
    ramp(rgb, 0.78, 0.995)
}

/// Violet light spilling onto a face round `(cx, cy)` out to `reach`.
fn spill(tex: &mut Tex, cx: f32, cy: f32, reach: f32, k: f32, skip: &Mask) {
    let glow = violet_ramp().at(5);
    for y in 0..SIZE {
        for x in 0..SIZE {
            if skip.get(x as i32, y as i32) {
                continue;
            }
            let d = (c(x) - cx).hypot(c(y) - cy);
            let w = (1.0 - d / reach).clamp(0.0, 1.0).powi(2) * k;
            if w > 0.01 {
                tex.set(x, y, tex.get(x, y).mix(glow, w));
            }
        }
    }
}

// --------------------------------------------------------------- the forge

/// Forge top: a violet-grey stone rim round an iron-ringed crucible of
/// violet fire, swirling, held by four iron clamps on the diagonals.
pub fn forge_top(_ctx: Ctx) -> Tex {
    let salt = seed("forge_top");
    let stone = forge_stone();
    let iron = iron_ramp();
    let fire = violet_ramp();
    let mut tex = Tex::from_fn(|x, y| quarters(salt, &stone, x, y));
    let mut parts = Mask::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
            let r = dx.hypot(dy);
            let (a, b) = ((dx - dy) * FRAC_1_SQRT_2, (dx + dy) * FRAC_1_SQRT_2);
            let clamp = (9.0..13.6).contains(&r) && a.abs().min(b.abs()) < 1.7;
            if r >= 11.0 && !clamp {
                continue;
            }
            let lit = -(dx + dy) / r.max(0.1);
            let col = if clamp {
                let end = if r > 12.6 { -0.25 } else { 0.0 };
                iron.pick(0.62 + lit * 0.22 + end, x, y)
            } else if r > 9.2 {
                iron.pick(0.48 + lit * 0.3, x, y)
            } else if r > 8.0 {
                // The ring's inner wall faces away from the light.
                iron.pick(0.3 - lit * 0.2, x, y)
            } else {
                let ang = dy.atan2(dx);
                let swirl = (ang * 3.0 + r * 0.75).sin();
                let flick = value(salt ^ 5, 8, 8, c(x), c(y));
                fire.pick(0.95 - r / 8.0 * 0.5 + swirl * 0.1 + (flick - 0.5) * 0.12, x, y)
            };
            tex.set(x, y, col);
            parts.set(x, y);
        }
    }
    cast_shadow(&mut tex, &parts, SOOT, 0.4);
    spill(&mut tex, 16.0, 16.0, 14.0, 0.3, &parts);
    for (x, y) in [(15, 15), (16, 15), (15, 16)] {
        tex.set(x, y, fire.lightest());
    }
    tex
}

/// Forge side: violet-grey masonry under an iron band, an arched iron
/// mouth on a bed of coals and violet flame, the spark socket at its foot.
pub fn forge_side(_ctx: Ctx) -> Tex {
    let salt = seed("forge_side");
    let mut tex = machine_side(salt, &forge_stone(), &iron_ramp());
    let iron = iron_ramp();
    let fire = violet_ramp();
    let coal = char_ramp();
    let (cx, top, bottom, half) = (16.0f32, 6.5f32, 25.0f32, 9.0f32);
    let spring = top + half;
    let inside = |px: f32, py: f32, h: f32| -> bool {
        let dx = px - cx;
        if dx.abs() >= h || py >= bottom {
            return false;
        }
        py >= spring || dx.hypot(py - spring) < h
    };
    let mut parts = Mask::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (px, py) = (c(x), c(y));
            if !inside(px, py, half) {
                continue;
            }
            let dx = px - cx;
            let col = if !inside(px, py, half - 1.8) || py > bottom - 1.0 {
                // The frame: round in section, lit on its top-left.
                let r = dx.hypot((py - spring).min(0.0));
                let lit = if py < spring {
                    -(dx + (py - spring)) / r.max(0.1)
                } else {
                    -dx.signum()
                };
                iron.pick(0.5 + lit * 0.28, x, y)
            } else if py > bottom - 4.0 {
                // Coals: dark lumps with violet embers between them.
                let lump = value(salt ^ 11, 8, 4, px * 1.6, py * 1.6);
                if lump > 0.55 {
                    coal.pick(0.25 + (lump - 0.55) * 1.6 - dx / half * 0.1, x, y)
                } else {
                    fire.pick(0.55 + (0.55 - lump) * 0.8, x, y)
                }
            } else {
                // Flame: brighter low down, in licking tongues.
                let tongue = ((dx * 0.9 + (py * 0.35).sin() * 1.6) * 0.7).cos();
                let flick = value(salt ^ 7, 8, 8, px, py);
                let t = 0.2 + (py - top) / (bottom - top) * 0.6 + tongue * 0.12 + (flick - 0.5) * 0.15;
                fire.pick(t, x, y)
            };
            tex.set(x, y, col);
            parts.set(x, y);
        }
    }
    // A keystone at the top of the arch.
    for y in 5..9 {
        for x in 14..18 {
            let lit = if x == 14 || y == 5 {
                0.25
            } else if x == 17 || y == 8 {
                -0.25
            } else {
                0.0
            };
            tex.set(x, y, forge_stone().pick(0.62 + lit, x, y));
            parts.set(x, y);
        }
    }
    cast_shadow(&mut tex, &parts, SOOT, 0.4);
    spill(&mut tex, 16.0, 20.0, 15.0, 0.22, &parts);
    for (x, y) in [(8, 17), (23, 17), (8, 23), (23, 23)] {
        rivet(&mut tex, &iron, x, y);
    }
    tex
}

// --------------------------------------------------------------- fossils

/// One fossil set in rock.
#[derive(Clone, Copy)]
enum Fossil {
    /// A coiled shell: centre, radius, turn.
    Coil(f32, f32, f32, f32),
    /// A ribbed fan shell: centre, radius, turn.
    Fan(f32, f32, f32, f32),
    /// A bone: its two ends and half its width.
    Bone(P, P, f32),
}

impl Fossil {
    /// The tone of the fossil at `p` (0..1, ridges dark), or `None` outside.
    fn tone(self, p: P) -> Option<f32> {
        match self {
            Fossil::Coil(cx, cy, r, turn) => {
                let (dx, dy) = (p.0 - cx, p.1 - cy);
                let d = dx.hypot(dy) / r;
                if d > 1.0 {
                    return None;
                }
                if d < 0.14 {
                    return Some(0.3);
                }
                let a = dy.atan2(dx) + turn;
                // Fewer whorls on a small coil, so its spiral survives.
                let turns = (r / 4.0).clamp(1.4, 2.6);
                let whorl = (d * turns - a / TAU).rem_euclid(1.0);
                if whorl < 0.26 {
                    return Some(0.22);
                }
                let rib = (a * 9.0).sin() * 0.06;
                Some(0.7 - d * 0.12 + rib - (dx + dy) / r * 0.12)
            }
            Fossil::Fan(cx, cy, r, turn) => {
                let (s, co) = turn.sin_cos();
                let (dx, dy) = (p.0 - cx, p.1 - cy);
                let (u, v) = (dx * co + dy * s, -dx * s + dy * co);
                let hinge = r * 0.6;
                let (hu, hv) = (u, hinge - v);
                let rr = hu.hypot(hv);
                let ang = hu.atan2(hv);
                // The two ears either side of the hinge.
                if hv.abs() < r * 0.2 && hu.abs() < r * 0.48 {
                    return Some(0.5 - u / r * 0.1);
                }
                let edge = r * 1.3 * (1.0 - 0.05 * (ang * 9.0).cos().abs());
                if ang.abs() > 1.0 || rr > edge || hv < 0.0 {
                    return None;
                }
                let rib = (ang * 9.0).cos();
                Some(0.62 + rib * 0.16 - rr / edge * 0.1 - (dx + dy) / r * 0.1)
            }
            Fossil::Bone(a, b, w) => {
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = dx.hypot(dy);
                let (nx, ny) = (-dy / len * w, dx / len * w);
                let knob = [a, b]
                    .iter()
                    .flat_map(|e| [(e.0 + nx, e.1 + ny), (e.0 - nx, e.1 - ny)])
                    .any(|k| (p.0 - k.0).hypot(p.1 - k.1) < w * 1.05);
                let shaft = seg_dist(a, b, p) < w * 0.8;
                if !knob && !shaft {
                    return None;
                }
                // Round across: lit on the side towards the top left.
                let side = ((p.0 - a.0) * -dy + (p.1 - a.1) * dx) / len / w;
                let lit = if nx + ny < 0.0 { side } else { -side };
                Some(0.62 + lit * 0.18)
            }
        }
    }
}

/// Sets `fossils` into stone, outlined dark where they meet the rock and
/// lit on their top-left edges, as the other seams set their finds. The
/// host rock is a little warmer than plain stone: a fossil bed.
fn fossil_seam(ctx: Ctx, salt: u32, fossils: &[Fossil]) -> Tex {
    let bone = bone_ramp();
    let outline = Rgba::hex(0x1a1220);
    let bed = Rgba::hex(0xb4a284);
    let mut tex = stone(ctx);
    for p in tex.px.iter_mut() {
        *p = p.mix(bed, 0.2);
    }
    let at = |fx: f32, fy: f32| fossils.iter().find_map(|f| f.tone((fx, fy)));
    let host = tex.clone();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (c(x), c(y));
            if let Some(t) = at(fx, fy) {
                let lit_edge = at(fx - 1.0, fy).is_none() || at(fx, fy - 1.0).is_none();
                let dark_edge = at(fx + 1.0, fy).is_none() || at(fx, fy + 1.0).is_none();
                let grain = (value(hash2(salt, 3), 8, 8, fx, fy) - 0.5) * 0.12;
                let t = if lit_edge {
                    0.9
                } else if dark_edge {
                    0.25
                } else {
                    t + grain
                };
                tex.set(x, y, bone.pick(t, x, y));
            } else {
                let near = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
                    .iter()
                    .any(|(dx, dy)| at(fx + dx, fy + dy).is_some());
                if near {
                    tex.set(x, y, outline);
                } else if at(fx - 1.0, fy - 1.0).is_some() || at(fx - 2.0, fy - 1.0).is_some() {
                    tex.set(x, y, host.get(x, y).mix(outline, 0.4));
                }
            }
        }
    }
    tex
}

/// Small fossil seam: stone with a small coiled shell, a fan shell and a
/// short bone in it.
pub fn small_fossil_seam(ctx: Ctx) -> Tex {
    fossil_seam(
        ctx,
        seed("small_fossil_seam"),
        &[
            Fossil::Coil(10.0, 10.5, 5.0, 0.4),
            Fossil::Fan(23.0, 21.0, 4.6, 0.5),
            Fossil::Bone((6.0, 24.0), (12.5, 27.5), 1.4),
        ],
    )
}

/// Medium fossil seam: more and bigger finds, a large coil, two fan shells,
/// a long bone and a small coil.
pub fn medium_fossil_seam(ctx: Ctx) -> Tex {
    fossil_seam(
        ctx,
        seed("medium_fossil_seam"),
        &[
            Fossil::Coil(11.5, 11.5, 8.0, 1.2),
            Fossil::Fan(25.5, 7.0, 4.2, -0.4),
            Fossil::Coil(25.5, 17.5, 3.2, 2.6),
            Fossil::Bone((14.5, 26.5), (26.0, 25.5), 1.8),
            Fossil::Fan(5.0, 26.0, 3.6, 0.3),
        ],
    )
}

// ------------------------------------------------------------ icon parts

/// Gold rank pips: `n` small diamonds from `at`, `step` apart.
fn pips(icon: &mut Icon, n: u32, at: P, step: P) {
    let gold = pip_ramp();
    for i in 0..n {
        let (px, py) = (at.0 + step.0 * i as f32, at.1 + step.1 * i as f32);
        icon.layer(true, |x, y| {
            let (dx, dy) = (c(x) - px, c(y) - py);
            (dx.abs() + dy.abs() < 2.1).then(|| gold.pick(0.62 - (dx + dy) * 0.16, x, y))
        });
    }
}

/// The usual pip column, top right.
fn pips_right(icon: &mut Icon, n: u32) {
    pips(icon, n, (27.5, 4.5), (0.0, 5.0));
}

/// A mark embossed on a lid, a jar or a disc, in coordinates scaled so the
/// mark spans -1..1 both ways.
#[derive(Clone, Copy, PartialEq)]
enum Mark {
    None,
    Ring,
    Arrow,
    Shield,
    Up,
    Diamond,
    Plus,
    Bars,
    Star,
    Dots,
}

impl Mark {
    fn hit(self, u: f32, v: f32) -> bool {
        let d = u.hypot(v);
        match self {
            Mark::None => false,
            Mark::Ring => (0.5..1.0).contains(&d),
            Mark::Arrow => {
                (v.abs() < 0.24 && (-1.0..0.1).contains(&u)) || ((0.0..1.0).contains(&u) && v.abs() < (1.0 - u) * 0.95)
            }
            Mark::Shield => (-0.85..1.0).contains(&v) && u.abs() < 0.8 - v.max(0.0) * 0.8,
            Mark::Up => {
                ((-1.0..0.0).contains(&v) && u.abs() < (v + 1.0) * 0.95) || (u.abs() < 0.28 && (-0.1..1.0).contains(&v))
            }
            Mark::Diamond => u.abs() + v.abs() < 1.0,
            Mark::Plus => (u.abs() < 0.3 && v.abs() < 1.0) || (v.abs() < 0.3 && u.abs() < 1.0),
            Mark::Bars => u.abs() < 1.0 && ((v - 0.5).abs() < 0.26 || (v + 0.5).abs() < 0.26),
            Mark::Star => {
                let a = u.atan2(-v);
                let k = ((5.0 * a).cos() * 0.5 + 0.5).powi(2);
                d < 0.42 + 0.58 * k
            }
            Mark::Dots => [(-0.6f32, -0.6f32), (0.0, 0.0), (0.6, 0.6)]
                .iter()
                .any(|&(a, b)| (u - a).hypot(v - b) < 0.36),
        }
    }

    /// Embossing at a pixel: `Some(true)` raised (lit), `Some(false)` its
    /// shadow below and right, `None` untouched. `(mu, mv)` is the pixel in
    /// mark units, `px` one pixel in mark units.
    fn emboss(self, mu: f32, mv: f32, px: P) -> Option<bool> {
        if self.hit(mu, mv) {
            Some(true)
        } else if self.hit(mu - px.0, mv - px.1) {
            Some(false)
        } else {
            None
        }
    }
}

// ------------------------------------------------------------- tinctures

/// A round flask with a corked neck: `fill` of its body holds `liquid`
/// with bubbles in it (of `fleck` when the tincture is a blend).
fn flask(liquid: u32, fleck: Option<u32>, fill: f32, rank: u32) -> Tex {
    let liq = if Lab::from_rgb(Rgba::hex(liquid)).l > 0.9 {
        bright(liquid)
    } else {
        tint(liquid)
    };
    let fl = fleck.map(tint);
    let glass = glass_ramp();
    let cork = plank_ramp();
    let (cx, cy, r) = (15.0f32, 20.5f32, 9.5f32);
    let level = cy + r - fill * 2.0 * r;
    // A blend's flecks are bigger, so its second colour reads.
    let big = if fleck.is_some() { 0.6 } else { 0.0 };
    let bubbles = [
        (cx - 3.0, cy + 4.0, 1.4f32 + big),
        (cx + 3.5, cy + 1.5, 1.1 + big),
        (cx + 0.5, cy + 6.5, 1.0 + big),
    ];
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let (px, py) = (c(x), c(y));
        let (dx, dy) = (px - cx, py - cy);
        let d = dx.hypot(dy);
        let body = d < r;
        let neck = dx.abs() < 3.0 && (5.0..cy).contains(&py);
        if !body && !neck {
            return None;
        }
        if body && py > level && d < r - 1.0 {
            for &(bx, by, br) in &bubbles {
                if by > level + 1.0 && (px - bx).hypot(py - by) < br {
                    return Some(match &fl {
                        Some(f) => f.pick(0.75, x, y),
                        None => liq.pick(0.92, x, y),
                    });
                }
            }
            let mut t = 0.55 - (dx + dy) / r * 0.28;
            if py < level + 1.0 {
                t += 0.3;
            }
            return Some(liq.pick(t, x, y));
        }
        // Glass: the wall a little brighter, a streak of light top left.
        let wall = (body && d > r - 1.2 && !(neck && py < cy - r * 0.8)) || (!body && dx.abs() > 2.0);
        let ang = dy.atan2(dx);
        let streak = body && (r - 3.6..r - 2.2).contains(&d) && (-2.6..-1.9).contains(&ang);
        let t = if streak {
            0.98
        } else if wall {
            0.58 - dx / r * 0.25
        } else {
            0.3 - (dx + dy) / r * 0.1
        };
        Some(glass.pick(t, x, y))
    });
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        let dx = px - cx;
        if dx.abs() < 2.4 && (1.0..4.0).contains(&py) {
            return Some(cork.pick(0.6 - dx * 0.12 + if py < 2.0 { 0.2 } else { 0.0 }, x, y));
        }
        (dx.abs() < 3.8 && (4.0..5.6).contains(&py)).then(|| glass.pick(0.75 - dx * 0.08, x, y))
    });
    pips_right(&mut icon, rank);
    icon.finish()
}

fn tincture(liquid: u32, fleck: Option<u32>, rank: u32) -> Tex {
    flask(liquid, fleck, 0.62, rank)
}

// ---------------------------------------------------------------- glazes

/// A squat glazed jar with a wooden lid, coloured by its leaning, a gloss
/// highlight and `mark` raised on its belly. `glazed: false` is bare clay.
fn jar(rgb: u32, mark: Mark, glazed: bool) -> Tex {
    let body = tint(rgb);
    let lid = plank_ramp();
    let (cx, cy, rx, ry) = (15.0f32, 20.0f32, 10.5f32, 9.0f32);
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let (px, py) = (c(x), c(y));
        let (u, v) = ((px - cx) / rx, (py - cy) / ry);
        let neck = (px - cx).abs() < 6.0 && (8.0..12.0).contains(&py);
        let belly = u.abs().powf(2.6) + v.abs().powf(2.6) < 1.0;
        if !belly && !neck {
            return None;
        }
        if !belly {
            return Some(body.pick(0.38 - (px - cx) / 6.0 * 0.15, x, y));
        }
        let mut t = 0.56 - u * 0.3 - v * 0.18;
        if glazed && ((u + 0.5) / 0.2).hypot((v + 0.5) / 0.14) < 1.0 {
            t = 1.0;
        }
        if !glazed && (py as i32) % 4 == 0 {
            // Bare clay shows the potter's wheel rings.
            t -= 0.08;
        }
        let (mu, mv) = ((px - cx) / 5.0, (py - cy - 1.0) / 5.0);
        match mark.emboss(mu, mv, (0.2, 0.2)) {
            Some(true) => t = 1.0,
            Some(false) => t = 0.12,
            None => {}
        }
        Some(body.pick(t, x, y))
    });
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        let dx = px - cx;
        if dx.abs() < 2.2 && (2.0..5.0).contains(&py) {
            return Some(lid.pick(0.7 - dx * 0.1, x, y));
        }
        (dx.abs() < 7.6 && (5.0..8.4).contains(&py))
            .then(|| lid.pick(0.6 - dx / 7.6 * 0.28 + if py < 6.0 { 0.2 } else { 0.0 }, x, y))
    });
    icon.finish()
}

// ---------------------------------------------------------------- salves

/// A round salve tin in three-quarter view: a coloured lid (split in two
/// colours when `second` is given) with `mark` stamped on it, over a steel
/// side.
fn tin(rgb: u32, second: Option<u32>, mark: Mark, rank: u32) -> Tex {
    let lid = tint(rgb);
    let lid2 = second.map(tint);
    let steel = steel_ramp();
    let (cx, cy, rx, ry, h) = (14.5f32, 12.5f32, 12.0f32, 7.5f32, 7.5f32);
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let (px, py) = (c(x), c(y));
        let u = (px - cx) / rx;
        if u.abs() > 1.0 {
            return None;
        }
        let half = ry * (1.0 - u * u).sqrt();
        let v = (py - cy) / ry;
        let pick_lid = |t: f32| match &lid2 {
            Some(l2) if u - v * 0.7 + (v * 3.0).sin() * 0.12 > 0.0 => l2.pick(t, x, y),
            _ => lid.pick(t, x, y),
        };
        if u * u + v * v <= 1.0 {
            let ring = (u * u + v * v).sqrt();
            let mut t = 0.6 - (u + v) * 0.14;
            if ring > 0.86 {
                t += if u + v < 0.0 { 0.25 } else { -0.22 };
            }
            match mark.emboss(u / 0.6, v / 0.6, (1.0 / (rx * 0.6), 1.0 / (ry * 0.6))) {
                Some(true) => t = 1.0,
                Some(false) => t = 0.1,
                None => {}
            }
            return Some(pick_lid(t));
        }
        if py >= cy && py <= cy + h + half {
            let below = py - (cy + half);
            if below < 2.0 {
                // The lid's skirt.
                return Some(pick_lid(0.42 - u * 0.25));
            }
            let mut t = 0.52 - u * 0.28;
            if py > cy + h + half - 1.0 {
                t -= 0.18;
            }
            if (u + 0.5).abs() < 0.07 {
                t += 0.25;
            }
            return Some(steel.pick(t, x, y));
        }
        None
    });
    pips_right(&mut icon, rank);
    icon.finish()
}

// ----------------------------------------------------------------- spurs

/// Spur: three crystal shards in `rgb` growing from a lump of rock, the
/// rank pips along the top left.
fn spur(rgb: u32, rank: u32, name: &str) -> Tex {
    let r = tint(rgb);
    let salt = seed(name);
    let mut icon = Icon::new();
    let up = -FRAC_PI_4;
    blade(&mut icon, (9.5, 25.5), up - 0.72, 13.0, 3.0, &r);
    blade(&mut icon, (8.0, 26.0), up + 0.05, 25.0, 5.0, &r);
    blade(&mut icon, (11.0, 27.0), up + 0.78, 13.0, 2.8, &r);
    Lump::new(salt, 8.5, 26.0, 5.2, 6).draw(&mut icon, &stone_ramp(), 0.15, 0.8, false);
    pips(&mut icon, rank, (4.5, 4.5), (5.0, 0.0));
    icon.finish()
}

// ---------------------------------------------------------------- washes

/// A tall corked bottle nearly full of `rgb`; a shift wash swirls with
/// pale bands.
fn bottle(rgb: u32, shift: bool, rank: u32) -> Tex {
    let liq = tint(rgb);
    let glass = glass_ramp();
    let cork = plank_ramp();
    let cx = 15.0f32;
    let level = 14.0f32;
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let (px, py) = (c(x), c(y));
        let dx = px - cx;
        let half = if (12.0..30.0).contains(&py) {
            // Rounded at the foot.
            let foot = (py - 27.5).max(0.0);
            6.8 - foot * foot * 0.6
        } else if (8.5..12.0).contains(&py) {
            2.5 + (py - 8.5) / 3.5 * 4.3
        } else if (4.5..8.5).contains(&py) {
            2.5
        } else {
            return None;
        };
        if dx.abs() >= half {
            return None;
        }
        let wall = dx.abs() > half - 1.1;
        if py > level && !wall && py < 29.0 {
            let mut t = 0.55 - dx / 6.8 * 0.3 - (py - 20.0) / 30.0;
            if py < level + 1.0 {
                t += 0.3;
            }
            if shift && (px - (py * 0.55).sin() * 2.5 + py * 0.4).rem_euclid(6.0) < 1.8 {
                t += 0.38;
            }
            return Some(liq.pick(t, x, y));
        }
        let streak = (dx + 4.0).abs() < 0.6 && (13.0..25.0).contains(&py);
        let t = if streak {
            0.98
        } else if wall {
            0.58 - dx / half * 0.25
        } else {
            0.3 - dx / 7.0 * 0.1
        };
        Some(glass.pick(t, x, y))
    });
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        let dx = px - cx;
        if dx.abs() < 2.2 && (1.0..4.0).contains(&py) {
            return Some(cork.pick(0.6 - dx * 0.12 + if py < 2.0 { 0.2 } else { 0.0 }, x, y));
        }
        (dx.abs() < 3.4 && (4.0..5.6).contains(&py)).then(|| glass.pick(0.75 - dx * 0.08, x, y))
    });
    pips_right(&mut icon, rank);
    icon.finish()
}

// ----------------------------------------------------------------- waxes

#[derive(Clone, Copy, PartialEq)]
enum Wax {
    /// Raw wax: a plain disc.
    Plain,
    /// Sealing wax: a disc with a raised rim and a star stamped in it.
    Seal,
    /// Unmaking wax: a dusky disc split by `rank` cracks.
    Cracked,
}

/// A disc of wax with a soft, dripped outline.
fn wax(rgb: u32, kind: Wax, rank: u32, name: &str) -> Tex {
    let w = tint(rgb);
    let salt = seed(name);
    let (cx, cy, r) = (14.0f32, 17.0f32, 11.5f32);
    let phase = unit(salt) * TAU;
    // Crack polylines, from near the middle out past the rim.
    let cracks: [[P; 3]; 3] = [
        [(13.0, 15.0), (8.0, 11.0), (4.0, 7.0)],
        [(15.0, 18.0), (20.5, 21.0), (24.0, 27.0)],
        [(14.0, 17.0), (15.5, 23.0), (12.0, 29.0)],
    ];
    let cut = |p: P| -> f32 {
        cracks
            .iter()
            .take(rank as usize)
            .map(|k| seg_dist(k[0], k[1], p).min(seg_dist(k[1], k[2], p)))
            .fold(f32::MAX, f32::min)
    };
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let (px, py) = (c(x), c(y));
        let (dx, dy) = (px - cx, py - cy);
        let a = dy.atan2(dx);
        let edge = r * (1.0 + 0.06 * (a * 5.0 + phase).sin() + 0.04 * (a * 3.0).cos());
        let d = dx.hypot(dy);
        if d > edge {
            return None;
        }
        let n = d / edge;
        let lit = -(dx + dy) / d.max(0.1);
        let mut t = 0.55 - (dx + dy) / r * 0.22;
        if n > 0.84 {
            t += lit * 0.2;
        }
        match kind {
            Wax::Plain => {}
            Wax::Seal => {
                // The stamp's pressed-in ring, its far wall lit.
                if (0.62..0.74).contains(&n) {
                    t -= lit * 0.25;
                }
                match Mark::Star.emboss(dx / 6.0, dy / 6.0, (1.0 / 6.0, 1.0 / 6.0)) {
                    Some(true) => t = 0.85,
                    Some(false) => t = 0.22,
                    None => {}
                }
            }
            Wax::Cracked => {
                if cut((px, py)) < 0.7 {
                    return None;
                }
                if cut((px - 1.0, py - 1.0)) < 0.7 {
                    t = 0.18;
                } else if cut((px + 1.0, py + 1.0)) < 0.7 {
                    t = 0.85;
                }
            }
        }
        Some(w.pick(t, x, y))
    });
    pips_right(&mut icon, if kind == Wax::Cracked { rank } else { 0 });
    icon.finish()
}

// ------------------------------------------------------------- materials

/// Charcoal: a charred log end and a smaller chunk, crazed into squares,
/// rings on the cut end, a few embers still in the cracks.
fn charcoal() -> Tex {
    let ch = char_ramp();
    let ember = Rgba::hex(0xe8742a);
    let salt = seed("charcoal");
    let mut icon = Icon::new();
    for (i, &(a, b, w)) in [
        ((19.0f32, 27.0f32), (26.5f32, 20.5f32), 3.6f32),
        ((6.5, 24.5), (20.0, 11.0), 6.2),
    ]
    .iter()
    .enumerate()
    {
        let (ax, ay) = (b.0 - a.0, b.1 - a.1);
        let len = ax.hypot(ay);
        let (ux, uy) = (ax / len, ay / len);
        icon.layer(true, |x, y| {
            let p = (c(x), c(y));
            let (dx, dy) = (p.0 - b.0, p.1 - b.1);
            let de = dx.hypot(dy);
            if de < w {
                // The cut end: growth rings and a split or two.
                let ring = (de * 0.9).rem_euclid(1.6) < 0.55;
                let t = 0.62 - de / w * 0.2 - (dx + dy) / w * 0.08 - if ring { 0.18 } else { 0.0 };
                return Some(ch.pick(t, x, y));
            }
            if seg_dist(a, b, p) >= w {
                return None;
            }
            let (qx, qy) = (p.0 - a.0, p.1 - a.1);
            let u = qx * ux + qy * uy;
            let v = -qx * uy + qy * ux;
            let mut t = 0.42 + v / w * 0.25;
            let col = (u / 3.4).floor();
            let crack = u.rem_euclid(3.4) < 0.7 || (v + col * 1.3).rem_euclid(3.2) < 0.6;
            if crack {
                if unit(hash2(salt, (col as i32 * 7 + i as i32 * 31 + (v * 0.5) as i32) as u32)) > 0.82 {
                    return Some(ember);
                }
                t = 0.06;
            }
            Some(ch.pick(t, x, y))
        });
    }
    icon.finish()
}

/// Draws one fossil as an icon in fossil bone.
fn fossil_icon(fossils: &[Fossil]) -> Tex {
    let bone = bone_ramp();
    let mut icon = Icon::new();
    for f in fossils {
        icon.layer(true, |x, y| f.tone((c(x), c(y))).map(|t| bone.pick(t + 0.08, x, y)));
    }
    icon.finish()
}

/// Fossil essence: a glowing ivory-gold drop with four small sparkles.
fn fossil_essence() -> Tex {
    let glow = ramp(0xf4dc9a, 0.45, 0.99);
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        let (cx, cy, r) = (15.5f32, 19.5f32, 9.0f32);
        let (dx, dy) = (px - cx, py - cy);
        let d = dx.hypot(dy);
        let inside = if py >= cy {
            d < r
        } else {
            let k = ((py - 3.0) / (cy - 3.0)).clamp(0.0, 1.0);
            dx.abs() < r * k.powf(0.75)
        };
        if !inside {
            return None;
        }
        if (px - (cx - 3.0)).hypot(py - (cy - 2.0)) < 1.8 {
            return Some(glow.lightest());
        }
        Some(glow.pick(0.95 - d / r * 0.45 + (-(dx + dy) / r) * 0.1, x, y))
    });
    for &(sx, sy) in &[(26.5f32, 6.5f32), (5.5, 9.5), (27.5, 22.5), (4.5, 26.5)] {
        icon.layer(true, |x, y| {
            let (dx, dy) = ((c(x) - sx).abs(), (c(y) - sy).abs());
            ((dx < 0.6 && dy < 2.1) || (dy < 0.6 && dx < 2.1)).then(|| glow.pick(0.9, x, y))
        });
    }
    icon.finish()
}

// -------------------------------------------------------------- lookups

/// Paints the icon called `name`, or `None` if this module does not know it.
pub fn paint_icon(name: &str) -> Option<Tex> {
    let rank = |n: &str| -> u32 { n.rsplit('_').next().and_then(|d| d.parse().ok()).unwrap_or(0) };
    let base = name
        .trim_end_matches(|ch: char| ch.is_ascii_digit())
        .trim_end_matches('_');
    let r = rank(name);
    Some(match base {
        "charcoal" => charcoal(),
        "small_fossil" => fossil_icon(&[Fossil::Fan(16.0, 16.0, 10.5, 0.0)]),
        "medium_fossil" => fossil_icon(&[Fossil::Coil(16.0, 16.0, 13.5, 0.9)]),
        "fossil_essence" => fossil_essence(),
        // Bases: each family's container in plain stuff.
        "tincture_base" => flask(0x9ac8e8, None, 0.3, 0),
        "glaze_base" => jar(0xc0805a, Mark::None, false),
        "salve_base" => tin(0xa8b0c0, None, Mark::None, 0),
        "spur_base" => spur(0xd0d4e0, 0, name),
        "wash_base" => bottle(0xdcd8c8, false, 0),
        "wax_base" => wax(0xe0c890, Wax::Plain, 0, name),
        // Tinctures.
        "pure_tincture" => tincture(0xf4f8ff, None, r),
        "rough_tincture" => tincture(0xd88a28, None, r),
        "wild_tincture" => tincture(0x4ec040, None, r),
        "sapping_tincture" => tincture(0x5a74a8, None, r),
        "tainted_tincture" => tincture(0x5a2a78, None, r),
        "mending_tincture" => tincture(0xe0506a, None, r),
        "wild_mending_tincture" => tincture(0xe0506a, Some(0x6ad050), r),
        "calming_tincture" => tincture(0x8a9cf0, None, r),
        "rough_calming_tincture" => tincture(0x8a9cf0, Some(0xe8a030), r),
        // Glazes.
        "thrift_glaze" => jar(0xe0b030, Mark::None, true),
        "swift_glaze" => jar(0x58b8f0, Mark::None, true),
        "odd_glaze" => jar(0xe040b0, Mark::None, true),
        "force_glaze" => jar(0xd83030, Mark::None, true),
        "lasting_glaze" => jar(0x8a5a30, Mark::None, true),
        "glow_glaze" => jar(0xfff0a0, Mark::None, true),
        "elder_glaze" => jar(0x30a8a0, Mark::None, true),
        "keeping_glaze" => jar(0x9aa6c0, Mark::Ring, true),
        "seeking_glaze" => jar(0xe88a40, Mark::Arrow, true),
        // Salves.
        "shield_salve" => tin(0x5a80c0, None, Mark::Shield, r),
        "boost_salve" => tin(0xf08a30, None, Mark::Up, r),
        "flaw_guard_salve" => tin(0xc83838, None, Mark::Ring, r),
        "quirk_guard_salve" => tin(0x9050c8, None, Mark::Diamond, r),
        "vigour_salve" => tin(0x50b040, None, Mark::Plus, r),
        "steady_salve" => tin(0xb09868, None, Mark::Bars, r),
        "fate_salve" => tin(0xe0c040, None, Mark::Star, r),
        "turnabout_salve" => tin(0xc83838, Some(0x4a70c8), Mark::None, 0),
        "gamble_salve" => tin(0xd04890, None, Mark::Dots, r),
        // Spurs.
        "flaw_turning_spur" => spur(0xe04848, 0, name),
        "vigour_spur" => spur(0x50c040, r, name),
        "boost_spur" => spur(0xf08a30, 0, name),
        "points_spur" => spur(0x40d0e0, r, name),
        // Washes.
        "boon_wash" => bottle(0x60d070, false, r),
        "flaw_wash" => bottle(0xd84040, false, r),
        "quirk_wash" => bottle(0xa050d8, false, r),
        "boon_shift_wash" => bottle(0x60d070, true, 0),
        "flaw_shift_wash" => bottle(0xd84040, true, 0),
        "quirk_shift_wash" => bottle(0xa050d8, true, 0),
        // Waxes.
        "sealing_wax" => wax(0xc02838, Wax::Seal, 0, name),
        "unmaking_wax" => wax(0x6a5a8a, Wax::Cracked, r, name),
        _ => return None,
    })
}

/// The painter for one of this module's block textures.
pub fn painter(name: &str) -> Option<fn(Ctx) -> Tex> {
    Some(match name {
        "forge_top" => forge_top,
        "forge_side" => forge_side,
        "small_fossil_seam" => small_fossil_seam,
        "medium_fossil_seam" => medium_fossil_seam,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every milestone 17 item has its own icon from this module, and no two
    /// are alike.
    #[test]
    fn every_forging_item_has_a_distinct_icon() {
        let blocks = bm_core::BlockRegistry::builtin();
        let items = bm_core::ItemRegistry::builtin(&blocks);
        let mut names: Vec<String> = items
            .defs()
            .iter()
            .filter(|d| matches!(d.kind, bm_core::ItemKind::Ingredient(_)))
            .map(|d| d.name.clone())
            .collect();
        assert_eq!(names.len(), 66, "the forging ingredients");
        for n in [
            "charcoal",
            "small_fossil",
            "medium_fossil",
            "fossil_essence",
            "tincture_base",
            "glaze_base",
            "salve_base",
            "spur_base",
            "wash_base",
            "wax_base",
        ] {
            names.push(n.to_string());
        }
        let icons: Vec<Tex> = names
            .iter()
            .map(|n| paint_icon(n).unwrap_or_else(|| panic!("no forge icon for `{n}`")))
            .collect();
        for i in 0..icons.len() {
            for j in i + 1..icons.len() {
                assert_ne!(icons[i], icons[j], "`{}` and `{}` look the same", names[i], names[j]);
            }
        }
        for n in ["forge_top", "forge_side", "small_fossil_seam", "medium_fossil_seam"] {
            assert!(painter(n).is_some(), "no painter for `{n}`");
        }
    }
}
