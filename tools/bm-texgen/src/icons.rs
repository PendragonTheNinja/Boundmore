//! Item icons: 32 x 32 sprites on a transparent background for every item
//! that is not a block (`assets/items.toml`), written to
//! `assets/textures/items/<icon>.png`.
//!
//! The rules follow the block brief where they apply (light from the top
//! left, hue-shifted ramps, chunky shapes) plus two of their own: a 1 pixel
//! dark outline round the whole sprite, so it reads on any slot colour, and
//! shapes big enough to survive being shown at 16 x 16. Tools share one
//! composition: the handle runs from the bottom left to the top right with
//! the head at the top, so a hotbar of tools reads as a set.

use std::f32::consts::FRAC_1_SQRT_2;

use crate::color::{Ramp, Rgba};
use crate::materials::{
    coal_ramp, copper_ramp, end_grain_ramp, flame_ramp, native_iron_ramp, plank_ramp, rust_ramp, steel_ramp, stone_ramp,
};
use crate::noise::{Rng, hash2, seed, unit};
use crate::tex::{SIZE, Tex, c};

/// The outline round every icon, and the contact line where one part
/// overlaps another: a deep purple-black rather than flat black.
pub const OUTLINE: Rgba = Rgba::hex(0x1a1222);

const CLEAR: Rgba = Rgba::new(0.0, 0.0, 0.0, 0.0);

pub(crate) fn cord_ramp() -> Ramp {
    Ramp::from_hex(&[0x4a2c22, 0x7a5232, 0xa8804e, 0xd0aa70, 0xeed49c])
}

pub(crate) fn vine_ramp() -> Ramp {
    Ramp::from_hex(&[0x123a2c, 0x1d5a3a, 0x2f7f3c, 0x4aaa3e, 0x86c940])
}

pub(crate) fn flint_ramp() -> Ramp {
    Ramp::from_hex(&[0x0e0c1c, 0x1a1830, 0x2a2848, 0x3c3c62, 0x55587e, 0x7c80a2, 0xb4b8d0])
}

pub(crate) fn leather_ramp() -> Ramp {
    Ramp::from_hex(&[0x2a1220, 0x4a2026, 0x6c322c, 0x8c4a36, 0xa86642])
}

// ----------------------------------------------------------------- drawing

/// An icon being drawn, layer by layer, back to front.
pub(crate) struct Icon {
    tex: Tex,
}

impl Icon {
    pub(crate) fn new() -> Self {
        Self {
            tex: Tex::filled(CLEAR),
        }
    }

    pub(crate) fn opaque(&self, x: i32, y: i32) -> bool {
        (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) && self.tex.get(x as usize, y as usize).a > 0.0
    }

    /// Draws one part. `f` gives the colour of each pixel the part covers.
    /// Where it lies over earlier parts, they get a dark contact line along
    /// its edge: strong below and right of it (its shadow), softer above and
    /// left. `contact: false` joins faces of one solid without a line.
    pub(crate) fn layer(&mut self, contact: bool, f: impl Fn(usize, usize) -> Option<Rgba>) {
        let cover: Vec<Option<Rgba>> = (0..SIZE * SIZE).map(|i| f(i % SIZE, i / SIZE)).collect();
        let covered = |x: i32, y: i32| {
            (0..SIZE as i32).contains(&x)
                && (0..SIZE as i32).contains(&y)
                && cover[y as usize * SIZE + x as usize].is_some()
        };
        if contact {
            let before = self.tex.clone();
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let (ix, iy) = (x as i32, y as i32);
                    if covered(ix, iy) || !self.opaque(ix, iy) {
                        continue;
                    }
                    let k = if covered(ix - 1, iy) || covered(ix, iy - 1) {
                        0.7
                    } else if covered(ix + 1, iy) || covered(ix, iy + 1) {
                        0.45
                    } else {
                        continue;
                    };
                    self.tex.set(x, y, before.get(x, y).mix(OUTLINE, k));
                }
            }
        }
        for (i, col) in cover.into_iter().enumerate() {
            if let Some(col) = col {
                self.tex.set(i % SIZE, i / SIZE, col);
            }
        }
    }

    /// Outlines the sprite: every clear pixel beside an opaque one.
    pub(crate) fn finish(self) -> Tex {
        let mut out = self.tex.clone();
        for y in 0..SIZE {
            for x in 0..SIZE {
                let (ix, iy) = (x as i32, y as i32);
                if self.opaque(ix, iy) {
                    continue;
                }
                if self.opaque(ix - 1, iy)
                    || self.opaque(ix + 1, iy)
                    || self.opaque(ix, iy - 1)
                    || self.opaque(ix, iy + 1)
                {
                    out.set(x, y, OUTLINE);
                }
            }
        }
        out
    }
}

pub(crate) type P = (f32, f32);

pub(crate) fn inside_poly(poly: &[P], x: f32, y: f32) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0) {
            inside = !inside;
        }
    }
    inside
}

fn cross(o: P, a: P, b: P) -> f32 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

pub(crate) fn in_triangle(a: P, b: P, c: P, p: P) -> bool {
    let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

pub(crate) fn seg_dist(a: P, b: P, p: P) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.0 - a.0 - dx * t).hypot(p.1 - a.1 - dy * t)
}

// ----------------------------------------------------------------- lumps

/// A faceted lump (coal, ore, a nugget): an irregular polygon cut into
/// triangular facets that meet at an apex nudged towards the light, each
/// facet shaded by how far it turns to the top left.
pub(crate) struct Lump {
    poly: Vec<P>,
    apex: P,
    tilt: Vec<f32>,
}

impl Lump {
    pub(crate) fn new(salt: u32, cx: f32, cy: f32, r: f32, sides: usize) -> Self {
        let mut rng = Rng::new(salt);
        let start = rng.range(0.0, std::f32::consts::TAU);
        let poly = (0..sides)
            .map(|i| {
                let a = start + (i as f32 + rng.range(-0.25, 0.25)) / sides as f32 * std::f32::consts::TAU;
                let rr = r * rng.range(0.82, 1.08);
                (cx + a.cos() * rr, cy + a.sin() * rr * 0.9)
            })
            .collect();
        let tilt = (0..sides).map(|_| rng.range(0.75, 1.25)).collect();
        Self {
            poly,
            apex: (cx - r * 0.28, cy - r * 0.3),
            tilt,
        }
    }

