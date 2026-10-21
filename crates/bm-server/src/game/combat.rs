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
use super::{Game, send_slots, wear_slot};
use bm_core::combat::{CHARGE_TICKS, DamageKind, charge_multiplier, dealt, shot_step, taken};
use bm_core::creatures::{Attack, roll_drops};
use bm_core::mining::{is_crit, skilled_tool, swing_ticks};
use bm_core::movement::{PhysicsWorld, look_dir};
use bm_core::raycast::raycast_shaped;
use bm_core::skills::defeat_debt;
use bm_core::{BlockPos, HOTBAR_SLOTS, PlayerState};
use bm_protocol::{ServerMsg, ShotView, Target};
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
const SHOT_TELL_RANGE: f64 = 96.0;

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

/// A shot in flight.
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
}

/// The fixed roll a hit on creature `cid` makes for its crit: as a swing's
/// at a block, by who hit, at which input, and what.
fn creature_crit(chance: f32, player: u32, at: u32, cid: u32) -> bool {
    is_crit(chance, player, at, BlockPos::new(cid as i64, -1, -(at as i64)))
}

/// How far back a player's hit is judged: the client's interpolation delay,
/// plus the inputs still waiting (their timeline runs behind), at most
/// [`MAX_REWIND_TICKS`].
fn rewind_for(pending: usize) -> u32 {
    (REWIND_TICKS + pending as u32).min(MAX_REWIND_TICKS)
}

