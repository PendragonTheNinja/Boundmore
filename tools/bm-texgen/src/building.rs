//! Building blocks (milestone 22, `docs/milestones/22-building-blocks.md`):
//! the crafted families builders work with. Each rock (stone, basalt,
//! limestone) gives bricks, refined, carved and tiled blocks; marble comes
//! plain, bordered and as a frieze; each timber gives refined and carved
//! wood; soils, clay, peat and gravel are pressed into compact blocks; the
//! glow crystal is refined and faceted; glass comes clear, framed and
//! ornate; each metal and gem becomes a block; and the lantern glows. Also
//! the icons of the other two rocks' stones, the brick mixes and wax.
//!
//! A family keeps its material: every block of a rock is painted through
//! that rock's own ramp with that rock's own grain (stone's slabs, basalt's
//! columns and pale flecks, limestone's beds and fossils), so a basalt brick
//! and a carved basalt block read as one rock. Each kind keeps its shape
//! whatever the material: bricks are courses in mortar, refined is one
//! dressed block with a bevel, carved is a sunk panel with a raised motif,
//! tiled is four square tiles. Shapes are height fields lit from the top
//! left (`Field::light`), so every edge catches the light the same way.
//!
//! Colourable blocks keep their material's ramp, as stone, timber and brick
//! do: the game recolours a face by its luma around its average, so what
//! matters is the light and shade, not the hue.

use std::f32::consts::{PI, TAU};

use crate::color::{Ramp, Rgba};
use crate::gathered::{blob, ramp as soft_ramp};
use crate::gems::Gem;
use crate::icons::{Icon, Lump, seg_dist};
use crate::living::{basalt_ramp, clay_ramp, gnarled_wood_ramp, limestone_ramp, mud_ramp, pale_wood_ramp, peat_ramp};
use crate::materials::{
    RockStyle, copper_ramp, crystal_ramp, flame_ramp, mortar_ramp, pebble_ramps, plank_ramp, rock, soil_ramp, specks,
    steel_ramp, stone_ramp, stone_style, timber_with,
};
use crate::noise::{Rng, cell, fbm, hash2, seed, unit, value, wrap_delta, wrap_i};
use crate::spark::{alloy_ramp, dark_iron_ramp};
use crate::stations::iron_ramp;
use crate::tex::{Ctx, Field, SIZE, SIZEF, Tex, c};
use crate::worlds::{gold_alloy_ramp, titanium_alloy_ramp};

// ----------------------------------------------------------------- shapes

/// A field from a function of the pixel centre, in units.
pub(crate) fn field(f: impl Fn(f32, f32) -> f32) -> Field {
    Field::from_fn(|x, y| f(c(x), c(y)))
}

/// How far a point lies inside the rectangle (negative outside).
pub(crate) fn inset(p: (f32, f32), x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    (p.0 - x0).min(x1 - p.0).min(p.1 - y0).min(y1 - p.1)
}

/// How far a point lies inside the face.
pub(crate) fn face_inset(p: (f32, f32)) -> f32 {
    inset(p, 0.0, 0.0, SIZEF, SIZEF)
}

/// A bevelled rise: 0 at `d` = 0, 1 by `w` units in.
pub(crate) fn rise(d: f32, w: f32) -> f32 {
    (d / w).clamp(0.0, 1.0)
}

/// A soft bump on the band `lo..hi` of `d`: 0 outside, 1 in the middle.
pub(crate) fn ridge(d: f32, lo: f32, hi: f32) -> f32 {
    let mid = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    (1.0 - ((d - mid) / half).abs()).clamp(0.0, 1.0)
}

/// Paints a height field lit from the top left through `ramp`: `tone`
/// gives each pixel's tone before the light, `k` how strongly slopes shade.
pub(crate) fn shade(ramp: &Ramp, height: &Field, k: f32, tone: impl Fn(usize, usize, f32) -> f32) -> Tex {
    let light = height.light();
    Tex::from_fn(|x, y| ramp.pick(tone(x, y, height.at(x, y)) + light.at(x, y) * k, x, y))
}

/// Sets a few one-pixel sparkles (a white centre and a soft cross).
pub(crate) fn sparkles(tex: &mut Tex, at: &[(i32, i32)]) {
    let white = Rgba::hex(0xffffff);
    for &(x, y) in at {
        for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (px, py) = (wrap_i(x + dx), wrap_i(y + dy));
            let col = if dx == 0 && dy == 0 {
                white
            } else {
                tex.get(px, py).mix(white, 0.5)
            };
            tex.set(px, py, col);
        }
    }
}

// ------------------------------------------------------------------ rocks

/// The three rocks the rock families are cut from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rock {
    Stone,
    Basalt,
    Limestone,
}

impl Rock {
    pub const ALL: [Rock; 3] = [Rock::Stone, Rock::Basalt, Rock::Limestone];

    pub fn name(self) -> &'static str {
        match self {
            Rock::Stone => "stone",
            Rock::Basalt => "basalt",
            Rock::Limestone => "limestone",
        }
    }

    pub(crate) fn ramp(self) -> Ramp {
        match self {
            Rock::Stone => stone_ramp(),
            Rock::Basalt => basalt_ramp(),
            Rock::Limestone => limestone_ramp(),
        }
    }

    /// The rock's own pattern (as its natural block paints it).
    fn style(self) -> RockStyle {
        match self {
            Rock::Stone => stone_style(),
            Rock::Basalt => RockStyle {
                salt: seed("basalt"),
                spacing: 7.0,
                stretch: 0.55,
                bevel: 1.8,
                cracks: 0.8,
                tone: 0.48,
                spread: 0.22,
                tilt: 0.03,
                rim: 0.26,
                mottle: 0.16,
            },
            Rock::Limestone => RockStyle {
                salt: seed("limestone"),
                spacing: 10.0,
                stretch: 2.6,
                bevel: 2.2,
                cracks: 0.3,
                tone: 0.58,
                spread: 0.16,
                tilt: 0.02,
                rim: 0.18,
                mottle: 0.16,
            },
        }
    }

    /// The middle tone its blocks are painted at.
    fn tone(self) -> f32 {
        match self {
            Rock::Stone => 0.56,
            Rock::Basalt => 0.56,
            Rock::Limestone => 0.58,
        }
    }

    /// The rock's grain around zero: its slabs, columns or beds, softened
    /// so they read as the stone a block was cut from, not as joins.
    pub(crate) fn grain(self, salt: u32) -> Field {
        let mut style = self.style();
        style.salt = hash2(style.salt, salt);
        style.cracks *= 0.4;
        let f = rock(Ctx::BASE, &style);
        let tone = style.tone;
        let beds = self == Rock::Limestone;
        Field::from_fn(|x, y| {
            let mut g = (f.at(x, y) - tone).clamp(-0.22, 0.22);
            if beds {
                // Limestone's faint layering.
                g += (c(y) / SIZEF * 4.0 * TAU).sin() * 0.03;
            }
            g
        })
    }

    /// The rock's own flecks: basalt's pale crystals, limestone's shells.
    pub(crate) fn flecks(self, tex: &mut Tex, salt: u32) {
        match self {
            Rock::Stone => specks(Ctx::BASE, tex, salt, 7.0, 0.25, Rgba::hex(0x3f4560)),
            Rock::Basalt => specks(Ctx::BASE, tex, salt, 6.0, 0.4, Rgba::hex(0xb8cad6)),
            Rock::Limestone => specks(Ctx::BASE, tex, salt, 6.5, 0.4, Rgba::hex(0x8c7058)),
        }
    }
}

