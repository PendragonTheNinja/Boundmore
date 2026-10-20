//! The crafting chain's machines (milestone 11): the crafting table, the
//! extractor, the furnace's crucible, and the relic seam they draw old
//! fragments from. Drawn as made objects, like `stations`, with its helpers
//! and palettes. Four extractor blocks stand together as one machine, so
//! its side's bands and pipe meet across neighbours; the crucible stands on
//! a furnace base, so its foot course matches the furnace's cap.

use crate::color::{Ramp, Rgba};
use crate::materials::{copper_ramp, end_grain_ramp, plank_ramp};
use crate::noise::{fbm, hash2, seed, unit, value};
use crate::stations::{
    Mask, SOOT, cast_shadow, fire_ramp, furnace_ramp, gouge, grain, iron_ramp, masonry, plate, rivet,
};
use crate::tex::{Ctx, SIZE, Tex, c};

// ---------------------------------------------------------- crafting table

/// Crafting table top: three broad planks, a pale end-grain cutting board
/// let into them with knife marks, an iron set-square along its right and
/// bottom edges and a wooden mallet resting at the back.
pub fn crafting_table_top(_ctx: Ctx) -> Tex {
    let salt = seed("crafting_table_top");
    let wood = plank_ramp();
    let end = end_grain_ramp();
    let iron = iron_ramp();
    let dark = wood.at(0);
    let mut tex = Tex::from_fn(|x, y| {
        let bounds = [0usize, 11, 21, 32];
        let p = (0..3).find(|&i| y < bounds[i + 1]).unwrap();
        let row = y - bounds[p];
        let h = bounds[p + 1] - bounds[p];
        if row == h - 1 {
            return dark;
        }
        let mut t = 0.62 + (unit(hash2(salt, p as u32)) - 0.5) * 0.14;
        t += grain(hash2(salt, p as u32 + 5), x, y, false) * 0.32;
        if row == 0 {
            t += 0.2;
        } else if row == h - 2 {
            t -= 0.12;
        }
        if x == 0 {
            t += 0.12;
        }
        if x == SIZE - 1 {
            t -= 0.16;
        }
        wood.pick(t, x, y)
    });
    // The cutting board: end-grain blocks glued in a check.
    let mut board = Mask::new();
    for y in 5..22 {
        for x in 4..21 {
            let (lx, ly) = (x - 4, y - 5);
            let block = (lx / 4 + ly / 4) % 2;
            let mut t = 0.6 + block as f32 * 0.1 + (fbm(salt ^ 9, 8, 8, 2, c(x), c(y)) - 0.5) * 0.14;
            if lx == 0 || ly == 0 {
                t += 0.24;
            }
            if lx == 16 || ly == 16 {
                t -= 0.3;
            }
            tex.set(x, y, end.pick(t, x, y));
            board.set(x, y);
        }
    }
    cast_shadow(&mut tex, &board, Rgba::hex(0x2a1420), 0.45);
    let groove = end.at(1).mix(Rgba::hex(0x2a1420), 0.3);
    let lip = end.lightest();
    gouge(&mut tex, (7.0, 9.0), (12.0, 11.0), groove, lip);
    gouge(&mut tex, (9.0, 15.0), (14.0, 18.0), groove, lip);
    gouge(&mut tex, (15.0, 8.0), (17.0, 12.0), groove, lip);
    // The set-square: two iron arms meeting at the bottom right, marked off.
    let mut tools = Mask::new();
    plate(&mut tex, &mut tools, &iron, 24, 6, 27, 27, 0.5);
    plate(&mut tex, &mut tools, &iron, 6, 24, 27, 27, 0.52);
    for x in (8..24).step_by(3) {
        tex.set(x, 25, iron.at(2));
    }
    // The mallet: a pale wooden head on a short handle.
    for y in 1..5 {
        for x in 22..30 {
            let t = match (x, y) {
                (22, _) | (_, 1) => 0.9,
                (29, _) | (_, 4) => 0.4,
                _ => 0.66,
            };
            tex.set(x, y, end.pick(t, x, y));
            tools.set(x, y);
        }
    }
    for x in 13..22 {
        for y in 2..4 {
            tex.set(x, y, wood.pick(if y == 2 { 0.78 } else { 0.42 }, x, y));
            tools.set(x, y);
        }
    }
    cast_shadow(&mut tex, &tools, Rgba::hex(0x24101c), 0.55);
    tex
}

