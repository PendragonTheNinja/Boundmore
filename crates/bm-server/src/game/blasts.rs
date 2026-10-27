//! Bombs, blasts, beams and lifts on the server (milestone 13 part 2,
//! `docs/milestones/13-creatures-and-combat.md`): a player's thrown bomb on
//! the shared cadence, with energy and wear; blasts that hurt creatures and
//! blocks (never players) for a player's bomb, and players (never through
//! walls) for a popper or a drifter's bomb; a drifter's bolts, homing bolts,
//! lobbed bombs, beam and lift. Shots themselves fly in `combat.rs`.

use super::combat::{BlastSpec, SHOT_TELL_RANGE, Shooter, Shot, ShotHit, creature_crit, rewind_for};
use super::creatures::{BEAM_TICKS, KNOCK_UP, LIFT_HEIGHT, Volley, clear_sight, creature_box};
use super::mining::roll;
use super::{Game, send_slots, wear_slot};
use bm_core::block::Health;
use bm_core::combat::{BOMB_FUSE_TICKS, BOMB_GRAVITY, DamageKind, SHOT_GRAVITY, blast_damage, throw_speed};
use bm_core::creatures::Attack;
use bm_core::mining::{armour as block_armour, block_health, swing_ticks};
use bm_core::movement::{MoveConfig, PhysicsWorld, TICK_HZ, look_dir};
use bm_core::{BlockPos, BlockState, HOTBAR_SLOTS};
use bm_protocol::{ServerMsg, ShotKind};
use glam::DVec3;

/// Blocks take this share of a blast's damage, through the same health and
/// armour as swings: a **guess** (Boundless's bombs broke blocks; how hard
/// is not published).
pub(super) const BLAST_BLOCK_SHARE: f64 = 0.25;

/// A bomb's fall-off: `combat::shot_step` with it falls at
/// `combat::BOMB_GRAVITY`.
pub(super) const BOMB_FALLOFF: f32 = (BOMB_GRAVITY / SHOT_GRAVITY) as f32;

/// What a bomb's `ShotView::range` says: it ends by its fuse long before.
const BOMB_RANGE: f32 = 1000.0;

/// A drifter's bolts fly straight (no fall-off) this many times its range
/// before they are spent (a **guess**).
const BOLT_REACH: f32 = 1.25;
/// Its homing bolts fly at this share of its bolt speed, so a sprint can
/// outrun the turn (a **guess**), and twice as far as a bolt.
const HOMING_SPEED_SHARE: f32 = 0.6;
const HOMING_REACH: f32 = 2.0 * BOLT_REACH;
/// Its lobbed bombs: thrown at this speed (blocks a second), they go off
/// after 2 s or on touching a player, knocking back at this speed (all
/// **guesses**).
const DRIFTER_BOMB_SPEED: f64 = 12.0;
pub(super) const DRIFTER_BOMB_FUSE_TICKS: u32 = 2 * TICK_HZ;
const DRIFTER_BOMB_KNOCKBACK: f32 = 8.0;

impl Game {
    /// Throws a bomb from the bag in hotbar `slot` right after input `at`,
    /// held `hold` ticks since it was lit: the shared cadence, the bag's
    /// energy (too tired: nothing, no wear), one use, and a bomb in flight
    /// with what is left of its fuse.
    pub(super) fn throw(&mut self, id: u32, at: u32, slot: u8, yaw: u16, pitch: i16, hold: u16) {
        let Some(p) = self.players.get(&id) else { return };
        if p.state.down || slot as usize >= HOTBAR_SLOTS || !self.ready_at(id, at) {
            return tracing::debug!("refused throw from #{id} at {at}");
        }
        // Through its traits (milestone 17), as the holder stands now.
        let held = bm_core::forge::Held::of(&p.state, &self.planets[p.planet].day, self.tick as f64);
        let Some(bag) = p
            .inventory
            .get(slot as usize)
            .and_then(|s| self.items.get(s.item)?.bomb_of(&s, &held))
        else {
            return tracing::debug!("#{id} threw with no bombs in slot {slot}");
        };
        let effects = p.effects.brewed(&p.state);
        let bag = effects.skilled_bomb(&bag);
        let rewind = rewind_for(p.pending.len());
        let pi = p.planet;
        let p = self.players.get_mut(&id).expect("present");
        p.last_swing = Some((at, swing_ticks(bag.action_speed)));
        if !p.state.try_spend(bag.energy, bm_core::food::Drain::Ranged) {
            return;
        }
        let hold = (hold as u32).min(BOMB_FUSE_TICKS);
        let eye = p.state.eye(&self.movement);
        let vel = look_dir(yaw, pitch) * throw_speed(hold, bag.throw_speed);
        if roll(&mut self.rng) >= p.state.wear_spared(effects.wear_saved()) {
            let before = p.inventory.clone();
            let broke = wear_slot(&self.items, &mut p.inventory, slot as usize);
            send_slots(p, &before);
            if let Some(text) = broke {
                p.out.send(&ServerMsg::Notice { text });
            }
        }
        let shot = Shot {
            crit_chance: bag.crit_chance,
            crit_effect: bag.crit_effect,
            // Held to the end of its fuse, it goes off as it leaves the hand.
            fuse: (BOMB_FUSE_TICKS - hold).max(1),
            blast: Some(BlastSpec {
                radius: bag.radius,
                knockback: bag.knockback,
            }),
            ..Shot::new(
                pi,
                Shooter::Player { id, at, rewind },
                ShotKind::Bomb,
                eye,
                vel,
                BOMB_FALLOFF,
                BOMB_RANGE,
                bag.damage,
            )
        };
        self.launch(shot);
        self.weapon_fired_near(pi, id, eye);
    }

