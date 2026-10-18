//! One function per material. Each paints a 32 x 32 face from tileable
//! fields through a few hand-picked ramps, with light from the top left.

use crate::color::{Ramp, Rgba};
use crate::noise::{Site, cell, hash, hash2, seed, unit, wrap_delta, wrap_i};
use crate::stations;
use crate::tex::{Ctx, Field, SIZE, SIZEF, Tex, c, paint, smoothstep};

// ---------------------------------------------------------------- palettes

pub(crate) fn stone_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x2b2d45, 0x3f4560, 0x56607a, 0x6f7b93, 0x8995ab, 0xa5b0c2, 0xc4cbd6, 0xe4e2dc,
    ])
}

fn deep_stone_ramp() -> Ramp {
    Ramp::from_hex(&[0x1c1a2c, 0x2a2740, 0x3a3654, 0x4b4766, 0x5e5a7a, 0x75718f, 0x938ca8])
}

fn bedrock_ramp() -> Ramp {
    Ramp::from_hex(&[0x0c0a12, 0x15111f, 0x1f192d, 0x2a223c, 0x372c4e, 0x463963, 0x5c4a80])
}

fn soil_ramp() -> Ramp {
    Ramp::from_hex(&[0x2e1a1e, 0x47271f, 0x643826, 0x7f4c2f, 0x99623a, 0xb07a4a, 0xc8965f])
}

fn grass_ramp() -> Ramp {
    Ramp::from_hex(&[0x1b4a3a, 0x22633c, 0x2f7f3c, 0x43993a, 0x62b23a, 0x86c940, 0xb4de58])
}

fn sand_ramp() -> Ramp {
    Ramp::from_hex(&[0x8a5a3c, 0xb07a46, 0xcd9a56, 0xe2b866, 0xefcf80, 0xf7e2a0, 0xfff2c8])
}

fn snow_ramp() -> Ramp {
    Ramp::from_hex(&[0x7b8cc4, 0x9caad6, 0xbac6e6, 0xd4ddf2, 0xe8eefa, 0xf6f9ff, 0xfffcf4])
}

fn water_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x183a8a, 0x1f4fa8, 0x2a68c2, 0x3a84d6, 0x55a2e4, 0x7ec2f0, 0xb8e4fa, 0xeefbff,
    ])
}

pub(crate) fn bark_ramp() -> Ramp {
    Ramp::from_hex(&[0x2a1820, 0x3e2420, 0x573424, 0x6e4529, 0x85582f, 0x9d6e3a, 0xb8874a])
}

pub(crate) fn end_grain_ramp() -> Ramp {
    Ramp::from_hex(&[0x5c3a26, 0x7a5232, 0x98693e, 0xb3814c, 0xc99a5e, 0xddb574, 0xecd091])
}

pub(crate) fn plank_ramp() -> Ramp {
    Ramp::from_hex(&[0x4a2a1e, 0x6e4026, 0x8f5a30, 0xac743c, 0xc48d4a, 0xd8a85c, 0xecc477])
}

fn brick_ramp() -> Ramp {
    Ramp::from_hex(&[0x4a1a24, 0x6e2428, 0x8f332c, 0xad4632, 0xc45c3a, 0xd87a48, 0xe99c62])
}

fn mortar_ramp() -> Ramp {
    Ramp::from_hex(&[0x5a5064, 0x7e7584, 0xa29aa2, 0xc4bab6, 0xe0d6cc])
}

fn foliage_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x0f2f2c, 0x164236, 0x1d5a3a, 0x26733c, 0x34903c, 0x4aaa3e, 0x6ec244, 0x9cd856,
    ])
}

fn crystal_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x14526a, 0x1d7690, 0x2b9db0, 0x46c2cc, 0x74dce0, 0xa6eef0, 0xd4fbfa, 0xffffff,
    ])
}

pub(crate) fn copper_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x4a1a14, 0x7a2e18, 0xa8461e, 0xd0672a, 0xec8c3a, 0xfbb456, 0xffd98a, 0xfff3c4,
    ])
}

pub(crate) fn coal_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x0a0810, 0x13101c, 0x1d1a2a, 0x29263a, 0x3a3850, 0x5a5a78, 0x9a9cc8, 0xe0e4ff,
    ])
}

/// Pebble kinds for gravel: blue-grey, warm grey, tan, slate.
fn pebble_ramps() -> [Ramp; 4] {
    [
        Ramp::from_hex(&[0x3c4258, 0x59627c, 0x7c879e, 0xa3adbf, 0xccd2dc]),
        Ramp::from_hex(&[0x4a3c3c, 0x6b5a54, 0x8e7c70, 0xb19f8e, 0xd4c4ae]),
        Ramp::from_hex(&[0x5a4030, 0x80603f, 0xa8824f, 0xc9a466, 0xe6c888]),
        Ramp::from_hex(&[0x2a2a3a, 0x3c3c52, 0x52546c, 0x6c7088, 0x8e94aa]),
    ]
}

