//! Gathering objects (milestone 11): the plants, boulders and fungi that
//! grow on every world, drawn as small models (bm-mesh `props`).
//!
//! - **Plants** are pictures on a clear background: the model shows the
//!   whole texture on two crossed upright quads, and the renderer cuts out
//!   every texel whose alpha is under one half. Alpha is therefore only ever
//!   0 or 1. Each plant stands with its foot on the bottom row, in the
//!   middle; shapes are at least two or three pixels thick so they survive
//!   the mipmaps.
//! - **Boulders** are opaque: the model's tops take the top-left quarter
//!   (columns 0..16 of rows 0..16), its sides the lower half (rows 16..32),
//!   the bottom row resting on the ground.
//! - **Fungi** are opaque: the cap's top is the top-left quarter, its rim
//!   columns 16..32 of rows 0..8, the gills seen from below columns 16..32
//!   of rows 8..16, the stem the lower half's left side (columns 0..16 of
//!   rows 16..32; the rest repeats it).
//!
//! Every one is Boundmore's own: invented silhouettes and patterns, painted
//! with the brief's rules (light from the top left, hue-shifted ramps built
//! from the catalogue colour, clumped detail rather than noise).

use crate::color::{HUE_BLUE, HUE_GOLD, HUE_PURPLE, Ramp, RampSpec, Rgba};
use crate::noise::{Rng, Site, cell, fbm, hash2, scatter_into, seed, unit, value};
use crate::tex::{Ctx, SIZE, Tex, c, smoothstep};

const CLEAR: Rgba = Rgba::new(0.0, 0.0, 0.0, 0.0);

/// A ramp around a catalogue colour, `dark` to `light` in Oklab lightness.
fn ramp(rgb: u32, dark: f32, light: f32) -> Ramp {
    RampSpec::new(Rgba::hex(rgb), 7, dark, light).build()
}

fn leaf_ramp(rgb: u32) -> Ramp {
    RampSpec::new(Rgba::hex(rgb), 7, 0.3, 0.86)
        .cool(HUE_BLUE, 0.3)
        .warm(HUE_GOLD, 0.3)
        .chroma(1.0, 0.9)
        .build()
}

// ------------------------------------------------------------ plant toolkit

type P = (f32, f32);

/// How a blade's half width runs from its foot (t = 0) to its tip (t = 1).
#[derive(Clone, Copy)]
enum Profile {
    /// Widest at the foot, to a sharp point.
    Spike,
    /// Narrow at the foot, widest a third of the way, pointed.
    Leaf,
    /// An ellipse: a petal, a pad, a bell.
    Round,
    /// The same width all along: a stem.
    Strap,
    /// Narrow at the foot, flaring open at the end: a trumpet.
    Flare,
}

impl Profile {
    fn at(self, t: f32) -> f32 {
        match self {
            Profile::Spike => (1.0 - t).powf(0.85) * (t * 5.0 + 0.55).min(1.0),
            Profile::Leaf => (std::f32::consts::PI * t.powf(0.7)).sin().max(0.0),
            Profile::Round => (1.0 - (2.0 * t - 1.0).powi(2)).max(0.0).sqrt(),
            Profile::Strap => 1.0,
            Profile::Flare => 0.3 + 0.7 * t.powf(1.6),
        }
    }
}

/// A blade drawn along a quadratic curve from `foot` to `tip`, bowed by
/// `bend` pixels to its left (seen walking from foot to tip).
#[derive(Clone, Copy)]
struct Blade {
    foot: P,
    tip: P,
    bend: f32,
    width: f32,
    profile: Profile,
    /// Tone offset: back blades darker, front ones lighter.
    tone: f32,
    /// A paler midrib down the middle.
    rib: bool,
}

impl Blade {
    fn point(&self, t: f32) -> P {
        let (dx, dy) = (self.tip.0 - self.foot.0, self.tip.1 - self.foot.1);
        let len = dx.hypot(dy).max(1e-3);
        let (nx, ny) = (dy / len, -dx / len);
        let ctrl = (
            (self.foot.0 + self.tip.0) * 0.5 + nx * self.bend * 2.0,
            (self.foot.1 + self.tip.1) * 0.5 + ny * self.bend * 2.0,
        );
        let u = 1.0 - t;
        (
            u * u * self.foot.0 + 2.0 * u * t * ctrl.0 + t * t * self.tip.0,
            u * u * self.foot.1 + 2.0 * u * t * ctrl.1 + t * t * self.tip.1,
        )
    }

    /// For a point: (t along the blade, signed distance across it: negative
    /// to the left, its half width there).
    fn locate(&self, p: P) -> Option<(f32, f32, f32)> {
        const STEPS: usize = 48;
        let mut best = (f32::MAX, 0.0, 0.0);
        let mut prev = self.point(0.0);
        for i in 1..=STEPS {
            let t1 = i as f32 / STEPS as f32;
            let next = self.point(t1);
            let (sx, sy) = (next.0 - prev.0, next.1 - prev.1);
            let l2 = (sx * sx + sy * sy).max(1e-6);
            let k = (((p.0 - prev.0) * sx + (p.1 - prev.1) * sy) / l2).clamp(0.0, 1.0);
            let (qx, qy) = (prev.0 + sx * k, prev.1 + sy * k);
            let d = (p.0 - qx).hypot(p.1 - qy);
            if d < best.0 {
                let side = (sx * (p.1 - qy) - sy * (p.0 - qx)).signum();
                let t = (i as f32 - 1.0 + k) / STEPS as f32;
                best = (d, t, side);
            }
            prev = next;
        }
        let (d, t, side) = best;
        let half = self.width * self.profile.at(t);
        (d <= half && half > 0.35).then_some((t, d * side, half))
    }
}

