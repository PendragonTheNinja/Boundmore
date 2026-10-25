//! Combat on the server (milestone 13, `docs/milestones/13-creatures-and-combat.md`):
//! slings and tool strikes on the input timeline, sharing one cadence with
//! block swings; server-side shots judged against creatures rewound to where
//! the shooter saw them; damage, XP and per-player drops; creature attacks
//! on players; defeat with Boundless's XP debt, and the return to safe
//! ground. The rules themselves are `bm_core::combat` and
//! `bm_core::creatures`; this applies them.

use super::creatures::{
    CREATURE_SHOT_FALLOFF, EMPTY_AFTER_KILL_TICKS, KNOCK_UP, MAX_REWIND_TICKS, REWIND_TICKS, column, creature_box,
    provoke, segment_hits_box,
};
use super::mining::roll;
use super::{Game, send_slots, swing_tool_of, wear_slot};
use bm_core::combat::{
    CHARGE_TICKS, DamageKind, HOMING_TURN_PER_TICK, charge_multiplier, dealt, homing_step, shot_step, taken,
};
use bm_core::creatures::{Attack, roll_drops};
use bm_core::forge::{Held, MANY_SHOTS_SPREAD, TraitId, many_shots};
use bm_core::items::ToolClass;
use bm_core::mining::{is_crit, skilled_tool_keen, swing_ticks};
use bm_core::movement::{PhysicsWorld, look_dir};
use bm_core::raycast::raycast_shaped;
use bm_core::skills::defeat_debt;
use bm_core::{BlockPos, HOTBAR_SLOTS, PlayerState};
use bm_protocol::{ServerMsg, ShotKind, ShotView, Target};
use glam::DVec3;

/// Health a defeated player comes back with, as a share of their max: half,
/// a **guess** (Boundless's is unpublished; Nathan may measure it).
pub(super) const RETURN_HEALTH_SHARE: f64 = 0.5;

/// A defeated player comes back this far from where they fell, at least and
/// at most, in blocks.
const RETURN_MIN: f64 = 8.0;
const RETURN_MAX: f64 = 24.0;
/// Places tried before falling back to the planet's arrival point.
const RETURN_TRIES: usize = 48;

/// Players within this many blocks of a shot are told it flies.
pub(super) const SHOT_TELL_RANGE: f64 = 96.0;

/// A bomb that strikes terrain rests this far short of where it struck
/// (blocks), so it lies in the air beside the block.
const BOMB_REST_GAP: f64 = 0.05;

/// What a defeat by a fall is called (`ServerMsg::Defeated::by`).
pub(super) const BY_A_FALL: &str = "a fall";

/// Who fired a shot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Shooter {
    /// A player, at input `at`, judged against creatures `rewind` ticks back.
    Player {
        id: u32,
        at: u32,
        rewind: u32,
    },
    Creature(u32),
}

/// A shot in flight: a pellet, a bolt, a homing bolt or a bomb.
pub(super) struct Shot {
    pub id: u32,
    pub planet: usize,
    pub from: Shooter,
    pub pos: DVec3,
    pub vel: DVec3,
    pub falloff: f32,
    pub range: f32,
    pub travelled: f64,
    /// Damage before armour and crits.
    pub damage: u32,
    pub crit_chance: f32,
    pub crit_effect: f32,
    /// Players told it flies: they hear when it ends.
    pub watchers: Vec<u32>,
    pub kind: ShotKind,
    /// The player a homing bolt turns toward.
    pub homing: Option<u32>,
    /// A bomb: ticks of fuse left, and its blast.
    pub fuse: u32,
    pub blast: Option<BlastSpec>,
    /// A bomb lying where it landed.
    pub resting: bool,
}

/// What a bomb's blast does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BlastSpec {
    pub radius: f32,
    pub knockback: f32,
}

impl Shot {
    /// A pellet or bolt (or, with `kind`, any shot without a blast) from
    /// `from` at `pos` with `vel`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        planet: usize,
        from: Shooter,
        kind: ShotKind,
        pos: DVec3,
        vel: DVec3,
        falloff: f32,
        range: f32,
        damage: u32,
    ) -> Self {
        Self {
            id: 0,
            planet,
            from,
            pos,
            vel,
            falloff,
            range,
            travelled: 0.0,
            damage,
            crit_chance: 0.0,
            crit_effect: 0.0,
            watchers: Vec::new(),
            kind,
            homing: None,
            fuse: 0,
            blast: None,
            resting: false,
        }
    }
}

