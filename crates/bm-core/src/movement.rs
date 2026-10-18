//! Player movement, shared by server and client (ADR-0004).
//!
//! The server is the authority on where every player is. The client runs the
//! same [`step`] on the same inputs to predict its own movement, so the server's
//! answer normally matches the prediction exactly and no correction is visible.
//! For that to hold, this code must be **deterministic across machines**:
//!
//! - fixed timestep ([`TICK_DT`]), never frame time;
//! - angles arrive quantised ([`PlayerInput::yaw`]) and are turned into a
//!   direction with `libm`, which is pure Rust and bit-identical everywhere,
//!   never `f64::sin`, which calls the platform's maths library;
//! - plain `f64` arithmetic only (Rust never fuses multiply-adds on its own).

use crate::coords::BlockPos;
use crate::planet::PlanetShape;
use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Simulation rate, shared by server and client prediction.
pub const TICK_HZ: u32 = 60;
pub const TICK_DT: f64 = 1.0 / TICK_HZ as f64;

/// Angle units per full turn for [`PlayerInput::yaw`].
pub const YAW_UNITS: f64 = 65536.0;
/// Pitch units per quarter turn (straight up is `PITCH_LIMIT`).
pub const PITCH_UNITS: f64 = 16384.0;
pub const PITCH_LIMIT: i16 = 16000;

/// Movement feel. Every number here is a calibration target against the game
/// this succeeds; see `docs/feel.md` for what each should be checked against.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoveConfig {
    pub walk_speed: f64,
    pub sprint_speed: f64,
    pub swim_speed: f64,
    pub jump_velocity: f64,
    pub gravity: f64,
    pub ground_accel: f64,
    pub air_accel: f64,
    pub water_gravity_scale: f64,
    pub water_drag: f64,
    pub max_fall_speed: f64,
    pub half_width: f64,
    pub height: f64,
    pub eye_height: f64,
}

impl Default for MoveConfig {
    fn default() -> Self {
        Self {
            walk_speed: 4.6,
            sprint_speed: 7.6,
            swim_speed: 3.2,
            // Clears a 1.4-block ledge: v = sqrt(2 g h) with g = 26, h = 1.45.
            jump_velocity: 8.7,
            gravity: 26.0,
            ground_accel: 55.0,
            air_accel: 14.0,
            water_gravity_scale: 0.2,
            water_drag: 3.5,
            max_fall_speed: 55.0,
            // Nathan, 2026-10-18: a character reads as two blocks tall, and
            // no character fits under anything lower than two blocks. 1.9
            // passes a 2-block gap with room to spare and never a 1.5 one.
            half_width: 0.3,
            height: 1.9,
            eye_height: 1.75,
        }
    }
}

/// One tick of player intent. Small and quantised: it travels many times a
/// second, redundantly (ADR-0004).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInput {
    /// -1, 0 or 1. Larger values are clamped.
    pub forward: i8,
    pub strafe: i8,
    pub jump: bool,
    pub sprint: bool,
    /// Heading in 1/65536 turns. 0 faces -Z; a quarter turn faces +X.
    pub yaw: u16,
    pub pitch: i16,
}

impl PlayerInput {
    pub fn from_angles(yaw_rad: f64, pitch_rad: f64) -> (u16, i16) {
        let turns = (yaw_rad / std::f64::consts::TAU).rem_euclid(1.0);
        let yaw = ((turns * YAW_UNITS).round() as u32 & 0xffff) as u16;
        let pitch = (pitch_rad / std::f64::consts::FRAC_PI_2 * PITCH_UNITS)
            .round()
            .clamp(-(PITCH_LIMIT as f64), PITCH_LIMIT as f64) as i16;
        (yaw, pitch)
    }
}

pub fn yaw_radians(yaw: u16) -> f64 {
    yaw as f64 / YAW_UNITS * std::f64::consts::TAU
}

pub fn pitch_radians(pitch: i16) -> f64 {
    pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT) as f64 / PITCH_UNITS * std::f64::consts::FRAC_PI_2
}

/// Horizontal forward and right unit vectors for a yaw. Yaw 0 faces -Z.
pub fn heading(yaw: u16) -> (DVec3, DVec3) {
    let a = yaw_radians(yaw);
    let (s, c) = (libm::sin(a), libm::cos(a));
    let forward = DVec3::new(s, 0.0, -c);
    let right = DVec3::new(c, 0.0, s);
    (forward, right)
}

/// Full look direction (unit) for yaw and pitch.
pub fn look_dir(yaw: u16, pitch: i16) -> DVec3 {
    let (f, _) = heading(yaw);
    let p = pitch_radians(pitch);
    let (sp, cp) = (libm::sin(p), libm::cos(p));
    DVec3::new(f.x * cp, sp, f.z * cp)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    /// Centre of the feet. Canonical on the planet (ADR-0002).
    pub pos: DVec3,
    pub vel: DVec3,
    pub on_ground: bool,
}