    /// Tells everyone near `pos` a blast of `radius` went off there.
    fn tell_blast(&self, pi: usize, pos: DVec3, radius: f32) {
        let shape = self.planets[pi].world.shape;
        let msg = ServerMsg::Blast {
            pos: pos.as_vec3().to_array(),
            radius,
        };
        for o in self.players.values().filter(|o| o.planet == pi) {
            if shape.delta(pos, o.state.pos).length() <= SHOT_TELL_RANGE {
                o.out.send(&msg);
            }
        }
    }

    /// A player's bomb goes off at `pos`: every creature in reach that the
    /// blast can see takes its share (crits by the fixed roll, armour,
    /// threat, XP on a kill) and is thrown back; blocks take a quarter and
    /// drop nothing. Players are never hurt.
    pub(super) fn player_blast(&mut self, pi: usize, id: u32, at: u32, pos: DVec3, what: &ShotHit, blast: BlastSpec) {
        self.tell_blast(pi, pos, blast.radius);
        let radius = blast.radius as f64;
        let world = &self.planets[pi].world;
        let shape = world.shape;
        let mut hits = Vec::new();
        for c in self.herds[pi].list.iter().filter(|c| c.alive()) {
            let def = self.creature_kinds.get(c.kind).expect("present");
            let (lo, hi) = creature_box(def, pos + shape.delta(pos, c.state.pos));
            let near = pos.clamp(lo, hi);
            let dist = near.distance(pos);
            let centre = (lo + hi) * 0.5;
            if dist <= radius && (clear_sight(world, pos, centre) || clear_sight(world, pos, near)) {
                hits.push((c.id, dist, away(pos, centre)));
            }
        }
        for (cid, dist, dir) in hits {
            let damage = blast_damage(what.damage, dist, radius);
            let crit = creature_crit(what.crit_chance, id, at, cid);
            self.player_hits_creature(pi, id, cid, damage, crit, what.crit_effect, None, Some("bomb"));
            if let Some(c) = self.herds[pi].get_mut(cid).filter(|c| c.alive() && !c.flying) {
                c.state.vel = dir * blast.knockback as f64 + DVec3::new(0.0, KNOCK_UP, 0.0);
                c.state.on_ground = false;
            }
        }
        self.blast_blocks(pi, id, pos, what.damage, radius);
    }

    /// Blocks within `radius` of `pos` take [`BLAST_BLOCK_SHARE`] of a
    /// blast of `damage` by player `id`, against their health and armour on
    /// this tier. One broken this way drops nothing and gives no XP
    /// (Boundless). Never inside a claim that is not theirs, nor props,
    /// machines, stations, storage, portals, claim stones or a shrine's
    /// ground.
    fn blast_blocks(&mut self, pi: usize, id: u32, pos: DVec3, damage: u32, radius: f64) {
        let Some(account) = self.players.get(&id).map(|p| p.account.clone()) else {
            return;
        };
        let tick = self.tick;
        let tier = self.planets[pi].tier;
        let shape = self.planets[pi].world.shape;
        let r = radius.ceil() as i64;
        let centre = BlockPos::containing(pos.x, pos.y, pos.z);
        for dx in -r..=r {
            for dy in -r..=r {
                for dz in -r..=r {
                    let b = centre.offset(dx, dy, dz);
                    if !shape.contains_y(b.y) {
                        continue;
                    }
                    let mid = DVec3::new(b.x as f64 + 0.5, b.y as f64 + 0.5, b.z as f64 + 0.5);
                    let dist = mid.distance(pos);
                    if dist > radius {
                        continue;
                    }
                    let b = shape.wrap_block(b);
                    let Some(state) = self.may_blast(pi, &account, b) else {
                        continue;
                    };
                    let def = self.planets[pi].world.registry.get(state.id);
                    let armour = if def.health == Health::Plant {
                        0
                    } else {
                        block_armour(tier)
                    };
                    let raw = blast_damage(damage, dist, radius) as f64 * BLAST_BLOCK_SHARE;
                    let dealt = (raw - armour as f64).max(0.0).floor() as u32;
                    if dealt == 0 {
                        continue;
                    }
                    let health = block_health(def, tier);
                    let total = self.damage[pi].get(b, state, tick).saturating_add(dealt);
                    if total >= health {
                        self.block_gone(pi, b, state.id, None);
                    } else {
                        self.damage[pi].set(b, state, total, tick);
                        self.tell_health(pi, b, total);
                    }
                }
            }
        }
    }

