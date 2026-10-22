//! Crafting stations and other built blocks: the workbench, the furnace,
//! the claim stone and the warp shrine. Unlike terrain these are made things, so they are drawn
//! as objects (frames, legs, bars, carved runes) rather than grown from noise,
//! with the same palette rules: light from the top left, hue-shifted shade,
//! chunky shapes that survive the mipmaps.
//!
//! Side faces need not tile seamlessly; they are drawn so that two blocks
//! standing side by side read as two objects (legs meet legs, stones meet
//! stones), and the bands that run across a face line up at the edges.

use crate::color::{Ramp, Rgba};
use crate::materials::{end_grain_ramp, plank_ramp};
use crate::noise::{cell, fbm, hash, hash2, seed, unit, value};
use crate::tex::{Ctx, Field, SIZE, Tex, c, smoothstep};

// ---------------------------------------------------------------- palettes

pub(crate) fn iron_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x15141f, 0x26263a, 0x3b3d52, 0x55596e, 0x737a8e, 0x99a0b2, 0xc6ccd8, 0xeef0f4,
    ])
}

/// Furnace masonry: a neutral, slightly purple grey, warmer than the
/// everyday stone so the furnace reads as built from fired, sooty blocks.
pub(crate) fn furnace_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x1c1a26, 0x2b2934, 0x3d3a46, 0x514d58, 0x67626c, 0x7d7882, 0x979198, 0xb8b0b0,
    ])
}

/// Fire, from the dull red at the edge of the glow to the white-hot heart.
pub(crate) fn fire_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x4a1018, 0x7e1c18, 0xb8341a, 0xe4581c, 0xff8a26, 0xffb840, 0xffe27a, 0xfffbd8,
    ])
}

/// The claim stone's dark rock: deep blue-violet, almost black in shadow.
fn slate_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x0e0d18, 0x181626, 0x221f36, 0x2e2a46, 0x3b3658, 0x4a456c, 0x5d5882, 0x77739c,
    ])
}

/// Rune light: pale blue, white at the core.
fn rune_ramp() -> Ramp {
    Ramp::from_hex(&[0x1c2c66, 0x264a9e, 0x3874cc, 0x58a0ec, 0x88c8fa, 0xbde6ff, 0xeefbff])
}

/// The warp shrine's stone: a dark, warm plum-grey basalt, less blue than
/// the claim stone's slate.
fn basalt_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x120e16, 0x1e1822, 0x2b2330, 0x3a3040, 0x4a3f50, 0x5d5064, 0x73647a, 0x8c7c92,
    ])
}

/// Inlaid silver, cool and bright.
fn silver_ramp() -> Ramp {
    Ramp::from_hex(&[0x2c2a40, 0x4a4a64, 0x70728e, 0x9a9eb8, 0xc4c8da, 0xe6e8f2, 0xffffff])
}

/// Warp light: deep violet at the edge to white at the heart.
fn warp_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x240c44, 0x3e1676, 0x5c24aa, 0x7e3ad4, 0xa260ee, 0xc490fa, 0xe4c8ff, 0xfff6ff,
    ])
}

pub(crate) const SOOT: Rgba = Rgba::hex(0x150d14);

// ----------------------------------------------------------------- helpers

/// A pixel mask on the face.
#[derive(Clone)]
pub(crate) struct Mask(Vec<bool>);

impl Mask {
    pub(crate) fn new() -> Self {
        Self(vec![false; SIZE * SIZE])
    }

    pub(crate) fn get(&self, x: i32, y: i32) -> bool {
        (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) && self.0[y as usize * SIZE + x as usize]
    }

    pub(crate) fn set(&mut self, x: usize, y: usize) {
        self.0[y * SIZE + x] = true;
    }
}

/// A bevelled rectangle `[x0, x1) x [y0, y1)`: lit top row and left column,
/// shaded bottom row and right column, a bright top-left corner.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plate(
    tex: &mut Tex,
    mask: &mut Mask,
    ramp: &Ramp,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    tone: f32,
) {
    for y in y0..y1 {
        for x in x0..x1 {
            let mut t = tone;
            if y == y0 || x == x0 {
                t += 0.24;
            }
            if y + 1 == y1 || x + 1 == x1 {
                t -= 0.26;
            }
            if x == x0 && y == y0 {
                t += 0.12;
            }
            tex.set(x, y, ramp.pick(t, x, y));
            mask.set(x, y);
        }
    }
}

/// Darkens the pixels just below and right of `mask` (not wrapping): the
/// shadow a raised object casts with light from the top left.
pub(crate) fn cast_shadow(tex: &mut Tex, mask: &Mask, col: Rgba, k: f32) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (ix, iy) = (x as i32, y as i32);
            if mask.get(ix, iy) {
                continue;
            }
            let near = mask.get(ix - 1, iy - 1) || mask.get(ix - 1, iy) || mask.get(ix, iy - 1);
            if near {
                tex.set(x, y, tex.get(x, y).mix(col, k));
            }
        }
    }
}

/// A carved line from `a` to `b`: a dark groove with a lit lip on its
/// lower-right side (its far wall faces the light).
pub(crate) fn gouge(tex: &mut Tex, a: (f32, f32), b: (f32, f32), dark: Rgba, lip: Rgba) {
    let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) * 2.0).ceil() as usize + 1;
    let mut cut = Mask::new();
    for i in 0..n {
        let t = i as f32 / (n - 1).max(1) as f32;
        let x = (a.0 + (b.0 - a.0) * t).round() as usize;
        let y = (a.1 + (b.1 - a.1) * t).round() as usize;
        if x < SIZE && y < SIZE {
            cut.set(x, y);
        }
    }
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (ix, iy) = (x as i32, y as i32);
            if cut.get(ix, iy) {
                tex.set(x, y, dark);
            } else if cut.get(ix - 1, iy) || cut.get(ix, iy - 1) {
                tex.set(x, y, tex.get(x, y).mix(lip, 0.55));
            }
        }
    }
}

/// Wood grain: long streaks along x (or along y when `vertical`).
pub(crate) fn grain(salt: u32, x: usize, y: usize, vertical: bool) -> f32 {
    let g = if vertical {
        fbm(salt, 8, 2, 2, c(x), c(y))
    } else {
        fbm(salt, 2, 8, 2, c(x), c(y))
    };
    smoothstep(0.35, 0.75, g) - 0.5
}