/// A plant picture being painted, back to front.
struct Sprite {
    tex: Tex,
}

impl Sprite {
    fn new() -> Self {
        Self {
            tex: Tex::filled(CLEAR),
        }
    }

    fn opaque(&self, x: i32, y: i32) -> bool {
        (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) && self.tex.get(x as usize, y as usize).a > 0.0
    }

    /// Paints every pixel `f` covers. Where the new part lies over earlier
    /// ones, they get a dark contact line just outside it, so leaves in
    /// front stand off those behind.
    fn part(&mut self, f: impl Fn(usize, usize) -> Option<Rgba>) {
        let cover: Vec<Option<Rgba>> = (0..SIZE * SIZE).map(|i| f(i % SIZE, i / SIZE)).collect();
        let covered = |x: i32, y: i32| {
            (0..SIZE as i32).contains(&x)
                && (0..SIZE as i32).contains(&y)
                && cover[y as usize * SIZE + x as usize].is_some()
        };
        let before = self.tex.clone();
        for y in 0..SIZE {
            for x in 0..SIZE {
                let (ix, iy) = (x as i32, y as i32);
                if covered(ix, iy) || !self.opaque(ix, iy) {
                    continue;
                }
                if covered(ix - 1, iy) || covered(ix, iy - 1) || covered(ix + 1, iy) || covered(ix, iy + 1) {
                    self.tex.set(x, y, before.get(x, y).mix(Rgba::hex(0x101820), 0.45));
                }
            }
        }
        for (i, col) in cover.into_iter().enumerate() {
            if let Some(col) = col {
                self.tex.set(i % SIZE, i / SIZE, col.with_alpha(1.0));
            }
        }
    }

    /// A shaded blade: lit on the side facing the top left, darker towards
    /// its foot and its edges, a paler midrib if it has one.
    fn blade(&mut self, b: &Blade, ramp: &Ramp) {
        self.part(|x, y| {
            let p = (c(x), c(y));
            let (t, across, half) = b.locate(p)?;
            let q2 = b.point((t + 0.02).min(1.0));
            let q1 = b.point((t - 0.02).max(0.0));
            let (tx, ty) = (q2.0 - q1.0, q2.1 - q1.1);
            let tl = tx.hypot(ty).max(1e-4);
            // The left normal; the side the pixel is on faces that way.
            let (nx, ny) = (ty / tl, -tx / tl);
            let s = across.signum();
            let facing = -(nx * s * 0.6 + ny * s * 0.8);
            let edge = across.abs() / half.max(0.5);
            let mut tone = 0.5 + b.tone + facing * 0.22 * edge.min(1.0) + (t - 0.4) * 0.2;
            if edge > 0.82 && half > 1.2 {
                tone -= 0.14;
            }
            if b.rib && across.abs() < 0.55 && t > 0.08 && t < 0.92 {
                tone += 0.2;
            }
            Some(ramp.pick(tone, x, y))
        });
    }

    /// A filled ellipse lit from the top left, `tone` its middle tone.
    fn disc(&mut self, centre: P, r: P, ramp: &Ramp, tone: f32) {
        self.part(|x, y| {
            let (dx, dy) = ((c(x) - centre.0) / r.0, (c(y) - centre.1) / r.1);
            let d = dx.hypot(dy);
            if d > 1.0 {
                return None;
            }
            let lit = -(dx * 0.6 + dy * 0.8);
            Some(ramp.pick(tone + lit * 0.25 - d * 0.08, x, y))
        });
    }

    fn dot(&mut self, x: i32, y: i32, col: Rgba) {
        if self.opaque(x, y) {
            self.tex.set(x as usize, y as usize, col);
        }
    }

    fn finish(self) -> Tex {
        self.tex
    }
}

/// A fan of blades from one foot: `n` blades with angles (degrees from
/// straight up, negative to the left) spread over `spread`, lengths from
/// `len`, each bowed outwards by `droop`.
#[allow(clippy::too_many_arguments)]
fn fan(
    s: &mut Sprite,
    rng: &mut Rng,
    foot: P,
    n: usize,
    spread: (f32, f32),
    len: (f32, f32),
    width: f32,
    droop: f32,
    profile: Profile,
    ramp: &Ramp,
) {
    // Back blades first (the outer ones and every other), front ones last.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| {
        let mid = (n as f32 - 1.0) / 2.0;
        -((i as f32 - mid).abs() * 10.0) as i32 + (i % 2) as i32 * 3
    });
    for (k, &i) in order.iter().enumerate() {
        let f = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 };
        let ang = (spread.0 + (spread.1 - spread.0) * f + rng.range(-6.0, 6.0)).to_radians();
        let l = rng.range(len.0, len.1) * (1.0 - 0.25 * (ang.abs() / 1.5).min(1.0));
        let tip = (foot.0 + ang.sin() * l, foot.1 - ang.cos() * l);
        let b = Blade {
            foot,
            tip,
            // Bow outwards: left blades bend left (positive), right ones right.
            bend: -(droop * ang.sin().signum() * ang.sin().abs().sqrt()),
            width: width * rng.range(0.85, 1.1),
            profile,
            tone: -0.22 + 0.3 * k as f32 / n.max(1) as f32,
            rib: true,
        };
        s.blade(&b, ramp);
    }
}

// ------------------------------------------------------------------ plants

