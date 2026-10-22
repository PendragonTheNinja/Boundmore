//! Deterministic, tileable noise on the 32 x 32 torus (in units: see
//! `tex::at_res` for painting it at more pixels).
//!
//! Everything here wraps modulo the texture size, so any texture painted
//! from it is seamless on all four edges. Hashing is a fixed integer mix,
//! so every run paints the same pixels on every machine.

use crate::tex::SIZEF;

/// A 32-bit integer mix (Chris Wellons' "lowbias32").
pub fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

pub fn hash2(a: u32, b: u32) -> u32 {
    hash(a ^ hash(b.wrapping_add(0x9e37_79b9)))
}

pub fn hash3(a: u32, b: u32, c: u32) -> u32 {
    hash(a ^ hash2(b, c))
}

/// A hash as a float in 0..1.
pub fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

/// A seed for a named salt, so material code can say `seed("stone")`.
pub fn seed(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5, |h, b| hash(h ^ b as u32))
}

/// A small deterministic random stream for scattering and walking.
#[derive(Clone, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(hash(seed ^ 0x5bd1_e995))
    }

    pub fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        hash(self.0)
    }

    /// A float in 0..1.
    pub fn f(&mut self) -> f32 {
        unit(self.next_u32())
    }

    /// A float in lo..hi.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Value noise with `px` x `py` lattice cells across the tile (each must
/// divide 32 for the result to tile). Returns 0..1.
pub fn value(seed: u32, px: u32, py: u32, x: f32, y: f32) -> f32 {
    let u = x * px as f32 / SIZEF;
    let v = y * py as f32 / SIZEF;
    let (iu, iv) = (u.floor(), v.floor());
    let (fu, fv) = (fade(u - iu), fade(v - iv));
    let (iu, iv) = (iu as i32, iv as i32);
    let at = |i: i32, j: i32| {
        let i = i.rem_euclid(px as i32) as u32;
        let j = j.rem_euclid(py as i32) as u32;
        unit(hash3(seed, i, j))
    };
    let a = at(iu, iv) + (at(iu + 1, iv) - at(iu, iv)) * fu;
    let b = at(iu, iv + 1) + (at(iu + 1, iv + 1) - at(iu, iv + 1)) * fu;
    a + (b - a) * fv
}

/// Fractal sum of `octaves` value-noise layers starting at `period` cells,
/// each twice as fine and half as strong. Returns roughly 0..1.
pub fn fbm(seed: u32, px: u32, py: u32, octaves: u32, x: f32, y: f32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut total = 0.0;
    for o in 0..octaves {
        sum += value(hash2(seed, o), px << o, py << o, x, y) * amp;
        total += amp;
        amp *= 0.5;
    }
    // Stretch back towards 0..1: summed noise clusters around the middle.
    let v = sum / total;
    (0.5 + (v - 0.5) * (1.0 + 0.35 * (octaves as f32 - 1.0))).clamp(0.0, 1.0)
}

/// Shortest signed distance from `a` to `b` on the 32-wide circle.
pub fn wrap_delta(d: f32) -> f32 {
    (d + SIZEF * 0.5).rem_euclid(SIZEF) - SIZEF * 0.5
}

/// Wraps a pixel coordinate into the face (0..32 at the default size).
pub fn wrap_i(v: i32) -> usize {
    v.rem_euclid(crate::tex::res() as i32) as usize
}

/// A point of a cellular pattern, with a few random numbers of its own for
/// the material to spend on tone, tilt, kind and so on.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub x: f32,
    pub y: f32,
    pub r: [f32; 4],
}

/// The nearest site to a point, and the distance to the edge of its cell.
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub index: usize,
    /// Distance to the site.
    pub d1: f32,
    /// Distance to the nearest Voronoi edge (the bisector with the next site).
    pub edge: f32,
    /// Offset from the site to the point (wrapped).
    pub dx: f32,
    pub dy: f32,
    /// Index of the neighbour across the nearest edge.
    pub other: usize,
    /// Unit direction from this site towards that neighbour: which side of
    /// the shape the nearest edge is on.
    pub nx: f32,
    pub ny: f32,
}

impl Cell {
    /// +1 on the top-left rim of the shape (facing the light), -1 on the
    /// bottom-right rim, fading to 0 by `width` pixels in from the edge.
    pub fn rim(&self, width: f32) -> f32 {
        let facing = -(self.nx + self.ny) * std::f32::consts::FRAC_1_SQRT_2;
        let near = (1.0 - self.edge / width).clamp(0.0, 1.0);
        facing * near
    }
}

/// Finds the cell containing (x, y). `sy` stretches the y axis of the metric:
/// above 1 the cells come out wide and flat, like strata.
pub fn cell(sites: &[Site], x: f32, y: f32, sy: f32) -> Cell {
    let rel: Vec<(f32, f32)> = sites
        .iter()
        .map(|s| (wrap_delta(s.x - x), wrap_delta(s.y - y) * sy))
        .collect();
    let mut best = 0;
    let mut bd = f32::MAX;
    for (i, &(dx, dy)) in rel.iter().enumerate() {
        let d = dx * dx + dy * dy;
        if d < bd {
            bd = d;
            best = i;
        }
    }
    let (ax, ay) = rel[best];
    let mut edge = f32::MAX;
    let mut other = best;
    let (mut ex, mut ey) = (0.0, 0.0);
    for (j, &(bx, by)) in rel.iter().enumerate() {
        if j == best {
            continue;
        }
        let (nx, ny) = (bx - ax, by - ay);
        let len = nx.hypot(ny);
        if len < 1e-4 {
            continue;
        }
        let e = ((ax + bx) * 0.5 * nx + (ay + by) * 0.5 * ny) / len;
        if e < edge {
            edge = e;
            other = j;
            ex = nx / len;
            ey = ny / len;
        }
    }
    Cell {
        index: best,
        d1: bd.sqrt(),
        edge,
        dx: -ax,
        dy: -ay / sy,
        other,
        nx: ex,
        ny: ey,
    }
}

/// How far a point is from the nearest tile edge.
pub fn edge_distance(x: f32, y: f32) -> f32 {
    x.min(SIZEF - x).min(y).min(SIZEF - y)
}

/// Dart-throws sites at least `min_dist` apart (on the torus) into the region
/// `keep` accepts, adding to `out`. Deterministic for a seed.
/// `sy` scales vertical distance as in `cell`, so with the same `sy` there
/// the cells come out `1 / sy` times taller than wide.
pub fn scatter_into(
    out: &mut Vec<Site>,
    seed: u32,
    min_dist: f32,
    sy: f32,
    tries: u32,
    keep: impl Fn(f32, f32) -> bool,
) {
    let mut rng = Rng::new(seed);
    for _ in 0..tries {
        let x = rng.range(0.0, SIZEF);
        let y = rng.range(0.0, SIZEF);
        let r = [rng.f(), rng.f(), rng.f(), rng.f()];
        if !keep(x, y) {
            continue;
        }
        let ok = out.iter().all(|s| {
            let dx = wrap_delta(s.x - x);
            let dy = wrap_delta(s.y - y) * sy;
            dx * dx + dy * dy >= min_dist * min_dist
        });
        if ok {
            out.push(Site { x, y, r });
        }
    }
}