// ------------------------------------------------------------------ rocks

/// Shared recipe for cellular rock: slabs from a Voronoi pattern, each with
/// its own tone and tilt, lit on its top-left rim and shaded on its
/// bottom-right, with dark cracks along some (not all) of the joins.
struct RockStyle {
    salt: u32,
    /// Minimum distance between slab centres: the slab size.
    spacing: f32,
    /// Vertical scale of the metric: > 1 makes flat strata.
    stretch: f32,
    /// Rim width in pixels.
    bevel: f32,
    /// How much of the network of joins is open crack (0..1).
    cracks: f32,
    /// Middle tone and per-slab spread.
    tone: f32,
    spread: f32,
    /// How strongly each slab tilts towards or away from the light.
    tilt: f32,
    /// Strength of the rim lighting and of the soft interior mottling.
    rim: f32,
    mottle: f32,
}

/// Per-pixel tone of a rock face.
fn rock(ctx: Ctx, s: &RockStyle) -> Field {
    let sites = ctx.sites_stretched(s.salt, s.spacing, s.stretch);
    Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&sites, fx, fy, s.stretch);
        let site = &sites[k.index];
        // Slab tone and a tilt, so neighbouring slabs differ.
        let mut t = s.tone + (site.r[0] - 0.5) * s.spread;
        t += ((site.r[1] - 0.5) * k.dx + (site.r[2] - 0.5) * k.dy) * s.tilt;
        // Soft mottling inside the slab, in clumps.
        t += (ctx.fbm(s.salt ^ 0x51, 4, 4, 2, fx, fy) - 0.5) * s.mottle;
        // Rim light: top-left edges catch light, bottom-right edges fall away.
        t += k.rim(s.bevel) * s.rim;
        // Open cracks along part of the join network; elsewhere a soft groove.
        let open = ctx.value(s.salt ^ 0x77, 4, 4, fx, fy) < s.cracks;
        if k.edge < 0.55 && open {
            t = 0.03;
        } else if k.edge < 1.2 {
            t -= if open { 0.1 } else { 0.06 };
        }
        t
    })
}

fn stone_style() -> RockStyle {
    RockStyle {
        salt: seed("stone"),
        spacing: 9.0,
        stretch: 1.45,
        bevel: 2.4,
        cracks: 0.4,
        tone: 0.56,
        spread: 0.2,
        tilt: 0.03,
        rim: 0.22,
        mottle: 0.2,
    }
}

/// Stone: cool blue-grey slabs, a little layered, with lit rims and dark
/// cracks between some of them. The everyday rock of the world.
pub fn stone(ctx: Ctx) -> Tex {
    paint(&stone_ramp(), &rock(ctx, &stone_style()))
}

/// Deep stone: darker, purple-leaning, in long flat strata.
pub fn deep_stone(ctx: Ctx) -> Tex {
    let style = RockStyle {
        salt: seed("deep_stone"),
        spacing: 8.5,
        stretch: 2.2,
        bevel: 2.0,
        cracks: 0.75,
        tone: 0.52,
        spread: 0.22,
        tilt: 0.02,
        rim: 0.2,
        mottle: 0.18,
    };
    paint(&deep_stone_ramp(), &rock(ctx, &style))
}

/// Bedrock: near-black and purple, small tight blocks, dense with cracks,
/// only faint highlights. Nothing gets through it.
pub fn bedrock(ctx: Ctx) -> Tex {
    let style = RockStyle {
        salt: seed("bedrock"),
        spacing: 6.0,
        stretch: 1.0,
        bevel: 1.8,
        cracks: 0.9,
        tone: 0.5,
        spread: 0.3,
        tilt: 0.03,
        rim: 0.26,
        mottle: 0.15,
    };
    paint(&bedrock_ramp(), &rock(ctx, &style))
}

// ------------------------------------------------------------------- soil

/// Soil: warm brown crumbs in soft clumps, purple-brown shadow between some
/// of them, a few dark specks and small pebbles. Crumbly, not rocky.
pub fn soil(ctx: Ctx) -> Tex {
    let salt = seed("soil");
    let clods = ctx.sites(salt, 4.6);
    let tone = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&clods, fx, fy, 1.0);
        let site = &clods[k.index];
        let mut t = 0.5 + (site.r[0] - 0.5) * 0.16;
        t += (ctx.fbm(salt ^ 1, 4, 4, 2, fx, fy) - 0.5) * 0.45;
        t += k.rim(2.2) * 0.2;
        let open = ctx.value(salt ^ 2, 8, 8, fx, fy) < 0.3;
        if k.edge < 0.6 && open {
            t = 0.08;
        }
        t
    });
    let mut tex = paint(&soil_ramp(), &tone);
    specks(ctx, &mut tex, salt ^ 4, 5.5, 0.45, Rgba::hex(0x3a2022));
    pebbles(ctx, &mut tex, salt ^ 3, 11.0, 0.6, &pebble_ramps()[1]);
    tex
}

