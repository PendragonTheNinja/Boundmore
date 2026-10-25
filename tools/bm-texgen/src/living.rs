//! Milestone 19, Living Worlds (`docs/milestones/19-living-worlds.md`):
//! the new rocks (basalt, limestone), soils (clay, peat, mud), grasses
//! (meadow and moss turf), trunks (pale and gnarled) and leaves (fronds,
//! glossy foliage). Each reads clearly apart from the stone, soil, turf,
//! trunk and foliage it joins, in colour and in pattern.

use crate::color::{Ramp, Rgba};
use crate::materials::{RockStyle, grass_with, rock, specks, stroke, trunk_top_with, turf_side_with};
use crate::noise::{cell, fbm, hash2, seed, value, wrap_delta, wrap_i};
use crate::tex::{Ctx, Field, SIZEF, Tex, c, detail, fine_octaves, hires, paint, res, scale, smoothstep};

/// The textures this milestone adds, in catalogue order.
pub const BLOCKS: [&str; 15] = [
    "basalt",
    "limestone",
    "clay",
    "peat",
    "mud",
    "meadow_turf_top",
    "meadow_turf_side",
    "moss_turf_top",
    "moss_turf_side",
    "pale_trunk_side",
    "pale_trunk_top",
    "gnarled_trunk_side",
    "gnarled_trunk_top",
    "fronds",
    "glossy_foliage",
];

pub fn painter(name: &str) -> Option<fn(Ctx) -> Tex> {
    Some(match name {
        "basalt" => basalt,
        "limestone" => limestone,
        "clay" => clay,
        "peat" => peat,
        "mud" => mud,
        "meadow_turf_top" => meadow_turf_top,
        "meadow_turf_side" => meadow_turf_side,
        "moss_turf_top" => moss_turf_top,
        "moss_turf_side" => moss_turf_side,
        "pale_trunk_side" => pale_trunk_side,
        "pale_trunk_top" => pale_trunk_top,
        "gnarled_trunk_side" => gnarled_trunk_side,
        "gnarled_trunk_top" => gnarled_trunk_top,
        "fronds" => fronds,
        "glossy_foliage" => glossy_foliage,
        _ => return None,
    })
}

// --------------------------------------------------------------- palettes

fn basalt_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x0c0d18, 0x151826, 0x1f2333, 0x2a3042, 0x363e52, 0x465066, 0x5a667c, 0x76849a,
    ])
}

fn limestone_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x6a5244, 0x8a705a, 0xa68e74, 0xbea88a, 0xd2bf9e, 0xe2d3b4, 0xefe4cc, 0xfaf4e4,
    ])
}

fn clay_ramp() -> Ramp {
    Ramp::from_hex(&[0x4a1c1e, 0x6a2a24, 0x8a3c2a, 0xa64e30, 0xbe643a, 0xd27e48, 0xe29c62])
}

fn peat_ramp() -> Ramp {
    Ramp::from_hex(&[0x0e080c, 0x180e12, 0x241614, 0x321e18, 0x42281e, 0x543424, 0x68422c])
}

fn mud_ramp() -> Ramp {
    Ramp::from_hex(&[0x1e1a1e, 0x2c2628, 0x3c3430, 0x4c4238, 0x5c5044, 0x6e6252, 0x847664])
}

fn meadow_ramp() -> Ramp {
    Ramp::from_hex(&[0x30501c, 0x48681e, 0x628422, 0x7ea028, 0x9cba30, 0xbcd042, 0xdce468])
}

fn moss_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x0a2228, 0x0f3232, 0x15443c, 0x1c5846, 0x256c50, 0x32825a, 0x469866, 0x66b07a,
    ])
}

fn pale_bark_ramp() -> Ramp {
    Ramp::from_hex(&[0x6e6460, 0x8e847c, 0xaca296, 0xc6bcac, 0xdad2c0, 0xeae4d4, 0xf8f4e8])
}

fn pale_wood_ramp() -> Ramp {
    Ramp::from_hex(&[0x8a7456, 0xa48c68, 0xbca47a, 0xd0ba8e, 0xe0cea4, 0xeee0bc, 0xf8f0d6])
}

fn gnarled_bark_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x120a0e, 0x22141a, 0x32201e, 0x442c24, 0x583a2a, 0x6e4c34, 0x866240, 0x9e7a52,
    ])
}

