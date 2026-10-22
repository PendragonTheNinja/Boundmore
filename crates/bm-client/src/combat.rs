//! Fighting on the client (milestone 13): when a sling fires, the shots in
//! the air, the damage numbers that float up, and the defeat screen's state.
//! Pure logic, tested without a window.
//!
//! The server rules every shot and hit (ADR-0004). The client fires on the
//! same input timeline the server judges: slings, strikes on creatures and
//! swings at blocks share one cooldown (`mining::Swinger`, the last use of
//! any of them, timed by `mining::swing_ticks` on the item's skilled action
//! speed), and a shot spends its energy right after its input as the server
//! does (`Predictor::spend`). Shots are only drawn here: ours at once, from
//! our eye, others' from `ServerMsg::Shot`, all flown by `combat::shot_step`
//! exactly as the server flies them.
//!
//! Bombs (milestone 13 part 2) light while the attack button is held
//! (`Fuse`) and are thrown when it is let go, or by themselves when the fuse
//! runs out in hand. Ours are drawn at once, falling at `BOMB_GRAVITY` and
//! resting where they land until their fuse ends; the blast itself is drawn
//! only when the server's `Blast` comes (`crate::effects`). Homing bolts turn
//! toward their target by `combat::homing_step`, as the server turns them.

use crate::creatures::ray_box;
use bm_core::combat::{
    BOMB_FUSE_TICKS, BOMB_GRAVITY, CHARGE_TICKS, HOMING_TURN_PER_TICK, SHOT_GRAVITY, homing_step, shot_step,
};
use bm_core::{BlockPos, PlanetShape, raycast};
use bm_protocol::{ShotKind, Target};
use glam::DVec3;

/// How a sling fires while the button is down (the sling epics, M12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireMode {
    /// One shot a click; a click while cooling down fires once ready.
    Click,
    /// Rapid Fire: shots keep coming while the button is held.
    Rapid,
    /// Draw: holding charges the shot (from when the sling is ready, up to
    /// `CHARGE_TICKS`); letting go fires it.
    Draw,
}

impl FireMode {
    pub fn of(effects: &bm_core::skills::Effects) -> Self {
        if effects.rapid_fire {
            FireMode::Rapid
        } else if effects.charging {
            FireMode::Draw
        } else {
            FireMode::Click
        }
    }
}

/// A shot to fire: right after input `at`, held `charge` ticks with Draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fire {
    pub at: u32,
    pub charge: u16,
}

/// The sling's trigger: turns the attack button into shots on the shared
/// cooldown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Trigger {
    /// A click waiting for the cooldown.
    queued: bool,
    /// Draw: the input the charge started at (once ready and held).
    charging: Option<u32>,
    held: bool,
}

impl Trigger {
    /// One frame: `pressed` if the button went down since the last frame,
    /// `held` if it is down now, this frame's inputs `first..=latest`, and
    /// the input the cooldown allows (`Swinger::ready_at`). Returns the shot
    /// to fire, if any. The caller starts the cooldown with it.
    pub fn update(
        &mut self,
        mode: FireMode,
        pressed: bool,
        held: bool,
        ready: u32,
        first: u32,
        latest: u32,
    ) -> Option<Fire> {
        let released = self.held && !held;
        self.held = held;
        // The earliest input this frame a shot may follow.
        let next = (ready <= latest).then(|| ready.max(first.min(latest)));
        match mode {
            FireMode::Click => {
                self.charging = None;
                self.queued |= pressed;
                let at = next.filter(|_| self.queued)?;
                self.queued = false;
                Some(Fire { at, charge: 0 })
            }
            FireMode::Rapid => {
                self.charging = None;
                self.queued |= pressed;
                let at = next.filter(|_| self.queued || held)?;
                self.queued = false;
                Some(Fire { at, charge: 0 })
            }
            FireMode::Draw => {
                if held || pressed {
                    if self.charging.is_none()
                        && let Some(at) = next
                    {
                        self.charging = Some(at);
                    }
                    if held {
                        return None;
                    }
                }
                // Let go (or a click shorter than a frame): fire what was
                // charged, or as soon as ready if it never started.
                if released || pressed {
                    match self.charging.take() {
                        Some(start) => {
                            // Let go somewhere in this frame: on its first input.
                            let at = first.min(latest).max(start);
                            let charge = (at - start).min(CHARGE_TICKS) as u16;
                            return Some(Fire { at, charge });
                        }
                        None => self.queued = true,
                    }
                }
                let at = next.filter(|_| self.queued)?;
                self.queued = false;
                Some(Fire { at, charge: 0 })
            }
        }
    }

    /// How far a Draw charge has come at input `now`, 0 to 1 (None when not
    /// charging), for the ring on screen.
    pub fn charge(&self, now: u32) -> Option<f32> {
        self.charging
            .map(|s| now.saturating_sub(s).min(CHARGE_TICKS) as f32 / CHARGE_TICKS as f32)
    }

    /// Forgets a queued click and a charge (the sling was put away).
    pub fn reset(&mut self) {
        *self = Self {
            held: self.held,
            ..Self::default()
        };
    }
}

/// A bomb to throw: right after input `at`, held `hold` ticks since it was
/// lit (`ClientMsg::Throw`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toss {
    pub at: u32,
    pub hold: u16,
}

