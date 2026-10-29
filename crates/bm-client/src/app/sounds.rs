//! What the player hears (milestone 30). The game's code is not touched
//! to make a sound: this module watches what changed since the last frame
//! (a swing, a hook out, health lost, a block gone) and what the server
//! says, and plays the matching sound from where it happened. The only
//! calls into it are one a frame (`frame`), one a server message
//! (`message`) and one a menu action (`action`).

use super::{App, Session};
use bm_audio::{Audio, Bus, Listener, Mat, Sfx, place};
use bm_core::items::{ItemKind, ToolClass};
use bm_core::{BlockPos, BlockState};
use bm_protocol::{ClientMsg, MeteorPhase, Pose, ServerMsg, Target};
use glam::DVec3;
use std::collections::HashMap;

/// How far each kind of sound carries, in blocks.
const NEAR_RANGE: f64 = 24.0;
const BREAK_RANGE: f64 = 40.0;
const STEP_RANGE: f64 = 20.0;
const CREATURE_RANGE: f64 = 32.0;
const WHISTLE_RANGE: f64 = 48.0;
const STICK_RANGE: f64 = 64.0;
const BLAST_RANGE: f64 = 96.0;
const METEOR_RANGE: f64 = 400.0;
/// Blocks walked between footsteps (a stride and a half; Boundless's
/// pace by ear, a guess for Nathan's listening test).
const STRIDE: f64 = 1.9;
/// Rope reeled in between ratchet ticks.
const REEL_TICK: f64 = 0.7;
/// Falling faster than this lands with a sound (blocks a second).
const LAND_SPEED: f64 = 4.0;

/// The sound state carried between frames.
pub(super) struct Ears {
    pub(super) audio: Audio,
    /// The planet the remembered state belongs to: a new one starts afresh
    /// (nothing plays for the jump between planets).
    planet: Option<u16>,
    swing_ready: u32,
    place_cooldown: f64,
    on_ground: bool,
    vel_y: f64,
    in_water: bool,
    hurt: u32,
    /// Each hand's hook: stuck, and its rope.
    hooks: [Option<(bool, f64)>; 2],
    walked: f64,
    reeled: f64,
    poses: HashMap<u32, Pose>,
    falling: HashMap<u32, bool>,
    /// Other players: where they were, and how far they walked.
    others: HashMap<u32, (DVec3, f64)>,
}

impl Ears {
    pub(super) fn new(assets: &std::path::Path) -> Ears {
        let dir = assets.join("sounds");
        // Tests never open a device.
        Ears::with(if cfg!(test) {
            Audio::silent(&dir)
        } else {
            Audio::new(&dir)
        })
    }

    pub(super) fn with(audio: Audio) -> Ears {
        Ears {
            audio,
            planet: None,
            swing_ready: 0,
            place_cooldown: 0.0,
            on_ground: true,
            vel_y: 0.0,
            in_water: false,
            hurt: 0,
            hooks: [None; 2],
            walked: 0.0,
            reeled: 0.0,
            poses: HashMap::new(),
            falling: HashMap::new(),
            others: HashMap::new(),
        }
    }

    fn at(&mut self, l: &Listener, sfx: Sfx, pos: DVec3, range: f64, gain: f32) {
        if let Some(p) = place(l, pos, range) {
            self.audio.play(sfx, Bus::Effects, gain * p.gain, p.pan);
        }
    }

    fn flat(&mut self, sfx: Sfx, gain: f32) {
        self.audio.play(sfx, Bus::Effects, gain, 0.0);
    }
}

fn centre(p: BlockPos) -> DVec3 {
    DVec3::new(p.x as f64 + 0.5, p.y as f64 + 0.5, p.z as f64 + 0.5)
}

fn listener(s: &Session, yaw: f64) -> Listener {
    Listener {
        shape: s.world.shape,
        eye: s.me.state.eye(&s.movement),
        forward: DVec3::new(yaw.sin(), 0.0, -yaw.cos()),
    }
}

fn mat(s: &Session, block: bm_core::BlockId) -> Mat {
    Mat::of(s.world.registry.get(block))
}

/// The surface under feet at `pos`: water if the feet are in it.
fn surface(s: &Session, pos: DVec3) -> Option<Mat> {
    let feet = BlockPos::containing(pos.x, pos.y + 0.1, pos.z);
    if s.world.block(feet).is_some_and(|b| s.world.flags.liquid(b)) {
        return Some(Mat::Water);
    }
    let under = BlockPos::containing(pos.x, pos.y - 0.05, pos.z);
    let b = s.world.block(under).filter(|b| !b.is_air())?;
    Some(mat(s, b))
}