fn gnarled_wood_ramp() -> Ramp {
    Ramp::from_hex(&[0x3a2018, 0x52301e, 0x6a4026, 0x825230, 0x98663c, 0xae7c4c, 0xc49460])
}

fn fronds_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x081c1c, 0x0e2c26, 0x143e2c, 0x1c5232, 0x266838, 0x327e40, 0x44964a, 0x62b05a,
    ])
}

fn glossy_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x1a3612, 0x284c16, 0x3a661a, 0x50821e, 0x6a9e24, 0x88b82e, 0xa8cc44, 0xd2e888,
    ])
}

// ------------------------------------------------------------------ rocks

/// One-pixel flecks (two at a time above 32 pixels), each lit lighter on
/// its top-left neighbour: crystals or fossil grit set into a rock.
fn flecks(ctx: Ctx, tex: &mut Tex, salt: u32, spacing: f32, chance: f32, col: Rgba, glint: Rgba) {
    let u = 1.0 / detail();
    let px = (u * scale()).round().max(1.0) as i32;
    for s in ctx.sites(salt, spacing * u).into_iter().filter(|s| s.r[0] < chance) {
        let (x, y) = ((s.x * scale()) as i32, (s.y * scale()) as i32);
        for j in 0..px {
            for i in 0..px {
                tex.set(wrap_i(x + i), wrap_i(y + j), col);
            }
        }
        if s.r[1] < 0.5 {
            let (gx, gy) = (wrap_i(x - px), wrap_i(y - px));
            tex.set(gx, gy, tex.get(gx, gy).mix(glint, 0.6));
        } else {
            let (gx, gy) = (wrap_i(x + px), wrap_i(y));
            tex.set(gx, gy, tex.get(gx, gy).mix(col, 0.45));
        }
    }
}

/// Basalt: blue-black rock in tall columns (a columnar join, unlike
/// stone's flat slabs), dense with cracks, a few pale crystal flecks.
pub fn basalt(ctx: Ctx) -> Tex {
    let salt = seed("basalt");
    let style = RockStyle {
        salt,
        spacing: 7.0,
        stretch: 0.55,
        bevel: 1.8,
        cracks: 0.8,
        tone: 0.48,
        spread: 0.22,
        tilt: 0.03,
        rim: 0.26,
        mottle: 0.16,
    };
    let mut tex = paint(&basalt_ramp(), &rock(ctx, &style));
    flecks(
        ctx,
        &mut tex,
        salt ^ 0x31,
        6.5,
        0.38,
        Rgba::hex(0xb8cad6),
        Rgba::hex(0xeef6ff),
    );
    tex
}

/// Limestone: pale cream rock in long flat beds, faint horizontal layering
/// across them, few cracks, small shell-like fossil specks and dark pores.
pub fn limestone(ctx: Ctx) -> Tex {
    let salt = seed("limestone");
    let style = RockStyle {
        salt,
        spacing: 10.0,
        stretch: 2.6,
        bevel: 2.2,
        cracks: 0.3,
        tone: 0.58,
        spread: 0.16,
        tilt: 0.02,
        rim: 0.18,
        mottle: 0.16,
    };
    let base = rock(ctx, &style);
    let tone = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let wob = (ctx.fbm(salt ^ 2, 2, 2, 2, fx, fy) - 0.5) * 3.0;
        // Five beds of layering down the tile (an integer count, so it tiles).
        let band = ((fy + wob) / SIZEF * 5.0 * std::f32::consts::TAU).sin();
        base.at(x, y) + band * 0.045 - smoothstep(0.82, 1.0, band) * 0.05
    });
    let mut tex = paint(&limestone_ramp(), &tone);
    // Fossils: a little curl of three or four darker pixels around a pale one.
    let shell = Rgba::hex(0x8c7058);
    let u = 1.0 / detail();
    let px = (u * scale()).round().max(1.0) as i32;
    for s in ctx.sites(salt ^ 0x51, 9.0 * u).into_iter().filter(|s| s.r[0] < 0.55) {
        let (x, y) = ((s.x * scale()) as i32, (s.y * scale()) as i32);
        let curl: &[(i32, i32)] = if s.r[1] < 0.5 {
            &[(0, 0), (1, 0), (2, 1), (1, 2)]
        } else {
            &[(1, 0), (0, 1), (0, 2), (1, 2), (2, 2)]
        };
        for &(dx, dy) in curl {
            for j in 0..px {
                for i in 0..px {
                    let (qx, qy) = (wrap_i(x + dx * px + i), wrap_i(y + dy * px + j));
                    tex.set(qx, qy, tex.get(qx, qy).mix(shell, 0.75));
                }
            }
        }
        for j in 0..px {
            for i in 0..px {
                let (qx, qy) = (wrap_i(x + px + i), wrap_i(y + px + j));
                tex.set(qx, qy, Rgba::hex(0xfcf8ec));
            }
        }
    }
    specks(ctx, &mut tex, salt ^ 0x61, 6.0, 0.35, Rgba::hex(0x7a6250));
    tex
}