/// Spikeleaf: a desert rosette of stiff, sword-like leaves, sun-bleached
/// olive with pale tips, two of them grown long and arching.
pub fn spikeleaf(_ctx: Ctx) -> Tex {
    let mut rng = Rng::new(seed("spikeleaf"));
    let leaves = leaf_ramp(0xb0a860);
    let mut s = Sprite::new();
    let foot = (16.0, 31.5);
    fan(
        &mut s,
        &mut rng,
        foot,
        7,
        (-80.0, 80.0),
        (14.0, 18.0),
        3.0,
        1.4,
        Profile::Spike,
        &leaves,
    );
    fan(
        &mut s,
        &mut rng,
        foot,
        6,
        (-42.0, 42.0),
        (20.0, 29.0),
        3.2,
        0.9,
        Profile::Spike,
        &leaves,
    );
    // Sun-bleached tips.
    let pale = leaves.at(6).mix(Rgba::hex(0xfff2c0), 0.4);
    for y in 0..14 {
        for x in 0..SIZE {
            if s.opaque(x as i32, y) && !s.opaque(x as i32, y - 1) {
                s.dot(x as i32, y, pale);
            }
        }
    }
    s.finish()
}

/// Hornroot: low broad leaves under three curving stems, each ending in an
/// orange trumpet flower with a dark throat.
pub fn hornroot(_ctx: Ctx) -> Tex {
    let mut rng = Rng::new(seed("hornroot"));
    let leaves = leaf_ramp(0x5a8a40);
    let flower = RampSpec::new(Rgba::hex(0xe8843a), 7, 0.45, 0.94)
        .cool(HUE_PURPLE, 0.3)
        .warm(HUE_GOLD, 0.4)
        .build();
    let mut s = Sprite::new();
    let foot = (16.0, 31.5);
    let stems = [((9.0, 11.0), 2.0), ((17.5, 4.5), -0.8), ((23.5, 12.0), -2.0)];
    for (i, &(top, bend)) in stems.iter().enumerate() {
        let st = Blade {
            foot: (15.0 + i as f32, 31.0),
            tip: top,
            bend,
            width: 1.05,
            profile: Profile::Strap,
            tone: -0.08,
            rib: false,
        };
        s.blade(&st, &leaves);
    }
    fan(
        &mut s,
        &mut rng,
        foot,
        5,
        (-80.0, 80.0),
        (9.0, 12.0),
        3.4,
        1.4,
        Profile::Leaf,
        &leaves,
    );
    // Trumpets: flaring out from each stem's top, sideways and up.
    for (i, &(top, _)) in stems.iter().enumerate() {
        let dir: f32 = [-0.9, 0.15, 0.9][i];
        let mouth = (top.0 + dir * 4.5, top.1 - 5.5);
        let b = Blade {
            foot: (top.0, top.1 + 1.0),
            tip: mouth,
            bend: -dir * 0.8,
            width: 4.2,
            profile: Profile::Flare,
            tone: 0.0,
            rib: false,
        };
        s.blade(&b, &flower);
        // The open mouth: a dark throat inside a pale lip.
        s.disc(mouth, (3.6, 2.4), &flower, 0.9);
        s.disc(
            (mouth.0 + 0.3, mouth.1 + 0.4),
            (1.9, 1.2),
            &ramp(0x8a2a18, 0.25, 0.55),
            0.3,
        );
    }
    s.finish()
}

/// Stiltfrond: a thin stem standing on splayed stilt roots, a fan of
/// drooping fronds at its top like a small palm.
pub fn stiltfrond(_ctx: Ctx) -> Tex {
    let mut rng = Rng::new(seed("stiltfrond"));
    let fronds = leaf_ramp(0x60965a);
    let bark = ramp(0x8a6a48, 0.3, 0.75);
    let mut s = Sprite::new();
    for (fx, bend) in [(10.0, -1.0), (22.0, 1.0), (16.0, 0.0)] {
        s.blade(
            &Blade {
                foot: (fx, 31.5),
                tip: (16.0, 22.0),
                bend,
                width: 1.0,
                profile: Profile::Strap,
                tone: -0.1,
                rib: false,
            },
            &bark,
        );
    }
    s.blade(
        &Blade {
            foot: (16.0, 23.0),
            tip: (15.0, 9.0),
            bend: 0.6,
            width: 1.3,
            profile: Profile::Strap,
            tone: 0.0,
            rib: false,
        },
        &bark,
    );
    let crown = (15.0, 9.5);
    // Fronds arch up and out from the crown and droop at their tips: the
    // back ones first, darker.
    let tips: [(P, f32, f32); 9] = [
        ((3.0, 4.0), 1.2, -0.2),
        ((27.0, 3.5), -1.2, -0.2),
        ((9.0, 1.0), 0.6, -0.12),
        ((21.0, 1.0), -0.6, -0.12),
        ((1.0, 15.0), -2.4, -0.04),
        ((30.0, 14.5), 2.4, -0.04),
        ((5.0, 21.0), -3.2, 0.06),
        ((26.0, 20.5), 3.2, 0.06),
        ((15.5, 1.0), 0.3, 0.1),
    ];
    for (tip, bend, tone) in tips {
        let b = Blade {
            foot: crown,
            tip,
            bend: bend + rng.range(-0.3, 0.3),
            width: 2.3,
            profile: Profile::Leaf,
            tone,
            rib: true,
        };
        s.blade(&b, &fronds);
    }
    s.finish()
}