/// Glyphs for runes, 5 x 5, top row first, bit 4 = left column. Invented
/// for Boundmore: verticals and diagonals, like letters cut with a chisel.
const RUNES: [[u8; 5]; 8] = [
    // A branching stem.
    [0b10101, 0b10101, 0b01110, 0b00100, 0b00100],
    // A ring round a seed.
    [0b01110, 0b10001, 0b10101, 0b10001, 0b01110],
    // A ladder.
    [0b01010, 0b01110, 0b01010, 0b01110, 0b01010],
    // An hourglass.
    [0b11111, 0b01010, 0b00100, 0b01010, 0b11111],
    // Two chevrons.
    [0b10001, 0b01010, 0b10101, 0b01010, 0b00100],
    // A forked bolt.
    [0b00010, 0b00100, 0b01110, 0b00100, 0b01000],
    // A door under a lintel.
    [0b11111, 0b01010, 0b01010, 0b01010, 0b11011],
    // A hook turning back on itself.
    [0b01110, 0b00001, 0b01101, 0b01001, 0b00110],
];

/// Adds glyph `g` with its top-left at (x, y) to `mask`.
fn rune(mask: &mut Mask, g: usize, x: usize, y: usize) {
    for (row, bits) in RUNES[g].iter().enumerate() {
        for col in 0..5 {
            if bits & (0b10000 >> col) != 0 {
                mask.set(x + col, y + row);
            }
        }
    }
}

/// Paints glowing marks: the core in pale rune light, a soft halo of blue
/// on the stone around it (stronger beside the mark than diagonally).
fn glow(tex: &mut Tex, marks: &Mask, ramp: &Ramp, core: f32) {
    let halo = ramp.at(3);
    let base = tex.clone();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (ix, iy) = (x as i32, y as i32);
            if marks.get(ix, iy) {
                // Brighter where the mark is thick.
                let n = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .filter(|(dx, dy)| marks.get(ix + dx, iy + dy))
                    .count();
                tex.set(x, y, ramp.pick(core + n as f32 * 0.05, x, y));
                continue;
            }
            let side = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .filter(|(dx, dy)| marks.get(ix + dx, iy + dy))
                .count();
            let diag = [(1, 1), (-1, 1), (1, -1), (-1, -1)]
                .iter()
                .filter(|(dx, dy)| marks.get(ix + dx, iy + dy))
                .count();
            let k = (side as f32 * 0.2 + diag as f32 * 0.07).min(0.45);
            if k > 0.0 {
                tex.set(x, y, base.get(x, y).mix(halo, k));
            }
        }
    }
}

// --------------------------------------------------------------- workbench

/// Workbench top: three thick planks held between breadboard ends, an iron
/// vise clamping a scrap of wood at the back, a row of peg holes along the
/// front (one with its peg standing in it) and a few knife and chisel marks.
pub fn workbench_top(_ctx: Ctx) -> Tex {
    let salt = seed("workbench_top");
    let wood = plank_ramp();
    let iron = iron_ramp();
    let dark = wood.at(0);
    let mut tex = Tex::from_fn(|x, y| {
        // Breadboard ends, grain running across the planks.
        if !(4..28).contains(&x) {
            let lx = x % 28;
            let mut t = 0.4 + grain(hash2(salt, (x >= 28) as u32 + 11), x, y, true) * 0.32;
            if lx == 0 {
                t += 0.24;
            }
            if lx == 3 {
                t -= 0.22;
            }
            if y == 0 {
                t += 0.16;
            }
            if y == SIZE - 1 {
                t -= 0.26;
            }
            // Two dowels pinning each end.
            if lx == 1 && (y == 5 || y == 26) {
                return end_grain_ramp().at(5);
            }
            if lx == 2 && (y == 6 || y == 27) {
                return end_grain_ramp().at(1);
            }
            return wood.pick(t, x, y);
        }
        let bounds = [0usize, 11, 22, 32];
        let p = (0..3).find(|&i| y < bounds[i + 1]).unwrap();
        let row = y - bounds[p];
        let h = bounds[p + 1] - bounds[p];
        if row == h - 1 {
            return dark;
        }
        let mut t = 0.56 + (unit(hash2(salt, p as u32)) - 0.5) * 0.16;
        t += grain(hash2(salt, p as u32 + 3), x, y, false) * 0.34;
        if row == 0 {
            t += 0.22;
        } else if row == h - 2 {
            t -= 0.12;
        }
        // The planks sit in the shade of the end to their left.
        if x == 4 {
            t -= 0.14;
        }
        if x == 27 {
            t += 0.1;
        }
        wood.pick(t, x, y)
    });
    // A knot in the middle plank.
    for (x, y, t) in [(8, 15, 0.12), (9, 15, 0.2), (8, 16, 0.25), (9, 16, 0.08), (10, 16, 0.6)] {
        tex.set(x, y, wood.pick(t, x, y));
    }
    // Knife and chisel marks.
    let groove = wood.at(1).mix(Rgba::hex(0x2a1420), 0.3);
    let lip = wood.lightest();
    gouge(&mut tex, (6.0, 4.0), (9.0, 7.0), groove, lip);
    gouge(&mut tex, (13.0, 17.0), (17.0, 16.0), groove, lip);
    gouge(&mut tex, (20.0, 17.0), (22.0, 19.0), groove, lip);
    // Peg holes along the front plank, a peg standing in the third.
    for hx in [7usize, 13, 19] {
        let hy = 26;
        let deep = Rgba::hex(0x1c1018);
        tex.set(hx, hy, deep);
        tex.set(hx + 1, hy, deep);
        tex.set(hx, hy + 1, deep);
        tex.set(hx + 1, hy + 1, dark.mix(deep, 0.4));
        for (x, y) in [(hx + 2, hy + 1), (hx + 1, hy + 2), (hx + 2, hy + 2)] {
            tex.set(x, y, tex.get(x, y).mix(lip, 0.4));
        }
    }
    {
        let end = end_grain_ramp();
        let (px, py) = (24usize, 25usize);
        let mut peg = Mask::new();
        for (dx, dy, t) in [
            (1, 0, 0.75),
            (2, 0, 0.6),
            (0, 1, 0.75),
            (1, 1, 0.95),
            (2, 1, 0.55),
            (3, 1, 0.3),
            (0, 2, 0.6),
            (1, 2, 0.55),
            (2, 2, 0.35),
            (3, 2, 0.2),
            (1, 3, 0.3),
            (2, 3, 0.15),
        ] {
            tex.set(px + dx, py + dy, end.pick(t, px + dx, py + dy));
            peg.set(px + dx, py + dy);
        }
        cast_shadow(&mut tex, &peg, Rgba::hex(0x2a1420), 0.5);
    }
    // The vise: a fixed back jaw, a sliding front jaw, a scrap of wood
    // clamped between them, the screw and its tommy bar.
    let mut vise = Mask::new();
    plate(&mut tex, &mut vise, &iron, 15, 1, 28, 4, 0.5);
    plate(&mut tex, &mut vise, &iron, 15, 7, 28, 10, 0.48);
    // The clamped scrap: pale end grain, lit along its top.
    let end = end_grain_ramp();
    for y in 4..7 {
        for x in 17..26 {
            let t = match (x, y) {
                (17, _) | (_, 4) => 0.95,
                (25, _) | (_, 6) => 0.45,
                _ => 0.72,
            };
            tex.set(x, y, end.pick(t, x, y));
            vise.set(x, y);
        }
    }
    plate(&mut tex, &mut vise, &iron, 20, 10, 23, 12, 0.4);
    plate(&mut tex, &mut vise, &iron, 17, 12, 26, 14, 0.58);
    for x in [16usize, 26] {
        plate(&mut tex, &mut vise, &iron, x, 11, x + 1, 15, 0.72);
    }
    cast_shadow(&mut tex, &vise, Rgba::hex(0x24101c), 0.55);
    tex
}