/// Once a frame, before the frame's own work: what changed since the
/// last one.
pub(super) fn frame(app: &mut App, dt: f64) {
    let App {
        ears,
        session,
        settings,
        egui,
        yaw,
        grabbed,
        last_hand,
        creep_on,
        start,
        ..
    } = app;
    ears.audio.set_volumes(settings.options.volume);
    // A click on a menu (the mouse is free and over one).
    if !*grabbed
        && let Some(e) = egui
        && e.ctx.input(|i| i.pointer.primary_clicked())
        && e.ctx.is_pointer_over_egui()
    {
        ears.audio.ui(Sfx::Click);
    }
    let Some(s) = session else {
        ears.planet = None;
        return;
    };
    let st = &s.me.state;
    let water_now = surface(s, st.pos) == Some(Mat::Water) && !st.on_ground;
    let fresh = ears.planet != Some(s.planet.id);
    if fresh {
        // Arrived: remember how things stand, play nothing for it.
        ears.planet = Some(s.planet.id);
        ears.swing_ready = s.swinger.ready_at();
        ears.place_cooldown = s.place_cooldown;
        ears.on_ground = st.on_ground;
        ears.vel_y = st.vel.y;
        ears.in_water = water_now;
        ears.hurt = st.hurt;
        ears.hooks = st.hooks.map(|h| h.map(|h| (h.stuck.is_some(), h.rope)));
        ears.poses.clear();
        ears.falling.clear();
        ears.others.clear();
        return;
    }
    let l = listener(s, *yaw);
    let hand = [s.selected, s.selected_off][(*last_hand).min(1)];
    let held = s.items.slot(hand);
    let def = held.as_ref().and_then(|h| s.items.def(h));

    // Blocks broken since the last frame (ours predicted, others' told).
    for (p, was) in s.world.broken.iter().take(6) {
        let d = s.world.registry.get(was.id);
        if !d.liquid {
            let m = Mat::of(d);
            ears.at(&l, m.broken(), centre(*p), BREAK_RANGE, 1.0);
        }
    }

    // A swing (tools, slings and bombs share the one swing timer).
    let ready = s.swinger.ready_at();
    if ready > ears.swing_ready {
        match def.map(|d| &d.kind) {
            Some(ItemKind::Sling(_)) => ears.flat(Sfx::Sling, 0.8),
            Some(ItemKind::Bomb(_)) => ears.flat(Sfx::Throw, 0.9),
            kind => {
                let sfx = match kind {
                    Some(ItemKind::Tool(t)) => match t.class {
                        Some(ToolClass::Hammer) => Sfx::SwingHammer,
                        Some(ToolClass::Axe) => Sfx::SwingAxe,
                        Some(ToolClass::Shovel | ToolClass::Tiller) => Sfx::SwingShovel,
                        _ => Sfx::SwingLight,
                    },
                    _ => Sfx::SwingLight,
                };
                ears.flat(sfx, 0.8);
                // Landing on the block aimed at, unless a creature was the
                // target (its hit is the server's to say).
                let on_creature = s.aim_creature.is_some_and(|(_, d)| d <= s.reach());
                if !on_creature
                    && let Some(hit) = s.target
                    && let Some(b) = s.world.block(hit.block).filter(|b| !b.is_air())
                {
                    ears.at(&l, mat(s, b).hit(), centre(hit.block), NEAR_RANGE, 1.0);
                }
            }
        }
    }
    ears.swing_ready = ready;

    // A block placed or a chisel cut (the place pause restarted).
    if s.place_cooldown > ears.place_cooldown
        && let Some(hit) = s.target
    {
        if def.and_then(|d| d.chisel()).is_some() {
            ears.at(&l, Sfx::Chisel, centre(hit.block), NEAR_RANGE, 1.0);
        } else if let Some((block, _)) = held.and_then(|h| s.place_choice(h.item)) {
            let m = mat(s, block);
            ears.at(&l, m.placed(), centre(hit.adjacent()), NEAR_RANGE, 1.0);
        }
    }
    ears.place_cooldown = s.place_cooldown;

    // Our body: steps, jumps, landings, water.
    let quiet = if *creep_on { 0.45 } else { 1.0 };
    if st.on_ground && !st.down {
        let v = (st.vel.x * st.vel.x + st.vel.z * st.vel.z).sqrt();
        ears.walked += v * dt;
        if ears.walked >= STRIDE {
            ears.walked = 0.0;
            if let Some(m) = surface(s, st.pos) {
                ears.flat(m.step(), 0.8 * quiet);
            }
        }
    } else if st.submerged || water_now {
        let v = st.vel.length();
        ears.walked += v * dt;
        if ears.walked >= STRIDE * 1.5 {
            ears.walked = 0.0;
            ears.flat(Sfx::StepWater, 0.6);
        }
    }
    if ears.on_ground && !st.on_ground && st.vel.y > 2.0 {
        ears.flat(Sfx::Jump, 0.7 * quiet);
    }
    if !ears.on_ground && st.on_ground && ears.vel_y < -LAND_SPEED {
        let hard = ((-ears.vel_y - LAND_SPEED) / 12.0).clamp(0.3, 1.0) as f32;
        ears.flat(Sfx::Land, hard);
        ears.walked = 0.0;
    }
    if water_now && !ears.in_water && ears.vel_y < -3.0 {
        ears.flat(Sfx::Splash, 0.9);
    }
    if st.hurt > ears.hurt {
        ears.audio.play(Sfx::Hurt, Bus::Effects, 0.9, 0.0);
    }
    ears.on_ground = st.on_ground;
    ears.vel_y = st.vel.y;
    ears.in_water = water_now;
    ears.hurt = st.hurt;

    // Grapple hooks: out, stuck, let go; the reel as the rope comes in.
    for h in 0..2 {
        let now = st.hooks[h].map(|k| (k.stuck.is_some(), k.rope));
        match (ears.hooks[h], now) {
            (None, Some(_)) => ears.flat(Sfx::GrappleFire, 0.8),
            (Some(_), None) => ears.flat(Sfx::GrappleRelease, 0.7),
            (Some((false, _)), Some((true, _))) => {
                if let Some(k) = st.hooks[h] {
                    ears.at(&l, Sfx::GrappleStick, k.pos, STICK_RANGE, 1.0);
                }
            }
            (Some((true, before)), Some((true, rope))) if rope < before => {
                ears.reeled += before - rope;
                if ears.reeled >= REEL_TICK {
                    ears.reeled = 0.0;
                    ears.flat(Sfx::Reel, 0.6);
                }
            }
            _ => {}
        }
    }
    ears.hooks = now_hooks(st);

    // Creatures: a wind-up, a popper lit.
    for c in &s.seen {
        let before = ears.poses.insert(c.id, c.pose);
        if before == Some(c.pose) || before.is_none() {
            continue;
        }
        match c.pose {
            Pose::Windup => ears.at(&l, Sfx::CreatureWindup, c.pos, CREATURE_RANGE, 1.0),
            Pose::Lit => ears.at(&l, Sfx::PopperWhistle, c.pos, WHISTLE_RANGE, 1.0),
            _ => {}
        }
    }
    if ears.poses.len() > s.seen.len() + 64 {
        let alive: std::collections::HashSet<u32> = s.seen.iter().map(|c| c.id).collect();
        ears.poses.retain(|id, _| alive.contains(id));
    }

    // Other players' footsteps.
    let local = start.elapsed().as_secs_f64() / bm_core::TICK_DT;
    let shape = s.world.shape;
    let others = s.remotes.sample(local, shape);
    let mut steps = Vec::new();
    for (id, smp) in &others {
        let e = ears.others.entry(*id).or_insert((smp.pos, 0.0));
        let d = shape.delta(e.0, smp.pos);
        let flat = (d.x * d.x + d.z * d.z).sqrt();
        // Walking on level ground (not flying past, not a teleport).
        if d.y.abs() < 0.05 && flat < 1.0 {
            e.1 += flat;
            if e.1 >= STRIDE {
                e.1 = 0.0;
                steps.push(smp.pos);
            }
        }
        e.0 = smp.pos;
    }
    if ears.others.len() > others.len() {
        ears.others.retain(|id, _| others.iter().any(|o| o.0 == *id));
    }
    for pos in steps {
        if let Some(m) = surface(s, pos) {
            ears.at(&l, m.step(), pos, STEP_RANGE, 0.8);
        }
    }
}