/// Nightlily: round floating pads at its foot and a cup of violet petals
/// with a golden heart.
pub fn nightlily(_ctx: Ctx) -> Tex {
    let mut rng = Rng::new(seed("nightlily"));
    let pads = leaf_ramp(0x3e7a52);
    let petals = ramp(0x7a5aaa, 0.34, 0.9);
    let mut s = Sprite::new();
    s.blade(
        &Blade {
            foot: (16.0, 31.5),
            tip: (16.0, 20.0),
            bend: 0.5,
            width: 1.2,
            profile: Profile::Strap,
            tone: -0.1,
            rib: false,
        },
        &pads,
    );
    for (x, y, rx, ry, t) in [
        (8.0, 28.0, 6.5, 3.0, 0.38),
        (24.0, 27.5, 6.5, 3.2, 0.42),
        (16.0, 29.5, 7.0, 2.4, 0.55),
    ] {
        s.disc((x, y), (rx, ry), &pads, t);
        // The notch every pad has.
        s.part(|px, py| {
            let (dx, dy) = (c(px) - x, c(py) - y);
            (dy > 0.0 && dx.abs() < 0.6 && dy < ry).then(|| pads.at(1))
        });
    }
    let heart = (16.0, 18.5);
    fan(
        &mut s,
        &mut rng,
        heart,
        5,
        (-75.0, 75.0),
        (8.0, 10.0),
        3.2,
        0.6,
        Profile::Round,
        &petals,
    );
    fan(
        &mut s,
        &mut rng,
        heart,
        3,
        (-30.0, 30.0),
        (9.0, 11.0),
        3.0,
        0.3,
        Profile::Round,
        &petals,
    );
    s.disc((16.0, 17.0), (2.4, 1.6), &ramp(0xe0b040, 0.5, 0.95), 0.6);
    s.finish()
}

/// Ridge cactus: a ribbed column with one arm raised, pale spines along its
/// ribs and a pink flower on top.
pub fn ridge_cactus(_ctx: Ctx) -> Tex {
    let green = leaf_ramp(0x5a8c50);
    let pink = ramp(0xe070a0, 0.45, 0.92);
    let spine = Rgba::hex(0xf4ecd0);
    let mut s = Sprite::new();
    // A column: x from x0 to x1, from y0 down to the foot, round on top.
    let column = |s: &mut Sprite, x0: f32, x1: f32, y0: f32, y1: f32| {
        let w = x1 - x0;
        s.part(|x, y| {
            let (fx, fy) = (c(x), c(y));
            if fx < x0 || fx > x1 || fy > y1 {
                return None;
            }
            let u = (fx - x0) / w;
            let top = y0 + (w * 0.5) * (1.0 - (1.0 - (2.0 * u - 1.0).powi(2)).max(0.0).sqrt());
            if fy < top {
                return None;
            }
            // Ribs every three pixels: a lit crest, a dark groove.
            let rib = ((fx - x0) / 3.0).fract();
            let mut t = 0.52 - (u - 0.35) * 0.45;
            if rib < 0.34 {
                t += 0.14;
            } else if rib > 0.7 {
                t -= 0.18;
            }
            if fy < top + 1.0 {
                t += 0.14;
            }
            Some(green.pick(t, x, y))
        });
    };
    // The arm: out to the left, then up.
    s.part(|x, y| {
        let (fx, fy) = (c(x), c(y));
        ((7.0..13.0).contains(&fx) && (19.0..23.0).contains(&fy)).then(|| green.pick(0.42 - (fy - 19.0) * 0.06, x, y))
    });
    column(&mut s, 4.5, 10.5, 9.0, 22.0);
    column(&mut s, 11.0, 21.5, 5.0, 32.0);
    // Spines on the crests.
    let mut rng = Rng::new(seed("ridge_cactus"));
    for _ in 0..34 {
        let x = rng.range(4.0, 22.0) as i32;
        let y = rng.range(7.0, 31.0) as i32;
        if (x - 4) % 3 == 0 || (x - 11) % 3 == 1 {
            s.dot(x, y, spine);
        }
    }
    // The flower on the crown.
    let mut r2 = Rng::new(seed("ridge_cactus_flower"));
    fan(
        &mut s,
        &mut r2,
        (16.0, 6.0),
        5,
        (-70.0, 70.0),
        (4.5, 5.5),
        2.0,
        0.3,
        Profile::Round,
        &pink,
    );
    s.disc((16.0, 5.0), (1.4, 1.0), &ramp(0xf0d060, 0.5, 0.95), 0.6);
    s.finish()
}

/// Knot aloe: a low rosette of thick, blue-green fleshy leaves, pale
/// flecks in bands across them.
pub fn knot_aloe(_ctx: Ctx) -> Tex {
    let mut rng = Rng::new(seed("knot_aloe"));
    let leaves = leaf_ramp(0x6ea078);
    let mut s = Sprite::new();
    let foot = (16.0, 31.5);
    fan(
        &mut s,
        &mut rng,
        foot,
        6,
        (-84.0, 84.0),
        (15.0, 18.0),
        5.0,
        2.6,
        Profile::Spike,
        &leaves,
    );
    fan(
        &mut s,
        &mut rng,
        foot,
        4,
        (-45.0, 45.0),
        (17.0, 22.0),
        4.8,
        1.4,
        Profile::Spike,
        &leaves,
    );
    fan(
        &mut s,
        &mut rng,
        foot,
        2,
        (-12.0, 12.0),
        (13.0, 16.0),
        4.4,
        0.4,
        Profile::Spike,
        &leaves,
    );
    let fleck = leaves.at(6).mix(Rgba::hex(0xe8f4e0), 0.5);
    for _ in 0..40 {
        let x = rng.range(3.0, 29.0) as i32;
        let y = rng.range(10.0, 30.0) as i32;
        if y % 3 == 0 {
            s.dot(x, y, fleck);
        }
    }
    s.finish()
}

