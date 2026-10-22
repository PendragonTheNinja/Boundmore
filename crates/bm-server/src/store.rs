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
use bm_core::forge::Forged;
use bm_core::portal::PortalAxis;
use bm_core::skills::{Character, MAX_PAGES, Page};
use bm_core::{
    BlockPos, CHUNK_FORMAT_VERSION, ChunkPos, Colour, DayCycle, Inventory, ItemRegistry, ItemStack, PlanetShape,
    PlayerState, PlotPos, Station,
};
use bm_worldgen::{GENERATOR_VERSION, PlanetGenConfig, PlanetGenConfigV1, PlanetKinds};
use crossbeam_channel::{Receiver, Sender};
use glam::DVec3;
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
/// Lowercased account name -> [`SavedReclaim`]: what came back from that
/// account's lapsed claims. Added in M6 without a version bump, as `STORAGE`
/// was: a new table changes nothing already saved.
const RECLAIM: TableDefinition<&str, &[u8]> = TableDefinition::new("reclaim");
/// Portal number (unique in the universe, never reused) -> [`SavedPortal`].
/// Added in M8 without a version bump, as `STORAGE` was; the next portal
/// number is kept in `META` under "next_portal".
const PORTALS: TableDefinition<u32, &[u8]> = TableDefinition::new("portals");
/// Each planet's regions' cooldowns and its meteors, by planet id (save
/// version 7, milestone 14).
const SKIES: TableDefinition<u16, &[u8]> = TableDefinition::new("planet_skies");
/// Planet id and canonical block position -> [`SavedCrop`]: one growing
/// crop (save version 10, milestone 16 part 2, ADR-0010).
const CROPS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("planet_crops");
/// Regions each account has discovered (save version 7).
const DISCOVERIES: TableDefinition<&str, &[u8]> = TableDefinition::new("discoveries");

/// Save version 1 and 2 tables, read only to upgrade, then deleted.
const V1_PLAYERS: TableDefinition<&str, &[u8]> = TableDefinition::new("players");
const V2_CHUNKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("chunks");
const V2_STATIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("stations");

/// Bump when the layout of anything below changes, and teach
/// [`Store::upgrade`] the step from the previous version.
///
/// 3 (M4): one file per universe, tables keyed by planet; player data records
/// the planet; claim ids are unique across the universe.
///
/// 4 (M11): a station saves its furnace fuel slot, banked heat and whether
/// its current craft's heat is paid ([`SavedStation`]); version 3 rows are
/// rewritten with an empty slot, no heat and the craft paid.
///
/// 5 (M12): player data saves the character's progression
/// ([`SavedCharacter`]: XP, marks, skill pages); a station saves the craft
/// XP its finished output holds for whoever collects it. Version 3 and 4
/// players become level 1 with no XP, and their stations hold none.
///
/// 6 (M13): player data saves the health lost ([`PlayerData::hurt`]) and the
/// character's XP debt from defeats ([`SavedCharacter::debt`]). Older players
/// start unhurt with no debt.
///
/// 7 (M14): regions and meteorites, in new tables only.
///
/// 8 (M15): a station saves its wear and a spark generator its spark
/// ([`SavedStation`]); older stations are rewritten fully repaired with no
/// spark.
///
/// 9 (M16): a player saves hunger, the food buff and sickness
/// ([`PlayerData::food`]), and a station the full buckets its jobs hold
/// ([`SavedStation::containers`]); older players start unhungry with no
/// buff, older jobs with fresh buckets.
///
/// 10 (M16 part 2): crops in a new table ([`SavedCrop`]); version 9 opens
/// with none.
pub const SAVE_VERSION: u32 = 10;

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
/// every stack saved before colours existed reads (ADR-0007). A tagged stack
/// (a portal token, ADR-0009) saves its tag after an `@`, always last
/// (`portal_token@17`, `name#3@17` with both); no `@` is tag 0, so every
/// older save reads unchanged. A forged stack (milestone 17) saves its
/// traits after a `!`, before any tag (`iron_hammer!mighty5,quick3`,
/// `iron_hammer!` for forged with none); no `!` is unforged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedStack {
    pub item: String,
    pub count: u16,
    pub wear: u16,
}

impl SavedStack {
    pub fn new(items: &ItemRegistry, s: &ItemStack) -> Option<Self> {
        let name = &items.get(s.item)?.name;
        let mut item = name.clone();
        if s.colour != Colour::NATURAL {
            item.push_str(&format!("#{}", s.colour.0));
        }
        if s.forge.done {
            item.push('!');
            item.push_str(&s.forge.to_suffix());
        }
        if s.tag != 0 {
            item.push_str(&format!("@{}", s.tag));
        }
        Some(Self {
            item,
            count: s.count,
            wear: s.wear,
        })
    }

    /// The stack in this catalogue, if the item still exists.
    pub fn resolve(&self, items: &ItemRegistry) -> Option<ItemStack> {
        let (rest, tag) = match self.item.rsplit_once('@') {
            Some((rest, t)) => (rest, t.parse().ok()?),
            None => (self.item.as_str(), 0),
        };
        let (rest, forge) = match rest.split_once('!') {
            Some((rest, traits)) => (rest, Forged::from_suffix(traits)),
            None => (rest, Forged::default()),
        };
        let (name, colour) = match rest.split_once('#') {
            Some((name, c)) => (name, Colour(c.parse().ok()?)),
            None => (rest, Colour::NATURAL),
        };
        let id = items.id(name).ok()?;
        Some(ItemStack {
            item: id,
            count: self.count,
            wear: self.wear,
            colour,
            tag,
            forge,
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

/// A player's body as saved: where they are and how they are moving. This
/// is the layout `PlayerState` had before grapples (postcard is positional),
/// kept apart from it so a hook in flight is never saved and new movement
/// state never changes the save format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedBody {
    pub pos: DVec3,
    pub vel: DVec3,
    pub on_ground: bool,
}

impl From<PlayerState> for SavedBody {
    fn from(s: PlayerState) -> Self {
        Self {
            pos: s.pos,
            vel: s.vel,
            on_ground: s.on_ground,
        }
    }
}

impl From<SavedBody> for PlayerState {
    fn from(b: SavedBody) -> Self {
        Self {
            pos: b.pos,
            vel: b.vel,
            on_ground: b.on_ground,
            hook: None,
            // Energy is never saved: a player arrives rested. The body
            // (skills) is set from the character on joining.
            ..Default::default()
        }
    }
}

/// A planet's sky as saved (save version 7).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedSky {
    pub rest_until: Vec<u64>,
    pub squares: Vec<(i64, i64, u64)>,
    pub meteors: Vec<SavedMeteor>,
}

/// A meteor as saved: always reopened dormant (rule 16).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedMeteor {
    pub id: u32,
    pub site: BlockPos,
    pub level: u8,
    /// The heart closes at this tick.
    pub until: u64,
    pub cleared: bool,
    pub entitled: Vec<String>,
    pub looted: Vec<String>,
    pub shell: Vec<BlockPos>,
    pub broken: u32,
}

/// Regions each account has discovered, by planet id (save version 7).
pub type Discoveries = std::collections::BTreeMap<u16, Vec<u16>>;

/// Everything saved about one account's character.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerData {
    /// The planet they are on (its id).
    pub planet: u16,
    /// Where on it. None: never placed there (starts at its shrine).
    pub state: Option<SavedBody>,
    pub inventory: Vec<Option<SavedStack>>,
    /// Unix seconds when the player was last connected; 0 if never recorded
    /// (the server then counts from when it first sees the 0). Their claims
    /// lapse after `claim_keep_days` without a visit.
    pub last_seen: u64,
    /// Levels, XP, marks and skill pages (save version 5).
    pub character: SavedCharacter,
    /// Health lost (milestone 13, save version 6; 0: unhurt).
    pub hurt: u32,
    /// Hunger, the food buff and sickness (milestone 16, save version 9).
    pub food: SavedFood,
}