    /// Brightness 0..1 of facet `i` under light from the top left.
    fn facet_light(&self, i: usize) -> f32 {
        let n = self.poly.len();
        let (a, b) = (self.poly[i], self.poly[(i + 1) % n]);
        let m = ((a.0 + b.0) * 0.5 - self.apex.0, (a.1 + b.1) * 0.5 - self.apex.1);
        let len = m.0.hypot(m.1).max(1e-3);
        let s = self.tilt[i];
        let nrm = (m.0 / len * s, m.1 / len * s, 1.0);
        let nl = (nrm.0 * nrm.0 + nrm.1 * nrm.1 + 1.0).sqrt();
        let l = (-0.5f32, -0.62f32, 0.6f32);
        let ll = (l.0 * l.0 + l.1 * l.1 + l.2 * l.2).sqrt();
        ((nrm.0 * l.0 + nrm.1 * l.1 + nrm.2 * l.2) / (nl * ll)).clamp(0.0, 1.0)
    }

    /// Which facet a point is in, if it is inside the lump.
    pub(crate) fn facet(&self, p: P) -> Option<usize> {
        if !inside_poly(&self.poly, p.0, p.1) {
            return None;
        }
        let n = self.poly.len();
        (0..n)
            .find(|&i| in_triangle(self.apex, self.poly[i], self.poly[(i + 1) % n], p))
            .or(Some(0))
    }

    /// Draws the lump through `ramp`; `lo..hi` is the tone range from the
    /// facet in deepest shade to the one facing the light.
    pub(crate) fn draw(&self, icon: &mut Icon, ramp: &Ramp, lo: f32, hi: f32, glint: bool) {
        let n = self.poly.len();
        let lights: Vec<f32> = (0..n).map(|i| self.facet_light(i)).collect();
        let best = (0..n).max_by(|&a, &b| lights[a].total_cmp(&lights[b])).unwrap();
        icon.layer(true, |x, y| {
            let p = (c(x), c(y));
            let i = self.facet(p)?;
            let mut t = lo + (hi - lo) * lights[i];
            // Ridges between facets catch a little light.
            let ridge = seg_dist(self.apex, self.poly[i], p).min(seg_dist(self.apex, self.poly[(i + 1) % n], p));
            if ridge < 0.6 {
                t += 0.08;
            }
            if glint && i == best && (p.0 - self.apex.0).hypot(p.1 - self.apex.1) < 1.3 {
                return Some(ramp.lightest());
            }
            Some(ramp.pick(t, x, y))
        });
    }
}

// ----------------------------------------------------------------- tools

/// The tool frame: `u` runs along the handle towards the top right, `v`
/// across it towards the bottom right. The origin sits on a pixel centre so
/// the handle's edges are clean 45 degree steps.
#[derive(Clone, Copy)]
struct Frame {
    ox: f32,
    oy: f32,
}

impl Frame {
    fn uv(&self, x: usize, y: usize) -> (f32, f32) {
        let (dx, dy) = (c(x) - self.ox, c(y) - self.oy);
        ((dx - dy) * FRAC_1_SQRT_2, (dx + dy) * FRAC_1_SQRT_2)
    }
}

/// Signed distance to a box centred on (cu, cv) with half sizes (hu, hv)
/// and corners rounded by `round`.
fn rounded_box(u: f32, v: f32, cu: f32, cv: f32, hu: f32, hv: f32, round: f32) -> f32 {
    let qu = (u - cu).abs() - (hu - round);
    let qv = (v - cv).abs() - (hv - round);
    qu.max(0.0).hypot(qv.max(0.0)) + qu.max(qv).min(0.0) - round
}

/// Half the handle's width: three pixel diagonals.
const HANDLE: f32 = 1.1;

/// A wooden handle from `u0` to `u1`, round in section (lit on its top-left
/// side), with a leather grip wound round its lower end.
fn handle(icon: &mut Icon, f: Frame, u0: f32, u1: f32, salt: u32) {
    let wood = plank_ramp();
    let leather = leather_ramp();
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        let along = u.clamp(u0, u1);
        if (u - along).hypot(v) > HANDLE {
            return None;
        }
        let side = -v / HANDLE;
        if u < u0 + 5.5 {
            let band = ((u - u0) * FRAC_1_SQRT_2 * 2.0).round() as i32 % 3 == 2;
            let t = 0.5 + side * 0.32 - if band { 0.24 } else { 0.0 };
            return Some(leather.pick(t, x, y));
        }
        // Grain in short clumps along the handle.
        let clump = unit(hash2(salt, (u * 0.6).floor() as i32 as u32)) - 0.5;
        let t = 0.55 + side * 0.3 + clump * 0.12;
        Some(wood.pick(t, x, y))
    });
}

/// Cord lashed round a stone head and its handle: a band across the handle
/// at `uc`, wound in alternating light and dark turns.
fn lashing(icon: &mut Icon, f: Frame, metal: Metal, uc: f32, half_u: f32, half_v: f32) {
    // Wood tools are bound with green withies (foliage in their recipe).
    let cord = if metal == Metal::Wood { vine_ramp() } else { cord_ramp() };
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        if (u - uc).abs() > half_u || v.abs() > half_v {
            return None;
        }
        let turn = ((x + y) / 2) % 2 == 0;
        let t = 0.6 - v / half_v * 0.25 + if turn { 0.15 } else { -0.2 };
        Some(cord.pick(t, x, y))
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Metal {
    Wood,
    Stone,
    Copper,
    Iron,
}

impl Metal {
    /// Cast metal: smooth, with a sheen and a glint.
    fn cast(self) -> bool {
        matches!(self, Metal::Copper | Metal::Iron)
    }

    fn ramp(self) -> Ramp {
        match self {
            Metal::Wood => end_grain_ramp(),
            Metal::Stone => stone_ramp(),
            Metal::Copper => copper_ramp(),
            Metal::Iron => steel_ramp(),
        }
    }

    /// The seed of this metal's version of `tool`.
    fn salt(self, tool: &str) -> u32 {
        let metal = match self {
            Metal::Wood => "wood",
            Metal::Stone => "stone",
            Metal::Copper => "copper",
            Metal::Iron => "iron",
        };
        seed(&format!("{metal}_{tool}"))
    }

    /// Knapped stone breaks into flat facets; cast copper is smooth.
    fn facet(self, salt: u32, u: f32, v: f32) -> f32 {
        match self {
            Metal::Stone => {
                let (i, j) = ((u / 2.6).floor() as i32, ((v + u * 0.4) / 2.6).floor() as i32);
                (unit(hash2(salt, (i * 31 + j) as u32)) - 0.5) * 0.2
            }
            // Carved wood: grain running along the head, in short streaks.
            Metal::Wood => {
                let streak = (unit(hash2(salt, ((v * 0.8).floor() as i32 + 40) as u32)) - 0.5) * 0.18;
                let knot = if (u * 0.5).floor() as i32 % 5 == 2 && v.abs() < 1.0 {
                    -0.12
                } else {
                    0.0
                };
                streak + knot
            }
            Metal::Copper | Metal::Iron => 0.0,
        }
    }
}

fn hammer(metal: Metal) -> Tex {
    let salt = metal.salt("hammer");
    let f = Frame { ox: 5.5, oy: 26.5 };
    let ramp = metal.ramp();
    let mut icon = Icon::new();
    handle(&mut icon, f, 0.0, 24.0, salt);
    let (uc, hu, hv) = (21.0, 3.6, 7.8);
    let round = if !metal.cast() { 2.2 } else { 1.0 };
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        if rounded_box(u, v, uc, 0.0, hu, hv, round) > 0.0 {
            return None;
        }
        let mut t = 0.5 - v / hv * 0.22 + metal.facet(salt, u, v);
        // End faces: the top-left one in full light, the other in shade.
        if v < -hv + 1.3 {
            t += 0.3;
        }
        if v > hv - 1.3 {
            t -= 0.3;
        }
        // Top-right face catches some light, the underside falls away.
        if u > uc + hu - 1.0 {
            t += 0.14;
        }
        if u < uc - hu + 1.0 {
            t -= 0.2;
        }
        if metal.cast() {
            // A long sheen on the cast metal and a glint at its lit end.
            if (u - (uc + 1.2)).abs() < 0.6 && v < hv - 2.0 {
                t += 0.2;
            }
            if (u - (uc + 1.2)).abs() < 0.8 && v < -hv + 2.2 {
                return Some(ramp.lightest());
            }
            // The handle's wedged end showing through the eye.
            if (u - (uc + hu - 0.8)).abs() < 0.6 && v.abs() < 0.8 {
                return Some(plank_ramp().at(5));
            }
        }
        Some(ramp.pick(t, x, y))
    });
    if !metal.cast() {
        lashing(&mut icon, f, metal, uc - 0.3, 4.3, 1.9);
    }
    icon.finish()
}