impl PlayerState {
    pub fn eye(&self, cfg: &MoveConfig) -> DVec3 {
        self.pos + DVec3::new(0.0, cfg.eye_height, 0.0)
    }

    /// Whether the body overlaps a block (used to refuse placing into players).
    pub fn overlaps_block(&self, cfg: &MoveConfig, b: BlockPos) -> bool {
        let (lo, hi) = aabb(self.pos, cfg);
        let (bx, by, bz) = (b.x as f64, b.y as f64, b.z as f64);
        lo.x < bx + 1.0 && hi.x > bx && lo.y < by + 1.0 && hi.y > by && lo.z < bz + 1.0 && hi.z > bz
    }
}

/// What movement needs to know about the world. Positions passed in may be
/// off the canonical lap; implementations wrap them.
pub trait PhysicsWorld {
    fn solid(&self, p: BlockPos) -> bool;
    fn liquid(&self, p: BlockPos) -> bool;
}

const SKIN: f64 = 1e-6;

fn aabb(pos: DVec3, cfg: &MoveConfig) -> (DVec3, DVec3) {
    (
        DVec3::new(pos.x - cfg.half_width, pos.y, pos.z - cfg.half_width),
        DVec3::new(pos.x + cfg.half_width, pos.y + cfg.height, pos.z + cfg.half_width),
    )
}

fn approach(v: DVec3, target: DVec3, max_delta: f64) -> DVec3 {
    let d = target - v;
    let len = d.length();
    if len <= max_delta || len == 0.0 {
        target
    } else {
        v + d * (max_delta / len)
    }
}

/// Advances one tick. Deterministic: same state, input and world give the same
/// result on every machine.
pub fn step(
    s: &mut PlayerState,
    input: &PlayerInput,
    cfg: &MoveConfig,
    planet: PlanetShape,
    world: &impl PhysicsWorld,
) {
    let dt = TICK_DT;
    let in_water = world.liquid(BlockPos::containing(s.pos.x, s.pos.y + cfg.height * 0.4, s.pos.z));

    let (fwd, right) = heading(input.yaw);
    let f = input.forward.clamp(-1, 1) as f64;
    let r = input.strafe.clamp(-1, 1) as f64;
    let mut wish = fwd * f + right * r;
    let wl = wish.length();
    if wl > 0.0 {
        wish /= wl;
    }
    let speed = if in_water {
        cfg.swim_speed
    } else if input.sprint && f > 0.0 {
        cfg.sprint_speed
    } else {
        cfg.walk_speed
    };
    let accel = if s.on_ground { cfg.ground_accel } else { cfg.air_accel };
    let horiz = approach(DVec3::new(s.vel.x, 0.0, s.vel.z), wish * speed, accel * dt);
    s.vel.x = horiz.x;
    s.vel.z = horiz.z;

    if in_water {
        s.vel.y -= cfg.gravity * cfg.water_gravity_scale * dt;
        if input.jump {
            s.vel.y += cfg.gravity * 0.6 * dt;
        }
        let drag = 1.0 / (1.0 + cfg.water_drag * dt);
        s.vel.y *= drag;
        s.vel.y = s.vel.y.clamp(-cfg.swim_speed, cfg.swim_speed);
    } else {
        if input.jump && s.on_ground {
            s.vel.y = cfg.jump_velocity;
        }
        s.vel.y = (s.vel.y - cfg.gravity * dt).max(-cfg.max_fall_speed);
    }

    // Sub-step so no axis moves more than a little under half a block per
    // pass: a fast fall can never tunnel through a one-block floor.
    let travel = s.vel * dt;
    let steps = (travel.abs().max_element() / 0.45).ceil().max(1.0) as u32;
    let part = travel / steps as f64;
    s.on_ground = false;
    for _ in 0..steps {
        if move_axis(s, 1, part.y, cfg, world) {
            if part.y < 0.0 {
                s.on_ground = true;
            }
            s.vel.y = 0.0;
        }
        if move_axis(s, 0, part.x, cfg, world) {
            s.vel.x = 0.0;
        }
        if move_axis(s, 2, part.z, cfg, world) {
            s.vel.z = 0.0;
        }
    }
    // Standing still on the ground still needs to know it is on the ground.
    if !s.on_ground && s.vel.y <= 0.0 {
        let probe = PlayerState {
            pos: s.pos - DVec3::new(0.0, 2.0 * SKIN, 0.0),
            ..*s
        };
        s.on_ground = intersects(&probe, cfg, world);
    }
    s.pos = planet.wrap_point(s.pos);
}