/// Frostbell: a tuft of narrow leaves and three arching stems, each
/// hanging a white bell tipped with green.
pub fn frostbell(_ctx: Ctx) -> Tex {
    let mut rng = Rng::new(seed("frostbell"));
    let leaves = leaf_ramp(0x4e8a5a);
    let bells = RampSpec::new(Rgba::hex(0xdce6f0), 7, 0.55, 0.99)
        .cool(HUE_BLUE, 0.5)
        .build();
    let mut s = Sprite::new();
    let foot = (16.0, 31.5);
    fan(
        &mut s,
        &mut rng,
        foot,
        5,
        (-40.0, 40.0),
        (12.0, 17.0),
        1.6,
        0.8,
        Profile::Leaf,
        &leaves,
    );
    for (top, bend) in [((7.0, 9.0), 3.0), ((19.0, 5.0), 2.6), ((26.5, 12.0), -2.5)] {
        s.blade(
            &Blade {
                foot: (16.0, 31.0),
                tip: top,
                bend,
                width: 0.9,
                profile: Profile::Strap,
                tone: -0.05,
                rib: false,
            },
            &leaves,
        );
        // The bell hangs down from the stem's top.
        let b = Blade {
            foot: (top.0, top.1 - 0.5),
            tip: (top.0 + 0.5, top.1 + 7.5),
            bend: 0.0,
            width: 3.0,
            profile: Profile::Flare,
            tone: 0.1,
            rib: false,
        };
        s.blade(&b, &bells);
        s.dot(top.0 as i32, top.1 as i32, leaves.at(4));
        s.dot(top.0 as i32 + 1, top.1 as i32 + 5, leaves.at(4));
    }
    s.finish()
}

/// Curlfern: fronds of paired leaflets, each rising to a tight curl, the
/// young fiddlehead not yet open.
pub fn curlfern(_ctx: Ctx) -> Tex {
    let leaves = leaf_ramp(0x50963e);
    let mut s = Sprite::new();
    let fronds: [(P, f32, f32); 5] = [
        ((5.0, 13.0), 3.0, -0.18),
        ((27.0, 12.0), -3.0, -0.14),
        ((10.0, 6.0), 1.8, -0.02),
        ((22.0, 5.0), -1.6, 0.02),
        ((16.0, 3.5), 0.4, 0.1),
    ];
    for (i, &(top, bend, tone)) in fronds.iter().enumerate() {
        let stem = Blade {
            foot: (15.5 + i as f32 * 0.25, 31.5),
            tip: top,
            bend,
            width: 0.9,
            profile: Profile::Strap,
            tone,
            rib: false,
        };
        s.blade(&stem, &leaves);
        // Leaflets in pairs down the stem, shorter towards the tip.
        for k in 1..6 {
            let t = k as f32 / 6.5;
            let at = stem.point(1.0 - t);
            let next = stem.point((1.0 - t + 0.05).min(1.0));
            let (dx, dy) = (next.0 - at.0, next.1 - at.1);
            let l = dx.hypot(dy).max(1e-3);
            let (nx, ny) = (-dy / l, dx / l);
            let size = 2.5 + t * 3.5;
            for side in [-1.0f32, 1.0] {
                s.blade(
                    &Blade {
                        foot: at,
                        tip: (
                            at.0 + nx * side * size + dx / l * 1.5,
                            at.1 + ny * side * size + dy / l * 1.5 - 1.0,
                        ),
                        bend: 0.0,
                        width: 1.3,
                        profile: Profile::Leaf,
                        tone: tone + 0.05,
                        rib: false,
                    },
                    &leaves,
                );
            }
        }
        // The curl: a spiral of shrinking dots rolled inwards at the tip.
        let turn = if bend > 0.0 { 1.0 } else { -1.0 };
        s.part(|x, y| {
            let (fx, fy) = (c(x), c(y));
            for j in 0..14 {
                let a = j as f32 * 0.55;
                let r = 2.6 * (1.0 - j as f32 / 16.0);
                let (cx, cy) = (top.0 + turn * (a.cos() * r - r), top.1 - a.sin() * r);
                if (fx - cx).hypot(fy - cy) < 1.15 - j as f32 * 0.03 {
                    return Some(leaves.pick(0.62 + tone - j as f32 * 0.015, x, y));
                }
            }
            None
        });
    }
    s.finish()
}

// ---------------------------------------------------------------- boulders

/// What makes one boulder kind its own.
struct RockLook {
    name: &'static str,
    rgb: u32,
    /// Sites per face for the chunky facets.
    facet: f32,
    pattern: RockPattern,
}

#[derive(Clone, Copy, PartialEq)]
enum RockPattern {
    /// Lichen patches on top.
    Lichen,
    /// Long vertical streaks down the sides.
    Streaks,
    /// Strata in bands across the sides and rings on top.
    Strata,
    /// One pale band round the stone.
    Ring,
    /// Glassy flaked chips with glints.
    Chips,
    /// A white salt crust in specks.
    Crust,
}