/// The fixed roll a hit on creature `cid` makes for its crit: as a swing's
/// at a block, by who hit, at which input, and what.
pub(super) fn creature_crit(chance: f32, player: u32, at: u32, cid: u32) -> bool {
    is_crit(chance, player, at, BlockPos::new(cid as i64, -1, -(at as i64)))
}

/// How far back a player's hit is judged: the client's interpolation delay,
/// plus the inputs still waiting (their timeline runs behind), at most
/// [`MAX_REWIND_TICKS`].
pub(super) fn rewind_for(pending: usize) -> u32 {
    (REWIND_TICKS + pending as u32).min(MAX_REWIND_TICKS)
}

impl Game {
    /// Whether the cadence allows a use (swing, strike, shot) at input `at`.
    pub(super) fn ready_at(&self, id: u32, at: u32) -> bool {
        self.players.get(&id).is_some_and(|p| {
            p.last_swing
                .is_none_or(|(last, ticks)| at as u64 >= last as u64 + ticks as u64)
        })
    }

    /// Fires the sling in hotbar `slot` right after input `at`.
    pub(super) fn shoot(&mut self, id: u32, at: u32, slot: u8, yaw: u16, pitch: i16, charge: u16) {
        let Some(p) = self.players.get(&id) else { return };
        if p.state.down || slot as usize >= HOTBAR_SLOTS || !self.ready_at(id, at) {
            return tracing::debug!("refused shot from #{id} at {at}");
        }
        // Through its traits (milestone 17), as the holder stands now.
        let held = Held::of(&p.state, &self.planets[p.planet].day, self.tick as f64);
        let stack = p.inventory.get(slot as usize);
        let Some(sling) = stack.and_then(|s| self.items.get(s.item)?.sling_of(&s, &held)) else {
            return tracing::debug!("#{id} shot with no sling in slot {slot}");
        };
        // Many Shots: several pellets a trigger, each a share of the damage,
        // fanned out sideways; one energy and one wear for them all.
        let (shots, share) = many_shots(stack.map_or(0, |s| s.forge.level(TraitId::ManyShots)));
        let effects = p.effects;
        let sling = effects.skilled_sling(&sling);
        let rewind = rewind_for(p.pending.len());
        let pi = p.planet;
        let p = self.players.get_mut(&id).expect("present");
        // One cadence for swings, strikes and shots: the last use.
        p.last_swing = Some((at, swing_ticks(sling.action_speed)));
        // Too tired: nothing fires and nothing wears (the client predicts
        // the same, `try_spend`, whatever the charge).
        if !p.state.try_spend(sling.energy, bm_core::food::Drain::Ranged) {
            return;
        }
        let charge = if effects.charging {
            (charge as u32).min(CHARGE_TICKS)
        } else {
            0
        };
        let damage = (sling.damage as f64 * charge_multiplier(charge) * share)
            .round()
            .max(1.0) as u32;
        let eye = p.state.eye(&self.movement);
        let dir = look_dir(yaw, pitch);
        // One durability a shot, unless Careful Hands spares it.
        if roll(&mut self.rng) >= p.state.wear_spared(effects.wear_saved()) {
            let before = p.inventory.clone();
            let broke = wear_slot(&self.items, &mut p.inventory, slot as usize);
            send_slots(p, &before);
            if let Some(text) = broke {
                p.out.send(&ServerMsg::Notice { text });
            }
        }
        for i in 0..shots {
            let angle = (i as f64 - (shots - 1) as f64 * 0.5) * MANY_SHOTS_SPREAD;
            let (sin, cos) = angle.sin_cos();
            let d = DVec3::new(dir.x * cos - dir.z * sin, dir.y, dir.x * sin + dir.z * cos);
            let shot = Shot {
                crit_chance: sling.crit_chance,
                crit_effect: sling.crit_effect,
                ..Shot::new(
                    pi,
                    Shooter::Player { id, at, rewind },
                    ShotKind::Pellet,
                    eye,
                    d * sling.shot_speed as f64,
                    sling.falloff,
                    sling.range,
                    damage,
                )
            };
            self.launch(shot);
        }
        self.weapon_fired_near(pi, id, eye);
    }

