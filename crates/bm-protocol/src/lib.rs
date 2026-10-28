//! The wire protocol (ADR-0004).
//!
//! Two channels share one QUIC connection:
//!
//! - **Reliable and ordered**: one bidirectional stream carrying [`ClientMsg`]
//!   and [`ServerMsg`] frames. World state travels here (chunks, block
//!   changes, edit results), and ordering matters: a block change must never
//!   overtake the chunk it applies to.
//! - **Unreliable datagrams**: [`ClientDatagram`] inputs and [`ServerDatagram`]
//!   snapshots. Movement is sent many times a second and every packet is
//!   superseded by the next, so a lost one is never resent. Inputs carry the
//!   last few ticks redundantly so a single loss costs nothing.
//!
//! Everything is `postcard` (compact varints). Reliable frames are prefixed with
//! a little-endian `u32` length. Every message from the other side is untrusted:
//! decoders bound sizes, and the server validates content.

use bm_core::{
    BlockDef, BlockPos, BlockState, ChunkPos, ColourPalette, DayCycle, Inventory, ItemDef, ItemStack, Mount,
    MoveConfig, PlanetLook, PlanetShape, PlayerInput, PlayerState, PlotPos, Recipe, Station,
    creatures::CreatureDef,
    skills::{Character, SkillDef},
};
use glam::DVec3;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Bump on any change to a message below. Client and server must match.
pub const PROTOCOL_VERSION: u32 = 54;

/// QUIC application protocol name; also refuses non-Boundmore peers early.
pub const ALPN: &[u8] = b"boundmore/1";

/// Largest reliable frame accepted (a chunk is at most ~300 KiB encoded).
pub const MAX_FRAME: usize = 1 << 20;

/// Inputs repeated per datagram. At 60 Hz, 6 inputs survive 5 consecutive
/// lost packets (~83 ms) without the server missing a tick.
pub const INPUT_REDUNDANCY: usize = 6;

pub const MAX_NAME_LEN: usize = 24;

/// Longest chat line, in characters. The server cuts longer ones.
pub const MAX_CHAT_LEN: usize = 240;
/// Blocks within which `Channel::Local` is heard.
pub const LOCAL_CHAT_RANGE: f64 = 96.0;