// ------------------------------------------------------------------ soils

/// Clay: dense red-orange earth, smooth where soil is crumbly: broad soft
/// lumps smeared sideways, a few drying cracks and pale grit.
pub fn clay(ctx: Ctx) -> Tex {
    let salt = seed("clay");
    let lumps = ctx.sites_stretched(salt, 8.0, 1.6);
    let height = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&lumps, fx, fy, 1.6);
        let dome = 1.0 - (k.d1 / 7.0).min(1.0);
        dome * 0.45 + lumps[k.index].r[0] * 0.2 + ctx.fbm(salt ^ 1, 2, 8, 2, fx, fy) * 0.35
    })
    .blur()
    .blur();
    let light = height.light();
    let tone = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&lumps, fx, fy, 1.6);
        let mut t = 0.5 + light.at(x, y) * 1.3 + (height.at(x, y) - 0.5) * 0.4;
        t += (ctx.value(salt ^ 2, 16, 4, fx, fy) - 0.5) * 0.08;
        // Drying cracks along part of the lump joins.
        if k.edge < 0.5 && ctx.value(salt ^ 3, 4, 4, fx, fy) < 0.4 {
            t = 0.06;
        } else if k.edge < 1.4 {
            t -= 0.06;
        }
        t
    });
    let mut tex = paint(&clay_ramp(), &tone);
    specks(ctx, &mut tex, salt ^ 4, 7.0, 0.3, Rgba::hex(0xf0c08a));
    tex
}

/// Peat: near-black brown packed fibre, streaked sideways, with tangled
/// lighter rootlets and a few dark wet hollows.
pub fn peat(ctx: Ctx) -> Tex {
    let salt = seed("peat");
    let ramp = peat_ramp();
    let mut tone = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let mut t = 0.42 + (ctx.fbm(salt, 4, 4, 2 + fine_octaves(), fx, fy) - 0.5) * 0.35;
        // Fibre: fine streaks, longer across than down.
        t += (ctx.value(salt ^ 1, 4, 32, fx, fy) - 0.5) * 0.3;
        if ctx.fbm(salt ^ 2, 4, 4, 2, fx, fy) < 0.3 {
            t -= 0.2;
        }
        t
    });
    let n = res();
    let u = 1.0 / detail();
    // Short fibres every which way.
    for s in ctx.sites(salt ^ 3, 2.6 * u) {
        let ang = s.r[0] * std::f32::consts::PI * 0.5 - 0.25 * std::f32::consts::PI;
        let len = (1.5 + s.r[1] * 2.0) * u;
        let t = if s.r[2] < 0.5 { 0.62 } else { 0.18 };
        let b = (s.x + ang.cos() * len, s.y + ang.sin() * len);
        stroke((s.x, s.y), b, 0.9 * u, 0.7 * u, |x, y, _| tone.v[y * n + x] = t);
    }
    let mut tex = paint(&ramp, &tone);
    // Rootlets: thin wavy tan threads.
    let root = Ramp::from_hex(&[0x5a3e28, 0x7a5636, 0x9a7048, 0xb68a5c]);
    for s in ctx.sites(salt ^ 4, 10.0 * u).into_iter().filter(|s| s.r[0] < 0.55) {
        let mut p = (s.x, s.y);
        let mut ang = s.r[1] * std::f32::consts::TAU;
        let steps = 3 + (s.r[2] * 3.0) as u32;
        for i in 0..steps {
            ang += (s.r[3] - 0.5) * 0.8 + if i % 2 == 0 { 0.3 } else { -0.3 };
            let q = (p.0 + ang.cos() * 2.0 * u, p.1 + ang.sin() * 2.0 * u);
            let lit = 0.15 + (i as f32 / steps as f32) * 0.5;
            stroke(p, q, 0.95 * u, 0.85 * u, |x, y, _| tex.set(x, y, root.pick(lit, x, y)));
            p = q;
        }
    }
    tex
}

