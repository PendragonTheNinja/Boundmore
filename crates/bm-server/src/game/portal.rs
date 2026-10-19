//! Portals in play (milestone 8, ADR-0009): the window, its actions, open
//! and dormant surfaces, walking through, charging, and what claims and
//! breaking do to them. The records themselves live in `crate::portals`.

use super::{Game, Refusal, in_reach, send_slots};
use crate::portals::{Charge, frame_blocks, landmark};
use bm_core::movement::{PhysicsWorld, PlayerState};
use bm_core::portal::{PortalFrame, SHAPE_RULE, find_frame};
use bm_core::{BlockId, BlockPos, BlockState, ChunkPos, ItemStack, PlotPos};
use bm_protocol::{PortalAction, PortalView, ServerMsg};
use glam::DVec3;
use std::collections::BTreeSet;

/// How long a traveller is held at a portal waiting for the far side's
/// chunks before the trip is given up (10 s).
const TRIP_WAIT_TICKS: u64 = 600;
/// Charges older than the cooldown are forgotten this often (once a minute).
const FORGET_CHARGES_EVERY: u64 = 3600;

/// A trip under way: the traveller is held where they stepped in until the
/// far side's chunks are resident (the game loop never waits for them).
#[derive(Clone, Debug)]
pub(super) struct Trip {
    from: u32,
    to: u32,
    /// Where they may come out, best first.
    exits: Vec<BlockPos>,
    /// Chunks of the far side kept resident for the trip.
    pinned: Vec<ChunkPos>,
    since: u64,
    /// Where they are held meanwhile.
    pub(super) held: PlayerState,
}

const NOT_YOUR_CLAIM: &str = "A portal opens only on your own claim";

impl Game {
    /// The text a portal is known by: "Verdance, near 812, 140".
    fn portal_place(&self, pid: u32) -> Option<String> {
        let p = self.portals.get(pid)?;
        let planet = self.planets.get(p.planet as usize)?;
        let at = landmark(planet.world.shape, &p.frame);
        Some(format!("{}, near {}, {}", planet.name, at.x, at.z))
    }

    /// What a token bound to `tag` shows: "2 by 3, Verdance, near 812, 140".
    pub(super) fn token_name(&self, tag: u32) -> String {
        match (self.portals.get(tag), self.portal_place(tag)) {
            (Some(p), Some(place)) => format!("{} by {}, {place}", p.frame.width, p.frame.height),
            _ => "a portal that is gone".into(),
        }
    }

    fn portal_view(&self, pid: u32) -> Option<PortalView> {
        let p = self.portals.get(pid)?;
        let r = &self.portal_rules;
        Some(PortalView {
            corner: p.frame.corner,
            width: p.frame.width,
            height: p.frame.height,
            link: p.link.and_then(|l| self.portal_place(l)),
            open: self.portals.is_open(pid, r),
            fuel: p.fuel,
            capacity: r.capacity,
            trip_cost: self.portals.trip_cost(pid, r),
            cooldown_hours: self.cfg.portal_charge_cooldown as f32 / 3600.0,
        })
    }

    /// Sends a portal's window to everyone watching it (or closes it for
    /// them if it is gone).
    fn push_portal(&self, pid: u32) {
        let mut view = None;
        for p in self.players.values().filter(|p| p.portal == Some(pid)) {
            let v = view.get_or_insert_with(|| self.portal_view(pid));
            p.out.send(&ServerMsg::Portal(v.clone()));
        }
    }

    /// After portals changed: their blocks follow, their watchers hear.
    fn portals_changed(&mut self, ids: &[u32]) {
        self.sync_portals();
        for pid in ids {
            self.push_portal(*pid);
        }
    }

