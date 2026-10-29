//! The atlas (milestone 27, `docs/milestones/27-atlas.md`): opening one
//! binds it to the world it is opened on; while it is open the player is
//! sent its world's map tiles near the regions they have discovered, each
//! fogged where they have not ([`crate::atlas::Tile::frame_for`]), and sent
//! again when blocks in them change.

use super::*;
use bm_core::regions::SPACING;
use bm_core::{ItemKind, atlas};
use bm_protocol::AtlasView;

/// A player's open atlas.
pub(super) struct OpenAtlas {
    pub slot: u16,
    pub planet: u16,
    /// Tiles sent (or found to hold nothing of theirs to see).
    pub sent: HashSet<(u16, u16)>,
    /// The resource loaded in it (part 2), and whether its heat map (or its
    /// clearing) has been sent.
    pub heat: Option<ItemId>,
    pub heat_sent: bool,
}

/// Tile jobs running at once, over every planet.
const ATLAS_JOBS: usize = 4;
/// Tiles one player is sent a tick.
const TILES_PER_TICK: usize = 4;
/// An edited tile is drawn again this long after its first edit (half a
/// second), so a run of edits draws it once.
const REDRAW_AFTER: u64 = 30;
/// Tiles leave memory five minutes after the last atlas on the planet
/// closes.
const ATLAS_IDLE_TICKS: u64 = 5 * 60 * 60;

impl Game {
    /// `ClientMsg::OpenAtlas`.
    pub(super) fn open_atlas(&mut self, id: u32, slot: Option<u16>) {
        let Some(slot) = slot else {
            if let Some(p) = self.players.get_mut(&id) {
                p.atlas = None;
            }
            return;
        };
        let Some(p) = self.players.get_mut(&id) else { return };
        let Some(stack) = p.inventory.get(slot as usize) else {
            return;
        };
        if !self
            .items
            .get(stack.item)
            .is_some_and(|d| matches!(d.kind, ItemKind::Atlas))
        {
            return;
        }
        let planet = match atlas::bound(stack.tag) {
            Some(planet) => planet,
            None => {
                // Bound for good to the world it is first opened on.
                let here = self.planets[p.planet].id;
                let before = p.inventory.clone();
                if let Some(s) = p.inventory.slots[slot as usize].as_mut() {
                    s.tag = atlas::pack(Some(here), atlas::loaded(s.tag));
                }
                send_slots(p, &before);
                here
            }
        };
        let Some(pi) = self.planets.iter().position(|w| w.id == planet) else {
            self.notice(id, "The world this atlas maps is gone");
            return;
        };
        let account = p.account.clone();
        let loaded = p.inventory.get(slot as usize).and_then(|s| atlas::loaded(s.tag));
        let view = self.atlas_view(pi, &account, slot, false);
        let Some(p) = self.players.get_mut(&id) else { return };
        p.out.send(&ServerMsg::Atlas(view));
        p.atlas = Some(OpenAtlas {
            slot,
            planet,
            sent: HashSet::new(),
            heat: loaded,
            heat_sent: false,
        });
    }

    fn atlas_view(&self, pi: usize, account: &str, slot: u16, closed: bool) -> AtlasView {
        let planet = &self.planets[pi];
        let sky = &self.skies[pi];
        AtlasView {
            slot,
            planet: planet.id,
            name: planet.name.clone(),
            shape: planet.world.shape,
            layout_seed: sky.layout_seed,
            region_names: sky.names.clone(),
            discovered: self
                .discoveries
                .get(account)
                .and_then(|d| d.get(&planet.id))
                .cloned()
                .unwrap_or_default(),
            shrine: planet.shrine,
            closed,
        }
    }

    /// A region was discovered on planet `pi`: an atlas of it open shows
    /// the new land.
    pub(super) fn atlas_discovered(&mut self, id: u32, pi: usize) {
        let planet = self.planets[pi].id;
        let Some(p) = self.players.get(&id) else { return };
        let Some(slot) = p.atlas.as_ref().filter(|a| a.planet == planet).map(|a| a.slot) else {
            return;
        };
        let view = self.atlas_view(pi, &p.account.clone(), slot, false);
        if let Some(p) = self.players.get_mut(&id) {
            p.out.send(&ServerMsg::Atlas(view));
            if let Some(a) = p.atlas.as_mut() {
                a.sent.clear();
                a.heat_sent = false;
            }
        }
    }