/// Passwords: at least this long. (Longer is checked by `MAX_PASSWORD_LEN`.)
pub const MIN_PASSWORD_LEN: usize = 8;
pub const MAX_PASSWORD_LEN: usize = 128;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// The first message on a connection. The password travels only inside
    /// QUIC's TLS and is never stored or logged in clear (Argon2id at rest).
    Hello {
        protocol: u32,
        name: String,
        password: String,
        /// Create the account (refused if the name is taken).
        register: bool,
    },
    /// One swing of the item in hotbar `slot` (a tool, or anything: a
    /// non-tool swings as a bare hand) at the block `pos`, following input
    /// `at` (queued with the edits, like `Edit`). The server checks reach and
    /// the time since the last swing on the input timeline
    /// (`mining::swing_ticks` of the item that made it), then damages every
    /// block the swing hits (`mining::aoe_blocks` from the look at `at`).
    /// Blocks it breaks are answered with `EditResult { seq, .. }` each;
    /// blocks it only damages with `BlockHealth`. A refused swing is
    /// answered with `EditResult { accepted: false }` (the true block) and
    /// `BlockHealth` (its true damage) for the hit block.
    Swing { seq: u32, at: u32, pos: BlockPos, slot: u8 },
    /// `at` is the input sequence the edit follows: the server applies it
    /// right after simulating that input, exactly where the client's
    /// prediction put it, so both judge "is a player in the way" on the same
    /// body position.
    Edit {
        seq: u32,
        at: u32,
        pos: BlockPos,
        action: EditAction,
    },
    /// Move up to `count` items between inventory slots (merge, split, or a
    /// whole-stack swap; see `Inventory::move_items`).
    MoveItems { from: u8, to: u8, count: u16 },
    /// Throw a whole slot away.
    Discard { slot: u8 },
    /// Craft recipe `recipe` (an index into `Welcome::recipes`) `count`
    /// times. Hand recipes need no station and finish at once; station
    /// recipes name a station block of the right kind within reach, where
    /// they queue.
    Craft {
        recipe: u16,
        count: u16,
        station: Option<BlockPos>,
        /// For a recipe that passes a colour on (`Recipe::tint`, milestone
        /// 22), the colour to make; None lets the server pick the colour
        /// most crafts can be made in. Ignored for other recipes.
        colour: Option<bm_core::Colour>,
    },
    /// Open or shut the door or trapdoor at `pos` (milestone 22 part 2),
    /// with every door block joined to it. The server answers with block
    /// changes, or a `Notice` saying why not.
    SwingDoor { pos: BlockPos },
    /// Write (or, with a blank text, unwrite) the sign at `pos` (milestone
    /// 22 part 4). A first write on a panel joins the panels around it. The
    /// server answers with `Sign`, or a `Notice` saying why not.
    WriteSign {
        pos: BlockPos,
        text: bm_core::signs::SignText,
    },
    /// Start (or with `None`, stop) receiving `Station` updates for one
    /// station, e.g. while its screen is open.
    WatchStation { pos: Option<BlockPos> },
    /// Move a station's finished output into the inventory (as much as fits).
    CollectStation { pos: BlockPos },
    /// Move up to `count` fuel between inventory `slot` and the furnace at
    /// `pos` (any of its blocks, in reach): `into` puts fuel in (only items
    /// with `fuel > 0`; merging with what is there), else takes it out. The
    /// server answers with `Slots` and `Station` (milestone 11).
    StationFuel {
        pos: BlockPos,
        slot: u8,
        count: u16,
        into: bool,
    },
    /// A change to the character's skills (milestone 12), following input
    /// `at` (queued with the edits): what the active page does changes from
    /// that input on, on both sides (movement, reach, energy). Answered with
    /// `Character`, or a `Notice` saying why not.
    Skill { at: u32, action: SkillAction },
    /// Add a plot to one of your claims; it must touch the claim by a face
    /// and be unclaimed, within your allowance.
    ClaimPlot { claim: u32, plot: PlotPos },
    /// Give up one plot of your claims (never the one holding its claim stone).
    ReleasePlot { plot: PlotPos },
    /// Travel to another planet from a warp shrine within reach. The server
    /// answers with `Travel`, or a `Notice` saying why not.
    Warp { shrine: BlockPos, planet: u16 },
    /// Start (Some) or stop (None) receiving a storage block within reach;
    /// the server answers with `Storage` and repeats it on every change.
    OpenStorage { pos: Option<BlockPos> },
    /// Open (Some) or close (None) a shop stand or request basket within
    /// reach (milestone 24 part 2): answered with `Shop`, repeated on every
    /// change. Its owner also gets it as storage (`Storage`).
    OpenShop { pos: Option<BlockPos> },
    /// Do something at the open shop stand or basket.
    Shop { pos: BlockPos, action: ShopAction },
    /// Open (Some) the atlas in this inventory slot, binding it to the world
    /// we stand on if it is not bound yet, or close (None) the open one
    /// (milestone 27). Answered with `Atlas` and the discovered tiles
    /// (`AtlasTile`), sent again as they change while it is open.
    OpenAtlas { slot: Option<u8> },
    /// Ask, accept, decline or end a friendship with the player of this
    /// name (milestone 25). Answered with `Friends`.
    Friend { name: String, action: FriendAction },
    /// Give a friend these rights (`bm_core::rights`) on one of our claims;
    /// 0 takes them all. Answered with the claim's `Claim`.
    SetRights { claim: u32, friend: String, rights: u8 },
    /// Open (Some) or close (None) the guild control within reach
    /// (milestone 25 part 2): answered with `GuildControl`.
    OpenGuildControl { pos: Option<BlockPos> },
    /// Open (Some) or close (None) the helix machine within reach
    /// (milestone 25 part 3): answered with `Helix`.
    OpenHelix { pos: Option<BlockPos> },
    /// Do something in a guild. Answered with `Guilds`, and a notice when
    /// refused.
    Guild(GuildAction),
    /// Move up to `count` between the inventory and the open storage block,
    /// or within either: same rules as `MoveItems` (merge, split, whole-stack
    /// swap). The server answers with `Slots` and/or `Storage`.
    StorageMove {
        pos: BlockPos,
        from: SlotRef,
        to: SlotRef,
        count: u16,
    },
    /// A chat line. `Whisper` goes to one player by name, anywhere.
    Chat { channel: Channel, text: String },
    /// Take reclaimed items (from a lapsed claim) into the inventory, as many
    /// as fit, at a warp shrine within reach. Answered with `Slots` and
    /// `Reclaim`.
    CollectReclaim { shrine: BlockPos },
    /// Start (Some: any conduit of the portal, within reach) or stop (None)
    /// watching a portal's window (ADR-0009); the server answers with
    /// `Portal` and repeats it on every change, or a `Notice` saying why not.
    OpenPortal { pos: Option<BlockPos> },
    /// Act on the open portal (`pos`: any of its conduits or surface blocks).
    Portal { pos: BlockPos, action: PortalAction },
    /// Fires the sling in `slot` right after input `at`, along `yaw` and
    /// `pitch` (milestone 13). `charge`: ticks held with Draw (the server
    /// clamps it). Energy is spent at `at` on both sides, as for swings.
    Shoot {
        at: u32,
        slot: u8,
        yaw: u16,
        pitch: i16,
        charge: u16,
    },
    /// Swings the tool in `slot` at a creature right after input `at`
    /// (tools and the hand stone hit creatures, on the swing timing).
    Strike { at: u32, creature: u32, slot: u8 },
    /// Hits a dead creature's body for your own drops (once per body).
    Loot { creature: u32 },
    /// Defeated: come back on safe ground near where you fell.
    Return,
    /// Throws a bomb from the bag in `slot` right after input `at`, along
    /// `yaw` and `pitch`, held `hold` ticks since it was lit (the server
    /// clamps it; a short hold drops it at your feet, `combat::throw_speed`;
    /// its fuse ran from the light). Energy and one use are spent at `at`.
    Throw {
        at: u32,
        slot: u8,
        yaw: u16,
        pitch: i16,
        hold: u16,
    },
    /// Takes your own share from the meteor heart at `pos` (milestone 14),
    /// once per meteor.
    LootHeart { pos: BlockPos },
    /// Eats one of the food in hotbar `slot` right after input `at`
    /// (milestone 16): its hunger off, its buff on, raw meat's sickness. The
    /// client applies the same at `at` (`PlayerState::eat`); the server
    /// answers with `Slots`, or nothing if it refuses.
    Eat { at: u32, slot: u8 },
    /// Fills one empty bucket from hotbar `slot` at the water block `pos`,
    /// within reach (milestone 16). Answered with `Slots`, or a `Notice`.
    FillBucket { slot: u8, pos: BlockPos },
    /// What is the crop at `pos`? (milestone 16) Answered with `CropInfo`
    /// if it is one.
    LookCrop { pos: BlockPos },
    /// Starts a forging session at the forge at `pos` (any of its blocks, in
    /// reach; milestone 17): the gear in inventory slot `gear`, and a deck of
    /// (inventory slot, count) ingredient stacks, moved into the forge.
    /// Answered with `Slots` and `Station`, or a `Notice`.
    ForgeBegin {
        pos: BlockPos,
        gear: u8,
        deck: Vec<(u8, u16)>,
    },
    /// Uses one ingredient from deck slot `slot` in the session at `pos`: one
    /// round. `round` is the number of rounds already played as the client
    /// saw it; a mismatch (a double click) is ignored. Answered with
    /// `Station`, and `Forged` if the session ended.
    ForgeUse { pos: BlockPos, slot: u8, round: u32 },
    /// Creative mode on or off, right after input `at` (a testing aid; the
    /// server allows it only when started with `--creative`). Predicted at
    /// once; answered with `Creative`, which says what holds.
    Creative { at: u32, on: bool },
    /// A typed command for the server (`/give item count`, `/heal`): only
    /// on a server that allows creative mode. Answered with a `Notice`.
    Command { text: String },
}

/// What the portal window's buttons do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortalAction {
    /// A token bound to this portal, into the pack.
    MakeToken,
    /// Link with the portal the token in this pack slot was made at, using
    /// the token up.
    Link { slot: u8 },
    /// Forget the link, closing both sides.
    Unlink,
    /// Up to `count` wayshards from this pack slot into the tank.
    AddFuel { slot: u8, count: u16 },
    /// Up to `count` wayshards from the tank into the pack.
    TakeFuel { count: u16 },
}

/// What `ClientMsg::Skill` does (`bm_core::skills::Character`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillAction {
    /// One rank of this skill on the active page.
    Learn(String),
    /// Take back one rank (a cleanse point, free below level 20).
    Cleanse(String),
    /// Clear the active page (free below level 20, else 800 marks).
    ResetPage,
    /// A new page for 600 marks.
    BuyPage,
    /// Make this page active (free inside your own claim, else once per 15
    /// minutes).
    SwitchPage(u8),
    /// Buy this many plots (1 to 80) for 30 marks each; they add to the
    /// account's plot allowance (Nathan, 2026-10-21).
    BuyPlots(u16),
}

/// A slot in the inventory or in an open storage block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotRef {
    Inventory(u8),
    Storage(u8),
}

