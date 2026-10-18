//! Periodic gradient noise.
//!
//! A planet wraps in X and Z, so its terrain must tile with the planet's width
//! or a seam appears where the laps meet. Gradient noise tiles exactly when the
//! lattice hash is taken modulo a whole number of cells, so every noise here
//! takes a period in lattice cells, and an fBm doubles the period with the
//! frequency so every octave still tiles.
//!
//! Everything is integer hashing plus `f64` add and multiply, so a seed gives
//! the same terrain on every machine.

use std::f64::consts::{FRAC_1_SQRT_2, SQRT_2};

#[inline]
fn mix(mut h: u64) -> u64 {
    // SplitMix64 finaliser: cheap, and every input bit reaches every output bit.
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

#[inline]
pub fn hash3(seed: u64, x: i64, y: i64, z: i64) -> u64 {
    let mut h = mix(seed ^ 0x9e37_79b9_7f4a_7c15);
    h = mix(h ^ x as u64);
    h = mix(h ^ (y as u64).wrapping_mul(0x632b_e59b_d9b4_e019));
    mix(h ^ (z as u64).wrapping_mul(0x8cb9_2ba7_2f3d_8dd7))
}

/// A uniform value in `[0, 1)` for a lattice point.
#[inline]
pub fn hash01(seed: u64, x: i64, y: i64, z: i64) -> f64 {
    (hash3(seed, x, y, z) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

const GRAD2: [(f64, f64); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (FRAC_1_SQRT_2, FRAC_1_SQRT_2),
    (-FRAC_1_SQRT_2, FRAC_1_SQRT_2),
    (FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
    (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
];

const GRAD3: [(f64, f64, f64); 12] = [
    (1.0, 1.0, 0.0),
    (-1.0, 1.0, 0.0),
    (1.0, -1.0, 0.0),
    (-1.0, -1.0, 0.0),
    (1.0, 0.0, 1.0),
    (-1.0, 0.0, 1.0),
    (1.0, 0.0, -1.0),
    (-1.0, 0.0, -1.0),
    (0.0, 1.0, 1.0),
    (0.0, -1.0, 1.0),
    (0.0, 1.0, -1.0),
    (0.0, -1.0, -1.0),
];

/// 2D Perlin noise in roughly `[-1, 1]`, periodic every `period` cells in both
/// axes. `x`, `z` are in lattice units.
pub fn perlin2(seed: u64, x: f64, z: f64, period: i64) -> f64 {
    let (xf, zf) = (x.floor(), z.floor());
    let (xi, zi) = (xf as i64, zf as i64);
    let (tx, tz) = (x - xf, z - zf);
    let g = |cx: i64, cz: i64, dx: f64, dz: f64| {
        let h = hash3(seed, cx.rem_euclid(period), 0, cz.rem_euclid(period));
        let (gx, gz) = GRAD2[(h >> 61) as usize];
        gx * dx + gz * dz
    };
    let n00 = g(xi, zi, tx, tz);
    let n10 = g(xi + 1, zi, tx - 1.0, tz);
    let n01 = g(xi, zi + 1, tx, tz - 1.0);
    let n11 = g(xi + 1, zi + 1, tx - 1.0, tz - 1.0);
    let (u, v) = (fade(tx), fade(tz));
    lerp(lerp(n00, n10, u), lerp(n01, n11, u), v) * SQRT_2
}

/// 3D Perlin noise in roughly `[-1, 1]`, periodic every `period` cells in X and
/// Z (Y does not wrap).
pub fn perlin3(seed: u64, x: f64, y: f64, z: f64, period: i64) -> f64 {
    let (xf, yf, zf) = (x.floor(), y.floor(), z.floor());
    let (xi, yi, zi) = (xf as i64, yf as i64, zf as i64);
    let (tx, ty, tz) = (x - xf, y - yf, z - zf);
    let g = |cx: i64, cy: i64, cz: i64, dx: f64, dy: f64, dz: f64| {
        let h = hash3(seed, cx.rem_euclid(period), cy, cz.rem_euclid(period));
        let (gx, gy, gz) = GRAD3[(h % 12) as usize];
        gx * dx + gy * dy + gz * dz
    };
    let (u, v, w) = (fade(tx), fade(ty), fade(tz));
    let x00 = lerp(g(xi, yi, zi, tx, ty, tz), g(xi + 1, yi, zi, tx - 1.0, ty, tz), u);
    let x10 = lerp(
        g(xi, yi + 1, zi, tx, ty - 1.0, tz),
        g(xi + 1, yi + 1, zi, tx - 1.0, ty - 1.0, tz),
        u,
    );
    let x01 = lerp(
        g(xi, yi, zi + 1, tx, ty, tz - 1.0),
        g(xi + 1, yi, zi + 1, tx - 1.0, ty, tz - 1.0),
        u,
    );
    let x11 = lerp(
        g(xi, yi + 1, zi + 1, tx, ty - 1.0, tz - 1.0),
        g(xi + 1, yi + 1, zi + 1, tx - 1.0, ty - 1.0, tz - 1.0),
        u,
    );
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w)
}

/// Fractal noise that tiles a planet. `cells` is the number of lattice cells
/// around the planet at the first octave; each octave doubles it.
#[derive(Clone, Copy, Debug)]
pub struct Fbm {
    pub seed: u64,
    pub cells: i64,
    pub octaves: u32,
    pub gain: f64,
}

impl Fbm {
    /// `feature` is the approximate size of the largest features, in blocks.
    /// It is rounded so a whole number of cells fits around the planet.
    pub fn new(seed: u64, planet_width: i64, feature: f64, octaves: u32, gain: f64) -> Self {
        let cells = ((planet_width as f64 / feature).round() as i64).max(1);
        Self {
            seed,
            cells,
            octaves,
            gain,
        }
    }

    /// Sample at block coordinates; `width` is the planet width in blocks.
    pub fn sample2(&self, x: f64, z: f64, width: f64) -> f64 {
        let (mut sum, mut amp, mut norm) = (0.0, 1.0, 0.0);
        let mut cells = self.cells;
        for o in 0..self.octaves {
            let scale = cells as f64 / width;
            sum += amp * perlin2(self.seed.wrapping_add(o as u64), x * scale, z * scale, cells);
            norm += amp;
            amp *= self.gain;
            cells *= 2;
        }
        sum / norm
    }

    /// Ridged variant: sharp crests where the base noise crosses zero.
    pub fn ridged2(&self, x: f64, z: f64, width: f64) -> f64 {
        let (mut sum, mut amp, mut norm) = (0.0, 1.0, 0.0);
        let mut cells = self.cells;
        for o in 0..self.octaves {
            let scale = cells as f64 / width;
            let n = 1.0 - perlin2(self.seed.wrapping_add(o as u64), x * scale, z * scale, cells).abs();
            sum += amp * n * n;
            norm += amp;
            amp *= self.gain;
            cells *= 2;
        }
        sum / norm
    }

    /// 3D sample; Y uses the same block scale as X and Z.
    pub fn sample3(&self, x: f64, y: f64, z: f64, width: f64) -> f64 {
        let (mut sum, mut amp, mut norm) = (0.0, 1.0, 0.0);
        let mut cells = self.cells;
        for o in 0..self.octaves {
            let scale = cells as f64 / width;
            sum += amp * perlin3(self.seed.wrapping_add(o as u64), x * scale, y * scale, z * scale, cells);
            norm += amp;
            amp *= self.gain;
            cells *= 2;
        }
        sum / norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_tiles_with_its_period() {
        for i in 0..50 {
            let x = i as f64 * 0.37;
            let z = i as f64 * 0.91;
            // Shifting by whole periods only rounds the fraction, so compare
            // to within float noise rather than exactly.
            assert!((perlin2(7, x, z, 8) - perlin2(7, x + 8.0, z - 16.0, 8)).abs() < 1e-12);
            assert!((perlin3(7, x, 1.3, z, 8) - perlin3(7, x - 8.0, 1.3, z + 8.0, 8)).abs() < 1e-12);
        }
    }

    #[test]
    fn fbm_tiles_with_the_planet() {
        let w = 1024.0;
        let f = Fbm::new(3, 1024, 300.0, 5, 0.5);
        for i in 0..50 {
            let x = i as f64 * 13.7;
            let z = i as f64 * 7.3;
            let a = f.sample2(x, z, w);
            let b = f.sample2(x + w, z - w, w);
            assert!((a - b).abs() < 1e-12, "{a} vs {b}");
        }
    }

    #[test]
    fn noise_stays_in_range_and_varies() {
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for i in 0..20_000 {
            let x = i as f64 * 0.173;
            let n = perlin2(1, x, x * 0.61, 1 << 20);
            lo = lo.min(n);
            hi = hi.max(n);
            let n3 = perlin3(1, x, x * 0.3, x * 0.7, 1 << 20);
            assert!((-1.01..=1.01).contains(&n3));
        }
        assert!(lo > -1.01 && hi < 1.01);
        assert!(hi - lo > 1.0, "range {lo}..{hi}");
    }

    #[test]
    fn hash01_is_uniformish() {
        let mean: f64 = (0..10_000).map(|i| hash01(5, i, 0, -i)).sum::<f64>() / 10_000.0;
        assert!((mean - 0.5).abs() < 0.02, "{mean}");
    }
}