/// Little dark two- and three-pixel clumps.
fn specks(ctx: Ctx, tex: &mut Tex, salt: u32, spacing: f32, chance: f32, col: Rgba) {
    for s in ctx.sites(salt, spacing).into_iter().filter(|s| s.r[0] < chance) {
        let (x, y) = (s.x as i32, s.y as i32);
        let shape: &[(i32, i32)] = if s.r[1] < 0.5 {
            &[(0, 0), (1, 0)]
        } else {
            &[(0, 0), (0, 1), (1, 1)]
        };
        for &(dx, dy) in shape {
            let (px, py) = (wrap_i(x + dx), wrap_i(y + dy));
            tex.set(px, py, tex.get(px, py).mix(col, 0.7));
        }
    }
}

/// Sprinkles small domed pebbles over a texture.
fn pebbles(ctx: Ctx, tex: &mut Tex, salt: u32, spacing: f32, chance: f32, ramp: &Ramp) {
    let sites: Vec<Site> = ctx
        .sites(salt, spacing)
        .into_iter()
        .filter(|s| s.r[0] < chance)
        .collect();
    for s in &sites {
        let r = 1.0 + s.r[1] * 0.9;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let dx = wrap_delta(c(x) - s.x);
                let dy = wrap_delta(c(y) - s.y);
                let d = (dx * dx + dy * dy * 1.3).sqrt();
                if d < r {
                    // Lit from the top left.
                    let l = -(dx + dy) / (r * 2.0);
                    tex.set(x, y, ramp.pick(0.5 + l * 0.9 + (s.r[2] - 0.5) * 0.2, x, y));
                } else if d < r + 0.9 && dx + dy > 0.4 {
                    // Contact shadow on the bottom-right side.
                    let base = tex.get(x, y);
                    tex.set(x, y, base.mix(Rgba::hex(0x2e1a1e), 0.45));
                }
            }
        }
    }
}

// ------------------------------------------------------------------ grass

/// Grass seen from above: soft clumps lit on their top left, darker hollows,
/// and short blades standing out of the clumps with a lit tip and a shadow.
fn grass(ctx: Ctx, salt: u32) -> Tex {
    let ramp = grass_ramp();
    let tufts = ctx.sites(salt, 5.5);
    let tone = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&tufts, fx, fy, 1.0);
        let site = &tufts[k.index];
        let mut t = 0.46 + (site.r[0] - 0.5) * 0.2;
        t += (1.0 - (k.d1 / 4.0).min(1.0)) * 0.1;
        t += k.rim(2.4) * 0.22;
        t += (ctx.fbm(salt ^ 9, 2, 2, 2, fx, fy) - 0.5) * 0.4;
        let hollow = ctx.value(salt ^ 4, 8, 8, fx, fy) < 0.4;
        if k.edge < 0.7 && hollow {
            t -= 0.22;
        }
        t
    });
    let mut tex = paint(&ramp, &tone);
    // Tufts of three blades standing out of the clumps: a tall middle
    // blade and two shorter ones splaying out, lit tips, a shadow at the foot.
    for b in ctx.sites(salt ^ 0x1234, 5.0) {
        if b.r[0] > 0.7 {
            continue;
        }
        let (x, y) = (b.x as i32, b.y as i32);
        let tall = 3 + (b.r[1] * 1.5) as i32;
        let lit = 0.72 + b.r[3] * 0.28;
        for (dx, len, top) in [(-1, tall - 1, 1), (0, tall, 0), (1, tall - 1, 1)] {
            for i in 0..len {
                let px = wrap_i(x + dx);
                let py = wrap_i(y + top + i);
                let t = lit - i as f32 * 0.1 - (dx == 1) as u8 as f32 * 0.12;
                tex.set(px, py, ramp.pick(t, px, py));
            }
        }
        for dx in 0..=2 {
            let (sx, sy) = (wrap_i(x + dx), wrap_i(y + tall + 1 - (dx == 0) as i32));
            tex.set(sx, sy, tex.get(sx, sy).mix(ramp.darkest(), 0.4));
        }
    }
    tex
}

/// Turf top: rich green grass.
pub fn turf_top(ctx: Ctx) -> Tex {
    grass(ctx, seed("turf_top"))
}

