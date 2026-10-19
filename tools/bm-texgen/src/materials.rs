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

/// Rust: dull red-brown iron oxide, to a warm ochre where it catches light.
pub(crate) fn rust_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x2a1216, 0x4a1c1c, 0x6c2a22, 0x8c3c28, 0xaa5430, 0xc47040, 0xdc945c, 0xf0be8a,
    ])
}

/// Iron and steel: a cool grey with a clear blue sheen, deeper and bluer
/// than stone, to a near-white specular highlight.
pub(crate) fn steel_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x10142a, 0x1c2544, 0x2c3a62, 0x435680, 0x5f789e, 0x87a0c0, 0xb8cade, 0xeef6ff,
    ])
}

/// Native iron in ore: the steel ramp a step brighter, so the nodules read
/// as silver against the rust.
pub(crate) fn native_iron_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x1c2544, 0x2c3a62, 0x435680, 0x5f789e, 0x87a0c0, 0xb8cade, 0xe2ecf8, 0xffffff,
    ])
}

/// Sandstone: warmer, redder and darker than loose sand.
fn sandstone_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x4e2428, 0x74382e, 0x985236, 0xb86e40, 0xd08c50, 0xe2aa66, 0xf0c886, 0xfae2b0,
    ])
}

/// Ice: deep cold blue in the body to white in the cracks and bubbles.
fn ice_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x24508e, 0x326cae, 0x4688c8, 0x62a6dc, 0x86c2ec, 0xb0daf6, 0xdaf0fc, 0xffffff,
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

// -------------------------------------------------------------- sandstone

/// The sandstone beds down a side face, top to bottom, in pixels (sum 32):
/// hard beds stand proud, soft ones are eroded back under them.
const BEDS: [(usize, bool); 7] = [
    (5, true),
    (3, false),
    (6, true),
    (4, false),
    (6, true),
    (3, false),
    (5, true),
];

/// Sandstone side: layered beds of warm sandstone, each its own shade (some
/// pinker, some paler), the hard ones lit along their top lip, the soft ones
/// shadowed under the overhang and pitted. The bed lines wander a little but
/// tile both ways, so a cliff of sandstone reads as one run of strata.
pub fn sandstone_side(_ctx: Ctx) -> Tex {
    let salt = seed("sandstone_side");
    let ramp = sandstone_ramp();
    let n = BEDS.len();
    let mut tops = vec![0.0f32; n + 1];
    for i in 0..n {
        tops[i + 1] = tops[i] + BEDS[i].0 as f32;
    }
    // Each bed line wobbles along x; the line at 32 is the line at 0.
    let line = |i: usize, fx: f32| {
        let w = crate::noise::fbm(hash2(salt, (i % n) as u32), 4, 1, 2, fx, 0.5) - 0.5;
        tops[i] + w * 2.6
    };
    let pink = Rgba::hex(0xc0605a);
    let cream = Rgba::hex(0xf8e4bc);
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let f = if fy < line(0, fx) { fy + SIZEF } else { fy };
        let i = (0..n).find(|&i| f < line(i + 1, fx)).unwrap_or(n - 1);
        let (depth, thick) = (f - line(i, fx), line(i + 1, fx) - line(i, fx));
        let (_, hard) = BEDS[i];
        let id = hash2(salt ^ 0x33, i as u32);
        let mut t = if hard { 0.6 } else { 0.42 } + (unit(id) - 0.5) * 0.18;
        // Fine laminae along the bed, in clumped streaks.
        let lam = crate::noise::value(hash2(salt, i as u32 + 40), 4, 16, fx, fy);
        t += (smoothstep(0.3, 0.7, lam) - 0.5) * 0.12;
        // Cross-bedding in the thick hard beds: faint diagonal laminae.
        if hard && thick > 5.5 {
            let ph = (fx / SIZEF * 4.0 - depth / 3.0).rem_euclid(1.0);
            if ph < 0.18 {
                t -= 0.07;
            }
        }
        if hard {
            if depth < 1.0 {
                t += 0.24;
            } else if depth < 2.0 {
                t += 0.08;
            }
            if thick - depth < 1.0 {
                t -= 0.16;
            }
        } else {
            // Under the hard bed's overhang.
            if depth < 1.0 {
                t = 0.08;
            } else if depth < 2.0 {
                t -= 0.16;
            }
        }
        let mut col = ramp.pick(t, x, y);
        // Some beds pinker, some paler, in the bed's own measure.
        let tint = unit(hash2(id, 7));
        if tint < 0.35 {
            col = col.mix(pink, 0.22);
        } else if tint > 0.75 {
            col = col.mix(cream, 0.18);
        }
        col
    });
    // Pits in the soft beds, lit on their far (bottom-right) wall.
    for s in Ctx::BASE.sites(salt ^ 0x71, 5.0) {
        if s.r[0] > 0.6 {
            continue;
        }
        let (x, y) = (s.x as usize % SIZE, s.y as usize % SIZE);
        let f = if c(y) < line(0, c(x)) { c(y) + SIZEF } else { c(y) };
        let i = (0..n).find(|&i| f < line(i + 1, c(x))).unwrap_or(n - 1);
        if BEDS[i].1 || f - line(i, c(x)) < 1.5 {
            continue;
        }
        let dark = ramp.at(1);
        tex.set(x, y, dark);
        if s.r[1] < 0.5 {
            tex.set((x + 1) % SIZE, y, dark);
            let (lx, ly) = ((x + 2) % SIZE, y);
            tex.set(lx, ly, tex.get(lx, ly).mix(ramp.at(6), 0.5));
        } else {
            let (lx, ly) = ((x + 1) % SIZE, (y + 1) % SIZE);
            tex.set(lx, ly, tex.get(lx, ly).mix(ramp.at(6), 0.5));
        }
    }
    // A few joints split the hard beds top to bottom: a dark crack with a
    // lit lip on its right.
    for (i, &(_, hard)) in BEDS.iter().enumerate() {
        let id = hash2(salt ^ 0x95, i as u32);
        if !hard || unit(id) > 0.6 {
            continue;
        }
        let x = (unit(hash(id)) * SIZEF) as usize % SIZE;
        for y in 0..SIZE {
            let fx = c(x);
            let f = if c(y) < line(0, fx) { c(y) + SIZEF } else { c(y) };
            let inside = f > line(i, fx) + 1.0 && f < line(i + 1, fx) - 0.5;
            if inside {
                tex.set(x, y, ramp.at(1));
                let r = (x + 1) % SIZE;
                tex.set(r, y, tex.get(r, y).mix(ramp.at(7), 0.35));
            }
        }
    }
    tex
}