/// Bricks: four courses of 16 x 8 bricks in running bond, each brick its
/// own shade with the rock's grain in it, bevelled, a few chipped corners,
/// set in mortar a shade darker than the rock.
fn rock_bricks(r: Rock) -> Tex {
    let name = format!("{}_bricks", r.name());
    let salt = seed(&name);
    let grain = r.grain(salt);
    let base = r.tone();
    // Brick-local coordinates: (course, brick in course, x and y in brick).
    let local = |fx: f32, fy: f32| {
        let row = (fy / 8.0).floor() as i32;
        let off = if row % 2 == 0 { 0.0 } else { 8.0 };
        let bx = (fx + off).rem_euclid(SIZEF);
        (row, (bx / 16.0).floor() as i32, bx % 16.0, fy % 8.0)
    };
    let height = field(|fx, fy| {
        let (_, _, lx, ly) = local(fx, fy);
        let d = inset((lx, ly), 0.0, 0.0, 15.0, 7.0);
        if d <= 0.0 { 0.0 } else { 0.25 + 0.75 * rise(d, 1.6) }
    });
    let mut tex = shade(&r.ramp(), &height, 0.42, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        let (row, col, lx, ly) = local(fx, fy);
        if h <= 0.0 {
            return base - 0.3 + (value(salt ^ 3, 8, 8, fx, fy) - 0.5) * 0.1;
        }
        let id = hash2(salt, (row * 2 + col) as u32);
        let mut t = base + (unit(id) - 0.5) * 0.14 + grain.at(x, y) * 0.7;
        // A chipped corner on some bricks.
        let corner = (unit(hash2(id, 1)) * 4.0) as u32;
        let (cx, cy) = [(0.0, 0.0), (15.0, 0.0), (0.0, 7.0), (15.0, 7.0)][corner as usize];
        if unit(hash2(id, 2)) < 0.5 && (lx - cx).abs() + (ly - cy).abs() < 2.2 {
            t -= 0.18;
        }
        t
    });
    r.flecks(&mut tex, salt ^ 9);
    tex
}

/// Refined: one dressed block filling the face, smooth, its rock's grain
/// faint in it, a broad bevel round the edge and a crisp arris inside it.
fn refined_rock(r: Rock) -> Tex {
    let name = format!("refined_{}", r.name());
    let salt = seed(&name);
    let grain = r.grain(salt);
    let base = r.tone() + 0.04;
    let height = field(|fx, fy| {
        let d = face_inset((fx, fy));
        rise(d, 2.5) - ridge(d, 3.0, 4.0) * 0.25
    });
    let mut tex = shade(&r.ramp(), &height, 0.5, |x, y, _| {
        let (fx, fy) = (c(x), c(y));
        // Tooling: faint even strokes across the face.
        let tool = (value(salt ^ 4, 1, 16, fx, fy) - 0.5) * 0.05;
        base + grain.at(x, y) * 0.35 + tool + (0.5 - (fx + fy) / 64.0) * 0.08
    });
    r.flecks(&mut tex, salt ^ 9);
    tex
}

/// Carved: a dressed block with a sunk panel, and in it a raised diamond
/// holding a rosette, with a boss at each corner of the panel.
fn carved_rock(r: Rock) -> Tex {
    let name = format!("carved_{}", r.name());
    let salt = seed(&name);
    let grain = r.grain(salt);
    let base = r.tone() + 0.02;
    let height = field(carved_height);
    let mut tex = shade(&r.ramp(), &height, 0.5, |x, y, h| {
        base + grain.at(x, y) * 0.35 + (h - 0.6) * 0.18
    });
    r.flecks(&mut tex, salt ^ 9);
    tex
}

/// The carved panel's relief, shared by the carved rocks.
fn carved_height(fx: f32, fy: f32) -> f32 {
    let p = (fx, fy);
    let mut h = rise(face_inset(p), 2.0);
    // The sunk panel.
    let panel = inset(p, 4.0, 4.0, 28.0, 28.0);
    if panel > 0.0 {
        h -= 0.6 * rise(panel, 1.5);
    }
    let (dx, dy) = (fx - 16.0, fy - 16.0);
    // The raised diamond, and a sunk diamond in it.
    let diamond = 10.5 - (dx.abs() + dy.abs());
    if diamond > 0.0 {
        h += 0.6 * rise(diamond, 1.6);
    }
    let inner = 6.0 - (dx.abs() + dy.abs());
    if inner > 0.0 {
        h -= 0.35 * rise(inner, 1.0);
    }
    // A rosette: four petals round a boss.
    for (px, py) in [(0.0, -2.6), (2.6, 0.0), (0.0, 2.6), (-2.6, 0.0)] {
        let d = 1.7 - (dx - px).hypot(dy - py);
        if d > 0.0 {
            h += 0.3 * rise(d, 1.0);
        }
    }
    let boss = 1.3 - dx.hypot(dy);
    if boss > 0.0 {
        h += 0.4 * rise(boss, 0.8);
    }
    // Bosses in the panel's corners.
    for (bx, by) in [(7.5, 7.5), (24.5, 7.5), (7.5, 24.5), (24.5, 24.5)] {
        let d = 1.8 - (fx - bx).hypot(fy - by);
        if d > 0.0 {
            h += 0.45 * rise(d, 1.0);
        }
    }
    h
}

/// Tiled: four square tiles in thin grout, each with a groove round its
/// edge and a polished sheen in its top-left corner; the two on one
/// diagonal a shade lighter, like a laid floor.
fn tiled_rock(r: Rock) -> Tex {
    let name = format!("tiled_{}", r.name());
    let salt = seed(&name);
    let grain = r.grain(salt);
    let base = r.tone() + 0.02;
    let local = |fx: f32, fy: f32| {
        let (tx, ty) = ((fx / 16.0).floor() as i32, (fy / 16.0).floor() as i32);
        (tx, ty, fx % 16.0, fy % 16.0)
    };
    let height = field(|fx, fy| {
        let (_, _, lx, ly) = local(fx, fy);
        let d = inset((lx, ly), 0.0, 0.0, 15.0, 15.0);
        if d <= 0.0 {
            0.0
        } else {
            0.3 + 0.7 * rise(d, 1.4) - ridge(d, 2.6, 4.0) * 0.4
        }
    });
    let mut tex = shade(&r.ramp(), &height, 0.45, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        let (tx, ty, lx, ly) = local(fx, fy);
        if h <= 0.0 {
            return base - 0.34;
        }
        let checker = if (tx + ty) % 2 == 0 { 0.07 } else { -0.05 };
        let sheen = (1.0 - (lx + ly) / 14.0).max(0.0) * 0.12;
        base + checker + sheen + grain.at(x, y) * 0.35
    });
    r.flecks(&mut tex, salt ^ 9);
    tex
}

// ----------------------------------------------------------------- marble

/// Marble: near-white with cool grey shadows, so any colour takes on it.
fn marble_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x4c4a5a, 0x6c6a7a, 0x8e8c98, 0xaeabb2, 0xc8c5c6, 0xdcd9d6, 0xebe8e2, 0xf8f6f0,
    ])
}

/// Marble's tone before any relief: soft clouds crossed by a few long
/// warped veins and finer threads.
fn marble_tone(salt: u32, fx: f32, fy: f32) -> f32 {
    let cloud = (fbm(salt, 2, 2, 3, fx, fy) - 0.5) * 0.18;
    let warp = (fbm(salt ^ 1, 2, 2, 3, fx, fy) - 0.5) * 1.6;
    // Integer frequencies, so the veins tile.
    let v1 = ((fx * 1.0 + fy * 2.0) / SIZEF + warp).mul_add(TAU, 0.0).sin().abs();
    let warp2 = (fbm(salt ^ 2, 4, 4, 2, fx, fy) - 0.5) * 1.2;
    let v2 = ((fx * 3.0 - fy * 1.0) / SIZEF + warp2).mul_add(TAU, 0.0).sin().abs();
    let mut t = 0.7 + cloud;
    t -= (1.0 - (v1 / 0.09).min(1.0)) * 0.32 + (1.0 - (v1 / 0.3).min(1.0)) * 0.06;
    t -= (1.0 - (v2 / 0.06).min(1.0)) * 0.16;
    t
}