/// Turf side: the soil face, with the grass hanging over its upper edge in
/// a ragged painted fringe and a soft shadow under the fringe.
pub fn turf_side(_ctx: Ctx) -> Tex {
    let mut tex = soil(Ctx::BASE);
    let salt = seed("turf_side");
    let grass = grass_ramp();
    // Fringe depth per column, in clumps of a few columns, plus a few
    // longer blades that hang further.
    let depth: Vec<f32> = (0..SIZE)
        .map(|x| {
            let fx = c(x);
            let mut d = 4.0 + crate::noise::fbm(salt, 8, 1, 2, fx, 0.5) * 5.0;
            let blade = unit(hash2(salt, x as u32));
            if blade > 0.78 {
                d += 2.0 + (blade - 0.78) * 12.0;
            }
            d
        })
        .collect();
    for x in 0..SIZE {
        let d = depth[x];
        for y in 0..SIZE {
            let fy = y as f32;
            if fy < d.floor() {
                // Lighter at the top where the light reaches, darker inside.
                let t = 0.72 - fy / 9.0 * 0.45 + (crate::noise::value(salt ^ 5, 8, 4, c(x), c(y)) - 0.5) * 0.3;
                // Lit left edge of each clump.
                let left = depth[(x + SIZE - 1) % SIZE];
                let t = if left < fy + 1.0 { t + 0.18 } else { t };
                tex.set(x, y, grass.pick(t, x, y));
            } else if fy < d + 1.5 {
                // Shadow cast by the fringe onto the soil.
                let base = tex.get(x, y);
                tex.set(x, y, base.mix(Rgba::hex(0x24121e), 0.55));
            }
        }
    }
    tex
}

// ------------------------------------------------------------------- sand

/// Sand: warm gold with wind ripples (gentle on the windward side, a short
/// shaded lee) and a few darker and paler grains in clumps.
pub fn sand(ctx: Ctx) -> Tex {
    let salt = seed("sand");
    let height = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let warp = (ctx.fbm(salt, 2, 2, 2, fx, fy) - 0.5) * 6.0;
        // Four crests down the tile, slightly diagonal; integer frequencies tile.
        let phase = ((fx * 1.0 + fy * 4.0) / SIZEF + warp / SIZEF * 4.0).rem_euclid(1.0);
        let ripple = if phase < 0.75 {
            phase / 0.75
        } else {
            (1.0 - phase) / 0.25
        };
        ripple * 0.6 + ctx.fbm(salt ^ 1, 4, 4, 2, fx, fy) * 0.4
    });
    let light = height.blur().light();
    let tone = Field::from_fn(|x, y| 0.55 + light.at(x, y) * 1.6 + (height.at(x, y) - 0.5) * 0.15);
    let mut tex = paint(&sand_ramp(), &tone);
    grains(ctx, &mut tex, salt ^ 2, Rgba::hex(0xa86a42), Rgba::hex(0xfff6dc));
    tex
}

/// Little two-pixel clumps of darker and paler grains.
fn grains(ctx: Ctx, tex: &mut Tex, salt: u32, dark: Rgba, pale: Rgba) {
    for s in ctx.sites(salt, 5.0) {
        if s.r[0] > 0.45 {
            continue;
        }
        let (x, y) = (s.x as usize % SIZE, s.y as usize % SIZE);
        let col = if s.r[1] < 0.5 { dark } else { pale };
        tex.set(x, y, col);
        let (nx, ny) = if s.r[2] < 0.5 {
            ((x + 1) % SIZE, y)
        } else {
            (x, (y + 1) % SIZE)
        };
        tex.set(nx, ny, tex.get(nx, ny).mix(col, 0.6));
    }
}

// ----------------------------------------------------------------- gravel

/// Gravel: many small rounded pebbles of mixed kinds packed together, each
/// lit on its top left, with dark gaps between. Busier and rounder than stone.
pub fn gravel(ctx: Ctx) -> Tex {
    let salt = seed("gravel");
    let sites = ctx.sites(salt, 4.2);
    let ramps = pebble_ramps();
    let gap = Rgba::hex(0x221a26);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&sites, fx, fy, 1.0);
        let site = &sites[k.index];
        if k.edge < 0.5 {
            return gap;
        }
        let kind = (site.r[0] * 4.0) as usize % 4;
        let mut t = 0.5 + (site.r[1] - 0.5) * 0.3;
        t += k.rim(2.0) * 0.35;
        t += (1.0 - (k.d1 / 3.0).min(1.0)) * 0.12;
        ramps[kind].pick(t, x, y)
    })
}

// ------------------------------------------------------------------- snow

