//! The authoritative simulation: one thread, a fixed 60 Hz tick, for every
//! planet of the universe (ADR-0006).
//!
//! Each tick: take network events, move finished chunk loads in, step every
//! player by the inputs they sent, advance station jobs, stream chunks, and
//! every third tick send snapshots. The loop owns all game state (worlds,
//! inventories, stations, claims); nothing else mutates it, so no locks are
//! needed and every rule is enforced in one place.
//!
//! Work follows players: a planet nobody is on holds no chunks (they are
//! saved and dropped as the last player leaves) and costs only its running
//! station jobs, which are timestamp driven. Nothing scans every planet's
//! chunks per tick.

use crate::claims::ClaimError;
use crate::config::ServerConfig;
use crate::crafting::{self, CraftError, MAX_CRAFT_COUNT};
use crate::net::{Event, Outbox};
use crate::planet::Planet;
use crate::store::{PlayerData, Store, WriteBatch, load_slots, save_slots};
use anyhow::{Context, Result};
use bm_core::items::title_case;
use bm_core::movement::{self, MoveConfig, PlayerInput, PlayerState};
use bm_core::{
    BlockId, BlockPos, CHUNK_SIZE, ChunkPos, HOTBAR_SLOTS, Inventory, ItemId, ItemRegistry, PlanetShape, PlotPos,
    RecipeBook, Station, TICK_DT,
};
use bm_net::quinn::Connection;
use bm_protocol::{
    ClientMsg, EditAction, PLAYERS_PER_SNAPSHOT, PlanetSummary, RemotePlayer, ServerDatagram, ServerMsg, Travel,
    Welcome,
};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use glam::DVec3;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
/// The planet list goes out at most this often when player counts change.
const UNIVERSE_EVERY: u64 = 60;
/// After travel, edits and break holds tagged with an input up to this many
/// past the last one the server had are refused: the client sent them about
/// the planet it left, before it heard of the travel.
const TRAVEL_GUARD_INPUTS: u32 = 30;

/// What waits in a player's queue for the input it follows.
#[derive(Clone, Copy, Debug)]
enum Queued {
    Edit {
        seq: u32,
        pos: BlockPos,
        action: EditAction,
    },
    StartBreak {
        pos: BlockPos,
        slot: u8,
    },
}

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
    /// Index into `Game::planets` (its id).
    planet: usize,
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
    /// Edits and break holds waiting for the input they follow, in the order
    /// sent (their `at` never decreases).
    edits: VecDeque<(u32, Queued)>,
    /// The station whose updates this player receives.
    watching: Option<BlockPos>,
    /// Edits and holds tagged with an input up to this one came from before
    /// the last travel (see `TRAVEL_GUARD_INPUTS`).
    travel_guard: Option<u32>,
}

/// A player who left, held in memory until the save holding their last state
/// is durable: a quick rejoin must not read an older copy from disk.
struct Departed {
    planet: usize,
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
    planets: Vec<Planet>,
    /// Where new players start.
    start: usize,
    items: ItemRegistry,
    recipes: RecipeBook,
    claim_stone: BlockId,
    warp_shrine: BlockId,
    /// Claim ids are unique across the universe.
    next_claim: u32,
    store: Store,
    writer: Sender<WriteBatch>,
    events: Receiver<Event>,
    players: HashMap<u32, Player>,
    departed: HashMap<String, Departed>,
    tick: u64,
    movement: MoveConfig,
    stats: Stats,
    /// Player counts changed since the planet list last went out.
    universe_dirty: bool,
    universe_sent: u64,
    load: Arc<Mutex<Vec<PlanetLoad>>>,
}