    /// A weapon fired near a herd angers it (Boundless's wildstock).
    pub(super) fn weapon_fired_near(&mut self, pi: usize, id: u32, eye: DVec3) {
        let shape = self.planets[pi].world.shape;
        let kinds = &self.creature_kinds;
        let tick = self.tick;
        for c in self.herds[pi].list.iter_mut().filter(|c| c.alive()) {
            let def = kinds.get(c.kind).expect("present");
            if def.behaviour == bm_core::creatures::Behaviour::Herd
                && shape.delta(eye, c.state.pos).length() <= def.notice as f64
            {
                c.add_threat(id, 1.0);
                provoke(c, def, id, tick);
            }
        }
    }

    /// Puts a shot in flight and tells the players near it (never a player
    /// their own shot: they drew it already).
    pub(super) fn launch(&mut self, mut shot: Shot) {
        shot.id = self.next_shot;
        self.next_shot = self.next_shot.wrapping_add(1).max(1);
        let shape = self.planets[shot.planet].world.shape;
        let view = ServerMsg::Shot(ShotView {
            id: shot.id,
            from: match shot.from {
                Shooter::Player { id, .. } => Target::Player(id),
                Shooter::Creature(c) => Target::Creature(c),
            },
            pos: shot.pos.as_vec3().to_array(),
            vel: shot.vel.as_vec3().to_array(),
            falloff: shot.falloff,
            range: shot.range,
            kind: shot.kind,
            homing: shot.homing.map(Target::Player),
        });
        let shooter = match shot.from {
            Shooter::Player { id, .. } => Some(id),
            Shooter::Creature(_) => None,
        };
        for (oid, o) in &self.players {
            if o.planet == shot.planet
                && Some(*oid) != shooter
                && shape.delta(shot.pos, o.state.pos).length() <= SHOT_TELL_RANGE
            {
                o.out.send(&view);
                shot.watchers.push(*oid);
            }
        }
        self.shots.push(shot);
    }