/// Marble: a smooth veined block with a fine bevel.
fn marble(_ctx: Ctx) -> Tex {
    let salt = seed("marble");
    let height = field(|fx, fy| rise(face_inset((fx, fy)), 1.5));
    shade(&marble_ramp(), &height, 0.4, |x, y, _| marble_tone(salt, c(x), c(y)))
}

/// Bordered marble: veined marble inside a carved border band: a groove,
/// a raised band of beads, and a second groove.
fn bordered_marble(_ctx: Ctx) -> Tex {
    let salt = seed("bordered_marble");
    let height = field(|fx, fy| {
        let d = face_inset((fx, fy));
        let mut h = rise(d, 1.5);
        h -= ridge(d, 2.0, 3.6) * 0.5;
        h -= ridge(d, 7.0, 8.6) * 0.5;
        if (3.6..7.0).contains(&d) {
            // Beads along the band, every four units.
            let along = if (fx - 16.0).abs() > (fy - 16.0).abs() { fy } else { fx };
            let (bx, by) = (wrap_delta(along - 2.0).rem_euclid(4.0) - 2.0, d - 5.3);
            h += (1.0 - bx.hypot(by) / 1.6).max(0.0) * 0.45;
        }
        h
    });
    shade(&marble_ramp(), &height, 0.45, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        let d = face_inset((fx, fy));
        let veins = marble_tone(salt, fx, fy);
        // The band is smoother: its veins faint.
        let t = if d < 8.6 { 0.72 + (veins - 0.7) * 0.35 } else { veins };
        t + (h - 1.0) * 0.12
    })
}

/// Marble frieze: a band across the middle between two mouldings, in it a
/// row of raised rosettes with diamonds between them, plain veined marble
/// above and below.
fn marble_frieze(_ctx: Ctx) -> Tex {
    let salt = seed("marble_frieze");
    let height = field(|fx, fy| {
        let mut h = 0.5;
        // Mouldings above and below the band, and the band sunk between.
        h += ridge(fy, 7.5, 11.0) * 0.45 + ridge(fy, 21.0, 24.5) * 0.45;
        if (11.0..21.0).contains(&fy) {
            h -= 0.3;
            let lx = fx % 16.0;
            let (dx, dy) = (lx - 8.0, fy - 16.0);
            // A rosette in each half, petals round a boss.
            let r = dx.hypot(dy);
            let petals = 3.6 + (dy.atan2(dx) * 6.0).cos() * 0.8;
            if r < petals {
                h += 0.4 * rise(petals - r, 1.2);
            }
            if r < 1.4 {
                h += 0.2;
            }
            // A small diamond between rosettes.
            let ex = wrap_delta(fx - 0.0).abs().min((fx - 16.0).abs());
            let dm = 2.6 - (ex + dy.abs());
            if dm > 0.0 {
                h += 0.35 * rise(dm, 1.0);
            }
        }
        h
    });
    shade(&marble_ramp(), &height, 0.5, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        let veins = marble_tone(salt, fx, fy);
        let band = (11.0..21.0).contains(&fy);
        let t = if band { 0.66 + (veins - 0.7) * 0.3 } else { veins };
        t + (h - 0.5) * 0.15
    })
}

// ------------------------------------------------------------------- wood

/// The three timbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wood {
    Timber,
    Pale,
    Gnarled,
}

impl Wood {
    pub const ALL: [Wood; 3] = [Wood::Timber, Wood::Pale, Wood::Gnarled];

    /// The timber's block name.
    pub fn name(self) -> &'static str {
        match self {
            Wood::Timber => "timber",
            Wood::Pale => "pale_timber",
            Wood::Gnarled => "gnarled_timber",
        }
    }

    pub(crate) fn ramp(self) -> Ramp {
        match self {
            Wood::Timber => plank_ramp(),
            Wood::Pale => pale_wood_ramp(),
            Wood::Gnarled => gnarled_wood_ramp(),
        }
    }

    /// Grain: streaks along `x` (or along `y`), a little wavier for gnarled.
    pub(crate) fn grain(self, salt: u32, fx: f32, fy: f32, across: bool) -> f32 {
        let wave = match self {
            Wood::Gnarled => (fbm(salt ^ 7, 2, 2, 2, fx, fy) - 0.5) * 3.0,
            _ => 0.0,
        };
        let g = if across {
            fbm(salt, 8, 2, 2, fx + wave, fy)
        } else {
            fbm(salt, 2, 8, 2, fx, fy + wave)
        };
        let fine = if across {
            value(salt ^ 3, 32, 1, fx, fy)
        } else {
            value(salt ^ 3, 1, 32, fx, fy)
        };
        (g - 0.5) * 0.5 + (fine - 0.5) * 0.12
    }
}

/// Pale and gnarled timber: the plain timber's planks in their trunk's
/// wood; gnarled has dark knots in its planks.
fn other_timber(w: Wood) -> Tex {
    let salt = seed(w.name());
    let mut tex = timber_with(salt, &w.ramp());
    if w == Wood::Gnarled {
        let ramp = w.ramp();
        // A knot in some planks: dark rings round a darker heart.
        for (kx, ky) in [(9.0, 3.5), (24.5, 19.5), (16.0, 27.5)] {
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let (dx, dy) = (wrap_delta(c(x) - kx), (c(y) - ky) * 1.6);
                    let r = dx.hypot(dy);
                    if r < 1.2 {
                        tex.set(x, y, ramp.at(0));
                    } else if r < 3.0 && ((r * 2.2) as i32) % 2 == 0 {
                        tex.set(x, y, tex.get(x, y).mix(ramp.at(1), 0.6));
                    }
                }
            }
        }
    }
    tex
}

/// Refined wood: two wide planed boards, no nails or butt joints, a fine
/// straight grain and a soft sheen along each board's upper edge.
fn refined_wood(w: Wood) -> Tex {
    let salt = seed(&format!("refined_{}", w.name()));
    let height = field(|_, fy| {
        let ly = fy % 16.0;
        let d = ly.min(15.0 - ly);
        if d <= 0.0 { 0.0 } else { 0.3 + 0.7 * rise(d, 1.3) }
    });
    shade(&w.ramp(), &height, 0.45, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        if h <= 0.0 {
            return 0.08;
        }
        let board = (fy / 16.0) as u32;
        let ly = fy % 16.0;
        let off = (unit(hash2(salt, board)) - 0.5) * 0.12;
        let sheen = (-((ly - 3.5) / 2.5).powi(2)).exp() * 0.1;
        0.55 + off + sheen + w.grain(hash2(salt, board), fx, fy, true) * 0.8
    })
}

/// Carved wood: a mitred frame (rails with their grain across, stiles with
/// it down) round a raised, bevelled panel with a carved rosette.
fn carved_wood(w: Wood) -> Tex {
    let salt = seed(&format!("carved_{}", w.name()));
    let height = field(|fx, fy| {
        let p = (fx, fy);
        let (dx, dy) = (fx - 16.0, fy - 16.0);
        let mut h = rise(face_inset(p), 1.5);
        // A groove along each mitre.
        if face_inset(p) < 6.0 && (dx.abs() - dy.abs()).abs() < 0.7 {
            h -= 0.25;
        }
        let panel = inset(p, 6.0, 6.0, 26.0, 26.0);
        if panel > 0.0 {
            // Sunk, then raised again to a field in the middle.
            h = h - 0.55 + 0.45 * rise(panel - 1.0, 3.0);
            // The rosette: four sunk petals and a raised boss.
            for (px, py) in [(0.0, -3.0), (3.0, 0.0), (0.0, 3.0), (-3.0, 0.0)] {
                let d = 2.1 - (dx - px).hypot(dy - py);
                if d > 0.0 {
                    h -= 0.35 * rise(d, 0.9);
                }
            }
            let boss = 1.4 - dx.hypot(dy);
            if boss > 0.0 {
                h += 0.3 * rise(boss, 0.8);
            }
        }
        h
    });
    shade(&w.ramp(), &height, 0.5, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        let (dx, dy) = (fx - 16.0, fy - 16.0);
        let frame = inset((fx, fy), 6.0, 6.0, 26.0, 26.0) <= 0.0;
        // Stiles (left and right) run their grain down, rails across.
        let across = !(frame && dx.abs() > dy.abs());
        let part = if !frame {
            0
        } else if across {
            1
        } else {
            2
        };
        let g = w.grain(hash2(salt, part), fx, fy, across);
        let off = [0.04, -0.02, 0.0][part as usize];
        0.52 + off + g * 0.7 + (h - 0.7) * 0.15
    })
}