/// Mud: wet brown-grey sludge in soft swirls, with glossy patches where
/// it is wettest: smoother, darker, and shining pale on every crest that
/// faces the light.
pub fn mud(ctx: Ctx) -> Tex {
    let salt = seed("mud");
    let ramp = mud_ramp();
    let height = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let wx = fx + (ctx.fbm(salt, 2, 2, 2, fx, fy) - 0.5) * 8.0;
        let wy = fy + (ctx.fbm(salt ^ 1, 2, 2, 2, fx, fy) - 0.5) * 8.0;
        ctx.fbm(salt ^ 2, 3, 3, 2, wx, wy)
    })
    .blur();
    let light = height.light();
    let shine = Rgba::hex(0xc8d0d8);
    // Only the steepest lit slopes shine, whatever the light's scale.
    let mut sorted = light.v.clone();
    sorted.sort_by(f32::total_cmp);
    let q = |f: f32| sorted[((sorted.len() - 1) as f32 * f) as usize];
    let (lo, mid, hi) = (q(0.8), q(0.9), q(0.985));
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let wet = smoothstep(0.5, 0.56, ctx.fbm(salt ^ 3, 4, 4, 2, fx, fy));
        let l = light.at(x, y);
        let dry = 0.48 + l * 1.5 + (height.at(x, y) - 0.5) * 0.5 + (ctx.value(salt ^ 4, 16, 16, fx, fy) - 0.5) * 0.12;
        let slick = 0.22 + l * 1.6 + (height.at(x, y) - 0.5) * 0.3;
        let col = ramp.pick(dry * (1.0 - wet) + slick * wet, x, y);
        // Shine in a thin streak along each lit slope, not over all of it.
        let streak = smoothstep(lo, mid, l) * (1.0 - smoothstep(mid, hi, l));
        col.mix(shine, streak * wet * 0.7)
    });
    // A few grit specks on the dry mud.
    specks(ctx, &mut tex, salt ^ 6, 6.0, 0.35, Rgba::hex(0x1e1a1e));
    tex
}

// ------------------------------------------------------------------ grass

/// Meadow turf top: sunny yellow-green grass with taller tufts.
pub fn meadow_turf_top(ctx: Ctx) -> Tex {
    grass_with(ctx, seed("meadow_turf_top"), &meadow_ramp(), 2)
}

/// Meadow turf side: a long ragged meadow fringe over clay.
pub fn meadow_turf_side(_ctx: Ctx) -> Tex {
    turf_side_with(
        clay(Ctx::BASE),
        seed("meadow_turf_side"),
        &meadow_ramp(),
        (4.0, 7.0),
        Rgba::hex(0x3a1416),
    )
}

/// Moss turf top: a soft deep blue-green carpet of tiny rounded cushions,
/// each lit on its top left, with pale fuzzy tips and no blades.
pub fn moss_turf_top(ctx: Ctx) -> Tex {
    let salt = seed("moss_turf_top");
    let ramp = moss_ramp();
    let cushions = ctx.sites(salt, 3.4);
    let height = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&cushions, fx, fy, 1.0);
        let dome = 1.0 - (k.d1 / 3.0).min(1.0);
        dome * 0.6 + cushions[k.index].r[0] * 0.15 + ctx.fbm(salt ^ 1, 2, 2, 2, fx, fy) * 0.5
    })
    .blur();
    let light = height.light();
    let tone = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let mut t = 0.42 + light.at(x, y) * 1.6 + (height.at(x, y) - 0.5) * 0.6;
        t += (ctx.value(salt ^ 2, 16, 16, fx, fy) - 0.5) * 0.12;
        t
    });
    let mut tex = paint(&ramp, &tone);
    // Fuzzy tips: pale sprinkles on the lit crowns of the cushions.
    let tip = ramp.lightest().mix(Rgba::hex(0xd8f0a0), 0.4);
    for s in ctx.sites(salt ^ 3, 2.4 / detail()) {
        let (x, y) = (wrap_i((s.x * scale()) as i32), wrap_i((s.y * scale()) as i32));
        if tone.at(x, y) > 0.58 && s.r[0] < 0.7 {
            tex.set(x, y, tex.get(x, y).mix(tip, 0.7));
        }
    }
    tex
}

