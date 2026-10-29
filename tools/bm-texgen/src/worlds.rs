//! Higher worlds (milestone 18): the gold, titanium, medium coal and hard
//! coal seams' colours, and the icons of what they give: ores, bars,
//! alloys and the alloys' compact, refined and machined forms, the two
//! coals, and the bonding agents silver, gold and titanium gear is glued
//! with. The tools, grapples and spanners of the three alloys are drawn by
//! `icons` (its `Metal`), the atmosphere dishes by `food`.
//!
//! Each metal keeps one ramp through its chain, like copper, iron and
//! silver: gold is a bright yellow, gold alloy (gold and copper) a warm amber gold,
//! titanium a pale teal-white, and titanium alloy a deep blue with a teal
//! sheen, so a bar and the alloy made from it never read alike.

use crate::color::Ramp;
use crate::gathered::ramp;
use crate::icons::{Icon, Lump, blade, ingot};
use crate::materials::{deep_stone_ramp, end_grain_ramp, stone_ramp};
use crate::noise::{hash2, seed, unit};
use crate::spark::{compact, machined, refined};
use crate::tex::{Tex, c, grid};

// ---------------------------------------------------------------- palettes

/// Gold: a brassy brown shade through bright yellow to a cream highlight.
pub(crate) fn gold_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x3e2a12, 0x6e5014, 0xa07c16, 0xcca61c, 0xeac62e, 0xfce258, 0xfff4a0, 0xfffde8,
    ])
}

/// Gold alloy (gold with copper): a deeper, warmer gold with red in its
/// shadows, more yellow than copper and redder than the gold bar.
pub(crate) fn gold_alloy_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x40181a, 0x702e20, 0xa65a22, 0xd08a26, 0xeab036, 0xfad25c, 0xfff0a0, 0xfffcec,
    ])
}

/// Titanium: a pale teal-white metal, greener and lighter than silver's
/// slate or steel's blue.
pub(crate) fn titanium_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x101a26, 0x1e3040, 0x34505e, 0x527682, 0x7aa0aa, 0xa6c8cc, 0xd6eeee, 0xffffff,
    ])
}

/// Titanium alloy: a deep violet-blue in shade running to a teal sheen,
/// like heat-tinted metal.
pub(crate) fn titanium_alloy_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x1e1438, 0x2a2a62, 0x2c4888, 0x2e6ea4, 0x3c9abc, 0x72c6d2, 0xbeeaea, 0xffffff,
    ])
}

/// Medium coal: a deeper black than coal, with brighter, glossier lights.
pub(crate) fn medium_coal_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x050407, 0x0b0a10, 0x131219, 0x1d1c25, 0x2c2b38, 0x4a4b5e, 0x8a8ea8, 0xdce0f0,
    ])
}

/// Hard coal: black with a faint blue-violet sheen on its lit faces.
pub(crate) fn hard_coal_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x06050e, 0x0c0a18, 0x141024, 0x1e1834, 0x2c2448, 0x463a70, 0x7a70b0, 0xd4ccf4,
    ])
}

// ------------------------------------------------------------------ icons

/// Gold ore: a lump of stone with fat gold nuggets in it, rounder and
/// fewer than silver ore's.
fn gold_ore() -> Tex {
    let salt = seed("gold_ore");
    let mut icon = Icon::new();
    Lump::new(salt, 16.0, 17.0, 12.2, 8).draw(&mut icon, &stone_ramp(), 0.12, 0.8, false);
    let gold = gold_ramp();
    for (i, &(x, y, r)) in [
        (12.0, 13.0, 4.2),
        (20.5, 15.5, 3.2),
        (14.5, 21.5, 3.6),
        (22.0, 22.5, 2.2),
    ]
    .iter()
    .enumerate()
    {
        Lump::new(hash2(salt, 10 + i as u32), x, y, r, 6).draw(&mut icon, &gold, 0.3, 0.98, true);
    }
    icon.finish()
}

