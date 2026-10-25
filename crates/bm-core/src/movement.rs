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
//!
//! The grapple is part of movement for the same reason: where the hook
//! sticks and how the rope swings decide where the player goes, so both
//! sides run it in [`step`] from the same inputs and the same item.

use crate::coords::BlockPos;
use crate::planet::PlanetShape;
use crate::raycast::raycast_shaped;
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
    /// Upward speed while pushing against a wall with jump held in water:
    /// enough to clear a shore one block above the surface.
    pub water_exit_velocity: f64,
    pub max_fall_speed: f64,
    /// No player moves faster than this, however flung or reeled: Boundless
    /// capped players and creatures at 40 m/s so the world can stream in
    /// ahead of them (`docs/research/boundless-tuning.md` 1.4).
    pub max_speed: f64,
    /// Lean acceleration while hanging from a grapple in the air (WASD
    /// pumps the swing; there is no speed cap, as in a real swing).
    pub swing_accel: f64,
    /// Horizontal speed lost per second (as a fraction) while flying faster
    /// than a sprint after letting go of a grapple: the fling carries.
    pub fling_drag: f64,
    pub half_width: f64,
    pub height: f64,
    pub eye_height: f64,
}

impl Default for MoveConfig {
    fn default() -> Self {
        Self {
            // An untrained Boundless character: run 3.0, sprint 5.0 m/s
            // (tuning 1.6; Nathan, 2026-10-20). Agility will add 0.3 and 0.6
            // a point when skills exist.
            walk_speed: 3.0,
            sprint_speed: 5.0,
            swim_speed: 3.2,
            // Clears a 1.4-block ledge: v = sqrt(2 g h) with g = 26, h = 1.45.
            jump_velocity: 8.7,
            gravity: 26.0,
            ground_accel: 55.0,
            air_accel: 14.0,
            water_gravity_scale: 0.2,
            water_drag: 3.5,
            water_exit_velocity: 10.5,
            // Boundless's terminal fall speed (tuning 1.4).
            max_fall_speed: 40.0,
            max_speed: 40.0,
            swing_accel: 9.0,
            fling_drag: 0.4,
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
    /// The hotbar slot in hand this tick: a grapple there is what fires.
    pub slot: u8,
    /// Fire the grapple this tick, or let go (or recall) if one is out.
    pub fire: bool,
    /// Rope: 1 reels in, -1 pays out. Larger values are clamped.
    pub reel: i8,
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

/// What a grapple does, from its item (`assets/items.toml`; Boundless's
/// numbers in `docs/research/boundless-tuning.md` 1.1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GrappleSpec {
    /// How far the hook flies before it is lost; also the longest rope.
    pub range: f64,
    /// Hook flight speed, blocks per second.
    pub shot_speed: f64,
    /// The rope's top speed coming in and going out, blocks per second.
    pub reel_in_speed: f64,
    pub reel_out_speed: f64,
    /// Reels in by itself once stuck (a hookshot): pulls, swings poorly. In
    /// Boundless a forge boon, not a kind of grapple.
    pub hookshot: bool,
    /// Energy a shot costs (`energy`).
    pub energy: u16,
}

/// A fired grapple hook. It flies straight (no gravity, as in the game this
/// succeeds) until it strikes a block, then holds the rope.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hook {
    /// Canonical, like the player's position.
    pub pos: DVec3,
    /// Flight velocity; zero once stuck.
    pub vel: DVec3,
    /// Flying: the distance flown. Stuck: the rope's length.
    pub rope: f64,
    /// The block it is stuck in; None while it flies.
    pub stuck: Option<BlockPos>,
    /// How fast the rope is being taken in (out when negative), blocks per
    /// second: it gathers speed at [`REEL_ACCEL`] up to the grapple's reel
    /// speed, and stops when the reel is let go.
    pub reel: f64,
}

/// Where the rope meets the body, above the feet: the hands.
pub const ROPE_HEIGHT: f64 = 1.2;
/// The shortest rope: reeling in stops here.
pub const ROPE_MIN: f64 = 1.0;
/// How fast the reel gathers speed, blocks per second squared. Boundless
/// reeled like a force towards the grapple's reel speed, not a fixed rope
/// speed: players measured about 40 blocks of travel to reach 40 blocks per
/// second (tuning 1.4), which is about this.
pub const REEL_ACCEL: f64 = 20.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    /// Centre of the feet. Canonical on the planet (ADR-0002).
    pub pos: DVec3,
    pub vel: DVec3,
    pub on_ground: bool,
    /// The grapple hook, while one is out. Never saved: a player who leaves
    /// lets go.
    pub hook: Option<Hook>,
    /// Energy used, out of `energy::MAX_ENERGY` (0: rested). Never saved:
    /// a player arrives rested.
    pub spent: u16,
    /// Ticks until the bar starts refilling (`energy::PAUSE_TICKS` after
    /// each use).
    pub rest: u16,
    /// What the character's skills do to the body (milestone 12). Set by
    /// the server at an input both sides agree on (`ClientMsg::Skill`), so
    /// prediction never disagrees. Default: untrained.
    pub body: Body,
    /// Jump was held on the last tick (a double jump needs a fresh press).
    pub jump_held: bool,
    /// The double jump was used since last on the ground.
    pub air_jumped: bool,
    /// Health lost, out of `max_health()` (0: unhurt; milestone 13,
    /// `combat`). Saved with the character.
    pub hurt: u32,
    /// Ticks since last dealing or taking damage (saturating): regeneration
    /// waits for `combat::CALM_TICKS`.
    pub calm: u16,
    /// Counts 0 to `combat::REGEN_PERIOD_TICKS`: healing comes in steps.
    pub beat: u16,
    /// Ticks since a grapple hook was let go (saturating; `u16::MAX` once a
    /// landing has used the grace): falls inside `combat::FALL_GRACE_TICKS`
    /// do no harm (Nathan, 2026-10-21).
    pub loose: u16,
    /// Defeated: lies still until the server brings them back.
    pub down: bool,
    /// Hunger, in 1/600 points (milestone 16, `food`). Saved.
    pub hunger: u32,
    /// The food buff. Saved.
    pub buff: crate::food::FoodBuff,
    /// Ticks of sickness left (raw meat).
    pub sick: u16,
    /// Ticks until the next bite.
    pub chew: u16,
    /// The quirks of the item in hand (milestone 17 part 2), set by the
    /// caller before each step from the slot the input names, on server and
    /// client alike.
    pub quirks: crate::forge::Quirks,
    /// Steps taken (wrapping): the dice of the random quirks, so both sides
    /// roll the same.
    pub clock: u32,
    /// The air of the planet the player is on (milestone 18, `atmosphere`).
    /// Set by the server when they arrive; the client takes it from the
    /// state it is sent.
    pub air: crate::atmosphere::Air,
    /// Ticks of air used, out of `atmosphere::AIR_TICKS` (0: full). Saved.
    pub air_used: u16,
}

