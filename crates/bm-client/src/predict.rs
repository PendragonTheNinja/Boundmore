//! Client-side prediction for our own player, and interpolation for others
//! (ADR-0004).
//!
//! We move instantly on our own inputs by running the same `movement::step`
//! the server runs. Each snapshot says which input the server has applied and
//! where that left us; if our record of that tick disagrees, we take the
//! server's state and replay the inputs it has not seen yet. Because the step
//! is deterministic, disagreement only happens when the worlds differ (a chunk
//! we lack, an edit in flight) or an input was lost, and the visible jump is
//! smoothed away over a few frames.

use bm_core::movement::{self, GrappleSpec, MoveConfig, PlayerInput, PlayerState};
use bm_core::{PhysicsWorld, PlanetShape};
use glam::DVec3;
use std::collections::{HashMap, VecDeque};

/// Inputs kept for replay: two seconds at 60 Hz.
const HISTORY: usize = 120;
/// Position error that counts as a disagreement (bit-identical replay makes
/// anything above float noise a real difference).
const TOLERANCE: f64 = 1e-6;
/// Visual correction decays to ~5% in this many seconds.
const SMOOTHING_SECONDS: f64 = 0.15;

struct Entry {
    seq: u32,
    input: PlayerInput,
    grapple: Option<GrappleSpec>,
    state: PlayerState,
}

pub struct Predictor {
    pub state: PlayerState,
    /// The state one tick earlier, for render interpolation.
    pub previous: PlayerState,
    pub seq: u32,
    /// Each input with the grapple in hand when it ran (replays must use the
    /// same one) and the state it led to.
    history: VecDeque<Entry>,
    /// Render-only offset absorbing corrections, decaying to zero.
    pub correction: DVec3,
    pub corrections: u64,
    last_snapshot_tick: u64,
    /// Moved by the server on this planet (`jump`): the next snapshot is
    /// where we are, not a disagreement to count.
    adopt_next: bool,
}

impl Predictor {
    pub fn new(spawn: PlayerState) -> Self {
        Self {
            state: spawn,
            previous: spawn,
            seq: 0,
            history: VecDeque::with_capacity(HISTORY),
            correction: DVec3::ZERO,
            corrections: 0,
            last_snapshot_tick: 0,
            adopt_next: false,
        }
    }

    /// Puts us at `spawn` on another planet. Input sequences carry on (the
    /// server counts them across the journey); what we predicted on the old
    /// planet means nothing on the new one, so it is forgotten, and the next
    /// snapshots are taken as they come until we predict again.
    pub fn teleport(&mut self, spawn: PlayerState) {
        self.state = spawn;
        self.previous = spawn;
        self.history.clear();
        self.correction = DVec3::ZERO;
    }

    /// The server moved us on this planet at server tick `tick` (through a
    /// portal): a jump, not a correction. Inputs still in flight were sent
    /// before we knew; the server applies them after the move, so they are
    /// kept. Snapshots from before the move are ignored, and the first one
    /// after it is taken as where we are, with every input it has not seen
    /// replayed from there: from then on prediction agrees again.
    pub fn jump(&mut self, to: PlayerState, tick: u64) {
        self.correction = DVec3::ZERO;
        if tick < self.last_snapshot_tick {
            // A snapshot from after the move overtook the message and has
            // put us there already.
            return;
        }
        self.state = to;
        self.previous = to;
        self.last_snapshot_tick = tick;
        self.adopt_next = true;
    }

    /// Runs one tick on a fresh input, with `grapple` what the item in
    /// `input.slot` can do. Returns its sequence number.
    pub fn tick(
        &mut self,
        input: PlayerInput,
        grapple: Option<GrappleSpec>,
        cfg: &MoveConfig,
        shape: PlanetShape,
        world: &impl PhysicsWorld,
    ) -> u32 {
        self.seq += 1;
        self.previous = self.state;
        movement::step(&mut self.state, &input, cfg, grapple.as_ref(), shape, world);
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(Entry {
            seq: self.seq,
            input,
            grapple,
            state: self.state,
        });
        self.seq
    }

