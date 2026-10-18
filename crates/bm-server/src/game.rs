//! The authoritative simulation: one thread, a fixed 60 Hz tick.
//!
//! Each tick: take network events, move finished chunk loads in, step every
//! player by the inputs they sent, advance station jobs, stream chunks, and
//! every third tick send snapshots. The loop owns all game state (world,
//! inventories, stations, claims); nothing else mutates it, so no locks are
//! needed and every rule is enforced in one place.

use crate::claims::{ClaimError, Claims};
use crate::config::ServerConfig;
use crate::crafting::{self, CraftError, MAX_CRAFT_COUNT, Stations};
use crate::net::{Event, Outbox};
use crate::store::{PlayerData, Store, WriteBatch, load_slots, save_slots};
use crate::world::World;
use anyhow::Result;
use bm_core::items::title_case;
use bm_core::movement::{self, MoveConfig, PlayerInput, PlayerState};
use bm_core::{
    BlockId, BlockPos, CHUNK_SIZE, ChunkPos, DayCycle, HOTBAR_SLOTS, Inventory, ItemId, ItemRegistry, PlanetShape,
    PlotPos, RecipeBook, Station, TICK_DT,
};
use bm_net::quinn::Connection;
use bm_protocol::{ClientMsg, EditAction, PLAYERS_PER_SNAPSHOT, RemotePlayer, ServerDatagram, ServerMsg, Welcome};
use bm_worldgen::Generator;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use glam::DVec3;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Snapshots go out every this many ticks (20 Hz).
const SNAPSHOT_EVERY: u64 = 3;
const SAVE_EVERY: u64 = 60 * 5;
const EVICT_EVERY: u64 = 60 * 10;
const MARK_WANTED_EVERY: u64 = 60;
/// A watched station with a job running is re-sent this often (4 Hz), so its
/// progress bar moves.
const STATION_UPDATE_EVERY: u64 = 15;
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
/// How far ahead of the simulated inputs an edit may be tagged (2 s).
const MAX_EDIT_LEAD: u32 = 120;
const MAX_QUEUED_EDITS: usize = 64;
/// A break may arrive this much sooner than its break time (the block's
/// hardness over the tool's speed), in seconds plus a fraction of that time,
/// to absorb network jitter between the player's StartBreak and Break.
const BREAK_SLACK_SECONDS: f64 = 0.12;
const BREAK_SLACK_FRACTION: f64 = 0.2;

/// The block a player is holding break on.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mining {
    pos: BlockPos,
    /// Tick the hold began.
    since: u64,
    /// Hotbar slot held, and the item in it then (a tool, or anything).
    slot: u8,
    tool: Option<ItemId>,
}

struct Player {
    /// Lowercased account name.
    account: String,
    name: String,
    conn: Connection,
    out: Outbox,
    state: PlayerState,
    inventory: Inventory,
    last_seq: u32,
    last_input: PlayerInput,
    pending: BTreeMap<u32, PlayerInput>,
    bank: f64,
    sent: HashSet<ChunkPos>,
    wanted: Vec<ChunkPos>,
    center: Option<ChunkPos>,
    edit_tokens: f64,
    mining: Option<Mining>,
    /// Edits waiting for the input they follow: (at, seq, pos, action).
    edits: VecDeque<(u32, u32, BlockPos, EditAction)>,
    /// The station whose updates this player receives.
    watching: Option<BlockPos>,
}

/// A player who left, held in memory until the save holding their last state
/// is durable: a quick rejoin must not read an older copy from disk.
struct Departed {
    state: PlayerState,
    inventory: Inventory,
    /// Fires (or disconnects) once their save is written; None until sent.
    written: Option<Receiver<()>>,
}

/// Why an edit was refused: `Quiet` for what the client already prevents
/// (timing, reach), `Told` for what the player should read.
enum Refusal {
    Quiet(&'static str),
    Told(String),
}

impl From<&'static str> for Refusal {
    fn from(why: &'static str) -> Self {
        Refusal::Quiet(why)
    }
}