    /// The portal whose conduit or surface is at `pos`, if this player may
    /// use its window: in reach, loaded, on their own claim. A complete
    /// rectangle with no record yet gets one.
    fn portal_access(&mut self, id: u32, pos: BlockPos) -> Result<u32, Refusal> {
        let (canon, block) = self.block_access(id, pos)?;
        if block != self.portal_conduit && block != self.portal_surface {
            return Err("not a portal".into());
        }
        let p = self.players.get(&id).ok_or("no such player")?;
        let account = p.account.clone();
        let planet = &self.planets[p.planet];
        let shape = planet.world.shape;
        let own = |b: BlockPos| planet.claims.at_block(b).is_some_and(|c| c.owner == account);
        if !own(canon) {
            return Err(Refusal::Told(NOT_YOUR_CLAIM.into()));
        }
        if let Some(pid) = self.portals.at(planet.id, canon) {
            return match self.portals.get(pid) {
                Some(r) if r.owner == account => Ok(pid),
                _ => Err(Refusal::Told("That portal is not yours".into())),
            };
        }
        let (conduit, surface) = (self.portal_conduit, self.portal_surface);
        let world = &planet.world;
        let frame = find_frame(canon, |q| world.block(q).is_some_and(|b| b == conduit || b == surface))
            .map_err(|_| Refusal::Told(SHAPE_RULE.into()))?;
        let blocks = frame_blocks(shape, &frame);
        if blocks.iter().any(|b| self.portals.at(planet.id, *b).is_some()) {
            return Err(Refusal::Told(SHAPE_RULE.into()));
        }
        if !blocks.iter().all(|b| own(*b)) {
            return Err(Refusal::Told(NOT_YOUR_CLAIM.into()));
        }
        let (planet_id, name) = (planet.id, planet.name.clone());
        let pid = self
            .portals
            .create(planet_id, shape, frame, &account)
            .ok_or(Refusal::Told(SHAPE_RULE.into()))?;
        tracing::info!(
            "{account} made portal {pid}, {} by {}, on {name} at {:?}",
            frame.width,
            frame.height,
            shape.wrap_block(frame.corner)
        );
        Ok(pid)
    }

    /// Whether a watched portal's window may stay open: it exists on this
    /// player's planet, part of it is in reach, and it is on their claim.
    fn portal_still_usable(&self, id: u32, pid: u32) -> bool {
        let (Some(p), Some(r)) = (self.players.get(&id), self.portals.get(pid)) else {
            return false;
        };
        let planet = &self.planets[p.planet];
        if r.planet != planet.id || r.owner != p.account {
            return false;
        }
        let shape = planet.world.shape;
        let blocks = frame_blocks(shape, &r.frame);
        blocks
            .iter()
            .all(|b| planet.claims.at_block(*b).is_some_and(|c| c.owner == p.account))
            && blocks
                .iter()
                .any(|b| in_reach(shape, &self.movement, self.cfg.reach, &p.state, *b))
    }

    /// Closes every portal window its watcher may no longer use.
    pub(super) fn check_open_portals(&mut self) {
        let open: Vec<(u32, u32)> = self
            .players
            .iter()
            .filter_map(|(id, p)| p.portal.map(|w| (*id, w)))
            .collect();
        for (id, pid) in open {
            if !self.portal_still_usable(id, pid) {
                self.close_portal(id);
            }
        }
    }

    fn close_portal(&mut self, id: u32) {
        if let Some(p) = self.players.get_mut(&id)
            && p.portal.take().is_some()
        {
            p.out.send(&ServerMsg::Portal(None));
        }
    }

    pub(super) fn open_portal(&mut self, id: u32, pos: Option<BlockPos>) {
        let Some(pos) = pos else {
            if let Some(p) = self.players.get_mut(&id) {
                p.portal = None;
            }
            return;
        };
        match self.portal_access(id, pos) {
            Ok(pid) => {
                let view = self.portal_view(pid);
                if let Some(p) = self.players.get_mut(&id) {
                    p.portal = Some(pid);
                    p.out.send(&ServerMsg::Portal(view));
                }
                // A new record's blocks are already conduits; nothing to sync.
            }
            Err(r) => {
                self.tell(id, r);
                if let Some(p) = self.players.get_mut(&id) {
                    p.portal = None;
                    p.out.send(&ServerMsg::Portal(None));
                }
            }
        }
    }