/// Snow: soft drifts lit from the top left, pale blue-lavender in the
/// hollows, near-white on the crests, with a rare glint.
pub fn snow(ctx: Ctx) -> Tex {
    let salt = seed("snow");
    let height = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let wx = fx + (ctx.fbm(salt, 2, 2, 2, fx, fy) - 0.5) * 6.0;
        let wy = fy + (ctx.fbm(salt ^ 1, 2, 2, 2, fx, fy) - 0.5) * 6.0;
        ctx.fbm(salt ^ 2, 2, 2, 2, wx, wy)
    })
    .blur()
    .blur();
    // Lambert-ish light from the top left on the smoothed drifts.
    let tone = Field::from_fn(|x, y| {
        let (ix, iy) = (x as i32, y as i32);
        let slope =
            (height.get(ix + 1, iy) - height.get(ix - 1, iy)) + (height.get(ix, iy + 1) - height.get(ix, iy - 1));
        0.64 + slope * 2.0 + (height.at(x, y) - 0.5) * 0.45
    });
    let mut tex = paint(&snow_ramp(), &tone);
    // A rare glint on the brightest crests.
    for s in ctx.sites(salt ^ 3, 9.0) {
        if s.r[0] < 0.45 {
            let (x, y) = (s.x as usize % SIZE, s.y as usize % SIZE);
            if tone.at(x, y) > 0.68 {
                tex.set(x, y, Rgba::hex(0xffffff));
                let nx = (x + 1) % SIZE;
                tex.set(nx, y, tex.get(nx, y).mix(Rgba::hex(0xffffff), 0.5));
            }
        }
    }
    tex
}

// ------------------------------------------------------------------ water

/// Water: deep saturated blue in soft swells, with broken bright wave
/// lines along the crests; mostly opaque so it reads as a body, not a tint.
pub fn water(ctx: Ctx) -> Tex {
    let salt = seed("water");
    let ramp = water_ramp();
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let wx = fx + (ctx.fbm(salt ^ 1, 2, 2, 2, fx, fy) - 0.5) * 10.0;
        let wy = fy + (ctx.fbm(salt ^ 2, 2, 2, 2, fx, fy) - 0.5) * 10.0;
        let swell = ctx.fbm(salt ^ 3, 2, 2, 2, wx, wy);
        let mut t = 0.22 + swell * 0.36;
        // Crest lines: contours of a second field, broken into strokes.
        let w = ctx.fbm(salt ^ 5, 2, 4, 2, wx, wy) * 4.0;
        let band = w.fract();
        let crest = 1.0 - smoothstep(0.0, 0.16, band);
        let lip = smoothstep(0.84, 1.0, band) * 0.5;
        let broken = ctx.value(salt ^ 4, 8, 8, fx, fy) > 0.32;
        let lit = if broken { crest.max(lip) } else { lip * 0.5 };
        t += lit * 0.5;
        ramp.pick(t, x, y).with_alpha(0.8 + lit * 0.08)
    })
}

// ------------------------------------------------------------------- wood

/// Trunk side: bark in tall plates split by deep furrows, each plate lit on
/// its left rim, purple-brown shadow in the cracks.
pub fn trunk_side(_ctx: Ctx) -> Tex {
    let salt = seed("trunk_side");
    let stretch = 0.3;
    let plates = Ctx::BASE.sites_stretched(salt, 4.2, stretch);
    let tone = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let wx = fx + (crate::noise::fbm(salt ^ 1, 1, 4, 2, fx, fy) - 0.5) * 3.0;
        let k = cell(&plates, wx, fy, stretch);
        let p = &plates[k.index];
        if k.edge < 0.55 {
            return 0.02;
        }
        let mut t = 0.5 + (p.r[0] - 0.5) * 0.25;
        // Plates bulge: lit on the left, shaded on the right.
        t -= k.dx * 0.07;
        t += k.rim(1.6) * 0.2;
        // Fine vertical grain inside the plate.
        t += (crate::noise::value(salt ^ 2, 16, 2, wx, fy) - 0.5) * 0.25;
        t
    });
    paint(&bark_ramp(), &tone)
}

/// Trunk top: end grain in warm rings around a dark heart, a radial check
/// crack, and a band of bark around the square edge.
pub fn trunk_top(_ctx: Ctx) -> Tex {
    let salt = seed("trunk_top");
    let wood = end_grain_ramp();
    let bark = bark_ramp();
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x) - 16.0, c(y) - 16.0);
        // Rounded square near the edge, round in the middle.
        let round = fx.hypot(fy);
        let square = (fx.powi(4) + fy.powi(4)).powf(0.25);
        let r = round + (square - round) * smoothstep(6.0, 14.0, round);
        let r = r + (crate::noise::fbm(salt, 4, 4, 2, c(x), c(y)) - 0.5) * 1.6;
        if r > 13.6 {
            // Bark band, lit on the top-left of the block.
            let t = 0.45 + (r - 13.6) * 0.05 - (fx + fy) / 32.0 * 0.4;
            return bark.pick(t, x, y);
        }
        if r > 12.8 {
            return bark.at(1);
        }
        let ring = (r / 2.4).fract();
        let mut t = 0.6 - smoothstep(0.65, 0.95, ring) * 0.35;
        t -= (fx + fy) / 32.0 * 0.12;
        t -= smoothstep(10.0, 13.0, r) * 0.1;
        if r < 1.6 {
            t = 0.12;
        }
        // One check crack from the heart towards the lower right.
        let ang = fy.atan2(fx);
        let crack = (ang - 0.7).abs() < 0.11 + 0.03 * (r / 4.0).sin();
        if crack && r > 2.0 && r < 11.0 {
            t = 0.02;
        }
        wood.pick(t, x, y)
    })
}