/// Workbench side: the thick top's edge, a square leg at each corner, an
/// apron rail with tools hanging from it (a hammer, a saw, a chisel), a crate
/// on the low shelf, and dark boards at the back in the bench's shade.
pub fn workbench_side(_ctx: Ctx) -> Tex {
    let salt = seed("workbench_side");
    let wood = plank_ramp();
    let iron = iron_ramp();
    let shade = Rgba::hex(0x1e0e18);
    let mut tex = Tex::from_fn(|x, y| {
        // The top's front edge.
        if y < 5 {
            let mut t = 0.55 + grain(salt, x, y, false) * 0.3;
            if y == 0 {
                t += 0.3;
            }
            if y == 4 {
                t -= 0.26;
            }
            return wood.pick(t, x, y);
        }
        // Legs at the corners.
        if !(4..28).contains(&x) {
            let lx = x % 28;
            let mut t = 0.48 + grain(hash2(salt, 1 + (x >= 28) as u32), x, y, true) * 0.3;
            t += [0.22, 0.04, -0.06, -0.24][lx];
            // In the shade of the top for a pixel, darker at the foot.
            if y == 5 {
                t -= 0.24;
            }
            if y == SIZE - 1 {
                t -= 0.16;
            }
            return wood.pick(t, x, y);
        }
        // The apron rail just under the top.
        if y < 9 {
            let mut t = 0.46 + grain(hash2(salt, 2), x, y, false) * 0.25;
            if y == 5 {
                t -= 0.3;
            }
            if y == 6 {
                t += 0.14;
            }
            if y == 8 {
                t -= 0.22;
            }
            return wood.pick(t, x, y);
        }
        // The low shelf.
        if (25..28).contains(&y) {
            let t = 0.5 + grain(hash2(salt, 3), x, y, false) * 0.25 + [0.24, 0.0, -0.24][y - 25];
            return wood.pick(t, x, y);
        }
        // Back boards in deep shade, a little lighter towards the floor.
        let board = (x - 4) / 6;
        let bx = (x - 4) % 6;
        let mut t = 0.08 + (y as f32 - 9.0) / 22.0 * 0.12;
        t += (unit(hash2(salt, 40 + board as u32)) - 0.5) * 0.06;
        t += grain(hash2(salt, 50 + board as u32), x, y, true) * 0.1;
        if bx == 0 {
            return wood.at(0).mix(shade, 0.5);
        }
        if bx == 1 {
            t += 0.06;
        }
        if y >= 28 {
            t -= 0.04;
        }
        wood.pick(t, x, y).mix(shade, 0.3)
    });
    let mut tools = Mask::new();
    // Hammer, hung by its head on the rail.
    plate(&mut tex, &mut tools, &iron, 5, 10, 11, 13, 0.5);
    for y in 13..21 {
        for x in 7..9 {
            let t = if x == 7 { 0.72 } else { 0.48 } - if y == 20 { 0.18 } else { 0.0 };
            tex.set(x, y, wood.pick(t, x, y));
            tools.set(x, y);
        }
    }
    // Saw: a wooden grip with a hole, the blade hanging below it, teeth on
    // its left edge.
    for y in 9..13 {
        for x in 12..18 {
            let hole = (13..16).contains(&x) && y == 10;
            let t = if hole {
                0.0
            } else if y == 9 || x == 12 {
                0.78
            } else if x == 17 || y == 12 {
                0.3
            } else {
                0.55
            };
            tex.set(x, y, wood.pick(t, x, y));
            tools.set(x, y);
        }
    }
    for y in 13..23 {
        let right = 18 - (y - 13) / 4;
        for x in 12..right {
            let t = if x == 12 {
                if y % 2 == 0 { 0.82 } else { 0.32 }
            } else if x + 1 == right {
                0.42
            } else if x == 13 {
                0.86
            } else {
                0.66
            };
            tex.set(x, y, iron.pick(t, x, y));
            tools.set(x, y);
        }
    }
    // Chisel: a round handle, a brass ferrule, the steel and its bevel.
    for y in 10..20 {
        for x in 21..23 {
            let lit = x == 21;
            let (ramp, t) = match y {
                10..=14 => (&wood, if lit { 0.78 } else { 0.5 }),
                15 => (&end_grain_ramp(), if lit { 0.95 } else { 0.7 }),
                16..=18 => (&iron, if lit { 0.82 } else { 0.55 }),
                _ => (&iron, if lit { 0.98 } else { 0.7 }),
            };
            tex.set(x, y, ramp.pick(t, x, y));
            tools.set(x, y);
        }
    }
    // Pegs the tools hang from.
    for x in [8usize, 14, 21] {
        tex.set(x, 9, wood.at(1));
    }
    // A small crate on the shelf.
    for y in 19..25 {
        for x in 19..27 {
            let edge = x == 19 || x == 26 || y == 19 || y == 24;
            let mut t = if edge { 0.42 } else { 0.62 };
            if y == 19 || x == 19 {
                t += 0.24;
            }
            if y == 24 || x == 26 {
                t -= 0.24;
            }
            if !edge && y == 21 {
                t = 0.22;
            }
            tex.set(x, y, wood.pick(t, x, y));
            tools.set(x, y);
        }
    }
    cast_shadow(&mut tex, &tools, shade, 0.6);
    tex
}

// ----------------------------------------------------------------- furnace

/// Furnace top: four dressed slabs laid round a square chimney, the chimney
/// barred with an iron grate and lit from below by the fire.
pub fn furnace_top(_ctx: Ctx) -> Tex {
    furnace_top_in(seed("furnace_top"), &furnace_ramp())
}

