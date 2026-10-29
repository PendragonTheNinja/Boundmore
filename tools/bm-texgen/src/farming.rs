//! Farming (milestone 16 part 2): the four growth stages of every crop,
//! drawn as plant pictures like the gathering plants (`flora.rs`: cut-out,
//! alpha 0 or 1, the foot on the bottom row in the middle), and the icons
//! of seeds, bulbs and grain.
//!
//! Each crop's stages read as one plant growing: stage 1 a sprout low in
//! the tile, 2 young, 3 grown without fruit, 4 ripe and plainly ready (red
//! berries, a tuber showing at the foot, gold grain heads). Each stage
//! stands taller and fuller than the last. Boundmore's own designs, painted
//! with the brief's rules.

use crate::color::{HUE_GOLD, HUE_PURPLE, Ramp, RampSpec, Rgba};
use crate::flora::{Blade, P, Profile, Sprite, fan, leaf_ramp, ramp};
use crate::gathered::blob;
use crate::icons::{Icon, cord_ramp, seg_dist};
use crate::noise::{Rng, hash2, seed, unit};
use crate::tex::{Tex, c, grid};

const FOOT: P = (16.0, 31.5);

/// A ramp for icon parts: purple shadows, golden lights.
fn icon_ramp(rgb: u32, dark: f32, light: f32) -> Ramp {
    crate::gathered::ramp(rgb, dark, light)
}

/// A ramp for ripe grain: warm, glowing gold.
fn grain_ramp(rgb: u32) -> Ramp {
    RampSpec::new(Rgba::hex(rgb), 7, 0.4, 0.95)
        .cool(HUE_PURPLE, 0.2)
        .warm(HUE_GOLD, 0.45)
        .build()
}

/// Mixes two `0xrrggbb` colours, `t` of the way from `a` to `b`.
pub(crate) fn mix_hex(a: u32, b: u32, t: f32) -> u32 {
    let ch = |v: u32, s: u32| ((v >> s) & 0xff) as f32;
    let m = |s: u32| ((ch(a, s) + (ch(b, s) - ch(a, s)) * t).round().clamp(0.0, 255.0) as u32) << s;
    m(16) | m(8) | m(0)
}

fn stalk(s: &mut Sprite, foot: P, tip: P, bend: f32, width: f32, tone: f32, r: &Ramp) {
    s.blade(
        &Blade {
            foot,
            tip,
            bend,
            width,
            profile: Profile::Strap,
            tone,
            rib: false,
        },
        r,
    );
}

#[allow(clippy::too_many_arguments)]
fn leaf(s: &mut Sprite, foot: P, tip: P, bend: f32, width: f32, profile: Profile, tone: f32, r: &Ramp) {
    s.blade(
        &Blade {
            foot,
            tip,
            bend,
            width,
            profile,
            tone,
            rib: true,
        },
        r,
    );
}

// ---------------------------------------------------------------- dewberry

