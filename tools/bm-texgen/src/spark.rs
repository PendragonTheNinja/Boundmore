//! Spark (milestone 15): the machines that run on it, the generator that
//! makes it, the links that carry it, the iron furnace, the relic cache and
//! the icons of the metal chain (bar, compact, refined, machined), the cog,
//! the relic component, cotton cloth and the spark itself.
//!
//! Drawn like `stations` and `machines`: made things with frames, bands and
//! rivets, light from the top left. Every spark machine carries a small
//! copper socket with an ember glow at the foot of its side, where its link
//! meets it, so the family reads as one.

use std::f32::consts::{FRAC_1_SQRT_2, TAU};

use crate::color::{Ramp, Rgba};
use crate::gathered::ramp as soft_ramp;
use crate::icons::{Icon, Lump, P, blade, ingot, inside_poly, seg_dist};
use crate::machines::{
    Plate, crucible_side_in, crucible_top_in, machine_stone, relic_light_ramp, relic_plates, relic_ramp,
};
use crate::materials::{copper_ramp, steel_ramp, stone, stone_ramp};
use crate::noise::{fbm, hash2, seed, unit, value};
use crate::stations::{Mask, SOOT, cast_shadow, furnace_side_in, furnace_top_in, iron_ramp, masonry, plate, rivet};
use crate::tex::{Ctx, SIZE, Tex, c};

// ---------------------------------------------------------------- palettes

/// Silver: cool blue-grey shade to white.
pub(crate) fn silver_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x1c1e30, 0x30344a, 0x4a5068, 0x6a7290, 0x8e98b2, 0xb4bed2, 0xdae0ec, 0xffffff,
    ])
}

/// Silver alloy: silver with a lilac cast, so the alloy reads apart from
/// the bar it is made from.
pub(crate) fn alloy_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x221a34, 0x3a2e52, 0x56487a, 0x7668a0, 0x9a8ec0, 0xc0b8da, 0xe4def2, 0xffffff,
    ])
}

/// Dark iron plate: the iron furnace and the spark generator.
pub(crate) fn dark_iron_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x0e0d16, 0x191924, 0x252634, 0x343646, 0x464a5c, 0x5c6276, 0x7a8296, 0xa4acbe,
    ])
}

/// Spark: an ember red through gold to a white-hot heart.
pub(crate) fn spark_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x4a1612, 0x82281a, 0xbc4a1c, 0xe8761e, 0xffa22e, 0xffc84e, 0xffe68e, 0xfffbe6,
    ])
}

/// The refinery's stone: a warm red-brown.
fn refinery_stone() -> Ramp {
    Ramp::from_hex(&[
        0x24161a, 0x3a2226, 0x543430, 0x6e4840, 0x8a5e52, 0xa67a6a, 0xc49a88, 0xe0c0ae,
    ])
}

/// The compactor's stone: a heavy blue-violet.
fn compactor_stone() -> Ramp {
    Ramp::from_hex(&[
        0x161828, 0x22263c, 0x323852, 0x464e6a, 0x5c6684, 0x76809e, 0x98a2bc, 0xc0c8da,
    ])
}

// ----------------------------------------------------------------- helpers

/// A stone rim in four quarters round a feature in the middle (the top of
/// a four-block machine), lit on its outer top-left edges.
pub(crate) fn quarters(salt: u32, stone: &Ramp, x: usize, y: usize) -> Rgba {
    let (fx, fy) = (c(x), c(y));
    if x == 15 || y == 15 {
        return stone.at(0);
    }
    let quarter = (fx > 16.0) as u32 + 2 * (fy > 16.0) as u32;
    let mut t = 0.52 + (unit(hash2(salt, quarter)) - 0.5) * 0.12;
    t += (fbm(hash2(salt, 7 + quarter), 8, 8, 2, fx, fy) - 0.5) * 0.18;
    if x == 0 || y == 0 || x == 16 || y == 16 {
        t += 0.22;
    }
    if x == SIZE - 1 || y == SIZE - 1 || x == 14 || y == 14 {
        t -= 0.24;
    }
    stone.pick(t, x, y)
}

/// A machine's side: a metal band along the top (rows 0..4), coursed
/// stone, and a plinth (rows 28..32) with the spark socket in its middle.
pub(crate) fn machine_side(salt: u32, stone: &Ramp, band: &Ramp) -> Tex {
    let mut tex = Tex::from_fn(|x, y| {
        if y < 4 {
            let t = 0.55 + [0.3, 0.1, 0.0, -0.3][y] + (fbm(salt ^ 3, 4, 2, 2, c(x), c(y)) - 0.5) * 0.12;
            return band.pick(t, x, y);
        }
        if y >= 28 {
            let t = 0.5 + [0.24, 0.04, 0.0, -0.24][y - 28];
            return stone.pick(t, x, y);
        }
        masonry(salt, stone, x, y)
    });
    let iron = iron_ramp();
    for x in (3..SIZE).step_by(7) {
        rivet(&mut tex, &iron, x, 2);
    }
    spark_socket(&mut tex);
    tex
}

/// The spark socket at the foot of a machine's side: a copper collar with
/// an ember glow inside, where a link meets it.
fn spark_socket(tex: &mut Tex) {
    let copper = copper_ramp();
    let spark = spark_ramp();
    let mut parts = Mask::new();
    for y in 27..32 {
        for x in 12..20 {
            let (lx, ly) = (x - 12, y - 27);
            let col = if lx == 0 || ly == 0 {
                copper.at(6)
            } else if lx == 7 || ly == 4 {
                copper.at(1)
            } else if (2..6).contains(&lx) && (1..4).contains(&ly) {
                spark.pick(0.95 - ((c(x) - 16.0).abs() + (c(y) - 29.5).abs()) * 0.14, x, y)
            } else {
                copper.at(4)
            };
            tex.set(x, y, col);
            parts.set(x, y);
        }
    }
    cast_shadow(tex, &parts, SOOT, 0.4);
}