    pub(super) fn portal_action(&mut self, id: u32, pos: BlockPos, action: PortalAction) {
        let pid = match self.portal_access(id, pos) {
            Ok(pid) => pid,
            Err(r) => return self.tell(id, r),
        };
        let rules = self.portal_rules;
        let (token, wayshard) = (self.portal_token, self.wayshard);
        let Some(p) = self.players.get_mut(&id) else { return };
        let before = p.inventory.clone();
        let mut changed = Vec::new();
        let mut told = None;
        match action {
            PortalAction::MakeToken => match p.inventory.slots.iter_mut().find(|s| s.is_none()) {
                Some(slot) => *slot = Some(ItemStack::tagged(token, pid)),
                None => told = Some("Your inventory is full".to_string()),
            },
            PortalAction::Link { slot } => {
                let stack = p.inventory.get(slot as usize).filter(|s| s.item == token);
                let here = self.portals.get(pid).map(|r| (r.frame.width, r.frame.height));
                let verdict = match stack {
                    None => Err("Choose a portal token to link with"),
                    Some(s) if s.tag == pid => Err("That token is for this portal"),
                    Some(s) => match self
                        .portals
                        .get(s.tag)
                        .filter(|o| (o.planet as usize) < self.planets.len())
                    {
                        None => Err("That token's portal is gone"),
                        Some(o) if Some((o.frame.width, o.frame.height)) != here => {
                            Err("That token is for a portal of another size")
                        }
                        Some(_) => Ok(s.tag),
                    },
                };
                match verdict {
                    Ok(tag) => {
                        p.inventory.take_one(slot as usize);
                        changed = self.portals.link(pid, tag);
                    }
                    Err(text) => told = Some(text.to_string()),
                }
            }
            PortalAction::Unlink => {
                changed = self.portals.unlink(pid);
                if changed.is_empty() {
                    told = Some("This portal is not linked".into());
                }
            }
            PortalAction::AddFuel { slot, count } => {
                match p
                    .inventory
                    .get(slot as usize)
                    .filter(|s| s.item == wayshard && s.tag == 0)
                {
                    None => told = Some("Only wayshards fuel a portal".into()),
                    Some(s) => {
                        let added = self.portals.add_fuel(pid, count.min(s.count) as u32, &rules);
                        if added > 0 {
                            let left = s.count - added as u16;
                            p.inventory.slots[slot as usize] = (left > 0).then_some(ItemStack { count: left, ..s });
                            changed.push(pid);
                        } else if count > 0 {
                            told = Some("The tank is full".into());
                        }
                    }
                }
            }
            PortalAction::TakeFuel { count } => {
                let fuel = self.portals.get(pid).map_or(0, |r| r.fuel);
                if self.portals.get(pid).is_some_and(|r| r.owner != p.account) {
                    told = Some("Only the portal's owner can take fuel out".into());
                } else {
                    let want = (count as u32).min(fuel) as u16;
                    let left = p.inventory.add(&self.items, wayshard, want);
                    let moved = want - left;
                    if moved > 0 {
                        self.portals.take_fuel(pid, moved as u32);
                        changed.push(pid);
                    } else if want > 0 {
                        told = Some("Your inventory is full".into());
                    }
                }
            }
        }
        send_slots(p, &before);
        if let Some(text) = told {
            p.out.send(&ServerMsg::Notice { text });
        }
        if matches!(action, PortalAction::MakeToken) {
            self.tell_tokens(id);
        }
        if !changed.contains(&pid) {
            changed.push(pid);
        }
        self.portals_changed(&changed);
    }

    /// Sends this player the names of tokens in their pack or open storage
    /// that they have not been told yet.
    pub(super) fn tell_tokens(&mut self, id: u32) {
        let Some(p) = self.players.get(&id) else { return };
        let token = self.portal_token;
        let mut tags: BTreeSet<u32> = p
            .inventory
            .slots
            .iter()
            .flatten()
            .filter(|s| s.item == token && s.tag != 0)
            .map(|s| s.tag)
            .collect();
        if let Some(pos) = p.storage {
            let planet = &self.planets[p.planet];
            tags.extend(
                planet
                    .storage
                    .slots(pos, 0)
                    .iter()
                    .flatten()
                    .filter(|s| s.item == token && s.tag != 0)
                    .map(|s| s.tag),
            );
        }
        tags.retain(|t| !p.told_tokens.contains(t));
        if tags.is_empty() {
            return;
        }
        let names: Vec<(u32, String)> = tags.iter().map(|t| (*t, self.token_name(*t))).collect();
        let p = self.players.get_mut(&id).expect("present");
        p.told_tokens.extend(tags);
        p.out.send(&ServerMsg::TokenNames { names });
    }