/// A growing crop as saved (`bm_core::crops::Crop`), its kind by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedCrop {
    pub crop: String,
    pub sown_at: u64,
    pub fertilised_at: Option<u64>,
    pub tilled: bool,
    pub wet: bool,
    pub epic: bool,
}

/// What food has done to a character (`PlayerState` hunger, buff, sick).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedFood {
    pub hunger: u32,
    pub buff: bm_core::food::FoodBuff,
    pub sick: u16,
}

impl From<&PlayerState> for SavedFood {
    fn from(s: &PlayerState) -> Self {
        Self {
            hunger: s.hunger,
            buff: s.buff,
            sick: s.sick,
        }
    }
}

/// A character's progression as saved (`bm_core::skills::Character`).
/// Skills are saved by id, never by position in the tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedCharacter {
    pub xp: u64,
    pub marks: u32,
    /// Each page's spent ranks, by skill id.
    pub pages: Vec<Vec<(String, u8)>>,
    pub active: u8,
    pub cleanses_used: u32,
    /// Universe tick of the last page switch away from home (0: never).
    pub switched_at: u64,
    /// Plots bought with marks.
    pub plots_bought: u32,
    /// XP debt from defeats (save version 6).
    pub debt: u64,
}

impl Default for SavedCharacter {
    fn default() -> Self {
        Character::default().into()
    }
}

impl From<Character> for SavedCharacter {
    fn from(c: Character) -> Self {
        Self {
            xp: c.xp,
            marks: c.marks,
            pages: c.pages.into_iter().map(|p| p.ranks.into_iter().collect()).collect(),
            active: c.active,
            cleanses_used: c.cleanses_used,
            switched_at: c.switched_at,
            plots_bought: c.plots_bought,
            debt: c.debt,
        }
    }
}

impl From<SavedCharacter> for Character {
    fn from(c: SavedCharacter) -> Self {
        let mut pages: Vec<Page> = c
            .pages
            .into_iter()
            .take(MAX_PAGES)
            .map(|p| Page {
                ranks: p.into_iter().collect(),
            })
            .collect();
        if pages.is_empty() {
            pages.push(Page::default());
        }
        let active = c.active.min(pages.len() as u8 - 1);
        Self {
            xp: c.xp,
            marks: c.marks,
            pages,
            active,
            cleanses_used: c.cleanses_used,
            switched_at: c.switched_at,
            plots_bought: c.plots_bought,
            debt: c.debt,
        }
    }
}

/// What waits for an account from its lapsed claims: the items, and the
/// notices it has not read yet (it was away when they lapsed).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedReclaim {
    pub items: Vec<SavedStack>,
    pub notices: Vec<String>,
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
    /// A furnace's fuel slot (save version 4).
    pub fuel: Option<SavedStack>,
    /// Heat banked from fuel already burnt (save version 4).
    pub heat: u32,
    /// The head job's current craft waits for heat: it has not started, and
    /// starts when fuel arrives (save version 4).
    pub waiting: bool,
    /// Craft XP of the finished output, given to whoever collects it (save
    /// version 5).
    pub xp: u32,
    /// A machine's wear left (save version 8; `spark::MACHINE_WEAR` when
    /// repaired, unused by stations that do not wear).
    pub wear: u32,
    /// A spark generator group's stored spark, the spark of the item burning
    /// and the ticks it has burnt, and its blocks (save version 8).
    pub spark: u32,
    pub burn_total: u32,
    pub burn_elapsed: u32,
    pub blocks: u32,
    /// Each job's full containers (buckets of water or milk), one list per
    /// job in order (save version 9).
    pub containers: Vec<Vec<SavedStack>>,
}

/// The station and player layouts save version 8 wrote, kept only to read
/// them for the upgrade.
mod v8 {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct SavedStation {
        pub kind: Station,
        pub jobs: Vec<SavedJob>,
        pub started: u64,
        pub output: Vec<Option<SavedStack>>,
        pub fuel: Option<SavedStack>,
        pub heat: u32,
        pub waiting: bool,
        pub xp: u32,
        pub wear: u32,
        pub spark: u32,
        pub burn_total: u32,
        pub burn_elapsed: u32,
        pub blocks: u32,
    }