/// Dewberry bush: two seed leaves, a young leafy tuft, a rounded bush on
/// woody stems, and the bush hung with clusters of red berries.
fn dewberry(stage: u8) -> Tex {
    let mut rng = Rng::new(seed(&format!("dewberry_{stage}")));
    let leaves = leaf_ramp(0x46a04a);
    let wood = ramp(0x7a5034, 0.3, 0.72);
    let mut s = Sprite::new();
    match stage {
        1 => {
            stalk(&mut s, FOOT, (16.0, 26.5), 0.3, 0.9, -0.05, &leaves);
            leaf(
                &mut s,
                (16.0, 27.0),
                (10.0, 24.5),
                0.6,
                2.4,
                Profile::Round,
                0.0,
                &leaves,
            );
            leaf(
                &mut s,
                (16.0, 27.0),
                (22.0, 24.0),
                -0.6,
                2.4,
                Profile::Round,
                0.08,
                &leaves,
            );
        }
        2 => {
            fan(
                &mut s,
                &mut rng,
                FOOT,
                5,
                (-70.0, 70.0),
                (9.0, 11.5),
                2.7,
                1.0,
                Profile::Leaf,
                &leaves,
            );
            fan(
                &mut s,
                &mut rng,
                (16.0, 27.0),
                3,
                (-30.0, 30.0),
                (6.0, 7.5),
                2.4,
                0.4,
                Profile::Leaf,
                &leaves,
            );
        }
        _ => {
            let ripe = stage == 4;
            let lift = if ripe { 5.0 } else { 0.0 };
            let spread = if ripe { 1.15 } else { 1.0 };
            let sx = |x: f32| 16.0 + (x - 16.0) * spread;
            for (tip, bend) in [((9.0, 20.0), 1.4), ((23.0, 19.5), -1.4), ((16.0, 16.0), 0.3)] {
                stalk(&mut s, FOOT, (sx(tip.0), tip.1 - lift), bend, 1.0, -0.1, &wood);
            }
            // Leaf clumps, back ones first.
            let mut clumps: Vec<(P, f32)> = vec![
                ((9.0, 21.0), -0.12),
                ((23.0, 20.5), -0.1),
                ((16.0, 15.0), -0.04),
                ((11.5, 26.0), 0.0),
                ((20.5, 26.0), 0.02),
                ((16.0, 20.5), 0.08),
            ];
            if ripe {
                clumps.insert(0, ((6.5, 15.5), -0.16));
                clumps.insert(1, ((25.5, 15.0), -0.16));
                clumps.insert(2, ((16.0, 9.5), -0.1));
            }
            for &((x, y), tone) in &clumps {
                let lifted = if y < 24.0 { lift * (24.0 - y) / 9.0 } else { 0.0 };
                let centre = (sx(x), y - lifted.min(lift));
                for k in 0..6 {
                    let a = (k as f32 / 6.0 * 360.0 + rng.range(-15.0, 15.0)).to_radians();
                    let l = rng.range(4.0, 5.6);
                    let tip = (centre.0 + a.sin() * l, centre.1 - a.cos() * l * 0.85);
                    leaf(
                        &mut s,
                        centre,
                        tip,
                        rng.range(-0.6, 0.6),
                        2.5,
                        Profile::Leaf,
                        tone,
                        &leaves,
                    );
                }
            }
            if ripe {
                let red = RampSpec::new(Rgba::hex(0xd8283c), 7, 0.3, 0.97)
                    .cool(HUE_PURPLE, 0.35)
                    .warm(HUE_GOLD, 0.2)
                    .build();
                let shine = Rgba::hex(0xffe8ec);
                for (x, y) in [
                    (8.0, 14.0),
                    (23.5, 12.5),
                    (15.0, 8.0),
                    (11.5, 21.0),
                    (21.0, 19.5),
                    (16.5, 14.5),
                    (6.5, 24.0),
                    (25.0, 23.5),
                    (15.5, 25.5),
                ] {
                    for (dx, dy) in [(0.0, 0.0), (2.6, 0.6), (1.2, 2.4)] {
                        let (bx, by) = (x + dx, y + dy);
                        s.disc((bx, by), (1.6, 1.6), &red, 0.55);
                        s.dot(bx as i32 - 1, by as i32 - 1, shine);
                    }
                }
            }
        }
    }
    s.finish()
}

// ----------------------------------------------------------------- tubers

struct TuberLook {
    leaf: u32,
    tuber: u32,
    profile: Profile,
    width: f32,
    gloss: bool,
    stripes: bool,
}

