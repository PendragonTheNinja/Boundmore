//! Gems and power (milestone 18 part 2): the six gem seams, the relic core
//! seam, the titanium furnace, the twelve power coils, and the icons of the
//! gem chain (rough, compact, refined), the relic core, bark, leather and
//! compacted coal. Gem tools, grapples, spanners and chisels are drawn by
//! `icons` (its `Metal::Gem`) through the ramps here.
//!
//! Gems are drawn as cut crystal: hard flat facets, a bright table, a white
//! glint, where ores are rounded nuggets and the glow crystal is long
//! prisms. Each gem keeps one ramp through its chain, its seam and the
//! coils it powers, so a ruby reads as a ruby everywhere.

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, TAU};

use crate::color::{Ramp, Rgba};
use crate::gathered::ramp as soft_ramp;
use crate::icons::{Icon, Lump, P, blade, inside_poly, leather_ramp, seg_dist};
use crate::machines::{Plate, crucible_side_in, crucible_top_in, relic_light_ramp, relic_plates, relic_ramp};
use crate::materials::{bark_ramp, coal_ramp, copper_ramp, stone, stone_ramp};
use crate::noise::{Rng, hash2, seed, unit, wrap_delta, wrap_i};
use crate::spark::{compact, dark_iron_ramp, iron_plates};
use crate::stations::{furnace_side_in, furnace_top_in, iron_ramp, rivet};
use crate::tex::{Ctx, SIZE, Tex, c};

// ---------------------------------------------------------------- the gems

/// The six gems, in the order the catalogue lists them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gem {
    Amethyst,
    Emerald,
    Sapphire,
    Topaz,
    Diamond,
    Ruby,
}

impl Gem {
    pub const ALL: [Gem; 6] = [
        Gem::Amethyst,
        Gem::Emerald,
        Gem::Sapphire,
        Gem::Topaz,
        Gem::Diamond,
        Gem::Ruby,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Gem::Amethyst => "amethyst",
            Gem::Emerald => "emerald",
            Gem::Sapphire => "sapphire",
            Gem::Topaz => "topaz",
            Gem::Diamond => "diamond",
            Gem::Ruby => "ruby",
        }
    }

    pub fn from_name(name: &str) -> Option<Gem> {
        Gem::ALL.into_iter().find(|g| g.name() == name)
    }

    /// The gem's colour, deep shade to white glint.
    pub(crate) fn ramp(self) -> Ramp {
        Ramp::from_hex(match self {
            Gem::Amethyst => &[
                0x1c0a30, 0x36145c, 0x55228c, 0x7638b8, 0x9a5cd8, 0xc08eee, 0xe4c8fc, 0xffffff,
            ],
            Gem::Emerald => &[
                0x05221a, 0x0a3c28, 0x10603c, 0x188a4e, 0x28b464, 0x5cd88a, 0xaaf2c2, 0xf2fff6,
            ],
            Gem::Sapphire => &[
                0x080e38, 0x0c1a64, 0x142c94, 0x1e44c2, 0x3266e2, 0x5e92f6, 0xa8c8ff, 0xf2f8ff,
            ],
            Gem::Topaz => &[
                0x3a1606, 0x682a08, 0x9e480c, 0xce6a10, 0xf08e1e, 0xffb646, 0xffdc8c, 0xfff6de,
            ],
            Gem::Diamond => &[
                0x24384a, 0x426278, 0x6a92a6, 0x94bece, 0xbce0ea, 0xdaf4f8, 0xf0fdff, 0xffffff,
            ],
            Gem::Ruby => &[
                0x2a0410, 0x50081c, 0x800e28, 0xae1634, 0xd62844, 0xf05c68, 0xffa4a8, 0xfff0f0,
            ],
        })
    }

    /// The machine this gem's coils power.
    fn machine(self) -> &'static str {
        match self {
            Gem::Diamond => "workbench",
            Gem::Ruby => "compactor",
            Gem::Sapphire => "extractor",
            Gem::Topaz => "refinery",
            Gem::Emerald => "mixer",
            Gem::Amethyst => "forge",
        }
    }
}

/// The teal light of a relic core: brighter and greener than the cyan cut
/// into relic plates.
fn core_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x0a3434, 0x12585a, 0x1a8280, 0x28aca0, 0x50d4bc, 0x96f2dc, 0xdcfff4, 0xffffff,
    ])
}

/// Titanium plate: the titanium furnace's walls, a pale blue-white metal
/// with deep blue-grey shade so the fire still reads against it.
fn titanium_plate_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x161e2e, 0x263244, 0x3a4a62, 0x546c86, 0x7892ac, 0x9eb8cc, 0xc6dcea, 0xeef8fe,
    ])
}

// ---------------------------------------------------------------- the seams

/// One crystal of a seam: its foot, direction, length and half width.
struct Crystal {
    foot: P,
    ang: f32,
    len: f32,
    half: f32,
}