/// Crafting table side: the thick top's edge, a drawer with a copper pull
/// under it, a square leg at each corner and a crossed brace between them.
pub fn crafting_table_side(_ctx: Ctx) -> Tex {
    let salt = seed("crafting_table_side");
    let wood = plank_ramp();
    let shade = Rgba::hex(0x1e0e18);
    let copper = copper_ramp();
    let mut tex = Tex::from_fn(|x, y| {
        if y < 5 {
            let mut t = 0.6 + grain(salt, x, y, false) * 0.3;
            t += [0.3, 0.1, 0.0, -0.06, -0.28][y];
            return wood.pick(t, x, y);
        }
        if !(4..28).contains(&x) {
            let lx = x % 28;
            let mut t = 0.52 + grain(hash2(salt, 1 + (x >= 28) as u32), x, y, true) * 0.3;
            t += [0.22, 0.04, -0.06, -0.24][lx];
            if y == 5 {
                t -= 0.24;
            }
            return wood.pick(t, x, y);
        }
        // The drawer front in its frame.
        if y < 13 {
            if y == 5 || y == 12 || x == 4 || x == 27 {
                return wood.at(1);
            }
            let mut t = 0.54 + grain(hash2(salt, 2), x, y, false) * 0.26;
            if y == 6 || x == 5 {
                t += 0.2;
            }
            if y == 11 || x == 26 {
                t -= 0.2;
            }
            return wood.pick(t, x, y);
        }
        // Below: back boards in shade, crossed by the brace.
        let (u, v) = (x as f32 - 4.0, y as f32 - 13.0);
        let d1 = u - v * 24.0 / 19.0;
        let d2 = u - (19.0 - v) * 24.0 / 19.0;
        let on = |d: f32| d.abs() < 1.7;
        if on(d1) || on(d2) {
            let lit = if on(d1) { d1 < -0.4 } else { d2 < -0.4 };
            let t = 0.5 + grain(hash2(salt, 3), x, y, true) * 0.2 + if lit { 0.18 } else { -0.12 };
            return wood.pick(t, x, y);
        }
        let t = 0.1 + v / 19.0 * 0.12 + grain(hash2(salt, 4), x, y, true) * 0.08;
        wood.pick(t, x, y).mix(shade, 0.3)
    });
    let mut pull = Mask::new();
    for (x, y, t) in [
        (14, 8, 0.95),
        (15, 8, 0.8),
        (16, 8, 0.7),
        (17, 8, 0.6),
        (14, 9, 0.6),
        (17, 9, 0.3),
    ] {
        tex.set(x, y, copper.pick(t, x, y));
        pull.set(x, y);
    }
    cast_shadow(&mut tex, &pull, shade, 0.5);
    tex
}

// --------------------------------------------------------------- extractor

/// The extractor's dressed stone: a cool grey, a little bluer than rock.
fn machine_stone() -> Ramp {
    Ramp::from_hex(&[
        0x1e2030, 0x2e3244, 0x424a5c, 0x586274, 0x6e7a8c, 0x8a96a6, 0xaab4c0, 0xd2d8de,
    ])
}

/// Pressed plant pulp: deep green to a pale yellow-green froth.
fn pulp_ramp() -> Ramp {
    Ramp::from_hex(&[0x14281c, 0x1e4024, 0x2e5c2a, 0x4a7a30, 0x6c9838, 0x9cba50, 0xd4e08a])
}