    /// Flies every shot one tick. Pellets and bolts end on a solid block,
    /// after their range, or on what they hit (a creature, for a player's
    /// shot; a player, for a creature's). A homing bolt turns toward its
    /// player as it flies. A bomb falls, rests where it lands, and goes off
    /// when its fuse ends or when it touches what it is meant for.
    pub(super) fn step_shots(&mut self) {
        let mut shots = std::mem::take(&mut self.shots);
        let tick = self.tick;
        let m = self.movement;
        let mut ended = Vec::new();
        shots.retain_mut(|s| {
            let planet = &self.planets[s.planet];
            let world = &planet.world;
            let shape = world.shape;
            let bomb = s.blast.is_some();
            if bomb {
                s.fuse = s.fuse.saturating_sub(1);
            }
            let (pos, vel) = if s.resting {
                (s.pos, DVec3::ZERO)
            } else if let Some(pid) = s.homing {
                match self.players.get(&pid).filter(|p| p.planet == s.planet && !p.state.down) {
                    Some(p) => {
                        let chest = p.state.pos + DVec3::new(0.0, m.height * 0.5, 0.0);
                        homing_step(s.pos, s.vel, s.pos + shape.delta(s.pos, chest), HOMING_TURN_PER_TICK)
                    }
                    None => shot_step(s.pos, s.vel, 0.0),
                }
            } else {
                shot_step(s.pos, s.vel, s.falloff)
            };
            let step = pos - s.pos;
            let len = step.length();
            // What it would hit first along this tick's path: (fraction, what).
            let mut first: Option<(f64, Option<Target>)> = None;
            if len > 1e-9
                && let Some(h) = raycast_shaped(s.pos, step, len, |b| world.solid(b).then(|| world.shape(b)))
            {
                first = Some((h.distance / len, None));
            }
            match s.from {
                Shooter::Player { rewind, .. } => {
                    for c in self.herds[s.planet].list.iter().filter(|c| c.alive()) {
                        let def = self.creature_kinds.get(c.kind).expect("present");
                        let at = c.rewound(tick, rewind);
                        // The creature's box on the shot's lap.
                        let feet = s.pos + shape.delta(s.pos, at);
                        let (lo, hi) = creature_box(def, feet);
                        if let Some(t) = segment_hits_box(s.pos, pos, lo, hi)
                            && first.is_none_or(|(f, _)| t < f)
                        {
                            first = Some((t, Some(Target::Creature(c.id))));
                        }
                    }
                }
                Shooter::Creature(_) => {
                    for (pid, p) in self
                        .players
                        .iter()
                        .filter(|(_, p)| p.planet == s.planet && !p.state.down)
                    {
                        let feet = s.pos + shape.delta(s.pos, p.state.pos);
                        let lo = DVec3::new(feet.x - m.half_width, feet.y, feet.z - m.half_width);
                        let hi = DVec3::new(feet.x + m.half_width, feet.y + m.height, feet.z + m.half_width);
                        if let Some(t) = segment_hits_box(s.pos, pos, lo, hi)
                            && first.is_none_or(|(f, _)| t < f)
                        {
                            first = Some((t, Some(Target::Player(*pid))));
                        }
                    }
                }
            }
            let travelled = s.travelled + len;
            // How it ends, if it does: what it hit, and where.
            let end: Option<(Option<Target>, DVec3)> = match first {
                Some((t, Some(target))) => Some((Some(target), s.pos + step * t)),
                Some((t, None)) if bomb => {
                    // It comes to rest just short of where it struck.
                    let back = if len > 1e-9 {
                        (t * len - BOMB_REST_GAP).max(0.0) / len
                    } else {
                        0.0
                    };
                    s.pos = shape.wrap_point(s.pos + step * back);
                    s.vel = DVec3::ZERO;
                    s.resting = true;
                    s.travelled = travelled;
                    (s.fuse == 0).then_some((None, s.pos))
                }
                Some((t, None)) => Some((None, s.pos + step * t)),
                None if bomb && s.fuse == 0 => Some((None, pos)),
                None if !bomb && travelled >= s.range as f64 => Some((None, pos)),
                None => {
                    s.pos = shape.wrap_point(pos);
                    s.vel = vel;
                    s.travelled = travelled;
                    None
                }
            };
            match end {
                None => true,
                Some((hit, at)) => {
                    ended.push((
                        s.planet,
                        s.id,
                        std::mem::take(&mut s.watchers),
                        hit,
                        shape.wrap_point(at),
                        ShotHit::of(s),
                    ));
                    false
                }
            }
        });
        self.shots = shots;
        for (pi, id, watchers, hit, at, what) in ended {
            for w in watchers {
                if let Some(o) = self.players.get(&w) {
                    o.out.send(&ServerMsg::ShotEnd { id });
                }
            }
            if let Some(blast) = what.blast {
                match what.from {
                    Shooter::Player { id, at: input, .. } => self.player_blast(pi, id, input, at, &what, blast),
                    Shooter::Creature(cid) => self.creature_blast(pi, cid, at, what.damage, blast),
                }
                continue;
            }
            match (hit, what.from) {
                (Some(Target::Creature(cid)), Shooter::Player { id, at, .. }) => {
                    let crit = creature_crit(what.crit_chance, id, at, cid);
                    self.player_hits_creature(pi, id, cid, what.damage, crit, what.crit_effect, None);
                }
                (Some(Target::Player(pid)), Shooter::Creature(cid)) => {
                    self.creature_hits_player(pi, cid, pid, what.damage, DamageKind::Kinetic);
                }
                _ => {}
            }
        }
    }

