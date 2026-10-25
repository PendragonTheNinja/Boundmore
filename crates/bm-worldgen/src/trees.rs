//! Tree shapes (milestone 19, rule 10): pure functions of a seed and a size.
//!
//! Every shape is drawn here in code (no models are copied): a trunk of
//! [`Part::Trunk`] blocks that always reaches the ground, and crowns of
//! [`Part::Leaves`] (plus [`Part::Leaves2`] where a shape mixes in a second
//! kind: fruit on a fruit tree, snow on a snowy conifer). The biome maps the
//! parts to its own trunk and leaf blocks.
//!
//! Randomness is integer hashing (splitmix64) and the few directions come from
//! a fixed table, so the same seed grows the same tree on every machine.

use serde::{Deserialize, Serialize};

/// What a tree block is made of: the biome maps these to block ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Part {
    Trunk,
    Leaves,
    /// A second leaf kind some shapes mix in: fruit on a fruit tree, snow on
    /// the top faces of a snowy conifer.
    Leaves2,
}

/// The tree shapes a biome can list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    BroadCrown,
    FruitTree,
    Conifer,
    SnowyConifer,
    Palm,
    BentPalm,
    GiantConifer,
    FlatTop,
    SwampTree,
    JungleTree,
    DeadTree,
    FallenLog,
    Stump,
    Bush,
}

/// One tree: blocks as offsets from its base (the block just above the ground
/// column it grows from; dy 0 is the first trunk block), each with its part.
/// Trunk blocks win over leaves where both land. Each position appears once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    pub blocks: Vec<(i32, i32, i32, Part)>,
}

impl Shape {
    /// Every shape, for tests and tools.
    pub const ALL: [Shape; 14] = [
        Shape::BroadCrown,
        Shape::FruitTree,
        Shape::Conifer,
        Shape::SnowyConifer,
        Shape::Palm,
        Shape::BentPalm,
        Shape::GiantConifer,
        Shape::FlatTop,
        Shape::SwampTree,
        Shape::JungleTree,
        Shape::DeadTree,
        Shape::FallenLog,
        Shape::Stump,
        Shape::Bush,
    ];

    /// The furthest any block of this shape reaches sideways from its base
    /// column, and the highest above the base: the generator's margins.
    pub fn reach(self) -> (i32, i32) {
        match self {
            Shape::BroadCrown => (9, 20),
            Shape::FruitTree => (4, 9),
            Shape::Conifer | Shape::SnowyConifer => (7, 30),
            Shape::Palm => (8, 21),
            Shape::BentPalm => (14, 21),
            Shape::GiantConifer => (11, 70),
            Shape::FlatTop => (12, 18),
            Shape::SwampTree => (8, 14),
            Shape::JungleTree => (10, 28),
            Shape::DeadTree => (7, 16),
            Shape::FallenLog => (10, 1),
            Shape::Stump => (1, 1),
            Shape::Bush => (3, 3),
        }
    }

    /// Grows one tree of this shape; `seed` (a hash of its world column)
    /// picks its size and details within the shape's range; `size` 0.0..=1.0
    /// scales it within that range too (the biome's size knob).
    pub fn grow(self, seed: u64, size: f64) -> Tree {
        let mut blocks = self.grow_unclamped(seed, size);
        let (side, up) = self.reach();
        // A safety net only: the shapes are drawn to fit (tests check it).
        blocks.retain(|b| b.0.abs() <= side && b.2.abs() <= side && (0..=up).contains(&b.1));
        Tree { blocks }
    }

    /// The furthest (Chebyshev) a leaf may sit from the nearest trunk block,
    /// or `None` for a shape with no trunk to hold it.
    fn leaf_limit(self) -> Option<i32> {
        match self {
            Shape::Bush => None,
            Shape::Palm | Shape::BentPalm => Some(8),
            Shape::SwampTree => Some(8),
            _ => Some(4),
        }
    }

