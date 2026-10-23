//! Crafting stations in play (milestone 11): the crafting table, four-block
//! machines and the furnace, as `machines` assembles them; queues, output,
//! fuel and heat live in `crafting::Stations`, at each machine's key block.
//!
//! Milestone 15: spark generators (one fuel slot and one store per joined
//! group), spark links ([`PlanetEnv`] answers where they lead), and wear.

use super::{Game, Refusal, send_slots, station_name};
use crate::crafting::{self, CraftError, MAX_CRAFT_COUNT, Surroundings};
use crate::machines::{self, Machine};
use crate::planet::Planet;
use crate::world::World;
use bm_core::spark;
use bm_core::{BlockId, BlockPos, INVENTORY_SLOTS, ItemStack, Station};
use bm_protocol::{ServerMsg, SparkView, StationView};

/// Whether placing a spark generator `id` at `canon` would join groups that
/// each hold fuel: joined generators share one fuel slot.
pub(super) fn joins_fuelled_generators(planet: &Planet, canon: BlockPos, id: BlockId) -> bool {
    let w = &planet.world;
    let with = |p: BlockPos| if p == canon { Some(id) } else { w.block(p) };
    let Some(m) = machines::machine_at(&w.registry, w.shape, with, canon) else {
        return false;
    };
    m.blocks
        .iter()
        .filter(|&&b| planet.stations.get(b).is_some_and(|s| s.fuel.is_some()))
        .count()
        > 1
}

/// A planet's world as the stations ask about it: where a machine's links
/// lead, what a furnace's base is.
pub(crate) struct PlanetEnv<'a> {
    world: &'a World,
    link: BlockId,
}

impl<'a> PlanetEnv<'a> {
    pub(crate) fn new(world: &'a World, link: BlockId) -> Self {
        Self { world, link }
    }

    fn machine(&self, canon: BlockPos) -> Option<Machine> {
        let w = self.world;
        machines::machine_at(&w.registry, w.shape, |p| w.block(p), canon)
    }
}

impl Surroundings for PlanetEnv<'_> {
    fn generators(&self, machine: BlockPos) -> Option<Vec<BlockPos>> {
        let w = self.world;
        let m = self.machine(machine)?;
        let blocks = machines::grid_generators(&w.registry, w.shape, |p| w.block(p), &m.blocks, self.link)?;
        let mut keys: Vec<BlockPos> = Vec::new();
        let mut covered: Vec<BlockPos> = Vec::new();
        for b in blocks {
            if covered.contains(&b) {
                continue;
            }
            let g = self.machine(b)?;
            covered.extend(g.blocks.iter().copied());
            if !keys.contains(&g.key) {
                keys.push(g.key);
            }
        }
        Some(keys)
    }

    fn heat_share(&self, furnace: BlockPos) -> Option<f32> {
        self.world.block(furnace).map(|b| self.world.registry.get(b).heat_share)
    }
}

/// "a Workbench", "an Extractor".
fn a_station(kind: Station) -> String {
    let name = station_name(kind);
    match name.chars().next() {
        Some('A' | 'E' | 'I' | 'O' | 'U') => format!("an {name}"),
        _ => format!("a {name}"),
    }
}

impl Game {
    /// The machine the block at `canon` (canonical, resident) belongs to on
    /// planet `pi`.
    pub(super) fn machine(&self, pi: usize, canon: BlockPos) -> Option<Machine> {
        let world = &self.planets[pi].world;
        machines::machine_at(&world.registry, world.shape, |p| world.block(p), canon)
    }

    /// The machine at `pos` if this player may use it (the rules of
    /// `block_access`), with the planet it is on.
    pub(super) fn machine_access(&self, id: u32, pos: BlockPos) -> Result<(usize, Machine), Refusal> {
        let (canon, _) = self.block_access(id, pos)?;
        let pi = self.planet_of(id).ok_or("no such player")?;
        let m = self.machine(pi, canon).ok_or("not a station")?;
        Ok((pi, m))
    }

    /// Where a working machine's record is: its key. A station saved at one
    /// block of a four-block machine before milestone 11 moves there when
    /// its group forms (if the key has none; any other waits its turn, its
    /// block unbreakable, and its jobs keep running).
    pub(super) fn machine_record(&mut self, pi: usize, m: &Machine) -> BlockPos {
        let stations = &mut self.planets[pi].stations;
        if m.idle.is_none()
            && !stations.exists(m.key)
            && let Some(&from) = m.blocks.iter().find(|&&b| stations.exists(b))
        {
            tracing::info!("station at {from:?} is now its machine's, at {:?}", m.key);
            stations.relocate(from, m.key);
        }
        m.key
    }

