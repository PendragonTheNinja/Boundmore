//! The authoritative simulation: one thread, a fixed 60 Hz tick, for every
//! planet of the universe (ADR-0006).
//!
//! Each tick: take network events, move finished chunk loads in, step every
//! player by the inputs they sent, advance station jobs, stream chunks, and
//! every third tick send snapshots. The loop owns all game state (worlds,
//! inventories, stations, storage, claims, chat); nothing else mutates it, so
//! no locks are needed and every rule is enforced in one place.
//!
//! Work follows players: a planet nobody is on holds no chunks (they are
//! saved and dropped as the last player leaves) and costs only its running
//! station jobs, which are timestamp driven. Nothing scans every planet's
//! chunks per tick.

use crate::chat::{self, ChatLimit, Verdict};
use crate::claims::ClaimError;
use crate::config::ServerConfig;
use crate::damage::DamageMap;
use crate::lapse::{self, Lapse};
use crate::net::{Event, Outbox};
use crate::planet::Planet;
use crate::portals::{PortalRules, Portals};
use crate::reclaim::Reclaims;
use crate::storage::move_stack;
use crate::store::{PlayerData, SavedFood, Store, WriteBatch, load_slots, save_slots};

mod blasts;
mod combat;
mod creative;
mod creatures;
mod doors;
mod exo;
mod farm;
mod food;
mod forge;
mod meteors;
mod mining;
mod portal;
mod skills;
mod stations;
use anyhow::{Context, Result};
use bm_core::creatures::CreatureKinds;
use bm_core::forge::Held;
use bm_core::movement::{self, MoveConfig, PlayerInput, PlayerState};
use bm_core::skills::{Character, Effects, SkillTree};
use bm_core::{
    BlockId, BlockPos, BlockState, CHUNK_SIZE, ChunkPos, Colour, ColourPalette, HOTBAR_SLOTS, Inventory, ItemId,
    ItemRegistry, ItemStack, Mount, PlanetShape, PlotPos, RecipeBook, Shape, Station, TICK_DT, ToolStats, chisel,
};
use bm_net::quinn::Connection;
use bm_protocol::{
    CREATURES_PER_SNAPSHOT, Channel, ChatLine, ClientMsg, CreatureView, EditAction, LOCAL_CHAT_RANGE, MAX_NAME_LEN,
    PLAYERS_PER_SNAPSHOT, PlanetSummary, RemoteCreature, RemotePlayer, ServerDatagram, ServerMsg, SkillAction, SlotRef,
    StorageView, Travel, Welcome,
};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use glam::DVec3;
use mining::holds_props;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The nearest players each player is told about (several snapshot datagrams).
const MAX_SEEN_PLAYERS: usize = 160;
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
/// A far column's offset from the player's (dx, dz), its squared
/// distance, and its level.
type FarOffset = (i32, i32, i32, u8);

/// Concurrent chunk loads across the whole server.
const MAX_PENDING_LOADS: usize = 512;
/// Far copies (milestone 20) sent per player per tick, after the near
/// chunks: about 1 MB a second.
const FAR_BYTES_PER_TICK: usize = 16 << 10;
/// Each player's far columns are looked over every this many ticks
/// (staggered), with that many ticks' budget.
const FAR_EVERY: u64 = 3;
const FAR_SCAN: usize = 256;
/// Far jobs in flight across the server while chunk loads wait (they come
/// first); with none waiting, one per loader thread.
const FAR_JOBS_WHILE_LOADING: usize = 1;
/// A planet with nobody on it drops its far copies from memory after this.
const FAR_IDLE_TICKS: u64 = 60 * 300;
/// Edits per second, and the burst allowed.
const EDIT_RATE: f64 = 12.0;
const EDIT_BURST: f64 = 24.0;
/// How far ahead of the simulated inputs an edit may be tagged (2 s).
const MAX_EDIT_LEAD: u32 = 120;
const MAX_QUEUED_EDITS: usize = 64;
/// The planet list goes out at most this often when player counts change.
const UNIVERSE_EVERY: u64 = 60;
/// How often open storage blocks are checked still to be in reach (4 Hz).
const STORAGE_CHECK_EVERY: u64 = 15;
/// After travel, edits and swings tagged with an input up to this many
/// past the last one the server had are refused: the client sent them about
/// the planet it left, before it heard of the travel.
const TRAVEL_GUARD_INPUTS: u32 = 30;
/// How often claims are checked for lapsing (once a second: a pass over the
/// claims, not the chunks).
const LAPSE_CHECK_EVERY: u64 = 60;
/// A lapsing claim turns one plot this often (four a second, across every
/// lapse), so nothing stalls however large the claim.
const LAPSE_PLOT_EVERY: u64 = 15;

/// What waits in a player's queue for the input it follows.
#[derive(Clone, Debug)]
enum Queued {
    Creative {
        at: u32,
        on: bool,
    },
    Edit {
        seq: u32,
        pos: BlockPos,
        action: EditAction,
    },
    Swing {
        seq: u32,
        at: u32,
        pos: BlockPos,
        slot: u8,
    },
    /// A change to the character's skills (milestone 12): what the page
    /// does to the body counts from the input it follows.
    Skill {
        at: u32,
        action: SkillAction,
    },
    /// A sling shot (milestone 13).
    Shoot {
        at: u32,
        slot: u8,
        yaw: u16,
        pitch: i16,
        charge: u16,
    },
    /// A tool swung at a creature (milestone 13).
    Strike {
        at: u32,
        creature: u32,
        slot: u8,
    },
    /// A bomb thrown (milestone 13 part 2).
    Throw {
        at: u32,
        slot: u8,
        yaw: u16,
        pitch: i16,
        hold: u16,
    },
    /// Food eaten (milestone 16): it changes the body from its input on.
    Eat {
        at: u32,
        slot: u8,
    },
}

impl Queued {
    /// The answer to an edit or swing refused before it was looked at (None
    /// for a skill change or an attack).
    fn seq_pos(&self) -> Option<(u32, BlockPos)> {
        match *self {
            Queued::Edit { seq, pos, .. } | Queued::Swing { seq, pos, .. } => Some((seq, pos)),
            Queued::Skill { .. }
            | Queued::Creative { .. }
            | Queued::Shoot { .. }
            | Queued::Strike { .. }
            | Queued::Throw { .. }
            | Queued::Eat { .. } => None,
        }
    }

    /// An attack: about the planet it was sent on, refused quietly.
    fn attack(&self) -> bool {
        matches!(
            self,
            Queued::Shoot { .. } | Queued::Strike { .. } | Queued::Throw { .. } | Queued::Eat { .. }
        )
    }
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
    /// Far copies sent (milestone 20) by column, each chunk's level from
    /// the bottom up (0: not sent).
    far_sent: HashMap<(i32, i32), Box<[u8]>>,
    far_center: Option<ChunkPos>,
    /// Every near chunk wanted has been sent (`stream`).
    near_complete: bool,
    /// Every column of the far offsets before this one has been sent.
    far_cursor: usize,
    edit_tokens: f64,
    /// The last accepted swing: the input it followed and the ticks its
    /// tool needs before the next (`mining::swing_ticks`).
    last_swing: Option<(u32, u32)>,
    /// Edits and swings waiting for the input they follow, in the order
    /// sent (their `at` never decreases).
    edits: VecDeque<(u32, Queued)>,
    /// The station whose updates this player receives.
    watching: Option<BlockPos>,
    /// The storage block this player has open (canonical).
    storage: Option<BlockPos>,
    chat: ChatLimit,
    /// Edits and swings tagged with an input up to this one came from before
    /// the last travel (see `TRAVEL_GUARD_INPUTS`).
    travel_guard: Option<u32>,
    /// The portal whose window this player has open.
    portal: Option<u32>,
    /// Set on coming out of a portal (or being refused at one): no portal
    /// takes them until they have stepped off every surface.
    portal_guard: bool,
    /// A portal trip waiting for the far side's chunks.
    trip: Option<portal::Trip>,
    /// Portal tokens whose names this player has been sent.
    told_tokens: HashSet<u32>,
    /// Progression (milestone 12), saved with the account.
    character: Character,
    /// What the active page does (kept in step with `character`).
    effects: Effects,
    /// XP changed since the character was last sent (sent at most every
    /// `skills::CHARACTER_EVERY` ticks).
    character_dirty: bool,
    /// The light-source epic shines now (not in daylight under open sky).
    inner_light: bool,
    /// Creatures this player has been told of (milestone 13), as last told.
    known: HashMap<u32, CreatureView>,
    /// The creatures the next snapshots carry, nearest first.
    in_view: Vec<RemoteCreature>,
    /// Scratch for finding them (kept to spare an allocation a snapshot).
    near_creatures: Vec<(f64, usize)>,
    /// Their defeat has been dealt with (debt, `Defeated`); cleared on
    /// returning.
    defeated: bool,
    /// The region and meteor square they are in, and whether they are near
    /// the region's centre (milestone 14), as last seen.
    region: Option<usize>,
    square: Option<(i64, i64)>,
    at_centre: bool,
}

