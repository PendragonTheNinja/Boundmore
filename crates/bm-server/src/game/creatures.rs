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
use bm_core::creatures::{
    Attack, Behaviour, CreatureDef, CreatureKinds, PROTECTED_BELOW_LEVEL, hostile_world, tiers_on,
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
        }
    }

    pub(super) fn alive(&self) -> bool {
        self.dead_at.is_none()
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
    let id = c.id;
    // Calm: look about now and then.
    if matches!(c.mind, Mind::Calm) && (tick + id as u64) % NOTICE_EVERY == 0 {
        notice(c, def, ctx);
    }
    loop {
        match c.mind {
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
                    c.mind = Mind::Angry {
                        target: top,
                        fight: Fight::Close,
                    };
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
                let charging = matches!(fight, Fight::Charge { .. });
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
        Behaviour::Ranged if ctx.hostile && !(p.protected && !c.threat.iter().any(|(t, _)| *t == p.id)) => {
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
            Fight::Close => {
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
        Attack::None => 1.0,
    };
    (s as f64 * TICK_HZ as f64) as u64
}

/// The movement settings a creature moves by this tick: a charge runs at
/// the charge's speed.
pub(super) fn move_config(def: &CreatureDef, base: &MoveConfig, mind: &Mind) -> MoveConfig {
    match (mind, def.attack) {
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
        _ => *base,
    }
}

/// The pose to show for a creature after it moved.
fn pose_of(c: &Creature, input: &PlayerInput, tick: u64) -> Pose {
    if !c.alive() {
        return Pose::Dead;
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
                    let input = if c.alive() {
                        think(c, def, &ctx, &mut self.rng, &mut acts)
                    } else {
                        PlayerInput {
                            yaw: c.yaw,
                            ..Default::default()
                        }
                    };
                    let cfg = move_config(def, &self.creature_moves[c.kind as usize], &c.mind);
                    movement::step(&mut c.state, &input, &cfg, None, ctx.shape, ctx.world);
                    // Falls: the share of a body's health the landing took,
                    // as a player would lose it (a dasher off a cliff).
                    if c.state.hurt > 0 {
                        let share = c.state.hurt as f64 / c.state.max_health() as f64;
                        c.state.hurt = 0;
                        c.state.down = false;
                        if c.alive() {
                            acts.push(Act::Fall { creature: c.id, share });
                        }
                    }
                    c.history[(tick % HISTORY as u64) as usize] = c.state.pos;
                    c.pose = pose_of(c, &input, tick);
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
        if herd.list.len() >= self.cfg.creatures_per_planet {
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
            herd.list.len(),
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
                self.add_creature(pi, kind, tier, feet);
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

    /// A new creature of `kind` at `tier` standing at `feet`. Returns its id.
    pub(super) fn add_creature(&mut self, pi: usize, kind: u16, tier: u8, feet: DVec3) -> u32 {
        let id = self.next_creature;
        self.next_creature = self.next_creature.wrapping_add(1).max(1);
        let feet = self.planets[pi].world.shape.wrap_point(feet);
        self.herds[pi].list.push(Creature::new(id, kind, tier, feet, self.tick));
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
                    self.add_creature(pi, kind, tier, feet);
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
                let amount = (share * def.tier(c.tier).health as f64).round() as u32;
                self.hurt_creature(pi, creature, None, amount, false);
            }
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
        let total: u32 = k.defs().iter().map(|d| d.spawn_weight as u32).sum();
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
            let input = think(c, def, &ctx, &mut rng, &mut acts);
            let m = move_config(def, &cfg, &c.mind);
            movement::step(&mut c.state, &input, &m, None, world.shape, world);
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
}
