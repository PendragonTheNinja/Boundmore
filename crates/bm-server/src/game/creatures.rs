//! Creatures in play (milestone 13, `docs/milestones/13-creatures-and-combat.md`):
//! spawning around players, despawning when nobody is near, AI by
//! behaviour, physics by `movement::step` (the players' own), threat lists,
//! and a short position history so hits are judged where the shooter saw
//! the creature. Creatures are never saved. The catalogue is
//! `bm_core::creatures`; combat itself (shots, strikes, damage, loot, defeat)
//! is `combat.rs`.

use super::Game;
use super::mining::roll;
use crate::world::World;
use bm_core::combat::DamageKind;
use bm_core::creatures::{
    Attack, Behaviour, CreatureDef, CreatureKinds, Exotic, PROTECTED_BELOW_LEVEL, hostile_world, roll_exotic, tiers_on,
};
use bm_core::movement::{self, MoveConfig, PhysicsWorld, PlayerInput, PlayerState, TICK_HZ};
use bm_core::raycast::raycast;
use bm_core::{BlockPos, PlanetShape};
use bm_protocol::{CreatureView, Pose, RemoteCreature, ServerMsg};
use glam::DVec3;
use std::collections::HashMap;

/// Positions kept per creature, one a tick: enough for the longest rewind.
pub(super) const HISTORY: usize = 16;

/// How far back hits are judged: the client draws creatures this many ticks
/// behind the newest snapshot (its interpolation delay).
pub(super) const REWIND_TICKS: u32 = 7;

/// The most a hit is ever rewound (200 ms, the spec's limit): a client
/// whose inputs arrive late gets this much and no more.
pub(super) const MAX_REWIND_TICKS: u32 = 12;

/// Spawns land this far from the player they spawn around, at least and at
/// most, in blocks (horizontal). A **guess**: out of sight, inside the view.
pub(super) const SPAWN_MIN: f64 = 24.0;
pub(super) const SPAWN_MAX: f64 = 48.0;

/// Creatures counted around a player when deciding whether to spawn more:
/// those within this many blocks.
pub(super) const CROWD_RANGE: f64 = 48.0;

/// Each player is considered for a spawn this often (once a second,
/// staggered by player id).
const SPAWN_EVERY: u64 = 60;

/// Tries for a spot per spawn pass.
const SPAWN_TRIES: usize = 4;

/// A creature with no player within this many blocks for [`LONELY_TICKS`]
/// vanishes. A **guess**.
pub(super) const DESPAWN_RANGE: f64 = 64.0;
pub(super) const LONELY_TICKS: u64 = 10 * TICK_HZ as u64;

/// A killed creature's 32 by 32 column spawns nothing for this long (5
/// minutes, a **guess**: Boundless kept a killed spot empty "for a while").
pub(super) const EMPTY_AFTER_KILL_TICKS: u64 = 5 * 60 * TICK_HZ as u64;

/// A body stays this long for players to take their drops (60 s, a **guess**).
pub(super) const BODY_TICKS: u64 = 60 * TICK_HZ as u64;

/// Creatures a player is told about: within this many blocks, nearest
/// first, at most [`MAX_SEEN_CREATURES`]. A **guess**: beyond where they
/// spawn and vanish.
pub(super) const VIEW_RANGE: f64 = 80.0;
pub(super) const MAX_SEEN_CREATURES: usize = 160;

/// Calm creatures look about for players this often (staggered by id).
const NOTICE_EVERY: u64 = 15;
/// Line of sight to the target is checked this often (staggered by id).
const SIGHT_EVERY: u64 = 10;

/// A wandering creature walks to a point this far away, at most.
const WANDER_RADIUS: f64 = 10.0;

/// Between charges of one run-up (Hardy and up charge more than once), the
/// creature pauses this long to turn. A **guess**.
const CHAIN_PAUSE_TICKS: u64 = 18;
/// Charges in one run-up by creature tier: once at tier 1 (Boundless), then
/// one more a tier up to three. A **guess** past tier 1.
pub(super) fn charges_per_run(tier: u8) -> u8 {
    tier.clamp(1, 3)
}
/// A charge that gets nowhere (blocked) ends after this long.
const CHARGE_MAX_TICKS: u16 = 3 * TICK_HZ as u16;

/// Upward speed a charge's knockback adds, blocks a second. A **guess**.
pub(super) const KNOCK_UP: f64 = 5.0;

/// A ranged creature keeps this far from its target, between the two, in
/// blocks. A **guess**.
const KEEP_NEAR: f64 = 7.0;
const KEEP_FAR: f64 = 14.0;
/// It aims this long before a volley (Pose::Windup). A **guess**.
const AIM_TICKS: u64 = 24;
/// It switches strafing direction this often. A **guess**.
const STRAFE_SWITCH_TICKS: u64 = 100;
/// Pose::Attack is shown this long after a charge hit or a volley.
const ATTACK_POSE_TICKS: u64 = 15;

/// Creature shots fall like a sling's with this fall-off. A **guess**.
pub(super) const CREATURE_SHOT_FALLOFF: f32 = 0.5;

// Milestone 13 part 2: the popper and the drifter. Every number here is a
// **guess** (Boundless never published them); `docs/feel.md` lists them.

/// A hammer strike stuns a popper this long (2 s, a **guess**): it neither
/// moves nor burns its fuse.
pub(super) const HAMMER_STUN_TICKS: u64 = 2 * TICK_HZ as u64;

/// Once its whistle ends, a popper leaps at a target within this many
/// blocks (a **guess**).
pub(super) const LEAP_RANGE: f64 = 5.0;
/// A leap's upward speed, blocks a second (a **guess**: about 0.7 blocks
/// high, 0.46 s in the air).
pub(super) const LEAP_UP: f64 = 6.0;
/// A leap's horizontal speed: what reaches the target, within these bounds
/// (blocks a second; **guesses**).
pub(super) const LEAP_MIN_SPEED: f64 = 4.0;
pub(super) const LEAP_MAX_SPEED: f64 = 14.0;
/// A leap that has not touched anything explodes after this long.
const LEAP_MAX_TICKS: u16 = 2 * TICK_HZ as u16;
/// A whistling popper that cannot reach its target explodes where it is
/// this long after its whistle ends (4 s, a **guess**).
pub(super) const CHASE_TIMEOUT_TICKS: u64 = 4 * TICK_HZ as u64;
/// A leaping popper touches a player when its body comes within this much
/// of theirs (blocks, a **guess**).
const CONTACT_MARGIN: f64 = 0.3;
/// A curled popper rolls: ground slows it at this rate (blocks a second
/// squared, against a walker's 55; a **guess**).
const CURL_ROLL_ACCEL: f64 = 4.0;

/// A drifter circles its target this far out, horizontally (a **guess**).
pub(super) const ORBIT_RADIUS: f64 = 6.0;
/// How fast it circles, radians a second (a **guess**).
const ORBIT_TURN: f64 = 0.4;
/// It slows as it nears where it is going: at most this many times the
/// distance left, a second.
const FLY_EASE: f64 = 3.0;
/// A calm drifter in the air sinks to the ground at this speed.
const LAND_SPEED: f64 = 3.0;
/// A beam lasts 2 s and hits every half second while its target stays in
/// sight (the half second is Boundless's; 2 s is a **guess**).
pub(super) const BEAM_TICKS: u64 = 2 * TICK_HZ as u64;
pub(super) const BEAM_HIT_TICKS: u64 = TICK_HZ as u64 / 2;
/// A lift marks where its target stands, and lifts them if they are still
/// within [`LIFT_RADIUS`] blocks of the mark this long after (1 s).
pub(super) const LIFT_DELAY_TICKS: u64 = TICK_HZ as u64;
pub(super) const LIFT_RADIUS: f64 = 2.0;
/// A lift throws its target this high (blocks, a **guess**: they fall
/// back and take fall damage as from any fall).
pub(super) const LIFT_HEIGHT: f64 = 15.0;

/// What a drifter's volley is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Volley {
    Bolt,
    Bomb,
    Homing,
    Beam,
}

/// The volleys a drifter of `tier` with `share` of its health left takes
/// turns with: bolts always, bombs below `bomb_below`, homing bolts below
/// `homing_below`, the beam from `beam_tier` (Boundless: attacks unlock as
/// its health drops).
pub(super) fn barrage_options(attack: &Attack, tier: u8, share: f64) -> Vec<Volley> {
    let mut v = vec![Volley::Bolt];
    if let Attack::Barrage {
        bomb_below,
        homing_below,
        beam_tier,
        ..
    } = *attack
    {
        if share < bomb_below as f64 {
            v.push(Volley::Bomb);
        }
        if share < homing_below as f64 {
            v.push(Volley::Homing);
        }
        if tier >= beam_tier {
            v.push(Volley::Beam);
        }
    }
    v
}

/// A drifter's beam under way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BeamOn {
    pub target: u32,
    pub until: u64,
    pub next_hit: u64,
}

/// A drifter's lift: the spot it marked under `target`, and when it lifts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LiftMark {
    pub target: u32,
    pub spot: DVec3,
    pub at: u64,
}

