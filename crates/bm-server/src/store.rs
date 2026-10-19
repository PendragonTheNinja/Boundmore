//! Persistence (ADR-0005, ADR-0006).
//!
//! One crash-safe embedded database per universe (redb: ACID, MVCC),
//! `universe.redb`. Per-planet tables are keyed by planet id first (chunks,
//! stations, the planet records); per-account tables by account (accounts,
//! player data, which records the planet the player is on). Only chunks
//! players have changed are stored; everything else is regenerated from the
//! planet's seed, so an untouched planet costs a few hundred bytes on disk.
//!
//! Reads happen on loader threads concurrently with the single writer thread.
//! Writes are batched: the game loop hands over every dirty chunk, player,
//! station and claim of every planet every few seconds, and they commit in one
//! transaction, so a block broken into an inventory is saved together with
//! that inventory, on any planet. Accounts are the exception: they are
//! created from the connection tasks, each in its own transaction, because a
//! name must be taken exactly once.
//!
//! Items are saved by name, never by `ItemId`: ids are positions in the
//! catalogue, and adding one block shifts every item after it.

use anyhow::{Context, Result, bail};
use bm_core::{
    BlockPos, CHUNK_FORMAT_VERSION, ChunkPos, Colour, DayCycle, Inventory, ItemRegistry, ItemStack, PlanetShape,
    PlayerState, PlotPos, Station,
};
use bm_worldgen::{GENERATOR_VERSION, PlanetGenConfig, PlanetGenConfigV1, PlanetKinds};
use crossbeam_channel::{Receiver, Sender};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, WriteTransaction};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// "universe" -> [`UniverseMeta`], "clock" -> the universe tick.
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
/// Planet id -> [`SavedPlanet`].
const PLANETS: TableDefinition<u16, &[u8]> = TableDefinition::new("planets");
/// Planet id and chunk position ([`chunk_key`]) -> encoded chunk.
const CHUNKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("planet_chunks");
/// Lowercased account name -> [`AccountRecord`].
const ACCOUNTS: TableDefinition<&str, &[u8]> = TableDefinition::new("accounts");
/// Lowercased account name -> [`PlayerData`].
const PLAYER_DATA: TableDefinition<&str, &[u8]> = TableDefinition::new("player_data");
/// Planet id and canonical block position ([`block_key`]) -> [`SavedStation`].
const STATIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("planet_stations");
/// Claim id (unique in the universe) -> [`SavedClaim`].
const CLAIMS: TableDefinition<u32, &[u8]> = TableDefinition::new("claims");
/// Planet id and canonical block position ([`block_key`]) -> the slots of a
/// storage block with something in it ([`SavedSlots`]). Added in
/// M5 without a version bump: a new table changes nothing already saved, and
/// `Store::open` creates it in an older save.
const STORAGE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("planet_storage");

/// Save version 1 and 2 tables, read only to upgrade, then deleted.
const V1_PLAYERS: TableDefinition<&str, &[u8]> = TableDefinition::new("players");
const V2_CHUNKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("chunks");
const V2_STATIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("stations");

/// Bump when the layout of anything below changes, and teach
/// [`Store::upgrade`] the step from the previous version.
///
/// 3 (M4): one file per universe, tables keyed by planet; player data records
/// the planet; claim ids are unique across the universe.
pub const SAVE_VERSION: u32 = 3;

/// The universe file in the data directory.
pub const UNIVERSE_FILE: &str = "universe.redb";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UniverseMeta {
    pub save_version: u32,
    pub chunk_format: u8,
}

/// A planet as saved: everything that decides its terrain, fixed when it was
/// created. The config file's planet list only creates planets; afterwards
/// these win.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedPlanet {
    pub generator_version: u32,
    pub name: String,
    /// Its kind's name in `assets/planets.toml` (look and tier come from there).
    pub kind: String,
    pub shape: PlanetShape,
    pub generation: PlanetGenConfig,
    pub day: DayCycle,
    /// Where the server put its warp shrine; None until it has.
    pub shrine: Option<BlockPos>,
}

/// [`SavedPlanet`] on disk. The generation settings are TOML text, not
/// postcard: they come from another crate and gain fields with new planet
/// kinds (`#[serde(default)]`), which postcard could not read back. TOML has
/// no unsigned 64-bit integers, so the seed is kept beside them.
#[derive(Serialize, Deserialize)]
struct PlanetRecord {
    generator_version: u32,
    name: String,
    kind: String,
    shape: PlanetShape,
    seed: u64,
    generation: String,
    day: DayCycle,
    shrine: Option<BlockPos>,
}

impl SavedPlanet {
    fn encode(&self) -> Result<Vec<u8>> {
        let generation = toml::to_string(&PlanetGenConfig {
            seed: 0,
            ..self.generation.clone()
        })
        .context("generation settings")?;
        Ok(postcard::to_stdvec(&PlanetRecord {
            generator_version: self.generator_version,
            name: self.name.clone(),
            kind: self.kind.clone(),
            shape: self.shape,
            seed: self.generation.seed,
            generation,
            day: self.day,
            shrine: self.shrine,
        })?)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let r: PlanetRecord = postcard::from_bytes(bytes).context("planet record is corrupt")?;
        let mut generation: PlanetGenConfig =
            toml::from_str(&r.generation).with_context(|| format!("planet `{}`: generation settings", r.name))?;
        generation.seed = r.seed;
        Ok(Self {
            generator_version: r.generator_version,
            name: r.name,
            kind: r.kind,
            shape: r.shape,
            generation,
            day: r.day,
            shrine: r.shrine,
        })
    }
}

/// An account as stored. `hash` is an Argon2id PHC string; the password
/// itself is never stored.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountRecord {
    /// The name as registered (what other players see).
    pub name: String,
    pub hash: String,
}

impl std::fmt::Debug for AccountRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountRecord")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// One stack as saved: by item name. A coloured stack saves its colour id
/// after a `#` (`stone#17`); a plain name is natural colour, which is how
/// every stack saved before colours existed reads (ADR-0007).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedStack {
    pub item: String,
    pub count: u16,
    pub wear: u16,
}