/// Who hears a chat line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    /// Players within `LOCAL_CHAT_RANGE` on the same planet.
    Local,
    /// Everyone on the same planet.
    Planet,
    /// Everyone on the server.
    Universe,
    /// One player, by name (the sender gets a copy). A friend or guildmate
    /// who is away gets it when they next play (milestone 25).
    Whisper(String),
    /// The sender's primary guild (milestone 25 part 2), by its tag as the
    /// server fills it in (what a client sends is ignored).
    Guild(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditAction {
    /// Places the block item in hotbar `slot`, spending one. For a prop (a
    /// torch), `mount` is the face it goes on, from the face the player
    /// clicked: `Floor` on top of the block below, a wall direction against
    /// that side neighbour (`ItemRegistry::prop_block` picks the variant).
    /// None for ordinary blocks.
    Place { slot: u8, mount: Option<Mount> },
    /// Cuts (or, with `restore`, fills back) the octants in `octants` of the
    /// block, with the chisel in hotbar `slot` (ADR-0008). A precise chisel
    /// moves exactly one octant, an ordinary one an edge pair
    /// (`bm_core::edge_pairs`).
    Chisel { slot: u8, octants: u8, restore: bool },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    pub player_id: u32,
    /// The account's name as it was registered.
    pub account: String,
    /// The planet the player is on. Chunks, block changes, claims and
    /// snapshots all belong to it until a `Travel`.
    pub planet: PlanetInfo,
    /// Every planet on this server, for the travel menu and the sky.
    pub universe: Vec<PlanetSummary>,
    /// The block catalogue. The client renders whatever the server says a
    /// block is (ADR-0003); it ships no copy of its own that could disagree.
    pub blocks: Vec<BlockDef>,
    /// The item catalogue (`ItemRegistry::from_defs` on the client).
    pub items: Vec<ItemDef>,
    /// The colour palette (ADR-0007): ids 1 to 255, 0 is natural.
    pub colours: ColourPalette,
    pub recipes: Vec<Recipe>,
    /// The skill tree (`assets/skills.toml`, milestone 12).
    pub skills: Vec<SkillDef>,
    /// This account's progression.
    pub character: Character,
    pub inventory: Inventory,
    pub movement: MoveConfig,
    pub spawn: PlayerState,
    /// Server tick the spawn state belongs to.
    pub tick: u64,
    /// Horizontal chunk radius the server streams around the player.
    pub view_radius: u8,
    /// Horizontal chunk radius of far copies (milestone 20; 0: none).
    pub far_radius: u8,
    /// Furthest a player may edit from their eye, in blocks.
    pub reach: f64,
    /// Every claim on the planet (later: only those near the player).
    pub claims: Vec<ClaimView>,
    /// Plots this account may claim in all.
    pub plot_allowance: u32,
    /// The creature catalogue (`assets/creatures.toml`).
    pub creatures: Vec<CreatureDef>,
}

/// Everything a client needs to be on a planet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanetInfo {
    pub id: u16,
    pub name: String,
    /// The kind's display name ("Arid").
    pub kind: String,
    pub tier: u8,
    /// Its air (milestone 18).
    pub air: bm_core::atmosphere::Air,
    pub shape: PlanetShape,
    /// Day length and phase on this planet; time of day is derived from
    /// snapshot ticks (one clock for the whole server).
    pub day: DayCycle,
    pub look: PlanetLook,
    /// `look.tints` resolved: one per block id, `NO_TINT` for none.
    pub tints: Vec<[u8; 3]>,
}