/// Sandstone top: a smooth bed plane in broad, barely parted slabs, soft
/// in tone, with rings of rust-pink staining wandering across it and a few
/// grains. Calmer than the side, and redder and stiller than loose sand.
pub fn sandstone_top(ctx: Ctx) -> Tex {
    let salt = seed("sandstone_top");
    let ramp = sandstone_ramp();
    let style = RockStyle {
        salt,
        spacing: 15.0,
        stretch: 1.0,
        bevel: 2.2,
        cracks: 0.2,
        tone: 0.64,
        spread: 0.08,
        tilt: 0.012,
        rim: 0.16,
        mottle: 0.14,
    };
    let tone = rock(ctx, &style);
    let rings = Field::from_fn(|x, y| ctx.fbm(salt ^ 2, 2, 2, 2, c(x), c(y))).blur();
    let pink = Rgba::hex(0xb0504a);
    let mut tex = Tex::from_fn(|x, y| {
        let t = tone.at(x, y);
        // Cracks are shallow in this soft stone.
        let col = ramp.pick(if t < 0.1 { 0.24 } else { t }, x, y);
        // Liesegang rings: bands of iron stain following the contours.
        let band = (rings.at(x, y) * 5.0).fract();
        if band < 0.1 {
            col.mix(pink, 0.34)
        } else if band < 0.2 {
            col.mix(pink, 0.14)
        } else {
            col
        }
    });
    grains(ctx, &mut tex, salt ^ 3, Rgba::hex(0x8a4a32), Rgba::hex(0xfff0d0));
    tex
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

// -------------------------------------------------------------------- ash

/// Ash: near-neutral warm grey, so the shader can give it any palette hue.
/// All the detail is light and shade, in a narrow mid-lightness band.
fn ash_ramp() -> Ramp {
    Ramp::from_hex(&[0x55514f, 0x6a6663, 0x7e7a76, 0x928e89, 0xa6a29c, 0xb9b5ae])
}

/// Ash: fine powder in soft lit clumps, a few darker cinders and faint
/// hairline cracks. Smoother than sand, softer than gravel, greyer than soil.
pub fn ash(ctx: Ctx) -> Tex {
    let salt = seed("ash");
    let clods = ctx.sites(salt, 6.5);
    // Soft clumps: a gently domed height per cell, lit from the top left.
    let height = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&clods, fx, fy, 1.0);
        let site = &clods[k.index];
        let dome = 1.0 - (k.d1 / 5.5).min(1.0);
        dome * 0.5 + site.r[0] * 0.2 + ctx.fbm(salt ^ 1, 4, 4, 2, fx, fy) * 0.35
    })
    .blur();
    let light = height.light();
    let tone = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let fine = ctx.value(salt ^ 2, 16, 16, fx, fy) - 0.5;
        // Faint cracks: thin dark contours of a warped field, broken up.
        let wx = fx + (ctx.fbm(salt ^ 3, 2, 2, 2, fx, fy) - 0.5) * 8.0;
        let wy = fy + (ctx.fbm(salt ^ 4, 2, 2, 2, fx, fy) - 0.5) * 8.0;
        let line = (ctx.fbm(salt ^ 5, 3, 3, 2, wx, wy) * 3.0).fract();
        let crack = (1.0 - smoothstep(0.0, 0.05, line.min(1.0 - line)))
            * if ctx.value(salt ^ 6, 6, 6, fx, fy) > 0.55 {
                1.0
            } else {
                0.0
            };
        0.5 + light.at(x, y) * 1.1 + (height.at(x, y) - 0.5) * 0.35 + fine * 0.14 - crack * 0.2
    });
    let mut tex = paint(&ash_ramp(), &tone);
    specks(ctx, &mut tex, salt ^ 7, 6.0, 0.5, Rgba::hex(0x34302f));
    tex
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
    ore_with(ctx, host, style, |_| ramp)
}