    /// Whether anything of the machine the block at `canon` belongs to stops
    /// it being broken: the station's name if so.
    pub(super) fn machine_contents(&self, pi: usize, canon: BlockPos) -> Option<&'static str> {
        let stations = &self.planets[pi].stations;
        if stations.has_contents(canon) {
            return Some(station_name(stations.get(canon)?.kind));
        }
        let m = self.machine(pi, canon)?;
        m.blocks
            .iter()
            .chain(std::iter::once(&m.key))
            .find(|&&b| stations.has_contents(b))
            .map(|_| station_name(m.kind))
    }

    pub(super) fn craft(&mut self, id: u32, index: u16, count: u16, station: Option<BlockPos>) {
        let Some(recipe) = self.recipes.get(index).cloned() else {
            return self.tell(id, "no such recipe".into());
        };
        if count == 0 || count > MAX_CRAFT_COUNT {
            return self.notice(id, &CraftError::TooMany.to_string());
        }
        // Crafting skills gate some recipes (milestone 12).
        if let Some(why) = self.craft_gate(id, &recipe) {
            return self.notice(id, &why);
        }
        // In the hands, or at any station: instant into the pack, and its
        // craft XP at once.
        if recipe.station == Station::Hand {
            match self.change_inventory(id, |inv, items| crafting::craft_by_hand(inv, items, &recipe, count)) {
                Some(Err(e)) => self.notice(id, &e.to_string()),
                Some(Ok(())) => self.add_xp(id, recipe.xp as u64 * count as u64),
                None => {}
            }
            return;
        }
        let needs = format!("That is made at {}", a_station(recipe.station));
        let Some(pos) = station else {
            return self.notice(id, &needs);
        };
        let (pi, m) = match self.machine_access(id, pos) {
            Ok(found) => found,
            Err(r) => return self.tell(id, r),
        };
        if m.kind != recipe.station {
            return self.notice(id, &needs);
        }
        if let Some(why) = &m.idle {
            return self.notice(id, why);
        }
        let key = self.machine_record(pi, &m);
        if !self.planets[pi].stations.has_room(key) {
            return self.notice(id, &CraftError::QueueFull.to_string());
        }
        let containers = match self.change_inventory(id, |inv, items| {
            crafting::take_inputs_keeping(inv, items, &recipe, count)
        }) {
            Some(Some(c)) => c,
            Some(None) => return self.notice(id, &CraftError::Materials.to_string()),
            None => return,
        };
        let planet = &mut self.planets[pi];
        let env = PlanetEnv::new(&planet.world, self.spark_link);
        if let Err(e) = planet.stations.queue(
            key,
            m.kind,
            index,
            count,
            containers,
            self.tick,
            &self.items,
            &self.recipes,
            &env,
        ) {
            // Unreachable (room was checked), but never take inputs silently.
            tracing::error!("queueing at {key:?} after taking inputs: {e}");
        }
        self.push_station(pi, key);
    }

    pub(super) fn watch(&mut self, id: u32, pos: Option<BlockPos>) {
        let target = match pos.map(|pos| self.machine_access(id, pos)) {
            None => None,
            Some(Ok((pi, m))) => Some(self.machine_record(pi, &m)),
            Some(Err(r)) => {
                self.tell(id, r);
                None
            }
        };
        let Some(pi) = self.planet_of(id) else { return };
        let account = self.players.get(&id).map(|p| p.account.clone());
        let view = target.and_then(|key| self.station_view(pi, key, account.as_deref()));
        let Some(p) = self.players.get_mut(&id) else { return };
        p.watching = target;
        if let Some(view) = view {
            p.out.send(&ServerMsg::Station(view));
        }
    }

    pub(super) fn collect(&mut self, id: u32, pos: BlockPos) {
        let (pi, m) = match self.machine_access(id, pos) {
            Ok(found) => found,
            Err(r) => return self.tell(id, r),
        };
        let key = self.machine_record(pi, &m);
        let Some(p) = self.players.get_mut(&id) else { return };
        let stations = &mut self.planets[pi].stations;
        let before = p.inventory.clone();
        // Station crafts give their XP to whoever collects them.
        let collected = stations.collect_xp(key, &mut p.inventory, &self.items);
        send_slots(p, &before);
        if let Some(xp) = collected {
            self.push_station(pi, key);
            self.add_xp(id, xp as u64);
        } else if stations
            .get(key)
            .is_some_and(|s| s.output.slots.iter().any(Option::is_some))
        {
            self.notice(id, "Your inventory is full");
        }
    }

    /// Moves fuel between inventory `slot` and a furnace's or a spark
    /// generator's fuel slot.
    pub(super) fn station_fuel(&mut self, id: u32, pos: BlockPos, slot: u8, count: u16, into: bool) {
        let (pi, m) = match self.machine_access(id, pos) {
            Ok(found) => found,
            Err(r) => return self.tell(id, r),
        };
        if !matches!(m.kind, Station::Furnace | Station::SparkGenerator) {
            return self.tell(id, "only a furnace or a spark generator takes fuel".into());
        }
        let slot = slot as usize;
        if slot >= INVENTORY_SLOTS || count == 0 {
            return self.tell(id, "no such slot".into());
        }
        let key = self.machine_record(pi, &m);
        if into {
            if let Some(why) = &m.idle {
                return self.notice(id, why);
            }
            let Some(stack) = self.players.get(&id).and_then(|p| p.inventory.get(slot)) else {
                return;
            };
            if self.items.get(stack.item).is_none_or(|d| d.fuel == 0) {
                return self.notice(id, "That does not burn");
            }
            let offer = ItemStack {
                count: count.min(stack.count),
                ..stack
            };
            let planet = &mut self.planets[pi];
            let env = PlanetEnv::new(&planet.world, self.spark_link);
            let put = planet
                .stations
                .add_fuel(key, m.kind, offer, self.tick, &self.items, &self.recipes, &env);
            if m.kind == Station::SparkGenerator {
                planet.stations.set_generator_blocks(key, m.blocks.len());
            }
            if put == 0 {
                return self.notice(id, "The fuel slot is full");
            }
            self.change_inventory(id, |inv, _| {
                if let Some(s) = &mut inv.slots[slot] {
                    s.count -= put;
                    if s.count == 0 {
                        inv.slots[slot] = None;
                    }
                }
            });
        } else {
            let Some(taken) = self.planets[pi].stations.take_fuel(key, count) else {
                return;
            };
            let left = self
                .change_inventory(id, |inv, items| {
                    // Into the slot asked for when it can hold it, else anywhere.
                    let fits = match &mut inv.slots[slot] {
                        None => {
                            inv.slots[slot] = Some(taken);
                            true
                        }
                        Some(s)
                            if s.item == taken.item
                                && s.colour == taken.colour
                                && s.tag == taken.tag
                                && s.wear == taken.wear
                                && items.get(s.item).is_some_and(|d| s.count + taken.count <= d.stack) =>
                        {
                            s.count += taken.count;
                            true
                        }
                        Some(_) => false,
                    };
                    if fits {
                        0
                    } else {
                        inv.add_coloured(items, taken.item, taken.colour, taken.count)
                    }
                })
                .unwrap_or(taken.count);
            if left > 0 {
                self.planets[pi]
                    .stations
                    .return_fuel(key, m.kind, ItemStack { count: left, ..taken });
                self.notice(id, "Your inventory is full");
            }
        }
        self.push_station(pi, key);
    }

    /// A station's window as the player with account `viewer` sees it (a
    /// forge's session shows only to its forger).
    pub(super) fn station_view(&self, pi: usize, key: BlockPos, viewer: Option<&str>) -> Option<StationView> {
        let planet = &self.planets[pi];
        let machine = planet.world.block(key).and_then(|_| self.machine(pi, key));
        let kind = match (planet.stations.get(key), &machine) {
            (Some(s), _) => s.kind,
            (None, Some(m)) => m.kind,
            (None, None) => return None,
        };
        let blocks = machine.as_ref().map_or(1, |m| m.blocks.len());
        let idle = machine.and_then(|m| m.idle);
        let mut view = planet.stations.view(key, kind, idle, self.tick, &self.recipes);
        view.spark = self.spark_view(pi, key, kind, blocks);
        if kind == Station::Forge {
            view.forge = Some(Box::new(self.forge_view(pi, key, viewer)));
        }
        Some(view)
    }

    /// Spark as the window of the station keyed at `key` shows it: a
    /// generator's own store, or what a machine that can need spark reaches.
    fn spark_view(&self, pi: usize, key: BlockPos, kind: Station, blocks: usize) -> Option<SparkView> {
        let planet = &self.planets[pi];
        match kind {
            Station::SparkGenerator => {
                let st = planet.stations.get(key);
                Some(SparkView {
                    stored: st.map_or(0, |s| s.spark),
                    capacity: spark::capacity(blocks),
                    linked: true,
                    burning: st.map_or(0, |s| s.burn_left()),
                })
            }
            Station::Workbench | Station::Mixer | Station::Refinery | Station::Compactor | Station::Forge => {
                let env = PlanetEnv::new(&planet.world, self.spark_link);
                let grid = env.generators(key).unwrap_or_default();
                let capacity = grid
                    .iter()
                    .map(|&g| env.machine(g).map_or(1, |m| m.blocks.len()))
                    .map(spark::capacity)
                    .fold(0u32, u32::saturating_add);
                Some(SparkView {
                    stored: planet.stations.grid_spark(&grid),
                    capacity,
                    linked: !grid.is_empty(),
                    burning: 0,
                })
            }
            _ => None,
        }
    }

    /// A spark generator block was placed at `canon`: the groups it joined
    /// become one, their stores added up at the new group's key.
    pub(super) fn generator_placed(&mut self, pi: usize, canon: BlockPos) {
        let Some(m) = self.machine(pi, canon).filter(|m| m.kind == Station::SparkGenerator) else {
            return;
        };
        let stations = &mut self.planets[pi].stations;
        stations.join_generators(m.key, &m.blocks, m.blocks.len());
        // Whoever watched one of the joined groups now watches the whole.
        for p in self.players.values_mut().filter(|p| p.planet == pi) {
            if p.watching.is_some_and(|w| m.blocks.contains(&w)) {
                p.watching = Some(m.key);
            }
        }
        self.push_station(pi, m.key);
    }

    /// A block of `old` (the machine as it stood) has been broken at
    /// `canon` by `by`. A generator's share of its group's spark is refunded
    /// to the breaker as spark items (1,000 each, rounded down; what does
    /// not fit is lost) and the rest stays with what is left; a machine that
    /// wears forgets its record.
    pub(super) fn machine_broken(&mut self, pi: usize, old: Machine, canon: BlockPos, by: Option<u32>) {
        if old.kind == Station::SparkGenerator {
            let mut groups: Vec<(BlockPos, usize)> = Vec::new();
            let mut covered: Vec<BlockPos> = vec![canon];
            for &b in &old.blocks {
                if covered.contains(&b) {
                    continue;
                }
                if let Some(g) = self.machine(pi, b) {
                    covered.extend(g.blocks.iter().copied());
                    groups.push((g.key, g.blocks.len()));
                }
            }
            let share = self.planets[pi]
                .stations
                .break_generator(old.key, old.blocks.len(), &groups);
            let refund = share / spark::SPARK_PER_ITEM;
            if refund > 0
                && let Some(id) = by
                && let Some(p) = self.players.get_mut(&id)
            {
                let before = p.inventory.clone();
                let mut left = refund;
                while left > 0 {
                    let n = left.min(u16::MAX as u32) as u16;
                    if p.inventory.add(&self.items, self.spark_item, n) > 0 {
                        break;
                    }
                    left -= n as u32;
                }
                send_slots(p, &before);
            }
            for (g, _) in groups {
                self.push_station(pi, g);
            }
        } else if spark::wears(old.kind) {
            self.planets[pi].stations.forget(old.key);
        }
    }

    /// A spanner swung at the block at `canon`: if it belongs to a working
    /// machine that wears, removes `amount` of its wear; returns the
    /// machine's key and the wear it has now, or None if it is no such
    /// machine.
    pub(super) fn repair_machine(&mut self, pi: usize, canon: BlockPos, amount: u32) -> Option<(BlockPos, u32)> {
        let m = self
            .machine(pi, canon)
            .filter(|m| spark::wears(m.kind) && m.idle.is_none())?;
        let key = self.machine_record(pi, &m);
        let planet = &mut self.planets[pi];
        let env = PlanetEnv::new(&planet.world, self.spark_link);
        let wear = planet
            .stations
            .repair(key, m.kind, amount, self.tick, &self.items, &self.recipes, &env);
        self.push_station(pi, key);
        Some((key, wear))
    }

    /// Sends a station's state to everyone on its planet watching it.
    pub(super) fn push_station(&self, pi: usize, key: BlockPos) {
        let mut view: Option<StationView> = None;
        for p in self
            .players
            .values()
            .filter(|p| p.planet == pi && p.watching == Some(key))
        {
            if view.is_none() {
                view = self.station_view(pi, key, Some(&p.account));
            }
            let Some(v) = &mut view else { continue };
            // Only a forge's view differs by who watches.
            if let Some(f) = &mut v.forge {
                **f = self.forge_view(pi, key, Some(&p.account));
            }
            p.out.send(&ServerMsg::Station(v.clone()));
        }
    }
}
