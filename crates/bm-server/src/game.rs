//! The authoritative simulation: one thread, a fixed 60 Hz tick.
//!
//! Each tick: take network events, move finished chunk loads in, step every
//! player by the inputs they sent, stream chunks, and every third tick send
//! snapshots. The loop owns all game state; nothing else mutates it, so no
//! locks are needed and every rule is enforced in one place.

use crate::config::ServerConfig;
use crate::net::{Event, Outbox};
use crate::store::{Store, WriteBatch};
use crate::world::World;
use bm_core::movement::{self, MoveConfig, PlayerInput, PlayerState};
use bm_core::{BlockId, BlockPos, CHUNK_SIZE, ChunkPos, TICK_DT};
use bm_net::quinn::Connection;
use bm_protocol::{ClientMsg, EditAction, PLAYERS_PER_SNAPSHOT, RemotePlayer, ServerDatagram, ServerMsg, Welcome};
use bm_worldgen::Generator;
use crossbeam_channel::{Receiver, Sender};
use glam::DVec3;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Snapshots go out every this many ticks (20 Hz).
const SNAPSHOT_EVERY: u64 = 3;
const SAVE_EVERY: u64 = 60 * 5;
const EVICT_EVERY: u64 = 60 * 10;
const MARK_WANTED_EVERY: u64 = 60;
/// Most inputs a player may bank while their packets are late. Bounds how far
/// a lagging client can "catch up" in one burst, which is what stops a client
/// from sending inputs faster than real time (a speed hack).
const INPUT_BANK: f64 = 8.0;
const MAX_PENDING_INPUTS: usize = 120;
/// Chunk streaming limits per player per tick.
const CHUNK_BYTES_PER_TICK: usize = 192 << 10;
const MAX_BACKLOG: usize = 768 << 10;
const STREAM_SCAN: usize = 512;
/// Concurrent chunk loads across the whole server.
const MAX_PENDING_LOADS: usize = 512;
/// Edits per second, and the burst allowed.
const EDIT_RATE: f64 = 12.0;
const EDIT_BURST: f64 = 24.0;

struct Player {
    name: String,
    conn: Connection,
    out: Outbox,
    state: PlayerState,
    last_seq: u32,
    last_input: PlayerInput,
    pending: BTreeMap<u32, PlayerInput>,
    bank: f64,
    sent: HashSet<ChunkPos>,
    wanted: Vec<ChunkPos>,
    center: Option<ChunkPos>,
    edit_tokens: f64,
}

pub struct Game {
    cfg: ServerConfig,
    planet_name: String,
    world: World,
    store: Store,
    writer: Sender<WriteBatch>,
    events: Receiver<Event>,
    players: HashMap<u32, Player>,
    tick: u64,
    movement: MoveConfig,
    spawn: BlockPos,
    stats: Stats,
}

#[derive(Default)]
struct Stats {
    chunks_sent: u64,
    bytes_sent: u64,
    edits: u64,
    refused_edits: u64,
    slow_ticks: u64,
}

impl Game {
    pub fn new(
        cfg: ServerConfig,
        planet_name: String,
        world: World,
        generator: &Generator,
        store: Store,
        writer: Sender<WriteBatch>,
        events: Receiver<Event>,
    ) -> Self {
        Self {
            spawn: generator.spawn_point(),
            cfg,
            planet_name,
            world,
            store,
            writer,
            events,
            players: HashMap::new(),
            tick: 0,
            movement: MoveConfig::default(),
            stats: Stats::default(),
        }
    }

    /// Runs until `running` is cleared, then saves everything and returns.
    pub fn run(mut self, running: Arc<AtomicBool>) {
        let dt = Duration::from_secs_f64(TICK_DT);
        let mut next = Instant::now();
        let mut last_report = Instant::now();
        while running.load(Ordering::Relaxed) {
            let started = Instant::now();
            self.step();
            let took = started.elapsed();
            if took > dt {
                self.stats.slow_ticks += 1;
            }
            next += dt;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else if now - next > Duration::from_millis(250) {
                tracing::warn!("server is {:?} behind; skipping ahead", now - next);
                next = now;
            }
            if last_report.elapsed() > Duration::from_secs(30) {
                self.report();
                last_report = Instant::now();
            }
        }
        self.shutdown();
    }

    fn report(&mut self) {
        let s = &self.stats;
        tracing::info!(
            "tick {} | {} players | {} chunks resident, {} loading | sent {} chunks ({} KiB) | edits {} ok {} refused | {} slow ticks",
            self.tick,
            self.players.len(),
            self.world.resident(),
            self.world.pending_loads(),
            s.chunks_sent,
            s.bytes_sent / 1024,
            s.edits,
            s.refused_edits,
            s.slow_ticks
        );
    }