/// Moss turf side: a thick, even, rounded moss lip over peat.
pub fn moss_turf_side(_ctx: Ctx) -> Tex {
    turf_side_with(
        peat(Ctx::BASE),
        seed("moss_turf_side"),
        &moss_ramp(),
        (6.0, 3.0),
        Rgba::hex(0x05080a),
    )
}

// ------------------------------------------------------------------- wood

/// Pale trunk side: smooth silvery-cream bark with dark horizontal marks
/// (short dashes, some in pairs), faint peeling bands and a few dark knots.
pub fn pale_trunk_side(_ctx: Ctx) -> Tex {
    let salt = seed("pale_trunk_side");
    let ctx = Ctx::BASE;
    let ramp = pale_bark_ramp();
    let tone = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let mut t = 0.66 + (fbm(salt, 2, 8, 2 + fine_octaves(), fx, fy) - 0.5) * 0.3;
        // Peeling bands across the trunk.
        t += (value(salt ^ 1, 2, 16, fx, fy) - 0.5) * 0.18;
        // Fine vertical sheen.
        t += (value(salt ^ 2, 16, 2, fx, fy) - 0.5) * 0.08;
        t
    });
    let mut tex = paint(&ramp, &tone);
    let dark = Rgba::hex(0x2a2226);
    let lit = Rgba::hex(0xfcfaf2);
    let u = 1.0 / detail();
    for s in ctx.sites(salt ^ 3, 5.0 * u).into_iter().filter(|s| s.r[0] < 0.75) {
        let len = (2.0 + s.r[1] * 5.0) * u;
        let (a, b) = ((s.x - len * 0.5, s.y), (s.x + len * 0.5, s.y));
        let w = if s.r[2] < 0.3 { 1.6 } else { 1.0 } * u;
        stroke(a, b, w, w * 0.6, |x, y, along| {
            let m = if along > 0.85 { 0.5 } else { 0.85 };
            tex.set(x, y, tex.get(x, y).mix(dark, m));
        });
        // A lit lip just under each mark.
        stroke((a.0, a.1 + u), (b.0, b.1 + u), 0.8 * u, 0.6 * u, |x, y, _| {
            tex.set(x, y, tex.get(x, y).mix(lit, 0.5));
        });
    }
    // Knots: a dark oval with a lighter ring.
    for s in ctx.sites(salt ^ 4, 14.0 * u).into_iter().filter(|s| s.r[0] < 0.5) {
        let n = res();
        for y in 0..n {
            for x in 0..n {
                let dx = wrap_delta(c(x) - s.x);
                let dy = wrap_delta(c(y) - s.y);
                let d = (dx * dx * 0.6 + dy * dy * 1.6).sqrt();
                if d < 1.4 * u {
                    tex.set(x, y, Rgba::hex(0x3a3034));
                } else if d < 2.3 * u {
                    tex.set(x, y, tex.get(x, y).mix(Rgba::hex(0x8a7e78), 0.5));
                }
            }
        }
    }
    tex
}

/// Pale trunk top: close cream rings and a silvery bark band.
pub fn pale_trunk_top(_ctx: Ctx) -> Tex {
    trunk_top_with(seed("pale_trunk_top"), &pale_wood_ramp(), &pale_bark_ramp(), 1.9, 1.0)
}