impl Crystal {
    /// Where `(x, y)` falls on the crystal: along it, across it (negative
    /// on the lit side), and its half width there; `None` if outside. The
    /// face wraps, so a crystal near an edge carries on across it.
    fn at(&self, x: f32, y: f32) -> Option<(f32, f32, f32)> {
        let (dx, dy) = (wrap_delta(x - self.foot.0), wrap_delta(y - self.foot.1));
        let (ax, ay) = (self.ang.cos(), self.ang.sin());
        let u = dx * ax + dy * ay;
        let lit = if ax - ay > 0.0 { 1.0 } else { -1.0 };
        let v = (-dx * ay + dy * ax) * lit;
        if !(0.0..=self.len).contains(&u) {
            return None;
        }
        let tip = self.half * 1.6;
        let w = self.half * ((self.len - u) / tip).min(1.0);
        (v.abs() <= w).then_some((u, v, w))
    }
}

/// A gem seam: stone with pockets of cut crystal, each a cluster of short
/// hexagonal crystals growing out of a dark hollow, three faces of each
/// showing (lit, front, shade), a pointed tip, bright rims and a few
/// white sparkles. Diamond carries more sparkles.
fn gem_seam(ctx: Ctx, gem: Gem) -> Tex {
    let name = format!("{}_seam", gem.name());
    let salt = seed(&name);
    let ramp = gem.ramp();
    let mut rng = Rng::new(salt);
    // Pockets spread over the face, each with a few crystals fanning up.
    let pockets: [(f32, f32, usize); 4] = [(7.5, 13.0, 3), (23.5, 10.0, 2), (14.5, 28.0, 3), (28.0, 24.5, 2)];
    let mut crystals = Vec::new();
    let mut hollows = Vec::new();
    for &(px, py, n) in &pockets {
        let (px, py) = (px + rng.range(-2.0, 2.0), py + rng.range(-1.5, 1.5));
        let big = n >= 3;
        hollows.push((px, py, if big { 3.4 } else { 2.4 }));
        for i in 0..n {
            let spread = (i as f32 - (n as f32 - 1.0) * 0.5) * 0.55 + rng.range(-0.22, 0.22);
            // The middle crystals stand tallest.
            let middle = 1.0 - (i as f32 - (n as f32 - 1.0) * 0.5).abs() / n as f32;
            crystals.push(Crystal {
                foot: (px + spread * 2.4 + rng.range(-0.5, 0.5), py + rng.range(-0.3, 0.6)),
                ang: -FRAC_PI_2 + spread,
                len: if big {
                    rng.range(6.5, 8.5) + middle * 3.5
                } else {
                    rng.range(4.5, 6.5)
                },
                half: if big { rng.range(1.5, 2.0) } else { rng.range(1.2, 1.5) },
            });
        }
    }
    // The topmost crystal at each pixel (later crystals stand in front).
    let top = |x: i32, y: i32| -> Option<(usize, f32, f32, f32)> {
        let (fx, fy) = (c(wrap_i(x)), c(wrap_i(y)));
        crystals
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, k)| k.at(fx, fy).map(|(u, v, w)| (i, u, v, w)))
    };
    let outline = ramp.darkest().mix(Rgba::hex(0x0c0814), 0.6);
    let mut tex = stone(ctx);
    // The hollows the crystals grow from, dark and tinted by the gem.
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (c(x), c(y));
            for &(hx, hy, r) in &hollows {
                let d = wrap_delta(fx - hx).hypot(wrap_delta(fy - hy) * 1.6);
                if d < r {
                    let k = if d < r - 1.0 { 0.75 } else { 0.45 };
                    tex.set(x, y, tex.get(x, y).mix(ramp.at(1).mix(outline, 0.5), k));
                }
            }
        }
    }
    let host = tex.clone();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (ix, iy) = (x as i32, y as i32);
            match top(ix, iy) {
                Some((i, u, v, w)) => {
                    let k = &crystals[i];
                    let third = w / 3.0;
                    let mut t = if v < -third {
                        0.76
                    } else if v > third {
                        0.3
                    } else {
                        0.54
                    };
                    // Brighter towards the tip, as if lit from inside.
                    t += u / k.len * 0.14;
                    if u > k.len - k.half * 1.6 && v < third {
                        t += 0.12;
                    }
                    let other = |dx: i32, dy: i32| top(ix + dx, iy + dy).map(|o| o.0) != Some(i);
                    if other(-1, 0) || other(0, -1) {
                        t = t.max(0.82);
                    } else if other(1, 0) || other(0, 1) {
                        t -= 0.22;
                    }
                    if (v + third).abs() < 0.45 && u > 1.0 {
                        t += 0.1;
                    }
                    tex.set(x, y, ramp.pick(t, x, y));
                }
                None => {
                    let near = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .any(|&(dx, dy)| top(ix + dx, iy + dy).is_some());
                    if near {
                        tex.set(x, y, outline);
                    } else if top(ix - 1, iy - 1).is_some() || top(ix - 2, iy - 1).is_some() {
                        tex.set(x, y, host.get(x, y).mix(outline, 0.4));
                    }
                }
            }
        }
    }
    // Sparkles: a white point with a soft cross, near some crystal tips.
    let share = if gem == Gem::Diamond { 0.85 } else { 0.4 };
    let white = Rgba::hex(0xffffff);
    for (i, k) in crystals.iter().enumerate() {
        if unit(hash2(salt ^ 0x51, i as u32)) > share {
            continue;
        }
        let at = k.len * 0.62;
        let (sx, sy) = (
            (k.foot.0 + k.ang.cos() * at).round() as i32,
            (k.foot.1 + k.ang.sin() * at).round() as i32,
        );
        for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (px, py) = (wrap_i(sx + dx), wrap_i(sy + dy));
            let col = if dx == 0 && dy == 0 {
                white
            } else {
                tex.get(px, py).mix(white, 0.55)
            };
            tex.set(px, py, col);
        }
    }
    if gem == Gem::Diamond {
        // Glitter in the rock round the pockets.
        for (i, &(hx, hy, r)) in hollows.iter().enumerate() {
            let a = unit(hash2(salt, 90 + i as u32)) * TAU;
            let (sx, sy) = ((hx + a.cos() * (r + 2.5)) as i32, (hy + a.sin() * (r + 1.5)) as i32);
            tex.set(wrap_i(sx), wrap_i(sy), ramp.at(6));
        }
    }
    tex
}