/// A hand tiller (milestone 16 part 2): a short handle ending in a crossbar
/// with three claw tines that curl down to bite the soil.
fn tiller(metal: Metal) -> Tex {
    let salt = metal.salt("tiller");
    let f = Frame { ox: 4.5, oy: 27.5 };
    let ramp = metal.ramp();
    let mut icon = Icon::new();
    handle(&mut icon, f, 0.0, 18.5, salt);
    let (ub, u0, len, curl) = (18.8f32, 19.6f32, 6.6f32, 4.2f32);
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        // The crossbar.
        if rounded_box(u, v, ub, 0.0, 1.5, 6.0, 0.8) <= 0.0 {
            let mut t = 0.5 - v / 6.0 * 0.2 + metal.facet(salt, u, v);
            if u > ub + 0.5 {
                t += 0.14;
            }
            if metal.cast() && (u - ub - 0.3).abs() < 0.5 && v < 3.5 {
                t += 0.22;
            }
            return Some(ramp.pick(t, x, y));
        }
        // Three tines, curling towards the bottom right, tapering.
        for v0 in [-4.6f32, 0.0, 4.6] {
            let k = (u - u0) / len;
            if !(-0.2..=1.0).contains(&k) {
                continue;
            }
            let centre = v0 + k.max(0.0).powi(2) * curl;
            let half = 1.55 - k.max(0.0) * 0.8;
            let d = v - centre;
            if d.abs() > half {
                continue;
            }
            let mut t = 0.55 - d / half * 0.25 + metal.facet(salt, u, v0);
            if k > 0.8 {
                t += 0.15;
            }
            if metal.cast() && d < -0.3 && k > 0.1 && k < 0.8 {
                t += 0.2;
            }
            return Some(ramp.pick(t, x, y));
        }
        None
    });
    if metal.cast() {
        // A ferrule where the handle meets the head.
        icon.layer(true, |x, y| {
            let (u, v) = f.uv(x, y);
            ((15.6..17.2).contains(&u) && v.abs() < 1.7).then(|| ramp.pick(0.62 - v / 1.7 * 0.3, x, y))
        });
    } else {
        lashing(&mut icon, f, metal, 16.4, 1.6, 1.8);
    }
    icon.finish()
}

fn axe(metal: Metal) -> Tex {
    let salt = metal.salt("axe");
    let f = Frame { ox: 7.5, oy: 27.5 };
    let ramp = metal.ramp();
    let mut icon = Icon::new();
    handle(&mut icon, f, 0.0, 27.0, salt);
    let uc = 22.0;
    let edge_v = -10.0;
    let bit_half = 5.8;
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        let du = u - uc;
        // The eye round the handle, with a short poll behind it.
        let eye = du.abs() <= 2.8 && (-2.6..=3.2).contains(&v);
        // The blade, widening to a curved bit.
        let w = 2.8 + ((-v - 2.5) / 7.5).clamp(0.0, 1.0) * (bit_half - 2.8);
        let edge = edge_v + (du / bit_half).powi(2) * 3.2;
        let blade = v < -2.4 && v >= edge && du.abs() <= w;
        if !eye && !blade {
            return None;
        }
        let mut t = 0.5 + metal.facet(salt, u, v);
        if blade {
            t += ((-v - 2.5) / 7.5).clamp(0.0, 1.0) * 0.12;
            if v - edge < 1.5 {
                // The honed edge.
                t = if !metal.cast() { 0.8 } else { 0.92 };
                if !metal.cast() && ((u * 1.2).floor() as i32) % 2 == 0 {
                    t -= 0.12;
                }
            }
        }
        if du > w - 1.0 || (eye && du > 1.8) {
            t += 0.14;
        }
        if du < -w + 1.0 || (eye && du < -1.8) {
            t -= 0.2;
        }
        if eye && v > 2.2 {
            t -= 0.22;
        }
        if metal.cast() && (du - 1.0).abs() < 0.6 && v < -3.5 && v - edge > 2.2 {
            t += 0.2;
        }
        Some(ramp.pick(t, x, y))
    });
    if !metal.cast() {
        lashing(&mut icon, f, metal, uc, 3.4, 1.9);
    } else {
        // The wedged end of the handle in the eye.
        icon.layer(false, |x, y| {
            let (u, v) = f.uv(x, y);
            ((u - (uc + 2.0)).abs() < 0.6 && v.abs() < 0.8).then(|| plank_ramp().at(5))
        });
    }
    icon.finish()
}

