//! Persistence (ADR-0005).
//!
//! One crash-safe embedded database per planet (redb: ACID, MVCC). Only
//! chunks players have changed are stored; everything else is regenerated
//! from the seed, so an untouched planet costs a few hundred bytes on disk.
//!
//! Reads happen on loader threads concurrently with the single writer thread.
//! Writes are batched: the game loop hands over every dirty chunk, player,
//! station and claim every few seconds, and they commit in one transaction,
//! so a block broken into an inventory is saved together with that inventory.
//! Accounts are the exception: they are created from the connection tasks,
//! each in its own transaction, because a name must be taken exactly once.
//!
//! Items are saved by name, never by `ItemId`: ids are positions in the
//! catalogue, and adding one block shifts every item after it.

use anyhow::{Context, Result, bail};
use bm_core::{
    BlockPos, CHUNK_FORMAT_VERSION, ChunkPos, Inventory, ItemRegistry, ItemStack, PlanetShape, PlayerState, PlotPos,
    Station,
};
use bm_worldgen::{GENERATOR_VERSION, PlanetGenConfig};
use crossbeam_channel::{Receiver, Sender};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, WriteTransaction};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
const CHUNKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("chunks");
/// Lowercased account name -> [`AccountRecord`].
const ACCOUNTS: TableDefinition<&str, &[u8]> = TableDefinition::new("accounts");
/// Lowercased account name -> [`PlayerData`].
const PLAYER_DATA: TableDefinition<&str, &[u8]> = TableDefinition::new("player_data");
/// Canonical block position -> [`SavedStation`].
const STATIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("stations");
/// Claim id -> [`SavedClaim`].
const CLAIMS: TableDefinition<u32, &[u8]> = TableDefinition::new("claims");
/// Save version 1's players: name as typed -> `PlayerState`. Read only to
/// upgrade, then deleted.
const V1_PLAYERS: TableDefinition<&str, &[u8]> = TableDefinition::new("players");

/// Bump when the layout of anything below changes, and teach
/// [`Store::upgrade`] the step from the previous version.
pub const SAVE_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanetMeta {
    pub save_version: u32,
    pub generator_version: u32,
    pub chunk_format: u8,
    pub name: String,
    pub shape: PlanetShape,
    pub generation: PlanetGenConfig,
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

/// One stack as saved: by item name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedStack {
    pub item: String,
    pub count: u16,
    pub wear: u16,
}

impl SavedStack {
    pub fn new(items: &ItemRegistry, s: &ItemStack) -> Option<Self> {
        Some(Self {
            item: items.get(s.item)?.name.clone(),
            count: s.count,
            wear: s.wear,
        })
    }

    /// The stack in this catalogue, if the item still exists.
    pub fn resolve(&self, items: &ItemRegistry) -> Option<ItemStack> {
        let id = items.id(&self.item).ok()?;
        Some(ItemStack {
            item: id,
            count: self.count,
            wear: self.wear,
        })
    }
}