    /// One tick. Public so tests can drive the loop without real time.
    pub fn step(&mut self) {
        self.tick += 1;
        while let Ok(ev) = self.events.try_recv() {
            self.handle(ev);
        }
        self.world.receive_loaded(self.tick);
        self.simulate();
        let ids: Vec<u32> = self.players.keys().copied().collect();
        for id in &ids {
            self.stream(*id);
        }
        if self.tick % SNAPSHOT_EVERY == 0 {
            self.snapshots();
        }
        if self.tick % SAVE_EVERY == 0 {
            self.save(None);
        }
        if self.tick % EVICT_EVERY == 0 {
            let chunks = self.world.evict_unwanted(self.tick.saturating_sub(EVICT_EVERY));
            if !chunks.is_empty() {
                let _ = self.writer.send(WriteBatch {
                    chunks,
                    ..Default::default()
                });
            }
        }
    }

    fn handle(&mut self, ev: Event) {
        match ev {
            Event::Join { id, name, conn, out } => self.join(id, name, conn, out),
            Event::Leave { id } => self.leave(id),
            Event::Input { id, latest, inputs } => {
                if let Some(p) = self.players.get_mut(&id) {
                    for (i, input) in inputs.into_iter().enumerate() {
                        let Some(seq) = latest.checked_sub(i as u32) else { break };
                        if seq > p.last_seq && p.pending.len() < MAX_PENDING_INPUTS {
                            p.pending.entry(seq).or_insert(input);
                        }
                    }
                }
            }
            Event::Msg { id, msg } => match msg {
                ClientMsg::Edit { seq, pos, action } => self.edit(id, seq, pos, action),
                ClientMsg::Hello { .. } => {}
            },
        }
    }

    fn join(&mut self, id: u32, name: String, conn: Connection, out: Outbox) {
        // A second login under the same name replaces the first: after a
        // crash the old connection lingers until its idle timeout, and the
        // player should not be locked out for that long. (Accounts will make
        // this an authenticated decision.)
        let mut replaced = None;
        if let Some(old) = self
            .players
            .iter()
            .find(|(_, p)| p.name.eq_ignore_ascii_case(&name))
            .map(|(id, _)| *id)
        {
            if let Some(p) = self.players.get(&old) {
                p.out.close("you logged in from somewhere else");
                // The save of this state is still on its way to disk.
                replaced = Some(p.state);
            }
            self.leave(old);
        }
        if self.players.len() >= self.cfg.max_players {
            out.close("this planet is full");
            return;
        }
        let saved = match replaced {
            Some(s) => Some(s),
            None => self.store.player(&name).unwrap_or_else(|e| {
                tracing::error!("loading {name}: {e:#}");
                None
            }),
        };
        let state = saved
            .filter(|s| s.pos.is_finite() && s.vel.is_finite() && self.world.shape.contains_y(s.pos.y as i64))
            .map(|s| PlayerState {
                pos: self.world.shape.wrap_point(s.pos),
                ..s
            })
            .unwrap_or_else(|| PlayerState {
                pos: DVec3::new(
                    self.spawn.x as f64 + 0.5,
                    self.spawn.y as f64,
                    self.spawn.z as f64 + 0.5,
                ),
                ..Default::default()
            });
        out.send(&ServerMsg::Welcome(Box::new(Welcome {
            player_id: id,
            planet: self.world.shape,
            planet_name: self.planet_name.clone(),
            blocks: self.world.registry.defs().to_vec(),
            movement: self.movement,
            spawn: state,
            tick: self.tick,
            view_radius: self.cfg.view_radius,
            reach: self.cfg.reach,
        })));
        for (oid, other) in &self.players {
            out.send(&ServerMsg::PlayerJoined {
                id: *oid,
                name: other.name.clone(),
            });
            other.out.send(&ServerMsg::PlayerJoined { id, name: name.clone() });
        }
        tracing::info!("{name} (#{id}) joined at {:.1?}", state.pos);
        self.players.insert(
            id,
            Player {
                name,
                conn,
                out,
                state,
                last_seq: 0,
                last_input: PlayerInput::default(),
                pending: BTreeMap::new(),
                bank: 0.0,
                sent: HashSet::new(),
                wanted: Vec::new(),
                center: None,
                edit_tokens: EDIT_BURST,
            },
        );
    }

    fn leave(&mut self, id: u32) {
        let Some(p) = self.players.remove(&id) else { return };
        tracing::info!("{} (#{id}) left", p.name);
        let _ = self.writer.send(WriteBatch {
            players: vec![(p.name, p.state)],
            ..Default::default()
        });
        for other in self.players.values() {
            other.out.send(&ServerMsg::PlayerLeft { id });
        }
    }