fn shovel(metal: Metal) -> Tex {
    let salt = metal.salt("shovel");
    let f = Frame { ox: 6.5, oy: 25.5 };
    let ramp = metal.ramp();
    let wood = plank_ramp();
    let mut icon = Icon::new();
    // The T grip at the end of the handle.
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        if rounded_box(u, v, -1.5, 0.0, 1.2, 3.8, 0.8) > 0.0 {
            return None;
        }
        let t = 0.55 - v / 3.8 * 0.2 + if u > -0.9 { 0.18 } else { -0.12 };
        Some(wood.pick(t, x, y))
    });
    handle(&mut icon, f, 0.0, 19.5, salt);
    let (b0, b1, half) = (19.0f32, 31.5f32, 4.7f32);
    let shoulder = 25.5;
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        if u < b0 || u > b1 {
            return None;
        }
        let w = if u < shoulder {
            half
        } else {
            half * (1.0 - ((u - shoulder) / (b1 - shoulder)).powi(2)).max(0.0).sqrt()
        };
        if v.abs() > w {
            return None;
        }
        // Dished: the near rim lit, the hollow and the far rim in shade.
        let mut t = 0.54 - v / half * 0.18 + metal.facet(salt, u, v);
        if v < -w + 1.1 {
            t += 0.26;
        }
        if v > w - 1.1 {
            t -= 0.26;
        }
        if u < b0 + 0.9 {
            t -= 0.16;
        }
        // A pressed spine up the middle, and a lighter worn tip.
        if v.abs() < 0.45 && u < shoulder {
            t += 0.14;
        }
        if u > b1 - 2.5 {
            t += 0.12;
        }
        if metal.cast() && (v + 2.2).abs() < 0.5 && u > b0 + 1.5 && u < b1 - 3.0 {
            t += 0.22;
        }
        Some(ramp.pick(t, x, y))
    });
    match metal {
        Metal::Stone | Metal::Wood => lashing(&mut icon, f, metal, b0 - 0.6, 2.2, 1.9),
        Metal::Copper | Metal::Iron => {
            // A riveted collar round the top of the handle.
            icon.layer(true, |x, y| {
                let (u, v) = f.uv(x, y);
                if (u - (b0 - 1.2)).abs() > 1.6 || v.abs() > 1.9 {
                    return None;
                }
                if (u - (b0 - 1.2)).abs() < 0.5 && v.abs() < 0.5 {
                    return Some(ramp.lightest());
                }
                Some(ramp.pick(0.5 - v / 1.9 * 0.3, x, y))
            });
        }
    }
    icon.finish()
}

// ---------------------------------------------------------------- spanners

/// A spanner (milestone 15): an open jaw at the top right like every
/// tool's head. Stone: a knapped head lashed to a wooden handle. Cast: one
/// piece of metal with a leather grip.
fn spanner(metal: Metal) -> Tex {
    let salt = metal.salt("spanner");
    let f = Frame { ox: 6.5, oy: 25.5 };
    let ramp = metal.ramp();
    let uc = 22.5f32;
    let mut icon = Icon::new();
    if metal.cast() {
        icon.layer(true, |x, y| {
            let (u, v) = f.uv(x, y);
            if u < -0.5 || u > uc - 3.0 || v.abs() > 1.8 {
                return None;
            }
            if u < 7.0 {
                let band = ((u * FRAC_1_SQRT_2 * 2.0).round() as i32) % 3 == 2;
                let t = 0.5 - v / 1.8 * 0.32 - if band { 0.24 } else { 0.0 };
                return Some(leather_ramp().pick(t, x, y));
            }
            let mut t = 0.52 - v / 1.8 * 0.3;
            if (v + 0.9).abs() < 0.4 && u > 9.0 {
                t += 0.2;
            }
            Some(ramp.pick(t, x, y))
        });
    } else {
        handle(&mut icon, f, 0.0, uc - 3.0, salt);
    }
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        let du = u - uc;
        let r = du.hypot(v);
        if r > 6.4 {
            return None;
        }
        // The jaw: a slot open to the top right.
        if du > -1.2 && v.abs() < 2.2 {
            return None;
        }
        let mut t = 0.52 - v / 6.4 * 0.22 + metal.facet(salt, u, v);
        if r > 5.3 {
            t += if v < 0.0 { 0.26 } else { -0.24 };
        }
        // The jaw's walls: the far one faces the light.
        if du > -2.4 && v.abs() < 3.2 {
            t += if v > 0.0 { 0.2 } else { -0.22 };
        }
        if metal.cast() && (du + 2.8).abs() < 0.7 && v < -3.5 {
            return Some(ramp.lightest());
        }
        Some(ramp.pick(t, x, y))
    });
    if !metal.cast() {
        lashing(&mut icon, f, metal, uc - 5.6, 1.6, 1.9);
    }
    icon.finish()
}

// ----------------------------------------------------------------- chisels

#[derive(Clone, Copy, PartialEq)]
enum Tip {
    Square,
    Slope,
    Bevel,
    Round,
}

/// A chisel: a short handle with a leather grip, a ferrule (lashing on
/// stone), and a flat blade whose tip tells the family. Precise chisels
/// are slimmer, taper to a needle and carry a gem on the ferrule.
fn chisel(metal: Metal, tip: Tip, precise: bool) -> Tex {
    let name = match (tip, precise) {
        (Tip::Square, false) => "square",
        (Tip::Slope, false) => "slope",
        (Tip::Bevel, false) => "bevel",
        (Tip::Round, false) => "round",
        (Tip::Square, true) => "precise_square",
        (Tip::Slope, true) => "precise_slope",
        (Tip::Bevel, true) => "precise_bevel",
        (Tip::Round, true) => "precise_round",
    };
    let salt = metal.salt(&format!("{name}_chisel"));
    let f = if precise {
        Frame { ox: 3.5, oy: 28.5 }
    } else {
        Frame { ox: 4.5, oy: 27.5 }
    };
    let ramp = metal.ramp();
    let mut icon = Icon::new();
    let (b0, ut) = (13.5f32, if precise { 32.5f32 } else { 30.5f32 });
    let w0 = if precise { 2.4 } else { 3.1 };
    handle(&mut icon, f, 0.0, 14.5, salt);
    // The blade half width at `u`, and where its tip ends at `v`.
    let half = move |u: f32| {
        if precise {
            let k = ((u - (b0 + 5.5)) / (ut - b0 - 5.5)).clamp(0.0, 1.0);
            w0 + (0.7 - w0) * k
        } else {
            w0
        }
    };
    let end = move |v: f32| {
        let w = w0;
        match tip {
            Tip::Square => ut,
            Tip::Slope => ut - (v + w) * if precise { 0.9 } else { 1.2 },
            Tip::Bevel => ut - v.abs() * if precise { 1.0 } else { 2.3 } - 0.2,
            Tip::Round => ut - w * (1.0 - (1.0 - (v / w).powi(2)).max(0.0).sqrt()),
        }
    };
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        let hw = half(u);
        if u < b0 || v.abs() > hw {
            return None;
        }
        let e = end(if precise { v * w0 / hw.max(0.5) } else { v });
        if u > e {
            return None;
        }
        let mut t = 0.52 - v / hw * 0.2 + metal.facet(salt, u, v);
        if v < -hw + 1.0 {
            t += 0.26;
        }
        if v > hw - 1.0 {
            t -= 0.26;
        }
        if tip == Tip::Round && !precise && v.abs() < 1.1 && u > b0 + 4.0 {
            // The gouge's hollow, in shade on its far wall.
            t -= if v > 0.0 { 0.12 } else { 0.0 } + 0.16;
        }
        if metal.cast() && (v + 0.9).abs() < 0.5 && u > b0 + 2.0 && u < e - 3.0 {
            t += 0.2;
        }
        // The honed edge at the tip.
        if e - u < 1.5 {
            t = if !metal.cast() { 0.82 } else { 0.95 };
        }
        Some(ramp.pick(t, x, y))
    });
    if !metal.cast() {
        lashing(&mut icon, f, metal, b0 + 0.2, 2.6, 2.2);
    } else {
        // The ferrule, wider than the handle.
        icon.layer(true, |x, y| {
            let (u, v) = f.uv(x, y);
            if (u - (b0 - 0.4)).abs() > 1.7 || v.abs() > 2.0 {
                return None;
            }
            let mut t = 0.5 - v / 2.0 * 0.3;
            if (u - (b0 - 0.4)).abs() < 0.5 && v < 0.2 {
                t += 0.25;
            }
            Some(ramp.pick(t, x, y))
        });
    }
    if precise {
        // A gem set in the ferrule.
        let gem = Ramp::from_hex(&[0x1c2a78, 0x2f5fd0, 0x4fa0ff, 0x9fe4ff, 0xe8fbff]);
        icon.layer(true, |x, y| {
            let (u, v) = f.uv(x, y);
            let (du, dv) = (u - (b0 - 0.2), v);
            if du.abs() + dv.abs() > 1.45 {
                return None;
            }
            let t = if du - dv > 0.3 {
                0.95
            } else if du + dv > 0.3 {
                0.4
            } else {
                0.75
            };
            Some(gem.pick(t, x, y))
        });
    }
    icon.finish()
}