/// [`furnace_top`] in another stone (the iron furnace's dark iron).
pub(crate) fn furnace_top_in(salt: u32, stone: &Ramp) -> Tex {
    let fire = fire_ramp();
    let iron = iron_ramp();
    // Pinwheel of four slabs round the hole [10, 22).
    let slabs = [
        (0usize, 0usize, 22usize, 10usize),
        (22, 0, 32, 22),
        (10, 22, 32, 32),
        (0, 10, 10, 32),
    ];
    let mut tex = Tex::from_fn(|x, y| {
        if (10..22).contains(&x) && (10..22).contains(&y) {
            let (lx, ly) = (x - 10, y - 10);
            // Soot round the hole: the top-left inner walls in shadow, the
            // bottom-right walls catching the glow.
            if lx == 0 || ly == 0 {
                return SOOT;
            }
            if lx == 11 || ly == 11 {
                return fire.at(1).mix(SOOT, 0.35);
            }
            // Bars: two each way, two pixels thick.
            let bar_x = matches!(lx, 4 | 5 | 8 | 9);
            let bar_y = matches!(ly, 4 | 5 | 8 | 9);
            if bar_x || bar_y {
                let lit = (bar_x && matches!(lx, 4 | 8)) || (bar_y && !bar_x && matches!(ly, 4 | 8));
                let t = if lit { 0.62 } else { 0.32 };
                return iron.pick(t, x, y);
            }
            // Fire seen between the bars: hottest in the middle.
            let d = (c(x) - 16.0).hypot(c(y) - 16.0);
            let flick = value(salt ^ 7, 8, 8, c(x), c(y));
            let t = 0.92 - d / 7.5 * 0.6 + (flick - 0.5) * 0.2;
            // The hole's own shadow on the top-left of each gap.
            let gap_x = matches!(lx, 1 | 6 | 10);
            let gap_y = matches!(ly, 1 | 6 | 10);
            let t = if gap_x || gap_y { t - 0.18 } else { t };
            return fire.pick(t, x, y);
        }
        let (i, &(x0, y0, x1, y1)) = slabs
            .iter()
            .enumerate()
            .find(|(_, s)| (s.0..s.2).contains(&x) && (s.1..s.3).contains(&y))
            .unwrap();
        // Joints between the slabs.
        if x + 1 == x1 && x1 < SIZE || y + 1 == y1 && y1 < SIZE {
            return stone.at(0);
        }
        let mut t = 0.52 + (unit(hash2(salt, i as u32)) - 0.5) * 0.14;
        t += (fbm(hash2(salt, 9 + i as u32), 8, 8, 2, c(x), c(y)) - 0.5) * 0.2;
        if x == x0 || y == y0 {
            t += 0.22;
        }
        let last_x = if x1 < SIZE { x1 - 2 } else { x1 - 1 };
        let last_y = if y1 < SIZE { y1 - 2 } else { y1 - 1 };
        if x == last_x || y == last_y {
            t -= 0.2;
        }
        // Soot and warmth near the chimney.
        let d = (c(x) - 16.0).abs().max((c(y) - 16.0).abs());
        let near = (1.0 - (d - 6.0) / 3.5).clamp(0.0, 1.0);
        let col = stone.pick(t - near * 0.12, x, y);
        col.mix(fire.at(2), near * 0.18)
    });
    // Chips and cracks from years of heat.
    let (dark, lip) = (stone.at(1), stone.at(6));
    gouge(&mut tex, (4.0, 3.0), (6.0, 4.0), dark, lip);
    gouge(&mut tex, (26.0, 6.0), (27.0, 9.0), dark, lip);
    gouge(&mut tex, (3.0, 20.0), (5.0, 23.0), dark, lip);
    gouge(&mut tex, (23.0, 26.0), (26.0, 27.0), dark, lip);
    gouge(&mut tex, (15.0, 4.0), (16.0, 6.0), dark, lip);
    tex
}

/// Coursed masonry for the furnace: four courses of six pixels between the
/// cap (rows 0..4) and the sill (rows 28..32), blocks of 6 to 10 pixels with
/// staggered joints, each block rounded (lit top-left rim, shaded bottom
/// right, softened corners) and set in dark mortar. Wraps horizontally.
pub(crate) fn masonry(salt: u32, stone: &Ramp, x: usize, y: usize) -> Rgba {
    let row = (y - 4) / 6;
    let ly = (y - 4) % 6;
    let rsalt = hash2(salt, 100 + row as u32);
    let offset = (unit(rsalt) * 8.0) as usize;
    let rel = (x + SIZE - offset) % SIZE;
    // Block boundaries along the row, the last block closing the loop.
    let mut start = 0;
    let mut n = 0u32;
    let (mut b0, mut b1) = (0, SIZE);
    while start < SIZE {
        let mut w = 6 + (unit(hash2(rsalt, n)) * 5.0) as usize;
        if SIZE - start < w + 6 {
            w = SIZE - start;
        }
        if rel < start + w {
            (b0, b1) = (start, start + w);
            break;
        }
        start += w;
        n += 1;
    }
    let (lx, w) = (rel - b0, b1 - b0);
    let h = 6;
    if lx == w - 1 || ly == h - 1 {
        return stone.at(0);
    }
    let id = hash2(rsalt, 50 + b0 as u32);
    let mut t = 0.5 + (unit(id) - 0.5) * 0.2;
    t += (0.5 - lx as f32 / w as f32) * 0.1 + (0.5 - ly as f32 / h as f32) * 0.1;
    t += (fbm(hash(id), 4, 4, 2, c(x), c(y)) - 0.5) * 0.2;
    if ly == 0 {
        t += 0.26;
    }
    if lx == 0 {
        t += 0.18;
    }
    if ly == h - 2 {
        t -= 0.24;
    }
    if lx == w - 2 {
        t -= 0.2;
    }
    let corner = (lx == 0 || lx == w - 2) && (ly == 0 || ly == h - 2);
    let col = stone.pick(t, x, y);
    if corner { col.mix(stone.at(0), 0.55) } else { col }
}

/// Furnace side: rounded stone blocks in rough courses under a dressed cap,
/// an arched mouth at the bottom middle with a fire burning on two logs, its
/// glow spilling onto the stones and the hearth sill.
pub fn furnace_side(_ctx: Ctx) -> Tex {
    let salt = seed("furnace_side");
    let stone = furnace_ramp();
    furnace_side_in(salt, &stone, &|x, y| masonry(salt, &stone, x, y))
}