    /// Applies each player's inputs in sequence order, one per tick of real
    /// time on average (ADR-0004).
    fn simulate(&mut self) {
        let shape = self.world.shape;
        for p in self.players.values_mut() {
            p.edit_tokens = (p.edit_tokens + EDIT_RATE * TICK_DT).min(EDIT_BURST);
            p.bank = (p.bank + 1.0).min(INPUT_BANK);
            while p.bank >= 1.0 {
                let Some((&seq, _)) = p.pending.first_key_value() else {
                    break;
                };
                let input = if seq == p.last_seq + 1 {
                    p.pending.pop_first().expect("present").1
                } else {
                    // The next input was lost beyond what redundancy covers:
                    // repeat the last one. The client sees our answer in the
                    // snapshot and corrects its prediction.
                    p.last_input
                };
                movement::step(&mut p.state, &input, &self.movement, shape, &self.world);
                p.last_seq += 1;
                p.last_input = input;
                p.bank -= 1.0;
            }
        }
    }

    fn view_list(&self, center: ChunkPos) -> Vec<ChunkPos> {
        let r = self.cfg.view_radius as i32;
        let h = self.cfg.view_height as i32;
        let mut list = Vec::new();
        for dy in -h..=h {
            let y = center.y + dy;
            if !self.world.shape.contains_chunk_y(y) {
                continue;
            }
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dz * dz <= r * r {
                        list.push((
                            dx * dx + dz * dz + dy * dy * 2,
                            self.world.shape.wrap_chunk(center.offset(dx, dy, dz)),
                        ));
                    }
                }
            }
        }
        list.sort_by_key(|(d, _)| *d);
        list.into_iter().map(|(_, p)| p).collect()
    }

    fn stream(&mut self, id: u32) {
        let Some(p) = self.players.get(&id) else { return };
        let here = self
            .world
            .shape
            .wrap_chunk(BlockPos::containing(p.state.pos.x, p.state.pos.y, p.state.pos.z).chunk());
        if p.center != Some(here) {
            let wanted = self.view_list(here);
            let (r, h) = (self.cfg.view_radius as i32 + 2, self.cfg.view_height as i32 + 2);
            let shape = self.world.shape;
            let p = self.players.get_mut(&id).expect("present");
            let forget: Vec<ChunkPos> = p
                .sent
                .iter()
                .copied()
                .filter(|c| {
                    let (dx, dy, dz) = shape.chunk_delta(here, *c);
                    dx * dx + dz * dz > r * r || dy.abs() > h
                })
                .collect();
            for c in forget {
                p.sent.remove(&c);
                p.out.send(&ServerMsg::Forget { pos: c });
            }
            p.wanted = wanted;
            p.center = Some(here);
        }

        let mark = self.tick % MARK_WANTED_EVERY == 0;
        let p = self.players.get_mut(&id).expect("present");
        if mark {
            for c in &p.wanted {
                if let Some(slot) = self.world.get_mut(*c) {
                    slot.wanted_at = self.tick;
                }
            }
        }
        let mut budget = CHUNK_BYTES_PER_TICK;
        let mut scanned = 0;
        for i in 0..p.wanted.len() {
            if scanned >= STREAM_SCAN || budget == 0 || p.out.backlog() > MAX_BACKLOG {
                break;
            }
            let c = p.wanted[i];
            if p.sent.contains(&c) {
                continue;
            }
            scanned += 1;
            match self.world.get_mut(c) {
                Some(slot) => {
                    slot.wanted_at = self.tick;
                    let frame = slot.frame(c);
                    budget = budget.saturating_sub(frame.len());
                    self.stats.chunks_sent += 1;
                    self.stats.bytes_sent += frame.len() as u64;
                    p.out.send_shared(frame);
                    p.sent.insert(c);
                }
                None => {
                    if self.world.pending_loads() < MAX_PENDING_LOADS {
                        self.world.request(c);
                    }
                }
            }
        }
    }

    fn snapshots(&self) {
        let shape = self.world.shape;
        let range = (self.cfg.view_radius as f64 + 1.0) * CHUNK_SIZE as f64;
        for p in self.players.values() {
            let mut near: Vec<(f64, RemotePlayer)> = self
                .players
                .iter()
                .filter(|(_, o)| !std::ptr::eq(*o, p))
                .filter_map(|(oid, o)| {
                    let d = shape.delta(p.state.pos, o.state.pos);
                    let dist2 = d.x * d.x + d.z * d.z;
                    (dist2 <= range * range).then_some({
                        (
                            dist2,
                            RemotePlayer {
                                id: *oid,
                                pos: [o.state.pos.x as f32, o.state.pos.y as f32, o.state.pos.z as f32],
                                yaw: o.last_input.yaw,
                                pitch: o.last_input.pitch,
                            },
                        )
                    })
                })
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            near.truncate(PLAYERS_PER_SNAPSHOT * 4);
            let others: Vec<RemotePlayer> = near.into_iter().map(|(_, r)| r).collect();
            let mut parts = others.chunks(PLAYERS_PER_SNAPSHOT).peekable();
            if parts.peek().is_none() {
                self.send_snapshot(p, Vec::new());
            }
            for part in parts {
                self.send_snapshot(p, part.to_vec());
            }
        }
    }

    fn send_snapshot(&self, p: &Player, others: Vec<RemotePlayer>) {
        bm_net::send_datagram(
            &p.conn,
            &ServerDatagram::Snapshot {
                tick: self.tick,
                ack: p.last_seq,
                you: p.state,
                others,
            },
        );
    }

    fn edit(&mut self, id: u32, seq: u32, pos: BlockPos, action: EditAction) {
        let verdict = self.check_edit(id, pos, action);
        let Some(p) = self.players.get_mut(&id) else { return };
        match verdict {
            Ok(new) => {
                self.world.set_block(pos, new);
                self.stats.edits += 1;
                let canon = self.world.shape.wrap_block(pos);
                p.out.send(&ServerMsg::EditResult {
                    seq,
                    pos,
                    block: new,
                    accepted: true,
                });
                let chunk = canon.chunk();
                for (oid, o) in &self.players {
                    if *oid != id && o.sent.contains(&chunk) {
                        o.out.send(&ServerMsg::BlockChanged { pos: canon, block: new });
                    }
                }
            }
            Err(why) => {
                self.stats.refused_edits += 1;
                tracing::debug!("refused edit from #{id} at {pos:?}: {why}");
                let actual = self.world.block(pos).unwrap_or(BlockId::AIR);
                p.out.send(&ServerMsg::EditResult {
                    seq,
                    pos,
                    block: actual,
                    accepted: false,
                });
            }
        }
    }

    /// Every rule an edit must pass. Returns the block to write.
    fn check_edit(&mut self, id: u32, pos: BlockPos, action: EditAction) -> Result<BlockId, &'static str> {
        let shape = self.world.shape;
        let p = self.players.get_mut(&id).ok_or("no such player")?;
        if p.edit_tokens < 1.0 {
            return Err("editing too fast");
        }
        p.edit_tokens -= 1.0;
        if !shape.contains_y(pos.y) {
            return Err("outside the planet");
        }
        let canon = shape.wrap_block(pos);
        if !p.sent.contains(&canon.chunk()) {
            return Err("chunk not loaded for this player");
        }
        let eye = p.state.eye(&self.movement);
        let centre = DVec3::new(canon.x as f64 + 0.5, canon.y as f64 + 0.5, canon.z as f64 + 0.5);
        if shape.delta(eye, centre).length() > self.cfg.reach + 0.87 {
            return Err("out of reach");
        }
        let current = self.world.block(canon).ok_or("chunk not resident")?;
        let reg = &self.world.registry;
        match action {
            EditAction::Break => {
                let def = reg.get(current);
                if current.is_air() || !def.breakable {
                    return Err("cannot break that");
                }
                Ok(BlockId::AIR)
            }
            EditAction::Place(block) => {
                if !reg.contains(block) || !reg.get(block).placeable {
                    return Err("cannot place that");
                }
                if !(current.is_air() || current == self.world.water) {
                    return Err("space is occupied");
                }
                let supported = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                    .iter()
                    .any(|&(dx, dy, dz)| {
                        self.world
                            .block(canon.offset(dx, dy, dz))
                            .is_some_and(|b| self.world.flags.solid(b))
                    });
                if !supported {
                    return Err("nothing to place against");
                }
                if reg.get(block).solid {
                    let m = self.movement;
                    let blocked = self.players.values().any(|o| {
                        let st = PlayerState {
                            pos: shape.nearest_lap(centre, o.state.pos),
                            ..o.state
                        };
                        st.overlaps_block(&m, canon) || st.overlaps_block(&m, pos)
                    });
                    if blocked {
                        return Err("a player is in the way");
                    }
                }
                Ok(block)
            }
        }
    }

    /// Hands every dirty chunk and every player's position to the writer.
    fn save(&mut self, done: Option<Sender<()>>) {
        let batch = WriteBatch {
            chunks: self.world.take_dirty(),
            players: self.players.values().map(|p| (p.name.clone(), p.state)).collect(),
            done,
        };
        if !batch.is_empty() || batch.done.is_some() {
            let _ = self.writer.send(batch);
        }
    }

    fn shutdown(&mut self) {
        tracing::info!("saving before shutdown");
        for p in self.players.values() {
            p.out.close("the server is shutting down");
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.save(Some(tx));
        if rx.recv_timeout(Duration::from_secs(30)).is_err() {
            tracing::error!("save did not finish within 30 s");
        }
        self.report();
    }
}