impl SavedStack {
    pub fn new(items: &ItemRegistry, s: &ItemStack) -> Option<Self> {
        let name = &items.get(s.item)?.name;
        Some(Self {
            item: if s.colour == Colour::NATURAL {
                name.clone()
            } else {
                format!("{name}#{}", s.colour.0)
            },
            count: s.count,
            wear: s.wear,
        })
    }

    /// The stack in this catalogue, if the item still exists.
    pub fn resolve(&self, items: &ItemRegistry) -> Option<ItemStack> {
        let (name, colour) = match self.item.split_once('#') {
            Some((name, c)) => (name, Colour(c.parse().ok()?)),
            None => (self.item.as_str(), Colour::NATURAL),
        };
        let id = items.id(name).ok()?;
        Some(ItemStack {
            item: id,
            count: self.count,
            wear: self.wear,
            colour,
        })
    }
}

/// A row of slots as saved (an inventory, a station's output, a storage block).
pub type SavedSlots = Vec<Option<SavedStack>>;

pub fn save_slots(items: &ItemRegistry, inv: &Inventory) -> SavedSlots {
    inv.slots
        .iter()
        .map(|s| s.as_ref().and_then(|s| SavedStack::new(items, s)))
        .collect()
}

/// Slots back into stacks. Items no longer in the catalogue are dropped with
/// a warning (nothing else can be done with them); the caller sanitizes.
pub fn load_slots(items: &ItemRegistry, slots: &[Option<SavedStack>], what: &str) -> Vec<Option<ItemStack>> {
    slots
        .iter()
        .map(|s| {
            let s = s.as_ref()?;
            let stack = s.resolve(items);
            if stack.is_none() {
                tracing::warn!("{what}: {} × `{}` is no longer an item; dropped", s.count, s.item);
            }
            stack
        })
        .collect()
}

/// Everything saved about one account's character.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerData {
    /// The planet they are on (its id).
    pub planet: u16,
    /// Where on it. None: never placed there (starts at its shrine).
    pub state: Option<PlayerState>,
    pub inventory: Vec<Option<SavedStack>>,
    /// Unix seconds when the player was last connected; 0 if never recorded.
    /// Claims lapse after `claim_keep_days` without a visit (not yet enforced).
    pub last_seen: u64,
}