/// A tuber plant: a sprout, a rosette, a full leafy top, and the leafy top
/// with its tubers pushing out of the ground at the foot.
fn tuber_crop(look: &TuberLook, name: &str, stage: u8) -> Tex {
    let mut rng = Rng::new(seed(&format!("{name}_{stage}")));
    let leaves = leaf_ramp(look.leaf);
    let w = look.width;
    let mut s = Sprite::new();
    match stage {
        1 => fan(
            &mut s,
            &mut rng,
            FOOT,
            3,
            (-40.0, 40.0),
            (5.5, 7.5),
            w * 0.6,
            0.5,
            look.profile,
            &leaves,
        ),
        2 => {
            fan(
                &mut s,
                &mut rng,
                FOOT,
                5,
                (-65.0, 65.0),
                (10.0, 13.0),
                w * 0.8,
                1.0,
                look.profile,
                &leaves,
            );
        }
        _ => {
            let more = if stage == 4 { 3.5 } else { 0.0 };
            fan(
                &mut s,
                &mut rng,
                FOOT,
                6,
                (-82.0, 82.0),
                (14.0 + more, 17.0 + more),
                w,
                1.6,
                look.profile,
                &leaves,
            );
            fan(
                &mut s,
                &mut rng,
                FOOT,
                4,
                (-35.0, 35.0),
                (19.0 + more, 22.0 + more),
                w,
                0.8,
                look.profile,
                &leaves,
            );
            if stage == 4 {
                let r = icon_ramp(look.tuber, 0.3, 0.95);
                for (i, &(x, y, rx, ry)) in [
                    (10.0f32, 29.0f32, 5.0f32, 3.4f32),
                    (22.0, 29.2, 4.6, 3.2),
                    (16.0, 30.0, 4.2, 2.6),
                ]
                .iter()
                .enumerate()
                {
                    s.part(|px, py| {
                        let (dx, dy) = ((c(px) - x) / rx, (c(py) - y) / ry);
                        let d = dx.hypot(dy);
                        if d > 1.0 {
                            return None;
                        }
                        let mut t = 0.52 - (dx * 0.6 + dy * 0.8) * 0.28 - d * 0.08;
                        if look.stripes && ((c(px) - x + i as f32) * 0.5).rem_euclid(2.0) < 0.6 {
                            t -= 0.25;
                        }
                        if look.gloss && (dx + 0.4).hypot(dy + 0.45) < 0.28 {
                            t = 1.0;
                        }
                        // An eye or two.
                        if (dx - 0.3).hypot(dy - 0.1) < 0.18 {
                            t -= 0.25;
                        }
                        Some(r.pick(t, px, py))
                    });
                }
            }
        }
    }
    s.finish()
}

// ----------------------------------------------------------------- grains

#[derive(Clone, Copy, PartialEq)]
enum Head {
    /// Wheat: upright ears with whiskers.
    Ear,
    /// Oats: loose spikelets dangling from arching stems.
    Droop,
    /// Rice: stems bowing over under a hanging string of grains.
    Panicle,
}

struct GrainLook {
    green: u32,
    ripe: u32,
    grain: u32,
    head: Head,
}

