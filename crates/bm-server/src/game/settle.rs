//! Prestige, settlements and footfall in play (milestone 24 part 4,
//! `docs/milestones/24-coin-and-trade.md`). Each claim's tally changes as
//! blocks are placed, chiselled and broken in it; a planet's settlements
//! are found again within [`FOOTFALL_EVERY`] ticks of a change to its claims
//! or any tally (and every [`SETTLE_EVERY`] ticks in any case), and players
//! there are told when they changed; a player stepping into a
//! settlement's claim pays its owner footfall, taken from no one.

use super::*;
use crate::settlements::{self, ClaimPlots, Settlement};
use crate::store::SavedFootfall;
use bm_core::prestige::{self, FOOTFALL_MIN_PRESTIGE};
use bm_protocol::SettlementView;

/// Ticks between finding settlements again (10 s).
pub(super) const SETTLE_EVERY: u64 = 600;
/// Ticks between footfall checks of where each player stands (1 s).
pub(super) const FOOTFALL_EVERY: u64 = 60;
/// Footfall looks back this far (5 days, in seconds).
const FOOTFALL_MEMORY: u64 = 5 * 86_400;
/// A visitor counts in a claim when it holds their plot or one this many
/// plots above or below (a claim is its columns to a visitor, as to a
/// settlement: standing on its roof is visiting it).
const PLOTS_AROUND: i32 = 4;
/// Longest settlement name.
pub(super) const NAME_MAX: usize = 32;

/// A planet's settlements and (claim, prestige) for each claim with any.
pub(super) type Found = (Vec<Settlement>, Vec<(u32, u64)>);

impl Game {
    /// The claim holding `canon` on planet `pi`, if any.
    fn claim_id_at(&self, pi: usize, canon: BlockPos) -> Option<u32> {
        self.planets[pi].claims.at_block(canon).map(|c| c.id)
    }

    /// The claim a player standing at `here` is in: one holding their plot
    /// or a plot a little above or below.
    fn claim_around(&self, pi: usize, here: BlockPos) -> Option<&crate::claims::Claim> {
        let plot = PlotPos::of(here, self.planets[pi].world.shape);
        (0..=PLOTS_AROUND)
            .flat_map(|d| [d, -d])
            .find_map(|dy| self.planets[pi].claims.at(PlotPos { y: plot.y + dy, ..plot }))
    }

    /// A block was placed (`new`) by a player at `canon`: its claim counts it.
    pub(super) fn prestige_placed(&mut self, pi: usize, canon: BlockPos, new: BlockId) {
        if let Some(claim) = self.claim_id_at(pi, canon) {
            let name = self.planets[pi].world.registry.get(new).name.clone();
            self.tallies.change(claim, |t| t.placed(&name));
        }
    }

    /// A block at `canon` was chiselled into `shape`.
    pub(super) fn prestige_chiselled(&mut self, pi: usize, canon: BlockPos, shape: u16) {
        if let Some(claim) = self.claim_id_at(pi, canon) {
            self.tallies.change(claim, |t| t.chiselled(shape));
        }
    }

    /// A block `old` at `canon` was broken: its claim counts one fewer.
    pub(super) fn prestige_removed(&mut self, pi: usize, canon: BlockPos, old: BlockId) {
        if let Some(claim) = self.claim_id_at(pi, canon) {
            let name = self.planets[pi].world.registry.get(old).name.clone();
            self.tallies.change(claim, |t| t.removed(&name));
        }
    }

    /// A claim is gone: its prestige and footfall go with it.
    pub(super) fn claim_forgotten(&mut self, claim: u32) {
        self.tallies.forget(claim);
        if self.footfall.remove(&claim).is_some() {
            self.footfall_dirty.insert(claim);
        }
    }

    /// A claim's prestige.
    pub(super) fn claim_prestige(&self, pi: usize, claim: u32) -> u64 {
        let reg = &self.planets[pi].world.registry;
        self.tallies
            .get(claim)
            .map_or(0, |t| t.score(|name| reg.id(name).map_or(0, |id| reg.get(id).prestige)))
    }