    /// Each tick: notes edits, takes finished tiles in, draws again the
    /// edited ones someone is looking at, and sends each open atlas what it
    /// still lacks.
    pub(super) fn tend_atlases(&mut self) {
        let tick = self.tick;
        for planet in &mut self.planets {
            for (x, z) in planet.world.take_edited() {
                planet.atlas.edited(x, z, tick);
            }
            planet.atlas.receive();
            let changed = planet.atlas.take_changed();
            if !changed.is_empty() {
                for p in self.players.values_mut() {
                    if let Some(a) = p.atlas.as_mut().filter(|a| a.planet == planet.id) {
                        for t in &changed {
                            a.sent.remove(t);
                        }
                    }
                }
            }
        }
        let mut running: usize = self.planets.iter().map(|p| p.atlas.running()).sum();
        for pi in 0..self.planets.len() {
            let planet_id = self.planets[pi].id;
            let viewers: Vec<u32> = self
                .players
                .iter()
                .filter(|(_, p)| p.atlas.as_ref().is_some_and(|a| a.planet == planet_id))
                .map(|(&id, _)| id)
                .collect();
            self.planets[pi].atlas.tend(tick, viewers.len(), ATLAS_IDLE_TICKS);
            // Edited tiles: drawn again if someone has them, else forgotten.
            for tile in self.planets[pi].atlas.take_stale(tick, REDRAW_AFTER) {
                let seen = viewers
                    .iter()
                    .any(|v| self.players[v].atlas.as_ref().is_some_and(|a| a.sent.contains(&tile)));
                if seen {
                    self.draw_tile(pi, tile);
                    running += 1;
                } else {
                    self.planets[pi].atlas.drop_tile(tile);
                }
            }
            for &id in &viewers {
                self.send_heat(pi, id, &mut running);
            }
            for id in viewers {
                let wanted = self.wanted_tiles(pi, &self.players[&id].account);
                let mut sent_now = 0;
                for tile in wanted {
                    if sent_now >= TILES_PER_TICK {
                        break;
                    }
                    if self.players[&id].atlas.as_ref().is_some_and(|a| a.sent.contains(&tile)) {
                        continue;
                    }
                    match self.planets[pi].atlas.get(tile).cloned() {
                        Some(t) => {
                            let discovered = self.discovered_on(&self.players[&id].account, planet_id);
                            let p = self.players.get_mut(&id).expect("listed above");
                            if let Some(frame) = t.frame_for(planet_id, tile, &discovered) {
                                self.stats.bytes_sent += frame.len() as u64;
                                p.out.send_shared(Arc::new(frame));
                                sent_now += 1;
                            }
                            if let Some(a) = p.atlas.as_mut() {
                                a.sent.insert(tile);
                            }
                        }
                        None => {
                            if running < ATLAS_JOBS && !self.planets[pi].atlas.is_building(tile) {
                                self.draw_tile(pi, tile);
                                running += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    fn discovered_on(&self, account: &str, planet: u16) -> Vec<u16> {
        self.discoveries
            .get(account)
            .and_then(|d| d.get(&planet))
            .cloned()
            .unwrap_or_default()
    }

    /// The tiles an atlas of planet `pi` may show this account: those
    /// within reach of a region it has discovered (a region reaches at most
    /// about one and a half region spacings from its centre).
    fn wanted_tiles(&self, pi: usize, account: &str) -> Vec<(u16, u16)> {
        let shape = self.planets[pi].world.shape;
        let n = atlas::tiles_per_side(shape) as i64;
        let reach = (SPACING * 3 / 2) / atlas::TILE_BLOCKS + 1;
        let regions = &self.skies[pi].regions;
        let mut out = Vec::new();
        for r in self.discovered_on(account, self.planets[pi].id) {
            if r as usize >= regions.len() {
                continue;
            }
            let [x, z] = regions.centre(r as usize);
            let (tx, tz) = atlas::tile_of(x as i64, z as i64);
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let t = (
                        (tx as i64 + dx).rem_euclid(n) as u16,
                        (tz as i64 + dz).rem_euclid(n) as u16,
                    );
                    if !out.contains(&t) {
                        out.push(t);
                    }
                }
            }
        }
        // Nearest the discovered centres first: the order they were added.
        out
    }

    /// Sends a viewer the heat map of what their atlas holds, once it is
    /// worked out (or that it holds nothing).
    fn send_heat(&mut self, pi: usize, id: u32, running: &mut usize) {
        let planet = self.planets[pi].id;
        let Some(a) = self.players[&id].atlas.as_ref().filter(|a| !a.heat_sent) else {
            return;
        };
        let Some(item) = a.heat else {
            let p = self.players.get_mut(&id).expect("a viewer");
            p.out.send(&ServerMsg::AtlasHeat {
                planet,
                item: None,
                side: 0,
                bands: Vec::new(),
            });
            p.atlas.as_mut().expect("checked").heat_sent = true;
            return;
        };
        match self.planets[pi].atlas.heat(item).cloned() {
            Some(h) => {
                let discovered = self.discovered_on(&self.players[&id].account, planet);
                let frame = h.frame_for(planet, item, &discovered);
                self.stats.bytes_sent += frame.len() as u64;
                let p = self.players.get_mut(&id).expect("a viewer");
                p.out.send_shared(Arc::new(frame));
                p.atlas.as_mut().expect("checked").heat_sent = true;
            }
            None => {
                if *running < ATLAS_JOBS && !self.planets[pi].atlas.heat_building(item) {
                    let sources: Vec<BlockId> = self.planets[pi]
                        .world
                        .registry
                        .iter()
                        .map(|(b, _)| b)
                        .filter(|&b| self.items.drop_of(b) == Some(item))
                        .collect();
                    let regions = self.skies[pi].regions.clone();
                    self.planets[pi].atlas.request_heat(item, sources, regions);
                    *running += 1;
                }
            }
        }
    }

    /// `ClientMsg::AtlasLoad`: one of what is in `from` goes into the atlas
    /// in `slot` (what it held comes back), or what it holds comes out.
    pub(super) fn atlas_load(&mut self, id: u32, slot: u16, from: Option<u16>) {
        let Some(p) = self.players.get(&id) else { return };
        let Some(atlas_stack) = p.inventory.get(slot as usize) else {
            return;
        };
        if !self
            .items
            .get(atlas_stack.item)
            .is_some_and(|d| matches!(d.kind, ItemKind::Atlas))
        {
            return;
        }
        let held = atlas::loaded(atlas_stack.tag);
        let incoming = match from {
            Some(f) => {
                let Some(s) = p.inventory.get(f as usize).filter(|_| f != slot) else {
                    return;
                };
                if s.colour != Colour::NATURAL {
                    self.notice(id, "An atlas only takes a resource in its natural colour");
                    return;
                }
                if !self.loadable(s.item) {
                    self.notice(
                        id,
                        "An atlas cannot find that: put in an ore, a gem or something gathered",
                    );
                    return;
                }
                Some(s.item)
            }
            None => None,
        };
        if held.is_none() && incoming.is_none() {
            return;
        }
        let p = self.players.get_mut(&id).expect("checked");
        let before = p.inventory.clone();
        if let (Some(f), Some(_)) = (from, incoming) {
            p.inventory.take_one(f as usize);
        }
        if let Some(old) = held
            && p.inventory.add(&self.items, old, 1) > 0
        {
            p.inventory = before;
            self.notice(id, "Your inventory is full");
            return;
        }
        if let Some(s) = p.inventory.slots[slot as usize].as_mut() {
            s.tag = atlas::pack(atlas::bound(s.tag), incoming);
        }
        send_slots(p, &before);
        if let Some(a) = p.atlas.as_mut().filter(|a| a.slot == slot) {
            a.heat = incoming;
            a.heat_sent = false;
        }
    }

    /// Whether an atlas can show where `item` lies: some block drops it.
    fn loadable(&self, item: ItemId) -> bool {
        self.planets.first().is_some_and(|w| {
            w.world
                .registry
                .iter()
                .any(|(b, d)| atlas::finds(d) && self.items.drop_of(b) == Some(item))
        })
    }

    fn draw_tile(&mut self, pi: usize, tile: (u16, u16)) {
        let planet = &mut self.planets[pi];
        let chunks = planet.atlas.tile_chunks(tile);
        let resident = planet.world.snapshots(&chunks);
        let regions = self.skies[pi].regions.clone();
        planet.atlas.request(tile, resident, regions);
    }
}