/// A bag of bombs' trigger: holding the attack button lights a bomb once the
/// shared cooldown allows; letting go throws it, harder the longer it was
/// held (`combat::throw_speed`); a fuse that runs out in hand throws it by
/// itself, and the next bomb waits for a new press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fuse {
    /// The input the bomb in hand was lit at.
    lit: Option<u32>,
    /// The fuse ran out in hand: nothing lights until the button is let go.
    spent: bool,
}

impl Fuse {
    /// One frame, as `Trigger::update`: `pressed` if the button went down
    /// since the last frame, `held` if it is down now, this frame's inputs
    /// `first..=latest`, and the input the cooldown allows. Returns the
    /// throw, if any; the caller starts the cooldown at its `at`.
    pub fn update(&mut self, pressed: bool, held: bool, ready: u32, first: u32, latest: u32) -> Option<Toss> {
        if !held {
            self.spent = false;
        }
        match self.lit {
            None => {
                if self.spent || !(held || pressed) || ready > latest {
                    // A tap let go before the cooldown allowed lights nothing.
                    return None;
                }
                let at = ready.max(first.min(latest));
                if held {
                    self.lit = Some(at);
                    None
                } else {
                    // A click shorter than a frame: lit and let go at once,
                    // dropped at our feet.
                    Some(Toss { at, hold: 0 })
                }
            }
            Some(lit) if held => {
                let end = lit + BOMB_FUSE_TICKS;
                (end <= latest).then(|| {
                    self.lit = None;
                    self.spent = true;
                    Toss {
                        at: end,
                        hold: BOMB_FUSE_TICKS as u16,
                    }
                })
            }
            Some(lit) => {
                // Let go somewhere in this frame: on its first input.
                self.lit = None;
                let at = first.min(latest).max(lit);
                Some(Toss {
                    at,
                    hold: (at - lit).min(BOMB_FUSE_TICKS) as u16,
                })
            }
        }
    }

    /// How much of the lit bomb's fuse has burned at input `now`, 0 to 1
    /// (None with no bomb lit), for the ring on screen and the bomb in hand.
    pub fn burned(&self, now: u32) -> Option<f32> {
        self.lit
            .map(|l| now.saturating_sub(l).min(BOMB_FUSE_TICKS) as f32 / BOMB_FUSE_TICKS as f32)
    }

    /// Ticks the lit bomb has been held at input `now`.
    pub fn held_for(&self, now: u32) -> Option<u32> {
        self.lit.map(|l| now.saturating_sub(l))
    }

    /// Forgets a lit bomb (the bag was put away: nothing was sent, so
    /// nothing happened).
    pub fn reset(&mut self) {
        self.lit = None;
    }
}

/// Fall-off that makes `shot_step` fall at `BOMB_GRAVITY`.
pub const BOMB_FALLOFF: f32 = (BOMB_GRAVITY / SHOT_GRAVITY) as f32;

/// Others' bombs are drawn at most this long (ticks) when no `ShotEnd`
/// comes: a safety net, longer than any fuse.
const OTHERS_BOMB_TICKS: u32 = 10 * bm_core::TICK_HZ;

/// A shot drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub pos: DVec3,
    /// The way it flies (unit; zero for a bomb at rest).
    pub dir: DVec3,
    pub mine: bool,
    pub kind: ShotKind,
    /// A bomb's fuse burned, 0 to 1 (ours; others' are unknown: None).
    pub burned: Option<f32>,
}

/// A shot drawn in the air.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    /// The server's id for others' shots; ours have none.
    pub id: Option<u32>,
    pub pos: DVec3,
    /// Where it was a tick ago, for drawing between ticks and the trail.
    pub prev: DVec3,
    pub vel: DVec3,
    pub falloff: f32,
    /// Blocks it may still fly.
    pub left: f64,
    /// Ticks before it starts flying (others' shots wait so they leave the
    /// shooter's hand where we draw the shooter).
    pub wait: u32,
    /// Ticks it flies on after `ShotEnd` (others' shots), then it goes.
    pub ending: Option<u32>,
    /// Ours: stops at a living creature's box (bombs do not).
    pub mine: bool,
    pub kind: ShotKind,
    /// What a homing bolt turns toward.
    pub homing: Option<Target>,
    /// A bomb's ticks left: ours its fuse (it goes when it ends: the blast
    /// is the server's to say), others' a safety net.
    pub fuse: Option<u32>,
    /// A bomb lying where it landed.
    pub resting: bool,
}

/// Others' shots are held back this many ticks: we draw everyone else
/// `INTERP_DELAY_TICKS` in the past, so their shots start there too.
pub const SERVER_SHOT_DELAY: u32 = crate::predict::INTERP_DELAY_TICKS as u32;

#[derive(Clone, Debug, Default)]
pub struct Shots {
    pub list: Vec<Shot>,
}

impl Shots {
    /// Our own shot, fired from `from` along `dir` (unit) at `speed`.
    pub fn fire(&mut self, from: DVec3, dir: DVec3, speed: f64, range: f64, falloff: f32) {
        self.list.push(Shot {
            id: None,
            pos: from,
            prev: from,
            vel: dir * speed,
            falloff,
            left: range,
            wait: 0,
            ending: None,
            mine: true,
            kind: ShotKind::Pellet,
            homing: None,
            fuse: None,
            resting: false,
        });
    }