    /// Removes a portal (broken with an empty tank, or its claim lapsed):
    /// its partner is unlinked, its watchers' windows close, and whoever
    /// knows a token of it hears it is gone. Returns its fuel.
    pub(super) fn remove_portal(&mut self, pid: u32) -> u32 {
        let Some(shape) = self
            .portals
            .get(pid)
            .and_then(|r| self.planets.get(r.planet as usize))
            .map(|pl| pl.world.shape)
        else {
            return 0;
        };
        let Some((gone, changed)) = self.portals.remove(pid, shape) else {
            return 0;
        };
        tracing::info!(
            "portal {pid} of {} is gone ({} wayshards in its tank)",
            gone.owner,
            gone.fuel
        );
        let gone_name = self.token_name(pid);
        for p in self.players.values_mut() {
            if p.portal == Some(pid) {
                p.portal = None;
                p.out.send(&ServerMsg::Portal(None));
            }
            if p.told_tokens.contains(&pid) {
                p.out.send(&ServerMsg::TokenNames {
                    names: vec![(pid, gone_name.clone())],
                });
            }
        }
        self.portals_changed(&changed);
        gone.fuel
    }

    /// Changes one block through the normal path: the chunk is marked dirty
    /// (and saved), and everyone who has the chunk hears of it (their light
    /// and mesh follow, as for any edit).
    fn set_block_for_all(&mut self, pi: usize, canon: BlockPos, state: BlockState) {
        self.planets[pi].world.set_state(canon, state);
        let chunk = canon.chunk();
        for o in self
            .players
            .values()
            .filter(|o| o.planet == pi && o.sent.contains(&chunk))
        {
            o.out.send(&ServerMsg::BlockChanged {
                pos: canon,
                block: state,
            });
        }
    }

    /// Sets every conduit or surface block of a rectangle to `target`, if
    /// all of it is resident. Returns whether it was.
    fn set_frame(&mut self, pi: usize, frame: &PortalFrame, target: BlockId) -> bool {
        let world = &self.planets[pi].world;
        let blocks = frame_blocks(world.shape, frame);
        if blocks.iter().any(|b| world.block(*b).is_none()) {
            return false;
        }
        let (conduit, surface) = (self.portal_conduit, self.portal_surface);
        for b in blocks {
            let current = self.planets[pi].world.block(b);
            if current != Some(target) && (current == Some(conduit) || current == Some(surface)) {
                self.set_block_for_all(pi, b, BlockState::plain(target));
            }
        }
        true
    }

    /// Makes the blocks of every portal that changed show its state: an open
    /// side's rectangle is surface, a dormant or unlinked one conduit, a
    /// removed one conduit. Portals whose chunks are not resident wait.
    pub(super) fn sync_portals(&mut self) {
        if self.portals.sync.is_empty() && self.portals.revert.is_empty() {
            return;
        }
        let ids: Vec<u32> = self.portals.sync.iter().copied().collect();
        for pid in ids {
            let Some(r) = self.portals.get(pid) else {
                self.portals.sync.remove(&pid);
                continue;
            };
            let (pi, frame) = (r.planet as usize, r.frame);
            if pi >= self.planets.len() {
                self.portals.sync.remove(&pid);
                continue;
            }
            if self.planets[pi].world.resident() == 0 {
                continue;
            }
            let target = if self.portals.is_open(pid, &self.portal_rules) {
                self.portal_surface
            } else {
                self.portal_conduit
            };
            if self.set_frame(pi, &frame, target) {
                self.portals.sync.remove(&pid);
            }
        }
        for (planet, frame) in std::mem::take(&mut self.portals.revert) {
            let pi = planet as usize;
            if pi < self.planets.len() && !self.set_frame(pi, &frame, self.portal_conduit) {
                self.portals.revert.push((planet, frame));
            }
        }
    }