/// Extractor top: a dressed stone rim round a copper-lipped vat of green
/// pulp, an iron press plate sunk in it with its nut in the middle.
pub fn extractor_top(_ctx: Ctx) -> Tex {
    let salt = seed("extractor_top");
    let stone = machine_stone();
    let copper = copper_ramp();
    let pulp = pulp_ramp();
    let iron = iron_ramp();
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let (dx, dy) = (fx - 16.0, fy - 16.0);
        let r = dx.hypot(dy);
        if r < 5.0 {
            if dx.abs() < 1.6 && dy.abs() < 1.6 {
                return iron.pick(0.85 - (dx + dy) * 0.15, x, y);
            }
            let t = 0.55 - (dx * 0.6 + dy * 0.8) / 5.0 * 0.25 - if r > 4.2 { 0.25 } else { 0.0 };
            return iron.pick(t, x, y);
        }
        if r < 10.0 {
            let froth = value(salt ^ 2, 8, 8, fx, fy);
            let t = 0.75 - (r - 5.0) / 5.0 * 0.5 + (froth - 0.5) * 0.3;
            return pulp.pick(t, x, y);
        }
        if r < 12.0 {
            // The copper lip: the far inner wall and the near outer rim lit.
            let lit = -(dx + dy) / r;
            let t = 0.55 + if r < 11.0 { -lit * 0.25 } else { lit * 0.3 };
            return copper.pick(t, x, y);
        }
        if x == 15 || y == 15 {
            return stone.at(0);
        }
        let quarter = (fx > 16.0) as u32 + 2 * (fy > 16.0) as u32;
        let mut t = 0.52 + (unit(hash2(salt, quarter)) - 0.5) * 0.12;
        t += (fbm(hash2(salt, 7 + quarter), 8, 8, 2, fx, fy) - 0.5) * 0.18;
        if x == 0 || y == 0 {
            t += 0.24;
        }
        if x == SIZE - 1 || y == SIZE - 1 {
            t -= 0.26;
        }
        if r < 13.0 {
            t -= 0.2;
        }
        stone.pick(t, x, y)
    })
}

/// Extractor side: courses of dressed stone between a riveted copper band
/// at the top and a plinth at the foot, a round glass port showing the
/// green pulp inside, and a copper pipe down the right with a valve wheel.
pub fn extractor_side(_ctx: Ctx) -> Tex {
    let salt = seed("extractor_side");
    let stone = machine_stone();
    let copper = copper_ramp();
    let pulp = pulp_ramp();
    let iron = iron_ramp();
    let mut tex = Tex::from_fn(|x, y| {
        if y < 4 {
            let t = 0.55 + [0.3, 0.1, 0.0, -0.3][y] + (fbm(salt ^ 3, 4, 2, 2, c(x), c(y)) - 0.5) * 0.12;
            return copper.pick(t, x, y);
        }
        if y >= 28 {
            let t = 0.5 + [0.24, 0.04, 0.0, -0.24][y - 28];
            return stone.pick(t, x, y);
        }
        masonry(salt, &stone, x, y)
    });
    for x in (3..SIZE).step_by(7) {
        rivet(&mut tex, &iron, x, 2);
    }
    // The port: an iron ring, pulp behind the glass, a bubble and a
    // highlight across the pane.
    let (px, py) = (13.0f32, 15.5f32);
    let mut parts = Mask::new();
    for y in 8..24 {
        for x in 5..22 {
            let (dx, dy) = (c(x) - px, c(y) - py);
            let r = dx.hypot(dy);
            if r > 7.0 {
                continue;
            }
            let col = if r > 5.4 {
                iron.pick(0.55 - (dx + dy) / r * 0.3, x, y)
            } else {
                let mut col = if dy > -2.0 {
                    pulp.pick(0.45 - dy / 6.0 * 0.2, x, y)
                } else {
                    pulp.at(0).mix(Rgba::hex(0x2a3a40), 0.5)
                };
                if (dx + 1.5).hypot(dy - 1.0) < 1.2 {
                    col = pulp.at(6);
                }
                if (dx + dy + 5.0).abs() < 0.8 && r < 4.6 {
                    col = col.mix(Rgba::hex(0xeef8ff), 0.55);
                }
                col
            };
            tex.set(x, y, col);
            parts.set(x, y);
        }
    }
    // The pipe and its valve wheel.
    for y in 4..28 {
        for x in 25..29 {
            let t = [0.85, 0.65, 0.45, 0.25][x - 25] - if y % 8 == 3 { 0.2 } else { 0.0 };
            tex.set(x, y, copper.pick(t, x, y));
            parts.set(x, y);
        }
    }
    for y in 15..22 {
        for x in 23..31 {
            let (dx, dy) = (c(x) - 27.0, c(y) - 18.5);
            let r = dx.hypot(dy);
            if (2.4..3.6).contains(&r) || (r < 3.0 && (dx.abs() < 0.6 || dy.abs() < 0.6)) {
                tex.set(x, y, iron.pick(0.7 - (dx + dy) / 4.0 * 0.3, x, y));
                parts.set(x, y);
            }
        }
    }
    cast_shadow(&mut tex, &parts, SOOT, 0.45);
    tex
}

// -------------------------------------------------------- furnace crucible