    /// Swings the tool in hotbar `slot` at creature `cid`, right after input
    /// `at`: the cadence, energy and wear of a block swing.
    pub(super) fn strike(&mut self, id: u32, at: u32, cid: u32, slot: u8) {
        let Some(p) = self.players.get(&id) else { return };
        if p.state.down || slot as usize >= HOTBAR_SLOTS || !self.ready_at(id, at) {
            return tracing::debug!("refused strike from #{id} at {at}");
        }
        let pi = p.planet;
        let Some(c) = self.herds[pi].get(cid).filter(|c| c.alive()) else {
            return tracing::debug!("#{id} struck at no living creature {cid}");
        };
        let def = self.creature_kinds.get(c.kind).expect("present");
        let rewind = rewind_for(p.pending.len());
        let shape = self.planets[pi].world.shape;
        let reach = self.cfg.reach + p.effects.reach_bonus() + def.half_width as f64;
        let centre = c.rewound(self.tick, rewind) + DVec3::new(0.0, def.height as f64 * 0.5, 0.0);
        if shape.delta(p.state.eye(&self.movement), centre).length() > reach + 0.5 {
            return tracing::debug!("#{id} struck at creature {cid} out of reach");
        }
        let held = Held::of(&p.state, &self.planets[pi].day, self.tick as f64);
        let Some(tool) = swing_tool_of(&self.items, p.inventory.get(slot as usize), &held) else {
            return tracing::debug!("#{id} struck with no tool in slot {slot}");
        };
        let effects = p.effects;
        // A gem tool strikes with its gem's element (milestone 18 part 3).
        let element = p
            .inventory
            .get(slot as usize)
            .and_then(|s| self.items.get(s.item))
            .and_then(|d| d.element);
        let tool = skilled_tool_keen(&tool, &effects, p.state.crit_multiplier());
        let p = self.players.get_mut(&id).expect("present");
        p.last_swing = Some((at, swing_ticks(tool.action_speed)));
        if !p.state.try_spend(tool.energy, bm_core::food::Drain::Tool) {
            return;
        }
        if roll(&mut self.rng) >= p.state.wear_spared(effects.wear_saved()) {
            let before = p.inventory.clone();
            let broke = wear_slot(&self.items, &mut p.inventory, slot as usize);
            send_slots(p, &before);
            if let Some(text) = broke {
                p.out.send(&ServerMsg::Notice { text });
            }
        }
        let crit = creature_crit(tool.crit_chance, id, at, cid);
        self.player_hits_creature(pi, id, cid, tool.damage, crit, tool.crit_effect, element);
        // A hammer stuns a popper.
        if tool.class == Some(ToolClass::Hammer) {
            let tick = self.tick;
            if let Some(c) = self.herds[pi].get_mut(cid).filter(|c| c.alive()) {
                let def = self.creature_kinds.get(c.kind).expect("present");
                c.hammered(def, tick);
            }
        }
    }