// ----------------------------------------------------------------- torches

/// A plain stick along the tool diagonal, bark lit on its top-left side,
/// with a cut end at the top showing pale wood.
fn stick() -> Tex {
    let salt = seed("stick");
    let f = Frame { ox: 4.5, oy: 27.5 };
    let wood = plank_ramp();
    let r = HANDLE + 0.9;
    let mut icon = Icon::new();
    // A twig stub partway up, so it reads as a stick, not a handle.
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        let along = u - 12.0;
        (along > 0.0 && along < 5.0 && (v + along * 0.9 + 1.2).abs() < 1.0).then(|| wood.pick(0.62 - v * 0.04, x, y))
    });
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        let along = u.clamp(0.0, 27.0);
        if (u - along).hypot(v) > r {
            return None;
        }
        if u > 26.0 {
            // The cut end, pale wood.
            return Some(end_grain_ramp().pick(0.75 - v * 0.08, x, y));
        }
        let side = -v / r;
        let clump = unit(hash2(salt, (u * 0.5).floor() as i32 as u32)) - 0.5;
        Some(wood.pick(0.5 + side * 0.32 + clump * 0.14, x, y))
    });
    icon.finish()
}

/// A knapped flint: a dark blue-black nodule with a pale chalky rind on one
/// side and a glassy glint where it was struck.
fn flint() -> Tex {
    let salt = seed("flint");
    let mut icon = Icon::new();
    Lump::new(salt, 16.0, 17.0, 11.0, 7).draw(&mut icon, &flint_ramp(), 0.1, 0.95, true);
    // The rind, on the lower right.
    let rind = Ramp::from_hex(&[0x8a8296, 0xb4aab4, 0xd8d0cc, 0xf0eae0]);
    let lump = Lump::new(salt, 16.0, 17.0, 11.0, 7);
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        lump.facet(p)?;
        let edge = (p.0 - 16.0) * 0.55 + (p.1 - 17.0) * 0.8;
        (edge > 6.0).then(|| rind.pick(0.35 + (edge - 6.0) * 0.08, x, y))
    });
    icon.finish()
}

/// The hand stone: a fist-sized cobble with one knapped edge, the tool a
/// player makes from nothing.
fn hand_stone() -> Tex {
    let salt = seed("hand_stone");
    let mut icon = Icon::new();
    Lump::new(salt, 15.5, 17.5, 12.0, 7).draw(&mut icon, &stone_ramp(), 0.14, 0.86, false);
    // The worked edge along the top right, pale where flakes came off.
    let ramp = stone_ramp();
    let lump = Lump::new(salt, 15.5, 17.5, 12.0, 7);
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        lump.facet(p)?;
        let edge = (p.0 - 15.5) * 0.7 - (p.1 - 17.5) * 0.7;
        if edge < 5.5 {
            return None;
        }
        let scallop = ((p.0 + p.1) * 0.7).sin() * 0.12;
        Some(ramp.pick(0.72 + scallop + (edge - 5.5) * 0.04, x, y))
    });
    icon.finish()
}

/// The torch: a stick standing a little tilted, its top charred, under a
/// flame of red, orange and gold with a pale heart.
fn torch() -> Tex {
    torch_icon(false)
}

/// The tallow torch: the torch with a fat head of cloth soaked in pale
/// tallow and bound with cord, under a bigger, hotter flame.
fn tallow_torch() -> Tex {
    torch_icon(true)
}