/// Spark light spilling onto a face round `(cx, cy)` out to `reach`.
fn spill(tex: &mut Tex, cx: f32, cy: f32, reach: f32, k: f32, skip: impl Fn(usize, usize) -> bool) {
    let glow = spark_ramp().at(4);
    for y in 0..SIZE {
        for x in 0..SIZE {
            if skip(x, y) {
                continue;
            }
            let d = (c(x) - cx).hypot(c(y) - cy);
            let w = (1.0 - d / reach).clamp(0.0, 1.0).powi(2) * k;
            if w > 0.01 {
                tex.set(x, y, tex.get(x, y).mix(glow, w));
            }
        }
    }
}

// ------------------------------------------------------------------- ores

/// Relic cache: a richer cousin of the relic seam, its plates bigger and
/// closer, two of them still holding a round core of cyan light in a
/// bronze ring.
pub fn relic_cache(ctx: Ctx) -> Tex {
    let plates: [Plate; 5] = [
        (8.0, 8.5, 6.0, 4.0, 0.25, true),
        (23.5, 7.5, 4.0, 5.0, -0.3, false),
        (16.0, 19.0, 7.5, 4.2, -0.05, true),
        (5.5, 27.0, 3.8, 2.8, 0.5, false),
        (26.0, 26.5, 4.5, 3.2, -0.4, true),
    ];
    let mut tex = relic_plates(seed("relic_cache"), stone(ctx), &plates);
    let bronze = relic_ramp();
    let light = relic_light_ramp();
    for &(cx, cy, r) in &[(16.0f32, 19.0f32, 3.8f32), (23.5, 7.5, 2.8)] {
        for y in 0..SIZE {
            for x in 0..SIZE {
                let (dx, dy) = (c(x) - cx, c(y) - cy);
                let d = dx.hypot(dy);
                if d > r + 1.6 {
                    continue;
                }
                let col = if d > r {
                    // Its faint light on the plate round it.
                    tex.get(x, y).mix(light.at(2), 0.35 * (1.0 - (d - r) / 1.6))
                } else if d > r - 1.2 {
                    bronze.pick(0.55 - (dx + dy) / d.max(0.1) * 0.32, x, y)
                } else if (dx + r * 0.35).hypot(dy + r * 0.35) < 0.9 {
                    light.lightest()
                } else {
                    light.pick(0.95 - d / r * 0.55, x, y)
                };
                tex.set(x, y, col);
            }
        }
    }
    tex
}

// ---------------------------------------------------------------- the mixer

/// Mixer top: a stone rim round a copper-lipped bowl of molten metal
/// swirling round an iron paddle, two blades crossed on a hub.
pub fn mixer_top(_ctx: Ctx) -> Tex {
    let salt = seed("mixer_top");
    let stone = machine_stone();
    let copper = copper_ramp();
    let iron = iron_ramp();
    let melt = alloy_ramp();
    let mut tex = Tex::from_fn(|x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
        let r = dx.hypot(dy);
        if r < 10.0 {
            // A spiral of brighter metal drawn round by the paddle.
            let a = dy.atan2(dx);
            let swirl = (a * 2.0 + r * 0.55).sin();
            let t = 0.62 + swirl * 0.18 - r / 10.0 * 0.22 + (value(salt, 8, 8, c(x), c(y)) - 0.5) * 0.1;
            return melt.pick(t, x, y);
        }
        if r < 12.0 {
            let lit = -(dx + dy) / r;
            let t = 0.55 + if r < 11.0 { -lit * 0.25 } else { lit * 0.3 };
            return copper.pick(t, x, y);
        }
        let near = if r < 13.0 { 0.2 } else { 0.0 };
        quarters(salt, &stone, x, y).mix(stone.at(0), near)
    });
    // The paddle: two iron blades crossed through a hub.
    let mut parts = Mask::new();
    let (s, co) = 0.45f32.sin_cos();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
            let (u, v) = (dx * co + dy * s, -dx * s + dy * co);
            let r = dx.hypot(dy);
            let hub = r < 2.6;
            let blade = r < 8.6 && (u.abs() < 1.1 || v.abs() < 1.1);
            if !hub && !blade {
                continue;
            }
            let t = if hub {
                0.8 - (dx + dy) / 2.6 * 0.25
            } else if u.abs() < 1.1 {
                0.58 - u * 0.2
            } else {
                0.58 - v * 0.2
            };
            tex.set(x, y, iron.pick(t, x, y));
            parts.set(x, y);
        }
    }
    cast_shadow(&mut tex, &parts, SOOT, 0.4);
    tex.set(15, 15, iron.lightest());
    tex
}