    /// The block at `canon` if a blast by `account` may damage it.
    fn may_blast(&self, pi: usize, account: &str, canon: BlockPos) -> Option<BlockState> {
        let planet = &self.planets[pi];
        let state = planet.world.state(canon)?;
        if state.id.is_air() {
            return None;
        }
        let def = planet.world.registry.get(state.id);
        let kept = !def.breakable
            || def.health == Health::Prop
            || def.mount.is_some()
            || def.station.is_some()
            || def.assembly.is_some()
            || def.storage > 0
            || state.id == self.claim_stone
            || state.id == self.portal_conduit
            || state.id == self.portal_surface;
        if kept
            || planet.guarded(canon)
            || self.portals.at(planet.id, canon).is_some()
            || self.meteor_guards(pi, canon).is_some()
        {
            return None;
        }
        if planet.claims.at_block(canon).is_some_and(|c| c.owner != account) {
            return None;
        }
        Some(state)
    }

    /// A blast of `damage` from creature `cid` at `pos`: every player within
    /// `radius` whom it can see (walls stop it) takes impulse damage, falling
    /// off to half at the edge, and is thrown back.
    pub(super) fn blast_players(&mut self, pi: usize, cid: u32, pos: DVec3, damage: u32, radius: f32, knockback: f32) {
        self.tell_blast(pi, pos, radius);
        let radius = radius as f64;
        let m = self.movement;
        let world = &self.planets[pi].world;
        let shape = world.shape;
        let mut hits = Vec::new();
        for (pid, p) in self.players.iter().filter(|(_, p)| p.planet == pi && !p.state.down) {
            let feet = pos + shape.delta(pos, p.state.pos);
            if let Some(dist) = blast_reaches(world, pos, radius, feet, &m) {
                let chest = feet + DVec3::new(0.0, m.height * 0.5, 0.0);
                hits.push((*pid, blast_damage(damage, dist, radius), away(pos, chest)));
            }
        }
        for (pid, amount, dir) in hits {
            if let Some(p) = self.players.get_mut(&pid) {
                // The client sees this as a correction, as a charge's.
                p.state.vel = dir * knockback as f64 + DVec3::new(0.0, KNOCK_UP, 0.0);
                p.state.on_ground = false;
            }
            self.creature_hits_player(pi, cid, pid, amount, DamageKind::Impulse);
        }
    }

    /// A creature's bomb goes off.
    pub(super) fn creature_blast(&mut self, pi: usize, cid: u32, pos: DVec3, damage: u32, blast: BlastSpec) {
        self.blast_players(pi, cid, pos, damage, blast.radius, blast.knockback);
    }

    /// A popper explodes: its blast, then its death. Its body stays to be
    /// looted; its attackers (if any) get the kill's XP.
    pub(super) fn popper_blast(&mut self, pi: usize, cid: u32) {
        let Some(c) = self.herds[pi].get(cid).filter(|c| c.alive()) else {
            return;
        };
        let def = self.creature_kinds.get(c.kind).expect("present");
        let Attack::Explode { radius, knockback, .. } = def.attack else {
            return;
        };
        let centre = c.state.pos + DVec3::new(0.0, def.height as f64 * 0.5, 0.0);
        let damage = c.hit_damage(def);
        let full = c.max_health(def);
        tracing::debug!("{} {cid} exploded", def.title(c.tier));
        self.blast_players(pi, cid, centre, damage, radius, knockback);
        if let Some(c) = self.herds[pi].get_mut(cid) {
            c.damage = full;
        }
        self.kill(pi, cid);
    }