/// [`furnace_side`] in another stone, its walls painted by `body` (rows 4
/// to 28 round the mouth).
pub(crate) fn furnace_side_in(salt: u32, stone: &Ramp, body: &dyn Fn(usize, usize) -> Rgba) -> Tex {
    let fire = fire_ramp();
    let (ax, ay, ar) = (16.0f32, 21.5f32, 6.5f32);
    let ring = 10.2f32;
    // Where the mouth is open, and where the arch stones are.
    let mouth = |fx: f32, fy: f32| fy < 28.0 && ((fx - ax).abs() < ar && fy >= ay || (fx - ax).hypot(fy - ay) < ar);
    let arch = |fx: f32, fy: f32| {
        !mouth(fx, fy) && fy < 28.0 && ((fx - ax).abs() < ring - 0.5 && fy >= ay || (fx - ax).hypot(fy - ay) < ring)
    };
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        // Dressed cap course along the top.
        if y < 4 {
            let joint = [10usize, 23].contains(&x);
            if joint {
                return stone.at(0);
            }
            let mut t = 0.6 + (fbm(salt ^ 3, 4, 2, 2, fx, fy) - 0.5) * 0.2;
            t += [0.28, 0.06, 0.0, -0.3][y];
            if [0usize, 11, 24].contains(&x) {
                t += 0.16;
            }
            return stone.pick(t, x, y);
        }
        // Hearth sill along the bottom.
        if y >= 28 {
            let joint = [5usize, 26].contains(&x);
            if joint {
                return stone.at(0);
            }
            let t = 0.56 + [0.26, 0.04, 0.0, -0.24][y - 28] + (fbm(salt ^ 4, 4, 2, 2, fx, fy) - 0.5) * 0.16;
            return stone.pick(t, x, y);
        }
        if mouth(fx, fy) {
            return SOOT;
        }
        if arch(fx, fy) {
            // Five voussoirs round the arch, a jamb block either side.
            let (dx, dy) = (fx - ax, fy - ay);
            let r = dx.hypot(dy);
            let (piece, joint) = if fy >= ay {
                (if dx < 0.0 { 10 } else { 11 }, y == 24)
            } else {
                let ang = dy.atan2(dx).to_degrees() + 180.0; // 0..180 over the top
                let off = (ang % 36.0).min(36.0 - ang % 36.0).to_radians() * r;
                ((ang / 36.0) as i32, off < 0.6 && ang > 5.0 && ang < 175.0)
            };
            if joint || r >= ring - 0.6 && fy < ay || fy >= ay && dx.abs() >= ring - 1.1 {
                return stone.at(0);
            }
            let mut t = 0.64 + (unit(hash2(salt, piece as u32)) - 0.5) * 0.12;
            // Lit on the outer top-left, shaded on the inside of the arch.
            let outer = if fy < ay { r > ring - 1.8 } else { dx.abs() > ring - 2.1 };
            if outer {
                t += -(dx + dy) / r.max(1.0) * 0.22;
            }
            let inner = if fy < ay { r < ar + 1.0 } else { dx.abs() < ar + 1.0 };
            if inner {
                t -= 0.24;
            }
            return stone.pick(t, x, y);
        }
        body(x, y)
    });
    // The fire: embers on the hearth, flames rising in three tongues, soot
    // at the crown of the arch.
    let flame = |x: usize| {
        let fx = c(x) - ax;
        3.5 + 6.0 * (1.0 - (fx / 7.5).powi(2)).max(0.0) + 1.6 * (fx * 1.25 + 0.6).cos()
    };
    for y in 14..28 {
        for x in 8..25 {
            let (fx, fy) = (c(x), c(y));
            if !mouth(fx, fy) {
                continue;
            }
            let up = 28.0 - fy;
            let h = flame(x);
            let heat = 1.0 - up / h;
            let flick = (value(salt ^ 9, 8, 4, fx, fy) - 0.5) * 0.24;
            let col = if heat > 0.0 {
                fire.pick(0.38 + heat * 0.66 + flick, x, y)
            } else {
                // The back of the oven, red where the flames light it.
                SOOT.mix(fire.at(1), (0.55 + heat * 0.9).clamp(0.0, 0.55))
            };
            tex.set(x, y, col);
        }
    }
    // Two charred logs across the embers, glowing at their cut ends and
    // along the cracks in their undersides.
    for (x0, x1, y) in [(10usize, 18usize, 25usize), (15, 23, 26)] {
        for x in x0..x1 {
            let end = x == x0;
            tex.set(x, y, if end { fire.at(6) } else { Rgba::hex(0x5a3428) });
            let under = if end {
                fire.at(4)
            } else if (x + y) % 3 == 0 {
                fire.at(5)
            } else {
                Rgba::hex(0x24121a)
            };
            tex.set(x, y + 1, under);
        }
    }
    // Glow on everything near the mouth: the sill most, the jambs next.
    let glow_col = fire.at(4);
    for y in 4..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (c(x), c(y));
            if mouth(fx, fy) {
                continue;
            }
            let d = (fx - ax).hypot((fy - 26.0) * 1.2);
            let mut k = (1.0 - d / 13.0).clamp(0.0, 1.0).powi(2) * 0.5;
            if y >= 28 && (fx - ax).abs() < ar + 1.0 {
                k += 0.18 * (1.0 - (y - 28) as f32 / 4.0);
            }
            if k > 0.0 {
                tex.set(x, y, tex.get(x, y).mix(glow_col, k));
            }
        }
    }
    tex
}

// ------------------------------------------------------------- claim stone

/// The claim stone's dark rock: broad chisel facets, softly mottled.
fn slate(salt: u32) -> Field {
    let sites = Ctx::BASE.sites(salt, 9.0);
    Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&sites, fx, fy, 1.0);
        let s = &sites[k.index];
        let mut t = 0.42 + (s.r[0] - 0.5) * 0.12;
        t += ((s.r[1] - 0.5) * k.dx + (s.r[2] - 0.5) * k.dy) * 0.025;
        t += k.rim(2.0) * 0.12;
        t += (fbm(salt ^ 1, 4, 4, 2, fx, fy) - 0.5) * 0.18;
        if k.edge < 0.6 {
            t -= 0.1;
        }
        t
    })
}

/// Claim stone top: a dressed slab of dark rock with a raised disc carved in
/// it: a glowing ring, four runes round it, a bright star at the heart.
pub fn claim_stone_top(_ctx: Ctx) -> Tex {
    let salt = seed("claim_stone_top");
    let rock = slate_ramp();
    let light = rune_ramp();
    let field = slate(salt);
    let mut tex = Tex::from_fn(|x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
        let r = dx.hypot(dy);
        let facing = -(dx + dy) / r.max(1.0);
        let mut t = field.at(x, y);
        // The block's dressed edge.
        if x == 0 || y == 0 {
            t += 0.24;
        }
        if x == SIZE - 1 || y == SIZE - 1 {
            t -= 0.24;
        }
        if r < 12.6 {
            // The disc: raised, smooth, lit on its top-left rim.
            t = 0.5 + (t - 0.42) * 0.5;
            if r > 11.4 {
                t += facing * 0.26;
            }
        } else if r < 13.8 {
            // The groove round it: dark, its outer wall lit at the bottom right.
            t = 0.08 - facing * 0.1;
        }
        rock.pick(t, x, y)
    });
    let mut marks = Mask::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
            let r = dx.hypot(dy);
            // The ring channel, and the star at the heart.
            if (10.2..11.3).contains(&r) || dx.abs() + dy.abs() < 2.6 {
                marks.set(x, y);
            }
        }
    }
    // Four runes, one at each quarter, and a dot between each pair.
    rune(&mut marks, 0, 14, 7);
    rune(&mut marks, 1, 20, 14);
    rune(&mut marks, 3, 14, 20);
    rune(&mut marks, 2, 7, 14);
    for (x, y) in [(11, 11), (20, 11), (11, 20), (20, 20)] {
        marks.set(x, y);
    }
    glow(&mut tex, &marks, &light, 0.68);
    // The heart of the star burns white.
    for (x, y) in [(15, 15), (16, 15), (15, 16), (16, 16)] {
        tex.set(x, y, light.lightest());
    }
    tex
}