pub fn amethyst_seam(ctx: Ctx) -> Tex {
    gem_seam(ctx, Gem::Amethyst)
}
pub fn emerald_seam(ctx: Ctx) -> Tex {
    gem_seam(ctx, Gem::Emerald)
}
pub fn sapphire_seam(ctx: Ctx) -> Tex {
    gem_seam(ctx, Gem::Sapphire)
}
pub fn topaz_seam(ctx: Ctx) -> Tex {
    gem_seam(ctx, Gem::Topaz)
}
pub fn diamond_seam(ctx: Ctx) -> Tex {
    gem_seam(ctx, Gem::Diamond)
}
pub fn ruby_seam(ctx: Ctx) -> Tex {
    gem_seam(ctx, Gem::Ruby)
}

/// Relic core seam: broken relic plates in stone like the relic seam's,
/// round one whole device still alive: a teal orb in a bronze ring, its
/// light spilling over the plates and running out along four grooves.
pub fn relic_core_seam(ctx: Ctx) -> Tex {
    let plates: [Plate; 4] = [
        (16.0, 15.5, 8.5, 6.0, 0.12, true),
        (4.5, 5.0, 3.5, 3.0, -0.4, false),
        (26.5, 27.0, 4.0, 2.8, 0.45, true),
        (27.0, 5.5, 2.6, 3.4, 0.2, false),
    ];
    let mut tex = relic_plates(seed("relic_core_seam"), stone(ctx), &plates);
    let bronze = relic_ramp();
    let teal = core_ramp();
    let (cx, cy, r) = (16.0f32, 15.5f32, 4.6f32);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (c(x) - cx, c(y) - cy);
            let d = dx.hypot(dy);
            let groove = (dx.abs() < 0.6 || dy.abs() < 0.6) && d < r + 5.5;
            let col = if d <= r - 1.4 {
                if (dx + 1.6).hypot(dy + 1.6) < 1.1 {
                    teal.lightest()
                } else {
                    teal.pick(1.0 - d / r * 0.55 + (dx + dy) * -0.015, x, y)
                }
            } else if d <= r {
                bronze.pick(0.55 - (dx + dy) / d.max(0.1) * 0.35, x, y)
            } else if d <= r + 0.9 {
                Rgba::hex(0x1a1220)
            } else if groove {
                teal.pick(0.82 - (d - r) / 5.5 * 0.3, x, y)
            } else if d < r + 4.0 {
                tex.get(x, y).mix(teal.at(4), 0.4 * (1.0 - (d - r) / 4.0))
            } else {
                continue;
            };
            tex.set(x, y, col);
        }
    }
    // Light flickering in the plates' own grooves nearby.
    let light = relic_light_ramp();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let p = tex.get(x, y);
            if p == light.at(3) || p == light.at(2) {
                tex.set(x, y, p.mix(teal.at(5), 0.5));
            }
        }
    }
    tex
}

// ------------------------------------------------------ the titanium furnace

/// Titanium furnace top: the furnace's chimney and grate in pale titanium
/// plate, riveted in iron.
pub fn titanium_furnace_top(_ctx: Ctx) -> Tex {
    let plate = titanium_plate_ramp();
    let mut tex = furnace_top_in(seed("titanium_furnace_top"), &plate);
    let iron = dark_iron_ramp();
    for (x, y) in [(2, 2), (18, 2), (24, 2), (28, 18), (2, 13), (2, 28), (13, 28), (28, 28)] {
        rivet(&mut tex, &iron, x, y);
    }
    tex
}

/// Titanium furnace side: the furnace's mouth and fire in walls of riveted
/// titanium plate.
pub fn titanium_furnace_side(_ctx: Ctx) -> Tex {
    let salt = seed("titanium_furnace_side");
    let plate = titanium_plate_ramp();
    let mut tex = furnace_side_in(salt, &plate, &|x, y| iron_plates(salt, &plate, x, y));
    let iron = dark_iron_ramp();
    for x in [3usize, 16, 27] {
        rivet(&mut tex, &iron, x, 1);
    }
    tex
}

/// The titanium furnace's crucible: a titanium alloy pot in titanium hoops.
pub fn titanium_furnace_crucible_top(_ctx: Ctx) -> Tex {
    crucible_top_in(
        seed("titanium_furnace_crucible_top"),
        &titanium_plate_ramp(),
        &crate::worlds::titanium_alloy_ramp(),
    )
}