    /// Finds planets' settlements again (all of them if `all`, else those
    /// whose claims or tallies changed); players on a planet whose
    /// settlements or prestige changed are told.
    pub(super) fn step_settlements(&mut self, all: bool) {
        self.settlements.resize_with(self.planets.len(), Default::default);
        self.settle_seen.resize(self.planets.len(), None);
        for pi in 0..self.planets.len() {
            let claims = &self.planets[pi].claims;
            let seen = (
                self.tallies.generation(),
                claims.all().count(),
                claims.all().map(|c| c.plots.len()).sum::<usize>(),
            );
            if !all && self.settle_seen[pi] == Some(seen) {
                continue;
            }
            self.settle_seen[pi] = Some(seen);
            let found = self.find_settlements(pi);
            if found != self.settlements[pi] {
                let capital = self.capital_name(pi);
                self.settlements[pi] = found;
                self.carry_names(pi);
                if self.capital_name(pi) != capital {
                    self.universe_dirty = true;
                }
                let msg = self.settlements_msg(pi);
                for p in self.players.values().filter(|p| p.planet == pi) {
                    p.out.send(&msg);
                }
            }
        }
    }

    fn find_settlements(&self, pi: usize) -> Found {
        let planet = &self.planets[pi];
        let prestige: Vec<(u32, u64)> = planet
            .claims
            .all()
            .map(|c| (c.id, self.claim_prestige(pi, c.id)))
            .filter(|&(_, p)| p > 0)
            .collect();
        let of = |id: u32| prestige.iter().find(|p| p.0 == id).map_or(0, |p| p.1);
        let claims: Vec<ClaimPlots> = planet
            .claims
            .all()
            .map(|c| ClaimPlots {
                id: c.id,
                owner: &c.owner,
                owner_name: &c.owner_name,
                plots: &c.plots,
                prestige: of(c.id),
            })
            .collect();
        let mut prestige = prestige;
        prestige.sort_unstable();
        (settlements::find(&claims, planet.world.shape), prestige)
    }

    /// A settlement's name lives under its key claim; when the key moves (a
    /// new warden, or the warden's top claim changed) the name a claim of
    /// it held moves with it.
    fn carry_names(&mut self, pi: usize) {
        let mut moves = Vec::new();
        for s in &self.settlements[pi].0 {
            if self.settlement_names.contains_key(&s.key) {
                continue;
            }
            if let Some(&from) = s.claims.iter().find(|c| self.settlement_names.contains_key(c)) {
                moves.push((from, s.key));
            }
        }
        for (from, to) in moves {
            if let Some(name) = self.settlement_names.remove(&from) {
                self.settlement_names.insert(to, name);
                self.names_dirty.insert(from);
                self.names_dirty.insert(to);
            }
        }
    }

    /// Planet `pi`'s capital's name, if it has one.
    pub(super) fn capital_name(&self, pi: usize) -> Option<String> {
        let found = self.settlements.get(pi)?;
        found.0.iter().find(|s| s.capital).map(|s| self.settlement_name(s))
    }

    fn settlement_name(&self, s: &Settlement) -> String {
        self.settlement_names
            .get(&s.key)
            .cloned()
            .unwrap_or_else(|| format!("{}'s {}", s.warden_name, s.rank.name()))
    }

    pub(super) fn settlements_msg(&self, pi: usize) -> ServerMsg {
        let (found, prestige) = self.settlements.get(pi).cloned().unwrap_or_default();
        ServerMsg::Settlements {
            settlements: found
                .iter()
                .map(|s| SettlementView {
                    name: self.settlement_name(s),
                    rank: s.rank.name().to_string(),
                    prestige: s.prestige,
                    warden: s.warden_name.clone(),
                    capital: s.capital,
                    claims: s.claims.clone(),
                })
                .collect(),
            prestige,
        }
    }

    /// Tells player `id` their planet's settlements (on arrival).
    pub(super) fn send_settlements(&mut self, id: u32) {
        let Some(pi) = self.players.get(&id).map(|p| p.planet) else {
            return;
        };
        self.settlements.resize_with(self.planets.len(), Default::default);
        let msg = self.settlements_msg(pi);
        if let Some(p) = self.players.get_mut(&id) {
            p.out.send(&msg);
            p.in_claim = None;
        }
    }