/// Mixer side: dressed stone under a copper band, a big iron cog turning
/// on a copper hub in the middle, the spark socket at its foot.
pub fn mixer_side(_ctx: Ctx) -> Tex {
    let salt = seed("mixer_side");
    let mut tex = machine_side(salt, &machine_stone(), &copper_ramp());
    let iron = steel_ramp();
    let copper = copper_ramp();
    let mut parts = Mask::new();
    let (cx, cy) = (16.0f32, 15.5f32);
    for y in 4..27 {
        for x in 3..29 {
            let (dx, dy) = (c(x) - cx, c(y) - cy);
            let r = dx.hypot(dy);
            let a = dy.atan2(dx) + 0.2;
            let tooth = (a / TAU * 8.0).rem_euclid(1.0) < 0.45;
            let outer = if tooth { 10.0 } else { 8.2 };
            if r > outer + 0.9 {
                continue;
            }
            if r > outer {
                tex.set(x, y, tex.get(x, y).mix(SOOT, 0.7));
                continue;
            }
            let lit = -(dx + dy) / r.max(0.1);
            let col = if r < 1.6 {
                SOOT
            } else if r < 4.0 {
                copper.pick(0.6 + lit * 0.3, x, y)
            } else if (5.6..6.6).contains(&r) {
                iron.pick(0.3 - lit * 0.1, x, y)
            } else if r > outer - 1.0 {
                iron.pick(0.7 + lit * 0.25, x, y)
            } else {
                iron.pick(0.62 + lit * 0.08, x, y)
            };
            tex.set(x, y, col);
            parts.set(x, y);
        }
    }
    cast_shadow(&mut tex, &parts, SOOT, 0.45);
    tex
}

// ------------------------------------------------------------- the refinery

/// Refinery top: warm stone round an iron-ringed crucible of white-hot
/// metal, fed by four copper pipes from the corners.
pub fn refinery_top(_ctx: Ctx) -> Tex {
    let salt = seed("refinery_top");
    let stone = refinery_stone();
    let copper = copper_ramp();
    let iron = iron_ramp();
    let spark = spark_ramp();
    let mut tex = Tex::from_fn(|x, y| quarters(salt, &stone, x, y));
    let mut parts = Mask::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
            let r = dx.hypot(dy);
            // The pipes along the diagonals, round in section.
            let (a, b) = ((dx - dy) * FRAC_1_SQRT_2, (dx + dy) * FRAC_1_SQRT_2);
            let across = a.abs().min(b.abs());
            if (8.5..19.0).contains(&r) && across < 1.9 {
                let side = if a.abs() < b.abs() { a } else { b };
                let joint = (r - 13.0).abs() < 0.7;
                let t = 0.55 - side * 0.18 - if joint { 0.25 } else { 0.0 };
                tex.set(x, y, copper.pick(t, x, y));
                parts.set(x, y);
            }
            if r < 9.0 {
                let lit = -(dx + dy) / r.max(0.1);
                let col = if r > 7.0 {
                    iron.pick(0.52 + lit * 0.3, x, y)
                } else {
                    let flick = value(salt ^ 5, 8, 8, c(x), c(y));
                    spark.pick(1.0 - r / 7.0 * 0.5 + (flick - 0.5) * 0.15, x, y)
                };
                tex.set(x, y, col);
                parts.set(x, y);
            }
        }
    }
    cast_shadow(&mut tex, &parts, SOOT, 0.4);
    spill(&mut tex, 16.0, 16.0, 13.0, 0.3, |x, y| parts.get(x as i32, y as i32));
    tex
}

/// Refinery side: warm stone under an iron band, a copper pipe down each
/// side, a pressure gauge with its needle up, and a barred window on the
/// glow inside; the spark socket at its foot.
pub fn refinery_side(_ctx: Ctx) -> Tex {
    let salt = seed("refinery_side");
    let mut tex = machine_side(salt, &refinery_stone(), &iron_ramp());
    let copper = copper_ramp();
    let iron = iron_ramp();
    let spark = spark_ramp();
    let mut parts = Mask::new();
    for y in 4..28 {
        for (x0, i) in [(3usize, 0usize), (25, 1)] {
            for x in x0..x0 + 4 {
                let t = [0.85, 0.65, 0.45, 0.25][x - x0] - if (y + i * 4) % 8 == 5 { 0.22 } else { 0.0 };
                tex.set(x, y, copper.pick(t, x, y));
                parts.set(x, y);
            }
        }
    }
    // The gauge: an iron rim, a pale face with ticks, a red needle.
    let (gx, gy) = (16.0f32, 11.5f32);
    for y in 5..19 {
        for x in 9..24 {
            let (dx, dy) = (c(x) - gx, c(y) - gy);
            let r = dx.hypot(dy);
            if r > 5.6 {
                continue;
            }
            let col = if r > 4.4 {
                iron.pick(0.55 - (dx + dy) / r * 0.3, x, y)
            } else {
                let face = Rgba::hex(0xe8e0cc).mix(Rgba::hex(0x8a8070), ((dx + dy) / 8.0 + 0.3).clamp(0.0, 0.6));
                let a = dy.atan2(dx);
                if r > 3.2 && (a / TAU * 8.0).rem_euclid(1.0) < 0.2 && dy < 1.0 {
                    Rgba::hex(0x2a2430)
                } else {
                    face
                }
            };
            tex.set(x, y, col);
            parts.set(x, y);
        }
    }
    for i in 0..4 {
        let (x, y) = ((gx + 0.9 * i as f32) as usize, (gy - 0.9 * i as f32) as usize);
        tex.set(x, y, Rgba::hex(0xc8281e));
    }
    tex.set(16, 11, Rgba::hex(0x2a2430));
    // The window: an iron frame round the glow, two bars across it.
    for y in 19..26 {
        for x in 10..22 {
            let (lx, ly) = (x - 10, y - 19);
            let col = if lx == 0 || ly == 0 {
                iron.at(6)
            } else if lx == 11 || ly == 6 {
                iron.at(1)
            } else if lx % 4 == 0 {
                iron.pick(0.5, x, y)
            } else {
                let flick = value(salt ^ 9, 8, 4, c(x), c(y));
                spark.pick(0.55 + ly as f32 * 0.07 + (flick - 0.5) * 0.2, x, y)
            };
            tex.set(x, y, col);
            parts.set(x, y);
        }
    }
    cast_shadow(&mut tex, &parts, SOOT, 0.45);
    tex
}