/// A planet as listed in the travel menu and drawn in the sky.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanetSummary {
    pub id: u16,
    pub name: String,
    pub kind: String,
    pub tier: u8,
    /// Its air (milestone 18), for the travel window.
    pub air: bm_core::atmosphere::Air,
    /// Linear RGB, as seen from other planets.
    pub color: [f32; 3],
    /// Blocks around, for its apparent size in the sky.
    pub width_blocks: u32,
    /// Players there when the list was sent.
    pub players: u16,
    /// A wandering world's host, the planet whose shrine reaches it
    /// (milestone 21); None for a permanent planet.
    pub host: Option<u16>,
    /// When a wandering world leaves (universe tick).
    pub leaves: Option<u64>,
    /// Its capital, the settlement with the most prestige there (milestone
    /// 24 part 4), by name.
    pub capital: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaimView {
    pub id: u32,
    /// Account name of the owner.
    pub owner: String,
    /// The block holding its claim stone.
    pub stone: BlockPos,
    pub plots: Vec<PlotPos>,
    /// Friends holding rights there (milestone 25), by name as shown, with
    /// their `bm_core::rights` bits.
    pub rights: Vec<(String, u8)>,
    /// The guild it is given to (milestone 25 part 2): (id, tag,
    /// controlled).
    pub guild: Option<(u32, String, bool)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JobView {
    pub recipe: u16,
    /// Crafts still to finish, including the one in progress.
    pub remaining: u16,
    /// Of the craft in progress, 0 to 1 (only the first job progresses).
    pub progress: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StationView {
    /// The machine's own position: for a four-block machine, its group's
    /// first block (lowest y, then z, then x, canonical); any of its blocks
    /// opens it (milestone 11).
    pub pos: BlockPos,
    pub station: Station,
    pub jobs: Vec<JobView>,
    /// Finished items waiting to be collected.
    pub output: Vec<ItemStack>,
    /// Why it does not work yet, in plain words ("A workbench needs 4 blocks
    /// touching", "A furnace needs its crucible on top"); None when it does.
    pub idle: Option<String>,
    /// A furnace's fuel slot, and the heat banked from fuel already burnt.
    /// A spark generator's fuel slot too (milestone 15).
    pub fuel: Option<ItemStack>,
    pub heat: u32,
    /// A machine's wear left and the most it holds (milestone 15); None for
    /// stations that do not wear.
    pub wear: Option<(u32, u32)>,
    /// Spark (milestone 15): a generator's own store, or what a spark
    /// machine's links reach; None for stations without spark.
    pub spark: Option<SparkView>,
    /// A forge's session and queue (milestone 17); None for other stations.
    pub forge: Option<Box<ForgeView>>,
    /// Power from coils (milestone 18); None for machines that take none.
    pub power: Option<PowerView>,
}

/// A machine's power as its window shows it (milestone 18).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerView {
    /// Power from its working coils.
    pub power: u32,
    /// Coils serving it, and how many of them are worn out.
    pub coils: u8,
    pub worn: u8,
}

/// Spark as a station's window shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SparkView {
    /// Spark stored (a generator group), or reachable through links (a
    /// machine: every generator group its links reach).
    pub stored: u32,
    /// The most those generators hold.
    pub capacity: u32,
    /// A machine: whether its links reach any generator. A generator: true.
    pub linked: bool,
    /// A generator: spark still to come from the fuel burning now.
    pub burning: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome(Box<Welcome>),
    Refused {
        reason: String,
    },
    /// Encoded with `Chunk::encode`.
    Chunk {
        pos: ChunkPos,
        data: Vec<u8>,
    },
    /// The server stops sending changes for this chunk; drop it.
    Forget {
        pos: ChunkPos,
    },
    /// A chunk's far copy (milestone 20), encoded with `FarChunk::encode`
    /// at the level of its ring; it replaces any copy held before.
    Far {
        pos: ChunkPos,
        data: Vec<u8>,
    },
    /// Drop this chunk's far copy (out of the far radius).
    ForgetFar {
        pos: ChunkPos,
    },
    BlockChanged {
        pos: BlockPos,
        block: BlockState,
    },
    /// The character's progression after any change (milestone 12): XP,
    /// level, marks, pages and ranks. `at` is the input from which a skill
    /// change counts (0 for XP and marks alone).
    Character {
        character: Character,
        at: u32,
    },
    /// Coin and feats (milestone 24): on joining and whenever they change
    /// (with the characters' batch).
    Ledger(bm_core::feats::Ledger),
    /// Damage a block has taken so far (milestone 9), sent to everyone who
    /// has its chunk whenever a swing changes it; 0 when it is reset (the
    /// block changed, or a refused swing's truth). Healing after
    /// `mining::HEAL_TICKS` without a hit is not sent: clients time it.
    BlockHealth {
        pos: BlockPos,
        damage: u32,
    },
    /// Answer to the sender's `Edit`, carrying what the block now is: the
    /// predicted change if accepted, the true block if refused.
    EditResult {
        seq: u32,
        pos: BlockPos,
        block: BlockState,
        accepted: bool,
    },
    PlayerJoined {
        id: u32,
        name: String,
    },
    PlayerLeft {
        id: u32,
    },
    /// Inventory slots that changed, with their new contents.
    Slots {
        changes: Vec<(u8, Option<ItemStack>)>,
    },
    /// A watched station's state (sent on change, and while a job runs).
    Station(StationView),
    /// A claim created or changed.
    Claim(ClaimView),
    ClaimRemoved {
        id: u32,
    },
    /// A line to show the player briefly ("Your inventory is full").
    Notice {
        text: String,
    },
    /// The player is now on another planet. The client drops every chunk,
    /// remote player, claim and station view of the old one; what follows on
    /// this stream belongs to the new one. Input sequences carry on.
    Travel(Box<Travel>),
    /// The planet list changed (players arriving and leaving, a new planet).
    Universe {
        planets: Vec<PlanetSummary>,
    },
    /// An open storage block's contents (`OpenStorage`), sent on open and on
    /// every change; `slots` empty with `closed` set when it is gone or out of
    /// reach.
    Storage(StorageView),
    /// A shop stand or request basket as its window shows it (milestone 24
    /// part 2); `closed` when it is gone or out of reach.
    Shop(ShopView),
    /// The open atlas's world (milestone 27); `closed` when it closes.
    Atlas(AtlasView),
    /// One tile of the open atlas's map (`bm_core::atlas`): `TILE_PIXELS`
    /// squared RGB pixels, row by row (z, then x), lz4-compressed; pixels
    /// past the planet's edge are left out of a short tile's rows, and
    /// `atlas::FOG` marks land not discovered.
    AtlasTile {
        planet: u16,
        tile: (u16, u16),
        rgb: Vec<u8>,
    },
    /// This player's friends and requests (milestone 25), on joining and on
    /// every change.
    Friends(FriendsView),
    /// Our guilds (milestone 25 part 2), on joining and on every change.
    Guilds(GuildsView),
    /// What our guilds' buffs add from right after input `at` (milestone 25
    /// part 3): run speed (blocks a second), grapple length (blocks),
    /// percent less lava damage, and percent faster block placing.
    GuildBody {
        at: u32,
        run: f64,
        grapple: f64,
        lava_percent: u16,
        place_percent: u16,
    },
    /// The open helix machine (milestone 25 part 3); `closed` when gone.
    Helix(HelixView),
    /// The open guild control, on open and on change; `closed` when gone.
    GuildControl(GuildControlView),
    /// The settlements of the player's planet and each claim's prestige
    /// there (milestone 24 part 4), sent on arrival and when they change.
    Settlements {
        settlements: Vec<SettlementView>,
        /// (claim id, prestige) for every claim with any.
        prestige: Vec<(u32, u64)>,
    },
    /// Chat lines, oldest first: one as it is said, or the recent history of
    /// the universe channel on joining.
    Chat {
        lines: Vec<ChatLine>,
    },
    /// What waits in this account's reclaim (items from lapsed claims), sent
    /// after the welcome when not empty and whenever it changes.
    Reclaim {
        items: Vec<ItemStack>,
    },
    /// The watched portal's window (`OpenPortal`), on open and on every
    /// change; None when it is gone, out of reach or no longer yours.
    Portal(PortalView),
    /// The portal window closed on the server's side (gone, out of reach,
    /// not ours, or a refused open): `id` names the portal, so a late close
    /// never shuts a window opened on another portal since; None closes
    /// whatever is open (an open that was refused).
    PortalClosed {
        id: Option<u32>,
    },
    /// Moved on the same planet (through a portal): take `spawn` as where we
    /// are at `tick`, a jump rather than a correction. Chunks, claims and
    /// views carry on.
    Teleport {
        spawn: PlayerState,
        tick: u64,
        /// The way to face on arriving (radians, `movement::heading`'s
        /// convention): out of the portal. None keeps the facing.
        yaw: Option<f64>,
    },
    /// What each portal token this player can see is bound to, by tag: for
    /// tooltips ("2 by 3, Verdance, near 812, 140"). Sent after the welcome
    /// and whenever a token reaches their pack or open storage.
    TokenNames {
        names: Vec<(u32, String)>,
    },
    /// A creature came into view, or changed (hurt, angry, died, looted by
    /// you). Its position travels in snapshots.
    Creature(CreatureView),
    /// A creature left view or vanished.
    CreatureGone {
        id: u32,
    },
    /// A shot is flying: draw it (`combat::shot_step` flies it exactly as the
    /// server does). Your own shots are drawn at once and not sent back.
    Shot(ShotView),
    /// A shot hit something or ran out of range.
    ShotEnd {
        id: u32,
    },
    /// Damage dealt, for the numbers that float up: to the one who dealt it
    /// and the one who took it.
    Hit {
        target: Target,
        amount: u32,
        crit: bool,
    },
    /// You were defeated. `debt` is the XP debt added.
    Defeated {
        debt: u64,
        by: String,
    },
    /// Something exploded here (a bomb, a popper, a drifter's bomb): draw it.
    Blast {
        pos: [f32; 3],
        radius: f32,
    },
    /// A drifter's beam from creature `from` to `to`, lasting `ticks`.
    Beam {
        from: u32,
        to: [f32; 3],
        ticks: u16,
    },
    /// This planet's named regions (milestone 14), sent on arrival:
    /// `bm_core::regions::RegionMap` lays them out from `layout_seed`; names
    /// by region index; the ones this character has discovered here.
    Regions {
        layout_seed: u64,
        names: Vec<String>,
        discovered: Vec<u16>,
    },
    /// You discovered region `region` (index), the `nth` on this planet, for
    /// `xp` XP.
    Discovered {
        region: u16,
        nth: u32,
        xp: u64,
    },
    /// A meteor on this planet, as it changes (and on arrival); every player
    /// on the planet gets these.
    Meteor(MeteorView),
    /// A meteor is gone from view (its heart closed, or you left).
    MeteorGone {
        id: u32,
    },
    /// A spanner hit a machine (milestone 15): its wear left now and the
    /// most it holds, for the bar under the crosshair.
    MachineWear {
        pos: BlockPos,
        wear: u32,
        max: u32,
    },
    /// Your meteor life: the meteor it belongs to, and the ticks left to get
    /// back into its zone (None: inside); None when you hold no life.
    MeteorLife {
        life: Option<(u32, Option<u32>)>,
    },
    /// The crop at `pos` (milestone 16, answering `LookCrop`): its name, its
    /// stage of `stages`, seconds until ripe (0: ripe), whether it is wild
    /// (no one sowed it: no replanting), sown on tilled soil, or a water
    /// crop with none beside it.
    CropInfo {
        pos: BlockPos,
        name: String,
        stage: u8,
        stages: u8,
        ripe_in: u32,
        wild: bool,
        tilled: bool,
        dry: bool,
    },
    /// A forging session at `pos` ended (milestone 17), told to its forger.
    Forged {
        pos: BlockPos,
        outcome: ForgeOutcome,
    },
    /// Whether creative mode holds from right after input `at` (the
    /// answer to `ClientMsg::Creative`, or the server's word at login).
    Creative {
        at: u32,
        on: bool,
    },
    /// The planet's air from right after input `at` (milestone 21: a
    /// wandering world's air climbs as it leaves).
    Air {
        at: u32,
        air: bm_core::atmosphere::Air,
    },
    /// A written sign anchored at `anchor` (its lower left panel as read),
    /// or None when it was unwritten: sent with the anchor's chunk and
    /// whenever it changes (milestone 22 part 4).
    Sign {
        anchor: BlockPos,
        sign: Option<bm_core::signs::SignView>,
    },
}

