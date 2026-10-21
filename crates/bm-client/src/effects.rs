//! Things that flash and fly (milestone 13 part 2): blasts (`ServerMsg::Blast`),
//! drifters' beams (`ServerMsg::Beam`), the camera's shake near a blast, shots
//! drawn by their kind, and the bag of bombs in hand. Drawn as boxes, like
//! creatures; nothing here changes the world (the server rules every blast).
//!
//! Every look here is Boundmore's own: a blast is a ball of hot boxes that
//! swells, cools to soot and rises as smoke, with sparks thrown out; a beam is
//! a pale cyan shaft from the drifter's lantern.

use crate::combat::Drawn;
use crate::render::BoxInstance;
use bm_core::PlanetShape;
use bm_protocol::ShotKind;
use glam::{DVec3, Quat, Vec3};
use std::f32::consts::TAU;

/// A blast is drawn this long (seconds).
pub const BLAST_SECONDS: f64 = 1.2;
/// Its light flashes this long.
pub const FLASH_SECONDS: f64 = 0.45;
/// The camera shakes within this many blast radii.
pub const SHAKE_RADII: f64 = 2.0;
/// The shake lasts this long, and moves the eye at most this far (blocks).
pub const SHAKE_SECONDS: f64 = 0.6;
pub const SHAKE_BLOCKS: f64 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    /// Canonical.
    pub pos: DVec3,
    pub radius: f32,
    /// Seconds.
    pub born: f64,
    /// Varies the boxes from blast to blast.
    pub seed: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    /// The creature it shines from.
    pub from: u32,
    /// Canonical.
    pub to: DVec3,
    pub born: f64,
    pub until: f64,
}

/// Blasts and beams being drawn.
#[derive(Clone, Debug, Default)]
pub struct Effects {
    pub blasts: Vec<Blast>,
    pub beams: Vec<Beam>,
    count: u32,
}

impl Effects {
    /// `ServerMsg::Blast` at `now` (seconds).
    pub fn blast(&mut self, pos: DVec3, radius: f32, now: f64) {
        self.count = self.count.wrapping_add(1);
        self.blasts.push(Blast {
            pos,
            radius: radius.clamp(0.5, 16.0),
            born: now,
            seed: self.count.wrapping_mul(2_654_435_761),
        });
    }

    /// `ServerMsg::Beam`: from creature `from` to `to`, `ticks` long. A new
    /// beam from the same creature replaces its last.
    pub fn beam(&mut self, from: u32, to: DVec3, ticks: u16, now: f64) {
        self.beams.retain(|b| b.from != from);
        self.beams.push(Beam {
            from,
            to,
            born: now,
            until: now + ticks as f64 / bm_core::TICK_HZ as f64,
        });
    }

    /// Drops the finished ones.
    pub fn update(&mut self, now: f64) {
        self.blasts.retain(|b| now - b.born < BLAST_SECONDS);
        self.beams.retain(|b| now < b.until);
    }

    pub fn clear(&mut self) {
        self.blasts.clear();
        self.beams.clear();
    }

    /// How far to move the camera now: blasts within `SHAKE_RADII` radii of
    /// `eye` shake it, harder the nearer and the newer.
    pub fn shake(&self, eye: DVec3, shape: PlanetShape, now: f64) -> DVec3 {
        let mut strength = 0.0f64;
        for b in &self.blasts {
            let age = now - b.born;
            if !(0.0..SHAKE_SECONDS).contains(&age) {
                continue;
            }
            let reach = b.radius as f64 * SHAKE_RADII;
            let near = 1.0 - shape.delta(eye, b.pos).length() / reach;
            if near > 0.0 {
                let fade = 1.0 - age / SHAKE_SECONDS;
                strength = strength.max(near * fade * fade);
            }
        }
        if strength <= 0.0 {
            return DVec3::ZERO;
        }
        let t = now * 40.0;
        DVec3::new(t.sin(), (t * 1.37 + 1.0).sin(), (t * 0.83 + 2.0).sin()) * SHAKE_BLOCKS * strength
    }

    /// Lights the blasts cast (`Renderer::set_point_lights`): a bright flash
    /// fading over `FLASH_SECONDS`.
    pub fn lights(&self, now: f64) -> Vec<(DVec3, u8)> {
        self.blasts
            .iter()
            .filter_map(|b| {
                let t = (now - b.born) / FLASH_SECONDS;
                (0.0..1.0)
                    .contains(&t)
                    .then(|| (b.pos + DVec3::new(0.0, 0.5, 0.0), (15.0 * (1.0 - t)).round() as u8))
                    .filter(|l| l.1 > 0)
            })
            .collect()
    }