impl Game {
    /// Whether the cadence allows a use (swing, strike, shot) at input `at`.
    fn ready_at(&self, id: u32, at: u32) -> bool {
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
        let Some(sling) = p
            .inventory
            .get(slot as usize)
            .and_then(|s| self.items.get(s.item))
            .and_then(|d| d.sling())
        else {
            return tracing::debug!("#{id} shot with no sling in slot {slot}");
        };
        let effects = p.effects;
        let sling = effects.skilled_sling(&sling);
        let rewind = rewind_for(p.pending.len());
        let pi = p.planet;
        let p = self.players.get_mut(&id).expect("present");
        // One cadence for swings, strikes and shots: the last use.
        p.last_swing = Some((at, swing_ticks(sling.action_speed)));
        // Too tired: nothing fires and nothing wears (the client predicts
        // the same, `try_spend`, whatever the charge).
        if !p.state.try_spend(sling.energy) {
            return;
        }
        let charge = if effects.charging {
            (charge as u32).min(CHARGE_TICKS)
        } else {
            0
        };
        let damage = (sling.damage as f64 * charge_multiplier(charge)).round() as u32;
        let eye = p.state.eye(&self.movement);
        let vel = look_dir(yaw, pitch) * sling.shot_speed as f64;
        // One durability a shot, unless Careful Hands spares it.
        if roll(&mut self.rng) >= effects.wear_saved() {
            let before = p.inventory.clone();
            let broke = wear_slot(&self.items, &mut p.inventory, slot as usize);
            send_slots(p, &before);
            if let Some(text) = broke {
                p.out.send(&ServerMsg::Notice { text });
            }
        }
        let shot = Shot {
            id: self.next_shot,
            planet: pi,
            from: Shooter::Player { id, at, rewind },
            pos: eye,
            vel,
            falloff: sling.falloff,
            range: sling.range,
            travelled: 0.0,
            damage,
            crit_chance: sling.crit_chance,
            crit_effect: sling.crit_effect,
            watchers: Vec::new(),
        };
        self.launch(shot);
        // A weapon fired near a herd angers it (Boundless's wildstock).
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
    fn launch(&mut self, mut shot: Shot) {
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
            kind: bm_protocol::ShotKind::Pellet,
            homing: None,
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

    /// Flies every shot one tick: it ends on a solid block, after its range,
    /// or on what it hits (a creature, for a player's shot; a player, for a
    /// creature's).
    pub(super) fn step_shots(&mut self) {
        let mut shots = std::mem::take(&mut self.shots);
        let tick = self.tick;
        let m = self.movement;
        let mut ended = Vec::new();
        shots.retain_mut(|s| {
            let planet = &self.planets[s.planet];
            let world = &planet.world;
            let shape = world.shape;
            let (pos, vel) = shot_step(s.pos, s.vel, s.falloff);
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
            let end = match first {
                Some((_, hit)) => Some(hit),
                None if travelled >= s.range as f64 => Some(None),
                None => None,
            };
            match end {
                None => {
                    s.pos = shape.wrap_point(pos);
                    s.vel = vel;
                    s.travelled = travelled;
                    true
                }
                Some(hit) => {
                    ended.push((s.planet, s.id, std::mem::take(&mut s.watchers), hit, ShotHit::of(s)));
                    false
                }
            }
        });
        self.shots = shots;
        for (pi, id, watchers, hit, what) in ended {
            for w in watchers {
                if let Some(o) = self.players.get(&w) {
                    o.out.send(&ServerMsg::ShotEnd { id });
                }
            }
            match (hit, what.from) {
                (Some(Target::Creature(cid)), Shooter::Player { id, at, .. }) => {
                    let crit = creature_crit(what.crit_chance, id, at, cid);
                    self.player_hits_creature(pi, id, cid, what.damage, crit, what.crit_effect);
                }
                (Some(Target::Player(pid)), Shooter::Creature(cid)) => {
                    self.creature_hits_player(pi, cid, pid, what.damage);
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
        let item = p.inventory.get(slot as usize).map(|s| s.item);
        let Some(tool) = self.items.swing_tool(item) else {
            return tracing::debug!("#{id} struck with no tool in slot {slot}");
        };
        let effects = p.effects;
        let tool = skilled_tool(&tool, &effects);
        let p = self.players.get_mut(&id).expect("present");
        p.last_swing = Some((at, swing_ticks(tool.action_speed)));
        if !p.state.try_spend(tool.energy) {
            return;
        }
        if roll(&mut self.rng) >= effects.wear_saved() {
            let before = p.inventory.clone();
            let broke = wear_slot(&self.items, &mut p.inventory, slot as usize);
            send_slots(p, &before);
            if let Some(text) = broke {
                p.out.send(&ServerMsg::Notice { text });
            }
        }
        let crit = creature_crit(tool.crit_chance, id, at, cid);
        self.player_hits_creature(pi, id, cid, tool.damage, crit, tool.crit_effect);
    }

    /// A player's hit lands on a creature: armour, threat, the numbers.
    fn player_hits_creature(&mut self, pi: usize, id: u32, cid: u32, damage: u32, crit: bool, crit_effect: f32) {
        let Some(c) = self.herds[pi].get(cid).filter(|c| c.alive()) else {
            return;
        };
        let def = self.creature_kinds.get(c.kind).expect("present");
        let amount = dealt(damage, crit, crit_effect, def.tier(c.tier).armour);
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
        if c.damage >= def.tier(c.tier).health {
            self.kill(pi, cid);
        }
    }

    /// A creature dies: its body stays for drops, every player who hurt it
    /// gets the kill's XP, and its column spawns nothing for a while.
    fn kill(&mut self, pi: usize, cid: u32) {
        let tick = self.tick;
        let Some(c) = self.herds[pi].get_mut(cid) else { return };
        let def = self.creature_kinds.get(c.kind).expect("present");
        c.dead_at = Some(tick);
        c.mind = super::creatures::Mind::Calm;
        c.threat.clear();
        c.pose = bm_protocol::Pose::Dead;
        let xp = def.tier(c.tier).xp as u64;
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
        let drops = roll_drops(def.tier(c.tier), luck, || roll(rng));
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
        let damage = def.tier(c.tier).damage;
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
        self.creature_hits_player(pi, cid, pid, damage);
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
        let damage = def.tier(c.tier).damage;
        for k in 0..n {
            let off = (k as f64 - (n - 1) as f64 / 2.0) * (spread as f64).to_radians();
            let (s, co) = (off.sin(), off.cos());
            let dir = DVec3::new(base.x * co - base.z * s, base.y, base.x * s + base.z * co);
            let shot = Shot {
                id: self.next_shot,
                planet: pi,
                from: Shooter::Creature(cid),
                pos: from,
                vel: dir * speed as f64,
                falloff: CREATURE_SHOT_FALLOFF,
                range,
                travelled: 0.0,
                damage,
                crit_chance: 0.0,
                crit_effect: 0.0,
                watchers: Vec::new(),
            };
            self.launch(shot);
        }
    }

    /// A creature's hit on a player: armour and resistance, then health; at
    /// 0, defeat.
    pub(super) fn creature_hits_player(&mut self, pi: usize, cid: u32, pid: u32, amount: u32) {
        let by = self.herds[pi]
            .get(cid)
            .map(|c| self.creature_kinds.get(c.kind).expect("present").title(c.tier))
            .unwrap_or_default();
        let r = roll(&mut self.rng);
        let Some(p) = self.players.get_mut(&pid) else { return };
        if p.state.down || p.planet != pi {
            return;
        }
        let amount = taken(amount, DamageKind::Kinetic, &p.effects.defence(), r);
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
        let health = (s.max_health() as f64 * RETURN_HEALTH_SHARE).round() as u32;
        s.revive(health);
        p.state = s;
        p.defeated = false;
        tracing::info!("{} returned at {:.1?}", p.name, s.pos);
        p.out.send(&ServerMsg::Teleport {
            spawn: s,
            tick: self.tick,
        });
    }
}

/// What a finished shot needs for its hit.
#[derive(Clone, Copy)]
pub(super) struct ShotHit {
    pub from: Shooter,
    pub damage: u32,
    pub crit_chance: f32,
    pub crit_effect: f32,
}

impl ShotHit {
    fn of(s: &Shot) -> Self {
        Self {
            from: s.from,
            damage: s.damage,
            crit_chance: s.crit_chance,
            crit_effect: s.crit_effect,
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