// ------------------------------------------------------------ the compactor

/// Compactor top: a heavy iron press plate in a stone rim, its square ram
/// cut with a cross of grooves and bolted at the corners.
pub fn compactor_top(_ctx: Ctx) -> Tex {
    let salt = seed("compactor_top");
    let stone = compactor_stone();
    let iron = iron_ramp();
    let dark = dark_iron_ramp();
    let mut tex = Tex::from_fn(|x, y| quarters(salt, &stone, x, y));
    let mut parts = Mask::new();
    plate(&mut tex, &mut parts, &iron, 4, 4, 28, 28, 0.45);
    let mut ram = Mask::new();
    plate(&mut tex, &mut ram, &dark, 9, 9, 23, 23, 0.6);
    for y in 10..22 {
        for x in 10..22 {
            let (lx, ly) = (x as i32 - 10, y as i32 - 10);
            if lx == ly || lx == 11 - ly {
                tex.set(x, y, dark.at(1));
            } else if lx == ly + 1 || lx == 12 - ly {
                tex.set(x, y, dark.at(5));
            }
        }
    }
    for (x, y) in [(6, 6), (25, 6), (6, 25), (25, 25)] {
        tex.set(x, y, iron.at(7));
        tex.set(x + 1, y, iron.at(4));
        tex.set(x, y + 1, iron.at(4));
        tex.set(x + 1, y + 1, iron.at(1));
    }
    cast_shadow(&mut tex, &ram, SOOT, 0.35);
    cast_shadow(&mut tex, &parts, SOOT, 0.45);
    tex
}

/// Compactor side: blue stone under an iron band, a thick piston up each
/// side, a press plate above a squashed copper block on an anvil between
/// them; the spark socket at its foot.
pub fn compactor_side(_ctx: Ctx) -> Tex {
    let salt = seed("compactor_side");
    let mut tex = machine_side(salt, &compactor_stone(), &iron_ramp());
    let iron = iron_ramp();
    let dark = dark_iron_ramp();
    let copper = copper_ramp();
    let mut parts = Mask::new();
    for y in 4..27 {
        for x0 in [3usize, 24] {
            for x in x0..x0 + 5 {
                let mut t = [0.82, 0.66, 0.5, 0.36, 0.2][x - x0];
                if y == 15 || y == 16 {
                    t -= 0.25;
                }
                tex.set(
                    x,
                    y,
                    if y < 15 {
                        iron.pick(t, x, y)
                    } else {
                        dark.pick(t + 0.1, x, y)
                    },
                );
                parts.set(x, y);
            }
        }
    }
    plate(&mut tex, &mut parts, &iron, 8, 6, 24, 11, 0.6);
    plate(&mut tex, &mut parts, &dark, 8, 21, 24, 26, 0.6);
    // The squashed block: copper, pressed flat, its edges bulging.
    for y in 13..21 {
        for x in 10..22 {
            let bulge = (y == 13 || y == 20) && (x == 10 || x == 21);
            if bulge {
                continue;
            }
            let mut t = 0.55 - (y as f32 - 13.0) / 8.0 * 0.25;
            if y == 13 || x == 10 {
                t += 0.25;
            }
            if y == 20 || x == 21 {
                t -= 0.25;
            }
            if y == 16 {
                t -= 0.18;
            }
            tex.set(x, y, copper.pick(t, x, y));
            parts.set(x, y);
        }
    }
    cast_shadow(&mut tex, &parts, SOOT, 0.45);
    tex
}

// ---------------------------------------------------------- the iron furnace

/// Riveted iron plates in staggered courses, for the iron furnace's walls
/// (rows 4..28, like `masonry`).
fn iron_plates(salt: u32, iron: &Ramp, x: usize, y: usize) -> Rgba {
    let row = (y - 4) / 6;
    let ly = (y - 4) % 6;
    let lx = (x + if row % 2 == 1 { 8 } else { 0 }) % 16;
    let id = hash2(salt, (row * 2 + (x + if row % 2 == 1 { 8 } else { 0 }) / 16 % 2) as u32);
    if ly == 5 || lx == 15 {
        return iron.at(0);
    }
    if ly == 2 && (lx == 2 || lx == 12) {
        return iron.at(7);
    }
    if ly == 2 && (lx == 3 || lx == 13) {
        return iron.at(1);
    }
    let mut t = 0.45 + (unit(id) - 0.5) * 0.12 + (fbm(id, 4, 4, 2, c(x), c(y)) - 0.5) * 0.12;
    if ly == 0 {
        t += 0.22;
    }
    if lx == 0 {
        t += 0.14;
    }
    if ly == 4 {
        t -= 0.2;
    }
    if lx == 14 {
        t -= 0.18;
    }
    iron.pick(t, x, y)
}

/// Iron furnace top: the stone furnace's chimney and grate in slabs of dark
/// iron, riveted at their corners.
pub fn iron_furnace_top(_ctx: Ctx) -> Tex {
    let dark = dark_iron_ramp();
    let mut tex = furnace_top_in(seed("iron_furnace_top"), &dark);
    let iron = iron_ramp();
    for (x, y) in [
        (2, 2),
        (18, 2),
        (24, 2),
        (28, 18),
        (2, 13),
        (2, 28),
        (13, 28),
        (28, 28),
        (24, 24),
    ] {
        rivet(&mut tex, &iron, x, y);
    }
    tex
}