pub fn titanium_furnace_crucible_side(_ctx: Ctx) -> Tex {
    crucible_side_in(
        seed("titanium_furnace_crucible_side"),
        &titanium_plate_ramp(),
        &crate::worlds::titanium_alloy_ramp(),
        &titanium_plate_ramp(),
    )
}

// ---------------------------------------------------------------- the coils

/// A power coil: a drum of copper wire wound in a riveted frame round a
/// cut gem set in its middle, the gem's light glowing on the windings.
/// Advanced coils have a pale titanium frame and straps, and a bigger,
/// brighter gem with a sparkle.
fn coil(gem: Gem, advanced: bool) -> Tex {
    let name = format!("{}{}_coil", if advanced { "advanced_" } else { "" }, gem.machine());
    let salt = seed(&name);
    let ramp = gem.ramp();
    let copper = copper_ramp();
    let frame = if advanced {
        titanium_plate_ramp()
    } else {
        dark_iron_ramp()
    };
    let r = if advanced { 8.6f32 } else { 6.4f32 };
    let reach = if advanced { 7.0 } else { 5.0 };
    let glow = if advanced { 0.6 } else { 0.4 };
    let (cx, cy) = (15.5f32, 15.5f32);
    // A cut gem: a diamond with its points trimmed, four facets round a
    // table, lit from the top left.
    let gem_at = |x: usize, y: usize| -> Option<Rgba> {
        let (dx, dy) = (c(x) - 0.5 - cx, c(y) - 0.5 - cy);
        let (fx, fy) = (dx + 0.5, dy + 0.5);
        if fx.abs() + fy.abs() > r || fx.abs().max(fy.abs()) > r * 0.8 {
            return None;
        }
        let rim = fx.abs() + fy.abs() > r - 1.0 || fx.abs().max(fy.abs()) > r * 0.8 - 1.0;
        let table = fx.abs() + fy.abs() < r * 0.42;
        let mut t = if table {
            0.72 - (fx + fy) * 0.02
        } else {
            match (fx < 0.0, fy < 0.0) {
                (true, true) => 0.9,
                (false, true) => 0.62,
                (true, false) => 0.48,
                (false, false) => 0.26,
            }
        };
        if rim {
            t += if fx + fy < 0.0 { 0.1 } else { -0.12 };
        }
        if (fx + r * 0.3).hypot(fy + r * 0.3) < if advanced { 1.3 } else { 0.9 } {
            return Some(ramp.lightest());
        }
        Some(ramp.pick(t, x, y))
    };
    let mut tex = Tex::from_fn(|x, y| {
        let edge = x.min(y).min(SIZE - 1 - x).min(SIZE - 1 - y);
        if edge < 3 {
            // The frame: lit on its outer top and left, shaded bottom and right.
            let mut t = 0.5;
            if (x == 0 || y == 0) && x < SIZE - 1 && y < SIZE - 1 {
                t += 0.3;
            }
            if x == SIZE - 1 || y == SIZE - 1 {
                t -= 0.3;
            }
            if edge == 2 && (x == 2 || y == 2) {
                t -= 0.25;
            }
            if edge == 2 && (x == SIZE - 3 || y == SIZE - 3) {
                t += 0.15;
            }
            t += (unit(hash2(salt, (x / 4 + y / 4 * 8) as u32)) - 0.5) * 0.08;
            return frame.pick(t, x, y);
        }
        if let Some(col) = gem_at(x, y) {
            return col;
        }
        let (dx, dy) = (c(x) - 0.5 - cx, c(y) - 0.5 - cy);
        let (fx, fy) = (dx + 0.5, dy + 0.5);
        let d = fx.abs() + fy.abs();
        if d <= r + 1.2 || fx.abs().max(fy.abs()) <= r * 0.8 + 1.0 && d <= r + 1.6 {
            // The setting the gem sits in.
            return dark_iron_ramp().pick(if fx + fy < 0.0 { 0.35 } else { 0.15 }, x, y);
        }
        if advanced && (6..9).contains(&x) || advanced && (23..26).contains(&x) {
            // Titanium straps over the windings.
            let lx = if x < 16 { x - 6 } else { x - 23 };
            let t = [0.82, 0.58, 0.32][lx] + if y == 3 { 0.1 } else { 0.0 };
            return frame.pick(t, x, y);
        }
        // Windings: wire three pixels high, round in section (a bright
        // top, a dark underside), on a drum lit from the left; a skewed gap
        // marks each turn.
        let row = (y - 3) / 3;
        let ly = (y - 3) % 3;
        let across = (c(x) - 9.0) / 20.0;
        let mut t = 0.62 - across * across * 0.6 - (c(x) - 16.0) / 13.0 * 0.12;
        t += [0.22, 0.02, -0.3][ly];
        if (x + row * 5) % 11 == 0 {
            t -= 0.3;
        }
        if ly == 0 && (8..11).contains(&x) {
            t += 0.15;
        }
        let mut col = copper.pick(t, x, y);
        let w = (1.0 - (d - r) / reach).clamp(0.0, 1.0).powi(2) * glow;
        if w > 0.02 {
            col = col.mix(ramp.at(5), w);
        }
        col
    });
    let iron = if advanced { dark_iron_ramp() } else { iron_ramp() };
    for (x, y) in [(1, 1), (29, 1), (1, 29), (29, 29), (15, 1), (15, 29)] {
        rivet(&mut tex, &iron, x, y);
    }
    if advanced {
        for &x in &[7usize, 24] {
            for y in [5usize, 26] {
                tex.set(x, y, iron.at(7));
                tex.set(x, y + 1, iron.at(1));
            }
        }
        // A four-point sparkle off the gem's lit corner.
        let white = Rgba::hex(0xffffff);
        let (sx, sy) = ((cx - r * 0.3) as usize, (cy - r * 0.3) as usize);
        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let (px, py) = ((sx as i32 + dx) as usize, (sy as i32 + dy) as usize);
            tex.set(px, py, tex.get(px, py).mix(white, 0.6));
        }
    }
    tex
}