/// Claim stone side: dark standing rock with a carved panel down the middle
/// holding a column of glowing runes, and a glowing band cut round the stone
/// near the top and the bottom (the bands meet across neighbouring stones).
pub fn claim_stone_side(_ctx: Ctx) -> Tex {
    let salt = seed("claim_stone_side");
    let rock = slate_ramp();
    let light = rune_ramp();
    let field = slate(salt);
    let mut tex = Tex::from_fn(|x, y| {
        let mut t = field.at(x, y);
        // The block's corners catch the light on the left.
        if x == 0 {
            t += 0.12;
        }
        if x == SIZE - 1 {
            t -= 0.12;
        }
        // Grooves either side of each band.
        if y == 1 || y == 28 {
            t = 0.06;
        }
        if y == 3 || y == 30 {
            t += 0.16;
        }
        // The recessed panel: dark top-left inner walls, lit bottom-right.
        if (11..21).contains(&x) && (5..27).contains(&y) {
            t = 0.22 + (t - 0.42) * 0.4;
            if x == 11 || y == 5 {
                t = 0.04;
            } else if x == 20 || y == 26 {
                t = 0.5;
            }
        }
        rock.pick(t, x, y)
    });
    let mut marks = Mask::new();
    for x in 0..SIZE {
        marks.set(x, 2);
        marks.set(x, 29);
    }
    rune(&mut marks, 4, 13, 7);
    rune(&mut marks, 1, 13, 14);
    rune(&mut marks, 5, 13, 21);
    glow(&mut tex, &marks, &light, 0.7);
    // The bands pulse brighter in clumps along their length.
    for x in 0..SIZE {
        for y in [2usize, 29] {
            if value(salt ^ 5, 4, 1, c(x), 0.5) > 0.62 {
                tex.set(x, y, light.at(5));
            }
        }
    }
    tex
}

// ------------------------------------------------------------- warp shrine

/// The shrine's basalt: big squared facets, softly mottled, darker and
/// smoother than the claim stone's slate.
fn basalt(salt: u32) -> Field {
    let sites = Ctx::BASE.sites(salt, 11.0);
    Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&sites, fx, fy, 1.0);
        let s = &sites[k.index];
        let mut t = 0.4 + (s.r[0] - 0.5) * 0.1;
        t += k.rim(2.0) * 0.1;
        t += (fbm(salt ^ 1, 4, 4, 2, fx, fy) - 0.5) * 0.14;
        t
    })
}

/// Distance from the middle of the face in an octagon metric.
fn octagon(dx: f32, dy: f32) -> f32 {
    dx.abs()
        .max(dy.abs())
        .max((dx.abs() + dy.abs()) * std::f32::consts::FRAC_1_SQRT_2)
}

/// A swirl of warp light: `arms` spiral arms turning inwards, brighter
/// towards the heart. `r` is the distance from the heart, `edge` the
/// radius at which it fades to the darkest violet.
fn swirl(dx: f32, dy: f32, r: f32, edge: f32, arms: f32, twist: f32) -> f32 {
    let a = dy.atan2(dx) / std::f32::consts::TAU;
    let s = (a * arms + r * twist).rem_euclid(1.0);
    let arm = 1.0 - smoothstep(0.0, 0.42, (s - 0.5).abs() * 2.0 - 0.2);
    0.18 + arm * 0.3 + (1.0 - r / edge).max(0.0) * 0.45
}

/// Warp shrine top: a slab of dark basalt with an octagonal well in it,
/// framed by a band of inlaid silver, a violet vortex of warp light turning
/// down inside it to a white heart, and a silver stud set with a violet gem
/// at each corner of the slab.
pub fn warp_shrine_top(_ctx: Ctx) -> Tex {
    let salt = seed("warp_shrine_top");
    let rock = basalt_ramp();
    let silver = silver_ramp();
    let light = warp_ramp();
    let field = basalt(salt);
    let (well, band) = (9.2f32, 11.4f32);
    let mut tex = Tex::from_fn(|x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
        let o = octagon(dx, dy);
        let facing = -(dx + dy) / dx.hypot(dy).max(1.0);
        if o < well {
            // The well: warp light, its far (bottom-right) lip lit violet
            // and its near wall in shadow.
            let mut t = swirl(dx, dy, o, well, 3.0, 0.11);
            if o > well - 1.2 {
                t -= facing * 0.22;
            }
            return light.pick(t, x, y);
        }
        if o < band {
            // The silver band, rounded: lit on its top-left outer edge.
            let mut t = 0.5;
            if o > band - 0.9 {
                t += facing * 0.3;
            } else if o < well + 0.8 {
                t -= facing * 0.3;
            }
            return silver.pick(t, x, y);
        }
        let mut t = field.at(x, y);
        // A groove round the band, and the slab's dressed edge.
        if o < band + 1.0 {
            t = 0.1 - facing * 0.08;
        }
        if x == 0 || y == 0 {
            t += 0.22;
        }
        if x == SIZE - 1 || y == SIZE - 1 {
            t -= 0.22;
        }
        rock.pick(t, x, y)
    });
    // Faint light thrown on the silver from the well.
    for y in 0..SIZE {
        for x in 0..SIZE {
            let o = octagon(c(x) - 16.0, c(y) - 16.0);
            if (well..well + 1.0).contains(&o) {
                tex.set(x, y, tex.get(x, y).mix(light.at(5), 0.35));
            }
        }
    }
    // The heart burns white, with a short cross of light.
    for (x, y) in [(15, 15), (16, 15), (15, 16), (16, 16)] {
        tex.set(x, y, light.lightest());
    }
    for (x, y) in [(15, 13), (16, 18), (13, 16), (18, 15)] {
        tex.set(x, y, light.at(6));
    }
    // Corner studs: a silver diamond with a glowing violet gem.
    let mut studs = Mask::new();
    for (cx, cy) in [(4usize, 4usize), (27, 4), (4, 27), (27, 27)] {
        for (dx, dy) in [
            (0i32, -2i32),
            (-1, -1),
            (0, -1),
            (1, -1),
            (-2, 0),
            (-1, 0),
            (1, 0),
            (2, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
            (0, 2),
        ] {
            let (x, y) = ((cx as i32 + dx) as usize, (cy as i32 + dy) as usize);
            let t = 0.55 - (dx + dy) as f32 * 0.12;
            tex.set(x, y, silver.pick(t, x, y));
            studs.set(x, y);
        }
        tex.set(cx, cy, light.at(6));
    }
    cast_shadow(&mut tex, &studs, SOOT, 0.45);
    tex
}