    /// Every blast's and beam's boxes, camera-relative to `eye`. `source`
    /// says where a creature's beam starts (canonical), if it is drawn.
    pub fn boxes(
        &self,
        eye: DVec3,
        shape: PlanetShape,
        now: f64,
        source: impl Fn(u32) -> Option<DVec3>,
    ) -> Vec<BoxInstance> {
        let mut out = Vec::new();
        for b in &self.blasts {
            let rel = (shape.nearest_lap(eye, b.pos) - eye).as_vec3();
            blast_boxes(&mut out, rel, b.radius, ((now - b.born) / BLAST_SECONDS) as f32, b.seed);
        }
        for beam in &self.beams {
            let Some(from) = source(beam.from) else { continue };
            let from = shape.nearest_lap(eye, from);
            let to = shape.nearest_lap(from, beam.to);
            beam_boxes(
                &mut out,
                (from - eye).as_vec3(),
                (to - eye).as_vec3(),
                (now - beam.born) as f32,
                (beam.until - now) as f32,
            );
        }
        out
    }
}

fn srgb(rgb: u32) -> Vec3 {
    Vec3::from(crate::colour::srgb_to_linear([
        (rgb >> 16) as u8,
        (rgb >> 8) as u8,
        rgb as u8,
    ]))
}