/// A player who left, held in memory until the save holding their last state
/// is durable: a quick rejoin must not read an older copy from disk.
struct Departed {
    planet: usize,
    state: PlayerState,
    inventory: Inventory,
    character: Character,
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

impl From<chisel::Refused> for Refusal {
    fn from(r: chisel::Refused) -> Self {
        match r {
            chisel::Refused::Quiet(why) => Refusal::Quiet(why),
            chisel::Refused::Told(t) => Refusal::Told(t),
        }
    }
}

impl From<ClaimError> for Refusal {
    fn from(e: ClaimError) -> Self {
        Refusal::Told(e.to_string())
    }
}

/// An accepted edit: the block to write and what else it does.
struct Accepted {
    block: BlockState,
    effect: Effect,
}

enum Effect {
    Placed {
        slot: usize,
    },
    /// Cut or filled back with the chisel in this hotbar slot, which wears.
    Chiselled {
        slot: usize,
    },
}

pub struct Game {
    cfg: ServerConfig,
    planets: Vec<Planet>,
    /// Where new players start.
    start: usize,
    items: ItemRegistry,
    /// The colour palette sent in every welcome (ADR-0007).
    colours: ColourPalette,
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
    /// Far columns around a player (milestone 20) as offsets with their
    /// level, nearest first, by planet width in chunks; built once.
    far_offsets: HashMap<u32, Arc<Vec<FarOffset>>>,
    universe_sent: u64,
    load: Arc<Mutex<Vec<PlanetLoad>>>,
    /// The last `chat::HISTORY` universe lines, oldest first (memory only).
    history: VecDeque<ChatLine>,
    ash: BlockId,
    /// Unix seconds each claim owner was last seen; an online player counts
    /// from when they leave.
    last_seen: HashMap<String, u64>,
    /// Claims crumbling now.
    lapses: Vec<Lapse>,
    /// Whether claims have been checked since the server started.
    lapses_checked: bool,
    /// Chunks lapses have finished with: unpinned once a save has taken
    /// them, so their ash is saved with the items reclaimed from them.
    unpin_after_save: Vec<(usize, ChunkPos)>,
    /// What lapsed claims gave back, per account.
    reclaims: Reclaims,
    /// Every portal in the universe (ADR-0009).
    portals: Portals,
    portal_rules: PortalRules,
    portal_conduit: BlockId,
    portal_surface: BlockId,
    portal_token: ItemId,
    wayshard: ItemId,
    /// Spark links, and the refund item of a broken generator (milestone 15).
    spark_link: BlockId,
    spark_item: ItemId,
    /// A bucket of water (milestone 16), what a filled bucket becomes.
    water_bucket: ItemId,
    /// Crop kinds and the blocks farming turns into one another
    /// (milestone 16 part 2).
    crops: bm_core::crops::CropKinds,
    /// Each grass and the soil under it (milestone 19: three of each);
    /// a tiller tills any of them, sowing on a grass bares its soil.
    grass_soils: Vec<(BlockId, BlockId)>,
    tilled_soil: BlockId,
    /// Damaged blocks, by planet index (milestone 9; never saved).
    damage: Vec<DamageMap>,
    /// Bonus yields are rolled from this (splitmix64).
    rng: u64,
    /// The skill tree (`assets/skills.toml`), sent in every welcome.
    skills: SkillTree,
    /// The creature catalogue (`assets/creatures.toml`), sent in every
    /// welcome, and how each kind moves.
    creature_kinds: CreatureKinds,
    creature_moves: Vec<MoveConfig>,
    /// Creatures by planet index (never saved).
    herds: Vec<creatures::Herds>,
    next_creature: u32,
    /// Shots in flight, on every planet.
    shots: Vec<combat::Shot>,
    next_shot: u32,
    /// Scratch the creature step reuses.
    seen_buf: Vec<creatures::Seen>,
    acts_buf: Vec<creatures::Act>,
    /// Regions and meteors by planet index (milestone 14).
    skies: Vec<meteors::Sky>,
    /// Regions discovered, by account (loaded on joining), and the accounts
    /// whose list changed since the last save.
    discoveries: HashMap<String, crate::store::Discoveries>,
    discoveries_dirty: HashSet<String>,
    next_meteor: u32,
    meteor_loot: bm_core::meteors::MeteorLoot,
    meteor_shell_id: BlockId,
    meteor_heart_id: BlockId,
    /// Planet kinds, for making wandering worlds (milestone 21).
    kinds: bm_worldgen::PlanetKinds,
    /// How many of `planets` are permanent; the rest are wandering-world
    /// slots (ADR-0011).
    permanent: usize,
    /// The tick the wanderers were last looked over.
    exo_checked: u64,
    /// Departed wanderers whose rows the next save deletes, with their hosts.
    forget_planets: Vec<(u16, u16)>,
    /// The lock block (milestone 22 part 2): beside a door on a claim, it
    /// keeps the door shut to all but the owner.
    lock: BlockId,
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
    /// Whole ticks since the last report: time, the longest, and the count.
    tick_time: Duration,
    tick_worst: Duration,
    ticks: u64,
    /// Game-thread time on far views (milestone 20) since the last report.
    far_time: Duration,
    /// Time spent on creatures and shots (stepping, AI, views) since the
    /// last report, the longest tick of it, and the ticks counted.
    creature_time: Duration,
    creature_worst: Duration,
    creature_ticks: u32,
    /// Of which telling players what creatures they see.
    view_time: Duration,
}

fn station_name(kind: Station) -> &'static str {
    match kind {
        Station::Hand => "hand",
        Station::Workbench => "Workbench",
        Station::Furnace => "Furnace",
        Station::CraftingTable => "Crafting Table",
        Station::Extractor => "Extractor",
        Station::Mixer => "Mixer",
        Station::Refinery => "Refinery",
        Station::Compactor => "Compactor",
        Station::SparkGenerator => "Spark Generator",
        Station::Forge => "Forge",
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A closed storage view: the client closes its window for `pos`.
fn closed_storage(pos: BlockPos) -> ServerMsg {
    ServerMsg::Storage(StorageView {
        pos,
        slots: Vec::new(),
        closed: true,
    })
}

/// The slot a `SlotRef` names, if it exists.
fn slot_of(inv: &[Option<ItemStack>], storage: &[Option<ItemStack>], r: SlotRef) -> Option<Option<ItemStack>> {
    match r {
        SlotRef::Inventory(i) => inv.get(i as usize).copied(),
        SlotRef::Storage(i) => storage.get(i as usize).copied(),
    }
}

fn set_slot(inv: &mut [Option<ItemStack>], storage: &mut [Option<ItemStack>], r: SlotRef, v: Option<ItemStack>) {
    match r {
        SlotRef::Inventory(i) => inv[i as usize] = v,
        SlotRef::Storage(i) => storage[i as usize] = v,
    }
}

/// Moves between two slots of a pack and a storage block (either may be in
/// either), with the rules of `Inventory::move_items`. Every index is
/// checked; all or nothing. Returns whether anything changed.
fn storage_move_slots(
    items: &ItemRegistry,
    inv: &mut [Option<ItemStack>],
    storage: &mut [Option<ItemStack>],
    from: SlotRef,
    to: SlotRef,
    count: u16,
) -> bool {
    if from == to {
        return false;
    }
    let (Some(src), Some(dst)) = (slot_of(inv, storage, from), slot_of(inv, storage, to)) else {
        return false;
    };
    match move_stack(items, src, dst, count) {
        Some((a, b)) if (a, b) != (src, dst) => {
            set_slot(inv, storage, from, a);
            set_slot(inv, storage, to, b);
            true
        }
        _ => false,
    }
}

/// Whether a block is within a player's reach (eye to block centre).
fn in_reach(shape: PlanetShape, m: &MoveConfig, reach: f64, state: &PlayerState, canon: BlockPos) -> bool {
    let centre = DVec3::new(canon.x as f64 + 0.5, canon.y as f64 + 0.5, canon.z as f64 + 0.5);
    shape.delta(state.eye(m), centre).length() <= reach + 0.87
}

/// Whether a player's body standing at `feet` overlaps the box `lo..hi`.
fn body_overlaps(m: &MoveConfig, feet: DVec3, lo: DVec3, hi: DVec3) -> bool {
    let (blo, bhi) = (
        DVec3::new(feet.x - m.half_width, feet.y, feet.z - m.half_width),
        DVec3::new(feet.x + m.half_width, feet.y + m.height, feet.z + m.half_width),
    );
    (0..3).all(|a| blo[a] < hi[a] && bhi[a] > lo[a])
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

/// One use off whatever wears in `slot` (a tool, a chisel): the notice to
/// send if that used it up, and it is gone.
fn wear_slot(items: &ItemRegistry, inv: &mut Inventory, slot: usize) -> Option<String> {
    let s = inv.slots.get_mut(slot)?.as_mut()?;
    let def = items.get(s.item)?;
    let durability = def.durability_of(s)?;
    s.wear += 1;
    if s.wear >= durability {
        let text = format!("Your {} broke", def.display);
        inv.slots[slot] = None;
        return Some(text);
    }
    None
}

/// The stats a swing uses holding `stack`, through its traits (milestone
/// 17); None for a bare hand or anything not a tool, which cannot break
/// blocks.
fn swing_tool_of(items: &ItemRegistry, stack: Option<ItemStack>, held: &Held) -> Option<ToolStats> {
    let s = stack?;
    items.get(s.item)?.tool_of(&s, held)
}

/// Makes a stored inventory safe: impossible stacks and spent tools go.
fn sanitize(inv: &mut Inventory, items: &ItemRegistry) {
    inv.sanitize(items);
    for slot in inv.slots.iter_mut() {
        if let Some(s) = slot {
            let spent = items
                .get(s.item)
                .and_then(|d| d.durability_of(s))
                .is_some_and(|d| s.wear >= d);
            let worn_material = s.wear > 0 && items.get(s.item).is_some_and(|d| d.durability().is_none());
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
        mut planets: Vec<Planet>,
        start: usize,
        next_claim: u32,
        items: ItemRegistry,
        recipes: RecipeBook,
        creature_kinds: CreatureKinds,
        store: Store,
        writer: Sender<WriteBatch>,
        events: Receiver<Event>,
        kinds: bm_worldgen::PlanetKinds,
    ) -> Result<Self> {
        let tick = store.clock().unwrap_or_else(|e| {
            tracing::error!("reading the universe clock: {e:#}; starting at 0");
            0
        });
        let registry = planets.first().context("no planets")?.world.registry.clone();
        let claim_stone = registry.id("claim_stone")?;
        let warp_shrine = registry.id("warp_shrine")?;
        let ash = registry.id("ash")?;
        let portal_conduit = registry.id("portal_conduit")?;
        let portal_surface = registry.id("portal_surface")?;
        let portal_token = items.id("portal_token")?;
        let wayshard = items.id("wayshard")?;
        let spark_link = registry.id("spark_link")?;
        let spark_item = items.id("spark")?;
        let water_bucket = items.id("water_bucket")?;
        let crops = bm_core::crops::CropKinds::build(include_str!("../../../assets/crops.toml"), &registry, &items)?;
        let grass_soils = [("turf", "soil"), ("meadow_turf", "clay"), ("moss_turf", "peat")]
            .iter()
            .map(|(g, s)| Ok((registry.id(g)?, registry.id(s)?)))
            .collect::<Result<Vec<_>, bm_core::RegistryError>>()?;
        let tilled_soil = registry.id("tilled_soil")?;
        for p in &mut planets {
            p.farm = crate::farm::Farm::load(store.crops(p.id)?, &crops);
        }
        let (saved_portals, next_portal) = store.portals()?;
        let portals = Portals::load(saved_portals, next_portal, |id| {
            planets.get(id as usize).map(|p| p.world.shape)
        });
        let portal_rules = cfg.portal_rules();
        let reclaims = Reclaims::load(store.reclaims()?, &items);
        let last_seen = claim_owners_last_seen(&store, &planets, planets[start].id);
        let planet_count = planets.len();
        let planets_permanent = planets.iter().take_while(|p| p.exo.is_none()).count();
        let creature_moves = creature_kinds.defs().iter().map(|d| d.move_config()).collect();
        let meteor_shell_id = registry.id("meteor_shell")?;
        let meteor_heart_id = registry.id("meteor_heart")?;
        let region_names = bm_core::regions::RegionNames::builtin();
        let mut skies = Vec::with_capacity(planet_count);
        for p in &planets {
            let generator = p.world.generator().clone();
            skies.push(meteors::Sky::new(
                &p.name,
                &p.kind,
                p.world.shape,
                &region_names,
                |x, z| meteors::land_of(&generator, x, z),
                store.sky(p.id)?,
                tick,
            ));
        }
        let next_meteor = skies
            .iter()
            .flat_map(|s| s.meteors.iter().map(|m| m.id))
            .max()
            .unwrap_or(0)
            + 1;
        if let Some(wear) = cfg.machine_wear_start {
            for p in &mut planets {
                p.stations.start_wear = wear.min(bm_core::spark::MACHINE_WEAR);
            }
        }
        for p in &mut planets {
            p.stations.forge_seconds = cfg.forge_queue_seconds;
        }
        Ok(Self {
            cfg,
            planets,
            start,
            items,
            colours: ColourPalette::builtin(),
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
            far_offsets: HashMap::new(),
            universe_sent: 0,
            load: Arc::new(Mutex::new(Vec::new())),
            history: VecDeque::new(),
            ash,
            last_seen,
            lapses: Vec::new(),
            lapses_checked: false,
            unpin_after_save: Vec::new(),
            reclaims,
            portals,
            portal_rules,
            portal_conduit,
            portal_surface,
            portal_token,
            wayshard,
            spark_link,
            spark_item,
            water_bucket,
            crops,
            grass_soils,
            tilled_soil,
            damage: (0..planet_count).map(|_| DamageMap::default()).collect(),
            rng: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0x2545_f491, |d| d.as_nanos() as u64),
            skills: SkillTree::builtin(),
            creature_kinds,
            creature_moves,
            herds: (0..planet_count).map(|_| creatures::Herds::default()).collect(),
            next_creature: 1,
            shots: Vec::new(),
            next_shot: 1,
            seen_buf: Vec::new(),
            acts_buf: Vec::new(),
            skies,
            discoveries: HashMap::new(),
            discoveries_dirty: HashSet::new(),
            next_meteor,
            meteor_loot: bm_core::meteors::MeteorLoot::builtin(),
            meteor_shell_id,
            meteor_heart_id,
            permanent: planets_permanent,
            kinds,
            exo_checked: tick,
            forget_planets: Vec::new(),
            lock: registry.id("lock")?,
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
            self.stats.tick_time += took;
            self.stats.tick_worst = self.stats.tick_worst.max(took);
            self.stats.ticks += 1;
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
                    "{} {}p {}c{}{}{}",
                    p.name,
                    p.players,
                    p.world.resident(),
                    if p.far.held() > 0 {
                        format!(" {}f", p.far.held())
                    } else {
                        String::new()
                    },
                    if p.world.pending_loads() > 0 {
                        format!("+{}l", p.world.pending_loads())
                    } else {
                        String::new()
                    },
                    if held > 0 { format!("+{held}s") } else { String::new() },
                )
            })
            .collect();
        // Creatures by kind ("12 grazer, 3 popper").
        let mut kinds = vec![0usize; self.creature_kinds.defs().len()];
        for c in self.herds.iter().flat_map(|h| &h.list) {
            kinds[c.kind as usize] += 1;
        }
        let kinds: Vec<String> = self
            .creature_kinds
            .iter()
            .filter(|(id, _)| kinds[*id as usize] > 0)
            .map(|(id, d)| format!("{} {}", kinds[id as usize], d.name))
            .collect();
        tracing::info!(
            "tick {} ({:.2} ms a tick, worst {:.1} ms, far {:.0} us) | {} players | {} | sent {} chunks ({} KiB) | edits {} ok {} \
             refused | {} slow ticks | {} creatures ({}), {} shots: {:.0} us a tick ({:.0} us views), worst {:.1} ms",
            self.tick,
            s.tick_time.as_secs_f64() * 1e3 / s.ticks.max(1) as f64,
            s.tick_worst.as_secs_f64() * 1e3,
            s.far_time.as_secs_f64() * 1e6 / s.ticks.max(1) as f64,
            self.players.len(),
            planets.join(", "),
            s.chunks_sent,
            s.bytes_sent / 1024,
            s.edits,
            s.refused_edits,
            s.slow_ticks,
            self.herds.iter().map(|h| h.list.len()).sum::<usize>(),
            kinds.join(", "),
            self.shots.len(),
            s.creature_time.as_secs_f64() * 1e6 / s.creature_ticks.max(1) as f64,
            s.view_time.as_secs_f64() * 1e6 / s.creature_ticks.max(1) as f64,
            s.creature_worst.as_secs_f64() * 1e3,
        );
        self.stats.tick_time = Duration::ZERO;
        self.stats.tick_worst = Duration::ZERO;
        self.stats.ticks = 0;
        self.stats.far_time = Duration::ZERO;
        self.stats.creature_time = Duration::ZERO;
        self.stats.creature_worst = Duration::ZERO;
        self.stats.creature_ticks = 0;
        self.stats.view_time = Duration::ZERO;
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
        let far = Instant::now();
        self.receive_far();
        self.stats.far_time += far.elapsed();
        self.catch_up_crops();
        // Before streaming: a chunk is never sent with a portal's old look.
        self.sync_portals();
        self.simulate();
        let creatures = Instant::now();
        self.step_creatures();
        self.step_shots();
        self.step_meteors();
        let views = Instant::now();
        if self.tick % SNAPSHOT_EVERY == 0 {
            self.sync_creature_views();
        }
        self.stats.view_time += views.elapsed();
        let took = creatures.elapsed();
        self.stats.creature_time += took;
        self.stats.creature_worst = self.stats.creature_worst.max(took);
        self.stats.creature_ticks += 1;
        self.step_portals();
        self.step_stations();
        if self.tick % farm::FARM_EVERY == 0 {
            self.step_farms();
        }
        self.step_lapses();
        self.step_exos();
        if self.tick % mining::HEAL_SWEEP_EVERY == 0 {
            self.heal_blocks();
        }
        if self.tick % STORAGE_CHECK_EVERY == 0 {
            self.check_open_storage();
            self.check_open_portals();
            self.update_inner_light();
        }
        if self.tick % skills::CHARACTER_EVERY == 0 {
            self.send_dirty_characters();
        }
        let ids: Vec<u32> = self.players.keys().copied().collect();
        for id in &ids {
            self.stream(*id);
            let far = Instant::now();
            self.stream_far(*id);
            self.stats.far_time += far.elapsed();
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
            for p in &mut self.planets {
                p.far.tend(self.tick, p.players, FAR_IDLE_TICKS);
            }
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
                let evicted = p.world.evict_unwanted(before, &rx);
                for (c, bytes) in &evicted {
                    p.far.rebuild(*c, bytes.clone());
                }
                chunks.extend(evicted.into_iter().map(|(c, bytes)| (id, c, bytes)));
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

    /// The planets players can see now (a wandering world only while it is
    /// in the sky).
    fn universe(&self) -> Vec<PlanetSummary> {
        self.planets
            .iter()
            .filter(|p| p.present(self.tick))
            .map(Planet::summary)
            .collect()
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
                ClientMsg::Skill { at, action } => self.queue(id, at, Queued::Skill { at, action }),
                ClientMsg::Creative { at, on } => self.queue(id, at, Queued::Creative { at, on }),
                ClientMsg::Command { text } => self.command(id, &text),
                ClientMsg::Eat { at, slot } => self.queue(id, at, Queued::Eat { at, slot }),
                ClientMsg::FillBucket { slot, pos } => self.fill_bucket(id, slot, pos),
                ClientMsg::LookCrop { pos } => self.look_crop(id, pos),
                ClientMsg::ForgeBegin { pos, gear, deck } => self.forge_begin(id, pos, gear, deck),
                ClientMsg::ForgeUse { pos, slot, round } => self.forge_use(id, pos, slot, round),
                ClientMsg::Throw {
                    at,
                    slot,
                    yaw,
                    pitch,
                    hold,
                } => self.queue(
                    id,
                    at,
                    Queued::Throw {
                        at,
                        slot,
                        yaw,
                        pitch,
                        hold,
                    },
                ),
                ClientMsg::Shoot {
                    at,
                    slot,
                    yaw,
                    pitch,
                    charge,
                } => self.queue(
                    id,
                    at,
                    Queued::Shoot {
                        at,
                        slot,
                        yaw,
                        pitch,
                        charge,
                    },
                ),
                ClientMsg::Strike { at, creature, slot } => self.queue(id, at, Queued::Strike { at, creature, slot }),
                ClientMsg::Loot { creature } => self.loot(id, creature),
                ClientMsg::Return => self.return_player(id),
                ClientMsg::LootHeart { pos } => self.loot_heart(id, pos),
                ClientMsg::StationFuel { pos, slot, count, into } => self.station_fuel(id, pos, slot, count, into),
                ClientMsg::Edit { seq, at, pos, action } => self.queue(id, at, Queued::Edit { seq, pos, action }),
                ClientMsg::Swing { seq, at, pos, slot } => self.queue(id, at, Queued::Swing { seq, at, pos, slot }),
                ClientMsg::MoveItems { from, to, count } => {
                    self.change_inventory(id, |inv, items| {
                        inv.move_items(items, from as usize, to as usize, count)
                    });
                }
                ClientMsg::Discard { slot } => {
                    self.change_inventory(id, |inv, _| inv.clear(slot as usize));
                }
                ClientMsg::SwingDoor { pos } => self.swing_door(id, pos),
                ClientMsg::Craft {
                    recipe,
                    count,
                    station,
                    colour,
                } => self.craft(id, recipe, count, station, colour),
                ClientMsg::WatchStation { pos } => self.watch(id, pos),
                ClientMsg::CollectStation { pos } => self.collect(id, pos),
                ClientMsg::ClaimPlot { claim, plot } => self.claim_plot(id, claim, plot),
                ClientMsg::ReleasePlot { plot } => self.release_plot(id, plot),
                ClientMsg::Warp { shrine, planet } => self.warp(id, shrine, planet),
                ClientMsg::OpenStorage { pos } => self.open_storage(id, pos),
                ClientMsg::StorageMove { pos, from, to, count } => self.storage_move(id, pos, from, to, count),
                ClientMsg::Chat { channel, text } => self.chat(id, channel, text),
                ClientMsg::CollectReclaim { shrine } => self.collect_reclaim(id, shrine),
                ClientMsg::OpenPortal { pos } => self.open_portal(id, pos),
                ClientMsg::Portal { pos, action } => self.portal_action(id, pos, action),
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
        // Told after the welcome: the wandering world they were on has gone.
        let mut came_back: Option<String> = None;
        let (planet, saved, mut inventory, character, hurt, food, air_used) = match self.departed.remove(&account) {
            Some(d) => (
                d.planet,
                Some(d.state),
                d.inventory,
                d.character,
                d.state.hurt,
                SavedFood::from(&d.state),
                d.state.air_used,
            ),
            None => match self.store.player(&account) {
                Ok(Some(data)) => {
                    // A planet that is gone reads as one this server does
                    // not host; a departed wanderer's players go to its host.
                    let (pi, why) = self.returning_to(data.planet).unwrap_or((usize::MAX, None));
                    let state = match why {
                        Some(_) => None,
                        None => data.state.map(PlayerState::from),
                    };
                    came_back = why;
                    (
                        pi,
                        state,
                        Inventory {
                            slots: load_slots(&self.items, &data.inventory, &name),
                        },
                        skills::sanitize(data.character.into(), &self.skills),
                        data.hurt,
                        data.food,
                        data.air_used,
                    )
                }
                Ok(None) => (
                    self.start,
                    None,
                    self.starting_inventory(),
                    Character::default(),
                    0,
                    SavedFood::default(),
                    0,
                ),
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
        // What the character's skills do to the body, from the first input.
        let effects = character.effects(&self.skills);
        let mut state = PlayerState {
            body: effects.body(),
            // Health lost and hunger go with the character, wherever they
            // start (milestone 16: the food buff and sickness too).
            hurt,
            hunger: food.hunger,
            buff: food.buff,
            sick: food.sick,
            // The air of where they are, and what they had left of it
            // (milestone 18).
            air: pl.air,
            air_used,
            // Never left lying defeated: one who left down comes back up.
            down: false,
            ..state
        };
        if state.hurt >= state.max_health() {
            let back = (state.max_health() as f64 * combat::RETURN_HEALTH_SHARE).round() as u32;
            state.hurt = state.max_health() - back.max(1);
        }
        state.clamp_to_maxima();
        out.send(&ServerMsg::Welcome(Box::new(Welcome {
            player_id: id,
            account: name.clone(),
            planet: pl.info(),
            universe: self.universe(),
            blocks: pl.world.registry.defs().to_vec(),
            items: self.items.defs().to_vec(),
            colours: self.colours.clone(),
            recipes: self.recipes.recipes.clone(),
            skills: self.skills.skills.clone(),
            character: character.clone(),
            inventory: inventory.clone(),
            movement: self.movement,
            spawn: state,
            tick: self.tick,
            view_radius: self.cfg.view_radius,
            far_radius: self.cfg.far_radius,
            reach: self.cfg.reach,
            claims: pl.claims.views(),
            // Plots bought with marks included (the client adds nothing).
            plot_allowance: self.cfg.plot_allowance.saturating_add(character.plots_bought),
            creatures: self.creature_kinds.defs().to_vec(),
        })));
        for (oid, other) in self.players.iter().filter(|(_, o)| o.planet == planet) {
            out.send(&ServerMsg::PlayerJoined {
                id: *oid,
                name: other.name.clone(),
            });
            other.out.send(&ServerMsg::PlayerJoined { id, name: name.clone() });
        }
        if !self.history.is_empty() {
            out.send(&ServerMsg::Chat {
                lines: self.history.iter().cloned().collect(),
            });
        }
        if let Some(text) = shrine_hint(shape, &pl.name, pl.shrine, state.pos) {
            out.send(&ServerMsg::Notice { text });
        }
        // Back in time: a claim crumbling now stops where it is.
        for text in self.stop_lapses_of(&account) {
            out.send(&ServerMsg::Notice { text });
        }
        for text in self.reclaims.take_notices(&account) {
            out.send(&ServerMsg::Notice { text });
        }
        let reclaimed = self.reclaims.items(&account);
        if !reclaimed.is_empty() {
            out.send(&ServerMsg::Reclaim {
                items: reclaimed.to_vec(),
            });
        }
        let pl = &self.planets[planet];
        tracing::info!("{name} (#{id}) joined on {} at {:.1?}", pl.name, state.pos);
        let arrived = format!("{name} arrived on {}", pl.name);
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
                far_sent: HashMap::new(),
                far_center: None,
                near_complete: false,
                far_cursor: 0,
                edit_tokens: EDIT_BURST,
                last_swing: None,
                edits: VecDeque::new(),
                watching: None,
                storage: None,
                chat: ChatLimit::new(self.tick),
                travel_guard: None,
                portal: None,
                portal_guard: false,
                trip: None,
                told_tokens: HashSet::new(),
                character,
                effects,
                character_dirty: false,
                inner_light: false,
                known: HashMap::new(),
                in_view: Vec::new(),
                near_creatures: Vec::new(),
                defeated: false,
                region: None,
                square: None,
                at_centre: false,
            },
        );
        let key = self.players[&id].account.clone();
        if !self.discoveries.contains_key(&key) {
            let found = self.store.discoveries(&key).unwrap_or_else(|e| {
                tracing::error!("reading {key}'s discoveries: {e:#}; starting with none");
                Default::default()
            });
            self.discoveries.insert(key, found);
        }
        self.arrived_in_sky(id);
        self.tell_tokens(id);
        self.server_line(planet, arrived);
        if let Some(text) = came_back {
            self.notice(id, &text);
        }
        if let Some(text) = self.players[&id].state.air_warning() {
            self.notice(id, &text);
        }
    }

    fn leave(&mut self, id: u32) {
        let Some(p) = self.players.remove(&id) else { return };
        tracing::info!("{} (#{id}) left", p.name);
        if let Some(trip) = &p.trip {
            self.unpin_trip(trip);
        }
        self.last_seen.insert(p.account.clone(), unix_now());
        let planet = p.planet;
        self.departed.insert(
            p.account,
            Departed {
                planet,
                // A player who leaves lets go of their grapple.
                state: PlayerState { hook: None, ..p.state },
                inventory: p.inventory,
                character: p.character,
                written: None,
            },
        );
        // Everything, not just the player: what they gathered and built
        // must reach the disk in the same transaction as their inventory.
        self.save(None);
        for other in self.players.values().filter(|o| o.planet == planet) {
            other.out.send(&ServerMsg::PlayerLeft { id });
        }
        self.server_line(planet, format!("{} left", p.name));
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
                    // repeat the last one, but never its shot (a repeated
                    // shot would let go of the hook it just fired). The
                    // client sees our answer in the snapshot and corrects
                    // its prediction.
                    PlayerInput {
                        fire: false,
                        ..p.last_input
                    }
                };
                // The grapple in the hand the input names, from our own copy
                // of the inventory: the client's word is only which slot.
                let slot = input.slot as usize;
                // Through its traits (milestone 17): the spec as the client
                // builds it, its energy by the holder as they stand before
                // this input's step.
                let held = Held::of(&p.state, &self.planets[p.planet].day, self.tick as f64);
                let grapple = p
                    .inventory
                    .slots
                    .get(slot)
                    .filter(|_| slot < HOTBAR_SLOTS)
                    .copied()
                    .flatten()
                    .and_then(|s| {
                        let d = self.items.get(s.item)?;
                        let mut g = d.grapple_of(&s, &Held::default())?;
                        g.energy = d.energy_of(&s, &held).unwrap_or(g.energy);
                        Some(g)
                    });
                // The body quirks of what is in that hand (milestone 17
                // part 2), set the same way the client sets them.
                p.state.quirks = bm_core::forge::Quirks::of(
                    p.inventory
                        .slots
                        .get(slot)
                        .filter(|_| slot < HOTBAR_SLOTS)
                        .and_then(|s| s.as_ref()),
                );
                let world = &self.planets[p.planet].world;
                // Held at a portal while its far side loads.
                let held = p.trip.as_ref().map(|t| t.held);
                let events = movement::step(
                    &mut p.state,
                    &input,
                    &self.movement,
                    grapple.as_ref(),
                    world.shape,
                    world,
                );
                if let Some(h) = held {
                    // Held in place, but a skill change since still counts.
                    p.state = PlayerState {
                        body: p.state.body,
                        ..h
                    };
                }
                p.last_seq += 1;
                p.last_input = input;
                p.bank -= 1.0;
                let fell = p.state.down && !p.defeated;
                if (events.fired || events.released)
                    && mining::roll(&mut self.rng) >= p.state.wear_spared(p.effects.wear_saved())
                {
                    let before = p.inventory.clone();
                    let mut broke = None;
                    if let Some(s) = p.inventory.slots[slot].as_mut() {
                        s.wear += 1;
                        if let Some(def) = self.items.get(s.item)
                            && def.durability_of(s).is_some_and(|d| s.wear >= d)
                        {
                            broke = Some(format!("Your {} broke", def.display));
                            p.inventory.slots[slot] = None;
                        }
                    }
                    send_slots(p, &before);
                    if let Some(text) = broke {
                        p.out.send(&ServerMsg::Notice { text });
                    }
                }
                if fell {
                    self.check_defeat(id, combat::BY_A_FALL);
                }
            }
        }
    }

    /// Applies an edit or swing right after the input it follows, in the
    /// order the client sent them: a swing at the next block must never
    /// overtake the break of the one before it.
    fn queue(&mut self, id: u32, at: u32, queued: Queued) {
        let Some(p) = self.players.get_mut(&id) else { return };
        if queued.attack() {
            // Sent about the planet they left, or too far ahead: dropped
            // (the client hears nothing of its own attacks).
            if p.travel_guard.is_some_and(|g| at <= g) {
                return;
            }
            if at <= p.last_seq && p.edits.is_empty() {
                self.apply_queued(id, queued);
            } else if at <= p.last_seq + MAX_EDIT_LEAD && p.edits.len() < MAX_QUEUED_EDITS {
                p.edits.push_back((at, queued));
            }
            return;
        }
        let Some((seq, pos)) = queued.seq_pos() else {
            // A skill change belongs to no planet: travel does not void it.
            if at <= p.last_seq && p.edits.is_empty() {
                self.apply_queued(id, queued);
            } else if at > p.last_seq + MAX_EDIT_LEAD || p.edits.len() >= MAX_QUEUED_EDITS {
                self.skill_refused(id, at, "Too much at once. Try again".into());
            } else {
                p.edits.push_back((at, queued));
            }
            return;
        };
        if p.travel_guard.is_some_and(|g| at <= g) {
            // Sent about the planet they left.
            self.edit_refused(id, seq, pos, "edit from before travelling");
            if matches!(queued, Queued::Swing { .. }) {
                self.send_health(id, pos);
            }
        } else if at <= p.last_seq && p.edits.is_empty() {
            // Late (its input was already simulated, e.g. a lost input was
            // repeated): apply now.
            self.apply_queued(id, queued);
        } else if at > p.last_seq + MAX_EDIT_LEAD || p.edits.len() >= MAX_QUEUED_EDITS {
            self.edit_refused(id, seq, pos, "edit too far ahead of movement");
            if matches!(queued, Queued::Swing { .. }) {
                self.send_health(id, pos);
            }
        } else {
            p.edits.push_back((at, queued));
        }
    }

    fn apply_queued(&mut self, id: u32, queued: Queued) {
        match queued {
            Queued::Edit { seq, pos, action } => self.edit(id, seq, pos, action),
            Queued::Swing { seq, at, pos, slot } => self.swing(id, seq, at, pos, slot),
            Queued::Skill { at, action } => self.skill(id, at, action),
            Queued::Creative { at, on } => self.set_creative(id, at, on),
            Queued::Shoot {
                at,
                slot,
                yaw,
                pitch,
                charge,
            } => self.shoot(id, at, slot, yaw, pitch, charge),
            Queued::Strike { at, creature, slot } => self.strike(id, at, creature, slot),
            Queued::Throw {
                at,
                slot,
                yaw,
                pitch,
                hold,
            } => self.throw(id, at, slot, yaw, pitch, hold),
            Queued::Eat { at, slot } => self.eat(id, at, slot),
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
        // Nothing left unsent: far copies may go (`stream_far`).
        p.near_complete = scanned == 0;
    }

    /// Takes finished far copies in; players holding a copy that changed are
    /// sent the new one.
    fn receive_far(&mut self) {
        for (pi, planet) in self.planets.iter_mut().enumerate() {
            planet.far.receive();
            let changed = planet.far.take_changed();
            if changed.is_empty() {
                continue;
            }
            for p in self.players.values_mut().filter(|p| p.planet == pi) {
                for c in &changed {
                    if let Some(level) = p
                        .far_sent
                        .get_mut(&(c.x, c.z))
                        .and_then(|col| col.get_mut(c.y as usize))
                        .filter(|l| **l > 0)
                    {
                        *level = 0;
                        p.far_cursor = 0;
                    }
                }
            }
        }
    }

    /// The far columns of a planet `width` chunks wide, as offsets from the
    /// player's column with their level, nearest first: within twice the
    /// view radius level 1, beyond it level 2, out to the far radius or half
    /// the planet (so no column is listed twice).
    fn far_offsets(&mut self, width: u32) -> Arc<Vec<FarOffset>> {
        let (far, mid) = (self.cfg.far_radius as i32, 2 * self.cfg.view_radius as i32);
        self.far_offsets
            .entry(width)
            .or_insert_with(|| {
                let r = far.min(width as i32 / 2);
                let mut list = Vec::new();
                let mut seen = HashSet::new();
                for dz in -r..=r {
                    for dx in -r..=r {
                        let d2 = dx * dx + dz * dz;
                        // Wrapped, -r and r are one column when r is half.
                        if d2 <= r * r && seen.insert((dx.rem_euclid(width as i32), dz.rem_euclid(width as i32))) {
                            list.push((d2, dx, dz));
                        }
                    }
                }
                list.sort_unstable();
                Arc::new(
                    list.into_iter()
                        .map(|(d2, dx, dz)| (dx, dz, d2, if d2 <= mid * mid { 1 } else { 2 }))
                        .collect(),
                )
            })
            .clone()
    }

    /// Far copies (milestone 20): every chunk within the far radius at its
    /// ring's level, nearest column first, once the near chunks are all sent.
    fn stream_far(&mut self, id: u32) {
        if self.cfg.far_radius == 0 || (self.tick + id as u64) % FAR_EVERY != 0 {
            return;
        }
        let Some(p) = self.players.get(&id) else { return };
        // Near chunks first: nothing far is done while any are unsent.
        if p.out.backlog() > MAX_BACKLOG / 2 || !p.near_complete {
            return;
        }
        let pi = p.planet;
        let shape = self.planets[pi].world.shape;
        let at = shape.wrap_chunk(BlockPos::containing(p.state.pos.x, p.state.pos.y, p.state.pos.z).chunk());
        let here = ChunkPos::new(at.x, 0, at.z);
        let offsets = self.far_offsets(shape.width_chunks);
        let loading: usize = self.planets.iter().map(|p| p.world.pending_loads()).sum();
        let jobs = if loading > 0 {
            FAR_JOBS_WHILE_LOADING
        } else {
            rayon::current_num_threads()
        };
        let mut running: usize = self.planets.iter().map(|p| p.far.running()).sum();
        let p = self.players.get_mut(&id).expect("present");
        let h = shape.height_chunks as usize;
        if p.far_center != Some(here) {
            // A step to the next column: what was sent within the old
            // cursor's reach, less the step, is still sent at its level
            // (inside the level ring), so the look-over resumes there.
            let stepped = p.far_center.is_some_and(|old| {
                let (dx, _, dz) = shape.chunk_delta(old, here);
                dx.abs() <= 1 && dz.abs() <= 1
            });
            let resume = if stepped && p.far_cursor > 0 {
                let mid = 2.0 * self.cfg.view_radius as f64;
                let done = (offsets[p.far_cursor - 1].2 as f64).sqrt();
                let safe = done.min(mid) - 2.0;
                offsets.partition_point(|o| (o.2 as f64).sqrt() <= safe)
            } else {
                0
            };
            let r = offsets.iter().map(|o| o.0.abs().max(o.1.abs())).max().unwrap_or(0) + 2;
            let gone: Vec<(i32, i32)> = p
                .far_sent
                .keys()
                .copied()
                .filter(|&(x, z)| {
                    let (dx, _, dz) = shape.chunk_delta(here, ChunkPos::new(x, 0, z));
                    dx * dx + dz * dz > r * r
                })
                .collect();
            for col in gone {
                let levels = p.far_sent.remove(&col).expect("listed");
                for (y, l) in levels.iter().enumerate() {
                    if *l > 0 {
                        p.out.send(&ServerMsg::ForgetFar {
                            pos: ChunkPos::new(col.0, y as i32, col.1),
                        });
                    }
                }
            }
            p.far_center = Some(here);
            p.far_cursor = resume;
        }
        if p.far_cursor >= offsets.len() {
            return;
        }
        let planet = &mut self.planets[pi];
        let mut budget = FAR_BYTES_PER_TICK * FAR_EVERY as usize;
        let mut blocked = false;
        let mut i = p.far_cursor;
        let mut scanned = 0;
        while i < offsets.len() && scanned < FAR_SCAN && budget > 0 {
            let (dx, dz, _, level) = offsets[i];
            scanned += 1;
            let col = shape.wrap_chunk(ChunkPos::new(here.x + dx, 0, here.z + dz));
            let sent = p
                .far_sent
                .entry((col.x, col.z))
                .or_insert_with(|| vec![0u8; h].into_boxed_slice());
            let mut whole = true;
            for y in 0..h {
                if sent[y] == level {
                    continue;
                }
                let c = ChunkPos::new(col.x, y as i32, col.z);
                if budget == 0 {
                    whole = false;
                    break;
                }
                match planet.far.get(c) {
                    Some(copy) => {
                        let frame = copy.frame(level).clone();
                        budget = budget.saturating_sub(frame.len());
                        self.stats.bytes_sent += frame.len() as u64;
                        p.out.send_shared(frame);
                        sent[y] = level;
                    }
                    None => {
                        whole = false;
                        if running < jobs && !planet.far.is_building(c) {
                            // From memory when the chunk is resident and saved.
                            let resident = planet.world.get(c).filter(|s| !s.dirty).map(|s| s.chunk.clone());
                            planet.far.request(c, resident);
                            running += 1;
                        }
                    }
                }
            }
            if !whole {
                blocked = true;
                // Every job slot busy: what lies further waits too.
                if running >= jobs {
                    break;
                }
            }
            if !blocked {
                p.far_cursor = i + 1;
            }
            i += 1;
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
                                hook: o
                                    .state
                                    .hook
                                    .map(|h| RemotePlayer::pack_hook(shape.delta(o.state.pos, h.pos))),
                                light: o
                                    .inventory
                                    .get(o.last_input.slot as usize)
                                    .filter(|_| (o.last_input.slot as usize) < HOTBAR_SLOTS)
                                    .map_or(0, |s| self.items.get(s.item).map_or(0, |d| d.held_light_of(&s)))
                                    .max(if o.inner_light { skills::INNER_LIGHT } else { 0 }),
                            },
                        ))
                    })
                    .collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
                near.truncate(MAX_SEEN_PLAYERS);
                let others: Vec<RemotePlayer> = near.into_iter().map(|(_, r)| r).collect();
                // Players first; creatures fill what room is left, then go
                // in snapshots of their own (`others` empty).
                let mut creatures = &p.in_view[..];
                let mut sent = false;
                for part in others.chunks(PLAYERS_PER_SNAPSHOT) {
                    let room = CREATURES_PER_SNAPSHOT * (PLAYERS_PER_SNAPSHOT - part.len()) / PLAYERS_PER_SNAPSHOT;
                    let (now, rest) = creatures.split_at(room.min(creatures.len()));
                    creatures = rest;
                    self.send_snapshot(p, part.to_vec(), now.to_vec());
                    sent = true;
                }
                for part in creatures.chunks(CREATURES_PER_SNAPSHOT) {
                    self.send_snapshot(p, Vec::new(), part.to_vec());
                    sent = true;
                }
                if !sent {
                    self.send_snapshot(p, Vec::new(), Vec::new());
                }
            }
        }
    }

    fn send_snapshot(&self, p: &Player, others: Vec<RemotePlayer>, creatures: Vec<RemoteCreature>) {
        bm_net::send_datagram(
            &p.conn,
            &ServerDatagram::Snapshot {
                planet: self.planets[p.planet].id,
                tick: self.tick,
                ack: p.last_seq,
                you: p.state,
                others,
                creatures,
            },
        );
    }

    /// A new player's pack: the hand stone, Boundless's starter tool, in the
    /// last hotbar slot (a bare hand breaks nothing).
    fn starting_inventory(&self) -> Inventory {
        let mut inv = Inventory::default();
        if let Ok(stone) = self.items.id("hand_stone") {
            inv.slots[HOTBAR_SLOTS - 1] = Some(ItemStack::new(stone, 1));
        }
        inv
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
        // Plots bought with marks add to it (Nathan, 2026-10-21).
        let bought = self
            .players
            .values()
            .find(|p| p.account == account)
            .map_or(0, |p| p.character.plots_bought);
        self.cfg.plot_allowance.saturating_add(bought).saturating_sub(elsewhere)
    }

    fn edit(&mut self, id: u32, seq: u32, pos: BlockPos, action: EditAction) {
        // A chisel cut costs energy first, whatever is ruled after (the
        // client spends it the same way when it sends the cut); too tired,
        // it does nothing but start the pause again.
        if let EditAction::Chisel { slot, .. } = action
            && let Some(p) = self.players.get_mut(&id)
            && let Some(cost) = p.inventory.get(slot as usize).and_then(|s| {
                let held = Held::of(&p.state, &self.planets[p.planet].day, self.tick as f64);
                self.items
                    .get(s.item)
                    .filter(|d| d.chisel().is_some())
                    .and_then(|d| d.energy_of(&s, &held))
            })
            && !p.state.try_spend(cost, bm_core::food::Drain::Tool)
        {
            return self.edit_refused(id, seq, pos, "too tired");
        }
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
        match accepted.effect {
            Effect::Placed { slot } => {
                p.inventory.take_one(slot);
                if new.id == self.claim_stone {
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
            Effect::Chiselled { slot } => {
                // The durability epic spares some wear.
                if mining::roll(&mut self.rng) >= p.state.wear_spared(p.effects.wear_saved()) {
                    broke = wear_slot(&self.items, &mut p.inventory, slot);
                }
            }
        }
        send_slots(p, &before);
        if let Some(text) = broke {
            p.out.send(&ServerMsg::Notice { text });
        }
        // Building inside your own claim earns the block's build XP.
        let build_xp = match accepted.effect {
            Effect::Placed { .. } if planet.claims.at_block(canon).is_some_and(|c| c.owner == p.account) => {
                planet.world.registry.get(new.id).build_xp as u64
            }
            _ => 0,
        };
        self.stats.edits += 1;
        self.change_block(pi, canon, new, Some(id));
        if matches!(accepted.effect, Effect::Placed { .. })
            && self.planets[pi].world.registry.get(new.id).station == Some(Station::SparkGenerator)
        {
            self.generator_placed(pi, canon);
        }
        self.add_xp(id, build_xp);
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
    }

    /// Changes one block on planet `pi` for everyone: the world (marked
    /// dirty, so it is saved), its damage (forgotten; watchers are told it
    /// reset unless it is now air), `BlockChanged` to every player who has
    /// the chunk except `by` (who changed it and was answered with an
    /// `EditResult`), and any prop it held up, which falls into `by`'s pack.
    /// Every block change of play goes through here.
    pub(super) fn change_block(&mut self, pi: usize, canon: BlockPos, new: BlockState, by: Option<u32>) {
        let old = self.planets[pi].world.set_state(canon, new);
        let reset = self.damage[pi].clear(canon, self.tick) && !new.id.is_air();
        self.crop_block_changed(pi, canon, new.id, by);
        // A machine block coming or going can make or unmake a machine
        // nearby: whoever watches one is told how it stands now.
        let reg = &self.planets[pi].world.registry;
        let machine_block = |s: Option<BlockState>| {
            s.is_some_and(|s| {
                let d = reg.get(s.id);
                d.assembly.is_some() || d.station.is_some()
            })
        };
        if machine_block(old) || machine_block(Some(new)) {
            let shape = self.planets[pi].world.shape;
            let near: BTreeSet<BlockPos> = self
                .players
                .values()
                .filter(|o| o.planet == pi)
                .filter_map(|o| o.watching)
                .filter(|w| {
                    let d = shape.delta(
                        DVec3::new(w.x as f64, w.y as f64, w.z as f64),
                        DVec3::new(canon.x as f64, canon.y as f64, canon.z as f64),
                    );
                    d.abs().max_element() <= 4.0
                })
                .collect();
            for w in near {
                self.push_station(pi, w);
            }
        }
        let chunk = canon.chunk();
        for (oid, o) in &self.players {
            if o.planet != pi || !o.sent.contains(&chunk) {
                continue;
            }
            if Some(*oid) != by {
                o.out.send(&ServerMsg::BlockChanged { pos: canon, block: new });
            }
            if reset {
                o.out.send(&ServerMsg::BlockHealth { pos: canon, damage: 0 });
            }
        }
        self.drop_unsupported_props(pi, canon, by);
    }

    fn edit_refused(&mut self, id: u32, seq: u32, pos: BlockPos, why: &str) {
        self.stats.refused_edits += 1;
        tracing::debug!("refused edit from #{id} at {pos:?}: {why}");
        let Some(p) = self.players.get(&id) else { return };
        // What the block is on the planet they are on now.
        let actual = self.planets[p.planet].world.state(pos).unwrap_or(BlockState::AIR);
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
        if p.state.down {
            return Err("defeated".into());
        }
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
        if !in_reach(
            shape,
            &self.movement,
            self.cfg.reach + p.effects.reach_bonus(),
            &p.state,
            canon,
        ) {
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
            EditAction::Place { slot, mount } => {
                let slot = slot as usize;
                let placer = p.state.pos;
                if slot >= HOTBAR_SLOTS {
                    return Err("not a hotbar slot".into());
                }
                let stack = p.inventory.get(slot).ok_or("nothing placeable in that slot")?;
                let mut block = self
                    .items
                    .get(stack.item)
                    .and_then(|d| d.block())
                    .ok_or("nothing placeable in that slot")?;
                if !reg.contains(block) || !reg.get(block).placeable {
                    return Err("cannot place that".into());
                }
                if !(current.is_air() || world.flags.liquid(current)) {
                    return Err("space is occupied".into());
                }
                // A prop (a torch) goes on the face the player clicked, and
                // needs that face to be solid; a mount on anything else is
                // ignored.
                if reg.get(block).is_prop() {
                    let mount = mount.ok_or("a prop needs a face to go on")?;
                    block = self
                        .items
                        .prop_block(reg, stack.item, mount)
                        .ok_or("no variant for that face")?;
                    let (dx, dy, dz) = mount.support();
                    let support = canon.offset(dx, dy, dz);
                    if !shape.contains_y(support.y) || !holds_props(world, support) {
                        return Err(Refusal::Told(
                            if mount == Mount::Floor {
                                "A torch needs something solid to stand on"
                            } else {
                                "A torch needs something solid to hang on"
                            }
                            .into(),
                        ));
                    }
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
                if let Some(kind) = self.crops.sown_as(block) {
                    Self::sowing_allowed(&self.crops, kind, world, canon, p.effects.farming)?;
                }
                if block == self.claim_stone {
                    if planet.exo.is_some() {
                        return Err(Refusal::Told(exo::NO_CLAIM.into()));
                    }
                    planet.claims.can_create(&p.account, canon, allowance)?;
                }
                // A working four-block machine never changes while it stands.
                if reg.get(block).assembly == Some(bm_core::block::Assembly::Four)
                    && crate::machines::exceeds_four(shape, |q| world.block(q), canon, block)
                {
                    return Err(Refusal::Told(format!(
                        "Only 4 {} blocks can touch",
                        bm_core::items::title_case(&reg.get(block).name)
                    )));
                }
                if reg.get(block).station == Some(Station::SparkGenerator)
                    && stations::joins_fuelled_generators(planet, canon, block)
                {
                    return Err(Refusal::Told(
                        "Joined spark generators share one fuel slot: take the fuel out of one first".into(),
                    ));
                }
                if block == self.portal_conduit {
                    if !planet.claims.at_block(canon).is_some_and(|c| c.owner == p.account) {
                        return Err(Refusal::Told("Portal conduits go only on your own claim".into()));
                    }
                    // Records and blocks must never disagree on a shape.
                    let touches = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                        .iter()
                        .any(|&(dx, dy, dz)| {
                            let n = canon.offset(dx, dy, dz);
                            shape.contains_y(n.y) && self.portals.at(planet.id, shape.wrap_block(n)).is_some()
                        });
                    if touches {
                        return Err(Refusal::Told("That would change the shape of a portal".into()));
                    }
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
                let colour = if reg.get(block).colourable {
                    stack.colour
                } else {
                    Colour::NATURAL
                };
                let mut placed = BlockState::coloured(block, colour);
                // A door stands on the side nearest whoever places it, a
                // furnishing takes its form turned to how it was placed
                // (milestone 22 parts 2 and 3).
                let def = reg.get(block);
                if def.swing.is_some() || def.form.is_some() {
                    let centre = DVec3::new(canon.x as f64 + 0.5, canon.y as f64 + 0.5, canon.z as f64 + 0.5);
                    let d = shape.delta(centre, placer);
                    let support = mount.and_then(|m| {
                        let (dx, dy, dz) = m.support();
                        world.state(canon.offset(dx, dy, dz))
                    });
                    placed.shape = bm_core::forms::placed_shape(def, mount, support, d.x, d.z);
                }
                Ok(Accepted {
                    block: placed,
                    effect: Effect::Placed { slot },
                })
            }
            EditAction::Chisel { slot, octants, restore } => {
                let slot = slot as usize;
                if slot >= HOTBAR_SLOTS {
                    return Err("not a hotbar slot".into());
                }
                let def = p.inventory.get(slot).and_then(|s| self.items.get(s.item));
                let advanced = def.is_some_and(|d| d.chisel_advanced());
                let (family, precise, _) = def.and_then(|d| d.chisel()).ok_or("not a chisel")?;
                let state = world.state(canon).ok_or("chunk not resident")?;
                // The chisels epic lets any chisel make a precise (one
                // octant) cut, as well as its own.
                let precise = precise || (p.effects.chisel_epic && octants.count_ones() == 1);
                let new = chisel::apply(reg.get(current), state, family, precise, advanced, octants, restore)?;
                if restore {
                    // Filling back must not close over a player: the octants
                    // put back are the solid boxes of `Shape(!octants)`.
                    let m = self.movement;
                    let corner = DVec3::new(canon.x as f64, canon.y as f64, canon.z as f64);
                    let filled = Shape::new(family, !octants).unwrap_or(Shape::FULL);
                    let blocked = self.players.values().filter(|o| o.planet == pi).any(|o| {
                        let feet = shape.nearest_lap(corner, o.state.pos);
                        filled.boxes().any(|(lo, hi)| {
                            body_overlaps(&m, feet, corner + DVec3::from_array(lo), corner + DVec3::from_array(hi))
                        })
                    });
                    if blocked {
                        return Err("a player is in the way".into());
                    }
                }
                Ok(Accepted {
                    block: BlockState { shape: new, ..state },
                    effect: Effect::Chiselled { slot },
                })
            }
        }
    }

    /// Whether `shrine` is a warp shrine within this player's reach.
    fn at_shrine(&self, id: u32, shrine: BlockPos) -> bool {
        let Some(p) = self.players.get(&id) else { return false };
        let planet = &self.planets[p.planet];
        let shape = planet.world.shape;
        shape.contains_y(shrine.y) && {
            let canon = shape.wrap_block(shrine);
            p.sent.contains(&canon.chunk())
                && planet.world.block(canon) == Some(self.warp_shrine)
                && in_reach(
                    shape,
                    &self.movement,
                    self.cfg.reach + p.effects.reach_bonus(),
                    &p.state,
                    canon,
                )
        }
    }

    /// Travel from a warp shrine within reach to another planet.
    fn warp(&mut self, id: u32, shrine: BlockPos, dest: u16) {
        if !self.at_shrine(id, shrine) {
            return self.notice(id, "Stand at a warp shrine to travel");
        }
        let Some(p) = self.players.get(&id) else { return };
        let from = &self.planets[p.planet];
        let Some(to) = self.index_of(dest).filter(|&i| self.planets[i].present(self.tick)) else {
            return self.notice(id, "That world is not in the sky");
        };
        if to == p.planet {
            let text = format!("You are already on {}", from.name);
            return self.notice(id, &text);
        }
        self.travel(id, to, None);
    }

    /// Moves a player to planet `to`, standing at `arrival` (a portal's far
    /// side) or, by default, beside its shrine. The client
    /// drops everything of the old planet on `Travel`, so nothing of it is
    /// forgotten one by one; everything after `Travel` on the stream belongs
    /// to the new planet.
    fn travel(&mut self, id: u32, to: usize, arrival: Option<PlayerState>) {
        let arrival = arrival.unwrap_or_else(|| self.planets[to].arrival());
        let Some(p) = self.players.get_mut(&id) else { return };
        let from = p.planet;
        p.planet = to;
        // Only the place changes: health, hunger, food, energy, air and
        // skills go with them (a warp used to heal and feed).
        let arrival = PlayerState {
            pos: arrival.pos,
            vel: DVec3::ZERO,
            on_ground: arrival.on_ground,
            hook: None,
            air: self.planets[to].air,
            ..p.state
        };
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
        p.watching = None;
        if p.portal.take().is_some() {
            p.out.send(&ServerMsg::Portal(None));
        }
        // Closed before `Travel`: after it, everything is the new planet's.
        if let Some(pos) = p.storage.take() {
            p.out.send(&closed_storage(pos));
        }
        p.sent.clear();
        p.wanted.clear();
        p.center = None;
        // The client drops the old planet's far copies on `Travel`.
        p.far_sent.clear();
        p.far_center = None;
        p.near_complete = false;
        p.far_cursor = 0;
        // The client drops the old planet's creatures on `Travel`.
        p.known.clear();
        p.in_view.clear();
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
        let (left, arrived) = (
            format!("{name} left"),
            format!("{name} arrived on {}", self.planets[to].name),
        );
        self.planets[to].players += 1;
        self.server_line(from, left);
        self.server_line(to, arrived);
        self.left_planet(from);
        // A meteor life does not travel.
        let account = self.players[&id].account.clone();
        for m in &mut self.skies[from].meteors {
            m.lives.remove(&account);
        }
        self.players[&id].out.send(&ServerMsg::MeteorLife { life: None });
        // Air they cannot breathe (milestone 18): say so on arrival.
        if let Some(text) = self.players[&id].state.air_warning() {
            self.notice(id, &text);
        }
        self.arrived_in_sky(id);
    }

    /// The storage block at `pos` and its slots, if this player may use it
    /// (the rules of stations).
    fn storage_access(&self, id: u32, pos: BlockPos) -> Result<(BlockPos, usize), Refusal> {
        let (canon, block) = self.block_access(id, pos)?;
        let planet = &self.planets[self.planet_of(id).ok_or("no such player")?];
        match planet.world.registry.get(block).storage {
            0 => Err("not a storage block".into()),
            n => Ok((canon, n as usize)),
        }
    }

    /// The block at `pos` if this player may use what it is: loaded for
    /// them, within reach, and not on someone else's claim.
    fn block_access(&self, id: u32, pos: BlockPos) -> Result<(BlockPos, BlockId), Refusal> {
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
        if !in_reach(
            shape,
            &self.movement,
            self.cfg.reach + p.effects.reach_bonus(),
            &p.state,
            canon,
        ) {
            return Err(Refusal::Told("That is out of reach".into()));
        }
        let block = planet.world.block(canon).ok_or("chunk not resident")?;
        if let Some(c) = planet.claims.at_block(canon)
            && c.owner != p.account
        {
            return Err(ClaimError::OwnedBy(c.owner_name.clone()).into());
        }
        Ok((canon, block))
    }

    fn tell(&self, id: u32, r: Refusal) {
        match r {
            Refusal::Told(text) => self.notice(id, &text),
            Refusal::Quiet(why) => tracing::debug!("refused request from #{id}: {why}"),
        }
    }

    /// Station jobs run on every planet, empty or not: they only look at the
    /// stations with a job running.
    fn step_stations(&mut self) {
        for pi in 0..self.planets.len() {
            let planet = &mut self.planets[pi];
            let env = stations::PlanetEnv::new(&planet.world, self.spark_link);
            let changed = planet.stations.step(self.tick, &self.items, &self.recipes, &env);
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

    fn storage_view(&self, pi: usize, canon: BlockPos, size: usize) -> StorageView {
        StorageView {
            pos: canon,
            slots: self.planets[pi].storage.slots(canon, size),
            closed: false,
        }
    }

    /// Sends a storage block's contents to everyone on its planet with it open.
    fn push_storage(&self, pi: usize, canon: BlockPos, size: usize) {
        let mut frame = None;
        for p in self
            .players
            .values()
            .filter(|p| p.planet == pi && p.storage == Some(canon))
        {
            let f = frame.get_or_insert_with(|| {
                Arc::new(bm_protocol::encode_frame(&ServerMsg::Storage(
                    self.storage_view(pi, canon, size),
                )))
            });
            p.out.send_shared(f.clone());
        }
    }

    /// Closes the storage block a player has open, telling them.
    fn close_storage(&mut self, id: u32) {
        if let Some(p) = self.players.get_mut(&id)
            && let Some(pos) = p.storage.take()
        {
            p.out.send(&closed_storage(pos));
        }
    }

    /// Closes a storage block for everyone who has it open (it is gone).
    fn close_storage_at(&mut self, pi: usize, canon: BlockPos) {
        for p in self
            .players
            .values_mut()
            .filter(|p| p.planet == pi && p.storage == Some(canon))
        {
            p.storage = None;
            p.out.send(&closed_storage(canon));
        }
    }

    /// Closes every open storage block its viewer may no longer use: out of
    /// reach, or now on someone else's claim.
    fn check_open_storage(&mut self) {
        let open: Vec<(u32, BlockPos)> = self
            .players
            .iter()
            .filter_map(|(id, p)| p.storage.map(|s| (*id, s)))
            .collect();
        for (id, pos) in open {
            if self.storage_access(id, pos).is_err() {
                self.close_storage(id);
            }
        }
    }

    fn open_storage(&mut self, id: u32, pos: Option<BlockPos>) {
        let Some(pos) = pos else {
            return self.close_storage(id);
        };
        let Some(pi) = self.planet_of(id) else { return };
        match self.storage_access(id, pos) {
            Ok((canon, size)) => {
                if self.players[&id].storage.is_some_and(|open| open != canon) {
                    self.close_storage(id);
                }
                let view = self.storage_view(pi, canon, size);
                let Some(p) = self.players.get_mut(&id) else { return };
                p.storage = Some(canon);
                p.out.send(&ServerMsg::Storage(view));
                self.tell_tokens(id);
            }
            Err(r) => {
                let shape = self.planets[pi].world.shape;
                let canon = if shape.contains_y(pos.y) {
                    shape.wrap_block(pos)
                } else {
                    pos
                };
                self.tell(id, r);
                if self.players[&id].storage == Some(canon) {
                    self.close_storage(id);
                } else if let Some(p) = self.players.get(&id) {
                    p.out.send(&closed_storage(canon));
                }
            }
        }
    }

    /// A move between the pack and the open storage block, or within either.
    /// Answered with `Slots` for the pack and `Storage` (to every viewer) for
    /// the block; a move that changes nothing is answered with the block's
    /// contents, so a client that guessed can put things back.
    fn storage_move(&mut self, id: u32, pos: BlockPos, from: SlotRef, to: SlotRef, count: u16) {
        let (canon, size) = match self.storage_access(id, pos) {
            Ok(a) => a,
            Err(r) => return self.tell(id, r),
        };
        let Some(p) = self.players.get_mut(&id) else { return };
        if p.storage != Some(canon) {
            return tracing::debug!("#{id} moved items in a storage block it has not open");
        }
        let pi = p.planet;
        let mut slots = self.planets[pi].storage.slots(canon, size);
        let before = p.inventory.clone();
        let stored = slots.clone();
        let moved = storage_move_slots(&self.items, &mut p.inventory.slots, &mut slots, from, to, count);
        send_slots(p, &before);
        if !moved {
            let view = ServerMsg::Storage(self.storage_view(pi, canon, size));
            if let Some(p) = self.players.get(&id) {
                p.out.send(&view);
            }
            return;
        }
        if slots != stored {
            self.planets[pi].storage.set(canon, slots);
            self.push_storage(pi, canon, size);
            let viewers: Vec<u32> = self
                .players
                .iter()
                .filter(|(_, p)| p.planet == pi && p.storage == Some(canon))
                .map(|(id, _)| *id)
                .collect();
            for v in viewers {
                self.tell_tokens(v);
            }
        }
    }

    /// Sends one chat line to every player `hears` picks, encoded once.
    fn send_line(&self, line: ChatLine, hears: impl Fn(&Player) -> bool) {
        let frame = Arc::new(bm_protocol::encode_frame(&ServerMsg::Chat { lines: vec![line] }));
        for p in self.players.values().filter(|p| hears(p)) {
            p.out.send_shared(frame.clone());
        }
    }

    /// A line from the server itself to everyone on a planet.
    fn server_line(&self, pi: usize, text: String) {
        let line = ChatLine {
            channel: Channel::Planet,
            from: String::new(),
            text,
            time: unix_now(),
        };
        self.send_line(line, |p| p.planet == pi);
    }

    fn chat(&mut self, id: u32, channel: Channel, text: String) {
        let Some(text) = chat::clean_text(&text) else { return };
        let tick = self.tick;
        let Some(p) = self.players.get_mut(&id) else { return };
        if let Verdict::TooFast { tell } = p.chat.check(tick) {
            if tell {
                p.out.send(&ServerMsg::Notice {
                    text: "You are talking too fast".into(),
                });
            }
            return;
        }
        let (from, pi, at) = (p.name.clone(), p.planet, p.state.pos);
        let mut line = ChatLine {
            channel: channel.clone(),
            from: from.clone(),
            text,
            time: unix_now(),
        };
        match channel {
            Channel::Local => {
                tracing::info!("chat local {} {from}: {}", self.planets[pi].name, line.text);
                let shape = self.planets[pi].world.shape;
                self.send_line(line, |o| {
                    o.planet == pi && shape.delta(at, o.state.pos).length() <= LOCAL_CHAT_RANGE
                });
            }
            Channel::Planet => {
                tracing::info!("chat planet {} {from}: {}", self.planets[pi].name, line.text);
                self.send_line(line, |o| o.planet == pi);
            }
            Channel::Universe => {
                tracing::info!("chat universe {from}: {}", line.text);
                self.history.push_back(line.clone());
                while self.history.len() > chat::HISTORY {
                    self.history.pop_front();
                }
                self.send_line(line, |_| true);
            }
            Channel::Whisper(to) => {
                let key = to.trim().to_lowercase();
                let Some(target) = self.players.values().find(|o| o.account == key).map(|o| o.name.clone()) else {
                    let shown: String = chat::clean_text(&to)
                        .unwrap_or_default()
                        .chars()
                        .take(MAX_NAME_LEN)
                        .collect();
                    return self.notice(id, &format!("Nobody called {shown} is here"));
                };
                // Whispered text is private: only that it happened is logged.
                tracing::info!("{from} whispered to {target}");
                line.channel = Channel::Whisper(target);
                let sender = self.players[&id].account.clone();
                // The sender gets a copy (once, if they whisper to themselves).
                self.send_line(line, |o| o.account == key || o.account == sender);
            }
        }
    }

    /// Takes reclaimed items into the inventory, as many as fit, at a warp
    /// shrine within reach.
    fn collect_reclaim(&mut self, id: u32, shrine: BlockPos) {
        if !self.at_shrine(id, shrine) {
            return self.notice(id, "Stand at a warp shrine to collect your reclaimed items");
        }
        let Some(p) = self.players.get_mut(&id) else { return };
        let before = p.inventory.clone();
        let had = !self.reclaims.items(&p.account).is_empty();
        let moved = self.reclaims.collect(&p.account, &mut p.inventory, &self.items);
        send_slots(p, &before);
        p.out.send(&ServerMsg::Reclaim {
            items: self.reclaims.items(&p.account).to_vec(),
        });
        self.tell_tokens(id);
        if !had {
            self.notice(id, "Nothing waits for you in reclaim");
        } else if !moved {
            self.notice(id, "Your inventory is full");
        }
    }

    /// Sends an account's reclaim to them, if they are online.
    fn push_reclaim(&self, account: &str) {
        if let Some(p) = self.players.values().find(|p| p.account == account) {
            p.out.send(&ServerMsg::Reclaim {
                items: self.reclaims.items(account).to_vec(),
            });
        }
    }

    /// Checks claims for lapsing, then moves the lapses on: each keeps the
    /// chunk of its next plot loaded and its generated terrain made, and
    /// every `LAPSE_PLOT_EVERY` ticks one plot turns.
    fn step_lapses(&mut self) {
        if !self.lapses_checked || self.tick % LAPSE_CHECK_EVERY == 0 {
            self.lapses_checked = true;
            self.find_lapses();
        }
        if self.lapses.is_empty() {
            return;
        }
        let tick = self.tick;
        for job in &mut self.lapses {
            let world = &mut self.planets[job.planet].world;
            if let Some(c) = job.next_chunk()
                && job.chunk != Some(c)
            {
                if let Some(old) = job.chunk.take() {
                    self.unpin_after_save.push((job.planet, old));
                }
                world.pin(c);
                job.chunk = Some(c);
                job.make_reference(world.generator(), c);
            }
            if let Some(c) = job.chunk {
                world.request(c, tick);
            }
            job.receive_reference();
        }
        if tick % LAPSE_PLOT_EVERY != 0 {
            return;
        }
        let ready = self.lapses.iter().position(|job| {
            job.plots.is_empty()
                || (job.reference.is_some() && job.chunk.is_some_and(|c| self.planets[job.planet].world.is_loaded(c)))
        });
        if let Some(i) = ready {
            self.lapse_plot(i);
        }
    }

    /// Starts a lapse for every claim whose owner has been away longer than
    /// the keep time. Owners never seen are counted from now.
    fn find_lapses(&mut self) {
        let now = unix_now();
        let keep = self.cfg.claim_keep_secs();
        let online: HashSet<&str> = self.players.values().map(|p| p.account.as_str()).collect();
        let mut start = Vec::new();
        for (pi, planet) in self.planets.iter().enumerate() {
            for c in planet.claims.all() {
                if online.contains(c.owner.as_str()) || self.lapses.iter().any(|l| l.planet == pi && l.claim == c.id) {
                    continue;
                }
                let seen = *self.last_seen.entry(c.owner.clone()).or_insert(now);
                let away = now.saturating_sub(seen);
                if away >= keep {
                    start.push(Lapse::new(
                        pi,
                        c.id,
                        c.owner.clone(),
                        away,
                        c.plots.iter().copied().collect(),
                        PlotPos::of(c.stone, planet.world.shape),
                    ));
                    tracing::info!(
                        "claim {} of {} on {} ({} plots) lapses: away {} days",
                        c.id,
                        c.owner_name,
                        planet.name,
                        c.plots.len(),
                        away / 86_400
                    );
                }
            }
        }
        self.lapses.extend(start);
    }

    /// Turns the next plot of lapse `i` to ash, and finishes the lapse when
    /// none are left.
    fn lapse_plot(&mut self, i: usize) {
        // Portals on the plot go first: unlinked, surfaces back to conduits
        // (which turn to ash below), their fuel to the owner's reclaim.
        let job = &self.lapses[i];
        let (pi, claim, owner) = (job.planet, job.claim, job.owner.clone());
        if let Some(&plot) = job.plots.front()
            && self.planets[pi]
                .claims
                .get(claim)
                .is_some_and(|c| c.plots.contains(&plot))
            && job.reference.is_some()
        {
            let fuel = self.lapse_portals(pi, plot);
            if fuel > 0 {
                let max = self.items.get(self.wayshard).map_or(1, |d| d.stack) as u32;
                let stacks =
                    (0..fuel.div_ceil(max)).map(|k| ItemStack::new(self.wayshard, (fuel - k * max).min(max) as u16));
                self.reclaims.add(&owner, stacks);
                self.push_reclaim(&owner);
                let job = &mut self.lapses[i];
                job.begun = true;
                job.reclaimed = true;
            }
        }
        let job = &mut self.lapses[i];
        let Some(plot) = job.plots.pop_front() else {
            return self.finish_lapse(i);
        };
        let pi = job.planet;
        let planet = &mut self.planets[pi];
        let mut changed = Vec::new();
        let mut items = Vec::new();
        let mut emptied = Vec::new();
        // A plot the owner released (or a claim gone) is left alone.
        let held = planet.claims.get(job.claim).is_some_and(|c| c.plots.contains(&plot));
        if let (true, Some(reference)) = (held, &job.reference) {
            let registry = planet.world.registry.clone();
            let water = planet.world.water;
            let origin = plot.origin();
            for y in 0..bm_core::PLOT_SIZE {
                for z in 0..bm_core::PLOT_SIZE {
                    for x in 0..bm_core::PLOT_SIZE {
                        let p = origin.offset(x, y, z);
                        if planet.storage.has_contents(p) {
                            items.extend(planet.storage.take(p));
                            emptied.push(p);
                        }
                        if planet.stations.has_contents(p) {
                            items.extend(planet.stations.take(p, &self.items, &self.recipes));
                        }
                        let Some(current) = planet.world.state(p) else { continue };
                        let generated = reference.state(p.local());
                        if let Some(ash) = lapse::ash_for(
                            current,
                            generated,
                            planet.plaza_state(p),
                            &registry,
                            &self.colours,
                            self.ash,
                            water,
                        ) {
                            planet.world.set_state(p, ash);
                            self.damage[pi].clear(p, self.tick);
                            changed.push((p, ash));
                        }
                    }
                }
            }
        }
        job.begun |= !changed.is_empty() || !items.is_empty();
        job.reclaimed |= !items.is_empty();
        let owner = job.owner.clone();
        let done = job.plots.is_empty();
        let chunk = plot.origin().chunk();
        for o in self
            .players
            .values()
            .filter(|o| o.planet == pi && o.sent.contains(&chunk))
        {
            for (pos, block) in &changed {
                o.out.send(&ServerMsg::BlockChanged {
                    pos: *pos,
                    block: *block,
                });
            }
        }
        for p in emptied {
            self.close_storage_at(pi, p);
        }
        if !items.is_empty() {
            self.reclaims.add(&owner, items);
            self.push_reclaim(&owner);
        }
        if done {
            self.finish_lapse(i);
        }
    }

    /// Removes a lapsed claim and says so.
    fn finish_lapse(&mut self, i: usize) {
        let job = self.lapses.remove(i);
        if let Some(c) = job.chunk {
            self.unpin_after_save.push((job.planet, c));
        }
        let pi = job.planet;
        let planet = &mut self.planets[pi];
        let Some(claim) = planet.claims.get(job.claim).cloned() else {
            return;
        };
        planet.claims.remove_by_stone(claim.stone);
        let name = planet.name.clone();
        for o in self.players.values().filter(|o| o.planet == pi) {
            o.out.send(&ServerMsg::ClaimRemoved { id: claim.id });
        }
        tracing::info!(
            "claim {} of {} on {name} has lapsed after {} days away; {} plots turned to ash",
            claim.id,
            claim.owner_name,
            job.away / 86_400,
            claim.plots.len()
        );
        self.server_line(pi, format!("A claim on {name} has crumbled to ash"));
        let mut text = format!(
            "Your claim on {name} lapsed after {} away.",
            lapse::months_away(job.away)
        );
        if job.reclaimed {
            text.push_str(" Its stored items wait for you at any warp shrine.");
        }
        match self
            .players
            .iter()
            .find(|(_, p)| p.account == job.owner)
            .map(|(id, _)| *id)
        {
            Some(id) => self.notice(id, &text),
            None => self.reclaims.add_notice(&job.owner, text),
        }
    }

    /// Stops every lapse of an account's claims (they are back). Returns
    /// notices for those that had already begun.
    fn stop_lapses_of(&mut self, account: &str) -> Vec<String> {
        let mut notices = Vec::new();
        let mut kept = Vec::new();
        for job in std::mem::take(&mut self.lapses) {
            if job.owner != account {
                kept.push(job);
                continue;
            }
            if let Some(c) = job.chunk {
                self.unpin_after_save.push((job.planet, c));
            }
            let name = &self.planets[job.planet].name;
            tracing::info!("the lapse of claim {} on {name} stopped: its owner is back", job.claim);
            if job.begun {
                notices.push(format!(
                    "Your claim on {name} had begun to crumble to ash. The rest of it is yours again."
                ));
            }
        }
        self.lapses = kept;
        notices
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
        let data = |planet: usize, state: PlayerState, inv: &Inventory, c: &Character| PlayerData {
            planet: planets[planet].id,
            state: Some(state.into()),
            inventory: save_slots(items, inv),
            last_seen: now,
            character: c.clone().into(),
            hurt: state.hurt,
            food: SavedFood::from(&state),
            air_used: state.air_used,
        };
        let mut players: Vec<(String, PlayerData)> = self
            .players
            .values()
            .map(|p| (p.account.clone(), data(p.planet, p.state, &p.inventory, &p.character)))
            .collect();
        let mut done: Vec<Sender<()>> = done.into_iter().collect();
        let mut written = None;
        for (account, d) in self.departed.iter_mut().filter(|(_, d)| d.written.is_none()) {
            players.push((account.clone(), data(d.planet, d.state, &d.inventory, &d.character)));
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
        let mut far = Vec::new();
        let mut stations = Vec::new();
        let mut claims = Vec::new();
        let mut storage = Vec::new();
        let mut crops = Vec::new();
        for p in &mut self.planets {
            p.world.forget_saved();
            let id = p.id;
            let dirty = p.world.take_dirty(&saved);
            for (c, bytes) in &dirty {
                p.far.rebuild(*c, bytes.clone());
            }
            chunks.extend(dirty.into_iter().map(|(c, b)| (id, c, b)));
            far.extend(p.far.take_unsaved().into_iter().map(|(c, b)| (id, c, b)));
            stations.extend(
                p.stations
                    .take_dirty(&self.items, &self.recipes)
                    .into_iter()
                    .map(|(pos, s)| (id, pos, s)),
            );
            claims.extend(p.claims.take_dirty());
            crops.extend(p.farm.take_dirty(&self.crops).into_iter().map(|(pos, c)| (id, pos, c)));
            storage.extend(
                p.storage
                    .take_dirty(&self.items)
                    .into_iter()
                    .map(|(pos, s)| (id, pos, s)),
            );
        }
        let (portals, next_portal) = self.portals.take_dirty();
        // Their ash is in this batch now, with what was reclaimed from them.
        for (pi, c) in self.unpin_after_save.drain(..) {
            self.planets[pi].world.unpin(c);
        }
        let mut skies = Vec::new();
        for (pi, sky) in self.skies.iter_mut().enumerate() {
            if std::mem::take(&mut sky.dirty) {
                skies.push((self.planets[pi].id, sky.saved()));
            }
        }
        let discoveries = self
            .discoveries_dirty
            .drain()
            .filter_map(|a| self.discoveries.get(&a).map(|d| (a, d.clone())))
            .collect();
        let batch = WriteBatch {
            chunks,
            far,
            players,
            stations,
            claims,
            storage,
            crops,
            reclaim: self.reclaims.take_dirty(&self.items),
            portals,
            next_portal,
            skies,
            discoveries,
            clock: Some(self.tick),
            forget_planets: std::mem::take(&mut self.forget_planets),
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

/// When each claim owner was last seen, from the save. An owner never seen
/// (`last_seen` 0, kept from before it was recorded, or no character at all)
/// is counted from now, and that is saved, so an old claim gets the full
/// keep time from the day this server first notices it.
fn claim_owners_last_seen(store: &Store, planets: &[Planet], start: u16) -> HashMap<String, u64> {
    let now = unix_now();
    let owners: BTreeSet<&str> = planets
        .iter()
        .flat_map(|p| p.claims.all().map(|c| c.owner.as_str()))
        .collect();
    let mut seen = HashMap::new();
    let mut stamped = Vec::new();
    for owner in owners {
        let data = match store.player(owner) {
            Ok(Some(d)) if d.last_seen > 0 => {
                seen.insert(owner.to_string(), d.last_seen);
                continue;
            }
            Ok(Some(d)) => d,
            Ok(None) => PlayerData {
                planet: start,
                ..Default::default()
            },
            Err(e) => {
                // Not written over: counted from now, in memory only.
                tracing::error!("reading {owner}: {e:#}; their claims are kept from now");
                seen.insert(owner.to_string(), now);
                continue;
            }
        };
        seen.insert(owner.to_string(), now);
        stamped.push((owner.to_string(), PlayerData { last_seen: now, ..data }));
    }
    if !stamped.is_empty() {
        tracing::info!(
            "{} claim owners had never been seen; their claims are kept from now",
            stamped.len()
        );
        if let Err(e) = store.write(&WriteBatch {
            players: stamped,
            ..Default::default()
        }) {
            tracing::error!("saving when claim owners were first seen: {e:#}");
        }
    }
    seen
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

#[cfg(test)]
mod storage_move_tests {
    use super::*;
    use bm_core::BlockRegistry;
    use bm_protocol::SlotRef::{Inventory as I, Storage as S};

    #[test]
    fn moves_cross_between_pack_and_storage_all_or_nothing() {
        let items = ItemRegistry::builtin(&BlockRegistry::builtin());
        let stone = items.id("stone").unwrap();
        let sand = items.id("sand").unwrap();
        let st = |item, count| Some(ItemStack::new(item, count));
        let mut inv = vec![None; 40];
        let mut chest = vec![None; 20];
        inv[0] = st(stone, 30);

        // Split into the chest, merge more onto it, then take some back.
        assert!(storage_move_slots(&items, &mut inv, &mut chest, I(0), S(19), 10));
        assert_eq!((inv[0], chest[19]), (st(stone, 20), st(stone, 10)));
        assert!(storage_move_slots(&items, &mut inv, &mut chest, I(0), S(19), 500));
        assert_eq!((inv[0], chest[19]), (None, st(stone, 30)));
        assert!(storage_move_slots(&items, &mut inv, &mut chest, S(19), I(5), 5));
        assert_eq!((inv[5], chest[19]), (st(stone, 5), st(stone, 25)));
        // Within the chest, and within the pack.
        assert!(storage_move_slots(&items, &mut inv, &mut chest, S(19), S(0), 25));
        assert_eq!((chest[0], chest[19]), (st(stone, 25), None));
        assert!(storage_move_slots(&items, &mut inv, &mut chest, I(5), I(6), 2));
        assert_eq!((inv[5], inv[6]), (st(stone, 3), st(stone, 2)));
        // A whole-stack swap across.
        inv[7] = st(sand, 4);
        assert!(storage_move_slots(&items, &mut inv, &mut chest, I(7), S(0), 4));
        assert_eq!((inv[7], chest[0]), (st(stone, 25), st(sand, 4)));
        // Refused, changing nothing: a part of a stack onto another item,
        // bad indices either side, the same slot, nothing in the source.
        let (i0, c0) = (inv.clone(), chest.clone());
        assert!(!storage_move_slots(&items, &mut inv, &mut chest, I(7), S(0), 3));
        assert!(!storage_move_slots(&items, &mut inv, &mut chest, I(7), S(20), 1));
        assert!(!storage_move_slots(&items, &mut inv, &mut chest, S(200), I(9), 1));
        assert!(!storage_move_slots(&items, &mut inv, &mut chest, I(40), S(1), 1));
        assert!(!storage_move_slots(&items, &mut inv, &mut chest, S(0), S(0), 1));
        assert!(!storage_move_slots(&items, &mut inv, &mut chest, S(1), I(9), 1));
        assert!(!storage_move_slots(&items, &mut inv, &mut chest, I(7), S(1), 0));
        assert_eq!((inv, chest), (i0, c0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worn_chisels_survive_loading_and_spent_ones_go() {
        let blocks = bm_core::BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let chisel = items.id("square_chisel").unwrap();
        let durability = items.get(chisel).unwrap().durability().unwrap();
        let mut inv = Inventory::default();
        inv.slots[0] = Some(ItemStack {
            wear: 3,
            ..ItemStack::new(chisel, 1)
        });
        inv.slots[1] = Some(ItemStack {
            wear: durability,
            ..ItemStack::new(chisel, 1)
        });
        sanitize(&mut inv, &items);
        assert_eq!(inv.get(0).map(|s| s.wear), Some(3), "a worn chisel is kept");
        assert_eq!(inv.get(1), None, "a spent one is not");
    }

    #[test]
    fn filling_back_checks_the_body_against_each_octant() {
        let m = MoveConfig::default();
        let feet = DVec3::new(0.5, 1.0, 0.5);
        // Standing on top of the block's lower half: touching is not overlapping.
        assert!(!body_overlaps(
            &m,
            feet,
            DVec3::new(0.0, 0.5, 0.0),
            DVec3::new(1.0, 1.0, 1.0)
        ));
        assert!(body_overlaps(
            &m,
            feet,
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::new(0.5, 1.5, 0.5)
        ));
        assert!(!body_overlaps(
            &m,
            feet,
            DVec3::new(1.0, 1.0, 0.0),
            DVec3::new(1.5, 1.5, 0.5)
        ));
    }
}