    /// The last `n` inputs, newest first, for a redundant input datagram.
    pub fn recent_inputs(&self, n: usize) -> Vec<PlayerInput> {
        self.history.iter().rev().take(n).map(|e| e.input).collect()
    }

    /// Applies the server's view of us. Snapshots may arrive out of order;
    /// older ones are ignored.
    pub fn reconcile(
        &mut self,
        tick: u64,
        ack: u32,
        server: PlayerState,
        cfg: &MoveConfig,
        shape: PlanetShape,
        world: &impl PhysicsWorld,
    ) {
        if tick <= self.last_snapshot_tick {
            return;
        }
        self.last_snapshot_tick = tick;
        while self.history.front().is_some_and(|e| e.seq < ack) {
            self.history.pop_front();
        }
        if std::mem::take(&mut self.adopt_next) {
            // The first word after a jump: where the server put us, plus
            // whatever it has not applied yet. Not a misprediction.
            self.state = server;
            if self.history.front().is_some_and(|e| e.seq == ack) {
                self.history.pop_front();
            }
            for e in self.history.iter_mut() {
                movement::step(&mut self.state, &e.input, cfg, e.grapple.as_ref(), shape, world);
                e.state = self.state;
            }
            self.seq = self.seq.max(ack);
            self.previous = self.state;
            self.correction = DVec3::ZERO;
            return;
        }
        if self.history.is_empty() {
            // Nothing predicted since joining or arriving: the server's state
            // is simply where we are, not a correction.
            self.state = server;
            self.previous = server;
            self.seq = self.seq.max(ack);
            return;
        }
        let agrees = match self.history.front() {
            Some(e) if e.seq == ack => same(&e.state, &server, shape),
            // The server is ahead of anything we remember (fell far behind):
            // its state is all we have.
            _ => false,
        };
        if agrees {
            return;
        }
        let before = self.state.pos;
        self.state = server;
        if self.history.front().is_some_and(|e| e.seq == ack) {
            self.history.pop_front();
        }
        for e in self.history.iter_mut() {
            movement::step(&mut self.state, &e.input, cfg, e.grapple.as_ref(), shape, world);
            e.state = self.state;
        }
        if ack > self.seq {
            // Inputs we never sent? Only possible after a reconnect; adopt.
            self.seq = ack;
        }
        self.previous = self.state;
        self.correction += shape.delta(self.state.pos, before);
        self.corrections += 1;
    }

    /// Decays the visual correction. Call once per rendered frame.
    pub fn decay_correction(&mut self, dt: f64) {
        let k = (-3.0 * dt / SMOOTHING_SECONDS).exp();
        self.correction *= k;
        // A large jump (teleport, respawn) is shown at once, not slid into.
        if self.correction.length() > 4.0 || self.correction.length() < 1e-4 {
            self.correction = DVec3::ZERO;
        }
    }

    /// Where to draw us, `alpha` of the way from the previous tick to now.
    pub fn render_pos(&self, alpha: f64, shape: PlanetShape) -> DVec3 {
        let from = self.previous.pos;
        let to = shape.nearest_lap(from, self.state.pos);
        from + (to - from) * alpha + self.correction
    }
}

fn same(a: &PlayerState, b: &PlayerState, shape: PlanetShape) -> bool {
    let hooks = match (a.hook, b.hook) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            x.stuck == y.stuck && shape.delta(x.pos, y.pos).length() < TOLERANCE && (x.rope - y.rope).abs() < TOLERANCE
        }
        _ => false,
    };
    shape.delta(a.pos, b.pos).length() < TOLERANCE
        && (a.vel - b.vel).length() < TOLERANCE
        && a.on_ground == b.on_ground
        && hooks
}

