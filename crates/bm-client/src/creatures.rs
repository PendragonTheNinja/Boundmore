//! Creatures on the client (milestone 13): what the server says about each
//! one (`ServerMsg::Creature`, `CreatureGone`), where it was in each snapshot,
//! and how it is drawn.
//!
//! Creatures are interpolated like remote players (`predict::Remotes`): drawn
//! `INTERP_DELAY_TICKS` behind the server's newest snapshot on the same clock,
//! moved across the seam with `nearest_lap`, turned the short way with
//! `lerp_angle`. One tick's creatures may come in several snapshot parts; each
//! creature is in one of them, and a later part of the same tick never steps
//! a creature back.
//!
//! The models are Boundmore's own chunky box creatures (`docs/art-direction.md`),
//! built in code, never Boundless's shapes:
//! - the **grazer**, a shaggy herd beast with a dark mane, a cream muzzle,
//!   curled horns and a patch of the planet's moss on its rump;
//! - the **pelter**, a low lizard on splayed legs with ember spines, a jaw that
//!   opens and a banded rattle tail that shakes as it takes aim;
//! - the **dasher**, a tall bird on long amber legs with a slender neck, tail
//!   plumes and a three-feather crest;
//! - the **popper** (part 2), a round, bouncy lump of soot cracked with
//!   embers, on stubby legs, with a wick for a tuft: it hops as it walks,
//!   tucks into an armoured ball when curled, and when lit glows and pulses
//!   faster and faster with an O-mouthed whistle and a sparking wick;
//! - the **drifter** (part 2), a floating bronze-rimmed bell with a glowing
//!   lantern face and six jointed tentacles that splay on the ground at rest
//!   and sway beneath it in flight.
//!
//! Each wears grey armour plates by tier (one more a tier, up to four), and
//! its coat leans a little towards the planet's colour, as Boundless tinted
//! its creatures from the biome. Exotics wear glowing violet markings and a
//! ring of motes (`EXOTIC`).

use crate::predict::{INTERP_DELAY_TICKS, lerp_angle};
use crate::render::BoxInstance;
use bm_core::PlanetShape;
use bm_core::creatures::CreatureDef;
use bm_protocol::{CreatureView, Pose, RemoteCreature};
use glam::{DVec3, Quat, Vec3};
use std::collections::{HashMap, VecDeque};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// How far a coat leans towards the planet's colour.
const PLANET_TINT: f32 = 0.3;
/// The hit flash fades over this many seconds.
const FLASH_SECONDS: f32 = 0.25;
/// Samples kept per creature.
const SAMPLES: usize = 32;
/// A creature missing from snapshots this many ticks is not drawn (it left
/// view without a `CreatureGone`, or snapshots stopped).
const STALE_TICKS: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub tick: u64,
    pub pos: DVec3,
    pub yaw: u16,
    pub pose: Pose,
}

/// A creature as it is drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub id: u32,
    pub kind: u16,
    pub tier: u8,
    /// Feet, canonical.
    pub pos: DVec3,
    pub yaw: u16,
    pub pose: Pose,
    pub dead: bool,
    pub looted: bool,
    pub angry: bool,
    /// Health lost.
    pub damage: u32,
    /// Exotic buffs (`creatures::Exotic` bits; 0 ordinary).
    pub exotic: u8,
    /// Its element (`creatures::Element::code`; 0 none).
    pub element: u8,
}

/// Easing state for one creature's animation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Anim {
    /// Walk cycle phase, radians.
    pub stride: f32,
    /// How much the legs swing, 0 standing to 1 at full stride.
    pub gait: f32,
    /// Leaning into a run, 0 to 1.
    pub lean: f32,
    /// Rearing (Windup), 0 to 1.
    pub rear: f32,
    /// Lunging (Attack), 0 to 1.
    pub lunge: f32,
    /// Rolled over dead, 0 to 1.
    pub fall: f32,
    /// A hit's flash, 1 just hit, fading to 0.
    pub flash: f32,
    /// Seconds since it appeared, for idle motion and the rattle.
    pub time: f32,
    /// Curled into a ball (a popper), 0 to 1.
    pub curl: f32,
    /// How far a curled ball has rolled, radians.
    pub roll: f32,
    /// Lit and whistling (a popper), 0 to 1; seconds lit; the glow's pulse
    /// phase, quickening the longer it is lit.
    pub heat: f32,
    pub lit: f32,
    pub pulse: f32,
    /// Stunned (a popper), 0 to 1.
    pub stun: f32,
    /// Flying (a drifter), 0 resting to 1 aloft.
    pub fly: f32,
    last: Option<DVec3>,
}

/// A popper ball's radius, for rolling.
const BALL_RADIUS: f32 = 0.3;

impl Anim {
    /// One frame of `dt` seconds for a creature now at `pos` doing `pose`.
    pub fn step(&mut self, pos: DVec3, pose: Pose, dead: bool, stride_len: f32, dt: f32, shape: PlanetShape) {
        let speed = match self.last {
            Some(was) if dt > 0.0 => {
                let d = shape.delta(was, pos);
                (d.x.hypot(d.z) as f32 / dt).min(30.0)
            }
            _ => 0.0,
        };
        self.last = Some(pos);
        self.time += dt;
        let ease = |x: &mut f32, target: f32, rate: f32| *x += (target - *x) * (1.0 - (-dt * rate).exp());
        let moving = if dead { 0.0 } else { (speed / 2.5).min(1.0) };
        ease(&mut self.gait, moving, 10.0);
        if !dead {
            self.stride = (self.stride + speed * dt * TAU / stride_len.max(0.1)) % TAU;
        }
        ease(&mut self.lean, (pose == Pose::Run && !dead) as u8 as f32, 6.0);
        ease(&mut self.rear, (pose == Pose::Windup && !dead) as u8 as f32, 9.0);
        ease(&mut self.lunge, (pose == Pose::Attack && !dead) as u8 as f32, 12.0);
        let alive = |p: Pose| (pose == p && !dead) as u8 as f32;
        ease(&mut self.curl, alive(Pose::Curled), 10.0);
        if pose == Pose::Curled {
            self.roll = (self.roll + speed * dt / BALL_RADIUS) % TAU;
        }
        ease(&mut self.heat, alive(Pose::Lit), 6.0);
        self.lit = if alive(Pose::Lit) > 0.0 { self.lit + dt } else { 0.0 };
        // Faster and faster: 1.5 beats a second, quickening by 4 a second.
        self.pulse = (self.pulse + dt * TAU * (1.5 + 4.0 * self.lit.min(4.0))) % TAU;
        ease(&mut self.stun, alive(Pose::Stunned), 8.0);
        ease(&mut self.fly, alive(Pose::Flying), 3.0);
        if dead || pose == Pose::Dead {
            ease(&mut self.fall, 1.0, 4.0);
        } else {
            self.fall = 0.0;
        }
        self.flash = (self.flash - dt / FLASH_SECONDS).max(0.0);
    }
}

/// Every creature the server has told us of.
#[derive(Default)]
pub struct Creatures {
    kinds: Vec<CreatureDef>,
    views: HashMap<u32, CreatureView>,
    samples: HashMap<u32, VecDeque<Sample>>,
    anims: HashMap<u32, Anim>,
}

impl Creatures {
    pub fn new(kinds: Vec<CreatureDef>) -> Self {
        Self {
            kinds,
            ..Default::default()
        }
    }

    pub fn kind(&self, kind: u16) -> Option<&CreatureDef> {
        self.kinds.get(kind as usize)
    }

    pub fn get(&self, id: u32) -> Option<&CreatureView> {
        self.views.get(&id)
    }

    /// `ServerMsg::Creature`: one came into view or changed. A kind we were
    /// not told of is ignored rather than trusted.
    pub fn view(&mut self, v: CreatureView) {
        if (v.kind as usize) < self.kinds.len() {
            self.views.insert(v.id, v);
        }
    }

    /// `ServerMsg::CreatureGone`.
    pub fn gone(&mut self, id: u32) {
        self.views.remove(&id);
        self.samples.remove(&id);
        self.anims.remove(&id);
    }

    /// Forgets everything (travel to another planet).
    pub fn clear(&mut self) {
        self.views.clear();
        self.samples.clear();
        self.anims.clear();
    }

    /// A hit landed on `id`: it flashes.
    pub fn flash(&mut self, id: u32) {
        self.anims.entry(id).or_default().flash = 1.0;
    }

    /// One snapshot part's creatures at server tick `tick`. Parts of one tick
    /// each carry different creatures; a creature seen twice in a tick keeps
    /// the last word.
    pub fn record(&mut self, tick: u64, list: &[RemoteCreature]) {
        for c in list {
            let s = Sample {
                tick,
                pos: DVec3::new(c.pos[0] as f64, c.pos[1] as f64, c.pos[2] as f64),
                yaw: c.yaw,
                pose: c.pose,
            };
            let q = self.samples.entry(c.id).or_default();
            match q.back_mut() {
                Some(b) if b.tick == tick => *b = s,
                Some(b) if b.tick > tick => {}
                _ => q.push_back(s),
            }
            while q.len() > SAMPLES {
                q.pop_front();
            }
        }
    }

