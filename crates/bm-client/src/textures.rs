//! Block textures: from the catalogue's names to GPU-ready layers.
//!
//! The server's catalogue names each face's texture (`docs/art-direction.md`);
//! the files live in the client's `assets/textures/`. Every distinct name
//! becomes a run of layers in one texture array (the base file plus any
//! `_1`.. `_3` variants), so the shader picks a layer per face and a variant
//! per block position with no texture switches at all.
//!
//! A missing file is reported loudly (log and title bar) and that block falls
//! back to its catalogue colour; it never becomes an invisible or garbage face.

use bm_core::BlockRegistry;
use std::collections::HashMap;
use std::path::Path;

pub const SIZE: u32 = 32;
pub const MIP_LEVELS: u32 = 6; // 32, 16, 8, 4, 2, 1
const MAX_VARIANTS: u32 = 4;
pub const CRACK_STAGES: u32 = 8;
/// Marks a face with no texture: the shader uses the block's colour.
pub const NO_TEXTURE: u32 = 0xffff;

/// RGBA8 sRGB pixels for one layer, mip 0.
pub type Layer = Vec<u8>;

pub struct TextureSet {
    pub layers: Vec<Layer>,
    /// Per block id: top, side, bottom as `first_layer | variants << 16`
    /// (or `NO_TEXTURE`), then a spare.
    pub faces: Vec<[u32; 4]>,
    /// First crack overlay layer.
    pub crack_base: Option<u32>,
    pub missing: Vec<String>,
}

fn load_png(path: &Path) -> Result<Layer, String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if img.width() != SIZE || img.height() != SIZE {
        return Err(format!(
            "{} is {}x{}, textures must be {SIZE}x{SIZE}",
            path.display(),
            img.width(),
            img.height()
        ));
    }
    Ok(img.to_rgba8().into_raw())
}

impl TextureSet {
    /// Loads every texture the catalogue names from `dir` (normally
    /// `assets/textures`).
    pub fn load(registry: &BlockRegistry, dir: &Path) -> Self {
        let blocks = dir.join("blocks");
        let mut layers = Vec::new();
        let mut by_name: HashMap<String, u32> = HashMap::new();
        let mut missing = Vec::new();
        let mut entry = |name: &str, layers: &mut Vec<Layer>, missing: &mut Vec<String>| -> u32 {
            if let Some(e) = by_name.get(name) {
                return *e;
            }
            let e = match load_png(&blocks.join(format!("{name}.png"))) {
                Ok(base) => {
                    let first = layers.len() as u32;
                    layers.push(base);
                    let mut count = 1;
                    while count < MAX_VARIANTS {
                        let path = blocks.join(format!("{name}_{count}.png"));
                        if !path.exists() {
                            break;
                        }
                        match load_png(&path) {
                            Ok(l) => layers.push(l),
                            Err(e) => {
                                tracing::error!("texture variant: {e}");
                                break;
                            }
                        }
                        count += 1;
                    }
                    first | count << 16
                }
                Err(e) => {
                    tracing::error!("missing texture: {e}");
                    missing.push(name.to_string());
                    NO_TEXTURE
                }
            };
            by_name.insert(name.to_string(), e);
            e
        };
        let mut faces = Vec::with_capacity(registry.len());
        for (_, def) in registry.iter() {
            faces.push(match def.textures.faces() {
                Some([top, side, bottom]) => [
                    entry(top, &mut layers, &mut missing),
                    entry(side, &mut layers, &mut missing),
                    entry(bottom, &mut layers, &mut missing),
                    0,
                ],
                None => [NO_TEXTURE, NO_TEXTURE, NO_TEXTURE, 0],
            });
        }
        let crack_base = {
            let first = layers.len() as u32;
            let stages: Result<Vec<Layer>, String> = (0..CRACK_STAGES)
                .map(|i| load_png(&dir.join("overlay").join(format!("crack_{i}.png"))))
                .collect();
            match stages {
                Ok(s) => {
                    layers.extend(s);
                    Some(first)
                }
                Err(e) => {
                    // Breaking must always show progress, so draw simple
                    // cracks rather than show nothing.
                    tracing::warn!("no crack overlay ({e}); using built-in cracks");
                    layers.extend((0..CRACK_STAGES).map(builtin_crack));
                    Some(first)
                }
            }
        };
        if layers.is_empty() {
            // The array needs one layer to exist at all.
            layers.push(vec![255; (SIZE * SIZE * 4) as usize]);
        }
        Self {
            layers,
            faces,
            crack_base,
            missing,
        }
    }
}

