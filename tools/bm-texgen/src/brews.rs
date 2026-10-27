//! Brews (milestone 23 part 2, `docs/milestones/23-water-and-brews.md`):
//! the five brew flasks, every brew in its flask, the three oils and the
//! purified and fortified liquids.
//!
//! The flasks are one family: a round glass belly with a neck, stoppered,
//! growing grander by grade. Grade 1 is plain glass with a fired clay
//! collar, foot and a cork; grade 2 trades the collar for copper; grade 3
//! wears silver alloy fittings and a metal cap; grade 4 gold, with a belt
//! round the belly and a knob on the cap; grade 5 is taller, caged in
//! titanium alloy straps. A brew is its grade's flask filled to above the
//! middle with the brew's colour, so the silhouette says the grade and the
//! colour says the kind. The icon rules of `icons.rs`: light from the top
//! left, a dark outline, chunky shapes.
//!
//! Oils are small stoppered vials; the liquids are the food buckets
//! (`food::bucket_icon`), the fortified ones sparkling like the shimmering
//! orb they are made with.

use crate::color::{Ramp, Rgba};
use crate::food::bucket_icon;
use crate::gathered::ramp;
use crate::icons::Icon;
use crate::machines::clay_ramp;
use crate::materials::copper_ramp;
use crate::spark::alloy_ramp;
use crate::tex::{Tex, c};
use crate::worlds::{gold_ramp, titanium_alloy_ramp};

// ---------------------------------------------------------------- flasks

/// A flask's shape and fittings, by grade.
#[derive(Clone, Copy)]
struct Flask {
    grade: u32,
    /// The belly: centre height and radii.
    cy: f32,
    rx: f32,
    ry: f32,
    /// The neck: half width and top.
    neck: f32,
    top: f32,
}

impl Flask {
    fn of(grade: u32) -> Self {
        let (cy, rx, ry, neck, top) = match grade {
            1 => (20.0, 8.5, 8.5, 2.5, 8.0),
            2 => (20.0, 9.0, 8.5, 2.5, 7.5),
            3 => (20.0, 9.5, 8.5, 3.0, 7.0),
            4 => (20.0, 10.5, 8.5, 3.0, 6.5),
            _ => (18.0, 8.5, 10.5, 3.0, 4.5),
        };
        Self {
            grade,
            cy,
            rx,
            ry,
            neck,
            top,
        }
    }

    /// The metal (or clay) of the fittings.
    fn fittings(self) -> Ramp {
        match self.grade {
            1 => clay_ramp(),
            2 => copper_ramp(),
            3 => alloy_ramp(),
            4 => gold_ramp(),
            _ => titanium_alloy_ramp(),
        }
    }

    /// Where the liquid's surface lies.
    fn fill(self) -> f32 {
        self.cy - self.ry * 0.25
    }

    /// The belly's coordinates at a point: `(u, v)` across and down, the
    /// unit circle being its edge.
    fn uv(self, p: (f32, f32)) -> (f32, f32) {
        ((p.0 - 16.0) / self.rx, (p.1 - self.cy) / self.ry)
    }

    fn in_belly(self, p: (f32, f32)) -> bool {
        let (u, v) = self.uv(p);
        u.hypot(v) <= 1.0
    }

    fn in_neck(self, p: (f32, f32)) -> bool {
        (p.0 - 16.0).abs() < self.neck && (self.top..self.cy).contains(&p.1)
    }
}

/// Glass: a cool blue-grey shade to white.
fn glass_ramp() -> Ramp {
    Ramp::from_hex(&[
        0x22344a, 0x3a5668, 0x5e7e90, 0x88a8b8, 0xb2cfdc, 0xd8eef4, 0xf2fbfd, 0xffffff,
    ])
}

/// Cork.
fn cork_ramp() -> Ramp {
    ramp(0xa87850, 0.4, 0.95)
}

/// What a brew does to its liquid besides its colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Look {
    Plain,
    /// Brighter towards the glass, as if lit from within.
    Glow,
    /// A white glow at the heart.
    Halo,
    /// Small bubbles rising.
    Bubbles,
}

