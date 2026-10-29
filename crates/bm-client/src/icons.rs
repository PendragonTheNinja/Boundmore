//! Item icons, made on the CPU once per catalogue (and once per colour a
//! coloured stack is seen in).
//!
//! A block item is drawn as a small isometric cube from its own face
//! textures (the look the M2 hotbar had), lit from the top left as the art
//! direction asks. A prop (a torch) and any other item uses `assets/textures/items/<icon>.png` if
//! there is one, and otherwise a coloured tile with its initials, so a
//! missing picture is obvious but never blank.
//!
//! Plain RGBA out, no GPU and no egui: the menus upload the images, and the
//! rasteriser stays testable.

use crate::items::{fallback_colour, initials};
use crate::textures::{self, NO_TEXTURE, TextureSet};
use bm_core::{BlockDef, BlockRegistry, ItemDef, ItemKind, NO_TINT};
use std::path::Path;

/// Icon images are this many pixels square: the size bm-texgen paints
/// item icons at, and the size a slot shows them 1:1 at when the screen's
/// scale allows (`menus::icon_pixels`).
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

/// `tints` is the planet's block tints (`PlanetInfo::tints`), so a block
/// item looks in the hand as it does in the ground here. `colour` (linear)
/// repaints a block item as the terrain shader does (`colour::recolour`);
/// a coloured block wears its colour, not the planet's tint.
pub fn icon_for(
    def: &ItemDef,
    blocks: &BlockRegistry,
    set: &TextureSet,
    items_dir: &Path,
    tints: &[[u8; 3]],
    colour: Option<[f32; 3]>,
) -> IconSource {
    if let ItemKind::Block(b) = def.kind
        && blocks.contains(b)
        && !blocks.get(b).is_prop()
    {
        let faces = set.faces.get(b.0 as usize).copied().unwrap_or([NO_TEXTURE; 4]);
        let tint = tints.get(b.0 as usize).copied().unwrap_or(NO_TINT);
        return IconSource::Image(block_icon(blocks.get(b), faces, &set.layers, tint, colour));
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

pub(crate) fn load_item_png(path: &Path) -> Option<Vec<u8>> {
    if !path.exists() {
        return None;
    }
    match image::open(path) {
        Ok(img) => Some(fit_icon(img.to_rgba8())),
        Err(e) => {
            tracing::warn!("item icon {}: {e}", path.display());
            None
        }
    }
}

/// Brings a picture of any size to `ICON_SIZE` square. The painted icons
/// are made at that size and pass through untouched. A smaller picture
/// that divides it (16 or 32 px pixel art) is enlarged by whole pixels so
/// it stays crisp; any other size is filtered smoothly, with alpha
/// premultiplied so edges do not pick up a dark fringe.
pub fn fit_icon(img: image::RgbaImage) -> Vec<u8> {
    use image::imageops::{FilterType, resize};
    let n = ICON_SIZE as u32;
    let (w, h) = img.dimensions();
    if (w, h) == (n, n) {
        return img.into_raw();
    }
    if w == h && w < n && n % w == 0 {
        return resize(&img, n, n, FilterType::Nearest).into_raw();
    }
    let mut pre = img;
    for p in pre.pixels_mut() {
        let a = p.0[3] as u32;
        for c in &mut p.0[..3] {
            *c = ((*c as u32 * a + 127) / 255) as u8;
        }
    }
    // Triangle is a box-like area filter when shrinking and plain linear
    // when enlarging.
    let mut out = resize(&pre, n, n, FilterType::Triangle);
    for p in out.pixels_mut() {
        let a = p.0[3] as u32;
        for c in &mut p.0[..3] {
            if let Some(v) = (*c as u32 * 255 + a / 2).checked_div(a) {
                *c = v.min(255) as u8;
            }
        }
    }
    out.into_raw()
}

/// Halves an `ICON_SIZE` icon (straight RGBA) with a 2 x 2 box filter in
/// premultiplied alpha: the copy small slots draw, since the menus' textures
/// have no mipmaps.
pub fn half_icon(rgba: &[u8]) -> Vec<u8> {
    let n = ICON_SIZE;
    let h = n / 2;
    let mut out = Vec::with_capacity(h * h * 4);
    for y in 0..h {
        for x in 0..h {
            let mut acc = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let i = ((y * 2 + dy) * n + x * 2 + dx) * 4;
                let a = rgba[i + 3] as u32;
                for c in 0..3 {
                    acc[c] += rgba[i + c] as u32 * a;
                }
                acc[3] += a;
            }
            for c in 0..3 {
                let v = (acc[c] + acc[3] / 2).checked_div(acc[3]).unwrap_or(0);
                out.push(v.min(255) as u8);
            }
            out.push(((acc[3] + 2) / 4) as u8);
        }
    }
    out
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

/// The cube: top, left and right faces, in the middle of the square with a
/// margin as wide as the painted icons leave, so a cube in a slot is no
/// bigger than a tool and leaves the slot's number clear.
fn faces() -> [Face; 3] {
    let n = ICON_SIZE as f32;
    let (cx, cy, size) = (n / 2.0, n / 2.0, n * 0.86);
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
/// texture face table (`first_layer | variants << 16`, ADR on textures);
/// `tint` multiplies its colour (sRGB, white for none); `colour` (linear, a
/// palette colour) repaints it instead, keeping its light and shade.
pub fn block_icon(
    def: &BlockDef,
    faces_entry: [u32; 4],
    layers: &[textures::Layer],
    tint: [u8; 3],
    colour: Option<[f32; 3]>,
) -> Vec<u8> {
    let n = ICON_SIZE;
    let tint = tint.map(textures::srgb_to_linear);
    let geometry = faces();
    // Top, side, side: the two visible sides share the side texture.
    let entry = [faces_entry[0], faces_entry[1], faces_entry[1]];
    let colours = [def.top_color.unwrap_or(def.color), def.color, def.color];
    let layer = |e: u32| (e != NO_TEXTURE).then(|| layers.get((e & 0xffff) as usize)).flatten();
    // Each face's average brightness, as the shader's palette carries it.
    let base: [f32; 3] = std::array::from_fn(|f| {
        crate::colour::face_luma(entry[f], layers)
            .unwrap_or_else(|| crate::colour::luma(crate::colour::srgb_to_linear(colours[f])))
    });
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
                            let s = textures::layer_size(px);
                            let tx = ((u * s as f32) as usize).min(s - 1);
                            let ty = ((v * s as f32) as usize).min(s - 1);
                            let i = (ty * s + tx) * 4;
                            ([px[i], px[i + 1], px[i + 2]], px[i + 3] as f32 / 255.0)
                        }
                        None => (colours[f], 1.0),
                    };
                    let lin = crate::colour::srgb_to_linear(rgb);
                    let lit = match colour {
                        Some(c) => crate::colour::recolour(lin, base[f], c),
                        None => std::array::from_fn(|c| lin[c] * tint[c]),
                    };
                    // Shade in linear light, weighted by coverage and alpha.
                    for c in 0..3 {
                        acc[c] += lit[c] * SHADES[f] * alpha;
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
        let img = block_icon(stone, [NO_TEXTURE; 4], &[], NO_TINT, None);
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
            NO_TINT,
            None,
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
        match icon_for(def("copper_ore"), &blocks, &set, &dir, &[], None) {
            IconSource::Initials { text, .. } => assert_eq!(text, "CO"),
            other => panic!("{other:?}"),
        }
        // Block items never need a picture file.
        assert!(matches!(
            icon_for(def("stone"), &blocks, &set, &dir, &[], None),
            IconSource::Image(_)
        ));
        // A dropped-in picture of any size is used, scaled to the icon size.
        image::RgbaImage::from_pixel(16, 16, image::Rgba([1, 2, 3, 255]))
            .save(dir.join("coal.png"))
            .unwrap();
        match icon_for(def("coal"), &blocks, &set, &dir, &[], None) {
            IconSource::Image(px) => assert_eq!(&px[..4], &[1, 2, 3, 255]),
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coloured_icons_keep_detail_ignore_the_tint_and_repeat_exactly() {
        let blocks = BlockRegistry::builtin();
        let stone = blocks.get(blocks.id("stone").unwrap());
        // A two-tone texture: the recoloured icon keeps both tones.
        let s = textures::SIZE as usize;
        let mut layer = Vec::with_capacity(s * s * 4);
        for i in 0..s * s {
            let v = if (i / s / 8) % 2 == 0 { 90 } else { 170 };
            layer.extend_from_slice(&[v, v, v, 255]);
        }
        let entry = [0, 0, 0, 0];
        let azure = Some(crate::colour::srgb_to_linear([40, 110, 210]));
        let a = block_icon(stone, entry, std::slice::from_ref(&layer), NO_TINT, azure);
        let b = block_icon(stone, entry, std::slice::from_ref(&layer), [255, 100, 100], azure);
        assert_eq!(a, b, "the planet's tint does not touch a coloured block");
        assert_eq!(
            a,
            block_icon(stone, entry, std::slice::from_ref(&layer), NO_TINT, azure),
            "deterministic"
        );
        let c = ICON_SIZE / 2;
        let lit: Vec<[u8; 4]> = (c / 4..c * 3 / 4).map(|y| px(&a, c, y)).collect();
        assert!(lit.iter().all(|p| p[2] > p[0] && p[2] > p[1]), "blue: {lit:?}");
        let (lo, hi) = (
            lit.iter().map(|p| p[2]).min().unwrap(),
            lit.iter().map(|p| p[2]).max().unwrap(),
        );
        assert!(hi - lo > 25, "light and shade survive: {lo} to {hi}");
        let plain = block_icon(stone, entry, std::slice::from_ref(&layer), NO_TINT, None);
        assert_ne!(a, plain);
    }

    #[test]
    fn dropped_in_pictures_of_any_size_fit_cleanly() {
        let n = ICON_SIZE as u32;
        // The painted size passes through untouched.
        let own = image::RgbaImage::from_fn(n, n, |x, y| image::Rgba([x as u8, y as u8, 7, 255]));
        assert_eq!(fit_icon(own.clone()), own.into_raw());
        // Pixel art that divides the size is enlarged by whole pixels.
        let art = image::RgbaImage::from_fn(32, 32, |x, _| {
            image::Rgba(if x % 2 == 0 { [255, 0, 0, 255] } else { [0, 0, 255, 255] })
        });
        let img = fit_icon(art);
        assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
        assert_eq!(px(&img, 1, 5), [255, 0, 0, 255]);
        assert_eq!(px(&img, 2, 5), [0, 0, 255, 255]);
        // A large picture is shrunk smoothly, and a red dot on clear keeps
        // its red at its soft edge rather than darkening.
        let big = image::RgbaImage::from_fn(256, 256, |x, y| {
            let d = (x as f32 - 128.0).hypot(y as f32 - 128.0);
            image::Rgba(if d < 80.0 { [220, 40, 30, 255] } else { [0, 0, 0, 0] })
        });
        let img = fit_icon(big);
        assert_eq!(img.len(), ICON_SIZE * ICON_SIZE * 4);
        let c = ICON_SIZE / 2;
        assert_eq!(px(&img, c, c), [220, 40, 30, 255]);
        let edge = (0..ICON_SIZE)
            .map(|x| px(&img, x, c))
            .find(|p| p[3] > 0 && p[3] < 255)
            .expect("a soft edge");
        assert!(edge[0] > 200 && edge[1] < 60, "no dark fringe: {edge:?}");
        // An odd size is filtered, not dropped.
        let odd = image::RgbaImage::from_pixel(48, 48, image::Rgba([9, 9, 9, 255]));
        assert_eq!(px(&fit_icon(odd), c, c), [9, 9, 9, 255]);
    }

    #[test]
    fn the_half_size_copy_averages_without_a_fringe() {
        let n = ICON_SIZE;
        let mut img = vec![0u8; n * n * 4];
        // Opaque green in the left half of each 2 x 2 block, clear beside it.
        for y in 0..n {
            for x in (0..n).step_by(2) {
                img[(y * n + x) * 4..][..4].copy_from_slice(&[20, 200, 40, 255]);
            }
        }
        let half = half_icon(&img);
        assert_eq!(half.len(), n * n);
        assert_eq!(&half[..4], &[20, 200, 40, 128]);
    }

    #[test]
    fn block_icons_wear_the_planets_tint() {
        let blocks = BlockRegistry::builtin();
        let stone = blocks.get(blocks.id("stone").unwrap());
        let plain = block_icon(stone, [NO_TEXTURE; 4], &[], NO_TINT, None);
        let red = block_icon(stone, [NO_TEXTURE; 4], &[], [255, 128, 128], None);
        let c = ICON_SIZE / 2;
        let (a, b) = (px(&plain, c, c / 2), px(&red, c, c / 2));
        assert_eq!(a[0], b[0], "full red is kept");
        assert!(b[1] < a[1] && b[2] < a[2], "{a:?} {b:?}");
    }
}