/// Iron furnace side: the stone furnace's arched mouth and fire, in walls
/// of riveted dark iron plate.
pub fn iron_furnace_side(_ctx: Ctx) -> Tex {
    let salt = seed("iron_furnace_side");
    let dark = dark_iron_ramp();
    let mut tex = furnace_side_in(salt, &dark, &|x, y| iron_plates(salt, &dark, x, y));
    let iron = iron_ramp();
    for x in [3usize, 16, 27] {
        rivet(&mut tex, &iron, x, 1);
    }
    tex
}

/// The iron furnace's crucible: an iron pot in dark iron hoops on a dark
/// iron course.
pub fn iron_furnace_crucible_top(_ctx: Ctx) -> Tex {
    crucible_top_in(seed("iron_furnace_crucible_top"), &dark_iron_ramp(), &iron_ramp())
}

pub fn iron_furnace_crucible_side(_ctx: Ctx) -> Tex {
    crucible_side_in(
        seed("iron_furnace_crucible_side"),
        &dark_iron_ramp(),
        &iron_ramp(),
        &dark_iron_ramp(),
    )
}

// ---------------------------------------------------------- spark generator

/// Spark generator top: a dark iron box lid, bolted at its corners, round a
/// glowing ember-gold core in an iron ring.
pub fn spark_generator_top(_ctx: Ctx) -> Tex {
    let salt = seed("spark_generator_top");
    let dark = dark_iron_ramp();
    let iron = iron_ramp();
    let spark = spark_ramp();
    let mut tex = Tex::from_fn(|x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
        let r = dx.hypot(dy);
        if r < 7.0 {
            let flick = value(salt, 8, 8, c(x), c(y));
            let t = 1.02 - r / 7.0 * 0.6 + (flick - 0.5) * 0.18;
            return spark.pick(t, x, y);
        }
        if r < 9.0 {
            let lit = -(dx + dy) / r;
            let t = 0.5 + if r < 8.0 { -lit * 0.25 } else { lit * 0.3 };
            return iron.pick(t, x, y);
        }
        let mut t = 0.5 + (fbm(salt ^ 2, 8, 8, 2, c(x), c(y)) - 0.5) * 0.12;
        if x <= 1 || y <= 1 {
            t += 0.24;
        }
        if x >= SIZE - 2 || y >= SIZE - 2 {
            t -= 0.24;
        }
        if x == 3 || y == 3 {
            t -= 0.2;
        }
        if x == SIZE - 4 || y == SIZE - 4 {
            t += 0.14;
        }
        dark.pick(t, x, y)
    });
    for (x, y) in [(5, 5), (25, 5), (5, 25), (25, 25)] {
        tex.set(x, y, iron.at(7));
        tex.set(x + 1, y, iron.at(4));
        tex.set(x, y + 1, iron.at(4));
        tex.set(x + 1, y + 1, iron.at(1));
    }
    spill(&mut tex, 16.0, 16.0, 13.0, 0.35, |x, y| {
        (c(x) - 16.0).hypot(c(y) - 16.0) < 9.0
    });
    tex
}

/// Spark generator side: dark iron panels, a tall barred window on the
/// glowing core, glowing vents low down and two copper terminals where
/// links fasten.
pub fn spark_generator_side(_ctx: Ctx) -> Tex {
    let salt = seed("spark_generator_side");
    let dark = dark_iron_ramp();
    let iron = iron_ramp();
    let spark = spark_ramp();
    let copper = copper_ramp();
    let mut tex = Tex::from_fn(|x, y| {
        if y < 3 {
            return iron.pick(0.45 + [0.3, 0.05, -0.25][y], x, y);
        }
        if y >= 29 {
            return dark.pick(0.5 + [0.2, 0.0, -0.25][y - 29], x, y);
        }
        let mut t = 0.48 + (fbm(salt, 8, 8, 2, c(x), c(y)) - 0.5) * 0.12;
        if x == 0 {
            t += 0.2;
        }
        if x == SIZE - 1 {
            t -= 0.24;
        }
        if y == 3 {
            t -= 0.2;
        }
        dark.pick(t, x, y)
    });
    for x in [2usize, 9, 21, 28] {
        rivet(&mut tex, &iron, x, 1);
    }
    let mut parts = Mask::new();
    // The window.
    for y in 6..24 {
        for x in 11..21 {
            let (lx, ly) = (x - 11, y - 6);
            let col = if lx == 0 || ly == 0 {
                iron.at(6)
            } else if lx == 9 || ly == 17 {
                iron.at(1)
            } else if ly % 6 == 0 {
                iron.pick(0.45, x, y)
            } else {
                let d = (c(x) - 16.0).abs() / 4.0;
                let flick = value(salt ^ 4, 4, 8, c(x), c(y));
                spark.pick(
                    0.98 - d * 0.4 - (ly as f32 / 17.0 - 0.6).abs() * 0.3 + (flick - 0.5) * 0.18,
                    x,
                    y,
                )
            };
            tex.set(x, y, col);
            parts.set(x, y);
        }
    }
    // Vents: three slits each side, faintly lit from inside.
    for y in [22usize, 24, 26] {
        for x in (3..8).chain(24..29) {
            tex.set(x, y, spark.at(1).mix(SOOT, 0.3));
            tex.set(x, y + 1, dark.at(5));
        }
    }
    // The terminals: copper studs on iron bosses.
    for (cx, cy) in [(5.5f32, 9.5f32), (26.5, 9.5)] {
        for y in 6..14 {
            for x in 2..30 {
                let (dx, dy) = (c(x) - cx, c(y) - cy);
                let r = dx.hypot(dy);
                if r > 3.2 {
                    continue;
                }
                let lit = -(dx + dy) / r.max(0.1);
                let col = if r > 2.0 {
                    iron.pick(0.45 + lit * 0.3, x, y)
                } else {
                    copper.pick(0.6 + lit * 0.3, x, y)
                };
                tex.set(x, y, col);
                parts.set(x, y);
            }
        }
    }
    cast_shadow(&mut tex, &parts, SOOT, 0.45);
    spill(&mut tex, 16.0, 15.0, 11.0, 0.25, |x, y| parts.get(x as i32, y as i32));
    tex
}