/// What skills change about moving and energy (`skills::Effects::body`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Body {
    /// Added to `MoveConfig::walk_speed` and `sprint_speed` (Swiftness).
    pub run_bonus: f64,
    pub sprint_bonus: f64,
    /// Jumps about two blocks higher (the high-jump epic).
    pub high_jump: bool,
    /// One more jump in mid-air (the double-jump epic).
    pub double_jump: bool,
    /// Added to `energy::MAX_ENERGY` and `energy::REGEN_PER_TICK`.
    pub energy_bonus: u16,
    pub regen_bonus: u16,
    /// Added to a grapple's shot speed and range (grapple mastery).
    pub grapple_shot_bonus: f64,
    pub grapple_range_bonus: f64,
    /// Added to `combat::BASE_HEALTH` (Toughness, Iron Body).
    pub health_bonus: u32,
    /// Taken off every fall's damage (Soft Landing).
    pub falling_armour: u16,
    /// Mending: +25 health every 5 s, in combat too.
    pub mending: bool,
    /// Protection points against each air line (acid, charged, burning)
    /// from the ward skills (milestone 18).
    pub wards: [u8; 3],
}

/// Extra height the high-jump epic adds, in blocks (Boundless's skill text:
/// "Extends your Jump height by 2m", tuning 5.4a).
pub const HIGH_JUMP_EXTRA: f64 = 2.0;
/// A double jump's speed, as a fraction of a jump's: about half a block
/// more (players, tuning 1.6; a **guess** at the impulse).
pub const DOUBLE_JUMP_FRACTION: f64 = 0.7;

/// Slippery's grip on the ground, as a share of the usual (**guess**).
pub const SLIPPERY_GRIP: f64 = 0.15;
/// Leaden's pull down in water, as a multiple of the usual (**guess**).
pub const LEADEN_SINK: f64 = 3.0;
/// Bouncing: a landing faster than this (blocks a second) springs back with
/// this share of its speed (**guesses**).
pub const BOUNCE_MIN: f64 = 6.0;
pub const BOUNCE_KEEP: f64 = 0.5;
/// Jumpy's chance of a jump on any tick on the ground: about one every 4 s
/// (**guess**; players saw a holder "jumping up and down while running").
pub const JUMPY_CHANCE: f64 = 1.0 / 240.0;
/// Twitchy's chance of going off on any tick: about every 10 s (**guess**).
pub const TWITCHY_CHANCE: f64 = 1.0 / 600.0;

/// What happened in a tick that the caller acts on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepEvents {
    /// A grapple was fired (it wears by one).
    pub fired: bool,
    /// Twitchy (milestone 17 part 2): the item in hand goes off by itself
    /// this tick. A grapple already fired inside the step; anything else is
    /// used by the client as if clicked.
    pub twitched: bool,
    /// A grapple was let go by firing again (it wears by one too, as in
    /// Boundless; putting it away costs nothing).
    pub released: bool,
}

impl PlayerState {
    pub fn eye(&self, cfg: &MoveConfig) -> DVec3 {
        self.pos + DVec3::new(0.0, cfg.eye_height, 0.0)
    }

    /// Where the rope meets the body.
    pub fn hands(&self) -> DVec3 {
        self.pos + DVec3::new(0.0, ROPE_HEIGHT, 0.0)
    }