/// How a forging session ended.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ForgeOutcome {
    /// Sealed: the gear, with its traits, is in the forge's queue.
    Sealed(ItemStack),
    /// Unmade: the gear back unforged, and these ingredients back.
    Unmade { gear: ItemStack, returned: Vec<ItemStack> },
    /// Stability ran out: the gear and the ingredients used are lost.
    Destroyed(ItemStack),
}

/// A forge as its window shows it (part of `StationView`, milestone 17).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ForgeView {
    /// The session running, if any.
    pub session: Option<SessionView>,
    /// Who is forging, when it is not the watcher.
    pub forger: Option<String>,
    /// A sealed item in the queue: the item, how far along (0..1) and
    /// seconds left. Done, it waits in the station's `output`.
    pub queued: Option<(ItemStack, f32, u32)>,
}

/// A forging session as its forger sees it: everything but the dice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionView {
    pub item: ItemStack,
    pub gear: bm_core::forge::Gear,
    pub stats: bm_core::forge::SessionStats,
    pub stability: i32,
    pub vigour: i32,
    pub pools: [i32; 3],
    pub traits: [Vec<(bm_core::forge::TraitId, u32)>; 3],
    pub buffs: Vec<bm_core::forge::Buff>,
    pub deck: Vec<bm_core::forge::DeckSlot>,
    /// Ingredients used so far.
    pub used: u32,
    pub rounds: u32,
    pub last: bm_core::forge::RoundReport,
}

impl SessionView {
    pub fn of(s: &bm_core::forge::Session) -> Self {
        Self {
            item: s.item,
            gear: s.gear,
            stats: s.stats,
            stability: s.stability,
            vigour: s.vigour,
            pools: s.pools,
            traits: s.traits.clone(),
            buffs: s.buffs.clone(),
            deck: s.deck.clone(),
            used: s.used.len() as u32,
            rounds: s.rounds,
            last: s.last.clone(),
        }
    }
}

/// A meteor's phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeteorPhase {
    /// In the air: lands at server tick `lands_at`.
    Falling { lands_at: u64 },
    /// Landed; waves dropping. `wave` is the current one (1-based), `left`
    /// its creatures still alive; `lives` the players holding a life.
    Fighting { wave: u8, waves: u8, left: u16, lives: u16 },
    /// Every wave cleared: the shell breaks and the heart opens, until
    /// server tick `until`.
    Cleared { until: u64 },
    /// Failed or left too long: the heart pays a little, until `until`.
    Dormant { until: u64 },
}

/// A meteor as players see it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeteorView {
    pub id: u32,
    /// Where it falls from (high in the sky) and where it lands (the heart).
    pub from: [f32; 3],
    pub site: BlockPos,
    pub level: u8,
    pub phase: MeteorPhase,
    /// The portals' centres while it is fighting (empty otherwise).
    pub portals: [[f32; 3]; 3],
    /// The beam over the site shows until this server tick.
    pub beam_until: u64,
    /// You have taken your share of the heart.
    pub looted: bool,
}