// --------------------------------------------------------------- spark link

/// Spark link: two twisted copper cables crossing on a dark ground, joined
/// at a copper boss with a spark of light in it. They meet the middle of
/// every edge, so links in a row read as one line.
pub fn spark_link(_ctx: Ctx) -> Tex {
    let salt = seed("spark_link");
    let dark = dark_iron_ramp();
    let copper = copper_ramp();
    let spark = spark_ramp();
    // A twisted cable along `along` with `across` the offset from its axis.
    let cable = |along: f32, across: f32, x: usize, y: usize| -> Option<Rgba> {
        if across.abs() > 3.6 {
            return None;
        }
        if across.abs() > 2.9 {
            return Some(Rgba::hex(0x14101a));
        }
        let twist = ((along + across * 1.1) / 8.0 * TAU).cos();
        let mut t = 0.58 + twist * 0.22 - across / 3.0 * 0.16;
        if twist < -0.75 {
            t = 0.12;
        }
        Some(copper.pick(t, x, y))
    };
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let (dx, dy) = (fx - 16.0, fy - 16.0);
        let r = dx.hypot(dy);
        if r < 5.6 {
            if r < 1.8 {
                return spark.pick(1.0 - r * 0.25, x, y);
            }
            if r < 2.8 {
                return dark.at(1);
            }
            let lit = -(dx + dy) / r;
            return copper.pick(0.55 + lit * 0.3 + if r > 4.8 { -0.15 } else { 0.0 }, x, y);
        }
        // The horizontal cable over the vertical one.
        if let Some(col) = cable(fx, dy, x, y) {
            return col;
        }
        if let Some(col) = cable(fy, dx, x, y) {
            return col;
        }
        let t = 0.32 + (fbm(salt, 8, 8, 2, fx, fy) - 0.5) * 0.14;
        // Quilted insulation: a soft diamond stitch.
        let stitch = ((fx + fy) as i32 % 8 == 0 || (fx - fy + 64.0) as i32 % 8 == 0) as u8 as f32;
        dark.pick(t - stitch * 0.1, x, y)
    })
}

// ------------------------------------------------------------------ icons

fn silver_ore() -> Tex {
    let salt = seed("silver_ore");
    let mut icon = Icon::new();
    Lump::new(salt, 16.0, 17.0, 12.2, 8).draw(&mut icon, &stone_ramp(), 0.12, 0.8, false);
    let silver = silver_ramp();
    for (i, &(x, y, r)) in [
        (12.0, 12.5, 3.4),
        (20.0, 12.0, 2.6),
        (18.0, 20.5, 3.6),
        (10.0, 21.0, 2.4),
        (23.5, 18.0, 1.9),
    ]
    .iter()
    .enumerate()
    {
        Lump::new(hash2(salt, 10 + i as u32), x, y, r, 5).draw(&mut icon, &silver, 0.3, 0.98, true);
    }
    icon.finish()
}

/// Relic component: an octagonal bronze housing with a cyan lens in its
/// middle, grooves of light running out from it and four studs.
fn relic_component() -> Tex {
    let bronze = relic_ramp();
    let light = relic_light_ramp();
    let oct: Vec<P> = (0..8)
        .map(|i| {
            let a = (i as f32 + 0.5) / 8.0 * TAU;
            (16.0 + a.cos() * 13.0, 16.0 + a.sin() * 13.0)
        })
        .collect();
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&oct, p.0, p.1) {
            return None;
        }
        let (dx, dy) = (p.0 - 16.0, p.1 - 16.0);
        let r = dx.hypot(dy);
        let edge = (0..8)
            .map(|i| seg_dist(oct[i], oct[(i + 1) % 8], p))
            .fold(f32::MAX, f32::min);
        let lit = -(dx + dy) / r.max(0.1);
        if edge < 1.6 {
            return Some(bronze.pick(0.55 + lit * 0.35, x, y));
        }
        if r < 5.0 {
            if (dx + 1.6).hypot(dy + 1.6) < 1.2 {
                return Some(light.lightest());
            }
            return Some(light.pick(0.95 - r / 5.0 * 0.5, x, y));
        }
        if r < 6.6 {
            return Some(bronze.pick(0.45 - lit * 0.3, x, y));
        }
        if dx.abs() < 0.9 || dy.abs() < 0.9 {
            return Some(light.pick(0.6, x, y));
        }
        for (sx, sy) in [(-6.5f32, -6.5f32), (6.5, -6.5), (-6.5, 6.5), (6.5, 6.5)] {
            let d = (dx - sx).hypot(dy - sy);
            if d < 1.7 {
                return Some(bronze.pick(if dx + dy < sx + sy { 0.95 } else { 0.3 }, x, y));
            }
        }
        let mut col = bronze.pick(0.5 - (dx + dy) / 26.0 * 0.3, x, y);
        if value(seed("relic_component"), 4, 4, p.0, p.1) > 0.7 {
            col = col.mix(Rgba::hex(0x5aa08a), 0.5);
        }
        Some(col)
    });
    icon.finish()
}