/// What the AI decided this tick, applied by the game after every creature
/// of the planet has moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Act {
    /// A charge reached a player: hit them, knocking them along `dir`.
    Charge { creature: u32, target: u32, dir: DVec3 },
    /// Fire a volley at the player.
    Volley { creature: u32, target: u32 },
    /// Landed hard: this much of its health.
    Fall { creature: u32, share: f64 },
    /// A popper blows up where it is.
    Explode { creature: u32 },
    /// A drifter fires a bolt, a bomb or a homing bolt at the player.
    Fire { creature: u32, target: u32, volley: Volley },
    /// A drifter's beam starts on the player.
    Beam { creature: u32, target: u32 },
    /// A beam under way hits the player (in sight).
    BeamHit { creature: u32, target: u32 },
    /// A lift throws the player up.
    Lift { creature: u32, target: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Fight {
    /// Getting into position (approaching, strafing, cooling down).
    Close,
    /// Rearing or aiming until `until`; `left` charges remain after the next.
    Windup { until: u64, left: u8 },
    /// Running along `dir` from `from`.
    Charge {
        dir: DVec3,
        from: DVec3,
        left: u8,
        ticks: u16,
    },
    /// A popper whistling: `left` ticks of fuse to burn, lit at `since`.
    Lit { left: u32, since: u64 },
    /// A popper leaping at `speed` blocks a second, `ticks` in.
    Leap { ticks: u16, speed: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Mind {
    Calm,
    Flee { from: u32 },
    Angry { target: u32, fight: Fight },
}

pub(super) struct Creature {
    pub id: u32,
    pub kind: u16,
    pub tier: u8,
    pub state: PlayerState,
    pub yaw: u16,
    /// Health lost.
    pub damage: u32,
    pub mind: Mind,
    /// Who hurt it and how much (damage x `Effects::threat`): it attacks
    /// the top one.
    pub threat: Vec<(u32, f64)>,
    /// Everyone who damaged it: each gets the kill's XP.
    pub hurt_by: Vec<u32>,
    /// Feet position at the end of each tick, by tick modulo [`HISTORY`].
    pub history: [DVec3; HISTORY],
    pub born: u64,
    pub pose: Pose,
    /// The tick it died: a body until [`BODY_TICKS`] later.
    pub dead_at: Option<u64>,
    /// Accounts that have taken their drops from the body.
    pub looted: Vec<String>,
    /// Since when no player has been within [`DESPAWN_RANGE`].
    pub lonely_since: Option<u64>,
    /// Where it is wandering, and until when it wanders or idles.
    pub wander: Option<DVec3>,
    pub wander_until: u64,
    /// The last tick it saw its target, and whether it sees it now.
    pub seen_at: u64,
    pub sees: bool,
    /// No attack starts before this tick (the attack's cooldown).
    pub next_attack: u64,
    /// Pose::Attack until this tick.
    pub attack_until: u64,
    /// Its exotic buffs (milestone 13 part 2), rolled at spawn.
    pub exotic: Exotic,
    /// A popper: whether it has had its one chance to curl, and until when
    /// it is curled (cannot be hurt) or stunned (by a hammer).
    pub curl_tried: bool,
    pub curled_until: u64,
    pub stunned_until: u64,
    /// A drifter: in the air, and where it is flying to.
    pub flying: bool,
    pub fly_to: Option<DVec3>,
    /// Volleys fired, to take turns among those unlocked.
    pub volleys: u32,
    pub beam: Option<BeamOn>,
    pub lift: Option<LiftMark>,
    /// No lift before this tick.
    pub next_lift: u64,
    /// The meteor whose wave it belongs to (milestone 14): it stays while
    /// the meteor fights, and does not count toward the planet's cap.
    pub meteor: Option<u32>,
}

impl Creature {
    pub(super) fn new(id: u32, kind: u16, tier: u8, pos: DVec3, tick: u64) -> Self {
        Self {
            id,
            kind,
            tier,
            state: PlayerState {
                pos,
                // No grace from a grapple it never held.
                loose: u16::MAX,
                ..Default::default()
            },
            yaw: (id.wrapping_mul(40503) & 0xffff) as u16,
            damage: 0,
            mind: Mind::Calm,
            threat: Vec::new(),
            hurt_by: Vec::new(),
            history: [pos; HISTORY],
            born: tick,
            pose: Pose::Idle,
            dead_at: None,
            looted: Vec::new(),
            lonely_since: None,
            wander: None,
            wander_until: tick + (id as u64 % 120),
            seen_at: tick,
            sees: false,
            next_attack: 0,
            attack_until: 0,
            exotic: Exotic::default(),
            curl_tried: false,
            curled_until: 0,
            stunned_until: 0,
            flying: false,
            fly_to: None,
            volleys: 0,
            beam: None,
            lift: None,
            next_lift: 0,
            meteor: None,
        }
    }

    pub(super) fn alive(&self) -> bool {
        self.dead_at.is_none()
    }

    /// Its health at full: its tier's, tripled by the exotic health buff.
    pub(super) fn max_health(&self, def: &CreatureDef) -> u32 {
        self.exotic.health(def.tier(self.tier).health)
    }

    pub(super) fn armour(&self, def: &CreatureDef) -> u32 {
        self.exotic.armour(def.tier(self.tier).armour)
    }

    /// Damage one of its hits deals before the player's armour.
    pub(super) fn hit_damage(&self, def: &CreatureDef) -> u32 {
        self.exotic.damage(def.tier(self.tier).damage)
    }

    /// XP its kill gives each attacker.
    pub(super) fn xp(&self, def: &CreatureDef) -> u32 {
        self.exotic.xp(def.tier(self.tier).xp)
    }

    /// Share of its health left, 0 to 1.
    pub(super) fn health_share(&self, def: &CreatureDef) -> f64 {
        let max = self.max_health(def).max(1) as f64;
        (1.0 - self.damage as f64 / max).max(0.0)
    }

    pub(super) fn curled(&self, tick: u64) -> bool {
        tick < self.curled_until
    }

    pub(super) fn stunned(&self, tick: u64) -> bool {
        tick < self.stunned_until
    }

    /// A player's hit of `amount` (after armour) lands at `tick`: what it
    /// takes. A curled popper takes nothing. The first hit on a popper curls
    /// it with its chance (`roll` in 0..1), from then on.
    pub(super) fn struck(&mut self, def: &CreatureDef, tick: u64, roll: f64, amount: u32) -> u32 {
        let amount = if self.curled(tick) { 0 } else { amount };
        if let Attack::Explode { curl, curl_chance, .. } = def.attack
            && !self.curl_tried
        {
            self.curl_tried = true;
            if roll < curl_chance as f64 {
                self.curled_until = tick + (curl as f64 * TICK_HZ as f64).round() as u64;
            }
        }
        amount
    }

    /// A hammer strike at `tick`: a popper is stunned for
    /// [`HAMMER_STUN_TICKS`].
    pub(super) fn hammered(&mut self, def: &CreatureDef, tick: u64) {
        if def.behaviour == Behaviour::Exploder {
            self.stunned_until = tick + HAMMER_STUN_TICKS;
        }
    }

    pub(super) fn angry(&self) -> bool {
        matches!(self.mind, Mind::Angry { .. })
    }

    /// Where it stood `ticks` ago (at most [`HISTORY`] - 1; never before it
    /// was born).
    pub(super) fn rewound(&self, now: u64, ticks: u32) -> DVec3 {
        let ticks = (ticks as u64)
            .min(HISTORY as u64 - 1)
            .min(now.saturating_sub(self.born));
        self.history[((now - ticks) % HISTORY as u64) as usize]
    }

    /// Whether it stays at `tick`: a body until its time is up, a living
    /// creature until nobody has been within [`DESPAWN_RANGE`] for
    /// [`LONELY_TICKS`].
    pub(super) fn stays(&mut self, seen: &[Seen], shape: PlanetShape, tick: u64) -> bool {
        if self.dead_at.is_some_and(|t| tick >= t + BODY_TICKS) {
            return false;
        }
        // Its meteor takes it away when it goes dormant.
        if self.meteor.is_some() && self.alive() {
            return true;
        }
        let near = seen
            .iter()
            .any(|p| horizontal(shape.delta(self.state.pos, p.pos)).length() <= DESPAWN_RANGE);
        if near {
            self.lonely_since = None;
            return true;
        }
        let since = *self.lonely_since.get_or_insert(tick);
        tick < since + LONELY_TICKS
    }

    /// Marks the body looted by `account`: false if it is not a body, or
    /// they already took their drops (one roll each).
    pub(super) fn take_loot(&mut self, account: &str) -> bool {
        if self.alive() || self.looted.iter().any(|a| a == account) {
            return false;
        }
        self.looted.push(account.to_string());
        true
    }

    /// Adds threat from player `id`.
    pub(super) fn add_threat(&mut self, id: u32, amount: f64) {
        match self.threat.iter_mut().find(|(p, _)| *p == id) {
            Some((_, t)) => *t += amount,
            None => self.threat.push((id, amount)),
        }
    }

    pub(super) fn forget(&mut self, id: u32) {
        self.threat.retain(|(p, _)| *p != id);
        if let Mind::Angry { target, .. } | Mind::Flee { from: target } = self.mind
            && target == id
        {
            self.mind = Mind::Calm;
        }
    }

    /// The player with the most threat among those `valid` allows.
    pub(super) fn top_threat(&self, valid: impl Fn(u32) -> bool) -> Option<u32> {
        self.threat
            .iter()
            .filter(|(p, _)| valid(*p))
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|(p, _)| *p)
    }

    /// What the player `account` sees of it.
    pub(super) fn view(&self, account: &str) -> CreatureView {
        CreatureView {
            id: self.id,
            kind: self.kind,
            tier: self.tier,
            damage: self.damage,
            angry: self.angry() && self.alive(),
            dead: !self.alive(),
            looted: self.looted.iter().any(|a| a == account),
            exotic: self.exotic.0,
        }
    }

    pub(super) fn remote(&self) -> RemoteCreature {
        let p = self.state.pos;
        RemoteCreature {
            id: self.id,
            pos: [p.x as f32, p.y as f32, p.z as f32],
            yaw: self.yaw,
            pose: self.pose,
        }
    }
}

/// The creatures of one planet.
#[derive(Default)]
pub(super) struct Herds {
    pub list: Vec<Creature>,
    /// Columns (32 by 32 blocks) where a creature was killed: nothing spawns
    /// there until the tick given.
    pub empty: HashMap<(i64, i64), u64>,
}

impl Herds {
    pub(super) fn get(&self, id: u32) -> Option<&Creature> {
        self.list.iter().find(|c| c.id == id)
    }

    pub(super) fn get_mut(&mut self, id: u32) -> Option<&mut Creature> {
        self.list.iter_mut().find(|c| c.id == id)
    }

    /// Whether a spawn may land in the column holding `p` at `tick`.
    pub(super) fn column_open(&self, p: BlockPos, tick: u64) -> bool {
        self.empty.get(&column(p)).is_none_or(|&until| tick >= until)
    }
}

/// What a block is to a creature looking for somewhere to stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ground {
    /// Collides: something to stand on.
    Solid,
    Water,
    /// Air, or what does not collide (a plant, a prop): room.
    Open,
}

/// What creatures ask of a planet's blocks: physics, plus what a resident
/// block is (None: not loaded). `World` answers on the server; tests use a
/// small world of their own.
pub(super) trait Terrain: PhysicsWorld {
    fn ground(&self, p: BlockPos) -> Option<Ground>;
    fn planet(&self) -> PlanetShape;
}

impl Terrain for World {
    fn ground(&self, p: BlockPos) -> Option<Ground> {
        let b = self.block(p)?;
        Some(if b == self.water {
            Ground::Water
        } else if self.flags.solid(b) {
            Ground::Solid
        } else {
            Ground::Open
        })
    }

    fn planet(&self) -> PlanetShape {
        self.shape
    }
}

/// The 32 by 32 column holding a block (a chunk column).
pub(super) fn column(p: BlockPos) -> (i64, i64) {
    (p.x >> 5, p.z >> 5)
}

/// A player as creatures see them this tick.
#[derive(Clone, Copy, Debug)]
pub(super) struct Seen {
    pub id: u32,
    pub pos: DVec3,
    pub eye: DVec3,
    pub down: bool,
    /// New-player protection: creatures leave them alone unless attacked.
    pub protected: bool,
    /// How far creatures notice them, as a multiple (Quiet Step).
    pub noticed_at: f64,
}

/// The yaw facing horizontally along `d`.
pub(super) fn yaw_toward(d: DVec3) -> u16 {
    PlayerInput::from_angles(d.x.atan2(-d.z), 0.0).0
}

/// Whether nothing solid lies between two points.
pub(super) fn clear_sight(world: &impl PhysicsWorld, from: DVec3, to: DVec3) -> bool {
    let d = to - from;
    let len = d.length();
    len < 1e-6 || raycast(from, d, len, |p| world.solid(p)).is_none()
}

/// A spot a body of `height` can stand at in the column holding `x, z`: a
/// solid block with room above it (air or what does not collide, such as a
/// plant; never water), all resident, searching from `top` down to
/// `bottom`. Returns the feet position.
pub(super) fn surface(world: &impl Terrain, x: i64, z: i64, top: i64, bottom: i64, height: f64) -> Option<DVec3> {
    let clear = height.ceil().max(1.0) as i64;
    let mut room = 0;
    for y in (bottom..=top).rev() {
        match world.ground(BlockPos::new(x, y, z))? {
            Ground::Water => room = 0,
            Ground::Open => room += 1,
            Ground::Solid if room >= clear => {
                let feet = DVec3::new(x as f64 + 0.5, y as f64 + 1.0, z as f64 + 0.5);
                return Some(world.planet().wrap_point(feet));
            }
            Ground::Solid => room = 0,
        }
    }
    None
}

/// The point of the box `lo..hi` nearest `p`.
fn nearest_in_box(p: DVec3, lo: DVec3, hi: DVec3) -> DVec3 {
    p.clamp(lo, hi)
}

/// Whether a ball of `radius` at `centre` touches a player's body standing
/// at `feet` (both on the same lap).
pub(super) fn ball_hits_body(centre: DVec3, radius: f64, feet: DVec3, m: &MoveConfig) -> bool {
    let lo = DVec3::new(feet.x - m.half_width, feet.y, feet.z - m.half_width);
    let hi = DVec3::new(feet.x + m.half_width, feet.y + m.height, feet.z + m.half_width);
    nearest_in_box(centre, lo, hi).distance(centre) <= radius
}

/// Where the segment `from + t (to - from)`, t in 0..1, first enters the
/// box `lo..hi`, if it does (a slab test).
pub(super) fn segment_hits_box(from: DVec3, to: DVec3, lo: DVec3, hi: DVec3) -> Option<f64> {
    let d = to - from;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for a in 0..3 {
        if d[a].abs() < 1e-12 {
            if from[a] < lo[a] || from[a] > hi[a] {
                return None;
            }
            continue;
        }
        let (mut near, mut far) = ((lo[a] - from[a]) / d[a], (hi[a] - from[a]) / d[a]);
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        t0 = t0.max(near);
        t1 = t1.min(far);
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// The box of a creature of `def` standing at `feet`.
pub(super) fn creature_box(def: &CreatureDef, feet: DVec3) -> (DVec3, DVec3) {
    let w = def.half_width as f64;
    (
        DVec3::new(feet.x - w, feet.y, feet.z - w),
        DVec3::new(feet.x + w, feet.y + def.height as f64, feet.z + w),
    )
}

/// How many more creatures may spawn around a player with `near` already
/// near them (cap `near_cap`), on a planet holding `on_planet` (cap
/// `planet_cap`): a group never takes either past its cap.
pub(super) fn spawn_room(near: usize, near_cap: usize, on_planet: usize, planet_cap: usize) -> usize {
    near_cap.saturating_sub(near).min(planet_cap.saturating_sub(on_planet))
}

/// Picks a kind for a spawn on a world of `tier`, by spawn weight among the
/// kinds that live there; `r` in 0..1.
pub(super) fn pick_kind(kinds: &CreatureKinds, world_tier: u8, r: f64) -> Option<u16> {
    let ok = |d: &CreatureDef| d.min_world_tier <= world_tier && d.spawn_weight > 0;
    let total: u32 = kinds
        .defs()
        .iter()
        .filter(|d| ok(d))
        .map(|d| d.spawn_weight as u32)
        .sum();
    if total == 0 {
        return None;
    }
    let mut pick = (r * total as f64) as u32;
    for (id, d) in kinds.iter().filter(|(_, d)| ok(d)) {
        if pick < d.spawn_weight as u32 {
            return Some(id);
        }
        pick -= d.spawn_weight as u32;
    }
    None
}

/// A creature tier for a spawn of `def` on a world of `world_tier`:
/// uniformly from the tiers the world holds, capped at the kind's top.
pub(super) fn pick_tier(def: &CreatureDef, world_tier: u8, r: f64) -> u8 {
    let range = tiers_on(world_tier);
    let top = (*range.end()).min(def.top_tier()).max(1);
    let n = top as f64;
    1 + ((r * n) as u8).min(top - 1)
}

/// The planet a creature lives on, for its AI: who is there and what the
/// world is like.
pub(super) struct Ctx<'a, W> {
    pub world: &'a W,
    pub shape: PlanetShape,
    pub seen: &'a [Seen],
    pub tick: u64,
    pub hostile: bool,
    /// Players' bodies (for a charge reaching one).
    pub body: MoveConfig,
}

impl<W> Ctx<'_, W> {
    fn player(&self, id: u32) -> Option<&Seen> {
        self.seen.iter().find(|s| s.id == id)
    }

    /// Horizontal and full offsets from `from` to `to` across the seam.
    fn delta(&self, from: DVec3, to: DVec3) -> DVec3 {
        self.shape.delta(from, to)
    }
}

fn horizontal(d: DVec3) -> DVec3 {
    DVec3::new(d.x, 0.0, d.z)
}

/// One tick of thinking for a living creature: its mind may change, and it
/// says how it moves (an input for `movement::step`) and what it does.
pub(super) fn think(
    c: &mut Creature,
    def: &CreatureDef,
    ctx: &Ctx<impl Terrain>,
    rng: &mut u64,
    acts: &mut Vec<Act>,
) -> PlayerInput {
    let tick = ctx.tick;
    // Stunned or curled: no will of its own (a curled ball rolls on).
    if c.stunned(tick) || c.curled(tick) {
        return PlayerInput {
            yaw: c.yaw,
            ..Default::default()
        };
    }
    let mut input = think_inner(c, def, ctx, rng, acts);
    // A popper hops wherever it goes.
    if def.behaviour == Behaviour::Exploder && input.forward != 0 && c.state.on_ground {
        input.jump = true;
    }
    input
}

fn think_inner(
    c: &mut Creature,
    def: &CreatureDef,
    ctx: &Ctx<impl Terrain>,
    rng: &mut u64,
    acts: &mut Vec<Act>,
) -> PlayerInput {
    let tick = ctx.tick;
    let id = c.id;
    // Calm: look about now and then.
    if matches!(c.mind, Mind::Calm) && (tick + id as u64) % NOTICE_EVERY == 0 {
        notice(c, def, ctx);
    }
    loop {
        match c.mind {
            // A drifter rests on the ground; in the air, it comes down.
            Mind::Calm if def.behaviour == Behaviour::Flyer => {
                c.beam = None;
                c.lift = None;
                c.fly_to = c.flying.then(|| c.state.pos - DVec3::new(0.0, 2.0, 0.0));
                return PlayerInput {
                    yaw: c.yaw,
                    ..Default::default()
                };
            }
            Mind::Calm => return wander(c, def, ctx, rng),
            Mind::Flee { from } => {
                let Some(p) = ctx.player(from).filter(|p| !p.down) else {
                    c.mind = Mind::Calm;
                    continue;
                };
                let d = horizontal(ctx.delta(c.state.pos, p.pos));
                if d.length() > def.give_up as f64 {
                    c.mind = Mind::Calm;
                    continue;
                }
                c.wander = None;
                return run(c, ctx, -d, true);
            }
            Mind::Angry { target, fight } => {
                // The top of the threat list, among players still here.
                let top = c.top_threat(|p| ctx.player(p).is_some_and(|s| !s.down));
                let Some(top) = top else {
                    c.threat.clear();
                    c.mind = Mind::Calm;
                    continue;
                };
                if top != target {
                    // A popper's fuse burns on whoever it turns to.
                    let fight = match fight {
                        Fight::Lit { .. } | Fight::Leap { .. } => fight,
                        _ => Fight::Close,
                    };
                    c.mind = Mind::Angry { target: top, fight };
                    c.seen_at = tick;
                    c.sees = true;
                    continue;
                }
                let p = *ctx.player(target).expect("checked");
                let d = ctx.delta(c.state.pos, p.pos);
                if (tick + id as u64) % SIGHT_EVERY == 0 {
                    let eye = c.state.pos + DVec3::new(0.0, def.height as f64 * 0.8, 0.0);
                    c.sees = clear_sight(ctx.world, eye, eye + ctx.delta(eye, p.eye));
                }
                if c.sees {
                    c.seen_at = tick;
                }
                let forget = (def.forget as f64 * TICK_HZ as f64) as u64;
                // A charge under way runs its course.
                let charging = matches!(fight, Fight::Charge { .. } | Fight::Leap { .. });
                if !charging && (horizontal(d).length() > def.give_up as f64 || tick > c.seen_at + forget) {
                    c.forget(target);
                    if !c.threat.is_empty() {
                        c.mind = Mind::Angry {
                            target: u32::MAX,
                            fight: Fight::Close,
                        };
                    }
                    continue;
                }
                return fight_tick(c, def, ctx, &p, d, fight, acts);
            }
        }
    }
}

/// A calm creature looks for players to react to.
fn notice(c: &mut Creature, def: &CreatureDef, ctx: &Ctx<impl Terrain>) {
    let nearest = ctx
        .seen
        .iter()
        .filter(|p| !p.down)
        .map(|p| (p, horizontal(ctx.delta(c.state.pos, p.pos)).length()))
        .filter(|(p, d)| *d <= def.notice as f64 * p.noticed_at)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    let Some((p, _)) = nearest else { return };
    match def.behaviour {
        Behaviour::Flee => c.mind = Mind::Flee { from: p.id },
        Behaviour::Ranged | Behaviour::Exploder | Behaviour::Flyer
            if ctx.hostile && !(p.protected && !c.threat.iter().any(|(t, _)| *t == p.id)) =>
        {
            let eye = c.state.pos + DVec3::new(0.0, def.height as f64 * 0.8, 0.0);
            if clear_sight(ctx.world, eye, eye + ctx.delta(eye, p.eye)) {
                c.add_threat(p.id, 1.0);
                c.mind = Mind::Angry {
                    target: p.id,
                    fight: Fight::Close,
                };
                c.seen_at = ctx.tick;
                c.sees = true;
            }
        }
        // Herds never start a fight; a ranged creature on a gentle world
        // only fights back.
        _ => {}
    }
}

/// Becomes angry at (or, if it flees, runs from) a player who hurt it or
/// fired near it.
pub(super) fn provoke(c: &mut Creature, def: &CreatureDef, by: u32, tick: u64) {
    if !c.alive() {
        return;
    }
    match def.behaviour {
        Behaviour::Flee => c.mind = Mind::Flee { from: by },
        _ => {
            if !c.angry() {
                c.mind = Mind::Angry {
                    target: by,
                    fight: Fight::Close,
                };
                c.seen_at = tick;
                c.sees = true;
            }
        }
    }
}

fn wander(c: &mut Creature, def: &CreatureDef, ctx: &Ctx<impl Terrain>, rng: &mut u64) -> PlayerInput {
    let tick = ctx.tick;
    if let Some(goal) = c.wander {
        let d = horizontal(ctx.delta(c.state.pos, goal));
        if d.length() < 0.8 || tick >= c.wander_until {
            c.wander = None;
            c.wander_until = tick + (2.0 + roll(rng) * 5.0) as u64 * TICK_HZ as u64;
        } else {
            return run(c, ctx, d, false);
        }
    }
    if tick >= c.wander_until {
        let a = roll(rng) * std::f64::consts::TAU;
        let r = 2.0 + roll(rng) * (WANDER_RADIUS - 2.0);
        c.wander = Some(
            ctx.shape
                .wrap_point(c.state.pos + DVec3::new(a.sin() * r, 0.0, a.cos() * r)),
        );
        c.wander_until = tick + (r / def.walk_speed as f64 * 2.0 * TICK_HZ as f64) as u64 + TICK_HZ as u64;
    }
    PlayerInput {
        yaw: c.yaw,
        ..Default::default()
    }
}

/// Moves along `d` (horizontal), running or walking, jumping when a block
/// is in the way or in water.
fn run(c: &mut Creature, ctx: &Ctx<impl Terrain>, d: DVec3, sprint: bool) -> PlayerInput {
    let d = horizontal(d);
    if d.length() < 1e-6 {
        return PlayerInput {
            yaw: c.yaw,
            ..Default::default()
        };
    }
    c.yaw = yaw_toward(d);
    PlayerInput {
        forward: 1,
        sprint,
        jump: blocked(c, ctx, d.normalize()),
        yaw: c.yaw,
        ..Default::default()
    }
}

/// Whether the way ahead is a step to jump (or it is swimming).
fn blocked(c: &Creature, ctx: &Ctx<impl Terrain>, dir: DVec3) -> bool {
    let p = c.state.pos;
    if ctx.world.liquid(BlockPos::containing(p.x, p.y + 0.4, p.z)) {
        return true;
    }
    if !c.state.on_ground {
        return false;
    }
    let ahead = p + dir * 0.9;
    ctx.world.solid(BlockPos::containing(ahead.x, p.y + 0.6, ahead.z))
}

/// One tick of fighting `p`, `d` away.
fn fight_tick(
    c: &mut Creature,
    def: &CreatureDef,
    ctx: &Ctx<impl Terrain>,
    p: &Seen,
    d: DVec3,
    fight: Fight,
    acts: &mut Vec<Act>,
) -> PlayerInput {
    let tick = ctx.tick;
    let flat = horizontal(d);
    let dist = flat.length();
    let face = |c: &mut Creature| {
        if dist > 1e-6 {
            c.yaw = yaw_toward(flat);
        }
        PlayerInput {
            yaw: c.yaw,
            ..Default::default()
        }
    };
    let set = |c: &mut Creature, fight: Fight| {
        c.mind = Mind::Angry { target: p.id, fight };
    };
    match def.attack {
        Attack::Charge {
            range, radius, windup, ..
        } => match fight {
            Fight::Close | Fight::Lit { .. } | Fight::Leap { .. } => {
                if tick >= c.next_attack && c.sees && dist <= range as f64 * 0.7 && c.state.on_ground {
                    let until = tick + (windup as f64 * TICK_HZ as f64) as u64;
                    set(
                        c,
                        Fight::Windup {
                            until,
                            left: charges_per_run(c.tier) - 1,
                        },
                    );
                    return face(c);
                }
                if dist > 2.5 {
                    // Close in, walking while the charge cools down.
                    run(c, ctx, flat, tick >= c.next_attack)
                } else {
                    face(c)
                }
            }
            Fight::Windup { until, left } => {
                if tick < until {
                    return face(c);
                }
                let dir = if dist > 1e-6 {
                    flat / dist
                } else {
                    movement::heading(c.yaw).0
                };
                set(
                    c,
                    Fight::Charge {
                        dir,
                        from: c.state.pos,
                        left,
                        ticks: 0,
                    },
                );
                c.yaw = yaw_toward(dir);
                charge_input(c, ctx, dir)
            }
            Fight::Charge { dir, from, left, ticks } => {
                let head = c.state.pos + dir * def.half_width as f64 + DVec3::new(0.0, def.height as f64 * 0.5, 0.0);
                let feet = head + ctx.delta(head, p.pos);
                let gone = horizontal(ctx.delta(from, c.state.pos)).length();
                let hit = ball_hits_body(head, radius as f64, feet, &ctx.body);
                let over = hit || gone >= range as f64 || ticks >= CHARGE_MAX_TICKS
                    // Stopped by something.
                    || (ticks > 20 && horizontal(c.state.vel).length() < 1.0);
                if hit {
                    acts.push(Act::Charge {
                        creature: c.id,
                        target: p.id,
                        dir,
                    });
                    c.attack_until = tick + ATTACK_POSE_TICKS;
                }
                if !over {
                    set(
                        c,
                        Fight::Charge {
                            dir,
                            from,
                            left,
                            ticks: ticks + 1,
                        },
                    );
                    return charge_input(c, ctx, dir);
                }
                if left > 0 {
                    set(
                        c,
                        Fight::Windup {
                            until: tick + CHAIN_PAUSE_TICKS,
                            left: left - 1,
                        },
                    );
                } else {
                    c.next_attack = tick + attack_cooldown(def);
                    set(c, Fight::Close);
                }
                face(c)
            }
        },
        Attack::Shot { range, .. } => match fight {
            Fight::Windup { until, .. } => {
                if tick >= until {
                    acts.push(Act::Volley {
                        creature: c.id,
                        target: p.id,
                    });
                    c.attack_until = tick + ATTACK_POSE_TICKS;
                    c.next_attack = tick + attack_cooldown(def);
                    set(c, Fight::Close);
                }
                face(c)
            }
            _ => {
                if tick >= c.next_attack && c.sees && dist <= range as f64 {
                    set(
                        c,
                        Fight::Windup {
                            until: tick + AIM_TICKS,
                            left: 0,
                        },
                    );
                    return face(c);
                }
                if !c.sees || dist > KEEP_FAR {
                    return run(c, ctx, flat, true);
                }
                if dist < KEEP_NEAR {
                    let mut i = run(c, ctx, -flat, true);
                    // Backs off facing away; turns to aim when it fires.
                    i.forward = 1;
                    return i;
                }
                // Strafes around the target, facing it.
                let mut i = face(c);
                i.strafe = if ((tick + c.id as u64) / STRAFE_SWITCH_TICKS) % 2 == 0 {
                    1
                } else {
                    -1
                };
                i.sprint = true;
                let side = movement::heading(c.yaw).1 * i.strafe as f64;
                i.jump = blocked(c, ctx, side);
                i
            }
        },
        Attack::Explode { fuse, .. } => popper_tick(c, def, ctx, p, d, fight, fuse, acts),
        Attack::Barrage { .. } => flyer_tick(c, def, ctx, p, d, acts),
        Attack::None => {
            if dist > 3.0 {
                run(c, ctx, flat, true)
            } else {
                face(c)
            }
        }
    }
}

fn charge_input(c: &Creature, ctx: &Ctx<impl Terrain>, dir: DVec3) -> PlayerInput {
    PlayerInput {
        forward: 1,
        sprint: true,
        jump: blocked(c, ctx, dir),
        yaw: c.yaw,
        ..Default::default()
    }
}

fn attack_cooldown(def: &CreatureDef) -> u64 {
    let s = match def.attack {
        Attack::Charge { cooldown, .. } | Attack::Shot { cooldown, .. } => cooldown,
        Attack::Explode { fuse, .. } => fuse,
        Attack::Barrage { cooldown, .. } => cooldown,
        Attack::None => 1.0,
    };
    (s as f64 * TICK_HZ as f64) as u64
}

/// The movement settings a creature moves by this tick: a charge runs at
/// the charge's speed, a leap flies at its own, a curled ball rolls, and
/// the exotic speed buff speeds the rest.
pub(super) fn move_config(def: &CreatureDef, base: &MoveConfig, c: &Creature, tick: u64) -> MoveConfig {
    let mut m = match (&c.mind, def.attack) {
        (
            Mind::Angry {
                fight: Fight::Charge { .. },
                ..
            },
            Attack::Charge { speed, .. },
        ) => MoveConfig {
            sprint_speed: speed as f64,
            ..*base
        },
        (
            Mind::Angry {
                fight: Fight::Leap { speed, .. },
                ..
            },
            _,
        ) => {
            return MoveConfig {
                sprint_speed: *speed,
                ..*base
            };
        }
        _ => *base,
    };
    if c.curled(tick) {
        m.ground_accel = CURL_ROLL_ACCEL;
    }
    let k = c.exotic.speed();
    if k != 1.0 {
        m.walk_speed *= k;
        m.sprint_speed *= k;
        m.swim_speed *= k;
    }
    m
}

/// One tick of a whistling popper closing in on `p`, `d` away: it burns
/// its fuse (`fuse` seconds), then leaps and explodes on contact, on
/// landing, or where it is if it cannot reach them.
#[allow(clippy::too_many_arguments)]
fn popper_tick(
    c: &mut Creature,
    def: &CreatureDef,
    ctx: &Ctx<impl Terrain>,
    p: &Seen,
    d: DVec3,
    fight: Fight,
    fuse: f32,
    acts: &mut Vec<Act>,
) -> PlayerInput {
    let tick = ctx.tick;
    let fuse_ticks = (fuse as f64 * TICK_HZ as f64).round() as u32;
    let flat = horizontal(d);
    let set = |c: &mut Creature, fight: Fight| {
        c.mind = Mind::Angry { target: p.id, fight };
    };
    let idle = |c: &Creature| PlayerInput {
        yaw: c.yaw,
        ..Default::default()
    };
    let chase = |c: &mut Creature| {
        if flat.length() > 1.0 {
            run(c, ctx, flat, true)
        } else {
            if flat.length() > 1e-6 {
                c.yaw = yaw_toward(flat);
            }
            idle(c)
        }
    };
    match fight {
        Fight::Lit { left, since } if left > 0 => {
            set(c, Fight::Lit { left: left - 1, since });
            chase(c)
        }
        Fight::Lit { since, .. } => {
            if c.state.on_ground && d.length() <= LEAP_RANGE {
                let dir = if flat.length() > 1e-6 {
                    flat.normalize()
                } else {
                    movement::heading(c.yaw).0
                };
                let air = 2.0 * LEAP_UP / ctx.body.gravity;
                let speed = (flat.length() / air).clamp(LEAP_MIN_SPEED, LEAP_MAX_SPEED);
                c.state.vel = dir * speed + DVec3::new(0.0, LEAP_UP, 0.0);
                c.state.on_ground = false;
                c.yaw = yaw_toward(dir);
                set(c, Fight::Leap { ticks: 0, speed });
                return PlayerInput {
                    forward: 1,
                    sprint: true,
                    yaw: c.yaw,
                    ..Default::default()
                };
            }
            if tick >= since + fuse_ticks as u64 + CHASE_TIMEOUT_TICKS {
                acts.push(Act::Explode { creature: c.id });
                return idle(c);
            }
            chase(c)
        }
        Fight::Leap { ticks, speed } => {
            let centre = c.state.pos + DVec3::new(0.0, def.height as f64 * 0.5, 0.0);
            let feet = centre + ctx.delta(centre, p.pos);
            let r = def.half_width.max(def.height * 0.5) as f64 + CONTACT_MARGIN;
            let touched = ball_hits_body(centre, r, feet, &ctx.body);
            if touched || (ticks > 3 && c.state.on_ground) || ticks >= LEAP_MAX_TICKS {
                acts.push(Act::Explode { creature: c.id });
                return idle(c);
            }
            set(
                c,
                Fight::Leap {
                    ticks: ticks + 1,
                    speed,
                },
            );
            PlayerInput {
                forward: 1,
                sprint: true,
                yaw: c.yaw,
                ..Default::default()
            }
        }
        // Triggered: it lights its fuse and whistles.
        _ => {
            set(
                c,
                Fight::Lit {
                    left: fuse_ticks,
                    since: tick,
                },
            );
            chase(c)
        }
    }
}

/// One tick of an angry drifter fighting `p`, `d` away: in the air,
/// circling above them, firing its volleys in turn, beaming and lifting.
fn flyer_tick(
    c: &mut Creature,
    def: &CreatureDef,
    ctx: &Ctx<impl Terrain>,
    p: &Seen,
    d: DVec3,
    acts: &mut Vec<Act>,
) -> PlayerInput {
    let Attack::Barrage {
        range,
        cooldown,
        hover,
        lift_tier,
        lift_cooldown,
        ..
    } = def.attack
    else {
        return PlayerInput::default();
    };
    let tick = ctx.tick;
    c.flying = true;
    let a = (tick + c.id as u64 * 97) as f64 * ORBIT_TURN / TICK_HZ as f64;
    let at = c.state.pos + d;
    c.fly_to = Some(at + DVec3::new(a.cos() * ORBIT_RADIUS, hover as f64, a.sin() * ORBIT_RADIUS));
    let flat = horizontal(d);
    if flat.length() > 1e-6 {
        c.yaw = yaw_toward(flat);
    }
    let eye = c.state.pos + DVec3::new(0.0, def.height as f64 * 0.5, 0.0);
    let chest = |s: &Seen| s.pos + DVec3::new(0.0, ctx.body.height * 0.5, 0.0);
    // The beam, while it lasts.
    if let Some(mut b) = c.beam {
        if tick >= b.next_hit {
            b.next_hit += BEAM_HIT_TICKS;
            if let Some(t) = ctx.player(b.target).filter(|t| !t.down) {
                let to = chest(t);
                if clear_sight(ctx.world, eye, eye + ctx.delta(eye, to)) {
                    acts.push(Act::BeamHit {
                        creature: c.id,
                        target: t.id,
                    });
                }
            }
        }
        // Its last hit lands as it ends.
        c.beam = (b.next_hit <= b.until).then_some(b);
    }
    // The lift: still on the mark a second later, up they go.
    if let Some(l) = c.lift
        && tick >= l.at
    {
        c.lift = None;
        if let Some(t) = ctx.player(l.target).filter(|t| !t.down)
            && ctx.delta(l.spot, t.pos).length() <= LIFT_RADIUS
        {
            acts.push(Act::Lift {
                creature: c.id,
                target: t.id,
            });
        }
    }
    let dist = d.length();
    let lift_cd = (lift_cooldown as f64 * TICK_HZ as f64) as u64;
    if c.tier >= lift_tier && tick >= c.next_lift && c.lift.is_none() && c.sees && dist <= range as f64 {
        c.lift = Some(LiftMark {
            target: p.id,
            spot: p.pos,
            at: tick + LIFT_DELAY_TICKS,
        });
        c.next_lift = tick + lift_cd;
    }
    if c.beam.is_none() && tick >= c.next_attack && c.sees && dist <= range as f64 {
        let options = barrage_options(&def.attack, c.tier, c.health_share(def));
        let volley = options[c.volleys as usize % options.len()];
        c.volleys = c.volleys.wrapping_add(1);
        let mut wait = (cooldown as f64 * TICK_HZ as f64) as u64;
        if volley == Volley::Beam {
            c.beam = Some(BeamOn {
                target: p.id,
                until: tick + BEAM_TICKS,
                next_hit: tick + BEAM_HIT_TICKS,
            });
            acts.push(Act::Beam {
                creature: c.id,
                target: p.id,
            });
            wait += BEAM_TICKS;
        } else {
            acts.push(Act::Fire {
                creature: c.id,
                target: p.id,
                volley,
            });
        }
        c.next_attack = tick + wait;
        c.attack_until = tick + ATTACK_POSE_TICKS;
    }
    PlayerInput {
        yaw: c.yaw,
        ..Default::default()
    }
}

/// Whether a body `half_width` wide and `height` tall standing at `feet`
/// overlaps nothing solid.
pub(super) fn box_clear(world: &impl PhysicsWorld, feet: DVec3, half_width: f64, height: f64) -> bool {
    let e = 1e-6;
    let lo = BlockPos::containing(feet.x - half_width, feet.y, feet.z - half_width);
    let hi = BlockPos::containing(feet.x + half_width - e, feet.y + height - e, feet.z + half_width - e);
    for x in lo.x..=hi.x {
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                if world.solid(BlockPos::new(x, y, z)) {
                    return false;
                }
            }
        }
    }
    true
}