/// A grain crop: a few blades, a young tuft, tall green stalks, and the
/// stalks turned gold under their heads.
fn grain_crop(look: &GrainLook, name: &str, stage: u8) -> Tex {
    let mut rng = Rng::new(seed(&format!("{name}_{stage}")));
    let green = leaf_ramp(look.green);
    let mut s = Sprite::new();
    match stage {
        1 => fan(
            &mut s,
            &mut rng,
            FOOT,
            3,
            (-25.0, 25.0),
            (6.5, 9.0),
            1.6,
            0.4,
            Profile::Spike,
            &green,
        ),
        2 => fan(
            &mut s,
            &mut rng,
            FOOT,
            5,
            (-40.0, 40.0),
            (11.0, 14.5),
            1.6,
            0.9,
            Profile::Spike,
            &green,
        ),
        3 => {
            for (tip, bend) in [((10.5, 10.0), 0.6), ((16.5, 7.5), 0.1), ((22.0, 10.5), -0.6)] {
                stalk(&mut s, (16.0, 31.5), tip, bend, 0.75, -0.02, &green);
                // The young head still sheathed in green.
                leaf(
                    &mut s,
                    (tip.0, tip.1 + 3.0),
                    (tip.0 + (tip.0 - 16.0) * 0.1, tip.1 - 2.0),
                    0.0,
                    1.3,
                    Profile::Round,
                    0.08,
                    &green,
                );
            }
            fan(
                &mut s,
                &mut rng,
                FOOT,
                6,
                (-55.0, 55.0),
                (13.0, 18.0),
                1.7,
                1.8,
                Profile::Spike,
                &green,
            );
        }
        _ => {
            let straw = leaf_ramp(mix_hex(look.green, look.ripe, 0.6));
            let heads = grain_ramp(look.grain);
            match look.head {
                Head::Ear => {
                    let ears = [
                        ((6.5, 15.0), 1.2),
                        ((11.5, 11.5), 0.6),
                        ((16.5, 10.0), 0.0),
                        ((21.5, 12.0), -0.6),
                        ((26.0, 15.5), -1.2),
                    ];
                    for &(tip, bend) in &ears {
                        stalk(&mut s, (16.0, 31.5), tip, bend, 0.75, 0.0, &straw);
                    }
                    for &(tip, _) in &ears {
                        let mid = (tip.0 + (tip.0 - 16.0) * 0.06, tip.1 - 4.2);
                        // Whiskers first, behind the ear, standing straight up.
                        for side in [-1.0f32, 0.0, 1.0] {
                            stalk(
                                &mut s,
                                (mid.0 + side * 1.0, mid.1 - 1.0),
                                (mid.0 + side * 1.8, mid.1 - 8.0),
                                0.0,
                                0.45,
                                0.25,
                                &heads,
                            );
                        }
                        s.part(|x, y| {
                            let p = (c(x), c(y));
                            let (dx, dy) = ((p.0 - mid.0) / 2.3, (p.1 - mid.1) / 4.4);
                            if dx.hypot(dy) > 1.0 {
                                return None;
                            }
                            // Kernels in two staggered columns, a dark seam
                            // between them.
                            let left = p.0 < mid.0;
                            let row = ((p.1 - mid.1 + if left { 0.0 } else { 1.0 }) / 2.0).floor() as i32;
                            let mut t = 0.6 - dx * 0.2 - dy * 0.1 + if row % 2 == 0 { 0.12 } else { -0.06 };
                            if (p.0 - mid.0).abs() < 0.4 {
                                t -= 0.2;
                            }
                            Some(heads.pick(t, x, y))
                        });
                    }
                }
                Head::Droop => {
                    for (tip, bend, out) in [
                        ((5.5, 11.0), 1.8, -1.0),
                        ((25.5, 10.5), -1.8, 1.0),
                        ((10.5, 6.5), 1.0, -1.0),
                        ((21.5, 6.0), -1.0, 1.0),
                        ((16.0, 3.5), 0.3, 1.0),
                    ] {
                        let b = Blade {
                            foot: (16.0, 31.5),
                            tip,
                            bend,
                            width: 0.75,
                            profile: Profile::Strap,
                            tone: 0.0,
                            rib: false,
                        };
                        s.blade(&b, &straw);
                        // Spikelets dangling along the top third.
                        for k in 0..3 {
                            let at = b.point(0.72 + k as f32 * 0.13);
                            let side = if k % 2 == 0 { out } else { -out };
                            leaf(
                                &mut s,
                                at,
                                (at.0 + side * 2.4, at.1 + 3.6),
                                0.0,
                                1.3,
                                Profile::Round,
                                0.05 * k as f32,
                                &heads,
                            );
                        }
                    }
                }
                Head::Panicle => {
                    for (tip, bend, out) in [
                        ((7.5, 11.0), 1.2, -1.0),
                        ((24.5, 11.5), -1.2, 1.0),
                        ((12.0, 6.5), 0.6, -1.0),
                        ((20.5, 7.0), -0.6, 1.0),
                        ((16.5, 4.5), 0.0, 1.0),
                    ] {
                        stalk(&mut s, (16.0, 31.5), tip, bend, 0.75, 0.0, &straw);
                        // The panicle bows over and hangs: a string of grains.
                        let hang = Blade {
                            foot: tip,
                            tip: (tip.0 + out * 4.5, tip.1 + 7.5),
                            bend: -out * 1.6,
                            width: 0.6,
                            profile: Profile::Strap,
                            tone: 0.0,
                            rib: false,
                        };
                        s.blade(&hang, &straw);
                        for k in 0..6 {
                            let at = hang.point(k as f32 / 5.0);
                            s.disc(at, (1.3, 1.1), &heads, 0.5 + (k % 2) as f32 * 0.1);
                        }
                    }
                }
            }
            fan(
                &mut s,
                &mut rng,
                FOOT,
                6,
                (-60.0, 60.0),
                (13.0, 18.0),
                1.7,
                2.0,
                Profile::Spike,
                &straw,
            );
        }
    }
    s.finish()
}

const DEWBERRY: &str = "dewberry";

fn tuber_look(name: &str) -> Option<TuberLook> {
    Some(match name {
        "tuber" => TuberLook {
            leaf: 0x4f9a40,
            tuber: 0x9a6a40,
            profile: Profile::Leaf,
            width: 3.2,
            gloss: false,
            stripes: false,
        },
        "waxy_tuber" => TuberLook {
            leaf: 0x86b03a,
            tuber: 0xe8c860,
            profile: Profile::Round,
            width: 3.0,
            gloss: true,
            stripes: false,
        },
        "exotic_tuber" => TuberLook {
            leaf: 0x7a5a9a,
            tuber: 0x9a3a6a,
            profile: Profile::Spike,
            width: 3.4,
            gloss: false,
            stripes: true,
        },
        _ => return None,
    })
}