/// What fills a flask.
#[derive(Clone, Copy)]
struct Fill {
    rgb: u32,
    look: Look,
}

/// A flask of `grade`, empty or filled.
fn flask(grade: u32, fill: Option<Fill>) -> Tex {
    let f = Flask::of(grade);
    let glass = glass_ramp();
    let metal = f.fittings();
    let liquid = fill.map(|k| match k.look {
        Look::Glow => ramp(k.rgb, 0.55, 1.0),
        Look::Halo => ramp(k.rgb, 0.4, 1.0),
        _ => ramp(k.rgb, 0.22, 0.95),
    });
    let mut icon = Icon::new();
    // The glass belly and neck, and what is in them.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let belly = f.in_belly(p);
        if !belly && !f.in_neck(p) {
            return None;
        }
        let (u, v) = f.uv(p);
        let d = if belly { u.hypot(v) } else { 0.6 };
        // The highlight: a curved streak on the upper left of the belly.
        let streak = belly && (-0.66..-0.42).contains(&u) && (-0.55..0.25).contains(&v);
        let neck_shine = !belly && (p.0 - (16.0 - f.neck + 1.0)).abs() < 0.5;
        if streak || neck_shine {
            return Some(glass.at(7));
        }
        let wet = fill.is_some() && belly && p.1 >= f.fill();
        if let (true, Some(k), Some(l)) = (wet, fill, &liquid) {
            if p.1 < f.fill() + 1.0 {
                // The surface catches the light.
                return Some(l.at(l.len() - 2));
            }
            // Light brews sit high on their ramp, deep ones low, so a yellow
            // stays bright and a purple deep.
            let base = Rgba::hex(k.rgb);
            let luma = 0.2126 * base.r + 0.7152 * base.g + 0.0722 * base.b;
            let mut t = 0.25 + luma * 0.6 - u * 0.25 - v * 0.15;
            match k.look {
                Look::Plain => {}
                Look::Glow => t += 0.1 + (d - 0.5).max(0.0) * 1.6,
                Look::Halo => {
                    let heart = (p.0 - 16.0).hypot(p.1 - (f.cy + 1.5 * f.ry / 8.5));
                    if heart < 2.5 {
                        return Some(Rgba::hex(0xffffff));
                    }
                    if heart < 4.5 {
                        t = t.max(0.9);
                    }
                }
                Look::Bubbles => {
                    for (bx, by, r) in [(18.5f32, 25.0f32, 1.3f32), (14.0, 22.5, 1.0), (19.5, 20.5, 0.8)] {
                        let e = (p.0 - bx).hypot(p.1 - (f.cy - 20.0 + by));
                        if e < r {
                            return Some(l.at(l.len() - 1));
                        }
                        if e < r + 0.8 {
                            return Some(l.at(1));
                        }
                    }
                }
            }
            return Some(l.pick(t, x, y));
        }
        // Empty glass: the far wall seen through it, brighter at the rim.
        let t = 0.42 - u * 0.15 - v * 0.08 + (d - 0.7).max(0.0) * 0.8;
        Some(glass.pick(t, x, y))
    });
    // A belt round the belly (grade 4) or a cage of straps (grade 5).
    if f.grade >= 4 {
        icon.layer(true, |x, y| {
            let p = (c(x), c(y));
            if !f.in_belly(p) {
                return None;
            }
            let (u, v) = f.uv(p);
            let t = 0.6 - u * 0.3;
            if f.grade == 4 {
                return (0.18..0.36).contains(&v).then(|| metal.pick(t, x, y));
            }
            // Meridians: two straps curving with the belly, and a ring under its shoulder.
            let w = (1.0 - v * v).max(0.01).sqrt();
            let m = (u / w).abs();
            let strap = (0.6..0.76).contains(&m) && (-0.72..0.8).contains(&v);
            let ring = (-0.66..-0.52).contains(&v);
            (strap || ring).then(|| metal.pick(t + if ring { 0.1 } else { 0.0 }, x, y))
        });
    }
    // The foot.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let bottom = f.cy + f.ry;
        let half = f.rx * if f.grade >= 3 { 0.7 } else { 0.55 };
        let h = if f.grade >= 3 { 2.5 } else { 2.0 };
        ((p.0 - 16.0).abs() < half && (bottom - h..bottom + 1.0).contains(&p.1))
            .then(|| metal.pick(0.62 - (p.0 - 16.0) / half * 0.3, x, y))
    });
    // The collar at the neck's top.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let half = f.neck + if f.grade >= 3 { 2.0 } else { 1.5 };
        let h = if f.grade >= 2 { 3.0 } else { 2.5 };
        ((p.0 - 16.0).abs() < half && (f.top..f.top + h).contains(&p.1))
            .then(|| metal.pick(0.65 - (p.0 - 16.0) / half * 0.3, x, y))
    });
    // A band where the neck meets the belly (copper and above).
    if f.grade >= 2 {
        icon.layer(true, |x, y| {
            let p = (c(x), c(y));
            let at = f.cy - f.ry + 0.2;
            let half = f.neck + 1.0;
            ((p.0 - 16.0).abs() < half && (at - 1.0..at + 1.0).contains(&p.1))
                .then(|| metal.pick(0.6 - (p.0 - 16.0) / half * 0.3, x, y))
        });
    }
    // The stopper: cork in plain flasks, a metal cap with a knob above.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let dx = p.0 - 16.0;
        if f.grade <= 2 {
            let ok = dx.abs() < f.neck - 0.3 && (f.top - 3.5..f.top).contains(&p.1);
            return ok.then(|| cork_ramp().pick(0.6 - dx / f.neck * 0.3, x, y));
        }
        let cap = dx.abs() < f.neck + 0.5 && (f.top - 2.5..f.top).contains(&p.1);
        let knob = f.grade >= 4 && dx.hypot(p.1 - (f.top - 3.6)) < 1.6;
        (cap || knob).then(|| metal.pick(0.7 - dx / (f.neck + 0.5) * 0.3, x, y))
    });
    icon.finish()
}

