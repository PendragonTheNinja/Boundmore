//! Item icons, made on the CPU once per catalogue.
//!
//! A block item is drawn as a small isometric cube from its own face
//! textures (the look the M2 hotbar had), lit from the top left as the art
//! direction asks. Any other item uses `assets/textures/items/<icon>.png` if
//! there is one, and otherwise a coloured tile with its initials, so a
//! missing picture is obvious but never blank.
//!
//! Plain RGBA out, no GPU and no egui: the menus upload the images, and the
//! rasteriser stays testable.

use crate::items::{fallback_colour, initials};
use crate::textures::{self, NO_TEXTURE, TextureSet};
use bm_core::{BlockDef, BlockRegistry, ItemDef, ItemKind};
use std::path::Path;

/// Icon images are this many pixels square (shown at ~40 px, so a little
/// over 1.5x supersampled).
pub const ICON_SIZE: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub enum IconSource {
    /// Straight (not premultiplied) sRGB RGBA, `ICON_SIZE` square.
    Image(Vec<u8>),
    /// No picture: a tile of this colour with these letters.
    Initials { text: String, color: [u8; 3] },
}

/// Face shades: top, left, right, as in the M2 hotbar.
const SHADES: [f32; 3] = [1.0, 0.80, 0.62];

pub fn icon_for(def: &ItemDef, blocks: &BlockRegistry, set: &TextureSet, items_dir: &Path) -> IconSource {
    if let ItemKind::Block(b) = def.kind
        && blocks.contains(b)
    {
        let faces = set.faces.get(b.0 as usize).copied().unwrap_or([NO_TEXTURE; 4]);
        return IconSource::Image(block_icon(blocks.get(b), faces, &set.layers));
    }
    let file = def.icon.as_deref().unwrap_or(&def.name);
    match load_item_png(&items_dir.join(format!("{file}.png"))) {
        Some(px) => IconSource::Image(px),
        None => IconSource::Initials {
            text: initials(&def.display),
            color: fallback_colour(&def.name),
        },
    }
}

fn load_item_png(path: &Path) -> Option<Vec<u8>> {
    if !path.exists() {
        return None;
    }
    match image::open(path) {
        Ok(img) => {
            let img = img.to_rgba8();
            let n = ICON_SIZE as u32;
            let img = if img.dimensions() == (n, n) {
                img
            } else {
                // Nearest keeps hand-painted pixel art crisp.
                image::imageops::resize(&img, n, n, image::imageops::FilterType::Nearest)
            };
            Some(img.into_raw())
        }
        Err(e) => {
            tracing::warn!("item icon {}: {e}", path.display());
            None
        }
    }
}

/// One face as a parallelogram `a + u (b - a) + v (d - a)`, u and v in 0..1,
/// which is also its texture coordinate.
struct Face {
    a: [f32; 2],
    b: [f32; 2],
    d: [f32; 2],
}

impl Face {
    fn uv(&self, p: [f32; 2]) -> Option<(f32, f32)> {
        let e1 = [self.b[0] - self.a[0], self.b[1] - self.a[1]];
        let e2 = [self.d[0] - self.a[0], self.d[1] - self.a[1]];
        let r = [p[0] - self.a[0], p[1] - self.a[1]];
        let det = e1[0] * e2[1] - e1[1] * e2[0];
        let u = (r[0] * e2[1] - r[1] * e2[0]) / det;
        let v = (e1[0] * r[1] - e1[1] * r[0]) / det;
        ((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)).then_some((u, v))
    }
}

/// The cube: top, left and right faces, filling the square with a small
/// margin.
fn faces() -> [Face; 3] {
    let n = ICON_SIZE as f32;
    let (cx, cy, size) = (n / 2.0, n / 2.0, n * 0.94);
    let w = size * 0.44;
    let top = [cx, cy - size * 0.5];
    let right = [cx + w, cy - size * 0.25];
    let mid = [cx, cy];
    let left = [cx - w, cy - size * 0.25];
    let left_b = [cx - w, cy + size * 0.25];
    let bottom = [cx, cy + size * 0.5];
    [
        Face {
            a: top,
            b: right,
            d: left,
        },
        Face {
            a: left,
            b: mid,
            d: left_b,
        },
        Face {
            a: mid,
            b: right,
            d: bottom,
        },
    ]
}