/// The clusters an ore style keeps.
fn ore_clusters(ctx: Ctx, style: &OreStyle) -> Vec<Site> {
    ctx.sites(style.salt, style.spacing)
        .into_iter()
        .filter(|s| s.r[0] < style.keep)
        .collect()
}

/// `ore`, with the ramp chosen per facet (`ramp_of` gets the facet index):
/// for ores of two minerals grown together.
fn ore_with<'a>(ctx: Ctx, host: Tex, style: &OreStyle, ramp_of: impl Fn(usize) -> &'a Ramp) -> Tex {
    let clusters = ore_clusters(ctx, style);
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
                let ramp = ramp_of(k.index);
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

/// Iron vein: knots of rust-red ore grown through with silvery steel-blue
/// nodules, the stone round them stained with rust. Redder and duller than
/// copper's bright orange, with cold metal glints copper does not have.
pub fn iron_vein(ctx: Ctx) -> Tex {
    let style = OreStyle {
        salt: seed("iron_vein"),
        spacing: 10.0,
        radius: 5.4,
        keep: 0.9,
        facet: 2.4,
        tone: 0.46,
        spread: 0.38,
        rim_tone: 0.86,
        glow: 0.1,
        glints: 0.3,
        outline: Rgba::hex(0x2a1418),
    };
    let salt = style.salt;
    let clusters = ore_clusters(ctx, &style);
    let rust = rust_ramp();
    let steel = native_iron_ramp();
    // Rust bleeds into the stone round each cluster, in soft clumps.
    let mut host = stone(ctx);
    let stain = rust.at(3);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (c(x), c(y));
            let d = clusters
                .iter()
                .map(|s| {
                    let r = style.radius * (0.7 + 0.6 * s.r[1]);
                    wrap_delta(fx - s.x).hypot(wrap_delta(fy - s.y) * 1.2) - r
                })
                .fold(f32::MAX, f32::min);
            let k = (1.0 - smoothstep(-1.0, 3.5, d)) * (0.3 + ctx.fbm(salt ^ 3, 8, 8, 1, fx, fy) * 0.5);
            if k > 0.18 {
                host.set(x, y, host.get(x, y).mix(stain, k.min(0.55)));
            }
        }
    }
    let silver = |i: usize| unit(hash2(salt ^ 0x5e, i as u32)) < 0.4;
    ore_with(ctx, host, &style, |i| if silver(i) { &steel } else { &rust })
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

