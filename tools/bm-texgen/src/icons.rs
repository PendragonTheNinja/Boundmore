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
use crate::materials::{coal_ramp, copper_ramp, native_iron_ramp, plank_ramp, rust_ramp, steel_ramp, stone_ramp};
use crate::noise::{Rng, hash2, seed, unit};
use crate::tex::{SIZE, Tex, c};

/// The outline round every icon, and the contact line where one part
/// overlaps another: a deep purple-black rather than flat black.
pub const OUTLINE: Rgba = Rgba::hex(0x1a1222);

const CLEAR: Rgba = Rgba::new(0.0, 0.0, 0.0, 0.0);

fn cord_ramp() -> Ramp {
    Ramp::from_hex(&[0x4a2c22, 0x7a5232, 0xa8804e, 0xd0aa70, 0xeed49c])
}

fn leather_ramp() -> Ramp {
    Ramp::from_hex(&[0x2a1220, 0x4a2026, 0x6c322c, 0x8c4a36, 0xa86642])
}

// ----------------------------------------------------------------- drawing

/// An icon being drawn, layer by layer, back to front.
struct Icon {
    tex: Tex,
}

impl Icon {
    fn new() -> Self {
        Self {
            tex: Tex::filled(CLEAR),
        }
    }

    fn opaque(&self, x: i32, y: i32) -> bool {
        (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) && self.tex.get(x as usize, y as usize).a > 0.0
    }

    /// Draws one part. `f` gives the colour of each pixel the part covers.
    /// Where it lies over earlier parts, they get a dark contact line along
    /// its edge: strong below and right of it (its shadow), softer above and
    /// left. `contact: false` joins faces of one solid without a line.
    fn layer(&mut self, contact: bool, f: impl Fn(usize, usize) -> Option<Rgba>) {
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
    fn finish(self) -> Tex {
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

type P = (f32, f32);

fn inside_poly(poly: &[P], x: f32, y: f32) -> bool {
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

fn in_triangle(a: P, b: P, c: P, p: P) -> bool {
    let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

fn seg_dist(a: P, b: P, p: P) -> f32 {
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
struct Lump {
    poly: Vec<P>,
    apex: P,
    tilt: Vec<f32>,
}

impl Lump {
    fn new(salt: u32, cx: f32, cy: f32, r: f32, sides: usize) -> Self {
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
    fn facet(&self, p: P) -> Option<usize> {
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
    fn draw(&self, icon: &mut Icon, ramp: &Ramp, lo: f32, hi: f32, glint: bool) {
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
fn lashing(icon: &mut Icon, f: Frame, uc: f32, half_u: f32, half_v: f32) {
    let cord = cord_ramp();
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
    Stone,
    Copper,
    Iron,
}

impl Metal {
    fn ramp(self) -> Ramp {
        match self {
            Metal::Stone => stone_ramp(),
            Metal::Copper => copper_ramp(),
            Metal::Iron => steel_ramp(),
        }
    }

    /// The seed of this metal's version of `tool`.
    fn salt(self, tool: &str) -> u32 {
        let metal = match self {
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
    let round = if metal == Metal::Stone { 2.2 } else { 1.0 };
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
        if metal != Metal::Stone {
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
    if metal == Metal::Stone {
        lashing(&mut icon, f, uc - 0.3, 4.3, 1.9);
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
                t = if metal == Metal::Stone { 0.8 } else { 0.92 };
                if metal == Metal::Stone && ((u * 1.2).floor() as i32) % 2 == 0 {
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
        if metal != Metal::Stone && (du - 1.0).abs() < 0.6 && v < -3.5 && v - edge > 2.2 {
            t += 0.2;
        }
        Some(ramp.pick(t, x, y))
    });
    if metal == Metal::Stone {
        lashing(&mut icon, f, uc, 3.4, 1.9);
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
        if metal != Metal::Stone && (v + 2.2).abs() < 0.5 && u > b0 + 1.5 && u < b1 - 3.0 {
            t += 0.22;
        }
        Some(ramp.pick(t, x, y))
    });
    match metal {
        Metal::Stone => lashing(&mut icon, f, b0 - 0.6, 2.2, 1.9),
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
fn ingot(ramp: Ramp) -> Tex {
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

/// Paints the icon called `name`, or `None` if no painter knows it.
pub fn paint_icon(name: &str) -> Option<Tex> {
    Some(match name {
        "coal" => coal(),
        "copper_ore" => copper_ore(),
        "copper_ingot" => ingot(copper_ramp()),
        "iron_ore" => iron_ore(),
        "iron_ingot" => ingot(steel_ramp()),
        "stone_hammer" => hammer(Metal::Stone),
        "stone_shovel" => shovel(Metal::Stone),
        "stone_axe" => axe(Metal::Stone),
        "copper_hammer" => hammer(Metal::Copper),
        "copper_shovel" => shovel(Metal::Copper),
        "copper_axe" => axe(Metal::Copper),
        "iron_hammer" => hammer(Metal::Iron),
        "iron_shovel" => shovel(Metal::Iron),
        "iron_axe" => axe(Metal::Iron),
        _ => return None,
    })
}