fn grain_look(name: &str) -> Option<GrainLook> {
    Some(match name {
        "oats" => GrainLook {
            green: 0x5a9a48,
            ripe: 0xc8b878,
            grain: 0xe8d490,
            head: Head::Droop,
        },
        "wheat" => GrainLook {
            green: 0x4e9440,
            ripe: 0xc89a40,
            grain: 0xd89a2c,
            head: Head::Ear,
        },
        "rice" => GrainLook {
            green: 0x4aa050,
            ripe: 0x9ab048,
            grain: 0xd8c860,
            head: Head::Panicle,
        },
        _ => return None,
    })
}

/// Every crop, in catalogue order (`assets/crops.toml`).
pub const CROPS: [&str; 7] = [
    "dewberry",
    "tuber",
    "waxy_tuber",
    "exotic_tuber",
    "oats",
    "wheat",
    "rice",
];

/// Paints the crop stage texture `<crop>_<stage>`, or `None` if `name` is
/// not one.
pub fn paint_crop(name: &str) -> Option<Tex> {
    let (crop, stage) = name.rsplit_once('_')?;
    let stage: u8 = stage.parse().ok()?;
    if !(1..=4).contains(&stage) {
        return None;
    }
    if crop == DEWBERRY {
        return Some(dewberry(stage));
    }
    if let Some(look) = tuber_look(crop) {
        return Some(tuber_crop(&look, crop, stage));
    }
    grain_look(crop).map(|look| grain_crop(&look, crop, stage))
}

// ------------------------------------------------------------------ icons

/// A small cloth pouch tied at the neck, tinted towards its crop, with a
/// few of its seeds spilled in front.
fn pouch(cloth: u32, seeds: u32, name: &str) -> Tex {
    let cl = icon_ramp(mix_hex(0xc8a878, cloth, 0.4), 0.32, 0.92);
    let sd = icon_ramp(seeds, 0.28, 0.95);
    let cord = cord_ramp();
    let salt = seed(name);
    let mut icon = Icon::new();
    // The ruffled mouth above the tie, behind the body.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !(4.5..13.0).contains(&p.1) {
            return None;
        }
        let half = 2.8 + (13.0 - p.1) * 0.55 + ((p.0 * 1.3).sin() * 0.4);
        if (p.0 - 14.5).abs() > half {
            return None;
        }
        let fold = ((p.0 - 14.5) * 0.9).round() as i32 % 2 == 0;
        Some(cl.pick(0.5 + if fold { 0.12 } else { -0.1 } - (p.0 - 14.5) / half * 0.15, x, y))
    });
    blob(&mut icon, (14.5, 20.0), (9.5, 8.6), 0.0, &cl, 0.05, false, salt);
    // A fold down the body.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (u, v) = ((p.0 - 14.5) / 9.5, (p.1 - 20.0) / 8.6);
        (u.hypot(v) < 0.85 && seg_dist((11.0, 14.0), (9.5, 25.0), p) < 0.6).then(|| cl.at(1))
    });
    // The tie.
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        ((p.0 - 14.5).abs() < 4.6 && (11.5..13.5).contains(&p.1)).then(|| cord.pick(0.55 - (p.0 - 14.5) * 0.05, x, y))
    });
    for (i, &(x, y, turn)) in [(23.5f32, 27.0f32, 0.5f32), (27.0, 22.5, -0.4), (19.5, 28.5, -0.2)]
        .iter()
        .enumerate()
    {
        blob(
            &mut icon,
            (x, y),
            (2.8, 2.0),
            turn,
            &sd,
            0.0,
            true,
            hash2(salt, i as u32),
        );
    }
    icon.finish()
}