/// Warp shrine side: a round-headed gateway in dark basalt, its arch of
/// silver-capped voussoirs round a curtain of violet warp light, a keystone
/// set with a gem, a glowing rune on each pillar, and a cornice and a plinth
/// that run on into the next shrine.
pub fn warp_shrine_side(_ctx: Ctx) -> Tex {
    let salt = seed("warp_shrine_side");
    let rock = basalt_ramp();
    let silver = silver_ramp();
    let light = warp_ramp();
    let field = basalt(salt);
    let (spring, r_in, r_out, sill) = (14.0f32, 7.0f32, 9.4f32, 27.0f32);
    // Inside the opening, inside the arch (opening included).
    let open = |fx: f32, fy: f32| {
        let dx = fx - 16.0;
        fy < sill && ((fy >= spring && dx.abs() < r_in) || dx.hypot(fy - spring) < r_in)
    };
    let arch = |fx: f32, fy: f32| {
        let dx = fx - 16.0;
        fy < sill && ((fy >= spring && dx.abs() < r_out) || dx.hypot(fy - spring) < r_out)
    };
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let dx = fx - 16.0;
        if open(fx, fy) {
            // A curtain of light: brightest down the middle, shimmering in
            // vertical streaks, with a swirl at the head of the arch.
            let mut t = 0.3 + (1.0 - dx.abs() / r_in) * 0.32;
            t += (smoothstep(0.35, 0.75, value(salt ^ 2, 8, 2, fx, fy)) - 0.5) * 0.24;
            let r = dx.hypot(fy - (spring + 1.0));
            if r < 5.0 {
                t = t.max(swirl(dx, fy - (spring + 1.0), r, 5.5, 2.0, 0.2) + 0.1);
            }
            // The reveal: the near (top-left) inner wall throws a shadow.
            if !open(fx - 1.0, fy) || !open(fx, fy - 1.0) {
                t -= 0.2;
            }
            return light.pick(t, x, y);
        }
        if arch(fx, fy) {
            // Voussoirs: silver-capped stones radiating round the opening,
            // their joints every few pixels.
            let (rx, ry) = (dx, (fy - spring).min(0.0));
            let r = rx.hypot(ry);
            let ang = ry.atan2(rx);
            let mut t = 0.52;
            let joint = if fy < spring {
                ((ang / std::f32::consts::PI * 7.0).rem_euclid(1.0) - 0.5).abs() > 0.42
            } else {
                (fy as usize - spring as usize) % 5 == 4
            };
            let (nx, ny) = (rx / r.max(0.5), ry / r.max(0.5));
            let facing = -(nx + ny) * std::f32::consts::FRAC_1_SQRT_2;
            if r > r_out - 1.0 || dx.abs() > r_out - 1.0 && fy >= spring {
                t += facing * 0.3;
            }
            if joint {
                return rock.pick(0.1, x, y);
            }
            return silver.pick(t, x, y);
        }
        let mut t = field.at(x, y);
        // Cornice: rows 0..4, lit lip, dark undercut.
        if y < 4 {
            t += 0.08;
            if y == 0 {
                t += 0.22;
            }
            if y == 3 {
                t = 0.06;
            }
            return rock.pick(t, x, y);
        }
        // Plinth: rows 27..32, lit on its top step.
        if fy > sill {
            t += 0.04;
            if y == 27 {
                t += 0.24;
            }
            if y == SIZE - 1 {
                t -= 0.2;
            }
            return rock.pick(t, x, y);
        }
        // Coursed blocks on the pillars, joints staggered course to course.
        let course = (y - 4) / 6;
        let row = (y - 4) % 6;
        let jx = if course % 2 == 0 { 0 } else { 3 };
        if row == 5 {
            return rock.pick(0.08, x, y);
        }
        if x == jx || x == SIZE - 1 - jx {
            return rock.pick(0.08, x, y);
        }
        if row == 0 {
            t += 0.16;
        }
        if row == 4 {
            t -= 0.08;
        }
        if x == jx + 1 || x == SIZE - jx {
            t += 0.12;
        }
        rock.pick(t, x, y)
    });
    // The silver inlay along the cornice, meeting the next shrine's.
    for x in 0..SIZE {
        tex.set(x, 1, silver.pick(0.66, x, 1));
        tex.set(x, 2, silver.pick(0.36, x, 2));
    }
    // Warp light spills onto the sill in front of the opening.
    for y in 27..29 {
        for x in 0..SIZE {
            let dx = (c(x) - 16.0).abs();
            if dx < r_in {
                let k = (1.0 - dx / r_in) * if y == 27 { 0.6 } else { 0.3 };
                tex.set(x, y, tex.get(x, y).mix(light.at(5), k));
            }
        }
    }
    // The keystone: a silver block at the crown with a glowing gem.
    let mut key = Mask::new();
    plate(&mut tex, &mut key, &silver, 14, 4, 18, 8, 0.56);
    tex.set(15, 5, light.at(7));
    tex.set(16, 5, light.at(6));
    tex.set(15, 6, light.at(5));
    tex.set(16, 6, light.at(4));
    cast_shadow(&mut tex, &key, SOOT, 0.4);
    // A rune on each pillar, and a few sparks in the curtain.
    let mut marks = Mask::new();
    rune(&mut marks, 6, 1, 13);
    rune(&mut marks, 7, 26, 13);
    glow(&mut tex, &marks, &light, 0.66);
    for s in Ctx::BASE.sites(salt ^ 9, 4.0) {
        let (x, y) = (s.x as usize % SIZE, s.y as usize % SIZE);
        if s.r[0] < 0.4 && open(c(x), c(y)) && open(c(x) - 1.0, c(y)) && open(c(x), c(y) - 1.0) {
            tex.set(x, y, light.lightest());
        }
    }
    tex
}

// ------------------------------------------------------------ storage chest

/// A plank pixel for the chest: `lx`/`ly` are the offsets inside the plank
/// (`len` long along its grain), so each board gets a lit upper edge, a dark
/// lower joint and its own tone.
fn chest_board(salt: u32, id: u32, x: usize, y: usize, ly: usize, h: usize, vertical: bool) -> Rgba {
    let wood = plank_ramp();
    let mut t = 0.5 + (unit(hash2(salt, id)) - 0.5) * 0.16 + grain(hash2(salt, 10 + id), x, y, vertical) * 0.3;
    if ly == 0 {
        t += 0.2;
    }
    if ly + 1 == h {
        t -= 0.3;
    }
    wood.pick(t, x, y)
}

