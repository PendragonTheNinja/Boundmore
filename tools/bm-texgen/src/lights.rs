//! Lights of milestone 26: the six gem torches (block textures in the
//! torch's layout, and their icons) and the crystal lantern.
//!
//! A gem torch is a wooden stick holding a rough gem in a cord binding; the
//! gem burns with a cold light of its own colour where the torch's flame
//! would be. The block texture keeps the torch's places (bm-mesh `props`):
//! the stick's sides on columns 14..18 of rows 14..32, the glowing head on
//! columns 13..19 of rows 6..12.

use crate::color::Ramp;
use crate::gems::Gem;
use crate::icons::{Icon, blade};
use crate::materials::{crystal_ramp, plank_ramp};
use crate::noise::{hash2, seed, unit};
use crate::spark::dark_iron_ramp;
use crate::tex::{Ctx, SIZEF, Tex, c};

/// The gem torches, by name (`<gem>_torch`).
pub const GEM_TORCHES: [&str; 6] = [
    "amethyst_torch",
    "emerald_torch",
    "sapphire_torch",
    "topaz_torch",
    "diamond_torch",
    "ruby_torch",
];

fn gem_of(name: &str) -> Option<Gem> {
    Gem::from_name(name.strip_suffix("_torch")?).filter(|g| Gem::ALL.contains(g))
}

/// A block texture of this module, if `name` is one.
pub fn paint(name: &str) -> Option<Tex> {
    if name == "crystal_lantern" {
        return Some(crystal_lantern(Ctx::BASE));
    }
    gem_of(name).map(gem_torch)
}

/// An icon of this module, if `name` is one.
pub fn paint_icon(name: &str) -> Option<Tex> {
    if name == "atlas" {
        return Some(atlas_icon());
    }
    gem_of(name).map(gem_torch_icon)
}

/// The atlas (milestone 27; its recipe is copper and glass): a glass globe
/// of a small world, sea and green land under a white glint, held in a
/// copper ring on a copper foot.
fn atlas_icon() -> Tex {
    let salt = seed("atlas_icon");
    let copper = crate::materials::copper_ramp();
    let sea = Ramp::from_hex(&[0x0e2a5a, 0x16407e, 0x2260a4, 0x3a86c6, 0x6aaee0]);
    let land = Ramp::from_hex(&[0x1e4a1e, 0x2e6e2a, 0x48923a, 0x6eb24e, 0x9ccc6a]);
    let (cx, cy, r) = (16.0f32, 13.5f32, 9.5f32);
    let mut icon = Icon::new();
    // The foot and the stem.
    icon.layer(true, |x, y| {
        let (fx, fy) = (c(x), c(y));
        let foot = (26.5..29.5).contains(&fy) && (fx - 16.0).abs() < 7.5 - (fy - 26.5);
        let stem = (22.0..26.5).contains(&fy) && (fx - 16.0).abs() < 1.6;
        (foot || stem).then(|| copper.pick(0.55 - (fx - 16.0) / 20.0, x, y))
    });
    // The globe: sea and land, lit from the top left.
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - cx, c(y) - cy);
        let d = dx.hypot(dy);
        if d > r {
            return None;
        }
        let lit = (0.75 - (dx + dy) / (2.5 * r)).clamp(0.0, 1.0) * (1.0 - (d / r).powi(4) * 0.5);
        // Land in blobs, a little noisy at the shore.
        let u = dx / r;
        let v = dy / r;
        let shore = (u * 3.1 + 0.4).sin() * (v * 2.7 - 0.6).cos()
            + (u * 5.3 - v * 4.1).sin() * 0.35
            + (unit(hash2(salt, (x * 32 + y) as u32)) - 0.5) * 0.15;
        let glint = (dx + 3.5).hypot(dy + 4.0) < 1.6;
        Some(if glint {
            crate::color::Rgba::hex(0xf4fbff)
        } else if shore > 0.25 {
            land.pick(lit, x, y)
        } else {
            sea.pick(lit, x, y)
        })
    });
    // The copper ring round its middle, tilted.
    icon.layer(true, |x, y| {
        let (dx, dy) = (c(x) - cx, c(y) - cy);
        let e = (dx / (r + 1.4)).powi(2) + ((dy - dx * 0.18) / 2.6).powi(2);
        let front = dy - dx * 0.18 > -0.3;
        ((0.78..1.0).contains(&e) && front).then(|| copper.pick(0.75 - dx / 30.0, x, y))
    });
    icon.finish()
}