    impl From<SavedStation> for super::SavedStation {
        fn from(s: SavedStation) -> Self {
            Self {
                kind: s.kind,
                jobs: s.jobs,
                started: s.started,
                output: s.output,
                fuel: s.fuel,
                heat: s.heat,
                waiting: s.waiting,
                xp: s.xp,
                wear: s.wear,
                spark: s.spark,
                burn_total: s.burn_total,
                burn_elapsed: s.burn_elapsed,
                blocks: s.blocks,
                containers: Vec::new(),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct PlayerData {
        pub planet: u16,
        pub state: Option<SavedBody>,
        pub inventory: Vec<Option<SavedStack>>,
        pub last_seen: u64,
        pub character: SavedCharacter,
        pub hurt: u32,
    }

    impl From<PlayerData> for super::PlayerData {
        fn from(p: PlayerData) -> Self {
            Self {
                planet: p.planet,
                state: p.state,
                inventory: p.inventory,
                last_seen: p.last_seen,
                character: p.character,
                hurt: p.hurt,
                // No hunger, no buff.
                food: SavedFood::default(),
            }
        }
    }
}

pub use v8::{PlayerData as V8PlayerData, SavedStation as V8SavedStation};

/// The station layout save versions 5 to 7 wrote, kept only to read them
/// for the upgrade.
mod v7 {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct SavedStation {
        pub kind: Station,
        pub jobs: Vec<SavedJob>,
        pub started: u64,
        pub output: Vec<Option<SavedStack>>,
        pub fuel: Option<SavedStack>,
        pub heat: u32,
        pub waiting: bool,
        pub xp: u32,
    }

    impl From<SavedStation> for super::SavedStation {
        fn from(s: SavedStation) -> Self {
            Self {
                kind: s.kind,
                jobs: s.jobs,
                started: s.started,
                output: s.output,
                fuel: s.fuel,
                heat: s.heat,
                waiting: s.waiting,
                xp: s.xp,
                // Fully repaired, no spark.
                wear: bm_core::spark::MACHINE_WEAR,
                spark: 0,
                burn_total: 0,
                burn_elapsed: 0,
                blocks: 1,
                containers: Vec::new(),
            }
        }
    }
}

pub use v7::SavedStation as V7SavedStation;

/// The layouts save version 3 wrote, kept only to read them for the upgrade.
mod v3 {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct SavedStation {
        pub kind: Station,
        pub jobs: Vec<SavedJob>,
        pub started: u64,
        pub output: Vec<Option<SavedStack>>,
    }

    impl From<SavedStation> for super::SavedStation {
        fn from(s: SavedStation) -> Self {
            Self {
                kind: s.kind,
                jobs: s.jobs,
                started: s.started,
                output: s.output,
                fuel: None,
                heat: 0,
                waiting: false,
                xp: 0,
                wear: bm_core::spark::MACHINE_WEAR,
                spark: 0,
                burn_total: 0,
                burn_elapsed: 0,
                blocks: 1,
                containers: Vec::new(),
            }
        }
    }
}

pub use v3::SavedStation as V3SavedStation;

/// The layouts save version 4 wrote (and version 3, for players), kept only
/// to read them for the upgrade.
mod v4 {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct SavedStation {
        pub kind: Station,
        pub jobs: Vec<SavedJob>,
        pub started: u64,
        pub output: Vec<Option<SavedStack>>,
        pub fuel: Option<SavedStack>,
        pub heat: u32,
        pub waiting: bool,
    }

    impl From<SavedStation> for super::SavedStation {
        fn from(s: SavedStation) -> Self {
            Self {
                kind: s.kind,
                jobs: s.jobs,
                started: s.started,
                output: s.output,
                fuel: s.fuel,
                heat: s.heat,
                waiting: s.waiting,
                xp: 0,
                wear: bm_core::spark::MACHINE_WEAR,
                spark: 0,
                burn_total: 0,
                burn_elapsed: 0,
                blocks: 1,
                containers: Vec::new(),
            }
        }
    }

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    pub struct PlayerData {
        pub planet: u16,
        pub state: Option<SavedBody>,
        pub inventory: Vec<Option<SavedStack>>,
        pub last_seen: u64,
    }

    impl From<PlayerData> for super::PlayerData {
        fn from(p: PlayerData) -> Self {
            Self {
                planet: p.planet,
                state: p.state,
                inventory: p.inventory,
                last_seen: p.last_seen,
                // An untrained character: level 1, no XP.
                character: SavedCharacter::default(),
                hurt: 0,
                food: SavedFood::default(),
            }
        }
    }
}

pub use v4::{PlayerData as V4PlayerData, SavedStation as V4SavedStation};

/// The layouts save version 5 wrote for players, kept only to read them for
/// the upgrade.
mod v5 {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SavedCharacter {
        pub xp: u64,
        pub marks: u32,
        pub pages: Vec<Vec<(String, u8)>>,
        pub active: u8,
        pub cleanses_used: u32,
        pub switched_at: u64,
        pub plots_bought: u32,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct PlayerData {
        pub planet: u16,
        pub state: Option<SavedBody>,
        pub inventory: Vec<Option<SavedStack>>,
        pub last_seen: u64,
        pub character: SavedCharacter,
    }

    impl From<PlayerData> for super::PlayerData {
        fn from(p: PlayerData) -> Self {
            let c = p.character;
            Self {
                planet: p.planet,
                state: p.state,
                inventory: p.inventory,
                last_seen: p.last_seen,
                character: super::SavedCharacter {
                    xp: c.xp,
                    marks: c.marks,
                    pages: c.pages,
                    active: c.active,
                    cleanses_used: c.cleanses_used,
                    switched_at: c.switched_at,
                    plots_bought: c.plots_bought,
                    // No defeats before creatures.
                    debt: 0,
                },
                // Unhurt: there was no health before.
                hurt: 0,
                food: SavedFood::default(),
            }
        }
    }
}

pub use v5::{PlayerData as V5PlayerData, SavedCharacter as V5SavedCharacter};

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

/// A portal as saved (ADR-0009): where its rectangle is, whose it is, what
/// it links to and what its tank holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPortal {
    pub planet: u16,
    pub corner: BlockPos,
    pub axis: PortalAxis,
    pub width: u8,
    pub height: u8,
    /// Lowercased account name.
    pub owner: String,
    pub link: Option<u32>,
    pub fuel: u32,
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
        pub state: Option<SavedBody>,
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
        tx.open_table(RECLAIM)?;
        tx.open_table(PORTALS)?;
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
                if u.save_version < 3 {
                    bail!("no upgrade from universe save version {}", u.save_version);
                }
                let u = if u.save_version < SAVE_VERSION {
                    let from = u.save_version;
                    let tx = self.db.begin_write()?;
                    let stations = match from {
                        3 => upgrade_3_to_4(&tx)?,
                        4 => upgrade_stations_4_to_5(&tx)?,
                        5..=7 => upgrade_stations(&tx, from, |b| {
                            postcard::from_bytes::<v7::SavedStation>(b).map(Into::into)
                        })?,
                        8 => upgrade_stations(&tx, from, |b| {
                            postcard::from_bytes::<v8::SavedStation>(b).map(Into::into)
                        })?,
                        // 9 to 10 only adds the crops table.
                        _ => 0,
                    };
                    // Version 6 to 7 only adds tables (skies, discoveries),
                    // which read as empty until written.
                    let players = if from <= 4 {
                        upgrade_players(&tx, from, |b| postcard::from_bytes::<v4::PlayerData>(b).map(Into::into))?
                    } else if from == 5 {
                        upgrade_players(&tx, from, |b| postcard::from_bytes::<v5::PlayerData>(b).map(Into::into))?
                    } else if from <= 8 {
                        // Versions 6 to 8 share a player layout.
                        upgrade_players(&tx, from, |b| postcard::from_bytes::<v8::PlayerData>(b).map(Into::into))?
                    } else {
                        0
                    };
                    let u = UniverseMeta {
                        save_version: SAVE_VERSION,
                        ..u
                    };
                    tx.open_table(META)?
                        .insert("universe", postcard::to_stdvec(&u)?.as_slice())?;
                    tx.commit()?;
                    if from == 9 {
                        tracing::info!(
                            "upgraded the universe from save version 9 to {SAVE_VERSION} (crops added; nothing changed)"
                        );
                    } else if from == 8 {
                        tracing::info!(
                            "upgraded the universe from save version {from} to {SAVE_VERSION} ({stations} stations \
                             and {players} characters; characters start unhungry with no food buff)"
                        );
                    } else if from >= 6 {
                        tracing::info!(
                            "upgraded the universe from save version {from} to {SAVE_VERSION} ({stations} stations \
                             fully repaired with no spark; {players} characters unhungry; planets as they were)"
                        );
                    } else {
                        let characters = if from <= 4 {
                            "start at level 1"
                        } else {
                            "keep their progress"
                        };
                        tracing::info!(
                            "upgraded the universe from save version {from} to {SAVE_VERSION} ({stations} stations, \
                             {players} characters {characters}, unhurt and with no XP debt)"
                        );
                    }
                    u
                } else {
                    u
                };
                // Older chunk formats still decode (`Chunk::decode` reads
                // every earlier version); chunks are rewritten in the current
                // one as they change. Only a newer format is refused.
                if u.chunk_format > CHUNK_FORMAT_VERSION || u.chunk_format == 0 {
                    bail!(
                        "universe chunk format {} is newer than this server's {}; update the server",
                        u.chunk_format,
                        CHUNK_FORMAT_VERSION
                    );
                }
                if u.chunk_format < CHUNK_FORMAT_VERSION {
                    tracing::info!(
                        "universe chunks are in format {}; they open as they are and are saved in format {}",
                        u.chunk_format,
                        CHUNK_FORMAT_VERSION
                    );
                    let tx = self.db.begin_write()?;
                    tx.open_table(META)?.insert(
                        "universe",
                        postcard::to_stdvec(&UniverseMeta {
                            chunk_format: CHUNK_FORMAT_VERSION,
                            ..u
                        })?
                        .as_slice(),
                    )?;
                    tx.commit()?;
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
        if meta.chunk_format > CHUNK_FORMAT_VERSION || meta.chunk_format == 0 {
            bail!(
                "planet chunk format {} is newer than this server's {}; update the server",
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
        upgrade_3_to_4(&tx)?;
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

    /// Every account's reclaim. A corrupt record is reported and skipped
    /// (left on disk untouched).
    pub fn reclaims(&self) -> Result<Vec<(String, SavedReclaim)>> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(RECLAIM)?;
        let mut out = Vec::new();
        for row in t.iter()? {
            let (k, v) = row?;
            match postcard::from_bytes(v.value()) {
                Ok(r) => out.push((k.value().to_string(), r)),
                Err(e) => tracing::error!("the reclaim of `{}` is corrupt ({e}); skipped", k.value()),
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

    /// Every portal in the universe, and the next free portal number (above
    /// every stored key, even a corrupt one, and never below the saved next
    /// number, so a number is never used twice).
    pub fn portals(&self) -> Result<(Vec<(u32, SavedPortal)>, u32)> {
        let tx = self.db.begin_read()?;
        let mut next = match tx.open_table(META)?.get("next_portal")? {
            Some(v) => postcard::from_bytes(v.value()).context("next portal number is corrupt")?,
            None => 1,
        };
        let t = tx.open_table(PORTALS)?;
        let mut out = Vec::new();
        for row in t.iter()? {
            let (k, v) = row?;
            let id = k.value();
            next = next.max(id.saturating_add(1));
            match postcard::from_bytes(v.value()) {
                Ok(p) => out.push((id, p)),
                Err(e) => tracing::error!("saved portal {id} is corrupt ({e}); skipped"),
            }
        }
        Ok((out, next))
    }

    /// A planet's growing crops (none before save version 10). A corrupt
    /// record is reported and skipped (its block reads as wild).
    pub fn crops(&self, planet: u16) -> Result<Vec<(BlockPos, SavedCrop)>> {
        let tx = self.db.begin_read()?;
        let t = match tx.open_table(CROPS) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let (lo, hi) = planet_range(planet);
        let mut out = Vec::new();
        for row in t.range(lo.as_slice()..hi.as_slice())? {
            let (k, v) = row?;
            match (block_from_key(k.value()), postcard::from_bytes(v.value())) {
                (Some(p), Ok(c)) => out.push((p, c)),
                _ => tracing::error!("a saved crop is corrupt; skipped"),
            }
        }
        Ok(out)
    }

    /// A planet's saved sky (None before save version 7 or for a new planet).
    pub fn sky(&self, planet: u16) -> Result<Option<SavedSky>> {
        let tx = self.db.begin_read()?;
        let t = match tx.open_table(SKIES) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(match t.get(planet)? {
            Some(v) => match postcard::from_bytes(v.value()) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::error!("planet {planet}'s saved meteors are corrupt ({e}); starting fresh");
                    None
                }
            },
            None => None,
        })
    }

    /// The regions an account has discovered, by planet id.
    pub fn discoveries(&self, key: &str) -> Result<Discoveries> {
        let tx = self.db.begin_read()?;
        let t = match tx.open_table(DISCOVERIES) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Discoveries::new()),
            Err(e) => return Err(e.into()),
        };
        Ok(match t.get(key)? {
            Some(v) => postcard::from_bytes(v.value()).context("discoveries are corrupt")?,
            None => Discoveries::new(),
        })
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
            let mut crops = tx.open_table(CROPS)?;
            for (planet, p, c) in &batch.crops {
                let key = block_key(*planet, *p);
                match c {
                    Some(c) => crops.insert(key.as_slice(), postcard::to_stdvec(c)?.as_slice())?,
                    None => crops.remove(key.as_slice())?,
                };
            }
            drop(crops);
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
            let mut reclaim = tx.open_table(RECLAIM)?;
            for (key, r) in &batch.reclaim {
                match r {
                    Some(r) => reclaim.insert(key.as_str(), postcard::to_stdvec(r)?.as_slice())?,
                    None => reclaim.remove(key.as_str())?,
                };
            }
            let mut portals = tx.open_table(PORTALS)?;
            for (id, p) in &batch.portals {
                match p {
                    Some(p) => portals.insert(*id, postcard::to_stdvec(p)?.as_slice())?,
                    None => portals.remove(*id)?,
                };
            }
            if let Some(next) = batch.next_portal {
                tx.open_table(META)?
                    .insert("next_portal", postcard::to_stdvec(&next)?.as_slice())?;
            }
            let mut skies = tx.open_table(SKIES)?;
            for (planet, sky) in &batch.skies {
                skies.insert(*planet, postcard::to_stdvec(sky)?.as_slice())?;
            }
            let mut found = tx.open_table(DISCOVERIES)?;
            for (key, d) in &batch.discoveries {
                found.insert(key.as_str(), postcard::to_stdvec(d)?.as_slice())?;
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

/// Version 4 gives every station a fuel slot, banked heat and a waiting
/// flag: each version 3 row is rewritten with an empty slot, no heat and its
/// craft paid (a job under way keeps going as it did). A row that does not
/// read as version 3 is left untouched and reported, as `stations` reports
/// a corrupt row. Returns how many rows were rewritten.
fn upgrade_3_to_4(tx: &WriteTransaction) -> Result<usize> {
    let mut t = tx.open_table(STATIONS)?;
    let old: Vec<(Vec<u8>, Vec<u8>)> = t
        .iter()?
        .map(|r| r.map(|(k, v)| (k.value().to_vec(), v.value().to_vec())))
        .collect::<Result<_, _>>()?;
    let mut n = 0;
    for (k, v) in old {
        match postcard::from_bytes::<v3::SavedStation>(&v) {
            Ok(s) => {
                let new: SavedStation = s.into();
                t.insert(k.as_slice(), postcard::to_stdvec(&new)?.as_slice())?;
                n += 1;
            }
            Err(e) => tracing::error!("a version 3 station is corrupt ({e}); left as it was"),
        }
    }
    Ok(n)
}

/// Version 5 gives each station the craft XP its output holds: version 4
/// rows are rewritten holding none. (Version 3 rows go straight to the
/// current layout in [`upgrade_3_to_4`].)
fn upgrade_stations_4_to_5(tx: &WriteTransaction) -> Result<usize> {
    let mut t = tx.open_table(STATIONS)?;
    let old: Vec<(Vec<u8>, Vec<u8>)> = t
        .iter()?
        .map(|r| r.map(|(k, v)| (k.value().to_vec(), v.value().to_vec())))
        .collect::<Result<_, _>>()?;
    let mut n = 0;
    for (k, v) in old {
        match postcard::from_bytes::<v4::SavedStation>(&v) {
            Ok(s) => {
                let new: SavedStation = s.into();
                t.insert(k.as_slice(), postcard::to_stdvec(&new)?.as_slice())?;
                n += 1;
            }
            Err(e) => tracing::error!("a version 4 station is corrupt ({e}); left as it was"),
        }
    }
    Ok(n)
}

/// Version 8 gives each station its wear and spark: rows of versions 5 to 7
/// (one layout) are rewritten fully repaired with no spark. (Older rows go
/// straight to the current layout in the steps above.)
/// Rewrites every station row in the current layout: `read` decodes one in
/// the layout of save version `from` (5 to 7: no wear or spark, so fully
/// repaired with none; 8: no containers, so jobs get fresh buckets).
fn upgrade_stations(
    tx: &WriteTransaction,
    from: u32,
    read: impl Fn(&[u8]) -> Result<SavedStation, postcard::Error>,
) -> Result<usize> {
    let mut t = tx.open_table(STATIONS)?;
    let old: Vec<(Vec<u8>, Vec<u8>)> = t
        .iter()?
        .map(|r| r.map(|(k, v)| (k.value().to_vec(), v.value().to_vec())))
        .collect::<Result<_, _>>()?;
    let mut n = 0;
    for (k, v) in old {
        match read(&v) {
            Ok(new) => {
                t.insert(k.as_slice(), postcard::to_stdvec(&new)?.as_slice())?;
                n += 1;
            }
            Err(e) => tracing::error!("a version {from} station is corrupt ({e}); left as it was"),
        }
    }
    Ok(n)
}

/// Rewrites every player row in the current layout: `read` decodes one in
/// the layout of save version `from`. Version 5 saved each character's
/// progression (a version 3 or 4 player, the same layout, becomes level 1
/// with no XP, keeping everything else); version 6 adds health lost and XP
/// debt (older players start unhurt with no debt). A row that does not read
/// is left untouched and reported (its login then fails cleanly rather than
/// starting afresh over it).
fn upgrade_players(
    tx: &WriteTransaction,
    from: u32,
    read: impl Fn(&[u8]) -> Result<PlayerData, postcard::Error>,
) -> Result<usize> {
    let mut t = tx.open_table(PLAYER_DATA)?;
    let old: Vec<(String, Vec<u8>)> = t
        .iter()?
        .map(|r| r.map(|(k, v)| (k.value().to_string(), v.value().to_vec())))
        .collect::<Result<_, _>>()?;
    let mut n = 0;
    for (key, v) in old {
        match read(&v) {
            Ok(new) => {
                t.insert(key.as_str(), postcard::to_stdvec(&new)?.as_slice())?;
                n += 1;
            }
            Err(e) => tracing::error!("version {from} player data of `{key}` is corrupt ({e}); left as it was"),
        }
    }
    Ok(n)
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
            match postcard::from_bytes::<SavedBody>(v.value()) {
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
                        character: SavedCharacter::default(),
                        hurt: 0,
                        food: SavedFood::default(),
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
    /// Crops by planet id; None deletes (harvested away or broken).
    pub crops: Vec<(u16, BlockPos, Option<SavedCrop>)>,
    /// Storage block slots by planet id; None deletes (it was emptied).
    pub storage: Vec<(u16, BlockPos, Option<SavedSlots>)>,
    /// Reclaims by lowercased account name; None deletes (it was emptied).
    pub reclaim: Vec<(String, Option<SavedReclaim>)>,
    /// Portals by number; None deletes (it was broken or lapsed).
    pub portals: Vec<(u32, Option<SavedPortal>)>,
    /// The next portal number, when it changed.
    pub next_portal: Option<u32>,
    /// Planet skies that changed (regions' rests, meteors), by planet id.
    pub skies: Vec<(u16, SavedSky)>,
    /// Discoveries that changed, by lowercased account name.
    pub discoveries: Vec<(String, Discoveries)>,
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
            && self.crops.is_empty()
            && self.reclaim.is_empty()
            && self.portals.is_empty()
            && self.next_portal.is_none()
            && self.skies.is_empty()
            && self.discoveries.is_empty()
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
        pub state: Option<SavedBody>,
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
        stations: &[(BlockPos, V3SavedStation)],
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

    fn kinds() -> PlanetKinds {
        PlanetKinds::builtin(&bm_core::BlockRegistry::builtin())
    }

    fn at(x: f64) -> SavedBody {
        SavedBody {
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
            fuel: Some(SavedStack {
                item: "coal".into(),
                count: 4,
                wear: 0,
            }),
            heat: 50,
            waiting: true,
            xp: 17,
            wear: bm_core::spark::MACHINE_WEAR,
            spark: 0,
            burn_total: 0,
            burn_elapsed: 0,
            blocks: 1,
            containers: Vec::new(),
        };
        let claim = SavedClaim {
            planet: 2,
            owner: "pendragon".into(),
            owner_name: "Pendragon".into(),
            stone: BlockPos::new(1, 2, 3),
            plots: vec![PlotPos::new(0, 0, 0)],
        };
        let spot = BlockPos::new(-5, 40, 1 << 40);
        let portal = SavedPortal {
            planet: 2,
            corner: spot,
            axis: PortalAxis::Z,
            width: 2,
            height: 3,
            owner: "pendragon".into(),
            link: Some(4),
            fuel: 499,
        };
        let sky = SavedSky {
            rest_until: vec![0, 99, 5],
            squares: vec![(1, -2, 400)],
            meteors: vec![SavedMeteor {
                id: 3,
                site: BlockPos::new(5, 140, 6),
                level: 2,
                until: 9000,
                cleared: true,
                entitled: vec!["pendragon".into()],
                looted: vec![],
                shell: vec![BlockPos::new(6, 140, 6)],
                broken: 1,
            }],
        };
        let found: Discoveries = [(2u16, vec![0u16, 4])].into_iter().collect();
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
                        character: SavedCharacter {
                            xp: 12_345,
                            marks: 700,
                            pages: vec![vec![("power".into(), 3)], vec![]],
                            active: 1,
                            cleanses_used: 2,
                            switched_at: 99,
                            plots_bought: 4,
                            debt: 321,
                        },
                        hurt: 77,
                        food: Default::default(),
                    },
                )],
                stations: vec![(2, spot, Some(station.clone()))],
                claims: vec![(4, Some(claim.clone()))],
                storage: vec![(2, spot, Some(vec![None, Some(stack.clone())]))],
                crops: vec![(
                    2,
                    spot,
                    Some(SavedCrop {
                        crop: "oats".into(),
                        sown_at: 77,
                        fertilised_at: None,
                        tilled: true,
                        wet: false,
                        epic: false,
                    }),
                )],
                reclaim: vec![(
                    "pendragon".into(),
                    Some(SavedReclaim {
                        items: vec![stack.clone()],
                        notices: vec!["gone".into()],
                    }),
                )],
                portals: vec![(9, Some(portal.clone()))],
                next_portal: Some(12),
                skies: vec![(2, sky.clone())],
                discoveries: vec![("pendragon".into(), found.clone())],
                clock: Some(777),
                done: Vec::new(),
            })
            .unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.universe(&kinds()).unwrap(), None);
        assert_eq!(s.sky(2).unwrap(), Some(sky), "skies round-trip");
        assert_eq!(s.sky(3).unwrap(), None);
        assert_eq!(s.discoveries("pendragon").unwrap(), found);
        assert!(s.discoveries("nobody").unwrap().is_empty());
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
        let c: Character = p.character.clone().into();
        assert_eq!((c.xp, c.marks, c.active, c.plots_bought), (12_345, 700, 1, 4));
        assert_eq!(c.pages[0].rank("power"), 3);
        assert_eq!(SavedCharacter::from(c), p.character, "characters round-trip");
        assert_eq!(s.stations(2).unwrap(), vec![(spot, station)]);
        assert_eq!(s.storage(2).unwrap(), vec![(spot, vec![None, Some(stack.clone())])]);
        assert_eq!(
            s.crops(2).unwrap(),
            vec![(
                spot,
                SavedCrop {
                    crop: "oats".into(),
                    sown_at: 77,
                    fertilised_at: None,
                    tilled: true,
                    wet: false,
                    epic: false
                }
            )]
        );
        assert!(s.crops(3).unwrap().is_empty());
        assert!(s.storage(3).unwrap().is_empty());
        assert!(s.stations(1).unwrap().is_empty() && s.stations(3).unwrap().is_empty());
        assert_eq!(s.claims().unwrap(), (vec![(4, claim)], 5));
        assert_eq!(s.clock().unwrap(), 777);
        assert_eq!(
            s.reclaims().unwrap(),
            vec![(
                "pendragon".to_string(),
                SavedReclaim {
                    items: vec![stack.clone()],
                    notices: vec!["gone".into()],
                }
            )]
        );

        assert_eq!(s.portals().unwrap(), (vec![(9, portal)], 12));

        // Deletions.
        s.write(&WriteBatch {
            portals: vec![(9, None)],
            stations: vec![(2, spot, None)],
            claims: vec![(4, None)],
            storage: vec![(2, spot, None)],
            crops: vec![(2, spot, None)],
            reclaim: vec![("pendragon".into(), None)],
            ..Default::default()
        })
        .unwrap();
        assert!(s.reclaims().unwrap().is_empty());
        assert!(s.stations(2).unwrap().is_empty());
        assert!(s.storage(2).unwrap().is_empty());
        assert!(s.crops(2).unwrap().is_empty());
        assert_eq!(s.claims().unwrap(), (vec![], 1));
        assert_eq!(s.portals().unwrap(), (vec![], 12), "a number is never used again");
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
            fuel: None,
            heat: 0,
            waiting: false,
            xp: 0,
            wear: bm_core::spark::MACHINE_WEAR,
            spark: 0,
            burn_total: 0,
            burn_elapsed: 0,
            blocks: 1,
            containers: Vec::new(),
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

    /// A version 3 universe (M4 to M10) opens: its stations are rewritten
    /// with an empty fuel slot, no heat and their craft paid, and the
    /// universe is marked version 4; it then reopens as it is.
    #[test]
    fn a_version_3_universe_gains_fuel_and_heat_on_its_stations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("u.redb");
        let old = V3SavedStation {
            kind: Station::Furnace,
            jobs: vec![SavedJob {
                output: "glass".into(),
                output_count: 1,
                inputs: vec![("sand".into(), 2), ("soil".into(), 2)],
                remaining: 2,
            }],
            started: 77,
            output: vec![
                Some(SavedStack {
                    item: "glass".into(),
                    count: 1,
                    wear: 0,
                }),
                None,
            ],
        };
        let spot = BlockPos::new(3, -4, 5);
        {
            let s = Store::open(&path).unwrap();
            let tx = s.db.begin_write().unwrap();
            tx.open_table(META)
                .unwrap()
                .insert(
                    "universe",
                    postcard::to_stdvec(&UniverseMeta {
                        save_version: 3,
                        chunk_format: CHUNK_FORMAT_VERSION,
                    })
                    .unwrap()
                    .as_slice(),
                )
                .unwrap();
            {
                let mut t = tx.open_table(STATIONS).unwrap();
                t.insert(
                    block_key(1, spot).as_slice(),
                    postcard::to_stdvec(&old).unwrap().as_slice(),
                )
                .unwrap();
                // A row no version reads stays as it was.
                t.insert(block_key(1, BlockPos::new(0, 0, 0)).as_slice(), [0xffu8; 3].as_slice())
                    .unwrap();
            }
            tx.commit().unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert!(s.universe(&kinds()).unwrap().is_none());
        let want: SavedStation = old.clone().into();
        assert_eq!((want.fuel.clone(), want.heat, want.waiting), (None, 0, false));
        assert_eq!(s.stations(1).unwrap(), vec![(spot, want.clone())]);
        let version = |s: &Store| {
            let tx = s.db.begin_read().unwrap();
            let bytes = tx
                .open_table(META)
                .unwrap()
                .get("universe")
                .unwrap()
                .unwrap()
                .value()
                .to_vec();
            postcard::from_bytes::<UniverseMeta>(&bytes).unwrap().save_version
        };
        assert_eq!(version(&s), SAVE_VERSION);
        drop(s);
        let s = Store::open(&path).unwrap();
        assert!(
            s.universe(&kinds()).unwrap().is_none(),
            "reopens without upgrading again"
        );
        assert_eq!(s.stations(1).unwrap(), vec![(spot, want)]);
    }

    /// A version 4 universe (M11) opens: every character starts at level 1
    /// with no XP, keeping where they were and what they carried; stations
    /// hold no craft XP; it is marked version 5 and reopens as it is.
    #[test]
    fn a_version_4_universe_gives_every_character_level_1() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("u.redb");
        let stack = SavedStack {
            item: "stone".into(),
            count: 3,
            wear: 0,
        };
        let old_player = V4PlayerData {
            planet: 1,
            state: Some(SavedBody {
                pos: DVec3::new(4.0, 5.0, 6.0),
                vel: DVec3::ZERO,
                on_ground: true,
            }),
            inventory: vec![Some(stack.clone()), None],
            last_seen: 1_700_000_123,
        };
        let old_station = V4SavedStation {
            kind: Station::Furnace,
            jobs: vec![],
            started: 5,
            output: vec![Some(stack.clone())],
            fuel: Some(stack.clone()),
            heat: 40,
            waiting: false,
        };
        let spot = BlockPos::new(3, -4, 5);
        {
            let s = Store::open(&path).unwrap();
            let tx = s.db.begin_write().unwrap();
            tx.open_table(META)
                .unwrap()
                .insert(
                    "universe",
                    postcard::to_stdvec(&UniverseMeta {
                        save_version: 4,
                        chunk_format: CHUNK_FORMAT_VERSION,
                    })
                    .unwrap()
                    .as_slice(),
                )
                .unwrap();
            {
                let mut t = tx.open_table(PLAYER_DATA).unwrap();
                t.insert("alice", postcard::to_stdvec(&old_player).unwrap().as_slice())
                    .unwrap();
                // A row no version reads stays as it was.
                t.insert("broken", [0xffu8; 2].as_slice()).unwrap();
                let mut t = tx.open_table(STATIONS).unwrap();
                t.insert(
                    block_key(1, spot).as_slice(),
                    postcard::to_stdvec(&old_station).unwrap().as_slice(),
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert!(s.universe(&kinds()).unwrap().is_none());
        let p = s.player("alice").unwrap().unwrap();
        let want: PlayerData = old_player.clone().into();
        assert_eq!(p, want);
        assert_eq!(p.inventory, old_player.inventory);
        assert_eq!(p.last_seen, 1_700_000_123);
        let c: Character = p.character.into();
        assert_eq!(c, Character::default(), "level 1, no XP, one empty page");
        assert_eq!(c.level(), 1);
        assert!(s.player("broken").is_err(), "left as it was");
        let st = s.stations(1).unwrap();
        assert_eq!(st.len(), 1);
        assert_eq!((st[0].1.heat, st[0].1.xp, st[0].1.fuel.clone()), (40, 0, Some(stack)));
        drop(s);
        let s = Store::open(&path).unwrap();
        assert!(
            s.universe(&kinds()).unwrap().is_none(),
            "reopens without upgrading again"
        );
        assert_eq!(s.player("alice").unwrap().unwrap(), want);
    }

    /// A version 6 universe (M13) opens as it is: characters keep their
    /// health and XP debt (a version 5 reading of them would garble them),
    /// it has no skies or discoveries yet, and it is marked version 7.
    #[test]
    fn a_version_6_universe_opens_with_characters_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("u.redb");
        let player = PlayerData {
            planet: 1,
            state: None,
            inventory: vec![],
            last_seen: 1_700_000_000,
            character: SavedCharacter {
                xp: 777,
                marks: 5,
                pages: vec![vec![("power".into(), 1)]],
                active: 0,
                cleanses_used: 0,
                switched_at: 0,
                plots_bought: 0,
                debt: 321,
            },
            hurt: 77,
            food: Default::default(),
        };
        {
            let s = Store::open(&path).unwrap();
            let tx = s.db.begin_write().unwrap();
            tx.open_table(META)
                .unwrap()
                .insert(
                    "universe",
                    postcard::to_stdvec(&UniverseMeta {
                        save_version: 6,
                        chunk_format: CHUNK_FORMAT_VERSION,
                    })
                    .unwrap()
                    .as_slice(),
                )
                .unwrap();
            tx.open_table(PLAYER_DATA)
                .unwrap()
                .insert("alice", postcard::to_stdvec(&player).unwrap().as_slice())
                .unwrap();
            tx.commit().unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert!(s.universe(&kinds()).unwrap().is_none());
        assert_eq!(s.player("alice").unwrap().unwrap(), player, "untouched");
        assert_eq!(s.sky(1).unwrap(), None);
        assert!(s.discoveries("alice").unwrap().is_empty());
        let tx = s.db.begin_read().unwrap();
        let meta = tx.open_table(META).unwrap();
        let u: UniverseMeta = postcard::from_bytes(meta.get("universe").unwrap().unwrap().value()).unwrap();
        assert_eq!(u.save_version, SAVE_VERSION);
    }

    /// A version 7 universe (M14) opens with every station fully repaired
    /// and no spark, keeping everything else; it is marked version 8 and
    /// reopens as it is.
    #[test]
    fn a_version_7_universe_gives_stations_full_wear_and_no_spark() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("u.redb");
        let stack = SavedStack {
            item: "coal".into(),
            count: 3,
            wear: 0,
        };
        let old = V7SavedStation {
            kind: Station::Workbench,
            jobs: vec![SavedJob {
                output: "cog".into(),
                output_count: 1,
                inputs: vec![("copper_ingot".into(), 1)],
                remaining: 2,
            }],
            started: 5,
            output: vec![Some(stack.clone())],
            fuel: None,
            heat: 0,
            waiting: false,
            xp: 33,
        };
        let spot = BlockPos::new(3, 40, 5);
        {
            let s = Store::open(&path).unwrap();
            let tx = s.db.begin_write().unwrap();
            tx.open_table(META)
                .unwrap()
                .insert(
                    "universe",
                    postcard::to_stdvec(&UniverseMeta {
                        save_version: 7,
                        chunk_format: CHUNK_FORMAT_VERSION,
                    })
                    .unwrap()
                    .as_slice(),
                )
                .unwrap();
            tx.open_table(STATIONS)
                .unwrap()
                .insert(
                    block_key(1, spot).as_slice(),
                    postcard::to_stdvec(&old).unwrap().as_slice(),
                )
                .unwrap();
            tx.commit().unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert!(s.universe(&kinds()).unwrap().is_none());
        let st = s.stations(1).unwrap();
        let want: SavedStation = old.clone().into();
        assert_eq!(st, vec![(spot, want.clone())]);
        assert_eq!(
            (st[0].1.wear, st[0].1.spark, st[0].1.xp, st[0].1.jobs.len()),
            (bm_core::spark::MACHINE_WEAR, 0, 33, 1)
        );
        drop(s);
        let s = Store::open(&path).unwrap();
        assert!(s.universe(&kinds()).unwrap().is_none(), "reopens as it is");
        assert_eq!(s.stations(1).unwrap(), vec![(spot, want)]);
        let tx = s.db.begin_read().unwrap();
        let meta = tx.open_table(META).unwrap();
        let u: UniverseMeta = postcard::from_bytes(meta.get("universe").unwrap().unwrap().value()).unwrap();
        assert_eq!(u.save_version, SAVE_VERSION);
    }

    /// A version 8 universe (M15) opens: characters keep everything and
    /// start unhungry with no food buff; stations keep everything, their
    /// jobs holding no buckets; it is marked version 9.
    #[test]
    fn a_version_8_universe_starts_everyone_unhungry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("u.redb");
        let stack = SavedStack {
            item: "stone".into(),
            count: 3,
            wear: 0,
        };
        let old_player = V8PlayerData {
            planet: 1,
            state: None,
            inventory: vec![Some(stack.clone()), None],
            last_seen: 1_700_000_123,
            character: SavedCharacter::default(),
            hurt: 77,
        };
        let old_station = V8SavedStation {
            kind: Station::Workbench,
            jobs: vec![SavedJob {
                output: "cog".into(),
                output_count: 1,
                inputs: vec![("copper_ingot".into(), 1)],
                remaining: 2,
            }],
            started: 5,
            output: vec![Some(stack.clone())],
            fuel: None,
            heat: 0,
            waiting: false,
            xp: 33,
            wear: 1234,
            spark: 0,
            burn_total: 0,
            burn_elapsed: 0,
            blocks: 1,
        };
        let spot = BlockPos::new(3, 40, 5);
        {
            let s = Store::open(&path).unwrap();
            let tx = s.db.begin_write().unwrap();
            tx.open_table(META)
                .unwrap()
                .insert(
                    "universe",
                    postcard::to_stdvec(&UniverseMeta {
                        save_version: 8,
                        chunk_format: CHUNK_FORMAT_VERSION,
                    })
                    .unwrap()
                    .as_slice(),
                )
                .unwrap();
            tx.open_table(PLAYER_DATA)
                .unwrap()
                .insert("alice", postcard::to_stdvec(&old_player).unwrap().as_slice())
                .unwrap();
            tx.open_table(STATIONS)
                .unwrap()
                .insert(
                    block_key(1, spot).as_slice(),
                    postcard::to_stdvec(&old_station).unwrap().as_slice(),
                )
                .unwrap();
            tx.commit().unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert!(s.universe(&kinds()).unwrap().is_none());
        let p = s.player("alice").unwrap().unwrap();
        assert_eq!(
            (p.hurt, p.inventory.clone(), p.food),
            (77, old_player.inventory.clone(), SavedFood::default())
        );
        let st = s.stations(1).unwrap();
        assert_eq!(st, vec![(spot, old_station.clone().into())]);
        assert_eq!((st[0].1.wear, st[0].1.containers.len()), (1234, 0));
        drop(s);
        let s = Store::open(&path).unwrap();
        assert!(s.universe(&kinds()).unwrap().is_none(), "reopens as it is");
        assert_eq!(s.player("alice").unwrap().unwrap(), p);
    }

    /// A version 5 universe (M12) opens: every character keeps its level,
    /// marks and pages, and starts unhurt with no XP debt; stations are as
    /// they were; it is marked version 6 and reopens as it is.
    #[test]
    fn a_version_5_universe_keeps_progress_and_starts_unhurt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("u.redb");
        let stack = SavedStack {
            item: "stone".into(),
            count: 3,
            wear: 0,
        };
        let old_player = V5PlayerData {
            planet: 1,
            state: Some(SavedBody {
                pos: DVec3::new(4.0, 5.0, 6.0),
                vel: DVec3::ZERO,
                on_ground: true,
            }),
            inventory: vec![Some(stack.clone()), None],
            last_seen: 1_700_000_123,
            character: V5SavedCharacter {
                xp: 54_321,
                marks: 90,
                pages: vec![vec![("power".into(), 2)]],
                active: 0,
                cleanses_used: 1,
                switched_at: 7,
                plots_bought: 3,
            },
        };
        let station = SavedStation {
            kind: Station::Furnace,
            jobs: vec![],
            started: 5,
            output: vec![Some(stack.clone())],
            fuel: None,
            heat: 40,
            waiting: false,
            xp: 9,
            wear: bm_core::spark::MACHINE_WEAR,
            spark: 0,
            burn_total: 0,
            burn_elapsed: 0,
            blocks: 1,
            containers: Vec::new(),
        };
        let spot = BlockPos::new(3, -4, 5);
        {
            let s = Store::open(&path).unwrap();
            let tx = s.db.begin_write().unwrap();
            tx.open_table(META)
                .unwrap()
                .insert(
                    "universe",
                    postcard::to_stdvec(&UniverseMeta {
                        save_version: 5,
                        chunk_format: CHUNK_FORMAT_VERSION,
                    })
                    .unwrap()
                    .as_slice(),
                )
                .unwrap();
            {
                let mut t = tx.open_table(PLAYER_DATA).unwrap();
                t.insert("alice", postcard::to_stdvec(&old_player).unwrap().as_slice())
                    .unwrap();
                t.insert("broken", [0xffu8; 2].as_slice()).unwrap();
                let mut t = tx.open_table(STATIONS).unwrap();
                t.insert(
                    block_key(1, spot).as_slice(),
                    postcard::to_stdvec(&station).unwrap().as_slice(),
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert!(s.universe(&kinds()).unwrap().is_none());
        let p = s.player("alice").unwrap().unwrap();
        assert_eq!(
            (p.planet, p.state, p.inventory.clone()),
            (1, old_player.state, old_player.inventory.clone())
        );
        assert_eq!(p.hurt, 0, "unhurt");
        let c: Character = p.character.clone().into();
        assert_eq!((c.xp, c.marks, c.cleanses_used, c.plots_bought), (54_321, 90, 1, 3));
        assert_eq!(c.pages[0].rank("power"), 2);
        assert_eq!(c.debt, 0, "no debt");
        assert!(s.player("broken").is_err(), "left as it was");
        assert_eq!(s.stations(1).unwrap(), vec![(spot, station)], "stations unchanged");
        drop(s);
        let s = Store::open(&path).unwrap();
        assert!(
            s.universe(&kinds()).unwrap().is_none(),
            "reopens without upgrading again"
        );
        assert_eq!(s.player("alice").unwrap().unwrap(), p);
        // Health and debt are kept from now on.
        s.write(&WriteBatch {
            players: vec![(
                "alice".into(),
                PlayerData {
                    hurt: 250,
                    character: SavedCharacter {
                        debt: 75,
                        ..p.character.clone()
                    },
                    ..p.clone()
                },
            )],
            ..Default::default()
        })
        .unwrap();
        let p = s.player("alice").unwrap().unwrap();
        assert_eq!((p.hurt, p.character.debt), (250, 75));
    }

    /// A universe saved before chunk format 2 opens (old chunks still decode)
    /// and is marked current; one from a newer server is refused. Saves made
    /// by the M4 and M5 servers are format 1 (2026-10-19).
    #[test]
    fn older_chunk_formats_open_and_newer_are_refused() {
        let with_format = |format: u8| {
            let dir = tempfile::tempdir().unwrap();
            let s = Store::open(&dir.path().join("u.redb")).unwrap();
            let tx = s.db.begin_write().unwrap();
            tx.open_table(META)
                .unwrap()
                .insert(
                    "universe",
                    postcard::to_stdvec(&UniverseMeta {
                        save_version: SAVE_VERSION,
                        chunk_format: format,
                    })
                    .unwrap()
                    .as_slice(),
                )
                .unwrap();
            tx.commit().unwrap();
            (dir, s)
        };
        let (_dir, s) = with_format(1);
        assert!(s.universe(&kinds()).is_ok(), "format 1 opens");
        let rtx = s.db.begin_read().unwrap();
        let bytes = rtx
            .open_table(META)
            .unwrap()
            .get("universe")
            .unwrap()
            .unwrap()
            .value()
            .to_vec();
        let u: UniverseMeta = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(u.chunk_format, CHUNK_FORMAT_VERSION, "and is marked current");
        let (_dir, s) = with_format(CHUNK_FORMAT_VERSION + 1);
        assert!(s.universe(&kinds()).is_err(), "a newer format is refused");
    }

    /// Writes a planet exactly as save version 1 did: its meta, a chunk, and
    /// the old players table.
    fn write_v1_planet(path: &Path, players: &[(&str, SavedBody)]) {
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
        let st = V3SavedStation {
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
        assert_eq!(s.stations(0).unwrap(), vec![(BlockPos::new(1, -2, 3), st.into())]);
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
        // Tokens carry their portal after an `@`, after any colour.
        let token = items.id("portal_token").unwrap();
        let mut inv = Inventory::default();
        inv.slots[0] = Some(ItemStack::tagged(token, 17));
        inv.slots[1] = Some(ItemStack {
            colour: Colour(4),
            ..ItemStack::tagged(token, u32::MAX)
        });
        inv.slots[2] = Some(ItemStack::new(token, 1));
        let saved = save_slots(&items, &inv);
        let names: Vec<_> = saved
            .iter()
            .take(3)
            .map(|s| s.as_ref().unwrap().item.as_str())
            .collect();
        assert_eq!(names, ["portal_token@17", "portal_token#4@4294967295", "portal_token"]);
        assert_eq!(load_slots(&items, &saved, "test"), inv.slots);
        let bad = vec![Some(SavedStack {
            item: "portal_token@x".into(),
            count: 1,
            wear: 0,
        })];
        assert_eq!(load_slots(&items, &bad, "test"), vec![None], "a bad tag is no item");
        let gone = vec![Some(SavedStack {
            item: "unobtainium".into(),
            count: 1,
            wear: 0,
        })];
        assert_eq!(load_slots(&items, &gone, "test"), vec![None]);
    }
}