/// A cog: eight teeth round a face with four round holes and a hub.
fn cog() -> Tex {
    let copper = copper_ramp();
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
        let r = dx.hypot(dy);
        let a = dy.atan2(dx) + 0.2;
        let tooth = (a / TAU * 8.0).rem_euclid(1.0) < 0.45;
        let outer = if tooth { 14.0 } else { 11.2 };
        if r > outer || r < 3.0 {
            return None;
        }
        for i in 0..4 {
            let b = (i as f32 + 0.5) / 4.0 * TAU;
            if (dx - b.cos() * 7.2).hypot(dy - b.sin() * 7.2) < 2.0 {
                return None;
            }
        }
        let lit = -(dx + dy) / r;
        let t = if r > outer - 1.3 {
            0.55 + lit * 0.35
        } else if r < 4.4 {
            0.6 - lit * 0.3
        } else {
            0.55 + lit * 0.1
        };
        Some(copper.pick(t, x, y))
    });
    icon.finish()
}

/// Compact metal: a pressed cube in three-quarter view, a groove round its
/// middle and a square stamped in its top.
pub(crate) fn compact(ramp: Ramp) -> Tex {
    let (apex, l, r, ctr) = (
        (16.0f32, 3.5f32),
        (4.0f32, 10.0f32),
        (28.0f32, 10.0f32),
        (16.0f32, 16.5f32),
    );
    let (bl, br, b) = ((4.0f32, 23.0f32), (28.0f32, 23.0f32), (16.0f32, 29.5f32));
    let top = [apex, r, ctr, l];
    let left = [l, ctr, b, bl];
    let right = [ctr, r, br, b];
    let mid = |a: P, z: P| ((a.0 + z.0) * 0.5, (a.1 + z.1) * 0.5);
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (face, base) = if inside_poly(&top, p.0, p.1) {
            (0, 0.78)
        } else if inside_poly(&left, p.0, p.1) {
            (1, 0.52)
        } else if inside_poly(&right, p.0, p.1) {
            (2, 0.3)
        } else {
            return None;
        };
        let mut t = base;
        match face {
            0 => {
                if seg_dist(l, apex, p) < 0.9 || seg_dist(apex, r, p) < 0.9 {
                    t += 0.16;
                }
                // The stamped square: a smaller diamond, its far walls lit.
                let (dx, dy) = (p.0 - 16.0, (p.1 - 10.0) * 2.0);
                let d = dx.abs() + dy.abs();
                if (5.0..6.2).contains(&d) {
                    t += if dy > 0.0 { 0.14 } else { -0.2 };
                }
            }
            1 => {
                let g = seg_dist(mid(l, bl), mid(ctr, b), p);
                if g < 0.7 {
                    t -= 0.22;
                } else if g < 1.6 && p.1 > mid(l, bl).1 + (p.0 - 4.0) * 0.54 {
                    t += 0.1;
                }
                if seg_dist(l, ctr, p) < 0.9 {
                    t += 0.18;
                }
            }
            _ => {
                let g = seg_dist(mid(ctr, b), mid(r, br), p);
                if g < 0.7 {
                    t -= 0.2;
                }
                if seg_dist(ctr, r, p) < 0.9 {
                    t += 0.14;
                }
            }
        }
        Some(ramp.pick(t, x, y))
    });
    icon.finish()
}

/// Refined metal: a thick polished disc in three-quarter view, a sheen
/// across its face and a turned ring cut in it.
pub(crate) fn refined(ramp: Ramp) -> Tex {
    let (cx, cy, rx, ry, h) = (16.0f32, 12.0f32, 13.0f32, 6.5f32, 7.0f32);
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let (px, py) = (c(x), c(y));
        let u = (px - cx) / rx;
        if u.abs() > 1.0 {
            return None;
        }
        let half = ry * (1.0 - u * u).sqrt();
        let v = (py - cy) / ry;
        if u * u + v * v <= 1.0 {
            // The top face.
            let ring = (u * u + v * v).sqrt();
            let mut t = 0.72 - (u + v) * 0.1;
            if (0.55..0.65).contains(&ring) {
                t += if v > 0.0 { 0.14 } else { -0.2 };
            }
            if (u * 0.8 + v + 0.3).abs() < 0.12 {
                t += 0.22;
            }
            if ring > 0.9 && u + v < 0.0 {
                t += 0.14;
            }
            return Some(ramp.pick(t, x, y));
        }
        if py >= cy && py <= cy + h + half {
            // The rim, round across and lit on the left.
            let mut t = 0.5 - u * 0.25;
            if py > cy + h + half - 1.0 {
                t -= 0.16;
            }
            if (u + 0.45).abs() < 0.07 {
                t += 0.24;
            }
            return Some(ramp.pick(t, x, y));
        }
        None
    });
    icon.finish()
}

