//! Health, damage and defeat (milestone 13, `docs/milestones/13-creatures-and-combat.md`,
//! tuning 3.3).
//!
//! Health lives in `PlayerState` beside energy, as integers, and is stepped
//! in `movement::step`: falls, the stand-in regeneration and Mending happen
//! there, so client and server agree on every tick. Hits from creatures are
//! the server's alone; the client adopts the server's health without
//! counting a correction (`bm-client` `predict.rs`).

use crate::movement::{PlayerState, TICK_HZ};

/// An untrained character's health (Boundless's base).
pub const BASE_HEALTH: u32 = 600;

/// Ticks without dealing or taking damage before the stand-in regeneration
/// starts: 10 s (a **guess**; Decided 2).
pub const CALM_TICKS: u16 = (TICK_HZ * 10) as u16;

/// Regeneration happens in steps every 5 s, as Boundless counted it.
pub const REGEN_PERIOD_TICKS: u16 = (TICK_HZ * 5) as u16;

/// Regeneration while Full (milestone 16; M13's stand-in until food, Nathan
/// 2026-10-21): 7 a step out of combat, 1.4 a second, a full 600 in about 7
/// minutes (the rate Boundless players reported for being fed; the 2018
/// food index says 4 a step).
pub const CALM_REGEN: u32 = 7;

/// Lava takes this percent of max health a burn: a third (players measured
/// 30% in 2017 and a third in 2018; forum check 2). No skill lessens it.
pub const LAVA_PERCENT: u32 = 33;
/// Ticks between lava burns: one at the touch, then one every second
/// (**guess**: the burns are periodic, but nobody timed them; Measure 6).
pub const LAVA_TICKS: u16 = crate::TICK_HZ as u16;

/// Mending (the health regeneration epic): +25 every 5 s, in combat too
/// (Boundless's skill text, tuning 5.4a).
pub const MENDING_REGEN: u32 = 25;

/// After letting go of a grapple, the first landing within this many ticks
/// takes no fall damage (Nathan, 2026-10-21: a swing let go too early must
/// not kill; 3 s is a **guess**).
pub const FALL_GRACE_TICKS: u16 = (TICK_HZ * 3) as u16;

/// Falls landing slower than this do no harm (blocks a second): a drop of
/// about 2.8 blocks at gravity 26. A **guess**.
pub const SAFE_IMPACT: f64 = 12.0;

/// Above [`SAFE_IMPACT`], the share of max health a fall takes grows as the
/// square of the extra speed over this span, reaching all of it at 40 (the
/// fastest fall), about 30 blocks. Slightly gentler than Boundless (Decided
/// 3); the curve is a **guess** to tune in play.
pub const LETHAL_SPAN: f64 = 28.0;

/// Landing on a soft block (soil, sand, snow, leaves) halves a fall's
/// damage (Boundless Release 197: "surface material matters"). The half is a
/// **guess**.
pub const SOFT_LANDING: f64 = 0.5;

/// Gravity on a shot with fall-off 1, in blocks a second squared: a sling's
/// `falloff` (0.5 or 0.6) scales it. Boundless publishes fall-off but not
/// what it means in metres: a **guess** (a stone sling's shot drops about a
/// block over its 15-block range).
pub const SHOT_GRAVITY: f64 = 20.0;

/// Draw (the charging epic): holding a sling charges it over 2 s
/// (Boundless's charge time) to `1 + charge effect` times the damage
/// (charge effect 1 on every slingbow: double).
pub const CHARGE_TICKS: u32 = 2 * TICK_HZ;

/// Damage multiplier for a shot held `ticks` with Draw.
pub fn charge_multiplier(ticks: u32) -> f64 {
    1.0 + (ticks.min(CHARGE_TICKS) as f64 / CHARGE_TICKS as f64)
}