/// One tick of flight toward `c.fly_to`: no gravity, at most its run speed,
/// moving axis by axis and never into a solid block; wraps the seam. A calm
/// flyer that meets the ground going down lands.
fn fly(c: &mut Creature, def: &CreatureDef, ctx: &Ctx<impl Terrain>) {
    let calm = !c.angry();
    let speed = if calm {
        LAND_SPEED
    } else {
        def.run_speed as f64 * c.exotic.speed()
    };
    let goal = c.fly_to.unwrap_or(c.state.pos);
    let d = ctx.delta(c.state.pos, goal);
    let dist = d.length();
    let v = if dist < 1e-6 {
        DVec3::ZERO
    } else {
        d / dist * speed.min(dist * FLY_EASE)
    };
    let step = v * movement::TICK_DT;
    let (w, h) = (def.half_width as f64, def.height as f64);
    let mut pos = c.state.pos;
    let mut vel = v;
    let mut grounded = false;
    for axis in [1, 0, 2] {
        if step[axis] == 0.0 {
            continue;
        }
        let mut next = pos;
        next[axis] += step[axis];
        if box_clear(ctx.world, next, w, h) {
            pos = next;
        } else {
            vel[axis] = 0.0;
            grounded |= axis == 1 && step[axis] < 0.0;
        }
    }
    c.state.pos = ctx.shape.wrap_point(pos);
    c.state.vel = vel;
    c.state.on_ground = false;
    if calm && grounded {
        c.flying = false;
        c.fly_to = None;
        c.state.on_ground = true;
        c.state.vel = DVec3::ZERO;
    }
}