    /// Our own bomb, thrown from `from` along `dir` (unit) at `speed`, its
    /// fuse `fuse` ticks from going off.
    pub fn throw(&mut self, from: DVec3, dir: DVec3, speed: f64, fuse: u32) {
        self.list.push(Shot {
            id: None,
            pos: from,
            prev: from,
            vel: dir * speed,
            falloff: BOMB_FALLOFF,
            left: f64::INFINITY,
            wait: 0,
            ending: None,
            mine: true,
            kind: ShotKind::Bomb,
            homing: None,
            fuse: Some(fuse),
            resting: false,
        });
    }

    /// A blast at `pos` of `radius`: our own bombs within it are what went
    /// off (the server's word came before our fuse ended).
    pub fn blast(&mut self, pos: DVec3, radius: f64, shape: PlanetShape) {
        self.list
            .retain(|s| !(s.mine && s.kind == ShotKind::Bomb && shape.delta(pos, s.pos).length() <= radius.max(1.5)));
    }

    /// `ServerMsg::Shot`: someone else's (or a creature's) shot.
    pub fn server(&mut self, s: &bm_protocol::ShotView) {
        let pos = DVec3::from_array(s.pos.map(|v| v as f64));
        self.list.retain(|x| x.id != Some(s.id));
        self.list.push(Shot {
            id: Some(s.id),
            pos,
            prev: pos,
            vel: DVec3::from_array(s.vel.map(|v| v as f64)),
            falloff: s.falloff,
            left: s.range as f64,
            wait: SERVER_SHOT_DELAY,
            ending: None,
            mine: false,
            kind: s.kind,
            homing: s.homing.filter(|_| s.kind == ShotKind::Homing),
            fuse: (s.kind == ShotKind::Bomb).then_some(OTHERS_BOMB_TICKS),
            resting: false,
        });
    }

    /// `ServerMsg::ShotEnd`: it hit or ran out; drawn on for the delay its
    /// start was held back, so it ends where the server ended it.
    pub fn end(&mut self, id: u32) {
        if let Some(s) = self.list.iter_mut().find(|s| s.id == Some(id)) {
            s.ending = Some(SERVER_SHOT_DELAY);
        }
    }

    /// One tick of every shot. `solid` says what stops a shot; `creature`
    /// whether a segment (from, to) enters a living creature's box (our own
    /// shots only: the server decides others'); `target` where a homing
    /// bolt's target is now (its chest), if known.
    pub fn tick(
        &mut self,
        shape: PlanetShape,
        solid: impl Fn(BlockPos) -> bool,
        creature: impl Fn(DVec3, DVec3) -> bool,
        target: impl Fn(Target) -> Option<DVec3>,
    ) {
        self.list.retain_mut(|s| {
            s.prev = s.pos;
            if s.wait > 0 {
                s.wait -= 1;
                return true;
            }
            if let Some(n) = &mut s.ending {
                if *n == 0 {
                    return false;
                }
                *n -= 1;
            }
            if let Some(n) = &mut s.fuse {
                if *n == 0 {
                    return false;
                }
                *n -= 1;
            }
            if s.kind == ShotKind::Bomb {
                return bomb_tick(s, shape, &solid);
            }
            let (pos, vel) = match (s.kind, s.homing.and_then(&target)) {
                (ShotKind::Homing, Some(t)) => {
                    homing_step(s.pos, s.vel, shape.nearest_lap(s.pos, t), HOMING_TURN_PER_TICK)
                }
                // A homing bolt with nothing to turn to flies straight.
                (ShotKind::Homing, None) => (s.pos + s.vel * bm_core::movement::TICK_DT, s.vel),
                _ => shot_step(s.pos, s.vel, s.falloff),
            };
            let step = pos - s.pos;
            let len = step.length();
            if len <= 0.0 || s.left <= 0.0 {
                return false;
            }
            let dir = step / len;
            if let Some(hit) = raycast(s.pos, dir, len.min(s.left), &solid) {
                s.pos += dir * hit.distance;
                return false;
            }
            if s.mine && creature(s.pos, pos) {
                return false;
            }
            s.left -= len;
            s.pos = shape.wrap_point(pos);
            s.prev = shape.nearest_lap(s.pos, s.prev);
            s.vel = vel;
            s.left > 0.0
        });
    }