/// Titanium ore: a lump of dark deep rock with pale crystals of titanium
/// growing out of it towards the top right.
fn titanium_ore() -> Tex {
    let salt = seed("titanium_ore");
    let ti = titanium_ramp();
    let mut icon = Icon::new();
    let up = -std::f32::consts::FRAC_PI_4;
    // Crystals behind the rock, their feet hidden in it.
    blade(&mut icon, (15.0, 16.0), up - 0.5, 13.5, 2.6, &ti);
    blade(&mut icon, (17.0, 17.0), up + 0.1, 14.5, 3.0, &ti);
    blade(&mut icon, (18.0, 18.5), up + 0.75, 11.0, 2.4, &ti);
    Lump::new(salt, 13.5, 21.0, 10.5, 7).draw(&mut icon, &deep_stone_ramp(), 0.18, 0.95, false);
    // Small crystals set in the rock's face.
    for (i, &(x, y, r)) in [(9.5, 19.0, 2.2), (14.5, 25.0, 1.9)].iter().enumerate() {
        Lump::new(hash2(salt, 10 + i as u32), x, y, r, 4).draw(&mut icon, &ti, 0.35, 1.0, true);
    }
    icon.finish()
}

/// Coal lumps through `ramp`: `sides` facets each (fewer is more angular).
fn coal_lumps(name: &str, ramp: &Ramp, sides: usize, glint: bool) -> Tex {
    let salt = seed(name);
    let mut icon = Icon::new();
    Lump::new(hash2(salt, 2), 20.5, 11.5, 7.2, sides).draw(&mut icon, ramp, 0.16, 0.92, glint);
    Lump::new(hash2(salt, 1), 12.5, 18.5, 9.6, sides + 1).draw(&mut icon, ramp, 0.18, 0.96, glint);
    Lump::new(hash2(salt, 3), 23.0, 23.0, 5.6, sides).draw(&mut icon, ramp, 0.16, 0.92, glint);
    icon.finish()
}

/// Medium coal: three glossy black lumps, each with a bright glint.
fn medium_coal() -> Tex {
    coal_lumps("medium_coal", &medium_coal_ramp(), 6, true)
}

/// Hard coal: three sharp-edged black blocks with a violet sheen.
fn hard_coal() -> Tex {
    let salt = seed("hard_coal");
    let ramp = hard_coal_ramp();
    let mut icon = Icon::new();
    Lump::new(hash2(salt, 2), 20.0, 11.5, 7.6, 4).draw(&mut icon, &ramp, 0.12, 1.0, false);
    Lump::new(hash2(salt, 1), 12.5, 19.0, 10.0, 5).draw(&mut icon, &ramp, 0.12, 1.0, true);
    Lump::new(hash2(salt, 3), 23.5, 23.0, 5.8, 4).draw(&mut icon, &ramp, 0.12, 1.0, false);
    icon.finish()
}

/// A bonding agent: a corked glass jar of thick paste. The plain agent is
/// amber glue swirled with orange; the strong one is a glowing teal paste
/// flecked with sparkles, a copper wire twisted round its neck.
fn bonding_agent(strong: bool) -> Tex {
    if strong {
        bonding_agent_in(
            ramp(0x30b0b8, 0.3, 0.98),
            ramp(0xa0f0f0, 0.5, 1.0),
            "strong_bonding_agent",
        )
    } else {
        bonding_agent_in(ramp(0xe0a030, 0.35, 0.96), ramp(0xf07020, 0.35, 0.95), "bonding_agent")
    }
}

/// The supreme bonding agent (milestone 21): a deep violet paste swirled
/// with the shadow orb's light.
pub fn supreme_bonding_agent() -> Tex {
    bonding_agent_in(
        ramp(0x5a2a9a, 0.25, 0.95),
        ramp(0xd8b0ff, 0.5, 1.0),
        "supreme_bonding_agent",
    )
}

