//! The review contact sheet: every texture at 4x, a 3 x 3 tiling at 2x to
//! show seams (mixing variants where a texture has them), and the face
//! shrunk to 8 x 8 and 4 x 4 the way mipmaps will see it from a distance.

use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::tex::{SIZE, Tex};
use crate::{Family, find};

const BIG: usize = 4;
const TILE: usize = 2;
const GAP: usize = 8;
const LABEL: usize = 18;
const PAD: usize = 14;
const CELL_W: usize = SIZE * BIG + GAP + SIZE * 3 * TILE + PAD;
const CELL_H: usize = LABEL + SIZE * 3 * TILE + PAD;
const COLUMNS: usize = 5;

/// One entry on the sheet.
struct Entry {
    label: String,
    tex: Tex,
    /// The nine faces of the tiled panel, row by row.
    tiles: Vec<Tex>,
    /// Whether the tiled panel sits on a checkerboard (translucent faces).
    checker: bool,
}

/// Box-filters a face down to `n` x `n` (in roughly linear light).
pub fn shrink(tex: &Tex, n: usize) -> Tex {
    let k = SIZE / n;
    let mut out = Tex::default();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (bx, by) = (x / k * k, y / k * k);
            let mut acc = [0.0f32; 4];
            for j in 0..k {
                for i in 0..k {
                    let p = tex.get(bx + i, by + j);
                    acc[0] += p.r.powf(2.2) * p.a;
                    acc[1] += p.g.powf(2.2) * p.a;
                    acc[2] += p.b.powf(2.2) * p.a;
                    acc[3] += p.a;
                }
            }
            let a = acc[3].max(1e-6);
            let m = (k * k) as f32;
            out.set(
                x,
                y,
                Rgba::new(
                    (acc[0] / a).powf(1.0 / 2.2),
                    (acc[1] / a).powf(1.0 / 2.2),
                    (acc[2] / a).powf(1.0 / 2.2),
                    acc[3] / m,
                ),
            );
        }
    }
    out
}

/// Which face of a family goes in each of the nine tiled slots, so a
/// variant's panel shows it next to the base and its siblings.
fn mix_pattern(fam: &Family, i: usize) -> Vec<Tex> {
    let n = fam.faces.len();
    if i == 0 || n == 1 {
        return vec![fam.faces[i].clone(); 9];
    }
    (0..9)
        .map(|s| {
            let pick = if s == 4 || s % 2 == 0 { i } else { (i + s) % n };
            fam.faces[pick].clone()
        })
        .collect()
}

/// Composites `top` over `base`.
fn over(base: &Tex, top: &Tex) -> Tex {
    Tex {
        px: base.px.iter().zip(&top.px).map(|(b, c)| b.over(*c)).collect(),
        n: base.n,
    }
}

/// What an icon is judged against: three flat slot colours, then the
/// blocks it will most often be held in front of.
const ICON_GROUNDS: [&str; 6] = ["stone", "timber", "turf_top", "sand", "snow", "deep_stone"];
const SLOT_COLOURS: [u32; 3] = [0x1c1d26, 0x4a4e60, 0xc8ccd8];

fn icon_tiles(families: &[Family], icon: &Tex) -> Vec<Tex> {
    let mut grounds: Vec<Tex> = SLOT_COLOURS.iter().map(|&c| Tex::filled(Rgba::hex(c))).collect();
    grounds.extend(ICON_GROUNDS.iter().map(|n| find(families, n).base().clone()));
    grounds.iter().map(|g| over(g, icon)).collect()
}