// ------------------------------------------------------------------ icons

/// Rough gem: a chunk of crystal broken from the rock, a few points still
/// growing out of it, a bit of stone at its foot.
fn rough(gem: Gem) -> Tex {
    let salt = seed(&format!("rough_{}", gem.name()));
    let ramp = gem.ramp();
    let mut icon = Icon::new();
    let up = -FRAC_PI_2;
    blade(&mut icon, (14.0, 22.0), up - 0.55, 14.0, 3.2, &ramp);
    blade(&mut icon, (18.0, 22.0), up + 0.5, 12.0, 2.8, &ramp);
    blade(&mut icon, (15.5, 23.0), up + 0.05, 19.0, 3.8, &ramp);
    Lump::new(salt, 16.0, 25.0, 9.0, 7).draw(&mut icon, &stone_ramp(), 0.15, 0.8, false);
    Lump::new(hash2(salt, 1), 11.0, 22.5, 4.4, 5).draw(&mut icon, &ramp, 0.3, 0.98, true);
    Lump::new(hash2(salt, 2), 22.5, 24.0, 3.2, 5).draw(&mut icon, &ramp, 0.3, 0.98, true);
    icon.finish()
}

/// Compact gem: a step-cut gem seen from above, a big flat table inside
/// rings of stepped facets, each step lit by which way it faces.
fn cut(gem: Gem) -> Tex {
    let ramp = gem.ramp();
    let (x0, x1, y0, y1, k) = (4.0f32, 28.0f32, 7.0f32, 26.0f32, 4.5f32);
    let oct: [P; 8] = [
        (x0 + k, y0),
        (x1 - k, y0),
        (x1, y0 + k),
        (x1, y1 - k),
        (x1 - k, y1),
        (x0 + k, y1),
        (x0, y1 - k),
        (x0, y0 + k),
    ];
    // Each edge's outward normal.
    let normals: Vec<P> = (0..8)
        .map(|i| {
            let (a, b) = (oct[i], oct[(i + 1) % 8]);
            let (ex, ey) = (b.0 - a.0, b.1 - a.1);
            let l = ex.hypot(ey);
            (ey / l, -ex / l)
        })
        .collect();
    let light = (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2);
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&oct, p.0, p.1) {
            return None;
        }
        let (i, d) = (0..8)
            .map(|i| (i, seg_dist(oct[i], oct[(i + 1) % 8], p)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        let facing = normals[i].0 * light.0 + normals[i].1 * light.1;
        let t = if d < 1.0 {
            0.5 + facing * 0.35
        } else if d < 3.6 {
            0.5 + facing * 0.3 + 0.06
        } else if d < 6.0 {
            0.55 - facing * 0.18
        } else {
            // The table, with a sheen across it.
            let sheen = (p.0 - p.1 * 0.8 - 6.0).abs() < 1.2;
            0.66 + if sheen { 0.2 } else { 0.0 }
        };
        // The steps' joins.
        if (d - 3.6).abs() < 0.45 || (d - 6.0).abs() < 0.45 {
            return Some(ramp.pick(t - 0.16, x, y));
        }
        Some(ramp.pick(t, x, y))
    });
    icon.layer(false, |x, y| {
        let (dx, dy) = (c(x) - 11.5, c(y) - 13.0);
        (dx.abs() + dy.abs() < 1.3).then(|| ramp.lightest())
    });
    icon.finish()
}