fn torch_icon(tallow: bool) -> Tex {
    let salt = seed(if tallow { "tallow_torch_icon" } else { "torch_icon" });
    let f = Frame { ox: 9.5, oy: 27.5 };
    let wood = plank_ramp();
    let fire = flame_ramp();
    let soot = Ramp::from_hex(&[0x1e1420, 0x2a1a24, 0x3a2228, 0x4e2e2a]);
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        let along = u.clamp(0.0, 17.0);
        if (u - along).hypot(v) > HANDLE + 0.4 {
            return None;
        }
        let side = -v / HANDLE;
        if u > 13.5 {
            return Some(soot.pick(0.5 + side * 0.3, x, y));
        }
        let clump = unit(hash2(salt, (u * 0.5).floor() as i32 as u32)) - 0.5;
        Some(wood.pick(0.5 + side * 0.3 + clump * 0.14, x, y))
    });
    if tallow {
        // The head: turns of tallow-soaked cloth, fatter than the stick,
        // each turn lit on its top-left side, bound with cord at its foot.
        let cloth = crate::materials::tallow_cloth_ramp();
        let cord = cord_ramp();
        icon.layer(true, |x, y| {
            let (u, v) = f.uv(x, y);
            if !(10.0..=17.5).contains(&u) || v.abs() > 3.0 {
                return None;
            }
            let side = -v / 3.0;
            if u < 11.8 {
                let turn = (x + y) % 2 == 0;
                return Some(cord.pick(0.5 + side * 0.25 + if turn { 0.15 } else { -0.15 }, x, y));
            }
            let band = ((u - 11.8) / 1.9).floor() as i32 % 2 == 0;
            let t = 0.58 + side * 0.3 + if band { 0.1 } else { -0.12 };
            Some(cloth.pick(t, x, y))
        });
    }
    // The flame: a teardrop pointing up, centred on the stick's top.
    let (tx, ty) = if tallow { (21.0f32, 11.5f32) } else { (21.5f32, 12.0f32) };
    let (size, height, hot) = if tallow {
        (6.4f32, 11.0f32, 0.1f32)
    } else {
        (5.2, 9.5, 0.0)
    };
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - tx, c(y) - ty);
        // Round below, drawn up into a tip; a lick to one side.
        let r = if dy > 0.0 {
            size
        } else {
            size * (1.0 + dy / height).max(0.0)
        };
        let lean = dx + dy * 0.18;
        if lean.hypot(dy.max(0.0) * 1.1) > r || dy < -height {
            return None;
        }
        let heat = 1.0 - lean.hypot(dy - 1.5) / (size + 0.3);
        let wave = (unit(hash2(salt, (y / 2) as u32)) - 0.5) * 0.08;
        Some(fire.pick(0.12 + hot + heat.max(0.0) * 0.95 + wave, x, y))
    });
    icon.finish()
}

// ----------------------------------------------------------------- materials

fn coal() -> Tex {
    let salt = seed("coal");
    let ramp = coal_ramp();
    let mut icon = Icon::new();
    Lump::new(hash2(salt, 2), 20.5, 11.5, 7.0, 6).draw(&mut icon, &ramp, 0.16, 0.9, true);
    Lump::new(hash2(salt, 1), 12.5, 18.5, 9.4, 7).draw(&mut icon, &ramp, 0.18, 0.95, true);
    Lump::new(hash2(salt, 3), 23.0, 23.0, 5.4, 6).draw(&mut icon, &ramp, 0.16, 0.9, true);
    icon.finish()
}

fn iron_ore() -> Tex {
    let salt = seed("iron_ore");
    let mut icon = Icon::new();
    Lump::new(salt, 16.0, 17.0, 12.2, 8).draw(&mut icon, &stone_ramp(), 0.12, 0.8, false);
    // Broad knots of rust-red ore, with steel-blue nodules bright in them.
    let rust = rust_ramp();
    for (i, &(x, y, r)) in [(12.5, 14.0, 5.2), (19.0, 20.5, 4.6)].iter().enumerate() {
        Lump::new(hash2(salt, 10 + i as u32), x, y, r, 6).draw(&mut icon, &rust, 0.3, 0.92, false);
    }
    let steel = native_iron_ramp();
    for (i, &(x, y, r)) in [
        (9.0, 11.0, 2.4),
        (21.0, 10.0, 2.8),
        (22.0, 23.0, 2.2),
        (10.0, 22.0, 2.6),
    ]
    .iter()
    .enumerate()
    {
        Lump::new(hash2(salt, 20 + i as u32), x, y, r, 5).draw(&mut icon, &steel, 0.3, 0.98, true);
    }
    icon.finish()
}

fn copper_ore() -> Tex {
    let salt = seed("copper_ore");
    let mut icon = Icon::new();
    Lump::new(salt, 16.0, 17.0, 12.2, 8).draw(&mut icon, &stone_ramp(), 0.12, 0.8, false);
    let copper = copper_ramp();
    for (i, &(x, y, r)) in [
        (11.5, 13.0, 3.6),
        (19.5, 11.0, 2.8),
        (17.5, 20.5, 3.3),
        (10.0, 21.5, 2.4),
    ]
    .iter()
    .enumerate()
    {
        Lump::new(hash2(salt, 10 + i as u32), x, y, r, 5).draw(&mut icon, &copper, 0.25, 0.95, true);
    }
    icon.finish()
}

/// A bevelled metal bar in three-quarter view: a tapered top face in the
/// light, the long front face, the shaded end, a maker's mark on top.
pub(crate) fn ingot(ramp: Ramp) -> Tex {
    let (len, depth, height) = (22.0f32, 9.0f32, 7.5f32);
    let (ox, oy) = (2.0f32, 23.0f32);
    let proj = |x: f32, y: f32, z: f32| (ox + x + z * 0.72, oy - y - z * 0.62);
    let (ix0, ix1, iz0, iz1) = (2.4, len - 2.4, 1.8, depth - 1.8);
    let top = [
        proj(ix0, height, iz0),
        proj(ix1, height, iz0),
        proj(ix1, height, iz1),
        proj(ix0, height, iz1),
    ];
    let front = [proj(0.0, 0.0, 0.0), proj(len, 0.0, 0.0), top[1], top[0]];
    let end = [proj(len, 0.0, 0.0), proj(len, 0.0, depth), top[2], top[1]];
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&end, p.0, p.1) {
            return None;
        }
        let mut t = 0.3;
        if seg_dist(end[3], end[2], p) < 0.8 {
            t += 0.16;
        }
        Some(ramp.pick(t, x, y))
    });
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&front, p.0, p.1) {
            return None;
        }
        // Lit along its top edge, darker towards the foot and the right.
        let mut t = 0.62 - (p.0 - ox) / len * 0.1;
        if seg_dist(front[3], front[2], p) < 0.9 {
            t += 0.24;
        }
        if seg_dist(front[0], front[1], p) < 0.9 {
            t -= 0.16;
        }
        Some(ramp.pick(t, x, y))
    });
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&top, p.0, p.1) {
            return None;
        }
        let mut t = 0.76 - (p.0 - ox) / len * 0.12;
        if seg_dist(top[0], top[1], p) < 0.9 || seg_dist(top[3], top[0], p) < 0.9 {
            t += 0.16;
        }
        // A diagonal sheen across the face.
        let s = p.0 + (p.1 - oy) * 0.9;
        if (s - 8.0).abs() < 1.0 {
            t += 0.24;
        }
        Some(ramp.pick(t, x, y))
    });
    // The maker's mark: a small stamped diamond, its far walls lit.
    let (mx, my) = proj(len * 0.62, height, depth * 0.5);
    icon.layer(false, |x, y| {
        let (dx, dy) = (c(x) - mx, c(y) - my);
        let d = dx.abs() + dy.abs() * 1.4;
        if d > 2.2 {
            return None;
        }
        let t = if dx + dy < 0.0 { 0.4 } else { 0.8 };
        Some(ramp.pick(t, x, y))
    });
    icon.finish()
}