// ---------------------------------------------------------- compact earth

/// A pressed block: the material squeezed into four wavy beds, each its
/// own shade, split by dark seams, with a bevel round the block.
fn pressed(name: &str, ramp: &Ramp, grit: Rgba, base: f32) -> Tex {
    let salt = seed(name);
    let bed = |fx: f32, fy: f32| {
        let wave = (fx / SIZEF * TAU * 2.0 + unit(salt) * TAU).sin() * 1.0
            + (fx / SIZEF * TAU + unit(hash2(salt, 1)) * TAU).sin() * 0.8;
        let by = (fy + wave).rem_euclid(SIZEF);
        ((by / 8.0) as u32, by % 8.0)
    };
    let height = field(|fx, fy| {
        let (_, ly) = bed(fx, fy);
        rise(face_inset((fx, fy)), 1.6) - ridge(ly, -0.6, 0.8) * 0.4
    });
    let mut tex = shade(ramp, &height, 0.45, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        let (k, _) = bed(fx, fy);
        let off = [0.06, -0.05, 0.03, -0.08][k as usize % 4];
        let lumps = (fbm(salt ^ 2, 8, 4, 2, fx, fy) - 0.5) * 0.3;
        base + off + lumps + (h - 1.0) * 0.2
    });
    specks(Ctx::BASE, &mut tex, salt ^ 5, 6.0, 0.35, grit);
    tex
}

/// Compact gravel: small pebbles pressed flat in a pale cement, the block
/// bevelled round its edge.
fn compact_gravel(_ctx: Ctx) -> Tex {
    let salt = seed("compact_gravel");
    let sites = Ctx::BASE.sites(salt, 3.3);
    let ramps = pebble_ramps();
    let cement = mortar_ramp();
    let edge = field(|fx, fy| rise(face_inset((fx, fy)), 1.6));
    let light = edge.light();
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&sites, fx, fy, 1.0);
        let site = &sites[k.index];
        let l = light.at(x, y) * 0.5 + (edge.at(x, y) - 1.0) * 0.2;
        if k.edge < 0.7 || site.r[3] < 0.12 {
            return cement.pick(0.5 + l + (value(salt ^ 1, 8, 8, fx, fy) - 0.5) * 0.15, x, y);
        }
        let kind = (site.r[0] * 4.0) as usize % 4;
        let t = 0.52 + (site.r[1] - 0.5) * 0.3 + k.rim(1.4) * 0.3 + l;
        ramps[kind].pick(t, x, y)
    })
}

// ------------------------------------------------------------ glow crystal

/// Refined glow crystal: a smooth cut slab, glowing brightest in its
/// middle, ghosts of its prisms running up through it, a crisp bright
/// arris inside the bevel.
fn refined_glow_crystal(_ctx: Ctx) -> Tex {
    let salt = seed("refined_glow_crystal");
    let height = field(|fx, fy| rise(face_inset((fx, fy)), 2.2));
    let mut tex = shade(&crystal_ramp(), &height, 0.55, |x, y, _| {
        let (fx, fy) = (c(x), c(y));
        let d = face_inset((fx, fy));
        let r = (fx - 16.0).hypot(fy - 16.0);
        let glow = (1.0 - r / 17.0).max(0.0) * 0.3;
        let ghost = (value(salt, 8, 1, fx, fy) - 0.5) * 0.16;
        let arris = ridge(d, 3.0, 4.2) * 0.22;
        0.5 + glow + ghost + arris
    });
    sparkles(&mut tex, &[(11, 10), (21, 22)]);
    tex
}

/// A cut-gem face: an octagonal table in the middle, eight facets round
/// it falling to the edge, each lit by which way it turns, `t0` the tone
/// of the table. Returns the tone and whether the point is on a facet edge.
fn gem_cut(fx: f32, fy: f32, cx: f32, cy: f32, half: f32, t0: f32) -> (f32, bool) {
    let (dx, dy) = (fx - cx, fy - cy);
    let table = half * 0.45;
    let oct = dx.abs().max(dy.abs()).max((dx.abs() + dy.abs()) * 0.72);
    if oct < table {
        // The table, lighter towards the top left.
        return (t0 - (dx + dy) / half * 0.12, (oct - table).abs() < 0.45);
    }
    // Which of eight facets, and which way it faces.
    let a = dy.atan2(dx);
    let sector = ((a / (TAU / 8.0)).round() as i32).rem_euclid(8);
    let facing = sector as f32 * TAU / 8.0;
    // Light from the top left: facets facing up-left (-3pi/4) brightest.
    let lit = (facing + 0.75 * PI).cos();
    let mid = a - facing;
    let edge = (mid.abs() - TAU / 16.0).abs() * dx.hypot(dy) < 0.5;
    (t0 - 0.15 + lit * 0.3, edge)
}

/// Faceted glow crystal: four small cut stones, each a table with facets
/// round it, set edge to edge with thin dark seams.
fn faceted_glow_crystal(_ctx: Ctx) -> Tex {
    let ramp = crystal_ramp();
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let (lx, ly) = (fx % 16.0, fy % 16.0);
        if inset((lx, ly), 0.0, 0.0, 15.0, 15.0) <= 0.0 {
            return ramp.at(1);
        }
        let (t, edge) = gem_cut(lx, ly, 7.5, 7.5, 7.5, 0.72);
        ramp.pick(if edge { t + 0.25 } else { t }, x, y)
    });
    sparkles(&mut tex, &[(5, 5), (21, 5), (5, 21), (21, 21)]);
    tex
}

// ------------------------------------------------------------------ glass

/// Clear glass: almost nothing there, a hairline frame and one faint
/// streak of reflection.
fn clear_glass(_ctx: Ctx) -> Tex {
    let pane = Rgba::hex(0xe4f6fc).with_alpha(0.06);
    let lit = Rgba::hex(0xf8fdff).with_alpha(0.75);
    let dark = Rgba::hex(0xa6bccc).with_alpha(0.7);
    let streak = Rgba::hex(0xffffff).with_alpha(0.32);
    let last = SIZE - 1;
    Tex::from_fn(|x, y| {
        if y == 0 || x == 0 {
            return lit;
        }
        if y == last || x == last {
            return dark;
        }
        let d = x + y;
        if (9..=10).contains(&d) || d == 14 {
            return streak;
        }
        pane
    })
}