/// One tick of a shot's flight: gravity, then movement. Returns the new
/// position and velocity. Deterministic, so a client can draw a shot the
/// server announced exactly where the server flies it.
pub fn shot_step(pos: glam::DVec3, vel: glam::DVec3, falloff: f32) -> (glam::DVec3, glam::DVec3) {
    let dt = crate::movement::TICK_DT;
    let vel = vel - glam::DVec3::new(0.0, SHOT_GRAVITY * falloff as f64 * dt, 0.0);
    (pos + vel * dt, vel)
}

/// A blast of `damage` reaches `radius` blocks, falling off linearly to half
/// at the edge (a **guess**: Boundless's bombs list a fall-off of 0.5).
/// `dist` beyond the radius takes nothing.
pub fn blast_damage(damage: u32, dist: f64, radius: f64) -> u32 {
    if dist > radius || radius <= 0.0 {
        return 0;
    }
    libm::round(damage as f64 * (1.0 - 0.5 * dist / radius)) as u32
}

/// A bomb's fuse, from lighting it: 3 s (Boundless's decoded bomb data; the
/// bag lists 3.85 s of "charge time"). A bomb still in hand when it runs out
/// is thrown first, as in Boundless.
pub const BOMB_FUSE_TICKS: u32 = 3 * TICK_HZ;

/// Holding a lit bomb this long throws it at full speed; a tap drops it at
/// your feet (Boundless Release 188). 1 s is a **guess**.
pub const THROW_FULL_TICKS: u32 = TICK_HZ;

/// Throw speed for a bomb held `hold` ticks, out of `full` (the bag's
/// projectile speed, 25 for every Boundless bag).
pub fn throw_speed(hold: u32, full: f32) -> f64 {
    full as f64 * (hold.min(THROW_FULL_TICKS) as f64 / THROW_FULL_TICKS as f64)
}

/// Gravity on a thrown bomb, blocks a second squared: its fall-off 0.5 times
/// [`SHOT_GRAVITY`] (a **guess**, as for shots).
pub const BOMB_GRAVITY: f64 = SHOT_GRAVITY * 0.5;

/// One tick of a homing bolt: turns its velocity toward `target` by at most
/// `turn` radians, keeping its speed, then moves. No gravity. Deterministic
/// (client and server fly it alike).
pub fn homing_step(pos: glam::DVec3, vel: glam::DVec3, target: glam::DVec3, turn: f64) -> (glam::DVec3, glam::DVec3) {
    let speed = vel.length();
    let want = (target - pos).normalize_or_zero();
    let have = vel.normalize_or_zero();
    let new = if want == glam::DVec3::ZERO || have == glam::DVec3::ZERO {
        vel
    } else {
        let cos = have.dot(want).clamp(-1.0, 1.0);
        let angle = libm::acos(cos);
        if angle <= turn {
            want * speed
        } else {
            // Rotate `have` toward `want` by `turn` within their plane.
            let ortho = (want - have * cos).normalize_or_zero();
            (have * libm::cos(turn) + ortho * libm::sin(turn)) * speed
        }
    };
    (pos + new * crate::movement::TICK_DT, new)
}

/// How fast a homing bolt turns: 1.5 radians a second (a **guess**: dodgeable
/// at a sprint).
pub const HOMING_TURN_PER_TICK: f64 = 1.5 / TICK_HZ as f64;

/// A character's health and defences from their skills.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Defence {
    pub kinetic_armour: u32,
    pub impulse_armour: u32,
    pub falling_armour: u32,
    /// Chance (0..1) a hit of that type does nothing.
    pub kinetic_resist: f32,
    pub impulse_resist: f32,
}

/// What hurts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageKind {
    /// One target: shots, charges, swings.
    Kinetic,
    /// An area: blasts, beams.
    Impulse,
    /// Landing hard.
    Falling,
}

/// Max health for a vitality attribute level (0 to 9) [tuning 3.3.4]:
/// `600 + ((vit - 10) / 90)^1.5 x 3400`, vit = 10 + 10 x level, rounded to
/// 10, then x3 with the health epic. Level 1 gives 730, 9 gives 4000.
pub fn max_health(vitality: f64, health_epic: bool) -> u32 {
    let v = (vitality / 9.0).clamp(0.0, 1.0);
    let h = (600.0 + 3400.0 * v * libm::sqrt(v)) / 10.0;
    let h = libm::round(h) as u32 * 10;
    if health_epic { h * 3 } else { h }
}