impl From<ClaimError> for Refusal {
    fn from(e: ClaimError) -> Self {
        Refusal::Told(e.to_string())
    }
}

/// An accepted edit: the block to write and what else it does.
struct Accepted {
    block: BlockId,
    effect: Effect,
}

enum Effect {
    Broke {
        old: BlockId,
        drop: Option<ItemId>,
        /// The hotbar slot of a matching tool, which wears.
        wear: Option<usize>,
    },
    Placed {
        slot: usize,
    },
}

pub struct Game {
    cfg: ServerConfig,
    planet_name: String,
    world: World,
    items: ItemRegistry,
    recipes: RecipeBook,
    stations: Stations,
    claims: Claims,
    claim_stone: BlockId,
    store: Store,
    writer: Sender<WriteBatch>,
    events: Receiver<Event>,
    players: HashMap<u32, Player>,
    departed: HashMap<String, Departed>,
    tick: u64,
    day: DayCycle,
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

fn station_name(kind: Station) -> &'static str {
    match kind {
        Station::Hand => "hand",
        Station::Workbench => "Workbench",
        Station::Furnace => "Furnace",
    }
}

/// Whether a block is within a player's reach (eye to block centre).
fn in_reach(shape: PlanetShape, m: &MoveConfig, reach: f64, state: &PlayerState, canon: BlockPos) -> bool {
    let centre = DVec3::new(canon.x as f64 + 0.5, canon.y as f64 + 0.5, canon.z as f64 + 0.5);
    shape.delta(state.eye(m), centre).length() <= reach + 0.87
}

/// Sends the slots that differ from `before`.
fn send_slots(p: &Player, before: &Inventory) {
    let changes: Vec<_> = (0..p.inventory.slots.len())
        .filter(|&i| before.slots.get(i) != p.inventory.slots.get(i))
        .map(|i| (i as u8, p.inventory.get(i)))
        .collect();
    if !changes.is_empty() {
        p.out.send(&ServerMsg::Slots { changes });
    }
}

/// Makes a stored inventory safe: impossible stacks and spent tools go.
fn sanitize(inv: &mut Inventory, items: &ItemRegistry) {
    inv.sanitize(items);
    for slot in inv.slots.iter_mut() {
        if let Some(s) = slot {
            let spent = items
                .get(s.item)
                .and_then(|d| d.tool())
                .is_some_and(|(_, _, d)| s.wear >= d);
            let worn_material = s.wear > 0 && items.get(s.item).is_some_and(|d| d.tool().is_none());
            if spent || worn_material {
                *slot = None;
            }
        }
    }
}

