//! The forge in play (milestone 17, `docs/milestones/17-forging.md`):
//! starting a session, playing its rounds, sealing into the queue, and the
//! forge's window. The rules of a session are `bm_core::forge`'s; the
//! record lives in the forge's station (`crafting::Stations`).

use super::stations::PlanetEnv;
use super::{Game, mining, send_slots};
use crate::forging::{self, Forging};
use bm_core::forge::{DECK_STACK, Ended, Gear, Refused, Session, queue_band};
use bm_core::{BlockPos, INVENTORY_SLOTS, ItemStack, Station};
use bm_protocol::{ForgeOutcome, ForgeView, ServerMsg, SessionView};

impl Game {
    /// A seed for a session's dice, from the server's generator.
    fn forge_seed(&mut self) -> u64 {
        let hi = (mining::roll(&mut self.rng) * (1u64 << 32) as f64) as u64;
        let lo = (mining::roll(&mut self.rng) * (1u64 << 32) as f64) as u64;
        hi << 32 | lo
    }

    pub(super) fn forge_begin(&mut self, id: u32, pos: BlockPos, gear_slot: u8, deck: Vec<(u8, u16)>) {
        let (pi, m) = match self.machine_access(id, pos) {
            Ok(found) => found,
            Err(r) => return self.tell(id, r),
        };
        if m.kind != Station::Forge {
            return self.notice(id, "That is not a forge");
        }
        if let Some(why) = &m.idle {
            return self.notice(id, why);
        }
        let key = self.machine_record(pi, &m);
        let Some(p) = self.players.get(&id) else { return };
        if let Some(f) = self.planets[pi].stations.forge(key) {
            if let Some(s) = &f.session {
                let text = if s.account == p.account {
                    "You are forging here already".to_string()
                } else {
                    format!("{} is forging here", s.name)
                };
                return self.notice(id, &text);
            }
            if f.queued.is_some() {
                return self.notice(id, "The forge is busy with a sealed item: wait for it");
            }
        }
        // The gear.
        let gear_slot = gear_slot as usize;
        let Some(gear_stack) = p.inventory.get(gear_slot) else {
            return self.notice(id, "Put the gear to forge in the forge");
        };
        let Some(def) = self.items.get(gear_stack.item) else {
            return;
        };
        let Some(gear) = Gear::of(def).filter(|_| def.flex > 0) else {
            return self.notice(id, "That cannot be forged");
        };
        if gear_stack.forge.done {
            return self.notice(id, "That has been forged already");
        }
        let flex = def.flex;
        // The deck: each slot's count added up, then stacks of the same
        // ingredient together, in the order asked.
        let forger = p.effects.forger();
        let mut asked: Vec<(usize, u32)> = Vec::new();
        for &(slot, count) in &deck {
            let slot = slot as usize;
            if slot >= INVENTORY_SLOTS || slot == gear_slot || count == 0 {
                return self.notice(id, "That is not in your inventory");
            }
            match asked.iter_mut().find(|a| a.0 == slot) {
                Some(a) => a.1 += count as u32,
                None => asked.push((slot, count as u32)),
            }
        }
        if asked.is_empty() {
            return self.notice(id, "Put at least one ingredient in the deck");
        }
        let mut slots: Vec<bm_core::forge::DeckSlot> = Vec::new();
        for &(slot, count) in &asked {
            let Some(stack) = p.inventory.get(slot).filter(|s| s.count as u32 >= count) else {
                return self.notice(id, "You do not have that many");
            };
            let Some(mut ds) = forging::deck_slot(&self.items, stack) else {
                return self.notice(id, "Only forge ingredients go in the deck");
            };
            ds.stack.count = 0;
            let ds = match slots.iter_mut().find(|d| d.stack.stacks_with(&ds.stack)) {
                Some(d) => d,
                None => {
                    slots.push(ds);
                    slots.last_mut().expect("just pushed")
                }
            };
            let total = ds.stack.count as u32 + count;
            if total > DECK_STACK as u32 {
                return self.notice(id, &format!("A deck slot holds at most {DECK_STACK} of one ingredient"));
            }
            ds.stack.count = total as u16;
        }
        if slots.len() > forger.deck_slots() {
            return self.notice(
                id,
                &format!("Your deck holds at most {} kinds of ingredient", forger.deck_slots()),
            );
        }
        let (account, name) = (p.account.clone(), p.name.clone());
        // Everything checked: take it all out.
        self.change_inventory(id, |inv, _| {
            inv.slots[gear_slot] = None;
            for &(slot, count) in &asked {
                if let Some(s) = &mut inv.slots[slot] {
                    s.count -= count as u16;
                    if s.count == 0 {
                        inv.slots[slot] = None;
                    }
                }
            }
        });
        let seed = self.forge_seed();
        // The forge's power from its coils (milestone 18) raises
        // Effectiveness.
        let power = {
            let planet = &mut self.planets[pi];
            let env = super::stations::PlanetEnv::new(&planet.world, self.spark_link);
            planet.stations.forge_power(key, &env)
        };
        let session = Session::new(gear_stack, gear, forger.stats(flex, power as f64), slots, seed);
        self.planets[pi]
            .stations
            .begin_forge(key, Forging { account, name, session });
        if let Some(p) = self.players.get_mut(&id) {
            p.watching = Some(key);
        }
        self.push_station(pi, key);
    }