/// A bulb: round at the bottom, drawn up to a neck with a green shoot,
/// roots below.
fn bulb(rgb: u32, gloss: bool, stripes: bool, name: &str) -> Tex {
    let r = icon_ramp(rgb, 0.3, 0.95);
    let root = icon_ramp(0xd8c49a, 0.4, 0.95);
    let shoot = icon_ramp(0x5aa040, 0.3, 0.9);
    let salt = seed(name);
    let mut icon = Icon::new();
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        for end in [(11.0f32, 30.0f32), (14.5, 30.5), (18.0, 30.5), (21.5, 29.5)] {
            if seg_dist((16.0, 26.0), end, p) < 0.6 {
                return Some(root.pick(0.5, x, y));
            }
        }
        None
    });
    // The neck, behind the body.
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        if !(6.0..16.0).contains(&p.1) {
            return None;
        }
        let half = 1.2 + (p.1 - 6.0) * 0.45;
        ((p.0 - 16.0).abs() < half).then(|| r.pick(0.55 - (p.0 - 16.0) / half * 0.3, x, y))
    });
    blob(&mut icon, (16.0, 20.0), (9.0, 7.8), 0.0, &r, 0.03, gloss, salt);
    icon.layer(false, |x, y| {
        let p = (c(x), c(y));
        let (u, v) = ((p.0 - 16.0) / 9.0, (p.1 - 20.0) / 7.8);
        if u.hypot(v) > 0.92 {
            return None;
        }
        // Skin lines curving round it, or the exotic's stripes.
        let line = (u * (1.0 - v * v * 0.3) * 4.0).rem_euclid(1.0) < 0.13;
        if stripes && (u * 3.0).rem_euclid(1.0) < 0.35 {
            return Some(r.at(1));
        }
        (line && !stripes).then(|| r.at(2))
    });
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        (seg_dist((16.2, 7.0), (19.0, 2.5), p) < 1.0).then(|| shoot.pick(0.6, x, y))
    });
    icon.finish()
}

/// A sheaf of five stalks bound at the waist, their heads fanned at the top.
fn sheaf(stalk_rgb: u32, head_rgb: u32, head: Head, name: &str) -> Tex {
    let st = icon_ramp(stalk_rgb, 0.35, 0.92);
    let hd = grain_ramp(head_rgb);
    let cord = cord_ramp();
    let salt = seed(name);
    let waist = (16.0f32, 19.0f32);
    let tops: Vec<P> = (0..5)
        .map(|i| {
            let f = i as f32 - 2.0;
            (
                16.0 + f * 4.6,
                10.0 + f.abs() * 1.6 + if head == Head::Ear { -1.0 } else { 0.0 },
            )
        })
        .collect();
    let mut icon = Icon::new();
    for (i, &top) in tops.iter().enumerate() {
        let f = i as f32 - 2.0;
        let foot = (16.0 - f * 2.2, 29.5);
        icon.layer(true, |x, y| {
            let p = (c(x), c(y));
            let d = seg_dist(foot, waist, p).min(seg_dist(waist, top, p));
            (d < 0.85).then(|| st.pick(0.55 - f * 0.06 + unit(hash2(salt, (grid(y) / 3) as u32)) * 0.1, x, y))
        });
    }
    for (i, &top) in tops.iter().enumerate() {
        let dir = {
            let (dx, dy) = (top.0 - waist.0, top.1 - waist.1);
            let l = dx.hypot(dy);
            (dx / l, dy / l)
        };
        match head {
            Head::Ear => {
                let tip = (top.0 + dir.0 * 6.5, top.1 + dir.1 * 6.5);
                icon.layer(false, |x, y| {
                    let p = (c(x), c(y));
                    (seg_dist(tip, (tip.0 + dir.0 * 3.0 + 0.8, tip.1 + dir.1 * 3.0), p) < 0.5).then(|| hd.at(5))
                });
                icon.layer(true, |x, y| {
                    let p = (c(x), c(y));
                    let d = seg_dist(top, tip, p);
                    if d > 1.9 {
                        return None;
                    }
                    let along = ((p.0 - top.0) * dir.0 + (p.1 - top.1) * dir.1) / 1.5;
                    let t = 0.55 - (p.0 - top.0 - dir.0 * 3.0) * 0.08
                        + if (along.floor() as i32) % 2 == 0 { 0.12 } else { -0.06 };
                    Some(hd.pick(t, x, y))
                });
            }
            Head::Droop => {
                let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                for (k, off) in [(0.0f32, -1.5f32), (side * 2.6, 1.5), (-side * 2.0, 3.0)]
                    .iter()
                    .enumerate()
                {
                    let centre = (top.0 + off.0, top.1 + off.1);
                    blob(
                        &mut icon,
                        centre,
                        (1.6, 2.3),
                        0.0,
                        &hd,
                        0.0,
                        false,
                        hash2(salt, (i * 4 + k) as u32),
                    );
                }
            }
            Head::Panicle => {
                let side = if top.0 < 16.0 { -1.0 } else { 1.0 };
                for k in 0..4 {
                    let kf = k as f32;
                    let centre = (top.0 + side * kf * 1.3, top.1 - 1.5 + kf * kf * 0.55);
                    blob(
                        &mut icon,
                        centre,
                        (1.5, 1.3),
                        0.0,
                        &hd,
                        0.0,
                        false,
                        hash2(salt, (i * 4 + k) as u32),
                    );
                }
            }
        }
    }
    icon.layer(true, |x, y| {
        let p = (c(x), c(y));
        ((p.0 - waist.0).abs() < 4.8 && (p.1 - waist.1).abs() < 1.5).then(|| cord.pick(0.6 - (p.0 - 16.0) * 0.04, x, y))
    });
    icon.finish()
}

