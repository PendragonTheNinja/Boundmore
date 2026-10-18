//! Colour: sRGB values, Oklab for perceptual mixing, and palette ramps.
//!
//! Every texture is painted from a handful of ramps, each a short run of
//! colours from shadow to highlight. Ramps are built in OKLCH so a shadow
//! can lean towards blue or purple and a highlight towards warm gold while
//! lightness stays even, which is the hue-shifted shading the brief asks for.

/// An sRGB colour with components in 0..=1 and straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// From a `0xRRGGBB` literal, opaque.
    pub const fn hex(v: u32) -> Self {
        Self::new(
            ((v >> 16) & 0xff) as f32 / 255.0,
            ((v >> 8) & 0xff) as f32 / 255.0,
            (v & 0xff) as f32 / 255.0,
            1.0,
        )
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    pub fn to_u8(self) -> [u8; 4] {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(self.r), q(self.g), q(self.b), q(self.a)]
    }

    pub fn from_u8(p: [u8; 4]) -> Self {
        Self::new(
            p[0] as f32 / 255.0,
            p[1] as f32 / 255.0,
            p[2] as f32 / 255.0,
            p[3] as f32 / 255.0,
        )
    }

    /// Multiplies the colour (not alpha) in linear light, as a renderer's
    /// face shading would.
    pub fn scaled(self, k: f32) -> Self {
        let f = |c: f32| to_srgb(to_linear(c) * k);
        Self::new(f(self.r), f(self.g), f(self.b), self.a)
    }

    /// Mixes towards `other` by `t` in Oklab, alpha linearly.
    pub fn mix(self, other: Self, t: f32) -> Self {
        let a = Lab::from_rgb(self);
        let b = Lab::from_rgb(other);
        let l = Lab {
            l: a.l + (b.l - a.l) * t,
            a: a.a + (b.a - a.a) * t,
            b: a.b + (b.b - a.b) * t,
        };
        l.to_rgb(self.a + (other.a - self.a) * t)
    }

    /// Source-over compositing of `top` onto `self`, in sRGB like most
    /// image tools (good enough for review images).
    pub fn over(self, top: Self) -> Self {
        let a = top.a + self.a * (1.0 - top.a);
        if a <= 0.0 {
            return Self::new(0.0, 0.0, 0.0, 0.0);
        }
        let c = |t: f32, s: f32| (t * top.a + s * self.a * (1.0 - top.a)) / a;
        Self::new(c(top.r, self.r), c(top.g, self.g), c(top.b, self.b), a)
    }
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Oklab (Björn Ottosson, 2020).
#[derive(Clone, Copy, Debug)]
pub struct Lab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

impl Lab {
    pub fn from_rgb(c: Rgba) -> Self {
        let (r, g, b) = (to_linear(c.r), to_linear(c.g), to_linear(c.b));
        let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        Self {
            l: 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            a: 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            b: 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        }
    }