fn rock(look: &RockLook) -> Tex {
    let salt = seed(look.name);
    let r = RampSpec::new(Rgba::hex(look.rgb), 8, 0.28, 0.86)
        .cool(HUE_PURPLE, 0.35)
        .warm(HUE_GOLD, 0.2)
        .build();
    let mut sites = Vec::new();
    scatter_into(&mut sites, salt, look.facet, 1.0, 3000, |_, _| true);
    let lichen = RampSpec::new(Rgba::hex(0x8a9a58), 5, 0.5, 0.82).build();
    let crust = Rgba::hex(0xf4f0e4);
    let pale = r.at(7).mix(Rgba::hex(0xf0e0f0), 0.3);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let top = x < 16 && y < 16;
        let k = cell(
            &sites,
            fx,
            fy,
            if look.pattern == RockPattern::Strata && !top {
                2.2
            } else {
                1.0
            },
        );
        let f: &Site = &sites[k.index];
        let mut t = 0.5 + (f.r[0] - 0.5) * 0.24 + k.rim(1.6) * 0.18;
        t += (fbm(salt ^ 3, 4, 4, 2, fx, fy) - 0.5) * 0.12;
        if k.edge < 0.6 {
            t -= 0.16;
        }
        if top {
            // Seen from above: lighter, the bevel edges of the model darker.
            t += 0.12;
            let d = (fx - 8.0).abs().max((fy - 8.0).abs());
            t -= smoothstep(6.0, 8.0, d) * 0.12;
        } else if y >= 16 {
            // The side: lit just under its top edge, dark at the ground.
            let ly = y - 16;
            if ly < 2 {
                t += 0.14;
            }
            if ly >= 14 {
                t -= 0.22;
            }
            t -= ly as f32 / 15.0 * 0.12;
        }
        let mut col = r.pick(t, x, y);
        match look.pattern {
            RockPattern::Lichen => {
                let l = fbm(salt ^ 9, 4, 4, 2, fx, fy);
                if (top && l > 0.58) || ((16..19).contains(&y) && l > 0.64) {
                    col = lichen.pick(0.3 + (l - 0.58) * 3.0 + k.rim(1.2) * 0.2, x, y);
                }
            }
            RockPattern::Streaks if !top => {
                let s = value(salt ^ 5, 16, 1, fx, fy);
                if s > 0.68 {
                    col = col.mix(r.at(1), 0.45);
                } else if s < 0.22 {
                    col = col.mix(r.at(6), 0.3);
                }
            }
            RockPattern::Strata => {
                let band = if top {
                    ((fx - 8.0).hypot(fy - 8.0) / 2.6) as i32
                } else {
                    (fy / 4.0) as i32
                };
                let inside = if top {
                    ((fx - 8.0).hypot(fy - 8.0) / 2.6).fract()
                } else {
                    (fy / 4.0).fract()
                };
                if inside < 0.2 {
                    col = col.mix(r.at(0), 0.55);
                } else if band % 2 == 0 {
                    col = col.mix(r.at(5), 0.22);
                }
            }
            RockPattern::Ring => {
                let on = if top {
                    ((fx - 8.0).hypot(fy - 8.0) - 4.5).abs() < 1.1
                } else {
                    (21.0..24.0).contains(&fy)
                };
                if on {
                    col = pale.mix(r.at(4), if top { 0.2 } else { (fy - 21.0) / 6.0 });
                }
            }
            RockPattern::Chips => {
                // Conchoidal ripples round each chip's middle, a glint at
                // its top-left.
                let ripple = (k.d1 * 1.4).fract();
                if ripple < 0.25 {
                    col = col.mix(r.at(6), 0.25);
                }
                if f.r[3] < 0.35 && k.dx < -0.5 && k.dy < -0.5 && k.d1 < 1.6 {
                    col = r.at(7).mix(Rgba::hex(0xffffff), 0.4);
                }
            }
            RockPattern::Crust => {
                let h = unit(hash2(salt, (y / 2 * 16 + x / 2) as u32));
                let n = value(salt ^ 7, 8, 8, fx, fy);
                if n > 0.55 && h > 0.6 {
                    col = crust.mix(r.at(5), 0.25);
                } else if n > 0.62 {
                    col = col.mix(crust, 0.35);
                }
            }
            _ => {}
        }
        col
    })
}

macro_rules! rock_fn {
    ($name:ident, $rgb:expr, $facet:expr, $pat:expr, $doc:literal) => {
        #[doc = $doc]
        pub fn $name(_ctx: Ctx) -> Tex {
            rock(&RockLook {
                name: stringify!($name),
                rgb: $rgb,
                facet: $facet,
                pattern: $pat,
            })
        }
    };
}

rock_fn!(
    boulder,
    0x82807c,
    4.2,
    RockPattern::Lichen,
    "Boulder: grey field stone in chunky facets, lichen on top."
);
rock_fn!(
    stalk_boulder,
    0x968c78,
    4.0,
    RockPattern::Streaks,
    "Stalk boulder: tan, streaked down its sides."
);
rock_fn!(
    stacked_boulder,
    0x78746e,
    4.5,
    RockPattern::Strata,
    "Stacked boulder: layered strata, rings on top."
);
rock_fn!(
    ring_boulder,
    0x8c888c,
    4.2,
    RockPattern::Ring,
    "Ring boulder: mauve grey with a pale band round it."
);
rock_fn!(
    chip_boulder,
    0x6e6e76,
    3.6,
    RockPattern::Chips,
    "Chip boulder: dark, glassy flaked chips with glints."
);
rock_fn!(
    tapered_boulder,
    0x887e74,
    4.2,
    RockPattern::Crust,
    "Tapered boulder: warm grey under a white salt crust."
);

// ------------------------------------------------------------------- fungi

#[derive(Clone, Copy, PartialEq)]
enum CapPattern {
    /// Dark tar spots.
    Spots,
    /// Concentric ridged zones.
    Zones,
    /// Pale branching veins.
    Veins,
    /// Bright rays bursting from the middle.
    Rays,
    /// A glossy streak and drips round the rim.
    Gloss,
    /// Glowing freckles.
    Glow,
}

struct CapLook {
    name: &'static str,
    cap: u32,
    gills: u32,
    stem: u32,
    mark: u32,
    pattern: CapPattern,
}

