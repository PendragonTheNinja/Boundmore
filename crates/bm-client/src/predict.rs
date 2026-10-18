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

use bm_core::movement::{self, MoveConfig, PlayerInput, PlayerState};
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

pub struct Predictor {
    pub state: PlayerState,
    /// The state one tick earlier, for render interpolation.
    pub previous: PlayerState,
    pub seq: u32,
    history: VecDeque<(u32, PlayerInput, PlayerState)>,
    /// Render-only offset absorbing corrections, decaying to zero.
    pub correction: DVec3,
    pub corrections: u64,
    last_snapshot_tick: u64,
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
        }
    }

    /// Runs one tick on a fresh input. Returns its sequence number.
    pub fn tick(&mut self, input: PlayerInput, cfg: &MoveConfig, shape: PlanetShape, world: &impl PhysicsWorld) -> u32 {
        self.seq += 1;
        self.previous = self.state;
        movement::step(&mut self.state, &input, cfg, shape, world);
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back((self.seq, input, self.state));
        self.seq
    }

    /// The last `n` inputs, newest first, for a redundant input datagram.
    pub fn recent_inputs(&self, n: usize) -> Vec<PlayerInput> {
        self.history.iter().rev().take(n).map(|(_, i, _)| *i).collect()
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
        while self.history.front().is_some_and(|(s, _, _)| *s < ack) {
            self.history.pop_front();
        }
        let agrees = match self.history.front() {
            Some((s, _, predicted)) if *s == ack => same(predicted, &server, shape),
            // The server is ahead of anything we remember (we just joined,
            // or fell far behind): its state is all we have.
            _ => ack == 0 && self.seq == 0,
        };
        if agrees {
            return;
        }
        let before = self.state.pos;
        self.state = server;
        if self.history.front().is_some_and(|(s, _, _)| *s == ack) {
            self.history.pop_front();
        }
        for entry in self.history.iter_mut() {
            movement::step(&mut self.state, &entry.1, cfg, shape, world);
            entry.2 = self.state;
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
    shape.delta(a.pos, b.pos).length() < TOLERANCE && (a.vel - b.vel).length() < TOLERANCE && a.on_ground == b.on_ground
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
                movement::step(&mut s, i, &MoveConfig::default(), shape(), world);
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
            p.tick(*input, &cfg, shape(), &w);
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
    fn a_disagreement_is_corrected_and_replayed() {
        let cfg = MoveConfig::default();
        let inputs: Vec<PlayerInput> = (1..=100).map(walk).collect();
        // The server's world had a higher floor for the first 50 ticks.
        let server_world = Floor(12);
        let truth = server_states(&inputs[..50], &server_world);
        let mut p = Predictor::new(spawn());
        let client_world = Floor(10);
        for input in &inputs[..60] {
            p.tick(*input, &cfg, shape(), &client_world);
        }
        p.reconcile(1, 50, truth[49], &cfg, shape(), &client_world);
        assert_eq!(p.corrections, 1);
        // Our state is now the server's, plus our ten unacknowledged inputs.
        let mut expect = truth[49];
        for input in &inputs[50..60] {
            movement::step(&mut expect, input, &cfg, shape(), &client_world);
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
            p.tick(walk(i), &cfg, shape(), &w);
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
    fn remotes_interpolate_between_snapshots() {
        let mut r = Remotes::default();
        let at = |tick: u64, x: f64| RemoteSample {
            tick,
            pos: DVec3::new(x, 0.0, 0.0),
            yaw: 0,
            pitch: 0,
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