pub fn contact_sheet(families: &[Family], cracks: &[Tex], icons: &[(String, Tex)]) -> Canvas {
    let mut entries = Vec::new();
    for fam in families {
        let translucent = fam.base().px.iter().any(|p| p.a < 1.0);
        for i in 0..fam.faces.len() {
            entries.push(Entry {
                label: fam.file_stem(i),
                tex: fam.faces[i].clone(),
                tiles: mix_pattern(fam, i),
                checker: translucent,
            });
        }
    }
    // Cracks over a spread of hosts, light to dark.
    let hosts = [
        "snow",
        "sand",
        "turf_top",
        "timber",
        "stone",
        "brick",
        "deep_stone",
        "coal_seam",
        "bedrock",
    ];
    for (i, crack) in cracks.iter().enumerate() {
        let tiles = hosts.iter().map(|h| over(find(families, h).base(), crack)).collect();
        entries.push(Entry {
            label: format!("crack_{i}"),
            tex: crack.clone(),
            tiles,
            checker: true,
        });
    }
    // Item icons, each over slot colours and common blocks.
    for (name, icon) in icons {
        entries.push(Entry {
            label: format!("item {name}"),
            tex: icon.clone(),
            tiles: icon_tiles(families, icon),
            checker: true,
        });
    }

    let rows = entries.len().div_ceil(COLUMNS);
    let mut sheet = Canvas::new(PAD + COLUMNS * CELL_W, PAD + rows * CELL_H, Rgba::hex(0x1c1d26));
    for (n, e) in entries.iter().enumerate() {
        let x0 = (PAD + (n % COLUMNS) * CELL_W) as i64;
        let y0 = (PAD + (n / COLUMNS) * CELL_H) as i64;
        sheet.text(&e.label, x0, y0, 2, Rgba::hex(0xe8e4d8));
        let y = y0 + LABEL as i64;
        if e.checker {
            sheet.checker(x0, y, SIZE * BIG, SIZE * BIG, 8);
        }
        sheet.blit(&e.tex, x0, y, BIG);
        // Shrunk to 8 x 8 and 4 x 4 (each drawn 32 px wide).
        let small_y = y + (SIZE * BIG) as i64 + 16;
        for (j, n) in [8usize, 4].into_iter().enumerate() {
            let sx = x0 + j as i64 * 48;
            if e.checker {
                sheet.checker(sx, small_y, SIZE, SIZE, 8);
            }
            sheet.blit(&shrink(&e.tex, n), sx, small_y, 1);
        }
        let tx = x0 + (SIZE * BIG + GAP) as i64;
        if e.checker {
            sheet.checker(tx, y, SIZE * 3 * TILE, SIZE * 3 * TILE, 12);
        }
        for (s, t) in e.tiles.iter().enumerate() {
            let (cx, cy) = ((s % 3) * SIZE * TILE, (s / 3) * SIZE * TILE);
            sheet.blit(t, tx + cx as i64, y + cy as i64, TILE);
        }
    }
    sheet
}

const ICON_BG: u32 = 0x343746;
const ICON_CELL_W: usize = SIZE * BIG + GAP + SIZE * 2 + GAP + 16 * 4 + PAD;
const ICON_CELL_H: usize = LABEL + SIZE * BIG + PAD;
const ICON_COLUMNS: usize = 3;

/// Every item icon at 4x on a mid-dark ground, and beside it the icon at its
/// real size, at 16 x 16, and that 16 x 16 enlarged so its pixels can be
/// judged, with a row of each icon over common blocks.
pub fn icon_sheet(families: &[Family], icons: &[(String, Tex)]) -> Canvas {
    let rows = icons.len().div_ceil(ICON_COLUMNS);
    let mut sheet = Canvas::new(
        PAD + ICON_COLUMNS * ICON_CELL_W,
        PAD + rows * ICON_CELL_H,
        Rgba::hex(ICON_BG),
    );
    for (n, (name, icon)) in icons.iter().enumerate() {
        let x0 = (PAD + (n % ICON_COLUMNS) * ICON_CELL_W) as i64;
        let y0 = (PAD + (n / ICON_COLUMNS) * ICON_CELL_H) as i64;
        sheet.text(name, x0, y0, 2, Rgba::hex(0xe8e4d8));
        let y = y0 + LABEL as i64;
        sheet.blit(icon, x0, y, BIG);
        // Real size, then 16 x 16 at real size.
        let sx = x0 + (SIZE * BIG + GAP) as i64;
        sheet.blit(icon, sx, y, 1);
        let half = shrink(icon, 16);
        for ty in 0..16 {
            for tx in 0..16 {
                sheet.blend(sx + tx as i64, y + 40 + ty as i64, half.get(tx * 2, ty * 2));
            }
        }
        // Over three common blocks, at real size.
        for (i, t) in icon_tiles(families, icon).iter().skip(3).take(3).enumerate() {
            sheet.blit(t, sx + (i as i64 % 2) * 32, y + 64 + (i as i64 / 2) * 32, 1);
        }
        // The 16 x 16 version enlarged 4x.
        let bx = sx + (SIZE * 2 + GAP) as i64;
        sheet.blit(&half, bx, y, 2);
    }
    sheet
}