// -------------------------------------------------------------------- ice

/// Ice: clear cold blue in broad plates, each deeper in its middle and lit
/// on its top-left rim, with white fracture lines along some of the joins,
/// strings of trapped air bubbles and a frosty sheen across the upper left.
/// Translucent: the body lets about a third of the light through, the white
/// cracks and bubbles are nearly opaque.
pub fn ice(ctx: Ctx) -> Tex {
    let salt = seed("ice");
    let ramp = ice_ramp();
    let plates = ctx.sites(salt, 11.0);
    // Depth in the ice: soft clouds of deeper and clearer blue.
    let depth = Field::from_fn(|x, y| ctx.fbm(salt ^ 1, 2, 2, 3, c(x), c(y))).blur();
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let k = cell(&plates, fx, fy, 1.0);
        let p = &plates[k.index];
        let mut t = 0.5 + (p.r[0] - 0.5) * 0.1;
        t += (depth.at(x, y) - 0.5) * 0.5;
        let mut a = 0.62 + (0.5 - t) * 0.16;
        // Fractures along a few of the joins: a white line, lit on the side
        // facing the light, with a deep blue shadow beside it.
        let open = ctx.value(salt ^ 2, 4, 4, fx, fy) < 0.34;
        if open && k.edge < 0.55 {
            t = if k.rim(1.0) > -0.2 { 0.97 } else { 0.84 };
            a = 0.9;
        } else if open && k.edge < 1.4 {
            t += if k.rim(2.0) < 0.0 { -0.14 } else { 0.1 };
            a += 0.05;
        }
        // Frost sheen in a diagonal band across the upper left.
        let d = fx + fy;
        if (14.0..16.0).contains(&d) || (19.0..20.0).contains(&d) {
            t += 0.12;
            a += 0.05;
        }
        ramp.pick(t, x, y).with_alpha(a.clamp(0.5, 0.95))
    });
    // Trapped air: little bubbles, lit white at the top left, a blue ring.
    for b in ctx.sites(salt ^ 3, 5.0) {
        if b.r[0] > 0.24 {
            continue;
        }
        let (x, y) = (b.x as i32, b.y as i32);
        let big = b.r[1] < 0.6;
        let px = |dx: i32, dy: i32| (wrap_i(x + dx), wrap_i(y + dy));
        let (hx, hy) = px(0, 0);
        tex.set(hx, hy, ramp.at(7).with_alpha(0.92));
        if big {
            for (dx, dy) in [(1, 0), (0, 1)] {
                let (qx, qy) = px(dx, dy);
                tex.set(qx, qy, ramp.at(5).with_alpha(0.82));
            }
            let (qx, qy) = px(1, 1);
            tex.set(qx, qy, ramp.at(2).with_alpha(0.8));
        } else {
            let (qx, qy) = px(1, 1);
            tex.set(qx, qy, ramp.at(3).with_alpha(0.76));
        }
    }
    tex
}

// ---------------------------------------------------------------- portals

/// Wayshard: a cold violet-blue, deep indigo in shadow to a lilac-white
/// heart. Bluer than the warp shrine's violet, far deeper than the glow
/// crystal's pale cyan.
pub(crate) fn wayshard_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x140a3a, 0x221466, 0x322298, 0x4434c4, 0x5a50e2, 0x7a76f6, 0xa2a6ff, 0xd2daff, 0xffffff,
    ])
}

/// Dressed stone for portal conduits: a smooth lilac-grey, paler and
/// warmer than deep stone, so a portal frame reads as built, not dug.
pub(crate) fn conduit_stone_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x221e34, 0x322d4a, 0x433d62, 0x56507a, 0x6a6490, 0x807aa6, 0x9c96bc, 0xbeb8d4,
    ])
}

/// The portal surface: deep blue-violet to near-white streaks.
fn portal_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x1a0c58, 0x281692, 0x3a26c0, 0x5240e0, 0x6e62f2, 0x9290fc, 0xbcc2ff, 0xeaf0ff,
    ])
}

/// One crystal shard: a blade from its foot at (x, y) along the unit axis
/// (ax, ay), `len` long, `half` wide, ending in a point.
struct Shard {
    x: f32,
    y: f32,
    ax: f32,
    ay: f32,
    len: f32,
    half: f32,
}