/// One tick of a creature: it thinks (if alive), moves (flying, or by the
/// players' physics), may land hard, and is recorded for rewinding.
pub(super) fn body_tick(
    c: &mut Creature,
    def: &CreatureDef,
    ctx: &Ctx<impl Terrain>,
    base: &MoveConfig,
    rng: &mut u64,
    acts: &mut Vec<Act>,
) {
    let tick = ctx.tick;
    let input = if c.alive() {
        think(c, def, ctx, rng, acts)
    } else {
        PlayerInput {
            yaw: c.yaw,
            ..Default::default()
        }
    };
    if c.flying && c.alive() {
        fly(c, def, ctx);
    } else {
        // Dead in the air: it falls.
        c.flying = false;
        // Standing still on the ground: nothing to move (most creatures,
        // most ticks; their ground is re-checked every second in case it
        // went).
        let still = input.forward == 0
            && input.strafe == 0
            && !input.jump
            && c.state.on_ground
            && c.state.vel.length_squared() < 1e-9
            && (tick + c.id as u64) % 60 != 0;
        if !still {
            let cfg = move_config(def, base, c, tick);
            movement::step(&mut c.state, &input, &cfg, None, ctx.shape, ctx.world);
        }
    }
    // Falls: the share of a body's health the landing took, as a player
    // would lose it (a dasher off a cliff). A curled popper landing hard
    // goes off instead.
    if c.state.hurt > 0 {
        let share = c.state.hurt as f64 / c.state.max_health() as f64;
        c.state.hurt = 0;
        c.state.down = false;
        if c.alive() {
            if def.behaviour == Behaviour::Exploder && c.curled(tick) {
                acts.push(Act::Explode { creature: c.id });
            } else {
                acts.push(Act::Fall { creature: c.id, share });
            }
        }
    }
    c.history[(tick % HISTORY as u64) as usize] = c.state.pos;
    c.pose = pose_of(c, &input, tick);
}