/// Framed glass: a pale pane in a metal frame with a cross bar through
/// the middle, each bar lit on its top and left.
fn framed_glass(_ctx: Ctx) -> Tex {
    let metal = iron_ramp();
    let pane = Rgba::hex(0xd4eef6).with_alpha(0.16);
    let streak = Rgba::hex(0xffffff).with_alpha(0.5);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let d = face_inset((fx, fy));
        let bar_x = (fx - 16.0).abs();
        let bar_y = (fy - 16.0).abs();
        // The frame and the cross: tone by which side of the bar this is.
        let on_frame = d < 2.0;
        let on_bar = bar_x < 1.0 || bar_y < 1.0;
        if on_frame || on_bar {
            let t = if on_frame {
                let edge_l = fx.min(fy) < 1.0;
                let edge_r = (SIZEF - fx).min(SIZEF - fy) < 1.0;
                if edge_l {
                    0.85
                } else if edge_r {
                    0.25
                } else {
                    0.55
                }
            } else if bar_x < 1.0 && bar_y < 1.0 {
                0.75
            } else if (bar_x < 1.0 && fx < 16.0) || (bar_y < 1.0 && fy < 16.0) {
                0.72
            } else {
                0.38
            };
            return metal.pick(t, x, y).with_alpha(1.0);
        }
        // Each quarter's own reflection streak.
        let (qx, qy) = (fx % 16.0, fy % 16.0);
        if ((qx + qy) - 9.0).abs() < 0.8 {
            return streak;
        }
        pane
    })
}

/// Ornate glass: leaded panes in a diamond lattice inside a lead frame,
/// alternate panes faintly tinted, a small round boss where leads cross.
fn ornate_glass(_ctx: Ctx) -> Tex {
    let lead = dark_iron_ramp();
    let pane_a = Rgba::hex(0xdcf2f8).with_alpha(0.28);
    let pane_b = Rgba::hex(0xf4eccc).with_alpha(0.34);
    let streak = Rgba::hex(0xffffff).with_alpha(0.55);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let d = face_inset((fx, fy));
        if d < 1.5 {
            let t = if fx.min(fy) < 1.0 { 0.75 } else { 0.4 };
            return lead.pick(t, x, y);
        }
        // Diamond lattice: lines along both diagonals every 8 units.
        let u = (fx + fy).rem_euclid(16.0);
        let v = (fx - fy).rem_euclid(16.0);
        let (du, dv) = (u.min(16.0 - u), v.min(16.0 - v));
        let node = du.hypot(dv);
        if node < 1.8 {
            return lead.pick(0.8 - (fx + fy).rem_euclid(1.0) * 0.0 - node * 0.15, x, y);
        }
        if du < 0.75 || dv < 0.75 {
            return lead.pick(if du < 0.75 { 0.6 } else { 0.45 }, x, y);
        }
        let which = (((fx + fy) / 16.0).floor() as i32 + ((fx - fy) / 16.0).floor() as i32).rem_euclid(2);
        if du < 2.0 && (fx + fy).rem_euclid(16.0) < 8.0 && which == 0 {
            return streak;
        }
        if which == 0 { pane_a } else { pane_b }
    })
}

// ----------------------------------------------------------------- metals

/// The five metals that become blocks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MetalBlock {
    Copper,
    Iron,
    SilverAlloy,
    GoldAlloy,
    TitaniumAlloy,
}

impl MetalBlock {
    pub const ALL: [MetalBlock; 5] = [
        MetalBlock::Copper,
        MetalBlock::Iron,
        MetalBlock::SilverAlloy,
        MetalBlock::GoldAlloy,
        MetalBlock::TitaniumAlloy,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MetalBlock::Copper => "copper",
            MetalBlock::Iron => "iron",
            MetalBlock::SilverAlloy => "silver_alloy",
            MetalBlock::GoldAlloy => "gold_alloy",
            MetalBlock::TitaniumAlloy => "titanium_alloy",
        }
    }

    pub(crate) fn ramp(self) -> Ramp {
        match self {
            MetalBlock::Copper => copper_ramp(),
            MetalBlock::Iron => steel_ramp(),
            MetalBlock::SilverAlloy => alloy_ramp(),
            MetalBlock::GoldAlloy => gold_alloy_ramp(),
            MetalBlock::TitaniumAlloy => titanium_alloy_ramp(),
        }
    }
}

/// The metal forms.
pub const METAL_FORMS: [&str; 6] = ["compact", "refined", "machined", "circuit", "filigree", "grid"];

/// Compact metal: ingots stacked in four courses, alternate courses turned
/// over, each ingot a trapezoid with a bright top face and dark gaps.
fn compact_metal(m: MetalBlock) -> Tex {
    let salt = seed(&format!("compact_{}", m.name()));
    let ramp = m.ramp();
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let row = (fy / 8.0) as i32;
        let off = if row % 2 == 0 { 0.0 } else { 8.0 };
        let bx = (fx + off).rem_euclid(SIZEF);
        let (col, lx, ly) = ((bx / 16.0) as i32, bx % 16.0, fy % 8.0);
        // Upright ingots narrow at the top, turned ones at the bottom.
        let up = row % 2 == 0;
        let depth = if up { 7.0 - ly } else { ly };
        let slant = 0.4 * depth.clamp(0.0, 7.0) / 2.0;
        let d = inset((lx, ly), slant + 0.5, 0.5, 15.5 - slant, 7.5);
        if d <= 0.0 {
            return ramp.at(0);
        }
        let id = hash2(salt, (row * 2 + col) as u32);
        let mut t = 0.5 + (unit(id) - 0.5) * 0.1;
        // The face towards the light: the top for upright ingots.
        let top = if up { ly < 2.5 } else { ly > 5.5 };
        t += if top && up {
            0.28
        } else if top {
            -0.12
        } else {
            0.0
        };
        // Slanted ends lit on the left, shaded on the right.
        if lx < slant + 1.5 {
            t += 0.14;
        } else if lx > 14.5 - slant {
            t -= 0.16;
        }
        // A bright rim along the top edge, a mould mark in the middle.
        if d < 1.0 && ly < 1.5 {
            t += 0.12;
        }
        if (lx - 8.0).abs() < 1.6 && (ly - 4.0).abs() < 0.9 && !top {
            t -= 0.12;
        }
        ramp.pick(t, x, y)
    })
}

/// Refined metal: one polished plate filling the face, bevelled, two broad
/// diagonal glints across it.
fn refined_metal(m: MetalBlock) -> Tex {
    let salt = seed(&format!("refined_{}", m.name()));
    let height = field(|fx, fy| rise(face_inset((fx, fy)), 2.0));
    let light = height.light();
    let ramp = m.ramp();
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let s = (fx + fy).rem_euclid(SIZEF);
        let glint = (-((s - 12.0) / 2.6).powi(2)).exp() * 0.32 + (-((s - 20.0) / 1.4).powi(2)).exp() * 0.18;
        let brushed = (value(salt, 1, 32, fx, fy) - 0.5) * 0.06;
        let t = 0.46 + glint + brushed - (fy - 16.0) / 32.0 * 0.12 + light.at(x, y) * 0.55;
        ramp.pick(t, x, y)
    })
}

/// Machined metal: four brushed plates, the brushing turned in alternate
/// plates, each with a rivet in every corner.
fn machined_metal(m: MetalBlock) -> Tex {
    let salt = seed(&format!("machined_{}", m.name()));
    let height = field(|fx, fy| {
        let (lx, ly) = (fx % 16.0, fy % 16.0);
        let d = inset((lx, ly), 0.0, 0.0, 15.0, 15.0);
        if d <= 0.0 {
            return 0.0;
        }
        let mut h = 0.4 + 0.6 * rise(d, 1.2);
        for (rx, ry) in [(3.5, 3.5), (11.5, 3.5), (3.5, 11.5), (11.5, 11.5)] {
            let r = (lx - rx).hypot(ly - ry);
            if r < 1.6 {
                h += (1.0 - r / 1.6).sqrt() * 0.7;
            }
        }
        h
    });
    shade(&m.ramp(), &height, 0.5, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        if h <= 0.0 {
            return 0.08;
        }
        let (px, py) = ((fx / 16.0) as i32, (fy / 16.0) as i32);
        let across = (px + py) % 2 == 0;
        let brush = if across {
            value(hash2(salt, 1), 2, 32, fx, fy)
        } else {
            value(hash2(salt, 2), 32, 2, fx, fy)
        };
        0.5 + (brush - 0.5) * 0.16 + if across { 0.04 } else { -0.03 }
    })
}

