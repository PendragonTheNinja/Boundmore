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

use crate::net::NetEvent;
use bm_core::{
    BlockId, BlockPos, BlockRegistry, BlockState, CHUNK_SIZE, Chunk, ChunkPos, Colour, ColourPalette, DayCycle,
    Inventory, ItemRegistry, ItemStack, LocalPos, MoveConfig, PlanetLook, PlanetShape, PlayerState, PlotPos,
    RecipeBook, Station,
};
use bm_protocol::{
    Channel, ChatLine, ClaimView, ClientMsg, EditAction, JobView, MAX_CHAT_LEN, PlanetInfo, PlanetSummary, ServerMsg,
    StationView, StorageView, Travel, Welcome,
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
/// The west end of the grapple demo's arch beam, which runs ten blocks east.
pub const GRAPPLE_BEAM: [i64; 3] = [124, 56, 118];
/// The demo's planets: name, kind, and the width shown for it in the sky and
/// menu. (The demo's own patch is tiny; these are sizes real planets have.)
const PLANETS: [(&str, &str, u32); 4] = [
    ("Hearthmoor", "temperate", 2048),
    ("Saffra", "arid", 3072),
    ("Rimeholt", "frozen", 2560),
    ("Verdance", "lush", 4096),
];
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

struct DemoStation {
    station: Station,
    jobs: Vec<JobView>,
    output: Vec<ItemStack>,
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
    watched: Option<BlockPos>,
    claims: Vec<ClaimView>,
    /// Storage blocks' contents by position (first planet only).
    chests: HashMap<BlockPos, Vec<Option<ItemStack>>>,
    open_chest: Option<BlockPos>,
    /// What waits in the reclaim, as a server keeps it per account.
    reclaim: Vec<ItemStack>,
    colours: ColourPalette,
    last: Instant,
    push_timer: f64,
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
        let sample: [(&str, u16, u16); 17] = [
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
        stations.insert(
            BlockPos::new(128, 44, 126),
            DemoStation {
                station: Station::Workbench,
                jobs: vec![JobView {
                    recipe: recipe_index(&book, &items, "stone_axe"),
                    remaining: 3,
                    progress: 0.35,
                }],
                output: vec![ItemStack::new(id("stone_shovel"), 1), ItemStack::new(id("furnace"), 1)],
            },
        );
        stations.insert(
            BlockPos::new(130, 44, 126),
            DemoStation {
                station: Station::Furnace,
                jobs: vec![],
                output: vec![],
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
            },
            ClaimView {
                id: 2,
                owner: "Wren".into(),
                stone: BlockPos::new(150, 44, 118),
                plots: theirs,
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
            watched: None,
            claims,
            chests,
            open_chest: None,
            reclaim: Vec::new(),
            colours: ColourPalette::builtin(),
            last: Instant::now(),
            push_timer: 0.0,
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
            inventory: self.inv.clone(),
            movement: MoveConfig::default(),
            spawn: PlayerState {
                pos: DVec3::from_array(SPAWN),
                vel: DVec3::ZERO,
                on_ground: true,
                hook: None,
            },
            tick: 0,
            view_radius: 4,
            reach: 7.0,
            claims: self.claims.clone(),
            plot_allowance: 64,
        };
        self.events.push_back(NetEvent::Connected(Box::new(w)));
        let chunks = self.chunks_around_spawn();
        self.events.extend(chunks.into_iter().map(NetEvent::Msg));
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
            },
            tick: 0,
            claims,
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
        if let Some(pos) = self.watched
            && let Some(s) = self.stations.get(&pos)
        {
            let view = StationView {
                pos,
                station: s.station,
                jobs: s.jobs.clone(),
                output: s.output.clone(),
            };
            self.events.push_back(NetEvent::Msg(ServerMsg::Station(view)));
        }
    }

    fn run_stations(&mut self, dt: f64) {
        let mut changed = false;
        for s in self.stations.values_mut() {
            let Some(job) = s.jobs.first_mut() else { continue };
            let Some(r) = self.book.get(job.recipe) else { continue };
            job.progress += (dt / r.seconds.max(0.1) as f64) as f32;
            changed = true;
            if job.progress >= 1.0 {
                job.progress = 0.0;
                job.remaining -= 1;
                let (out, n) = (r.output, r.output_count);
                match s.output.iter_mut().find(|o| o.item == out) {
                    Some(o) => o.count += n,
                    None => s.output.push(ItemStack::new(out, n)),
                }
                if job.remaining == 0 {
                    s.jobs.remove(0);
                }
            }
        }
        self.push_timer += dt;
        if changed && self.push_timer > 0.5 {
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
            ClientMsg::Craft { recipe, count, station } => self.craft(recipe, count, station),
            ClientMsg::WatchStation { pos } => {
                self.watched = pos;
                self.push_station();
            }
            ClientMsg::CollectStation { pos } => {
                if let Some(s) = self.stations.get_mut(&pos) {
                    let mut left = Vec::new();
                    for o in s.output.drain(..) {
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
            // M8: portals (ADR-0009), not built yet.
            ClientMsg::OpenPortal { .. } | ClientMsg::Portal { .. } => {}
            ClientMsg::StartBreak { .. } | ClientMsg::Hello { .. } => {}
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

    fn craft(&mut self, recipe: u16, count: u16, station: Option<BlockPos>) {
        let Some(r) = self.book.get(recipe).cloned() else {
            return;
        };
        let mut made = 0;
        for _ in 0..count {
            let mut trial = self.inv.clone();
            if r.inputs.iter().any(|(i, n)| trial.remove(*i, *n).is_none()) {
                break;
            }
            if r.station == Station::Hand && trial.add_all(&self.items, r.output, r.output_count).is_none() {
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
            && let Some(s) = station.and_then(|p| self.stations.get_mut(&p))
        {
            s.jobs.push(JobView {
                recipe,
                remaining: made,
                progress: 0.0,
            });
            self.push_station();
        }
    }

    /// One use of the chisel in `slot`; it breaks at its durability.
    fn wear_chisel(&mut self, slot: usize) {
        let before = self.inv.clone();
        if let Some(s) = self.inv.slots[slot].as_mut() {
            s.wear += 1;
            if let Some(def) = self.items.get(s.item)
                && def.durability().is_some_and(|d| s.wear >= d)
            {
                let text = format!("Your {} broke", def.display);
                self.inv.slots[slot] = None;
                self.notice(&text);
            }
        }
        self.slots_since(&before);
    }

    fn edit(&mut self, seq: u32, pos: BlockPos, action: EditAction) {
        let pos = shape().wrap_block(pos);
        let old_state = self.ground(pos);
        let old = old_state.id;
        let full_chest = self.here == 0 && self.chests.get(&pos).is_some_and(|c| c.iter().any(Option::is_some));
        let (block, accepted) = match action {
            EditAction::Break if full_chest => {
                let name = bm_core::items::title_case(&self.blocks.get(old).name);
                self.notice(&format!("Empty the {name} before breaking it"));
                (old_state, false)
            }
            EditAction::Break => {
                // A block that yields itself keeps its colour (ADR-0007).
                let drop = self.items.drop_of(old).map(|item| {
                    let keeps = self.items.get(item).and_then(|d| d.block()) == Some(old);
                    (item, if keeps { old_state.colour } else { Colour::NATURAL })
                });
                match drop {
                    Some((item, colour)) if self.inv.add_all_coloured(&self.items, item, colour, 1).is_none() => {
                        self.notice("Your inventory is full");
                        (old_state, false)
                    }
                    _ => (BlockState::AIR, true),
                }
            }
            EditAction::Place { slot } => {
                let stack = self.inv.get(slot as usize);
                let block = stack.and_then(|s| self.items.get(s.item)).and_then(|d| d.block());
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
                let chisel = self
                    .inv
                    .get(slot as usize)
                    .and_then(|s| self.items.get(s.item))
                    .and_then(|d| d.chisel());
                match chisel {
                    Some((family, precise, _)) => {
                        match bm_core::chisel::apply(self.blocks.get(old), old_state, family, precise, octants, restore)
                        {
                            Ok(shape) => {
                                self.wear_chisel(slot as usize);
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
        };
        if accepted {
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
                put(128, 44, 126, b("workbench"));
                put(130, 44, 126, b("furnace"));
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
                    color: k.look.color,
                    width_blocks: *width,
                    players: (i == 0) as u16 + [0, 3, 0, 7][i],
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
        d.send(ClientMsg::Edit {
            seq: 1,
            at: 0,
            pos,
            action: EditAction::Break,
        });
        let ev = d.poll();
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
            action: EditAction::Place { slot: slot as u8 },
        });
        let placed = d.poll().into_iter().find_map(|e| match e {
            NetEvent::Msg(ServerMsg::EditResult { block, accepted, .. }) => Some((block, accepted)),
            _ => None,
        });
        let (block, accepted) = placed.unwrap();
        assert!(accepted);
        assert_eq!(block.colour, stack.colour);
        d.send(ClientMsg::Edit {
            seq: 2,
            at: 0,
            pos,
            action: EditAction::Break,
        });
        d.poll();
        assert_eq!(
            d.inv.slots[slot].unwrap().count,
            stack.count,
            "it came back in its colour"
        );
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
}