/// A number in 0..1 from `seed` and `i` (a hash: the same every frame).
fn hash(seed: u32, i: u32) -> f32 {
    let mut x = seed ^ i.wrapping_mul(0x9e37_79b9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// A unit direction from two numbers in 0..1, leaning upward by `up`.
fn direction(a: f32, b: f32, up: f32) -> Vec3 {
    let yaw = a * TAU;
    let y = (b * 2.0 - 1.0 + up).clamp(-1.0, 1.0);
    let r = (1.0 - y * y).max(0.0).sqrt();
    Vec3::new(yaw.cos() * r, y, yaw.sin() * r)
}

fn glowing(center: Vec3, half: Vec3, rot: Quat, colour: Vec3, glow: f32) -> BoxInstance {
    BoxInstance {
        center: center.to_array(),
        half_size: half.max(Vec3::splat(0.0)).to_array(),
        color: [colour.x, colour.y, colour.z, 1.0],
        rotation: rot.normalize().to_array(),
        light: [1.0, 1.0, 1.0, glow],
    }
}

/// Boxes of a blast at `rel` (camera-relative) of `radius`, `t` of the way
/// through its life (0 to 1).
pub fn blast_boxes(out: &mut Vec<BoxInstance>, rel: Vec3, radius: f32, t: f32, seed: u32) {
    if !(0.0..1.0).contains(&t) {
        return;
    }
    let white = srgb(0xfff6d8);
    let yellow = srgb(0xffd34a);
    let orange = srgb(0xff7a1e);
    let red = srgb(0xc8301a);
    let soot = srgb(0x3a3432);
    let smoke = srgb(0x77706a);
    // The fireball: swells fast to the blast's radius, then shrinks as it
    // cools from white through orange to soot.
    let swell = 1.0 - (1.0 - (t / 0.25).min(1.0)).powi(3);
    let cool = ((t - 0.12) / 0.5).clamp(0.0, 1.0);
    let size = radius * (0.25 + 0.75 * swell) * (1.0 - cool * 0.85);
    let hot = if cool < 0.3 {
        white.lerp(yellow, cool / 0.3)
    } else if cool < 0.7 {
        yellow.lerp(orange, (cool - 0.3) / 0.4)
    } else {
        orange.lerp(red, (cool - 0.7) / 0.3)
    };
    let glow = 1.0 - cool * 0.6;
    if size > 0.02 {
        // A rough ball: three crossed slabs and a turned core.
        out.push(glowing(
            rel,
            Vec3::new(size, size * 0.62, size * 0.62),
            Quat::IDENTITY,
            hot,
            glow,
        ));
        out.push(glowing(
            rel,
            Vec3::new(size * 0.62, size, size * 0.62),
            Quat::IDENTITY,
            hot,
            glow,
        ));
        out.push(glowing(
            rel,
            Vec3::new(size * 0.62, size * 0.62, size),
            Quat::IDENTITY,
            hot,
            glow,
        ));
        let core = Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.6);
        out.push(glowing(rel, Vec3::splat(size * 0.7), core, white.lerp(hot, 0.5), 1.0));
    }
    // Lumps of flame around the ball, flung out a little.
    for i in 0..12 {
        let d = direction(hash(seed, i), hash(seed, i + 100), 0.2);
        let reach = radius * (0.45 + 0.4 * hash(seed, i + 200)) * (0.4 + 0.6 * swell);
        let s = radius * (0.18 + 0.14 * hash(seed, i + 300)) * (1.0 - cool * 0.9);
        if s <= 0.01 {
            continue;
        }
        let rot = Quat::from_rotation_y(hash(seed, i + 400) * TAU) * Quat::from_rotation_x(hash(seed, i + 500) * TAU);
        let c = if i % 3 == 0 {
            yellow.lerp(hot, cool)
        } else {
            orange.lerp(red, cool)
        };
        out.push(glowing(rel + d * reach, Vec3::splat(s), rot, c, glow));
    }
    // Sparks thrown out, falling as they fly.
    for i in 0..18 {
        let d = direction(hash(seed, i + 600), hash(seed, i + 700), 0.5);
        let speed = radius * (2.2 + 2.0 * hash(seed, i + 800));
        let age = t * BLAST_SECONDS as f32;
        let p = d * speed * age - Vec3::Y * 6.0 * age * age;
        let s = 0.06 * (1.0 - t);
        if s > 0.005 {
            out.push(glowing(
                rel + p,
                Vec3::splat(s),
                Quat::IDENTITY,
                yellow.lerp(orange, t),
                1.0,
            ));
        }
    }
    // Smoke: soot puffs that rise and spread once the fire cools.
    if t > 0.15 {
        let k = (t - 0.15) / 0.85;
        for i in 0..7 {
            let d = direction(hash(seed, i + 900), hash(seed, i + 950), 0.6);
            let p = d * radius * (0.3 + 0.5 * k) + Vec3::Y * radius * 0.9 * k;
            let s = radius * (0.22 + 0.2 * hash(seed, i + 990)) * (0.6 + 0.6 * k) * (1.0 - k * k);
            let rot = Quat::from_rotation_y(hash(seed, i + 1000) * TAU + k);
            if s > 0.01 {
                out.push(BoxInstance {
                    light: [1.0, 0.0, 1.0, 0.0],
                    ..glowing(rel + p, Vec3::splat(s), rot, soot.lerp(smoke, k), 0.0)
                });
            }
        }
    }
}

/// Boxes of a beam from `from` to `to` (camera-relative): a pale shaft with
/// a bright core, flickering, `age` seconds old with `left` to go.
pub fn beam_boxes(out: &mut Vec<BoxInstance>, from: Vec3, to: Vec3, age: f32, left: f32) {
    let d = to - from;
    let len = d.length();
    if len < 0.01 {
        return;
    }
    let dir = d / len;
    let rot = Quat::from_rotation_arc(Vec3::Z, dir);
    // Grows in and narrows out.
    let grow = (age / 0.12).clamp(0.0, 1.0) * (left / 0.15).clamp(0.0, 1.0);
    let flicker = 0.85 + 0.15 * (age * 47.0).sin();
    let w = 0.11 * grow * flicker;
    let mid = from + d * 0.5;
    out.push(glowing(mid, Vec3::new(w, w, len * 0.5), rot, srgb(0x7fe6ff), 0.9));
    out.push(glowing(
        mid,
        Vec3::new(w * 0.45, w * 0.45, len * 0.5 + 0.02),
        rot,
        srgb(0xf2feff),
        1.0,
    ));
    // Rings sliding down the shaft, and a splash where it lands.
    for i in 0..4 {
        let f = ((age * 2.5 + i as f32 * 0.25) % 1.0).clamp(0.0, 1.0);
        let r = w * 1.9;
        out.push(glowing(
            from + d * f,
            Vec3::new(r, r, 0.025),
            rot * Quat::from_rotation_z(age * 3.0 + i as f32),
            srgb(0xb8f4ff),
            1.0,
        ));
    }
    let splash = 0.22 * grow * (0.8 + 0.2 * (age * 31.0).sin());
    out.push(glowing(
        to,
        Vec3::splat(splash),
        Quat::from_rotation_y(age * 4.0) * Quat::from_rotation_x(0.6),
        srgb(0xd8fbff),
        1.0,
    ));
}

/// Colours of shots.
/// Pellets (linear): ours a pale gold, everyone else's ember.
const PELLET_OURS: Vec3 = Vec3::new(1.0, 0.78, 0.36);
const PELLET_THEIRS: Vec3 = Vec3::new(1.0, 0.32, 0.08);
const BOLT: u32 = 0x8ff0ff;
const BOLT_CORE: u32 = 0xf4ffff;
const HOMING: u32 = 0xd85cff;
const HOMING_CORE: u32 = 0xffd8ff;
const BOMB_SHELL: u32 = 0x2b2a2e;
const BOMB_BAND: u32 = 0xb87333;
const FUSE_CORD: u32 = 0x8a6a44;
const SPARK: u32 = 0xffd36a;

/// A shot's boxes at `rel` (camera-relative), drawn by its kind; `time`
/// (seconds) flickers sparks and pulses.
pub fn shot_boxes(out: &mut Vec<BoxInstance>, rel: Vec3, s: &Drawn, time: f32, light: [f32; 4]) {
    let dir = s.dir.as_vec3();
    match s.kind {
        ShotKind::Pellet => {
            let colour = if s.mine { PELLET_OURS } else { PELLET_THEIRS };
            for i in 0..4 {
                let fade = 1.0 - i as f32 * 0.24;
                let at = rel - dir * (i as f32 * 0.16);
                out.push(glowing(
                    at,
                    Vec3::splat(0.065 * fade),
                    Quat::IDENTITY,
                    colour * fade,
                    1.0,
                ));
            }
        }
        ShotKind::Bolt => {
            // A long glowing dart with a white-hot core.
            let rot = Quat::from_rotation_arc(Vec3::Z, if dir == Vec3::ZERO { Vec3::Z } else { dir });
            out.push(glowing(
                rel - dir * 0.2,
                Vec3::new(0.07, 0.07, 0.32),
                rot,
                srgb(BOLT),
                1.0,
            ));
            out.push(glowing(
                rel - dir * 0.15,
                Vec3::new(0.035, 0.035, 0.36),
                rot,
                srgb(BOLT_CORE),
                1.0,
            ));
            for i in 1..4 {
                let fade = 1.0 - i as f32 * 0.25;
                out.push(glowing(
                    rel - dir * (0.45 + i as f32 * 0.2),
                    Vec3::splat(0.05 * fade),
                    rot,
                    srgb(BOLT) * fade,
                    1.0,
                ));
            }
        }
        ShotKind::Homing => {
            // A pulsing orb that trails a fading tail of motes.
            let pulse = 1.0 + 0.25 * (time * 14.0).sin();
            let spin = Quat::from_rotation_y(time * 5.0) * Quat::from_rotation_x(time * 3.0);
            out.push(glowing(rel, Vec3::splat(0.16 * pulse), spin, srgb(HOMING), 1.0));
            out.push(glowing(
                rel,
                Vec3::splat(0.09 * pulse),
                spin.inverse(),
                srgb(HOMING_CORE),
                1.0,
            ));
            for i in 1..7 {
                let fade = 1.0 - i as f32 * 0.14;
                let wobble = Vec3::new((time * 9.0 + i as f32).sin(), (time * 7.0 + i as f32 * 2.0).cos(), 0.0) * 0.04;
                out.push(glowing(
                    rel - dir * (i as f32 * 0.17) + wobble,
                    Vec3::splat(0.07 * fade),
                    spin,
                    srgb(HOMING) * fade,
                    1.0,
                ));
            }
        }
        ShotKind::Bomb => bomb_boxes(out, rel, s.burned, time, light, Quat::from_rotation_y(time * 3.0)),
    }
}

/// A round bomb at `rel` with a sparking fuse; `burned` (0 to 1) shortens the
/// fuse and quickens the sparks.
pub fn bomb_boxes(out: &mut Vec<BoxInstance>, rel: Vec3, burned: Option<f32>, time: f32, light: [f32; 4], turn: Quat) {
    let lit = |c: Vec3, glow: f32| [c.x, c.y, c.z, glow];
    let shell = srgb(BOMB_SHELL);
    let part = |at: Vec3, half: Vec3, colour: Vec3| BoxInstance {
        center: (rel + turn * at).to_array(),
        half_size: half.to_array(),
        color: [colour.x, colour.y, colour.z, 1.0],
        rotation: turn.to_array(),
        light,
    };
    let r = 0.14;
    out.push(part(Vec3::ZERO, Vec3::new(r, r * 0.8, r * 0.8), shell));
    out.push(part(Vec3::ZERO, Vec3::new(r * 0.8, r, r * 0.8), shell));
    out.push(part(Vec3::ZERO, Vec3::new(r * 0.8, r * 0.8, r), shell));
    // A copper band and the neck.
    out.push(part(Vec3::ZERO, Vec3::new(r * 1.02, 0.025, r * 0.84), srgb(BOMB_BAND)));
    out.push(part(
        Vec3::new(0.0, r + 0.02, 0.0),
        Vec3::new(0.05, 0.03, 0.05),
        srgb(BOMB_BAND),
    ));
    // The fuse cord, burning down.
    let left = 1.0 - burned.unwrap_or(0.3);
    let cord = 0.04 + 0.08 * left;
    out.push(part(
        Vec3::new(0.0, r + 0.05 + cord * 0.5, 0.0),
        Vec3::new(0.015, cord * 0.5, 0.015),
        srgb(FUSE_CORD),
    ));
    let tip = Vec3::new(0.0, r + 0.06 + cord, 0.0);
    let rate = 18.0 + 30.0 * burned.unwrap_or(0.3);
    let flick = 0.75 + 0.25 * (time * rate).sin();
    let mut spark = part(tip, Vec3::splat(0.035 * flick), srgb(SPARK));
    spark.light = lit(srgb(SPARK), 1.0);
    out.push(spark);
    for i in 0..3u32 {
        let a = time * rate * 0.7 + i as f32 * 2.1;
        let off = Vec3::new(
            a.cos() * 0.06,
            0.03 + ((time * 13.0 + i as f32) % 1.0) * 0.08,
            a.sin() * 0.06,
        );
        let mut s = part(tip + off, Vec3::splat(0.012), srgb(0xffa040));
        s.light = lit(srgb(0xffa040), 1.0);
        out.push(s);
    }
}

/// The bag of bombs in hand: a tied pouch of the bag's metal colour, and
/// while a bomb is lit, the bomb held up above it with its fuse burning.
/// `turn` is the camera's, `hand` where the hand is (camera-relative).
pub fn held_bombs(
    name: &str,
    burned: Option<f32>,
    time: f32,
    turn: Quat,
    hand: Vec3,
    light: [f32; 4],
) -> Vec<BoxInstance> {
    let metal = if name.contains("iron") {
        srgb(0x9aa0aa)
    } else {
        srgb(0xc9773f)
    };
    let cloth = srgb(0x7a5a3a);
    let cloth_dark = srgb(0x5a4028);
    let turn = turn * Quat::from_rotation_x(0.15) * Quat::from_rotation_z(-0.2);
    let k = 1.0;
    let part = |at: Vec3, half: Vec3, rot: Quat, colour: Vec3| BoxInstance {
        center: (hand + turn * (at * k)).to_array(),
        half_size: (half * k).to_array(),
        color: [colour.x, colour.y, colour.z, 1.0],
        rotation: (turn * rot).normalize().to_array(),
        light,
    };
    let mut out = vec![
        // The pouch: a round sack, tied at the neck with a metal clasp.
        part(
            Vec3::new(0.0, -0.06, 0.0),
            Vec3::new(0.085, 0.07, 0.085),
            Quat::IDENTITY,
            cloth,
        ),
        part(
            Vec3::new(0.0, -0.06, 0.0),
            Vec3::new(0.07, 0.085, 0.07),
            Quat::IDENTITY,
            cloth,
        ),
        part(
            Vec3::new(0.0, 0.03, 0.0),
            Vec3::new(0.045, 0.025, 0.045),
            Quat::IDENTITY,
            cloth_dark,
        ),
        part(
            Vec3::new(0.0, 0.035, 0.0),
            Vec3::new(0.05, 0.012, 0.05),
            Quat::IDENTITY,
            metal,
        ),
        part(
            Vec3::new(0.0, 0.07, 0.0),
            Vec3::new(0.04, 0.025, 0.035),
            Quat::from_rotation_z(0.3),
            cloth_dark,
        ),
        // A metal plate stitched to the front.
        part(
            Vec3::new(0.0, -0.06, -0.087),
            Vec3::new(0.04, 0.035, 0.006),
            Quat::IDENTITY,
            metal,
        ),
    ];
    if let Some(b) = burned {
        // The lit bomb, held up and trembling more as the fuse runs down.
        let shake = Vec3::new((time * 41.0).sin(), (time * 37.0).cos(), 0.0) * 0.006 * (1.0 + b * 3.0);
        let at = hand + turn * (Vec3::new(-0.08, 0.2, -0.05) + shake);
        bomb_boxes(&mut out, at, Some(b), time, light, turn);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blasts_and_beams_last_their_time() {
        let shape = PlanetShape::new(8, 4);
        let mut e = Effects::default();
        e.blast(DVec3::new(10.0, 44.0, 10.0), 3.0, 5.0);
        e.beam(7, DVec3::new(12.0, 44.0, 10.0), 30, 5.0);
        let source = |id| (id == 7).then_some(DVec3::new(12.0, 48.0, 10.0));
        let eye = DVec3::new(10.0, 45.0, 14.0);
        assert!(
            e.boxes(eye, shape, 5.1, source).len() > 20,
            "a ball, flames, sparks and a beam"
        );
        // Half a second of beam (30 ticks): gone after it, the blast later.
        e.update(5.45);
        assert_eq!((e.blasts.len(), e.beams.len()), (1, 1));
        e.update(5.51);
        assert_eq!((e.blasts.len(), e.beams.len()), (1, 0));
        e.update(5.0 + BLAST_SECONDS + 0.01);
        assert!(e.blasts.is_empty());
        // A beam from a creature not drawn is not drawn either.
        e.beam(9, DVec3::ZERO, 60, 6.0);
        assert!(e.boxes(eye, shape, 6.1, source).is_empty());
        // A new beam from the same creature replaces the last.
        e.beam(9, DVec3::ONE, 60, 6.2);
        assert_eq!(e.beams.len(), 1);
    }

    #[test]
    fn blasts_flash_and_shake_only_the_near() {
        let shape = PlanetShape::new(8, 4);
        let mut e = Effects::default();
        let at = DVec3::new(10.0, 44.0, 10.0);
        e.blast(at, 3.0, 0.0);
        let near = e.shake(at + DVec3::new(2.0, 1.0, 0.0), shape, 0.05);
        assert!(near.length() > 0.0 && near.length() <= SHAKE_BLOCKS * 1.8);
        assert_eq!(
            e.shake(at + DVec3::new(7.0, 0.0, 0.0), shape, 0.05),
            DVec3::ZERO,
            "beyond two radii"
        );
        assert_eq!(e.shake(at, shape, SHAKE_SECONDS + 0.1), DVec3::ZERO, "over");
        // Across the seam counts as near.
        let w = shape.width_blocks() as f64;
        let mut e2 = Effects::default();
        e2.blast(DVec3::new(0.5, 44.0, 10.0), 3.0, 0.0);
        assert!(e2.shake(DVec3::new(w - 1.0, 44.0, 10.0), shape, 0.05).length() > 0.0);
        // The flash is bright at once and fades out.
        assert_eq!(e.lights(0.0)[0].1, 15);
        assert!(e.lights(FLASH_SECONDS * 0.8)[0].1 < 5);
        assert!(e.lights(FLASH_SECONDS + 0.01).is_empty());
    }

    #[test]
    fn a_blast_swells_then_cools() {
        let size = |t: f32| {
            let mut out = Vec::new();
            blast_boxes(&mut out, Vec3::ZERO, 3.0, t, 1);
            out[0].half_size[0]
        };
        assert!(size(0.2) > size(0.0), "swells");
        assert!(size(0.7) < size(0.2), "cools and shrinks");
        let mut out = Vec::new();
        blast_boxes(&mut out, Vec3::ZERO, 3.0, 1.0, 1);
        assert!(out.is_empty(), "over at the end of its life");
    }

    #[test]
    fn the_bag_shows_a_lit_bomb_only_while_lit() {
        let unlit = held_bombs(
            "copper_bombs",
            None,
            0.0,
            Quat::IDENTITY,
            Vec3::ZERO,
            [1.0, 0.0, 1.0, 0.0],
        );
        let lit = held_bombs(
            "copper_bombs",
            Some(0.5),
            0.0,
            Quat::IDENTITY,
            Vec3::ZERO,
            [1.0, 0.0, 1.0, 0.0],
        );
        assert!(lit.len() > unlit.len() + 5);
        assert!(lit.iter().any(|b| b.light[3] >= 1.0), "the fuse glows");
        assert!(unlit.iter().all(|b| b.light[3] == 0.0));
    }
}