/// A hit of `amount` of `kind` after armour, or 0 if resisted. `roll` is a
/// number in 0..1 fixed by the hit (the server's).
pub fn taken(amount: u32, kind: DamageKind, d: &Defence, roll: f64) -> u32 {
    let (armour, resist) = match kind {
        DamageKind::Kinetic => (d.kinetic_armour, d.kinetic_resist),
        DamageKind::Impulse => (d.impulse_armour, d.impulse_resist),
        DamageKind::Falling => (d.falling_armour, 0.0),
    };
    if roll < resist as f64 {
        return 0;
    }
    amount.saturating_sub(armour)
}

/// Damage a hit deals to a creature [tuning 3.3.1]: `damage` (already
/// multiplied by skills), times `1 + crit_effect` on a critical, rounded,
/// less the target's armour, never below 0.
pub fn dealt(damage: u32, crit: bool, crit_effect: f32, armour: u32) -> u32 {
    let mult = if crit { 1.0 + crit_effect as f64 } else { 1.0 };
    (libm::round(damage as f64 * mult) as u32).saturating_sub(armour)
}

/// Fall damage for landing at `impact` blocks a second (positive, down) with
/// `max` health, before falling armour.
pub fn fall_damage(impact: f64, max: u32, soft: bool) -> u32 {
    if impact <= SAFE_IMPACT {
        return 0;
    }
    let x = ((impact - SAFE_IMPACT) / LETHAL_SPAN).min(1.0);
    let share = x * x * if soft { SOFT_LANDING } else { 1.0 };
    libm::round(share * max as f64) as u32
}

impl PlayerState {
    /// Health at full (600 plus what skills add, `Body::health_bonus`).
    /// Hearty (a food buff) adds 20 to 100%.
    pub fn max_health(&self) -> u32 {
        let base = BASE_HEALTH.saturating_add(self.body.health_bonus) as u64;
        (base * (100 + self.health_up_percent() as u64) / 100) as u32
    }

    /// Health left.
    pub fn health(&self) -> u32 {
        self.max_health().saturating_sub(self.hurt)
    }

    /// Takes `amount` (already after armour). At 0 health the player is
    /// down (defeated) until the server brings them back.
    pub fn take(&mut self, amount: u32) {
        if self.down {
            return;
        }
        self.calm = 0;
        self.hurt = self.hurt.saturating_add(amount).min(self.max_health());
        if self.hurt >= self.max_health() && !self.lifesave() {
            self.down = true;
            self.hook = None;
        }
    }

    /// Dealing damage counts as combat too: no calm regeneration for a while.
    pub fn fought(&mut self) {
        self.calm = 0;
    }

    /// Back on their feet with `health` (the server's respawn).
    /// One tick in or out of lava (milestone 18 part 3): touching it burns
    /// [`LAVA_PERCENT`] of max health at once, then again every
    /// [`LAVA_TICKS`] while in it; stepping out and back in within that
    /// time burns no sooner (Boundless's burns are periodic).
    pub(crate) fn lava_tick(&mut self, in_lava: bool) {
        self.lava_wait = self.lava_wait.saturating_sub(1);
        if in_lava && self.lava_wait == 0 && !self.down {
            // Lava Protection (a guild buff, milestone 25) takes some off.
            let burn = (self.max_health() * LAVA_PERCENT / 100).max(1);
            let kept = 100 - self.body.guild_lava_percent.min(100) as u32;
            self.take((burn * kept / 100).max(1));
            self.lava_wait = LAVA_TICKS;
        }
    }

    pub fn revive(&mut self, health: u32) {
        self.down = false;
        self.hurt = self.max_health().saturating_sub(health.max(1));
        self.calm = 0;
        self.loose = u16::MAX;
        // Back with full air (milestone 18): time to reach a shrine.
        self.air_used = 0;
    }