    /// Where every known creature is at server tick `now` (the remote
    /// players' clock, `Remotes::server_tick`), drawn `INTERP_DELAY_TICKS`
    /// behind it as remote players are. Positions are canonical.
    pub fn sample(&self, now: f64, shape: PlanetShape) -> Vec<Seen> {
        let t = now - INTERP_DELAY_TICKS;
        let mut out = Vec::new();
        for (id, q) in &self.samples {
            let (Some(v), Some(last)) = (self.views.get(id), q.back()) else {
                continue;
            };
            if (last.tick as f64) < t - STALE_TICKS {
                continue;
            }
            let s = match q.iter().position(|s| s.tick as f64 > t) {
                Some(0) => q[0],
                Some(i) => {
                    let (a, b) = (q[i - 1], q[i]);
                    let f = (t - a.tick as f64) / (b.tick - a.tick) as f64;
                    let to = shape.nearest_lap(a.pos, b.pos);
                    Sample {
                        tick: t as u64,
                        pos: shape.wrap_point(a.pos + (to - a.pos) * f),
                        yaw: lerp_angle(a.yaw, b.yaw, f),
                        pose: if f < 0.5 { a.pose } else { b.pose },
                    }
                }
                None => *last,
            };
            out.push(Seen {
                id: *id,
                kind: v.kind,
                tier: v.tier,
                pos: s.pos,
                yaw: s.yaw,
                pose: if v.dead { Pose::Dead } else { s.pose },
                dead: v.dead,
                looted: v.looted,
                angry: v.angry,
                damage: v.damage,
                exotic: v.exotic,
                element: v.element,
            });
        }
        out.sort_by_key(|s| s.id);
        out
    }

    /// Steps every drawn creature's animation by `dt` seconds.
    pub fn animate(&mut self, seen: &[Seen], dt: f32, shape: PlanetShape) {
        for s in seen {
            let stride = self.kind(s.kind).map_or(1.0, |k| model_of(k).stride_len());
            self.anims
                .entry(s.id)
                .or_default()
                .step(s.pos, s.pose, s.dead, stride, dt, shape);
        }
    }

    pub fn anim(&self, id: u32) -> Anim {
        self.anims.get(&id).copied().unwrap_or_default()
    }