// ----------------------------------------------------------------- grapples

/// A grapple: a three-clawed hook on a short shaft, pointing to the top
/// right like every tool, its rope running back to a coil (or, on a
/// hookshot, to a winch drum that reels it in).
fn grapple(metal: Metal, hookshot: bool) -> Tex {
    let salt = metal.salt(if hookshot { "hookshot" } else { "grapple" });
    let f = Frame { ox: 4.5, oy: 27.5 };
    let ramp = metal.ramp();
    let cord = cord_ramp();
    let mut icon = Icon::new();
    // The rope from the coil or drum up to the shaft's eye.
    icon.layer(false, |x, y| {
        let (u, v) = f.uv(x, y);
        let wave = (u * 0.9).sin() * 0.5;
        if !(5.0..=13.0).contains(&u) || (v - wave).abs() > 0.75 {
            return None;
        }
        let turn = ((u * FRAC_1_SQRT_2 * 2.0).round() as i32) % 2 == 0;
        Some(cord.pick(if turn { 0.7 } else { 0.45 }, x, y))
    });
    if hookshot {
        // The winch: a drum wound with rope between two metal cheeks, set
        // square across the shaft line.
        icon.layer(true, |x, y| {
            let (u, v) = f.uv(x, y);
            if rounded_box(u, v, 4.5, 0.0, 3.6, 5.4, 1.0) > 0.0 {
                return None;
            }
            let side = -v / 5.4;
            if (u - 4.5).abs() > 2.2 {
                let t = 0.5 + side * 0.3 + metal.facet(salt, u, v);
                return Some(ramp.pick(if u < 4.5 { t + 0.15 } else { t - 0.1 }, x, y));
            }
            let turn = ((v * FRAC_1_SQRT_2 * 2.0).round() as i32) % 2 == 0;
            Some(cord.pick(0.5 + side * 0.25 + if turn { 0.12 } else { -0.15 }, x, y))
        });
        // The crank's knob.
        icon.layer(true, |x, y| {
            let (u, v) = f.uv(x, y);
            if (u - 1.0).hypot(v - 4.6) > 1.3 {
                return None;
            }
            Some(plank_ramp().pick(0.6 - (v - 4.6) * 0.2, x, y))
        });
    } else {
        // A coil of rope: a ring wound in light and dark turns.
        let (cx, cy, r0, r1) = (9.5f32, 22.0f32, 3.2f32, 7.2f32);
        icon.layer(true, |x, y| {
            let (dx, dy) = (c(x) - cx, (c(y) - cy) * 1.15);
            let r = dx.hypot(dy);
            if r < r0 || r > r1 {
                return None;
            }
            let ring = ((r - r0) / (r1 - r0) * 3.0).floor() as i32;
            let turn = (dy.atan2(dx) * 4.0).floor() as i32 + ring;
            // Lit on the outer top left, shaded towards the hole.
            let lit = -(dx + dy) / r * 0.18;
            let t = 0.5 + lit + if turn % 2 == 0 { 0.12 } else { -0.12 } - if ring == 0 { 0.15 } else { 0.0 };
            Some(cord.pick(t, x, y))
        });
    }
    // The shaft, with an eye where the rope ties on.
    let (s0, head) = (12.0f32, 23.5f32);
    icon.layer(true, |x, y| {
        let (u, v) = f.uv(x, y);
        let eye = (u - s0).hypot(v) < 1.9;
        if !eye && !((s0..=head + 4.0).contains(&u) && v.abs() <= 1.0) {
            return None;
        }
        if eye && (u - s0).hypot(v) < 0.8 {
            return Some(OUTLINE);
        }
        // The point beyond the claws narrows to a tip.
        if u > head && v.abs() > 1.0 - (u - head) / 4.0 {
            return None;
        }
        let t = 0.55 - v * 0.25 + metal.facet(salt, u, v);
        Some(ramp.pick(if u > head + 2.5 { 0.92 } else { t }, x, y))
    });
    // Two claws arching out from the head and back towards the rope, like
    // an umbrella's ribs, their points lit.
    for side in [-1.0f32, 1.0] {
        icon.layer(true, |x, y| {
            let (u, v) = f.uv(x, y);
            let (cu, r) = (head - 4.2, 4.6);
            let d = (u - cu).hypot(v);
            if (d - r).abs() > 1.0 || u < cu - 0.8 || v * side < 0.4 {
                return None;
            }
            let tip = u < cu + 1.1;
            let t = 0.55 - v * 0.08 - (d - r) * 0.2;
            Some(ramp.pick(if tip { 0.92 } else { t }, x, y))
        });
    }
    icon.finish()
}

// ----------------------------------------------------------------- portals

/// A crystal blade from its foot at (x, y) at angle `ang` (screen
/// radians, 0 = right, y down), `len` long and `half` wide: a long hexagonal
/// prism, three faces visible (the one towards the top left in the light,
/// the far one in shade, a front face between them under a bright ridge),
/// a faceted point at the far end and a short worn point at the foot.
pub(crate) fn blade(icon: &mut Icon, foot: P, ang: f32, len: f32, half: f32, ramp: &Ramp) {
    let (ax, ay) = (ang.cos(), ang.sin());
    // Which side of the axis faces the light: +1 when the -v side does.
    let lit = if ax - ay > 0.0 { 1.0 } else { -1.0 };
    let (tip, heel) = (half * 2.0, half * 0.9);
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - foot.0, c(y) - foot.1);
        let u = dx * ax + dy * ay;
        let v = (-dx * ay + dy * ax) * lit;
        if u < 0.0 || u > len {
            return None;
        }
        let w = half * ((len - u) / tip).min(1.0).min(u / heel + 0.25);
        if v.abs() > w {
            return None;
        }
        let third = w / 3.0;
        let mut t = if v < -third {
            0.74
        } else if v > third {
            0.28
        } else {
            0.5
        };
        // Brighter towards the point, as if lit from inside.
        t += u / len * 0.16;
        if (v + third).abs() < 0.5 {
            t += 0.16;
        }
        // The point's facets: the lit ones flare.
        if u > len - tip && v < third {
            t += 0.14;
        }
        if u < heel {
            t -= 0.14;
        }
        Some(ramp.pick(t, x, y))
    });
}