    /// One tick of healing: Mending every 5 s, and the calm regeneration
    /// every 5 s once 10 s have passed without combat, while Full
    /// (milestone 16). Sickness stops both.
    pub(crate) fn heal_tick(&mut self, tick_in_period: bool) {
        if self.down {
            return;
        }
        self.calm = self.calm.saturating_add(1);
        if !tick_in_period {
            return;
        }
        if self.sick > 0 {
            return;
        }
        let mut heal = 0;
        if self.body.mending {
            heal += MENDING_REGEN;
        }
        if self.calm >= CALM_TICKS && self.is_full() {
            heal += CALM_REGEN;
        }
        self.hurt = self.hurt.saturating_sub(heal);
    }

    /// Landing at `impact`: fall damage unless hooked or in the grace after
    /// letting go (which this landing uses up). Returns the damage taken.
    pub(crate) fn land(&mut self, impact: f64, soft: bool) -> u32 {
        let graced = self.loose < FALL_GRACE_TICKS;
        self.loose = u16::MAX;
        if graced || self.anchor().is_some() || self.down {
            return 0;
        }
        let raw = fall_damage(impact, self.max_health(), soft);
        let d = raw.saturating_sub(self.falling_armour());
        if d > 0 {
            self.take(d);
        }
        d
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn lava_protection_takes_its_share_off_the_burn() {
        let mut plain = crate::PlayerState::default();
        plain.lava_tick(true);
        let mut kept = crate::PlayerState::default();
        kept.body.guild_lava_percent = 50;
        kept.lava_tick(true);
        let full = plain.max_health() * LAVA_PERCENT / 100;
        assert_eq!((plain.hurt, kept.hurt), (full, full / 2));
    }

    use super::*;

    #[test]
    fn health_follows_boundless_vitality_curve() {
        // tuning 3.3.4: vit 10 600, 20 730, 30 960, 40 1250, 50 1610, 60 2010,
        // 70 2450, 80 2930, 90 3450, 100 4000.
        let want = [600, 730, 960, 1250, 1610, 2010, 2450, 2930, 3450, 4000];
        for (lvl, w) in want.iter().enumerate() {
            assert_eq!(max_health(lvl as f64, false), *w, "vitality level {lvl}");
        }
        assert_eq!(max_health(9.0, true), 12_000);
    }

    #[test]
    fn armour_and_resistance() {
        let d = Defence {
            kinetic_armour: 50,
            kinetic_resist: 0.25,
            ..Default::default()
        };
        assert_eq!(taken(100, DamageKind::Kinetic, &d, 0.9), 50);
        assert_eq!(taken(100, DamageKind::Kinetic, &d, 0.1), 0, "resisted");
        assert_eq!(taken(30, DamageKind::Kinetic, &d, 0.9), 0, "armour eats small hits");
        assert_eq!(taken(100, DamageKind::Impulse, &d, 0.1), 100);
    }

    #[test]
    fn dealt_matches_the_damage_calculator() {
        // tuning 3.3.1: Rift tooltip (2400 + 1020) x 1.95 = 6669.
        assert_eq!(dealt(6669, false, 0.75, 0), 6669);
        // A crit with effect 0.2: x1.2, then armour.
        assert_eq!(dealt(300, true, 0.2, 50), 310);
        assert_eq!(dealt(300, false, 0.2, 400), 0, "armour can stop a hit entirely");
    }

    #[test]
    fn falls() {
        assert_eq!(fall_damage(11.0, 600, false), 0);
        // About 11 blocks (24 m/s): a fifth.
        let d = fall_damage(24.0, 600, false);
        assert!((100..140).contains(&d), "{d}");
        assert_eq!(fall_damage(40.0, 600, false), 600, "the fastest fall is lethal");
        assert_eq!(fall_damage(40.0, 600, true), 300, "soft ground halves it");
    }

    #[test]
    fn grapples_never_cause_a_fall_death() {
        let mut s = PlayerState {
            loose: 30,
            ..Default::default()
        };
        assert_eq!(s.land(40.0, false), 0, "inside the grace after letting go");
        assert_eq!(s.land(40.0, false), 600, "the grace covers one landing");
        assert!(s.down);
    }

    #[test]
    fn shots_arc_and_draw_doubles() {
        let (mut p, mut v) = (glam::DVec3::ZERO, glam::DVec3::new(30.0, 0.0, 0.0));
        let mut ticks = 0;
        while p.x < 15.0 {
            (p, v) = shot_step(p, v, 0.5);
            ticks += 1;
        }
        assert_eq!(ticks, 30, "a stone sling's shot crosses its range in half a second");
        assert!((-1.5..-1.0).contains(&p.y), "drops about a block: {}", p.y);
        assert_eq!(charge_multiplier(0), 1.0);
        assert_eq!(charge_multiplier(CHARGE_TICKS * 2), 2.0);
    }

    #[test]
    fn falling_through_the_movement_step() {
        use crate::coords::BlockPos;
        use crate::movement::{MoveConfig, PhysicsWorld, PlayerInput, step};
        struct Floor(bool);
        impl PhysicsWorld for Floor {
            fn solid(&self, p: BlockPos) -> bool {
                p.y < 0
            }
            fn liquid(&self, _: BlockPos) -> bool {
                false
            }
            fn soft(&self, _: BlockPos) -> bool {
                self.0
            }
        }
        let shape = crate::planet::PlanetShape::new(8, 8);
        let fall = |height: f64, soft: bool, loose: u16| {
            let mut s = PlayerState {
                pos: glam::DVec3::new(10.0, height, 10.0),
                loose,
                ..Default::default()
            };
            for _ in 0..600 {
                step(
                    &mut s,
                    &PlayerInput::default(),
                    &MoveConfig::default(),
                    None,
                    shape,
                    &Floor(soft),
                );
            }
            s
        };
        assert_eq!(fall(2.0, false, u16::MAX).hurt, 0, "a short drop is harmless");
        let ten = fall(10.0, false, u16::MAX).hurt;
        assert!((60..160).contains(&ten), "ten blocks onto rock: {ten}");
        assert_eq!(
            fall(10.0, true, u16::MAX).hurt,
            ten.div_ceil(2).max(ten / 2),
            "soft ground halves it"
        );
        let s = fall(40.0, false, u16::MAX);
        assert!(s.down, "forty blocks onto rock is a defeat");
        assert_eq!(fall(40.0, false, 0).hurt, 0, "just let go of a grapple: no harm");
    }

    #[test]
    fn blasts_and_homing() {
        assert_eq!(blast_damage(900, 0.0, 3.0), 900);
        assert_eq!(blast_damage(900, 3.0, 3.0), 450);
        assert_eq!(blast_damage(900, 3.1, 3.0), 0);
        assert_eq!(throw_speed(0, 25.0), 0.0);
        assert_eq!(throw_speed(THROW_FULL_TICKS * 3, 25.0), 25.0);
        // A bolt fired sideways turns toward its target and keeps its speed.
        let (mut p, mut v) = (glam::DVec3::ZERO, glam::DVec3::new(0.0, 0.0, 10.0));
        let target = glam::DVec3::new(10.0, 0.0, 0.0);
        for _ in 0..120 {
            (p, v) = homing_step(p, v, target, HOMING_TURN_PER_TICK);
        }
        assert!((v.length() - 10.0).abs() < 1e-9);
        assert!(v.x > 0.0, "it has turned toward the target: {v:?}");
    }

    #[test]
    fn regeneration() {
        let mut s = PlayerState {
            loose: u16::MAX,
            ..Default::default()
        };
        s.take(100);
        for t in 0..CALM_TICKS as u32 + REGEN_PERIOD_TICKS as u32 {
            s.heal_tick(t % REGEN_PERIOD_TICKS as u32 == 0);
        }
        assert!(s.hurt < 100 && s.hurt >= 100 - 2 * CALM_REGEN, "{}", s.hurt);
        s.body.mending = true;
        s.take(200);
        let before = s.hurt;
        s.heal_tick(true);
        assert_eq!(before - s.hurt, MENDING_REGEN, "Mending works in combat");
    }
}