/// Paints a farming icon (seeds, bulbs, grain), or `None` if `name` is not
/// one.
pub fn paint_icon(name: &str) -> Option<Tex> {
    Some(match name {
        "dewberry_seed" => pouch(0xc04050, 0x8a3040, name),
        "oat_seed" => pouch(0xd8c890, 0xe0cc88, name),
        "wheat_seed" => pouch(0xd0a040, 0xc89040, name),
        "rice_seed" => pouch(0x8ab060, 0xf0e8c8, name),
        "tuber_bulb" => bulb(0xa87444, false, false, name),
        "waxy_tuber_bulb" => bulb(0xecc858, true, false, name),
        "exotic_tuber_bulb" => bulb(0xa84478, false, true, name),
        "oat_grain" => sheaf(0xc8b070, 0xe8d490, Head::Droop, name),
        "wheat_grain" => sheaf(0xc89a40, 0xd89a2c, Head::Ear, name),
        "rice_grain" => sheaf(0x9ab048, 0xe8e0b0, Head::Panicle, name),
        _ => return None,
    })
}

/// Every farming icon this module paints.
pub const ICONS: [&str; 10] = [
    "dewberry_seed",
    "oat_seed",
    "wheat_seed",
    "rice_seed",
    "tuber_bulb",
    "waxy_tuber_bulb",
    "exotic_tuber_bulb",
    "oat_grain",
    "wheat_grain",
    "rice_grain",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tex::SIZE;

    fn top_row(t: &Tex) -> usize {
        (0..SIZE).find(|&y| (0..SIZE).any(|x| t.get(x, y).a > 0.0)).unwrap()
    }

    fn filled(t: &Tex) -> usize {
        t.px.iter().filter(|p| p.a > 0.0).count()
    }

    #[test]
    fn every_crop_stage_grows_from_the_last_and_stands_on_the_ground() {
        for crop in CROPS {
            let mut last: Option<Tex> = None;
            for stage in 1..=4 {
                let name = format!("{crop}_{stage}");
                let t = paint_crop(&name).unwrap_or_else(|| panic!("{name} not painted"));
                assert!(
                    t.px.iter().all(|p| p.a == 0.0 || p.a == 1.0),
                    "{name} has partial alpha"
                );
                assert!(filled(&t) > 12, "{name} is empty");
                let foot: Vec<usize> = (0..SIZE).filter(|&x| t.get(x, SIZE - 1).a > 0.0).collect();
                assert!(foot.iter().any(|&x| (11..21).contains(&x)), "{name} foot {foot:?}");
                assert_eq!(t.get(0, 0).a, 0.0, "{name}");
                assert_eq!(t.get(31, 0).a, 0.0, "{name}");
                if let Some(prev) = &last {
                    assert!(
                        top_row(&t) < top_row(prev),
                        "{name} is not taller than stage {}",
                        stage - 1
                    );
                    assert!(
                        filled(&t) > filled(prev),
                        "{name} is not fuller than stage {}",
                        stage - 1
                    );
                }
                last = Some(t);
            }
        }
    }

    #[test]
    fn every_farming_icon_paints_something() {
        for name in ICONS {
            let t = paint_icon(name).unwrap();
            assert!(t.mean().a > 0.15, "{name} is mostly empty");
        }
        let t = crate::materials::paint_named("tilled_soil", 0).unwrap();
        assert!(t.px.iter().all(|p| p.a >= 1.0), "tilled soil is opaque");
    }

    #[test]
    fn mix_hex_ends() {
        assert_eq!(mix_hex(0x102030, 0xf0e0d0, 0.0), 0x102030);
        assert_eq!(mix_hex(0x102030, 0xf0e0d0, 1.0), 0xf0e0d0);
    }
}