fn cord_ramp() -> Ramp {
    Ramp::from_hex(&[0x2a1a18, 0x4a2e22, 0x6a462c, 0x8e6a42])
}

/// The gem torch's block texture: soot behind, washed with the gem's glow
/// near the head; the glowing gem on the flame's box; the cord binding and
/// the stick below.
fn gem_torch(gem: Gem) -> Tex {
    let salt = seed(&format!("{}_torch", gem.name()));
    let ramp = gem.ramp();
    let wood = plank_ramp();
    let cord = cord_ramp();
    let soot = Ramp::from_hex(&[0x100c14, 0x18121c, 0x221a26, 0x2e2230]);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        // The gem: facets in two tones split down the middle, a bright
        // heart, the brightest at the top where it burns.
        if (13..19).contains(&x) && (6..12).contains(&y) {
            let across = 1.0 - ((fx - 16.0).abs() / 3.0);
            let up = (12.0 - fy) / 6.0;
            let facet = if fx < 16.0 { 0.1 } else { -0.06 };
            let glint = unit(hash2(salt, (x * 7 + y * 3) as u32)) > 0.86;
            let t = if glint {
                1.0
            } else {
                0.42 + across * 0.34 + up * 0.2 + facet
            };
            return ramp.pick(t, x, y);
        }
        let side = match x {
            13 | 14 => 0.2,
            15 => 0.08,
            16 => -0.04,
            _ => -0.2,
        };
        // The binding: turns of cord holding the gem to the stick.
        if (13..19).contains(&x) && (12..15).contains(&y) {
            let turn = (x + y) % 2 == 0;
            return cord.pick(0.5 + side + if turn { 0.15 } else { -0.12 }, x, y);
        }
        // The stick, grain in short clumps, a knot near the foot.
        if (14..18).contains(&x) && y >= 15 {
            let clump = unit(hash2(salt, (y / 3) as u32 * 4 + x as u32 / 2)) - 0.5;
            let mut t = 0.52 + side + clump * 0.14;
            if (24..26).contains(&y) && x == 16 {
                t -= 0.22;
            }
            return wood.pick(t, x, y);
        }
        // Soot, washed with the gem's light near the head.
        let d = ((fx - 16.0).powi(2) + (fy - 9.0).powi(2)).sqrt();
        let glow = (1.0 - d / 13.0).max(0.0);
        soot.pick(0.15, x, y).mix(ramp.at(5), glow * glow * 0.7)
    })
}

/// The gem torch's icon: a stick standing a little tilted, a rough gem bound
/// to its top.
fn gem_torch_icon(gem: Gem) -> Tex {
    let salt = seed(&format!("{}_torch_icon", gem.name()));
    let ramp = gem.ramp();
    let wood = plank_ramp();
    let cord = cord_ramp();
    // The stick runs from the foot at (9.5, 27.5) up and right at 45 degrees.
    let (ox, oy) = (9.5f32, 27.5f32);
    let uv = |x: usize, y: usize| {
        let (dx, dy) = (c(x) - ox, c(y) - oy);
        (
            (dx - dy) * std::f32::consts::FRAC_1_SQRT_2,
            (dx + dy) * std::f32::consts::FRAC_1_SQRT_2,
        )
    };
    let mut icon = Icon::new();
    icon.layer(true, |x, y| {
        let (u, v) = uv(x, y);
        let along = u.clamp(0.0, 15.0);
        if (u - along).hypot(v) > 1.5 {
            return None;
        }
        let side = -v / 1.5;
        if u > 12.5 {
            let turn = (x + y) % 2 == 0;
            return Some(cord.pick(0.5 + side * 0.25 + if turn { 0.15 } else { -0.15 }, x, y));
        }
        let clump = unit(hash2(salt, (u * 0.5).floor() as i32 as u32)) - 0.5;
        Some(wood.pick(0.5 + side * 0.3 + clump * 0.14, x, y))
    });
    // The gem: three blades of crystal standing out of the binding.
    let up = -std::f32::consts::FRAC_PI_2;
    let tip = (19.5f32, 15.0f32);
    blade(&mut icon, tip, up + 0.35, 8.5, 2.6, &ramp);
    blade(&mut icon, (tip.0 + 1.2, tip.1 + 0.6), up + 0.95, 7.0, 2.2, &ramp);
    blade(&mut icon, (tip.0 - 0.6, tip.1 + 0.2), up - 0.2, 6.0, 2.0, &ramp);
    icon.finish()
}