    /// Shots to draw this frame, `alpha` of the way through the tick.
    /// Waiting shots are not drawn.
    pub fn drawn(&self, alpha: f64) -> Vec<Drawn> {
        self.list
            .iter()
            .filter(|s| s.wait == 0)
            .map(|s| Drawn {
                pos: s.prev + (s.pos - s.prev) * alpha,
                dir: s.vel.normalize_or_zero(),
                mine: s.mine,
                kind: s.kind,
                burned: s
                    .fuse
                    .filter(|_| s.mine)
                    .map(|n| 1.0 - n as f32 / BOMB_FUSE_TICKS as f32),
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }
}

/// How far above the ground a resting bomb's middle lies.
pub const BOMB_REST: f64 = 0.15;

/// One tick of a bomb: it falls at `BOMB_GRAVITY` until it meets a block.
/// Landing on a floor it rests there; striking a wall or a ceiling it stops
/// short and drops. Returns whether it is still drawn.
fn bomb_tick(s: &mut Shot, shape: PlanetShape, solid: &impl Fn(BlockPos) -> bool) -> bool {
    if s.resting {
        return true;
    }
    let (pos, vel) = shot_step(s.pos, s.vel, s.falloff);
    let step = pos - s.pos;
    let len = step.length();
    if len <= 0.0 {
        return true;
    }
    let dir = step / len;
    match raycast(s.pos, dir, len, solid) {
        Some(hit) if hit.normal == [0, 1, 0] || hit.normal == [0, 0, 0] => {
            s.pos = shape.wrap_point(hit.point + DVec3::new(0.0, BOMB_REST, 0.0));
            s.vel = DVec3::ZERO;
            s.resting = true;
        }
        Some(hit) => {
            // Off a wall: stop just short of it, and fall from there.
            let n = DVec3::new(hit.normal[0] as f64, hit.normal[1] as f64, hit.normal[2] as f64);
            s.pos = shape.wrap_point(hit.point + n * 0.1);
            s.vel = DVec3::ZERO;
        }
        None => {
            s.pos = shape.wrap_point(pos);
            s.vel = vel;
        }
    }
    s.prev = shape.nearest_lap(s.pos, s.prev);
    true
}

/// Whether the segment from `a` to `b` passes through the box.
pub fn segment_hits(a: DVec3, b: DVec3, lo: DVec3, hi: DVec3) -> bool {
    let d = b - a;
    let len = d.length();
    if len <= 0.0 {
        return a.cmpge(lo).all() && a.cmple(hi).all();
    }
    ray_box(a, d / len, lo, hi).is_some_and(|t| t <= len)
}

/// Seconds a damage number floats.
pub const FLOAT_SECONDS: f64 = 1.1;

/// Where a number floats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    /// Over a point in the world (a creature), canonical.
    World(DVec3),
    /// Ours: by the crosshair.
    Me,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Floater {
    pub at: Anchor,
    pub amount: u32,
    pub crit: bool,
    /// Damage we took (red), not damage we dealt.
    pub taken: bool,
    pub born: f64,
    /// A little sideways spread, so numbers in a row do not stack.
    pub drift: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Floaters {
    pub list: Vec<Floater>,
    count: u32,
}

impl Floaters {
    pub fn add(&mut self, at: Anchor, amount: u32, crit: bool, taken: bool, now: f64) {
        self.count = self.count.wrapping_add(1);
        let drift = ((self.count.wrapping_mul(2_654_435_761) >> 16) % 100) as f32 / 100.0 - 0.5;
        self.list.push(Floater {
            at,
            amount,
            crit,
            taken,
            born: now,
            drift,
        });
    }

    /// Drops the finished ones.
    pub fn update(&mut self, now: f64) {
        self.list.retain(|f| now - f.born < FLOAT_SECONDS);
    }

    /// Each number with how far it has risen (0 to 1) and its opacity.
    pub fn visible(&self, now: f64) -> Vec<(Floater, f32, f32)> {
        self.list
            .iter()
            .filter_map(|f| {
                let t = ((now - f.born) / FLOAT_SECONDS) as f32;
                (0.0..1.0).contains(&t).then(|| {
                    let rise = 1.0 - (1.0 - t) * (1.0 - t);
                    let alpha = if t < 0.7 { 1.0 } else { (1.0 - t) / 0.3 };
                    (*f, rise, alpha)
                })
            })
            .collect()
    }
}

/// Health we lost that no `Hit` explained (a fall) is shown as its own
/// number after this long, giving a `Hit` that lags the snapshot time to
/// arrive.
pub const UNEXPLAINED_DELAY: f64 = 0.3;
/// A `Hit` waits this long for the health drop it explains.
const CREDIT_SECONDS: f64 = 1.0;

/// Matches health drops with the server's `Hit`s, so every loss shows one
/// number: a hit's when it comes, a fall's of its own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Losses {
    /// Damage told by `Hit`s and not yet seen as a drop.
    credit: u32,
    credit_at: f64,
    /// Drops no `Hit` explains yet.
    pending: u32,
    pending_at: f64,
    last_hurt: Option<u32>,
}

impl Losses {
    /// A `Hit` on us of `amount`.
    pub fn hit(&mut self, amount: u32, now: f64) {
        self.credit += amount;
        self.credit_at = now;
        // A drop seen before its `Hit` is explained now.
        let used = self.pending.min(self.credit);
        self.pending -= used;
        self.credit -= used;
    }

    /// Health lost so far (`PlayerState::hurt`) this frame. Returns a loss
    /// to show on its own (a fall), once it is clear no `Hit` explains it.
    pub fn observe(&mut self, hurt: u32, now: f64) -> Option<u32> {
        if now - self.credit_at > CREDIT_SECONDS {
            self.credit = 0;
        }
        if let Some(was) = self.last_hurt
            && hurt > was
        {
            let drop = hurt - was;
            let used = drop.min(self.credit);
            self.credit -= used;
            if drop > used {
                if self.pending == 0 {
                    self.pending_at = now;
                }
                self.pending += drop - used;
            }
        }
        self.last_hurt = Some(hurt);
        if self.pending > 0 && now - self.pending_at >= UNEXPLAINED_DELAY {
            return Some(std::mem::take(&mut self.pending));
        }
        None
    }

    /// Starts again from `hurt` (a respawn, a journey).
    pub fn reset(&mut self, hurt: u32) {
        *self = Self {
            last_hurt: Some(hurt),
            ..Self::default()
        };
    }
}

/// The defeat screen's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Defeat {
    #[default]
    Standing,
    /// Down: who did it (empty until the server says) and the XP debt added.
    Down { by: String, debt: u64 },
    /// Return was pressed at `since` (seconds); waiting to be brought back.
    Returning { by: String, debt: u64, since: f64 },
}

/// Return may be pressed again if the server has not answered in this long.
const RETURN_RETRY: f64 = 3.0;

impl Defeat {
    /// `ServerMsg::Defeated`.
    pub fn defeated(&mut self, by: String, debt: u64) {
        *self = Defeat::Down { by, debt };
    }