/// Fired clay: deep brown to a warm terracotta.
fn clay_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x2a1418, 0x4a2220, 0x6c3426, 0x8e4a30, 0xa8603a, 0xc27a4a, 0xd89a66, 0xecc090,
    ])
}

/// Furnace crucible top: dressed stone round a thick clay crucible full of
/// molten metal, a dark skin of dross drifting on it.
pub fn furnace_crucible_top(_ctx: Ctx) -> Tex {
    let salt = seed("furnace_crucible_top");
    let stone = furnace_ramp();
    let clay = clay_ramp();
    let fire = fire_ramp();
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let (dx, dy) = (fx - 16.0, fy - 16.0);
        let r = dx.hypot(dy);
        if r < 8.0 {
            let swirl = fbm(salt ^ 5, 4, 4, 2, fx, fy);
            if swirl > 0.64 && r > 2.0 {
                // Dross: a dark crust with a glowing crack round it.
                return if swirl < 0.68 {
                    fire.at(3)
                } else {
                    SOOT.mix(fire.at(1), 0.3)
                };
            }
            let t = 0.98 - r / 8.0 * 0.45 + (swirl - 0.5) * 0.2;
            return fire.pick(t, x, y);
        }
        if r < 11.5 {
            // The clay lip: glowing inside, lit on its top-left outside.
            let lit = -(dx + dy) / r;
            let mut t = 0.52 + if r < 9.5 { -lit * 0.2 } else { lit * 0.28 };
            if r < 8.8 {
                return clay.pick(t, x, y).mix(fire.at(4), 0.45);
            }
            if r > 10.8 {
                t -= 0.24;
            }
            return clay.pick(t, x, y);
        }
        if x == 16 || y == 16 {
            return stone.at(0);
        }
        let quarter = (fx > 16.0) as u32 + 2 * (fy > 16.0) as u32;
        let mut t = 0.52 + (unit(hash2(salt, quarter)) - 0.5) * 0.12;
        t += (fbm(hash2(salt, 9 + quarter), 8, 8, 2, fx, fy) - 0.5) * 0.18;
        if x == 0 || y == 0 {
            t += 0.22;
        }
        if x == SIZE - 1 || y == SIZE - 1 {
            t -= 0.24;
        }
        let near = if r < 12.5 { 0.18 } else { 0.0 };
        stone
            .pick(t - near, x, y)
            .mix(fire.at(2), ((14.0 - r) / 6.0).clamp(0.0, 0.25))
    })
}

/// Furnace crucible side: a clay pot bound with two riveted iron hoops, a
/// spout notched in its lip with metal glowing in it, soot and heat cracks
/// low down, standing on a stone course like the furnace's own cap.
pub fn furnace_crucible_side(_ctx: Ctx) -> Tex {
    let salt = seed("furnace_crucible_side");
    let stone = furnace_ramp();
    let clay = clay_ramp();
    let fire = fire_ramp();
    let iron = iron_ramp();
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        if y >= 28 {
            if [9usize, 22].contains(&x) {
                return stone.at(0);
            }
            let t = 0.58 + [0.28, 0.06, 0.0, -0.28][y - 28] + (fbm(salt ^ 4, 4, 2, 2, fx, fy) - 0.5) * 0.18;
            return stone.pick(t, x, y);
        }
        // The pot: round across (lit on the left), darker and sooty low down.
        let u = (fx - 16.0) / 16.0;
        let mut t = 0.6 - u * 0.28 - (u * u) * 0.2;
        t += (fbm(salt ^ 1, 4, 4, 2, fx, fy) - 0.5) * 0.14;
        if y < 3 {
            t += [0.3, 0.14, -0.2][y];
        }
        let soot = ((fy - 18.0) / 10.0).clamp(0.0, 1.0);
        let col = clay.pick(t - soot * 0.18, x, y).mix(SOOT, soot * 0.35);
        if (6..9).contains(&y) || (20..23).contains(&y) {
            let ly = if y < 9 { y - 6 } else { y - 20 };
            return iron.pick(0.52 + [0.26, 0.0, -0.28][ly] - u * 0.15, x, y);
        }
        col
    });
    for x in (4..SIZE - 4).step_by(9) {
        rivet(&mut tex, &iron, x, 7);
        rivet(&mut tex, &iron, x + 4, 21);
    }
    for y in 0..4 {
        for x in 13..19 {
            let half = 3.0 - y as f32 * 0.7;
            if (c(x) - 16.0).abs() < half {
                tex.set(x, y, if y < 2 { fire.at(6) } else { fire.at(4) });
            }
        }
    }
    let dark = clay.at(0);
    gouge(&mut tex, (5.0, 24.0), (8.0, 27.0), dark, fire.at(2));
    gouge(&mut tex, (23.0, 23.0), (25.0, 26.5), dark, fire.at(2));
    gouge(&mut tex, (14.0, 11.0), (16.0, 13.0), dark, clay.at(5));
    tex
}