    /// A drifter fires `volley` at player `pid`: a straight bolt, a homing
    /// bolt, or a lobbed bomb.
    pub(super) fn drifter_fire(&mut self, pi: usize, cid: u32, pid: u32, volley: Volley) {
        let (Some(c), Some(p)) = (self.herds[pi].get(cid), self.players.get(&pid)) else {
            return;
        };
        let def = self.creature_kinds.get(c.kind).expect("present");
        let Attack::Barrage {
            bolt_speed,
            range,
            bomb_radius,
            ..
        } = def.attack
        else {
            return;
        };
        let shape = self.planets[pi].world.shape;
        let from = c.state.pos + DVec3::new(0.0, def.height as f64 * 0.5, 0.0);
        let chest = p.state.pos + DVec3::new(0.0, self.movement.height * 0.5, 0.0);
        let d = shape.delta(from, chest);
        let dir = d.normalize_or_zero();
        let damage = c.hit_damage(def);
        let from_c = Shooter::Creature(cid);
        let shot = match volley {
            Volley::Bolt => Shot::new(
                pi,
                from_c,
                ShotKind::Bolt,
                from,
                dir * bolt_speed as f64,
                0.0,
                range * BOLT_REACH,
                damage,
            ),
            Volley::Homing => Shot {
                homing: Some(pid),
                ..Shot::new(
                    pi,
                    from_c,
                    ShotKind::Homing,
                    from,
                    dir * (bolt_speed * HOMING_SPEED_SHARE) as f64,
                    0.0,
                    range * HOMING_REACH,
                    damage,
                )
            },
            Volley::Bomb => {
                // Lobbed to drop onto them.
                let flat = DVec3::new(d.x, 0.0, d.z).length().max(0.1);
                let t = flat / DRIFTER_BOMB_SPEED;
                let lift = 0.5 * BOMB_GRAVITY * t * t;
                let aim = DVec3::new(d.x, d.y + lift, d.z).normalize_or_zero();
                Shot {
                    fuse: DRIFTER_BOMB_FUSE_TICKS,
                    blast: Some(BlastSpec {
                        radius: bomb_radius,
                        knockback: DRIFTER_BOMB_KNOCKBACK,
                    }),
                    ..Shot::new(
                        pi,
                        from_c,
                        ShotKind::Bomb,
                        from,
                        aim * DRIFTER_BOMB_SPEED,
                        BOMB_FALLOFF,
                        BOMB_RANGE,
                        damage,
                    )
                }
            }
            Volley::Beam => return,
        };
        self.launch(shot);
    }

    /// A drifter's beam starts on player `pid`: everyone near sees it.
    pub(super) fn beam_start(&mut self, pi: usize, cid: u32, pid: u32) {
        let Some(p) = self.players.get(&pid) else { return };
        let shape = self.planets[pi].world.shape;
        let chest = p.state.pos + DVec3::new(0.0, self.movement.height * 0.5, 0.0);
        let msg = ServerMsg::Beam {
            from: cid,
            to: chest.as_vec3().to_array(),
            ticks: BEAM_TICKS as u16,
        };
        for o in self.players.values().filter(|o| o.planet == pi) {
            if shape.delta(chest, o.state.pos).length() <= SHOT_TELL_RANGE {
                o.out.send(&msg);
            }
        }
    }

    /// A drifter's lift throws player `pid` straight up, high enough to
    /// rise [`LIFT_HEIGHT`] blocks; they fall back as from any height.
    pub(super) fn lift(&mut self, pi: usize, pid: u32) {
        let g = self.movement.gravity;
        let Some(p) = self.players.get_mut(&pid).filter(|p| p.planet == pi && !p.state.down) else {
            return;
        };
        p.state.vel = DVec3::new(0.0, (2.0 * g * LIFT_HEIGHT).sqrt(), 0.0);
        p.state.on_ground = false;
        tracing::debug!("{} was lifted", p.name);
    }
}

/// How far a blast at `pos` is from a player standing at `feet` (on its
/// lap), if it reaches them: within `radius` of their body, and in sight of
/// their feet, chest or eyes (walls stop a blast).
pub(super) fn blast_reaches(
    world: &impl PhysicsWorld,
    pos: DVec3,
    radius: f64,
    feet: DVec3,
    m: &MoveConfig,
) -> Option<f64> {
    let lo = DVec3::new(feet.x - m.half_width, feet.y, feet.z - m.half_width);
    let hi = DVec3::new(feet.x + m.half_width, feet.y + m.height, feet.z + m.half_width);
    let dist = pos.clamp(lo, hi).distance(pos);
    if dist > radius {
        return None;
    }
    [0.3, m.height * 0.5, m.eye_height]
        .iter()
        .any(|y| clear_sight(world, pos, feet + DVec3::new(0.0, *y, 0.0)))
        .then_some(dist)
}