/// Refined gem: a polished brilliant seen from the side, a crown of
/// facets over a bright girdle and a long pointed pavilion, with a star of
/// light on its crown.
fn brilliant(gem: Gem) -> Tex {
    let ramp = gem.ramp();
    let (gy, ty, py) = (12.5f32, 5.5f32, 29.0f32);
    let (gl, gr, tl, tr) = (3.0f32, 29.0f32, 10.0f32, 22.0f32);
    let crown: [P; 4] = [(tl, ty), (tr, ty), (gr, gy), (gl, gy)];
    let pavilion: [P; 3] = [(gl, gy + 1.0), (gr, gy + 1.0), (16.0, py)];
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if inside_poly(&crown, p.0, p.1) {
            // Crown facets: the table on top, kites below it.
            let s = (p.1 - ty) / (gy - ty);
            let left = tl + (gl - tl) * s;
            let right = tr + (gr - tr) * s;
            let k = ((p.0 - left) / (right - left) * 5.0).floor().clamp(0.0, 4.0) as i32;
            let mut t = [0.86, 0.74, 0.62, 0.46, 0.32][k as usize];
            if p.1 < ty + 1.0 {
                t += 0.1;
            }
            let joint = ((p.0 - left) / (right - left) * 5.0).fract();
            if joint < 0.12 && k > 0 {
                t -= 0.14;
            }
            return Some(ramp.pick(t, x, y));
        }
        if (p.1 - gy - 0.5).abs() < 0.6 && p.0 > gl && p.0 < gr {
            // The girdle.
            return Some(ramp.pick(0.88 - (p.0 - gl) / (gr - gl) * 0.3, x, y));
        }
        if inside_poly(&pavilion, p.0, p.1) {
            // Pavilion facets meet at the point; light comes back through
            // the left ones.
            let a = (p.0 - 16.0).atan2(py - p.1);
            let k = ((a + 0.9) / 1.8 * 4.0).floor().clamp(0.0, 3.0) as usize;
            let mut t = [0.6, 0.44, 0.3, 0.2][k];
            t += (1.0 - (py - p.1) / (py - gy)) * 0.12;
            return Some(ramp.pick(t, x, y));
        }
        None
    });
    // A star of light on the crown.
    icon.layer(false, |x, y| {
        let (dx, dy) = (c(x) - 11.5, c(y) - 8.5);
        ((dx.abs() < 0.6 && dy.abs() < 2.6) || (dy.abs() < 0.6 && dx.abs() < 2.6)).then(|| ramp.lightest())
    });
    icon.finish()
}

/// Relic core: an orb of teal light in a bronze ring, four studs round it
/// and light leaking out through four gaps in the ring.
fn relic_core() -> Tex {
    let bronze = relic_ramp();
    let teal = core_ramp();
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
        let r = dx.hypot(dy);
        let lit = -(dx + dy) / r.max(0.1);
        for (sx, sy) in [(-9.6f32, -9.6f32), (9.6, -9.6), (-9.6, 9.6), (9.6, 9.6)] {
            if (dx - sx).hypot(dy - sy) < 2.6 {
                return Some(bronze.pick(if dx + dy < sx + sy { 0.9 } else { 0.35 }, x, y));
            }
        }
        if r > 13.0 {
            return None;
        }
        if r > 9.2 {
            if dx.abs() < 1.2 || dy.abs() < 1.2 {
                return Some(teal.pick(0.75 - (r - 9.2) * 0.08, x, y));
            }
            let mut t = 0.5 + lit * 0.3;
            if r > 12.0 {
                t -= 0.1;
            }
            if r < 10.2 {
                t -= 0.2;
            }
            return Some(bronze.pick(t, x, y));
        }
        if (dx + 3.2).hypot(dy + 3.2) < 1.8 {
            return Some(teal.lightest());
        }
        Some(teal.pick(0.98 - r / 9.2 * 0.55 + lit * 0.08, x, y))
    });
    icon.finish()
}

/// Bark: a curling strip of rough bark, ridged along its length, one edge
/// rolled over to show the pale inner side.
fn bark() -> Tex {
    let salt = seed("bark");
    let outer = bark_ramp();
    let inner = soft_ramp(0xd8b07a, 0.4, 0.95);
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - 16.0, c(y) - 16.0);
        let u = (dx - dy) * FRAC_1_SQRT_2;
        let v = (dx + dy) * FRAC_1_SQRT_2;
        // Ragged, torn ends.
        let end = 13.0 - unit(hash2(salt, ((v + 8.0) as i32) as u32)) * 2.2;
        let half = 5.0 + (u * 0.25).sin() * 0.6;
        if u.abs() > end || v.abs() > half {
            return None;
        }
        if v > half - 2.2 {
            // The rolled edge: the inner side, lit on its curve.
            let t = 0.5 + (half - v - 1.1) * 0.25 + if v > half - 0.7 { -0.2 } else { 0.0 };
            return Some(inner.pick(t, x, y));
        }
        // Ridges along the strip, cracks across it.
        let ridge = ((v + 6.0) * 0.9).floor() as i32;
        let mut t = 0.5 - v / half * 0.18 + if ridge % 2 == 0 { 0.12 } else { -0.1 };
        t += (unit(hash2(salt ^ 7, ridge as u32)) - 0.5) * 0.12;
        if (u * 0.5 + unit(hash2(salt ^ 3, ridge as u32)) * 3.0).rem_euclid(4.0) < 0.5 {
            t -= 0.2;
        }
        if v < -half + 1.0 {
            t += 0.16;
        }
        Some(outer.pick(t, x, y))
    });
    icon.finish()
}