impl Shard {
    /// (u along the axis, v across it, half width at u) for a point.
    fn local(&self, px: f32, py: f32) -> (f32, f32, f32) {
        let (dx, dy) = (wrap_delta(px - self.x), wrap_delta(py - self.y));
        let u = dx * self.ax + dy * self.ay;
        let v = -dx * self.ay + dy * self.ax;
        let tip = self.half * 2.2;
        let w = self.half * ((self.len - u) / tip).clamp(0.0, 1.0) * (0.7 + u / 3.0).min(1.0);
        (u, v, w)
    }

    /// How far a point lies outside the shard (0 inside).
    fn outside(&self, px: f32, py: f32) -> f32 {
        let (u, v, w) = self.local(px, py);
        let along = (-u).max(u - self.len).max(0.0);
        let across = (v.abs() - w).max(0.0);
        along.hypot(across)
    }
}

/// Wayshard vein: clusters of slim violet-blue crystal blades breaking out
/// of deep stone, each blade with a lit face and a shaded face either side
/// of a bright ridge, brightening to its point, a dark seat where it meets
/// the rock and a faint violet glow cast on the stone round it. Blades, not
/// copper's and iron's rounded nuggets; deep and violet, not glow crystal's
/// pale cyan prisms.
pub fn wayshard_vein(ctx: Ctx) -> Tex {
    let salt = seed("wayshard_vein");
    let ramp = wayshard_ramp();
    let host = deep_stone(ctx);
    let mut shards: Vec<Shard> = Vec::new();
    for (i, s) in ctx.sites(salt, 12.0).iter().enumerate() {
        if s.r[0] > 0.9 {
            continue;
        }
        let mut rng = crate::noise::Rng::new(hash2(salt, i as u32));
        let n = 3 + (s.r[1] * 1.8) as usize;
        // Clusters lean up and to one side, never straight down.
        let base = -std::f32::consts::FRAC_PI_2 + (s.r[2] - 0.5) * 2.2;
        for j in 0..n {
            let a = base + (j as f32 - (n - 1) as f32 * 0.5) * 0.5 + rng.range(-0.15, 0.15);
            shards.push(Shard {
                x: s.x + rng.range(-1.2, 1.2),
                y: s.y + rng.range(-0.6, 0.6) + 4.0,
                ax: a.cos(),
                ay: a.sin(),
                len: rng.range(9.5, 13.0) * if j == n / 2 { 1.15 } else { 0.75 },
                half: rng.range(1.8, 2.5),
            });
        }
    }
    let outline = Rgba::hex(0x0e0822);
    let glow = ramp.at(3);
    let which = |px: f32, py: f32| shards.iter().rposition(|s| s.outside(px, py) == 0.0);
    let mut tex = host.clone();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (c(x), c(y));
            if let Some(k) = which(fx, fy) {
                let s = &shards[k];
                let (u, v, w) = s.local(fx, fy);
                // Which side of the ridge faces the light (from the top left).
                let lit_neg = (s.ax - s.ay) * std::f32::consts::FRAC_1_SQRT_2;
                let side = if v < 0.0 { lit_neg } else { -lit_neg };
                let mut t = 0.46 + side * 0.2 + (u / s.len) * 0.24;
                if v.abs() < 0.55 {
                    t += 0.16;
                }
                // The rim: a bright edge on the lit side, dark on the other.
                if v.abs() > w - 0.8 {
                    t += if side > 0.0 { 0.12 } else { -0.16 };
                }
                // Seated in the rock at its foot.
                if u < 1.2 {
                    t -= 0.18;
                }
                let tip = u > s.len - 1.6 && v.abs() < 0.8;
                tex.set(x, y, if tip { ramp.at(7) } else { ramp.pick(t, x, y) });
                continue;
            }
            let d = shards.iter().map(|s| s.outside(fx, fy)).fold(f32::MAX, f32::min);
            if d < 0.75 {
                tex.set(x, y, outline);
            } else if d < 4.5 {
                // The faint light the shards throw on the stone.
                let k = 0.5 * (1.0 - (d - 0.75) / 3.75);
                tex.set(x, y, host.get(x, y).mix(glow, k));
            }
        }
    }
    // Sparkles on a few blade tips.
    let white = ramp.lightest();
    for s in shards.iter().filter(|s| s.half > 2.1) {
        let (px, py) = (s.x + s.ax * (s.len - 2.2), s.y + s.ay * (s.len - 2.2));
        let (ix, iy) = (px.floor() as i32, py.floor() as i32);
        tex.set(wrap_i(ix), wrap_i(iy), white);
    }
    tex
}