/// What a flying shot is, for drawing it (and flying it: bombs fall at
/// `combat::BOMB_GRAVITY`, homing bolts turn by `combat::homing_step`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShotKind {
    /// A sling's pellet or a pelter's spit.
    Pellet,
    /// A drifter's bolt.
    Bolt,
    /// A thrown or lobbed bomb.
    Bomb,
    /// A homing bolt, turning toward `ShotView::homing`.
    Homing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Player(u32),
    Creature(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureView {
    pub id: u32,
    /// Index into `Welcome::creatures`.
    pub kind: u16,
    pub tier: u8,
    /// Health lost.
    pub damage: u32,
    /// Hostile to players right now (for the name's colour).
    pub angry: bool,
    pub dead: bool,
    /// You have taken your drops from this body.
    pub looted: bool,
    /// Exotic buffs (`creatures::Exotic` bits; 0: ordinary). Exotics wear
    /// markings.
    pub exotic: u8,
    /// Its element (`creatures::Element::code`; 0: none; milestone 18).
    pub element: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShotView {
    pub id: u32,
    pub from: Target,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub falloff: f32,
    pub range: f32,
    pub kind: ShotKind,
    /// What a homing bolt turns toward.
    pub homing: Option<Target>,
}

/// What a guild window asks (milestone 25 part 2). Members are named as
/// shown; the server finds their accounts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuildAction {
    /// Found a guild at the open control (on our own claim).
    Found {
        pos: BlockPos,
        name: String,
        tag: String,
    },
    /// Set the open control (on our own claim) to one of our guilds, or
    /// blank.
    SetControl {
        pos: BlockPos,
        guild: Option<u32>,
    },
    /// Ask to join the guild of the open control.
    Ask {
        guild: u32,
    },
    Accept {
        guild: u32,
        who: String,
    },
    Decline {
        guild: u32,
        who: String,
    },
    Leave {
        guild: u32,
    },
    Remove {
        guild: u32,
        who: String,
    },
    SetRank {
        guild: u32,
        who: String,
        rank: bm_core::guilds::Rank,
    },
    /// Claim rights (`bm_core::rights`) and powers (`bm_core::guilds`).
    SetRights {
        guild: u32,
        who: String,
        rights: u8,
        powers: u8,
    },
    Donate {
        guild: u32,
        coin: u64,
    },
    Pay {
        guild: u32,
        to: String,
        coin: u64,
    },
    SetPrimary {
        guild: u32,
    },
    /// Give one of our claims to a guild we are in (aligned, or controlled
    /// by it), or take it back (None).
    Align {
        claim: u32,
        guild: Option<u32>,
        controlled: bool,
    },
    /// Start the open helix machine's buff at `tier` (milestone 25 part 3).
    StartBuff {
        pos: BlockPos,
        tier: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberView {
    pub name: String,
    pub rank: bm_core::guilds::Rank,
    pub rights: u8,
    pub powers: u8,
    pub online: bool,
}

/// One of our guilds in full.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuildView {
    pub id: u32,
    pub name: String,
    pub tag: String,
    pub coin: u64,
    pub members: Vec<MemberView>,
    /// Waiting to join (shown to those with the Membership power).
    pub asked: Vec<String>,
    /// The log, newest last (Controllers and up): (unix seconds, text).
    pub log: Vec<(u64, String)>,
    /// Claims given to it, with whether controlled.
    pub claims: Vec<(u32, bool)>,
    /// Endeavour to spend on buffs (milestone 25 part 3).
    pub endeavour: u64,
    /// Buffs running: (kind, tier, unix seconds they end, whether we get
    /// it).
    pub buffs: Vec<(bm_core::guilds::BuffKind, u8, u64, bool)>,
}

/// Our guilds and which is primary (milestone 25 part 2).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuildsView {
    pub guilds: Vec<GuildView>,
    pub primary: Option<u32>,
}

/// A helix machine as its window shows it (milestone 25 part 3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelixView {
    pub pos: BlockPos,
    pub kind: bm_core::guilds::BuffKind,
    /// The guild whose claim it stands on: (id, name, coin, endeavour).
    pub guild: Option<(u32, String, u64, u64)>,
    /// The buff running from it: (tier, unix seconds it ends).
    pub running: Option<(u8, u64)>,
    /// We may start a buff (the guild's Buffs power).
    pub may_start: bool,
    pub closed: bool,
}

/// A guild control as its window shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuildControlView {
    pub pos: BlockPos,
    /// The guild it is set to: (id, name, tag, members).
    pub guild: Option<(u32, String, String, u32)>,
    /// It stands on our own claim (we may found a guild or set it).
    pub mine: bool,
    /// We have asked to join its guild.
    pub asked: bool,
    pub closed: bool,
}

/// What to do about a friend (milestone 25).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FriendAction {
    Ask,
    Accept,
    Decline,
    Remove,
}

/// One friend as the friends window shows them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FriendView {
    pub name: String,
    /// Playing now, and on which planet.
    pub planet: Option<String>,
}

/// This player's friends and requests (milestone 25).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FriendsView {
    pub friends: Vec<FriendView>,
    /// Players asking to be our friend.
    pub asked_by: Vec<String>,
}

/// What a shop stand or request basket's window asks (milestone 24 part 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShopAction {
    /// Owner: a slot's price in coin a piece (0: not for sale or not wanted).
    SetPrice { slot: u8, price: u64 },
    /// Owner: the till into their coin.
    TakeTill,
    /// Owner of a basket: put this much of their coin into its till.
    Charge { amount: u64 },
    /// Anyone else: buy `count` from a stand's slot.
    Buy { slot: u8, count: u16 },
    /// Anyone else: sell up to `count` from pack slot `from` into a basket's
    /// slot.
    Sell { slot: u8, from: u8, count: u16 },
}

/// Which of the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShopKind {
    Stand,
    Basket,
}

/// One settlement as players see it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SettlementView {
    /// Its name: the warden's choice, or "<warden>'s <rank>".
    pub name: String,
    /// Its rank's name ("Outpost" to "Great City").
    pub rank: String,
    pub prestige: u64,
    /// The warden's name as shown.
    pub warden: String,
    /// The planet's top settlement.
    pub capital: bool,
    pub claims: Vec<u32>,
}

/// The open atlas's world (milestone 27).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AtlasView {
    /// The inventory slot it was opened from.
    pub slot: u8,
    pub planet: u16,
    pub name: String,
    pub shape: PlanetShape,
    /// Its regions as `Regions` gives them: names by index (laid out from
    /// `layout_seed`) and the ones this character has discovered there.
    pub layout_seed: u64,
    pub region_names: Vec<String>,
    pub discovered: Vec<u16>,
    /// Its warp shrine.
    pub shrine: BlockPos,
    pub closed: bool,
}

/// A shop stand or request basket as its window shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShopView {
    pub pos: BlockPos,
    pub kind: ShopKind,
    /// The owner's name as shown.
    pub owner: String,
    /// The viewer owns it.
    pub mine: bool,
    pub slots: Vec<Option<ItemStack>>,
    /// Coin a piece, by slot (0: not for sale or not wanted).
    pub prices: Vec<u64>,
    /// Coin in it: only its owner is told (0 for everyone else), except a
    /// basket's, which sellers need to see.
    pub till: u64,
    /// The viewer's sales fee here, in tenths of a percent (the stand's for a
    /// buyer, their own for a basket's seller).
    pub fee_permille: u64,
    /// Gone or out of reach: the window closes.
    pub closed: bool,
}