/// Gnarled trunk side: near-black bark in thick twisted ridges split by
/// deep furrows, the ridges winding across the trunk as they climb.
pub fn gnarled_trunk_side(_ctx: Ctx) -> Tex {
    let salt = seed("gnarled_trunk_side");
    let stretch = 0.32;
    let plates = Ctx::BASE.sites_stretched(salt, 6.0, stretch);
    let tone = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        // Twist: one full sideways swing per tile height, plus knotty warp.
        let twist = (fy / SIZEF * std::f32::consts::TAU).sin() * 3.0;
        let wx = fx + twist + (fbm(salt ^ 1, 2, 2, 2 + fine_octaves(), fx, fy) - 0.5) * 3.0;
        let k = cell(&plates, wx, fy, stretch);
        let p = &plates[k.index];
        if k.edge < if hires() { 0.6 } else { 0.85 } {
            return 0.0;
        }
        let mut t = 0.52 + (p.r[0] - 0.5) * 0.3;
        // Rounded ridges: bright down their left shoulder, dark on the right.
        t -= k.dx * 0.1;
        t += k.rim(2.2) * 0.3;
        // Furrow walls fall into shadow.
        t -= (1.0 - smoothstep(1.0, 2.0, k.edge)) * 0.15;
        t += (value(salt ^ 2, 16, 2, wx, fy) - 0.5) * 0.2;
        t
    });
    paint(&gnarled_bark_ramp(), &tone)
}

/// Gnarled trunk top: dark wood in wobbling rings and a thick bark band.
pub fn gnarled_trunk_top(_ctx: Ctx) -> Tex {
    trunk_top_with(
        seed("gnarled_trunk_top"),
        &gnarled_wood_ramp(),
        &gnarled_bark_ramp(),
        2.8,
        3.6,
    )
}

// ----------------------------------------------------------------- leaves

/// Fronds: dark needle sprays in three layers, each a stem with paired
/// needles slanting forward, lit from the top left.
pub fn fronds(ctx: Ctx) -> Tex {
    let salt = seed("fronds");
    let mut tone = Field::from_fn(|x, y| 0.02 + ctx.fbm(salt, 4, 4, 2 + fine_octaves(), c(x), c(y)) * 0.12);
    let n = res();
    let du = if hires() { 1.0 / detail() } else { 1.0 };
    for (i, &(base, spacing)) in [(0.36, 6.5), (0.56, 7.0), (0.76, 7.5)].iter().enumerate() {
        for s in ctx.sites(hash2(salt, i as u32 + 1), spacing * du) {
            // Sprays droop: mostly pointing down and to either side.
            let ang = std::f32::consts::FRAC_PI_2 + (s.r[0] - 0.5) * 2.2;
            let (ca, sa) = (ang.cos(), ang.sin());
            let len = (5.0 + s.r[1] * 3.0) * du;
            let lt = base + (s.r[3] - 0.5) * 0.1;
            let tip = (s.x + ca * len, s.y + sa * len);
            let steps = (len / (1.3 * du)) as u32;
            for j in 1..=steps {
                let f = j as f32 / steps as f32;
                let (px, py) = (s.x + ca * len * f, s.y + sa * len * f);
                let nl = (2.8 - f * 1.4) * du;
                for side in [-1.0f32, 1.0] {
                    let a = ang + side * 0.85;
                    let q = (px + a.cos() * nl, py + a.sin() * nl);
                    // The side facing the light is brighter.
                    let lit = -(a.cos() + a.sin()) * 0.06;
                    stroke((px, py), q, 0.9 * du, 0.5 * du, |x, y, along| {
                        tone.v[y * n + x] = lt + lit + along * 0.1;
                    });
                }
            }
            stroke((s.x, s.y), tip, 1.0 * du, 0.6 * du, |x, y, _| {
                tone.v[y * n + x] = lt - 0.1;
            });
        }
    }
    paint(&fronds_ramp(), &tone)
}