fn now_hooks(st: &bm_core::PlayerState) -> [Option<(bool, f64)>; 2] {
    st.hooks.map(|h| h.map(|k| (k.stuck.is_some(), k.rope)))
}

/// Each server message, before the game applies it (so what a block was is
/// still known).
pub(super) fn message(app: &mut App, m: &ServerMsg) {
    let App { ears, session, yaw, .. } = app;
    // Interface sounds need no world.
    match m {
        ServerMsg::Notice { .. } => {
            ears.audio.ui(Sfx::Notice);
            return;
        }
        ServerMsg::Travel(_) | ServerMsg::Teleport { .. } => {
            ears.flat(Sfx::Warp, 0.9);
            return;
        }
        ServerMsg::Defeated { .. } => {
            ears.flat(Sfx::Defeat, 1.0);
            return;
        }
        _ => {}
    }
    let Some(s) = session.as_ref() else { return };
    if ears.planet != Some(s.planet.id) {
        return;
    }
    let l = listener(s, *yaw);
    match m {
        ServerMsg::Blast { pos, .. } => {
            let at = DVec3::from_array(pos.map(|v| v as f64));
            ears.at(&l, Sfx::Blast, at, BLAST_RANGE, 1.0);
        }
        ServerMsg::Hit {
            target: Target::Creature(id),
            ..
        } => {
            if let Some(c) = s.seen.iter().find(|c| c.id == *id) {
                ears.at(
                    &l,
                    Sfx::CreatureHit,
                    c.pos + DVec3::new(0.0, 0.8, 0.0),
                    CREATURE_RANGE,
                    1.0,
                );
            }
        }
        ServerMsg::BlockChanged { pos, block } => {
            let was = s.world.state(*pos).unwrap_or(BlockState::AIR);
            if was.id == block.id {
                return;
            }
            let (old, new) = (s.world.registry.get(was.id), s.world.registry.get(block.id));
            if old.toggle.is_some() && new.toggle.is_some() {
                let sfx = if new.name.ends_with("_open") {
                    Sfx::DoorOpen
                } else {
                    Sfx::DoorClose
                };
                ears.at(&l, sfx, centre(*pos), NEAR_RANGE, 1.0);
            } else if (was.id.is_air() || old.liquid) && !block.id.is_air() && !new.liquid {
                ears.at(&l, Mat::of(new).placed(), centre(*pos), NEAR_RANGE, 1.0);
            }
        }
        ServerMsg::Slots { changes } => {
            let gained = changes.iter().any(|(i, new)| {
                let old = s.items.inventory.slots.get(*i as usize).cloned().flatten();
                match (old, new) {
                    (None, Some(_)) => true,
                    (Some(o), Some(n)) => o.item == n.item && n.count > o.count,
                    _ => false,
                }
            });
            if gained {
                ears.audio.ui(Sfx::Pickup);
            }
        }
        ServerMsg::Ledger(l) => {
            if s.ledger.as_ref().is_some_and(|old| l.coin > old.coin) {
                ears.audio.ui(Sfx::Coin);
            }
        }
        ServerMsg::Shot(v) => {
            // Creatures' and other players' shots (ours played as we shot).
            let ours = v.from == Target::Player(s.player_id);
            if !ours {
                let at = DVec3::from_array(v.pos.map(|x| x as f64));
                ears.at(&l, Sfx::Sling, at, CREATURE_RANGE, 0.8);
            }
        }
        ServerMsg::Meteor(v) => {
            let falling = matches!(v.phase, MeteorPhase::Falling { .. });
            let was = ears.falling.insert(v.id, falling);
            if was == Some(true) && !falling {
                ears.at(&l, Sfx::MeteorLand, centre(v.site), METEOR_RANGE, 1.0);
            }
        }
        _ => {}
    }
}

/// A menu action: crafting asks for a job.
pub(super) fn action(app: &mut App, a: &crate::menus::UiAction) {
    if let crate::menus::UiAction::Send(ClientMsg::Craft { .. } | ClientMsg::StationJob { .. }) = a {
        app.ears.audio.ui(Sfx::Craft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silent() -> Ears {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sounds");
        Ears::with(Audio::silent(&dir))
    }

    #[test]
    fn every_sound_the_client_can_play_is_in_the_bank() {
        let ears = silent();
        assert!(ears.audio.bank().missing().is_empty());
    }
}