    pub fn to_rgb(self, alpha: f32) -> Rgba {
        let l = self.l + 0.396_337_78 * self.a + 0.215_803_76 * self.b;
        let m = self.l - 0.105_561_346 * self.a - 0.063_854_17 * self.b;
        let s = self.l - 0.089_484_18 * self.a - 1.291_485_5 * self.b;
        let (l, m, s) = (l * l * l, m * m * m, s * s * s);
        Rgba::new(
            to_srgb(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
            to_srgb(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
            to_srgb(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
            alpha,
        )
    }

    fn lch(self) -> (f32, f32, f32) {
        (self.l, self.a.hypot(self.b), self.b.atan2(self.a).to_degrees())
    }

    fn from_lch(l: f32, c: f32, h: f32) -> Self {
        let h = h.to_radians();
        Self {
            l,
            a: c * h.cos(),
            b: c * h.sin(),
        }
    }
}

/// Moves hue `h` towards `target` (degrees) by `t` along the short way round.
fn hue_towards(h: f32, target: f32, t: f32) -> f32 {
    let d = (target - h + 540.0).rem_euclid(360.0) - 180.0;
    h + d * t
}

/// How to build a ramp from one base colour.
#[derive(Clone, Copy, Debug)]
pub struct RampSpec {
    /// The colour in the middle of the ramp.
    pub base: Rgba,
    /// Number of colours.
    pub steps: usize,
    /// Oklab lightness of the darkest and lightest step.
    pub dark: f32,
    pub light: f32,
    /// Hue the shadows lean towards and how far (0..1) at the darkest step.
    pub cool_hue: f32,
    pub cool: f32,
    /// Hue the highlights lean towards and how far at the lightest step.
    pub warm_hue: f32,
    pub warm: f32,
    /// Chroma multiplier at the dark and light ends (1 = keep).
    pub dark_chroma: f32,
    pub light_chroma: f32,
}

/// Hues (Oklab degrees) the ramps lean towards.
pub const HUE_PURPLE: f32 = 300.0;
pub const HUE_BLUE: f32 = 265.0;
pub const HUE_GOLD: f32 = 85.0;
pub const HUE_CYAN: f32 = 200.0;

impl RampSpec {
    pub fn new(base: Rgba, steps: usize, dark: f32, light: f32) -> Self {
        Self {
            base,
            steps,
            dark,
            light,
            cool_hue: HUE_BLUE,
            cool: 0.35,
            warm_hue: HUE_GOLD,
            warm: 0.25,
            dark_chroma: 1.0,
            light_chroma: 0.85,
        }
    }

    pub fn cool(mut self, hue: f32, amount: f32) -> Self {
        self.cool_hue = hue;
        self.cool = amount;
        self
    }

    pub fn warm(mut self, hue: f32, amount: f32) -> Self {
        self.warm_hue = hue;
        self.warm = amount;
        self
    }

    pub fn chroma(mut self, dark: f32, light: f32) -> Self {
        self.dark_chroma = dark;
        self.light_chroma = light;
        self
    }

    pub fn build(self) -> Ramp {
        let (_, c0, h0) = Lab::from_rgb(self.base).lch();
        let n = self.steps.max(2);
        let colors = (0..n)
            .map(|i| {
                let t = i as f32 / (n - 1) as f32;
                let l = self.dark + (self.light - self.dark) * t;
                // -1 at the dark end, 0 in the middle, +1 at the light end.
                let s = t * 2.0 - 1.0;
                let (h, c) = if s < 0.0 {
                    let k = -s;
                    (
                        hue_towards(h0, self.cool_hue, self.cool * k),
                        c0 * (1.0 + (self.dark_chroma - 1.0) * k),
                    )
                } else {
                    (
                        hue_towards(h0, self.warm_hue, self.warm * s),
                        c0 * (1.0 + (self.light_chroma - 1.0) * s),
                    )
                };
                Lab::from_lch(l, c, h).to_rgb(1.0)
            })
            .collect();
        Ramp { colors }
    }
}

/// A run of colours from shadow to highlight.
#[derive(Clone, Debug)]
pub struct Ramp {
    pub colors: Vec<Rgba>,
}

/// 4x4 ordered-dither thresholds in 0..1.
const BAYER: [[f32; 4]; 4] = [
    [0.0 / 16.0, 8.0 / 16.0, 2.0 / 16.0, 10.0 / 16.0],
    [12.0 / 16.0, 4.0 / 16.0, 14.0 / 16.0, 6.0 / 16.0],
    [3.0 / 16.0, 11.0 / 16.0, 1.0 / 16.0, 9.0 / 16.0],
    [15.0 / 16.0, 7.0 / 16.0, 13.0 / 16.0, 5.0 / 16.0],
];

/// Width of the dithered band at each ramp boundary, as a fraction of one
/// step. Small, so dither only softens the edge between two flat areas.
const DITHER_BAND: f32 = 0.3;

impl Ramp {
    pub fn from_hex(hex: &[u32]) -> Self {
        Self {
            colors: hex.iter().map(|&h| Rgba::hex(h)).collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// The colour for tone `t` in 0..1, quantised to the ramp, with a thin
    /// band of ordered dither where two steps meet.
    pub fn pick(&self, t: f32, x: usize, y: usize) -> Rgba {
        let n = self.colors.len();
        let p = t.clamp(0.0, 1.0) * (n - 1) as f32;
        let i = p.floor() as usize;
        if i >= n - 1 {
            return self.colors[n - 1];
        }
        let f = p - i as f32;
        let threshold = 0.5 + (BAYER[y % 4][x % 4] - 0.5) * DITHER_BAND;
        if f > threshold {
            self.colors[i + 1]
        } else {
            self.colors[i]
        }
    }

    /// Step `i`, clamped.
    pub fn at(&self, i: usize) -> Rgba {
        self.colors[i.min(self.colors.len() - 1)]
    }

    pub fn darkest(&self) -> Rgba {
        self.colors[0]
    }

    pub fn lightest(&self) -> Rgba {
        self.colors[self.colors.len() - 1]
    }
}