    /// The nearest creature whose box the ray from `eye` along `dir` (unit)
    /// enters within `max`, and how far along: `living` only, or bodies too.
    pub fn pick(
        &self,
        seen: &[Seen],
        eye: DVec3,
        dir: DVec3,
        max: f64,
        shape: PlanetShape,
        living: bool,
    ) -> Option<(u32, f64)> {
        seen.iter()
            .filter(|s| !living || !s.dead)
            .filter_map(|s| {
                let k = self.kind(s.kind)?;
                let (lo, hi) = body_box(k, shape.nearest_lap(eye, s.pos), s.dead);
                ray_box(eye, dir, lo, hi).filter(|&d| d <= max).map(|d| (s.id, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The box a shot or a strike hits, at canonical `pos`.
    pub fn hit_box(&self, s: &Seen, pos: DVec3) -> Option<(DVec3, DVec3)> {
        self.kind(s.kind).map(|k| body_box(k, pos, s.dead))
    }

    /// Every drawn creature's boxes, camera-relative to `eye`, coloured
    /// towards `planet` (linear RGB), each lit by `light` at its middle.
    pub fn boxes(
        &self,
        seen: &[Seen],
        eye: DVec3,
        shape: PlanetShape,
        planet: [f32; 3],
        light: impl Fn(DVec3) -> [f32; 4],
    ) -> Vec<BoxInstance> {
        let mut out = Vec::new();
        for s in seen {
            let Some(k) = self.kind(s.kind) else { continue };
            let pos = shape.nearest_lap(eye, s.pos);
            let anim = self.anim(s.id);
            let mut l = light(pos + DVec3::new(0.0, k.height as f64 * 0.5, 0.0));
            if s.looted {
                // A looted body is spent: drawn dim until it goes.
                l[2] *= 0.4;
            }
            let parts = model(k, s.tier, s.exotic, &anim, planet);
            let turn = Quat::from_rotation_y(-bm_core::movement::yaw_radians(s.yaw) as f32);
            let base = (pos - eye).as_vec3();
            for p in parts {
                let flash = anim.flash;
                let colour = p.colour.lerp(Vec3::new(1.0, 0.32, 0.26), flash * 0.7);
                out.push(BoxInstance {
                    center: (base + turn * p.pos).to_array(),
                    half_size: p.half.to_array(),
                    color: [colour.x, colour.y, colour.z, 1.0],
                    rotation: (turn * p.rot).normalize().to_array(),
                    light: [l[0], l[1], l[2], (p.glow + flash * 0.5).min(1.0)],
                });
            }
        }
        out
    }
}

/// A creature's collision box at `pos` (feet): its kind's width and height;
/// a body lies on its side, as wide as it was tall.
fn body_box(k: &CreatureDef, pos: DVec3, dead: bool) -> (DVec3, DVec3) {
    let (hw, h) = (k.half_width as f64, k.height as f64);
    let (hw, h) = if dead { (hw.max(h * 0.5), hw * 2.0) } else { (hw, h) };
    (pos - DVec3::new(hw, 0.0, hw), pos + DVec3::new(hw, h, hw))
}

/// Where a ray from `from` along `dir` first enters the box, if it does
/// (0 from inside).
pub fn ray_box(from: DVec3, dir: DVec3, lo: DVec3, hi: DVec3) -> Option<f64> {
    let mut near = 0.0f64;
    let mut far = f64::INFINITY;
    for a in 0..3 {
        let (o, d, l, h) = (from[a], dir[a], lo[a], hi[a]);
        if d.abs() < 1e-12 {
            if o < l || o > h {
                return None;
            }
            continue;
        }
        let (t1, t2) = ((l - o) / d, (h - o) / d);
        near = near.max(t1.min(t2));
        far = far.min(t1.max(t2));
        if near > far {
            return None;
        }
    }
    Some(near)
}

// ---- models ----

/// One box of a model, in the creature's own space: feet at the origin,
/// facing -Z, up +Y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub pos: Vec3,
    pub half: Vec3,
    pub rot: Quat,
    /// Linear RGB.
    pub colour: Vec3,
    /// 0 lit normally, up to 1 glowing (embers, eyes).
    pub glow: f32,
}

/// A place in a model to hang parts from: a joint.
#[derive(Clone, Copy, Debug)]
struct Frame {
    at: Vec3,
    rot: Quat,
}

impl Frame {
    const ROOT: Frame = Frame {
        at: Vec3::ZERO,
        rot: Quat::IDENTITY,
    };

    /// A joint at `offset` (in this frame), turned by `rot`.
    fn child(&self, offset: Vec3, rot: Quat) -> Frame {
        Frame {
            at: self.at + self.rot * offset,
            rot: self.rot * rot,
        }
    }

    /// A box centred at `offset` in this frame.
    fn part(&self, offset: Vec3, half: Vec3, colour: Vec3) -> Part {
        Part {
            pos: self.at + self.rot * offset,
            half,
            rot: self.rot,
            colour,
            glow: 0.0,
        }
    }
}

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn rx(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}

fn ry(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}

fn rz(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}

/// Colours of a model: `rgb` is an sRGB hex colour; `coat` leans towards
/// the planet's colour, `raw` does not.
struct Paint {
    planet: Vec3,
}

impl Paint {
    fn raw(&self, rgb: u32) -> Vec3 {
        let c = crate::colour::srgb_to_linear([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
        Vec3::from(c)
    }

    fn coat(&self, rgb: u32) -> Vec3 {
        let c = self.raw(rgb);
        // Keep the coat's brightness: lean the hue, not the light.
        let toward = self.planet * (luma(c) / luma(self.planet).max(0.02));
        c.lerp(toward.min(Vec3::ONE), PLANET_TINT)
    }

    /// The planet's own colour, brightened, for moss.
    fn moss(&self) -> Vec3 {
        let m = self.planet * (0.22 / luma(self.planet).max(0.02));
        m.min(Vec3::ONE).lerp(self.raw(0x6f9a3a), 0.25)
    }
}

fn luma(c: Vec3) -> f32 {
    c.dot(v(0.2126, 0.7152, 0.0722))
}

const PLATE: u32 = 0x8d939b;
/// Exotics' markings and motes: a glowing violet no coat has.
pub const EXOTIC: u32 = 0xd25cff;
const PLATE_EDGE: u32 = 0x5d636c;

/// Armour plates a creature of `tier` wears: none at tier 1, one more a tier
/// up to four.
pub fn plates(tier: u8) -> usize {
    (tier.max(1) as usize - 1).min(4)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Model {
    Grazer,
    Pelter,
    Dasher,
    Popper,
    Drifter,
    /// A kind this client has no model for: a plain box of its size.
    Plain,
}

impl Model {
    fn stride_len(self) -> f32 {
        match self {
            Model::Grazer => 1.3,
            Model::Pelter => 0.7,
            Model::Dasher => 1.1,
            Model::Popper => 0.9,
            Model::Drifter => 1.5,
            Model::Plain => 1.0,
        }
    }
}

fn model_of(k: &CreatureDef) -> Model {
    match k.name.as_str() {
        "grazer" => Model::Grazer,
        "pelter" => Model::Pelter,
        "dasher" => Model::Dasher,
        "popper" => Model::Popper,
        "drifter" => Model::Drifter,
        _ => Model::Plain,
    }
}

/// A creature's boxes for its kind, tier, exotic buffs and animation, its
/// coat tinted towards `planet` (linear RGB).
pub fn model(k: &CreatureDef, tier: u8, exotic: u8, a: &Anim, planet: [f32; 3]) -> Vec<Part> {
    let paint = Paint {
        planet: Vec3::from(planet),
    };
    let ex = exotic != 0;
    let mut out = Vec::new();
    match model_of(k) {
        Model::Grazer => grazer(&mut out, tier, ex, a, &paint),
        Model::Pelter => pelter(&mut out, tier, ex, a, &paint),
        Model::Dasher => dasher(&mut out, tier, ex, a, &paint),
        Model::Popper => popper(&mut out, tier, ex, a, &paint),
        Model::Drifter => drifter(&mut out, tier, ex, a, &paint),
        Model::Plain => {
            let hw = k.half_width;
            out.push(Frame::ROOT.part(
                v(0.0, k.height / 2.0, 0.0),
                v(hw, k.height / 2.0, hw),
                paint.coat(0x9a7a5a),
            ));
        }
    }
    if ex && a.fall < 0.5 {
        aura(&mut out, k, a, &paint);
    }
    out
}

/// A marking on an exotic: glowing violet.
fn mark(f: &Frame, offset: Vec3, half: Vec3, p: &Paint) -> Part {
    Part {
        glow: 0.7,
        ..f.part(offset, half, p.raw(EXOTIC))
    }
}

/// An exotic's aura: motes circling it slowly, rising and falling.
fn aura(out: &mut Vec<Part>, k: &CreatureDef, a: &Anim, p: &Paint) {
    let r = k.half_width * 1.25;
    for i in 0..6 {
        let f = i as f32 / 6.0;
        let ang = a.time * 0.8 + f * TAU;
        let y = k.height * (0.25 + 0.6 * (0.5 + 0.5 * (a.time * 1.1 + f * 9.0).sin()));
        let rot = ry(a.time * 2.0 + f * 3.0) * rx(0.6);
        out.push(Part {
            pos: v(ang.cos() * r, y, ang.sin() * r),
            half: Vec3::splat(0.035),
            rot,
            colour: p.raw(EXOTIC).lerp(Vec3::ONE, 0.25),
            glow: 1.0,
        });
    }
}

/// Ease in and out, 0 to 1.
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The frame a body hangs from: rolled onto its side (`belly_up` false) or
/// over onto its back as it dies, about a pivot on the ground so it ends up
/// lying there. `side` is the body's half width, `back` the height of its
/// back.
fn body_frame(a: &Anim, side: f32, back: f32, belly_up: bool) -> Frame {
    let f = smooth(a.fall);
    if f <= 0.0 {
        return Frame::ROOT;
    }
    let (pivot, angle) = if belly_up {
        (v(0.0, back * 0.5, 0.0), PI * f)
    } else {
        (v(side, 0.0, 0.0), -FRAC_PI_2 * f)
    };
    // Turning about `pivot`: at = pivot - rot * pivot.
    let rot = rz(angle);
    Frame {
        at: pivot - rot * pivot,
        rot,
    }
}

/// The grazer: a shaggy herd beast about 1.4 tall and 1.2 wide.
fn grazer(out: &mut Vec<Part>, tier: u8, exotic: bool, a: &Anim, p: &Paint) {
    let hide = p.coat(0x8a5a34);
    let mane = p.coat(0x4a2e1c);
    let shag = p.coat(0x5e3b22);
    let cream = p.raw(0xeadcbc);
    let horn = p.raw(0xe2d2a8);
    let horn_tip = p.raw(0x9c8a68);
    let hoof = p.raw(0x2f2620);
    let dark = p.raw(0x1b1512);
    let body = body_frame(a, 0.5, 1.25, false);
    // Rearing lifts the front about the back hips; a charge drops the head.
    let rear = smooth(a.rear) * 0.42 - a.lean * 0.06 + (a.time * 1.3).sin() * 0.01;
    let torso = body
        .child(v(0.0, 0.62, 0.45), rx(rear))
        .child(v(0.0, 0.0, -0.45), Quat::IDENTITY);
    out.push(torso.part(v(0.0, 0.25, 0.05), v(0.42, 0.32, 0.6), hide));
    // Belly fur, lighter.
    out.push(torso.part(v(0.0, -0.05, 0.05), v(0.36, 0.05, 0.5), p.coat(0xa47a52)));
    // The mane: a heavy shag over the shoulders, hanging in tufts.
    out.push(torso.part(v(0.0, 0.42, -0.33), v(0.48, 0.36, 0.3), mane));
    for (x, z, h) in [
        (-0.4, -0.5, 0.2),
        (0.4, -0.5, 0.2),
        (-0.42, -0.2, 0.16),
        (0.42, -0.2, 0.16),
    ] {
        out.push(torso.part(v(x, 0.0, z), v(0.08, h, 0.1), shag));
    }
    // Moss on the rump, in the planet's own colour.
    out.push(torso.part(v(0.06, 0.58, 0.32), v(0.26, 0.03, 0.22), p.moss()));
    out.push(torso.part(v(-0.1, 0.62, 0.42), v(0.1, 0.04, 0.08), p.moss() * 1.15));
    // Armour plates along the flanks.
    for i in 0..plates(tier) {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let z = -0.05 + (i / 2) as f32 * 0.32;
        out.push(torso.part(v(side * 0.44, 0.28, z), v(0.03, 0.2, 0.13), p.raw(PLATE)));
        out.push(torso.part(v(side * 0.46, 0.28, z), v(0.015, 0.16, 0.09), p.raw(PLATE_EDGE)));
    }
    // Exotic: bold stripes down the flanks.
    if exotic {
        for side in [-1.0, 1.0] {
            for z in [0.12, 0.3, 0.48] {
                out.push(mark(&torso, v(side * 0.425, 0.3, z), v(0.012, 0.24, 0.045), p));
            }
        }
    }
    // Head, from the neck: dropped low in a charge, nodding as it walks.
    let nod = (a.stride * 2.0).sin() * 0.05 * a.gait;
    let head = torso.child(v(0.0, 0.3, -0.62), rx(-smooth(a.lunge) * 0.5 - a.lean * 0.15 + nod));
    out.push(head.part(v(0.0, -0.05, -0.2), v(0.24, 0.24, 0.24), hide));
    out.push(head.part(v(0.0, 0.16, -0.12), v(0.26, 0.1, 0.18), mane));
    out.push(head.part(v(0.0, -0.16, -0.46), v(0.18, 0.13, 0.12), cream));
    out.push(head.part(v(0.0, -0.1, -0.58), v(0.13, 0.05, 0.015), dark));
    for x in [-0.245, 0.245] {
        out.push(head.part(v(x, 0.02, -0.33), v(0.012, 0.045, 0.04), dark));
    }
    // Curled horns: out, back, down and forward again.
    for s in [-1.0, 1.0] {
        let h = head.child(v(s * 0.24, 0.16, -0.12), Quat::IDENTITY);
        out.push(h.part(v(s * 0.06, 0.02, 0.0), v(0.08, 0.07, 0.08), horn));
        out.push(h.part(v(s * 0.17, 0.08, 0.08), v(0.07, 0.07, 0.08), horn));
        out.push(h.part(v(s * 0.22, -0.02, 0.17), v(0.06, 0.08, 0.06), horn));
        out.push(h.part(v(s * 0.2, -0.13, 0.1), v(0.05, 0.05, 0.06), horn));
        out.push(h.part(v(s * 0.16, -0.16, 0.0), v(0.04, 0.04, 0.05), horn_tip));
    }
    // Legs from the hips; diagonal pairs swing together. Rearing tucks the
    // front pair up.
    let swing = 0.55 * a.gait;
    for (x, z, phase, front) in [
        (-0.27, -0.42, 0.0, true),
        (0.27, -0.42, PI, true),
        (-0.27, 0.48, PI, false),
        (0.27, 0.48, 0.0, false),
    ] {
        let tuck = if front { -smooth(a.rear) * 0.9 } else { 0.0 };
        let parent = if front {
            torso
        } else {
            body.child(v(0.0, 0.62, 0.0), Quat::IDENTITY)
        };
        let leg = parent.child(v(x, 0.0, z), rx((a.stride + phase).sin() * swing + tuck));
        out.push(leg.part(v(0.0, -0.26, 0.0), v(0.11, 0.28, 0.11), hide));
        out.push(leg.part(v(0.0, -0.12, 0.0), v(0.125, 0.13, 0.125), shag));
        out.push(leg.part(v(0.0, -0.56, 0.0), v(0.12, 0.06, 0.13), hoof));
    }
    // A short tufted tail, swishing.
    let tail = torso.child(
        v(0.0, 0.42, 0.64),
        rx(-0.5 + (a.time * 2.1).sin() * 0.15 * (1.0 - a.gait * 0.5)),
    );
    out.push(tail.part(v(0.0, -0.14, 0.02), v(0.05, 0.15, 0.05), hide));
    out.push(tail.part(v(0.0, -0.32, 0.02), v(0.08, 0.08, 0.08), mane));
}

/// The pelter: a low lizard about 0.9 tall on splayed legs.
fn pelter(out: &mut Vec<Part>, tier: u8, exotic: bool, a: &Anim, p: &Paint) {
    let scales = p.coat(0x2f7a6c);
    let back_scales = p.coat(0x245e57);
    let belly = p.raw(0xe8d7a0);
    let ember = p.raw(0xff7a24);
    let ember_hot = p.raw(0xffc04a);
    let band = p.raw(0x2a2420);
    let eye = p.raw(0xffe066);
    let mouth = p.raw(0xc8361c);
    let body = body_frame(a, 0.45, 0.72, true);
    // Low, bobbing as it scurries; aiming raises the front a little.
    let bob = (a.stride * 2.0).sin().abs() * 0.03 * a.gait;
    let torso = body.child(v(0.0, 0.42 + bob, 0.0), rx(smooth(a.rear) * 0.12));
    out.push(torso.part(v(0.0, 0.0, 0.02), v(0.3, 0.16, 0.5), scales));
    out.push(torso.part(v(0.0, 0.14, 0.02), v(0.22, 0.04, 0.44), back_scales));
    out.push(torso.part(v(0.0, -0.15, 0.0), v(0.25, 0.03, 0.42), belly));
    // Ember spines down the back, tallest at the shoulders, raised when it
    // takes aim.
    let bristle = smooth(a.rear).max(smooth(a.lunge)) * 0.35;
    for i in 0..5 {
        let z = -0.34 + i as f32 * 0.17;
        let h = 0.13 - i as f32 * 0.017;
        let spine = torso.child(v(0.0, 0.17, z), rx(0.5 - bristle));
        out.push(Part {
            glow: 0.35,
            ..spine.part(v(0.0, h * 0.8, 0.0), v(0.04, h, 0.05), ember)
        });
        out.push(Part {
            glow: 0.6,
            ..spine.part(v(0.0, h * 1.7, 0.0), v(0.025, 0.03, 0.03), ember_hot)
        });
    }
    // Armour plates on the shoulders and hips.
    for i in 0..plates(tier) {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let z = if i < 2 { -0.25 } else { 0.28 };
        out.push(torso.part(v(side * 0.3, 0.06, z), v(0.04, 0.1, 0.13), p.raw(PLATE)));
        out.push(torso.part(v(side * 0.32, 0.06, z), v(0.02, 0.07, 0.09), p.raw(PLATE_EDGE)));
    }
    // Exotic: glowing spots along both flanks.
    if exotic {
        for side in [-1.0, 1.0] {
            for z in [-0.12, 0.06, 0.24, 0.42] {
                out.push(mark(&torso, v(side * 0.305, 0.0, z), v(0.012, 0.055, 0.055), p));
            }
        }
    }
    // Head and jaw: the jaw drops as it aims and snaps as it shoots.
    let head = torso.child(v(0.0, 0.05, -0.5), rx(-a.lean * 0.1 + smooth(a.rear) * 0.15));
    out.push(head.part(v(0.0, 0.03, -0.18), v(0.2, 0.11, 0.2), scales));
    out.push(head.part(v(0.0, 0.13, -0.12), v(0.16, 0.03, 0.15), back_scales));
    for s in [-1.0, 1.0] {
        out.push(Part {
            glow: 0.55,
            ..head.part(v(s * 0.17, 0.08, -0.26), v(0.035, 0.04, 0.045), eye)
        });
        out.push(head.part(v(s * 0.15, 0.14, -0.26), v(0.06, 0.02, 0.06), back_scales));
    }
    let open = smooth(a.rear) * 0.55 + smooth(a.lunge) * 0.3 + (a.time * 0.7).sin().max(0.0) * 0.05;
    out.push(Part {
        glow: 0.4,
        ..head.part(v(0.0, -0.08, -0.16), v(0.14, 0.02, 0.15), mouth)
    });
    let jaw = head.child(v(0.0, -0.08, -0.02), rx(-open));
    out.push(jaw.part(v(0.0, -0.03, -0.18), v(0.17, 0.04, 0.18), belly));
    // Splayed legs: out sideways, then down; they sweep forward and back
    // in a lizard's gait.
    for (x, z, phase) in [(-1.0, -0.3, 0.0), (1.0, -0.3, PI), (-1.0, 0.32, PI), (1.0, 0.32, 0.0)] {
        let sweep = (a.stride + phase).sin() * 0.5 * a.gait;
        let lift = ((a.stride + phase).cos().max(0.0)) * 0.25 * a.gait;
        let hip = torso.child(v(x * 0.26, -0.04, z), ry(sweep * x) * rz(x * lift));
        out.push(hip.part(v(x * 0.14, 0.0, 0.0), v(0.14, 0.06, 0.065), scales));
        let knee = hip.child(v(x * 0.27, 0.0, 0.0), Quat::IDENTITY);
        out.push(knee.part(v(0.0, -0.17, 0.0), v(0.06, 0.17, 0.06), scales));
        out.push(knee.part(v(0.0, -0.35, -0.04), v(0.09, 0.03, 0.11), back_scales));
    }
    // Tail: two segments and a banded rattle that rises and shakes on aim.
    let sway = (a.stride).sin() * 0.3 * a.gait + (a.time * 1.1).sin() * 0.08;
    let shake = smooth(a.rear) * (a.time * 55.0).sin() * 0.35;
    let t1 = torso.child(v(0.0, 0.0, 0.5), ry(sway) * rx(-smooth(a.rear) * 0.35));
    out.push(t1.part(v(0.0, 0.0, 0.17), v(0.13, 0.1, 0.18), scales));
    let t2 = t1.child(v(0.0, 0.0, 0.34), ry(sway * 0.8) * rx(-smooth(a.rear) * 0.45));
    out.push(t2.part(v(0.0, 0.0, 0.15), v(0.09, 0.075, 0.16), back_scales));
    let rattle = t2.child(v(0.0, 0.0, 0.3), rz(shake) * ry(shake * 0.6));
    for i in 0..4 {
        let (colour, glow) = if i % 2 == 0 { (ember, 0.3) } else { (band, 0.0) };
        let s = 0.075 - i as f32 * 0.008;
        out.push(Part {
            glow,
            ..rattle.part(v(0.0, 0.0, 0.04 + i as f32 * 0.075), v(s, s, 0.04), colour)
        });
    }
}

/// The dasher: a tall bird about 1.2 tall on long amber legs.
fn dasher(out: &mut Vec<Part>, tier: u8, exotic: bool, a: &Anim, p: &Paint) {
    let plumage = p.coat(0x5b6fd6);
    let wing = p.coat(0x37489a);
    let breast = p.raw(0xf2ead2);
    let amber = p.raw(0xf2a23a);
    let beak = p.raw(0xf5b442);
    let dark = p.raw(0x18161c);
    let coral = p.raw(0xff6f5e);
    let gold = p.raw(0xffd04a);
    let teal = p.raw(0x2fb3a6);
    let body = body_frame(a, 0.3, 0.95, false);
    // Leaning forward at a sprint, bobbing with each step.
    let bob = (a.stride * 2.0).sin().abs() * 0.04 * a.gait;
    let torso = body.child(v(0.0, 0.78 + bob, 0.05), rx(-a.lean * 0.35));
    out.push(torso.part(v(0.0, 0.0, 0.0), v(0.22, 0.2, 0.3), plumage));
    out.push(torso.part(v(0.0, -0.04, -0.22), v(0.18, 0.16, 0.12), breast));
    // Wings folded at the sides, opening a little at a run.
    for s in [-1.0, 1.0] {
        let w = torso.child(v(s * 0.22, 0.08, 0.0), rz(s * (0.1 + a.lean * 0.5)));
        out.push(w.part(v(s * 0.03, -0.06, 0.06), v(0.04, 0.14, 0.24), wing));
        out.push(w.part(v(s * 0.035, -0.17, 0.22), v(0.03, 0.05, 0.1), p.coat(0x2a3778)));
        // Exotic: bars across the wings.
        if exotic {
            for z in [-0.06, 0.06, 0.18] {
                out.push(mark(&w, v(s * 0.072, -0.06, z), v(0.012, 0.12, 0.028), p));
            }
        }
    }
    // Armour plates on the shoulders and back.
    for i in 0..plates(tier) {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let z = if i < 2 { -0.12 } else { 0.14 };
        out.push(torso.part(v(side * 0.12, 0.2, z), v(0.09, 0.03, 0.1), p.raw(PLATE)));
    }
    // Tail plumes fanned up and back.
    for (yaw, colour) in [(-0.35, teal), (0.0, plumage), (0.35, teal)] {
        let f = torso.child(
            v(0.0, 0.08, 0.28),
            ry(yaw) * rx(-0.55 + (a.time * 1.7 + yaw).sin() * 0.05),
        );
        out.push(f.part(v(0.0, 0.0, 0.24), v(0.035, 0.025, 0.24), colour));
        out.push(f.part(v(0.0, 0.0, 0.52), v(0.045, 0.03, 0.07), gold));
    }
    // A slender neck, the head jutting forward at a sprint.
    let peck = (a.time * 0.9).sin().max(0.0).powi(8) * 0.3 * (1.0 - a.gait);
    let neck = torso.child(v(0.0, 0.12, -0.22), rx(-(a.lean * 0.6 + peck)));
    out.push(neck.part(v(0.0, 0.16, -0.03), v(0.065, 0.18, 0.065), plumage));
    let head = neck.child(v(0.0, 0.34, -0.06), rx(a.lean * 0.4));
    out.push(head.part(v(0.0, 0.0, -0.03), v(0.1, 0.09, 0.13), plumage));
    out.push(head.part(v(0.0, -0.02, -0.2), v(0.04, 0.035, 0.08), beak));
    for s in [-1.0, 1.0] {
        out.push(head.part(v(s * 0.1, 0.025, -0.07), v(0.012, 0.03, 0.03), dark));
    }
    // The crest: three feathers fanned on the crown.
    for (x, roll, colour) in [(-0.04, 0.35, coral), (0.0, 0.0, gold), (0.04, -0.35, coral)] {
        let c = head.child(v(x, 0.08, 0.02), rz(roll) * rx(0.45 + (a.time * 3.0).sin() * 0.04));
        out.push(c.part(v(0.0, 0.09, 0.0), v(0.016, 0.1, 0.035), colour));
    }
    // Long amber legs, a big stride at a run.
    let swing = 0.75 * a.gait;
    for (x, phase) in [(-0.1, 0.0), (0.1, PI)] {
        let leg = body.child(v(x, 0.62, 0.08), rx((a.stride + phase).sin() * swing));
        out.push(leg.part(v(0.0, -0.06, 0.0), v(0.07, 0.1, 0.07), plumage));
        out.push(leg.part(v(0.0, -0.32, 0.0), v(0.03, 0.27, 0.03), amber));
        out.push(leg.part(v(0.0, -0.6, -0.07), v(0.07, 0.02, 0.1), amber));
    }
}

/// The popper: a round, bouncy lump of soot and ember about 0.8 tall on
/// stubby legs, with a wick for a tuft. It hops as it walks; curled, it is a
/// tight armoured ball that rolls; lit, it glows and pulses faster and
/// faster, whistles with an O mouth and its wick throws sparks; stunned, it
/// wobbles in a daze under circling stars.
fn popper(out: &mut Vec<Part>, tier: u8, exotic: bool, a: &Anim, p: &Paint) {
    let soot = p.coat(0x3d3537);
    let soot_hi = p.coat(0x5a4d4e);
    let ash = p.raw(0xb8ab9c);
    let ember = p.raw(0xff6a1a);
    let ember_hot = p.raw(0xffc04a);
    let eye = p.raw(0xfff0b0);
    let dark = p.raw(0x161112);
    let wick = p.raw(0x8a6a44);
    let star = p.raw(0xffe45a);
    let curl = smooth(a.curl);
    let heat = smooth(a.heat);
    let stun = smooth(a.stun);
    let dead = smooth(a.fall);
    let beat = heat * (0.5 + 0.5 * a.pulse.sin());
    let body = body_frame(a, 0.34, 0.76, false);
    // Hops: up with each stride, squashing as it lands.
    let land = a.stride.sin().abs();
    let open = 1.0 - curl;
    let hop = land * 0.2 * a.gait * open;
    let squash = (1.0 - land) * 0.1 * a.gait * open;
    let swell = 1.0 + 0.08 * beat;
    let (sx, sy) = ((1.0 + squash) * swell, (1.0 - squash) * swell);
    // Lit: a shiver; stunned: a slow dazed circle; curled: rolling.
    let shiver = (a.time * 31.0).sin() * 0.07 * heat;
    let daze = (a.time * 2.6).cos() * 0.16 * stun;
    let daze2 = (a.time * 2.6).sin() * 0.2 * stun;
    let r = 0.32 - 0.03 * curl;
    let torso = body.child(
        v(0.0, 0.44 - (0.44 - r) * curl + hop, 0.0),
        rz(shiver + daze) * rx(daze2 - a.roll * curl),
    );
    let k = v(sx, sy, sx);
    let blob = |f: &Frame, off: Vec3, half: Vec3, c: Vec3| f.part(off * k, half * k, c);
    // Heat flushes the soot red.
    let coat = |c: Vec3| c.lerp(p.raw(0x8a2a14), beat * 0.45);
    out.push(blob(&torso, v(0.0, 0.0, 0.0), v(r, r * 0.78, r), coat(soot)));
    out.push(blob(&torso, v(0.0, 0.02, 0.0), v(r * 0.8, r, r * 0.8), coat(soot)));
    out.push(blob(
        &torso,
        v(0.0, -0.02, 0.0),
        v(r * 1.06, r * 0.62, r * 0.8),
        coat(soot),
    ));
    out.push(blob(
        &torso,
        v(0.0, -0.02, 0.0),
        v(r * 0.8, r * 0.62, r * 1.06),
        coat(soot),
    ));
    out.push(blob(
        &torso,
        v(0.0, r * 0.92, 0.03),
        v(r * 0.55, r * 0.14, r * 0.55),
        coat(soot_hi),
    ));
    // Ember cracks in the soot, brighter as it heats.
    let crack = ember.lerp(ember_hot, beat);
    let crack_glow = if dead > 0.5 { 0.1 } else { 0.45 + 0.55 * beat };
    for (off, half, turn) in [
        (v(r * 1.07, 0.03, 0.08), v(0.012, 0.1, 0.022), rx(0.35)),
        (v(r * 1.07, -0.09, -0.07), v(0.012, 0.06, 0.02), rx(-0.4)),
        (v(-r * 1.07, 0.0, 0.1), v(0.012, 0.09, 0.022), rx(-0.3)),
        (v(-r * 1.07, -0.07, -0.08), v(0.012, 0.07, 0.02), rx(0.5)),
        (v(0.06, 0.04, r * 1.07), v(0.022, 0.11, 0.012), rz(0.3)),
        (v(-0.07, -0.06, r * 1.07), v(0.02, 0.07, 0.012), rz(-0.4)),
        (v(0.09, r * 1.0, -0.08), v(0.07, 0.012, 0.02), ry(0.5)),
        (v(-0.08, r * 1.0, 0.12), v(0.06, 0.012, 0.02), ry(-0.6)),
    ] {
        let f = torso.child(off * k, turn);
        out.push(Part {
            glow: crack_glow,
            ..f.part(Vec3::ZERO, half, crack)
        });
    }
    // The face, hidden once curled.
    if curl < 0.6 {
        let front = -r * 1.0;
        out.push(blob(&torso, v(0.0, -0.11, front), v(0.17, 0.12, 0.02), ash));
        for s in [-1.0, 1.0] {
            let eye_glow = if dead > 0.5 { 0.0 } else { 0.6 };
            out.push(Part {
                glow: eye_glow,
                ..blob(&torso, v(s * 0.12, 0.07, front - 0.005), v(0.075, 0.08, 0.02), eye)
            });
            // Pupils roll about when dazed.
            let (dx, dy) = (
                (a.time * 8.0 + s).cos() * 0.03 * stun,
                (a.time * 8.0 + s).sin() * 0.03 * stun,
            );
            out.push(blob(
                &torso,
                v(s * 0.11 + dx, 0.06 + dy, front - 0.025),
                v(0.035, 0.045, 0.01),
                dark,
            ));
            // Brows, knitted when lit.
            let brow = torso.child(v(s * 0.12, 0.17, front - 0.01) * k, rz(s * 0.4 * heat));
            out.push(brow.part(Vec3::ZERO, v(0.08, 0.02, 0.02), dark));
        }
        // The mouth: a slit, an O while it whistles.
        out.push(blob(
            &torso,
            v(0.0, -0.08, front - 0.02),
            v(0.07 - 0.03 * heat, 0.015 + 0.03 * heat, 0.012),
            dark,
        ));
        if heat > 0.05 {
            out.push(Part {
                glow: 1.0,
                ..blob(
                    &torso,
                    v(0.0, -0.08, front - 0.028),
                    v(0.025 * heat, 0.025 * heat, 0.01),
                    ember_hot,
                )
            });
        }
    }
    // Armour plates on its back.
    for i in 0..plates(tier) {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let z = 0.06 + (i / 2) as f32 * 0.17;
        let f = torso.child(v(side * 0.15, r * 0.85, z) * k, rz(-side * 0.55) * rx(0.25));
        out.push(f.part(Vec3::ZERO, v(0.11, 0.025, 0.075), p.raw(PLATE)));
        out.push(f.part(v(0.0, 0.02, 0.0), v(0.08, 0.012, 0.05), p.raw(PLATE_EDGE)));
    }
    // Curled: shell bands close over it like an armoured ball.
    if curl > 0.05 {
        for j in 0..6 {
            let band = torso.child(Vec3::ZERO, rx(-1.25 + j as f32 * 0.5));
            out.push(band.part(v(0.0, r * 1.04, 0.0), v(r * 0.92, 0.04, 0.12) * curl, soot_hi));
            out.push(Part {
                glow: crack_glow,
                ..band.part(v(0.0, r * 1.06, 0.125), v(r * 0.86, 0.01, 0.012) * curl, crack)
            });
        }
        for s in [-1.0, 1.0] {
            out.push(torso.part(v(s * r * 1.05, 0.0, 0.0), v(0.035, r * 0.72, r * 0.72) * curl, soot_hi));
        }
    }
    // Stubby legs, tucked away when curled.
    if curl < 0.9 {
        for (x, z, phase) in [
            (-0.17, -0.13, 0.0),
            (0.17, -0.13, PI),
            (-0.17, 0.14, PI),
            (0.17, 0.14, 0.0),
        ] {
            let kick = (a.stride + phase).sin() * 0.45 * a.gait;
            let leg = torso.child(v(x, -0.26 * open - 0.1 * curl, z), rx(kick));
            out.push(leg.part(v(0.0, -0.06, 0.0), v(0.07, 0.08, 0.075) * open, soot));
            out.push(leg.part(v(0.0, -0.145, -0.03), v(0.08, 0.025, 0.09) * open, ash));
        }
        // The wick on top: two twisted lengths and a smouldering tip.
        let sway = (a.time * 2.3).sin() * 0.12 + hop * 1.5;
        let w0 = torso.child(v(0.0, r * 0.95 * sy, 0.05), rx(-0.25 + sway) * rz(0.15));
        out.push(w0.part(v(0.0, 0.065, 0.0), v(0.028, 0.07, 0.028) * open, wick));
        let w1 = w0.child(v(0.0, 0.13 * open, 0.0), rz(-0.5) * rx(0.2));
        out.push(w1.part(v(0.0, 0.05, 0.0), v(0.022, 0.055, 0.022) * open, wick));
        let tip = w1.child(v(0.0, 0.11 * open, 0.0), Quat::IDENTITY);
        let flick = 0.8 + 0.2 * (a.time * (9.0 + 20.0 * heat)).sin();
        out.push(Part {
            glow: if dead > 0.5 { 0.0 } else { 0.35 + 0.65 * heat },
            ..tip.part(
                Vec3::ZERO,
                Vec3::splat(0.04 * flick * (1.0 + 0.4 * heat)) * open,
                ember_hot,
            )
        });
        // Lit: sparks spit from the wick.
        if heat > 0.05 {
            for i in 0..5 {
                let t = (a.time * 2.5 + i as f32 * 0.2) % 1.0;
                let ang = i as f32 * 2.4 + (a.time * 2.5 + i as f32 * 0.2).floor() * 1.7;
                let off = v(
                    ang.cos() * 0.18 * t,
                    0.04 + 0.22 * t - 0.25 * t * t,
                    ang.sin() * 0.18 * t,
                );
                out.push(Part {
                    glow: 1.0,
                    ..tip.part(off, Vec3::splat(0.022 * (1.0 - t) * heat), ember_hot)
                });
            }
        }
    }
    // Exotic: glowing spots on its flanks and crown.
    if exotic {
        for s in [-1.0, 1.0] {
            out.push(mark(&torso, v(s * r * 1.09, 0.1, -0.08) * k, v(0.012, 0.045, 0.045), p));
            out.push(mark(&torso, v(s * r * 1.09, -0.05, 0.12) * k, v(0.012, 0.04, 0.04), p));
            out.push(mark(
                &torso,
                v(s * 0.12, r * 1.03, -0.14) * k,
                v(0.045, 0.012, 0.045),
                p,
            ));
        }
    }
    // Stunned: stars circling over it.
    if stun > 0.05 {
        for i in 0..3 {
            let ang = a.time * 3.0 + i as f32 * TAU / 3.0;
            let f = body.child(
                v(
                    ang.cos() * 0.28,
                    0.98 + (a.time * 5.0 + i as f32).sin() * 0.03,
                    ang.sin() * 0.28,
                ),
                rz(a.time * 4.0),
            );
            for half in [v(0.055, 0.016, 0.016), v(0.016, 0.055, 0.016)] {
                out.push(Part {
                    glow: 0.9,
                    ..f.part(Vec3::ZERO, half * stun, star)
                });
            }
        }
    }
}

/// The drifter: a bronze-rimmed bell about 1.3 tall with a glowing lantern
/// face, floating on six jointed tentacles. At rest it sits low with its
/// tentacles splayed on the ground; flying, it rises, bobs and the
/// tentacles sway beneath it in a wave; dead, it tips over and they go limp.
fn drifter(out: &mut Vec<Part>, tier: u8, exotic: bool, a: &Anim, p: &Paint) {
    let shell = p.coat(0x4a3a7a);
    let shell_hi = p.coat(0x6650a0);
    let rib = p.coat(0x30244f);
    let bronze = p.raw(0xc89b4e);
    let dark = p.raw(0x12101c);
    let flesh = p.coat(0x8a64a8);
    let flesh_dark = p.coat(0x6a4a88);
    let tip = p.raw(0xeccbff);
    let lantern = p.raw(0x9ff0ff);
    let fly = smooth(a.fly);
    let dead = smooth(a.fall);
    let alive = 1.0 - dead;
    let bob = (a.time * 1.3).sin() * 0.06 * fly;
    let rim_y = (0.2 + 0.36 * fly + bob) * alive + 0.05 * dead;
    // Tipped over as it dies, about the rim's edge.
    let pivot = v(0.5, 0.0, 0.0);
    let topple = rz(-0.3 * dead);
    let base = Frame {
        at: pivot - topple * pivot,
        rot: topple,
    };
    let sway = (a.time * 0.9).sin() * 0.05 * fly;
    let bell = base.child(v(0.0, rim_y, 0.0), rz(sway) * rx(a.lean * 0.15 * fly));
    out.push(bell.part(v(0.0, 0.2, 0.0), v(0.44, 0.16, 0.44), shell));
    out.push(bell.part(v(0.0, 0.03, 0.0), v(0.5, 0.045, 0.5), bronze));
    out.push(bell.part(v(0.0, 0.36, 0.0), v(0.4, 0.04, 0.4), shell_hi));
    out.push(bell.part(v(0.0, 0.43, 0.0), v(0.35, 0.09, 0.35), shell));
    out.push(bell.part(v(0.0, 0.57, 0.0), v(0.25, 0.07, 0.25), shell));
    out.push(bell.part(v(0.0, 0.67, 0.0), v(0.13, 0.04, 0.13), bronze));
    let glow = |g: f32| g * alive;
    out.push(Part {
        glow: glow(0.8),
        ..bell.part(v(0.0, 0.75, 0.0), v(0.045, 0.05, 0.045), lantern)
    });
    // The lantern's light spilling from under the rim.
    out.push(Part {
        glow: glow(0.9),
        ..bell.part(v(0.0, -0.02, 0.0), v(0.36, 0.025, 0.36), lantern)
    });
    // Ribs down the bell, all round but the face.
    for i in 0..8 {
        if i == 4 {
            continue;
        }
        let phi = i as f32 * TAU / 8.0;
        let f = bell.child(v(phi.sin() * 0.445, 0.2, phi.cos() * 0.445), ry(phi));
        out.push(f.part(Vec3::ZERO, v(0.035, 0.15, 0.02), rib));
    }
    // The face-plate: a pane of light in a bronze frame, slit eyes.
    let face = bell.child(v(0.0, 0.21, -0.45), Quat::IDENTITY);
    let shine = 0.7 + 0.3 * (a.time * 2.0).sin();
    out.push(face.part(Vec3::ZERO, v(0.25, 0.15, 0.015), bronze));
    out.push(Part {
        glow: glow(shine),
        ..face.part(
            v(0.0, 0.0, -0.01),
            v(0.21, 0.115, 0.012),
            lantern.lerp(dark, dead * 0.6),
        )
    });
    for s in [-1.0, 1.0] {
        let e = face.child(v(s * 0.09, 0.03, -0.024), rz(-s * 0.25));
        out.push(e.part(Vec3::ZERO, v(0.06, 0.022, 0.006), dark));
    }
    out.push(face.part(v(0.0, -0.06, -0.024), v(0.018, 0.035, 0.006), dark));
    // Armour plates round its middle.
    for i in 0..plates(tier) {
        let phi = [FRAC_PI_2, -FRAC_PI_2, 0.0, PI][i];
        let f = bell.child(v(phi.sin() * 0.36, 0.43, phi.cos() * 0.36), ry(phi));
        out.push(f.part(Vec3::ZERO, v(0.13, 0.075, 0.025), p.raw(PLATE)));
        out.push(f.part(v(0.0, 0.0, 0.02), v(0.09, 0.05, 0.012), p.raw(PLATE_EDGE)));
    }
    // Exotic: glowing spots round the dome.
    if exotic {
        for i in 0..6 {
            let phi = i as f32 * TAU / 6.0 + 0.3;
            let f = bell.child(v(phi.sin() * 0.255, 0.57, phi.cos() * 0.255), ry(phi));
            out.push(mark(&f, Vec3::ZERO, v(0.04, 0.04, 0.012), p));
        }
    }
    // Tentacles: six, between the ribs, each four joints and a pale tip.
    // Each joint tilts outward from straight down: splayed and curling up at
    // rest, a wave swaying down them in flight, flat and limp when dead.
    const SEG: f32 = 0.15;
    for i in 0..6 {
        let phi = i as f32 * TAU / 6.0 + TAU / 12.0;
        let mut joint = bell.child(v(phi.sin() * 0.38, 0.0, phi.cos() * 0.38), ry(phi));
        for j in 0..4 {
            let breathe = (a.time * 0.8 + i as f32).sin() * 0.05;
            let rest = if j == 0 { 1.0 + breathe } else { 0.25 };
            let wave = (a.time * 1.8 + i as f32 * 1.3 - j as f32 * 0.9).sin() * 0.28;
            let flying = wave + if j == 0 { 0.15 } else { 0.0 };
            let limp = if j == 0 { 1.4 } else { 0.06 };
            let tilt = (rest + (flying - rest) * fly) * alive + limp * dead;
            joint = joint.child(Vec3::ZERO, rx(-tilt));
            let th = 0.06 - j as f32 * 0.008;
            let colour = if exotic && j == 1 {
                p.raw(EXOTIC)
            } else if j % 2 == 0 {
                flesh
            } else {
                flesh_dark
            };
            out.push(Part {
                glow: if exotic && j == 1 { 0.6 } else { 0.0 },
                ..joint.part(v(0.0, -SEG * 0.5, 0.0), v(th, SEG * 0.5 + 0.01, th), colour)
            });
            joint = joint.child(v(0.0, -SEG, 0.0), Quat::IDENTITY);
        }
        out.push(Part {
            glow: glow(0.5),
            ..joint.part(v(0.0, -0.025, 0.0), v(0.03, 0.035, 0.03), tip)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::creatures::{CreatureKinds, Exotic};

    fn kinds() -> Vec<CreatureDef> {
        CreatureKinds::builtin().defs().to_vec()
    }

    fn view(id: u32, kind: u16) -> CreatureView {
        CreatureView {
            id,
            kind,
            tier: 1,
            damage: 0,
            angry: false,
            dead: false,
            looted: false,
            exotic: 0,
            element: 0,
        }
    }

    fn rc(id: u32, pos: [f32; 3]) -> RemoteCreature {
        RemoteCreature {
            id,
            pos,
            yaw: 0,
            pose: Pose::Walk,
        }
    }

    /// The bounds of a model's boxes, each corner turned.
    fn bounds(parts: &[Part]) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for p in parts {
            for i in 0..8 {
                let c = Vec3::new(
                    if i & 1 == 0 { -1.0 } else { 1.0 },
                    if i & 2 == 0 { -1.0 } else { 1.0 },
                    if i & 4 == 0 { -1.0 } else { 1.0 },
                );
                let w = p.pos + p.rot * (c * p.half);
                lo = lo.min(w);
                hi = hi.max(w);
            }
        }
        (lo, hi)
    }

    #[test]
    fn creatures_interpolate_across_the_seam() {
        let shape = PlanetShape::new(8, 4);
        let w = shape.width_blocks() as f32;
        let mut c = Creatures::new(kinds());
        c.view(view(5, 0));
        // Walking east over the seam: from just west of it to just past it.
        c.record(100, &[rc(5, [w - 0.5, 44.0, 10.0])]);
        c.record(103, &[rc(5, [0.5, 44.0, 10.0])]);
        let s = c.sample(101.5 + INTERP_DELAY_TICKS, shape);
        assert_eq!(s.len(), 1);
        let x = s[0].pos.x;
        assert!(
            x < 0.01 || x > w as f64 - 0.01,
            "at the seam, not across the planet: {x}"
        );
        assert!(shape.delta(s[0].pos, DVec3::new(0.0, 44.0, 10.0)).length() < 0.01);
        // Unknown to us (no `Creature` yet): not drawn.
        c.record(103, &[rc(6, [10.0, 44.0, 10.0])]);
        assert_eq!(c.sample(103.0 + INTERP_DELAY_TICKS, shape).len(), 1);
        // Gone is gone.
        c.gone(5);
        assert!(c.sample(103.0 + INTERP_DELAY_TICKS, shape).is_empty());
    }

    #[test]
    fn one_tick_in_several_parts_is_one_sample_each() {
        let shape = PlanetShape::new(8, 4);
        let mut c = Creatures::new(kinds());
        for id in 0..60 {
            c.view(view(id, (id % 3) as u16));
        }
        for tick in [10u64, 13] {
            let all: Vec<RemoteCreature> = (0..60).map(|id| rc(id, [id as f32, 44.0, tick as f32])).collect();
            // 40 a part, as the server splits them.
            for part in all.chunks(bm_protocol::CREATURES_PER_SNAPSHOT) {
                c.record(tick, part);
            }
            // A repeat of a part changes nothing.
            c.record(tick, &all[..5]);
        }
        let s = c.sample(11.5 + INTERP_DELAY_TICKS, shape);
        assert_eq!(s.len(), 60, "every creature from every part");
        assert!(s.iter().all(|x| (x.pos.z - 11.5).abs() < 1e-6), "between the ticks");
        // A part of an older tick arriving late never steps one back.
        c.record(10, &[rc(0, [99.0, 44.0, 0.0])]);
        let s = c.sample(13.0 + INTERP_DELAY_TICKS, shape);
        assert_eq!(s[0].pos.x, 0.0);
    }

    #[test]
    fn rays_find_the_nearest_box() {
        let lo = DVec3::new(1.0, 0.0, -1.0);
        let hi = DVec3::new(2.0, 2.0, 1.0);
        assert_eq!(ray_box(DVec3::new(0.0, 1.0, 0.0), DVec3::X, lo, hi), Some(1.0));
        assert_eq!(ray_box(DVec3::new(0.0, 3.0, 0.0), DVec3::X, lo, hi), None, "above it");
        assert_eq!(ray_box(DVec3::new(3.0, 1.0, 0.0), DVec3::X, lo, hi), None, "behind us");
        assert_eq!(
            ray_box(DVec3::new(1.5, 1.0, 0.0), DVec3::X, lo, hi),
            Some(0.0),
            "inside"
        );
        // Picking: the nearer of two in a line, bodies only when asked.
        let shape = PlanetShape::new(8, 4);
        let mut c = Creatures::new(kinds());
        c.view(view(1, 0));
        c.view(CreatureView {
            dead: true,
            ..view(2, 0)
        });
        c.record(1, &[rc(1, [10.0, 44.0, 5.0]), rc(2, [6.0, 44.0, 5.0])]);
        let seen = c.sample(1.0 + INTERP_DELAY_TICKS, shape);
        let eye = DVec3::new(0.0, 44.5, 5.0);
        let pick = |living| c.pick(&seen, eye, DVec3::X, 40.0, shape, living).map(|p| p.0);
        assert_eq!(pick(false), Some(2));
        assert_eq!(pick(true), Some(1));
        assert_eq!(c.pick(&seen, eye, DVec3::X, 5.0, shape, true), None, "out of range");
        // Across the seam, from the far side.
        let w = shape.width_blocks() as f64;
        let back = DVec3::new(w - 2.0, 44.5, 5.0);
        assert_eq!(c.pick(&seen, back, DVec3::X, 40.0, shape, true).map(|p| p.0), Some(1));
    }

    #[test]
    fn models_are_the_size_of_their_kind() {
        for k in kinds() {
            assert_ne!(model_of(&k), Model::Plain, "{} has a model", k.name);
            let parts = model(&k, 1, 0, &Anim::default(), [0.3, 0.55, 0.35]);
            let (lo, hi) = bounds(&parts);
            let height = hi.y - lo.y;
            assert!(lo.y.abs() < 0.08, "{} stands on the ground: {}", k.name, lo.y);
            assert!(
                (height - k.height).abs() < k.height * 0.35,
                "{} is {height} tall, its body {}",
                k.name,
                k.height
            );
            let width = (hi.x - lo.x).max(0.0);
            assert!(
                width < k.half_width * 2.0 * 1.8 && width > k.half_width,
                "{} is {width} wide",
                k.name
            );
            assert!(parts.len() > 20, "{} is more than a box", k.name);
        }
    }

    #[test]
    fn plates_come_with_tier_and_bodies_lie_down() {
        assert_eq!((1..=6).map(plates).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 4]);
        let plate = Paint { planet: Vec3::ONE }.raw(PLATE);
        for k in kinds() {
            let count = |tier| {
                model(&k, tier, 0, &Anim::default(), [0.3, 0.55, 0.35])
                    .iter()
                    .filter(|p| p.colour == plate)
                    .count()
            };
            assert_eq!(count(1), 0, "{}", k.name);
            assert_eq!(count(3), 2, "{}", k.name);
            assert_eq!(count(5), 4, "{}", k.name);
            // Dead: grazers, dashers and poppers on their side, pelters
            // belly up, drifters tipped and limp; each lies on the ground,
            // lower than it stood.
            let dead = Anim {
                fall: 1.0,
                ..Default::default()
            };
            let parts = model(&k, 1, 0, &dead, [0.3, 0.55, 0.35]);
            let (lo, hi) = bounds(&parts);
            assert!(lo.y > -0.25 && lo.y < 0.2, "{} lies on the ground: {}", k.name, lo.y);
            assert!(hi.y < k.height * 1.05, "{} lies low: {}", k.name, hi.y);
            let up = parts[0].rot * Vec3::Y;
            match k.name.as_str() {
                "pelter" => assert!(up.y < -0.9, "the pelter is belly up: {up}"),
                "drifter" => {
                    assert!(up.y > 0.5 && up.y < 0.99, "the drifter has tipped: {up}");
                    assert!(parts.iter().all(|p| p.glow == 0.0), "its lantern is out");
                }
                _ => assert!(up.y.abs() < 0.1, "the {} is on its side: {up}", k.name),
            }
        }
    }

    #[test]
    fn poses_move_the_model() {
        let shape = PlanetShape::new(8, 4);
        let k = kinds();
        let still = Anim::default();
        // Walking swings the legs and the cycle advances with distance.
        let mut a = Anim::default();
        let mut pos = DVec3::new(10.0, 44.0, 10.0);
        for _ in 0..30 {
            pos.z -= 0.05;
            a.step(pos, Pose::Walk, false, 1.3, 1.0 / 60.0, shape);
        }
        assert!(a.gait > 0.5 && a.stride > 0.0);
        let grazer = &k[0];
        assert_ne!(model(grazer, 1, 0, &a, [0.3; 3]), model(grazer, 1, 0, &still, [0.3; 3]));
        // A windup rears the grazer (its head rises) and shakes the
        // pelter's rattle.
        let mut w = Anim::default();
        for _ in 0..40 {
            w.step(pos, Pose::Windup, false, 1.3, 1.0 / 60.0, shape);
        }
        assert!(w.rear > 0.9);
        let top = |parts: Vec<Part>| bounds(&parts).1.y;
        assert!(top(model(grazer, 1, 0, &w, [0.3; 3])) > top(model(grazer, 1, 0, &still, [0.3; 3])) + 0.1);
        let pelter = &k[1];
        let tail_end = |a: &Anim| *model(pelter, 1, 0, a, [0.3; 3]).last().unwrap();
        let mut w2 = w;
        w2.time += 0.013;
        assert_ne!(tail_end(&w).rot, tail_end(&w2).rot, "the rattle shakes");
        // Hits flash and fade; death rolls over and stays.
        let mut h = Anim {
            flash: 1.0,
            ..Default::default()
        };
        h.step(pos, Pose::Idle, false, 1.0, 0.1, shape);
        assert!(h.flash > 0.0 && h.flash < 1.0);
        for _ in 0..120 {
            h.step(pos, Pose::Dead, true, 1.0, 1.0 / 60.0, shape);
        }
        assert_eq!(h.flash, 0.0);
        assert!(h.fall > 0.95);
    }

    fn kind_named(name: &str) -> CreatureDef {
        kinds().into_iter().find(|k| k.name == name).expect("builtin kind")
    }

    /// An animation after `secs` seconds standing in `pose`.
    fn held(pose: Pose, secs: f32) -> Anim {
        let shape = PlanetShape::new(8, 4);
        let mut a = Anim::default();
        let pos = DVec3::new(10.0, 44.0, 10.0);
        for _ in 0..(secs * 60.0) as usize {
            a.step(pos, pose, false, 1.0, 1.0 / 60.0, shape);
        }
        a
    }

    fn glow(parts: &[Part]) -> f32 {
        parts.iter().map(|p| p.glow).sum()
    }

    #[test]
    fn the_popper_curls_glows_and_reels() {
        let k = kind_named("popper");
        let planet = [0.3, 0.55, 0.35];
        let idle = model(&k, 1, 0, &Anim::default(), planet);
        // Curled: a ball, no taller than it is wide, legs and wick tucked.
        let curled = model(&k, 1, 0, &held(Pose::Curled, 1.0), planet);
        let (lo, hi) = bounds(&curled);
        assert!(lo.y.abs() < 0.08, "the ball sits on the ground: {}", lo.y);
        assert!(hi.y - lo.y < bounds(&idle).1.y - 0.2, "lower than standing");
        assert!((hi.y - lo.y) < (hi.x - lo.x) * 1.15, "round");
        // Lit: hotter the longer, and its pulse quickens.
        let early = held(Pose::Lit, 0.3);
        let late = held(Pose::Lit, 2.0);
        assert!(glow(&model(&k, 1, 0, &late, planet)) > glow(&idle) + 2.0, "it glows");
        assert!(late.heat > 0.95 && late.lit > early.lit);
        let rate = |a: &Anim| {
            let mut b = *a;
            b.step(
                DVec3::new(10.0, 44.0, 10.0),
                Pose::Lit,
                false,
                1.0,
                0.01,
                PlanetShape::new(8, 4),
            );
            (b.pulse - a.pulse).rem_euclid(TAU)
        };
        assert!(rate(&late) > rate(&early) * 1.5, "faster and faster");
        // Stunned: stars over its head.
        let stunned = model(&k, 1, 0, &held(Pose::Stunned, 1.0), planet);
        assert!(bounds(&stunned).1.y > 0.9, "stars circle above it");
        assert!(stunned.len() >= idle.len() + 6);
        // Walking, it hops: its body leaves the ground.
        let shape = PlanetShape::new(8, 4);
        let mut a = Anim::default();
        let mut pos = DVec3::new(10.0, 44.0, 10.0);
        let mut highest = 0.0f32;
        for _ in 0..60 {
            pos.z -= 0.04;
            a.step(pos, Pose::Walk, false, model_of(&k).stride_len(), 1.0 / 60.0, shape);
            highest = highest.max(bounds(&model(&k, 1, 0, &a, planet)).0.y);
        }
        assert!(highest > 0.1, "hops: {highest}");
    }

    #[test]
    fn the_drifter_rises_to_fly_and_its_tentacles_sway() {
        let k = kind_named("drifter");
        let planet = [0.3, 0.55, 0.35];
        let rest = model(&k, 1, 0, &Anim::default(), planet);
        let flying = held(Pose::Flying, 3.0);
        let aloft = model(&k, 1, 0, &flying, planet);
        let (rlo, rhi) = bounds(&rest);
        let (flo, fhi) = bounds(&aloft);
        assert!(fhi.y > rhi.y + 0.25, "the bell rises: {} {}", fhi.y, rhi.y);
        assert!(flo.y < 0.05 && flo.y > -0.3, "tentacles hang to its feet: {}", flo.y);
        assert!(fhi.x - flo.x < rhi.x - rlo.x - 0.4, "they hang, not splay");
        let mut later = flying;
        later.time += 0.4;
        assert_ne!(model(&k, 1, 0, &later, planet), aloft, "they sway");
        assert!(glow(&rest) > 1.0, "its lantern face glows");
    }

    #[test]
    fn exotics_wear_markings_and_motes() {
        let planet = [0.3, 0.55, 0.35];
        let mark = Paint { planet: Vec3::ONE }.raw(EXOTIC);
        for k in kinds() {
            let plain = model(&k, 2, 0, &Anim::default(), planet);
            let exotic = model(&k, 2, Exotic::HEALTH, &Anim::default(), planet);
            assert!(plain.iter().all(|p| p.colour != mark), "{}", k.name);
            let marks = exotic.iter().filter(|p| p.colour == mark && p.glow > 0.0).count();
            assert!(marks >= 6, "{} wears markings: {marks}", k.name);
            assert!(exotic.len() >= plain.len() + 12, "{} has its motes", k.name);
        }
    }

    #[test]
    fn boxes_are_camera_relative_and_dim_when_looted() {
        let shape = PlanetShape::new(8, 4);
        let mut c = Creatures::new(kinds());
        c.view(CreatureView {
            looted: true,
            dead: true,
            ..view(1, 2)
        });
        c.record(1, &[rc(1, [10.0, 44.0, 5.0])]);
        let seen = c.sample(1.0 + INTERP_DELAY_TICKS, shape);
        let eye = DVec3::new(10.0, 44.0, 8.0);
        let boxes = c.boxes(&seen, eye, shape, [0.3; 3], |_| BoxInstance::OPEN_SKY);
        assert!(!boxes.is_empty());
        assert!(boxes.iter().all(|b| (b.light[2] - 0.4).abs() < 1e-6));
        let mid = boxes.iter().map(|b| Vec3::from(b.center)).sum::<Vec3>() / boxes.len() as f32;
        assert!(mid.z < -2.0 && mid.z > -4.0, "in front of the eye: {mid}");
    }
}
