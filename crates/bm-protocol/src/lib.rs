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
    BlockDef, BlockPos, BlockState, ChunkPos, ColourPalette, DayCycle, Inventory, ItemDef, ItemStack, MoveConfig,
    PlanetLook, PlanetShape, PlayerInput, PlayerState, PlotPos, Recipe, Station,
};
use glam::DVec3;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Bump on any change to a message below. Client and server must match.
pub const PROTOCOL_VERSION: u32 = 12;

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
    /// The player has started holding break on this block, holding the item
    /// in hotbar `slot` (a tool, or anything: a non-tool breaks by hand). The
    /// server times the hold from here: a break sooner than the tool allows
    /// is refused. `at` is the input sequence it follows, as for `Edit`: it
    /// is queued with the edits, so it can never overtake the break of the
    /// block before it.
    StartBreak { at: u32, pos: BlockPos, slot: u8 },
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
    },
    /// Start (or with `None`, stop) receiving `Station` updates for one
    /// station, e.g. while its screen is open.
    WatchStation { pos: Option<BlockPos> },
    /// Move a station's finished output into the inventory (as much as fits).
    CollectStation { pos: BlockPos },
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
    /// One player, by name (the sender gets a copy).
    Whisper(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditAction {
    /// The item yielded goes into the inventory; refused if it cannot fit.
    Break,
    /// Places the block item in hotbar `slot`, spending one.
    Place { slot: u8 },
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
    pub inventory: Inventory,
    pub movement: MoveConfig,
    pub spawn: PlayerState,
    /// Server tick the spawn state belongs to.
    pub tick: u64,
    /// Horizontal chunk radius the server streams around the player.
    pub view_radius: u8,
    /// Furthest a player may edit from their eye, in blocks.
    pub reach: f64,
    /// Every claim on the planet (later: only those near the player).
    pub claims: Vec<ClaimView>,
    /// Plots this account may claim in all.
    pub plot_allowance: u32,
}

/// Everything a client needs to be on a planet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanetInfo {
    pub id: u16,
    pub name: String,
    /// The kind's display name ("Arid").
    pub kind: String,
    pub tier: u8,
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
    /// Linear RGB, as seen from other planets.
    pub color: [f32; 3],
    /// Blocks around, for its apparent size in the sky.
    pub width_blocks: u32,
    /// Players there when the list was sent.
    pub players: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaimView {
    pub id: u32,
    /// Account name of the owner.
    pub owner: String,
    /// The block holding its claim stone.
    pub stone: BlockPos,
    pub plots: Vec<PlotPos>,
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
    pub pos: BlockPos,
    pub station: Station,
    pub jobs: Vec<JobView>,
    /// Finished items waiting to be collected.
    pub output: Vec<ItemStack>,
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
    BlockChanged {
        pos: BlockPos,
        block: BlockState,
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
    Portal(Option<PortalView>),
    /// Moved on the same planet (through a portal): take `spawn` as where we
    /// are at `tick`, a jump rather than a correction. Chunks, claims and
    /// views carry on.
    Teleport {
        spawn: PlayerState,
        tick: u64,
    },
    /// What each portal token this player can see is bound to, by tag: for
    /// tooltips ("2 by 3, Verdance, near 812, 140"). Sent after the welcome
    /// and whenever a token reaches their pack or open storage.
    TokenNames {
        names: Vec<(u32, String)>,
    },
}

/// A portal as its window shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortalView {
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
    },
}

/// Remote players per snapshot datagram, keeping each under the ~1200-byte
/// path MTU QUIC assumes even when every one of them has a grapple out.
pub const PLAYERS_PER_SNAPSHOT: usize = 28;

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
                };
                PLAYERS_PER_SNAPSHOT
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
                    color: [0.5; 3],
                    width_blocks: 2048,
                    players: 100,
                })
                .collect(),
            blocks: blocks.defs().to_vec(),
            items: items.defs().to_vec(),
            colours: ColourPalette::builtin(),
            recipes: recipes.recipes.clone(),
            inventory: Inventory::default(),
            movement: MoveConfig::default(),
            spawn: PlayerState::default(),
            tick: 0,
            view_radius: 10,
            reach: 7.0,
            claims: vec![ClaimView {
                id: 1,
                owner: "x".into(),
                stone: BlockPos::new(0, 0, 0),
                plots: vec![PlotPos::new(0, 0, 0); 64],
            }],
            plot_allowance: 64,
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