/// Timber: four planks with long grain, staggered butt joints, lit top
/// edges, dark gaps and a pair of nail heads at each joint.
pub fn timber(_ctx: Ctx) -> Tex {
    let salt = seed("timber");
    let ramp = plank_ramp();
    let plank_h = 8;
    let joints = [5usize, 21, 12, 28];
    Tex::from_fn(|x, y| {
        let p = y / plank_h;
        let row = y % plank_h;
        let tone_off = (unit(hash2(salt, p as u32)) - 0.5) * 0.2;
        let joint = joints[p];
        // Grain: long streaks along the plank.
        let g = crate::noise::fbm(hash2(salt, p as u32 + 7), 2, 8, 2, c(x), c(y));
        let mut t = 0.5 + tone_off + (smoothstep(0.35, 0.75, g) - 0.5) * 0.35;
        if row == 0 {
            t += 0.25;
        } else if row == plank_h - 2 {
            t -= 0.12;
        } else if row == plank_h - 1 {
            return ramp.at(0);
        }
        if x == joint {
            return ramp.at(0);
        }
        if x == (joint + 1) % SIZE {
            t += 0.2;
        }
        if x == (joint + SIZE - 1) % SIZE {
            t -= 0.12;
        }
        // Nail heads either side of the joint.
        let nail_x = [(joint + SIZE - 2) % SIZE, (joint + 2) % SIZE];
        if (row == 3) && nail_x.contains(&x) {
            return Rgba::hex(0x3a3440);
        }
        if row == 2 && nail_x.contains(&x) {
            return Rgba::hex(0xb8b0b8);
        }
        ramp.pick(t, x, y)
    })
}

// ------------------------------------------------------------------ brick

/// Brick: running bond of 16 x 8 bricks, each its own shade of red, bevelled
/// light on the top left and dark on the bottom right, a few chips, set in
/// warm mortar that sits in the bricks' shadow.
pub fn brick(_ctx: Ctx) -> Tex {
    let salt = seed("brick");
    let ramp = brick_ramp();
    let mortar = mortar_ramp();
    Tex::from_fn(|x, y| {
        let row = y / 8;
        let off = if row % 2 == 0 { 0 } else { 8 };
        let bx = (x + off) % SIZE;
        let col = bx / 16;
        let (lx, ly) = (bx % 16, y % 8);
        if ly == 7 || lx == 15 {
            // Mortar, shaded just under and right of a brick.
            let t = if ly == 7 && lx < 15 { 0.35 } else { 0.55 };
            let n = crate::noise::value(salt ^ 3, 8, 8, c(x), c(y));
            return mortar.pick(t + (n - 0.5) * 0.3, x, y);
        }
        let id = hash2(salt, (row * 2 + col) as u32);
        let mut t = 0.48 + (unit(id) - 0.5) * 0.25;
        t += (crate::noise::fbm(hash(id), 4, 4, 2, c(x), c(y)) - 0.5) * 0.3;
        if ly == 0 || lx == 0 {
            t += 0.22;
        }
        if ly == 6 || lx == 14 {
            t -= 0.2;
        }
        // Chips: small dark clumps near edges.
        let chip_x = (unit(hash2(id, 5)) * 13.0) as usize;
        let chip_y = 1 + (unit(hash2(id, 6)) * 4.0) as usize;
        if (lx == chip_x || lx == chip_x + 1) && ly == chip_y {
            t -= 0.28;
        } else if lx == chip_x + 1 && ly == chip_y + 1 {
            t += 0.12;
        }
        ramp.pick(t, x, y)
    })
}

// ---------------------------------------------------------------- foliage

/// Foliage: leaves in three layers, deep blue-green at the back to bright
/// yellow-green at the front, each leaf lit on its top left.
pub fn foliage(ctx: Ctx) -> Tex {
    let salt = seed("foliage");
    let ramp = foliage_ramp();
    let mut tone = Field::from_fn(|x, y| 0.08 + ctx.fbm(salt, 4, 4, 2, c(x), c(y)) * 0.15);
    let layers = [(0.32, 4.2), (0.5, 4.6), (0.68, 5.2)];
    for (i, &(base, spacing)) in layers.iter().enumerate() {
        let leaves = ctx.sites(hash2(salt, i as u32 + 1), spacing);
        for leaf in &leaves {
            let ang = leaf.r[0] * std::f32::consts::TAU;
            let (ca, sa) = (ang.cos(), ang.sin());
            let len = 2.6 + leaf.r[1] * 1.4;
            let wid = 1.4 + leaf.r[2] * 0.6;
            let lt = base + (leaf.r[3] - 0.5) * 0.12;
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let dx = wrap_delta(c(x) - leaf.x);
                    let dy = wrap_delta(c(y) - leaf.y);
                    let u = dx * ca + dy * sa;
                    let v = -dx * sa + dy * ca;
                    // Teardrop: wide near the stem end, pointed at the tip.
                    let taper = 1.0 - (u / len + 1.0) * 0.25;
                    let q = (u / len).powi(2) + (v / (wid * taper)).powi(2);
                    if q < 1.0 {
                        let l = -(dx + dy) / (len + wid);
                        tone.v[y * SIZE + x] = lt + l * 0.25 + (1.0 - q) * 0.06;
                    }
                }
            }
        }
    }
    paint(&ramp, &tone)
}