/// Rasterises a block's isometric icon. `faces` is the block's entry in the
/// texture face table (`first_layer | variants << 16`, ADR on textures).
pub fn block_icon(def: &BlockDef, faces_entry: [u32; 4], layers: &[textures::Layer]) -> Vec<u8> {
    let n = ICON_SIZE;
    let geometry = faces();
    // Top, side, side: the two visible sides share the side texture.
    let entry = [faces_entry[0], faces_entry[1], faces_entry[1]];
    let colours = [def.top_color.unwrap_or(def.color), def.color, def.color];
    let layer = |e: u32| (e != NO_TEXTURE).then(|| layers.get((e & 0xffff) as usize)).flatten();
    let mut out = vec![0u8; n * n * 4];
    const SS: usize = 3;
    for y in 0..n {
        for x in 0..n {
            let mut acc = [0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let p = [
                        x as f32 + (sx as f32 + 0.5) / SS as f32,
                        y as f32 + (sy as f32 + 0.5) / SS as f32,
                    ];
                    let Some((f, (u, v))) = geometry.iter().enumerate().find_map(|(i, g)| g.uv(p).map(|uv| (i, uv)))
                    else {
                        continue;
                    };
                    let (rgb, alpha) = match layer(entry[f]) {
                        Some(px) => {
                            let s = textures::SIZE as usize;
                            let tx = ((u * s as f32) as usize).min(s - 1);
                            let ty = ((v * s as f32) as usize).min(s - 1);
                            let i = (ty * s + tx) * 4;
                            ([px[i], px[i + 1], px[i + 2]], px[i + 3] as f32 / 255.0)
                        }
                        None => (colours[f], 1.0),
                    };
                    // Shade in linear light, weighted by coverage and alpha.
                    for c in 0..3 {
                        acc[c] += textures::srgb_to_linear(rgb[c]) * SHADES[f] * alpha;
                    }
                    acc[3] += alpha;
                }
            }
            let o = (y * n + x) * 4;
            if acc[3] > 0.0 {
                for c in 0..3 {
                    out[o + c] = textures::linear_to_srgb(acc[c] / acc[3]);
                }
                out[o + 3] = (acc[3] / (SS * SS) as f32 * 255.0).round() as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::ItemRegistry;

    fn px(img: &[u8], x: usize, y: usize) -> [u8; 4] {
        let i = (y * ICON_SIZE + x) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    fn empty_set(blocks: &BlockRegistry) -> TextureSet {
        TextureSet {
            layers: vec![],
            faces: vec![[NO_TEXTURE; 4]; blocks.len()],
            crack_base: None,
            missing: vec![],
        }
    }

    #[test]
    fn block_icons_are_cubes_lit_from_the_top_left() {
        let blocks = BlockRegistry::builtin();
        let stone = blocks.get(blocks.id("stone").unwrap());
        let img = block_icon(stone, [NO_TEXTURE; 4], &[]);
        assert_eq!(img.len(), ICON_SIZE * ICON_SIZE * 4);
        let c = ICON_SIZE / 2;
        assert_eq!(px(&img, 0, 0)[3], 0, "corners are transparent");
        assert_eq!(px(&img, ICON_SIZE - 1, ICON_SIZE - 1)[3], 0);
        let top = px(&img, c, c / 2);
        let left = px(&img, c / 2, c + c / 3);
        let right = px(&img, c + c / 2, c + c / 3);
        assert_eq!([top[3], left[3], right[3]], [255; 3]);
        assert_eq!(&top[..3], &stone.color, "the lit top is the block's own colour");
        assert!(top[0] > left[0] && left[0] > right[0], "{top:?} {left:?} {right:?}");
    }

    #[test]
    fn textured_faces_sample_their_layer() {
        let blocks = BlockRegistry::builtin();
        let stone = blocks.get(blocks.id("stone").unwrap());
        let red = vec![[200u8, 10, 10, 255]; (textures::SIZE * textures::SIZE) as usize].concat();
        // Variants packed in the upper bits must not change the layer read.
        let img = block_icon(
            stone,
            [3 | 3 << 16, 3 | 3 << 16, 3 | 3 << 16, 0],
            &[vec![], vec![], vec![], red],
        );
        let c = ICON_SIZE / 2;
        assert_eq!(px(&img, c, c / 2), [200, 10, 10, 255]);
    }

    #[test]
    fn items_without_pictures_fall_back_to_initials() {
        let blocks = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let set = empty_set(&blocks);
        let dir = std::env::temp_dir().join(format!("bm-icons-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let def = |n: &str| items.get(items.id(n).unwrap()).unwrap();
        match icon_for(def("copper_ore"), &blocks, &set, &dir) {
            IconSource::Initials { text, .. } => assert_eq!(text, "CO"),
            other => panic!("{other:?}"),
        }
        // Block items never need a picture file.
        assert!(matches!(
            icon_for(def("stone"), &blocks, &set, &dir),
            IconSource::Image(_)
        ));
        // A dropped-in picture of any size is used, scaled to the icon size.
        image::RgbaImage::from_pixel(16, 16, image::Rgba([1, 2, 3, 255]))
            .save(dir.join("coal.png"))
            .unwrap();
        match icon_for(def("coal"), &blocks, &set, &dir) {
            IconSource::Image(px) => assert_eq!(&px[..4], &[1, 2, 3, 255]),
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