fn fungus(look: &CapLook) -> Tex {
    let salt = seed(look.name);
    let cap = RampSpec::new(Rgba::hex(look.cap), 8, 0.28, 0.9)
        .cool(HUE_PURPLE, 0.35)
        .warm(HUE_GOLD, 0.25)
        .build();
    let gills = RampSpec::new(Rgba::hex(look.gills), 6, 0.3, 0.85).build();
    let stem = RampSpec::new(Rgba::hex(look.stem), 6, 0.45, 0.94).build();
    let mark = Rgba::hex(look.mark);
    let glow = look.pattern == CapPattern::Glow;
    let mut spots = Vec::new();
    scatter_into(&mut spots, salt ^ 1, 3.4, 1.0, 600, |x, y| x < 16.0 && y < 16.0);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        if y < 16 && x < 16 {
            // The cap seen from above: domed, lit top-left.
            let (dx, dy) = ((fx - 8.0) / 8.0, (fy - 8.0) / 8.0);
            let d = dx.hypot(dy);
            let mut t = 0.62 - (dx * 0.6 + dy * 0.8) * 0.22 - d * 0.1;
            t += (fbm(salt ^ 4, 4, 4, 2, fx, fy) - 0.5) * 0.1;
            let mut col = cap.pick(t, x, y);
            match look.pattern {
                CapPattern::Spots | CapPattern::Glow => {
                    for s in &spots {
                        let r = 1.0 + s.r[0] * 0.9;
                        let e = (fx - s.x).hypot(fy - s.y);
                        if e < r {
                            col = if glow {
                                mark.mix(Rgba::hex(0xffffff), (1.0 - e / r) * 0.6)
                            } else {
                                mark.mix(cap.at(0), e / r * 0.4)
                            };
                        }
                    }
                }
                CapPattern::Zones => {
                    let z = (d * 8.0 / 1.8).fract();
                    if z < 0.25 {
                        col = col.mix(cap.at(1), 0.5);
                    } else if z > 0.75 {
                        col = col.mix(cap.at(6), 0.3);
                    }
                }
                CapPattern::Veins => {
                    let a = dy.atan2(dx);
                    let v = ((a * 3.0 + d * 6.0).sin() * (a * 5.0 - d * 4.0).cos()).abs();
                    if v < 0.12 && d > 0.15 {
                        col = col.mix(mark, 0.75);
                    }
                }
                CapPattern::Rays => {
                    let a = dy.atan2(dx);
                    let ray = ((a * 7.0).sin() + 1.0) * 0.5;
                    if ray > 0.8 && d > 0.12 {
                        col = col.mix(mark, 0.65 * (1.0 - d * 0.4));
                    }
                    if d < 0.2 {
                        col = mark;
                    }
                }
                CapPattern::Gloss => {
                    let streak = ((fx - 5.5) * 0.8 + (fy - 5.0) * 0.6).abs() < 1.0 && d < 0.75;
                    if streak {
                        col = col.mix(Rgba::hex(0xf4f0ff), 0.55);
                    }
                }
            }
            return col;
        }
        if y < 8 {
            // The rim, seen from the side: its top edge in the light, a
            // darker lower lip.
            let mut t = 0.56 + [0.22, 0.12, 0.04, 0.0, -0.02, -0.06, -0.14, -0.3][y];
            t += (fbm(salt ^ 6, 4, 2, 2, fx, fy) - 0.5) * 0.12;
            let mut col = cap.pick(t, x, y);
            let u = x - 16;
            match look.pattern {
                CapPattern::Spots | CapPattern::Glow => {
                    let h = unit(hash2(salt, (u / 3) as u32 * 8 + y as u32 / 3));
                    if h > 0.72 && y > 0 && y < 6 {
                        col = if glow { mark } else { mark.mix(cap.at(0), 0.3) };
                    }
                }
                CapPattern::Gloss => {
                    if y >= 6 && u % 5 == 2 {
                        col = col.mix(Rgba::hex(0xe8e4ff), 0.5);
                    }
                    if y == 7 && u % 5 == 2 {
                        col = Rgba::hex(0xc8c4f0);
                    }
                }
                CapPattern::Zones => {
                    if y == 3 || y == 5 {
                        col = col.mix(cap.at(1), 0.45);
                    }
                }
                CapPattern::Rays | CapPattern::Veins => {
                    if u % 4 == 1 && y < 6 {
                        col = col.mix(mark, 0.4);
                    }
                }
            }
            return col;
        }
        if y < 16 {
            // Gills seen from below, radiating from the stem.
            let (dx, dy) = (fx - 24.0, fy - 12.0);
            let a = dy.atan2(dx);
            let d = dx.hypot(dy * 2.0);
            if d < 2.2 {
                return stem.pick(0.5, x, y);
            }
            let line = ((a * 7.0).sin() + 1.0) * 0.5;
            let t = 0.5 + if line > 0.6 { 0.2 } else { -0.15 } - d / 10.0 * 0.15;
            let col = gills.pick(t, x, y);
            return if glow { col.mix(mark, 0.35) } else { col };
        }
        // The stem: pale, fibrous lengthways, shaded under the cap and
        // darker at the foot; lit on its left.
        let u = x % 16;
        let mut t = 0.6 + (8.0 - u as f32) / 16.0 * 0.3;
        t += (value(salt ^ 8, 8, 2, fx, fy) - 0.5) * 0.14;
        if y < 18 {
            t -= 0.3;
        }
        if y >= 29 {
            t -= 0.12;
        }
        let col = stem.pick(t, x, y);
        if glow && y < 20 { col.mix(mark, 0.25) } else { col }
    })
}