impl Game {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cfg: ServerConfig,
        planet_name: String,
        world: World,
        generator: &Generator,
        items: ItemRegistry,
        recipes: RecipeBook,
        store: Store,
        writer: Sender<WriteBatch>,
        events: Receiver<Event>,
    ) -> Result<Self> {
        let tick = store.clock().unwrap_or_else(|e| {
            tracing::error!("reading the planet clock: {e:#}; starting at 0");
            0
        });
        // Claims and stations are few and must work while nobody is near, so
        // they live in memory. Failing to read them is fatal: running without
        // them would leave claimed land open and stations empty.
        let stations = Stations::load(store.stations()?, &items, &recipes);
        let (claims, next_claim) = store.claims()?;
        let claims = Claims::load(world.shape, claims, next_claim);
        let claim_stone = world.registry.id("claim_stone")?;
        Ok(Self {
            spawn: generator.spawn_point(),
            cfg,
            planet_name,
            world,
            items,
            recipes,
            stations,
            claims,
            claim_stone,
            store,
            writer,
            events,
            players: HashMap::new(),
            departed: HashMap::new(),
            tick,
            day: DayCycle::default(),
            movement: MoveConfig::default(),
            stats: Stats::default(),
        })
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
        self.step_stations();
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
            Event::Join {
                id,
                account,
                name,
                conn,
                out,
            } => self.join(id, account, name, conn, out),
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
                ClientMsg::Edit { seq, at, pos, action } => {
                    if let Some(p) = self.players.get_mut(&id) {
                        if at <= p.last_seq {
                            // Late (its input was already simulated, e.g. a
                            // lost input was repeated): apply now.
                            self.edit(id, seq, pos, action);
                        } else if at > p.last_seq + MAX_EDIT_LEAD || p.edits.len() >= MAX_QUEUED_EDITS {
                            self.edit_refused(id, seq, pos, "edit too far ahead of movement");
                        } else {
                            p.edits.push_back((at, seq, pos, action));
                        }
                    }
                }
                ClientMsg::StartBreak { pos, slot } => self.start_break(id, pos, slot),
                ClientMsg::MoveItems { from, to, count } => {
                    self.change_inventory(id, |inv, items| {
                        inv.move_items(items, from as usize, to as usize, count)
                    });
                }
                ClientMsg::Discard { slot } => {
                    self.change_inventory(id, |inv, _| inv.clear(slot as usize));
                }
                ClientMsg::Craft { recipe, count, station } => self.craft(id, recipe, count, station),
                ClientMsg::WatchStation { pos } => self.watch(id, pos),
                ClientMsg::CollectStation { pos } => self.collect(id, pos),
                ClientMsg::ClaimPlot { claim, plot } => self.claim_plot(id, claim, plot),
                ClientMsg::ReleasePlot { plot } => self.release_plot(id, plot),
                ClientMsg::Hello { .. } => {}
            },
        }
    }

    fn join(&mut self, id: u32, account: String, name: String, conn: Connection, out: Outbox) {
        // A second login to the same account replaces the first: after a
        // crash the old connection lingers until its idle timeout, and the
        // player should not be locked out for that long. Only an
        // authenticated login gets this far.
        if let Some(old) = self
            .players
            .iter()
            .find(|(_, p)| p.account == account)
            .map(|(id, _)| *id)
        {
            if let Some(p) = self.players.get(&old) {
                p.out.close("you logged in from somewhere else");
            }
            // Its state moves to `departed`, where it is picked up below.
            self.leave(old);
        }
        if self.players.len() >= self.cfg.max_players {
            out.close("this planet is full");
            return;
        }
        let (saved, mut inventory) = match self.departed.remove(&account) {
            Some(d) => (Some(d.state), d.inventory),
            None => match self.store.player(&account) {
                Ok(Some(data)) => (
                    data.state,
                    Inventory {
                        slots: load_slots(&self.items, &data.inventory, &name),
                    },
                ),
                Ok(None) => (None, Inventory::default()),
                Err(e) => {
                    // Never start them afresh: the empty inventory would be
                    // saved over the real one.
                    tracing::error!("loading {name}: {e:#}");
                    out.close("your character could not be loaded; try again in a moment");
                    return;
                }
            },
        };
        sanitize(&mut inventory, &self.items);
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
            account: name.clone(),
            planet: self.world.shape,
            planet_name: self.planet_name.clone(),
            blocks: self.world.registry.defs().to_vec(),
            items: self.items.defs().to_vec(),
            recipes: self.recipes.recipes.clone(),
            inventory: inventory.clone(),
            movement: self.movement,
            spawn: state,
            tick: self.tick,
            view_radius: self.cfg.view_radius,
            reach: self.cfg.reach,
            day: self.day,
            claims: self.claims.views(),
            plot_allowance: self.cfg.plot_allowance,
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
                account,
                name,
                conn,
                out,
                state,
                inventory,
                last_seq: 0,
                last_input: PlayerInput::default(),
                pending: BTreeMap::new(),
                bank: 0.0,
                sent: HashSet::new(),
                wanted: Vec::new(),
                center: None,
                edit_tokens: EDIT_BURST,
                mining: None,
                edits: VecDeque::new(),
                watching: None,
            },
        );
    }

    fn leave(&mut self, id: u32) {
        let Some(p) = self.players.remove(&id) else { return };
        tracing::info!("{} (#{id}) left", p.name);
        self.departed.insert(
            p.account,
            Departed {
                state: p.state,
                inventory: p.inventory,
                written: None,
            },
        );
        // Everything, not just the player: what they gathered and built
        // must reach the disk in the same transaction as their inventory.
        self.save(None);
        for other in self.players.values() {
            other.out.send(&ServerMsg::PlayerLeft { id });
        }
    }

    /// Applies each player's inputs in sequence order, one per tick of real
    /// time on average (ADR-0004).
    fn simulate(&mut self) {
        let shape = self.world.shape;
        let ids: Vec<u32> = self.players.keys().copied().collect();
        for id in ids {
            {
                let p = self.players.get_mut(&id).expect("present");
                p.edit_tokens = (p.edit_tokens + EDIT_RATE * TICK_DT).min(EDIT_BURST);
                p.bank = (p.bank + 1.0).min(INPUT_BANK);
            }
            loop {
                // Edits tagged with an input already simulated happen now,
                // before the next input moves the player.
                self.apply_due_edits(id);
                let p = self.players.get_mut(&id).expect("present");
                if p.bank < 1.0 {
                    break;
                }
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

    fn apply_due_edits(&mut self, id: u32) {
        loop {
            let Some(p) = self.players.get_mut(&id) else { return };
            match p.edits.front() {
                Some(&(at, ..)) if at <= p.last_seq => {
                    let (_, seq, pos, action) = p.edits.pop_front().expect("present");
                    self.edit(id, seq, pos, action);
                }
                _ => return,
            }
        }
    }

    /// Every chunk of every column within the view radius (M2: planets are
    /// bounded in height, so whole columns are sent and sky light is exact
    /// from the top down). Nearest columns first; within a column, the
    /// chunks nearest the player's height first.
    fn view_list(&self, center: ChunkPos, eye_cy: i32) -> Vec<ChunkPos> {
        let r = self.cfg.view_radius as i32;
        let mut list = Vec::new();
        for dz in -r..=r {
            for dx in -r..=r {
                let d2 = dx * dx + dz * dz;
                if d2 > r * r {
                    continue;
                }
                for y in 0..self.world.shape.height_chunks as i32 {
                    let key = d2 * 16 + (y - eye_cy).abs();
                    list.push((
                        key,
                        self.world
                            .shape
                            .wrap_chunk(ChunkPos::new(center.x + dx, y, center.z + dz)),
                    ));
                }
            }
        }
        list.sort_by_key(|(d, _)| *d);
        list.into_iter().map(|(_, p)| p).collect()
    }

    fn stream(&mut self, id: u32) {
        let Some(p) = self.players.get(&id) else { return };
        let at = self
            .world
            .shape
            .wrap_chunk(BlockPos::containing(p.state.pos.x, p.state.pos.y, p.state.pos.z).chunk());
        // Only the column matters: whole columns are streamed.
        let here = ChunkPos::new(at.x, 0, at.z);
        if p.center != Some(here) {
            let wanted = self.view_list(here, at.y);
            let r = self.cfg.view_radius as i32 + 2;
            let shape = self.world.shape;
            let p = self.players.get_mut(&id).expect("present");
            let forget: Vec<ChunkPos> = p
                .sent
                .iter()
                .copied()
                .filter(|c| {
                    let (dx, _, dz) = shape.chunk_delta(here, *c);
                    dx * dx + dz * dz > r * r
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

    fn notice(&self, id: u32, text: &str) {
        if let Some(p) = self.players.get(&id) {
            p.out.send(&ServerMsg::Notice { text: text.into() });
        }
    }

    /// Runs `f` on a player's inventory and sends them the slots it changed.
    fn change_inventory<R>(&mut self, id: u32, f: impl FnOnce(&mut Inventory, &ItemRegistry) -> R) -> Option<R> {
        let p = self.players.get_mut(&id)?;
        let before = p.inventory.clone();
        let r = f(&mut p.inventory, &self.items);
        send_slots(p, &before);
        Some(r)
    }

    fn start_break(&mut self, id: u32, pos: BlockPos, slot: u8) {
        let canon = self.world.shape.wrap_block(pos);
        let tick = self.tick;
        let Some(p) = self.players.get_mut(&id) else { return };
        let tool = ((slot as usize) < HOTBAR_SLOTS)
            .then(|| p.inventory.get(slot as usize).map(|s| s.item))
            .flatten();
        // Holding on, the hold continues; a new block or a different tool
        // starts it again, so a fast tool cannot borrow a slow one's time.
        if p.mining
            .is_none_or(|m| m.pos != canon || m.slot != slot || m.tool != tool)
        {
            p.mining = Some(Mining {
                pos: canon,
                since: tick,
                slot,
                tool,
            });
        }
    }

    fn edit(&mut self, id: u32, seq: u32, pos: BlockPos, action: EditAction) {
        let accepted = match self.check_edit(id, pos, action) {
            Ok(a) => a,
            Err(Refusal::Quiet(why)) => return self.edit_refused(id, seq, pos, why),
            Err(Refusal::Told(text)) => {
                self.notice(id, &text);
                return self.edit_refused(id, seq, pos, &text);
            }
        };
        let canon = self.world.shape.wrap_block(pos);
        let new = accepted.block;
        let Some(p) = self.players.get_mut(&id) else { return };
        p.out.send(&ServerMsg::EditResult {
            seq,
            pos,
            block: new,
            accepted: true,
        });
        let before = p.inventory.clone();
        let mut broke = None;
        let mut claim_created = None;
        let mut claim_removed = None;
        match accepted.effect {
            Effect::Broke { old, drop, wear } => {
                if let Some(item) = drop {
                    // Checked to fit.
                    p.inventory.add(&self.items, item, 1);
                }
                if let Some(slot) = wear
                    && let Some(s) = p.inventory.slots[slot].as_mut()
                {
                    s.wear += 1;
                    if let Some(def) = self.items.get(s.item)
                        && def.tool().is_some_and(|(_, _, durability)| s.wear >= durability)
                    {
                        broke = Some(format!("Your {} broke", def.display));
                        p.inventory.slots[slot] = None;
                    }
                }
                if old == self.claim_stone {
                    claim_removed = self.claims.remove_by_stone(canon);
                }
            }
            Effect::Placed { slot } => {
                p.inventory.take_one(slot);
                if new == self.claim_stone {
                    match self.claims.create(&p.account, &p.name, canon, self.cfg.plot_allowance) {
                        Ok(claim) => claim_created = Some(claim),
                        Err(e) => tracing::error!("claim checked but not created: {e}"),
                    }
                }
            }
        }
        send_slots(p, &before);
        if let Some(text) = broke {
            p.out.send(&ServerMsg::Notice { text });
        }
        self.world.set_block(pos, new);
        self.stats.edits += 1;
        let chunk = canon.chunk();
        for (oid, o) in &self.players {
            if *oid != id && o.sent.contains(&chunk) {
                o.out.send(&ServerMsg::BlockChanged { pos: canon, block: new });
            }
        }
        if let Some(claim) = claim_created {
            if let Some(p) = self.players.get(&id) {
                p.out.send(&ServerMsg::Notice {
                    text: format!(
                        "This land is yours. It stays yours as long as you visit at least once every {} months.",
                        self.cfg.claim_keep_days / 30
                    ),
                });
            }
            self.broadcast_claim(claim);
        }
        if let Some(claim) = claim_removed {
            for o in self.players.values() {
                o.out.send(&ServerMsg::ClaimRemoved { id: claim });
            }
        }
    }

    fn edit_refused(&mut self, id: u32, seq: u32, pos: BlockPos, why: &str) {
        self.stats.refused_edits += 1;
        tracing::debug!("refused edit from #{id} at {pos:?}: {why}");
        let actual = self.world.block(pos).unwrap_or(BlockId::AIR);
        if let Some(p) = self.players.get(&id) {
            p.out.send(&ServerMsg::EditResult {
                seq,
                pos,
                block: actual,
                accepted: false,
            });
        }
    }

    /// Every rule an edit must pass. Changes nothing but the rate tokens and
    /// the break hold; `edit` applies what it returns.
    fn check_edit(&mut self, id: u32, pos: BlockPos, action: EditAction) -> Result<Accepted, Refusal> {
        let shape = self.world.shape;
        let p = self.players.get_mut(&id).ok_or("no such player")?;
        if p.edit_tokens < 1.0 {
            return Err("editing too fast".into());
        }
        p.edit_tokens -= 1.0;
        if !shape.contains_y(pos.y) {
            return Err("outside the planet".into());
        }
        let canon = shape.wrap_block(pos);
        if !p.sent.contains(&canon.chunk()) {
            return Err("chunk not loaded for this player".into());
        }
        if !in_reach(shape, &self.movement, self.cfg.reach, &p.state, canon) {
            return Err("out of reach".into());
        }
        let current = self.world.block(canon).ok_or("chunk not resident")?;
        if let Some(c) = self.claims.at_block(canon)
            && c.owner != p.account
        {
            return Err(ClaimError::OwnedBy(c.owner_name.clone()).into());
        }
        let reg = &self.world.registry;
        match action {
            EditAction::Break => {
                let def = reg.get(current);
                if current.is_air() || !def.breakable {
                    return Err("cannot break that".into());
                }
                // The tool counts only if the slot still holds it.
                let tool = p
                    .mining
                    .filter(|m| m.pos == canon && p.inventory.get(m.slot as usize).map(|s| s.item) == m.tool)
                    .and_then(|m| m.tool);
                let seconds = self.items.break_seconds(def, tool) as f64;
                if seconds > 0.0 {
                    let held = match p.mining {
                        Some(m) if m.pos == canon => (self.tick - m.since) as f64 * TICK_DT,
                        _ => return Err("break without holding".into()),
                    };
                    if held + BREAK_SLACK_SECONDS + seconds * BREAK_SLACK_FRACTION < seconds {
                        return Err("broken too fast".into());
                    }
                }
                if self.stations.has_contents(canon) {
                    return Err(Refusal::Told(format!(
                        "Empty the {} before breaking it",
                        title_case(&def.name)
                    )));
                }
                let drop = self.items.drop_of(current);
                if let Some(item) = drop
                    && !p.inventory.can_fit(&self.items, item, 1)
                {
                    return Err(Refusal::Told("Your inventory is full".into()));
                }
                let wear = tool
                    .and_then(|t| self.items.get(t))
                    .and_then(|d| d.tool())
                    .filter(|(class, _, _)| class.suits(def.material))
                    .and(p.mining.map(|m| m.slot as usize));
                p.mining = None;
                Ok(Accepted {
                    block: BlockId::AIR,
                    effect: Effect::Broke {
                        old: current,
                        drop,
                        wear,
                    },
                })
            }
            EditAction::Place { slot } => {
                let slot = slot as usize;
                if slot >= HOTBAR_SLOTS {
                    return Err("not a hotbar slot".into());
                }
                let block = p
                    .inventory
                    .get(slot)
                    .and_then(|s| self.items.get(s.item))
                    .and_then(|d| d.block())
                    .ok_or("nothing placeable in that slot")?;
                if !reg.contains(block) || !reg.get(block).placeable {
                    return Err("cannot place that".into());
                }
                if !(current.is_air() || current == self.world.water) {
                    return Err("space is occupied".into());
                }
                let supported = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                    .iter()
                    .any(|&(dx, dy, dz)| {
                        self.world
                            .block(canon.offset(dx, dy, dz))
                            .is_some_and(|b| self.world.flags.solid(b))
                    });
                if !supported {
                    return Err("nothing to place against".into());
                }
                if block == self.claim_stone {
                    self.claims.can_create(&p.account, canon, self.cfg.plot_allowance)?;
                }
                if reg.get(block).solid {
                    let m = self.movement;
                    let centre = DVec3::new(canon.x as f64 + 0.5, canon.y as f64 + 0.5, canon.z as f64 + 0.5);
                    let blocked = self.players.values().any(|o| {
                        let st = PlayerState {
                            pos: shape.nearest_lap(centre, o.state.pos),
                            ..o.state
                        };
                        st.overlaps_block(&m, canon) || st.overlaps_block(&m, pos)
                    });
                    if blocked {
                        return Err("a player is in the way".into());
                    }
                }
                Ok(Accepted {
                    block,
                    effect: Effect::Placed { slot },
                })
            }
        }
    }

    /// The station at `pos` if this player may use it: loaded for them,
    /// within reach, and not on someone else's claim.
    fn station_access(&self, id: u32, pos: BlockPos) -> Result<(BlockPos, Station), Refusal> {
        let shape = self.world.shape;
        let p = self.players.get(&id).ok_or("no such player")?;
        if !shape.contains_y(pos.y) {
            return Err("outside the planet".into());
        }
        let canon = shape.wrap_block(pos);
        if !p.sent.contains(&canon.chunk()) {
            return Err("chunk not loaded for this player".into());
        }
        if !in_reach(shape, &self.movement, self.cfg.reach, &p.state, canon) {
            return Err(Refusal::Told("That is out of reach".into()));
        }
        let block = self.world.block(canon).ok_or("chunk not resident")?;
        let kind = self.world.registry.get(block).station.ok_or("not a station")?;
        if let Some(c) = self.claims.at_block(canon)
            && c.owner != p.account
        {
            return Err(ClaimError::OwnedBy(c.owner_name.clone()).into());
        }
        Ok((canon, kind))
    }

    fn tell(&self, id: u32, r: Refusal) {
        match r {
            Refusal::Told(text) => self.notice(id, &text),
            Refusal::Quiet(why) => tracing::debug!("refused request from #{id}: {why}"),
        }
    }

    fn craft(&mut self, id: u32, index: u16, count: u16, station: Option<BlockPos>) {
        let Some(recipe) = self.recipes.get(index).cloned() else {
            return self.tell(id, "no such recipe".into());
        };
        if count == 0 || count > MAX_CRAFT_COUNT {
            return self.notice(id, &CraftError::TooMany.to_string());
        }
        if recipe.station == Station::Hand {
            if let Some(Err(e)) =
                self.change_inventory(id, |inv, items| crafting::craft_by_hand(inv, items, &recipe, count))
            {
                self.notice(id, &e.to_string());
            }
            return;
        }
        let needs = format!("That is made at a {}", station_name(recipe.station));
        let Some(pos) = station else {
            return self.notice(id, &needs);
        };
        let canon = match self.station_access(id, pos) {
            Ok((canon, kind)) if kind == recipe.station => canon,
            Ok(_) => return self.notice(id, &needs),
            Err(r) => return self.tell(id, r),
        };
        if !self.stations.has_room(canon) {
            return self.notice(id, &CraftError::QueueFull.to_string());
        }
        match self.change_inventory(id, |inv, _| crafting::take_inputs(inv, &recipe, count)) {
            Some(true) => {}
            Some(false) => return self.notice(id, &CraftError::Materials.to_string()),
            None => return,
        }
        if let Err(e) = self.stations.queue(canon, recipe.station, index, count, self.tick) {
            // Unreachable (room was checked), but never take inputs silently.
            tracing::error!("queueing at {canon:?} after taking inputs: {e}");
        }
        self.push_station(canon);
    }

    fn watch(&mut self, id: u32, pos: Option<BlockPos>) {
        let target = match pos {
            None => None,
            Some(pos) => match self.station_access(id, pos) {
                Ok((canon, _)) => Some(canon),
                Err(r) => {
                    self.tell(id, r);
                    None
                }
            },
        };
        let view = target.and_then(|canon| self.station_view(canon));
        let Some(p) = self.players.get_mut(&id) else { return };
        p.watching = target;
        if let Some(view) = view {
            p.out.send(&ServerMsg::Station(view));
        }
    }

    fn collect(&mut self, id: u32, pos: BlockPos) {
        let canon = match self.station_access(id, pos) {
            Ok((canon, _)) => canon,
            Err(r) => return self.tell(id, r),
        };
        let Some(p) = self.players.get_mut(&id) else { return };
        let before = p.inventory.clone();
        let moved = self.stations.collect(canon, &mut p.inventory, &self.items);
        send_slots(p, &before);
        if moved {
            self.push_station(canon);
        } else if self
            .stations
            .get(canon)
            .is_some_and(|s| s.output.slots.iter().any(Option::is_some))
        {
            self.notice(id, "Your inventory is full");
        }
    }

    fn station_view(&self, canon: BlockPos) -> Option<bm_protocol::StationView> {
        let kind = match self.stations.get(canon) {
            Some(s) => s.kind,
            None => self
                .world
                .block(canon)
                .and_then(|b| self.world.registry.get(b).station)?,
        };
        Some(self.stations.view(canon, kind, self.tick, &self.recipes))
    }

    /// Sends a station's state to everyone watching it.
    fn push_station(&self, canon: BlockPos) {
        let mut view = None;
        for p in self.players.values().filter(|p| p.watching == Some(canon)) {
            if view.is_none() {
                view = self.station_view(canon);
            }
            if let Some(v) = &view {
                p.out.send(&ServerMsg::Station(v.clone()));
            }
        }
    }

    fn step_stations(&mut self) {
        for pos in self.stations.step(self.tick, &self.items, &self.recipes) {
            self.push_station(pos);
        }
        if self.tick % STATION_UPDATE_EVERY == 0 {
            let watched: BTreeSet<BlockPos> = self.players.values().filter_map(|p| p.watching).collect();
            let running: BTreeSet<BlockPos> = self.stations.running().collect();
            for pos in watched.intersection(&running) {
                self.push_station(*pos);
            }
        }
    }

    fn broadcast_claim(&self, claim: u32) {
        if let Some(c) = self.claims.get(claim) {
            let msg = ServerMsg::Claim(c.view());
            for p in self.players.values() {
                p.out.send(&msg);
            }
        }
    }

    fn claim_plot(&mut self, id: u32, claim: u32, plot: PlotPos) {
        let Some(account) = self.players.get(&id).map(|p| p.account.clone()) else {
            return;
        };
        match self.claims.add_plot(&account, claim, plot, self.cfg.plot_allowance) {
            Ok(()) => self.broadcast_claim(claim),
            Err(e) => self.notice(id, &e.to_string()),
        }
    }

    fn release_plot(&mut self, id: u32, plot: PlotPos) {
        let Some(account) = self.players.get(&id).map(|p| p.account.clone()) else {
            return;
        };
        match self.claims.release_plot(&account, plot) {
            Ok(claim) => self.broadcast_claim(claim),
            Err(e) => self.notice(id, &e.to_string()),
        }
    }

    /// Hands every dirty chunk, station and claim, every player's state and
    /// every newly departed player's to the writer, in one batch.
    fn save(&mut self, done: Option<Sender<()>>) {
        let items = &self.items;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let data = |state: PlayerState, inv: &Inventory| PlayerData {
            state: Some(state),
            inventory: save_slots(items, inv),
            last_seen: now,
        };
        let mut players: Vec<(String, PlayerData)> = self
            .players
            .values()
            .map(|p| (p.account.clone(), data(p.state, &p.inventory)))
            .collect();
        let mut done: Vec<Sender<()>> = done.into_iter().collect();
        let mut written = None;
        for (account, d) in self.departed.iter_mut().filter(|(_, d)| d.written.is_none()) {
            players.push((account.clone(), data(d.state, &d.inventory)));
            let rx = written.get_or_insert_with(|| {
                let (tx, rx) = crossbeam_channel::bounded(1);
                done.push(tx);
                rx
            });
            d.written = Some(rx.clone());
        }
        let batch = WriteBatch {
            chunks: self.world.take_dirty(),
            players,
            stations: self.stations.take_dirty(&self.items, &self.recipes),
            claims: self.claims.take_dirty(),
            clock: Some(self.tick),
            done,
        };
        if !batch.is_empty() || !batch.done.is_empty() {
            let _ = self.writer.send(batch);
        }
        // Departed players whose save is durable are read from disk again.
        self.departed.retain(|_, d| {
            d.written
                .as_ref()
                .is_none_or(|rx| matches!(rx.try_recv(), Err(TryRecvError::Empty)))
        });
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