    /// The hook, if it is stuck: where, and the rope's length.
    pub fn anchor(&self) -> Option<(DVec3, f64)> {
        self.hook.filter(|h| h.stuck.is_some()).map(|h| (h.pos, h.rope))
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
    /// The shape of a solid block (ADR-0008): its kept octants are what
    /// collides. Client and server must answer identically (invariant 3).
    fn shape(&self, _p: BlockPos) -> crate::block::Shape {
        crate::block::Shape::FULL
    }
    /// A soft block (soil, sand, snow, leaves: `Health::Soft`) softens a
    /// fall (`combat::SOFT_LANDING`). Client and server must agree.
    fn soft(&self, _p: BlockPos) -> bool {
        false
    }
}

const SKIN: f64 = 1e-6;

/// The tallest obstacle a walker steps onto without jumping. Every cut in the
/// world is a multiple of half a block (ADR-0008), so this takes a half-block
/// step (a slab, a slope's lower half) and never a full block; the extra 0.05
/// is margin, not a feel number. Shared by client and server, so a constant
/// rather than part of [`MoveConfig`].
pub const STEP_HEIGHT: f64 = 0.55;

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

/// Advances one tick. Deterministic: same state, input, grapple and world
/// give the same result on every machine. `grapple` is what the item in
/// `input.slot` can do, if it is a grapple; the caller looks it up in its own
/// copy of the inventory.
pub fn step(
    s: &mut PlayerState,
    input: &PlayerInput,
    cfg: &MoveConfig,
    grapple: Option<&GrappleSpec>,
    planet: PlanetShape,
    world: &impl PhysicsWorld,
) -> StepEvents {
    let dt = TICK_DT;
    let mut events = StepEvents::default();
    // Defeated: no control, the body only falls (and lets go of any rope).
    let still = PlayerInput {
        yaw: input.yaw,
        pitch: input.pitch,
        slot: input.slot,
        ..Default::default()
    };
    let input = if s.down { &still } else { input };
    let grapple = if s.down { None } else { grapple };
    let had_hook = s.hook.is_some();
    // Whether the grapple was used this tick (energy: a use restarts the
    // pause; a tick without one rests).
    let mut used = false;
    // Grapple mastery lengthens the rope and speeds the shot.
    let skilled = grapple.map(|g| GrappleSpec {
        range: g.range + s.body.grapple_range_bonus,
        shot_speed: g.shot_speed + s.body.grapple_shot_bonus,
        ..*g
    });
    let grapple = skilled.as_ref();
    // Twitchy: the item in hand goes off by itself now and then (Release
    // 209's wonky trigger; how often is a **guess**).
    let twitch = !s.down
        && s.quirks.has(crate::forge::Quirks::TWITCHY)
        && crate::forge::Quirks::roll(s.clock, 2) < TWITCHY_CHANCE;
    events.twitched = twitch;
    let fire = input.fire || (twitch && s.hook.is_none());
    match grapple {
        // Nothing in hand holds the rope: putting the grapple away lets go.
        None => s.hook = None,
        // Firing again lets go of a stuck hook or recalls a flying one.
        Some(_) if fire && s.hook.is_some() => {
            s.hook = None;
            events.released = true;
        }
        Some(g) if fire => {
            used = true;
            // Too tired to fire: nothing happens but the pause starts again.
            if s.try_spend(g.energy, crate::food::Drain::Ranged) {
                s.hook = Some(Hook {
                    pos: s.eye(cfg),
                    vel: look_dir(input.yaw, input.pitch) * g.shot_speed,
                    rope: 0.0,
                    stuck: None,
                    reel: 0.0,
                });
                events.fired = true;
            }
        }
        Some(_) => {}
    }
    if !used {
        s.recover();
    }
    if let (Some(g), Some(h)) = (grapple, s.hook) {
        s.hook = hook_tick(h, g, s.hands(), input.reel, planet, world);
    }
    if had_hook && s.hook.is_none() {
        s.loose = 0;
    } else if s.hook.is_none() {
        s.loose = s.loose.saturating_add(1);
    }
    s.beat = (s.beat + 1) % crate::combat::REGEN_PERIOD_TICKS;
    s.heal_tick(s.beat == 0);
    s.food_tick(s.beat == 0);
    s.air_tick(s.beat == 0);
    s.quirk_tick(s.beat == 0);
    let hooked = s.anchor().is_some();
    let in_water = world.liquid(BlockPos::containing(s.pos.x, s.pos.y + cfg.height * 0.4, s.pos.z));

    let (fwd, right) = heading(input.yaw);
    let f = input.forward.clamp(-1, 1) as f64;
    let r = input.strafe.clamp(-1, 1) as f64;
    let mut wish = fwd * f + right * r;
    let wl = wish.length();
    if wl > 0.0 {
        wish /= wl;
    }
    // Hunger from moving on foot (milestone 16): running and sprinting.
    if s.on_ground && !in_water && wl > 0.0 {
        s.add_hunger(if input.sprint && f > 0.0 {
            crate::food::SPRINT_HUNGER_PER_TICK
        } else {
            crate::food::RUN_HUNGER_PER_TICK
        });
    }
    let q = s.quirks;
    let walk = cfg.walk_speed + s.body.run_bonus;
    let sprint = cfg.sprint_speed + s.body.sprint_bonus;
    // Sprinter: walking and running 25% slower, the sprint's bonus 50%
    // bigger (Release 209).
    let (walk, sprint) = if q.has(crate::forge::Quirks::SPRINTER) {
        (walk * 0.75, walk * 0.75 + (sprint - walk) * 1.5)
    } else {
        (walk, sprint)
    };
    let speed = if in_water {
        cfg.swim_speed
    } else if input.sprint && f > 0.0 {
        sprint
    } else {
        walk
    };
    let flat = DVec3::new(s.vel.x, 0.0, s.vel.z);
    let horiz = if s.on_ground || in_water {
        let accel = if s.on_ground {
            // Slippery: the ground barely grips (**guess**: 15%).
            cfg.ground_accel
                * if q.has(crate::forge::Quirks::SLIPPERY) {
                    SLIPPERY_GRIP
                } else {
                    1.0
                }
        } else {
            cfg.air_accel
        };
        approach(flat, wish * speed, accel * dt)
    } else if hooked {
        // Hanging on the rope: leaning pumps the swing, nothing brakes it.
        flat + wish * cfg.swing_accel * dt
    } else if flat.length() > cfg.sprint_speed + s.body.sprint_bonus {
        // Flung by a grapple: steer, but keep the speed, which fades slowly.
        let speed = flat.length() * (1.0 - cfg.fling_drag * dt);
        let steered = flat + wish * cfg.air_accel * dt;
        steered * (speed / steered.length())
    } else {
        approach(flat, wish * speed, cfg.air_accel * dt)
    };
    s.vel.x = horiz.x;
    s.vel.z = horiz.z;

    // Climbing out of water (Nathan, 2026-10-18: "I can't jump out of
    // water"). A swimmer floats with their feet most of a block under the
    // surface, from where no jump reaches a shore a block above it. Pushing
    // against a solid block with jump held pops them up out of the water.
    let against_wall = wl > 0.0 && {
        let probe = PlayerState {
            pos: s.pos + wish * 0.08,
            ..*s
        };
        intersects(&probe, cfg, world)
    };
    if in_water && input.jump && against_wall {
        s.vel.y = cfg.water_exit_velocity;
    } else if in_water {
        // Leaden: sinks, and swimming up barely helps (**guess**).
        let sink = if q.has(crate::forge::Quirks::LEADEN) {
            LEADEN_SINK
        } else {
            1.0
        };
        s.vel.y -= cfg.gravity * cfg.water_gravity_scale * sink * dt;
        if input.jump && sink == 1.0 {
            s.vel.y += cfg.gravity * 0.6 * dt;
        }
        let drag = 1.0 / (1.0 + cfg.water_drag * dt);
        s.vel.y *= drag;
        s.vel.y = s.vel.y.clamp(-cfg.swim_speed, cfg.swim_speed);
    } else {
        // Jumpy: jumps now and then by itself, and gives a double jump
        // (Release 209).
        let jumpy = q.has(crate::forge::Quirks::JUMPY);
        let random_jump = jumpy && s.on_ground && crate::forge::Quirks::roll(s.clock, 1) < JUMPY_CHANCE;
        let fresh = input.jump && !s.jump_held;
        if (input.jump || random_jump) && s.on_ground {
            // v^2 = v0^2 + 2 g h: the same jump, h higher (the high-jump
            // epic, Springy food).
            let extra = if s.body.high_jump { HIGH_JUMP_EXTRA } else { 0.0 } + s.jump_extra();
            s.vel.y = if extra > 0.0 {
                (cfg.jump_velocity * cfg.jump_velocity + 2.0 * cfg.gravity * extra).sqrt()
            } else {
                cfg.jump_velocity
            };
            s.add_hunger(crate::food::JUMP_HUNGER);
        } else if fresh && (s.body.double_jump || jumpy) && !s.air_jumped && s.hook.is_none() {
            s.vel.y = cfg.jump_velocity * DOUBLE_JUMP_FRACTION;
            s.air_jumped = true;
            s.add_hunger(crate::food::JUMP_HUNGER);
        }
        s.vel.y = (s.vel.y - cfg.gravity * dt).max(-cfg.max_fall_speed);
    }

    // The rope: once taut it takes away any speed outwards from the hook and
    // gives back what it takes to end the tick exactly at its length.
    // Gravity turns into swing, and a shortened rope pulls.
    if let Some((anchor, rope)) = s.anchor() {
        let r = planet.delta(anchor, s.hands());
        let dist = r.length();
        if dist > 1e-9 {
            let n = r / dist;
            let out = s.vel.dot(n);
            if dist + out * dt > rope {
                let wanted = ((rope - dist) / dt).max(-cfg.max_speed);
                s.vel += n * (wanted - out);
            }
        }
    }
    let speed = s.vel.length();
    if speed > cfg.max_speed {
        s.vel *= cfg.max_speed / speed;
    }

    // Stepping up (ADR-0008): only a walker on the ground who is not rising
    // (a jump this tick sets the velocity upwards) and not swimming.
    let may_step = s.on_ground && s.vel.y <= 0.0 && !in_water;

    // Sub-step so no axis moves more than a little under half a block per
    // pass: a fast fall can never tunnel through a one-block floor.
    let travel = s.vel * dt;
    let steps = (travel.abs().max_element() / 0.45).ceil().max(1.0) as u32;
    let part = travel / steps as f64;
    let was_on_ground = s.on_ground;
    let mut impact = 0.0;
    s.on_ground = false;
    for _ in 0..steps {
        if move_axis(s, 1, part.y, cfg, world) {
            if part.y < 0.0 {
                s.on_ground = true;
                impact = -s.vel.y;
            }
            s.vel.y = 0.0;
        }
        let before = *s;
        let mut blocked_x = move_axis(s, 0, part.x, cfg, world);
        let mut blocked_z = move_axis(s, 2, part.z, cfg, world);
        if may_step
            && (blocked_x || blocked_z)
            && let Some((stepped, bx, bz)) = step_up(&before, s, part, cfg, world)
        {
            *s = stepped;
            (blocked_x, blocked_z) = (bx, bz);
            s.on_ground = true;
            s.vel.y = 0.0;
        }
        if blocked_x {
            s.vel.x = 0.0;
        }
        if blocked_z {
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
    if s.on_ground && !was_on_ground && !in_water && impact > 0.0 {
        let below = BlockPos::containing(s.pos.x, s.pos.y - 0.5, s.pos.z);
        s.land(impact, world.soft(below));
        // Bouncing: a hard landing springs back up at half the speed
        // (**guess**). The landing itself hurts as any would; Boundless's
        // quirk also hurt on every bounce, a bug players hated.
        if q.has(crate::forge::Quirks::BOUNCING) && impact > BOUNCE_MIN && !s.down {
            s.vel.y = impact * BOUNCE_KEEP;
            s.on_ground = false;
        }
    }
    s.jump_held = input.jump;
    if s.on_ground {
        s.air_jumped = false;
    }
    s.clock = s.clock.wrapping_add(1);
    events
}

/// One tick of the hook: flight until it strikes a block or flies its range,
/// then the reel. None when it is lost, or the block it held is gone.
fn hook_tick(
    mut h: Hook,
    g: &GrappleSpec,
    hands: DVec3,
    reel: i8,
    planet: PlanetShape,
    world: &impl PhysicsWorld,
) -> Option<Hook> {
    let dt = TICK_DT;
    let Some(block) = h.stuck else {
        let reach = (g.shot_speed * dt).min(g.range - h.rope);
        let hit = raycast_shaped(h.pos, h.vel, reach, |p| world.solid(p).then(|| world.shape(p)));
        if let Some(hit) = hit {
            h.pos = planet.wrap_point(hit.point);
            h.vel = DVec3::ZERO;
            h.stuck = Some(planet.wrap_block(hit.block));
            h.rope = planet.delta(h.pos, hands).length().clamp(ROPE_MIN, g.range);
            return Some(h);
        }
        h.rope += reach;
        if h.rope >= g.range {
            return None;
        }
        h.pos = planet.wrap_point(h.pos + h.vel * (reach / g.shot_speed));
        return Some(h);
    };
    if !world.solid(block) {
        return None;
    }
    let target = match reel.clamp(-1, 1) {
        1 => g.reel_in_speed,
        -1 => -g.reel_out_speed,
        _ if g.hookshot => g.reel_in_speed,
        _ => 0.0,
    };
    // The reel gathers speed towards its target and stops at once when let
    // go, so the rope holds where it was left.
    h.reel = if target == 0.0 {
        0.0
    } else if h.reel < target {
        (h.reel + REEL_ACCEL * dt).min(target)
    } else {
        (h.reel - REEL_ACCEL * dt).max(target)
    };
    if h.reel > 0.0 {
        // Slack is taken up at once, so reeling in always pulls.
        let dist = planet.delta(h.pos, hands).length();
        h.rope = h.rope.min(dist);
    }
    let rope = h.rope - h.reel * dt;
    h.rope = rope.clamp(ROPE_MIN, g.range);
    if h.rope != rope {
        h.reel = 0.0;
    }
    Some(h)
}

/// The horizontal move of one sub-step tried again from `STEP_HEIGHT` higher
/// (as far as the headroom allows), then settled back down onto whatever is
/// underneath. Taken only if it gets further than the plain move did and
/// lands on something: a half-block ledge is climbed, a full-block wall is
/// not (raised half a block, it still blocks). Returns the stepped state and
/// whether X and Z were still blocked.
fn step_up(
    before: &PlayerState,
    plain: &PlayerState,
    part: DVec3,
    cfg: &MoveConfig,
    world: &impl PhysicsWorld,
) -> Option<(PlayerState, bool, bool)> {
    let mut t = *before;
    move_axis(&mut t, 1, STEP_HEIGHT, cfg, world);
    let risen = t.pos.y - before.pos.y;
    if risen <= 0.0 {
        return None;
    }
    let bx = move_axis(&mut t, 0, part.x, cfg, world);
    let bz = move_axis(&mut t, 2, part.z, cfg, world);
    if !move_axis(&mut t, 1, -risen, cfg, world) || t.pos.y <= before.pos.y {
        return None;
    }
    let gain = |p: &PlayerState| {
        let (dx, dz) = (p.pos.x - before.pos.x, p.pos.z - before.pos.z);
        dx * dx + dz * dz
    };
    (gain(&t) > gain(plain) + 1e-12).then_some((t, bx, bz))
}

fn intersects(s: &PlayerState, cfg: &MoveConfig, world: &impl PhysicsWorld) -> bool {
    let (lo, hi) = aabb(s.pos, cfg);
    for_solid_boxes(world, lo, hi, |_, _| true)
}

/// Calls `f` with every solid box overlapping `lo..hi` (more than [`SKIN`]
/// deep), stopping when it returns true. A full block is one unit box; only
/// a chiselled one is asked for its shape (ADR-0008), which gives one
/// half-block box per kept octant.
fn for_solid_boxes(world: &impl PhysicsWorld, lo: DVec3, hi: DVec3, mut f: impl FnMut(DVec3, DVec3) -> bool) -> bool {
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
                let b = BlockPos::new(x, y, z);
                if !world.solid(b) {
                    continue;
                }
                let base = DVec3::new(x as f64, y as f64, z as f64);
                let shape = world.shape(b);
                if shape.is_full() {
                    if f(base, base + DVec3::ONE) {
                        return true;
                    }
                    continue;
                }
                for (l, h) in shape.boxes() {
                    let (bl, bh) = (base + DVec3::from_array(l), base + DVec3::from_array(h));
                    let overlaps = (0..3).all(|a| bl[a] < hi[a] - SKIN && bh[a] > lo[a] + SKIN);
                    if overlaps && f(bl, bh) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Moves along one axis, stopping flush against the first solid box.
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
    for_solid_boxes(world, lo, hi, |bl, bh| {
        if delta > 0.0 {
            limit = limit.min(bl[axis] - above);
        } else {
            limit = limit.max(bh[axis] + below);
        }
        blocked = true;
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
    use crate::block::Shape;
    use crate::shape::{ShapeFamily, octant};
    use std::collections::{HashMap, HashSet};

    struct Flat {
        solid: HashSet<BlockPos>,
        shaped: HashMap<BlockPos, Shape>,
        floor: i64,
        water_below: i64,
    }

    impl PhysicsWorld for Flat {
        fn solid(&self, p: BlockPos) -> bool {
            p.y < self.floor || self.solid.contains(&p) || self.shaped.contains_key(&p)
        }
        fn liquid(&self, p: BlockPos) -> bool {
            p.y < self.water_below && !self.solid(p)
        }
        fn shape(&self, p: BlockPos) -> Shape {
            self.shaped.get(&p).copied().unwrap_or(Shape::FULL)
        }
    }

    fn world() -> Flat {
        Flat {
            solid: HashSet::new(),
            shaped: HashMap::new(),
            floor: 10,
            water_below: 0,
        }
    }

    /// The bottom half of a block (its top four octants cut away).
    fn slab() -> Shape {
        Shape::new(ShapeFamily::Square, 0xf0).unwrap()
    }

    fn east() -> PlayerInput {
        let (yaw, _) = PlayerInput::from_angles(std::f64::consts::FRAC_PI_2, 0.0); // +X
        PlayerInput {
            forward: 1,
            yaw,
            ..Default::default()
        }
    }

    fn standing(x: f64, y: f64) -> PlayerState {
        PlayerState {
            pos: DVec3::new(x, y, 5.5),
            on_ground: true,
            ..Default::default()
        }
    }

    #[test]
    fn walks_up_a_half_slab_without_jumping() {
        let mut w = world();
        for x in 8..30 {
            w.shaped.insert(BlockPos::new(x, 10, 5), slab());
        }
        let mut s = standing(5.5, 10.0);
        run(&mut s, east(), 200, &w);
        assert!(s.pos.x > 12.0, "stopped at the slab: {:?}", s.pos);
        assert!((s.pos.y - 10.5).abs() < 1e-9 && s.on_ground, "{:?}", s.pos);
    }

    #[test]
    fn walks_up_a_staircase_of_halves() {
        // Each block east is half a block higher: slab, full, full + slab, ...
        let mut w = world();
        for k in 0..12i64 {
            let x = 8 + k;
            for y in 10..10 + (k + 1) / 2 {
                w.solid.insert(BlockPos::new(x, y, 5));
            }
            if k % 2 == 0 {
                w.shaped.insert(BlockPos::new(x, 10 + k / 2, 5), slab());
            }
        }
        for x in 20..40 {
            for y in 10..16 {
                w.solid.insert(BlockPos::new(x, y, 5));
            }
        }
        let mut s = standing(5.5, 10.0);
        run(&mut s, east(), 500, &w);
        assert!(s.pos.x > 22.0 && (s.pos.y - 16.0).abs() < 1e-9, "{:?}", s.pos);
    }

    #[test]
    fn a_full_block_wall_is_not_stepped_onto() {
        let mut w = world();
        w.solid.insert(BlockPos::new(8, 10, 5));
        let mut s = standing(5.5, 10.0);
        run(&mut s, east(), 120, &w);
        assert!((s.pos.x - 7.7).abs() < 1e-9 && s.pos.y == 10.0, "{:?}", s.pos);
        // Neither is a half slab on top of a full block (1.5 high).
        let mut w = world();
        w.solid.insert(BlockPos::new(8, 10, 5));
        w.shaped.insert(BlockPos::new(8, 11, 5), slab());
        let mut s = standing(5.5, 10.0);
        run(&mut s, east(), 120, &w);
        assert!((s.pos.x - 7.7).abs() < 1e-9, "{:?}", s.pos);
    }

    #[test]
    fn a_slab_overhead_blocks_like_its_kept_half() {
        // A top-half slab (bottom octants cut) at y = 12 leaves 2.5 blocks of
        // headroom, so a walker passes under it; at y = 11 it leaves 1.5.
        let top = Shape::new(ShapeFamily::Square, 0x0f).unwrap();
        let pass = |roof: i64| {
            let mut w = world();
            for x in 8..20 {
                w.shaped.insert(BlockPos::new(x, roof, 5), top);
            }
            let mut s = standing(5.5, 10.0);
            run(&mut s, east(), 200, &w);
            s.pos.x
        };
        assert!(pass(12) > 12.0);
        assert!(pass(11) < 8.0, "1.5 blocks is too low");
    }

    #[test]
    fn falls_onto_exactly_the_kept_octants_of_every_shape() {
        let cfg = MoveConfig {
            half_width: 0.1,
            height: 0.2,
            ..MoveConfig::default()
        };
        for removed in 1u8..0xff {
            let shape = Shape::new(ShapeFamily::Slope, removed).unwrap();
            let mut w = world();
            w.shaped.insert(BlockPos::new(5, 10, 5), shape);
            for (ox, oz) in [(0u8, 0u8), (1, 0), (0, 1), (1, 1)] {
                let mut s = PlayerState {
                    pos: DVec3::new(5.25 + ox as f64 * 0.5, 13.0, 5.25 + oz as f64 * 0.5),
                    ..Default::default()
                };
                for _ in 0..240 {
                    step(&mut s, &PlayerInput::default(), &cfg, None, planet(), &w);
                }
                let want = if shape.keeps(octant(ox, 1, oz)) {
                    11.0
                } else if shape.keeps(octant(ox, 0, oz)) {
                    10.5
                } else {
                    10.0
                };
                assert!(
                    (s.pos.y - want).abs() < 1e-9 && s.on_ground,
                    "shape {removed:#04x} column ({ox}, {oz}): {} not {want}",
                    s.pos.y
                );
            }
        }
    }

    #[test]
    fn shaped_worlds_give_bit_identical_results() {
        let mut w = world();
        for x in 95..106 {
            for z in 95..106 {
                let removed = ((x * 31 + z * 17) % 255 + 1) as u8;
                if removed != 0xff && (x + z) % 3 != 0 {
                    w.shaped.insert(
                        BlockPos::new(x, 10 + (x + z) % 2, z),
                        Shape::new(ShapeFamily::Square, removed).unwrap(),
                    );
                }
            }
        }
        let inputs: Vec<PlayerInput> = (0..900u32)
            .map(|i| PlayerInput {
                forward: 1,
                strafe: (i / 50 % 3) as i8 - 1,
                jump: i % 97 == 0,
                sprint: i % 200 < 100,
                yaw: (i * 311) as u16,
                ..Default::default()
            })
            .collect();
        let cfg = MoveConfig::default();
        let simulate = || {
            let mut s = PlayerState {
                pos: DVec3::new(100.5, 14.0, 100.5),
                ..Default::default()
            };
            let mut trace = Vec::new();
            for inp in &inputs {
                step(&mut s, inp, &cfg, None, planet(), &w);
                trace.push((s.pos.x.to_bits(), s.pos.y.to_bits(), s.pos.z.to_bits()));
            }
            trace
        };
        assert_eq!(simulate(), simulate());
    }

    fn planet() -> PlanetShape {
        PlanetShape::new(16, 4)
    }

    fn run(s: &mut PlayerState, input: PlayerInput, ticks: u32, w: &Flat) {
        let cfg = MoveConfig::default();
        for _ in 0..ticks {
            step(s, &input, &cfg, None, planet(), w);
        }
    }

    fn quirky(q: u16) -> PlayerState {
        PlayerState {
            quirks: crate::forge::Quirks(q),
            ..standing(5.5, 10.0)
        }
    }

    #[test]
    fn sprinter_walks_slower_and_sprints_faster() {
        use crate::forge::Quirks;
        let w = world();
        let (mut plain, mut slow) = (standing(5.5, 10.0), quirky(Quirks::SPRINTER));
        run(&mut plain, east(), 120, &w);
        run(&mut slow, east(), 120, &w);
        assert!(
            (slow.vel.x - plain.vel.x * 0.75).abs() < 1e-9,
            "{} vs {}",
            slow.vel.x,
            plain.vel.x
        );
        let sprint = PlayerInput { sprint: true, ..east() };
        let (mut plain, mut fast) = (standing(5.5, 10.0), quirky(Quirks::SPRINTER));
        run(&mut plain, sprint, 120, &w);
        run(&mut fast, sprint, 120, &w);
        let cfg = MoveConfig::default();
        let want = cfg.walk_speed * 0.75 + (cfg.sprint_speed - cfg.walk_speed) * 1.5;
        assert!((fast.vel.x - want).abs() < 1e-9 && fast.vel.x > plain.vel.x);
    }

    #[test]
    fn slippery_ground_barely_grips() {
        use crate::forge::Quirks;
        let w = world();
        let (mut plain, mut icy) = (standing(5.5, 10.0), quirky(Quirks::SLIPPERY));
        run(&mut plain, east(), 6, &w);
        run(&mut icy, east(), 6, &w);
        assert!(icy.vel.x < plain.vel.x * 0.5, "{} vs {}", icy.vel.x, plain.vel.x);
        assert_eq!(icy.falling_armour(), plain.falling_armour() + 40);
    }

    #[test]
    fn bouncing_springs_back_from_a_hard_landing() {
        use crate::forge::Quirks;
        let w = world();
        let drop = |q| PlayerState {
            pos: DVec3::new(5.5, 20.0, 5.5),
            quirks: Quirks(q),
            ..Default::default()
        };
        let (mut plain, mut bouncy) = (drop(0), drop(Quirks::BOUNCING));
        let mut bounced = false;
        for _ in 0..200 {
            run(&mut plain, PlayerInput::default(), 1, &w);
            run(&mut bouncy, PlayerInput::default(), 1, &w);
            bounced |= bouncy.vel.y > 0.0;
        }
        assert!(bounced, "it bounced");
        assert!(plain.on_ground && bouncy.on_ground, "and settles");
        assert_eq!(plain.hurt, bouncy.hurt, "the bounces do not hurt again");
    }

    #[test]
    fn jumpy_jumps_by_itself_and_doubles_the_same_on_both_sides() {
        use crate::forge::Quirks;
        let w = world();
        let mut a = quirky(Quirks::JUMPY);
        let mut b = a;
        let mut jumps = 0;
        for _ in 0..2400 {
            let was = a.on_ground;
            run(&mut a, PlayerInput::default(), 1, &w);
            run(&mut b, PlayerInput::default(), 1, &w);
            jumps += (was && a.vel.y > 0.0) as u32;
        }
        assert_eq!(a, b, "the same dice on both sides");
        assert!((3..30).contains(&jumps), "about one every 4 s: {jumps} in 40 s");
        // A double jump without the epic.
        let mut s = quirky(Quirks::JUMPY);
        run(
            &mut s,
            PlayerInput {
                jump: true,
                ..Default::default()
            },
            1,
            &w,
        );
        run(&mut s, PlayerInput::default(), 10, &w);
        let v = s.vel.y;
        run(
            &mut s,
            PlayerInput {
                jump: true,
                ..Default::default()
            },
            1,
            &w,
        );
        assert!(s.air_jumped && s.vel.y > v);
    }

    #[test]
    fn twitchy_goes_off_now_and_then() {
        use crate::forge::Quirks;
        let w = world();
        let cfg = MoveConfig::default();
        let mut s = quirky(Quirks::TWITCHY);
        let mut twitches = 0;
        for _ in 0..6000 {
            twitches += step(&mut s, &PlayerInput::default(), &cfg, None, planet(), &w).twitched as u32;
        }
        assert!((3..30).contains(&twitches), "about every 10 s: {twitches} in 100 s");
    }

    #[test]
    fn gnawing_and_draining_raise_the_maximum_and_drain() {
        use crate::forge::Quirks;
        let w = world();
        let plain = standing(5.5, 10.0);
        let (mut gnaw, mut drain) = (quirky(Quirks::GNAWING), quirky(Quirks::DRAINING));
        assert_eq!(gnaw.full_energy() as u32, plain.full_energy() as u32 * 130 / 100);
        assert_eq!(drain.max_health(), plain.max_health() * 130 / 100);
        run(&mut gnaw, PlayerInput::default(), 600, &w);
        run(&mut drain, PlayerInput::default(), 599, &w);
        assert!(gnaw.hunger > 0, "hungry from standing still");
        assert_eq!(drain.hurt, crate::forge::DRAINING_PER_BEAT, "5 every 5 s");
        // Being Full heals faster than it drains, once calm.
        run(&mut drain, PlayerInput::default(), 1200, &w);
        assert!(drain.hurt <= crate::forge::DRAINING_PER_BEAT, "{}", drain.hurt);
    }

    #[test]
    fn leaden_sinks_in_water() {
        use crate::forge::Quirks;
        let mut w = world();
        w.floor = 0;
        w.water_below = 40;
        let at = |q| PlayerState {
            pos: DVec3::new(5.5, 30.0, 5.5),
            quirks: Quirks(q),
            ..Default::default()
        };
        let (mut plain, mut lead) = (at(0), at(Quirks::LEADEN));
        let up = PlayerInput {
            jump: true,
            ..Default::default()
        };
        run(&mut plain, up, 120, &w);
        run(&mut lead, up, 120, &w);
        assert!(lead.pos.y < plain.pos.y - 3.0, "{} vs {}", lead.pos.y, plain.pos.y);
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
            ..Default::default()
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
                200,
                &w,
            );
            s.pos.x
        };
        assert!(walk_into(12) > 12.0, "a 2-block tunnel is passable");
        assert!(walk_into(11) < 8.0, "a 1-block tunnel is not");
    }

    #[test]
    fn skills_change_jumps_and_speed() {
        let w = world();
        let peak = |body: Body, presses: &[u32]| {
            let mut s = PlayerState {
                body,
                ..standing(5.5, 10.0)
            };
            let mut top = s.pos.y;
            for i in 0..120u32 {
                let input = PlayerInput {
                    jump: presses.contains(&i),
                    ..Default::default()
                };
                run(&mut s, input, 1, &w);
                top = top.max(s.pos.y);
            }
            top - 10.0
        };
        let plain = peak(Body::default(), &[0]);
        let high = peak(
            Body {
                high_jump: true,
                ..Default::default()
            },
            &[0],
        );
        assert!((high - plain - HIGH_JUMP_EXTRA).abs() < 0.1, "{plain} {high}");
        // A second press in the air jumps again only with the epic.
        let double = Body {
            double_jump: true,
            ..Default::default()
        };
        assert!((peak(Body::default(), &[0, 20]) - plain).abs() < 1e-9);
        assert!(peak(double, &[0, 20]) > plain + 0.4);
        // Swiftness adds speed.
        let mut s = PlayerState {
            body: Body {
                run_bonus: 0.3 * 9.0,
                ..Default::default()
            },
            ..standing(5.5, 10.0)
        };
        run(&mut s, east(), 120, &w);
        assert!(s.pos.x - 5.5 > 2.0 * 5.0, "{:?}", s.pos);
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
    fn swimmers_climb_out_onto_a_shore_one_block_above_the_water() {
        // Water fills y < 20 (its surface is the top of y = 19); the shore at
        // x >= 8 stands one block higher (top at y = 21).
        let mut w = world();
        w.floor = 0;
        w.water_below = 20;
        for x in 8..60 {
            for z in 0..12 {
                for y in 0..21 {
                    w.solid.insert(BlockPos::new(x, y, z));
                }
            }
        }
        let mut s = PlayerState {
            pos: DVec3::new(5.5, 15.0, 5.5),
            ..Default::default()
        };
        let (yaw, _) = PlayerInput::from_angles(std::f64::consts::FRAC_PI_2, 0.0); // +X
        run(
            &mut s,
            PlayerInput {
                forward: 1,
                jump: true,
                yaw,
                ..Default::default()
            },
            300,
            &w,
        );
        assert!(s.pos.x > 8.3 && s.pos.y >= 21.0, "still in the water at {:?}", s.pos);
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
                ..Default::default()
            })
            .collect();
        let cfg = MoveConfig::default();
        let simulate = || {
            let mut s = PlayerState {
                pos: DVec3::new(100.5, 20.0, 100.5),
                ..Default::default()
            };
            for inp in &inputs {
                step(&mut s, inp, &cfg, None, planet(), &w);
            }
            (s.pos.x.to_bits(), s.pos.y.to_bits(), s.pos.z.to_bits())
        };
        assert_eq!(simulate(), simulate());
    }

    const GRAPPLE: GrappleSpec = GrappleSpec {
        range: 26.0,
        shot_speed: 65.0,
        reel_in_speed: 10.0,
        reel_out_speed: 10.0,
        hookshot: false,
        // Free, so these tests are about the rope; energy has its own.
        energy: 0,
    };

    /// A flat ceiling of blocks at `y` over the area around the player.
    fn ceiling(w: &mut Flat, y: i64) {
        for x in -20..30 {
            for z in -20..30 {
                w.solid.insert(BlockPos::new(x, y, z));
            }
        }
    }

    fn up() -> PlayerInput {
        PlayerInput {
            pitch: PITCH_LIMIT,
            ..Default::default()
        }
    }

    /// Runs `ticks` ticks holding `g`, returning how many shots were fired.
    fn hold(s: &mut PlayerState, input: PlayerInput, ticks: u32, g: Option<&GrappleSpec>, w: &Flat) -> u32 {
        let cfg = MoveConfig::default();
        let mut fired = 0;
        for _ in 0..ticks {
            fired += step(s, &input, &cfg, g, planet(), w).fired as u32;
        }
        fired
    }

    fn fire(input: PlayerInput) -> PlayerInput {
        PlayerInput { fire: true, ..input }
    }

    #[test]
    fn grapple_shots_cost_energy_and_a_tired_shot_restarts_the_pause() {
        // An iron grapple (80 a shot, tuning 1.1) fires 7 times from a full
        // bar of 600; letting go costs nothing.
        let w = world();
        let iron = GrappleSpec { energy: 80, ..GRAPPLE };
        let mut s = standing(5.5, 10.0);
        let mut shots = 0;
        for _ in 0..40 {
            // Fire, then fire again to let go.
            shots += hold(&mut s, fire(up()), 1, Some(&iron), &w);
            hold(&mut s, fire(up()), 1, Some(&iron), &w);
        }
        assert_eq!(shots, 7);
        // Each shot adds 0.16 hunger (energy / 500): one point off the bar.
        assert_eq!(s.max_energy(), 599);
        assert_eq!(s.energy(), 599 - 7 * 80);
        assert_eq!(s.rest, crate::energy::PAUSE_TICKS, "the last try restarted the pause");
        // Rested long enough, it fires again.
        hold(&mut s, up(), (crate::energy::PAUSE_TICKS + 60) as u32, Some(&iron), &w);
        assert_eq!(hold(&mut s, fire(up()), 1, Some(&iron), &w), 1);
    }

    #[test]
    fn running_sprinting_and_jumping_make_you_hungry() {
        // Milestone 16 guesses: run 0.5 a second, sprint 1, a jump 1.
        let w = world();
        let scale = crate::food::HUNGER_SCALE;
        let mut s = standing(5.5, 10.0);
        hold(&mut s, PlayerInput::default(), 600, None, &w);
        assert_eq!(s.hunger, 0, "standing still costs nothing");
        let run = PlayerInput {
            forward: 1,
            ..Default::default()
        };
        hold(&mut s, run, 600, None, &w);
        assert_eq!(s.hunger, 5 * scale, "10 s running");
        hold(&mut s, PlayerInput { sprint: true, ..run }, 600, None, &w);
        assert_eq!(s.hunger, 15 * scale, "then 10 s sprinting");
        let mut j = standing(5.5, 10.0);
        hold(
            &mut j,
            PlayerInput {
                jump: true,
                ..Default::default()
            },
            1,
            None,
            &w,
        );
        assert_eq!(j.hunger, scale, "one jump");
        // Sated: nothing.
        let mut f = standing(5.5, 10.0);
        f.buff = crate::food::FoodBuff {
            kind: crate::food::Buff::Sated,
            rank: 1,
            left: 10_000,
        };
        hold(&mut f, run, 600, None, &w);
        assert_eq!(f.hunger, 0);
    }

    #[test]
    fn springy_food_jumps_higher() {
        let w = world();
        let mut s = standing(5.5, 10.0);
        s.buff = crate::food::FoodBuff {
            kind: crate::food::Buff::Springy,
            rank: 5,
            left: 10_000,
        };
        let mut peak: f64 = 0.0;
        hold(
            &mut s,
            PlayerInput {
                jump: true,
                ..Default::default()
            },
            1,
            None,
            &w,
        );
        for _ in 0..120 {
            hold(&mut s, PlayerInput::default(), 1, None, &w);
            peak = peak.max(s.pos.y);
        }
        let cfg = MoveConfig::default();
        let plain = cfg.jump_velocity * cfg.jump_velocity / (2.0 * cfg.gravity);
        assert!(peak - 10.0 > plain + 1.8, "two metres higher: {}", peak - 10.0);
    }

    #[test]
    fn a_hook_flies_and_sticks_in_the_first_block_it_meets() {
        let mut w = world();
        ceiling(&mut w, 22);
        let mut s = standing(5.5, 10.0);
        assert_eq!(hold(&mut s, fire(up()), 1, Some(&GRAPPLE), &w), 1);
        assert!(s.hook.is_some_and(|h| h.stuck.is_none()), "flying after one tick");
        hold(&mut s, up(), 20, Some(&GRAPPLE), &w);
        let h = s.hook.expect("still out");
        assert_eq!(h.stuck.map(|b| b.y), Some(22));
        assert!((h.pos.y - 22.0).abs() < 1e-9, "on the ceiling's face: {}", h.pos.y);
        assert!((h.rope - (22.0 - 11.2)).abs() < 0.2, "rope is the distance: {}", h.rope);
    }

    #[test]
    fn a_hook_that_meets_nothing_in_range_is_lost() {
        let w = world();
        let mut s = standing(5.5, 10.0);
        hold(&mut s, fire(up()), 1, Some(&GRAPPLE), &w);
        // 26 blocks at 65 per second: gone within half a second.
        hold(&mut s, up(), 30, Some(&GRAPPLE), &w);
        assert_eq!(s.hook, None);
    }

    #[test]
    fn grapple_mastery_lengthens_the_rope_and_a_faster_shot_flies_no_further() {
        // A ceiling 28.8 blocks above the hands: past a 26-block grapple.
        let mut w = world();
        ceiling(&mut w, 40);
        let shoot = |body: Body| {
            let mut s = PlayerState {
                body,
                ..standing(5.5, 10.0)
            };
            hold(&mut s, fire(up()), 1, Some(&GRAPPLE), &w);
            hold(&mut s, up(), 40, Some(&GRAPPLE), &w);
            s.hook.and_then(|h| h.stuck)
        };
        // A faster shot alone still stops at the grapple's range.
        let fast = Body {
            grapple_shot_bonus: 25.0,
            ..Default::default()
        };
        assert_eq!(shoot(fast), None);
        // One rank of mastery: 5 blocks longer, and it reaches.
        let longer = Body {
            grapple_range_bonus: 5.0,
            ..fast
        };
        assert_eq!(shoot(longer).map(|b| b.y), Some(40));
    }

    #[test]
    fn firing_again_or_putting_the_grapple_away_lets_go() {
        let mut w = world();
        ceiling(&mut w, 22);
        let mut s = standing(5.5, 10.0);
        hold(&mut s, fire(up()), 1, Some(&GRAPPLE), &w);
        hold(&mut s, up(), 20, Some(&GRAPPLE), &w);
        assert!(s.anchor().is_some());
        assert_eq!(
            hold(&mut s, fire(up()), 1, Some(&GRAPPLE), &w),
            0,
            "letting go is not a shot"
        );
        assert_eq!(s.hook, None);
        hold(&mut s, fire(up()), 1, Some(&GRAPPLE), &w);
        hold(&mut s, up(), 20, Some(&GRAPPLE), &w);
        hold(&mut s, up(), 1, None, &w);
        assert_eq!(s.hook, None, "nothing in hand holds the rope");
    }

    #[test]
    fn reeling_in_lifts_and_the_rope_then_holds() {
        let mut w = world();
        ceiling(&mut w, 22);
        let mut s = standing(5.5, 10.0);
        hold(&mut s, fire(up()), 1, Some(&GRAPPLE), &w);
        hold(&mut s, up(), 20, Some(&GRAPPLE), &w);
        let reel_in = PlayerInput { reel: 1, ..up() };
        hold(&mut s, reel_in, 60, Some(&GRAPPLE), &w);
        assert!(!s.on_ground);
        assert!(
            s.pos.y > 15.0,
            "a second of reeling, gathering speed to 10 per second: {}",
            s.pos.y
        );
        // Let go of the reel: hanging, not falling.
        let y = s.pos.y;
        hold(&mut s, up(), 120, Some(&GRAPPLE), &w);
        assert!((s.pos.y - y).abs() < 0.3, "hangs where it was: {y} then {}", s.pos.y);
        // Paying out lowers back to the ground.
        let reel_out = PlayerInput { reel: -1, ..up() };
        hold(&mut s, reel_out, 240, Some(&GRAPPLE), &w);
        assert!(s.on_ground && (s.pos.y - 10.0).abs() < 1e-9, "{}", s.pos.y);
    }

    #[test]
    fn the_reel_gathers_speed_and_stops_when_let_go() {
        let mut w = world();
        ceiling(&mut w, 22);
        let mut s = standing(5.5, 10.0);
        hold(&mut s, fire(up()), 1, Some(&GRAPPLE), &w);
        hold(&mut s, up(), 20, Some(&GRAPPLE), &w);
        let reel_in = PlayerInput { reel: 1, ..up() };
        hold(&mut s, reel_in, 1, Some(&GRAPPLE), &w);
        let h = s.hook.unwrap();
        assert!(
            (h.reel - REEL_ACCEL * TICK_DT).abs() < 1e-12,
            "one tick of gathering: {}",
            h.reel
        );
        hold(&mut s, reel_in, 60, Some(&GRAPPLE), &w);
        assert_eq!(
            s.hook.unwrap().reel,
            GRAPPLE.reel_in_speed,
            "up to the grapple's reel speed"
        );
        hold(&mut s, up(), 1, Some(&GRAPPLE), &w);
        assert_eq!(s.hook.unwrap().reel, 0.0);
    }

    #[test]
    fn nobody_moves_faster_than_the_cap() {
        let mut w = world();
        w.floor = -1000;
        let cfg = MoveConfig::default();
        let mut s = PlayerState {
            pos: DVec3::new(5.5, 500.0, 5.5),
            vel: DVec3::new(60.0, 0.0, 0.0),
            ..Default::default()
        };
        for _ in 0..600 {
            hold(&mut s, PlayerInput::default(), 1, None, &w);
            assert!(s.vel.length() <= cfg.max_speed + 1e-9, "{:?}", s.vel);
        }
        assert!(
            (s.vel.y + cfg.max_fall_speed).abs() < 1.0,
            "falls at about the terminal speed: {:?}",
            s.vel
        );
    }

    #[test]
    fn a_hookshot_reels_by_itself() {
        let mut w = world();
        ceiling(&mut w, 22);
        let shot = GrappleSpec {
            hookshot: true,
            reel_in_speed: 16.0,
            ..GRAPPLE
        };
        let mut s = standing(5.5, 10.0);
        hold(&mut s, fire(up()), 1, Some(&shot), &w);
        hold(&mut s, up(), 90, Some(&shot), &w);
        let (anchor, rope) = s.anchor().expect("stuck");
        assert_eq!(rope, ROPE_MIN);
        assert!(
            (anchor.y - s.hands().y) < ROPE_MIN + 0.3,
            "pulled up to the hook: {}",
            s.pos.y
        );
    }

    #[test]
    fn a_swing_keeps_to_the_rope() {
        let mut w = world();
        w.floor = -100;
        w.solid.insert(BlockPos::new(5, 40, 5));
        // Hanging from a hook in the block's underside, released from the
        // side with the rope taut: it swings, never falls away.
        let anchor = DVec3::new(5.5, 40.0, 5.5);
        let mut s = PlayerState {
            pos: DVec3::new(15.5, 40.0 - ROPE_HEIGHT, 5.5),
            hook: Some(Hook {
                pos: anchor,
                vel: DVec3::ZERO,
                rope: 10.0,
                stuck: Some(BlockPos::new(5, 40, 5)),
                reel: 0.0,
            }),
            ..Default::default()
        };
        let mut lowest = f64::MAX;
        let mut far_side = f64::MAX;
        for _ in 0..240 {
            hold(&mut s, PlayerInput::default(), 1, Some(&GRAPPLE), &w);
            let d = planet().delta(anchor, s.hands()).length();
            assert!(d < 10.0 + 0.05, "the rope never stretches: {d}");
            lowest = lowest.min(s.pos.y);
            far_side = far_side.min(s.pos.x);
        }
        assert!(
            lowest > 40.0 - ROPE_HEIGHT - 10.0 - 0.05,
            "never below the rope: {lowest}"
        );
        assert!(far_side < 5.5 - 5.0, "swings through to the other side: {far_side}");
    }

    #[test]
    fn breaking_the_block_lets_go() {
        let mut w = world();
        ceiling(&mut w, 22);
        let mut s = standing(5.5, 10.0);
        hold(&mut s, fire(up()), 1, Some(&GRAPPLE), &w);
        hold(&mut s, up(), 20, Some(&GRAPPLE), &w);
        let block = s.hook.and_then(|h| h.stuck).expect("stuck");
        w.solid.remove(&block);
        hold(&mut s, up(), 1, Some(&GRAPPLE), &w);
        assert_eq!(s.hook, None);
    }

    #[test]
    fn letting_go_mid_swing_flings() {
        let mut w = world();
        w.floor = -100;
        let cfg = MoveConfig::default();
        let mut s = PlayerState {
            pos: DVec3::new(5.5, 50.0, 5.5),
            vel: DVec3::new(18.0, 4.0, 0.0),
            ..Default::default()
        };
        hold(&mut s, PlayerInput::default(), 30, Some(&GRAPPLE), &w);
        assert!(
            s.vel.x > 14.0,
            "half a second later the fling still carries: {}",
            s.vel.x
        );
        // An ordinary jump still stops when the keys are let go.
        let mut j = standing(5.5, 10.0);
        j.vel.x = cfg.walk_speed;
        j.on_ground = false;
        j.pos.y = 30.0;
        hold(&mut j, PlayerInput::default(), 30, None, &w);
        assert!(j.vel.x.abs() < 1e-9, "{}", j.vel.x);
    }

    #[test]
    fn grappling_gives_bit_identical_results() {
        let mut w = world();
        ceiling(&mut w, 30);
        let inputs: Vec<PlayerInput> = (0..900u32)
            .map(|i| PlayerInput {
                forward: (i / 50 % 3) as i8 - 1,
                strafe: (i / 70 % 3) as i8 - 1,
                fire: i % 150 == 0,
                reel: (i / 40 % 3) as i8 - 1,
                yaw: (i * 311) as u16,
                pitch: 9000 + (i % 7000) as i16,
                ..Default::default()
            })
            .collect();
        let cfg = MoveConfig::default();
        let simulate = || {
            let mut s = standing(100.5, 10.0);
            let mut trace = Vec::new();
            for inp in &inputs {
                step(&mut s, inp, &cfg, Some(&GRAPPLE), planet(), &w);
                let h = s.hook.map(|h| (h.pos.x.to_bits(), h.pos.y.to_bits(), h.rope.to_bits()));
                trace.push((s.pos.x.to_bits(), s.pos.y.to_bits(), s.pos.z.to_bits(), h));
            }
            trace
        };
        let a = simulate();
        assert!(a.iter().filter(|t| t.3.is_some()).count() > 100, "the test grapples");
        assert_eq!(a, simulate());
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
