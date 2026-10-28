//! `--demo-ui`: a hidden development mode that runs the client with no
//! server, so the menus can be seen and driven headlessly.
//!
//! It stands in for the server with a tiny, honest imitation: a flat patch of
//! ground (a test fixture, not world generation: the real client never makes
//! terrain, ADR-0003), a sample inventory, two stations and two claims. It
//! answers inventory, crafting, station, claim and edit messages with the same
//! `bm-core` rules the real server uses, so drags and crafts behave. It is not
//! a second server: no accounts, no reach or timing checks, no saves.
//!
//! It also stands in for a small universe: four planets, one of each kind in
//! `assets/planets.toml` with that kind's look, a warp shrine by the spawn on
//! each, and travel between them. After a warp the new planet's ground is
//! held back for a moment (or for good, with `hold_arrivals`) so the
//! travelling screen can be seen.
//!
//! Milestone 17's screens: `--demo-ui forge` (a session under way, played
//! by the real `bm_core::forge::Session`), `forge-queue` (a sealed hammer in
//! the queue, a forged one ready) and `forge-empty` (choose gear and deck).

use crate::net::NetEvent;
use bm_core::forge::Held;
use bm_core::portal::{PortalAxis, PortalFrame};
use bm_core::skills::{Character, SkillTree};
use bm_core::{
    BlockId, BlockPos, BlockRegistry, BlockState, CHUNK_SIZE, Chunk, ChunkPos, Colour, ColourPalette, DayCycle,
    Inventory, ItemRegistry, ItemStack, LocalPos, MoveConfig, PlanetLook, PlanetShape, PlayerState, PlotPos,
    RecipeBook, Station,
};
use bm_protocol::{
    Channel, ChatLine, ClaimView, ClientMsg, EditAction, JobView, MAX_CHAT_LEN, PlanetInfo, PlanetSummary,
    PortalAction, PortalView, ServerMsg, SparkView, StationView, StorageView, Travel, Welcome,
};
use glam::DVec3;
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

pub const SPAWN: [f64; 3] = [128.5, 44.0, 128.5];
const GROUND: i64 = 43;
const SHAPE: (u32, u32) = (8, 4);
/// The warp shrine by the spawn, on every demo planet.
pub const SHRINE: [i64; 3] = [128, 44, 131];
/// A storage chest with a few things in it, on the first planet.
pub const CHEST: [i64; 3] = [126, 44, 126];
/// The palette wall (`show_colours`): its lowest, westmost block. 17 wide
/// (16 hues and the greys) by 15 shades tall, facing the spawn.
pub const WALL: [i64; 3] = [120, 48, 114];
/// Where `show_shapes` puts the player: on a pillar behind the rows.
pub const SHAPES_VIEW: [f64; 3] = [128.5, 46.0, 130.5];

/// Where the furnishings showroom is seen from.
pub const FURNISHINGS_VIEW: [f64; 3] = [128.5, 47.0, 131.5];
/// The west end of the grapple demo's arch beam, which runs ten blocks east.
pub const GRAPPLE_BEAM: [i64; 3] = [124, 56, 118];
/// The torch demo's hall (`show_torches`), inside: west and east x, north
/// and south z, and its highest open layer. The spawn is at its south end.
/// The farm demo (milestone 16): seven crops in columns along x, their four
/// stages in rows along z, on tilled soil, seen from the south.
pub const FARM_X: i64 = 125;
pub const FARM_Z: i64 = 119;
pub const FARM_VIEW: [f64; 3] = [128.5, 44.0, 126.5];
pub const FARM_CROPS: [(&str, &str, f64); 7] = [
    ("dewberry", "Dewberry Bush", 3.0),
    ("tuber", "Tuber Plant", 3.0),
    ("waxy_tuber", "Waxy Tuber Plant", 4.0),
    ("exotic_tuber", "Exotic Tuber Plant", 6.0),
    ("oats", "Oats", 4.0),
    ("wheat", "Wheat", 6.0),
    ("rice", "Rice", 6.0),
];
pub const TORCH_HALL: (i64, i64, i64, i64, i64) = (124, 132, 106, 129, 47);
/// The demo's planets: name, kind, and the width shown for it in the sky and
/// menu. (The demo's own patch is tiny; these are sizes real planets have.)
const PLANETS: [(&str, &str, u32); 4] = [
    ("Hearthmoor", "temperate", 2048),
    ("Saffra", "arid", 3072),
    ("Rimeholt", "frozen", 2560),
    ("Verdance", "lush", 4096),
];
/// The portal demo's two open portals (`show_portals`), 1 by 2, ahead of
/// the spawn: each one's lower block.
pub const PORTALS: [[i64; 3]; 2] = [[122, 44, 121], [135, 44, 121]];
/// The half-broken stone block of `show_mining`, on the ground ahead of the
/// spawn.
pub const MINED: BlockPos = BlockPos::new(128, 44, 126);
/// Wayshards a demo portal's tank holds.
const PORTAL_CAPACITY: u32 = 500;
/// How long a new planet's ground is held back after a warp, by default.
const ARRIVAL_HOLD: f64 = 2.5;

#[derive(Deserialize)]
struct KindsFile {
    kind: Vec<KindDef>,
}

#[derive(Deserialize)]
struct KindDef {
    name: String,
    display: Option<String>,
    tier: u8,
    look: PlanetLook,
}

struct DemoPlanet {
    info: PlanetInfo,
    summary: PlanetSummary,
    edits: HashMap<BlockPos, BlockState>,
    /// No rolling ground: level everywhere (the colour wall's patch).
    flat: bool,
}

/// A portal as the demo keeps it: on which planet, its rectangle, its link
/// (an index into `Demo::portals`) and its tank. Its tokens' tag is its
/// index plus one.
struct DemoPortal {
    planet: usize,
    frame: PortalFrame,
    link: Option<usize>,
    fuel: u32,
}

struct DemoStation {
    station: Station,
    jobs: Vec<JobView>,
    output: Vec<ItemStack>,
    /// Why it does not work (too few blocks, no crucible), as a server says.
    idle: Option<String>,
    /// A furnace's fuel slot and banked heat, and whether the craft under
    /// way has had its heat.
    fuel: Option<ItemStack>,
    heat: u32,
    paid: bool,
    /// Milestone 15: a machine's wear left and most, and its spark.
    wear: Option<(u32, u32)>,
    spark: Option<SparkView>,
    /// Milestone 17: a forge's session and queue.
    forge: Option<DemoForge>,
}

/// What `show_forge` puts in its forge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgeShow {
    Session,
    Queue,
    Empty,
}

/// A forge as the demo keeps it: a session (the real rules, run here) and
/// a sealed item in the queue with its seconds in all and done.
#[derive(Default)]
struct DemoForge {
    session: Option<bm_core::forge::Session>,
    queued: Option<(ItemStack, f64, f64)>,
}

impl DemoStation {
    fn new(station: Station) -> Self {
        Self {
            station,
            jobs: Vec::new(),
            output: Vec::new(),
            idle: None,
            fuel: None,
            heat: 0,
            paid: false,
            wear: None,
            spark: None,
            forge: None,
        }
    }
}

/// The demo's machines on the first planet: a crafting table, a workbench
/// of four blocks in a T, a furnace (base and crucible) burning coal, and an
/// extractor one block short. Each lists its blocks, its first block first.
pub const TABLE: [i64; 3] = [130, 44, 128];
pub const WORKBENCH: [[i64; 3]; 4] = [[128, 44, 125], [127, 44, 126], [128, 44, 126], [129, 44, 126]];
pub const FURNACE: [[i64; 3]; 2] = [[131, 44, 126], [131, 45, 126]];
/// `show_forge`: a forge of four blocks in a row ahead of the spawn.
pub const FORGE: [[i64; 3]; 4] = [[127, 44, 123], [128, 44, 123], [129, 44, 123], [130, 44, 123]];
pub const EXTRACTOR: [[i64; 3]; 3] = [[133, 44, 124], [134, 44, 124], [135, 44, 124]];
/// Where `show_meteor` drops its meteor (x, z), and where the player stands
/// to watch it.
pub const METEOR_SITE: [i64; 2] = [128, 116];
pub const METEOR_VIEW: [f64; 3] = [128.5, 44.0, 131.5];

/// Where `show_textures` puts the player: looking north over turf, a patch
/// of bare soil, a stone wall with copper in it and a tree.
pub const TEXTURES_VIEW: [f64; 3] = [128.5, 44.0, 132.5];

/// `show_spark`: two spark generators joined, three links, and a mixer of
/// four blocks they feed, in a row to the north; behind them, two blocks
/// high, a refinery, a compactor, the iron furnace, the new ores and a
/// column of links, for their textures. The player stands south of them.
pub const SPARK_GENERATORS: [[i64; 3]; 2] = [[121, 44, 118], [122, 44, 118]];
pub const SPARK_LINKS: [[i64; 3]; 3] = [[123, 44, 118], [124, 44, 118], [125, 44, 118]];
pub const MIXER: [[i64; 3]; 4] = [[126, 44, 118], [127, 44, 118], [128, 44, 118], [129, 44, 118]];
pub const SPARK_VIEW: [f64; 3] = [125.5, 44.0, 122.5];

/// Where `show_flora` puts the player: on a stone post looking north over
/// rows of plants and boulders to a cave mouth with fungi in it.
pub const FLORA_VIEW: [f64; 3] = [128.5, 46.0, 130.5];

fn bp([x, y, z]: [i64; 3]) -> BlockPos {
    BlockPos::new(x, y, z)
}

pub struct Demo {
    account: String,
    events: VecDeque<NetEvent>,
    blocks: BlockRegistry,
    items: ItemRegistry,
    book: RecipeBook,
    inv: Inventory,
    planets: Vec<DemoPlanet>,
    here: usize,
    /// Chunks of a planet just warped to, and when they may go.
    held: Vec<ServerMsg>,
    release: Option<Instant>,
    hold: Option<Duration>,
    stations: HashMap<BlockPos, DemoStation>,
    /// Every block of a machine, to the machine's own position (its first
    /// block), as the server groups them.
    machine_of: HashMap<BlockPos, BlockPos>,
    watched: Option<BlockPos>,
    claims: Vec<ClaimView>,
    /// Storage blocks' contents by position (first planet only).
    chests: HashMap<BlockPos, Vec<Option<ItemStack>>>,
    open_chest: Option<BlockPos>,
    /// What waits in the reclaim, as a server keeps it per account.
    reclaim: Vec<ItemStack>,
    /// Portals (`show_portals`), and the one whose window is open.
    portals: Vec<DemoPortal>,
    open_portal: Option<usize>,
    colours: ColourPalette,
    /// Damage blocks have taken (first planet's rules, any planet):
    /// position -> (damage, input of the last hit).
    damage: HashMap<BlockPos, (u32, u32)>,
    /// The input and interval of the last swing, for the cadence.
    last_swing: Option<(u32, u32)>,
    /// `show_mining`: a block someone else keeps chipping at, so its bar
    /// stays up for a screenshot; re-sent every second.
    chipped: Option<(BlockPos, u32)>,
    chip_timer: f64,
    last: Instant,
    push_timer: f64,
    /// The skill tree and our character (milestone 12), ruled by the
    /// server's own `Character` methods.
    tree: SkillTree,
    character: Character,
    /// Creatures and fighting (milestone 13).
    fight: DemoFight,
}

/// Where `show_creatures` and `show_defeat` put the player.
pub const FIGHT_VIEW: [f64; 3] = [128.5, 44.0, 129.5];

/// A creature as the demo keeps it.
#[derive(Clone, Debug)]
struct DemoCreature {
    id: u32,
    kind: u16,
    tier: u8,
    pos: DVec3,
    yaw: u16,
    pose: bm_protocol::Pose,
    damage: u32,
    angry: bool,
    dead: bool,
    looted: bool,
    /// Walks back and forth between two x positions at this speed.
    pace: Option<(f64, f64, f64)>,
    /// Someone else keeps hitting it (a stand-in for another player), so
    /// its numbers keep floating up for a screenshot.
    beaten: bool,
    /// Exotic buffs (`creatures::Exotic` bits).
    exotic: u8,
}

/// A thrown bomb waiting for its fuse.
#[derive(Clone, Copy, Debug)]
struct DemoBomb {
    /// Demo seconds it goes off at.
    at: f64,
    pos: DVec3,
    radius: f32,
    damage: u32,
    crit_chance: f32,
    crit_effect: f32,
}

#[derive(Default)]
struct DemoFight {
    creatures: Vec<DemoCreature>,
    /// The demo's server tick, three a snapshot at 20 a second.
    tick: u64,
    snap_timer: f64,
    beat_timer: f64,
    beats: u32,
    rng: u64,
    /// Seconds the fight has run.
    clock: f64,
    bombs: Vec<DemoBomb>,
    /// `--demo-ui bombs`: blasts going off here in turn, every
    /// `SHOWCASE_SECONDS`, and when the next one is.
    showcase: Vec<DVec3>,
    next_showcase: f64,
    showcased: usize,
}

/// `--demo-ui bombs`: seconds between the blasts going off in view.
const SHOWCASE_SECONDS: f64 = 1.6;