/// Leather: a tanned sheet, rolled up at one end and tied, stitched along
/// its edge.
fn leather() -> Tex {
    let hide = soft_ramp(0xa4643a, 0.32, 0.9);
    let stitch = soft_ramp(0xf0dcb0, 0.5, 1.0);
    let cord = leather_ramp();
    let sheet: [P; 4] = [(4.0, 6.0), (20.0, 5.0), (21.0, 27.0), (5.0, 27.5)];
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        if !inside_poly(&sheet, p.0, p.1) {
            return None;
        }
        let edge = [(0, 1), (3, 0), (2, 3)]
            .iter()
            .map(|&(a, b)| seg_dist(sheet[a], sheet[b], p))
            .fold(f32::MAX, f32::min);
        if (1.4..2.2).contains(&edge) && (x + y) % 3 != 0 {
            return Some(stitch.pick(0.6, x, y));
        }
        let grain = unit(hash2(seed("leather"), ((x / 2) + (y / 2) * 16) as u32));
        let mut t = 0.56 - (p.0 + p.1 - 30.0) / 40.0 * 0.2 + (grain - 0.5) * 0.08;
        if seg_dist(sheet[0], sheet[1], p) < 1.0 || seg_dist(sheet[3], sheet[0], p) < 1.0 {
            t += 0.14;
        }
        Some(hide.pick(t, x, y))
    });
    // The roll: a cylinder standing up the right side.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let (dx, top, bottom) = (p.0 - 23.0, 4.0, 28.5);
        if dx.abs() > 5.0 || p.1 < top || p.1 > bottom {
            return None;
        }
        // The rolled end shows its spiral at the top.
        let cap = (p.1 - (top + 1.5)) * 2.4;
        if dx.hypot(cap) < 5.0 && p.1 < top + 3.0 {
            let r = dx.hypot(cap);
            let t = if (r * 0.9) as i32 % 2 == 0 { 0.7 } else { 0.32 };
            return Some(hide.pick(t, x, y));
        }
        let mut t = 0.56 - dx / 5.0 * 0.3;
        if (dx + 2.2).abs() < 0.6 {
            t += 0.18;
        }
        Some(hide.pick(t, x, y))
    });
    // A thong tied round the roll.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let dx = p.0 - 23.0;
        if dx.abs() > 5.4 || (p.1 - 18.0).abs() > 1.0 {
            return None;
        }
        Some(cord.pick(0.6 - dx / 5.4 * 0.35, x, y))
    });
    icon.finish()
}

/// Compacted coal: the compactor's pressed cube in each coal's black.
fn compact_coal(which: &str) -> Tex {
    match which {
        "compact_medium_coal" => compact(crate::worlds::medium_coal_ramp()),
        "compact_hard_coal" => compact(crate::worlds::hard_coal_ramp()),
        _ => compact(coal_ramp()),
    }
}

// ---------------------------------------------------------------- lookups

/// The block textures this part adds.
pub const BLOCKS: [&str; 23] = [
    "amethyst_seam",
    "emerald_seam",
    "sapphire_seam",
    "topaz_seam",
    "diamond_seam",
    "ruby_seam",
    "relic_core_seam",
    "titanium_furnace_top",
    "titanium_furnace_side",
    "titanium_furnace_crucible_top",
    "titanium_furnace_crucible_side",
    "workbench_coil",
    "compactor_coil",
    "extractor_coil",
    "refinery_coil",
    "mixer_coil",
    "forge_coil",
    "advanced_workbench_coil",
    "advanced_compactor_coil",
    "advanced_extractor_coil",
    "advanced_refinery_coil",
    "advanced_mixer_coil",
    "advanced_forge_coil",
];

/// The item icons this part adds: the gem chain, the relic core, bark,
/// leather and compacted coal, then each gem's gear (drawn by `icons`).
pub fn icon_names() -> Vec<String> {
    let mut names = Vec::new();
    for gem in Gem::ALL {
        for form in ["rough", "compact", "refined"] {
            names.push(format!("{form}_{}", gem.name()));
        }
    }
    for n in [
        "relic_core",
        "bark",
        "leather",
        "compact_coal",
        "compact_medium_coal",
        "compact_hard_coal",
    ] {
        names.push(n.to_string());
    }
    for gem in Gem::ALL {
        for tool in ["hammer", "shovel", "axe", "grapple", "spanner", "chisel"] {
            names.push(format!("{}_{tool}", gem.name()));
        }
    }
    names
}

/// Paints the icon called `name`, or `None` if this module does not know
/// it (gem gear is `icons`'s).
pub fn paint_icon(name: &str) -> Option<Tex> {
    match name {
        "relic_core" => return Some(relic_core()),
        "bark" => return Some(bark()),
        "leather" => return Some(leather()),
        "compact_coal" | "compact_medium_coal" | "compact_hard_coal" => return Some(compact_coal(name)),
        _ => {}
    }
    let (form, gem) = name.split_once('_')?;
    let gem = Gem::from_name(gem)?;
    Some(match form {
        "rough" => rough(gem),
        "compact" => cut(gem),
        "refined" => brilliant(gem),
        _ => return None,
    })
}