/// How far behind the server's newest snapshot remote players are drawn, in
/// ticks. Two snapshots' worth (at 20 Hz) so one lost datagram never leaves
/// us with nothing to interpolate towards.
pub const INTERP_DELAY_TICKS: f64 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteSample {
    pub tick: u64,
    pub pos: DVec3,
    pub yaw: u16,
    pub pitch: i16,
    /// Their grapple hook, from their feet.
    pub hook: Option<DVec3>,
}

#[derive(Default)]
pub struct Remotes {
    pub names: HashMap<u32, String>,
    samples: HashMap<u32, VecDeque<RemoteSample>>,
    /// Estimate of `server_tick - local_ticks`; the largest seen is the one
    /// with the least network delay in it.
    offset: Option<f64>,
}

impl Remotes {
    pub fn joined(&mut self, id: u32, name: String) {
        self.names.insert(id, name);
    }

    pub fn left(&mut self, id: u32) {
        self.names.remove(&id);
        self.samples.remove(&id);
    }

    /// Records a snapshot received at `local_ticks` (seconds × 60 on our clock).
    pub fn record(&mut self, tick: u64, local_ticks: f64, players: impl Iterator<Item = (u32, RemoteSample)>) {
        let est = tick as f64 - local_ticks;
        self.offset = Some(match self.offset {
            // Rise at once (less delay), fall slowly (a delay spike is not
            // the new normal, but a clock change eventually is).
            Some(o) if est < o => o - (o - est).min(0.05),
            _ => est,
        });
        for (id, s) in players {
            let q = self.samples.entry(id).or_default();
            if q.back().is_none_or(|b| b.tick < s.tick) {
                q.push_back(s);
            }
            while q.len() > 32 {
                q.pop_front();
            }
        }
    }

    /// Our best estimate of the server's tick right now, once any snapshot
    /// has arrived.
    pub fn server_tick(&self, local_ticks: f64) -> Option<f64> {
        self.offset.map(|o| local_ticks + o)
    }

    /// Interpolated positions at our current time. Positions are canonical;
    /// the renderer moves them to the lap nearest the camera.
    pub fn sample(&self, local_ticks: f64, shape: PlanetShape) -> Vec<(u32, RemoteSample)> {
        let Some(offset) = self.offset else { return Vec::new() };
        let t = local_ticks + offset - INTERP_DELAY_TICKS;
        let mut out = Vec::new();
        for (id, q) in &self.samples {
            let Some(last) = q.back() else { continue };
            // Not in a snapshot for a while: out of range or gone.
            if (last.tick as f64) < t - 60.0 {
                continue;
            }
            let i = q.iter().position(|s| s.tick as f64 > t);
            let s = match i {
                Some(0) => q[0],
                Some(i) => {
                    let (a, b) = (q[i - 1], q[i]);
                    let f = (t - a.tick as f64) / (b.tick - a.tick) as f64;
                    let to = shape.nearest_lap(a.pos, b.pos);
                    RemoteSample {
                        tick: t as u64,
                        pos: a.pos + (to - a.pos) * f,
                        yaw: lerp_angle(a.yaw, b.yaw, f),
                        pitch: (a.pitch as f64 + (b.pitch as f64 - a.pitch as f64) * f) as i16,
                        hook: match (a.hook, b.hook) {
                            (Some(ha), Some(hb)) => Some(ha + (hb - ha) * f),
                            _ if f < 0.5 => a.hook,
                            _ => b.hook,
                        },
                    }
                }
                None => *last,
            };
            out.push((*id, s));
        }
        out
    }
}