    fn grow_unclamped(self, seed: u64, size: f64) -> Vec<(i32, i32, i32, Part)> {
        let size = if size.is_finite() { size.clamp(0.0, 1.0) } else { 0.5 };
        let seed = splitmix64(seed ^ (self as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut b = Build {
            blocks: Vec::with_capacity(512),
            seed,
            rng: Rng(seed),
            size,
        };
        match self {
            Shape::BroadCrown => b.broad_crown(),
            Shape::FruitTree => b.fruit_tree(),
            Shape::Conifer => b.conifer(false),
            Shape::SnowyConifer => b.conifer(true),
            Shape::Palm => b.palm(false),
            Shape::BentPalm => b.palm(true),
            Shape::GiantConifer => b.giant_conifer(),
            Shape::FlatTop => b.flat_top(),
            Shape::SwampTree => b.swamp_tree(),
            Shape::JungleTree => b.jungle_tree(),
            Shape::DeadTree => b.dead_tree(),
            Shape::FallenLog => b.fallen_log(),
            Shape::Stump => b.stump(),
            Shape::Bush => b.bush(),
        }
        b.finish(self)
    }
}

/// splitmix64's finaliser: a good 64-bit mix of one word.
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A fixed hash of one block of one tree, 0.0..1.0 (edge roughness, accents).
fn cell01(seed: u64, x: i32, y: i32, z: i32) -> f64 {
    let mut h = splitmix64(seed ^ x as u32 as u64);
    h = splitmix64(h ^ ((y as u32 as u64) << 20));
    h = splitmix64(h ^ ((z as u32 as u64) << 40));
    (h >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        splitmix64(self.0)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
    /// Inclusive integer range.
    fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1) as u64) as i32
    }
    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

/// cos 45° (the diagonals).
const S: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// Sixteen compass directions (cos, sin of k × 22.5°), so no trigonometry runs.
const DIRS: [(f64, f64); 16] = [
    (1.0, 0.0),
    (0.923_879_5, 0.382_683_4),
    (S, S),
    (0.382_683_4, 0.923_879_5),
    (0.0, 1.0),
    (-0.382_683_4, 0.923_879_5),
    (-S, S),
    (-0.923_879_5, 0.382_683_4),
    (-1.0, 0.0),
    (-0.923_879_5, -0.382_683_4),
    (-S, -S),
    (-0.382_683_4, -0.923_879_5),
    (0.0, -1.0),
    (0.382_683_4, -0.923_879_5),
    (S, -S),
    (0.923_879_5, -0.382_683_4),
];

fn dir(k: i32) -> (f64, f64) {
    DIRS[k.rem_euclid(16) as usize]
}

/// Rounds to the nearest block (ties away from zero, the same everywhere).
fn r(v: f64) -> i32 {
    v.round() as i32
}

fn rank(p: Part) -> u8 {
    match p {
        Part::Trunk => 0,
        Part::Leaves2 => 1,
        Part::Leaves => 2,
    }
}

type P3 = (i32, i32, i32);

struct Build {
    blocks: Vec<(i32, i32, i32, Part)>,
    seed: u64,
    rng: Rng,
    size: f64,
}

impl Build {
    /// A value in `lo..=hi`, mostly set by the size knob, partly by the seed.
    fn pick(&mut self, lo: f64, hi: f64) -> f64 {
        let t = (0.6 * self.size + 0.4 * self.rng.unit()).clamp(0.0, 1.0);
        lo + (hi - lo) * t
    }
    fn pick_i(&mut self, lo: i32, hi: i32) -> i32 {
        r(self.pick(lo as f64, hi as f64))
    }

    fn put(&mut self, (x, y, z): P3, part: Part) {
        self.blocks.push((x, y, z, part));
    }

    /// A `w`×`w` trunk column from `y0` to `y1` inclusive, corner at (x, z).
    fn column(&mut self, x: i32, z: i32, y0: i32, y1: i32, w: i32) {
        for y in y0..=y1 {
            for dx in 0..w {
                for dz in 0..w {
                    self.put((x + dx, y, z + dz), Part::Trunk);
                }
            }
        }
    }

    /// A face-connected line of blocks from `a` to `b` (sideways steps first,
    /// so a rising branch climbs in stairs).
    fn line(&mut self, a: P3, b: P3, part: Part) {
        let d = (b.0 - a.0, b.1 - a.1, b.2 - a.2);
        let n = d.0.abs().max(d.1.abs()).max(d.2.abs());
        let mut cur = a;
        self.put(cur, part);
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let p = (
                a.0 + r(d.0 as f64 * t),
                a.1 + r(d.1 as f64 * t),
                a.2 + r(d.2 as f64 * t),
            );
            while cur != p {
                if cur.0 != p.0 {
                    cur.0 += (p.0 - cur.0).signum();
                } else if cur.2 != p.2 {
                    cur.2 += (p.2 - cur.2).signum();
                } else {
                    cur.1 += (p.1 - cur.1).signum();
                }
                self.put(cur, part);
            }
        }
    }