macro_rules! fungus_fn {
    ($name:ident, $cap:expr, $gills:expr, $stem:expr, $mark:expr, $pat:expr, $doc:literal) => {
        #[doc = $doc]
        pub fn $name(_ctx: Ctx) -> Tex {
            fungus(&CapLook {
                name: stringify!($name),
                cap: $cap,
                gills: $gills,
                stem: $stem,
                mark: $mark,
                pattern: $pat,
            })
        }
    };
}

fungus_fn!(
    tarspot_cap,
    0x963c32,
    0xe8d4b4,
    0xe4dcc8,
    0x1e1418,
    CapPattern::Spots,
    "Tarspot cap: a red-brown cap freckled with black tar spots."
);
fungus_fn!(
    tongue_bracket,
    0xc8965a,
    0xb08040,
    0xe8d0a4,
    0x8a5a2c,
    CapPattern::Zones,
    "Tongue bracket: tan caps ridged in concentric zones, ochre gills."
);
fungus_fn!(
    branch_coral,
    0xdc7896,
    0xf0b0c0,
    0xf4d4dc,
    0xffe0ea,
    CapPattern::Veins,
    "Branch coral: pink, laced with pale branching veins."
);
fungus_fn!(
    burst_cap,
    0xb464c8,
    0xd8a8e4,
    0xece0f0,
    0xff9cf0,
    CapPattern::Rays,
    "Burst cap: violet, bright rays bursting from its middle."
);
fungus_fn!(
    weeping_cap,
    0x786ea0,
    0x5a5280,
    0xd4d0e4,
    0xe8e4ff,
    CapPattern::Gloss,
    "Weeping cap: a glossy dusk-blue cap beaded with drips at its rim."
);
fungus_fn!(
    glowcap,
    0x60c4bc,
    0x7ae8dc,
    0xd8f4ee,
    0xb8fff4,
    CapPattern::Glow,
    "Glowcap: a cyan cap with glowing freckles and glowing gills."
);

/// Every gathering object's painter by texture name.
pub fn painter(name: &str) -> Option<fn(Ctx) -> Tex> {
    Some(match name {
        "spikeleaf" => spikeleaf,
        "hornroot" => hornroot,
        "stiltfrond" => stiltfrond,
        "nightlily" => nightlily,
        "ridge_cactus" => ridge_cactus,
        "knot_aloe" => knot_aloe,
        "frostbell" => frostbell,
        "curlfern" => curlfern,
        "boulder" => boulder,
        "stalk_boulder" => stalk_boulder,
        "stacked_boulder" => stacked_boulder,
        "ring_boulder" => ring_boulder,
        "chip_boulder" => chip_boulder,
        "tapered_boulder" => tapered_boulder,
        "tarspot_cap" => tarspot_cap,
        "tongue_bracket" => tongue_bracket,
        "branch_coral" => branch_coral,
        "burst_cap" => burst_cap,
        "weeping_cap" => weeping_cap,
        "glowcap" => glowcap,
        _ => return None,
    })
}

/// The plants (cut-out pictures), as opposed to boulders and fungi.
pub const PLANTS: [&str; 8] = [
    "spikeleaf",
    "hornroot",
    "stiltfrond",
    "nightlily",
    "ridge_cactus",
    "knot_aloe",
    "frostbell",
    "curlfern",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plants_are_cut_out_pictures_standing_on_the_bottom_row() {
        for name in PLANTS {
            let t = painter(name).unwrap()(Ctx::BASE);
            assert!(
                t.px.iter().all(|p| p.a == 0.0 || p.a == 1.0),
                "{name} has partial alpha"
            );
            let filled = t.px.iter().filter(|p| p.a > 0.0).count();
            assert!((180..800).contains(&filled), "{name} covers {filled} pixels");
            // Its foot is on the bottom row, near the middle.
            let foot: Vec<usize> = (0..SIZE).filter(|&x| t.get(x, SIZE - 1).a > 0.0).collect();
            assert!(!foot.is_empty(), "{name} does not stand on the bottom row");
            assert!(foot.iter().any(|&x| (11..21).contains(&x)), "{name} foot {foot:?}");
            // The corners above are sky.
            assert_eq!(t.get(0, 0).a, 0.0, "{name}");
            assert_eq!(t.get(31, 0).a, 0.0, "{name}");
        }
    }

    #[test]
    fn boulders_and_fungi_are_opaque_and_near_their_catalogue_colour() {
        let reg = bm_core::BlockRegistry::builtin();
        for (_, def) in reg.iter() {
            let Some(p) = painter(&def.name) else { continue };
            if PLANTS.contains(&def.name.as_str()) {
                continue;
            }
            let t = p(Ctx::BASE);
            assert!(t.px.iter().all(|p| p.a == 1.0), "{} has holes", def.name);
            // Readable at a distance as the colour the catalogue says: a
            // boulder all over, a fungus by its cap.
            let fungus = def.model == bm_core::Model::Fungus;
            let opaque: Vec<_> =
                t.px.iter()
                    .enumerate()
                    .filter(|(i, _)| !fungus || (i % SIZE < 16 && i / SIZE < 16))
                    .map(|(_, p)| p)
                    .collect();
            let mean = |f: fn(&Rgba) -> f32| opaque.iter().map(|p| f(p)).sum::<f32>() / opaque.len() as f32;
            let m = [mean(|p| p.r), mean(|p| p.g), mean(|p| p.b)];
            let want = def.color.map(|v| v as f32 / 255.0);
            let dist = (0..3).map(|i| (m[i] - want[i]).abs()).fold(0.0, f32::max);
            assert!(dist < 0.2, "{} mean {m:?} far from {want:?}", def.name);
        }
    }
}
