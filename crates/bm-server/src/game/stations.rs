//! Crafting stations in play (milestone 11): the crafting table, four-block
//! machines and the furnace, as `machines` assembles them; queues, output,
//! fuel and heat live in `crafting::Stations`, at each machine's key block.

use super::{Game, Refusal, send_slots, station_name};
use crate::crafting::{self, CraftError, MAX_CRAFT_COUNT};
use crate::machines::{self, Machine};
use bm_core::{BlockPos, INVENTORY_SLOTS, ItemStack, Station};
use bm_protocol::{ServerMsg, StationView};

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
    fn machine_access(&self, id: u32, pos: BlockPos) -> Result<(usize, Machine), Refusal> {
        let (canon, _) = self.block_access(id, pos)?;
        let pi = self.planet_of(id).ok_or("no such player")?;
        let m = self.machine(pi, canon).ok_or("not a station")?;
        Ok((pi, m))
    }

    /// Where a working machine's record is: its key. A station saved at one
    /// block of a four-block machine before milestone 11 moves there when
    /// its group forms (if the key has none; any other waits its turn, its
    /// block unbreakable, and its jobs keep running).
    fn machine_record(&mut self, pi: usize, m: &Machine) -> BlockPos {
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
        // In the hands, or at any station: instant into the pack.
        if recipe.station == Station::Hand {
            if let Some(Err(e)) =
                self.change_inventory(id, |inv, items| crafting::craft_by_hand(inv, items, &recipe, count))
            {
                self.notice(id, &e.to_string());
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
        match self.change_inventory(id, |inv, _| crafting::take_inputs(inv, &recipe, count)) {
            Some(true) => {}
            Some(false) => return self.notice(id, &CraftError::Materials.to_string()),
            None => return,
        }
        if let Err(e) =
            self.planets[pi]
                .stations
                .queue(key, m.kind, index, count, self.tick, &self.items, &self.recipes)
        {
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
        let view = target.and_then(|key| self.station_view(pi, key));
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
        let moved = stations.collect(key, &mut p.inventory, &self.items);
        send_slots(p, &before);
        if moved {
            self.push_station(pi, key);
        } else if stations
            .get(key)
            .is_some_and(|s| s.output.slots.iter().any(Option::is_some))
        {
            self.notice(id, "Your inventory is full");
        }
    }

    /// Moves fuel between inventory `slot` and a furnace's fuel slot.
    pub(super) fn station_fuel(&mut self, id: u32, pos: BlockPos, slot: u8, count: u16, into: bool) {
        let (pi, m) = match self.machine_access(id, pos) {
            Ok(found) => found,
            Err(r) => return self.tell(id, r),
        };
        if m.kind != Station::Furnace {
            return self.tell(id, "only a furnace takes fuel".into());
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
            let put = self.planets[pi]
                .stations
                .add_fuel(key, m.kind, offer, self.tick, &self.items, &self.recipes);
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

    pub(super) fn station_view(&self, pi: usize, key: BlockPos) -> Option<StationView> {
        let planet = &self.planets[pi];
        let machine = planet.world.block(key).and_then(|_| self.machine(pi, key));
        let kind = match (planet.stations.get(key), &machine) {
            (Some(s), _) => s.kind,
            (None, Some(m)) => m.kind,
            (None, None) => return None,
        };
        let idle = machine.and_then(|m| m.idle);
        Some(planet.stations.view(key, kind, idle, self.tick, &self.recipes))
    }

    /// Sends a station's state to everyone on its planet watching it.
    pub(super) fn push_station(&self, pi: usize, key: BlockPos) {
        let mut view = None;
        for p in self
            .players
            .values()
            .filter(|p| p.planet == pi && p.watching == Some(key))
        {
            if view.is_none() {
                view = self.station_view(pi, key);
            }
            if let Some(v) = &view {
                p.out.send(&ServerMsg::Station(v.clone()));
            }
        }
    }
}