/// The pose to show for a creature after it moved.
fn pose_of(c: &Creature, input: &PlayerInput, tick: u64) -> Pose {
    if !c.alive() {
        return Pose::Dead;
    }
    if c.stunned(tick) {
        return Pose::Stunned;
    }
    if c.curled(tick) {
        return Pose::Curled;
    }
    if c.flying {
        return Pose::Flying;
    }
    if let Mind::Angry {
        fight: Fight::Lit { .. } | Fight::Leap { .. },
        ..
    } = c.mind
    {
        return Pose::Lit;
    }
    if tick < c.attack_until {
        return Pose::Attack;
    }
    match c.mind {
        Mind::Angry {
            fight: Fight::Windup { .. },
            ..
        } => Pose::Windup,
        Mind::Angry {
            fight: Fight::Charge { .. },
            ..
        } => Pose::Attack,
        _ if input.forward != 0 || input.strafe != 0 => {
            if input.sprint {
                Pose::Run
            } else {
                Pose::Walk
            }
        }
        _ => Pose::Idle,
    }
}

impl Game {
    /// Moves every creature one tick, applies what their AI did, removes
    /// bodies and lonely creatures, and spawns new ones around players.
    pub(super) fn step_creatures(&mut self) {
        let tick = self.tick;
        if !self.cfg.placed_creatures.is_empty() {
            self.place_creatures();
        }
        for pi in 0..self.planets.len() {
            if self.planets[pi].players == 0 {
                // Nobody is there: the creatures go (they are never saved).
                self.herds[pi].list.clear();
                continue;
            }
            let mut seen = std::mem::take(&mut self.seen_buf);
            seen.clear();
            let m = self.movement;
            seen.extend(self.players.iter().filter(|(_, p)| p.planet == pi).map(|(id, p)| Seen {
                id: *id,
                pos: p.state.pos,
                eye: p.state.eye(&m),
                down: p.state.down,
                protected: p.character.level() < PROTECTED_BELOW_LEVEL,
                noticed_at: p.effects.noticed_at(),
            }));
            let mut acts = std::mem::take(&mut self.acts_buf);
            acts.clear();
            {
                let planet = &self.planets[pi];
                let ctx = Ctx {
                    world: &planet.world,
                    shape: planet.world.shape,
                    seen: &seen,
                    tick,
                    hostile: hostile_world(planet.tier),
                    body: m,
                };
                let kinds = &self.creature_kinds;
                for c in self.herds[pi].list.iter_mut() {
                    let def = kinds.get(c.kind).expect("spawned from the catalogue");
                    // A creature in terrain not loaded waits for it.
                    let p = c.state.pos;
                    if ctx.world.block(BlockPos::containing(p.x, p.y, p.z)).is_none() {
                        continue;
                    }
                    let base = &self.creature_moves[c.kind as usize];
                    body_tick(c, def, &ctx, base, &mut self.rng, &mut acts);
                }
            }
            for act in acts.drain(..) {
                self.creature_act(pi, act);
            }
            self.acts_buf = acts;
            if (tick + pi as u64) % 60 == 0 {
                self.cull_creatures(pi, &seen);
            }
            self.seen_buf = seen;
        }
        if self.cfg.creature_spawning {
            let ids: Vec<u32> = self
                .players
                .keys()
                .copied()
                .filter(|id| (tick + *id as u64) % SPAWN_EVERY == 0)
                .collect();
            for id in ids {
                self.spawn_near(id);
            }
        }
    }

    /// Removes bodies whose time is up and creatures nobody is near.
    fn cull_creatures(&mut self, pi: usize, seen: &[Seen]) {
        let tick = self.tick;
        let shape = self.planets[pi].world.shape;
        self.herds[pi].empty.retain(|_, until| *until > tick);
        self.herds[pi].list.retain_mut(|c| c.stays(seen, shape, tick));
    }

    /// Spawns a group around player `id` if there are few creatures near
    /// them: a spot out of their sight, on open ground, outside claims.
    fn spawn_near(&mut self, id: u32) {
        let Some(p) = self.players.get(&id) else { return };
        if p.state.down {
            return;
        }
        let pi = p.planet;
        let (pos, eye) = (p.state.pos, p.state.eye(&self.movement));
        let planet = &self.planets[pi];
        let shape = planet.world.shape;
        let herd = &self.herds[pi];
        let roaming = herd.list.iter().filter(|c| c.meteor.is_none()).count();
        if roaming >= self.cfg.creatures_per_planet {
            return;
        }
        let near = herd
            .list
            .iter()
            .filter(|c| horizontal(shape.delta(pos, c.state.pos)).length() <= CROWD_RANGE)
            .count();
        let room = spawn_room(
            near,
            self.cfg.creatures_near_player,
            roaming,
            self.cfg.creatures_per_planet,
        );
        if room == 0 {
            return;
        }
        let Some(kind) = pick_kind(&self.creature_kinds, planet.tier, roll(&mut self.rng)) else {
            return;
        };
        let def = self.creature_kinds.get(kind).expect("picked");
        let tier = pick_tier(def, planet.tier, roll(&mut self.rng));
        let mut spot = None;
        for _ in 0..SPAWN_TRIES {
            let a = roll(&mut self.rng) * std::f64::consts::TAU;
            let r = SPAWN_MIN + roll(&mut self.rng) * (SPAWN_MAX - SPAWN_MIN);
            let x = (pos.x + a.sin() * r).floor() as i64;
            let z = (pos.z + a.cos() * r).floor() as i64;
            let top = pos.y as i64 + 16;
            let Some(feet) = surface(&planet.world, x, z, top, pos.y as i64 - 24, def.height as f64) else {
                continue;
            };
            if !self.may_spawn_at(pi, feet) {
                continue;
            }
            // Out of sight: never appears in front of the player.
            let head = feet + DVec3::new(0.0, def.height as f64 * 0.8, 0.0);
            if clear_sight(&planet.world, eye, eye + shape.delta(eye, head)) {
                continue;
            }
            spot = Some(feet);
            break;
        }
        let Some(spot) = spot else { return };
        let (lo, hi) = (def.group[0] as f64, def.group[1] as f64);
        let n = (lo + roll(&mut self.rng) * (hi - lo + 1.0)).floor().min(hi) as usize;
        let height = def.height as f64;
        for k in 0..n.clamp(1, room) {
            let feet = if k == 0 {
                Some(spot)
            } else {
                let a = roll(&mut self.rng) * std::f64::consts::TAU;
                let r = 1.5 + roll(&mut self.rng) * 2.5;
                let (x, z) = (
                    (spot.x + a.sin() * r).floor() as i64,
                    (spot.z + a.cos() * r).floor() as i64,
                );
                surface(
                    &self.planets[pi].world,
                    x,
                    z,
                    spot.y as i64 + 4,
                    spot.y as i64 - 6,
                    height,
                )
                .filter(|f| self.may_spawn_at(pi, *f))
            };
            if let Some(feet) = feet {
                let world_tier = self.planets[pi].tier;
                let rng = &mut self.rng;
                let exotic = roll_exotic(world_tier, || roll(rng));
                self.add_creature(pi, kind, tier, feet, exotic);
            }
        }
    }