// ------------------------------------------------------------------- ores

/// How an ore sits in its host rock.
struct OreStyle {
    salt: u32,
    /// Distance between clusters and their radius.
    spacing: f32,
    radius: f32,
    /// Share of candidate clusters that are kept.
    keep: f32,
    /// Size of the facets inside a cluster.
    facet: f32,
    /// Facet tone range, the tone of the lit rim, and the inner glow.
    tone: f32,
    spread: f32,
    rim_tone: f32,
    glow: f32,
    /// Share of facets that carry a one-pixel glint.
    glints: f32,
    outline: Rgba,
}

/// Sets jewel-like ore clusters into a host texture. A cluster is a group of
/// whole facets (so its outline is cut, not round); each facet is a flat
/// plane tipped towards or away from the light, the cluster has a bright
/// top-left rim and a dark bottom-right edge, a dark outline where it meets
/// the rock, and a soft shadow cast below and right.
fn ore(ctx: Ctx, host: Tex, style: &OreStyle, ramp: &Ramp) -> Tex {
    let clusters: Vec<Site> = ctx
        .sites(style.salt, style.spacing)
        .into_iter()
        .filter(|s| s.r[0] < style.keep)
        .collect();
    let facets = ctx.sites(style.salt ^ 0x0f, style.facet);
    let in_cluster: Vec<bool> = facets
        .iter()
        .map(|f| {
            clusters.iter().any(|s| {
                let r = style.radius * (0.7 + 0.6 * s.r[1]);
                wrap_delta(f.x - s.x).hypot(wrap_delta(f.y - s.y) * 1.2) < r
            })
        })
        .collect();
    let cells: Vec<_> = (0..SIZE * SIZE)
        .map(|i| cell(&facets, c(i % SIZE), c(i / SIZE), 1.0))
        .collect();
    let is_ore = |x: i32, y: i32| in_cluster[cells[wrap_i(y) * SIZE + wrap_i(x)].index];
    let mut out = host.clone();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (ix, iy) = (x as i32, y as i32);
            if is_ore(ix, iy) {
                let k = cells[y * SIZE + x];
                let f = &facets[k.index];
                let mut t = style.tone + (f.r[0] - 0.5) * style.spread;
                t += ((f.r[1] - 0.5) * k.dx + (f.r[2] - 0.5) * k.dy) * 0.05;
                t += (1.0 - (k.d1 / style.facet).min(1.0)) * style.glow;
                t += k.rim(1.3) * 0.2;
                if k.edge < 0.5 {
                    t -= 0.12;
                }
                if !is_ore(ix - 1, iy) || !is_ore(ix, iy - 1) {
                    t = style.rim_tone;
                } else if !is_ore(ix + 1, iy) || !is_ore(ix, iy + 1) {
                    t = 0.1;
                }
                // A glint near the top-left corner of some facets.
                let glint = f.r[3] < style.glints && (k.dx + 1.0).abs() < 0.6 && (k.dy + 1.0).abs() < 0.6;
                let col = if glint { ramp.lightest() } else { ramp.pick(t, x, y) };
                out.set(x, y, col);
            } else {
                let touching = is_ore(ix - 1, iy) || is_ore(ix + 1, iy) || is_ore(ix, iy - 1) || is_ore(ix, iy + 1);
                if touching {
                    out.set(x, y, style.outline);
                } else if is_ore(ix - 1, iy - 1) || is_ore(ix - 2, iy - 1) || is_ore(ix - 1, iy - 2) {
                    out.set(x, y, host.get(x, y).mix(style.outline, 0.4));
                }
            }
        }
    }
    out
}

/// Copper vein: warm orange cut nuggets with gold rims, set in stone.
pub fn copper_vein(ctx: Ctx) -> Tex {
    let style = OreStyle {
        salt: seed("copper_vein"),
        spacing: 10.5,
        radius: 5.6,
        keep: 0.9,
        facet: 2.6,
        tone: 0.5,
        spread: 0.4,
        rim_tone: 0.9,
        glow: 0.2,
        glints: 0.35,
        outline: Rgba::hex(0x3a1a1c),
    };
    ore(ctx, stone(ctx), &style, &copper_ramp())
}