    /// A player's hit lands on a creature: armour, threat, the numbers. A
    /// curled popper takes nothing; the first hit on a popper may curl it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn player_hits_creature(
        &mut self,
        pi: usize,
        id: u32,
        cid: u32,
        damage: u32,
        crit: bool,
        crit_effect: f32,
        element: Option<bm_core::creatures::Element>,
    ) {
        let tick = self.tick;
        let curl = roll(&mut self.rng);
        let resist_roll = roll(&mut self.rng) as f32;
        let Some(c) = self.herds[pi].get_mut(cid).filter(|c| c.alive()) else {
            return;
        };
        let def = self.creature_kinds.get(c.kind).expect("present");
        // An elemental creature shrugs some hits off and takes double from
        // its opposite (milestone 18 part 3, tuning 3.3.3).
        let scale = match c.element {
            Some(e) => elemental_scale(bm_core::creatures::resistance(e, element), resist_roll),
            None => 1,
        };
        let amount = c.struck(def, tick, curl, dealt(damage, crit, crit_effect, c.armour(def)) * scale);
        if let Some(p) = self.players.get_mut(&id) {
            p.state.fought();
            p.out.send(&ServerMsg::Hit {
                target: Target::Creature(cid),
                amount,
                crit,
            });
        }
        self.hurt_creature(pi, cid, Some(id), amount, true);
    }

    /// Takes `amount` of a creature's health; `by` (a player) gains threat
    /// and, if `provoked`, its anger. At 0 it dies.
    pub(super) fn hurt_creature(&mut self, pi: usize, cid: u32, by: Option<u32>, amount: u32, provoked: bool) {
        let threat = by
            .and_then(|id| self.players.get(&id))
            .map_or(1.0, |p| p.effects.threat());
        let tick = self.tick;
        let Some(c) = self.herds[pi].get_mut(cid).filter(|c| c.alive()) else {
            return;
        };
        let def = self.creature_kinds.get(c.kind).expect("present");
        c.damage = c.damage.saturating_add(amount);
        if let Some(id) = by {
            if !c.hurt_by.contains(&id) {
                c.hurt_by.push(id);
            }
            c.add_threat(id, amount.max(1) as f64 * threat);
            if provoked {
                provoke(c, def, id, tick);
            }
        }
        if c.damage >= c.max_health(def) {
            self.kill(pi, cid);
        }
    }

    /// A creature dies: its body stays for drops, every player who hurt it
    /// gets the kill's XP, and its column spawns nothing for a while.
    pub(super) fn kill(&mut self, pi: usize, cid: u32) {
        let tick = self.tick;
        let Some(c) = self.herds[pi].get_mut(cid) else { return };
        let def = self.creature_kinds.get(c.kind).expect("present");
        c.dead_at = Some(tick);
        c.mind = super::creatures::Mind::Calm;
        c.threat.clear();
        c.pose = bm_protocol::Pose::Dead;
        c.flying = false;
        c.beam = None;
        c.lift = None;
        let xp = c.xp(def) as u64;
        let hurt_by = std::mem::take(&mut c.hurt_by);
        let at = c.state.pos;
        tracing::debug!("{} {cid} died", def.title(c.tier));
        let col = column(BlockPos::containing(at.x, at.y, at.z));
        self.herds[pi].empty.insert(col, tick + EMPTY_AFTER_KILL_TICKS);
        for id in hurt_by {
            self.add_xp(id, xp);
        }
    }

    /// Takes this player's own drops from a body in reach (once a body).
    pub(super) fn loot(&mut self, id: u32, cid: u32) {
        let Some(p) = self.players.get(&id) else { return };
        if p.state.down {
            return;
        }
        let pi = p.planet;
        let Some(c) = self.herds[pi].get(cid).filter(|c| !c.alive()) else {
            return tracing::debug!("#{id} looted no body {cid}");
        };
        let def = self.creature_kinds.get(c.kind).expect("present");
        let shape = self.planets[pi].world.shape;
        let reach = self.cfg.reach + p.effects.reach_bonus() + def.half_width as f64;
        let centre = c.state.pos + DVec3::new(0.0, def.height as f64 * 0.5, 0.0);
        if shape.delta(p.state.eye(&self.movement), centre).length() > reach + 0.5 {
            return self.notice(id, "That is out of reach");
        }
        let luck = p.effects.luck_multiplier();
        let account = p.account.clone();
        let rng = &mut self.rng;
        // An exotic gives more rolls.
        let extra = c.exotic.extra_rolls();
        let mut table = def.tier(c.tier).pool_for(c.element);
        table.drops_min = table.drops_min.saturating_add(extra);
        table.drops_max = table.drops_max.saturating_add(extra);
        let drops = roll_drops(&table, luck, || roll(rng));
        let c = self.herds[pi].get_mut(cid).expect("present");
        if !c.take_loot(&account) {
            return tracing::debug!("#{id} looted body {cid} again");
        }
        let view = c.view(&account);
        let p = self.players.get_mut(&id).expect("present");
        let before = p.inventory.clone();
        let mut lost = false;
        for (name, n) in drops {
            match self.items.id(&name) {
                Ok(item) => lost |= p.inventory.add(&self.items, item, n) > 0,
                Err(_) => tracing::error!("drop `{name}` is not an item (checked at startup)"),
            }
        }
        send_slots(p, &before);
        p.out.send(&ServerMsg::Creature(view.clone()));
        p.known.insert(cid, view);
        if lost {
            p.out.send(&ServerMsg::Notice {
                text: "Your pack is full".into(),
            });
        }
    }

    /// A charge reaches player `pid`: the hit, and knockback along `dir`.
    pub(super) fn charge_hit(&mut self, pi: usize, cid: u32, pid: u32, dir: DVec3) {
        let Some(c) = self.herds[pi].get(cid) else { return };
        let def = self.creature_kinds.get(c.kind).expect("present");
        let damage = c.hit_damage(def);
        let knock = match def.attack {
            Attack::Charge { knockback, .. } => knockback as f64,
            _ => 0.0,
        };
        if let Some(p) = self.players.get_mut(&pid)
            && !p.state.down
        {
            // The client sees this as a correction (agreed for now).
            p.state.vel = dir * knock + DVec3::new(0.0, KNOCK_UP, 0.0);
            p.state.on_ground = false;
        }
        self.creature_hits_player(pi, cid, pid, damage, DamageKind::Kinetic);
    }

    /// A pelter's volley at player `pid`: its burst for its tier, fanned
    /// `spread` degrees apart, aimed to drop onto them.
    pub(super) fn volley(&mut self, pi: usize, cid: u32, pid: u32) {
        let (Some(c), Some(p)) = (self.herds[pi].get(cid), self.players.get(&pid)) else {
            return;
        };
        let def = self.creature_kinds.get(c.kind).expect("present");
        let Attack::Shot {
            speed,
            range,
            burst,
            spread,
            ..
        } = def.attack
        else {
            return;
        };
        let shape = self.planets[pi].world.shape;
        let from = c.state.pos + DVec3::new(0.0, def.height as f64 * 0.8, 0.0);
        let aim = p.state.pos + DVec3::new(0.0, self.movement.height * 0.5, 0.0);
        let d = shape.delta(from, aim);
        let flat = DVec3::new(d.x, 0.0, d.z).length().max(0.1);
        // Lift the aim by the drop over the flight.
        let t = flat / speed as f64;
        let lift = 0.5 * bm_core::combat::SHOT_GRAVITY * CREATURE_SHOT_FALLOFF as f64 * t * t;
        let base = DVec3::new(d.x, d.y + lift, d.z).normalize();
        let n = burst[(c.tier.clamp(1, 3) - 1) as usize].max(1);
        let damage = c.hit_damage(def);
        for k in 0..n {
            let off = (k as f64 - (n - 1) as f64 / 2.0) * (spread as f64).to_radians();
            let (s, co) = (off.sin(), off.cos());
            let dir = DVec3::new(base.x * co - base.z * s, base.y, base.x * s + base.z * co);
            let shot = Shot::new(
                pi,
                Shooter::Creature(cid),
                ShotKind::Pellet,
                from,
                dir * speed as f64,
                CREATURE_SHOT_FALLOFF,
                range,
                damage,
            );
            self.launch(shot);
        }
    }

    /// A creature's hit of `kind` on a player: armour and resistance, then
    /// health; at 0, defeat.
    pub(super) fn creature_hits_player(&mut self, pi: usize, cid: u32, pid: u32, amount: u32, kind: DamageKind) {
        let by = self.herds[pi]
            .get(cid)
            .map(|c| self.creature_kinds.get(c.kind).expect("present").title(c.tier))
            .unwrap_or_default();
        let r = roll(&mut self.rng);
        let Some(p) = self.players.get_mut(&pid) else { return };
        if p.state.down || p.planet != pi {
            return;
        }
        let amount = taken(amount, kind, &p.state.guarded(p.effects.defence()), r);
        p.state.take(amount);
        p.out.send(&ServerMsg::Hit {
            target: Target::Player(pid),
            amount,
            crit: false,
        });
        self.check_defeat(pid, &by);
    }

    /// If player `id` has just gone down, their defeat: Boundless's XP
    /// debt, `Defeated`, and every creature forgets them. Once a defeat.
    pub(super) fn check_defeat(&mut self, id: u32, by: &str) {
        let Some(p) = self.players.get_mut(&id) else { return };
        if !p.state.down || p.defeated {
            return;
        }
        p.defeated = true;
        let debt = defeat_debt(p.character.xp, p.effects.death_epic);
        p.character.debt = p.character.debt.saturating_add(debt);
        tracing::info!("{} was defeated by {by} (XP debt {debt})", p.name);
        p.out.send(&ServerMsg::Defeated { debt, by: by.into() });
        p.character_dirty = false;
        p.out.send(&ServerMsg::Character {
            character: p.character.clone(),
            at: 0,
        });
        let pi = p.planet;
        for c in self.herds[pi].list.iter_mut() {
            c.forget(id);
        }
    }

    /// A defeated player comes back on safe ground near where they fell, at
    /// [`RETURN_HEALTH_SHARE`] of their health.
    pub(super) fn return_player(&mut self, id: u32) {
        let Some(p) = self.players.get(&id) else { return };
        if !p.state.down {
            return;
        }
        let pi = p.planet;
        let fell = p.state.pos;
        let height = self.movement.height;
        let mut spot = None;
        for _ in 0..RETURN_TRIES {
            let a = roll(&mut self.rng) * std::f64::consts::TAU;
            let r = RETURN_MIN + roll(&mut self.rng) * (RETURN_MAX - RETURN_MIN);
            let (x, z) = (
                (fell.x + a.sin() * r).floor() as i64,
                (fell.z + a.cos() * r).floor() as i64,
            );
            let world = &self.planets[pi].world;
            if let Some(feet) = super::creatures::surface(world, x, z, fell.y as i64 + 16, fell.y as i64 - 32, height) {
                spot = Some(feet);
                break;
            }
        }
        let arrival = self.planets[pi].arrival();
        let p = self.players.get_mut(&id).expect("present");
        let mut s = PlayerState {
            pos: spot.unwrap_or(arrival.pos),
            vel: DVec3::ZERO,
            on_ground: true,
            hook: None,
            ..p.state
        };
        // Boundless's hunger penalty for a defeat (milestone 16), and the
        // sickness is gone.
        s.defeat_hunger();
        s.sick = 0;
        let health = (s.max_health() as f64 * RETURN_HEALTH_SHARE).round() as u32;
        s.revive(health);
        p.state = s;
        p.defeated = false;
        tracing::info!("{} returned at {:.1?}", p.name, s.pos);
        p.out.send(&ServerMsg::Teleport {
            spawn: s,
            tick: self.tick,
        });
        // Returning gives up a meteor life (Boundless: the sanctum).
        self.meteor_life_lost(id);
    }
}