/// The circuit's traces: polylines in units, each ending in a pad. Laid
/// out so the lines that cross an edge come back in on the other side.
const TRACES: [&[(f32, f32)]; 6] = [
    &[(0.0, 5.5), (32.0, 5.5)],
    &[(20.5, 0.0), (20.5, 32.0)],
    &[(4.5, 12.5), (13.5, 12.5), (13.5, 26.5)],
    &[(27.5, 12.5), (24.5, 12.5), (24.5, 16.5)],
    &[(0.0, 29.5), (8.5, 29.5), (8.5, 21.0)],
    &[(28.5, 29.5), (32.0, 29.5)],
];

/// Pads at the ends of the traces.
const PADS: [(f32, f32); 6] = [
    (4.5, 12.5),
    (13.5, 26.5),
    (8.5, 21.0),
    (27.5, 12.5),
    (28.5, 29.5),
    (8.5, 5.5),
];

/// Circuit metal: a dark plate with a faint grid, bright raised traces
/// with pads, and a chip with pins.
fn circuit_metal(m: MetalBlock) -> Tex {
    let salt = seed(&format!("circuit_{}", m.name()));
    let ramp = m.ramp();
    let wrapd = |p: (f32, f32), q: (f32, f32)| (wrap_delta(p.0 - q.0), wrap_delta(p.1 - q.1));
    let trace = |fx: f32, fy: f32| {
        let mut best = f32::MAX;
        for line in TRACES {
            for w in line.windows(2) {
                best = best.min(seg_dist(w[0], w[1], (fx, fy)));
            }
        }
        best
    };
    let height = field(|fx, fy| {
        let mut h = rise(face_inset((fx, fy)), 1.2) * 0.3;
        if trace(fx, fy) < 1.0 {
            h += 0.4;
        }
        for &p in &PADS {
            let (dx, dy) = wrapd((fx, fy), p);
            let r = dx.hypot(dy);
            if r < 2.0 {
                h += if r < 0.9 { 0.1 } else { 0.45 };
            }
        }
        // The chip and its pins.
        if inset((fx, fy), 23.0, 19.0, 30.0, 26.0) > 0.0 {
            h += 0.5;
        }
        let pins = inset((fx, fy), 23.5, 18.0, 29.5, 27.0) > 0.0 && ((fx - 23.5) % 2.0) < 1.0;
        if pins && !(19.0..26.0).contains(&fy) {
            h += 0.35;
        }
        h
    });
    shade(&ramp, &height, 0.5, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        let grid = if fx % 4.0 < 1.0 || fy % 4.0 < 1.0 { 0.03 } else { 0.0 };
        let base = 0.22 + grid + (value(salt, 4, 4, fx, fy) - 0.5) * 0.05;
        if inset((fx, fy), 23.0, 19.0, 30.0, 26.0) > 0.0 {
            // The chip: a dark body with a pale mark.
            let mark = (fx - 25.0).hypot(fy - 21.0) < 0.9;
            return if mark { 0.55 } else { 0.12 };
        }
        if h > 0.62 {
            return 0.72;
        }
        if h > 0.5 {
            return 0.66;
        }
        base
    })
}

/// Filigree metal: interlaced rings of fine raised wire on a darker plate,
/// little beads where they meet.
fn filigree_metal(m: MetalBlock) -> Tex {
    let salt = seed(&format!("filigree_{}", m.name()));
    // Ring centres and radii, and beads.
    let rings: [((f32, f32), f32); 6] = [
        ((8.0, 8.0), 5.5),
        ((24.0, 24.0), 5.5),
        ((24.0, 8.0), 3.0),
        ((8.0, 24.0), 3.0),
        ((16.0, 16.0), 11.3),
        ((0.0, 0.0), 11.3),
    ];
    let beads = [
        (16.0, 0.0),
        (0.0, 16.0),
        (8.0, 8.0),
        (24.0, 24.0),
        (24.0, 8.0),
        (8.0, 24.0),
    ];
    let height = field(|fx, fy| {
        let mut h: f32 = 0.0;
        for &((cx, cy), r) in &rings {
            let d = wrap_delta(fx - cx).hypot(wrap_delta(fy - cy));
            h = h.max(1.0 - ((d - r).abs() / 0.9).min(1.0));
        }
        for &(bx, by) in &beads {
            let d = wrap_delta(fx - bx).hypot(wrap_delta(fy - by));
            h = h.max(1.0 - (d / 1.6).min(1.0));
        }
        h
    });
    shade(&m.ramp(), &height, 0.45, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        let plate = 0.26 + (value(salt, 4, 4, fx, fy) - 0.5) * 0.08;
        if h > 0.05 { 0.42 + h * 0.3 } else { plate }
    })
}

/// Grid metal: a grating of raised bars, lit on their top and left, over
/// deep square holes, a boss where bars cross.
fn grid_metal(m: MetalBlock) -> Tex {
    let ramp = m.ramp();
    let height = field(|fx, fy| {
        let (lx, ly) = (fx % 8.0, fy % 8.0);
        let (bx, by) = ((lx - 0.0).min(8.0 - lx), (ly - 0.0).min(8.0 - ly));
        let bar = |d: f32| (1.0 - (d / 1.6).min(1.0)).sqrt();
        let mut h = bar(bx).max(bar(by));
        if bx.hypot(by) < 1.7 {
            h += 0.25;
        }
        h
    });
    shade(&ramp, &height, 0.5, |x, y, h| {
        let (fx, fy) = (c(x), c(y));
        if h <= 0.0 {
            // The hole: deepest under the bars' shadow (bottom right).
            let (lx, ly) = (fx % 8.0, fy % 8.0);
            return 0.02 + ((8.0 - lx).min(8.0 - ly) / 8.0) * 0.12;
        }
        0.45 + h * 0.15
    })
}

// ------------------------------------------------------------------- gems

/// Compact gem: a block of rough crystals packed together, each split into
/// two facets, dark seams between them, a few glints.
fn compact_gem(g: Gem) -> Tex {
    let salt = seed(&format!("compact_{}_block", g.name()));
    let ramp = g.ramp();
    let sites = Ctx::BASE.sites(salt, 6.0);
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&sites, fx, fy, 1.0);
        let s = &sites[k.index];
        if k.edge < 0.55 {
            return ramp.at(0);
        }
        // Two facets per crystal, split on a line through its point.
        let ang = s.r[0] * TAU;
        let side = k.dx * ang.cos() + k.dy * ang.sin();
        let mut t = 0.5 + (s.r[1] - 0.5) * 0.2;
        t += if side < 0.0 { 0.14 } else { -0.12 };
        t += k.rim(1.6) * 0.25;
        t -= k.d1 * 0.015;
        ramp.pick(t, x, y)
    });
    let glints: Vec<(i32, i32)> = sites
        .iter()
        .filter(|s| s.r[2] < 0.3)
        .map(|s| (s.x as i32, s.y as i32))
        .collect();
    sparkles(&mut tex, &glints);
    tex
}

/// Polished gem: one large stone, cut with a table and eight facets,
/// filling the face, a bright girdle round its edge.
fn polished_gem(g: Gem) -> Tex {
    let ramp = g.ramp();
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let d = face_inset((fx, fy));
        if d < 1.0 {
            let t = if fx.min(fy) < 1.0 { 0.9 } else { 0.25 };
            return ramp.pick(t, x, y);
        }
        let (t, edge) = gem_cut(fx, fy, 16.0, 16.0, 15.0, 0.66);
        ramp.pick(if edge { t + 0.22 } else { t }, x, y)
    });
    sparkles(&mut tex, &[(12, 11)]);
    tex
}

// ---------------------------------------------------------------- lantern