fn lerp_angle(a: u16, b: u16, f: f64) -> u16 {
    let d = b.wrapping_sub(a) as i16 as f64;
    (a as f64 + d * f).rem_euclid(65536.0) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::BlockPos;

    struct Floor(i64);
    impl PhysicsWorld for Floor {
        fn solid(&self, p: BlockPos) -> bool {
            p.y < self.0
        }
        fn liquid(&self, _: BlockPos) -> bool {
            false
        }
    }

    fn shape() -> PlanetShape {
        PlanetShape::new(16, 8)
    }

    fn spawn() -> PlayerState {
        PlayerState {
            pos: DVec3::new(10.5, 20.0, 10.5),
            ..Default::default()
        }
    }

    fn walk(i: u32) -> PlayerInput {
        PlayerInput {
            forward: 1,
            jump: i % 30 == 0,
            yaw: (i * 300) as u16,
            ..Default::default()
        }
    }

    /// Runs the server side of the conversation: same steps, same inputs.
    fn server_states(inputs: &[PlayerInput], world: &Floor) -> Vec<PlayerState> {
        let mut s = spawn();
        inputs
            .iter()
            .map(|i| {
                movement::step(&mut s, i, &MoveConfig::default(), None, shape(), world);
                s
            })
            .collect()
    }

    #[test]
    fn matching_snapshots_cause_no_correction() {
        let (cfg, w) = (MoveConfig::default(), Floor(10));
        let inputs: Vec<PlayerInput> = (1..=200).map(walk).collect();
        let truth = server_states(&inputs, &w);
        let mut p = Predictor::new(spawn());
        for (i, input) in inputs.iter().enumerate() {
            p.tick(*input, None, &cfg, shape(), &w);
            if i >= 5 && i % 3 == 0 {
                // Server is 5 ticks behind, like a ~80 ms round trip.
                let ack = i as u32 - 4;
                p.reconcile(i as u64, ack, truth[ack as usize - 1], &cfg, shape(), &w);
            }
        }
        assert_eq!(p.corrections, 0);
        assert_eq!(p.state, truth[199]);
    }

    #[test]
    fn grappling_predicts_without_corrections() {
        let cfg = MoveConfig::default();
        // A ceiling to hook into, above a floor.
        struct Room;
        impl PhysicsWorld for Room {
            fn solid(&self, p: BlockPos) -> bool {
                p.y < 10 || p.y == 24
            }
            fn liquid(&self, _: BlockPos) -> bool {
                false
            }
        }
        let g = GrappleSpec {
            range: 26.0,
            shot_speed: 65.0,
            reel_speed: 7.0,
            hookshot: false,
        };
        let inputs: Vec<PlayerInput> = (1..=300u32)
            .map(|i| PlayerInput {
                forward: (i / 40 % 2) as i8,
                pitch: 12000,
                yaw: (i * 50) as u16,
                fire: i % 100 == 1,
                reel: if i % 100 < 50 { 1 } else { 0 },
                ..Default::default()
            })
            .collect();
        let mut truth = Vec::new();
        let mut s = spawn();
        for i in &inputs {
            movement::step(&mut s, i, &cfg, Some(&g), shape(), &Room);
            truth.push(s);
        }
        assert!(truth.iter().any(|s| s.anchor().is_some()), "the test hooks the ceiling");
        let mut p = Predictor::new(spawn());
        for (i, input) in inputs.iter().enumerate() {
            p.tick(*input, Some(g), &cfg, shape(), &Room);
            if i >= 5 && i % 3 == 0 {
                let ack = i as u32 - 4;
                p.reconcile(i as u64, ack, truth[ack as usize - 1], &cfg, shape(), &Room);
            }
        }
        assert_eq!(p.corrections, 0);
        assert_eq!(p.state, truth[299]);
        // A server that never saw the hook (it had no grapple) corrects us.
        let mut p = Predictor::new(spawn());
        for input in &inputs[..10] {
            p.tick(*input, Some(g), &cfg, shape(), &Room);
        }
        let mut bare = spawn();
        for i in &inputs[..5] {
            movement::step(&mut bare, i, &cfg, None, shape(), &Room);
        }
        p.reconcile(1, 5, bare, &cfg, shape(), &Room);
        assert_eq!(p.corrections, 1);
    }

    #[test]
    fn a_disagreement_is_corrected_and_replayed() {
        let cfg = MoveConfig::default();
        let inputs: Vec<PlayerInput> = (1..=100).map(walk).collect();
        // The server's world had a higher floor for the first 50 ticks.
        let server_world = Floor(12);
        let truth = server_states(&inputs[..50], &server_world);
        let mut p = Predictor::new(spawn());
        let client_world = Floor(10);
        for input in &inputs[..60] {
            p.tick(*input, None, &cfg, shape(), &client_world);
        }
        p.reconcile(1, 50, truth[49], &cfg, shape(), &client_world);
        assert_eq!(p.corrections, 1);
        // Our state is now the server's, plus our ten unacknowledged inputs.
        let mut expect = truth[49];
        for input in &inputs[50..60] {
            movement::step(&mut expect, input, &cfg, None, shape(), &client_world);
        }
        assert_eq!(p.state, expect);
        assert!(p.correction.length() > 0.0);
        for _ in 0..60 {
            p.decay_correction(1.0 / 60.0);
        }
        assert_eq!(p.correction, DVec3::ZERO);
    }

    #[test]
    fn out_of_order_snapshots_are_ignored() {
        let (cfg, w) = (MoveConfig::default(), Floor(10));
        let mut p = Predictor::new(spawn());
        let mut states = Vec::new();
        for i in 1..=30 {
            p.tick(walk(i), None, &cfg, shape(), &w);
            states.push(p.state);
        }
        let now = p.state;
        p.reconcile(10, 25, states[24], &cfg, shape(), &w);
        assert_eq!(p.corrections, 0);
        p.reconcile(9, 20, spawn(), &cfg, shape(), &w); // older, and wrong
        assert_eq!(p.corrections, 0);
        assert_eq!(p.state, now);
    }

    #[test]
    fn after_a_teleport_snapshots_are_adopted_until_we_predict_again() {
        let (cfg, w) = (MoveConfig::default(), Floor(10));
        let mut p = Predictor::new(spawn());
        for i in 1..=20 {
            p.tick(walk(i), None, &cfg, shape(), &w);
        }
        let elsewhere = PlayerState {
            pos: DVec3::new(3.5, 11.0, 4.5),
            ..Default::default()
        };
        p.teleport(elsewhere);
        assert_eq!(p.seq, 20, "sequences carry on");
        assert_eq!(p.state, elsewhere);
        assert!(p.recent_inputs(6).is_empty(), "the old planet's inputs are forgotten");
        // The server applied a few inputs that were in flight, on the new
        // planet: take its word, silently.
        let moved = PlayerState {
            pos: DVec3::new(3.6, 11.0, 4.5),
            ..elsewhere
        };
        p.reconcile(100, 20, moved, &cfg, shape(), &w);
        assert_eq!(p.state, moved);
        assert_eq!(p.corrections, 0);
        // Then prediction carries on from there, numbered after the old inputs.
        assert_eq!(p.tick(walk(1), None, &cfg, shape(), &w), 21);
    }

    #[test]
    fn a_jump_keeps_the_inputs_in_flight_and_is_no_correction() {
        let (cfg, w) = (MoveConfig::default(), Floor(10));
        let inputs: Vec<PlayerInput> = (1..=100).map(walk).collect();
        let portal = PlayerState {
            pos: DVec3::new(4.5, 10.0, 3.5),
            ..Default::default()
        };
        // The server: one tick per input; after input 50 (tick 50) it walks
        // us through a portal.
        let mut truth = server_states(&inputs[..50], &w);
        let mut s = portal;
        for i in &inputs[50..] {
            movement::step(&mut s, i, &cfg, None, shape(), &w);
            truth.push(s);
        }
        let mut p = Predictor::new(spawn());
        for (i, input) in inputs[..60].iter().enumerate() {
            p.tick(*input, None, &cfg, shape(), &w);
            if i >= 5 && i % 3 == 0 && i < 50 {
                let ack = i as u32 - 4;
                p.reconcile(i as u64, ack, truth[ack as usize - 1], &cfg, shape(), &w);
            }
        }
        // Inputs 51 to 60 are in flight when the Teleport arrives.
        p.jump(portal, 50);
        assert_eq!(p.state, portal);
        assert_eq!(p.seq, 60, "sequences carry on");
        // A snapshot from before the move, arriving late, is ignored.
        p.reconcile(48, 47, truth[46], &cfg, shape(), &w);
        assert_eq!(p.state, portal);
        for input in &inputs[60..70] {
            p.tick(*input, None, &cfg, shape(), &w);
        }
        // The first snapshot after the move has seen up to input 56: we take
        // it and replay 57 to 70 from there.
        p.reconcile(56, 56, truth[55], &cfg, shape(), &w);
        assert_eq!(p.state, truth[69]);
        assert_eq!(p.correction, DVec3::ZERO);
        // And prediction agrees from then on.
        for (i, input) in inputs[70..].iter().enumerate() {
            p.tick(*input, None, &cfg, shape(), &w);
            let ack = 66 + i as u32;
            p.reconcile(60 + i as u64, ack, truth[ack as usize - 1], &cfg, shape(), &w);
        }
        assert_eq!(p.corrections, 0);
        assert_eq!(p.state, truth[99]);
    }

    #[test]
    fn a_jump_already_overtaken_by_a_snapshot_changes_nothing() {
        let (cfg, w) = (MoveConfig::default(), Floor(10));
        let mut p = Predictor::new(spawn());
        for i in 1..=10 {
            p.tick(walk(i), None, &cfg, shape(), &w);
        }
        let there = PlayerState {
            pos: DVec3::new(2.5, 10.0, 2.5),
            ..Default::default()
        };
        p.reconcile(30, 10, there, &cfg, shape(), &w);
        let now = p.state;
        p.jump(there, 25);
        assert_eq!(p.state, now);
    }

    #[test]
    fn remotes_interpolate_between_snapshots() {
        let mut r = Remotes::default();
        let at = |tick: u64, x: f64| RemoteSample {
            tick,
            pos: DVec3::new(x, 0.0, 0.0),
            yaw: 0,
            pitch: 0,
            hook: None,
        };
        // Snapshots every 3 ticks, arriving with no delay.
        for k in 0..10u64 {
            r.record(k * 3, (k * 3) as f64, [(7, at(k * 3, k as f64 * 3.0))].into_iter());
        }
        // At local tick 27, we draw tick 27 - 7 = 20: between 18 and 21.
        let s = r.sample(27.0, shape());
        assert_eq!(s.len(), 1);
        assert!((s[0].1.pos.x - 20.0).abs() < 1e-9, "{}", s[0].1.pos.x);
    }

    #[test]
    fn remote_interpolation_crosses_the_seam() {
        let mut r = Remotes::default();
        let w = shape().width_blocks() as f64;
        let at = |tick: u64, x: f64| RemoteSample {
            tick,
            pos: DVec3::new(x, 0.0, 0.0),
            yaw: 0,
            pitch: 0,
            hook: None,
        };
        r.record(0, 0.0, [(1, at(0, w - 1.0))].into_iter());
        r.record(10, 10.0, [(1, at(10, 1.0))].into_iter());
        let s = r.sample(12.0, shape()); // draws tick 5, halfway
        let x = s[0].1.pos.x.rem_euclid(w);
        assert!(x.abs() < 1e-9 || (x - w).abs() < 1e-9, "went the long way: {x}");
    }

    #[test]
    fn yaw_interpolates_the_short_way() {
        assert_eq!(lerp_angle(65000, 500, 0.5), (65000u32 + 518) as u16);
    }
}