/// The damage multiple of a hit on an elemental creature given its
/// resistance to the attack and a roll in 0..1: 0 when resisted, 2 when the
/// hit finds its weakness, else 1.
fn elemental_scale(resistance: f32, roll: f32) -> u32 {
    if resistance > 0.0 && roll < resistance {
        0
    } else if resistance < 0.0 && roll < -resistance {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod element_tests {
    use super::elemental_scale;

    #[test]
    fn resisted_hits_do_nothing_and_weak_spots_double() {
        assert_eq!(elemental_scale(0.4, 0.39), 0, "physical, resisted");
        assert_eq!(elemental_scale(0.4, 0.41), 1);
        assert_eq!(elemental_scale(-0.3, 0.29), 2, "its opposite");
        assert_eq!(elemental_scale(-0.3, 0.31), 1);
        assert_eq!(elemental_scale(0.0, 0.0), 1, "neutral");
    }
}

/// What a finished shot needs for its hit.
#[derive(Clone, Copy)]
pub(super) struct ShotHit {
    pub from: Shooter,
    pub damage: u32,
    pub crit_chance: f32,
    pub crit_effect: f32,
    pub blast: Option<BlastSpec>,
}

impl ShotHit {
    fn of(s: &Shot) -> Self {
        Self {
            from: s.from,
            damage: s.damage,
            crit_chance: s.crit_chance,
            crit_effect: s.crit_effect,
            blast: s.blast,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::creatures::Creature;
    use super::*;
    use bm_core::creatures::CreatureKinds;
    use bm_core::movement::TICK_HZ;

    #[test]
    fn rewind_grows_with_late_inputs_up_to_200_ms() {
        assert_eq!(rewind_for(0), 7);
        assert_eq!(rewind_for(3), 10);
        assert_eq!(rewind_for(50), 12);
        assert_eq!(MAX_REWIND_TICKS as f64 / TICK_HZ as f64, 0.2);
    }

    #[test]
    fn a_shot_flies_into_a_creature_box_where_it_stood() {
        // A stone sling's shot, flown as the server flies it, enters a
        // grazer's box 10 blocks ahead, and misses the place it moved to.
        let k = CreatureKinds::builtin();
        let def = k.get(k.id("grazer").unwrap()).unwrap();
        let mut c = Creature::new(1, 0, 1, DVec3::new(10.0, 0.0, 0.0), 0);
        c.born = 0;
        let mut pos = DVec3::new(0.0, 0.9, 0.0);
        let mut vel = DVec3::new(30.0, 0.0, 0.0);
        // It has since walked two blocks aside.
        for t in 1..=20u64 {
            c.history[(t % 16) as usize] = DVec3::new(10.0, 0.0, if t > 10 { 2.0 } else { 0.0 });
        }
        let now = 20;
        let mut hit_then = false;
        let mut hit_now = false;
        for _ in 0..30 {
            let (p, v) = shot_step(pos, vel, 0.5);
            let (lo, hi) = creature_box(def, c.rewound(now, 12));
            hit_then |= segment_hits_box(pos, p, lo, hi).is_some();
            let (lo, hi) = creature_box(def, c.rewound(now, 0));
            hit_now |= segment_hits_box(pos, p, lo, hi).is_some();
            (pos, vel) = (p, v);
        }
        assert!(hit_then, "where the shooter saw it");
        assert!(!hit_now, "not where it is now");
    }

    #[test]
    fn crits_are_fixed_by_the_hit() {
        assert_eq!(creature_crit(0.5, 3, 100, 7), creature_crit(0.5, 3, 100, 7));
        assert!(!creature_crit(0.0, 3, 100, 7));
        assert!(creature_crit(1.0, 3, 100, 7));
        let n = (0..2000).filter(|at| creature_crit(0.2, 1, *at, 9)).count();
        assert!((300..500).contains(&n), "{n}");
    }
}