    /// Our predicted state says we are `down` (or not). Down without a
    /// `Defeated` yet still shows the screen; up again ends it.
    pub fn observe(&mut self, down: bool) {
        match (down, &self) {
            (true, Defeat::Standing) => {
                *self = Defeat::Down {
                    by: String::new(),
                    debt: 0,
                }
            }
            (false, Defeat::Down { .. } | Defeat::Returning { .. }) => *self = Defeat::Standing,
            _ => {}
        }
    }

    pub fn is_down(&self) -> bool {
        !matches!(self, Defeat::Standing)
    }

    /// Return pressed at `now`: whether to send `ClientMsg::Return` (once,
    /// unless the server is slow to answer).
    pub fn press_return(&mut self, now: f64) -> bool {
        match self {
            Defeat::Down { by, debt } => {
                *self = Defeat::Returning {
                    by: std::mem::take(by),
                    debt: *debt,
                    since: now,
                };
                true
            }
            Defeat::Returning { since, .. } if now - *since > RETURN_RETRY => {
                *since = now;
                true
            }
            _ => false,
        }
    }

    /// Who defeated us and the debt, for the screen.
    pub fn details(&self) -> Option<(&str, u64, bool)> {
        match self {
            Defeat::Standing => None,
            Defeat::Down { by, debt } => Some((by, *debt, false)),
            Defeat::Returning { by, debt, .. } => Some((by, *debt, true)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::mining::swing_ticks;
    use bm_core::movement::PlayerState;

    /// Runs a trigger over `frames` frames of 3 inputs each, the button held
    /// as `held(frame)` says, firing on the shared cooldown with an item
    /// swinging every `ticks`. Returns the inputs shots followed.
    fn run(mode: FireMode, ticks: u32, frames: u32, held: impl Fn(u32) -> bool) -> Vec<Fire> {
        let mut t = Trigger::default();
        let mut s = crate::mining::Swinger::default();
        let mut was = false;
        let mut out = Vec::new();
        for f in 0..frames {
            let (first, latest) = (f * 3 + 1, f * 3 + 3);
            let h = held(f);
            if let Some(fire) = t.update(mode, h && !was, h, s.ready_at(), first, latest) {
                s.swung(fire.at, ticks);
                out.push(fire);
            }
            was = h;
        }
        out
    }

    #[test]
    fn a_click_fires_once_and_waits_for_the_cooldown() {
        let ticks = swing_ticks(10.0);
        // Held for a second: one shot.
        let shots = run(FireMode::Click, ticks, 20, |f| f < 20);
        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0].at, 1);
        // Two quick clicks: the second waits for the cooldown and fires the
        // moment it allows.
        let shots = run(FireMode::Click, ticks, 40, |f| f == 0 || f == 2);
        assert_eq!(shots.len(), 2);
        assert_eq!(shots[1].at, 1 + ticks);
    }

    #[test]
    fn rapid_fire_shoots_while_held_at_the_slings_cadence() {
        // A stone sling (action speed 10) for six seconds: shots a second as
        // the swing curve says, never faster.
        let ticks = swing_ticks(10.0);
        let shots = run(FireMode::Rapid, ticks, 120, |_| true);
        let per_second = 60.0 / ticks as f64;
        assert_eq!(shots.len() as u32, 360u32.div_ceil(ticks));
        assert!(shots.windows(2).all(|w| w[1].at - w[0].at == ticks), "on the dot");
        // Boundless's stone slingbow fires a bit under twice a second.
        assert!((1.0..2.5).contains(&per_second), "{per_second}");
        // The quick copper sling (30) is faster.
        assert!(swing_ticks(30.0) < ticks);
    }

    #[test]
    fn draw_charges_from_ready_and_fires_on_release() {
        let ticks = swing_ticks(10.0);
        // Held for 30 frames (90 inputs), released: charged from input 1.
        let shots = run(FireMode::Draw, ticks, 40, |f| f < 30);
        assert_eq!(shots, vec![Fire { at: 91, charge: 90 }]);
        // Held far longer than the charge: capped.
        let shots = run(FireMode::Draw, ticks, 80, |f| f < 70);
        assert_eq!(shots[0].charge as u32, CHARGE_TICKS);
        // A press during the cooldown charges from when the sling is ready.
        let mut t = Trigger::default();
        let ready = 50;
        assert_eq!(t.update(FireMode::Draw, true, true, ready, 10, 12), None);
        assert_eq!(t.charge(12), None, "not ready: not charging");
        assert_eq!(t.update(FireMode::Draw, false, true, ready, 49, 51), None);
        assert_eq!(t.charge(60), Some(10.0 / CHARGE_TICKS as f32));
        assert_eq!(
            t.update(FireMode::Draw, false, false, ready, 70, 72),
            Some(Fire { at: 70, charge: 20 })
        );
        // A tap released before ready fires uncharged once ready.
        let mut t = Trigger::default();
        assert_eq!(t.update(FireMode::Draw, true, true, 50, 10, 12), None);
        assert_eq!(t.update(FireMode::Draw, false, false, 50, 13, 15), None);
        assert_eq!(
            t.update(FireMode::Draw, false, false, 50, 49, 51),
            Some(Fire { at: 50, charge: 0 })
        );
    }

    #[test]
    fn shots_cost_their_energy_and_tire_you_out() {
        // A stone sling held on Rapid Fire: 6 energy a shot, spent with
        // `try_spend` as the server spends it, whatever the charge.
        let ticks = swing_ticks(10.0);
        let mut p = PlayerState::default();
        let shots = run(FireMode::Rapid, ticks, 2000, |_| true);
        // Nothing refills here (that is `movement::step`): a full bar alone.
        let fired = shots
            .iter()
            .filter(|_| p.try_spend(6, bm_core::food::Drain::Ranged))
            .count();
        // Each shot adds 0.012 hunger (energy / 500): the 100th finds the bar
        // a point short.
        assert_eq!(fired, 99, "a full bar is about a hundred stone shots");
    }

    fn open(_: BlockPos) -> bool {
        false
    }

    #[test]
    fn shots_stop_at_walls_range_and_creatures() {
        let shape = PlanetShape::new(8, 4);
        let from = DVec3::new(10.5, 50.5, 10.5);
        let fly = |shots: &mut Shots, solid: &dyn Fn(BlockPos) -> bool, hit: &dyn Fn(DVec3, DVec3) -> bool| {
            let mut n = 0;
            while !shots.list.is_empty() && n < 200 {
                shots.tick(shape, solid, hit, |_| None);
                n += 1;
            }
            n
        };
        // Out of range: a stone sling's 15 blocks at 30 a second, half a second.
        let mut s = Shots::default();
        s.fire(from, DVec3::X, 30.0, 15.0, 0.5);
        assert_eq!(fly(&mut s, &open, &|_, _| false), 30);
        // A wall 5 blocks east stops it there.
        let mut s = Shots::default();
        s.fire(from, DVec3::X, 30.0, 15.0, 0.5);
        let wall = |p: BlockPos| p.x == 15;
        let mut ticks = 0;
        let mut last = from;
        while let Some(shot) = s.list.first().copied() {
            last = shot.pos;
            s.tick(shape, wall, |_, _| false, |_| None);
            ticks += 1;
        }
        assert!(ticks <= 11 && last.x < 15.0, "{ticks} {last}");
        // Our shot stops at a living creature's box; others' do not (the
        // server says when they end).
        let (lo, hi) = (DVec3::new(18.0, 49.0, 9.0), DVec3::new(19.0, 52.0, 12.0));
        let creature = |a: DVec3, b: DVec3| segment_hits(a, b, lo, hi);
        let mut s = Shots::default();
        s.fire(from, DVec3::X, 30.0, 15.0, 0.5);
        let n = fly(&mut s, &open, &creature);
        assert!((14..30).contains(&n), "stopped at the creature: {n}");
        let mut s = Shots::default();
        s.server(&bm_protocol::ShotView {
            id: 3,
            from: bm_protocol::Target::Creature(9),
            pos: from.as_vec3().to_array(),
            vel: [30.0, 0.0, 0.0],
            falloff: 0.5,
            range: 15.0,
            kind: bm_protocol::ShotKind::Pellet,
            homing: None,
        });
        assert_eq!(fly(&mut s, &open, &creature), 30 + SERVER_SHOT_DELAY as usize);
    }

    #[test]
    fn server_shots_start_late_and_end_late() {
        let shape = PlanetShape::new(8, 4);
        let mut s = Shots::default();
        let view = bm_protocol::ShotView {
            id: 7,
            from: bm_protocol::Target::Player(2),
            pos: [10.0, 50.0, 10.0],
            vel: [0.0, 0.0, -30.0],
            falloff: 0.5,
            range: 30.0,
            kind: bm_protocol::ShotKind::Pellet,
            homing: None,
        };
        s.server(&view);
        for _ in 0..SERVER_SHOT_DELAY {
            assert!(s.drawn(0.5).is_empty(), "held back");
            s.tick(shape, open, |_, _| false, |_| None);
        }
        assert_eq!(s.drawn(0.0).len(), 1);
        s.tick(shape, open, |_, _| false, |_| None);
        assert!(s.list[0].pos.z < 10.0, "flying");
        // The server ends it: drawn on for the delay, then gone.
        s.end(7);
        for _ in 0..SERVER_SHOT_DELAY {
            s.tick(shape, open, |_, _| false, |_| None);
            assert_eq!(s.list.len(), 1);
        }
        s.tick(shape, open, |_, _| false, |_| None);
        assert!(s.list.is_empty());
    }

    /// Runs a bomb trigger over `frames` frames of 3 inputs each, the button
    /// held as `held(frame)` says, on the shared cooldown with a bag
    /// throwing every `ticks`. Returns the throws.
    fn run_fuse(ticks: u32, frames: u32, held: impl Fn(u32) -> bool) -> Vec<Toss> {
        let mut f = Fuse::default();
        let mut s = crate::mining::Swinger::default();
        let mut was = false;
        let mut out = Vec::new();
        for frame in 0..frames {
            let (first, latest) = (frame * 3 + 1, frame * 3 + 3);
            let h = held(frame);
            if let Some(t) = f.update(h && !was, h, s.ready_at(), first, latest) {
                s.swung(t.at, ticks);
                out.push(t);
            }
            was = h;
        }
        out
    }

    #[test]
    fn bombs_light_on_hold_and_throw_on_release() {
        let ticks = bm_core::mining::swing_ticks(50.0);
        // Held 20 frames (60 inputs) from input 1, let go: thrown on the
        // first input after, held 60 ticks.
        assert_eq!(run_fuse(ticks, 40, |f| f < 20), vec![Toss { at: 61, hold: 60 }]);
        // A click shorter than a frame drops one at our feet.
        let mut f = Fuse::default();
        assert_eq!(f.update(true, false, 0, 4, 6), Some(Toss { at: 4, hold: 0 }));
        // Held through the whole fuse: thrown by itself the moment it runs
        // out, and nothing more lights until the button is let go.
        let fuse = BOMB_FUSE_TICKS;
        let tosses = run_fuse(ticks, 200, |f| f < 150);
        assert_eq!(
            tosses,
            vec![Toss {
                at: 1 + fuse,
                hold: fuse as u16
            }]
        );
        // Let go and pressed again: the next lights once the cooldown allows.
        let tosses = run_fuse(ticks, 300, |f| !(150..152).contains(&f));
        assert_eq!(tosses.len(), 2);
        assert!(tosses[1].at >= tosses[0].at + ticks);
        // Pressed during the cooldown: lit only once ready, and the hold
        // counts from then.
        let mut f = Fuse::default();
        assert_eq!(f.update(true, true, 50, 10, 12), None);
        assert_eq!(f.burned(12), None, "not ready: not lit");
        assert_eq!(f.update(false, true, 50, 49, 51), None);
        assert_eq!(f.held_for(60), Some(10));
        assert!((f.burned(50 + fuse / 2).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(f.update(false, false, 50, 70, 72), Some(Toss { at: 70, hold: 20 }));
        // A tap let go before the cooldown allowed throws nothing.
        let mut f = Fuse::default();
        assert_eq!(f.update(true, false, 50, 10, 12), None);
        assert_eq!(f.update(false, false, 50, 49, 51), None);
        // Put away while lit: forgotten.
        let mut f = Fuse::default();
        f.update(true, true, 0, 1, 3);
        f.reset();
        assert_eq!(f.update(false, false, 0, 4, 6), None);
    }

    #[test]
    fn throws_cost_energy_and_tire_you_out() {
        // A copper bag (900 damage: 90 energy a throw), thrown as fast as
        // the cooldown allows: a full bar is six throws.
        let mut p = PlayerState::default();
        let tosses = run_fuse(bm_core::mining::swing_ticks(50.0), 4000, |f| f % 4 == 0);
        assert!(tosses.len() > 20);
        let thrown = tosses
            .iter()
            .filter(|_| p.try_spend(90, bm_core::food::Drain::Ranged))
            .count();
        assert_eq!(thrown, 600 / 90);
    }

    #[test]
    fn bombs_fall_and_rest_on_the_ground_until_their_fuse_ends() {
        let shape = PlanetShape::new(8, 4);
        let floor = |p: BlockPos| p.y < 44;
        let mut s = Shots::default();
        // Thrown level from 1.6 above the floor at 10 a second, 60 ticks of
        // fuse left.
        s.throw(DVec3::new(10.5, 45.6, 10.5), DVec3::X, 10.0, 60);
        let mut landed = None;
        for t in 0..60 {
            s.tick(shape, floor, |_, _| true, |_| None);
            let b = s.list[0];
            if b.resting && landed.is_none() {
                landed = Some((t, b.pos));
            }
        }
        let (t, pos) = landed.expect("it lands");
        assert!(t < 40);
        assert!((pos.y - (44.0 + BOMB_REST)).abs() < 1e-9, "on the floor: {pos}");
        assert!(pos.x > 11.5, "ahead of us: {pos}");
        // It stays put (creatures do not stop it) until the fuse ends.
        assert_eq!(s.list[0].pos, pos);
        let d = s.drawn(0.5);
        assert_eq!((d[0].kind, d[0].dir), (ShotKind::Bomb, DVec3::ZERO));
        assert!(d[0].burned.unwrap() > 0.9);
        s.tick(shape, floor, |_, _| true, |_| None);
        assert!(s.list.is_empty(), "gone at fuse end");
        // A wall stops it; it drops at the wall's foot.
        let mut s = Shots::default();
        let wall = |p: BlockPos| p.y < 44 || p.x == 13;
        s.throw(DVec3::new(10.5, 45.6, 10.5), DVec3::X, 20.0, 120);
        for _ in 0..100 {
            s.tick(shape, wall, |_, _| false, |_| None);
        }
        let b = s.list[0];
        assert!(b.resting && b.pos.x < 13.0 && b.pos.x > 12.5, "{}", b.pos);
        // A blast where it lies takes it away.
        s.blast(b.pos + DVec3::new(0.5, 0.0, 0.0), 3.0, shape);
        assert!(s.list.is_empty());
    }

    #[test]
    fn shots_fly_by_their_kind() {
        let shape = PlanetShape::new(8, 4);
        let view = |id, kind, homing| bm_protocol::ShotView {
            id,
            from: Target::Creature(3),
            pos: [10.0, 60.0, 10.0],
            vel: [0.0, 0.0, -20.0],
            falloff: 0.5,
            range: 40.0,
            kind,
            homing,
        };
        let mut s = Shots::default();
        s.server(&view(1, ShotKind::Homing, Some(Target::Player(4))));
        s.server(&view(2, ShotKind::Bolt, None));
        // A homing bolt with no target to turn to flies straight, no gravity.
        s.server(&view(3, ShotKind::Homing, Some(Target::Creature(99))));
        s.server(&view(4, ShotKind::Bomb, None));
        let me = DVec3::new(30.0, 60.0, 0.0);
        let target = |t: Target| (t == Target::Player(4)).then_some(me);
        for _ in 0..SERVER_SHOT_DELAY + 30 {
            s.tick(shape, open, |_, _| false, target);
        }
        let get = |id| s.list.iter().find(|x| x.id == Some(id)).copied().unwrap();
        let homing = get(1);
        assert!(homing.vel.x > 0.0, "turned toward us: {}", homing.vel);
        assert!((homing.vel.length() - 20.0).abs() < 1e-6, "keeping its speed");
        assert_eq!(homing.pos.y, 60.0, "no gravity");
        assert!(get(2).pos.y < 60.0, "a bolt arcs like a pellet");
        let straight = get(3);
        assert_eq!((straight.vel, straight.pos.y), (DVec3::new(0.0, 0.0, -20.0), 60.0));
        assert!(get(4).vel.y < 0.0, "a bomb falls");
        let kinds: Vec<ShotKind> = s.drawn(0.0).iter().map(|d| d.kind).collect();
        assert_eq!(
            kinds,
            [ShotKind::Homing, ShotKind::Bolt, ShotKind::Homing, ShotKind::Bomb]
        );
        assert!(
            s.drawn(0.0).iter().all(|d| d.burned.is_none()),
            "others' fuses are not known"
        );
    }

    #[test]
    fn floaters_rise_and_fade() {
        let mut f = Floaters::default();
        f.add(Anchor::Me, 120, false, true, 10.0);
        f.add(Anchor::World(DVec3::ONE), 360, true, false, 10.5);
        let v = f.visible(10.2);
        assert_eq!(v.len(), 1, "the second is not born yet");
        assert!(v[0].1 > 0.0 && v[0].2 == 1.0);
        let v = f.visible(10.0 + FLOAT_SECONDS * 0.9);
        assert!(v[0].2 < 0.5, "fading");
        f.update(10.01 + FLOAT_SECONDS);
        assert_eq!(f.list.len(), 1);
        f.update(11.0 + FLOAT_SECONDS);
        assert!(f.list.is_empty());
    }

    #[test]
    fn falls_get_their_own_number_and_hits_only_theirs() {
        let mut l = Losses::default();
        assert_eq!(l.observe(0, 0.0), None);
        // A hit told before the snapshot shows the drop: no extra number.
        l.hit(100, 1.0);
        assert_eq!(l.observe(100, 1.05), None);
        assert_eq!(l.observe(100, 2.0), None);
        // The drop first, its hit a moment later: still none.
        assert_eq!(l.observe(130, 3.0), None);
        l.hit(30, 3.1);
        assert_eq!(l.observe(130, 3.5), None);
        // A fall: no hit comes, so it shows after the delay.
        assert_eq!(l.observe(250, 5.0), None);
        assert_eq!(l.observe(250, 5.0 + UNEXPLAINED_DELAY * 0.5), None);
        assert_eq!(l.observe(250, 5.01 + UNEXPLAINED_DELAY), Some(120));
        // Healing shows nothing; a reset (respawn) starts again.
        assert_eq!(l.observe(200, 6.0), None);
        l.reset(300);
        assert_eq!(l.observe(300, 7.0 + UNEXPLAINED_DELAY), None);
    }

    #[test]
    fn the_defeat_screen_follows_the_server() {
        let mut d = Defeat::default();
        assert!(!d.is_down());
        assert!(!d.press_return(0.0), "nothing to return from");
        // Down in prediction before the message: the screen shows anyway.
        d.observe(true);
        assert_eq!(d.details(), Some(("", 0, false)));
        d.defeated("Hardy Grazer".into(), 750);
        assert_eq!(d.details(), Some(("Hardy Grazer", 750, false)));
        // Return is sent once; pressed again only after a long silence.
        assert!(d.press_return(10.0));
        assert!(!d.press_return(11.0));
        assert_eq!(d.details(), Some(("Hardy Grazer", 750, true)));
        assert!(d.press_return(10.0 + RETURN_RETRY + 0.1));
        // Still down while waiting; up again ends it.
        d.observe(true);
        assert!(d.is_down());
        d.observe(false);
        assert_eq!(d, Defeat::Standing);
    }

    #[test]
    fn segments_hit_boxes() {
        let (lo, hi) = (DVec3::ZERO, DVec3::ONE);
        assert!(segment_hits(
            DVec3::new(-1.0, 0.5, 0.5),
            DVec3::new(0.2, 0.5, 0.5),
            lo,
            hi
        ));
        assert!(!segment_hits(
            DVec3::new(-1.0, 0.5, 0.5),
            DVec3::new(-0.2, 0.5, 0.5),
            lo,
            hi
        ));
        assert!(segment_hits(DVec3::splat(0.5), DVec3::splat(0.5), lo, hi));
    }
}