/// A queued station job, saved with its recipe spelled out so it can be
/// matched again (or refunded) after the recipe list changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedJob {
    pub output: String,
    pub output_count: u16,
    pub inputs: Vec<(String, u16)>,
    pub remaining: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedStation {
    pub kind: Station,
    pub jobs: Vec<SavedJob>,
    /// Universe tick the craft in progress began.
    pub started: u64,
    pub output: Vec<Option<SavedStack>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedClaim {
    /// The planet it is on.
    pub planet: u16,
    /// Lowercased account name.
    pub owner: String,
    /// The owner's name as registered.
    pub owner_name: String,
    pub stone: BlockPos,
    pub plots: Vec<PlotPos>,
}

/// The layouts save version 2 wrote, kept only to read them for the upgrade.
mod v2 {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct PlanetMeta {
        pub save_version: u32,
        pub generator_version: u32,
        pub chunk_format: u8,
        pub name: String,
        pub shape: PlanetShape,
        /// Postcard is positional: version 2 saved generator 1's settings.
        pub generation: PlanetGenConfigV1,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct PlayerData {
        pub state: Option<PlayerState>,
        pub inventory: Vec<Option<SavedStack>>,
        pub last_seen: u64,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct SavedClaim {
        pub owner: String,
        pub owner_name: String,
        pub stone: BlockPos,
        pub plots: Vec<PlotPos>,
    }
}

pub use v2::PlanetMeta as V2PlanetMeta;

pub fn chunk_key(planet: u16, p: ChunkPos) -> [u8; 14] {
    let mut k = [0u8; 14];
    k[0..2].copy_from_slice(&planet.to_be_bytes());
    k[2..6].copy_from_slice(&p.x.to_be_bytes());
    k[6..10].copy_from_slice(&p.y.to_be_bytes());
    k[10..14].copy_from_slice(&p.z.to_be_bytes());
    k
}

fn block_key(planet: u16, p: BlockPos) -> [u8; 26] {
    let mut k = [0u8; 26];
    k[0..2].copy_from_slice(&planet.to_be_bytes());
    k[2..10].copy_from_slice(&p.x.to_be_bytes());
    k[10..18].copy_from_slice(&p.y.to_be_bytes());
    k[18..26].copy_from_slice(&p.z.to_be_bytes());
    k
}

fn block_from_key(k: &[u8]) -> Option<BlockPos> {
    let part = |i: usize| Some(i64::from_be_bytes(k.get(i..i + 8)?.try_into().ok()?));
    (k.len() == 26).then_some(())?;
    Some(BlockPos::new(part(2)?, part(10)?, part(18)?))
}

/// Every key of one planet in a table keyed by [`chunk_key`] or [`block_key`].
fn planet_range(planet: u16) -> (Vec<u8>, Vec<u8>) {
    let lo = planet.to_be_bytes().to_vec();
    let hi = match planet.checked_add(1) {
        Some(n) => n.to_be_bytes().to_vec(),
        None => vec![0xff; 27],
    };
    (lo, hi)
}

/// What [`find_legacy_planet`] found.
#[derive(Debug, PartialEq)]
pub enum Legacy {
    None,
    One(PathBuf),
}

/// A single-planet save (version 1 or 2: `<planet name>.redb` with a `planet`
/// meta record) in the data directory, to be upgraded into the universe. If
/// several are there, the one named after `prefer` is chosen; otherwise
/// several is an error (which one should become the universe is the owner's
/// call).
pub fn find_legacy_planet(dir: &Path, prefer: &str) -> Result<Legacy> {
    let mut found = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(Legacy::None),
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "redb") || path.file_name().is_some_and(|n| n == UNIVERSE_FILE) {
            continue;
        }
        let db = match Database::open(&path) {
            Ok(db) => db,
            Err(e) => {
                tracing::warn!("{} is not a readable save ({e}); ignored", path.display());
                continue;
            }
        };
        let tx = db.begin_read()?;
        let is_planet = match tx.open_table(META) {
            Ok(t) => t.get("planet")?.is_some(),
            Err(_) => false,
        };
        if is_planet {
            found.push(path);
        }
    }
    if let Some(p) = found
        .iter()
        .find(|p| p.file_name().is_some_and(|n| *n == *legacy_file_name(prefer)))
    {
        return Ok(Legacy::One(p.clone()));
    }
    match found.len() {
        0 => Ok(Legacy::None),
        1 => Ok(Legacy::One(found.remove(0))),
        _ => bail!(
            "several old planet saves are in {}: {}; move all but the one to keep elsewhere, and it becomes the \
             first planet of the universe",
            dir.display(),
            found
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The file a single-planet server (save version 2 and before) used.
pub fn legacy_file_name(planet: &str) -> String {
    let s: String = planet
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("{s}.redb")
}

/// What the upgrade carried over, for the log and for tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpgradeReport {
    pub chunks: usize,
    pub players: usize,
    pub claims: usize,
    pub stations: usize,
}

#[derive(Clone)]
pub struct Store {
    db: Arc<Database>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Database::create(path).with_context(|| format!("opening {}", path.display()))?;
        // Create the tables so readers never see "table does not exist".
        let tx = db.begin_write()?;
        tx.open_table(META)?;
        tx.open_table(PLANETS)?;
        tx.open_table(CHUNKS)?;
        tx.open_table(ACCOUNTS)?;
        tx.open_table(PLAYER_DATA)?;
        tx.open_table(STATIONS)?;
        tx.open_table(CLAIMS)?;
        tx.open_table(STORAGE)?;
        tx.commit()?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Opens the universe in `dir`. If there is none yet but an old
    /// single-planet save is there, the universe is made from a copy of it
    /// (the old file is left untouched, as a backup) and upgraded in place.
    pub fn open_universe(dir: &Path, prefer: &str, kinds: &PlanetKinds) -> Result<(Self, Option<UpgradeReport>)> {
        let path = dir.join(UNIVERSE_FILE);
        if !path.exists()
            && let Legacy::One(old) = find_legacy_planet(dir, prefer)?
        {
            tracing::info!(
                "no universe yet; making one from the old planet save {} (left as it is, as a backup)",
                old.display()
            );
            let tmp = dir.join("universe.redb.upgrading");
            std::fs::copy(&old, &tmp).with_context(|| format!("copying {}", old.display()))?;
            std::fs::rename(&tmp, &path).context("moving the copied save into place")?;
        }
        let store = Self::open(&path)?;
        let report = store.universe(kinds)?;
        Ok((store, report))
    }

    /// Checks the universe's versions, writing them if it is new; a planet
    /// save of an older layout is upgraded in place. Refuses a save from a
    /// newer server.
    pub fn universe(&self, kinds: &PlanetKinds) -> Result<Option<UpgradeReport>> {
        let tx = self.db.begin_read()?;
        let meta = tx.open_table(META)?;
        let universe = meta.get("universe")?.map(|v| v.value().to_vec());
        let old_planet = meta.get("planet")?.map(|v| v.value().to_vec());
        drop(meta);
        drop(tx);
        match (universe, old_planet) {
            (Some(bytes), _) => {
                let u: UniverseMeta = postcard::from_bytes(&bytes).context("universe metadata is corrupt")?;
                if u.save_version > SAVE_VERSION {
                    bail!(
                        "universe save version {} is newer than this server's {}; update the server",
                        u.save_version,
                        SAVE_VERSION
                    );
                }
                if u.save_version < SAVE_VERSION {
                    bail!("no upgrade from universe save version {}", u.save_version);
                }
                if u.chunk_format != CHUNK_FORMAT_VERSION {
                    bail!(
                        "universe chunk format {} is not {}",
                        u.chunk_format,
                        CHUNK_FORMAT_VERSION
                    );
                }
                Ok(None)
            }
            (None, Some(bytes)) => self.upgrade(&bytes, kinds).map(Some),
            (None, None) => {
                let tx = self.db.begin_write()?;
                tx.open_table(META)?.insert(
                    "universe",
                    postcard::to_stdvec(&UniverseMeta {
                        save_version: SAVE_VERSION,
                        chunk_format: CHUNK_FORMAT_VERSION,
                    })?
                    .as_slice(),
                )?;
                tx.commit()?;
                Ok(None)
            }
        }
    }

    /// Brings a single-planet save (version 1 or 2) up to [`SAVE_VERSION`]
    /// in one transaction: a crash mid-upgrade leaves the old layout intact.
    fn upgrade(&self, meta_bytes: &[u8], kinds: &PlanetKinds) -> Result<UpgradeReport> {
        let meta: v2::PlanetMeta = postcard::from_bytes(meta_bytes).context("planet metadata is corrupt")?;
        if meta.save_version > 2 {
            bail!(
                "planet save version {} has no `universe` record; it is corrupt",
                meta.save_version
            );
        }
        // Version 2 saves were all made by generator 1, whose terrain
        // generator 2 reproduces block for block from the converted settings
        // (`PlanetGenConfigV1`), so every edit is kept.
        if meta.generator_version != 1 {
            bail!(
                "planet `{}` was generated by generator v{}, which this server (v{}) cannot reproduce; it cannot be \
                 opened (start a fresh universe, or use the server that made it)",
                meta.name,
                meta.generator_version,
                GENERATOR_VERSION
            );
        }
        if meta.chunk_format != CHUNK_FORMAT_VERSION {
            bail!(
                "planet chunk format {} is not {}",
                meta.chunk_format,
                CHUNK_FORMAT_VERSION
            );
        }
        let from = meta.save_version;
        let tx = self.db.begin_write()?;
        if from == 1 {
            upgrade_1_to_2(&tx)?;
        }
        let report = upgrade_2_to_3(&tx, &meta, kinds)?;
        tx.commit()?;
        tracing::info!(
            "upgraded planet `{}` from save version {from} to {SAVE_VERSION}: it is planet 0 of the universe",
            meta.name
        );
        Ok(report)
    }

    /// The saved planet with this id.
    pub fn planet(&self, id: u16) -> Result<Option<SavedPlanet>> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(PLANETS)?;
        match t.get(id)? {
            Some(v) => Ok(Some(SavedPlanet::decode(v.value())?)),
            None => Ok(None),
        }
    }

    /// Ids of every saved planet.
    pub fn planet_ids(&self) -> Result<Vec<u16>> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(PLANETS)?;
        let mut out = Vec::new();
        for row in t.iter()? {
            out.push(row?.0.value());
        }
        Ok(out)
    }

    pub fn put_planet(&self, id: u16, planet: &SavedPlanet) -> Result<()> {
        let tx = self.db.begin_write()?;
        tx.open_table(PLANETS)?.insert(id, planet.encode()?.as_slice())?;
        tx.commit()?;
        Ok(())
    }

    pub fn chunk(&self, planet: u16, p: ChunkPos) -> Result<Option<Vec<u8>>> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(CHUNKS)?;
        Ok(t.get(chunk_key(planet, p).as_slice())?.map(|v| v.value().to_vec()))
    }

    /// The account with this (lowercased) key.
    pub fn account(&self, key: &str) -> Result<Option<AccountRecord>> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(ACCOUNTS)?;
        match t.get(key)? {
            Some(v) => Ok(Some(
                postcard::from_bytes(v.value()).context("account record is corrupt")?,
            )),
            None => Ok(None),
        }
    }

    /// Creates an account unless the key is taken (checked in the same
    /// transaction, so two registrations racing for one name cannot both win).
    /// Returns whether it was created.
    pub fn create_account(&self, key: &str, record: &AccountRecord) -> Result<bool> {
        let tx = self.db.begin_write()?;
        {
            let mut t = tx.open_table(ACCOUNTS)?;
            if t.get(key)?.is_some() {
                return Ok(false);
            }
            t.insert(key, postcard::to_stdvec(record)?.as_slice())?;
        }
        tx.commit()?;
        Ok(true)
    }

    pub fn player(&self, key: &str) -> Result<Option<PlayerData>> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(PLAYER_DATA)?;
        match t.get(key)? {
            Some(v) => Ok(Some(postcard::from_bytes(v.value()).context("player data is corrupt")?)),
            None => Ok(None),
        }
    }

    /// Every station of a planet with something in it. A corrupt record is
    /// reported and skipped (left on disk untouched).
    pub fn stations(&self, planet: u16) -> Result<Vec<(BlockPos, SavedStation)>> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(STATIONS)?;
        let (lo, hi) = planet_range(planet);
        let mut out = Vec::new();
        for row in t.range(lo.as_slice()..hi.as_slice())? {
            let (k, v) = row?;
            match (block_from_key(k.value()), postcard::from_bytes(v.value())) {
                (Some(p), Ok(s)) => out.push((p, s)),
                _ => tracing::error!("a saved station is corrupt; skipped"),
            }
        }
        Ok(out)
    }

    /// Every storage block of a planet with something in it, as saved. A
    /// corrupt record is reported and skipped (left on disk untouched).
    pub fn storage(&self, planet: u16) -> Result<Vec<(BlockPos, SavedSlots)>> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(STORAGE)?;
        let (lo, hi) = planet_range(planet);
        let mut out = Vec::new();
        for row in t.range(lo.as_slice()..hi.as_slice())? {
            let (k, v) = row?;
            match (block_from_key(k.value()), postcard::from_bytes(v.value())) {
                (Some(p), Ok(s)) => out.push((p, s)),
                _ => tracing::error!("a saved storage block is corrupt; skipped"),
            }
        }
        Ok(out)
    }

    /// Every claim in the universe, and the next free claim id (above every
    /// stored key, even a corrupt one, so a new claim never overwrites a
    /// record).
    pub fn claims(&self) -> Result<(Vec<(u32, SavedClaim)>, u32)> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(CLAIMS)?;
        let mut out = Vec::new();
        let mut next = 1;
        for row in t.iter()? {
            let (k, v) = row?;
            let id = k.value();
            next = next.max(id.saturating_add(1));
            match postcard::from_bytes(v.value()) {
                Ok(c) => out.push((id, c)),
                Err(e) => tracing::error!("saved claim {id} is corrupt ({e}); skipped"),
            }
        }
        Ok((out, next))
    }

    /// The universe clock in ticks (0 for a new universe).
    pub fn clock(&self) -> Result<u64> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(META)?;
        Ok(match t.get("clock")? {
            Some(v) => postcard::from_bytes(v.value())?,
            None => 0,
        })
    }

    pub fn write(&self, batch: &WriteBatch) -> Result<()> {
        let tx = self.db.begin_write()?;
        {
            if let Some(clock) = batch.clock {
                tx.open_table(META)?
                    .insert("clock", postcard::to_stdvec(&clock)?.as_slice())?;
            }
            let mut chunks = tx.open_table(CHUNKS)?;
            for (planet, p, bytes) in &batch.chunks {
                chunks.insert(chunk_key(*planet, *p).as_slice(), bytes.as_slice())?;
            }
            let mut players = tx.open_table(PLAYER_DATA)?;
            for (key, data) in &batch.players {
                players.insert(key.as_str(), postcard::to_stdvec(data)?.as_slice())?;
            }
            let mut stations = tx.open_table(STATIONS)?;
            for (planet, p, s) in &batch.stations {
                let key = block_key(*planet, *p);
                match s {
                    Some(s) => stations.insert(key.as_slice(), postcard::to_stdvec(s)?.as_slice())?,
                    None => stations.remove(key.as_slice())?,
                };
            }
            let mut storage = tx.open_table(STORAGE)?;
            for (planet, p, slots) in &batch.storage {
                let key = block_key(*planet, *p);
                match slots {
                    Some(slots) => storage.insert(key.as_slice(), postcard::to_stdvec(slots)?.as_slice())?,
                    None => storage.remove(key.as_slice())?,
                };
            }
            let mut claims = tx.open_table(CLAIMS)?;
            for (id, c) in &batch.claims {
                match c {
                    Some(c) => claims.insert(*id, postcard::to_stdvec(c)?.as_slice())?,
                    None => claims.remove(*id)?,
                };
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Waits until this is the only handle on the database (loader jobs
    /// still finishing hold clones), so it is closed when this one drops.
    pub fn wait_until_sole_owner(&self, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while Arc::strong_count(&self.db) > 1 {
            if start.elapsed() > timeout {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        true
    }

    /// Edited chunks stored for one planet.
    pub fn stored_chunks(&self, planet: u16) -> Result<u64> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(CHUNKS)?;
        let (lo, hi) = planet_range(planet);
        let mut n = 0;
        for row in t.range(lo.as_slice()..hi.as_slice())? {
            row?;
            n += 1;
        }
        Ok(n)
    }
}

/// Version 2 adds accounts, inventories, stations and claims. Version 1's
/// players were keyed by the name they typed and held only a position: each
/// becomes the character of whoever registers that name (lowercased; if two
/// differed only by case, the first is kept), with an empty inventory.
fn upgrade_1_to_2(tx: &WriteTransaction) -> Result<()> {
    let mut moved = Vec::new();
    {
        let old = tx.open_table(V1_PLAYERS)?;
        for row in old.iter()? {
            let (k, v) = row?;
            match postcard::from_bytes::<PlayerState>(v.value()) {
                Ok(state) => moved.push((k.value().to_lowercase(), state)),
                Err(e) => tracing::warn!("version 1 player `{}` is corrupt ({e}); not carried over", k.value()),
            }
        }
    }
    {
        let mut new = tx.open_table(PLAYER_DATA)?;
        for (key, state) in &moved {
            if new.get(key.as_str())?.is_none() {
                let data = v2::PlayerData {
                    state: Some(*state),
                    inventory: Vec::new(),
                    last_seen: 0,
                };
                new.insert(key.as_str(), postcard::to_stdvec(&data)?.as_slice())?;
            }
        }
    }
    tx.delete_table(V1_PLAYERS)?;
    tracing::info!("carried {} version 1 players over", moved.len());
    Ok(())
}

/// Version 3 makes the planet the first of a universe: its tables gain the
/// planet id, its players and claims record it, and its settings become
/// planet 0's record, of kind `temperate`, generator 2 with the settings that
/// reproduce its generator 1 terrain. Everything is kept.
fn upgrade_2_to_3(tx: &WriteTransaction, meta: &v2::PlanetMeta, kinds: &PlanetKinds) -> Result<UpgradeReport> {
    const PLANET: u16 = 0;
    let mut report = UpgradeReport::default();
    let prefixed = |k: &[u8]| {
        let mut key = PLANET.to_be_bytes().to_vec();
        key.extend_from_slice(k);
        key
    };

    let old: Vec<(Vec<u8>, Vec<u8>)> = {
        let t = tx.open_table(V2_CHUNKS)?;
        t.iter()?
            .map(|r| r.map(|(k, v)| (k.value().to_vec(), v.value().to_vec())))
            .collect::<Result<_, _>>()?
    };
    {
        let mut t = tx.open_table(CHUNKS)?;
        for (k, v) in &old {
            if k.len() != 12 {
                tracing::error!("an old chunk key is malformed; skipped");
                continue;
            }
            t.insert(prefixed(k).as_slice(), v.as_slice())?;
            report.chunks += 1;
        }
    }
    tx.delete_table(V2_CHUNKS)?;

    let old: Vec<(Vec<u8>, Vec<u8>)> = {
        let t = tx.open_table(V2_STATIONS)?;
        t.iter()?
            .map(|r| r.map(|(k, v)| (k.value().to_vec(), v.value().to_vec())))
            .collect::<Result<_, _>>()?
    };
    {
        let mut t = tx.open_table(STATIONS)?;
        for (k, v) in &old {
            t.insert(prefixed(k).as_slice(), v.as_slice())?;
            report.stations += 1;
        }
    }
    tx.delete_table(V2_STATIONS)?;

    {
        let mut t = tx.open_table(CLAIMS)?;
        let old: Vec<(u32, Vec<u8>)> = t
            .iter()?
            .map(|r| r.map(|(k, v)| (k.value(), v.value().to_vec())))
            .collect::<Result<_, _>>()?;
        for (id, bytes) in old {
            match postcard::from_bytes::<v2::SavedClaim>(&bytes) {
                Ok(c) => {
                    let new = SavedClaim {
                        planet: PLANET,
                        owner: c.owner,
                        owner_name: c.owner_name,
                        stone: c.stone,
                        plots: c.plots,
                    };
                    t.insert(id, postcard::to_stdvec(&new)?.as_slice())?;
                    report.claims += 1;
                }
                // Left in place: `claims()` reports and skips it, and its id
                // stays taken.
                Err(e) => tracing::error!("old claim {id} is corrupt ({e}); left as it is"),
            }
        }
    }

    {
        let mut t = tx.open_table(PLAYER_DATA)?;
        let old: Vec<(String, Vec<u8>)> = t
            .iter()?
            .map(|r| r.map(|(k, v)| (k.value().to_string(), v.value().to_vec())))
            .collect::<Result<_, _>>()?;
        for (key, bytes) in old {
            match postcard::from_bytes::<v2::PlayerData>(&bytes) {
                Ok(p) => {
                    let new = PlayerData {
                        planet: PLANET,
                        state: p.state,
                        inventory: p.inventory,
                        last_seen: p.last_seen,
                    };
                    t.insert(key.as_str(), postcard::to_stdvec(&new)?.as_slice())?;
                    report.players += 1;
                }
                // Left in place, a login fails cleanly ("could not be
                // loaded") rather than starting them afresh over it.
                Err(e) => tracing::error!("old player data of `{key}` is corrupt ({e}); left as it is"),
            }
        }
    }

    let kind = kinds
        .get("temperate")
        .or_else(|| kinds.kinds.iter().find(|k| k.tier == 1))
        .or_else(|| kinds.kinds.first())
        .context("no planet kinds")?;
    let planet = SavedPlanet {
        generator_version: GENERATOR_VERSION,
        name: meta.name.clone(),
        kind: kind.name.clone(),
        shape: meta.shape,
        generation: PlanetGenConfig::from(meta.generation.clone()),
        day: DayCycle::default(),
        shrine: None,
    };
    tx.open_table(PLANETS)?.insert(PLANET, planet.encode()?.as_slice())?;
    let mut m = tx.open_table(META)?;
    m.remove("planet")?;
    m.insert(
        "universe",
        postcard::to_stdvec(&UniverseMeta {
            save_version: SAVE_VERSION,
            chunk_format: CHUNK_FORMAT_VERSION,
        })?
        .as_slice(),
    )?;
    tracing::info!(
        "kept {} edited chunks, {} stations, {} claims and {} characters",
        report.chunks,
        report.stations,
        report.claims,
        report.players
    );
    Ok(report)
}

#[derive(Default)]
pub struct WriteBatch {
    /// By planet id.
    pub chunks: Vec<(u16, ChunkPos, Vec<u8>)>,
    /// By lowercased account name.
    pub players: Vec<(String, PlayerData)>,
    /// By planet id; None deletes (the station was emptied).
    pub stations: Vec<(u16, BlockPos, Option<SavedStation>)>,
    /// None deletes (the claim was released).
    pub claims: Vec<(u32, Option<SavedClaim>)>,
    /// Storage block slots by planet id; None deletes (it was emptied).
    pub storage: Vec<(u16, BlockPos, Option<SavedSlots>)>,
    /// The universe clock, so days and station jobs continue across restarts.
    pub clock: Option<u64>,
    /// Each is signalled once the batch is durable (shutdown waits on one;
    /// departed players' data and evicted chunks are held in memory until
    /// theirs fires).
    pub done: Vec<Sender<()>>,
}

impl WriteBatch {
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
            && self.players.is_empty()
            && self.stations.is_empty()
            && self.claims.is_empty()
            && self.storage.is_empty()
    }
}

/// The single writer thread. A failed commit is retried; the batch is never
/// silently dropped, because the game loop has already cleared its dirty flags.
pub fn spawn_writer(store: Store) -> (Sender<WriteBatch>, std::thread::JoinHandle<()>) {
    let (tx, rx): (Sender<WriteBatch>, Receiver<WriteBatch>) = crossbeam_channel::unbounded();
    let handle = std::thread::Builder::new()
        .name("store-writer".into())
        .spawn(move || {
            for batch in rx {
                let mut attempt = 0;
                loop {
                    match store.write(&batch) {
                        Ok(()) => break,
                        Err(e) => {
                            attempt += 1;
                            tracing::error!("save failed (attempt {attempt}): {e:#}");
                            std::thread::sleep(std::time::Duration::from_millis(250 * attempt.min(20)));
                        }
                    }
                }
                for done in &batch.done {
                    let _ = done.send(());
                }
            }
        })
        .expect("spawn store writer");
    (tx, handle)
}

/// Test support: writes a single-planet save exactly as save versions 1 and
/// 2 did, so upgrades can be tested from outside the crate.
pub mod legacy {
    use super::*;

    /// One player of a version 2 save.
    pub struct V2Player<'a> {
        pub key: &'a str,
        pub state: Option<PlayerState>,
        pub inventory: Vec<Option<SavedStack>>,
    }

    /// A version 2 planet: its meta, chunks, players, stations, claims
    /// (owner key, owner name, stone, plots) and clock.
    #[allow(clippy::too_many_arguments)]
    pub fn write_v2_planet(
        path: &Path,
        meta: &V2PlanetMeta,
        chunks: &[(ChunkPos, Vec<u8>)],
        players: &[V2Player],
        stations: &[(BlockPos, SavedStation)],
        claims: &[(u32, &str, &str, BlockPos, Vec<PlotPos>)],
        clock: u64,
    ) -> Result<()> {
        let db = Database::create(path)?;
        let tx = db.begin_write()?;
        {
            let mut m = tx.open_table(META)?;
            m.insert("planet", postcard::to_stdvec(meta)?.as_slice())?;
            m.insert("clock", postcard::to_stdvec(&clock)?.as_slice())?;
            let mut t = tx.open_table(V2_CHUNKS)?;
            for (p, bytes) in chunks {
                let mut k = [0u8; 12];
                k[0..4].copy_from_slice(&p.x.to_be_bytes());
                k[4..8].copy_from_slice(&p.y.to_be_bytes());
                k[8..12].copy_from_slice(&p.z.to_be_bytes());
                t.insert(k.as_slice(), bytes.as_slice())?;
            }
            tx.open_table(ACCOUNTS)?;
            let mut t = tx.open_table(PLAYER_DATA)?;
            for p in players {
                let data = v2::PlayerData {
                    state: p.state,
                    inventory: p.inventory.clone(),
                    last_seen: 0,
                };
                t.insert(p.key, postcard::to_stdvec(&data)?.as_slice())?;
            }
            let mut t = tx.open_table(V2_STATIONS)?;
            for (p, s) in stations {
                let k = &block_key(0, *p)[2..];
                t.insert(k, postcard::to_stdvec(s)?.as_slice())?;
            }
            let mut t = tx.open_table(CLAIMS)?;
            for (id, owner, owner_name, stone, plots) in claims {
                let c = v2::SavedClaim {
                    owner: owner.to_string(),
                    owner_name: owner_name.to_string(),
                    stone: *stone,
                    plots: plots.clone(),
                };
                t.insert(*id, postcard::to_stdvec(&c)?.as_slice())?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The meta record of a version 2 planet with default generation.
    pub fn v2_meta(name: &str, shape: PlanetShape, seed: u64, generator_version: u32) -> V2PlanetMeta {
        let d = PlanetGenConfig::default();
        V2PlanetMeta {
            save_version: 2,
            generator_version,
            chunk_format: CHUNK_FORMAT_VERSION,
            name: name.into(),
            shape,
            generation: PlanetGenConfigV1 {
                seed,
                sea_level: d.sea_level,
                snow_line: d.snow_line,
                mountain_height: d.mountain_height,
                tree_density: d.tree_density,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    fn kinds() -> PlanetKinds {
        PlanetKinds::builtin(&bm_core::BlockRegistry::builtin())
    }

    fn at(x: f64) -> PlayerState {
        PlayerState {
            pos: DVec3::new(x, 2.0, 3.0),
            ..Default::default()
        }
    }

    fn planet(name: &str) -> SavedPlanet {
        SavedPlanet {
            generator_version: GENERATOR_VERSION,
            name: name.into(),
            kind: "arid".into(),
            shape: PlanetShape::new(16, 4),
            generation: kinds().get("arid").unwrap().generation(u64::MAX - 3),
            day: DayCycle::default(),
            shrine: Some(BlockPos::new(1, 2, 3)),
        }
    }

    #[test]
    fn everything_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("u.redb");
        let stack = SavedStack {
            item: "stone".into(),
            count: 7,
            wear: 0,
        };
        let station = SavedStation {
            kind: Station::Furnace,
            jobs: vec![SavedJob {
                output: "glass".into(),
                output_count: 1,
                inputs: vec![("sand".into(), 2)],
                remaining: 3,
            }],
            started: 99,
            output: vec![Some(stack.clone()), None],
        };
        let claim = SavedClaim {
            planet: 2,
            owner: "pendragon".into(),
            owner_name: "Pendragon".into(),
            stone: BlockPos::new(1, 2, 3),
            plots: vec![PlotPos::new(0, 0, 0)],
        };
        let spot = BlockPos::new(-5, 40, 1 << 40);
        {
            let s = Store::open(&path).unwrap();
            assert_eq!(s.universe(&kinds()).unwrap(), None);
            s.put_planet(2, &planet("Emberdune")).unwrap();
            s.write(&WriteBatch {
                chunks: vec![
                    (2, ChunkPos::new(-1, 2, 3), vec![9, 9]),
                    (3, ChunkPos::new(-1, 2, 3), vec![8]),
                ],
                players: vec![(
                    "pendragon".into(),
                    PlayerData {
                        planet: 2,
                        state: Some(at(1.0)),
                        inventory: vec![Some(stack.clone())],
                        last_seen: 1_700_000_000,
                    },
                )],
                stations: vec![(2, spot, Some(station.clone()))],
                claims: vec![(4, Some(claim.clone()))],
                storage: vec![(2, spot, Some(vec![None, Some(stack.clone())]))],
                clock: Some(777),
                done: Vec::new(),
            })
            .unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.universe(&kinds()).unwrap(), None);
        assert_eq!(
            s.planet(2).unwrap(),
            Some(planet("Emberdune")),
            "settings and a huge seed round-trip"
        );
        assert_eq!(s.planet(1).unwrap(), None);
        assert_eq!(s.planet_ids().unwrap(), vec![2]);
        assert_eq!(s.chunk(2, ChunkPos::new(-1, 2, 3)).unwrap(), Some(vec![9, 9]));
        assert_eq!(
            s.chunk(3, ChunkPos::new(-1, 2, 3)).unwrap(),
            Some(vec![8]),
            "planets apart"
        );
        assert_eq!(s.chunk(2, ChunkPos::new(0, 0, 0)).unwrap(), None);
        assert_eq!((s.stored_chunks(2).unwrap(), s.stored_chunks(0).unwrap()), (1, 0));
        let p = s.player("pendragon").unwrap().unwrap();
        assert_eq!(p.planet, 2);
        assert_eq!(p.state.unwrap().pos, DVec3::new(1.0, 2.0, 3.0));
        assert_eq!(p.inventory, vec![Some(stack.clone())]);
        assert_eq!(s.stations(2).unwrap(), vec![(spot, station)]);
        assert_eq!(s.storage(2).unwrap(), vec![(spot, vec![None, Some(stack)])]);
        assert!(s.storage(3).unwrap().is_empty());
        assert!(s.stations(1).unwrap().is_empty() && s.stations(3).unwrap().is_empty());
        assert_eq!(s.claims().unwrap(), (vec![(4, claim)], 5));
        assert_eq!(s.clock().unwrap(), 777);

        // Deletions.
        s.write(&WriteBatch {
            stations: vec![(2, spot, None)],
            claims: vec![(4, None)],
            storage: vec![(2, spot, None)],
            ..Default::default()
        })
        .unwrap();
        assert!(s.stations(2).unwrap().is_empty());
        assert!(s.storage(2).unwrap().is_empty());
        assert_eq!(s.claims().unwrap(), (vec![], 1));
    }

    #[test]
    fn the_last_planet_id_has_a_range_too() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("u.redb")).unwrap();
        let st = SavedStation {
            kind: Station::Workbench,
            jobs: vec![],
            started: 0,
            output: vec![],
        };
        s.write(&WriteBatch {
            stations: vec![(u16::MAX, BlockPos::new(1, 1, 1), Some(st.clone()))],
            chunks: vec![(u16::MAX, ChunkPos::new(0, 0, 0), vec![1])],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(s.stations(u16::MAX).unwrap().len(), 1);
        assert_eq!(s.stored_chunks(u16::MAX).unwrap(), 1);
    }

    #[test]
    fn accounts_are_created_once() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("u.redb")).unwrap();
        let rec = AccountRecord {
            name: "Alice".into(),
            hash: "h".into(),
        };
        assert!(s.create_account("alice", &rec).unwrap());
        assert!(
            !s.create_account(
                "alice",
                &AccountRecord {
                    name: "ALICE".into(),
                    ..rec.clone()
                }
            )
            .unwrap()
        );
        assert_eq!(s.account("alice").unwrap(), Some(rec));
        assert_eq!(s.account("bob").unwrap(), None);
    }

    #[test]
    fn refuses_newer_saves() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("u.redb")).unwrap();
        {
            let tx = s.db.begin_write().unwrap();
            tx.open_table(META)
                .unwrap()
                .insert(
                    "universe",
                    postcard::to_stdvec(&UniverseMeta {
                        save_version: SAVE_VERSION + 1,
                        chunk_format: CHUNK_FORMAT_VERSION,
                    })
                    .unwrap()
                    .as_slice(),
                )
                .unwrap();
            tx.commit().unwrap();
        }
        assert!(s.universe(&kinds()).is_err());

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.redb");
        let meta = legacy::v2_meta("Old", PlanetShape::new(16, 4), 1, 7);
        legacy::write_v2_planet(&path, &meta, &[], &[], &[], &[], 0).unwrap();
        let s = Store::open(&path).unwrap();
        assert!(s.universe(&kinds()).is_err(), "an unknown generator is refused");
    }

    /// Writes a planet exactly as save version 1 did: its meta, a chunk, and
    /// the old players table.
    fn write_v1_planet(path: &Path, players: &[(&str, PlayerState)]) {
        let db = Database::create(path).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut m = legacy::v2_meta("Test", PlanetShape::new(16, 4), 5, 1);
            m.save_version = 1;
            tx.open_table(META)
                .unwrap()
                .insert("planet", postcard::to_stdvec(&m).unwrap().as_slice())
                .unwrap();
            let mut k = [0u8; 12];
            k[4..8].copy_from_slice(&1i32.to_be_bytes());
            tx.open_table(V2_CHUNKS)
                .unwrap()
                .insert(k.as_slice(), [1u8, 2].as_slice())
                .unwrap();
            let mut t = tx.open_table(V1_PLAYERS).unwrap();
            for (name, state) in players {
                t.insert(*name, postcard::to_stdvec(state).unwrap().as_slice()).unwrap();
            }
        }
        tx.commit().unwrap();
    }

    #[test]
    fn a_version_1_planet_is_upgraded_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.redb");
        write_v1_planet(
            &path,
            &[("Pendragon", at(5.0)), ("pendragon", at(6.0)), ("Bob", at(7.0))],
        );
        let s = Store::open(&path).unwrap();
        let report = s.universe(&kinds()).unwrap().unwrap();
        assert_eq!((report.chunks, report.players), (1, 2));
        let p = s.planet(0).unwrap().unwrap();
        assert_eq!(
            (p.name.as_str(), p.kind.as_str(), p.generation.seed),
            ("Test", "temperate", 5)
        );
        assert_eq!(
            s.chunk(0, ChunkPos::new(0, 1, 0)).unwrap(),
            Some(vec![1, 2]),
            "edits kept"
        );
        let bob = s.player("bob").unwrap().unwrap();
        assert_eq!((bob.planet, bob.state.unwrap().pos.x), (0, 7.0));
        assert!(bob.inventory.is_empty());
        // Redb iterates keys in order: "Pendragon" sorts before "pendragon".
        assert_eq!(s.player("pendragon").unwrap().unwrap().state.unwrap().pos.x, 5.0);
        drop(s);
        // Reopening finds version 3 and changes nothing.
        let s = Store::open(&path).unwrap();
        assert_eq!(s.universe(&kinds()).unwrap(), None);
        let tx = s.db.begin_read().unwrap();
        assert!(tx.open_table(V1_PLAYERS).is_err(), "the old table is gone");
        assert!(tx.open_table(V2_CHUNKS).is_err(), "the old table is gone");
    }

    #[test]
    fn a_version_2_planet_keeps_everything() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.redb");
        let mut meta = legacy::v2_meta("Old", PlanetShape::new(16, 4), 9, 1);
        meta.generation.tree_density = 0.02;
        let stone = SavedStack {
            item: "stone".into(),
            count: 3,
            wear: 0,
        };
        let st = SavedStation {
            kind: Station::Workbench,
            jobs: vec![],
            started: 0,
            output: vec![Some(stone.clone())],
        };
        legacy::write_v2_planet(
            &path,
            &meta,
            &[(ChunkPos::new(-1, 1, 0), vec![1])],
            &[legacy::V2Player {
                key: "alice",
                state: Some(at(4.0)),
                inventory: vec![Some(stone.clone())],
            }],
            &[(BlockPos::new(1, -2, 3), st.clone())],
            &[(3, "alice", "Alice", BlockPos::new(1, 2, 3), vec![PlotPos::new(0, 0, 0)])],
            50,
        )
        .unwrap();
        let s = Store::open(&path).unwrap();
        let report = s.universe(&kinds()).unwrap().unwrap();
        assert_eq!(
            report,
            UpgradeReport {
                chunks: 1,
                players: 1,
                claims: 1,
                stations: 1
            }
        );
        assert_eq!(s.chunk(0, ChunkPos::new(-1, 1, 0)).unwrap(), Some(vec![1]));
        assert_eq!(s.stations(0).unwrap(), vec![(BlockPos::new(1, -2, 3), st)]);
        let (claims, next) = s.claims().unwrap();
        assert_eq!(
            (claims[0].0, claims[0].1.planet, claims[0].1.owner_name.as_str(), next),
            (3, 0, "Alice", 4)
        );
        let alice = s.player("alice").unwrap().unwrap();
        assert_eq!((alice.planet, alice.state.unwrap().pos.x), (0, 4.0));
        assert_eq!(alice.inventory, vec![Some(stone)]);
        assert_eq!(s.clock().unwrap(), 50);
        let p = s.planet(0).unwrap().unwrap();
        assert_eq!(p.generator_version, GENERATOR_VERSION);
        assert_eq!(
            p.generation,
            PlanetGenConfig::from(meta.generation.clone()),
            "settings kept"
        );
        assert_eq!((p.name.as_str(), p.shape), ("Old", meta.shape));
    }

    #[test]
    fn stacks_are_saved_by_name() {
        let blocks = bm_core::BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let mut inv = Inventory::default();
        inv.slots[3] = Some(ItemStack {
            wear: 12,
            ..ItemStack::new(items.id("stone_hammer").unwrap(), 1)
        });
        inv.slots[5] = Some(ItemStack::coloured(items.id("stone").unwrap(), Colour(77), 9));
        let saved = save_slots(&items, &inv);
        assert_eq!(saved[3].as_ref().unwrap().item, "stone_hammer");
        assert_eq!(saved[5].as_ref().unwrap().item, "stone#77");
        assert_eq!(load_slots(&items, &saved, "test"), inv.slots);
        let gone = vec![Some(SavedStack {
            item: "unobtainium".into(),
            count: 1,
            wear: 0,
        })];
        assert_eq!(load_slots(&items, &gone, "test"), vec![None]);
    }
}