/// Machined metal: a hex nut, its six facets lit from the top left, a
/// bevel round its threaded hole.
pub(crate) fn machined(ramp: Ramp) -> Tex {
    let hex: Vec<P> = (0..6)
        .map(|i| {
            let a = i as f32 / 6.0 * TAU;
            (16.0 + a.cos() * 14.0, 16.0 + a.sin() * 13.0)
        })
        .collect();
    let light = (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2);
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&hex, p.0, p.1) {
            return None;
        }
        let (dx, dy) = (p.0 - 16.0, p.1 - 16.0);
        let r = dx.hypot(dy);
        if r < 4.2 {
            // The threads: dark rings, lit on their far side.
            let t = 0.12 + if (r * 1.5) as i32 % 2 == 0 { 0.1 } else { 0.0 } + if dx + dy > 0.0 { 0.1 } else { 0.0 };
            return Some(ramp.pick(t, x, y));
        }
        if r < 6.4 {
            // The bevel round the hole faces inwards: lit on the far side.
            let lit = (dx * light.0 + dy * light.1) / r;
            return Some(ramp.pick(0.55 - lit * 0.3, x, y));
        }
        // Which facet: by angle, each facing outwards.
        let a = dy.atan2(dx).rem_euclid(TAU);
        let i = ((a / TAU * 6.0) + 0.5).floor() as i32 % 6;
        let mid = (i as f32) / 6.0 * TAU;
        let facing = mid.cos() * light.0 + mid.sin() * light.1;
        let mut t = 0.5 + facing * 0.3;
        let edge = (0..6)
            .map(|k| seg_dist(hex[k], hex[(k + 1) % 6], p))
            .fold(f32::MAX, f32::min);
        if edge < 1.0 {
            t += 0.12;
        }
        Some(ramp.pick(t, x, y))
    });
    icon.finish()
}

/// Cotton cloth: a soft pale square with a fine weave, a blue stitched
/// hem and a corner turned back.
fn cotton_cloth() -> Tex {
    let cloth = soft_ramp(0xeee4d2, 0.4, 0.95);
    let hem = soft_ramp(0x4a6ab4, 0.3, 0.8);
    let quad: [P; 4] = [(5.0, 8.0), (26.0, 5.0), (28.0, 26.0), (6.0, 28.0)];
    let fold: [P; 3] = [(26.0, 5.0), (28.0, 26.0), (20.0, 13.0)];
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&quad, p.0, p.1) {
            return None;
        }
        let edge = (0..4)
            .map(|i| seg_dist(quad[i], quad[(i + 1) % 4], p))
            .fold(f32::MAX, f32::min);
        if (1.3..2.3).contains(&edge) && (x + y) % 3 != 0 {
            return Some(hem.pick(0.6, x, y));
        }
        let over = (x + y) % 2 == 0;
        let mut t = 0.62 + if over { 0.05 } else { -0.05 } - (p.0 + p.1 - 30.0) / 40.0 * 0.22;
        if seg_dist(quad[0], quad[1], p) < 1.0 || seg_dist(quad[3], quad[0], p) < 1.0 {
            t += 0.12;
        }
        Some(cloth.pick(t, x, y))
    });
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        inside_poly(&fold, p.0, p.1).then(|| cloth.pick(0.32 + if (x + y) % 2 == 0 { 0.04 } else { -0.04 }, x, y))
    });
    icon.finish()
}

/// Unused spark: a glowing ember shard with two smaller ones at its foot
/// and a white-hot heart.
fn spark_item() -> Tex {
    let ramp = spark_ramp();
    let mut icon = Icon::new();
    let up = -std::f32::consts::FRAC_PI_4;
    blade(&mut icon, (9.0, 26.5), up - 0.7, 13.0, 3.0, &ramp);
    blade(&mut icon, (7.0, 27.0), up + 0.05, 28.0, 5.2, &ramp);
    blade(&mut icon, (11.0, 28.0), up + 0.75, 12.0, 2.8, &ramp);
    icon.layer(false, |x, y| {
        let (dx, dy) = (c(x) - 16.5, c(y) - 16.5);
        (dx.abs() + dy.abs() < 1.6).then(|| ramp.lightest())
    });
    icon.finish()
}

/// Paints the icon called `name`, or `None` if this module does not know it.
pub fn paint_icon(name: &str) -> Option<Tex> {
    Some(match name {
        "silver_ore" => silver_ore(),
        "silver_ingot" => ingot(silver_ramp()),
        "silver_alloy" => ingot(alloy_ramp()),
        "relic_component" => relic_component(),
        "cog" => cog(),
        "compact_copper" => compact(copper_ramp()),
        "compact_iron" => compact(steel_ramp()),
        "compact_silver_alloy" => compact(alloy_ramp()),
        "refined_copper" => refined(copper_ramp()),
        "refined_iron" => refined(steel_ramp()),
        "refined_silver_alloy" => refined(alloy_ramp()),
        "machined_copper" => machined(copper_ramp()),
        "machined_iron" => machined(steel_ramp()),
        "machined_silver_alloy" => machined(alloy_ramp()),
        "cotton_cloth" => cotton_cloth(),
        "spark" => spark_item(),
        _ => return None,
    })
}

/// The painter for one of this module's block textures.
pub fn painter(name: &str) -> Option<fn(Ctx) -> Tex> {
    Some(match name {
        "relic_cache" => relic_cache,
        "mixer_top" => mixer_top,
        "mixer_side" => mixer_side,
        "refinery_top" => refinery_top,
        "refinery_side" => refinery_side,
        "compactor_top" => compactor_top,
        "compactor_side" => compactor_side,
        "iron_furnace_top" => iron_furnace_top,
        "iron_furnace_side" => iron_furnace_side,
        "iron_furnace_crucible_top" => iron_furnace_crucible_top,
        "iron_furnace_crucible_side" => iron_furnace_crucible_side,
        "spark_generator_top" => spark_generator_top,
        "spark_generator_side" => spark_generator_side,
        "spark_link" => spark_link,
        _ => return None,
    })
}