/// The lantern: a warm flame on a stub of tallow behind glass, lighting
/// the panes amber around it, in a dark iron frame with corner rivets.
fn lantern(_ctx: Ctx) -> Tex {
    let iron = dark_iron_ramp();
    let flame = flame_ramp();
    let glow = soft_ramp(0xf0a040, 0.45, 1.0);
    let tallow = soft_ramp(0xe8dcb0, 0.5, 0.95);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let d = face_inset((fx, fy));
        // The frame, its caps top and bottom heavier.
        let cap = !(4.0..28.0).contains(&fy);
        if d < 3.0 || cap {
            for (rx, ry) in [(2.0, 2.0), (30.0, 2.0), (2.0, 30.0), (30.0, 30.0)] {
                let r = (fx - rx).hypot(fy - ry);
                if r < 1.2 {
                    return iron.pick(if r < 0.6 && fx < rx + 0.1 { 0.95 } else { 0.7 }, x, y);
                }
            }
            let lit = if fx.min(fy) < 1.0 {
                0.75
            } else if (SIZEF - fx).min(SIZEF - fy) < 1.0 {
                0.15
            } else {
                0.42
            };
            return iron.pick(lit, x, y);
        }
        // Thin bars down the glass.
        if (fx - 9.5).abs() < 0.5 || (fx - 22.5).abs() < 0.5 {
            return iron.pick(0.5, x, y);
        }
        // The flame: a teardrop above the wick.
        let (dx, dy) = (fx - 16.0, fy - 15.0);
        let w = if dy > 0.0 {
            3.2 * (1.0 - dy / 4.5).max(0.0)
        } else {
            3.2 * (1.0 + dy / 7.0).max(0.0)
        };
        if dx.abs() < w && dy < 4.5 {
            let core = 1.0 - dx.abs() / w.max(0.1);
            return flame.pick(0.45 + core * 0.45 - dy.min(0.0) / 14.0, x, y);
        }
        // The tallow stub and its wick.
        if dx.abs() < 2.5 && (20.5..26.0).contains(&fy) {
            return tallow.pick(0.6 - dx / 5.0, x, y);
        }
        // The panes, lit by the flame.
        let r = dx.hypot((fy - 16.0) * 0.8);
        glow.pick(0.95 - r / 16.0, x, y)
    })
}

// ------------------------------------------------------------------ icons

/// Stones of a rock: the stones icon's heap through that rock's ramp.
fn stones_of(r: Rock) -> Tex {
    let salt = seed("stones");
    let ramp = r.ramp();
    let mut icon = Icon::new();
    for (i, &(x, y, rad)) in [
        (10.5, 21.5, 6.6),
        (22.0, 22.0, 6.2),
        (16.0, 13.0, 6.0),
        (16.5, 25.5, 4.8),
    ]
    .iter()
    .enumerate()
    {
        Lump::new(hash2(salt, i as u32), x, y, rad, 6).draw(&mut icon, &ramp, 0.3, 0.96, false);
    }
    icon.finish()
}

/// A brick mix: a heap of red clay and grey ash with chips of the rock
/// pressed into it.
fn brick_mix(r: Rock) -> Tex {
    let name = format!("{}_brick_mix", r.name());
    let salt = seed(&name);
    let clay = clay_ramp();
    let mud = mud_ramp();
    let rock = r.ramp();
    let mut icon = Icon::new();
    blob(&mut icon, (16.0, 20.0), (12.5, 8.5), 0.0, &clay, 0.08, false, salt);
    // Streaks of ash and mud folded into the clay.
    icon.layer(false, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let (dx, dy) = ((fx - 16.0) / 11.5, (fy - 20.5) / 7.5);
        if dx.hypot(dy) > 0.95 {
            return None;
        }
        let fold = (fy + (fx * 0.7).sin() * 1.5).rem_euclid(4.5);
        (fold < 1.0).then(|| mud.pick(0.7 - dx * 0.2 - dy * 0.2, x, y))
    });
    let mut rng = Rng::new(salt);
    for (i, &(x, y, rad)) in [
        (11.0, 16.0, 3.6),
        (20.0, 14.5, 3.2),
        (23.5, 21.0, 2.8),
        (13.5, 23.0, 2.6),
        (17.5, 19.0, 2.2),
    ]
    .iter()
    .enumerate()
    {
        let jx = rng.range(-0.5, 0.5);
        Lump::new(hash2(salt, i as u32), x + jx, y, rad, 5).draw(&mut icon, &rock, 0.3, 0.95, false);
    }
    icon.finish()
}

/// Wax: a round cake of pale yellow wax, a honeycomb stamped on its top, a
/// drip down its side.
fn wax() -> Tex {
    let ramp = soft_ramp(0xecc85a, 0.35, 0.98);
    let (cx, top, bottom, rx, ry) = (16.0f32, 13.0f32, 21.0f32, 11.5f32, 5.0f32);
    let mut icon = Icon::new();
    // The side: a band between two ellipses, lighter on the left.
    icon.layer(true, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let dx = (fx - cx) / rx;
        if dx.abs() > 1.0 {
            return None;
        }
        let half = ry * (1.0 - dx * dx).sqrt();
        if fy < top || fy > bottom + half {
            return None;
        }
        Some(ramp.pick(
            0.55 - dx * 0.25 + if fy > bottom + half - 1.0 { -0.1 } else { 0.0 },
            x,
            y,
        ))
    });
    // The top.
    icon.layer(false, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let (dx, dy) = ((fx - cx) / rx, (fy - top) / ry);
        if dx.hypot(dy) > 1.0 {
            return None;
        }
        // The stamped hexagon.
        let (hx, hy) = ((fx - cx).abs(), (fy - top).abs() * 2.0);
        let hex = hx.max((hx * 0.5 + hy * 0.866).abs());
        let t = if (hex - 5.0).abs() < 0.6 {
            0.55
        } else if (hex - 5.0).abs() < 1.1 && fy > top {
            0.95
        } else {
            0.82 - dx * 0.1
        };
        Some(ramp.pick(t, x, y))
    });
    // A drip down the side.
    icon.layer(false, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let r = if fy > 20.0 {
            (fx - 20.0).hypot(fy - 20.5)
        } else {
            (fx - 20.0).abs() + 0.0
        };
        (fy > top + 3.0 && r < 1.3).then(|| ramp.pick(0.88, x, y))
    });
    icon.finish()
}

// ---------------------------------------------------------------- lookups

/// Every block texture this milestone adds, in catalogue order.
pub fn block_names() -> Vec<String> {
    let mut names = Vec::new();
    for r in Rock::ALL {
        let n = r.name();
        names.push(format!("{n}_bricks"));
        names.push(format!("refined_{n}"));
        names.push(format!("carved_{n}"));
        names.push(format!("tiled_{n}"));
    }
    for n in ["marble", "bordered_marble", "marble_frieze"] {
        names.push(n.to_string());
    }
    for w in Wood::ALL {
        let n = w.name();
        if w != Wood::Timber {
            names.push(n.to_string());
        }
        names.push(format!("refined_{n}"));
        names.push(format!("carved_{n}"));
    }
    for n in [
        "compact_soil",
        "compact_clay",
        "compact_peat",
        "compact_gravel",
        "refined_glow_crystal",
        "faceted_glow_crystal",
        "clear_glass",
        "framed_glass",
        "ornate_glass",
    ] {
        names.push(n.to_string());
    }
    for m in MetalBlock::ALL {
        for form in METAL_FORMS {
            names.push(format!("{form}_{}", m.name()));
        }
    }
    for g in Gem::ALL.into_iter().chain(Gem::ASTRAL) {
        names.push(format!("compact_{}", g.name()));
        names.push(format!("polished_{}", g.name()));
    }
    names.push("lantern".to_string());
    names
}

/// The item icons this milestone adds.
pub const ICONS: [&str; 6] = [
    "basalt_stones",
    "limestone_stones",
    "stone_brick_mix",
    "basalt_brick_mix",
    "limestone_brick_mix",
    "wax",
];