/// Wayshard: a long violet-blue crystal blade pointing to the top right,
/// a shorter one grown against its foot standing more upright, and a white
/// glint near the long one's point. Slim and pointed, where copper and iron
/// ore are rounded lumps of rock.
fn wayshard() -> Tex {
    let ramp = crate::materials::wayshard_ramp();
    let mut icon = Icon::new();
    let up = -std::f32::consts::FRAC_PI_4;
    blade(&mut icon, (8.0, 27.0), up - 0.55, 16.0, 3.2, &ramp);
    blade(&mut icon, (6.0, 27.5), up, 29.0, 4.4, &ramp);
    blade(&mut icon, (10.0, 28.5), up + 0.62, 13.0, 2.6, &ramp);
    icon.layer(false, |x, y| {
        let (dx, dy) = (c(x) - 22.5, c(y) - 10.5);
        (dx.abs() + dy.abs() < 1.1).then(|| ramp.lightest())
    });
    icon.finish()
}

/// Portal token: a stone tag like a key's bow, framed in copper, a wayshard
/// set in its middle, hung from a copper ring at the top and cut with a
/// key's bit of teeth along its foot, so it reads as a key to a place.
fn portal_token() -> Tex {
    let salt = seed("portal_token");
    let stone = crate::materials::conduit_stone_ramp();
    let copper = copper_ramp();
    let shard = crate::materials::wayshard_ramp();
    let mut icon = Icon::new();
    // The ring at the top, behind the tag.
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 6.0);
        let r = dx.hypot(dy);
        if !(2.0..=4.4).contains(&r) {
            return None;
        }
        Some(copper.pick(0.55 - (dx + dy) / r * 0.28, x, y))
    });
    // The tag: a shield-like slab with chamfered top corners and teeth at
    // the foot.
    let (x0, x1, y0, y1) = (6.0f32, 26.0f32, 8.0f32, 27.0f32);
    let tooth = |x: f32| -> f32 {
        // Foot line: three teeth of different depths.
        match ((x - x0) / 4.0).floor() as i32 {
            0 => y1 + 2.0,
            1 => y1,
            2 => y1 + 3.0,
            3 => y1 + 1.0,
            _ => y1 + 2.5,
        }
    };
    let inside = move |px: f32, py: f32| {
        px >= x0
            && px <= x1
            && py >= y0
            && py <= tooth(px)
            && (px - x0) + (py - y0) >= 3.0
            && (x1 - px) + (py - y0) >= 3.0
    };
    icon.layer(true, |x, y| {
        let (px, py) = (c(x), c(y));
        if !inside(px, py) {
            return None;
        }
        // The copper frame: 2 pixels in from every edge.
        let near = |dx: f32, dy: f32| !inside(px + dx, py + dy);
        let rim = near(-2.0, 0.0) || near(2.0, 0.0) || near(0.0, -2.0) || near(0.0, 2.0);
        let lead = near(-1.0, 0.0) || near(0.0, -1.0) || near(-1.0, -1.0);
        let trail = near(1.0, 0.0) || near(0.0, 1.0) || near(1.0, 1.0);
        if rim {
            let t = if lead {
                0.82
            } else if trail {
                0.28
            } else {
                0.56
            };
            return Some(copper.pick(t, x, y));
        }
        // Stone face, a little mottled, shaded under the frame's top lip.
        let mut t = 0.5 + (unit(hash2(salt, (x / 3 + y / 3 * 11) as u32)) - 0.5) * 0.12;
        if near(-3.0, 0.0) || near(0.0, -3.0) {
            t -= 0.18;
        }
        if near(3.0, 0.0) || near(0.0, 3.0) {
            t += 0.1;
        }
        Some(stone.pick(t, x, y))
    });
    // The wayshard, cut on its point, in a copper bezel.
    let (gx, gy) = (16.0f32, 17.0f32);
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - gx, c(y) - gy);
        let d = dx.abs() + dy.abs() * 0.8;
        if d > 6.2 {
            return None;
        }
        if d > 4.6 {
            let facing = -(dx + dy) / (dx.abs() + dy.abs()).max(0.1);
            return Some(copper.pick(0.55 + facing * 0.3, x, y));
        }
        let t = match (dx < 0.0, dy < 0.0) {
            (true, true) => 0.74,
            (false, true) => 0.56,
            (true, false) => 0.44,
            (false, false) => 0.26,
        };
        if (dx + 1.5).abs() < 0.6 && (dy + 1.5).abs() < 0.6 {
            return Some(shard.lightest());
        }
        Some(shard.pick(if d < 1.2 { 0.86 } else { t }, x, y))
    });
    icon.finish()
}

/// Paints the icon called `name`, or `None` if no painter knows it.
pub fn paint_icon(name: &str) -> Option<Tex> {
    Some(match name {
        "coal" => coal(),
        "copper_ore" => copper_ore(),
        "copper_ingot" => ingot(copper_ramp()),
        "iron_ore" => iron_ore(),
        "iron_ingot" => ingot(steel_ramp()),
        "wood_hammer" => hammer(Metal::Wood),
        "wood_shovel" => shovel(Metal::Wood),
        "wood_axe" => axe(Metal::Wood),
        "hand_stone" => hand_stone(),
        "stick" => stick(),
        "flint" => flint(),
        "torch" => torch(),
        "tallow_torch" => tallow_torch(),
        "stone_hammer" => hammer(Metal::Stone),
        "stone_shovel" => shovel(Metal::Stone),
        "stone_axe" => axe(Metal::Stone),
        "copper_hammer" => hammer(Metal::Copper),
        "copper_shovel" => shovel(Metal::Copper),
        "copper_axe" => axe(Metal::Copper),
        "iron_hammer" => hammer(Metal::Iron),
        "iron_shovel" => shovel(Metal::Iron),
        "iron_axe" => axe(Metal::Iron),
        "square_chisel" => chisel(Metal::Stone, Tip::Square, false),
        "slope_chisel" => chisel(Metal::Copper, Tip::Slope, false),
        "bevel_chisel" => chisel(Metal::Iron, Tip::Bevel, false),
        "round_chisel" => chisel(Metal::Iron, Tip::Round, false),
        "precise_square_chisel" => chisel(Metal::Iron, Tip::Square, true),
        "precise_slope_chisel" => chisel(Metal::Iron, Tip::Slope, true),
        "copper_grapple" => grapple(Metal::Copper, false),
        "iron_grapple" => grapple(Metal::Iron, false),
        "wayshard" => wayshard(),
        "portal_token" => portal_token(),
        "stone_spanner" => spanner(Metal::Stone),
        "copper_spanner" => spanner(Metal::Copper),
        "iron_spanner" => spanner(Metal::Iron),
        "wood_tiller" => tiller(Metal::Wood),
        "metal_tiller" => tiller(Metal::Iron),
        other => {
            return crate::gathered::paint(other)
                .or_else(|| crate::food::paint(other))
                .or_else(|| crate::farming::paint_icon(other))
                .or_else(|| crate::creatures::paint(other))
                .or_else(|| crate::spark::paint_icon(other));
        }
    })
}