// ---------------------------------------------------------------- oils

/// A small stoppered vial of oil.
fn oil(rgb: u32) -> Tex {
    let glass = glass_ramp();
    let oil = ramp(rgb, 0.3, 1.0);
    let mut icon = Icon::new();
    // The vial: a tube with a round bottom, and its neck.
    let in_vial = |p: (f32, f32)| {
        let dx = p.0 - 16.0;
        let tube = dx.abs() < 6.0 && (12.0..23.0).contains(&p.1);
        let bottom = dx.hypot(p.1 - 23.0) < 6.0;
        let neck = dx.abs() < 2.5 && (7.0..12.5).contains(&p.1);
        let shoulder = dx.hypot(p.1 - 12.5) < 6.0 && p.1 > 10.5;
        tube || bottom || neck || (shoulder && p.1 < 12.5 && dx.abs() < 6.0)
    };
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !in_vial(p) {
            return None;
        }
        let u = (p.0 - 16.0) / 6.0;
        if (-0.62..-0.38).contains(&u) && (13.0..24.0).contains(&p.1) {
            return Some(glass.at(7));
        }
        if p.1 >= 14.5 {
            if p.1 < 15.5 {
                return Some(oil.at(oil.len() - 2));
            }
            return Some(oil.pick(0.62 - u * 0.3 - (p.1 - 20.0) / 30.0, x, y));
        }
        Some(glass.pick(0.45 - u * 0.15, x, y))
    });
    // A drop of oil sliding down the glass, glossy.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let d = ((p.0 - 19.0) / 1.2).hypot((p.1 - 11.0) / 1.6);
        (d < 1.0 && in_vial(p)).then(|| oil.at(oil.len() - 1))
    });
    // The lip and the cork.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        let dx = p.0 - 16.0;
        if dx.abs() < 3.5 && (6.5..8.5).contains(&p.1) {
            return Some(glass.pick(0.7 - dx / 3.5 * 0.25, x, y));
        }
        (dx.abs() < 2.3 && (2.0..6.5).contains(&p.1)).then(|| cork_ramp().pick(0.6 - dx / 2.3 * 0.3, x, y))
    });
    icon.finish()
}