    pub(super) fn forge_use(&mut self, id: u32, pos: BlockPos, slot: u8, round: u32) {
        let (pi, m) = match self.machine_access(id, pos) {
            Ok(found) => found,
            Err(r) => return self.tell(id, r),
        };
        if m.kind != Station::Forge {
            return;
        }
        let key = self.machine_record(pi, &m);
        let Some(account) = self.players.get(&id).map(|p| p.account.clone()) else {
            return;
        };
        let stations = &mut self.planets[pi].stations;
        let Some(f) = stations.forge(key).and_then(|f| f.session.as_ref()) else {
            return self.notice(id, "Nothing is being forged here");
        };
        if f.account != account {
            let text = format!("{} is forging here", f.name);
            return self.notice(id, &text);
        }
        if f.session.rounds != round {
            // A double click: the round it meant has been played.
            return;
        }
        let f = stations.forging_mut(key).expect("checked");
        let ended = match f.session.use_ingredient(slot as usize) {
            Ok(e) => e,
            Err(Refused::NoVigour) => return self.notice(id, "Not enough vigour for that"),
            Err(Refused::Empty) => return self.notice(id, "None of that is left"),
            Err(Refused::NotAnIngredient) => return,
        };
        let Some(ended) = ended else {
            return self.push_station(pi, key);
        };
        let f = stations.end_forge(key).expect("a session was running");
        let s = f.session;
        let leftovers = s.leftovers();
        let (outcome, back) = match ended {
            Ended::Sealed(forged) => {
                let item = ItemStack {
                    forge: forged,
                    ..s.item
                };
                let rank = self.items.get(item.item).map_or(0, |d| d.rank);
                let band = queue_band(forged.rank(rank));
                let planet = &mut self.planets[pi];
                let env = PlanetEnv::new(&planet.world, self.spark_link);
                planet.stations.seal(key, item, band, self.tick, &self.items, &env);
                (ForgeOutcome::Sealed(item), leftovers)
            }
            Ended::Unmade(kinds) => {
                let returned: Vec<ItemStack> = kinds.into_iter().map(|(st, n)| ItemStack { count: n, ..st }).collect();
                let mut back = vec![s.item];
                back.extend(returned.iter().copied());
                back.extend(leftovers);
                (ForgeOutcome::Unmade { gear: s.item, returned }, back)
            }
            Ended::Destroyed => (ForgeOutcome::Destroyed(s.item), leftovers),
        };
        self.give_back(id, pi, key, back);
        if let Some(p) = self.players.get(&id) {
            p.out.send(&ServerMsg::Forged { pos: key, outcome });
        }
        self.push_station(pi, key);
    }

    /// Gives what a session returns to its forger: into the inventory, what
    /// does not fit into the forge's output, and anything left after that
    /// into their reclaim (never lost).
    fn give_back(&mut self, id: u32, pi: usize, key: BlockPos, stacks: Vec<ItemStack>) {
        let Some(p) = self.players.get_mut(&id) else { return };
        let before = p.inventory.clone();
        let mut over = Vec::new();
        for s in stacks.into_iter().filter(|s| s.count > 0) {
            let left = p.inventory.add_stack(&self.items, s);
            if left > 0 {
                over.push(ItemStack { count: left, ..s });
            }
        }
        send_slots(p, &before);
        let account = p.account.clone();
        let mut lost = Vec::new();
        for s in over {
            let left = self.planets[pi]
                .stations
                .add_output(key, Station::Forge, s, &self.items);
            if left > 0 {
                lost.push(ItemStack { count: left, ..s });
            }
        }
        if !lost.is_empty() {
            self.reclaims.add(&account, lost);
            self.push_reclaim(&account);
            self.notice(
                id,
                "Your inventory and the forge are full: the rest waits in reclaim at a warp shrine",
            );
        }
    }

    /// The forge part of a forge's window as `viewer` (an account) sees it:
    /// its forger sees the session (never the dice); anyone else who forges.
    pub(super) fn forge_view(&self, pi: usize, key: BlockPos, viewer: Option<&str>) -> ForgeView {
        let Some(f) = self.planets[pi].stations.forge(key) else {
            return ForgeView::default();
        };
        let mine = f.session.as_ref().filter(|s| Some(s.account.as_str()) == viewer);
        ForgeView {
            session: mine.map(|s| SessionView::of(&s.session)),
            forger: f.session.as_ref().filter(|_| mine.is_none()).map(|s| s.name.clone()),
            queued: f.queued.as_ref().map(|q| {
                let (progress, left) = q.progress(self.tick);
                (q.item, progress, left)
            }),
        }
    }
}