    /// Whether a creature may appear standing at `feet`: not on a claim,
    /// not by a warp shrine, not where one was lately killed.
    fn may_spawn_at(&self, pi: usize, feet: DVec3) -> bool {
        let planet = &self.planets[pi];
        let b = planet
            .world
            .shape
            .wrap_block(BlockPos::containing(feet.x, feet.y, feet.z));
        planet.claims.at_block(b).is_none() && !planet.guarded(b) && self.herds[pi].column_open(b, self.tick)
    }

    /// A new creature of `kind` at `tier` standing at `feet`, with
    /// `exotic`'s buffs. Returns its id.
    pub(super) fn add_creature(&mut self, pi: usize, kind: u16, tier: u8, feet: DVec3, exotic: Exotic) -> u32 {
        let id = self.next_creature;
        self.next_creature = self.next_creature.wrapping_add(1).max(1);
        let feet = self.planets[pi].world.shape.wrap_point(feet);
        let mut c = Creature::new(id, kind, tier, feet, self.tick);
        c.exotic = exotic;
        self.herds[pi].list.push(c);
        id
    }

    /// Creatures placed by the config (tests), beside where players arrive on
    /// the start planet, once a player is there and the ground under each
    /// is loaded: on the surface of its column, near the height given.
    pub(super) fn place_creatures(&mut self) {
        let pi = self.start;
        if self.planets[pi].players == 0 {
            return;
        }
        let arrival = self.planets[pi].arrival().pos;
        let mut waiting = std::mem::take(&mut self.cfg.placed_creatures);
        waiting.retain(|c| {
            let Some(kind) = self.creature_kinds.id(&c.kind) else {
                return false;
            };
            let def = self.creature_kinds.get(kind).expect("present");
            let tier = c.tier.clamp(1, def.top_tier());
            let at = arrival + DVec3::from_array(c.offset);
            let (x, y, z) = (at.x.floor() as i64, at.y.floor() as i64, at.z.floor() as i64);
            match surface(&self.planets[pi].world, x, z, y + 8, y - 16, def.height as f64) {
                Some(feet) => {
                    self.add_creature(pi, kind, tier, feet, Exotic(c.exotic));
                    false
                }
                None => true,
            }
        });
        self.cfg.placed_creatures = waiting;
    }

    /// What one creature's AI did.
    fn creature_act(&mut self, pi: usize, act: Act) {
        match act {
            Act::Charge { creature, target, dir } => self.charge_hit(pi, creature, target, dir),
            Act::Volley { creature, target } => self.volley(pi, creature, target),
            Act::Fall { creature, share } => {
                let Some(c) = self.herds[pi].get(creature) else { return };
                let def = self.creature_kinds.get(c.kind).expect("present");
                let amount = (share * c.max_health(def) as f64).round() as u32;
                self.hurt_creature(pi, creature, None, amount, false);
            }
            Act::Explode { creature } => self.popper_blast(pi, creature),
            Act::Fire {
                creature,
                target,
                volley,
            } => self.drifter_fire(pi, creature, target, volley),
            Act::Beam { creature, target } => self.beam_start(pi, creature, target),
            Act::BeamHit { creature, target } => {
                let Some(c) = self.herds[pi].get(creature).filter(|c| c.alive()) else {
                    return;
                };
                let def = self.creature_kinds.get(c.kind).expect("present");
                let damage = c.hit_damage(def);
                self.creature_hits_player(pi, creature, target, damage, DamageKind::Impulse);
            }
            Act::Lift { target, .. } => self.lift(pi, target),
        }
    }