/// A frame pixel: darker wood than the boards, with a lit outer rim.
fn chest_frame(salt: u32, x: usize, y: usize, lit: bool, shaded: bool) -> Rgba {
    let wood = plank_ramp();
    let mut t = 0.17 + grain(hash2(salt, 99), x, y, false) * 0.2;
    if lit {
        t += 0.2;
    }
    if shaded {
        t -= 0.16;
    }
    wood.pick(t, x, y)
}

#[allow(clippy::too_many_arguments)]
/// A flat iron strap pixel: lit on its upper/left edge, shaded on the
/// lower/right, so a band reads as raised metal.
fn strap(
    tex: &mut Tex,
    mask: &mut Mask,
    iron: &Ramp,
    x: usize,
    y: usize,
    top: bool,
    bottom: bool,
    left: bool,
    right: bool,
) {
    let mut t = 0.52;
    if top || left {
        t += 0.26;
    }
    if bottom || right {
        t -= 0.28;
    }
    tex.set(x, y, iron.pick(t, x, y));
    mask.set(x, y);
}

/// A rivet: one bright pixel with the strap's dark under it.
pub(crate) fn rivet(tex: &mut Tex, iron: &Ramp, x: usize, y: usize) {
    tex.set(x, y, iron.at(7));
    if x + 1 < SIZE {
        tex.set(x + 1, y, iron.at(2));
    }
}

/// Chest front: a dark wooden frame round horizontal boards, an iron band
/// across the lid seam, corner brackets, and a lock plate with a keyhole in
/// the upper half. The frame runs to every edge, so two chests side by side
/// read as two chests; the band meets itself across the join.
pub fn storage_chest_side(_ctx: Ctx) -> Tex {
    let salt = seed("storage_chest_side");
    let iron = iron_ramp();
    let shade = Rgba::hex(0x1e0e18);
    // Board rows between the frame (3 px) and the bottom rail.
    let rows: [(usize, usize); 4] = [(3, 8), (8, 12), (15, 22), (22, 29)];
    let mut tex = Tex::from_fn(|x, y| {
        if !(3..SIZE - 3).contains(&y) || !(3..SIZE - 3).contains(&x) {
            let lit = y == 0 || x == 0;
            let shaded = y == SIZE - 1 || x == SIZE - 1;
            return chest_frame(salt, x, y, lit, shaded);
        }
        for (i, (a, b)) in rows.iter().enumerate() {
            if (*a..*b).contains(&y) {
                return chest_board(salt, i as u32, x, y, y - a, b - a, false);
            }
        }
        // Band rows get overpainted; wood underneath.
        chest_board(salt, 7, x, y, 1, 3, false)
    });
    let mut metal = Mask::new();
    // The band across the seam, y 12..15, running to both edges.
    for y in 12..15 {
        for x in 0..SIZE {
            strap(&mut tex, &mut metal, &iron, x, y, y == 12, y == 14, false, false);
        }
    }
    // Corner brackets: L shapes, arm 7 long, 3 thick, at all four corners.
    for (cx, cy) in [(0usize, 0usize), (SIZE - 1, 0), (0, SIZE - 1), (SIZE - 1, SIZE - 1)] {
        for k in 0..7usize {
            for w in 0..3usize {
                let (xa, ya) = (if cx == 0 { k } else { cx - k }, if cy == 0 { w } else { cy - w });
                let (xb, yb) = (if cx == 0 { w } else { cx - w }, if cy == 0 { k } else { cy - k });
                for (x, y) in [(xa, ya), (xb, yb)] {
                    strap(
                        &mut tex,
                        &mut metal,
                        &iron,
                        x,
                        y,
                        y == 0,
                        y == SIZE - 1,
                        x == 0,
                        x == SIZE - 1,
                    );
                }
            }
        }
        let rx = if cx == 0 { 1 } else { cx - 2 };
        let ry = if cy == 0 { 1 } else { cy - 1 };
        rivet(&mut tex, &iron, rx, ry);
    }
    // Lock plate, upper half, centred.
    let mut lock = Mask::new();
    plate(&mut tex, &mut lock, &iron, 12, 8, 20, 18, 0.5);
    // Keyhole: round head, narrow slot.
    let hole = Rgba::hex(0x0b0a12);
    for (x, y) in [
        (15, 11),
        (16, 11),
        (15, 12),
        (16, 12),
        (15, 13),
        (16, 13),
        (15, 14),
        (16, 14),
        (15, 15),
        (16, 15),
    ] {
        tex.set(x, y, hole);
    }
    for (x, y) in [(14, 11), (17, 11), (14, 12), (17, 12)] {
        tex.set(x, y, hole);
    }
    tex.set(14, 10, iron.at(2));
    tex.set(17, 10, iron.at(2));
    for (x, y) in [(13, 9), (18, 9), (13, 16), (18, 16)] {
        tex.set(x, y, iron.at(7));
    }
    for (x, y) in [(15, 10), (16, 10)] {
        tex.set(x, y, hole);
    }
    metal.0.iter_mut().zip(lock.0.iter()).for_each(|(a, b)| *a |= *b);
    cast_shadow(&mut tex, &metal, shade, 0.55);
    tex
}

/// Chest lid: boards running left to right in a dark frame, two iron straps
/// across them, rivets on every strap. Same wood and iron as the front.
pub fn storage_chest_top(_ctx: Ctx) -> Tex {
    let salt = seed("storage_chest_top");
    let iron = iron_ramp();
    let shade = Rgba::hex(0x1e0e18);
    let h = 6usize;
    let mut tex = Tex::from_fn(|x, y| {
        if !(2..SIZE - 2).contains(&y) || !(2..SIZE - 2).contains(&x) {
            let lit = y == 0 || x == 0;
            let shaded = y == SIZE - 1 || x == SIZE - 1;
            return chest_frame(salt, x, y, lit, shaded);
        }
        let p = (y - 2) / h;
        let ly = (y - 2) % h;
        let mut t = chest_board(salt, 20 + p as u32, x, y, ly, h, false);
        // Butt joints in the boards, staggered.
        let joint = [10usize, 22, 15, 6, 25][p.min(4)];
        if x == joint {
            t = plank_ramp().at(1);
        }
        t
    });
    let mut metal = Mask::new();
    for x0 in [6usize, 23] {
        for y in 0..SIZE {
            for x in x0..x0 + 3 {
                strap(&mut tex, &mut metal, &iron, x, y, false, false, x == x0, x == x0 + 2);
            }
        }
        for y in [4usize, 16, 27] {
            rivet(&mut tex, &iron, x0, y);
        }
    }
    cast_shadow(&mut tex, &metal, shade, 0.55);
    tex
}