    /// `/settlement name`: the warden of the settlement they stand in names it.
    pub(super) fn name_settlement(&mut self, id: u32, args: &[&str]) {
        let Some((pi, account, feet)) = self
            .players
            .get(&id)
            .map(|p| (p.planet, p.account.clone(), p.state.pos))
        else {
            return;
        };
        let name = args.join(" ");
        let name = name.trim();
        if name.is_empty() {
            return self.notice(id, "Name the settlement you stand in with /settlement name");
        }
        if name.chars().count() > NAME_MAX || name.chars().any(|c| c.is_control()) {
            return self.notice(id, &format!("A settlement name is at most {NAME_MAX} plain characters"));
        }
        let shape = self.planets[pi].world.shape;
        let here = shape.wrap_block(BlockPos::containing(feet.x, feet.y, feet.z));
        let Some(claim) = self.claim_around(pi, here).map(|c| c.id) else {
            return self.notice(id, "Stand in a settlement to name it");
        };
        self.settlements.resize_with(self.planets.len(), Default::default);
        let Some(s) = self.settlements[pi]
            .0
            .iter()
            .find(|s| s.claims.contains(&claim))
            .cloned()
        else {
            return self.notice(id, "This claim is not part of a settlement yet");
        };
        if s.warden != account {
            return self.notice(id, &format!("Only its warden, {}, can name it", s.warden_name));
        }
        // Any name a claim of it held goes; the key holds the new one.
        for c in &s.claims {
            if self.settlement_names.remove(c).is_some() {
                self.names_dirty.insert(*c);
            }
        }
        self.settlement_names.insert(s.key, name.to_string());
        self.names_dirty.insert(s.key);
        self.notice(id, &format!("This settlement is now {name}"));
        if s.capital {
            self.universe_dirty = true;
        }
        let msg = self.settlements_msg(pi);
        for p in self.players.values().filter(|p| p.planet == pi) {
            p.out.send(&msg);
        }
    }

    /// Each player who has stepped into another's claim pays its owner
    /// footfall, if the claim has the prestige and is in a settlement.
    pub(super) fn step_footfall(&mut self) {
        let now = unix_now();
        let ids: Vec<u32> = self.players.keys().copied().collect();
        for id in ids {
            let Some(p) = self.players.get(&id) else { continue };
            let pi = p.planet;
            let shape = self.planets[pi].world.shape;
            let feet = p.state.pos;
            let here = shape.wrap_block(BlockPos::containing(feet.x, feet.y, feet.z));
            let claim = self
                .claim_around(pi, here)
                .filter(|c| c.owner != p.account)
                .map(|c| (c.id, c.owner.clone()));
            let entered = claim.as_ref().map(|c| c.0);
            let was = p.in_claim;
            if let Some(p) = self.players.get_mut(&id) {
                p.in_claim = entered;
            }
            let Some((claim, owner)) = claim else { continue };
            if was == Some(claim) {
                continue;
            }
            let account = self.players[&id].account.clone();
            self.visit(pi, claim, &owner, &account, now);
        }
    }

    /// `visitor` stepped into `claim` (owned by `owner`) at `now`.
    fn visit(&mut self, pi: usize, claim: u32, owner: &str, visitor: &str, now: u64) {
        if self.claim_prestige(pi, claim) < FOOTFALL_MIN_PRESTIGE {
            return;
        }
        self.settlements.resize_with(self.planets.len(), Default::default);
        let Some(rank) = self.settlements[pi]
            .0
            .iter()
            .find(|s| s.claims.contains(&claim))
            .map(|s| s.rank)
        else {
            return;
        };
        let rec = self.footfall.entry(claim).or_default();
        rec.paid.retain(|(_, t)| now.saturating_sub(*t) < FOOTFALL_MEMORY);
        let today = rec.paid.iter().filter(|(_, t)| now.saturating_sub(*t) < 86_400).count() as u32;
        let last = rec.paid.iter().find(|(a, _)| a == visitor).map(|&(_, t)| t);
        let days = last.map(|t| (now.saturating_sub(t) / 86_400) as u32);
        let coin = prestige::footfall(rank, days, today);
        if coin == 0 {
            return;
        }
        rec.paid.retain(|(a, _)| a != visitor);
        rec.paid.push((visitor.to_string(), now));
        self.footfall_dirty.insert(claim);
        self.pay_account(owner, coin);
        match self.player_by_account(owner) {
            Some(oid) => self.count(oid, "footfall", coin),
            None => self.note_away(owner, &[("footfall", coin)]),
        }
    }

    /// What changed for the next save.
    pub(super) fn take_settle_dirty(&mut self) -> SettleDirty {
        SettleDirty {
            tallies: self.tallies.take_dirty(),
            names: std::mem::take(&mut self.names_dirty)
                .into_iter()
                .map(|id| (id, self.settlement_names.get(&id).cloned()))
                .collect(),
            footfall: std::mem::take(&mut self.footfall_dirty)
                .into_iter()
                .map(|id| (id, self.footfall.get(&id).cloned()))
                .collect(),
        }
    }
}

pub(super) struct SettleDirty {
    pub tallies: Vec<(u32, Option<bm_core::prestige::Tally>)>,
    pub names: Vec<(u32, Option<String>)>,
    pub footfall: Vec<(u32, Option<SavedFootfall>)>,
}