fn bonding_agent_in(paste: Ramp, swirl: Ramp, name: &str) -> Tex {
    let glass = Ramp::from_hex(&[0x3a4a64, 0x5a7090, 0x8aa6c0, 0xb8d0e0, 0xe0eef6, 0xffffff]);
    let cork = end_grain_ramp();
    let salt = seed(name);
    // The stronger agents fleck and wire their jars.
    let strong = name != "bonding_agent";
    let (x0, x1, y0, y1, round) = (7.5f32, 24.5f32, 11.0f32, 28.5f32, 4.0f32);
    let mut icon = Icon::new();
    // The jar's body and neck.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let neck = (p.0 - 16.0).abs() < 5.5 && (7.5..12.0).contains(&p.1);
        let (qx, qy) = (
            (x0 + round - p.0).max(p.0 - (x1 - round)).max(0.0),
            (y0 + round - p.1).max(p.1 - (y1 - round)).max(0.0),
        );
        let body = (x0..x1).contains(&p.0) && (y0..y1).contains(&p.1) && qx.hypot(qy) < round;
        if !body && !neck {
            return None;
        }
        let rim = body && (p.0 - x0 < 1.2 || x1 - p.0 < 1.2 || y1 - p.1 < 1.2);
        if neck || rim || p.1 < 14.0 {
            // Glass: lighter on the left, a lip at the neck's top.
            let mut t = 0.55 - (p.0 - 16.0) / 9.0 * 0.25;
            if neck && p.1 < 9.0 {
                t += 0.25;
            }
            return Some(glass.pick(t, x, y));
        }
        // The paste, folded through with wavy streaks of a second colour.
        let (dx, dy) = (p.0 - 16.0, p.1 - 21.0);
        let lit = -(dx / 8.5) * 0.25 - dy / 7.5 * 0.15;
        let streak = (p.1 + p.0 * 0.45 + (p.0 * 0.6).sin() * 1.4).rem_euclid(5.5);
        if p.1 > 15.5 && streak < 1.3 {
            return Some(swirl.pick(0.6 + lit, x, y));
        }
        if strong {
            let cell = (((p.0 * 0.5) as i32) * 13 + (p.1 * 0.5) as i32) as u32;
            if unit(hash2(salt, cell)) > 0.88 {
                return Some(swirl.lightest());
            }
        }
        Some(paste.pick(0.5 + lit + if p.1 < 15.5 { 0.2 } else { 0.0 }, x, y))
    });
    // A shine down the glass on the left.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        ((p.0 - 10.0).abs() < 0.7 && (16.0..25.0).contains(&p.1)).then(|| glass.lightest())
    });
    // The cork.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        ((p.0 - 16.0).abs() < 4.5 && (3.5..8.5).contains(&p.1)).then(|| cork.pick(0.62 - (p.0 - 16.0) * 0.05, x, y))
    });
    if strong {
        let copper = crate::materials::copper_ramp();
        icon.layer(true, |x, y| {
            let p = (c(x), c(y));
            if (p.0 - 16.0).abs() > 6.5 || !(9.0..11.0).contains(&p.1) {
                return None;
            }
            let twist = (grid(x) + grid(y)) % 2 == 0;
            Some(copper.pick(if twist { 0.85 } else { 0.5 }, x, y))
        });
    }
    icon.finish()
}

// ---------------------------------------------------------------- lookups

/// The block textures this milestone adds, painted now so the files are in
/// place when the catalogue names them.
pub const BLOCKS: [&str; 4] = ["gold_seam", "titanium_seam", "medium_coal_seam", "hard_coal_seam"];

/// The item icons this milestone adds (painted here, by `icons` or by
/// `food`), painted now so the files are in place when the catalogue names
/// them.
pub const ICONS: [&str; 43] = [
    "gold_ore",
    "gold_ingot",
    "gold_alloy",
    "titanium_ore",
    "titanium_ingot",
    "titanium_alloy",
    "medium_coal",
    "hard_coal",
    "compact_gold_alloy",
    "refined_gold_alloy",
    "machined_gold_alloy",
    "compact_titanium_alloy",
    "refined_titanium_alloy",
    "machined_titanium_alloy",
    "bonding_agent",
    "strong_bonding_agent",
    "silver_hammer",
    "silver_shovel",
    "silver_axe",
    "gold_hammer",
    "gold_shovel",
    "gold_axe",
    "titanium_hammer",
    "titanium_shovel",
    "titanium_axe",
    "silver_grapple",
    "gold_grapple",
    "titanium_grapple",
    "silver_spanner",
    "gold_spanner",
    "titanium_spanner",
    "acid_chilli",
    "spark_chilli",
    "fire_chilli",
    "acid_rice",
    "spark_rice",
    "fire_rice",
    "raw_acid_pie",
    "acid_pie",
    "spark_pie",
    "fire_pie",
    "purified_milk",
    "purified_butter",
];