/// Paints one of this module's block textures, or `None`.
pub fn paint(name: &str) -> Option<Tex> {
    let ctx = Ctx::BASE;
    let fixed: Option<fn(Ctx) -> Tex> = match name {
        "marble" => Some(marble),
        "bordered_marble" => Some(bordered_marble),
        "marble_frieze" => Some(marble_frieze),
        "compact_soil" => Some(|_| pressed("compact_soil", &soil_ramp(), Rgba::hex(0xc8965f), 0.52)),
        "compact_clay" => Some(|_| pressed("compact_clay", &clay_ramp(), Rgba::hex(0xf0c08a), 0.52)),
        "compact_peat" => Some(|_| pressed("compact_peat", &peat_ramp(), Rgba::hex(0x9a7048), 0.55)),
        "compact_gravel" => Some(compact_gravel),
        "refined_glow_crystal" => Some(refined_glow_crystal),
        "faceted_glow_crystal" => Some(faceted_glow_crystal),
        "clear_glass" => Some(clear_glass),
        "framed_glass" => Some(framed_glass),
        "ornate_glass" => Some(ornate_glass),
        "lantern" => Some(lantern),
        _ => None,
    };
    if let Some(f) = fixed {
        return Some(f(ctx));
    }
    for r in Rock::ALL {
        let n = r.name();
        if name == format!("{n}_bricks") {
            return Some(rock_bricks(r));
        }
        for (form, f) in [
            ("refined", refined_rock as fn(Rock) -> Tex),
            ("carved", carved_rock),
            ("tiled", tiled_rock),
        ] {
            if name == format!("{form}_{n}") {
                return Some(f(r));
            }
        }
    }
    for w in Wood::ALL {
        let n = w.name();
        if name == n && w != Wood::Timber {
            return Some(other_timber(w));
        }
        if name == format!("refined_{n}") {
            return Some(refined_wood(w));
        }
        if name == format!("carved_{n}") {
            return Some(carved_wood(w));
        }
    }
    let (form, rest) = name.split_once('_')?;
    if let Some(m) = MetalBlock::ALL.into_iter().find(|m| m.name() == rest) {
        return Some(match form {
            "compact" => compact_metal(m),
            "refined" => refined_metal(m),
            "machined" => machined_metal(m),
            "circuit" => circuit_metal(m),
            "filigree" => filigree_metal(m),
            "grid" => grid_metal(m),
            _ => return None,
        });
    }
    let g = Gem::from_name(rest)?;
    match form {
        "compact" => Some(compact_gem(g)),
        "polished" => Some(polished_gem(g)),
        _ => None,
    }
}

/// Paints one of this module's icons, or `None`.
pub fn paint_icon(name: &str) -> Option<Tex> {
    Some(match name {
        "basalt_stones" => stones_of(Rock::Basalt),
        "limestone_stones" => stones_of(Rock::Limestone),
        "stone_brick_mix" => brick_mix(Rock::Stone),
        "basalt_brick_mix" => brick_mix(Rock::Basalt),
        "limestone_brick_mix" => brick_mix(Rock::Limestone),
        "wax" => wax(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(p: Rgba) -> f32 {
        0.2126 * p.r + 0.7152 * p.g + 0.0722 * p.b
    }

    /// Mean difference in light and shade between two faces, each taken
    /// about its own average (what the game keeps when it recolours one).
    fn pattern_difference(a: &Tex, b: &Tex) -> f32 {
        let ma = a.px.iter().map(|p| luma(*p)).sum::<f32>() / a.px.len() as f32;
        let mb = b.px.iter().map(|p| luma(*p)).sum::<f32>() / b.px.len() as f32;
        a.px.iter()
            .zip(&b.px)
            .map(|(p, q)| ((luma(*p) - ma) - (luma(*q) - mb)).abs())
            .sum::<f32>()
            / a.px.len() as f32
    }

    /// Every new texture and icon is painted, 32 x 32, and no two are alike.
    #[test]
    fn every_building_texture_and_icon_is_painted_and_distinct() {
        let names = block_names();
        let texs: Vec<Tex> = names
            .iter()
            .map(|n| crate::materials::paint_named(n, 0).unwrap_or_else(|| panic!("no painter for `{n}`")))
            .collect();
        for (n, t) in names.iter().zip(&texs) {
            assert_eq!(t.n, 32, "{n}");
        }
        for i in 0..texs.len() {
            for j in i + 1..texs.len() {
                assert_ne!(texs[i], texs[j], "`{}` and `{}` look the same", names[i], names[j]);
            }
        }
        let icons: Vec<Tex> = ICONS
            .iter()
            .map(|n| crate::icons::paint_icon(n).unwrap_or_else(|| panic!("no icon for `{n}`")))
            .collect();
        for i in 0..icons.len() {
            for j in i + 1..icons.len() {
                assert_ne!(icons[i], icons[j], "`{}` and `{}` look the same", ICONS[i], ICONS[j]);
            }
        }
        // And the other rocks' stones are not stone's.
        let stones = crate::icons::paint_icon("stones").unwrap();
        assert_ne!(icons[0], stones);
        assert_ne!(icons[1], stones);
    }

    /// Within a rock, the four kinds differ in shape (light and shade, not
    /// just colour); across rocks, one kind differs in colour from afar.
    #[test]
    fn rock_kinds_differ_in_shape_and_rocks_in_colour() {
        let kinds = ["{}_bricks", "refined_{}", "carved_{}", "tiled_{}"];
        for r in Rock::ALL {
            let texs: Vec<Tex> = kinds
                .iter()
                .map(|k| paint(&k.replace("{}", r.name())).unwrap())
                .collect();
            for i in 0..texs.len() {
                for j in i + 1..texs.len() {
                    let d = pattern_difference(&texs[i], &texs[j]);
                    assert!(d > 0.03, "{} {} vs {}: {d}", r.name(), kinds[i], kinds[j]);
                }
            }
        }
        for k in kinds {
            let means: Vec<Rgba> = Rock::ALL
                .iter()
                .map(|r| paint(&k.replace("{}", r.name())).unwrap().mean())
                .collect();
            for i in 0..means.len() {
                for j in i + 1..means.len() {
                    let (a, b) = (means[i], means[j]);
                    let d = (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs();
                    assert!(
                        d > 0.1,
                        "{k}: {:?} vs {:?} look alike from afar ({d})",
                        Rock::ALL[i],
                        Rock::ALL[j]
                    );
                }
            }
        }
    }

    /// Each metal's six forms differ in shape, and each gem's two.
    #[test]
    fn metal_and_gem_forms_differ_in_shape() {
        for m in MetalBlock::ALL {
            let texs: Vec<Tex> = METAL_FORMS
                .iter()
                .map(|f| paint(&format!("{f}_{}", m.name())).unwrap())
                .collect();
            for i in 0..texs.len() {
                for j in i + 1..texs.len() {
                    let d = pattern_difference(&texs[i], &texs[j]);
                    assert!(d > 0.03, "{}: {} vs {}: {d}", m.name(), METAL_FORMS[i], METAL_FORMS[j]);
                }
            }
        }
        for g in Gem::ALL.into_iter().chain(Gem::ASTRAL) {
            let a = paint(&format!("compact_{}", g.name())).unwrap();
            let b = paint(&format!("polished_{}", g.name())).unwrap();
            assert!(pattern_difference(&a, &b) > 0.03, "{}", g.name());
        }
    }

    /// The glasses carry their own alpha; every other new block is opaque.
    #[test]
    fn only_the_glasses_are_translucent() {
        for n in block_names() {
            let t = paint(&n).unwrap();
            let clear = t.px.iter().any(|p| p.a < 1.0);
            assert_eq!(clear, n.ends_with("_glass"), "{n}");
        }
    }
}