/// Cracks drawn in code, for when no overlay files are installed: dark,
/// branching lines spreading from the middle, more of them each stage.
fn builtin_crack(stage: u32) -> Layer {
    let n = SIZE as i32;
    let mut px = vec![0u8; (SIZE * SIZE * 4) as usize];
    let mut put = |x: i32, y: i32, a: u8| {
        let (x, y) = (x.rem_euclid(n), y.rem_euclid(n));
        let i = ((y * n + x) * 4) as usize;
        let alpha = px[i + 3].max(a);
        px[i..i + 4].copy_from_slice(&[18, 14, 12, alpha]);
    };
    let mut seed = 0x9e37_79b9u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    // Each stage adds two more branches, each longer than the last.
    for branch in 0..(stage + 1) * 2 {
        let (mut x, mut y) = (n / 2, n / 2);
        let mut dir = (branch * 7 + next() % 3) % 8;
        let len = 4 + stage * 2 + next() % 4;
        for _ in 0..len {
            let (dx, dy) = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)][dir as usize];
            x += dx;
            y += dy;
            put(x, y, 230);
            if next() % 4 == 0 {
                dir = (dir + if next() % 2 == 0 { 1 } else { 7 }) % 8;
            }
        }
    }
    px
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> u8 {
    let c = if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// The next mip of a square RGBA8 sRGB image: a 2×2 box filter in linear
/// light, weighted by alpha so transparent pixels do not darken edges.
pub fn downsample(src: &[u8], size: u32) -> Vec<u8> {
    let half = (size / 2).max(1);
    let mut out = vec![0u8; (half * half * 4) as usize];
    for y in 0..half {
        for x in 0..half {
            let mut acc = [0f32; 3];
            let mut alpha = 0f32;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = ((x * 2 + dx).min(size - 1), (y * 2 + dy).min(size - 1));
                let i = ((sy * size + sx) * 4) as usize;
                let a = src[i + 3] as f32 / 255.0;
                for c in 0..3 {
                    acc[c] += srgb_to_linear(src[i + c]) * a;
                }
                alpha += a;
            }
            let o = ((y * half + x) * 4) as usize;
            for c in 0..3 {
                out[o + c] = linear_to_srgb(if alpha > 0.0 { acc[c] / alpha } else { 0.0 });
            }
            out[o + 3] = (alpha / 4.0 * 255.0 + 0.5) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, rgba: [u8; 4]) {
        let img = image::RgbaImage::from_pixel(SIZE, SIZE, image::Rgba(rgba));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        img.save(path).unwrap();
    }

    #[test]
    fn faces_resolve_with_variants_and_missing_files_fall_back() {
        let dir = std::env::temp_dir().join(format!("bm-tex-{}", std::process::id()));
        let blocks = dir.join("blocks");
        write_png(&blocks.join("turf_top.png"), [0, 200, 0, 255]);
        write_png(&blocks.join("turf_top_1.png"), [0, 180, 0, 255]);
        write_png(&blocks.join("turf_side.png"), [120, 90, 0, 255]);
        write_png(&blocks.join("soil.png"), [100, 70, 40, 255]);
        let reg = BlockRegistry::builtin();
        let set = TextureSet::load(&reg, &dir);
        let turf = &set.faces[reg.id("turf").unwrap().0 as usize];
        assert_eq!(turf[0] >> 16, 2, "turf top has a variant");
        assert_eq!(turf[1] >> 16, 1);
        let soil = &set.faces[reg.id("soil").unwrap().0 as usize];
        assert_eq!(soil[0], turf[2], "soil reuses the same layers as turf's bottom");
        let stone = &set.faces[reg.id("stone").unwrap().0 as usize];
        assert_eq!(stone[0], NO_TEXTURE);
        assert!(set.missing.contains(&"stone".to_string()));
        assert_eq!(
            set.faces[0],
            [NO_TEXTURE, NO_TEXTURE, NO_TEXTURE, 0],
            "air has no textures"
        );
        assert!(set.crack_base.is_some(), "built-in cracks stand in for missing files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn downsampling_preserves_flat_colour_and_ignores_transparency() {
        let mut src = vec![0u8; (SIZE * SIZE * 4) as usize];
        for px in src.chunks_mut(4) {
            px.copy_from_slice(&[200, 100, 50, 255]);
        }
        // A transparent black pixel must not darken its neighbours.
        src[0..4].copy_from_slice(&[0, 0, 0, 0]);
        let m = downsample(&src, SIZE);
        assert_eq!(m.len(), (16 * 16 * 4) as usize);
        assert_eq!(&m[4..8], &[200, 100, 50, 255]);
        assert_eq!(&m[0..3], &[200, 100, 50]);
        assert!(m[3] < 255 && m[3] > 180);
    }
}