/// Coal seam: near-black glossy lumps with a faint cold glint, in stone.
pub fn coal_seam(ctx: Ctx) -> Tex {
    let style = OreStyle {
        salt: seed("coal_seam"),
        spacing: 10.5,
        radius: 5.2,
        keep: 0.9,
        facet: 3.0,
        tone: 0.3,
        spread: 0.3,
        rim_tone: 0.72,
        glow: 0.05,
        glints: 0.5,
        outline: Rgba::hex(0x14101c),
    };
    ore(ctx, stone(ctx), &style, &coal_ramp())
}

// ---------------------------------------------------------------- crystal

/// Glow crystal: tall prisms of pale cyan crystal, each with a lit left
/// face and a shaded right face, bright hearts as if lit from inside,
/// bright top-left edges, deep teal joins, and a few sparkles.
pub fn glow_crystal(_ctx: Ctx) -> Tex {
    let salt = seed("glow_crystal");
    let stretch = 0.42;
    let prisms = Ctx::BASE.sites_stretched(salt, 5.2, stretch);
    let ramp = crystal_ramp();
    let mut tex = Tex::from_fn(|x, y| {
        let k = cell(&prisms, c(x), c(y), stretch);
        let f = &prisms[k.index];
        if k.edge < 0.5 {
            let r = k.rim(1.0);
            return if r > 0.35 {
                ramp.at(7)
            } else if r < -0.2 {
                ramp.at(1)
            } else {
                ramp.at(3)
            };
        }
        let mut t = 0.52 + (f.r[0] - 0.5) * 0.25;
        // Two faces per prism, split down the middle.
        t += if k.dx < 0.0 { 0.12 } else { -0.1 };
        // Brighter towards the top.
        t -= k.dy * 0.012;
        // Light from inside: bright hearts.
        let heart = (1.0 - k.dx.abs() / 3.0).max(0.0) * (1.0 - k.dy.abs() / 7.0).max(0.0);
        t += heart * 0.35;
        t += k.rim(1.6) * 0.18;
        ramp.pick(t, x, y)
    });
    // Four-point sparkles on a few hearts.
    let white = Rgba::hex(0xffffff);
    for f in prisms.iter().filter(|f| f.r[3] < 0.3) {
        let (x, y) = (f.x as i32, f.y as i32);
        for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (px, py) = (wrap_i(x + dx), wrap_i(y + dy));
            let col = if dx == 0 && dy == 0 {
                white
            } else {
                tex.get(px, py).mix(white, 0.6)
            };
            tex.set(px, py, col);
        }
    }
    tex
}

// ------------------------------------------------------------------ glass

/// Glass: almost clear pane with a pale frame lit on the top and left, and
/// two diagonal highlight streaks across the upper left.
pub fn glass(_ctx: Ctx) -> Tex {
    let pane = Rgba::hex(0xcfeef8).with_alpha(0.14);
    let frame_lit = Rgba::hex(0xf4fbff).with_alpha(0.95);
    let frame = Rgba::hex(0xc4dce8).with_alpha(0.9);
    let frame_dark = Rgba::hex(0x8aa4bc).with_alpha(0.9);
    let streak = Rgba::hex(0xffffff).with_alpha(0.55);
    let last = SIZE - 1;
    Tex::from_fn(|x, y| {
        if y == 0 || x == 0 {
            return frame_lit;
        }
        if y == last || x == last {
            return frame_dark;
        }
        if y == 1 || x == 1 {
            return frame;
        }
        if y == last - 1 || x == last - 1 {
            return frame.mix(frame_dark, 0.5);
        }
        let d = x + y;
        if (12..=14).contains(&d) || (18..=18).contains(&d) {
            return streak;
        }
        pane
    })
}

/// Every texture the game asks for, painted; `variant` is ignored by the
/// materials that have no variants.
pub fn paint_named(name: &str, variant: u32) -> Option<Tex> {
    let ctx = Ctx::new(variant);
    let f: fn(Ctx) -> Tex = match name {
        "bedrock" => bedrock,
        "stone" => stone,
        "deep_stone" => deep_stone,
        "soil" => soil,
        "turf_top" => turf_top,
        "turf_side" => turf_side,
        "sand" => sand,
        "gravel" => gravel,
        "snow" => snow,
        "water" => water,
        "trunk_side" => trunk_side,
        "trunk_top" => trunk_top,
        "foliage" => foliage,
        "coal_seam" => coal_seam,
        "copper_vein" => copper_vein,
        "glow_crystal" => glow_crystal,
        "timber" => timber,
        "brick" => brick,
        "glass" => glass,
        "workbench_top" => stations::workbench_top,
        "workbench_side" => stations::workbench_side,
        "furnace_top" => stations::furnace_top,
        "furnace_side" => stations::furnace_side,
        "claim_stone_top" => stations::claim_stone_top,
        "claim_stone_side" => stations::claim_stone_side,
        _ => return None,
    };
    Some(f(ctx))
}