/// The crystal lantern (Boundless's gleam lantern): a dark metal cage with
/// heavy caps, holding a block of glow crystal that fills it with light. It
/// takes its crystal's colour (tinted by the palette as a whole).
fn crystal_lantern(_ctx: Ctx) -> Tex {
    let salt = seed("crystal_lantern");
    let iron = dark_iron_ramp();
    let crystal = crystal_ramp();
    Tex::from_fn(|x, y| {
        let (fx, fy) = (c(x), c(y));
        let edge = fx.min(fy).min(SIZEF - fx).min(SIZEF - fy);
        let cap = !(5.0..27.0).contains(&fy);
        // The cage: a frame round the face, heavy caps, a rivet at each
        // corner, lit on its top and left.
        if edge < 2.5 || cap {
            for (rx, ry) in [(2.5, 2.5), (29.5, 2.5), (2.5, 29.5), (29.5, 29.5)] {
                if (fx - rx).hypot(fy - ry) < 1.2 {
                    return iron.pick(0.9, x, y);
                }
            }
            let lit = if fx.min(fy) < 1.0 {
                0.78
            } else if (SIZEF - fx).min(SIZEF - fy) < 1.0 {
                0.12
            } else if cap && (fy - 4.0).abs() < 0.6 {
                0.6
            } else {
                0.4
            };
            return iron.pick(lit, x, y);
        }
        // Two thin bars down the front.
        if (fx - 10.5).abs() < 0.55 || (fx - 21.5).abs() < 0.55 {
            return iron.pick(0.5, x, y);
        }
        // The crystal inside: prisms, brightest at its heart.
        let d = ((fx - 16.0) / 11.0).hypot((fy - 16.0) / 11.0);
        let prism = ((fx * 0.55 + (fy * 0.21).sin() * 1.5).floor() as i32).rem_euclid(5) as u32;
        let grain = unit(hash2(salt, prism)) - 0.5;
        let facet = if (fx * 0.55 + (fy * 0.21).sin() * 1.5).fract() < 0.5 {
            0.08
        } else {
            -0.06
        };
        let sparkle = unit(hash2(salt ^ 0x51, (x * 32 + y) as u32)) > 0.97;
        let t = if sparkle {
            1.0
        } else {
            0.92 - d * 0.42 + grain * 0.14 + facet
        };
        crystal.pick(t, x, y)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    #[test]
    fn gem_torches_glow_in_their_gems_colour() {
        for name in GEM_TORCHES {
            let tex = paint(name).expect(name);
            let gem = gem_of(name).unwrap();
            // The head (on average) is the gem's colour, brighter than the
            // soot.
            let mut head = Rgba::new(0.0, 0.0, 0.0, 1.0);
            for y in 6..12 {
                for x in 13..19 {
                    let p = tex.get(x, y);
                    head = Rgba::new(head.r + p.r / 36.0, head.g + p.g / 36.0, head.b + p.b / 36.0, 1.0);
                }
            }
            let soot = tex.get(2, 30);
            let luma = |c: Rgba| c.r * 0.3 + c.g * 0.59 + c.b * 0.11;
            assert!(luma(head) > luma(soot) + 0.2, "{name}");
            // Its strongest channel is the gem's (diamond: pale, every
            // channel bright).
            let mid = gem.ramp().at(4);
            let top = |c: Rgba| {
                let v = [c.r, c.g, c.b];
                (0..3).max_by(|&a, &b| v[a].total_cmp(&v[b])).unwrap()
            };
            if gem == Gem::Diamond {
                assert!(head.r.min(head.g).min(head.b) > 0.6, "{name}: {head:?}");
            } else {
                assert_eq!(top(head), top(mid), "{name}: {head:?}");
            }
            assert!(paint_icon(name).is_some(), "{name} icon");
        }
        assert!(paint("crystal_lantern").is_some());
        assert!(paint("torch").is_none());
    }
}