    /// Portals of a lapsing plot go: unlinked, their surfaces back to
    /// conduits (which then turn to ash like any block). Returns the fuel of
    /// their tanks, for the owner's reclaim.
    pub(super) fn lapse_portals(&mut self, pi: usize, plot: PlotPos) -> u32 {
        let shape = self.planets[pi].world.shape;
        let planet_id = self.planets[pi].id;
        let ids: Vec<u32> = self
            .portals
            .all()
            .filter(|r| r.planet == planet_id)
            .filter(|r| {
                frame_blocks(shape, &r.frame)
                    .iter()
                    .any(|b| PlotPos::of(*b, shape) == plot)
            })
            .map(|r| r.id)
            .collect();
        ids.into_iter().map(|pid| self.remove_portal(pid)).sum()
    }

    /// Each tick: every player stepping into an open surface goes through.
    pub(super) fn step_portals(&mut self) {
        if self.tick % FORGET_CHARGES_EVERY == 0 {
            self.portals.forget_old_charges(self.tick, &self.portal_rules);
        }
        let ids: Vec<u32> = self.players.keys().copied().collect();
        for id in ids {
            if self.players.get(&id).is_some_and(|p| p.trip.is_some()) {
                self.continue_trip(id);
            } else {
                self.step_into_portal(id);
            }
        }
    }

    /// Whether a player's body touches a surface block, and the open portal
    /// it belongs to if any.
    fn surface_touched(&self, id: u32) -> (bool, Option<u32>) {
        let Some(p) = self.players.get(&id) else {
            return (false, None);
        };
        let planet = &self.planets[p.planet];
        let (shape, world) = (planet.world.shape, &planet.world);
        let m = &self.movement;
        let s = p.state.pos;
        let span = |lo: f64, hi: f64| (lo.floor() as i64)..=((hi.ceil() as i64) - 1);
        let mut touched = false;
        let mut open = None;
        for y in span(s.y, s.y + m.height) {
            for z in span(s.z - m.half_width, s.z + m.half_width) {
                for x in span(s.x - m.half_width, s.x + m.half_width) {
                    let b = BlockPos::new(x, y, z);
                    if world.block(b) != Some(self.portal_surface) {
                        continue;
                    }
                    touched = true;
                    if open.is_none()
                        && let Some(pid) = self.portals.at(planet.id, shape.wrap_block(b))
                        && self.portals.is_open(pid, &self.portal_rules)
                    {
                        open = Some(pid);
                    }
                }
            }
        }
        (touched, open)
    }

    fn step_into_portal(&mut self, id: u32) {
        let (touched, open) = self.surface_touched(id);
        let Some(p) = self.players.get_mut(&id) else { return };
        if !touched {
            p.portal_guard = false;
            return;
        }
        if p.portal_guard {
            return;
        }
        let Some(from) = open else { return };
        let Some(to) = self.portals.get(from).and_then(|r| r.link) else {
            return;
        };
        let (Some(a), Some(b)) = (self.portals.get(from), self.portals.get(to)) else {
            return;
        };
        let Some(dest) = self.planets.get(b.planet as usize) else {
            return;
        };
        // Which face they walked in by: the way they move, or failing that
        // the side they stand on. They come out of the matching face.
        let here = &self.planets[p.planet];
        let normals = a.frame.normals();
        let v = p.state.vel;
        let along = v.x * normals[0].0 as f64 + v.z * normals[0].1 as f64;
        let forward = if along.abs() > 0.01 {
            along > 0.0
        } else {
            let c = a.frame.corner;
            let plane = DVec3::new(c.x as f64 + 0.5, c.y as f64, c.z as f64 + 0.5);
            let d = here.world.shape.delta(plane, p.state.pos);
            d.x * normals[0].0 as f64 + d.z * normals[0].1 as f64 <= 0.0
        };
        let mut exits: Vec<BlockPos> = b
            .frame
            .exit_feet()
            .into_iter()
            .map(|(e, _)| dest.world.shape.wrap_block(e))
            .collect();
        if !forward && exits.len() >= 2 {
            exits.swap(0, 1);
        }
        let mut pinned = BTreeSet::new();
        for e in &exits {
            for dy in -1..=1 {
                pinned.insert(dest.world.shape.wrap_chunk(e.offset(0, dy, 0).chunk()));
            }
        }
        let pinned: Vec<ChunkPos> = pinned.into_iter().collect();
        let held = PlayerState {
            vel: DVec3::ZERO,
            hook: None,
            ..p.state
        };
        p.state = held;
        p.trip = Some(Trip {
            from,
            to,
            exits,
            pinned: pinned.clone(),
            since: self.tick,
            held,
        });
        let world = &mut self.planets[b.planet as usize].world;
        for c in pinned {
            world.pin(c);
            world.request(c, self.tick);
        }
        self.continue_trip(id);
    }