// ---------------------------------------------------------------- liquids

/// A bucket of a liquid; `sparkle` adds the shimmering orb's glints.
fn liquid(rgb: u32, sparkle: bool) -> Tex {
    let mut icon = bucket_icon(Some(rgb));
    if sparkle {
        icon.layer(false, |x, y| {
            let p = (c(x), c(y));
            for (sx, sy, r) in [(11.5f32, 9.0f32, 1.6f32), (20.5, 9.5, 1.2)] {
                let (ex, ey) = ((p.0 - sx).abs(), (p.1 - sy).abs());
                if (ex < 0.6 && ey < r) || (ey < 0.6 && ex < r + 0.5) {
                    return Some(Rgba::hex(0xf4ffff));
                }
            }
            None
        });
        // A glint above the rim, outlined like the rest of the sprite.
        icon.layer(false, |x, y| {
            let (ex, ey) = ((c(x) - 26.5).abs(), (c(y) - 4.5).abs());
            let core = ex < 0.6 && ey < 0.6;
            ((ex < 0.6 && ey < 3.1) || (ey < 0.6 && ex < 3.1))
                .then(|| if core { Rgba::hex(0xffffff) } else { Rgba::hex(0x9ae8f4) })
        });
    }
    icon.finish()
}

// ---------------------------------------------------------------- lookups

/// Each kind of brew: its colour, look and the grades it comes in.
const BREWS: [(&str, u32, Look, &[u32]); 13] = [
    ("restoring", 0xe02a2a, Look::Plain, &[1, 2, 3, 4, 5]),
    ("mending", 0xe86a82, Look::Plain, &[1, 2, 3, 4, 5]),
    ("rousing", 0xf6d21e, Look::Plain, &[1, 2, 3, 4, 5]),
    ("steady", 0xd88e2a, Look::Plain, &[1, 2, 3, 4, 5]),
    ("swift", 0x5ab8f2, Look::Plain, &[1, 2, 3, 4, 5]),
    ("brawn", 0x5c2a90, Look::Plain, &[1, 2, 3, 4, 5]),
    ("clearing", 0x2444c0, Look::Plain, &[1, 2, 3, 4, 5]),
    ("hushed", 0xf4b4ca, Look::Plain, &[2, 3, 4]),
    ("glowing", 0xb8f4ff, Look::Glow, &[2, 3, 4]),
    // The ungraded brews and the flask each comes in.
    ("soothing", 0x6ec46e, Look::Plain, &[5]),
    ("taunting", 0xf26a1a, Look::Plain, &[5]),
    ("lifesaving", 0xf0c030, Look::Halo, &[3]),
    ("deepwater", 0x14968e, Look::Bubbles, &[1]),
];

/// Whether a kind is ungraded (named without `_N`).
fn ungraded(kind: &str) -> bool {
    matches!(kind, "soothing" | "taunting" | "lifesaving" | "deepwater")
}

/// The item icons this module paints, in catalogue order.
pub fn icon_names() -> Vec<String> {
    let mut names: Vec<String> = [
        "purified_water",
        "fortified_water",
        "fortified_milk",
        "sulphur_oil",
        "saltpetre_oil",
        "olivine_oil",
    ]
    .map(String::from)
    .to_vec();
    for g in 1..=5 {
        names.push(format!("brew_flask_{g}"));
    }
    for (kind, _, _, grades) in BREWS {
        if ungraded(kind) {
            names.push(format!("{kind}_brew"));
        } else {
            names.extend(grades.iter().map(|g| format!("{kind}_brew_{g}")));
        }
    }
    names
}