/// One planet's load, as the report shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanetLoad {
    pub name: String,
    pub players: usize,
    /// Chunks in memory.
    pub resident: usize,
    /// Chunk loads in flight.
    pub loading: usize,
    /// Evicted chunks kept until their save is durable.
    pub held: usize,
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
        planets: Vec<Planet>,
        start: usize,
        next_claim: u32,
        items: ItemRegistry,
        recipes: RecipeBook,
        store: Store,
        writer: Sender<WriteBatch>,
        events: Receiver<Event>,
    ) -> Result<Self> {
        let tick = store.clock().unwrap_or_else(|e| {
            tracing::error!("reading the universe clock: {e:#}; starting at 0");
            0
        });
        let registry = &planets.first().context("no planets")?.world.registry;
        let claim_stone = registry.id("claim_stone")?;
        let warp_shrine = registry.id("warp_shrine")?;
        Ok(Self {
            cfg,
            planets,
            start,
            items,
            recipes,
            claim_stone,
            warp_shrine,
            next_claim: next_claim.max(1),
            store,
            writer,
            events,
            players: HashMap::new(),
            departed: HashMap::new(),
            tick,
            movement: MoveConfig::default(),
            stats: Stats::default(),
            universe_dirty: false,
            universe_sent: 0,
            load: Arc::new(Mutex::new(Vec::new())),
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
        let planets: Vec<String> = self
            .planets
            .iter()
            .map(|p| {
                let held = p.world.held_for_saving();
                format!(
                    "{} {}p {}c{}{}",
                    p.name,
                    p.players,
                    p.world.resident(),
                    if p.world.pending_loads() > 0 {
                        format!("+{}l", p.world.pending_loads())
                    } else {
                        String::new()
                    },
                    if held > 0 { format!("+{held}s") } else { String::new() },
                )
            })
            .collect();
        tracing::info!(
            "tick {} | {} players | {} | sent {} chunks ({} KiB) | edits {} ok {} refused | {} slow ticks",
            self.tick,
            self.players.len(),
            planets.join(", "),
            s.chunks_sent,
            s.bytes_sent / 1024,
            s.edits,
            s.refused_edits,
            s.slow_ticks
        );
    }

    /// A handle on the per-planet load, refreshed every second.
    pub fn load_handle(&self) -> Arc<Mutex<Vec<PlanetLoad>>> {
        self.load.clone()
    }

    fn publish_load(&self) {
        let now = self
            .planets
            .iter()
            .map(|p| PlanetLoad {
                name: p.name.clone(),
                players: p.players,
                resident: p.world.resident(),
                loading: p.world.pending_loads(),
                held: p.world.held_for_saving(),
            })
            .collect();
        *self.load.lock().unwrap_or_else(|e| e.into_inner()) = now;
    }

    /// One tick. Public so tests can drive the loop without real time.
    pub fn step(&mut self) {
        self.tick += 1;
        while let Ok(ev) = self.events.try_recv() {
            self.handle(ev);
        }
        for p in &mut self.planets {
            p.world.receive_loaded(self.tick, p.players > 0);
        }
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
            let before = self.tick.saturating_sub(EVICT_EVERY);
            self.evict(None, before);
        }
        if self.tick % 60 == 0 {
            self.publish_load();
        }
        if self.universe_dirty && self.tick >= self.universe_sent + UNIVERSE_EVERY {
            self.send_universe();
        }
    }

    /// Evicts chunks unwanted since `before`, on one planet or all, saving
    /// the dirty ones.
    fn evict(&mut self, planet: Option<usize>, before: u64) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let mut chunks = Vec::new();
        for (i, p) in self.planets.iter_mut().enumerate() {
            if planet.is_none_or(|only| only == i) {
                let id = p.id;
                chunks.extend(
                    p.world
                        .evict_unwanted(before, &rx)
                        .into_iter()
                        .map(|(c, bytes)| (id, c, bytes)),
                );
            }
        }
        if !chunks.is_empty() {
            let _ = self.writer.send(WriteBatch {
                chunks,
                done: vec![tx],
                ..Default::default()
            });
        }
    }

    fn universe(&self) -> Vec<PlanetSummary> {
        self.planets.iter().map(Planet::summary).collect()
    }

    fn send_universe(&mut self) {
        let msg = ServerMsg::Universe {
            planets: self.universe(),
        };
        for p in self.players.values() {
            p.out.send(&msg);
        }
        self.universe_dirty = false;
        self.universe_sent = self.tick;
    }

    /// Someone arrived on or left a planet.
    fn counts_changed(&mut self) {
        self.universe_dirty = true;
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
                ClientMsg::Edit { seq, at, pos, action } => self.queue(id, at, Queued::Edit { seq, pos, action }),
                ClientMsg::StartBreak { at, pos, slot } => self.queue(id, at, Queued::StartBreak { pos, slot }),
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
                ClientMsg::Warp { shrine, planet } => self.warp(id, shrine, planet),
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
            out.close("this server is full");
            return;
        }
        let (planet, saved, mut inventory) = match self.departed.remove(&account) {
            Some(d) => (d.planet, Some(d.state), d.inventory),
            None => match self.store.player(&account) {
                Ok(Some(data)) => (
                    data.planet as usize,
                    data.state,
                    Inventory {
                        slots: load_slots(&self.items, &data.inventory, &name),
                    },
                ),
                Ok(None) => (self.start, None, Inventory::default()),
                Err(e) => {
                    // Never start them afresh: the empty inventory would be
                    // saved over the real one.
                    tracing::error!("loading {name}: {e:#}");
                    out.close("your character could not be loaded; try again in a moment");
                    return;
                }
            },
        };
        let (planet, saved) = if planet < self.planets.len() {
            (planet, saved)
        } else {
            tracing::warn!("{name} was on planet {planet}, which this server does not host; they start again");
            (self.start, None)
        };
        sanitize(&mut inventory, &self.items);
        let pl = &self.planets[planet];
        let shape = pl.world.shape;
        let state = saved
            .filter(|s| s.pos.is_finite() && s.vel.is_finite() && shape.contains_y(s.pos.y as i64))
            .map(|s| PlayerState {
                pos: shape.wrap_point(s.pos),
                ..s
            })
            .unwrap_or_else(|| pl.arrival());
        out.send(&ServerMsg::Welcome(Box::new(Welcome {
            player_id: id,
            account: name.clone(),
            planet: pl.info(),
            universe: self.universe(),
            blocks: pl.world.registry.defs().to_vec(),
            items: self.items.defs().to_vec(),
            recipes: self.recipes.recipes.clone(),
            inventory: inventory.clone(),
            movement: self.movement,
            spawn: state,
            tick: self.tick,
            view_radius: self.cfg.view_radius,
            reach: self.cfg.reach,
            claims: pl.claims.views(),
            plot_allowance: self.cfg.plot_allowance,
        })));
        for (oid, other) in self.players.iter().filter(|(_, o)| o.planet == planet) {
            out.send(&ServerMsg::PlayerJoined {
                id: *oid,
                name: other.name.clone(),
            });
            other.out.send(&ServerMsg::PlayerJoined { id, name: name.clone() });
        }
        if let Some(text) = shrine_hint(shape, &pl.name, pl.shrine, state.pos) {
            out.send(&ServerMsg::Notice { text });
        }
        tracing::info!("{name} (#{id}) joined on {} at {:.1?}", pl.name, state.pos);
        self.planets[planet].players += 1;
        self.counts_changed();
        self.players.insert(
            id,
            Player {
                account,
                planet,
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
                travel_guard: None,
            },
        );
    }

    fn leave(&mut self, id: u32) {
        let Some(p) = self.players.remove(&id) else { return };
        tracing::info!("{} (#{id}) left", p.name);
        let planet = p.planet;
        self.departed.insert(
            p.account,
            Departed {
                planet,
                state: p.state,
                inventory: p.inventory,
                written: None,
            },
        );
        // Everything, not just the player: what they gathered and built
        // must reach the disk in the same transaction as their inventory.
        self.save(None);
        for other in self.players.values().filter(|o| o.planet == planet) {
            other.out.send(&ServerMsg::PlayerLeft { id });
        }
        self.left_planet(planet);
    }

    /// A player has gone from `planet` (left the server or travelled).
    fn left_planet(&mut self, planet: usize) {
        let pl = &mut self.planets[planet];
        pl.players -= 1;
        if pl.players == 0 {
            // Nobody is there: it holds no chunks. Ones whose save is still
            // in flight are kept until it is durable.
            self.evict(Some(planet), u64::MAX);
        }
        self.counts_changed();
    }

    /// Applies each player's inputs in sequence order, one per tick of real
    /// time on average (ADR-0004).
    fn simulate(&mut self) {
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
                let world = &self.planets[p.planet].world;
                movement::step(&mut p.state, &input, &self.movement, world.shape, world);
                p.last_seq += 1;
                p.last_input = input;
                p.bank -= 1.0;
            }
        }
    }

    /// Applies an edit or break hold right after the input it follows, in
    /// the order the client sent them: a hold on the next block must never
    /// overtake the break of the one before it.
    fn queue(&mut self, id: u32, at: u32, queued: Queued) {
        let Some(p) = self.players.get_mut(&id) else { return };
        if p.travel_guard.is_some_and(|g| at <= g) {
            // Sent about the planet they left.
            if let Queued::Edit { seq, pos, .. } = queued {
                self.edit_refused(id, seq, pos, "edit from before travelling");
            }
        } else if at <= p.last_seq && p.edits.is_empty() {
            // Late (its input was already simulated, e.g. a lost input was
            // repeated): apply now.
            self.apply_queued(id, queued);
        } else if at > p.last_seq + MAX_EDIT_LEAD || p.edits.len() >= MAX_QUEUED_EDITS {
            if let Queued::Edit { seq, pos, .. } = queued {
                self.edit_refused(id, seq, pos, "edit too far ahead of movement");
            }
        } else {
            p.edits.push_back((at, queued));
        }
    }

    fn apply_queued(&mut self, id: u32, queued: Queued) {
        match queued {
            Queued::Edit { seq, pos, action } => self.edit(id, seq, pos, action),
            Queued::StartBreak { pos, slot } => self.start_break(id, pos, slot),
        }
    }

    fn apply_due_edits(&mut self, id: u32) {
        loop {
            let Some(p) = self.players.get_mut(&id) else { return };
            match p.edits.front() {
                Some(&(at, _)) if at <= p.last_seq => {
                    let (_, queued) = p.edits.pop_front().expect("present");
                    self.apply_queued(id, queued);
                }
                _ => return,
            }
        }
    }

    /// Every chunk of every column within the view radius (M2: planets are
    /// bounded in height, so whole columns are sent and sky light is exact
    /// from the top down). Nearest columns first; within a column, the
    /// chunks nearest the player's height first.
    fn view_list(&self, shape: PlanetShape, center: ChunkPos, eye_cy: i32) -> Vec<ChunkPos> {
        let r = self.cfg.view_radius as i32;
        let mut list = Vec::new();
        for dz in -r..=r {
            for dx in -r..=r {
                let d2 = dx * dx + dz * dz;
                if d2 > r * r {
                    continue;
                }
                for y in 0..shape.height_chunks as i32 {
                    let key = d2 * 16 + (y - eye_cy).abs();
                    list.push((key, shape.wrap_chunk(ChunkPos::new(center.x + dx, y, center.z + dz))));
                }
            }
        }
        list.sort_by_key(|(d, _)| *d);
        list.into_iter().map(|(_, p)| p).collect()
    }

    fn stream(&mut self, id: u32) {
        let Some(p) = self.players.get(&id) else { return };
        let pi = p.planet;
        let shape = self.planets[pi].world.shape;
        let at = shape.wrap_chunk(BlockPos::containing(p.state.pos.x, p.state.pos.y, p.state.pos.z).chunk());
        // Only the column matters: whole columns are streamed.
        let here = ChunkPos::new(at.x, 0, at.z);
        if p.center != Some(here) {
            let wanted = self.view_list(shape, here, at.y);
            let r = self.cfg.view_radius as i32 + 2;
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

        // Spread over the second, a slice of the players each tick, so no
        // tick marks everyone's view at once.
        let mark = (self.tick + id as u64) % MARK_WANTED_EVERY == 0;
        let p = self.players.get_mut(&id).expect("present");
        let world = &mut self.planets[pi].world;
        if mark {
            for c in &p.wanted {
                if let Some(slot) = world.get_mut(*c) {
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
            match world.get_mut(c) {
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
                    if world.pending_loads() < MAX_PENDING_LOADS {
                        world.request(c, self.tick);
                    }
                }
            }
        }
    }

    fn snapshots(&self) {
        let range = (self.cfg.view_radius as f64 + 1.0) * CHUNK_SIZE as f64;
        let mut by_planet: Vec<Vec<(u32, &Player)>> = vec![Vec::new(); self.planets.len()];
        for (id, p) in &self.players {
            by_planet[p.planet].push((*id, p));
        }
        for (pi, here) in by_planet.iter().enumerate() {
            let shape = self.planets[pi].world.shape;
            for (id, p) in here {
                let mut near: Vec<(f64, RemotePlayer)> = here
                    .iter()
                    .filter(|(oid, _)| oid != id)
                    .filter_map(|(oid, o)| {
                        let d = shape.delta(p.state.pos, o.state.pos);
                        let dist2 = d.x * d.x + d.z * d.z;
                        (dist2 <= range * range).then_some((
                            dist2,
                            RemotePlayer {
                                id: *oid,
                                pos: [o.state.pos.x as f32, o.state.pos.y as f32, o.state.pos.z as f32],
                                yaw: o.last_input.yaw,
                                pitch: o.last_input.pitch,
                            },
                        ))
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
    }

    fn send_snapshot(&self, p: &Player, others: Vec<RemotePlayer>) {
        bm_net::send_datagram(
            &p.conn,
            &ServerDatagram::Snapshot {
                planet: self.planets[p.planet].id,
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

    /// The planet a player is on.
    fn planet_of(&self, id: u32) -> Option<usize> {
        self.players.get(&id).map(|p| p.planet)
    }

    fn start_break(&mut self, id: u32, pos: BlockPos, slot: u8) {
        let tick = self.tick;
        let Some(p) = self.players.get_mut(&id) else { return };
        let canon = self.planets[p.planet].world.shape.wrap_block(pos);
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

    /// Plots `account` may still claim on planet `pi`: the universe-wide
    /// allowance less what they hold on every other planet.
    fn allowance_on(&self, account: &str, pi: usize) -> u32 {
        let elsewhere: u32 = self
            .planets
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != pi)
            .map(|(_, p)| p.claims.used_by(account))
            .sum();
        self.cfg.plot_allowance.saturating_sub(elsewhere)
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
        let Some(pi) = self.planet_of(id) else { return };
        let allowance = self.players.get(&id).map_or(0, |p| self.allowance_on(&p.account, pi));
        let planet = &mut self.planets[pi];
        let canon = planet.world.shape.wrap_block(pos);
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
                    claim_removed = planet.claims.remove_by_stone(canon);
                }
            }
            Effect::Placed { slot } => {
                p.inventory.take_one(slot);
                if new == self.claim_stone {
                    match planet
                        .claims
                        .create(self.next_claim, &p.account, &p.name, canon, allowance)
                    {
                        Ok(claim) => {
                            self.next_claim += 1;
                            claim_created = Some(claim);
                        }
                        Err(e) => tracing::error!("claim checked but not created: {e}"),
                    }
                }
            }
        }
        send_slots(p, &before);
        if let Some(text) = broke {
            p.out.send(&ServerMsg::Notice { text });
        }
        planet.world.set_block(pos, new);
        self.stats.edits += 1;
        let chunk = canon.chunk();
        for (oid, o) in &self.players {
            if *oid != id && o.planet == pi && o.sent.contains(&chunk) {
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
            self.broadcast_claim(pi, claim);
        }
        if let Some(claim) = claim_removed {
            for o in self.players.values().filter(|o| o.planet == pi) {
                o.out.send(&ServerMsg::ClaimRemoved { id: claim });
            }
        }
    }

    fn edit_refused(&mut self, id: u32, seq: u32, pos: BlockPos, why: &str) {
        self.stats.refused_edits += 1;
        tracing::debug!("refused edit from #{id} at {pos:?}: {why}");
        let Some(p) = self.players.get(&id) else { return };
        // What the block is on the planet they are on now.
        let actual = self.planets[p.planet].world.block(pos).unwrap_or(BlockId::AIR);
        p.out.send(&ServerMsg::EditResult {
            seq,
            pos,
            block: actual,
            accepted: false,
        });
    }

    /// Every rule an edit must pass. Changes nothing but the rate tokens and
    /// the break hold; `edit` applies what it returns.
    fn check_edit(&mut self, id: u32, pos: BlockPos, action: EditAction) -> Result<Accepted, Refusal> {
        let pi = self.planet_of(id).ok_or("no such player")?;
        let allowance = {
            let account = &self.players[&id].account;
            self.allowance_on(account, pi)
        };
        let planet = &self.planets[pi];
        let world = &planet.world;
        let shape = world.shape;
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
        let current = world.block(canon).ok_or("chunk not resident")?;
        if planet.guarded(canon) {
            return Err(Refusal::Told(
                "The ground around a warp shrine cannot be changed".into(),
            ));
        }
        if let Some(c) = planet.claims.at_block(canon)
            && c.owner != p.account
        {
            return Err(ClaimError::OwnedBy(c.owner_name.clone()).into());
        }
        let reg = &world.registry;
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
                if planet.stations.has_contents(canon) {
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
                if !(current.is_air() || current == world.water) {
                    return Err("space is occupied".into());
                }
                let supported = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                    .iter()
                    .any(|&(dx, dy, dz)| {
                        world
                            .block(canon.offset(dx, dy, dz))
                            .is_some_and(|b| world.flags.solid(b))
                    });
                if !supported {
                    return Err("nothing to place against".into());
                }
                if block == self.claim_stone {
                    planet.claims.can_create(&p.account, canon, allowance)?;
                }
                if reg.get(block).solid {
                    let m = self.movement;
                    let centre = DVec3::new(canon.x as f64 + 0.5, canon.y as f64 + 0.5, canon.z as f64 + 0.5);
                    let blocked = self.players.values().filter(|o| o.planet == pi).any(|o| {
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

    /// Travel from a warp shrine within reach to another planet.
    fn warp(&mut self, id: u32, shrine: BlockPos, dest: u16) {
        let Some(p) = self.players.get(&id) else { return };
        let from = &self.planets[p.planet];
        let shape = from.world.shape;
        let at_shrine = shape.contains_y(shrine.y) && {
            let canon = shape.wrap_block(shrine);
            p.sent.contains(&canon.chunk())
                && from.world.block(canon) == Some(self.warp_shrine)
                && in_reach(shape, &self.movement, self.cfg.reach, &p.state, canon)
        };
        if !at_shrine {
            return self.notice(id, "Stand at a warp shrine to travel");
        }
        let Some(to) = self.planets.iter().position(|pl| pl.id == dest) else {
            return self.tell(id, "no such planet".into());
        };
        if to == p.planet {
            let text = format!("You are already on {}", from.name);
            return self.notice(id, &text);
        }
        self.travel(id, to);
    }

    /// Moves a player to planet `to`, standing beside its shrine. The client
    /// drops everything of the old planet on `Travel`, so nothing of it is
    /// forgotten one by one; everything after `Travel` on the stream belongs
    /// to the new planet.
    fn travel(&mut self, id: u32, to: usize) {
        let arrival = self.planets[to].arrival();
        let Some(p) = self.players.get_mut(&id) else { return };
        let from = p.planet;
        p.planet = to;
        p.state = arrival;
        // Inputs and edits sent about the old planet are not replayed on the
        // new one; inputs carry on from the newest seen, standing still.
        let newest = p
            .pending
            .last_key_value()
            .map_or(p.last_seq, |(s, _)| *s)
            .max(p.last_seq);
        p.pending.clear();
        p.last_seq = newest;
        p.last_input = PlayerInput {
            yaw: p.last_input.yaw,
            pitch: p.last_input.pitch,
            ..Default::default()
        };
        p.bank = 0.0;
        p.travel_guard = Some(newest.saturating_add(TRAVEL_GUARD_INPUTS));
        p.edits.clear();
        p.mining = None;
        p.watching = None;
        p.sent.clear();
        p.wanted.clear();
        p.center = None;
        let name = p.name.clone();
        let planet = &self.planets[to];
        p.out.send(&ServerMsg::Travel(Box::new(Travel {
            planet: planet.info(),
            spawn: arrival,
            tick: self.tick,
            claims: planet.claims.views(),
        })));
        tracing::info!(
            "{name} (#{id}) travelled from {} to {}",
            self.planets[from].name,
            planet.name
        );
        let traveller = &self.players[&id];
        for (oid, o) in &self.players {
            if *oid == id {
                continue;
            }
            if o.planet == to {
                traveller.out.send(&ServerMsg::PlayerJoined {
                    id: *oid,
                    name: o.name.clone(),
                });
                o.out.send(&ServerMsg::PlayerJoined { id, name: name.clone() });
            } else if o.planet == from {
                o.out.send(&ServerMsg::PlayerLeft { id });
            }
        }
        self.planets[to].players += 1;
        self.left_planet(from);
    }

    /// The station at `pos` if this player may use it: loaded for them,
    /// within reach, and not on someone else's claim.
    fn station_access(&self, id: u32, pos: BlockPos) -> Result<(BlockPos, Station), Refusal> {
        let p = self.players.get(&id).ok_or("no such player")?;
        let planet = &self.planets[p.planet];
        let shape = planet.world.shape;
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
        let block = planet.world.block(canon).ok_or("chunk not resident")?;
        let kind = planet.world.registry.get(block).station.ok_or("not a station")?;
        if let Some(c) = planet.claims.at_block(canon)
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
        let Some(pi) = self.planet_of(id) else { return };
        if !self.planets[pi].stations.has_room(canon) {
            return self.notice(id, &CraftError::QueueFull.to_string());
        }
        match self.change_inventory(id, |inv, _| crafting::take_inputs(inv, &recipe, count)) {
            Some(true) => {}
            Some(false) => return self.notice(id, &CraftError::Materials.to_string()),
            None => return,
        }
        if let Err(e) = self.planets[pi]
            .stations
            .queue(canon, recipe.station, index, count, self.tick)
        {
            // Unreachable (room was checked), but never take inputs silently.
            tracing::error!("queueing at {canon:?} after taking inputs: {e}");
        }
        self.push_station(pi, canon);
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
        let Some(pi) = self.planet_of(id) else { return };
        let view = target.and_then(|canon| self.station_view(pi, canon));
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
        let pi = p.planet;
        let stations = &mut self.planets[pi].stations;
        let before = p.inventory.clone();
        let moved = stations.collect(canon, &mut p.inventory, &self.items);
        send_slots(p, &before);
        if moved {
            self.push_station(pi, canon);
        } else if stations
            .get(canon)
            .is_some_and(|s| s.output.slots.iter().any(Option::is_some))
        {
            self.notice(id, "Your inventory is full");
        }
    }

    fn station_view(&self, pi: usize, canon: BlockPos) -> Option<bm_protocol::StationView> {
        let planet = &self.planets[pi];
        let kind = match planet.stations.get(canon) {
            Some(s) => s.kind,
            None => planet
                .world
                .block(canon)
                .and_then(|b| planet.world.registry.get(b).station)?,
        };
        Some(planet.stations.view(canon, kind, self.tick, &self.recipes))
    }

    /// Sends a station's state to everyone on its planet watching it.
    fn push_station(&self, pi: usize, canon: BlockPos) {
        let mut view = None;
        for p in self
            .players
            .values()
            .filter(|p| p.planet == pi && p.watching == Some(canon))
        {
            if view.is_none() {
                view = self.station_view(pi, canon);
            }
            if let Some(v) = &view {
                p.out.send(&ServerMsg::Station(v.clone()));
            }
        }
    }

    /// Station jobs run on every planet, empty or not: they only look at the
    /// stations with a job running.
    fn step_stations(&mut self) {
        for pi in 0..self.planets.len() {
            let changed = self.planets[pi].stations.step(self.tick, &self.items, &self.recipes);
            for pos in changed {
                self.push_station(pi, pos);
            }
        }
        if self.tick % STATION_UPDATE_EVERY == 0 {
            let watched: BTreeSet<(usize, BlockPos)> = self
                .players
                .values()
                .filter_map(|p| p.watching.map(|w| (p.planet, w)))
                .collect();
            for (pi, pos) in watched {
                if self.planets[pi].stations.running().any(|r| r == pos) {
                    self.push_station(pi, pos);
                }
            }
        }
    }

    fn broadcast_claim(&self, pi: usize, claim: u32) {
        if let Some(c) = self.planets[pi].claims.get(claim) {
            let msg = ServerMsg::Claim(c.view());
            for p in self.players.values().filter(|p| p.planet == pi) {
                p.out.send(&msg);
            }
        }
    }

    fn claim_plot(&mut self, id: u32, claim: u32, plot: PlotPos) {
        let Some((account, pi)) = self.players.get(&id).map(|p| (p.account.clone(), p.planet)) else {
            return;
        };
        let allowance = self.allowance_on(&account, pi);
        match self.planets[pi].claims.add_plot(&account, claim, plot, allowance) {
            Ok(()) => self.broadcast_claim(pi, claim),
            Err(e) => self.notice(id, &e.to_string()),
        }
    }

    fn release_plot(&mut self, id: u32, plot: PlotPos) {
        let Some((account, pi)) = self.players.get(&id).map(|p| (p.account.clone(), p.planet)) else {
            return;
        };
        match self.planets[pi].claims.release_plot(&account, plot) {
            Ok(claim) => self.broadcast_claim(pi, claim),
            Err(e) => self.notice(id, &e.to_string()),
        }
    }

    /// Hands every planet's dirty chunks, stations and claims, every
    /// player's state and every newly departed player's to the writer, in one
    /// batch.
    fn save(&mut self, done: Option<Sender<()>>) {
        let items = &self.items;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let planets = &self.planets;
        let data = |planet: usize, state: PlayerState, inv: &Inventory| PlayerData {
            planet: planets[planet].id,
            state: Some(state),
            inventory: save_slots(items, inv),
            last_seen: now,
        };
        let mut players: Vec<(String, PlayerData)> = self
            .players
            .values()
            .map(|p| (p.account.clone(), data(p.planet, p.state, &p.inventory)))
            .collect();
        let mut done: Vec<Sender<()>> = done.into_iter().collect();
        let mut written = None;
        for (account, d) in self.departed.iter_mut().filter(|(_, d)| d.written.is_none()) {
            players.push((account.clone(), data(d.planet, d.state, &d.inventory)));
            let rx = written.get_or_insert_with(|| {
                let (tx, rx) = crossbeam_channel::bounded(1);
                done.push(tx);
                rx
            });
            d.written = Some(rx.clone());
        }
        // Chunks saved here stay resident; if they are evicted before this
        // batch is durable, their world keeps them until it is.
        let (tx, saved) = crossbeam_channel::bounded(1);
        done.push(tx);
        let mut chunks = Vec::new();
        let mut stations = Vec::new();
        let mut claims = Vec::new();
        for p in &mut self.planets {
            p.world.forget_saved();
            let id = p.id;
            chunks.extend(p.world.take_dirty(&saved).into_iter().map(|(c, b)| (id, c, b)));
            stations.extend(
                p.stations
                    .take_dirty(&self.items, &self.recipes)
                    .into_iter()
                    .map(|(pos, s)| (id, pos, s)),
            );
            claims.extend(p.claims.take_dirty());
        }
        let batch = WriteBatch {
            chunks,
            players,
            stations,
            claims,
            clock: Some(self.tick),
            done,
        };
        if !batch.is_empty() || batch.done.len() > 1 {
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

/// Where the planet's warp shrine is, for a player who arrives far from it
/// (characters kept from before shrines existed stand wherever they left).
/// North is -Z.
fn shrine_hint(shape: bm_core::PlanetShape, planet: &str, shrine: BlockPos, from: DVec3) -> Option<String> {
    let to = DVec3::new(shrine.x as f64 + 0.5, shrine.y as f64, shrine.z as f64 + 0.5);
    let d = shape.delta(from, to);
    let dist = (d.x * d.x + d.z * d.z).sqrt();
    if dist < 48.0 {
        return None;
    }
    // Eight compass points, from the angle east of north.
    let names = [
        "north",
        "north-east",
        "east",
        "south-east",
        "south",
        "south-west",
        "west",
        "north-west",
    ];
    let angle = d.x.atan2(-d.z).to_degrees().rem_euclid(360.0);
    let dir = names[((angle + 22.5) / 45.0) as usize % 8];
    Some(format!(
        "The warp shrine of {planet} is {} blocks {dir} of you.",
        dist.round() as i64
    ))
}

#[cfg(test)]
mod shrine_hint_tests {
    use super::*;

    #[test]
    fn far_players_are_told_where_the_shrine_is() {
        let shape = bm_core::PlanetShape::new(64, 12);
        let shrine = BlockPos::new(100, 150, 100);
        assert_eq!(
            shrine_hint(shape, "Verdance", shrine, DVec3::new(110.0, 150.0, 110.0)),
            None
        );
        let east = shrine_hint(shape, "Verdance", shrine, DVec3::new(0.5, 150.0, 100.5)).unwrap();
        assert_eq!(east, "The warp shrine of Verdance is 100 blocks east of you.");
        // Across the seam: from x 2000 on a 2048-block planet, x 100 is 148 east.
        let seam = shrine_hint(shape, "Verdance", shrine, DVec3::new(2000.5, 150.0, 300.5)).unwrap();
        assert!(seam.ends_with("blocks north-east of you."), "{seam}");
    }
}