    /// Updates each player's creatures in view: `Creature` for one that came
    /// into view or changed, `CreatureGone` for one that left or vanished,
    /// and the list the next snapshots carry (nearest first).
    pub(super) fn sync_creature_views(&mut self) {
        for p in self.players.values_mut() {
            let herd = &self.herds[p.planet];
            let shape = self.planets[p.planet].world.shape;
            let mut near = std::mem::take(&mut p.near_creatures);
            near.clear();
            for (i, c) in herd.list.iter().enumerate() {
                let d = shape.delta(p.state.pos, c.state.pos);
                let d2 = d.x * d.x + d.z * d.z;
                if d2 <= VIEW_RANGE * VIEW_RANGE {
                    near.push((d2, i));
                }
            }
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            near.truncate(MAX_SEEN_CREATURES);
            p.in_view.clear();
            for &(_, i) in &near {
                let c = &herd.list[i];
                p.in_view.push(c.remote());
                let view = c.view(&p.account);
                if p.known.get(&c.id) != Some(&view) {
                    p.out.send(&ServerMsg::Creature(view.clone()));
                    p.known.insert(c.id, view);
                }
            }
            if p.known.len() > p.in_view.len() {
                let in_view = &p.in_view;
                let out = &p.out;
                p.known.retain(|id, _| {
                    let keep = in_view.iter().any(|r| r.id == *id);
                    if !keep {
                        out.send(&ServerMsg::CreatureGone { id: *id });
                    }
                    keep
                });
            }
            p.near_creatures = near;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::creatures::CreatureKinds;

    fn kinds() -> CreatureKinds {
        CreatureKinds::builtin()
    }

    #[test]
    fn rewinding_reads_the_history() {
        let mut c = Creature::new(1, 0, 1, DVec3::ZERO, 100);
        for t in 101..=130u64 {
            c.history[(t % HISTORY as u64) as usize] = DVec3::new(t as f64, 0.0, 0.0);
        }
        assert_eq!(c.rewound(130, 0).x, 130.0);
        assert_eq!(c.rewound(130, REWIND_TICKS).x, 123.0);
        assert_eq!(c.rewound(130, MAX_REWIND_TICKS).x, 118.0);
        assert_eq!(c.rewound(130, 40).x, 115.0, "never past the history kept");
        // Never before it was born.
        let young = Creature::new(2, 0, 1, DVec3::splat(5.0), 128);
        assert_eq!(young.rewound(130, 12), DVec3::splat(5.0));
    }

    #[test]
    fn threat_picks_the_top_and_forgets() {
        let mut c = Creature::new(1, 0, 1, DVec3::ZERO, 0);
        c.add_threat(7, 300.0);
        c.add_threat(8, 200.0);
        c.add_threat(8, 200.0);
        assert_eq!(c.top_threat(|_| true), Some(8));
        assert_eq!(c.top_threat(|p| p != 8), Some(7), "only players still about");
        // Lure doubles threat: 150 x 2 overtakes.
        c.add_threat(9, 150.0 * 2.0 + 101.0);
        assert_eq!(c.top_threat(|_| true), Some(9));
        c.mind = Mind::Angry {
            target: 9,
            fight: Fight::Close,
        };
        c.forget(9);
        assert_eq!(c.mind, Mind::Calm);
        assert_eq!(c.top_threat(|_| true), Some(8));
    }

    #[test]
    fn segments_hit_boxes() {
        let k = kinds();
        let grazer = k.get(k.id("grazer").unwrap()).unwrap();
        let (lo, hi) = creature_box(grazer, DVec3::new(10.0, 5.0, 0.0));
        // Straight through the middle.
        let t = segment_hits_box(DVec3::new(8.0, 5.7, 0.0), DVec3::new(12.0, 5.7, 0.0), lo, hi).unwrap();
        assert!((t - 0.35).abs() < 1e-6, "{t}");
        // Above it, short of it, and starting inside it.
        assert!(segment_hits_box(DVec3::new(8.0, 7.0, 0.0), DVec3::new(12.0, 7.0, 0.0), lo, hi).is_none());
        assert!(segment_hits_box(DVec3::new(6.0, 5.7, 0.0), DVec3::new(9.0, 5.7, 0.0), lo, hi).is_none());
        assert_eq!(
            segment_hits_box(DVec3::new(10.0, 5.5, 0.0), DVec3::new(10.0, 9.0, 0.0), lo, hi),
            Some(0.0)
        );
        // A charge's ball against a player's body.
        let m = MoveConfig::default();
        assert!(ball_hits_body(DVec3::new(1.5, 1.0, 0.0), 1.8, DVec3::ZERO, &m));
        assert!(!ball_hits_body(DVec3::new(2.2, 1.0, 0.0), 1.8, DVec3::ZERO, &m));
    }

    #[test]
    fn kinds_and_tiers_follow_the_world() {
        let k = kinds();
        let dasher = k.id("dasher").unwrap();
        let grazer = k.id("grazer").unwrap();
        let mut count = HashMap::new();
        for i in 0..1000 {
            let id = pick_kind(&k, 1, i as f64 / 1000.0).unwrap();
            *count.entry(id).or_insert(0) += 1;
        }
        // Only kinds that live on tier 1 worlds share the weight there.
        let total: u32 = k
            .defs()
            .iter()
            .filter(|d| d.min_world_tier <= 1)
            .map(|d| d.spawn_weight as u32)
            .sum();
        let want = |id: u16| 1000.0 * k.get(id).unwrap().spawn_weight as f64 / total as f64;
        assert!((count[&grazer] as f64 - want(grazer)).abs() <= 2.0);
        assert!((count[&dasher] as f64 - want(dasher)).abs() <= 2.0);
        // Tier 1 and 2 worlds hold tier 1; tier 3 holds 1 and 2; the dasher
        // stops at 3 on any world.
        let def = k.get(grazer).unwrap();
        for r in [0.0, 0.5, 0.999] {
            assert_eq!(pick_tier(def, 1, r), 1);
            assert_eq!(pick_tier(def, 2, r), 1);
        }
        assert_eq!(pick_tier(def, 3, 0.2), 1);
        assert_eq!(pick_tier(def, 3, 0.7), 2);
        assert_eq!(pick_tier(def, 8, 0.999), 5);
        assert_eq!(pick_tier(k.get(dasher).unwrap(), 8, 0.999), 3);
        // A kind for higher worlds only never spawns low.
        let mut defs = k.defs().to_vec();
        for d in &mut defs {
            d.min_world_tier = 3;
        }
        let high = CreatureKinds::from_defs(defs).unwrap();
        assert_eq!(pick_kind(&high, 2, 0.5), None);
        assert!(pick_kind(&high, 3, 0.5).is_some());
    }

    #[test]
    fn yaw_faces_the_way_given() {
        let f = |d: DVec3| movement::heading(yaw_toward(d)).0;
        for d in [
            DVec3::X,
            DVec3::NEG_X,
            DVec3::Z,
            DVec3::NEG_Z,
            DVec3::new(1.0, 0.0, 1.0),
        ] {
            let got = f(d);
            assert!(got.distance(d.normalize()) < 1e-3, "{d} -> {got}");
        }
    }
}

#[cfg(test)]
mod ai_tests {
    use super::*;

    /// Flat ground (solid below y 10), a wall at x 3 if asked, water in
    /// column (5, 5), nothing loaded at x 300 and beyond.
    struct Flat {
        shape: PlanetShape,
        wall: bool,
    }

    impl PhysicsWorld for Flat {
        fn solid(&self, p: BlockPos) -> bool {
            p.y < 10 || (self.wall && p.x == 3 && p.y < 40)
        }
        fn liquid(&self, p: BlockPos) -> bool {
            (p.x, p.z) == (5, 5) && (10..12).contains(&p.y)
        }
    }

    impl Terrain for Flat {
        fn ground(&self, p: BlockPos) -> Option<Ground> {
            if p.x >= 300 {
                return None;
            }
            Some(if self.liquid(p) {
                Ground::Water
            } else if self.solid(p) {
                Ground::Solid
            } else {
                Ground::Open
            })
        }
        fn planet(&self) -> PlanetShape {
            self.shape
        }
    }

    fn flat(wall: bool) -> Flat {
        Flat {
            shape: PlanetShape::new(16, 12),
            wall,
        }
    }

    fn player(id: u32, x: f64) -> Seen {
        let pos = DVec3::new(x, 10.0, 0.5);
        Seen {
            id,
            pos,
            eye: pos + DVec3::new(0.0, 1.75, 0.0),
            down: false,
            protected: false,
            noticed_at: 1.0,
        }
    }

    /// Runs a creature's AI and physics for `ticks` from `tick`; the acts.
    fn run(
        c: &mut Creature,
        def: &CreatureDef,
        world: &Flat,
        seen: &[Seen],
        hostile: bool,
        tick: &mut u64,
        ticks: u64,
    ) -> Vec<Act> {
        let mut rng = 1;
        let mut acts = Vec::new();
        let cfg = def.move_config();
        for _ in 0..ticks {
            *tick += 1;
            let ctx = Ctx {
                world,
                shape: world.shape,
                seen,
                tick: *tick,
                hostile,
                body: MoveConfig::default(),
            };
            body_tick(c, def, &ctx, &cfg, &mut rng, &mut acts);
        }
        acts
    }

    fn kind(name: &str) -> (CreatureKinds, u16) {
        let k = CreatureKinds::builtin();
        let id = k.id(name).unwrap();
        (k, id)
    }

    #[test]
    fn creatures_stand_on_open_ground_never_water_or_unloaded() {
        let w = flat(false);
        assert_eq!(surface(&w, 1, 1, 30, 0, 1.4), Some(DVec3::new(1.5, 10.0, 1.5)));
        assert_eq!(surface(&w, 5, 5, 30, 0, 1.4), None, "water");
        assert_eq!(surface(&w, 400, 1, 30, 0, 1.4), None, "not loaded");
        assert_eq!(surface(&w, 1, 1, 10, 0, 1.4), None, "no room under the top searched");
    }

    #[test]
    fn spawns_never_pass_the_caps() {
        assert_eq!(spawn_room(0, 8, 0, 600), 8);
        assert_eq!(spawn_room(6, 8, 0, 600), 2, "a group fills the room left");
        assert_eq!(spawn_room(8, 8, 0, 600), 0);
        assert_eq!(spawn_room(9, 8, 0, 600), 0);
        assert_eq!(spawn_room(0, 8, 597, 600), 3, "the planet's cap too");
        assert_eq!(spawn_room(0, 8, 600, 600), 0);
    }

    #[test]
    fn lonely_creatures_and_old_bodies_go() {
        let shape = PlanetShape::new(16, 12);
        let mut c = Creature::new(1, 0, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        let near = [player(7, 30.0)];
        let far = [player(7, 100.0)];
        assert!(c.stays(&near, shape, 10));
        assert!(c.stays(&far, shape, 20), "the wait starts");
        assert!(c.stays(&far, shape, 20 + LONELY_TICKS - 1));
        assert!(!c.stays(&far, shape, 20 + LONELY_TICKS), "nobody near for 10 s");
        // Someone coming back resets the wait.
        let mut c = Creature::new(2, 0, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        assert!(c.stays(&far, shape, 20));
        assert!(c.stays(&near, shape, 500));
        assert!(c.stays(&far, shape, 20 + LONELY_TICKS + 1));
        // Across the seam counts as near: x 0.5 and x 500 are 12.5 apart.
        let mut c = Creature::new(3, 0, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        assert!(c.stays(&[player(7, 500.0)], shape, 1));
        assert!(c.stays(&[player(7, 500.0)], shape, 1 + LONELY_TICKS * 2));
        // A body stays its time, near or not.
        c.dead_at = Some(100);
        assert!(c.stays(&near, shape, 100 + BODY_TICKS - 1));
        assert!(!c.stays(&near, shape, 100 + BODY_TICKS));
    }

    #[test]
    fn each_player_loots_a_body_once() {
        let mut c = Creature::new(1, 0, 1, DVec3::ZERO, 0);
        assert!(!c.take_loot("alice"), "not while it lives");
        c.dead_at = Some(5);
        assert!(c.take_loot("alice"));
        assert!(!c.take_loot("alice"), "once");
        assert!(c.take_loot("bob"), "every player their own");
        assert!(c.view("alice").looted && c.view("bob").looted && !c.view("carol").looted);
    }

    #[test]
    fn a_herd_stays_calm_until_hurt_then_gives_up_far_away() {
        let (k, id) = kind("grazer");
        let def = k.get(id).unwrap();
        let w = flat(false);
        let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        let mut tick = 0;
        let near = [player(7, 5.0)];
        run(&mut c, def, &w, &near, true, &mut tick, 120);
        assert_eq!(c.mind, Mind::Calm, "herds never start a fight");
        c.add_threat(7, 300.0);
        provoke(&mut c, def, 7, tick);
        assert!(c.angry());
        run(&mut c, def, &w, &near, true, &mut tick, 1);
        assert!(c.angry(), "angry at whoever hurt it");
        // The target runs beyond give_up: it lets them go.
        let far = [player(7, c.state.pos.x + def.give_up as f64 + 5.0)];
        run(&mut c, def, &w, &far, true, &mut tick, 2);
        assert_eq!(c.mind, Mind::Calm);
        assert!(c.threat.is_empty());
    }

    #[test]
    fn an_angry_creature_forgets_a_target_it_cannot_see() {
        let (k, id) = kind("grazer");
        let def = k.get(id).unwrap();
        let w = flat(true);
        // Behind a wall, within give_up.
        let seen = [player(7, 8.0)];
        let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        let mut tick = 0;
        c.add_threat(7, 10.0);
        provoke(&mut c, def, 7, tick);
        let forget = (def.forget as f64 * TICK_HZ as f64) as u64;
        run(&mut c, def, &w, &seen, true, &mut tick, forget / 2);
        assert!(c.angry(), "still hunting");
        run(&mut c, def, &w, &seen, true, &mut tick, forget);
        assert_eq!(c.mind, Mind::Calm, "unseen for `forget` seconds");
    }

    #[test]
    fn ranged_creatures_attack_on_hostile_worlds_but_spare_new_players() {
        let (k, id) = kind("pelter");
        let def = k.get(id).unwrap();
        let w = flat(false);
        let at = DVec3::new(0.5, 10.0, 0.5);
        let check = |seen: Seen, hostile: bool, threat: bool| {
            let mut c = Creature::new(1, id, 1, at, 0);
            if threat {
                c.add_threat(seen.id, 50.0);
            }
            let mut tick = 0;
            run(&mut c, def, &w, &[seen], hostile, &mut tick, NOTICE_EVERY * 2);
            c.angry()
        };
        assert!(check(player(7, 10.0), true, false), "noticed on a hostile world");
        assert!(!check(player(7, 10.0), false, false), "a tier 1 world is passive");
        let new = Seen {
            protected: true,
            ..player(7, 10.0)
        };
        assert!(!check(new, true, false), "new players are spared");
        assert!(check(new, true, true), "unless they attacked it");
        assert!(!check(player(7, 30.0), true, false), "beyond notice");
        // Volleys follow, at the target.
        let mut c = Creature::new(1, id, 1, at, 0);
        let mut tick = 0;
        let acts = run(&mut c, def, &w, &[player(7, 10.0)], true, &mut tick, 6 * TICK_HZ as u64);
        let volleys = acts
            .iter()
            .filter(|a| matches!(a, Act::Volley { target: 7, .. }))
            .count();
        assert!((2..=4).contains(&volleys), "a volley every 2 s: {volleys}");
    }

    #[test]
    fn a_dasher_flees_and_quiet_step_halves_its_notice() {
        let (k, id) = kind("dasher");
        let def = k.get(id).unwrap();
        let w = flat(false);
        let at = DVec3::new(0.5, 10.0, 0.5);
        let x = def.notice as f64 * 0.75;
        let mut c = Creature::new(1, id, 1, at, 0);
        let mut tick = 0;
        run(&mut c, def, &w, &[player(7, x)], true, &mut tick, NOTICE_EVERY * 2);
        assert_eq!(c.mind, Mind::Flee { from: 7 });
        let start = c.state.pos.x;
        let acts = run(&mut c, def, &w, &[player(7, x)], true, &mut tick, 60);
        assert!(c.state.pos.x < start - 3.0, "runs away: {} -> {}", start, c.state.pos.x);
        assert!(acts.is_empty(), "never attacks");
        let quiet = Seen {
            noticed_at: 0.5,
            ..player(7, x)
        };
        let mut c = Creature::new(1, id, 1, at, 0);
        let mut tick = 0;
        run(&mut c, def, &w, &[quiet], true, &mut tick, NOTICE_EVERY * 2);
        assert_eq!(c.mind, Mind::Calm, "Quiet Step: noticed at half the range");
    }

    #[test]
    fn a_grazer_charges_once_a_run_up_at_tier_1_and_more_higher() {
        let (k, id) = kind("grazer");
        let def = k.get(id).unwrap();
        let w = flat(false);
        let charges = |tier: u8| {
            let mut c = Creature::new(1, id, tier, DVec3::new(0.5, 10.0, 0.5), 0);
            let mut tick = 0;
            c.add_threat(7, 100.0);
            provoke(&mut c, def, 7, 0);
            // Within its cooldown (3 s) of the first charge.
            let acts = run(&mut c, def, &w, &[player(7, 8.0)], true, &mut tick, 150);
            acts.iter()
                .filter(|a| matches!(a, Act::Charge { target: 7, .. }))
                .count()
        };
        assert_eq!(charges(1), 1);
        assert_eq!(charges(2), 2);
        assert_eq!(charges(3), 3);
    }

    // Milestone 13 part 2.

    #[test]
    fn exotic_buffs_change_its_numbers() {
        let (k, id) = kind("popper");
        let def = k.get(id).unwrap();
        let mut c = Creature::new(1, id, 2, DVec3::ZERO, 0);
        assert_eq!(
            (c.max_health(def), c.armour(def), c.hit_damage(def), c.xp(def)),
            (2500, 100, 1000, 240)
        );
        c.exotic = Exotic(Exotic::HEALTH | Exotic::ARMOUR | Exotic::DAMAGE | Exotic::SPEED);
        assert_eq!(
            (c.max_health(def), c.armour(def), c.hit_damage(def), c.xp(def)),
            (7500, 200, 1500, 360)
        );
        assert_eq!(c.view("a").exotic, c.exotic.0, "players see the buffs");
        let base = def.move_config();
        let m = move_config(def, &base, &c, 0);
        assert!((m.sprint_speed - base.sprint_speed * 1.3).abs() < 1e-9);
        assert!((m.walk_speed - base.walk_speed * 1.3).abs() < 1e-9);
        c.damage = 3750;
        assert!((c.health_share(def) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_popper_hops_about_calmly_on_a_gentle_world() {
        let (k, id) = kind("popper");
        let def = k.get(id).unwrap();
        let w = flat(false);
        let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        let mut tick = 0;
        let mut top: f64 = 0.0;
        for _ in 0..400 {
            run(&mut c, def, &w, &[player(7, 4.0)], false, &mut tick, 1);
            top = top.max(c.state.pos.y);
        }
        assert_eq!(c.mind, Mind::Calm, "a gentle world: only when hurt");
        assert!(top > 10.5, "it hops as it wanders: {top}");
    }

    #[test]
    fn a_popper_whistles_for_its_fuse_then_leaps_and_explodes() {
        let (k, id) = kind("popper");
        let def = k.get(id).unwrap();
        let Attack::Explode { fuse, .. } = def.attack else {
            panic!()
        };
        let w = flat(false);
        let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        let mut tick = 0;
        let seen = [player(7, 6.0)];
        let mut lit_at = None;
        let mut boom_at = None;
        for _ in 0..600 {
            let acts = run(&mut c, def, &w, &seen, true, &mut tick, 1);
            if lit_at.is_none() && c.pose == Pose::Lit {
                lit_at = Some(tick);
            }
            if acts.contains(&Act::Explode { creature: 1 }) {
                boom_at = Some(tick);
                break;
            }
        }
        let (lit, boom) = (lit_at.expect("it lit"), boom_at.expect("it exploded"));
        let fuse = (fuse as f64 * TICK_HZ as f64) as u64;
        assert!(boom >= lit + fuse, "whistles at least its fuse: {lit} -> {boom}");
        assert!(boom <= lit + fuse + 120, "then leaps soon: {lit} -> {boom}");
        // It reached the player.
        let d = horizontal(c.state.pos - seen[0].pos).length();
        assert!(d < 2.0, "{d}");
        // New players are spared, as by a pelter.
        let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        let mut tick = 0;
        let new = Seen {
            protected: true,
            ..player(7, 6.0)
        };
        run(&mut c, def, &w, &[new], true, &mut tick, 120);
        assert_eq!(c.mind, Mind::Calm);
    }

    #[test]
    fn a_popper_that_cannot_reach_explodes_where_it_is() {
        let (k, id) = kind("popper");
        let def = k.get(id).unwrap();
        // Behind a wall it cannot hop over.
        let w = flat(true);
        let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        let mut tick = 0;
        c.add_threat(7, 10.0);
        provoke(&mut c, def, 7, 0);
        let acts = run(
            &mut c,
            def,
            &w,
            &[player(7, 8.0)],
            true,
            &mut tick,
            90 + CHASE_TIMEOUT_TICKS + 5,
        );
        assert!(acts.contains(&Act::Explode { creature: 1 }));
        assert!(c.state.pos.x < 3.0, "never got past the wall");
    }

    #[test]
    fn curling_makes_a_popper_unhurt_once_and_only_once() {
        let (k, id) = kind("popper");
        let def = k.get(id).unwrap();
        let Attack::Explode { curl, .. } = def.attack else {
            panic!()
        };
        let curl = (curl as f64 * TICK_HZ as f64) as u64;
        let mut c = Creature::new(1, id, 1, DVec3::ZERO, 0);
        assert_eq!(c.struck(def, 100, 0.0, 300), 300, "the hit that curls it lands");
        assert!(c.curled(101));
        assert_eq!(c.struck(def, 150, 0.0, 300), 0, "a ball cannot be hurt");
        assert!(!c.curled(100 + curl));
        assert_eq!(c.struck(def, 100 + curl, 0.0, 300), 300, "uncurled");
        assert!(!c.curled(100 + curl + 1), "it curls only the first time");
        // Missed the chance: never curls.
        let mut c = Creature::new(1, id, 1, DVec3::ZERO, 0);
        assert_eq!(c.struck(def, 100, 0.99, 300), 300);
        assert_eq!(c.struck(def, 101, 0.0, 300), 300);
        assert!(!c.curled(102));
        // Other kinds never curl.
        let (k, gid) = kind("grazer");
        let mut g = Creature::new(2, gid, 1, DVec3::ZERO, 0);
        g.struck(k.get(gid).unwrap(), 100, 0.0, 300);
        assert!(!g.curled(101));
    }

    #[test]
    fn a_curled_popper_landing_hard_explodes() {
        let (k, id) = kind("popper");
        let def = k.get(id).unwrap();
        let w = flat(false);
        let drop = |curled: bool| {
            let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 22.0, 0.5), 0);
            if curled {
                c.curled_until = 1000;
            }
            let mut tick = 0;
            run(&mut c, def, &w, &[], false, &mut tick, 120)
        };
        let acts = drop(true);
        assert!(acts.contains(&Act::Explode { creature: 1 }), "{acts:?}");
        assert!(!acts.iter().any(|a| matches!(a, Act::Fall { .. })));
        let acts = drop(false);
        assert!(!acts.contains(&Act::Explode { creature: 1 }));
        assert!(acts.iter().any(|a| matches!(a, Act::Fall { .. })), "an ordinary fall");
        // A gentle landing does nothing, curled or not.
        let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 11.0, 0.5), 0);
        c.curled_until = 1000;
        let mut tick = 0;
        assert!(run(&mut c, def, &w, &[], false, &mut tick, 60).is_empty());
    }

    #[test]
    fn a_hammer_stuns_a_popper_and_its_fuse_waits() {
        let (k, id) = kind("popper");
        let def = k.get(id).unwrap();
        let w = flat(false);
        let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        let mut tick = 0;
        c.add_threat(7, 10.0);
        provoke(&mut c, def, 7, 0);
        let seen = [player(7, 8.0)];
        run(&mut c, def, &w, &seen, true, &mut tick, 20);
        let left = |c: &Creature| match c.mind {
            Mind::Angry {
                fight: Fight::Lit { left, .. },
                ..
            } => left,
            m => panic!("{m:?}"),
        };
        // Let it land from any hop first.
        while !c.state.on_ground {
            run(&mut c, def, &w, &seen, true, &mut tick, 1);
        }
        let before = left(&c);
        c.hammered(def, tick);
        // It skids to a stop, then stays.
        run(&mut c, def, &w, &seen, true, &mut tick, 30);
        let at = c.state.pos;
        run(&mut c, def, &w, &seen, true, &mut tick, HAMMER_STUN_TICKS - 31);
        assert_eq!(c.pose, Pose::Stunned);
        assert_eq!(left(&c), before, "no fuse burns while stunned");
        assert!(horizontal(c.state.pos - at).length() < 0.01, "and it does not move");
        run(&mut c, def, &w, &seen, true, &mut tick, 10);
        assert!(left(&c) < before, "the fuse burns again");
        // Only poppers.
        let (k, gid) = kind("grazer");
        let mut g = Creature::new(2, gid, 1, DVec3::ZERO, 0);
        g.hammered(k.get(gid).unwrap(), 0);
        assert!(!g.stunned(1));
    }

    #[test]
    fn a_drifter_takes_off_hovers_fires_and_lands_when_it_gives_up() {
        let (k, id) = kind("drifter");
        let def = k.get(id).unwrap();
        let Attack::Barrage { hover, .. } = def.attack else {
            panic!()
        };
        let w = flat(false);
        let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        let mut tick = 0;
        let seen = [player(7, 12.0)];
        run(&mut c, def, &w, &[player(7, 12.0)], false, &mut tick, 60);
        assert!(!c.flying && c.mind == Mind::Calm, "it rests on a gentle world");
        let acts = run(&mut c, def, &w, &seen, true, &mut tick, 8 * TICK_HZ as u64);
        assert!(c.flying);
        assert_eq!(c.pose, Pose::Flying);
        let d = w.shape.delta(seen[0].pos, c.state.pos);
        assert!((d.y - hover as f64).abs() < 1.5, "hovers above: {d}");
        assert!((horizontal(d).length() - ORBIT_RADIUS).abs() < 2.5, "circles: {d}");
        let fired: Vec<_> = acts
            .iter()
            .filter_map(|a| match a {
                Act::Fire { volley, target: 7, .. } => Some(*volley),
                _ => None,
            })
            .collect();
        assert!(fired.len() >= 2, "{fired:?}");
        assert!(fired.iter().all(|v| *v == Volley::Bolt), "only bolts at full health");
        // The target goes far away: it gives up and comes down.
        let gone = [player(7, 200.0)];
        run(&mut c, def, &w, &gone, true, &mut tick, 10 * TICK_HZ as u64);
        assert_eq!(c.mind, Mind::Calm);
        assert!(!c.flying);
        assert!((c.state.pos.y - 10.0).abs() < 0.1, "on the ground: {}", c.state.pos.y);
    }

    #[test]
    fn a_drifters_attacks_unlock_as_its_health_drops() {
        let (k, id) = kind("drifter");
        let def = k.get(id).unwrap();
        let a = &def.attack;
        assert_eq!(barrage_options(a, 1, 1.0), [Volley::Bolt]);
        assert_eq!(barrage_options(a, 1, 0.79), [Volley::Bolt, Volley::Bomb]);
        assert_eq!(
            barrage_options(a, 1, 0.59),
            [Volley::Bolt, Volley::Bomb, Volley::Homing]
        );
        assert_eq!(barrage_options(a, 3, 1.0), [Volley::Bolt, Volley::Beam]);
        // Below 60% it takes turns among all three.
        let w = flat(false);
        let mut c = Creature::new(1, id, 1, DVec3::new(0.5, 10.0, 0.5), 0);
        c.damage = c.max_health(def) / 2;
        let mut tick = 0;
        let acts = run(
            &mut c,
            def,
            &w,
            &[player(7, 12.0)],
            true,
            &mut tick,
            10 * TICK_HZ as u64,
        );
        let fired: Vec<_> = acts
            .iter()
            .filter_map(|a| match a {
                Act::Fire { volley, .. } => Some(*volley),
                _ => None,
            })
            .collect();
        assert!(fired.len() >= 3, "{fired:?}");
        assert_eq!(fired[..3], [Volley::Bolt, Volley::Bomb, Volley::Homing]);
        // From its beam tier, a beam: hits every half second for 2 s.
        let mut c = Creature::new(1, id, 3, DVec3::new(0.5, 10.0, 0.5), 0);
        c.volleys = 1;
        let mut tick = 0;
        let acts = run(&mut c, def, &w, &[player(7, 12.0)], true, &mut tick, 6 * TICK_HZ as u64);
        assert!(acts.iter().any(|a| matches!(a, Act::Beam { target: 7, .. })));
        let hits = acts.iter().filter(|a| matches!(a, Act::BeamHit { .. })).count();
        assert_eq!(hits as u64, BEAM_TICKS / BEAM_HIT_TICKS, "one beam's hits");
    }

    #[test]
    fn a_high_drifter_lifts_a_player_who_stays_on_its_mark() {
        let (k, id) = kind("drifter");
        let def = k.get(id).unwrap();
        let w = flat(false);
        let lifts = |moves: bool| {
            let mut c = Creature::new(1, id, 4, DVec3::new(0.5, 10.0, 0.5), 0);
            let mut tick = 0;
            let mut acts = run(&mut c, def, &w, &[player(7, 12.0)], true, &mut tick, 45);
            assert!(c.lift.is_some(), "it marked their spot");
            let then = if moves { player(7, 15.0) } else { player(7, 12.0) };
            acts.extend(run(&mut c, def, &w, &[then], true, &mut tick, 10 * TICK_HZ as u64));
            acts.iter().filter(|a| matches!(a, Act::Lift { target: 7, .. })).count()
        };
        assert_eq!(lifts(false), 1, "once, then its cooldown");
        assert_eq!(lifts(true), 0, "stepping off the mark dodges it");
        // Below its lift tier: never.
        let mut c = Creature::new(1, id, 3, DVec3::new(0.5, 10.0, 0.5), 0);
        let mut tick = 0;
        run(&mut c, def, &w, &[player(7, 12.0)], true, &mut tick, 120);
        assert!(c.lift.is_none());
    }
}