/// Paints one of this module's icons, or `None`.
pub fn paint_icon(name: &str) -> Option<Tex> {
    Some(match name {
        "purified_water" => liquid(0x6ab8f0, false),
        "fortified_water" => liquid(0x3a90e0, true),
        "fortified_milk" => liquid(0xeef6ff, true),
        "sulphur_oil" => oil(0xeac81e),
        "saltpetre_oil" => oil(0xdcdcd4),
        "olivine_oil" => oil(0x8a9c2a),
        _ => {
            if let Some(g) = name.strip_prefix("brew_flask_") {
                let g: u32 = g.parse().ok()?;
                return (1..=5).contains(&g).then(|| flask(g, None));
            }
            return brew(name);
        }
    })
}

/// A brew: `kind_brew_N`, or an ungraded `kind_brew`.
fn brew(name: &str) -> Option<Tex> {
    let (kind, grade) = match name.rsplit_once("_brew_") {
        Some((k, g)) => (k, Some(g.parse::<u32>().ok()?)),
        None => (name.strip_suffix("_brew")?, None),
    };
    let (_, rgb, look, grades) = BREWS.into_iter().find(|b| b.0 == kind)?;
    let grade = match grade {
        Some(g) if !ungraded(kind) && grades.contains(&g) => g,
        None if ungraded(kind) => grades[0],
        _ => return None,
    };
    Some(flask(grade, Some(Fill { rgb, look })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(t: &Tex) -> Vec<bool> {
        t.px.iter().map(|p| p.a > 0.0).collect()
    }

    /// Every icon is painted through the crate's lookup, and every one the
    /// catalogue names that this module knows is listed here.
    #[test]
    fn every_brew_icon_is_painted() {
        let names = icon_names();
        assert_eq!(names.len(), 6 + 5 + 7 * 5 + 2 * 3 + 4);
        for n in &names {
            let icon = crate::icons::paint_icon(n).unwrap_or_else(|| panic!("no icon for `{n}`"));
            assert_eq!(Some(&icon), paint_icon(n).as_ref(), "{n}");
            assert!(icon.px.iter().filter(|p| p.a > 0.0).count() > 150, "{n} is too small");
        }
        for n in crate::icon_names() {
            if n.contains("brew") || n.ends_with("_oil") || n.starts_with("fortified") || n == "purified_water" {
                assert!(names.contains(&n), "`{n}` is not listed by brews");
            }
        }
        assert!(paint_icon("restoring_brew_6").is_none());
        assert!(paint_icon("hushed_brew_1").is_none());
        assert!(paint_icon("brew_flask_0").is_none());
    }

    /// A brew wears its grade's flask: the same outline as the empty flask.
    /// Flasks differ by grade, and brews of one grade by their colour.
    #[test]
    fn brews_share_their_flask_and_differ_by_kind_and_grade() {
        let flasks: Vec<Tex> = (1..=5)
            .map(|g| paint_icon(&format!("brew_flask_{g}")).unwrap())
            .collect();
        for i in 0..5 {
            for j in i + 1..5 {
                assert_ne!(
                    mask(&flasks[i]),
                    mask(&flasks[j]),
                    "flasks {} and {} share a shape",
                    i + 1,
                    j + 1
                );
            }
        }
        for g in 1..=5 {
            let brews: Vec<(String, Tex)> = BREWS
                .iter()
                .filter(|b| !ungraded(b.0) && b.3.contains(&g))
                .map(|b| {
                    let n = format!("{}_brew_{g}", b.0);
                    let t = paint_icon(&n).unwrap();
                    (n, t)
                })
                .collect();
            for (n, t) in &brews {
                assert_eq!(mask(t), mask(&flasks[g as usize - 1]), "{n} is not in flask {g}");
            }
            for i in 0..brews.len() {
                for j in i + 1..brews.len() {
                    let (a, b) = (brews[i].1.mean(), brews[j].1.mean());
                    let diff = (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs();
                    assert!(diff > 0.02, "{} and {} look alike", brews[i].0, brews[j].0);
                }
            }
        }
        for (n, g) in [
            ("soothing_brew", 5),
            ("taunting_brew", 5),
            ("lifesaving_brew", 3),
            ("deepwater_brew", 1),
        ] {
            assert_eq!(mask(&paint_icon(n).unwrap()), mask(&flasks[g - 1]), "{n}");
        }
    }
}
