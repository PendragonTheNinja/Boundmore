//! `--demo-ui`: a hidden development mode that runs the client with no
//! server, so the menus can be seen and driven headlessly.
//!
//! It stands in for the server with a tiny, honest imitation: a flat patch of
//! ground (a test fixture, not world generation: the real client never makes
//! terrain, ADR-0003), a sample inventory, two stations and two claims. It
//! answers inventory, crafting, station, claim and edit messages with the same
//! `bm-core` rules the real server uses, so drags and crafts behave. It is not
//! a second server: no accounts, no reach or timing checks, no saves.

use crate::net::NetEvent;
use bm_core::{
    BlockId, BlockPos, BlockRegistry, CHUNK_SIZE, Chunk, ChunkPos, DayCycle, Inventory, ItemRegistry, ItemStack,
    LocalPos, MoveConfig, PlanetShape, PlayerState, PlotPos, RecipeBook, Station,
};
use bm_protocol::{ClaimView, ClientMsg, EditAction, JobView, ServerMsg, StationView, Welcome};
use glam::DVec3;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

pub const SPAWN: [f64; 3] = [128.5, 44.0, 128.5];
const GROUND: i64 = 43;
const SHAPE: (u32, u32) = (8, 4);

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
    edits: HashMap<BlockPos, BlockId>,
    stations: HashMap<BlockPos, DemoStation>,
    watched: Option<BlockPos>,
    claims: Vec<ClaimView>,
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
                item: id(name),
                count,
                wear,
            });
        }
        let b = |n: &str| blocks.id(n).expect("builtin block");
        let mut edits = HashMap::new();
        let mut put = |x, y, z, block| {
            edits.insert(BlockPos::new(x, y, z), block);
        };
        put(128, 44, 126, b("workbench"));
        put(130, 44, 126, b("furnace"));
        put(126, 44, 131, b("claim_stone"));
        put(124, 44, 125, b("glow_crystal"));
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
        let mut d = Self {
            account: account.to_string(),
            events: VecDeque::new(),
            blocks,
            items,
            book,
            inv,
            edits,
            stations,
            watched: None,
            claims,
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
            planet: shape(),
            planet_name: "Demo".into(),
            blocks: self.blocks.defs().to_vec(),
            items: self.items.defs().to_vec(),
            recipes: self.book.recipes.clone(),
            inventory: self.inv.clone(),
            movement: MoveConfig::default(),
            spawn: PlayerState {
                pos: DVec3::from_array(SPAWN),
                vel: DVec3::ZERO,
                on_ground: true,
            },
            tick: 0,
            view_radius: 4,
            reach: 7.0,
            day: DayCycle::default(),
            claims: self.claims.clone(),
            plot_allowance: 64,
        };
        self.events.push_back(NetEvent::Connected(Box::new(w)));
        // Whole columns, nearest first: light needs complete columns.
        let centre = BlockPos::new(SPAWN[0] as i64, 0, SPAWN[2] as i64).chunk();
        let mut cols: Vec<(i32, i32)> = (-3..=3).flat_map(|dx| (-3..=3).map(move |dz| (dx, dz))).collect();
        cols.sort_by_key(|(dx, dz)| dx * dx + dz * dz);
        for (dx, dz) in cols {
            for cy in 0..SHAPE.1 as i32 {
                let pos = ChunkPos::new(centre.x + dx, cy, centre.z + dz);
                let data = self.make_chunk(pos).encode();
                self.events.push_back(NetEvent::Msg(ServerMsg::Chunk { pos, data }));
            }
        }
    }

    fn ground(&self, p: BlockPos) -> BlockId {
        if let Some(b) = self.edits.get(&p) {
            return *b;
        }
        let b = |n: &str| self.blocks.id(n).unwrap_or(BlockId::AIR);
        // Gentle rolling ground away from the spawn clearing.
        let (dx, dz) = (p.x as f64 - SPAWN[0], p.z as f64 - SPAWN[2]);
        let far = ((dx * dx + dz * dz).sqrt() - 10.0).clamp(0.0, 6.0) / 6.0;
        let h = GROUND + (far * ((p.x as f64 * 0.21).sin() * 1.6 + (p.z as f64 * 0.17).cos() * 1.6)).round() as i64;
        match p.y {
            0 => b("bedrock"),
            y if y < h - 3 => b("stone"),
            y if y < h => b("soil"),
            y if y == h => b("turf"),
            _ => BlockId::AIR,
        }
    }

    fn make_chunk(&self, pos: ChunkPos) -> Chunk {
        let o = pos.origin();
        let mut c = Chunk::empty();
        let n = CHUNK_SIZE as u8;
        for y in 0..n {
            for z in 0..n {
                for x in 0..n {
                    let block = self.ground(o.offset(x as i64, y as i64, z as i64));
                    if !block.is_air() {
                        c.set(LocalPos::new(x, y, z), block);
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
            ClientMsg::StartBreak { .. } | ClientMsg::Hello { .. } => {}
        }
        self.slots_since(&before);
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

    fn edit(&mut self, seq: u32, pos: BlockPos, action: EditAction) {
        let pos = shape().wrap_block(pos);
        let old = self.ground(pos);
        let (block, accepted) = match action {
            EditAction::Break => match self.items.drop_of(old) {
                Some(item) if self.inv.add_all(&self.items, item, 1).is_none() => {
                    self.notice("Your inventory is full");
                    (old, false)
                }
                _ => (BlockId::AIR, true),
            },
            EditAction::Place { slot } => {
                let block = self
                    .inv
                    .get(slot as usize)
                    .and_then(|s| self.items.get(s.item))
                    .and_then(|d| d.block());
                match block {
                    Some(b) if old.is_air() => {
                        self.inv.take_one(slot as usize);
                        (b, true)
                    }
                    _ => (old, false),
                }
            }
        };
        if accepted {
            self.edits.insert(pos, block);
        }
        self.events.push_back(NetEvent::Msg(ServerMsg::EditResult {
            seq,
            pos,
            block,
            accepted,
        }));
    }
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
}