/// The painter for one of this module's block textures.
pub fn painter(name: &str) -> Option<fn(Ctx) -> Tex> {
    Some(match name {
        "amethyst_seam" => amethyst_seam,
        "emerald_seam" => emerald_seam,
        "sapphire_seam" => sapphire_seam,
        "topaz_seam" => topaz_seam,
        "diamond_seam" => diamond_seam,
        "ruby_seam" => ruby_seam,
        "relic_core_seam" => relic_core_seam,
        "titanium_furnace_top" => titanium_furnace_top,
        "titanium_furnace_side" => titanium_furnace_side,
        "titanium_furnace_crucible_top" => titanium_furnace_crucible_top,
        "titanium_furnace_crucible_side" => titanium_furnace_crucible_side,
        "workbench_coil" => |_| coil(Gem::Diamond, false),
        "compactor_coil" => |_| coil(Gem::Ruby, false),
        "extractor_coil" => |_| coil(Gem::Sapphire, false),
        "refinery_coil" => |_| coil(Gem::Topaz, false),
        "mixer_coil" => |_| coil(Gem::Emerald, false),
        "forge_coil" => |_| coil(Gem::Amethyst, false),
        "advanced_workbench_coil" => |_| coil(Gem::Diamond, true),
        "advanced_compactor_coil" => |_| coil(Gem::Ruby, true),
        "advanced_extractor_coil" => |_| coil(Gem::Sapphire, true),
        "advanced_refinery_coil" => |_| coil(Gem::Topaz, true),
        "advanced_mixer_coil" => |_| coil(Gem::Emerald, true),
        "advanced_forge_coil" => |_| coil(Gem::Amethyst, true),
        _ => return None,
    })
}

/// The review sheet of this part's art (`docs/media/m18-part2-icons.png`):
/// the new blocks beside stone, the glow crystal, the relic seam and the
/// iron furnace they follow, tiled, then every new icon beside the iron
/// ones it follows.
pub fn review_sheet() -> Result<crate::canvas::Canvas, String> {
    use crate::canvas::Canvas;
    let all = crate::families()?;
    let blocks: Vec<crate::Family> = [
        "stone",
        "glow_crystal",
        "relic_seam",
        "iron_furnace_top",
        "iron_furnace_side",
    ]
    .into_iter()
    .chain(BLOCKS)
    .map(|n| crate::find(&all, n).clone())
    .collect();
    let mut names: Vec<String> = [
        "iron_hammer",
        "compact_iron",
        "refined_iron",
        "relic_component",
        "hide",
        "coal",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    names.extend(icon_names());
    let icons: Vec<(String, Tex)> = names
        .iter()
        .map(|n| {
            crate::icons::paint_icon(n)
                .map(|t| (n.clone(), t))
                .ok_or_else(|| format!("no icon `{n}`"))
        })
        .collect::<Result<_, _>>()?;
    let top = crate::sheet::contact_sheet(&blocks, &[], &[]);
    let bottom = crate::sheet::icon_sheet(&all, &icons);
    let mut out = Canvas::new(top.w.max(bottom.w), top.h + bottom.h, Rgba::hex(0x1c1d26));
    for (y0, part) in [(0, &top), (top.h, &bottom)] {
        for y in 0..part.h {
            for x in 0..part.w {
                out.blend(x as i64, (y0 + y) as i64, part.get(x, y));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every part 2 texture and icon has a painter, and no two icons are
    /// alike (a gem's gear must not come out as another gem's).
    #[test]
    fn every_gems_and_power_texture_and_icon_is_painted_and_distinct() {
        for n in BLOCKS {
            let tex = crate::materials::paint_named(n, 0).unwrap_or_else(|| panic!("no painter for `{n}`"));
            assert!(tex.px.iter().all(|p| p.a >= 1.0), "`{n}` is not opaque");
        }
        let names = icon_names();
        assert_eq!(names.len(), 18 + 6 + 36);
        let icons: Vec<Tex> = names
            .iter()
            .map(|n| crate::icons::paint_icon(n).unwrap_or_else(|| panic!("no icon for `{n}`")))
            .collect();
        for i in 0..icons.len() {
            for j in i + 1..icons.len() {
                assert_ne!(icons[i], icons[j], "`{}` and `{}` look the same", names[i], names[j]);
            }
        }
        // The coils differ from each other.
        let coils: Vec<Tex> = BLOCKS[11..]
            .iter()
            .map(|n| crate::materials::paint_named(n, 0).unwrap())
            .collect();
        for i in 0..coils.len() {
            for j in i + 1..coils.len() {
                assert_ne!(coils[i], coils[j], "{} vs {}", BLOCKS[11 + i], BLOCKS[11 + j]);
            }
        }
    }

    /// The gem seams are told apart at a distance (art direction rule 2):
    /// each one's average colour differs from stone's, the glow crystal's,
    /// every metal seam's and each other's.
    #[test]
    fn the_gem_seams_differ_from_each_other_and_stone_at_a_distance() {
        let seams = &BLOCKS[..7];
        let mut names = vec![
            "stone",
            "glow_crystal",
            "relic_seam",
            "coal_seam",
            "copper_vein",
            "iron_vein",
            "silver_seam",
            "gold_seam",
            "titanium_seam",
        ];
        names.extend(seams);
        let means: Vec<_> = names
            .iter()
            .map(|n| crate::materials::paint_named(n, 0).unwrap().mean())
            .collect();
        for (i, n) in names.iter().enumerate() {
            for j in 0..i {
                if !seams.contains(n) {
                    continue;
                }
                let (a, b) = (means[i], means[j]);
                let d = (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs();
                assert!(d > 0.03, "`{n}` and `{}` look alike from afar ({d})", names[j]);
            }
        }
    }
}