/// Paints the icon called `name`, or `None` if this module does not know it.
pub fn paint_icon(name: &str) -> Option<Tex> {
    Some(match name {
        "gold_ore" => gold_ore(),
        "gold_ingot" => ingot(gold_ramp()),
        "gold_alloy" => ingot(gold_alloy_ramp()),
        "titanium_ore" => titanium_ore(),
        "titanium_ingot" => ingot(titanium_ramp()),
        "titanium_alloy" => ingot(titanium_alloy_ramp()),
        "medium_coal" => medium_coal(),
        "hard_coal" => hard_coal(),
        "compact_gold_alloy" => compact(gold_alloy_ramp()),
        "refined_gold_alloy" => refined(gold_alloy_ramp()),
        "machined_gold_alloy" => machined(gold_alloy_ramp()),
        "compact_titanium_alloy" => compact(titanium_alloy_ramp()),
        "refined_titanium_alloy" => refined(titanium_alloy_ramp()),
        "machined_titanium_alloy" => machined(titanium_alloy_ramp()),
        "bonding_agent" => bonding_agent(false),
        "strong_bonding_agent" => bonding_agent(true),
        _ => return None,
    })
}

/// The review sheet of this milestone's art (`docs/media/m18-icons.png`):
/// the four seams beside the coal and silver seams they join, tiled, then
/// every new icon beside the iron and silver ones it follows.
pub fn review_sheet() -> Result<crate::canvas::Canvas, String> {
    use crate::canvas::Canvas;
    use crate::color::Rgba;
    let all = crate::families()?;
    let seams: Vec<crate::Family> = ["coal_seam", "silver_seam"]
        .into_iter()
        .chain(BLOCKS)
        .map(|n| crate::find(&all, n).clone())
        .collect();
    let mut names: Vec<&str> = vec![
        "iron_hammer",
        "copper_ingot",
        "silver_ingot",
        "silver_alloy",
        "coal",
        "stewed_dewberries",
    ];
    names.extend(ICONS);
    let icons: Vec<(String, Tex)> = names
        .iter()
        .map(|n| {
            crate::icons::paint_icon(n)
                .map(|t| (n.to_string(), t))
                .ok_or_else(|| format!("no icon `{n}`"))
        })
        .collect::<Result<_, _>>()?;
    let top = crate::sheet::contact_sheet(&seams, &[], &[]);
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

    /// Every milestone 18 texture and icon has a painter, and no two icons
    /// are alike (a recoloured tool must not come out as another metal's).
    #[test]
    fn every_higher_worlds_texture_and_icon_is_painted_and_distinct() {
        for n in BLOCKS {
            assert!(crate::materials::paint_named(n, 0).is_some(), "no painter for `{n}`");
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
        // And none repeats an older metal's icon.
        for (old, new) in [
            ("iron_hammer", "silver_hammer"),
            ("silver_ingot", "titanium_ingot"),
            ("silver_alloy", "titanium_alloy"),
            ("copper_ingot", "gold_alloy"),
        ] {
            assert_ne!(
                crate::icons::paint_icon(old),
                crate::icons::paint_icon(new),
                "{old} vs {new}"
            );
        }
    }

    /// The new seams are told apart at a distance (art direction rule 2):
    /// each one's average colour differs from stone's, from every older
    /// seam's and from each other's.
    #[test]
    fn the_new_seams_differ_from_each_other_and_stone_at_a_distance() {
        let mut names = vec!["stone", "coal_seam", "silver_seam", "copper_vein", "iron_vein"];
        names.extend(BLOCKS);
        let means: Vec<_> = names
            .iter()
            .map(|n| crate::materials::paint_named(n, 0).unwrap().mean())
            .collect();
        for (i, n) in names.iter().enumerate() {
            for j in 0..i {
                if !BLOCKS.contains(n) {
                    continue;
                }
                let (a, b) = (means[i], means[j]);
                let d = (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs();
                assert!(d > 0.03, "`{n}` and `{}` look alike from afar ({d})", names[j]);
            }
        }
    }
}