/// The copper of a conduit's inlay at offset `o` across a band `w` pixels
/// wide (0 = the band's top or left edge): lit on its leading edge, shaded
/// on its trailing edge, a sheen down the middle.
fn conduit_copper(copper: &Ramp, o: f32, w: f32, x: usize, y: usize) -> Rgba {
    let t = if o < 1.0 {
        0.78
    } else if o > w - 1.0 {
        0.3
    } else {
        0.55 + (1.0 - ((o - w * 0.5) / (w * 0.5)).abs()) * 0.12
    };
    copper.pick(t, x, y)
}

/// Portal conduit: a dressed block of lilac-grey stone with a copper inlay
/// cut across it in a cross, carrying the power to a wayshard set in a copper
/// bezel in the middle. The inlay runs out of every edge and clasps the next
/// block's with a collar, so a filled rectangle of conduits reads as one
/// frame wired through with copper, a crystal at every node.
pub fn portal_conduit(_ctx: Ctx) -> Tex {
    let salt = seed("portal_conduit");
    let stone = conduit_stone_ramp();
    let copper = copper_ramp();
    let shard = wayshard_ramp();
    let shadow = Rgba::hex(0x1a1428);
    // The inlay band: 3 pixels from 15 to 17 in both directions, widening
    // to 5 (14 to 18) in a collar where it crosses the block's edge.
    let band = |a: usize, b: usize| -> Option<(f32, f32)> {
        let collar = !(2..30).contains(&b);
        let (lo, w) = if collar { (14, 5.0) } else { (15, 3.0) };
        (a >= lo && a < lo + w as usize).then(|| ((a - lo) as f32, w))
    };
    let mut tex = Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let (dx, dy) = (fx - 16.0, fy - 16.0);
        let l1 = dx.abs() + dy.abs();
        // The wayshard: a square-cut gem turned on its point, four facets.
        if l1 < 5.0 {
            let mut t = match (dx < 0.0, dy < 0.0) {
                (true, true) => 0.72,
                (false, true) => 0.56,
                (true, false) => 0.44,
                (false, false) => 0.28,
            };
            if l1 < 1.6 {
                t = 0.86;
            }
            if l1 > 4.0 {
                t -= 0.1;
            }
            return shard.pick(t, x, y);
        }
        // The bezel round it.
        if l1 < 7.5 {
            let facing = -(dx + dy) / l1;
            let t = if l1 > 6.5 {
                0.55 + facing * 0.3
            } else {
                0.5 - facing * 0.28
            };
            return copper.pick(t, x, y);
        }
        // The inlay: the vertical arm over the horizontal one.
        if let Some((o, w)) = band(x, y) {
            return conduit_copper(&copper, o, w, x, y);
        }
        if let Some((o, w)) = band(y, x) {
            return conduit_copper(&copper, o, w, x, y);
        }
        // Dressed stone: a soft mottle, faint tooling, a bevelled edge.
        let mut t = 0.5 + (crate::noise::fbm(salt, 4, 4, 2, fx, fy) - 0.5) * 0.22;
        let tool = crate::noise::value(salt ^ 1, 2, 16, fx, fy);
        if tool > 0.72 {
            t -= 0.06;
        }
        if x == 0 || y == 0 {
            t += 0.24;
        } else if x == 1 || y == 1 {
            t += 0.08;
        }
        if x == SIZE - 1 || y == SIZE - 1 {
            t -= 0.26;
        }
        stone.pick(t, x, y)
    });
    // Shadows the raised copper casts on the stone, below and right of it.
    let raised = |x: i32, y: i32| {
        let (x, y) = (wrap_i(x), wrap_i(y));
        let l1 = (c(x) - 16.0).abs() + (c(y) - 16.0).abs();
        l1 < 7.5 || band(x, y).is_some() || band(y, x).is_some()
    };
    let before = tex.clone();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (ix, iy) = (x as i32, y as i32);
            if !raised(ix, iy) && (raised(ix - 1, iy) || raised(ix, iy - 1) || raised(ix - 1, iy - 1)) {
                tex.set(x, y, before.get(x, y).mix(shadow, 0.5));
            }
        }
    }
    // Rivets on the collars, and the gem's glint.
    for (x, y) in [(16, 0), (16, 31), (0, 16), (31, 16)] {
        tex.set(x, y, copper.at(7));
    }
    tex.set(14, 14, shard.lightest());
    tex
}