impl DemoFight {
    /// A number in 0..1 (xorshift; the demo's own dice).
    fn rand(&mut self) -> f64 {
        if self.rng == 0 {
            self.rng = 0x9e37_79b9_7f4a_7c15;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn shape() -> PlanetShape {
    PlanetShape::new(SHAPE.0, SHAPE.1)
}

impl Demo {
    pub fn new(account: &str) -> Self {
        let blocks = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let book = RecipeBook::builtin(&items);
        let id = |n: &str| items.id(n).expect("builtin item");
        let mut inv = Inventory::default();
        let sample: [(&str, u16, u16); 23] = [
            ("stone_hammer", 1, 38),
            ("stone", 64, 0),
            ("soil", 37, 0),
            ("timber", 12, 0),
            ("glass", 8, 0),
            ("brick", 40, 0),
            ("workbench", 1, 0),
            ("claim_stone", 1, 0),
            ("copper_axe", 1, 310),
            ("glow_crystal", 3, 0),
            ("trunk", 5, 0),
            ("sand", 20, 0),
            ("copper_ore", 6, 0),
            ("coal", 9, 0),
            ("copper_ingot", 2, 0),
            ("stone_shovel", 1, 0),
            ("furnace", 1, 0),
            ("stones", 24, 0),
            ("stick", 12, 0),
            ("foliage", 16, 0),
            ("flint", 3, 0),
            ("sap", 4, 0),
            ("crafting_table", 1, 0),
        ];
        for (i, (name, count, wear)) in sample.into_iter().enumerate() {
            // The first ten fill the hotbar; the rest leave a gap or two.
            let slot = if i < 10 { i } else { i + 3 };
            inv.slots[slot] = Some(ItemStack {
                wear,
                ..ItemStack::new(id(name), count)
            });
        }
        let planets = demo_planets(&blocks);
        let mut stations = HashMap::new();
        let mut machine_of = HashMap::new();
        let mut machine = |blocks: &[[i64; 3]], st: DemoStation| {
            for b in blocks {
                machine_of.insert(bp(*b), bp(blocks[0]));
            }
            stations.insert(bp(blocks[0]), st);
        };
        machine(&[TABLE], DemoStation::new(Station::CraftingTable));
        machine(
            &WORKBENCH,
            DemoStation {
                jobs: vec![JobView {
                    recipe: recipe_index(&book, &items, "copper_axe"),
                    remaining: 2,
                    progress: 0.35,
                }],
                output: vec![
                    ItemStack::new(id("copper_shovel"), 1),
                    ItemStack::new(id("extractor"), 4),
                ],
                ..DemoStation::new(Station::Workbench)
            },
        );
        machine(
            &FURNACE,
            DemoStation {
                jobs: vec![JobView {
                    recipe: recipe_index(&book, &items, "copper_ingot"),
                    remaining: 3,
                    progress: 0.4,
                }],
                output: vec![ItemStack::new(id("copper_ingot"), 2)],
                fuel: Some(ItemStack::new(id("coal"), 6)),
                heat: 60,
                paid: true,
                ..DemoStation::new(Station::Furnace)
            },
        );
        machine(
            &EXTRACTOR,
            DemoStation {
                idle: Some("An extractor needs 4 blocks touching".into()),
                ..DemoStation::new(Station::Extractor)
            },
        );
        let mine = |x0: i32, x1: i32, z0: i32, z1: i32| {
            let mut v = Vec::new();
            for x in x0..=x1 {
                for z in z0..=z1 {
                    v.push(PlotPos::new(x, 5, z));
                }
            }
            v
        };
        let mut own = mine(15, 16, 15, 16);
        own.push(PlotPos::new(17, 5, 16));
        let mut theirs = mine(18, 19, 14, 15);
        theirs.push(PlotPos::new(18, 6, 14));
        let claims = vec![
            ClaimView {
                id: 1,
                owner: account.to_string(),
                stone: BlockPos::new(126, 44, 131),
                plots: own,
                rights: vec![("Wren".into(), bm_core::rights::BUILDER | bm_core::rights::WORKER)],
                guild: Some((1, "LANT".into(), false)),
            },
            ClaimView {
                id: 2,
                owner: "Wren".into(),
                stone: BlockPos::new(150, 44, 118),
                plots: theirs,
                rights: Vec::new(),
                guild: Some((1, "LANT".into(), false)),
            },
        ];
        let mut chest = vec![None; 20];
        let inside: [(usize, &str, u16, u16); 8] = [
            (0, "copper_ore", 24, 0),
            (1, "coal", 30, 0),
            (2, "stone", 100, 0),
            (3, "stone", 100, 0),
            (5, "timber", 40, 0),
            (6, "glass", 16, 0),
            (10, "stone_axe", 1, 12),
            (11, "copper_ingot", 5, 0),
        ];
        for (slot, name, count, wear) in inside {
            chest[slot] = Some(ItemStack {
                wear,
                ..ItemStack::new(id(name), count)
            });
        }
        let chests = HashMap::from([(BlockPos::new(CHEST[0], CHEST[1], CHEST[2]), chest)]);
        let mut d = Self {
            account: account.to_string(),
            events: VecDeque::new(),
            blocks,
            items,
            book,
            inv,
            planets,
            here: 0,
            held: Vec::new(),
            release: None,
            hold: Some(Duration::from_secs_f64(ARRIVAL_HOLD)),
            stations,
            machine_of,
            watched: None,
            claims,
            chests,
            open_chest: None,
            reclaim: Vec::new(),
            portals: Vec::new(),
            open_portal: None,
            colours: ColourPalette::builtin(),
            damage: HashMap::new(),
            last_swing: None,
            chipped: None,
            chip_timer: 0.0,
            last: Instant::now(),
            push_timer: 0.0,
            tree: SkillTree::builtin(),
            character: Character::default(),
            fight: DemoFight::default(),
        };
        d.welcome();
        d
    }

    fn welcome(&mut self) {
        let w = Welcome {
            player_id: 1,
            account: self.account.clone(),
            planet: self.planets[0].info.clone(),
            universe: self.universe(),
            blocks: self.blocks.defs().to_vec(),
            items: self.items.defs().to_vec(),
            colours: self.colours.clone(),
            recipes: self.book.recipes.clone(),
            skills: self.tree.skills.clone(),
            character: self.character.clone(),
            inventory: self.inv.clone(),
            movement: MoveConfig::default(),
            spawn: PlayerState {
                pos: DVec3::from_array(SPAWN),
                vel: DVec3::ZERO,
                on_ground: true,
                hook: None,
                ..Default::default()
            },
            tick: 0,
            view_radius: 4,
            far_radius: 0,
            reach: 4.0,
            claims: self.claims.clone(),
            plot_allowance: 64,
            creatures: bm_core::creatures::CreatureKinds::builtin().defs().to_vec(),
        };
        self.events.push_back(NetEvent::Connected(Box::new(w)));
        // Friends (milestone 25).
        self.events
            .push_back(NetEvent::Msg(ServerMsg::Friends(bm_protocol::FriendsView {
                friends: vec![
                    bm_protocol::FriendView {
                        name: "Wren".into(),
                        planet: Some("Verdance".into()),
                    },
                    bm_protocol::FriendView {
                        name: "Halloran".into(),
                        planet: None,
                    },
                ],
                asked_by: vec!["Tamsin".into()],
            })));
        // A guild (milestone 25 part 2).
        {
            use bm_core::guilds::Rank;
            use bm_core::rights::{ALL, BUILDER, FARMHAND, WORKER};
            let member = |name: &str, rank, rights, powers, online| bm_protocol::MemberView {
                name: name.into(),
                rank,
                rights,
                powers,
                online,
            };
            self.events
                .push_back(NetEvent::Msg(ServerMsg::Guilds(bm_protocol::GuildsView {
                    guilds: vec![bm_protocol::GuildView {
                        id: 1,
                        name: "Lanternwrights".into(),
                        tag: "LANT".into(),
                        coin: 12_480,
                        members: vec![
                            member(&self.account, Rank::Director, ALL, bm_core::guilds::ALL_POWERS, true),
                            member("Wren", Rank::Executive, ALL, bm_core::guilds::ALL_POWERS, true),
                            member(
                                "Halloran",
                                Rank::Controller,
                                BUILDER | WORKER,
                                bm_core::guilds::COIN,
                                false,
                            ),
                            member("Tamsin", Rank::Operative, FARMHAND, 0, true),
                        ],
                        asked: vec!["Odile".into()],
                        log: vec![
                            (0, format!("{} founded Lanternwrights", self.account)),
                            (1, "Wren gave 2500 coin".into()),
                            (2, format!("{} let Tamsin join", self.account)),
                        ],
                        claims: vec![(1, false), (2, false)],
                        endeavour: 1_850,
                        buffs: vec![(bm_core::guilds::BuffKind::GrappleRun, 3, u64::MAX, true)],
                    }],
                    primary: Some(1),
                })));
        }
        // The two claims make a settlement (milestone 24 part 4).
        self.events.push_back(NetEvent::Msg(ServerMsg::Settlements {
            settlements: vec![bm_protocol::SettlementView {
                name: "Riverholm".into(),
                rank: "Hamlet".into(),
                prestige: 61_250,
                warden: "Wren".into(),
                capital: true,
                claims: vec![1, 2],
            }],
            prestige: vec![(1, 18_400), (2, 42_850)],
        }));
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
        // Named regions (milestone 14), as the server names them; the demo's
        // land is all plain.
        let shape = self.planets[0].info.shape;
        let seed = bm_core::regions::layout_seed(&self.planets[0].info.name, shape);
        let map = bm_core::regions::RegionMap::new(shape, seed);
        let lands = vec![bm_core::regions::LandClass::Plain; map.len()];
        let names = bm_core::regions::RegionNames::builtin().name_all("temperate", &lands, seed);
        let here = map.locate(SPAWN[0], SPAWN[2]) as u16;
        self.events.push_back(NetEvent::Msg(ServerMsg::Regions {
            layout_seed: seed,
            names,
            discovered: vec![here],
        }));
    }

    fn universe(&self) -> Vec<PlanetSummary> {
        self.planets.iter().map(|p| p.summary.clone()).collect()
    }

    /// Keeps a warped-to planet's ground back for `secs` (infinite: for good),
    /// so the travelling screen stays up.
    pub fn hold_arrivals(&mut self, secs: f64) {
        self.hold = Duration::try_from_secs_f64(secs).ok();
    }

    fn warp(&mut self, planet: u16) {
        let Some(to) = self.planets.iter().position(|p| p.info.id == planet) else {
            return self.notice("The shrine knows no such world.");
        };
        if to == self.here {
            let name = self.planets[to].info.name.clone();
            return self.notice(&format!("You are already on {name}."));
        }
        self.planets[self.here].summary.players -= 1;
        self.planets[to].summary.players += 1;
        self.here = to;
        let claims = if to == 0 { self.claims.clone() } else { Vec::new() };
        self.events.push_back(NetEvent::Msg(ServerMsg::Travel(Box::new(Travel {
            planet: self.planets[to].info.clone(),
            spawn: PlayerState {
                pos: DVec3::from_array(SPAWN),
                vel: DVec3::ZERO,
                on_ground: true,
                hook: None,
                ..Default::default()
            },
            tick: 0,
            claims,
            yaw: None,
        }))));
        self.events.push_back(NetEvent::Msg(ServerMsg::Universe {
            planets: self.universe(),
        }));
        self.held = self.chunks_around_spawn();
        self.release = self.hold.map(|h| Instant::now() + h);
    }

    fn chunks_around_spawn(&self) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        // Whole columns, nearest first: light needs complete columns.
        let centre = BlockPos::new(SPAWN[0] as i64, 0, SPAWN[2] as i64).chunk();
        let mut cols: Vec<(i32, i32)> = (-3..=3).flat_map(|dx| (-3..=3).map(move |dz| (dx, dz))).collect();
        cols.sort_by_key(|(dx, dz)| dx * dx + dz * dz);
        for (dx, dz) in cols {
            for cy in 0..SHAPE.1 as i32 {
                let pos = ChunkPos::new(centre.x + dx, cy, centre.z + dz);
                let data = self.make_chunk(pos).encode();
                out.push(ServerMsg::Chunk { pos, data });
            }
        }
        out
    }

    /// Turns the first planet into a showroom for colours: level ground, no
    /// trees, the whole palette as a wall of stone (hues across, shades up;
    /// the greys last) and in front of it other materials in a spread of
    /// colours. A pack of coloured stacks too. The ground is sent again.
    pub fn show_colours(&mut self) {
        let b = |n: &str| self.blocks.id(n).expect("builtin block");
        let (trunk, foliage) = (b("trunk"), b("foliage"));
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.retain(|_, s| s.id != trunk && s.id != foliage);
        let stone = b("stone");
        let [x0, y0, z0] = WALL;
        for hue in 0..17i64 {
            for shade in 0..15i64 {
                let c = Colour((hue * 15 + shade + 1) as u8);
                planet.edits.insert(
                    BlockPos::new(x0 + hue, y0 + 14 - shade, z0),
                    BlockState::coloured(stone, c),
                );
            }
            for y in GROUND + 1..y0 {
                planet
                    .edits
                    .insert(BlockPos::new(x0 + hue, y, z0), BlockState::plain(stone));
            }
        }
        // Other materials, a column each, three colours up: a bright, a
        // middle and a dark shade of a hue that moves along the row.
        let materials = [
            "soil",
            "sand",
            "gravel",
            "trunk",
            "timber",
            "foliage",
            "glass",
            "brick",
            "ash",
            "snow",
            "sandstone",
            "ice",
            "turf",
            "deep_stone",
            "ash",
            "brick",
            "glass",
        ];
        for (i, name) in materials.iter().enumerate() {
            let block = b(name);
            let hue = (i as i64 * 5) % 16;
            for (k, shade) in [10i64, 5, 2].into_iter().enumerate() {
                let c = Colour((hue * 15 + shade + 1) as u8);
                let pos = BlockPos::new(x0 + i as i64, GROUND + 1 + k as i64, z0 + 2);
                planet.edits.insert(pos, BlockState::coloured(block, c));
            }
            // And the material as it is, on top.
            planet.edits.insert(
                BlockPos::new(x0 + i as i64, GROUND + 4, z0 + 2),
                BlockState::plain(block),
            );
        }
        let c = |name: &str| self.colours.id(name).expect("palette colour");
        let id = |n: &str| self.items.id(n).expect("builtin item");
        let coloured: [(&str, &str, u16); 12] = [
            ("stone", "Rich Azure", 64),
            ("stone", "Bright Crimson", 40),
            ("brick", "Deep Moss", 25),
            ("timber", "Light Amber", 18),
            ("glass", "Soft Violet", 9),
            ("sand", "Pale Gold", 33),
            ("ash", "Dusky Rose", 12),
            ("foliage", "True Teal", 7),
            ("soil", "Midnight Cobalt", 30),
            ("stone", "Snow", 50),
            ("stone", "Coal", 50),
            ("trunk", "Strong Ember", 6),
        ];
        let before = self.inv.clone();
        for (i, (item, colour, n)) in coloured.into_iter().enumerate() {
            let slot = if i < 7 { 3 + i } else { 13 + i };
            self.inv.slots[slot] = Some(ItemStack::coloured(id(item), c(colour), n));
        }
        self.slots_since(&before);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// Turns the first planet into a showroom for milestone 22's doors and
    /// furnishings: doors and trapdoors shut and open, upright poles and
    /// lying beams of every style and material, panes, furniture and every
    /// storage block, in rows from near to far.
    pub fn show_furnishings(&mut self) {
        use bm_core::doors::{Side, open_door, open_trapdoor, shut_door, shut_trapdoor};
        use bm_core::forms::{Form, FormKind, Style};
        let b = |n: &str| self.blocks.id(n).expect("builtin block");
        let c = |name: &str| self.colours.id(name).expect("palette colour");
        let mut put = Vec::new();
        let y = 44;
        let mut at = |x: i64, z: i64, name: &str, shape: bm_core::Shape, colour: Option<&str>| {
            put.push((
                BlockPos::new(x, y, z),
                BlockState {
                    id: b(name),
                    colour: colour.map(c).unwrap_or(Colour::NATURAL),
                    shape,
                },
            ));
        };
        let mats = ["wood", "stone", "metal", "crystal"];
        let styles = [
            ("plain", Style::Plain),
            ("stylish", Style::Stylish),
            ("ornate", Style::Ornate),
        ];
        // Nearest: doors shut and open, trapdoors shut and open.
        for (k, (s, _)) in styles.iter().enumerate() {
            for (m, mat) in mats.iter().enumerate() {
                let x = 116 + (k * 8 + m * 2) as i64;
                let open = (k + m) % 2 == 1;
                let name = format!("{s}_{mat}_door{}", if open { "_open" } else { "" });
                let shape = if open {
                    open_door(Side::South)
                } else {
                    shut_door(Side::South)
                };
                at(x, 127, &name, shape, None);
                let name = format!("{s}_{mat}_trapdoor{}", if open { "_open" } else { "" });
                let shape = if open {
                    open_trapdoor(Side::South)
                } else {
                    shut_trapdoor()
                };
                at(x + 1, 127, &name, shape, None);
            }
        }
        // Upright poles, then beams lying along x, by style then material.
        for (k, (s, style)) in styles.iter().enumerate() {
            for (m, mat) in mats.iter().enumerate() {
                let x = 116 + (k * 8 + m * 2) as i64;
                let tint = (*mat == "wood").then_some("Rich Azure");
                let pole = Form::new(FormKind::Pole, *style, 1).unwrap().shape();
                at(x, 124, &format!("{s}_{mat}_pole"), pole, None);
                let beam = Form::new(FormKind::Beam, *style, 0).unwrap().shape();
                at(x, 121, &format!("{s}_{mat}_beam"), beam, tint);
                at(x + 1, 121, &format!("{s}_{mat}_beam"), beam, tint);
            }
        }
        // Panes facing the view, then the plinth, table and chair.
        for (k, (s, style)) in styles.iter().enumerate() {
            for d in 0..2 {
                let pane = Form::new(FormKind::Pane, *style, 2).unwrap().shape();
                at(116 + (k * 3) as i64 + d, 118, &format!("{s}_glass_pane"), pane, None);
            }
        }
        let form = |kind, turn| Form::new(kind, Style::Plain, turn).unwrap().shape();
        at(126, 118, "stone_plinth", form(FormKind::Plinth, 0), None);
        at(128, 118, "wood_table", form(FormKind::Table, 0), None);
        at(129, 118, "wood_chair", form(FormKind::Chair, 3), None);
        at(127, 118, "wood_chair", form(FormKind::Chair, 1), Some("Bright Crimson"));
        // Furthest: every storage block.
        for (m, mat) in ["wood", "stone", "metal", "crystal", "wicker"].iter().enumerate() {
            for (k, (s, _)) in styles.iter().enumerate() {
                let x = 116 + (m * 4 + k) as i64;
                at(x, 115, &format!("{s}_{mat}_storage"), bm_core::Shape::FULL, None);
            }
        }
        // A post to look down from (`FURNISHINGS_VIEW`).
        for y in 44..47 {
            put.push((BlockPos::new(128, y, 131), BlockState::plain(b("stone"))));
        }
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// Signs (milestone 22 part 4) on a stone wall ahead of the spawn post
    /// (`SHAPES_VIEW`): a three by two panel sign in coloured text, a stone
    /// and a glowing metal sign hung on the wall, and a wood sign on its
    /// post in front.
    pub fn show_signs(&mut self) {
        use bm_core::forms::{Form, FormKind, Style};
        use bm_core::signs::{Align, SignText, SignView};
        let b = |n: &str| self.blocks.id(n).expect("builtin block");
        let form = |kind, turn| Form::new(kind, Style::Plain, turn).unwrap().shape();
        let stone = BlockState::plain(b("stone"));
        let mut put = Vec::new();
        for x in 122..136 {
            for y in 44..48 {
                put.push((BlockPos::new(x, y, 120), stone));
            }
        }
        let at = |name: &str, shape| BlockState {
            id: b(name),
            colour: Colour::NATURAL,
            shape,
        };
        for x in 124..127 {
            for y in 44..46 {
                put.push((
                    BlockPos::new(x, y, 121),
                    at("wood_sign_panel", form(FormKind::Panel, 2)),
                ));
            }
        }
        put.push((BlockPos::new(128, 45, 121), at("stone_sign", form(FormKind::Sign, 6))));
        put.push((BlockPos::new(130, 45, 121), at("metal_sign", form(FormKind::Sign, 6))));
        put.push((BlockPos::new(129, 44, 124), at("wood_sign", form(FormKind::Sign, 2))));
        for y in 44..46 {
            put.push((BlockPos::new(128, y, 130), stone));
        }
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
        let sign = |text: &str, align, across, up| SignView {
            text: SignText {
                text: text.into(),
                more: String::new(),
                align,
                italic: false,
            },
            across,
            up,
        };
        let signs = [
            (
                BlockPos::new(124, 44, 121),
                sign(
                    ":#a22:Mira's Forge\n:#222:Tools mended :#264:while you wait",
                    Align::Centre,
                    3,
                    2,
                ),
            ),
            (BlockPos::new(128, 45, 121), sign("North gate", Align::Centre, 1, 1)),
            (
                BlockPos::new(130, 45, 121),
                sign("Portal hall\nthis way", Align::Left, 1, 1),
            ),
            (
                BlockPos::new(129, 44, 124),
                sign("Welcome to\nHearthmoor", Align::Centre, 1, 1),
            ),
        ];
        for (anchor, view) in signs {
            self.events.push_back(NetEvent::Msg(ServerMsg::Sign {
                anchor,
                sign: Some(view),
            }));
        }
    }

    /// Turns the first planet into a showroom for chisel cuts (ADR-0008):
    /// level ground and, ahead of the spawn, a row per family (round,
    /// bevel, slope, square, nearest first) of the same
    /// cuts in natural and coloured materials. With `in_hand`, every chisel
    /// in the hotbar, a bevel chisel held, and a whole brick to aim at.
    pub fn show_shapes(&mut self, in_hand: bool) {
        use bm_core::{Shape, ShapeFamily, octant};
        let b = |n: &str| self.blocks.id(n).expect("builtin block");
        let c = |name: &str| self.colours.id(name).expect("palette colour");
        let o = |x, y, z| 1u8 << octant(x, y, z);
        // Front is +z, towards the spawn.
        let cuts: [u8; 6] = [
            o(0, 1, 1) | o(1, 1, 1),              // the top front edge: a ramp
            o(1, 1, 1),                           // one top corner
            0b1111_0000,                          // the top half: a slab
            o(1, 0, 1) | o(1, 1, 1),              // a front side edge
            o(0, 1, 1) | o(1, 1, 1) | o(1, 1, 0), // three of the top: a step
            o(0, 1, 1) | o(1, 1, 0),              // two opposite top corners
        ];
        let looks = [
            ("brick", None),
            ("timber", Some("Rich Azure")),
            ("sandstone", None),
            ("glass", None),
            ("brick", Some("Bright Crimson")),
            ("ash", Some("True Teal")),
        ];
        let mut put = Vec::new();
        let rows = [
            ShapeFamily::Round,
            ShapeFamily::Bevel,
            ShapeFamily::Slope,
            ShapeFamily::Square,
        ];
        for (row, family) in rows.into_iter().enumerate() {
            let (z, y) = (125 - 2 * row as i64, 44);
            for (k, &cut) in cuts.iter().enumerate() {
                let x = 123 + 2 * k as i64;
                // Natural ground takes slopes: show natural stone there.
                let (name, colour) = if family == ShapeFamily::Slope && k % 2 == 0 {
                    ("stone", None)
                } else {
                    looks[k]
                };
                let state = BlockState {
                    id: b(name),
                    colour: colour.map(c).unwrap_or(Colour::NATURAL),
                    shape: Shape::new(family, cut).expect("a shape"),
                };
                put.push((BlockPos::new(x, y, z), state));
            }
        }
        // A pillar behind the rows to look down from (`SHAPES_VIEW`).
        for y in 44..46 {
            put.push((BlockPos::new(128, y, 130), BlockState::plain(b("stone"))));
        }
        if in_hand {
            put.push((BlockPos::new(128, 44, 127), BlockState::plain(b("stone"))));
            put.push((BlockPos::new(128, 45, 127), BlockState::plain(b("brick"))));
        }
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        if in_hand {
            let id = |n: &str| self.items.id(n).expect("builtin item");
            let before = self.inv.clone();
            let chisels = [
                "square_chisel",
                "slope_chisel",
                "bevel_chisel",
                "round_chisel",
                "precise_square_chisel",
                "precise_slope_chisel",
            ];
            for (i, name) in chisels.into_iter().enumerate() {
                self.inv.slots[4 + i] = Some(ItemStack::new(id(name), 1));
            }
            self.slots_since(&before);
        }
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// Level ground with a stone arch to swing from (its beam along
    /// [`GRAPPLE_BEAM`]), and both grapples in the hotbar's last
    /// slots.
    pub fn show_grapple(&mut self) {
        let stone = BlockState::plain(self.blocks.id("stone").expect("builtin block"));
        let [x0, y, z] = GRAPPLE_BEAM;
        let mut put = Vec::new();
        for x in x0..x0 + 11 {
            put.push((BlockPos::new(x, y, z), stone));
        }
        for py in 44..y {
            put.push((BlockPos::new(x0, py, z), stone));
            put.push((BlockPos::new(x0 + 10, py, z), stone));
        }
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        let id = |n: &str| self.items.id(n).expect("builtin item");
        let before = self.inv.clone();
        for (i, name) in ["copper_grapple", "iron_grapple"].into_iter().enumerate() {
            self.inv.slots[7 + i] = Some(ItemStack::new(id(name), 1));
        }
        self.slots_since(&before);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// A long sealed stone hall around the spawn, dark but for the torches at
    /// its far (north) half: one standing on the floor and three on the
    /// walls. The spawn end is out of their reach, for the held torch in the
    /// hotbar's last slot.
    pub fn show_torches(&mut self) {
        let b = |n: &str| BlockState::plain(self.blocks.id(n).expect("builtin block"));
        let stone = b("stone");
        let (x0, x1, z0, z1, top) = TORCH_HALL;
        let mut put = Vec::new();
        for x in x0 - 1..=x1 + 1 {
            for z in z0 - 1..=z1 + 1 {
                for y in GROUND..=top + 1 {
                    let inside = (x0..=x1).contains(&x) && (z0..=z1).contains(&z) && y > GROUND && y <= top;
                    put.push((BlockPos::new(x, y, z), if inside { BlockState::AIR } else { stone }));
                }
            }
        }
        let y = GROUND + 1;
        put.push((BlockPos::new(127, y, 115), b("torch")));
        put.push((BlockPos::new(x1, y + 1, 112), b("torch_east")));
        put.push((BlockPos::new(x0, y + 1, 109), b("torch_west")));
        put.push((BlockPos::new(129, y + 1, z0), b("torch_north")));
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        let before = self.inv.clone();
        self.inv.slots[9] = Some(ItemStack::new(self.items.id("torch").expect("builtin item"), 12));
        self.slots_since(&before);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// Milestone 26: the torch hall lit in colour. Gem torches on the walls,
    /// a row of lume in six colours along the floor, a lume lantern at the
    /// far end, an amethyst torch in hand; no white light anywhere.
    /// Milestone 27: an atlas in hand (slot 9). Opening it answers with a
    /// 48-chunk map of this world, painted from a fixture landscape (islands,
    /// hills and snowy peaks, wrapping at the edges: a picture, not world
    /// generation), the spawn's region and two more discovered, the rest
    /// fogged as the server fogs it.
    pub fn show_atlas(&mut self) {
        let before = self.inv.clone();
        let copper = self.items.id("copper_ore").expect("builtin item");
        let mut atlas = ItemStack::new(self.items.id("atlas").expect("builtin item"), 1);
        atlas.tag = bm_core::atlas::pack(None, Some(copper));
        self.inv.slots[9] = Some(atlas);
        self.inv.slots[8] = Some(ItemStack::new(self.items.id("iron_ore").expect("builtin item"), 5));
        self.slots_since(&before);
    }

    /// The demo atlas's world: its shape, regions and the ones discovered.
    fn atlas_world(&self) -> (PlanetShape, bm_core::regions::RegionMap, u64, Vec<u16>) {
        let here = &self.planets[self.here].info;
        let shape = PlanetShape::new(48, SHAPE.1);
        let seed = bm_core::regions::layout_seed(&here.name, shape);
        let map = bm_core::regions::RegionMap::new(shape, seed);
        let home = map.locate(SPAWN[0], SPAWN[2]);
        let mut discovered = vec![home as u16];
        let [hx, hz] = map.centre(home);
        let mut others: Vec<usize> = (0..map.len()).filter(|&r| r != home).collect();
        others.sort_by(|&a, &b| map.distance(a, hx, hz).total_cmp(&map.distance(b, hx, hz)));
        discovered.extend(others.iter().take(2).map(|&r| r as u16));
        (shape, map, seed, discovered)
    }

    /// A fixture heat map for the demo atlas: bands from a pattern, hidden
    /// outside the discovered regions (as the server hides them).
    fn atlas_heat(&mut self, item: Option<bm_core::ItemId>) {
        use bm_core::atlas;
        let (shape, map, _, discovered) = self.atlas_world();
        let side = atlas::heat_side(shape);
        let k = item.map_or(0.0, |i| i.0 as f64 * 0.37);
        let bands: Vec<u8> = (0..side * side)
            .map(|i| {
                let (x, z) = ((i % side) as f64, (i / side) as f64);
                let c = atlas::HEAT_CELL as f64;
                if !discovered.contains(&(map.locate(x * c + c / 2.0, z * c + c / 2.0) as u16)) {
                    return atlas::HEAT_HIDDEN;
                }
                let tau = std::f64::consts::TAU / side as f64;
                let v =
                    (x * tau * 3.0 + k).sin() * (z * tau * 4.0 - k).cos() + (x * tau * 7.0 + z * tau * 5.0).sin() * 0.4;
                atlas::heat_band(v - 0.2, 1.2)
            })
            .collect();
        self.events.push_back(NetEvent::Msg(ServerMsg::AtlasHeat {
            planet: self.planets[self.here].info.id,
            item,
            side: side as u16,
            bands: if item.is_some() {
                lz4_flex::compress_prepend_size(&bands)
            } else {
                Vec::new()
            },
        }));
    }

    fn atlas_load(&mut self, slot: u8, from: Option<u8>) {
        let Some(tag) = self.inv.slots[slot as usize].map(|s| s.tag) else {
            return;
        };
        let before = self.inv.clone();
        if let Some(old) = bm_core::atlas::loaded(tag) {
            self.inv.add(&self.items, old, 1);
        }
        let incoming = from.and_then(|f| self.inv.take_one(f as usize)).map(|s| s.item);
        if let Some(s) = self.inv.slots[slot as usize].as_mut() {
            s.tag = bm_core::atlas::pack(bm_core::atlas::bound(tag), incoming);
        }
        self.slots_since(&before);
        self.atlas_heat(incoming);
    }

    fn open_atlas(&mut self, slot: u8) {
        use bm_core::atlas;
        let here = self.planets[self.here].info.clone();
        let (shape, map, seed, discovered) = self.atlas_world();
        let lands = vec![bm_core::regions::LandClass::Plain; map.len()];
        let names = bm_core::regions::RegionNames::builtin().name_all("temperate", &lands, seed);
        let mut loaded = None;
        if let Some(s) = self.inv.slots[slot as usize].as_mut() {
            loaded = atlas::loaded(s.tag);
            s.tag = atlas::pack(Some(here.id), loaded);
        }
        let changes = vec![(slot, self.inv.slots[slot as usize])];
        self.events.push_back(NetEvent::Msg(ServerMsg::Slots { changes }));
        self.events
            .push_back(NetEvent::Msg(ServerMsg::Atlas(bm_protocol::AtlasView {
                slot,
                planet: here.id,
                name: here.name.clone(),
                shape,
                layout_seed: seed,
                region_names: names,
                discovered: discovered.clone(),
                shrine: BlockPos::new(SPAWN[0] as i64 + 4, GROUND + 1, SPAWN[2] as i64),
                closed: false,
            })));
        let colour = |n: &str| {
            let d = self.blocks.get(self.blocks.id(n).expect("builtin block"));
            d.top_color.unwrap_or(d.color)
        };
        let (water, sand, turf, stone, snow) = (
            colour("water"),
            colour("sand"),
            colour("turf"),
            colour("stone"),
            colour("snow"),
        );
        let n = atlas::map_pixels(shape) as f64;
        let tau = std::f64::consts::TAU;
        // Land height at map pixel (x, z): sums of whole waves, so it wraps.
        let height = |x: f64, z: f64| {
            let (u, v) = (x / n * tau, z / n * tau);
            ((u * 2.0).sin() * (v * 3.0).cos() * 22.0
                + (u * 5.0 + 1.3).cos() * 9.0
                + (v * 4.0 + 0.7).sin() * 8.0
                + ((u + v) * 7.0).sin() * 4.0
                + 6.0)
                .round() as i64
        };
        let tiles = atlas::tiles_per_side(shape);
        for tz in 0..tiles {
            for tx in 0..tiles {
                let mut px = Vec::with_capacity(atlas::TILE_PIXELS * atlas::TILE_PIXELS * 3);
                for pz in 0..atlas::TILE_PIXELS {
                    for pxx in 0..atlas::TILE_PIXELS {
                        let (x, z) = atlas::pixel_column(shape, (tx, tz), pxx, pz).expect("whole tiles");
                        let r = map.locate(x as f64, z as f64) as u16;
                        if !discovered.contains(&r) {
                            px.extend_from_slice(&atlas::FOG);
                            continue;
                        }
                        let (mx, mz) = (
                            (x / atlas::BLOCKS_PER_PIXEL) as f64,
                            (z / atlas::BLOCKS_PER_PIXEL) as f64,
                        );
                        let h = height(mx, mz);
                        let (west, north) = (height(mx - 1.0, mz), height(mx, mz - 1.0));
                        let c = match h {
                            h if h < 0 => atlas::shade(water, 0, 0, 0, 0, Some(-h)),
                            h if h < 3 => atlas::shade(sand, h, west, north, 0, None),
                            h if h < 24 => atlas::shade(turf, h, west, north, 0, None),
                            h if h < 32 => atlas::shade(stone, h, west, north, 0, None),
                            h => atlas::shade(snow, h, west, north, 0, None),
                        };
                        px.extend_from_slice(&c);
                    }
                }
                self.events.push_back(NetEvent::Msg(ServerMsg::AtlasTile {
                    planet: here.id,
                    tile: (tx, tz),
                    rgb: lz4_flex::compress_prepend_size(&px),
                }));
            }
        }
        if loaded.is_some() {
            self.atlas_heat(loaded);
        }
    }

    pub fn show_lights(&mut self) {
        let b = |n: &str| BlockState::plain(self.blocks.id(n).expect("builtin block"));
        let paint = |n: &str, c: &str| {
            BlockState::coloured(
                self.blocks.id(n).expect("builtin block"),
                self.colours.id(c).expect("palette colour"),
            )
        };
        let stone = b("stone");
        let (x0, x1, z0, z1, top) = TORCH_HALL;
        let mut put = Vec::new();
        for x in x0 - 1..=x1 + 1 {
            for z in z0 - 1..=z1 + 1 {
                for y in GROUND..=top + 1 {
                    let inside = (x0..=x1).contains(&x) && (z0..=z1).contains(&z) && y > GROUND && y <= top;
                    put.push((BlockPos::new(x, y, z), if inside { BlockState::AIR } else { stone }));
                }
            }
        }
        let y = GROUND + 1;
        // Gem torches along both walls, one colour each.
        for (i, gem) in ["ruby", "topaz", "emerald"].into_iter().enumerate() {
            put.push((
                BlockPos::new(x1, y + 1, 124 - 5 * i as i64),
                b(&format!("{gem}_torch_east")),
            ));
        }
        for (i, gem) in ["sapphire", "amethyst", "diamond"].into_iter().enumerate() {
            put.push((
                BlockPos::new(x0, y + 1, 124 - 5 * i as i64),
                b(&format!("{gem}_torch_west")),
            ));
        }
        // Lume in the floor down the middle, light colours and dark.
        for (i, c) in [
            "Bright Crimson",
            "Bright Gold",
            "Bright Lime",
            "Bright Teal",
            "Bright Cobalt",
            "Bright Orchid",
            "Dark Orchid",
        ]
        .into_iter()
        .enumerate()
        {
            put.push((BlockPos::new(128, GROUND, 125 - 3 * i as i64), paint("glow_crystal", c)));
        }
        put.push((BlockPos::new(128, y, z0), paint("crystal_lantern", "Bright Sky")));
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        let before = self.inv.clone();
        self.inv.slots[9] = Some(ItemStack::new(
            self.items.id("amethyst_torch").expect("builtin item"),
            2,
        ));
        self.slots_since(&before);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// Milestone 16 part 2: a field of every crop at every stage on tilled
    /// soil (rice by a row of water), a tiller and seeds in hand.
    pub fn show_farm(&mut self) {
        let b = |n: &str| BlockState::plain(self.blocks.id(n).expect("builtin block"));
        let crops = bm_core::crops::CropKinds::builtin(&self.blocks, &self.items);
        let mut put = Vec::new();
        for (i, (name, _, _)) in FARM_CROPS.iter().enumerate() {
            let def = crops.get(crops.by_name(name).expect("crop")).expect("crop");
            for stage in 0..4 {
                let (x, z) = (FARM_X + i as i64, FARM_Z + stage);
                put.push((BlockPos::new(x, GROUND, z), b("tilled_soil")));
                put.push((
                    BlockPos::new(x, GROUND + 1, z),
                    BlockState::plain(def.stages[stage as usize]),
                ));
            }
        }
        for z in FARM_Z..FARM_Z + 4 {
            put.push((BlockPos::new(FARM_X + 7, GROUND, z), b("water")));
        }
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        let id = |n: &str| self.items.id(n).expect("builtin item");
        let before = self.inv.clone();
        for (slot, name, n) in [
            (0, "wood_tiller", 1),
            (1, "tuber_bulb", 6),
            (2, "oat_seed", 4),
            (3, "rice_seed", 4),
            (4, "wheat_seed", 3),
            (5, "dewberry_seed", 5),
            (6, "oat_grain", 9),
            (7, "flour", 2),
            (8, "tuber_risotto", 1),
            (9, "meat_loaf", 1),
        ] {
            self.inv.slots[slot] = Some(ItemStack::new(id(name), n));
        }
        self.slots_since(&before);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// The demo field's answer about a crop: tilled, not wild, its time to
    /// ripe by its stage.
    fn look_crop(&mut self, pos: BlockPos) {
        let (dx, dz) = (pos.x - FARM_X, pos.z - FARM_Z);
        if pos.y != GROUND + 1 || !(0..7).contains(&dx) || !(0..4).contains(&dz) {
            return;
        }
        let (_, name, hours) = FARM_CROPS[dx as usize];
        let stage = dz as u8 + 1;
        let ripe_in = ((3 - dz).max(0) as f64 / 3.0 * hours * 0.75 * 3600.0) as u32;
        self.events.push_back(NetEvent::Msg(ServerMsg::CropInfo {
            pos,
            name: name.into(),
            stage,
            stages: 4,
            ripe_in,
            wild: false,
            tilled: true,
            dry: false,
        }));
    }

    /// Milestone 16: a hotbar of food, buckets and berries, on the spawn.
    pub fn show_food(&mut self) {
        let id = |n: &str| self.items.id(n).expect("builtin item");
        let before = self.inv.clone();
        for (slot, name, n) in [
            (0, "meat_broth", 2),
            (1, "cooked_meat", 6),
            (2, "raw_meat", 3),
            (3, "dewberry_soup", 1),
            (4, "tuber_soup", 2),
            (5, "keen_soup", 1),
            (6, "guarding_broth", 1),
            (7, "dewberry", 14),
            (8, "bucket", 2),
            (9, "water_bucket", 1),
        ] {
            self.inv.slots[slot] = Some(ItemStack::new(id(name), n));
        }
        self.slots_since(&before);
    }

    /// Eats one from `slot`, as the server would (the client has applied
    /// the meal to its own body already; the demo does not simulate it).
    fn eat(&mut self, slot: u8) {
        let edible = self
            .inv
            .get(slot as usize)
            .and_then(|s| self.items.get(s.item))
            .is_some_and(|d| d.food().is_some());
        if edible {
            self.inv.take_one(slot as usize);
        }
    }

    /// Two open portals ahead of the spawn, linked to each other, in a brick
    /// surround (the surround is only a look: a portal is its own blocks),
    /// two more elsewhere to link with, and a pack with tokens for them and
    /// wayshards to fuel with.
    pub fn show_portals(&mut self) {
        let frame = |[x, y, z]: [i64; 3], width, height| PortalFrame {
            corner: BlockPos::new(x, y, z),
            axis: PortalAxis::X,
            width,
            height,
        };
        self.portals = vec![
            DemoPortal {
                planet: 0,
                frame: frame(PORTALS[0], 1, 2),
                link: Some(1),
                fuel: 37,
            },
            DemoPortal {
                planet: 0,
                frame: frame(PORTALS[1], 1, 2),
                link: Some(0),
                fuel: 12,
            },
            DemoPortal {
                planet: 1,
                frame: frame([212, 51, 96], 1, 2),
                link: None,
                fuel: 0,
            },
            DemoPortal {
                planet: 3,
                frame: frame([812, 47, 140], 2, 3),
                link: None,
                fuel: 0,
            },
        ];
        let brick = BlockState::plain(self.blocks.id("brick").expect("builtin block"));
        for [x, y, z] in PORTALS {
            for dy in -1..=2 {
                for dx in [-1, 1] {
                    self.planets[0].edits.insert(BlockPos::new(x + dx, y + dy, z), brick);
                }
            }
            self.planets[0].edits.insert(BlockPos::new(x, y + 2, z), brick);
        }
        for i in 0..self.portals.len() {
            self.refresh_portal(i, false);
        }
        let id = |n: &str| self.items.id(n).expect("builtin item");
        let (token, shard) = (id("portal_token"), id("wayshard"));
        let before = self.inv.clone();
        self.inv.slots[8] = Some(ItemStack::tagged(token, 3));
        self.inv.slots[9] = Some(ItemStack::new(shard, 60));
        self.inv.slots[25] = Some(ItemStack::tagged(token, 4));
        self.inv.slots[26] = Some(ItemStack::new(shard, 40));
        self.slots_since(&before);
        let names = (0..self.portals.len()).map(|i| self.token_name(i)).collect();
        self.events.push_back(NetEvent::Msg(ServerMsg::TokenNames { names }));
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// "Verdance, near 812, 140".
    fn portal_place(&self, i: usize) -> String {
        let p = &self.portals[i];
        let c = p.frame.corner;
        format!("{}, near {}, {}", self.planets[p.planet].info.name, c.x, c.z)
    }

    /// A token's tag and name: "2 by 3, Verdance, near 812, 140".
    fn token_name(&self, i: usize) -> (u32, String) {
        let f = self.portals[i].frame;
        let size = crate::portal::size_text(f.width, f.height);
        (i as u32 + 1, format!("{size}, {}", self.portal_place(i)))
    }

    fn trip_cost(&self, i: usize) -> u32 {
        let p = &self.portals[i];
        match p.link {
            Some(l) if self.portals[l].planet != p.planet => 2,
            _ => 1,
        }
    }

    fn portal_is_open(&self, i: usize) -> bool {
        self.portals[i].link.is_some() && self.portals[i].fuel >= self.trip_cost(i)
    }

    /// Shows a portal on this planet as surfaces when open, conduits when
    /// not, telling the client of each block that changed if `tell`.
    fn refresh_portal(&mut self, i: usize, tell: bool) {
        let p = &self.portals[i];
        if p.planet != self.here {
            return;
        }
        let name = if self.portal_is_open(i) {
            "portal_surface"
        } else {
            "portal_conduit"
        };
        let block = BlockState::plain(self.blocks.id(name).expect("builtin block"));
        let blocks: Vec<BlockPos> = p.frame.blocks().collect();
        for pos in blocks {
            if self.planets[self.here].edits.insert(pos, block) != Some(block) && tell {
                self.events
                    .push_back(NetEvent::Msg(ServerMsg::BlockChanged { pos, block }));
            }
        }
    }

    fn portal_at(&self, pos: BlockPos) -> Option<usize> {
        let pos = shape().wrap_block(pos);
        self.portals
            .iter()
            .position(|p| p.planet == self.here && p.frame.contains(pos))
    }

    fn push_portal(&mut self) {
        let Some(i) = self.open_portal else { return };
        let p = &self.portals[i];
        let view = PortalView {
            id: i as u32,
            corner: p.frame.corner,
            width: p.frame.width,
            height: p.frame.height,
            link: p.link.map(|l| self.portal_place(l)),
            open: self.portal_is_open(i),
            fuel: p.fuel,
            capacity: PORTAL_CAPACITY,
            trip_cost: self.trip_cost(i),
            cooldown_hours: 1.0,
        };
        self.events.push_back(NetEvent::Msg(ServerMsg::Portal(view)));
    }

    fn open_portal(&mut self, pos: Option<BlockPos>) {
        self.open_portal = None;
        let Some(pos) = pos else { return };
        match self.portal_at(pos) {
            Some(i) => {
                self.open_portal = Some(i);
                self.push_portal();
            }
            None => self.notice(bm_core::portal::SHAPE_RULE),
        }
    }

    /// The window's buttons, answered as the server would (ADR-0009).
    fn portal_action(&mut self, pos: BlockPos, action: PortalAction) {
        let Some(i) = self.portal_at(pos).filter(|i| self.open_portal == Some(*i)) else {
            return;
        };
        let shard = self.items.id("wayshard").expect("builtin item");
        let mut changed = vec![i];
        match action {
            PortalAction::MakeToken => {
                let token = self.items.id("portal_token").expect("builtin item");
                match self.inv.slots.iter().position(Option::is_none) {
                    Some(slot) => {
                        self.inv.slots[slot] = Some(ItemStack::tagged(token, i as u32 + 1));
                        let names = vec![self.token_name(i)];
                        self.events.push_back(NetEvent::Msg(ServerMsg::TokenNames { names }));
                    }
                    None => self.notice("Your inventory is full"),
                }
            }
            PortalAction::Link { slot } => {
                let stack = self.inv.get(slot as usize);
                let to = stack
                    .filter(|s| {
                        self.items
                            .get(s.item)
                            .is_some_and(|d| matches!(d.kind, bm_core::ItemKind::PortalToken))
                    })
                    .and_then(|s| (s.tag as usize).checked_sub(1))
                    .filter(|t| *t < self.portals.len());
                let (f, size) = (self.portals[i].frame, |f: PortalFrame| (f.width, f.height));
                match to {
                    None => self.notice("That token's portal is gone"),
                    Some(t) if t == i => self.notice("That token was made at this portal"),
                    Some(t) if size(self.portals[t].frame) != size(f) => {
                        self.notice("That token is for a portal of another size")
                    }
                    Some(t) => {
                        self.inv.slots[slot as usize] = None;
                        for end in [i, t] {
                            if let Some(old) = self.portals[end].link.take() {
                                self.portals[old].link = None;
                                changed.push(old);
                            }
                        }
                        self.portals[i].link = Some(t);
                        self.portals[t].link = Some(i);
                        changed.push(t);
                    }
                }
            }
            PortalAction::Unlink => {
                if let Some(other) = self.portals[i].link.take() {
                    self.portals[other].link = None;
                    changed.push(other);
                }
            }
            PortalAction::AddFuel { slot, count } => {
                if let Some(s) = self.inv.get(slot as usize).filter(|s| s.item == shard) {
                    let room = PORTAL_CAPACITY - self.portals[i].fuel;
                    let n = (count as u32).min(s.count as u32).min(room) as u16;
                    if n > 0 {
                        let left = s.count - n;
                        self.inv.slots[slot as usize] = (left > 0).then_some(ItemStack { count: left, ..s });
                        self.portals[i].fuel += n as u32;
                    }
                }
            }
            PortalAction::TakeFuel { count } => {
                let n = (count as u32).min(self.portals[i].fuel) as u16;
                let rest = self.inv.add(&self.items, shard, n);
                self.portals[i].fuel -= (n - rest) as u32;
                if rest > 0 {
                    self.notice("Your inventory is full");
                }
            }
        }
        for p in changed {
            self.refresh_portal(p, true);
        }
        self.push_portal();
    }

    /// Fills the reclaim, as a lapsed claim would, and tells the client.
    pub fn sample_reclaim(&mut self) {
        let id = |n: &str| self.items.id(n).expect("builtin item");
        let c = |name: &str| self.colours.id(name).expect("palette colour");
        self.reclaim = vec![
            ItemStack::coloured(id("stone"), c("Rich Azure"), 100),
            ItemStack::coloured(id("stone"), c("Rich Azure"), 37),
            ItemStack::coloured(id("brick"), c("Bold Crimson"), 64),
            ItemStack::new(id("timber"), 80),
            ItemStack::coloured(id("glass"), c("Light Sky"), 24),
            ItemStack::new(id("copper_ingot"), 12),
            ItemStack::coloured(id("ash"), c("Dusky Amber"), 15),
            ItemStack::new(id("coal"), 40),
            ItemStack::coloured(id("timber"), c("Deep Fern"), 30),
            ItemStack {
                wear: 40,
                ..ItemStack::new(id("copper_axe"), 1)
            },
        ];
        self.push_reclaim();
    }

    fn push_reclaim(&mut self) {
        self.events.push_back(NetEvent::Msg(ServerMsg::Reclaim {
            items: self.reclaim.clone(),
        }));
    }

    /// Moves what fits from the reclaim into the inventory, as the server
    /// does: whole stacks first, then what is left of one.
    fn collect_reclaim(&mut self) {
        let mut left = Vec::new();
        for stack in std::mem::take(&mut self.reclaim) {
            if stack.wear > 0 {
                match self.inv.slots.iter().position(Option::is_none) {
                    Some(i) => self.inv.slots[i] = Some(stack),
                    None => left.push(stack),
                }
                continue;
            }
            let rest = self
                .inv
                .add_coloured(&self.items, stack.item, stack.colour, stack.count);
            if rest > 0 {
                left.push(ItemStack { count: rest, ..stack });
            }
        }
        if !left.is_empty() {
            self.notice("Your inventory is full; the rest waits here");
        }
        self.reclaim = left;
        self.push_reclaim();
    }

    fn ground(&self, p: BlockPos) -> BlockState {
        let planet = &self.planets[self.here];
        if let Some(b) = planet.edits.get(&p) {
            return *b;
        }
        let b = |n: &str| self.blocks.id(n).unwrap_or(BlockId::AIR);
        let (top, under) = match planet.info.kind.as_str() {
            "Arid" => ("sand", "sandstone"),
            "Frozen" => ("snow", "soil"),
            _ => ("turf", "soil"),
        };
        // Gentle rolling ground away from the spawn clearing.
        let (dx, dz) = (p.x as f64 - SPAWN[0], p.z as f64 - SPAWN[2]);
        let far = match planet.flat {
            true => 0.0,
            false => ((dx * dx + dz * dz).sqrt() - 10.0).clamp(0.0, 6.0) / 6.0,
        };
        let h = GROUND + (far * ((p.x as f64 * 0.21).sin() * 1.6 + (p.z as f64 * 0.17).cos() * 1.6)).round() as i64;
        BlockState::plain(match p.y {
            0 => b("bedrock"),
            y if y < h - 3 => b("stone"),
            y if y < h => b(under),
            y if y == h => b(top),
            _ => BlockId::AIR,
        })
    }

    fn make_chunk(&self, pos: ChunkPos) -> Chunk {
        let o = pos.origin();
        let mut c = Chunk::empty();
        let n = CHUNK_SIZE as u8;
        for y in 0..n {
            for z in 0..n {
                for x in 0..n {
                    let block = self.ground(o.offset(x as i64, y as i64, z as i64));
                    if !block.id.is_air() {
                        c.set_state(LocalPos::new(x, y, z), block);
                    }
                }
            }
        }
        c
    }

    /// Everything that happened since the last call, as the network would
    /// have delivered it.
    pub fn poll(&mut self) -> Vec<NetEvent> {
        let dt = self.last.elapsed().as_secs_f64();
        self.last = Instant::now();
        self.run_stations(dt);
        self.chip_timer += dt;
        if let Some((pos, damage)) = self.chipped
            && self.chip_timer >= 1.0
        {
            self.chip_timer = 0.0;
            self.events
                .push_back(NetEvent::Msg(ServerMsg::BlockHealth { pos, damage }));
        }
        self.run_fight(dt);
        if self.release.is_some_and(|r| Instant::now() >= r) {
            self.release = None;
            let held = std::mem::take(&mut self.held);
            self.events.extend(held.into_iter().map(NetEvent::Msg));
        }
        self.events.drain(..).collect()
    }

    fn slots_since(&mut self, before: &Inventory) {
        let changes: Vec<(u8, Option<ItemStack>)> = (0..self.inv.slots.len())
            .filter(|&i| before.slots[i] != self.inv.slots[i])
            .map(|i| (i as u8, self.inv.slots[i]))
            .collect();
        if !changes.is_empty() {
            self.events.push_back(NetEvent::Msg(ServerMsg::Slots { changes }));
        }
    }

    fn notice(&mut self, text: &str) {
        self.events
            .push_back(NetEvent::Msg(ServerMsg::Notice { text: text.into() }));
    }

    fn push_station(&mut self) {
        self.push_view(self.watched);
    }

    /// Sends the machine at `pos` as it is now.
    fn push_view(&mut self, pos: Option<BlockPos>) {
        if let Some(pos) = pos
            && let Some(s) = self.stations.get(&pos)
        {
            let view = StationView {
                pos,
                station: s.station,
                jobs: s.jobs.clone(),
                output: s.output.clone(),
                idle: s.idle.clone(),
                fuel: s.fuel,
                heat: s.heat,
                wear: s.wear,
                spark: s.spark,
                forge: s.forge.as_ref().map(|f| Box::new(Self::forge_view(f))),
                power: None,
            };
            self.events.push_back(NetEvent::Msg(ServerMsg::Station(view)));
        }
    }

    fn run_stations(&mut self, dt: f64) {
        let mut changed = false;
        // Something came out: told at once, as nothing may change after.
        let mut finished = false;
        for s in self.stations.values_mut() {
            if s.idle.is_some() {
                continue;
            }
            // A sealed item in a forge's queue comes out when its time is up.
            if let Some(f) = s.forge.as_mut()
                && let Some((item, total, done)) = f.queued.as_mut()
            {
                *done += dt;
                changed = true;
                if *done >= *total {
                    s.output.push(*item);
                    f.queued = None;
                    finished = true;
                }
            }
            let Some(job) = s.jobs.first_mut() else { continue };
            let Some(r) = self.book.get(job.recipe) else { continue };
            // A furnace craft starts once it has its heat, burning fuel for
            // it one item at a time.
            if r.heat > 0 && !s.paid {
                while s.heat < r.heat as u32
                    && let Some(f) = s.fuel.as_mut()
                {
                    s.heat += self.items.get(f.item).map(|d| d.fuel as u32).unwrap_or(0);
                    f.count -= 1;
                    if f.count == 0 {
                        s.fuel = None;
                    }
                    changed = true;
                }
                if s.heat < r.heat as u32 {
                    continue;
                }
                s.heat -= r.heat as u32;
                s.paid = true;
            }
            job.progress += (dt / r.seconds.max(0.1) as f64) as f32;
            changed = true;
            if job.progress >= 1.0 {
                s.paid = false;
                job.progress = 0.0;
                job.remaining -= 1;
                let (out, n) = (r.output, r.output_count);
                match s.output.iter_mut().find(|o| o.item == out) {
                    Some(o) => o.count += n,
                    None => s.output.push(ItemStack::new(out, n)),
                }
                finished = true;
                if job.remaining == 0 {
                    s.jobs.remove(0);
                }
            }
        }
        self.push_timer += dt;
        if changed && self.push_timer > 0.5 || finished {
            self.push_timer = 0.0;
            self.push_station();
        }
    }

    pub fn send(&mut self, msg: ClientMsg) {
        let before = self.inv.clone();
        match msg {
            ClientMsg::MoveItems { from, to, count } => {
                self.inv.move_items(&self.items, from as usize, to as usize, count);
            }
            ClientMsg::Discard { slot } => {
                self.inv.clear(slot as usize);
            }
            ClientMsg::Craft {
                recipe,
                count,
                station,
                colour,
            } => self.craft(recipe, count, station, colour),
            ClientMsg::WatchStation { pos } => {
                self.watched = pos.map(|p| self.machine(p));
                self.push_station();
            }
            ClientMsg::StationFuel { pos, slot, count, into } => self.station_fuel(pos, slot, count, into),
            ClientMsg::CollectStation { pos } => {
                let pos = self.machine(pos);
                if let Some(s) = self.stations.get_mut(&pos) {
                    let mut left = Vec::new();
                    for o in s.output.drain(..) {
                        if !o.forge.is_plain() {
                            // Forged gear keeps its traits (milestone 17).
                            match self.inv.slots.iter_mut().find(|s| s.is_none()) {
                                Some(slot) => *slot = Some(o),
                                None => left.push(o),
                            }
                            continue;
                        }
                        let rest = self.inv.add(&self.items, o.item, o.count);
                        if rest > 0 {
                            left.push(ItemStack::new(o.item, rest));
                        }
                    }
                    s.output = left;
                }
                self.push_station();
            }
            ClientMsg::ClaimPlot { claim, plot } => {
                if let Some(c) = self
                    .claims
                    .iter_mut()
                    .find(|c| c.id == claim && c.owner == self.account)
                {
                    c.plots.push(plot.wrap(shape()));
                    let c = c.clone();
                    self.events.push_back(NetEvent::Msg(ServerMsg::Claim(c)));
                }
            }
            ClientMsg::ReleasePlot { plot } => {
                let stone_plot = |c: &ClaimView| PlotPos::of(c.stone, shape());
                if let Some(c) = self
                    .claims
                    .iter_mut()
                    .find(|c| c.owner == self.account && c.plots.contains(&plot))
                {
                    if stone_plot(c) == plot {
                        self.notice("That plot holds your claim stone. Break the stone to give up the claim.");
                    } else {
                        c.plots.retain(|p| *p != plot);
                        let c = c.clone();
                        self.events.push_back(NetEvent::Msg(ServerMsg::Claim(c)));
                    }
                }
            }
            ClientMsg::Edit { seq, pos, action, .. } => self.edit(seq, pos, action),
            ClientMsg::Warp { planet, .. } => {
                self.open_chest = None;
                self.open_portal = None;
                self.warp(planet);
            }
            ClientMsg::OpenStorage { pos } => self.open_storage(pos),
            ClientMsg::StorageMove { pos, from, to, count } => {
                if self.open_chest == Some(pos)
                    && let Some(chest) = self.chests.get_mut(&pos)
                    && crate::storage::apply_move(&self.items, &mut self.inv.slots, chest, from, to, count)
                {
                    self.push_storage(pos, false);
                }
            }
            ClientMsg::Chat { channel, text } => {
                let text: String = text.chars().filter(|c| !c.is_control()).take(MAX_CHAT_LEN).collect();
                let line = ChatLine {
                    channel,
                    from: self.account.clone(),
                    text,
                    time: unix_now(),
                };
                self.events
                    .push_back(NetEvent::Msg(ServerMsg::Chat { lines: vec![line] }));
            }
            ClientMsg::CollectReclaim { .. } => self.collect_reclaim(),
            ClientMsg::OpenPortal { pos } => self.open_portal(pos),
            ClientMsg::Portal { pos, action } => self.portal_action(pos, action),
            ClientMsg::Swing { seq, at, pos, slot } => self.swing(seq, at, pos, slot),
            ClientMsg::Skill { at, action } => self.skill(at, action),
            ClientMsg::Shoot {
                at,
                slot,
                yaw,
                pitch,
                charge,
            } => self.shoot(at, slot, yaw, pitch, charge),
            ClientMsg::Strike { at, creature, slot } => self.strike(at, creature, slot),
            ClientMsg::Throw {
                at,
                slot,
                yaw,
                pitch,
                hold,
            } => self.throw(at, slot, yaw, pitch, hold),
            ClientMsg::Loot { creature } => self.loot(creature),
            ClientMsg::Eat { slot, .. } => self.eat(slot),
            ClientMsg::OpenAtlas { slot: Some(slot) } => self.open_atlas(slot),
            ClientMsg::AtlasLoad { slot, from } => self.atlas_load(slot, from),
            ClientMsg::LookCrop { pos } => self.look_crop(pos),
            ClientMsg::ForgeBegin { pos, gear, deck } => self.forge_begin(pos, gear, deck),
            ClientMsg::ForgeUse { pos, slot, round } => self.forge_use(pos, slot, round),
            ClientMsg::Return => self.come_back(),
            ClientMsg::Hello { .. } => {}
            // The old hold-to-break message, until the protocol drops it.
            #[allow(unreachable_patterns)]
            _ => {}
        }
        self.slots_since(&before);
    }

    fn open_storage(&mut self, pos: Option<BlockPos>) {
        // As the server does: the block open before is closed first, and a
        // close is confirmed.
        if let Some(old) = self.open_chest.take() {
            self.push_storage(old, true);
        }
        let Some(pos) = pos else { return };
        if self.here == 0 && self.chests.contains_key(&pos) {
            self.open_chest = Some(pos);
            self.push_storage(pos, false);
        } else {
            self.push_storage(pos, true);
        }
    }

    fn push_storage(&mut self, pos: BlockPos, closed: bool) {
        let slots = match closed {
            false => self.chests.get(&pos).cloned().unwrap_or_default(),
            true => Vec::new(),
        };
        self.events
            .push_back(NetEvent::Msg(ServerMsg::Storage(StorageView { pos, slots, closed })));
    }

    /// A little of each channel, as a server would send on joining and
    /// after: the universe history first, then lines as they are said.
    pub fn sample_chat(&mut self) {
        let line = |channel: Channel, from: &str, text: &str| ChatLine {
            channel,
            from: from.into(),
            text: text.into(),
            time: unix_now(),
        };
        let me = self.account.clone();
        let lines = vec![
            line(Channel::Universe, "Ash", "Anyone found copper on Saffra yet?"),
            line(
                Channel::Universe,
                "Marrow",
                "Plenty in the river gravels by the big mesa.",
            ),
            line(Channel::Universe, "", "Wren arrived on Hearthmoor"),
            line(Channel::Local, "Wren", "Is that your workbench? Nice spot."),
            line(Channel::Planet, "Tamsin", "Selling glass, forty for a stack of timber."),
            line(
                Channel::Whisper(me.clone()),
                "Wren",
                "Want to dig the hill behind your claim together?",
            ),
            line(
                Channel::Whisper("Wren".into()),
                &me,
                "Sure, give me a minute to empty my pack.",
            ),
            line(Channel::Local, &me, "Back in a moment."),
            line(Channel::Universe, "", "Ash left"),
        ];
        self.events.push_back(NetEvent::Msg(ServerMsg::Chat { lines }));
    }

    /// The machine a block belongs to (its own position if none).
    fn machine(&self, pos: BlockPos) -> BlockPos {
        self.machine_of.get(&pos).copied().unwrap_or(pos)
    }

    /// A skill change after input `at` (taken as the tick: the demo runs
    /// one input a tick from 0), ruled as the server rules it; switching
    /// pages is never "at home" here.
    fn skill(&mut self, at: u32, action: bm_protocol::SkillAction) {
        match crate::skills::apply(&mut self.character, &self.tree, &action, at as u64, false) {
            Ok(()) => self.events.push_back(NetEvent::Msg(ServerMsg::Character {
                character: self.character.clone(),
                at,
            })),
            Err(why) => self.notice(&why.to_string()),
        }
    }

    /// `--demo-ui skills`: a level 24 character with marks, two pages and
    /// points spent on both, the first in use.
    pub fn show_skills(&mut self) {
        let mut c = Character {
            xp: bm_core::skills::xp_for_level(24) + 17_250,
            // Marks earned to 24, less a page bought.
            marks: (2..=24).map(bm_core::skills::marks_for_level).sum::<u32>() - bm_core::skills::PAGE_PRICE,
            ..Default::default()
        };
        c.pages.push(Default::default());
        let first = [
            ("power", 8),
            ("dexterity", 4),
            ("control", 2),
            ("hammer_mastery", 3),
            ("tool_crafting", 1),
            ("hammer_epic", 1),
            ("light_epic", 1),
        ];
        for (id, n) in first {
            for _ in 0..n {
                c.learn(&self.tree, id).expect("the demo page fits");
            }
        }
        c.active = 1;
        for (id, n) in [("zeal", 4), ("luck", 3), ("shovel_mastery", 2)] {
            for _ in 0..n {
                c.learn(&self.tree, id).expect("the demo page fits");
            }
        }
        c.active = 0;
        c.cleanses_used = 3;
        self.character = c;
        self.events.push_back(NetEvent::Msg(ServerMsg::Character {
            character: self.character.clone(),
            at: 0,
        }));
    }

    fn craft(&mut self, recipe: u16, count: u16, station: Option<BlockPos>, colour: Option<bm_core::Colour>) {
        let Some(r) = self.book.get(recipe).cloned() else {
            return;
        };
        if let Some(why) = crate::skills::recipe_gate(&self.tree, &self.character.effects(&self.tree), &r) {
            self.notice(&why);
            return;
        }
        let at = station.map(|p| self.machine(p));
        if r.station != Station::Hand {
            let Some(s) = at.and_then(|p| self.stations.get(&p)) else {
                self.notice("That is made at a machine");
                return;
            };
            if let Some(why) = s.idle.clone() {
                self.notice(&why);
                return;
            }
        }
        let mut made = 0;
        for _ in 0..count {
            let mut trial = self.inv.clone();
            // A tinted recipe takes its input in the chosen colour and makes
            // its output in it (milestone 22).
            let colour = colour.filter(|_| r.tint.is_some());
            let Some(plan) = r.plan_in(&trial, 1, colour) else {
                break;
            };
            let tinted = |i: bm_core::ItemId| r.input_of(i).map(|k| k as u8) == r.tint;
            if plan.iter().flatten().any(|(i, n)| match colour {
                Some(c) if tinted(*i) => trial.remove_coloured(*i, c, *n).is_none(),
                _ => trial.remove(*i, *n).is_none(),
            }) {
                break;
            }
            let made_in = colour.unwrap_or(bm_core::Colour::NATURAL);
            if r.station == Station::Hand
                && trial
                    .add_all_coloured(&self.items, r.output, made_in, r.output_count)
                    .is_none()
            {
                self.notice("Your inventory is full");
                break;
            }
            self.inv = trial;
            made += 1;
        }
        if made == 0 {
            self.notice("You do not have what that needs");
            return;
        }
        if r.station != Station::Hand
            && let Some(s) = at.and_then(|p| self.stations.get_mut(&p))
        {
            s.jobs.push(JobView {
                recipe,
                remaining: made,
                progress: 0.0,
            });
            self.push_station();
        }
    }

    /// Moves fuel between inventory `slot` and a furnace, as the server's
    /// `StationFuel` does: only fuel goes in, merging with what is there.
    fn station_fuel(&mut self, pos: BlockPos, slot: u8, count: u16, into: bool) {
        let m = self.machine(pos);
        let slot = slot as usize;
        let items = &self.items;
        let Some(st) = self
            .stations
            .get_mut(&m)
            .filter(|s| matches!(s.station, Station::Furnace | Station::SparkGenerator))
        else {
            return;
        };
        if slot >= self.inv.slots.len() || count == 0 {
            return;
        }
        let limit = |item| items.get(item).map(|d| d.stack).unwrap_or(1);
        if into {
            let Some(from) = self.inv.slots[slot] else { return };
            if items.get(from.item).is_none_or(|d| d.fuel == 0) || st.fuel.is_some_and(|f| f.item != from.item) {
                return;
            }
            let held = st.fuel.map(|f| f.count).unwrap_or(0);
            let n = count.min(from.count).min(limit(from.item).saturating_sub(held));
            if n == 0 {
                return;
            }
            st.fuel = Some(ItemStack::new(from.item, held + n));
            self.inv.slots[slot] = (from.count > n).then_some(ItemStack {
                count: from.count - n,
                ..from
            });
        } else {
            let Some(f) = st.fuel else { return };
            let have = self.inv.slots[slot];
            if have.is_some_and(|h| h.item != f.item || h.colour != f.colour) {
                return;
            }
            let held = have.map(|h| h.count).unwrap_or(0);
            let n = count.min(f.count).min(limit(f.item).saturating_sub(held));
            if n == 0 {
                return;
            }
            self.inv.slots[slot] = Some(ItemStack::new(f.item, held + n));
            st.fuel = (f.count > n).then_some(ItemStack::new(f.item, f.count - n));
        }
        // As the server answers: the machine, watched or not.
        self.push_view(Some(m));
    }

    /// Turns the first planet into a showroom of gathering objects, for a
    /// daylight screenshot: rows of every plant on its ground (spikeleaf
    /// and ridge cactus on sand, frostbell on snow, nightlily by water), rows
    /// of every boulder, and a stone cave mouth with every fungus inside.
    pub fn show_flora(&mut self) {
        let b = |n: &str| BlockState::plain(self.blocks.id(n).expect("builtin block"));
        let (stone, sand, snow, water, turf) = (b("stone"), b("sand"), b("snow"), b("water"), b("turf"));
        let mut put: Vec<(BlockPos, BlockState)> = Vec::new();
        let g = GROUND;
        // Two rows of four plants, two columns of each, the near row on
        // its own grounds.
        let plants = [
            ("spikeleaf", Some(sand), 124, 126),
            ("ridge_cactus", Some(sand), 126, 126),
            ("nightlily", None, 128, 126),
            ("frostbell", Some(snow), 130, 126),
            ("hornroot", None, 124, 124),
            ("stiltfrond", None, 126, 124),
            ("knot_aloe", None, 128, 124),
            ("curlfern", None, 130, 124),
        ];
        for (name, ground, x0, z0) in plants {
            for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (x, z) = (x0 + dx, z0 + dz);
                if let Some(gr) = ground {
                    put.push((BlockPos::new(x, g, z), gr));
                }
                put.push((BlockPos::new(x, g + 1, z), b(name)));
            }
            if name == "nightlily" {
                for dx in 0..2 {
                    put.push((BlockPos::new(x0 + dx, g, z0 + 2), water));
                }
            }
        }
        let boulders = [
            "boulder",
            "stalk_boulder",
            "stacked_boulder",
            "ring_boulder",
            "chip_boulder",
            "tapered_boulder",
        ];
        for (k, name) in boulders.iter().enumerate() {
            let x = 125 + k as i64;
            for z in [121, 122] {
                put.push((BlockPos::new(x, g, z), stone));
                put.push((BlockPos::new(x, g + 1, z), b(name)));
            }
        }
        // The cave: a stone hill with a mouth facing the rows, its floor
        // stone, its roof two layers thick under turf.
        for x in 121..=135 {
            for z in 113..=120 {
                let inside = (123..=133).contains(&x) && (116..=119).contains(&z);
                let mouth = (124..=132).contains(&x) && z == 120;
                for y in g..=g + 5 {
                    let block = if y == g {
                        stone
                    } else if (inside || mouth) && y <= g + 3 {
                        BlockState::AIR
                    } else if y == g + 5 {
                        turf
                    } else {
                        stone
                    };
                    put.push((BlockPos::new(x, y, z), block));
                }
            }
        }
        let fungi = [
            "tarspot_cap",
            "tongue_bracket",
            "branch_coral",
            "burst_cap",
            "weeping_cap",
            "glowcap",
        ];
        for (k, name) in fungi.iter().enumerate() {
            for z in 117..=119 {
                put.push((BlockPos::new(125 + k as i64, g + 1, z), b(name)));
            }
        }
        // The post the player stands on.
        for y in g + 1..=g + 2 {
            put.push((BlockPos::new(128, y, 130), stone));
        }
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// A meteor mid-fight (milestone 14), north of the spawn: its crater,
    /// shell and heart, three creature portals over it, a wave of angry
    /// creatures, the meteor bar and a meteor life.
    pub fn show_meteor(&mut self) {
        use bm_protocol::{MeteorPhase, MeteorView, Pose};
        let b = |n: &str| BlockState::plain(self.blocks.id(n).expect("builtin block"));
        let (shell, heart) = (b("meteor_shell"), b("meteor_heart"));
        let g = GROUND;
        let site = BlockPos::new(METEOR_SITE[0], g, METEOR_SITE[1]);
        let mut put: Vec<(BlockPos, BlockState)> = Vec::new();
        for dx in -4i64..=4 {
            for dz in -4i64..=4 {
                for dy in -1i64..=4 {
                    let d = ((dx * dx + dz * dz) as f64).sqrt() + (dy.max(0) as f64) * 0.3;
                    if d > 4.0 || (dy < 0 && d > 2.5) {
                        continue;
                    }
                    put.push((site.offset(dx, dy + 1, dz), BlockState::AIR));
                }
            }
        }
        for dx in -2i64..=2 {
            for dy in -2i64..=2 {
                for dz in -2i64..=2 {
                    if dx * dx + dy * dy + dz * dz <= 5 && (dx, dy, dz) != (0, 0, 0) {
                        put.push((site.offset(dx, dy + 1, dz), shell));
                    }
                }
            }
        }
        put.push((site.offset(0, 1, 0), heart));
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
        let centre = |dx: f64, dz: f64, dy: f64| {
            [
                (site.x as f64 + 0.5 + dx) as f32,
                (g as f64 + dy) as f32,
                (site.z as f64 + 0.5 + dz) as f32,
            ]
        };
        self.events.push_back(NetEvent::Msg(ServerMsg::Meteor(MeteorView {
            id: 1,
            from: centre(-40.0, -40.0, 220.0),
            site: site.offset(0, 1, 0),
            level: 2,
            phase: MeteorPhase::Fighting {
                wave: 2,
                waves: 4,
                left: 3,
                lives: 1,
            },
            portals: [centre(-7.0, 0.0, 12.0), centre(4.0, -6.0, 12.0), centre(4.0, 6.0, 12.0)],
            beam_until: u64::MAX / 2,
            looted: false,
        })));
        self.events
            .push_back(NetEvent::Msg(ServerMsg::MeteorLife { life: Some((1, None)) }));
        let k = self.kinds();
        let kind = |n: &str| k.id(n).expect("builtin creature");
        let at = |x: f64, z: f64| DVec3::new(x, (GROUND + 1) as f64, z);
        let deg = |d: f64| ((d / 360.0).rem_euclid(1.0) * 65536.0) as u16;
        let creature = |id, kind, tier, pos, yaw, pose| DemoCreature {
            id,
            kind,
            tier,
            pos,
            yaw,
            pose,
            damage: 0,
            angry: true,
            dead: false,
            looted: false,
            pace: None,
            beaten: false,
            exotic: 0,
        };
        let (sx, sz) = (site.x as f64 + 0.5, site.z as f64 + 0.5);
        self.fight.creatures = vec![
            creature(1, kind("grazer"), 2, at(sx - 5.0, sz + 5.5), deg(160.0), Pose::Walk),
            creature(2, kind("pelter"), 2, at(sx + 5.5, sz + 4.0), deg(200.0), Pose::Windup),
            DemoCreature {
                damage: 900,
                ..creature(3, kind("popper"), 2, at(sx - 1.0, sz + 6.5), deg(180.0), Pose::Lit)
            },
        ];
        for i in 0..self.fight.creatures.len() {
            self.push_creature(i);
        }
    }

    /// The six textures of the 128 trial (`docs/proposals/textures-128.md`)
    /// in one view: turf, a patch of bare soil, a stone wall with copper
    /// ore set in it, and a tree with a crown of foliage.
    pub fn show_textures(&mut self) {
        let b = |n: &str| BlockState::plain(self.blocks.id(n).expect("builtin block"));
        let (stone, soil, copper, trunk, leaves) = (b("stone"), b("soil"), b("copper_vein"), b("trunk"), b("foliage"));
        let mut put: Vec<(BlockPos, BlockState)> = Vec::new();
        let g = GROUND;
        // Bare soil in front, a little to the right.
        for x in 128..=130 {
            for z in 126..=127 {
                put.push((BlockPos::new(x, g, z), soil));
            }
        }
        // The wall: stone, four high, a column of copper and a few lone
        // pockets; a step at its left end shows a stone top.
        for x in 127..=133 {
            for y in g + 1..=g + 4 {
                let ore = (x == 130 && y <= g + 3)
                    || (x, y) == (131, g + 1)
                    || (x, y) == (132, g + 4)
                    || (x, y) == (128, g + 2);
                put.push((BlockPos::new(x, y, 121), if ore { copper } else { stone }));
            }
        }
        for x in 127..=128 {
            put.push((BlockPos::new(x, g + 1, 122), stone));
        }
        put.push((BlockPos::new(131, g + 1, 122), copper));
        // A raised step of turf, for its side.
        let turf = b("turf");
        for x in 132..=133 {
            for z in 125..=126 {
                put.push((BlockPos::new(x, g + 1, z), turf));
            }
        }
        // The tree: a trunk five high, a crown of foliage around its top.
        let (tx, tz) = (123, 119);
        for y in g + 1..=g + 5 {
            put.push((BlockPos::new(tx, y, tz), trunk));
        }
        for y in g + 4..=g + 7 {
            let r: i64 = if y >= g + 6 { 1 } else { 2 };
            for x in tx - r..=tx + r {
                for z in tz - r..=tz + r {
                    let corner = (x - tx).abs() == r && (z - tz).abs() == r && r > 1;
                    let core = x == tx && z == tz && y <= g + 5;
                    if !corner && !core {
                        put.push((BlockPos::new(x, y, z), leaves));
                    }
                }
            }
        }
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// `--demo-ui forge`: a forge of four blocks ahead of the spawn with a
    /// session under way on an iron hammer (a few rounds played: traits,
    /// buffs, a deck), answering `ForgeBegin` and `ForgeUse` by running the
    /// real `bm_core::forge::Session` (milestone 17). `ForgeShow::Queue`
    /// instead shows a sealed hammer in the queue and a finished one
    /// waiting; `ForgeShow::Empty` an idle forge, to start a session. A
    /// forged hammer and some ingredients go in the pack either way.
    pub fn show_forge(&mut self, show: ForgeShow) {
        let queue = show == ForgeShow::Queue;
        use bm_core::forge::{Forged, Forger, Gear};
        let b = |n: &str| BlockState::plain(self.blocks.id(n).expect("builtin block"));
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        for p in FORGE {
            planet.edits.insert(bp(p), b("forge"));
        }
        let id = |n: &str| self.items.id(n).expect("builtin item");
        let hammer = ItemStack::new(id("iron_hammer"), 1);
        let forged = ItemStack {
            forge: Forged::from_suffix("mighty5,quick3,wide_swing2,night_owl1,unlucky1"),
            ..hammer
        };
        let mut forge = DemoForge::default();
        if queue {
            let mut sealed = hammer;
            sealed.forge = Forged::from_suffix("mighty2,sturdy3,glowing1");
            forge.queued = Some((sealed, 240.0, 96.0));
        } else if show == ForgeShow::Session {
            let deck = [
                ("pure_tincture_1", 30),
                ("rough_tincture_1", 20),
                ("wild_tincture_1", 12),
                ("force_glaze", 4),
                ("boost_salve_1", 3),
                ("sealing_wax", 2),
            ];
            let deck = deck
                .iter()
                .map(|&(n, c)| bm_core::forge::DeckSlot {
                    stack: ItemStack::new(id(n), c),
                    ingredient: bm_core::forge::ingredient_by_name(n).expect("an ingredient"),
                })
                .collect();
            // A practised forger: Vitality 4, Power 3, Insight 5.
            let forger = Forger {
                vitality: 4.0,
                power: 3.0,
                insight: 5.0,
                epic: false,
            };
            let flex = self.items.get(hammer.item).map_or(0, |d| d.flex);
            let mut s = bm_core::forge::Session::new(hammer, Gear::Tool, forger.stats(flex, 0.0), deck, 17);
            for slot in [3, 3, 0, 1, 2, 0, 4, 1] {
                let _ = s.use_ingredient(slot);
            }
            forge.session = Some(s);
        }
        let first = bp(FORGE[0]);
        for p in FORGE {
            self.machine_of.insert(bp(p), first);
        }
        let mut station = DemoStation {
            forge: Some(forge),
            wear: Some((4_120, bm_core::spark::MACHINE_WEAR)),
            ..DemoStation::new(Station::Forge)
        };
        if queue {
            station.output.push(forged);
        }
        self.stations.insert(first, station);
        let before = self.inv.clone();
        self.inv.slots[1] = Some(forged);
        self.inv.slots[8] = Some(ItemStack::new(id("copper_grapple"), 1));
        for (n, c) in [
            ("pure_tincture_1", 24),
            ("rough_tincture_1", 8),
            ("force_glaze", 2),
            ("sealing_wax", 3),
            ("unmaking_wax_1", 1),
        ] {
            self.inv.add(&self.items, id(n), c);
        }
        self.slots_since(&before);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// The forge's view for its window.
    fn forge_view(f: &DemoForge) -> bm_protocol::ForgeView {
        bm_protocol::ForgeView {
            session: f.session.as_ref().map(bm_protocol::SessionView::of),
            forger: None,
            queued: f.queued.map(|(item, total, done)| {
                (
                    item,
                    (done / total).clamp(0.0, 1.0) as f32,
                    (total - done).max(0.0).ceil() as u32,
                )
            }),
        }
    }

    /// Puts a stack in the pack: onto a matching stack, else the first
    /// empty slot. What does not fit is lost (the demo keeps no reclaim).
    fn give(&mut self, stack: ItemStack) {
        if stack.forge.is_plain() && stack.tag == 0 {
            self.inv
                .add_coloured(&self.items, stack.item, stack.colour, stack.count);
        } else if let Some(slot) = self.inv.slots.iter_mut().find(|s| s.is_none()) {
            *slot = Some(stack);
        }
    }

    /// `ForgeBegin`, as the server rules it: unforged gear the forge takes,
    /// a deck of ingredient stacks (at most the forger's slots, 100 each),
    /// moved out of the pack; the session's stats from the character.
    fn forge_begin(&mut self, pos: BlockPos, gear: u8, deck: Vec<(u8, u16)>) {
        use bm_core::forge::{DECK_STACK, DeckSlot, Gear, Session};
        let pos = self.machine(pos);
        let forger = self.character.effects(&self.tree).forger();
        let Some(st) = self.stations.get(&pos).filter(|s| s.station == Station::Forge) else {
            return;
        };
        if st
            .forge
            .as_ref()
            .is_some_and(|f| f.session.is_some() || f.queued.is_some())
            || !st.output.is_empty()
        {
            return self.notice("The forge is busy");
        }
        let Some(item) = self.inv.get(gear as usize) else {
            return;
        };
        let Some(def) = self.items.get(item.item) else { return };
        let (Some(kind), false) = (Gear::of(def), item.forge.done) else {
            return self.notice("That cannot be forged");
        };
        let flex = def.flex;
        if deck.is_empty() || deck.len() > forger.deck_slots() {
            return self.notice("A deck needs 1 to 6 ingredient stacks");
        }
        let mut slots = Vec::new();
        for &(slot, count) in &deck {
            let Some(s) = self.inv.get(slot as usize) else { return };
            let Some(&bm_core::ItemKind::Ingredient(i)) = self.items.get(s.item).map(|d| &d.kind) else {
                return self.notice("Only forging ingredients go in the deck");
            };
            if slot == gear || count == 0 || count > s.count.min(DECK_STACK) || slots.iter().any(|(o, _)| *o == slot) {
                return;
            }
            slots.push((
                slot,
                DeckSlot {
                    stack: ItemStack { count, ..s },
                    ingredient: i,
                },
            ));
        }
        self.inv.clear(gear as usize);
        for (slot, d) in &slots {
            if let Some(s) = self.inv.slots[*slot as usize].as_mut() {
                s.count -= d.stack.count;
                if s.count == 0 {
                    self.inv.slots[*slot as usize] = None;
                }
            }
        }
        let seed = (self.fight.rand() * u32::MAX as f64) as u64;
        let session = Session::new(
            item,
            kind,
            forger.stats(flex, 0.0),
            slots.into_iter().map(|(_, d)| d).collect(),
            seed,
        );
        if let Some(st) = self.stations.get_mut(&pos) {
            st.forge.get_or_insert_with(DemoForge::default).session = Some(session);
        }
        self.push_view(Some(pos));
    }

    /// The forge at `pos`'s state, if it is a forge.
    fn forge_mut(&mut self, pos: BlockPos) -> Option<&mut DemoForge> {
        self.stations.get_mut(&pos).and_then(|s| s.forge.as_mut())
    }

    /// `ForgeUse`: one round of the session, by the real rules; a stale
    /// round number (a double click) is ignored.
    fn forge_use(&mut self, pos: BlockPos, slot: u8, round: u32) {
        use bm_core::forge::{Ended, Refused, queue_band};
        use bm_protocol::ForgeOutcome;
        let pos = self.machine(pos);
        let Some(mut s) = self.forge_mut(pos).and_then(|f| f.session.take()) else {
            return;
        };
        let result = if s.rounds == round {
            s.use_ingredient(slot as usize)
        } else {
            Err(Refused::Empty)
        };
        let ended = match result {
            Ok(Some(e)) => e,
            other => {
                if let Some(f) = self.forge_mut(pos) {
                    f.session = Some(s);
                }
                match other {
                    Err(Refused::NoVigour) => self.notice("Not enough vigour for that"),
                    Ok(None) => self.push_view(Some(pos)),
                    _ => {}
                }
                return;
            }
        };
        let rank_base = self.items.get(s.item.item).map_or(0, |d| d.rank);
        let outcome = match ended {
            Ended::Sealed(forged) => {
                let item = ItemStack {
                    forge: forged,
                    ..s.item
                };
                let band = queue_band(forged.rank(rank_base));
                if let Some(f) = self.forge_mut(pos) {
                    f.queued = Some((item, band.seconds as f64, 0.0));
                }
                ForgeOutcome::Sealed(item)
            }
            Ended::Unmade(back) => {
                let returned: Vec<ItemStack> = back.into_iter().map(|(k, n)| ItemStack { count: n, ..k }).collect();
                self.give(s.item);
                for r in &returned {
                    self.give(*r);
                }
                ForgeOutcome::Unmade { gear: s.item, returned }
            }
            Ended::Destroyed => ForgeOutcome::Destroyed(s.item),
        };
        for left in s.leftovers() {
            self.give(left);
        }
        self.events.push_back(NetEvent::Msg(ServerMsg::Forged { pos, outcome }));
        self.push_view(Some(pos));
    }

    /// Spark (milestone 15): generators, links and a working mixer in a
    /// row, more machines and the new ores behind (`SPARK_*`, `MIXER`), and
    /// a spanner and the mixer's inputs in the pack.
    pub fn show_spark(&mut self) {
        let b = |n: &str| BlockState::plain(self.blocks.id(n).expect("builtin block"));
        let mut put: Vec<(BlockPos, BlockState)> = Vec::new();
        for p in SPARK_GENERATORS {
            put.push((bp(p), b("spark_generator")));
        }
        for p in SPARK_LINKS {
            put.push((bp(p), b("spark_link")));
        }
        for p in MIXER {
            put.push((bp(p), b("mixer")));
        }
        // The back row, two high.
        let g = GROUND;
        for (x, lower, upper) in [
            (120, "refinery", "refinery"),
            (121, "refinery", "refinery"),
            (123, "compactor", "compactor"),
            (124, "compactor", "compactor"),
            (132, "iron_furnace", "iron_furnace_crucible"),
            (133, "silver_seam", "relic_cache"),
            (130, "spark_link", "spark_link"),
            (131, "spark_generator", "furnace_crucible"),
        ] {
            put.push((BlockPos::new(x, g + 1, 115), b(lower)));
            put.push((BlockPos::new(x, g + 2, 115), b(upper)));
        }
        let planet = &mut self.planets[0];
        planet.flat = true;
        planet.edits.clear();
        planet.edits.extend(put);
        let id = |n: &str| self.items.id(n).expect("builtin item");
        let fuel = Some(ItemStack::new(id("coal"), 12));
        let alloy = recipe_index(&self.book, &self.items, "silver_alloy");
        let grid = SparkView {
            stored: 6_400,
            capacity: bm_core::spark::capacity(SPARK_GENERATORS.len()),
            linked: true,
            burning: 60,
        };
        let generator = DemoStation {
            fuel,
            spark: Some(grid),
            ..DemoStation::new(Station::SparkGenerator)
        };
        let mixer = DemoStation {
            jobs: vec![JobView {
                recipe: alloy,
                remaining: 4,
                progress: 0.4,
            }],
            output: vec![ItemStack::new(id("silver_alloy"), 2)],
            wear: Some((8_450, bm_core::spark::MACHINE_WEAR)),
            spark: Some(SparkView { burning: 0, ..grid }),
            ..DemoStation::new(Station::Mixer)
        };
        for (blocks, st) in [(&SPARK_GENERATORS[..], generator), (&MIXER[..], mixer)] {
            for p in blocks {
                self.machine_of.insert(bp(*p), bp(blocks[0]));
            }
            self.stations.insert(bp(blocks[0]), st);
        }
        let before = self.inv.clone();
        self.inv.slots[9] = Some(ItemStack::new(id("iron_spanner"), 1));
        for (n, c) in [("silver_ingot", 6), ("iron_ingot", 6), ("coal", 20)] {
            self.inv.add(&self.items, id(n), c);
        }
        self.slots_since(&before);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
    }

    /// One use of the tool or chisel in `slot`; it breaks at its durability.
    fn wear(&mut self, slot: usize) {
        let before = self.inv.clone();
        if let Some(s) = self.inv.slots[slot].as_mut() {
            s.wear += 1;
            if let Some(def) = self.items.get(s.item)
                && def.durability_of(s).is_some_and(|d| s.wear >= d)
            {
                let text = format!("Your {} broke", def.display);
                self.inv.slots[slot] = None;
                self.notice(&text);
            }
        }
        self.slots_since(&before);
    }

    /// One swing, by the server's rules (`bm_core::mining`): the cadence on
    /// the input timeline, damage by tier with armour and the right tool,
    /// a critical roll per block, healing after `HEAL_TICKS` alone, breaks
    /// answered with `EditResult`, damage with `BlockHealth`. The demo has
    /// no look direction (it gets no inputs): an area tool's shape faces
    /// east. Unlike the server it checks no reach.
    fn swing(&mut self, seq: u32, at: u32, pos: BlockPos, slot: u8) {
        use crate::mining::skilled_tool;
        use bm_core::mining::{HEAL_TICKS, aoe_blocks, block_health, is_crit, swing_ticks};
        let hit = shape().wrap_block(pos);
        let heal = HEAL_TICKS as u32;
        self.damage.retain(|_, (_, last)| at < last.saturating_add(heal));
        let health_of = |d: &Self, p: BlockPos| d.damage.get(&p).map_or(0, |e| e.0);
        if let Some((last_at, ticks)) = self.last_swing
            && at < last_at.saturating_add(ticks)
        {
            // Too soon: refused, and the block's truth sent back.
            let damage = health_of(self, hit);
            self.events
                .push_back(NetEvent::Msg(ServerMsg::BlockHealth { pos: hit, damage }));
            return;
        }
        // A bare hand breaks nothing (Nathan, 2026-10-20). The tool through
        // its traits; the demo knows no time, health or hunger for quirks.
        let held = self.inv.get(slot as usize);
        let Some(tool) = held.and_then(|s| self.items.get(s.item)?.tool_of(&s, &Held::default())) else {
            let damage = health_of(self, hit);
            self.events
                .push_back(NetEvent::Msg(ServerMsg::BlockHealth { pos: hit, damage }));
            return;
        };
        let effects = self.character.effects(&self.tree);
        let tool = skilled_tool(&tool, &effects);
        self.last_swing = Some((at, swing_ticks(tool.action_speed)));
        // A spanner repairs a machine's wear and breaks nothing (milestone 15).
        if tool.class == Some(bm_core::items::ToolClass::Spanner) {
            let m = self.machine(hit);
            if let Some(s) = self.stations.get_mut(&m)
                && let Some((left, max)) = s.wear.as_mut()
            {
                *left = (*left + tool.repair).min(*max);
                let (wear, max) = (*left, *max);
                self.events
                    .push_back(NetEvent::Msg(ServerMsg::MachineWear { pos: m, wear, max }));
                self.push_view(Some(m));
            }
            return;
        }
        let tier = self.planets[self.here].info.tier;
        let area = tool.aoe.map_or(1.0, bm_core::Aoe::multiplier);
        let mut done = Vec::new();
        for p in aoe_blocks(tool.aoe, hit, DVec3::X, 1, at) {
            if !shape().contains_y(p.y) {
                continue;
            }
            let p = shape().wrap_block(p);
            if done.contains(&p) {
                continue;
            }
            done.push(p);
            let state = self.ground(p);
            let def = self.blocks.get(state.id);
            if state.id.is_air() || !def.breakable {
                continue;
            }
            let crit = is_crit(tool.crit_chance, 1, at, p);
            let dealt = bm_core::mining::swing_damage(&tool, def, tier, crit, area);
            if dealt == 0 {
                continue;
            }
            let total = health_of(self, p) + dealt;
            if total < block_health(def, tier) {
                self.damage.insert(p, (total, at));
                self.events
                    .push_back(NetEvent::Msg(ServerMsg::BlockHealth { pos: p, damage: total }));
                continue;
            }
            self.break_block(seq, p, state, at);
        }
        if held.is_some_and(|s| self.items.get(s.item).is_some_and(|d| d.tool().is_some())) {
            self.wear(slot as usize);
        }
    }

    /// Breaks `state` at `pos` for our swing `seq`: its drop goes into the
    /// inventory, refused if there is no room or it is a chest with things
    /// in it.
    fn break_block(&mut self, seq: u32, pos: BlockPos, state: BlockState, at: u32) {
        let old = state.id;
        let full_chest = self.here == 0 && self.chests.get(&pos).is_some_and(|c| c.iter().any(Option::is_some));
        if full_chest {
            let name = self.blocks.get(old).display_name();
            self.notice(&format!("Empty the {name} before breaking it"));
            return self.answer_edit(seq, pos, state, false);
        }
        // A block that yields itself keeps its colour (ADR-0007).
        let drop = self.items.drop_of(old).map(|item| {
            let keeps = self.items.get(item).and_then(|d| d.block()) == Some(old);
            (item, if keeps { state.colour } else { Colour::NATURAL })
        });
        if let Some((item, colour)) = drop
            && self.inv.add_all_coloured(&self.items, item, colour, 1).is_none()
        {
            // As the server: one point short of breaking, and the truth.
            self.notice("Your inventory is full");
            self.answer_edit(seq, pos, state, false);
            let damage = bm_core::mining::block_health(self.blocks.get(old), self.planets[self.here].info.tier) - 1;
            self.damage.insert(pos, (damage, at));
            self.events
                .push_back(NetEvent::Msg(ServerMsg::BlockHealth { pos, damage }));
            return;
        }
        self.answer_edit(seq, pos, BlockState::AIR, true);
    }

    /// Tests: puts `stack` in pack slot `slot`, telling the client.
    #[cfg(test)]
    pub fn put_slot(&mut self, slot: usize, stack: ItemStack) {
        let before = self.inv.clone();
        self.inv.slots[slot] = Some(stack);
        self.slots_since(&before);
    }

    /// Takes the player to the third tier world, facing a stone block on
    /// the ground two blocks ahead that someone has half broken, a stone
    /// hammer in hand (`--demo-ui mining`).
    pub fn show_mining(&mut self) {
        let lush = self
            .planets
            .iter()
            .position(|p| p.info.tier == 3)
            .expect("a tier 3 demo planet");
        let pos = MINED;
        let stone = BlockState::plain(self.blocks.id("stone").expect("builtin block"));
        self.planets[lush].edits.insert(pos, stone);
        let health = bm_core::mining::block_health(self.blocks.get(stone.id), 3);
        self.chipped = Some((pos, health * 3 / 5));
        self.chip_timer = 1.0;
        self.hold = Some(Duration::ZERO);
        let id = self.planets[lush].info.id;
        self.warp(id);
    }

    fn edit(&mut self, seq: u32, pos: BlockPos, action: EditAction) {
        let pos = shape().wrap_block(pos);
        let old_state = self.ground(pos);
        let old = old_state.id;
        let (block, accepted) = match action {
            EditAction::Place { slot, mount } => {
                let stack = self.inv.get(slot as usize);
                let own = stack.and_then(|s| self.items.get(s.item)).and_then(|d| d.block());
                // A prop goes on the face named, which must be solid.
                let block = match (stack, mount) {
                    (Some(s), Some(m)) => self.items.prop_block(&self.blocks, s.item, m).filter(|_| {
                        let (dx, dy, dz) = m.support();
                        let support = self.ground(shape().wrap_block(pos.offset(dx, dy, dz)));
                        self.blocks.get(support.id).solid
                    }),
                    (Some(s), None)
                        if self
                            .items
                            .prop_block(&self.blocks, s.item, bm_core::Mount::Floor)
                            .is_some() =>
                    {
                        None
                    }
                    _ => own,
                };
                match (block, stack) {
                    (Some(b), Some(s)) if old.is_air() => {
                        self.inv.take_one(slot as usize);
                        let colour = if self.blocks.get(b).colourable {
                            s.colour
                        } else {
                            Colour::NATURAL
                        };
                        (BlockState::coloured(b, colour), true)
                    }
                    _ => (old_state, false),
                }
            }
            EditAction::Chisel { slot, octants, restore } => {
                let def = self.inv.get(slot as usize).and_then(|s| self.items.get(s.item));
                let advanced = def.is_some_and(|d| d.chisel_advanced());
                let chisel = def.and_then(|d| d.chisel());
                match chisel {
                    Some((family, precise, _)) => {
                        match bm_core::chisel::apply(
                            self.blocks.get(old),
                            old_state,
                            family,
                            precise,
                            advanced,
                            octants,
                            restore,
                        ) {
                            Ok(shape) => {
                                self.wear(slot as usize);
                                (BlockState { shape, ..old_state }, true)
                            }
                            Err(bm_core::chisel::Refused::Told(t)) => {
                                self.notice(&t);
                                (old_state, false)
                            }
                            Err(bm_core::chisel::Refused::Quiet(_)) => (old_state, false),
                        }
                    }
                    None => (old_state, false),
                }
            }
            // Breaking is by swings now (`ClientMsg::Swing`); the old break
            // action, until the protocol drops it, is refused.
            #[allow(unreachable_patterns)]
            _ => (old_state, false),
        };
        self.answer_edit(seq, pos, block, accepted);
    }

    /// Records an edit's verdict and answers it, as the server does.
    fn answer_edit(&mut self, seq: u32, pos: BlockPos, block: BlockState, accepted: bool) {
        if accepted {
            self.damage.remove(&pos);
            self.planets[self.here].edits.insert(pos, block);
            if self.here == 0 {
                let slots = self.blocks.get(block.id).storage as usize;
                if slots > 0 {
                    self.chests.insert(pos, vec![None; slots]);
                } else if self.chests.remove(&pos).is_some() && self.open_chest == Some(pos) {
                    self.open_chest = None;
                    self.push_storage(pos, true);
                }
            }
        }
        self.events.push_back(NetEvent::Msg(ServerMsg::EditResult {
            seq,
            pos,
            block,
            accepted,
        }));
    }
}

/// The four demo planets, each with its kind's look, a shrine and, where
/// the kind grows them, trees. Only the first has the stations.
impl Demo {
    fn kinds(&self) -> bm_core::creatures::CreatureKinds {
        bm_core::creatures::CreatureKinds::builtin()
    }

    fn push_creature(&mut self, i: usize) {
        let c = &self.fight.creatures[i];
        let v = bm_protocol::CreatureView {
            id: c.id,
            kind: c.kind,
            tier: c.tier,
            damage: c.damage,
            angry: c.angry,
            dead: c.dead,
            looted: c.looted,
            exotic: c.exotic,
            element: 0,
        };
        self.events.push_back(NetEvent::Msg(ServerMsg::Creature(v)));
    }

    /// `--demo-ui creatures`: level ground with one of each creature in
    /// view: a grazer, a Tough pelter someone keeps hitting (taking aim), a
    /// dasher, a grazer's body, a Hardy grazer walking past, a Grim popper
    /// whistling, a popper curled into a ball, a drifter flying, and an
    /// exotic pelter; a bag of copper bombs, a stone and an iron sling in
    /// the hotbar.
    pub fn show_creatures(&mut self) {
        use bm_protocol::Pose;
        let b = |n: &str| self.blocks.id(n).expect("builtin block");
        let (trunk, foliage) = (b("trunk"), b("foliage"));
        let planet = &mut self.planets[0];
        planet.flat = true;
        // Open ground: the patch's trees and its row of stations would hide
        // the creatures.
        planet.edits.clear();
        let _ = (trunk, foliage);
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
        let k = self.kinds();
        let kind = |n: &str| k.id(n).expect("builtin creature");
        let at = |x: f64, z: f64| DVec3::new(x, (GROUND + 1) as f64, z);
        let creature = |id, kind, tier, pos, yaw, pose| DemoCreature {
            id,
            kind,
            tier,
            pos,
            yaw,
            pose,
            damage: 0,
            angry: false,
            dead: false,
            looted: false,
            pace: None,
            beaten: false,
            exotic: 0,
        };
        let deg = |d: f64| ((d / 360.0).rem_euclid(1.0) * 65536.0) as u16;
        self.fight.creatures = vec![
            creature(1, kind("grazer"), 1, at(124.5, 121.5), deg(35.0), Pose::Idle),
            DemoCreature {
                damage: 3100,
                angry: true,
                beaten: true,
                ..creature(2, kind("pelter"), 3, at(129.0, 121.0), deg(170.0), Pose::Windup)
            },
            creature(3, kind("dasher"), 1, at(133.0, 123.0), deg(-60.0), Pose::Idle),
            DemoCreature {
                dead: true,
                damage: 1500,
                ..creature(4, kind("grazer"), 1, at(126.0, 125.0), deg(80.0), Pose::Dead)
            },
            DemoCreature {
                pace: Some((119.0, 128.0, 1.4)),
                ..creature(5, kind("grazer"), 2, at(121.0, 115.5), deg(90.0), Pose::Walk)
            },
            DemoCreature {
                angry: true,
                ..creature(6, kind("popper"), 4, at(131.0, 124.5), deg(200.0), Pose::Lit)
            },
            creature(7, kind("popper"), 1, at(126.8, 123.6), deg(20.0), Pose::Curled),
            DemoCreature {
                angry: true,
                ..creature(
                    8,
                    kind("drifter"),
                    2,
                    at(124.5, 117.5) + DVec3::new(0.0, 2.2, 0.0),
                    deg(-20.0),
                    Pose::Flying,
                )
            },
            DemoCreature {
                damage: 300,
                exotic: bm_core::creatures::Exotic::HEALTH | bm_core::creatures::Exotic::ARMOUR,
                ..creature(9, kind("pelter"), 1, at(134.5, 120.0), deg(220.0), Pose::Idle)
            },
        ];
        for i in 0..self.fight.creatures.len() {
            self.push_creature(i);
        }
        // Slings in the last two hotbar slots.
        let before = self.inv.clone();
        for (slot, name) in [(7, "copper_bombs"), (8, "stone_sling"), (9, "iron_sling")] {
            let id = self.items.id(name).expect("builtin item");
            self.inv.slots[slot] = Some(ItemStack::new(id, 1));
        }
        self.slots_since(&before);
    }

    /// `--demo-ui bombs`: the creatures' ground with blasts going off among
    /// them in turn (as from other players' bombs), the bag of bombs in
    /// hand (the client holds and lets go of the button on a cycle).
    pub fn show_bombs(&mut self) {
        self.show_creatures();
        let y = (GROUND + 1) as f64 + crate::combat::BOMB_REST;
        self.fight.showcase = vec![DVec3::new(126.0, y, 118.5), DVec3::new(131.5, y, 117.0)];
        self.fight.next_showcase = self.fight.clock + 0.5;
    }

    /// `--demo-ui defeat`: a level 10 character just defeated by a Hardy
    /// Grazer, with the XP debt it costs.
    pub fn show_defeat(&mut self) {
        self.show_creatures();
        let xp = bm_core::skills::xp_for_level(10) + 2_400;
        let debt = bm_core::skills::defeat_debt(xp, false);
        self.character = Character {
            xp,
            debt,
            ..Default::default()
        };
        self.events.push_back(NetEvent::Msg(ServerMsg::Character {
            character: self.character.clone(),
            at: 0,
        }));
        self.events.push_back(NetEvent::Msg(ServerMsg::Defeated {
            debt,
            by: "Hardy Grazer".into(),
        }));
    }

    /// Moves the creatures, keeps hitting the beaten one, and sends a
    /// snapshot of them 20 times a second (`NetEvent::Observed`: the demo
    /// does not simulate the player).
    fn run_fight(&mut self, dt: f64) {
        if self.fight.creatures.is_empty() {
            return;
        }
        self.fight.clock += dt;
        let clock = self.fight.clock;
        let (due, waiting): (Vec<DemoBomb>, Vec<DemoBomb>) = std::mem::take(&mut self.fight.bombs)
            .into_iter()
            .partition(|b| b.at <= clock);
        self.fight.bombs = waiting;
        for b in due {
            self.blast(b);
        }
        if !self.fight.showcase.is_empty() && clock >= self.fight.next_showcase {
            self.fight.next_showcase = clock + SHOWCASE_SECONDS;
            let pos = self.fight.showcase[self.fight.showcased % self.fight.showcase.len()];
            self.fight.showcased += 1;
            self.events.push_back(NetEvent::Msg(ServerMsg::Blast {
                pos: pos.as_vec3().to_array(),
                radius: 2.5,
            }));
        }
        self.fight.snap_timer += dt;
        while self.fight.snap_timer >= 0.05 {
            self.fight.snap_timer -= 0.05;
            self.fight.tick += 3;
            for c in &mut self.fight.creatures {
                if let Some((from, to, speed)) = c.pace
                    && !c.dead
                {
                    let east = c.yaw < 32768;
                    c.pos.x += if east { speed } else { -speed } * 0.05;
                    if c.pos.x >= to {
                        c.yaw = 49152;
                    } else if c.pos.x <= from {
                        c.yaw = 16384;
                    }
                }
            }
            let creatures = self
                .fight
                .creatures
                .iter()
                .map(|c| bm_protocol::RemoteCreature {
                    id: c.id,
                    pos: c.pos.as_vec3().to_array(),
                    yaw: c.yaw,
                    pose: if c.dead { bm_protocol::Pose::Dead } else { c.pose },
                })
                .collect();
            self.events
                .push_back(NetEvent::Observed(bm_protocol::ServerDatagram::Snapshot {
                    planet: self.planets[self.here].info.id,
                    tick: self.fight.tick,
                    ack: 0,
                    you: PlayerState::default(),
                    others: Vec::new(),
                    creatures,
                }));
        }
        self.fight.beat_timer += dt;
        if self.fight.beat_timer >= 0.8 {
            self.fight.beat_timer = 0.0;
            self.fight.beats += 1;
            let crit = self.fight.beats % 3 == 0;
            // An untrained iron hammer's 700, a crit's 840, less armour 200.
            let amount = if crit { 640 } else { 500 };
            let k = self.kinds();
            for i in 0..self.fight.creatures.len() {
                let c = &mut self.fight.creatures[i];
                if !c.beaten || c.dead {
                    continue;
                }
                let health = k.get(c.kind).map_or(1, |d| d.tier(c.tier).health);
                // Never quite killed: it keeps its bar for the picture.
                c.damage = (c.damage + amount) % (health * 9 / 10);
                let id = c.id;
                self.events.push_back(NetEvent::Msg(ServerMsg::Hit {
                    target: bm_protocol::Target::Creature(id),
                    amount,
                    crit,
                }));
                self.push_creature(i);
            }
        }
    }

    /// Whether a swing, strike or shot at input `at` may happen (the shared
    /// cooldown), starting the next one `ticks` later if so.
    fn cadence(&mut self, at: u32, ticks: u32) -> bool {
        if let Some((last_at, last)) = self.last_swing
            && at < last_at.saturating_add(last)
        {
            return false;
        }
        self.last_swing = Some((at, ticks));
        true
    }

    /// `damage` (already skilled) on creature `i`, with the crit roll: the
    /// hit after armour, the death and the numbers.
    fn damage_creature(&mut self, i: usize, damage: u32, crit_chance: f32, crit_effect: f32) {
        let k = self.kinds();
        let crit = self.fight.rand() < crit_chance as f64;
        let c = &mut self.fight.creatures[i];
        let t = k.get(c.kind).expect("builtin kind").tier(c.tier);
        let dealt = bm_core::combat::dealt(damage, crit, crit_effect, t.armour);
        c.damage = (c.damage + dealt).min(t.health);
        c.angry = true;
        if c.damage >= t.health {
            c.dead = true;
            c.pose = bm_protocol::Pose::Dead;
        }
        let id = c.id;
        self.events.push_back(NetEvent::Msg(ServerMsg::Hit {
            target: bm_protocol::Target::Creature(id),
            amount: dealt,
            crit,
        }));
        self.push_creature(i);
    }

    /// A sling shot, by the server's rules: the shared cadence on the
    /// sling's skilled action speed, one wear, Draw's charge, and the shot
    /// flown from `FIGHT_VIEW`'s eye (the demo does not know where we stand)
    /// until it meets a living creature or its range. Unlike the server it
    /// spends no energy (the client predicts that) and flies through blocks.
    fn shoot(&mut self, at: u32, slot: u8, yaw: u16, pitch: i16, charge: u16) {
        let Some(sling) = self
            .inv
            .get(slot as usize)
            .and_then(|s| self.items.get(s.item)?.sling_of(&s, &Held::default()))
        else {
            return;
        };
        let effects = self.character.effects(&self.tree);
        let l = effects.skilled_sling(&sling);
        if !self.cadence(at, bm_core::mining::swing_ticks(l.action_speed)) {
            return;
        }
        self.wear(slot as usize);
        let charge = if effects.charging { charge as u32 } else { 0 };
        let damage = (l.damage as f64 * bm_core::combat::charge_multiplier(charge)).round() as u32;
        let eye = DVec3::from_array(FIGHT_VIEW) + DVec3::new(0.0, MoveConfig::default().eye_height, 0.0);
        let mut pos = eye;
        let mut vel = bm_core::movement::look_dir(yaw, pitch) * l.shot_speed as f64;
        let mut flown = 0.0;
        let k = self.kinds();
        while flown < l.range as f64 {
            let (next, v) = bm_core::combat::shot_step(pos, vel, l.falloff);
            flown += (next - pos).length();
            let hit = self.fight.creatures.iter().position(|c| {
                let d = k.get(c.kind).expect("builtin kind");
                let (hw, h) = (d.half_width as f64, d.height as f64);
                let (lo, hi) = (c.pos - DVec3::new(hw, 0.0, hw), c.pos + DVec3::new(hw, h, hw));
                !c.dead && crate::combat::segment_hits(pos, next, lo, hi)
            });
            if let Some(i) = hit {
                return self.damage_creature(i, damage, l.crit_chance, l.crit_effect);
            }
            (pos, vel) = (next, v);
        }
    }

    /// A bomb thrown, by the server's rules: the shared cadence on the bag's
    /// skilled action speed and one wear; flown from `FIGHT_VIEW`'s eye at
    /// `combat::throw_speed` for its hold, falling at `BOMB_GRAVITY` onto
    /// the demo's level ground, it goes off when the fuse lit `hold` ticks
    /// ago ends. Like `shoot`, it spends no energy (the client predicts
    /// that) and flies through blocks.
    fn throw(&mut self, at: u32, slot: u8, yaw: u16, pitch: i16, hold: u16) {
        let Some(bag) = self
            .inv
            .get(slot as usize)
            .and_then(|s| self.items.get(s.item)?.bomb_of(&s, &Held::default()))
        else {
            return;
        };
        let b = self.character.effects(&self.tree).skilled_bomb(&bag);
        if !self.cadence(at, bm_core::mining::swing_ticks(b.action_speed)) {
            return;
        }
        self.wear(slot as usize);
        let fuse_ticks = bm_core::combat::BOMB_FUSE_TICKS;
        let hold = (hold as u32).min(fuse_ticks);
        let mut pos = DVec3::from_array(FIGHT_VIEW) + DVec3::new(0.0, MoveConfig::default().eye_height, 0.0);
        let mut vel = bm_core::movement::look_dir(yaw, pitch) * bm_core::combat::throw_speed(hold, b.throw_speed);
        let rest = (GROUND + 1) as f64 + crate::combat::BOMB_REST;
        for _ in hold..fuse_ticks {
            let (next, v) = bm_core::combat::shot_step(pos, vel, crate::combat::BOMB_FALLOFF);
            if next.y <= rest {
                pos = DVec3::new(next.x, rest, next.z);
                break;
            }
            (pos, vel) = (next, v);
        }
        self.fight.bombs.push(DemoBomb {
            at: self.fight.clock + (fuse_ticks - hold) as f64 / bm_core::TICK_HZ as f64,
            pos,
            radius: b.radius,
            damage: b.damage,
            crit_chance: b.crit_chance,
            crit_effect: b.crit_effect,
        });
    }

    /// A bomb goes off: the blast for everyone to see, and impulse damage to
    /// every living creature within its radius (`combat::blast_damage`,
    /// half at the edge), less its armour.
    fn blast(&mut self, b: DemoBomb) {
        self.events.push_back(NetEvent::Msg(ServerMsg::Blast {
            pos: b.pos.as_vec3().to_array(),
            radius: b.radius,
        }));
        let k = self.kinds();
        for i in 0..self.fight.creatures.len() {
            let c = &self.fight.creatures[i];
            let height = k.get(c.kind).map_or(1.0, |d| d.height as f64);
            let dist = (c.pos + DVec3::new(0.0, height * 0.5, 0.0) - b.pos).length();
            let damage = bm_core::combat::blast_damage(b.damage, dist, b.radius as f64);
            if !c.dead && damage > 0 {
                self.damage_creature(i, damage, b.crit_chance, b.crit_effect);
            }
        }
    }

    /// A tool's strike on a creature: the shared cadence on the tool's
    /// skilled action speed, its damage (the same on creatures as on blocks:
    /// hand stone 200, iron 700), a crit roll, the creature's armour, one
    /// wear. Unlike the server it checks no reach.
    fn strike(&mut self, at: u32, creature: u32, slot: u8) {
        let held = self.inv.get(slot as usize);
        let Some(tool) = held.and_then(|s| self.items.get(s.item)?.tool_of(&s, &Held::default())) else {
            return;
        };
        let effects = self.character.effects(&self.tree);
        let tool = crate::mining::skilled_tool(&tool, &effects);
        if !self.cadence(at, bm_core::mining::swing_ticks(tool.action_speed)) {
            return;
        }
        self.wear(slot as usize);
        let Some(i) = self.fight.creatures.iter().position(|c| c.id == creature && !c.dead) else {
            return;
        };
        self.damage_creature(i, tool.damage, tool.crit_chance, tool.crit_effect);
    }

    /// Our own drops from a body, once (`creatures::roll_drops`).
    fn loot(&mut self, creature: u32) {
        let Some(i) = self.fight.creatures.iter().position(|c| c.id == creature) else {
            return;
        };
        let c = self.fight.creatures[i].clone();
        if !c.dead || c.looted {
            return;
        }
        let k = self.kinds();
        let tier = k.get(c.kind).expect("builtin kind").tier(c.tier).pool_for(None);
        let luck = self.character.effects(&self.tree).luck_multiplier();
        let fight = &mut self.fight;
        let drops = bm_core::creatures::roll_drops(&tier, luck, || fight.rand());
        let before = self.inv.clone();
        for (name, n) in drops {
            if let Ok(id) = self.items.id(&name) {
                let _ = self.inv.add_all_coloured(&self.items, id, Colour::NATURAL, n);
            }
        }
        self.slots_since(&before);
        self.fight.creatures[i].looted = true;
        self.push_creature(i);
    }

    /// Return after a defeat: back at the view, revived at half health.
    fn come_back(&mut self) {
        let mut spawn = PlayerState {
            pos: DVec3::from_array(FIGHT_VIEW),
            on_ground: true,
            down: true,
            ..Default::default()
        };
        spawn.revive(spawn.max_health() / 2);
        self.events.push_back(NetEvent::Msg(ServerMsg::Teleport {
            spawn,
            tick: self.fight.tick,
            yaw: None,
        }));
    }
}

fn demo_planets(blocks: &BlockRegistry) -> Vec<DemoPlanet> {
    let kinds: KindsFile = toml::from_str(include_str!("../../../assets/planets.toml")).expect("planets.toml is valid");
    let b = |n: &str| blocks.id(n).expect("builtin block");
    PLANETS
        .iter()
        .enumerate()
        .map(|(i, (name, kind, width))| {
            let k = kinds
                .kind
                .iter()
                .find(|k| k.name == *kind)
                .expect("kind in planets.toml");
            let display = k.display.clone().unwrap_or_else(|| bm_core::items::title_case(&k.name));
            let mut edits = HashMap::new();
            let mut put = |x, y, z, block| {
                edits.insert(BlockPos::new(x, y, z), BlockState::plain(block));
            };
            put(SHRINE[0], SHRINE[1], SHRINE[2], b("warp_shrine"));
            if i == 0 {
                let [x, y, z] = TABLE;
                put(x, y, z, b("crafting_table"));
                for [x, y, z] in WORKBENCH {
                    put(x, y, z, b("workbench"));
                }
                let [[bx, by, bz], [cx, cy, cz]] = FURNACE;
                put(bx, by, bz, b("furnace"));
                put(cx, cy, cz, b("furnace_crucible"));
                for [x, y, z] in EXTRACTOR {
                    put(x, y, z, b("extractor"));
                }
                put(126, 44, 131, b("claim_stone"));
                put(124, 44, 125, b("glow_crystal"));
                put(CHEST[0], CHEST[1], CHEST[2], b("storage_chest"));
            }
            if *kind != "arid" {
                for (tx, tz) in [(121, 120), (137, 119), (117, 135), (140, 138)] {
                    for y in 44..48 {
                        put(tx, y, tz, b("trunk"));
                    }
                    for dx in -2i64..=2 {
                        for dz in -2i64..=2 {
                            for y in 47..50 {
                                if dx.abs() + dz.abs() + (y - 48i64).abs() <= 3 && !(dx == 0 && dz == 0 && y < 48) {
                                    put(tx + dx, y, tz + dz, b("foliage"));
                                }
                            }
                        }
                    }
                }
            }
            let id = i as u16;
            DemoPlanet {
                info: PlanetInfo {
                    id,
                    name: name.to_string(),
                    kind: display.clone(),
                    tier: k.tier,
                    air: Default::default(),
                    shape: shape(),
                    // Days differ between planets, as on a real server.
                    day: DayCycle {
                        start_offset: DayCycle::default().start_offset + i as u64 * 9_000,
                        ..DayCycle::default()
                    },
                    tints: k.look.resolve_tints(blocks).expect("tints name blocks"),
                    look: k.look.clone(),
                },
                summary: PlanetSummary {
                    id,
                    name: name.to_string(),
                    kind: display,
                    tier: k.tier,
                    air: Default::default(),
                    color: k.look.color,
                    width_blocks: *width,
                    players: (i == 0) as u16 + [0, 3, 0, 7][i],
                    // The fourth shows as a wandering world by the first
                    // (milestone 21), leaving in about four days.
                    host: (i == 3).then_some(0),
                    leaves: (i == 3).then_some(100 * 3600 * 60),
                    capital: (i == 0).then(|| "Riverholm".to_string()),
                },
                edits,
                flat: false,
            }
        })
        .collect()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn recipe_index(book: &RecipeBook, items: &ItemRegistry, output: &str) -> u16 {
    let id = items.id(output).expect("builtin item");
    book.recipes
        .iter()
        .position(|r| r.output == id)
        .expect("builtin recipe") as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Swings at `pos` with hotbar `slot` as often as allowed until the
    /// demo answers with an `EditResult`; returns that poll's events.
    fn swing_until_answered(d: &mut Demo, pos: BlockPos, slot: u8) -> Vec<NetEvent> {
        for n in 1..100 {
            d.send(ClientMsg::Swing {
                seq: 1000 + n,
                at: n * 60,
                pos,
                slot,
            });
            let ev = d.poll();
            if ev
                .iter()
                .any(|e| matches!(e, NetEvent::Msg(ServerMsg::EditResult { pos: p, .. }) if *p == pos))
            {
                return ev;
            }
        }
        panic!("never broke")
    }

    fn healths(ev: &[NetEvent]) -> Vec<(BlockPos, u32)> {
        ev.iter()
            .filter_map(|e| match e {
                NetEvent::Msg(ServerMsg::BlockHealth { pos, damage }) => Some((*pos, *damage)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_spark_demo_shows_wear_and_spark_and_a_spanner_repairs() {
        let mut d = Demo::new("Tester");
        d.poll();
        d.show_spark();
        d.poll();
        let mixer = bp(MIXER[2]);
        d.send(ClientMsg::WatchStation { pos: Some(mixer) });
        let view = d
            .poll()
            .into_iter()
            .find_map(|e| match e {
                NetEvent::Msg(ServerMsg::Station(v)) => Some(v),
                _ => None,
            })
            .expect("the mixer answers");
        assert_eq!(view.pos, bp(MIXER[0]), "keyed by its first block");
        assert_eq!(view.station, Station::Mixer);
        assert_eq!(view.wear, Some((8_450, 10_000)));
        assert!(view.spark.is_some_and(|s| s.linked && s.stored == 6_400));
        // Slot 9 holds an iron spanner: 700 wear a hit, nothing broken.
        d.send(ClientMsg::Swing {
            seq: 1,
            at: 10,
            pos: mixer,
            slot: 9,
        });
        let ev = d.poll();
        assert!(
            ev.iter().any(|e| matches!(
                e,
                NetEvent::Msg(ServerMsg::MachineWear {
                    wear: 9_150,
                    max: 10_000,
                    ..
                })
            )),
            "a MachineWear answers the spanner"
        );
        assert!(
            !ev.iter()
                .any(|e| matches!(e, NetEvent::Msg(ServerMsg::BlockHealth { .. })))
        );
        assert_eq!(d.ground(mixer).id, d.blocks.id("mixer").unwrap());
    }

    #[test]
    fn the_demo_answers_swings_like_the_server() {
        let mut d = Demo::new("Tester");
        d.poll();
        // Slot 0 holds a stone hammer: three swings break stone on tier 1,
        // each 0.82 s (49 ticks) after the last.
        let stone = BlockPos::new(128, 39, 128);
        assert_eq!(d.ground(stone).id, d.blocks.id("stone").unwrap());
        let swing_at = |d: &mut Demo, pos, seq, at| {
            d.send(ClientMsg::Swing { seq, at, pos, slot: 0 });
            d.poll()
        };
        let swing = |d: &mut Demo, seq, at| {
            d.send(ClientMsg::Swing {
                seq,
                at,
                pos: stone,
                slot: 0,
            });
            d.poll()
        };
        let ticks = bm_core::mining::swing_ticks(15.0);
        assert_eq!(ticks, 49);
        assert_eq!(healths(&swing(&mut d, 1, 10)), [(stone, 400)]);
        // Too soon: refused, the truth sent back.
        assert_eq!(healths(&swing(&mut d, 2, 10 + ticks - 1)), [(stone, 400)]);
        assert_eq!(healths(&swing(&mut d, 3, 10 + ticks)), [(stone, 800)]);
        let ev = swing(&mut d, 4, 10 + 2 * ticks);
        assert!(ev.iter().any(|e| matches!(e,
            NetEvent::Msg(ServerMsg::EditResult { seq: 4, accepted: true, block, .. }) if block.id.is_air())));
        assert_eq!(d.ground(stone), BlockState::AIR);
        // Every swing wore the hammer (the refused one did not).
        assert_eq!(d.inv.slots[0].unwrap().wear, 38 + 3);
        // Left alone for five seconds, damage heals.
        let next = BlockPos::new(129, 39, 128);
        let at = 10 + 3 * ticks;
        assert_eq!(healths(&swing_at(&mut d, next, 5, at)), [(next, 400)]);
        let later = at + bm_core::mining::HEAL_TICKS as u32;
        assert_eq!(
            healths(&swing_at(&mut d, next, 6, later)),
            [(next, 400)],
            "healed, then hit afresh"
        );
        // A torch goes on the face named, and only on a solid one.
        let torch = d.items.id("torch").unwrap();
        d.inv.slots[5] = Some(ItemStack::new(torch, 4));
        let place = |d: &mut Demo, pos, mount| {
            d.send(ClientMsg::Edit {
                seq: 9,
                at: 0,
                pos,
                action: EditAction::Place {
                    slot: 5,
                    mount: Some(mount),
                },
            });
            d.poll().into_iter().find_map(|e| match e {
                NetEvent::Msg(ServerMsg::EditResult { block, accepted, .. }) => Some((block, accepted)),
                _ => None,
            })
        };
        // In the hole, hung on the stone to its north.
        let (b, ok) = place(&mut d, stone, bm_core::Mount::North).unwrap();
        assert!(ok && b.id == d.blocks.id("torch_north").unwrap(), "{b:?}");
        // Two swings of anything break it.
        let ev = swing_until_answered(&mut d, stone, 0);
        assert!(
            ev.iter()
                .any(|e| matches!(e, NetEvent::Msg(ServerMsg::EditResult { accepted: true, .. })))
        );
        assert_eq!(d.ground(stone), BlockState::AIR);
        // Over open air there is nothing to stand on.
        let (_, ok) = place(&mut d, BlockPos::new(128, 46, 128), bm_core::Mount::Floor).unwrap();
        assert!(!ok);
        // A full pack leaves a block one point short, and says so.
        let glass = d.items.id("glass").unwrap();
        for slot in d.inv.slots.iter_mut().skip(1) {
            *slot = Some(ItemStack::new(glass, 64));
        }
        let rock = BlockPos::new(130, 39, 128);
        let ev = swing_until_answered(&mut d, rock, 0);
        assert!(
            ev.iter()
                .any(|e| matches!(e, NetEvent::Msg(ServerMsg::EditResult { accepted: false, .. })))
        );
        assert_eq!(healths(&ev), [(rock, 1199)]);
        assert_eq!(d.ground(rock).id, d.blocks.id("stone").unwrap());
    }

    #[test]
    fn the_demo_answers_like_a_server() {
        let mut d = Demo::new("Tester");
        let first = d.poll();
        assert!(matches!(first[0], NetEvent::Connected(_)));
        assert!(first.len() > 100, "chunks follow the welcome");
        d.send(ClientMsg::MoveItems {
            from: 1,
            to: 30,
            count: 10,
        });
        let ev = d.poll();
        let slots = ev.iter().find_map(|e| match e {
            NetEvent::Msg(ServerMsg::Slots { changes }) => Some(changes.clone()),
            _ => None,
        });
        let changes = slots.expect("slots sent");
        assert_eq!(changes.len(), 2);
        assert_eq!(changes.iter().find(|c| c.0 == 30).unwrap().1.unwrap().count, 10);
    }

    fn station_views(ev: &[NetEvent]) -> Vec<StationView> {
        ev.iter()
            .filter_map(|e| match e {
                NetEvent::Msg(ServerMsg::Station(v)) => Some(v.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_demo_machines_answer_for_any_block_with_their_own_position() {
        let mut d = Demo::new("Tester");
        d.poll();
        // The workbench's middle block opens the machine at its first block.
        d.send(ClientMsg::WatchStation {
            pos: Some(bp(WORKBENCH[2])),
        });
        let v = station_views(&d.poll()).pop().expect("a station view");
        assert_eq!(
            (v.pos, v.station, v.idle.clone()),
            (bp(WORKBENCH[0]), Station::Workbench, None)
        );
        // The crucible opens the furnace at its base, with its fuel.
        d.send(ClientMsg::WatchStation {
            pos: Some(bp(FURNACE[1])),
        });
        let v = station_views(&d.poll()).pop().unwrap();
        assert_eq!((v.pos, v.station), (bp(FURNACE[0]), Station::Furnace));
        assert_eq!(v.fuel.map(|f| f.count), Some(6));
        // Three extractor blocks say why they do nothing and refuse a craft.
        d.send(ClientMsg::WatchStation {
            pos: Some(bp(EXTRACTOR[2])),
        });
        let v = station_views(&d.poll()).pop().unwrap();
        assert_eq!(v.idle.as_deref(), Some("An extractor needs 4 blocks touching"));
        let sackcloth = recipe_index(&d.book, &d.items, "sackcloth");
        let before = d.inv.clone();
        d.send(ClientMsg::Craft {
            recipe: sackcloth,
            count: 1,
            station: Some(bp(EXTRACTOR[1])),
            colour: None,
        });
        assert_eq!(d.inv, before);
    }

    #[test]
    fn the_demo_furnace_takes_fuel_in_and_gives_it_back() {
        let mut d = Demo::new("Tester");
        d.poll();
        let coal = d.items.id("coal").unwrap();
        let slot = (0..d.inv.slots.len())
            .find(|&i| d.inv.slots[i].is_some_and(|s| s.item == coal))
            .unwrap();
        let held = d.inv.slots[slot].unwrap().count;
        let pos = bp(FURNACE[1]);
        d.send(ClientMsg::StationFuel {
            pos,
            slot: slot as u8,
            count: 4,
            into: true,
        });
        let v = station_views(&d.poll()).pop().expect("the furnace answers");
        assert_eq!(v.fuel.map(|f| f.count), Some(6 + 4));
        assert_eq!(d.inv.slots[slot].unwrap().count, held - 4);
        // Stone is no fuel.
        let stone = (0..d.inv.slots.len())
            .find(|&i| d.inv.slots[i].is_some_and(|s| s.item == d.items.id("stone").unwrap()))
            .unwrap();
        d.send(ClientMsg::StationFuel {
            pos,
            slot: stone as u8,
            count: 5,
            into: true,
        });
        assert!(station_views(&d.poll()).is_empty());
        // All of it back out, onto the coal stack.
        d.send(ClientMsg::StationFuel {
            pos,
            slot: slot as u8,
            count: 10,
            into: false,
        });
        let v = station_views(&d.poll()).pop().unwrap();
        assert_eq!(v.fuel, None);
        assert_eq!(d.inv.slots[slot].unwrap().count, held + 6);
    }

    #[test]
    fn the_flora_showroom_holds_every_gathering_object() {
        let mut d = Demo::new("Tester");
        d.poll();
        d.show_flora();
        let placed: std::collections::HashSet<BlockId> = d.planets[0].edits.values().map(|s| s.id).collect();
        for (id, def) in d.blocks.iter() {
            if def.health == bm_core::Health::Plant {
                assert!(placed.contains(&id), "{} is missing", def.name);
            }
        }
        // Each stands on something solid.
        let flags = d.blocks.flags();
        for (pos, s) in d.planets[0].edits.clone() {
            if flags.mount(s.id).is_some() {
                let below = d.ground(pos.offset(0, -1, 0));
                assert!(flags.solid(below.id), "{pos:?} floats");
            }
        }
    }

    #[test]
    fn the_demo_travels_between_its_planets() {
        let mut d = Demo::new("Tester");
        d.hold_arrivals(0.0);
        let ev = d.poll();
        let NetEvent::Connected(w) = &ev[0] else { panic!() };
        assert_eq!(w.universe.len(), 4);
        assert_eq!(w.planet.id, 0);
        let shrine = BlockPos::new(SHRINE[0], SHRINE[1], SHRINE[2]);
        d.send(ClientMsg::Warp { shrine, planet: 1 });
        let ev = d.poll();
        let NetEvent::Msg(ServerMsg::Travel(t)) = &ev[0] else {
            panic!("travel first")
        };
        assert_eq!(t.planet.kind, "Arid");
        assert_ne!(
            t.planet.tints[d.blocks.id("sand").unwrap().0 as usize],
            bm_core::NO_TINT
        );
        assert!(t.claims.is_empty());
        assert!(ev.iter().any(|e| matches!(e, NetEvent::Msg(ServerMsg::Chunk { .. }))));
        // Warping to where you are is refused with a notice.
        d.send(ClientMsg::Warp { shrine, planet: 1 });
        assert!(matches!(&d.poll()[0], NetEvent::Msg(ServerMsg::Notice { .. })));
    }

    fn storage_views(ev: &[NetEvent]) -> Vec<StorageView> {
        ev.iter()
            .filter_map(|e| match e {
                NetEvent::Msg(ServerMsg::Storage(v)) => Some(v.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_demo_chest_opens_moves_and_refuses_breaking() {
        use bm_protocol::SlotRef;
        let mut d = Demo::new("Tester");
        d.poll();
        let pos = BlockPos::new(CHEST[0], CHEST[1], CHEST[2]);
        d.send(ClientMsg::OpenStorage { pos: Some(pos) });
        let views = storage_views(&d.poll());
        assert_eq!(views.len(), 1);
        assert!(!views[0].closed && views[0].slots.len() == 20);
        let coal = views[0].slots[1].unwrap();
        // Chest to an empty inventory slot: both sides are told.
        d.send(ClientMsg::StorageMove {
            pos,
            from: SlotRef::Storage(1),
            to: SlotRef::Inventory(35),
            count: coal.count,
        });
        let ev = d.poll();
        assert_eq!(storage_views(&ev)[0].slots[1], None);
        assert!(
            ev.iter()
                .any(|e| matches!(e, NetEvent::Msg(ServerMsg::Slots { changes })
            if changes.contains(&(35, Some(coal)))))
        );
        // A chest with things in it stays.
        let ev = swing_until_answered(&mut d, pos, 0);
        assert!(
            ev.iter()
                .any(|e| matches!(e, NetEvent::Msg(ServerMsg::EditResult { accepted: false, .. })))
        );
        assert!(ev.iter().any(|e| matches!(e, NetEvent::Msg(ServerMsg::Notice { text })
            if text == "Empty the Storage Chest before breaking it")));
        // Closing is confirmed.
        d.send(ClientMsg::OpenStorage { pos: None });
        let views = storage_views(&d.poll());
        assert!(views.len() == 1 && views[0].closed && views[0].slots.is_empty());
    }

    #[test]
    fn the_demo_reclaim_moves_what_fits_and_keeps_colours() {
        let mut d = Demo::new("Tester");
        d.poll();
        d.sample_reclaim();
        let waiting = d.reclaim.clone();
        assert!(waiting.iter().any(|s| s.colour != Colour::NATURAL));
        d.poll();
        let shrine = BlockPos::new(SHRINE[0], SHRINE[1], SHRINE[2]);
        d.send(ClientMsg::CollectReclaim { shrine });
        let ev = d.poll();
        assert!(ev.iter().any(|e| matches!(e, NetEvent::Msg(ServerMsg::Slots { .. }))));
        let Some(NetEvent::Msg(ServerMsg::Reclaim { items })) = ev
            .iter()
            .find(|e| matches!(e, NetEvent::Msg(ServerMsg::Reclaim { .. })))
        else {
            panic!("reclaim answered")
        };
        assert!(items.is_empty(), "everything fit: {items:?}");
        let azure = d.colours.id("Rich Azure").unwrap();
        let stone = d.items.id("stone").unwrap();
        let blue: u32 = d
            .inv
            .slots
            .iter()
            .flatten()
            .filter(|s| s.item == stone && s.colour == azure)
            .map(|s| s.count as u32)
            .sum();
        assert_eq!(blue, 137);
    }

    #[test]
    fn the_demo_chisels_by_the_rules_and_wears_the_chisel() {
        use bm_core::{Shape, ShapeFamily, octant};
        let mut d = Demo::new("Tester");
        d.poll();
        d.show_shapes(true);
        d.poll();
        let brick = BlockPos::new(128, 45, 127);
        let edge = 1 << octant(0, 1, 1) | 1 << octant(1, 1, 1);
        let edit = |d: &mut Demo, seq, pos, slot: u8, octants, restore| {
            d.send(ClientMsg::Edit {
                seq,
                at: 0,
                pos,
                action: EditAction::Chisel { slot, octants, restore },
            });
            d.poll().into_iter().find_map(|e| match e {
                NetEvent::Msg(ServerMsg::EditResult { block, accepted, .. }) => Some((block, accepted)),
                _ => None,
            })
        };
        // Slot 6 holds the bevel chisel (slots 4 to 9: square, slope, bevel...).
        let (block, ok) = edit(&mut d, 1, brick, 6, edge, false).unwrap();
        assert!(ok);
        assert_eq!(block.shape, Shape::new(ShapeFamily::Bevel, edge).unwrap());
        assert_eq!(d.inv.slots[6].unwrap().wear, 1);
        // A precise cut asked of an ordinary chisel is refused.
        assert!(!edit(&mut d, 2, brick, 6, 1, false).unwrap().1);
        // Another family on the same block is refused.
        assert!(
            !edit(&mut d, 3, brick, 4, 1 << octant(0, 0, 0) | 1 << octant(1, 0, 0), false)
                .unwrap()
                .1
        );
        // Filling it back makes it whole.
        let (block, ok) = edit(&mut d, 4, brick, 6, edge, true).unwrap();
        assert!(ok && block.shape.is_full());
        // Natural stone takes only slopes.
        let stone = BlockPos::new(128, 44, 127);
        assert!(!edit(&mut d, 5, stone, 4, edge, false).unwrap().1);
        assert!(edit(&mut d, 6, stone, 5, edge, false).unwrap().1);
    }

    #[test]
    fn the_demo_places_and_breaks_in_colour() {
        let mut d = Demo::new("Tester");
        d.poll();
        d.show_colours();
        d.poll();
        let slot = d
            .inv
            .slots
            .iter()
            .position(|s| s.is_some_and(|s| s.colour != Colour::NATURAL))
            .unwrap();
        let stack = d.inv.slots[slot].unwrap();
        let pos = BlockPos::new(130, 44, 131);
        d.send(ClientMsg::Edit {
            seq: 1,
            at: 0,
            pos,
            action: EditAction::Place {
                slot: slot as u8,
                mount: None,
            },
        });
        let placed = d.poll().into_iter().find_map(|e| match e {
            NetEvent::Msg(ServerMsg::EditResult { block, accepted, .. }) => Some((block, accepted)),
            _ => None,
        });
        let (block, accepted) = placed.unwrap();
        assert!(accepted);
        assert_eq!(block.colour, stack.colour);
        swing_until_answered(&mut d, pos, 0);
        assert_eq!(
            d.inv.slots[slot].unwrap().count,
            stack.count,
            "it came back in its colour"
        );
    }

    /// The `Hit`s and creature views among the demo's answers.
    fn fight_news(ev: &[NetEvent]) -> (Vec<(u32, u32, bool)>, Vec<bm_protocol::CreatureView>) {
        let mut hits = Vec::new();
        let mut views = Vec::new();
        for e in ev {
            match e {
                NetEvent::Msg(ServerMsg::Hit {
                    target: bm_protocol::Target::Creature(id),
                    amount,
                    crit,
                }) => hits.push((*id, *amount, *crit)),
                NetEvent::Msg(ServerMsg::Creature(v)) => views.push(v.clone()),
                _ => {}
            }
        }
        (hits, views)
    }

    #[test]
    fn the_demo_fights_by_the_rules() {
        let mut d = Demo::new("Tester");
        d.show_creatures();
        d.poll();
        let hammer = 0u8;
        let speed = d
            .items
            .swing_tool(d.items.id("stone_hammer").ok())
            .unwrap()
            .action_speed;
        let ticks = bm_core::mining::swing_ticks(speed);
        let strike = |d: &mut Demo, at| {
            d.send(ClientMsg::Strike {
                at,
                creature: 1,
                slot: hammer,
            });
            fight_news(&d.poll())
        };
        // A stone hammer on a tier 1 grazer (1500 health, no armour): 400 a
        // strike, on the swing cadence.
        let (hits, views) = strike(&mut d, 10);
        assert_eq!(hits, vec![(1, 400, false)]);
        assert!(views[0].angry && !views[0].dead);
        let (hits, _) = strike(&mut d, 11);
        assert!(hits.is_empty(), "too soon after the last");
        let wear = d.inv.get(hammer as usize).unwrap().wear;
        assert_eq!(wear, 38 + 1, "one wear a strike that landed");
        let mut at = 10;
        let mut last = Vec::new();
        for _ in 0..3 {
            at += ticks;
            last = strike(&mut d, at).1;
        }
        assert!(last[0].dead && last[0].damage == 1500, "four strikes kill it");
        // The cooldown is shared: a sling right after the strike waits.
        let shoot = |d: &mut Demo, at, (yaw, pitch)| {
            d.send(ClientMsg::Shoot {
                at,
                slot: 8,
                yaw,
                pitch,
                charge: 0,
            });
            fight_news(&d.poll())
        };
        // Aimed at the Tough pelter's middle from where the demo stands us.
        let eye = DVec3::from_array(FIGHT_VIEW) + DVec3::new(0.0, MoveConfig::default().eye_height, 0.0);
        let to = DVec3::new(129.0, 44.45, 121.0) - eye;
        let aim = bm_core::PlayerInput::from_angles(to.x.atan2(-to.z), (to.y / to.length()).asin());
        assert!(shoot(&mut d, at + 1, aim).0.is_empty(), "the strike's cooldown");
        // Stone sling 300 less the Tough pelter's 200 armour.
        let (hits, _) = shoot(&mut d, at + ticks, aim);
        assert_eq!(hits, vec![(2, 100, false)]);
        // Looting the body: our own drops once.
        d.send(ClientMsg::Loot { creature: 1 });
        let ev = d.poll();
        assert!(ev.iter().any(|e| matches!(e, NetEvent::Msg(ServerMsg::Slots { .. }))));
        assert!(fight_news(&ev).1[0].looted);
        d.send(ClientMsg::Loot { creature: 1 });
        assert!(fight_news(&d.poll()).1.is_empty(), "once");
        // A living creature has nothing to loot.
        d.send(ClientMsg::Loot { creature: 3 });
        assert!(fight_news(&d.poll()).1.is_empty());
    }

    fn blasts(ev: &[NetEvent]) -> Vec<([f32; 3], f32)> {
        ev.iter()
            .filter_map(|e| match e {
                NetEvent::Msg(ServerMsg::Blast { pos, radius }) => Some((*pos, *radius)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_demo_throws_bombs_by_the_rules() {
        use bm_core::combat::BOMB_FUSE_TICKS;
        let mut d = Demo::new("Tester");
        d.show_creatures();
        // A grazer at our feet, where a tapped bomb drops.
        let feet = DVec3::from_array(FIGHT_VIEW);
        d.fight.creatures[0].pos = feet + DVec3::new(1.0, 0.0, 0.0);
        d.poll();
        let throw = |d: &mut Demo, at, hold| {
            d.send(ClientMsg::Throw {
                at,
                slot: 7,
                yaw: 0,
                pitch: 0,
                hold,
            })
        };
        // A tap: dropped at our feet, going off when the whole fuse has
        // burned, its blast the bag's radius.
        throw(&mut d, 10, 0);
        throw(&mut d, 11, 0);
        assert_eq!(d.fight.bombs.len(), 1, "the second waits for the cooldown");
        assert_eq!(d.inv.get(7).unwrap().wear, 1);
        let fuse = BOMB_FUSE_TICKS as f64 / 60.0;
        d.run_fight(fuse - 0.1);
        assert!(blasts(&d.poll()).is_empty(), "not yet");
        d.run_fight(0.2);
        let ev = d.poll();
        let b = blasts(&ev);
        assert_eq!(b.len(), 1);
        assert!((b[0].0[1] as f64 - (GROUND + 1) as f64).abs() < 0.3, "on the ground");
        assert!((b[0].0[0] as f64 - feet.x).abs() < 0.1 && (b[0].0[2] as f64 - feet.z).abs() < 0.1);
        assert_eq!(b[0].1, 3.0);
        // The grazer a block away took the blast (900 near the middle, less
        // nothing: tier 1 has no armour).
        let (hits, _) = fight_news(&ev);
        assert!(hits.iter().any(|h| h.0 == 1 && h.1 > 450 && h.1 <= 900), "{hits:?}");
        // Held a second: thrown well away, and goes off sooner.
        let ticks = bm_core::mining::swing_ticks(50.0);
        throw(&mut d, 10 + ticks, 60);
        let bomb = d.fight.bombs[0];
        assert!((bomb.pos - feet).length() > 5.0, "thrown: {}", bomb.pos);
        assert!((bomb.at - d.fight.clock - (BOMB_FUSE_TICKS - 60) as f64 / 60.0).abs() < 1e-9);
    }

    #[test]
    fn the_bombs_screen_keeps_blasts_going_off() {
        let mut d = Demo::new("Tester");
        d.show_bombs();
        d.poll();
        let mut seen = 0;
        for _ in 0..80 {
            d.run_fight(0.05);
            seen += blasts(&d.poll()).len();
        }
        assert!(seen >= 2, "a blast every {SHOWCASE_SECONDS} s: {seen}");
        assert!(
            d.inv
                .get(7)
                .is_some_and(|s| d.items.get(s.item).and_then(|d| d.bomb()).is_some())
        );
    }

    #[test]
    fn the_demo_brings_us_back_at_half_health() {
        let mut d = Demo::new("Tester");
        d.show_defeat();
        let ev = d.poll();
        assert!(ev.iter().any(|e| matches!(
            e,
            NetEvent::Msg(ServerMsg::Defeated { debt: 750, by }) if by == "Hardy Grazer"
        )));
        assert!(ev.iter().any(|e| matches!(
            e,
            NetEvent::Msg(ServerMsg::Character { character, .. }) if character.debt == 750
        )));
        d.send(ClientMsg::Return);
        let back = d.poll().into_iter().find_map(|e| match e {
            NetEvent::Msg(ServerMsg::Teleport { spawn, .. }) => Some(spawn),
            _ => None,
        });
        let spawn = back.expect("a teleport back");
        assert!(!spawn.down);
        assert_eq!(spawn.health(), spawn.max_health() / 2);
        assert_eq!(spawn.pos, DVec3::from_array(FIGHT_VIEW));
    }

    #[test]
    fn the_demo_echoes_chat_cleaned() {
        let mut d = Demo::new("Tester");
        d.poll();
        d.send(ClientMsg::Chat {
            channel: Channel::Planet,
            text: format!("hi\u{7}{}", "x".repeat(400)),
        });
        let ev = d.poll();
        let Some(NetEvent::Msg(ServerMsg::Chat { lines })) = ev.first() else {
            panic!()
        };
        assert_eq!(lines[0].from, "Tester");
        assert!(lines[0].text.starts_with("hix"));
        assert_eq!(lines[0].text.chars().count(), MAX_CHAT_LEN);
    }

    fn portal_views(ev: &[NetEvent]) -> Vec<PortalView> {
        ev.iter()
            .filter_map(|e| match e {
                NetEvent::Msg(ServerMsg::Portal(v)) => Some(v.clone()),
                _ => None,
            })
            .collect()
    }

    fn notices(ev: &[NetEvent]) -> Vec<String> {
        ev.iter()
            .filter_map(|e| match e {
                NetEvent::Msg(ServerMsg::Notice { text }) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_demo_answers_skill_changes_by_the_rules() {
        let mut d = Demo::new("Tester");
        d.poll();
        d.show_skills();
        d.poll();
        let learn = |id: &str| bm_protocol::SkillAction::Learn(id.into());
        d.send(ClientMsg::Skill {
            at: 7,
            action: learn("agility"),
        });
        let ev = d.poll();
        let answered = ev.iter().find_map(|e| match e {
            NetEvent::Msg(ServerMsg::Character { character, at }) => Some((character.clone(), *at)),
            _ => None,
        });
        let (c, at) = answered.expect("a Character answers");
        assert_eq!((c.page().rank("agility"), at), (1, 7));
        d.send(ClientMsg::Skill {
            at: 8,
            action: learn("augment_crafting"),
        });
        assert_eq!(notices(&d.poll()), vec!["That skill comes later".to_string()]);
        let marks = d.character.marks;
        d.send(ClientMsg::Skill {
            at: 9,
            action: bm_protocol::SkillAction::BuyPlots(2),
        });
        d.poll();
        assert_eq!((d.character.plots_bought, d.character.marks), (2, marks - 60));
        d.send(ClientMsg::Skill {
            at: 10,
            action: bm_protocol::SkillAction::BuyPlots(80),
        });
        assert_eq!(notices(&d.poll()), vec!["You need 2400 marks".to_string()]);
    }

    #[test]
    fn the_demo_portals_link_fuel_and_open_by_the_rules() {
        let mut d = Demo::new("Tester");
        d.show_portals();
        d.poll();
        let [x, y, z] = PORTALS[0];
        let pos = BlockPos::new(x, y + 1, z);
        let act = |action| ClientMsg::Portal { pos, action };
        d.send(ClientMsg::OpenPortal { pos: Some(pos) });
        let v = portal_views(&d.poll()).pop().unwrap();
        assert_eq!((v.width, v.height, v.fuel, v.open), (1, 2, 37, true));
        assert_eq!(v.link.as_deref(), Some("Hearthmoor, near 135, 121"));
        // Take it all out: it goes dormant, its blocks turn to conduits.
        d.send(act(PortalAction::TakeFuel { count: 500 }));
        let ev = d.poll();
        let v = portal_views(&ev).pop().unwrap();
        assert_eq!((v.fuel, v.open), (0, false));
        let conduit = d.blocks.id("portal_conduit").unwrap();
        assert!(
            ev.iter()
                .any(|e| matches!(e, NetEvent::Msg(ServerMsg::BlockChanged { block, .. })
            if block.id == conduit))
        );
        // Fuel from a stack reopens it.
        d.send(act(PortalAction::AddFuel { slot: 9, count: 5 }));
        let v = portal_views(&d.poll()).pop().unwrap();
        assert_eq!((v.fuel, v.open), (5, true));
        // The 37 taken out joined the 60 in slot 9; 5 of those went back.
        assert_eq!(d.inv.get(9).unwrap().count, 92);
        // A token of another size, or of this portal, is refused and kept.
        d.send(act(PortalAction::Link { slot: 25 }));
        assert_eq!(notices(&d.poll()), vec!["That token is for a portal of another size"]);
        d.send(act(PortalAction::MakeToken));
        let ev = d.poll();
        assert!(
            ev.iter()
                .any(|e| matches!(e, NetEvent::Msg(ServerMsg::TokenNames { names })
            if names == &vec![(1, "1 by 2, Hearthmoor, near 122, 121".to_string())]))
        );
        let own = d.inv.slots.iter().position(|s| s.is_some_and(|s| s.tag == 1)).unwrap();
        d.send(act(PortalAction::Link { slot: own as u8 }));
        assert_eq!(notices(&d.poll()), vec!["That token was made at this portal"]);
        assert!(d.inv.get(own).is_some());
        // The token for Saffra links there, at two a trip; the old partner
        // is left unlinked and goes dormant.
        d.send(act(PortalAction::Link { slot: 8 }));
        let v = portal_views(&d.poll()).pop().unwrap();
        assert_eq!(v.link.as_deref(), Some("Saffra, near 212, 96"));
        assert_eq!((v.trip_cost, v.open), (2, true));
        assert!(d.inv.get(8).is_none(), "the token is used up");
        assert_eq!(d.portals[1].link, None);
        // Unlink closes it.
        d.send(act(PortalAction::Unlink));
        let v = portal_views(&d.poll()).pop().unwrap();
        assert_eq!((v.link, v.open), (None, false));
        assert_eq!(d.portals[2].link, None);
    }

    fn forged_news(ev: &[NetEvent]) -> Vec<bm_protocol::ForgeOutcome> {
        ev.iter()
            .filter_map(|e| match e {
                NetEvent::Msg(ServerMsg::Forged { outcome, .. }) => Some(outcome.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_demo_forge_runs_a_real_session_seals_and_queues() {
        let mut d = Demo::new("Tester");
        d.poll();
        d.show_forge(ForgeShow::Session);
        let pos = bp(FORGE[2]);
        d.send(ClientMsg::WatchStation { pos: Some(pos) });
        let v = station_views(&d.poll()).pop().expect("the forge's view");
        assert_eq!((v.pos, v.station), (bp(FORGE[0]), Station::Forge));
        let s = v.forge.and_then(|f| f.session).expect("a session under way");
        assert_eq!(s.rounds, 8, "a few rounds played");
        assert!(!s.buffs.is_empty(), "the force glazes last");
        assert!(s.traits.iter().any(|t| !t.is_empty()), "some traits landed");
        // A stale round (a double click) is ignored; the right one plays,
        // exactly as the real session does.
        let mut mirror = d.stations[&bp(FORGE[0])]
            .forge
            .as_ref()
            .unwrap()
            .session
            .clone()
            .unwrap();
        d.send(ClientMsg::ForgeUse {
            pos,
            slot: 0,
            round: s.rounds - 1,
        });
        assert!(station_views(&d.poll()).is_empty());
        d.send(ClientMsg::ForgeUse {
            pos,
            slot: 0,
            round: s.rounds,
        });
        mirror.use_ingredient(0).unwrap();
        let after = station_views(&d.poll()).pop().unwrap().forge.unwrap().session.unwrap();
        assert_eq!(after, bm_protocol::SessionView::of(&mirror));
        // Sealing: the traits go into the queue; the rest of the deck comes
        // back to the pack.
        let wax = d.items.id("sealing_wax").unwrap();
        let before = d.inv.count(wax);
        d.send(ClientMsg::ForgeUse {
            pos,
            slot: 5,
            round: after.rounds,
        });
        let ev = d.poll();
        let sealed = match forged_news(&ev).pop() {
            Some(bm_protocol::ForgeOutcome::Sealed(item)) => item,
            other => panic!("sealed, not {other:?}"),
        };
        assert_eq!(sealed.forge, mirror.forged());
        assert_eq!(d.inv.count(wax), before + 1, "the second wax comes back");
        let f = station_views(&ev).pop().unwrap().forge.unwrap();
        assert!(f.session.is_none());
        let (queued, progress, left) = f.queued.unwrap();
        assert_eq!((queued, progress), (sealed, 0.0));
        assert!(left >= 120, "two minutes at least");
        // Time passes: it waits in the output, and comes out forged.
        d.run_stations(left as f64 + 1.0);
        let st = &d.stations[&bp(FORGE[0])];
        assert_eq!(st.output, vec![sealed]);
        d.send(ClientMsg::CollectStation { pos });
        d.poll();
        assert!(d.inv.slots.contains(&Some(sealed)), "collected with its traits");
    }

    #[test]
    fn the_demo_forge_queue_runs_out_into_the_output() {
        let mut d = Demo::new("Tester");
        d.poll();
        d.show_forge(ForgeShow::Queue);
        let pos = bp(FORGE[0]);
        d.send(ClientMsg::WatchStation { pos: Some(pos) });
        d.send(ClientMsg::CollectStation { pos });
        d.poll();
        for _ in 0..300 {
            d.run_stations(0.5);
        }
        let v = station_views(&d.events.drain(..).collect::<Vec<_>>()).pop().unwrap();
        let f = v.forge.unwrap();
        assert!(f.queued.is_none(), "{:?}", f.queued);
        assert_eq!(v.output.len(), 1);
    }

    #[test]
    fn the_demo_forge_begins_from_the_pack_and_unmakes() {
        let mut d = Demo::new("Tester");
        d.poll();
        d.show_forge(ForgeShow::Session);
        let pos = bp(FORGE[0]);
        d.stations.get_mut(&pos).unwrap().forge = Some(DemoForge::default());
        let id = |d: &Demo, n: &str| d.items.id(n).unwrap();
        d.inv.slots[30] = Some(ItemStack::new(id(&d, "copper_hammer"), 1));
        d.inv.slots[31] = Some(ItemStack::new(id(&d, "unmaking_wax_1"), 2));
        let pure = (0..40)
            .find(|&i| d.inv.get(i).is_some_and(|s| s.item == id(&d, "pure_tincture_1")))
            .unwrap() as u8;
        let had = d.inv.get(pure as usize).unwrap().count;
        // Refused: forged gear, and stone in the deck.
        d.send(ClientMsg::ForgeBegin {
            pos,
            gear: 1,
            deck: vec![(pure, 5)],
        });
        assert_eq!(notices(&d.poll()), vec!["That cannot be forged".to_string()]);
        d.send(ClientMsg::ForgeBegin {
            pos,
            gear: 30,
            deck: vec![(pure, 5), (31, 1), (2, 1)],
        });
        assert_eq!(
            notices(&d.poll()),
            vec!["Only forging ingredients go in the deck".to_string()]
        );
        d.send(ClientMsg::ForgeBegin {
            pos,
            gear: 30,
            deck: vec![(pure, 5), (31, 1)],
        });
        let ev = d.poll();
        let s = station_views(&ev).pop().unwrap().forge.unwrap().session.unwrap();
        assert_eq!(d.inv.get(30), None, "the gear went in");
        assert_eq!(d.inv.get(pure as usize).unwrap().count, had - 5);
        assert_eq!(d.inv.get(31).unwrap().count, 1);
        let forger = d.character.effects(&d.tree).forger();
        assert_eq!(s.stats, forger.stats(110, 0.0), "the character's own stats");
        assert_eq!(s.deck.len(), 2);
        d.send(ClientMsg::ForgeUse { pos, slot: 0, round: 0 });
        d.poll();
        d.send(ClientMsg::ForgeUse { pos, slot: 1, round: 1 });
        let ev = d.poll();
        match forged_news(&ev).pop() {
            Some(bm_protocol::ForgeOutcome::Unmade { gear, .. }) => assert!(!gear.forge.done),
            other => panic!("unmade, not {other:?}"),
        }
        let hammer = id(&d, "copper_hammer");
        assert!(
            d.inv.slots.iter().flatten().any(|s| s.item == hammer && !s.forge.done),
            "the hammer comes back unforged"
        );
        // The four unused tinctures come back; the used one by a 25% chance.
        let n = d.inv.count(id(&d, "pure_tincture_1"));
        assert!(n == had as u32 - 1 || n == had as u32, "{n} of {had}");
    }
}