// ------------------------------------------------------------- relic seam

/// Tarnished old bronze, from deep shadow through brown to worn gold.
pub(crate) fn relic_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x1a1418, 0x30241e, 0x4c3a26, 0x6e5530, 0x90723c, 0xb0904c, 0xd0b46a, 0xf0dca0,
    ])
}

/// The faint cyan light cut into relic plates.
pub(crate) fn relic_light_ramp() -> Ramp {
    Ramp::from_hex(&[0x1c4a5a, 0x2a7a8a, 0x48b4c0, 0x8ae8ee, 0xe0ffff])
}

/// Relic seam: stone with angular fragments of some old made thing set in
/// it: tarnished bronze plates, broken at a corner, each cut with a line of
/// faint cyan light and touched with green verdigris; dark where the
/// plates meet the rock, as ores are.
pub fn relic_seam(ctx: Ctx) -> Tex {
    let salt = seed("relic_seam");
    let mut tex = crate::materials::stone(ctx);
    let bronze = relic_ramp();
    let light = relic_light_ramp();
    let verdigris = Rgba::hex(0x5aa08a);
    let outline = Rgba::hex(0x1a1220);
    // Plates: centre, half sizes, turn, and whether the light runs along.
    let plates: [(f32, f32, f32, f32, f32, bool); 4] = [
        (9.0, 10.0, 5.5, 3.5, 0.35, true),
        (23.0, 9.0, 3.5, 4.5, -0.2, false),
        (17.0, 24.0, 6.5, 3.0, 0.1, true),
        (4.0, 24.5, 2.6, 2.6, 0.7, false),
    ];
    let inside = |fx: f32, fy: f32| -> Option<(usize, f32, f32)> {
        for (i, &(px, py, hx, hy, turn, _)) in plates.iter().enumerate() {
            let (s, co) = turn.sin_cos();
            let (dx, dy) = (fx - px, fy - py);
            let (u, v) = (dx * co + dy * s, -dx * s + dy * co);
            if u.abs() < hx && v.abs() < hy && u + v < hx + hy - 2.0 {
                return Some((i, u / hx, v / hy));
            }
        }
        None
    };
    let host = tex.clone();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (c(x), c(y));
            if let Some((i, u, v)) = inside(fx, fy) {
                let along = plates[i].5;
                let lit_edge = inside(fx - 1.0, fy).is_none() || inside(fx, fy - 1.0).is_none();
                let dark_edge = inside(fx + 1.0, fy).is_none() || inside(fx, fy + 1.0).is_none();
                let mut t = 0.5 + (unit(hash2(salt, i as u32)) - 0.5) * 0.2 - (u + v) * 0.08;
                t += (value(hash2(salt, 20 + i as u32), 8, 8, fx, fy) - 0.5) * 0.18;
                let mut col = if lit_edge {
                    bronze.pick(0.92, x, y)
                } else if dark_edge {
                    bronze.pick(0.18, x, y)
                } else {
                    bronze.pick(t, x, y)
                };
                let line = if along { v.abs() < 0.22 } else { u.abs() < 0.22 };
                if !lit_edge && !dark_edge {
                    if line {
                        col = light.pick(0.75 - (u + v).abs() * 0.3, x, y);
                    } else if fbm(salt ^ i as u32, 4, 4, 2, fx, fy) > 0.63 {
                        col = col.mix(verdigris, 0.6);
                    }
                }
                tex.set(x, y, col);
            } else {
                let near = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
                    .iter()
                    .any(|(dx, dy)| inside(fx + dx, fy + dy).is_some());
                if near {
                    tex.set(x, y, outline);
                } else if inside(fx - 1.0, fy - 1.0).is_some() || inside(fx - 2.0, fy - 1.0).is_some() {
                    tex.set(x, y, host.get(x, y).mix(outline, 0.4));
                }
            }
        }
    }
    tex
}
