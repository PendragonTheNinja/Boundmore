//! The 32 x 32 canvas, scalar fields, top-left lighting, and the variant
//! context every material paints through.

use crate::color::{Ramp, Rgba};
use crate::noise::{self, Site, edge_distance, hash2, scatter_into, wrap_i};

pub const SIZE: usize = 32;
pub const SIZEF: f32 = SIZE as f32;

/// One 32 x 32 RGBA face.
#[derive(Clone, Debug, PartialEq)]
pub struct Tex {
    pub px: Vec<Rgba>,
}

impl Default for Tex {
    fn default() -> Self {
        Self::filled(Rgba::new(0.0, 0.0, 0.0, 0.0))
    }
}

impl Tex {
    pub fn filled(c: Rgba) -> Self {
        Self {
            px: vec![c; SIZE * SIZE],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Rgba {
        self.px[y * SIZE + x]
    }

    /// Reads with wrap-around, for neighbourhood tests that must tile.
    pub fn get_wrapped(&self, x: i32, y: i32) -> Rgba {
        self.get(wrap_i(x), wrap_i(y))
    }

    pub fn set(&mut self, x: usize, y: usize, c: Rgba) {
        self.px[y * SIZE + x] = c;
    }

    /// Builds a texture pixel by pixel.
    pub fn from_fn(mut f: impl FnMut(usize, usize) -> Rgba) -> Self {
        let mut t = Self::default();
        for y in 0..SIZE {
            for x in 0..SIZE {
                t.set(x, y, f(x, y));
            }
        }
        t
    }

    /// Mean colour in linear light, for the "readable at 4x4" check.
    pub fn mean(&self) -> Rgba {
        let n = self.px.len() as f32;
        let mut acc = [0.0f32; 4];
        for p in &self.px {
            acc[0] += p.r.powf(2.2);
            acc[1] += p.g.powf(2.2);
            acc[2] += p.b.powf(2.2);
            acc[3] += p.a;
        }
        Rgba::new(
            (acc[0] / n).powf(1.0 / 2.2),
            (acc[1] / n).powf(1.0 / 2.2),
            (acc[2] / n).powf(1.0 / 2.2),
            acc[3] / n,
        )
    }
}

/// A scalar per pixel: height, tone, a mask.
#[derive(Clone, Debug)]
pub struct Field {
    pub v: Vec<f32>,
}

impl Field {
    pub fn from_fn(mut f: impl FnMut(usize, usize) -> f32) -> Self {
        let mut v = Vec::with_capacity(SIZE * SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                v.push(f(x, y));
            }
        }
        Self { v }
    }

    pub fn get(&self, x: i32, y: i32) -> f32 {
        self.v[wrap_i(y) * SIZE + wrap_i(x)]
    }

    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.v[y * SIZE + x]
    }

    /// Light from the top left on this height field: positive where the
    /// surface faces the light, negative where it faces away. Central
    /// differences with wrap, so the result tiles.
    pub fn light(&self) -> Field {
        Field::from_fn(|x, y| {
            let (x, y) = (x as i32, y as i32);
            let dx = self.get(x + 1, y) - self.get(x - 1, y);
            let dy = self.get(x, y + 1) - self.get(x, y - 1);
            // Diagonal neighbours too, for a softer, rounder read.
            let dd = self.get(x + 1, y + 1) - self.get(x - 1, y - 1);
            (dx + dy) * 0.35 + dd * 0.3
        })
    }

    /// A 3x3 box blur with wrap: softens hard steps before lighting.
    pub fn blur(&self) -> Field {
        Field::from_fn(|x, y| {
            let (x, y) = (x as i32, y as i32);
            let mut s = 0.0;
            for j in -1..=1 {
                for i in -1..=1 {
                    let w = if i == 0 && j == 0 {
                        4.0
                    } else if i == 0 || j == 0 {
                        2.0
                    } else {
                        1.0
                    };
                    s += self.get(x + i, y + j) * w;
                }
            }
            s / 16.0
        })
    }
}

/// Paints a texture from a tone field through a ramp.
pub fn paint(ramp: &Ramp, tone: &Field) -> Tex {
    Tex::from_fn(|x, y| ramp.pick(tone.at(x, y), x, y))
}

/// Where variants may differ from the base: zero in a band along every edge
/// (so any variant sits next to any other without a seam), one in the middle.
pub const BAND_SOLID: f32 = 3.0;
pub const BAND_FREE: f32 = 9.0;

/// Sites whose centre is at least this far from every edge are free to move
/// between variants; the rest are shared by all of them.
pub const SITE_FREE: f32 = 8.0;

fn variant_mask(x: f32, y: f32) -> f32 {
    let d = edge_distance(x, y);
    let t = ((d - BAND_SOLID) / (BAND_FREE - BAND_SOLID)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The variant being painted. Variant 0 is the base texture; variants 1..3
/// share the base's edge band and rearrange the middle.
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    pub variant: u32,
}

impl Ctx {
    pub const BASE: Ctx = Ctx { variant: 0 };

    pub fn new(variant: u32) -> Self {
        Self { variant }
    }

    /// Blends a pattern painted with the base seed (at the edges) into the
    /// same pattern with this variant's seed (in the middle), keeping the
    /// contrast of the blend equal to that of either pattern.
    fn blend(&self, x: f32, y: f32, f: impl Fn(u32) -> f32, salt: u32) -> f32 {
        let base = f(salt);
        if self.variant == 0 {
            return base;
        }
        let m = variant_mask(x, y);
        if m <= 0.0 {
            return base;
        }
        let var = f(hash2(salt, self.variant.wrapping_mul(0x2545_f491)));
        let w = ((1.0 - m) * (1.0 - m) + m * m).sqrt();
        0.5 + ((base - 0.5) * (1.0 - m) + (var - 0.5) * m) / w
    }

    /// Variant-aware value noise (see `noise::value`).
    pub fn value(&self, salt: u32, px: u32, py: u32, x: f32, y: f32) -> f32 {
        self.blend(x, y, |s| noise::value(s, px, py, x, y), salt)
    }

    /// Variant-aware fractal noise (see `noise::fbm`).
    pub fn fbm(&self, salt: u32, px: u32, py: u32, octaves: u32, x: f32, y: f32) -> f32 {
        self.blend(x, y, |s| noise::fbm(s, px, py, octaves, x, y), salt)
    }

    /// Variant-aware sites: the base's sites near the edges, then this
    /// variant's own sites in the middle.
    pub fn sites(&self, salt: u32, min_dist: f32) -> Vec<Site> {
        self.sites_stretched(salt, min_dist, 1.0)
    }

    /// Sites spaced for a metric with vertical scale `sy` (see `noise::cell`).
    pub fn sites_stretched(&self, salt: u32, min_dist: f32, sy: f32) -> Vec<Site> {
        let tries = 4000;
        let mut base = Vec::new();
        scatter_into(&mut base, salt, min_dist, sy, tries, |_, _| true);
        if self.variant == 0 {
            return base;
        }
        let mut out: Vec<Site> = base
            .into_iter()
            .filter(|s| edge_distance(s.x, s.y) < SITE_FREE)
            .collect();
        let vseed = hash2(salt, self.variant.wrapping_mul(0x9e37_79b9));
        scatter_into(&mut out, vseed, min_dist, sy, tries, |x, y| {
            edge_distance(x, y) >= SITE_FREE
        });
        out
    }
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Pixel centre.
pub fn c(v: usize) -> f32 {
    v as f32 + 0.5
}