pub fn save_slots(items: &ItemRegistry, inv: &Inventory) -> Vec<Option<SavedStack>> {
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
    /// None: never placed in the world (starts at the spawn).
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
    /// Planet tick the craft in progress began.
    pub started: u64,
    pub output: Vec<Option<SavedStack>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedClaim {
    /// Lowercased account name.
    pub owner: String,
    /// The owner's name as registered.
    pub owner_name: String,
    pub stone: BlockPos,
    pub plots: Vec<PlotPos>,
}

fn chunk_key(p: ChunkPos) -> [u8; 12] {
    let mut k = [0u8; 12];
    k[0..4].copy_from_slice(&p.x.to_be_bytes());
    k[4..8].copy_from_slice(&p.y.to_be_bytes());
    k[8..12].copy_from_slice(&p.z.to_be_bytes());
    k
}

fn block_key(p: BlockPos) -> [u8; 24] {
    let mut k = [0u8; 24];
    k[0..8].copy_from_slice(&p.x.to_be_bytes());
    k[8..16].copy_from_slice(&p.y.to_be_bytes());
    k[16..24].copy_from_slice(&p.z.to_be_bytes());
    k
}

fn block_from_key(k: &[u8]) -> Option<BlockPos> {
    let part = |i: usize| Some(i64::from_be_bytes(k.get(i..i + 8)?.try_into().ok()?));
    (k.len() == 24).then_some(())?;
    Some(BlockPos::new(part(0)?, part(8)?, part(16)?))
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
        tx.open_table(CHUNKS)?;
        tx.open_table(ACCOUNTS)?;
        tx.open_table(PLAYER_DATA)?;
        tx.open_table(STATIONS)?;
        tx.open_table(CLAIMS)?;
        tx.commit()?;
        Ok(Self { db: Arc::new(db) })
    }

    /// The saved planet, or `fresh` saved as the planet if there is none.
    /// Refuses a planet made by a different generator (its edits were made
    /// against terrain this build would not regenerate) or by a newer server;
    /// an older save layout is upgraded in place.
    pub fn planet_meta(&self, fresh: PlanetMeta) -> Result<PlanetMeta> {
        let tx = self.db.begin_read()?;
        let existing = tx.open_table(META)?.get("planet")?.map(|v| v.value().to_vec());
        drop(tx);
        match existing {
            Some(bytes) => {
                let meta: PlanetMeta = postcard::from_bytes(&bytes).context("planet metadata is corrupt")?;
                if meta.save_version > SAVE_VERSION {
                    bail!(
                        "planet save version {} is newer than this server's {}; update the server",
                        meta.save_version,
                        SAVE_VERSION
                    );
                }
                if meta.generator_version != GENERATOR_VERSION {
                    bail!(
                        "planet `{}` was generated by generator v{}, this server is v{}; it cannot be opened",
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
                if meta.save_version < SAVE_VERSION {
                    return self.upgrade(meta);
                }
                Ok(meta)
            }
            None => {
                let tx = self.db.begin_write()?;
                tx.open_table(META)?
                    .insert("planet", postcard::to_stdvec(&fresh)?.as_slice())?;
                tx.commit()?;
                Ok(fresh)
            }
        }
    }

    /// Brings an older save up to [`SAVE_VERSION`], one step at a time, in a
    /// single transaction: a crash mid-upgrade leaves the old layout intact.
    fn upgrade(&self, mut meta: PlanetMeta) -> Result<PlanetMeta> {
        let from = meta.save_version;
        let tx = self.db.begin_write()?;
        while meta.save_version < SAVE_VERSION {
            match meta.save_version {
                1 => upgrade_1_to_2(&tx)?,
                v => bail!("no upgrade from save version {v}"),
            }
            meta.save_version += 1;
        }
        tx.open_table(META)?
            .insert("planet", postcard::to_stdvec(&meta)?.as_slice())?;
        tx.commit()?;
        tracing::info!(
            "upgraded planet `{}` from save version {from} to {SAVE_VERSION}",
            meta.name
        );
        Ok(meta)
    }

    pub fn chunk(&self, p: ChunkPos) -> Result<Option<Vec<u8>>> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(CHUNKS)?;
        Ok(t.get(chunk_key(p).as_slice())?.map(|v| v.value().to_vec()))
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

    /// Every station with something in it. A corrupt record is reported and
    /// skipped (left on disk untouched).
    pub fn stations(&self) -> Result<Vec<(BlockPos, SavedStation)>> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(STATIONS)?;
        let mut out = Vec::new();
        for row in t.iter()? {
            let (k, v) = row?;
            match (block_from_key(k.value()), postcard::from_bytes(v.value())) {
                (Some(p), Ok(s)) => out.push((p, s)),
                _ => tracing::error!("a saved station is corrupt; skipped"),
            }
        }
        Ok(out)
    }

    /// Every claim, and the next free claim id (above every stored key, even
    /// a corrupt one, so a new claim never overwrites a record).
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

    /// The planet clock in ticks (0 for a new planet).
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
            for (p, bytes) in &batch.chunks {
                chunks.insert(chunk_key(*p).as_slice(), bytes.as_slice())?;
            }
            let mut players = tx.open_table(PLAYER_DATA)?;
            for (key, data) in &batch.players {
                players.insert(key.as_str(), postcard::to_stdvec(data)?.as_slice())?;
            }
            let mut stations = tx.open_table(STATIONS)?;
            for (p, s) in &batch.stations {
                match s {
                    Some(s) => stations.insert(block_key(*p).as_slice(), postcard::to_stdvec(s)?.as_slice())?,
                    None => stations.remove(block_key(*p).as_slice())?,
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

    pub fn stored_chunks(&self) -> Result<u64> {
        let tx = self.db.begin_read()?;
        Ok(redb::ReadableTableMetadata::len(&tx.open_table(CHUNKS)?)?)
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
                let data = PlayerData {
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

#[derive(Default)]
pub struct WriteBatch {
    pub chunks: Vec<(ChunkPos, Vec<u8>)>,
    /// By lowercased account name.
    pub players: Vec<(String, PlayerData)>,
    /// None deletes (the station was emptied).
    pub stations: Vec<(BlockPos, Option<SavedStation>)>,
    /// None deletes (the claim was released).
    pub claims: Vec<(u32, Option<SavedClaim>)>,
    /// The planet clock, so days and station jobs continue across restarts.
    pub clock: Option<u64>,
    /// Each is signalled once the batch is durable (shutdown waits on one;
    /// departed players' data is held in memory until theirs fires).
    pub done: Vec<Sender<()>>,
}

impl WriteBatch {
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty() && self.players.is_empty() && self.stations.is_empty() && self.claims.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    fn meta() -> PlanetMeta {
        PlanetMeta {
            save_version: SAVE_VERSION,
            generator_version: GENERATOR_VERSION,
            chunk_format: CHUNK_FORMAT_VERSION,
            name: "Test".into(),
            shape: PlanetShape::new(16, 4),
            generation: PlanetGenConfig::default(),
        }
    }

    fn at(x: f64) -> PlayerState {
        PlayerState {
            pos: DVec3::new(x, 2.0, 3.0),
            ..Default::default()
        }
    }

    #[test]
    fn everything_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.redb");
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
            owner: "pendragon".into(),
            owner_name: "Pendragon".into(),
            stone: BlockPos::new(1, 2, 3),
            plots: vec![PlotPos::new(0, 0, 0)],
        };
        let spot = BlockPos::new(-5, 40, 1 << 40);
        {
            let s = Store::open(&path).unwrap();
            s.planet_meta(meta()).unwrap();
            s.write(&WriteBatch {
                chunks: vec![(ChunkPos::new(-1, 2, 3), vec![9, 9])],
                players: vec![(
                    "pendragon".into(),
                    PlayerData {
                        state: Some(at(1.0)),
                        inventory: vec![Some(stack.clone())],
                        last_seen: 1_700_000_000,
                    },
                )],
                stations: vec![(spot, Some(station.clone()))],
                claims: vec![(4, Some(claim.clone()))],
                clock: Some(777),
                done: Vec::new(),
            })
            .unwrap();
        }
        let s = Store::open(&path).unwrap();
        let mut other = meta();
        other.name = "Ignored".into();
        assert_eq!(s.planet_meta(other).unwrap().name, "Test");
        assert_eq!(s.chunk(ChunkPos::new(-1, 2, 3)).unwrap(), Some(vec![9, 9]));
        assert_eq!(s.chunk(ChunkPos::new(0, 0, 0)).unwrap(), None);
        let p = s.player("pendragon").unwrap().unwrap();
        assert_eq!(p.state.unwrap().pos, DVec3::new(1.0, 2.0, 3.0));
        assert_eq!(p.inventory, vec![Some(stack)]);
        assert_eq!(s.stations().unwrap(), vec![(spot, station)]);
        assert_eq!(s.claims().unwrap(), (vec![(4, claim)], 5));
        assert_eq!(s.clock().unwrap(), 777);

        // Deletions.
        s.write(&WriteBatch {
            stations: vec![(spot, None)],
            claims: vec![(4, None)],
            ..Default::default()
        })
        .unwrap();
        assert!(s.stations().unwrap().is_empty());
        assert_eq!(s.claims().unwrap(), (vec![], 1));
    }

    #[test]
    fn accounts_are_created_once() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("p.redb")).unwrap();
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
    fn refuses_another_generator_version_and_newer_saves() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("p.redb")).unwrap();
        let mut old = meta();
        old.generator_version = GENERATOR_VERSION + 1;
        s.planet_meta(old).unwrap();
        assert!(s.planet_meta(meta()).is_err());

        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("p.redb")).unwrap();
        let mut newer = meta();
        newer.save_version = SAVE_VERSION + 1;
        s.planet_meta(newer).unwrap();
        assert!(s.planet_meta(meta()).is_err());
    }

    /// Writes a planet exactly as save version 1 did: its meta, a chunk, and
    /// the old players table.
    pub(crate) fn write_v1_planet(path: &Path, players: &[(&str, PlayerState)]) {
        let db = Database::create(path).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut m = meta();
            m.save_version = 1;
            tx.open_table(META)
                .unwrap()
                .insert("planet", postcard::to_stdvec(&m).unwrap().as_slice())
                .unwrap();
            tx.open_table(CHUNKS)
                .unwrap()
                .insert(chunk_key(ChunkPos::new(0, 1, 0)).as_slice(), [1u8, 2].as_slice())
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
        let m = s.planet_meta(meta()).unwrap();
        assert_eq!(m.save_version, SAVE_VERSION);
        assert_eq!(m.name, "Test");
        assert_eq!(s.chunk(ChunkPos::new(0, 1, 0)).unwrap(), Some(vec![1, 2]), "edits kept");
        let bob = s.player("bob").unwrap().unwrap();
        assert_eq!(bob.state.unwrap().pos.x, 7.0);
        assert!(bob.inventory.is_empty());
        // Redb iterates keys in order: "Pendragon" sorts before "pendragon".
        assert_eq!(s.player("pendragon").unwrap().unwrap().state.unwrap().pos.x, 5.0);
        drop(s);
        // Reopening finds version 2 and changes nothing.
        let s = Store::open(&path).unwrap();
        assert_eq!(s.planet_meta(meta()).unwrap().save_version, SAVE_VERSION);
        let tx = s.db.begin_read().unwrap();
        assert!(tx.open_table(V1_PLAYERS).is_err(), "the old table is gone");
    }

    #[test]
    fn stacks_are_saved_by_name() {
        let blocks = bm_core::BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let mut inv = Inventory::default();
        inv.slots[3] = Some(ItemStack {
            item: items.id("stone_hammer").unwrap(),
            count: 1,
            wear: 12,
        });
        let saved = save_slots(&items, &inv);
        assert_eq!(saved[3].as_ref().unwrap().item, "stone_hammer");
        assert_eq!(load_slots(&items, &saved, "test"), inv.slots);
        let gone = vec![Some(SavedStack {
            item: "unobtainium".into(),
            count: 1,
            wear: 0,
        })];
        assert_eq!(load_slots(&items, &gone, "test"), vec![None]);
    }
}