/// A portal as its window shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortalView {
    /// The portal's id: its tokens carry it as their tag.
    pub id: u32,
    /// The portal's lowest corner.
    pub corner: BlockPos,
    pub width: u8,
    pub height: u8,
    /// Where it leads: "Verdance, near 812, 140"; None when unlinked.
    pub link: Option<String>,
    /// Linked and this side's tank can pay a trip.
    pub open: bool,
    /// Wayshards in the tank, and the most it holds.
    pub fuel: u32,
    pub capacity: u32,
    /// What one charged trip through this side costs.
    pub trip_cost: u32,
    /// Hours a traveller's charge lasts before they pay again.
    pub cooldown_hours: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageView {
    pub pos: BlockPos,
    pub slots: Vec<Option<ItemStack>>,
    pub closed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatLine {
    pub channel: Channel,
    /// The speaker's name; empty for the server itself.
    pub from: String,
    pub text: String,
    /// Unix seconds when it was said.
    pub time: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Travel {
    pub planet: PlanetInfo,
    pub spawn: PlayerState,
    /// Server tick the spawn state belongs to.
    pub tick: u64,
    /// Every claim on the new planet.
    pub claims: Vec<ClaimView>,
    /// The way to face on arriving, as in `ServerMsg::Teleport`.
    pub yaw: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientDatagram {
    /// `inputs[i]` is the input for tick sequence `latest - i`.
    Input { latest: u32, inputs: Vec<PlayerInput> },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemotePlayer {
    pub id: u32,
    /// Feet position, canonical. `f32` is ample on a planet a few km around.
    pub pos: [f32; 3],
    pub yaw: u16,
    pub pitch: i16,
    /// Their grapple hook while one is out, from their feet in 1/256 blocks
    /// (a hook never flies further than a grapple's range, well under 128).
    pub hook: Option<[i16; 3]>,
    /// Light from what they hold in hand (a torch), per channel (red,
    /// green, blue; milestone 26), 0 for none.
    pub light: [u8; 3],
}

impl RemotePlayer {
    /// Packs a hook at `offset` blocks from the feet for [`RemotePlayer::hook`].
    pub fn pack_hook(offset: DVec3) -> [i16; 3] {
        offset
            .to_array()
            .map(|v| (v * 256.0).round().clamp(i16::MIN as f64, i16::MAX as f64) as i16)
    }

    /// The hook's offset from the feet, in blocks.
    pub fn hook_offset(&self) -> Option<DVec3> {
        self.hook.map(|h| DVec3::from_array(h.map(|v| v as f64 / 256.0)))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerDatagram {
    /// World state at `tick`. Large player counts split one tick across
    /// several snapshots; `you` is repeated in each.
    Snapshot {
        /// The planet it describes; a snapshot from before a `Travel` arrives
        /// out of order with the stream and is dropped.
        planet: u16,
        tick: u64,
        /// Highest input sequence applied to `you`.
        ack: u32,
        you: PlayerState,
        others: Vec<RemotePlayer>,
        /// Creatures near you, nearest first, at most
        /// [`CREATURES_PER_SNAPSHOT`]. A snapshot full of players carries
        /// none: the rest go in further snapshots of the same tick.
        creatures: Vec<RemoteCreature>,
    },
}

/// What a creature is doing, for its animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pose {
    Idle,
    Walk,
    Run,
    /// Rearing before a charge, or aiming before a volley.
    Windup,
    Attack,
    Dead,
    /// Curled into a ball (a popper): cannot be hurt.
    Curled,
    /// Whistling before it explodes (a popper).
    Lit,
    /// Stunned by a hammer (a popper).
    Stunned,
    /// Airborne (a drifter in flight).
    Flying,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoteCreature {
    pub id: u32,
    pub pos: [f32; 3],
    pub yaw: u16,
    pub pose: Pose,
}

pub const CREATURES_PER_SNAPSHOT: usize = 40;

/// Remote players per snapshot datagram, keeping each under the ~1200-byte
/// path MTU QUIC assumes even when every one of them has a grapple out
/// (25 since milestone 23 put the brews in our own state; 23 since
/// milestone 26 gave held light three channels).
pub const PLAYERS_PER_SNAPSHOT: usize = 23;

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("frame of {0} bytes exceeds the limit")]
    TooLarge(usize),
    #[error("malformed message: {0}")]
    Malformed(#[from] postcard::Error),
}

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    postcard::to_stdvec(msg).expect("protocol types always serialize")
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, DecodeError> {
    if bytes.len() > MAX_FRAME {
        return Err(DecodeError::TooLarge(bytes.len()));
    }
    Ok(postcard::from_bytes(bytes)?)
}

/// A length-prefixed frame for the reliable stream.
pub fn encode_frame<T: Serialize>(msg: &T) -> Vec<u8> {
    let body = encode(msg);
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Cleans a player-chosen name: printable, trimmed, bounded.
pub fn clean_name(name: &str) -> Option<String> {
    let s: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '_' || *c == '-')
        .take(MAX_NAME_LEN)
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    (s.chars().count() >= 2).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let msgs = vec![
            ServerMsg::Chunk {
                pos: ChunkPos::new(-1, 2, 3),
                data: vec![1, 2, 3],
            },
            ServerMsg::EditResult {
                seq: 9,
                pos: BlockPos::new(1, 2, 3),
                block: BlockState::coloured(bm_core::BlockId(4), bm_core::Colour(9)),
                accepted: false,
            },
            ServerMsg::Regions {
                layout_seed: 99,
                names: vec!["Amber Hollows".into()],
                discovered: vec![0],
            },
            ServerMsg::Meteor(MeteorView {
                id: 3,
                from: [1.0, 400.0, 2.0],
                site: BlockPos::new(10, 140, -4),
                level: 2,
                phase: MeteorPhase::Fighting {
                    wave: 1,
                    waves: 4,
                    left: 3,
                    lives: 2,
                },
                portals: [[0.0; 3]; 3],
                beam_until: 1000,
                looted: false,
            }),
            ServerMsg::MeteorLife {
                life: Some((3, Some(120))),
            },
        ];
        for m in msgs {
            assert_eq!(decode::<ServerMsg>(&encode(&m)).unwrap(), m);
        }
    }

    #[test]
    fn full_snapshot_fits_a_datagram() {
        let snap = ServerDatagram::Snapshot {
            planet: u16::MAX,
            tick: u64::MAX,
            ack: u32::MAX,
            you: PlayerState {
                pos: DVec3::new(1e6, 1e3, 1e6),
                vel: DVec3::splat(-50.0),
                on_ground: true,
                spent: u16::MAX,
                rest: u16::MAX,
                body: bm_core::Body {
                    run_bonus: 99.0,
                    sprint_bonus: 99.0,
                    high_jump: true,
                    double_jump: true,
                    energy_bonus: u16::MAX,
                    regen_bonus: u16::MAX,
                    grapple_shot_bonus: 99.0,
                    grapple_range_bonus: 99.0,
                    health_bonus: u32::MAX,
                    falling_armour: u16::MAX,
                    mending: true,
                    wards: [u8::MAX; 3],
                    heal_bonus: u16::MAX,
                    buff_bonus: 90,
                    creative: true,
                    guild_run_bonus: 0.6,
                    guild_grapple_bonus: 25.0,
                    guild_lava_percent: 75,
                },
                jump_held: true,
                air_jumped: true,
                hurt: u32::MAX,
                calm: u16::MAX,
                beat: u16::MAX,
                loose: u16::MAX,
                down: true,
                hunger: u32::MAX,
                buff: bm_core::food::FoodBuff {
                    kind: bm_core::food::Buff::Studious,
                    rank: u8::MAX,
                    left: u32::MAX,
                },
                sick: u16::MAX,
                chew: u16::MAX,
                quirks: bm_core::forge::Quirks(u16::MAX),
                clock: u32::MAX,
                air: bm_core::atmosphere::Air {
                    line: bm_core::atmosphere::AirLine::Burning,
                    hazard: u8::MAX,
                },
                air_used: u16::MAX,
                lava_wait: u16::MAX,
                submerged: true,
                brew: bm_core::brew::BrewBuff {
                    kind: bm_core::brew::Brew::Deepwater,
                    grade: u8::MAX,
                    left: u32::MAX,
                },
                revive: u32::MAX,
                hook: Some(bm_core::Hook {
                    pos: DVec3::new(1e6, 1e3, 1e6),
                    vel: DVec3::splat(-50.0),
                    rope: 1e3,
                    stuck: Some(BlockPos::new(i32::MAX as i64, i32::MIN as i64, i32::MAX as i64)),
                    reel: -40.0,
                }),
            },
            others: vec![
                RemotePlayer {
                    id: u32::MAX,
                    pos: [1e6; 3],
                    yaw: u16::MAX,
                    pitch: i16::MIN,
                    hook: Some([i16::MIN; 3]),
                    light: [u8::MAX; 3],
                };
                PLAYERS_PER_SNAPSHOT
            ],
            creatures: Vec::new(),
        };
        let n = encode(&snap).len();
        assert!(n <= 1150, "{n} bytes");
        // Creatures go in snapshots of their own when players fill one.
        let ServerDatagram::Snapshot {
            planet, tick, ack, you, ..
        } = snap;
        let snap = ServerDatagram::Snapshot {
            planet,
            tick,
            ack,
            you,
            others: Vec::new(),
            creatures: vec![
                RemoteCreature {
                    id: u32::MAX,
                    pos: [1e6; 3],
                    yaw: u16::MAX,
                    pose: Pose::Dead,
                };
                CREATURES_PER_SNAPSHOT
            ],
        };
        let n = encode(&snap).len();
        assert!(n <= 1150, "{n} bytes");
    }

    #[test]
    fn redundant_inputs_are_small() {
        let d = ClientDatagram::Input {
            latest: 1_000_000,
            inputs: vec![
                PlayerInput {
                    forward: 1,
                    strafe: -1,
                    jump: true,
                    sprint: true,
                    yaw: 65535,
                    pitch: -16000,
                    slot: 9,
                    fire: true,
                    reel: -1,
                    descend: true,
                };
                INPUT_REDUNDANCY
            ],
        };
        // Sent 60 times a second: about 6 KB/s up at this size.
        let n = encode(&d).len();
        assert!(n <= 100, "{n} bytes");
    }

    #[test]
    fn a_full_welcome_fits_one_frame() {
        let blocks = bm_core::BlockRegistry::builtin();
        let items = bm_core::ItemRegistry::builtin(&blocks);
        let recipes = bm_core::RecipeBook::builtin(&items);
        let w = ServerMsg::Welcome(Box::new(Welcome {
            player_id: 1,
            account: "Pendragon".into(),
            planet: PlanetInfo {
                id: 0,
                name: "Verdance".into(),
                kind: "Temperate".into(),
                tier: 1,
                air: Default::default(),
                shape: PlanetShape::new(64, 12),
                day: DayCycle::default(),
                look: PlanetLook::default(),
                tints: vec![bm_core::NO_TINT; blocks.len()],
            },
            universe: (0..32)
                .map(|id| PlanetSummary {
                    id,
                    name: "A planet with quite a long name".into(),
                    kind: "Temperate".into(),
                    tier: 3,
                    air: Default::default(),
                    color: [0.5; 3],
                    width_blocks: 2048,
                    players: 100,
                    host: Some(3),
                    leaves: Some(u64::MAX),
                    capital: Some("A settlement with a long name's Great City".into()),
                })
                .collect(),
            blocks: blocks.defs().to_vec(),
            items: items.defs().to_vec(),
            colours: ColourPalette::builtin(),
            recipes: recipes.recipes.clone(),
            skills: bm_core::skills::SkillTree::builtin().skills,
            character: Character::default(),
            inventory: Inventory::default(),
            movement: MoveConfig::default(),
            spawn: PlayerState::default(),
            tick: 0,
            view_radius: 10,
            far_radius: 32,
            reach: 4.0,
            claims: vec![ClaimView {
                id: 1,
                owner: "x".into(),
                stone: BlockPos::new(0, 0, 0),
                plots: vec![PlotPos::new(0, 0, 0); 64],
                // As many friends with rights as a claim may hold.
                rights: vec![("A player with a long name".into(), 15); 50],
                guild: Some((9, "LANTS".into(), true)),
            }],
            plot_allowance: 64,
            creatures: bm_core::creatures::CreatureKinds::builtin().defs().to_vec(),
        }));
        let bytes = encode(&w);
        assert!(bytes.len() < MAX_FRAME / 8, "{} bytes", bytes.len());
        assert_eq!(decode::<ServerMsg>(&bytes).unwrap(), w);
    }

    #[test]
    fn garbage_does_not_panic() {
        for i in 0..2000u32 {
            let bytes: Vec<u8> = (0..(i % 64)).map(|j| (i.wrapping_mul(31) ^ (j * 17)) as u8).collect();
            let _ = decode::<ServerMsg>(&bytes);
            let _ = decode::<ClientMsg>(&bytes);
            let _ = decode::<ClientDatagram>(&bytes);
        }
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Pen  dragon\n"), Some("Pen dragon".into()));
        assert_eq!(clean_name("<script>"), Some("script".into()));
        assert_eq!(clean_name("x"), None);
        assert_eq!(clean_name(&"a".repeat(100)).unwrap().len(), MAX_NAME_LEN);
    }
}
