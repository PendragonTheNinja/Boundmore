//! Block colours on the client (ADR-0007): how a palette colour repaints a
//! texture, shared by the terrain shader and the CPU item icons so a block
//! in the hand looks as it does in the world.
//!
//! A coloured face keeps its texture's light and shade but takes the palette
//! colour's hue and brightness: each texel's brightness relative to the
//! face's average (`detail`) scales the palette colour. Natural (colour 0)
//! never comes here; it renders exactly as painted.
//!
//! ```text
//! detail = clamp(luma(texel) / luma(face average), 0.15, 3.0)
//! out    = max(colour * detail + DARK_DETAIL * (detail - 1), 0)
//! out    = soft_highlight(out)        // past KNEE, ease towards 1
//! ```
//!
//! `DARK_DETAIL` keeps a little grain on the darkest colours (where
//! `colour * detail` alone would flatten to black); `soft_highlight` keeps
//! highlights on the palest ones from clipping flat. Must match `recolour`
//! in `shaders/terrain.wgsl`.

use crate::textures::{self, NO_TEXTURE};
use bm_core::{Colour, ColourPalette};

/// Palette entries on the GPU: index = colour id, 0 unused (natural).
pub const PALETTE_COLOURS: usize = 256;
const DETAIL_MIN: f32 = 0.15;
const DETAIL_MAX: f32 = 3.0;
const DARK_DETAIL: f32 = 0.012;
const KNEE: f32 = 0.8;

/// Relative luminance of a linear colour.
pub fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

pub fn srgb_to_linear(c: [u8; 3]) -> [f32; 3] {
    c.map(textures::srgb_to_linear)
}

/// A texel (linear) on a face whose average luma is `base`, in `colour`
/// (linear).
pub fn recolour(texel: [f32; 3], base: f32, colour: [f32; 3]) -> [f32; 3] {
    let detail = (luma(texel) / base.max(0.002)).clamp(DETAIL_MIN, DETAIL_MAX);
    let out = colour.map(|c| (c * detail + DARK_DETAIL * (detail - 1.0)).max(0.0));
    let m = out[0].max(out[1]).max(out[2]);
    if m <= KNEE {
        return out;
    }
    let eased = KNEE + (1.0 - KNEE) * (1.0 - (-(m - KNEE) / (1.0 - KNEE)).exp());
    out.map(|c| c * eased / m)
}

/// Average luma of one texture layer (RGBA8 sRGB), weighted by alpha so the
/// holes in foliage and glass do not count.
pub fn layer_luma(layer: &[u8]) -> f32 {
    let (mut sum, mut weight) = (0.0, 0.0);
    for px in layer.chunks_exact(4) {
        let a = px[3] as f32 / 255.0;
        sum += luma(srgb_to_linear([px[0], px[1], px[2]])) * a;
        weight += a;
    }
    if weight > 0.0 { sum / weight } else { 0.0 }
}

/// Average luma of a face table entry (`first_layer | variants << 16`) over
/// all its variants, or None for no texture.
pub fn face_luma(entry: u32, layers: &[textures::Layer]) -> Option<f32> {
    if entry == NO_TEXTURE {
        return None;
    }
    let (first, variants) = ((entry & 0xffff) as usize, ((entry >> 16) as usize).max(1));
    let found: Vec<f32> = (first..first + variants)
        .filter_map(|i| layers.get(i))
        .map(|l| layer_luma(l))
        .collect();
    (!found.is_empty()).then(|| found.iter().sum::<f32>() / found.len() as f32)
}

/// The palette as the shader reads it: linear rgb per colour id, with its
/// luma in w; entry 0 (natural) is zero.
pub fn palette_uniform(p: &ColourPalette) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0; 4]; PALETTE_COLOURS];
    for (i, def) in p.colours.iter().enumerate().take(PALETTE_COLOURS - 1) {
        let [r, g, b] = srgb_to_linear(def.rgb);
        out[i + 1] = [r, g, b, luma([r, g, b])];
    }
    out
}

/// The palette colour of `c` (linear), or None for natural or an id the
/// palette does not hold.
pub fn linear(p: &ColourPalette, c: Colour) -> Option<[f32; 3]> {
    (c != Colour::NATURAL)
        .then(|| p.get(c).map(|d| srgb_to_linear(d.rgb)))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_packs_one_entry_per_id_with_natural_empty() {
        let p = ColourPalette::builtin();
        let u = palette_uniform(&p);
        assert_eq!(u.len(), 256);
        assert_eq!(std::mem::size_of_val(u.as_slice()), 4096, "a 4 KiB uniform");
        assert_eq!(u[0], [0.0; 4], "natural has no colour");
        for id in [1u8, 77, 255] {
            let rgb = srgb_to_linear(p.get(Colour(id)).unwrap().rgb);
            assert_eq!(u[id as usize][..3], rgb);
            assert!((u[id as usize][3] - luma(rgb)).abs() < 1e-6);
        }
    }

    #[test]
    fn an_average_texel_takes_the_colour_and_detail_survives() {
        let c = [0.2, 0.05, 0.6];
        let mid = recolour([0.3; 3], 0.3, c);
        for k in 0..3 {
            assert!((mid[k] - c[k]).abs() < 1e-5, "{mid:?}");
        }
        let dark = recolour([0.15; 3], 0.3, c);
        let light = recolour([0.45; 3], 0.3, c);
        assert!(luma(dark) < luma(mid) && luma(mid) < luma(light));
    }

    #[test]
    fn pale_colours_ease_into_highlights_and_dark_ones_keep_grain() {
        let snow = [0.9, 0.9, 0.92];
        let hi = recolour([0.6; 3], 0.4, snow);
        assert!(hi.iter().all(|v| *v <= 1.0), "{hi:?}");
        let higher = recolour([0.7; 3], 0.4, snow);
        assert!(luma(higher) > luma(hi), "highlights still step");
        let coal = [0.004, 0.004, 0.005];
        let (lo, up) = (recolour([0.2; 3], 0.4, coal), recolour([0.6; 3], 0.4, coal));
        assert!(luma(up) - luma(lo) > 0.01, "{lo:?} {up:?}");
    }

    #[test]
    fn face_luma_averages_variants_and_ignores_transparency() {
        let px = |rgba: [u8; 4]| rgba.repeat(4);
        let mut half_clear = px([255, 255, 255, 255]);
        half_clear[4..8].copy_from_slice(&[0, 0, 0, 0]);
        assert!((layer_luma(&half_clear) - 1.0).abs() < 1e-5);
        let layers = vec![px([0, 0, 0, 255]), px([255, 255, 255, 255]), px([0, 0, 0, 255])];
        assert!((face_luma(1 | 2 << 16, &layers).unwrap() - 0.5).abs() < 1e-5);
        assert_eq!(face_luma(NO_TEXTURE, &layers), None);
    }
}