/// Glossy foliage: broad yellow-green leaves in two layers, each with a
/// dark midrib and a bright waxy highlight near its top-left shoulder.
pub fn glossy_foliage(ctx: Ctx) -> Tex {
    let salt = seed("glossy_foliage");
    let mut tone = Field::from_fn(|x, y| 0.08 + ctx.fbm(salt, 4, 4, 2 + fine_octaves(), c(x), c(y)) * 0.14);
    let n = res();
    let du = if hires() { 1.0 / detail() } else { 1.0 };
    for (i, &(base, spacing)) in [(0.36, 6.4), (0.56, 7.2)].iter().enumerate() {
        for leaf in ctx.sites(hash2(salt, i as u32 + 1), spacing * du) {
            let ang = leaf.r[0] * std::f32::consts::TAU;
            let (ca, sa) = (ang.cos(), ang.sin());
            let len = (4.2 + leaf.r[1] * 1.6) * du;
            let wid = (2.4 + leaf.r[2] * 0.8) * du;
            let lt = base + (leaf.r[3] - 0.5) * 0.12;
            for y in 0..n {
                for x in 0..n {
                    let dx = wrap_delta(c(x) - leaf.x);
                    let dy = wrap_delta(c(y) - leaf.y);
                    let u = dx * ca + dy * sa;
                    let v = -dx * sa + dy * ca;
                    let taper = 1.0 - (u / len + 1.0) * 0.22;
                    let q = (u / len).powi(2) + (v / (wid * taper)).powi(2);
                    if q >= 1.0 {
                        continue;
                    }
                    // Dome light from the top left.
                    let l = -(dx + dy) / (len + wid);
                    let mut t = lt + l * 0.35 + (1.0 - q) * 0.08;
                    // Waxy highlight: a bright crescent on the lit shoulder.
                    let hx = -(len + wid) * 0.18;
                    let h = ((dx - hx).powi(2) + (dy - hx).powi(2)).sqrt();
                    if q < 0.75 && h < 1.4 * du && l > 0.05 {
                        t = 0.98;
                    } else if q < 0.8 && h < 2.3 * du && l > 0.0 {
                        t += 0.16;
                    }
                    if v.abs() < 0.45 * du && u.abs() < len * 0.85 && t < 0.9 {
                        t -= 0.12;
                    }
                    // Dark edge, so overlapping leaves part cleanly.
                    if q > 0.82 {
                        t -= 0.1;
                    }
                    tone.v[y * n + x] = t;
                }
            }
        }
    }
    paint(&glossy_ramp(), &tone)
}

/// The review sheet of this milestone's art (`docs/media/m19-materials.png`):
/// each new texture beside the stone, soil, turf, trunk and foliage it joins.
pub fn review_sheet() -> Result<crate::canvas::Canvas, String> {
    let all = crate::families()?;
    let names = [
        "stone",
        "basalt",
        "limestone",
        "deep_stone",
        "sandstone_side",
        "soil",
        "clay",
        "peat",
        "mud",
        "ash",
        "turf_top",
        "meadow_turf_top",
        "moss_turf_top",
        "turf_side",
        "meadow_turf_side",
        "moss_turf_side",
        "trunk_side",
        "pale_trunk_side",
        "gnarled_trunk_side",
        "trunk_top",
        "pale_trunk_top",
        "gnarled_trunk_top",
        "foliage",
        "fronds",
        "glossy_foliage",
    ];
    let fams: Vec<crate::Family> = names
        .into_iter()
        .map(|n| crate::find(&all, n).clone())
        .map(|mut f| {
            f.faces.truncate(1);
            f
        })
        .collect();
    Ok(crate::sheet::contact_sheet(&fams, &[], &[]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each new texture is told apart from its neighbours at a distance
    /// (art direction rule 2): its average colour differs from the older
    /// block it joins and from the others of its kind.
    #[test]
    fn the_new_materials_differ_from_their_kin_at_a_distance() {
        let groups: [&[&str]; 5] = [
            &["stone", "deep_stone", "basalt", "limestone", "sandstone_side"],
            &["soil", "clay", "peat", "mud", "ash"],
            &["turf_top", "meadow_turf_top", "moss_turf_top", "foliage"],
            &["trunk_side", "pale_trunk_side", "gnarled_trunk_side"],
            &["foliage", "fronds", "glossy_foliage"],
        ];
        for group in groups {
            let means: Vec<_> = group
                .iter()
                .map(|n| crate::materials::paint_named(n, 0).unwrap().mean())
                .collect();
            for i in 0..group.len() {
                for j in 0..i {
                    let (a, b) = (means[i], means[j]);
                    let d = (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs();
                    assert!(d > 0.06, "`{}` and `{}` look alike from afar ({d})", group[i], group[j]);
                }
            }
        }
    }

    #[test]
    fn every_new_texture_is_painted_opaque() {
        for n in BLOCKS {
            let t = crate::materials::paint_named(n, 0).unwrap_or_else(|| panic!("no painter for `{n}`"));
            assert!(t.px.iter().all(|p| p.a >= 1.0), "`{n}` has transparent pixels");
        }
    }
}