    /// Ends a trip without travelling, letting go of the far side.
    fn cancel_trip(&mut self, id: u32, why: Option<&str>) {
        let Some(p) = self.players.get_mut(&id) else { return };
        let Some(trip) = p.trip.take() else { return };
        if why.is_some() {
            // Told once: they step out before it is tried again.
            p.portal_guard = true;
        }
        self.unpin_trip(&trip);
        if let Some(text) = why {
            self.notice(id, text);
        }
    }

    pub(super) fn unpin_trip(&mut self, trip: &Trip) {
        if let Some(r) = self.portals.get(trip.to)
            && let Some(planet) = self.planets.get_mut(r.planet as usize)
        {
            for c in &trip.pinned {
                planet.world.unpin(*c);
            }
        }
    }

    fn continue_trip(&mut self, id: u32) {
        let Some(trip) = self.players.get(&id).and_then(|p| p.trip.clone()) else {
            return;
        };
        let rules = self.portal_rules;
        let still = self.portals.is_open(trip.from, &rules)
            && self.portals.get(trip.from).and_then(|r| r.link) == Some(trip.to);
        let Some(to_planet) = self.portals.get(trip.to).map(|r| r.planet as usize).filter(|_| still) else {
            return self.cancel_trip(id, None);
        };
        let world = &mut self.planets[to_planet].world;
        if trip.pinned.iter().any(|c| !world.is_loaded(*c)) {
            if self.tick - trip.since > TRIP_WAIT_TICKS {
                return self.cancel_trip(id, Some("The other side of this portal could not be reached"));
            }
            for c in &trip.pinned {
                world.request(*c, self.tick);
            }
            return;
        }
        let exit = trip
            .exits
            .iter()
            .copied()
            .find(|e| !world.solid(*e) && !world.solid(e.offset(0, 1, 0)) && world.solid(e.offset(0, -1, 0)));
        let Some(exit) = exit else {
            return self.cancel_trip(id, Some("The other side of this portal is blocked"));
        };
        let account = self.players[&id].account.clone();
        match self.portals.charge(trip.from, &account, self.tick, &rules) {
            Charge::CannotPay => {
                self.cancel_trip(id, Some("This portal is out of wayshards"));
                return self.portals_changed(&[trip.from]);
            }
            Charge::Paid(_) => self.portals_changed(&[trip.from]),
            Charge::Free => {}
        }
        let shape = self.planets[to_planet].world.shape;
        let spawn = PlayerState {
            pos: shape.wrap_point(DVec3::new(exit.x as f64 + 0.5, exit.y as f64, exit.z as f64 + 0.5)),
            vel: DVec3::ZERO,
            on_ground: true,
            hook: None,
        };
        let Some(p) = self.players.get_mut(&id) else { return };
        p.trip = None;
        p.portal_guard = true;
        let from_planet = p.planet;
        tracing::info!(
            "{} went through portal {} to portal {} on {}",
            p.name,
            trip.from,
            trip.to,
            self.planets[to_planet].name
        );
        if from_planet == to_planet {
            p.state = spawn;
            p.mining = None;
            p.out.send(&ServerMsg::Teleport { spawn, tick: self.tick });
        } else {
            self.travel(id, to_planet, Some(spawn));
            if let Some(p) = self.players.get_mut(&id) {
                p.portal_guard = true;
            }
        }
        self.unpin_trip(&trip);
    }
}