/// The way from `from` to `to`, level and outward (nothing if one is
/// straight above the other).
fn away(from: DVec3, to: DVec3) -> DVec3 {
    let d = to - from;
    let flat = DVec3::new(d.x, 0.0, d.z);
    if flat.length() < 1e-6 {
        DVec3::ZERO
    } else {
        flat.normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::super::creatures::segment_hits_box;
    use super::*;
    use bm_core::combat::{HOMING_TURN_PER_TICK, homing_step};

    /// Ground below y 10 and, if asked, a wall at x 3.
    struct Ground {
        wall: bool,
    }

    impl PhysicsWorld for Ground {
        fn solid(&self, p: BlockPos) -> bool {
            p.y < 10 || (self.wall && p.x == 3 && p.y < 40)
        }
        fn liquid(&self, _: BlockPos) -> bool {
            false
        }
    }

    #[test]
    fn a_blast_reaches_players_in_its_radius_but_not_through_walls() {
        let m = MoveConfig::default();
        let pos = DVec3::new(5.5, 10.5, 0.5);
        let feet = DVec3::new(1.5, 10.0, 0.5);
        let open = Ground { wall: false };
        let d = blast_reaches(&open, pos, 4.0, feet, &m).expect("in the open");
        assert!((d - (4.0 - m.half_width)).abs() < 1e-9, "to the body's edge: {d}");
        assert!(blast_reaches(&open, pos, 3.0, feet, &m).is_none(), "beyond the radius");
        assert!(
            blast_reaches(&Ground { wall: true }, pos, 4.0, feet, &m).is_none(),
            "a wall stops it"
        );
        // Half damage at the edge.
        assert_eq!(blast_damage(400, 3.7, 4.0), 215);
    }

    #[test]
    fn a_homing_bolt_reaches_a_standing_target() {
        let m = MoveConfig::default();
        let k = bm_core::creatures::CreatureKinds::builtin();
        let def = k.get(k.id("drifter").unwrap()).unwrap();
        let Attack::Barrage { bolt_speed, range, .. } = def.attack else {
            panic!()
        };
        let speed = (bolt_speed * HOMING_SPEED_SHARE) as f64;
        let feet = DVec3::new(12.0, 10.0, 0.5);
        let lo = DVec3::new(feet.x - m.half_width, feet.y, feet.z - m.half_width);
        let hi = DVec3::new(feet.x + m.half_width, feet.y + m.height, feet.z + m.half_width);
        let chest = feet + DVec3::new(0.0, m.height * 0.5, 0.0);
        // Fired straight at them, and fired 60 degrees off (as if they had
        // stepped aside): both arrive within the bolt's reach.
        for off in [0.0f64, 60.0] {
            let from = DVec3::new(0.5, 16.0, 0.5);
            let dir = (chest - from).normalize();
            let (s, c) = off.to_radians().sin_cos();
            let dir = DVec3::new(dir.x * c - dir.z * s, dir.y, dir.x * s + dir.z * c);
            let (mut pos, mut vel) = (from, dir * speed);
            let mut travelled = 0.0;
            let mut hit = false;
            while travelled < (range * HOMING_REACH) as f64 {
                let (p, v) = homing_step(pos, vel, chest, HOMING_TURN_PER_TICK);
                if segment_hits_box(pos, p, lo, hi).is_some() {
                    hit = true;
                    break;
                }
                travelled += (p - pos).length();
                (pos, vel) = (p, v);
            }
            assert!(hit, "{off} degrees off: missed, ended at {pos}");
        }
    }

    #[test]
    fn a_lift_rises_about_fifteen_blocks() {
        let m = MoveConfig::default();
        let v = (2.0 * m.gravity * LIFT_HEIGHT).sqrt();
        assert!(v < m.max_speed, "under the speed cap");
        let mut s = bm_core::PlayerState {
            pos: DVec3::new(0.5, 10.0, 0.5),
            vel: DVec3::new(0.0, v, 0.0),
            ..Default::default()
        };
        let mut top: f64 = 0.0;
        let shape = bm_core::PlanetShape::new(8, 8);
        for _ in 0..120 {
            bm_core::movement::step(&mut s, &Default::default(), &m, None, shape, &Ground { wall: false });
            top = top.max(s.pos.y);
        }
        assert!((top - 25.0).abs() < 1.0, "rose to {top}");
    }
}