    /// A full, slightly lumpy ellipsoid of leaves.
    fn blob(&mut self, c: (f64, f64, f64), rad: (f64, f64, f64), part: Part, rough: f64) {
        let (x0, x1) = ((c.0 - rad.0).floor() as i32 - 1, (c.0 + rad.0).ceil() as i32 + 1);
        let (y0, y1) = ((c.1 - rad.1).floor() as i32 - 1, (c.1 + rad.1).ceil() as i32 + 1);
        let (z0, z1) = ((c.2 - rad.2).floor() as i32 - 1, (c.2 + rad.2).ceil() as i32 + 1);
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let (fx, fy, fz) = (
                        (x as f64 - c.0) / rad.0,
                        (y as f64 - c.1) / rad.1,
                        (z as f64 - c.2) / rad.2,
                    );
                    let q = fx * fx + fy * fy + fz * fz;
                    if q <= 1.0 + rough * (cell01(self.seed, x, y, z) - 0.5) {
                        self.put((x, y, z), part);
                    }
                }
            }
        }
    }

    /// A flat disc of leaves one block thick.
    fn disc(&mut self, c: (f64, i32, f64), radius: f64, rough: f64) {
        if radius < 0.5 {
            self.put((r(c.0), c.1, r(c.2)), Part::Leaves);
            return;
        }
        let reach = radius.ceil() as i32 + 1;
        let (cx, cz) = (r(c.0), r(c.2));
        for x in cx - reach..=cx + reach {
            for z in cz - reach..=cz + reach {
                let (dx, dz) = (x as f64 - c.0, z as f64 - c.2);
                let lim = radius + rough * (cell01(self.seed, x, c.1, z) - 0.5);
                if dx * dx + dz * dz <= lim * lim {
                    self.put((x, c.1, z), Part::Leaves);
                }
            }
        }
    }

    /// One to three root flares on the ground beside a `w`-wide trunk.
    fn roots(&mut self, w: i32, max: i32) {
        let n = self.rng.int(1, max);
        let start = self.rng.int(0, 3);
        for i in 0..n {
            let (x, z) = match (start + i) % 4 {
                0 => (w, self.rng.int(0, w - 1)),
                1 => (-1, self.rng.int(0, w - 1)),
                2 => (self.rng.int(0, w - 1), w),
                _ => (self.rng.int(0, w - 1), -1),
            };
            self.put((x, 0, z), Part::Trunk);
            if w > 1 && self.rng.chance(0.5) {
                // A buttress: a second step out on the bigger trunks.
                let (ox, oz) = (
                    if x == w {
                        1
                    } else if x == -1 {
                        -1
                    } else {
                        0
                    },
                    if z == w {
                        1
                    } else if z == -1 {
                        -1
                    } else {
                        0
                    },
                );
                self.put((x + ox, 0, z + oz), Part::Trunk);
                self.put((x, 1, z), Part::Trunk);
            }
        }
    }

    fn broad_crown(&mut self) {
        let h = self.pick_i(12, 20);
        let rad = self.pick(5.0, 9.0).min((h - 5) as f64 / 1.3);
        let ry = rad * 0.68;
        let cy = h as f64 - ry;
        let fork = (cy - ry * 0.35).floor() as i32;
        self.column(0, 0, 0, fork, 1);
        self.roots(1, 3);
        let n = if rad > 6.5 { 4 } else { self.rng.int(3, 4) };
        let start = self.rng.int(0, 15);
        let tip_y = r(cy + ry * 0.25);
        for i in 0..n {
            let k = start + i * 16 / n + self.rng.int(-1, 1);
            let (dx, dz) = dir(k);
            let reach = rad * 0.5;
            let end = (r(dx * reach), tip_y, r(dz * reach));
            self.line((0, fork, 0), end, Part::Trunk);
            for side in [-2, 2] {
                let (tx, tz) = dir(k + side);
                let tw = rad * 0.85;
                self.line(end, (r(tx * tw), tip_y + 1, r(tz * tw)), Part::Trunk);
            }
            // A small lobe over each branch so the crown reads as lumpy.
            let lobe = rad * 0.45;
            self.blob(
                (dx * rad * 0.62, cy + ry * 0.2, dz * rad * 0.62),
                (lobe, lobe * 0.8, lobe),
                Part::Leaves,
                0.4,
            );
        }
        self.line((0, fork, 0), (0, r(cy + ry * 0.5), 0), Part::Trunk);
        self.blob((0.0, cy, 0.0), (rad, ry, rad), Part::Leaves, 0.35);
    }

    fn fruit_tree(&mut self) {
        let h = self.pick_i(6, 9);
        let rad = self.pick(3.0, 4.3);
        let ry = rad * 0.75;
        let cy = h as f64 - ry;
        let top = r(cy);
        self.column(0, 0, 0, top, 1);
        let start = self.rng.int(0, 15);
        for i in 0..3 {
            let (dx, dz) = dir(start + i * 5);
            let fork = (top - 1).max(1);
            self.line(
                (0, fork, 0),
                (r(dx * rad * 0.5), top + 1, r(dz * rad * 0.5)),
                Part::Trunk,
            );
        }
        self.blob((0.0, cy + 0.3, 0.0), (rad, ry, rad), Part::Leaves, 0.3);
        // Fruit dotted through the crown.
        let seed = self.seed ^ 0xF4F4;
        for b in self.blocks.iter_mut() {
            if b.3 == Part::Leaves && cell01(seed, b.0, b.1, b.2) < 0.12 {
                b.3 = Part::Leaves2;
            }
        }
    }

    fn conifer(&mut self, snowy: bool) {
        let h = self.pick_i(14, 30);
        let rmax = self.pick(3.5, 6.2).min(h as f64 / 4.0);
        let y0 = r(h as f64 * self.pick(0.18, 0.28)).max(2);
        self.column(0, 0, 0, h - 2, 1);
        self.roots(1, 2);
        self.put((0, h - 1, 0), Part::Leaves);
        self.put((0, h, 0), Part::Leaves);
        self.whorls(y0, h - 1, rmax, (0.0, 0.0), 1, if snowy { 0 } else { 1 });
    }

    /// Stacked tiers of leaves narrowing up a trunk centred at `c` (a `w`-wide
    /// trunk), from `y0` to `top`, `rmax` across at the bottom.
    fn whorls(&mut self, y0: i32, top: i32, rmax: f64, c: (f64, f64), w: i32, droop: i32) {
        let span = (top - y0).max(1) as f64;
        let step = if rmax > 7.0 { 4 } else { 3 };
        let mut y = y0;
        let mut turn = self.rng.int(0, 15);
        while y < top {
            let t = (y - y0) as f64 / span;
            let rw = (rmax * (1.0 - t) + 1.2 + (self.rng.unit() - 0.5) * 0.8).max(1.0);
            // Branches inside the tier, from the trunk's faces outwards.
            let len = rw - 2.0;
            if len >= 1.0 {
                turn += 3;
                for i in 0..8 {
                    let (dx, dz) = dir(turn + i * 2);
                    let from = (r(c.0 + dx * (w as f64 * 0.5)), y, r(c.1 + dz * (w as f64 * 0.5)));
                    let to = (
                        r(c.0 + dx * (len + w as f64 * 0.5)),
                        y,
                        r(c.1 + dz * (len + w as f64 * 0.5)),
                    );
                    self.line(from, to, Part::Trunk);
                }
            }
            for layer in 0..step.min(top - y + 1) {
                let lr = rw - layer as f64 * (rw / step as f64) * 0.9;
                self.disc((c.0, y + layer, c.1), lr + w as f64 * 0.5 - 0.5, 0.6);
            }
            // The tier's rim hangs one block lower.
            if droop > 0 || rw > 2.5 {
                let rim = rw + w as f64 * 0.5 - 0.5;
                let reach = rim.ceil() as i32 + 1;
                for x in r(c.0) - reach..=r(c.0) + reach {
                    for z in r(c.1) - reach..=r(c.1) + reach {
                        let (dx, dz) = (x as f64 - c.0, z as f64 - c.1);
                        let d2 = dx * dx + dz * dz;
                        if d2 <= rim * rim && d2 > (rim - 1.3) * (rim - 1.3) && cell01(self.seed, x, y, z) < 0.7 {
                            self.put((x, y - 1, z), Part::Leaves);
                        }
                    }
                }
            }
            y += step;
        }
    }

    fn palm(&mut self, bent: bool) {
        let h = self.pick_i(10, 20);
        let (bx, bz) = dir(self.rng.int(0, 15));
        let bend = if bent { self.pick(3.0, 7.0) } else { 0.0 };
        // The trunk: straight, or curving over more and more towards the top.
        let at = |y: i32| {
            let t = y as f64 / h as f64;
            let off = bend * t * t;
            (r(bx * off), y, r(bz * off))
        };
        let mut prev = at(0);
        for y in 1..h {
            let p = at(y);
            self.line(prev, p, Part::Trunk);
            prev = p;
        }
        let (tx, ty, tz) = prev;
        self.blob(
            (tx as f64, ty as f64 + 1.0, tz as f64),
            (1.6, 1.0, 1.6),
            Part::Leaves,
            0.3,
        );
        let n = self.rng.int(5, 8);
        let start = self.rng.int(0, 15);
        let len = self.pick_i(5, 7);
        for i in 0..n {
            let k = start + r(i as f64 * 16.0 / n as f64);
            let (dx, dz) = dir(k);
            let (px, pz) = (-dz, dx);
            let rise = len / 3;
            let mut last = (tx, ty + 1, tz);
            for s in 1..=len {
                let drop = if s <= rise {
                    0
                } else {
                    let u = (s - rise) as f64 / (len - rise) as f64;
                    r((0.4 * u + 0.6 * u * u) * len as f64 * 0.9)
                };
                let p = (tx + r(dx * s as f64), ty + 2 - drop, tz + r(dz * s as f64));
                self.line(last, p, Part::Leaves);
                if s >= 2 && s < len {
                    self.put((p.0 + r(px), p.1, p.2 + r(pz)), Part::Leaves);
                    self.put((p.0 - r(px), p.1, p.2 - r(pz)), Part::Leaves);
                }
                last = p;
            }
        }
    }

    fn giant_conifer(&mut self) {
        let h = self.pick_i(40, 70);
        let w = if h >= 58 && self.rng.chance(0.5) { 3 } else { 2 };
        let o = -(w - 1) / 2;
        let c = (o as f64 + (w - 1) as f64 / 2.0, o as f64 + (w - 1) as f64 / 2.0);
        let rmax = self.pick(6.0, 9.0) - (w - 2) as f64 * 0.5;
        let y0 = r(h as f64 * self.pick(0.42, 0.32));
        self.column(o, o, 0, h - 4, w);
        self.column(o + w / 2, o + w / 2, h - 3, h - 2, 1);
        // Buttress roots on all four sides.
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let len = self.rng.int(1, 3);
            for s in 1..=len {
                let (x, z) = (
                    if dx > 0 {
                        o + w - 1 + s
                    } else if dx < 0 {
                        o - s
                    } else {
                        o + self.rng.int(0, w - 1)
                    },
                    if dz > 0 {
                        o + w - 1 + s
                    } else if dz < 0 {
                        o - s
                    } else {
                        o + self.rng.int(0, w - 1)
                    },
                );
                for y in 0..=(len - s) {
                    self.put((x, y, z), Part::Trunk);
                }
            }
        }
        self.put((o + w / 2, h - 1, o + w / 2), Part::Leaves);
        self.put((o + w / 2, h, o + w / 2), Part::Leaves);
        self.whorls(y0, h - 1, rmax, c, w, 1);
    }

    fn flat_top(&mut self) {
        let h = self.pick_i(12, 18);
        let rad = self.pick(7.0, 11.0);
        let thick = if self.rng.chance(0.5) { 3 } else { 2 };
        let fork = h - self.rng.int(5, 7);
        let lean = dir(self.rng.int(0, 15));
        // A slight lean to the trunk, savanna-like.
        let fx = r(lean.0 * 1.4);
        let fz = r(lean.1 * 1.4);
        self.line((0, 0, 0), (fx, fork, fz), Part::Trunk);
        let canopy = h - 1;
        let n = self.rng.int(2, 3);
        let start = self.rng.int(0, 15);
        for i in 0..n {
            let (dx, dz) = dir(start + i * 16 / n);
            let tip = (fx + r(dx * rad * 0.45), canopy, fz + r(dz * rad * 0.45));
            self.line((fx, fork, fz), tip, Part::Trunk);
        }
        // Hidden spokes inside the canopy hold its rim.
        let spoke = rad - 3.5;
        for i in 0..8 {
            let (dx, dz) = dir(start + 1 + i * 2);
            self.line(
                (fx, canopy, fz),
                (fx + r(dx * spoke), canopy, fz + r(dz * spoke)),
                Part::Trunk,
            );
        }
        self.line((fx, fork, fz), (fx, canopy, fz), Part::Trunk);
        let c = (fx as f64, fz as f64);
        self.disc((c.0, canopy, c.1), rad, 1.2);
        self.disc((c.0, canopy + 1, c.1), rad - 1.0, 1.2);
        if thick == 3 {
            self.disc((c.0, canopy - 1, c.1), rad - 1.8, 1.4);
        }
    }

    fn swamp_tree(&mut self) {
        let h = self.pick_i(8, 14);
        let rad = self.pick(5.0, 8.0);
        let top = h - 2;
        self.column(0, 0, 0, top, 1);
        // Prop roots arching from the trunk down to the ground.
        let n = self.rng.int(3, 5);
        let start = self.rng.int(0, 15);
        for i in 0..n {
            let (dx, dz) = dir(start + i * 16 / n);
            let out = 2.0 + self.rng.unit();
            self.line((r(dx * out), 0, r(dz * out)), (r(dx), 2, r(dz)), Part::Trunk);
            self.line((r(dx), 2, r(dz)), (0, 3, 0), Part::Trunk);
        }
        for i in 0..6 {
            let (dx, dz) = dir(start + 1 + r(i as f64 * 16.0 / 6.0));
            let len = rad - 3.0;
            self.line((0, top - 2, 0), (r(dx * len), top, r(dz * len)), Part::Trunk);
        }
        let cy = top as f64 + 0.5;
        self.blob((0.0, cy, 0.0), (rad, 2.0, rad), Part::Leaves, 0.4);
        // Strands of leaves hanging from the crown's underside.
        let mut bottom: std::collections::BTreeMap<(i32, i32), i32> = std::collections::BTreeMap::new();
        for &(x, y, z, p) in &self.blocks {
            if p == Part::Leaves {
                let e = bottom.entry((x, z)).or_insert(y);
                *e = (*e).min(y);
            }
        }
        for ((x, z), y) in bottom {
            let roll = cell01(self.seed ^ 0x5717, x, 0, z);
            if roll < 0.35 {
                let len = 1 + (roll * 12.0) as i32;
                for d in 1..=len.min(y - 1) {
                    self.put((x, y - d, z), Part::Leaves);
                }
            }
        }
    }

    fn jungle_tree(&mut self) {
        let h = self.pick_i(15, 28);
        let top_r = self.pick(5.0, 7.0);
        let crown_y = h as f64 - 2.5;
        self.column(0, 0, 0, r(crown_y), 2);
        self.roots(2, 4);
        // Side crowns on branches at different heights.
        let n = self.rng.int(2, 3);
        let start = self.rng.int(0, 15);
        for i in 0..n {
            let k = start + i * 16 / n + self.rng.int(-1, 1);
            let (dx, dz) = dir(k);
            let y = r(h as f64 * (0.4 + 0.35 * (i as f64 + self.rng.unit()) / n as f64));
            let len = self.pick(3.0, 5.0);
            let (sx, sz) = (r(0.5 + dx), r(0.5 + dz));
            let end = (r(0.5 + dx * (len + 1.0)), y + 2, r(0.5 + dz * (len + 1.0)));
            self.line((sx, y, sz), end, Part::Trunk);
            let cr = self.pick(3.0, 4.0);
            self.blob(
                (end.0 as f64, end.1 as f64 + 1.0, end.2 as f64),
                (cr, 1.8, cr),
                Part::Leaves,
                0.4,
            );
        }
        // Top crown with a few branches under it.
        for i in 0..4 {
            let (dx, dz) = dir(start + 2 + i * 4);
            let len = top_r - 2.5;
            let base = r(crown_y) - 2;
            self.line(
                (r(0.5 + dx * 0.8), base, r(0.5 + dz * 0.8)),
                (r(0.5 + dx * len), base + 2, r(0.5 + dz * len)),
                Part::Trunk,
            );
        }
        self.blob((0.5, crown_y + 0.5, 0.5), (top_r, 2.3, top_r), Part::Leaves, 0.4);
    }

    fn dead_tree(&mut self) {
        let h = self.pick_i(6, 16);
        // A crooked trunk: one jog sideways somewhere up it.
        let jog_y = self.rng.int(h / 2, h - 1);
        let (jx, jz) = match self.rng.int(0, 4) {
            0 => (1, 0),
            1 => (-1, 0),
            2 => (0, 1),
            3 => (0, -1),
            _ => (0, 0),
        };
        self.column(0, 0, 0, jog_y, 1);
        self.line((0, jog_y, 0), (jx, jog_y, jz), Part::Trunk);
        self.column(jx, jz, jog_y, h - 1, 1);
        let n = self.rng.int(2, 5);
        let start = self.rng.int(0, 15);
        for i in 0..n {
            let k = start + i * 16 / n + self.rng.int(-1, 1);
            let (dx, dz) = dir(k);
            let y = self.rng.int((h * 2 / 5).max(2), h - 2);
            let (bx, bz) = if y > jog_y { (jx, jz) } else { (0, 0) };
            let len = self.rng.int(2, 5).min(h / 2);
            let end = (
                bx + r(dx * len as f64),
                (y + len / 2 + 1).min(h - 1),
                bz + r(dz * len as f64),
            );
            self.line((bx, y, bz), end, Part::Trunk);
            if self.rng.chance(0.5) {
                let (tx, tz) = dir(k + 3);
                self.line(
                    end,
                    (end.0 + r(tx * 2.0), (end.1 + 1).min(h - 1), end.2 + r(tz * 2.0)),
                    Part::Trunk,
                );
            }
        }
    }

    fn fallen_log(&mut self) {
        let len = self.pick_i(4, 9);
        let (dx, dz) = [(1, 0), (-1, 0), (0, 1), (0, -1)][self.rng.int(0, 3) as usize];
        let stump = self.rng.chance(0.5);
        let begin = if stump {
            self.put((0, 0, 0), Part::Trunk);
            if self.rng.chance(0.5) {
                self.put((0, 1, 0), Part::Trunk);
            }
            2
        } else {
            0
        };
        for s in begin..begin + len {
            self.put((dx * s, 0, dz * s), Part::Trunk);
        }
        // A broken branch stub and a little moss on top.
        let stub = begin + self.rng.int(1, len - 2);
        let side = if self.rng.chance(0.5) { 1 } else { -1 };
        self.put((dx * stub + dz * side, 0, dz * stub + dx * side), Part::Trunk);
        let moss = self.rng.int(0, 2);
        for _ in 0..moss {
            let s = begin + self.rng.int(0, len - 1);
            self.put((dx * s, 1, dz * s), Part::Leaves);
        }
    }

    fn stump(&mut self) {
        let h = self.pick_i(1, 2);
        self.column(0, 0, 0, h - 1, 1);
        let roots = self.rng.int(0, 3);
        let start = self.rng.int(0, 3);
        for i in 0..roots {
            let (x, z) = [(1, 0), (0, 1), (-1, 0), (0, -1)][((start + i) % 4) as usize];
            self.put((x, 0, z), Part::Trunk);
        }
    }

    fn bush(&mut self) {
        let hh = self.pick_i(1, 4);
        let rad = self.pick(1.0, 3.0);
        let stem = hh >= 3 && self.rng.chance(0.5);
        let base = if stem {
            self.put((0, 0, 0), Part::Trunk);
            1
        } else {
            0
        };
        let height = hh - base;
        let ry = (height as f64 / 2.0).max(0.6);
        let cy = base as f64 + (height - 1) as f64 / 2.0;
        self.blob((0.0, cy, 0.0), (rad + 0.4, ry + 0.1, rad + 0.4), Part::Leaves, 0.3);
        // Never empty, and nothing below the ground.
        self.put((0, base, 0), Part::Leaves);
        self.blocks.retain(|b| b.1 >= 0 && b.1 < hh);
    }

    fn finish(self, shape: Shape) -> Vec<(i32, i32, i32, Part)> {
        let mut blocks = self.blocks;
        blocks.retain(|b| b.1 >= 0);
        blocks.sort_unstable_by_key(|&(x, y, z, p)| (x, y, z, rank(p)));
        blocks.dedup_by_key(|b| (b.0, b.1, b.2));
        if let Some(limit) = shape.leaf_limit() {
            let mut trunk: Vec<P3> = blocks
                .iter()
                .filter(|b| b.3 == Part::Trunk)
                .map(|b| (b.1, b.0, b.2))
                .collect();
            trunk.sort_unstable();
            blocks.retain(|&(x, y, z, p)| {
                if p == Part::Trunk {
                    return true;
                }
                let from = trunk.partition_point(|t| t.0 < y - limit);
                trunk[from..]
                    .iter()
                    .take_while(|t| t.0 <= y + limit)
                    .any(|t| (t.1 - x).abs() <= limit && (t.2 - z).abs() <= limit)
            });
        }
        if shape == Shape::SnowyConifer {
            // Snow on every leaf with open air above it.
            let occupied: Vec<P3> = blocks.iter().map(|b| (b.0, b.1, b.2)).collect();
            for b in blocks.iter_mut() {
                if b.3 == Part::Leaves
                    && occupied.binary_search(&(b.0, b.1 + 1, b.2)).is_err()
                    && cell01(self.seed ^ 0x5A0, b.0, b.1, b.2) < 0.7
                {
                    b.3 = Part::Leaves2;
                }
            }
        }
        blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet, VecDeque};

    const SEEDS: u64 = 300;

    fn seeds() -> impl Iterator<Item = (u64, f64)> {
        (0..SEEDS).map(|i| (splitmix64(i * 7919 + 13), (i % 11) as f64 / 10.0))
    }

    #[test]
    fn trees_grow_deterministically() {
        for shape in Shape::ALL {
            for (seed, size) in seeds().take(40) {
                assert_eq!(shape.grow(seed, size), shape.grow(seed, size), "{shape:?}");
            }
        }
    }

    #[test]
    fn trees_fit_inside_their_reach_without_clipping() {
        for shape in Shape::ALL {
            let (side, up) = shape.reach();
            let mut seen = (0, 0);
            for (seed, size) in seeds() {
                let raw = shape.grow_unclamped(seed, size);
                assert!(!raw.is_empty(), "{shape:?} grew nothing");
                for &(x, y, z, _) in &raw {
                    seen.0 = seen.0.max(x.abs()).max(z.abs());
                    seen.1 = seen.1.max(y);
                    assert!(y >= 0, "{shape:?} below its base");
                }
                assert_eq!(
                    raw.len(),
                    shape.grow(seed, size).blocks.len(),
                    "{shape:?} {seed} clipped"
                );
            }
            assert!(
                seen.0 <= side && seen.1 <= up,
                "{shape:?} reached {seen:?}, declared {:?}",
                (side, up)
            );
            // The declared reach should not waste much generator margin.
            assert!(
                seen.0 + 3 >= side && seen.1 + 3 >= up,
                "{shape:?} reached only {seen:?} of {:?}",
                (side, up)
            );
        }
    }

    #[test]
    fn trees_have_no_duplicate_blocks() {
        for shape in Shape::ALL {
            for (seed, size) in seeds().take(30) {
                let t = shape.grow(seed, size);
                let set: HashSet<P3> = t.blocks.iter().map(|b| (b.0, b.1, b.2)).collect();
                assert_eq!(set.len(), t.blocks.len(), "{shape:?}");
            }
        }
    }

    #[test]
    fn trunks_are_connected_to_the_ground() {
        for shape in Shape::ALL {
            for (seed, size) in seeds() {
                let t = shape.grow(seed, size);
                let trunk: HashSet<P3> = t
                    .blocks
                    .iter()
                    .filter(|b| b.3 == Part::Trunk)
                    .map(|b| (b.0, b.1, b.2))
                    .collect();
                if shape != Shape::Bush {
                    assert!(trunk.contains(&(0, 0, 0)), "{shape:?} {seed}: no trunk at its base");
                }
                let mut reached: HashSet<P3> = trunk.iter().copied().filter(|p| p.1 == 0).collect();
                let mut queue: VecDeque<P3> = reached.iter().copied().collect();
                while let Some(p) = queue.pop_front() {
                    for d in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                        let n = (p.0 + d.0, p.1 + d.1, p.2 + d.2);
                        if trunk.contains(&n) && reached.insert(n) {
                            queue.push_back(n);
                        }
                    }
                }
                assert_eq!(
                    reached.len(),
                    trunk.len(),
                    "{shape:?} seed {seed} size {size}: floating trunk"
                );
            }
        }
    }

    #[test]
    fn leaves_stay_near_a_trunk() {
        for shape in Shape::ALL {
            let Some(limit) = shape.leaf_limit() else { continue };
            for (seed, size) in seeds() {
                let t = shape.grow(seed, size);
                let trunk: Vec<P3> = t
                    .blocks
                    .iter()
                    .filter(|b| b.3 == Part::Trunk)
                    .map(|b| (b.0, b.1, b.2))
                    .collect();
                for &(x, y, z, p) in &t.blocks {
                    if p != Part::Trunk {
                        let near = trunk
                            .iter()
                            .any(|t| (t.0 - x).abs().max((t.1 - y).abs()).max((t.2 - z).abs()) <= limit);
                        assert!(
                            near,
                            "{shape:?} seed {seed}: leaf at {:?} far from the trunk",
                            (x, y, z)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn size_scales_every_shape() {
        for shape in Shape::ALL {
            let (mut small, mut big) = (0usize, 0usize);
            for (seed, _) in seeds().take(60) {
                small += shape.grow(seed, 0.0).blocks.len();
                big += shape.grow(seed, 1.0).blocks.len();
                if shape != Shape::Stump {
                    // Any one seed grows a different tree at the two ends.
                    assert_ne!(shape.grow(seed, 0.0), shape.grow(seed, 1.0), "{shape:?}");
                }
            }
            assert!(
                big > small,
                "{shape:?}: size 1 ({big}) not bigger than size 0 ({small})"
            );
        }
    }

    #[test]
    fn shapes_have_their_heights_and_parts() {
        let tallest = |s: Shape, size: f64| {
            (0..50)
                .map(|i| {
                    s.grow(splitmix64(i), size)
                        .blocks
                        .iter()
                        .map(|b| b.1)
                        .max()
                        .unwrap_or(0)
                })
                .max()
                .unwrap()
        };
        assert!(tallest(Shape::GiantConifer, 0.0) >= 39);
        assert!(tallest(Shape::GiantConifer, 1.0) >= 60);
        assert!(tallest(Shape::Conifer, 1.0) >= 25);
        assert!(tallest(Shape::BroadCrown, 0.0) >= 11);
        assert!(tallest(Shape::Bush, 1.0) <= 3);
        for i in 0..20 {
            let parts = |s: Shape| -> HashMap<Part, usize> {
                let mut m = HashMap::new();
                for b in s.grow(splitmix64(i), 0.5).blocks {
                    *m.entry(b.3).or_default() += 1;
                }
                m
            };
            assert!(parts(Shape::SnowyConifer).get(&Part::Leaves2).copied().unwrap_or(0) > 10);
            assert!(!parts(Shape::Conifer).contains_key(&Part::Leaves2));
            assert!(!parts(Shape::DeadTree).contains_key(&Part::Leaves));
            assert!(!parts(Shape::Stump).contains_key(&Part::Leaves));
            assert!(parts(Shape::BroadCrown)[&Part::Leaves] > 150);
        }
    }

    /// Renders each shape (side and top views, three sizes, four seeds) to
    /// PPM images in `TREE_RENDER_DIR`, for looking at by eye:
    /// `TREE_RENDER_DIR=/tmp/trees cargo test -p bm-worldgen render_tree_shapes -- --ignored`.
    #[test]
    #[ignore]
    fn render_tree_shapes() {
        let Ok(dir) = std::env::var("TREE_RENDER_DIR") else {
            return;
        };
        let px = 4usize;
        for shape in Shape::ALL {
            let (side, up) = shape.reach();
            let cw = (2 * side + 3) as usize;
            let chh = (up + 2) as usize;
            let cols = 6;
            let (w, hgt) = (cols * cw * px, 2 * (chh + cw) * px);
            let mut img = vec![[235u8, 240, 245]; w * hgt];
            for cell in 0..12 {
                let size = [0.0, 0.5, 1.0][cell % 3];
                let seed = splitmix64(cell as u64 / 3 + 1);
                let t = shape.grow(seed, size);
                let (ox, oy) = ((cell % cols) * cw * px, (cell / cols) * (chh + cw) * px);
                let colour = |p: Part, shade: f64| -> [u8; 3] {
                    let c = match p {
                        Part::Trunk => [120.0, 80.0, 50.0],
                        Part::Leaves => [70.0, 160.0, 70.0],
                        Part::Leaves2 => [240.0, 240.0, 250.0],
                    };
                    [(c[0] * shade) as u8, (c[1] * shade) as u8, (c[2] * shade) as u8]
                };
                // Side view (looking along -z: the nearest block wins), then top view.
                let mut side_best: HashMap<(i32, i32), (i32, Part)> = HashMap::new();
                let mut top_best: HashMap<(i32, i32), (i32, Part)> = HashMap::new();
                for &(x, y, z, p) in &t.blocks {
                    let e = side_best.entry((x, y)).or_insert((z, p));
                    if z >= e.0 {
                        *e = (z, p);
                    }
                    let e = top_best.entry((x, z)).or_insert((y, p));
                    if y >= e.0 {
                        *e = (y, p);
                    }
                }
                let mut fill = |gx: usize, gy: usize, c: [u8; 3]| {
                    for a in 0..px {
                        for b in 0..px {
                            let (xx, yy) = (ox + gx * px + a, oy + gy * px + b);
                            if xx < w && yy < hgt {
                                img[yy * w + xx] = if a == px - 1 || b == px - 1 {
                                    [c[0] / 2 + 20, c[1] / 2 + 20, c[2] / 2 + 20]
                                } else {
                                    c
                                };
                            }
                        }
                    }
                };
                for (&(x, y), &(z, p)) in &side_best {
                    let shade = 0.65 + 0.35 * (z + side) as f64 / (2 * side) as f64;
                    fill((x + side + 1) as usize, (up - y + 1) as usize, colour(p, shade));
                }
                for (&(x, z), &(y, p)) in &top_best {
                    let shade = 0.55 + 0.45 * y as f64 / up.max(1) as f64;
                    fill((x + side + 1) as usize, chh + (z + side + 1) as usize, colour(p, shade));
                }
                fill((side + 1) as usize, up as usize + 2, [255, 0, 0]);
            }
            let mut out = format!("P6\n{w} {hgt}\n255\n").into_bytes();
            for p in img {
                out.extend_from_slice(&p);
            }
            std::fs::write(format!("{dir}/{shape:?}.ppm"), out).unwrap();
        }
    }
}