fn intersects(s: &PlayerState, cfg: &MoveConfig, world: &impl PhysicsWorld) -> bool {
    let (lo, hi) = aabb(s.pos, cfg);
    for_blocks(lo, hi, |b| world.solid(b))
}

fn for_blocks(lo: DVec3, hi: DVec3, mut f: impl FnMut(BlockPos) -> bool) -> bool {
    let (x0, y0, z0) = (
        (lo.x + SKIN).floor() as i64,
        (lo.y + SKIN).floor() as i64,
        (lo.z + SKIN).floor() as i64,
    );
    let (x1, y1, z1) = (
        (hi.x - SKIN).floor() as i64,
        (hi.y - SKIN).floor() as i64,
        (hi.z - SKIN).floor() as i64,
    );
    for y in y0..=y1 {
        for z in z0..=z1 {
            for x in x0..=x1 {
                if f(BlockPos::new(x, y, z)) {
                    return true;
                }
            }
        }
    }
    false
}

/// Moves along one axis, stopping flush against the first solid block.
/// Returns whether it was blocked.
fn move_axis(s: &mut PlayerState, axis: usize, delta: f64, cfg: &MoveConfig, world: &impl PhysicsWorld) -> bool {
    if delta == 0.0 {
        return false;
    }
    let mut p = s.pos;
    p[axis] += delta;
    let (lo, hi) = aabb(p, cfg);
    // Extent of the box from `pos` along this axis, below and above.
    let (below, above) = match axis {
        1 => (0.0, cfg.height),
        _ => (cfg.half_width, cfg.half_width),
    };
    let mut limit = p[axis];
    let mut blocked = false;
    for_blocks(lo, hi, |b| {
        if world.solid(b) {
            let face = [b.x, b.y, b.z][axis] as f64;
            if delta > 0.0 {
                limit = limit.min(face - above);
            } else {
                limit = limit.max(face + 1.0 + below);
            }
            blocked = true;
        }
        false
    });
    if blocked {
        // Never move backwards past where we started (we were already
        // touching or, after a correction, overlapping).
        p[axis] = if delta > 0.0 {
            limit.max(s.pos[axis])
        } else {
            limit.min(s.pos[axis])
        };
    }
    s.pos = p;
    blocked
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct Flat {
        solid: HashSet<BlockPos>,
        floor: i64,
        water_below: i64,
    }

    impl PhysicsWorld for Flat {
        fn solid(&self, p: BlockPos) -> bool {
            p.y < self.floor || self.solid.contains(&p)
        }
        fn liquid(&self, p: BlockPos) -> bool {
            p.y < self.water_below && !self.solid(p)
        }
    }

    fn world() -> Flat {
        Flat {
            solid: HashSet::new(),
            floor: 10,
            water_below: 0,
        }
    }

    fn planet() -> PlanetShape {
        PlanetShape::new(16, 4)
    }

    fn run(s: &mut PlayerState, input: PlayerInput, ticks: u32, w: &Flat) {
        let cfg = MoveConfig::default();
        for _ in 0..ticks {
            step(s, &input, &cfg, planet(), w);
        }
    }

    #[test]
    fn falls_and_lands_on_the_floor() {
        let w = world();
        let mut s = PlayerState {
            pos: DVec3::new(5.5, 30.0, 5.5),
            ..Default::default()
        };
        run(&mut s, PlayerInput::default(), 240, &w);
        assert!(s.on_ground);
        assert!((s.pos.y - 10.0).abs() < 1e-9, "{}", s.pos.y);
        assert_eq!(s.vel.y, 0.0);
    }

    #[test]
    fn fast_falls_do_not_tunnel() {
        let mut w = world();
        w.floor = -100;
        w.solid.insert(BlockPos::new(5, 0, 5));
        let mut s = PlayerState {
            pos: DVec3::new(5.5, 5.0, 5.5),
            vel: DVec3::new(0.0, -55.0, 0.0),
            on_ground: false,
        };
        run(&mut s, PlayerInput::default(), 10, &w);
        assert!((s.pos.y - 1.0).abs() < 1e-9, "{}", s.pos.y);
    }

    #[test]
    fn walls_stop_horizontal_motion() {
        let mut w = world();
        for y in 10..13 {
            w.solid.insert(BlockPos::new(8, y, 5));
        }
        let mut s = PlayerState {
            pos: DVec3::new(5.5, 10.0, 5.5),
            ..Default::default()
        };
        let (yaw, _) = PlayerInput::from_angles(std::f64::consts::FRAC_PI_2, 0.0); // +X
        run(
            &mut s,
            PlayerInput {
                forward: 1,
                yaw,
                ..Default::default()
            },
            120,
            &w,
        );
        assert!((s.pos.x - (8.0 - 0.3)).abs() < 1e-9, "{}", s.pos.x);
    }

    #[test]
    fn fits_a_two_block_tunnel_but_not_a_one_block_one() {
        let walk_into = |roof_y: i64| {
            let mut w = world();
            for x in 8..20 {
                w.solid.insert(BlockPos::new(x, roof_y, 5));
            }
            let mut s = PlayerState {
                pos: DVec3::new(5.5, 10.0, 5.5),
                on_ground: true,
                ..Default::default()
            };
            let (yaw, _) = PlayerInput::from_angles(std::f64::consts::FRAC_PI_2, 0.0);
            run(
                &mut s,
                PlayerInput {
                    forward: 1,
                    yaw,
                    ..Default::default()
                },
                120,
                &w,
            );
            s.pos.x
        };
        assert!(walk_into(12) > 12.0, "a 2-block tunnel is passable");
        assert!(walk_into(11) < 8.0, "a 1-block tunnel is not");
    }

    #[test]
    fn jump_clears_one_block_but_not_two() {
        let cfg = MoveConfig::default();
        let peak = cfg.jump_velocity * cfg.jump_velocity / (2.0 * cfg.gravity);
        assert!(peak > 1.2 && peak < 2.0, "peak {peak}");
    }

    #[test]
    fn sprint_reaches_sprint_speed() {
        let w = world();
        let mut s = PlayerState {
            pos: DVec3::new(5.5, 10.0, 5.5),
            on_ground: true,
            ..Default::default()
        };
        run(
            &mut s,
            PlayerInput {
                forward: 1,
                sprint: true,
                ..Default::default()
            },
            60,
            &w,
        );
        let v = DVec3::new(s.vel.x, 0.0, s.vel.z).length();
        assert!((v - MoveConfig::default().sprint_speed).abs() < 1e-9, "{v}");
    }

    #[test]
    fn position_wraps_across_the_seam() {
        let w = world();
        let mut s = PlayerState {
            pos: DVec3::new(511.9, 10.0, 5.5),
            on_ground: true,
            ..Default::default()
        };
        let (yaw, _) = PlayerInput::from_angles(std::f64::consts::FRAC_PI_2, 0.0);
        run(
            &mut s,
            PlayerInput {
                forward: 1,
                yaw,
                ..Default::default()
            },
            30,
            &w,
        );
        assert!(s.pos.x >= 0.0 && s.pos.x < 10.0, "{}", s.pos.x);
    }

    #[test]
    fn swimming_floats_up_with_jump() {
        let mut w = world();
        w.floor = 0;
        w.water_below = 20;
        let mut s = PlayerState {
            pos: DVec3::new(5.5, 5.0, 5.5),
            ..Default::default()
        };
        run(
            &mut s,
            PlayerInput {
                jump: true,
                ..Default::default()
            },
            120,
            &w,
        );
        assert!(s.pos.y > 8.0, "{}", s.pos.y);
    }

    #[test]
    fn same_inputs_give_bit_identical_results() {
        let w = world();
        let inputs: Vec<PlayerInput> = (0..600u32)
            .map(|i| PlayerInput {
                forward: (i % 3) as i8 - 1,
                strafe: (i % 5 % 3) as i8 - 1,
                jump: i % 37 == 0,
                sprint: i % 2 == 0,
                yaw: (i * 977) as u16,
                pitch: 0,
            })
            .collect();
        let cfg = MoveConfig::default();
        let simulate = || {
            let mut s = PlayerState {
                pos: DVec3::new(100.5, 20.0, 100.5),
                ..Default::default()
            };
            for inp in &inputs {
                step(&mut s, inp, &cfg, planet(), &w);
            }
            (s.pos.x.to_bits(), s.pos.y.to_bits(), s.pos.z.to_bits())
        };
        assert_eq!(simulate(), simulate());
    }

    #[test]
    fn angle_quantisation_round_trips() {
        let (yaw, pitch) = PlayerInput::from_angles(1.0, -0.5);
        assert!((yaw_radians(yaw) - 1.0).abs() < 1e-3);
        assert!((pitch_radians(pitch) + 0.5).abs() < 1e-3);
        let (_, p) = PlayerInput::from_angles(0.0, 10.0);
        assert_eq!(p, PITCH_LIMIT);
        let d = look_dir(0, 0);
        assert!((d - DVec3::new(0.0, 0.0, -1.0)).length() < 1e-12);
    }

    #[test]
    fn overlap_detects_the_body() {
        let cfg = MoveConfig::default();
        let s = PlayerState {
            pos: DVec3::new(5.5, 10.0, 5.5),
            ..Default::default()
        };
        assert!(s.overlaps_block(&cfg, BlockPos::new(5, 11, 5)));
        assert!(!s.overlaps_block(&cfg, BlockPos::new(5, 12, 5)));
        assert!(!s.overlaps_block(&cfg, BlockPos::new(6, 10, 5)));
    }
}