/// Portal surface: a sheet of violet-blue light in slow interleaved swirls,
/// deep where it is calm and breaking into pale streaks along the currents,
/// with a few bright motes. No centre and no edge, so a 6 by 6 portal reads
/// as one surface. Translucent: the calm parts show the far side dimly, the
/// streaks are nearly solid light.
pub fn portal_surface(ctx: Ctx) -> Tex {
    let salt = seed("portal_surface");
    let ramp = portal_ramp();
    // Swirls from a few waves at whole-tile frequencies (so they tile),
    // their coordinates bent by more such waves, and a little soft noise.
    let p = std::f32::consts::TAU / SIZEF;
    let flow = Field::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let wx = fx + 4.5 * (p * fy + 1.3).sin() + 2.5 * (p * (fx + fy) + 0.4).sin();
        let wy = fy + 4.5 * (p * fx + 2.1).sin() + 2.5 * (p * (fx - fy) + 2.7).sin();
        0.5 + 0.2 * (p * (wx + 2.0 * wy)).sin()
            + 0.14 * (p * (2.0 * wx - wy) + 1.0).sin()
            + 0.08 * (p * (wx - wy) + 2.2).sin()
            + (ctx.fbm(salt ^ 5, 4, 4, 1, fx, fy) - 0.5) * 0.12
    })
    .blur();
    let light = flow.light();
    let mut tex = Tex::from_fn(|x, y| {
        let h = flow.at(x, y);
        // Streaks: thin bright lines along the contours of the swirl, each
        // trailing a soft glow on one side.
        let band = (h * 3.0 + 0.2).fract();
        let streak = 1.0 - smoothstep(0.04, 0.09, band);
        let trail = (1.0 - smoothstep(0.09, 0.35, band)) * 0.45;
        let lit = streak.max(trail);
        let t = 0.18 + h * 0.4 + light.at(x, y) * 1.0 + lit * 0.5;
        ramp.pick(t, x, y).with_alpha(0.62 + h * 0.06 + lit * 0.16)
    });
    // Motes: a bright pixel with a soft glow either side.
    for m in ctx.sites(salt ^ 7, 9.0) {
        if m.r[0] > 0.25 {
            continue;
        }
        let (x, y) = (m.x as i32, m.y as i32);
        tex.set(wrap_i(x), wrap_i(y), ramp.lightest().with_alpha(0.9));
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            let (px, py) = (wrap_i(x + dx), wrap_i(y + dy));
            let base = tex.get(px, py);
            tex.set(px, py, base.mix(ramp.at(6), 0.5).with_alpha(base.a.max(0.78)));
        }
    }
    tex
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
        "ash" => ash,
        "snow" => snow,
        "water" => water,
        "trunk_side" => trunk_side,
        "trunk_top" => trunk_top,
        "foliage" => foliage,
        "coal_seam" => coal_seam,
        "copper_vein" => copper_vein,
        "iron_vein" => iron_vein,
        "sandstone_top" => sandstone_top,
        "sandstone_side" => sandstone_side,
        "ice" => ice,
        "glow_crystal" => glow_crystal,
        "timber" => timber,
        "brick" => brick,
        "glass" => glass,
        "wayshard_vein" => wayshard_vein,
        "portal_conduit" => portal_conduit,
        "portal_surface" => portal_surface,
        "workbench_top" => stations::workbench_top,
        "workbench_side" => stations::workbench_side,
        "furnace_top" => stations::furnace_top,
        "furnace_side" => stations::furnace_side,
        "claim_stone_top" => stations::claim_stone_top,
        "claim_stone_side" => stations::claim_stone_side,
        "warp_shrine_top" => stations::warp_shrine_top,
        "warp_shrine_side" => stations::warp_shrine_side,
        "storage_chest_top" => stations::storage_chest_top,
        "storage_chest_side" => stations::storage_chest_side,
        _ => return None,
    };
    Some(f(ctx))
}
