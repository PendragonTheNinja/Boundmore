//! Portals (milestone 8, ADR-0009): the server's record of each portal,
//! its link, its fuel tank, and who has paid to go through it.
//!
//! A portal is a filled, upright rectangle of conduits on its owner's claim
//! (`bm_core::portal`). Its record is made the first time its owner opens its
//! window, and is kept until a conduit of it is broken (only with an empty
//! tank) or its claim lapses. Records are universe-wide (a link may cross
//! planets) and portal numbers are never reused: the next one is saved.
//!
//! The blocks follow the record: an open side's rectangle is
//! `portal_surface`, a dormant or unlinked one `portal_conduit`. The game
//! writes them through its normal block path when every chunk of the
//! rectangle is resident; until then the portal waits in `sync` (a side whose
//! partner changed may be on a planet nobody is on).

use crate::store::SavedPortal;
use bm_core::portal::PortalFrame;
use bm_core::{BlockPos, PlanetShape};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The tunables of portals, from `ServerConfig`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalRules {
    /// Most wayshards a tank holds.
    pub capacity: u32,
    /// What one charged trip costs, to the same planet and to another.
    pub cost_same_planet: u32,
    pub cost_other_planet: u32,
    /// Ticks a traveller's charge lasts at one portal.
    pub cooldown_ticks: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Portal {
    pub id: u32,
    /// The planet's id.
    pub planet: u16,
    /// Its rectangle, the corner canonical (`blocks` wraps the rest).
    pub frame: PortalFrame,
    /// Lowercased account name of whoever made it.
    pub owner: String,
    /// The portal it leads to (and that leads back here).
    pub link: Option<u32>,
    /// Wayshards in its tank.
    pub fuel: u32,
}

impl Portal {
    fn saved(&self) -> SavedPortal {
        SavedPortal {
            planet: self.planet,
            corner: self.frame.corner,
            axis: self.frame.axis,
            width: self.frame.width,
            height: self.frame.height,
            owner: self.owner.clone(),
            link: self.link,
            fuel: self.fuel,
        }
    }
}

/// Every block of a frame, canonical on its planet.
pub fn frame_blocks(shape: PlanetShape, frame: &PortalFrame) -> Vec<BlockPos> {
    frame.blocks().map(|p| shape.wrap_block(p)).collect()
}

/// The spot a portal is known by: the bottom middle of its rectangle.
pub fn landmark(shape: PlanetShape, frame: &PortalFrame) -> BlockPos {
    shape.wrap_block(frame.at((frame.width - 1) / 2, 0))
}

/// What a charge came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charge {
    /// Within the cooldown of the last charge: nothing taken.
    Free,
    /// This many wayshards taken from the tank.
    Paid(u32),
    /// The tank holds less than the cost: nothing taken.
    CannotPay,
}

#[derive(Default)]
pub struct Portals {
    by_id: BTreeMap<u32, Portal>,
    /// Which portal each canonical rectangle block belongs to.
    blocks: HashMap<(u16, BlockPos), u32>,
    next: u32,
    /// Changed since the last save.
    dirty: BTreeSet<u32>,
    next_dirty: bool,
    /// Portals whose blocks may not show their state yet.
    pub sync: BTreeSet<u32>,
    /// Rectangles of removed portals whose surfaces go back to conduits.
    pub revert: Vec<(u16, PortalFrame)>,
    /// When each account was last charged at each portal, in ticks. Kept in
    /// memory only: a restart forgets it, so at worst a traveller pays once
    /// more than they would have.
    charged: HashMap<(u32, String), u64>,
}

impl Portals {
    /// Rebuilds from the save. `shape_of` gives the shape of each hosted
    /// planet; portals of planets not hosted are kept as saved but not
    /// indexed (nothing can reach them). A link that does not lead back is
    /// dropped. Every portal starts in `sync`, so its blocks are checked once.
    pub fn load(saved: Vec<(u32, SavedPortal)>, next: u32, shape_of: impl Fn(u16) -> Option<PlanetShape>) -> Self {
        let mut s = Self {
            next: next.max(1),
            ..Default::default()
        };
        for (id, p) in saved {
            s.next = s.next.max(id.saturating_add(1));
            let mut frame = PortalFrame {
                corner: p.corner,
                axis: p.axis,
                width: p.width,
                height: p.height,
            };
            if let Some(shape) = shape_of(p.planet) {
                frame.corner = shape.wrap_block(frame.corner);
                let blocks = frame_blocks(shape, &frame);
                if let Some(other) = blocks.iter().find_map(|b| s.blocks.get(&(p.planet, *b))) {
                    tracing::error!("portal {id} overlaps portal {other}; skipped (its record is left on disk)");
                    continue;
                }
                for b in blocks {
                    s.blocks.insert((p.planet, b), id);
                }
            }
            s.by_id.insert(
                id,
                Portal {
                    id,
                    planet: p.planet,
                    frame,
                    owner: p.owner,
                    link: p.link,
                    fuel: p.fuel,
                },
            );
        }
        let ids: Vec<u32> = s.by_id.keys().copied().collect();
        for id in ids {
            let link = s.by_id[&id].link;
            if let Some(l) = link
                && s.by_id.get(&l).and_then(|o| o.link) != Some(id)
            {
                tracing::warn!("portal {id} was linked to {l}, which does not link back; unlinked");
                s.by_id.get_mut(&id).expect("present").link = None;
                s.dirty.insert(id);
            }
            s.sync.insert(id);
        }
        s
    }

    pub fn get(&self, id: u32) -> Option<&Portal> {
        self.by_id.get(&id)
    }

    pub fn all(&self) -> impl Iterator<Item = &Portal> {
        self.by_id.values()
    }

    /// The portal a canonical block belongs to.
    pub fn at(&self, planet: u16, canon: BlockPos) -> Option<u32> {
        self.blocks.get(&(planet, canon)).copied()
    }

    /// Records a new portal. `frame`'s blocks must belong to no other portal.
    pub fn create(&mut self, planet: u16, shape: PlanetShape, frame: PortalFrame, owner: &str) -> Option<u32> {
        let frame = PortalFrame {
            corner: shape.wrap_block(frame.corner),
            ..frame
        };
        let blocks = frame_blocks(shape, &frame);
        if blocks.iter().any(|b| self.blocks.contains_key(&(planet, *b))) {
            return None;
        }
        let id = self.next;
        self.next += 1;
        self.next_dirty = true;
        for b in blocks {
            self.blocks.insert((planet, b), id);
        }
        self.by_id.insert(
            id,
            Portal {
                id,
                planet,
                frame,
                owner: owner.to_string(),
                link: None,
                fuel: 0,
            },
        );
        self.dirty.insert(id);
        self.sync.insert(id);
        Some(id)
    }

    /// Marks portals changed: saved, and their blocks brought up to date.
    fn touch(&mut self, ids: &[u32]) {
        for id in ids {
            if self.by_id.contains_key(id) {
                self.dirty.insert(*id);
                self.sync.insert(*id);
            }
        }
    }

    /// Forgets a portal's link, both ways. Returns the portals changed.
    pub fn unlink(&mut self, id: u32) -> Vec<u32> {
        let Some(partner) = self.by_id.get_mut(&id).and_then(|p| p.link.take()) else {
            return Vec::new();
        };
        let mut changed = vec![id];
        if let Some(o) = self.by_id.get_mut(&partner)
            && o.link == Some(id)
        {
            o.link = None;
            changed.push(partner);
        }
        self.touch(&changed);
        changed
    }

    /// Links two portals both ways, unlinking whatever either was linked to.
    /// Returns the portals changed (empty if refused: the same portal, or
    /// either missing).
    pub fn link(&mut self, a: u32, b: u32) -> Vec<u32> {
        if a == b || !self.by_id.contains_key(&a) || !self.by_id.contains_key(&b) {
            return Vec::new();
        }
        let mut changed = self.unlink(a);
        changed.extend(self.unlink(b));
        self.by_id.get_mut(&a).expect("present").link = Some(b);
        self.by_id.get_mut(&b).expect("present").link = Some(a);
        changed.extend([a, b]);
        changed.sort_unstable();
        changed.dedup();
        self.touch(&changed);
        changed
    }

    /// Removes a portal: its partner is unlinked, its blocks go back to
    /// conduits. Returns the record and the portals changed.
    pub fn remove(&mut self, id: u32, shape: PlanetShape) -> Option<(Portal, Vec<u32>)> {
        let changed = self.unlink(id);
        let p = self.by_id.remove(&id)?;
        for b in frame_blocks(shape, &p.frame) {
            self.blocks.remove(&(p.planet, b));
        }
        self.charged.retain(|(portal, _), _| *portal != id);
        self.dirty.insert(id);
        self.sync.remove(&id);
        self.revert.push((p.planet, p.frame));
        let changed = changed.into_iter().filter(|c| *c != id).collect();
        Some((p, changed))
    }

    /// What one charged trip through this side costs.
    pub fn trip_cost(&self, id: u32, rules: &PortalRules) -> u32 {
        let Some(p) = self.by_id.get(&id) else { return 0 };
        match p.link.and_then(|l| self.by_id.get(&l)) {
            Some(o) if o.planet != p.planet => rules.cost_other_planet,
            _ => rules.cost_same_planet,
        }
    }

    /// Linked, and its own tank can pay a trip.
    pub fn is_open(&self, id: u32, rules: &PortalRules) -> bool {
        self.by_id
            .get(&id)
            .is_some_and(|p| p.link.is_some() && p.fuel >= self.trip_cost(id, rules))
    }

    /// Puts up to `n` wayshards in the tank, as far as it holds. Returns how
    /// many went in.
    pub fn add_fuel(&mut self, id: u32, n: u32, rules: &PortalRules) -> u32 {
        let Some(p) = self.by_id.get_mut(&id) else { return 0 };
        let n = n.min(rules.capacity.saturating_sub(p.fuel));
        if n > 0 {
            p.fuel += n;
            self.touch(&[id]);
        }
        n
    }

    /// Takes up to `n` wayshards out of the tank. Returns how many came out.
    pub fn take_fuel(&mut self, id: u32, n: u32) -> u32 {
        let Some(p) = self.by_id.get_mut(&id) else { return 0 };
        let n = n.min(p.fuel);
        if n > 0 {
            p.fuel -= n;
            self.touch(&[id]);
        }
        n
    }

    /// Charges `account` for going through portal `id` at `tick`: at most
    /// once per cooldown, from the portal's own tank.
    pub fn charge(&mut self, id: u32, account: &str, tick: u64, rules: &PortalRules) -> Charge {
        let key = (id, account.to_string());
        if self
            .charged
            .get(&key)
            .is_some_and(|&t| tick.saturating_sub(t) < rules.cooldown_ticks)
        {
            return Charge::Free;
        }
        let cost = self.trip_cost(id, rules);
        let Some(p) = self.by_id.get_mut(&id) else {
            return Charge::CannotPay;
        };
        if p.fuel < cost {
            return Charge::CannotPay;
        }
        p.fuel -= cost;
        self.touch(&[id]);
        self.charged.insert(key, tick);
        Charge::Paid(cost)
    }

    /// Forgets charges older than the cooldown (they no longer matter).
    pub fn forget_old_charges(&mut self, tick: u64, rules: &PortalRules) {
        self.charged
            .retain(|_, t| tick.saturating_sub(*t) < rules.cooldown_ticks);
    }

    /// Changed portals for the save (None for one removed), and the next
    /// portal number if it changed.
    pub fn take_dirty(&mut self) -> (Vec<(u32, Option<SavedPortal>)>, Option<u32>) {
        let changed = std::mem::take(&mut self.dirty)
            .into_iter()
            .map(|id| (id, self.by_id.get(&id).map(Portal::saved)))
            .collect();
        let next = std::mem::take(&mut self.next_dirty).then_some(self.next);
        (changed, next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::portal::PortalAxis;

    const RULES: PortalRules = PortalRules {
        capacity: 500,
        cost_same_planet: 1,
        cost_other_planet: 2,
        cooldown_ticks: 3600 * 60,
    };

    fn shape() -> PlanetShape {
        PlanetShape::new(8, 4)
    }

    fn frame(x: i64, w: u8, h: u8) -> PortalFrame {
        PortalFrame {
            corner: BlockPos::new(x, 40, 10),
            axis: PortalAxis::X,
            width: w,
            height: h,
        }
    }

    #[test]
    fn records_index_their_blocks_and_never_reuse_numbers() {
        let mut p = Portals::default();
        let a = p.create(0, shape(), frame(10, 2, 3), "alice").unwrap();
        assert_eq!(p.at(0, BlockPos::new(11, 42, 10)), Some(a));
        assert_eq!(p.at(1, BlockPos::new(11, 42, 10)), None, "planets apart");
        assert_eq!(p.at(0, BlockPos::new(12, 42, 10)), None);
        assert!(
            p.create(0, shape(), frame(11, 1, 2), "alice").is_none(),
            "overlap refused"
        );
        // Across the seam: a frame starting off the lap is stored on it.
        let w = shape().width_blocks();
        let b = p.create(0, shape(), frame(w - 1, 2, 2), "alice").unwrap();
        assert_eq!(p.get(b).unwrap().frame.corner.x, w - 1);
        assert_eq!(p.at(0, BlockPos::new(0, 41, 10)), Some(b));
        let (gone, changed) = p.remove(a, shape()).unwrap();
        assert!(changed.is_empty());
        assert_eq!(gone.id, a);
        assert_eq!(p.at(0, BlockPos::new(11, 42, 10)), None);
        assert_eq!(p.revert, vec![(0, frame(10, 2, 3))]);
        let c = p.create(0, shape(), frame(10, 2, 3), "alice").unwrap();
        assert!(c > b && b > a, "numbers only go up");

        // Saved and loaded: the same records, the same next number.
        let (saved, next) = p.take_dirty();
        let saved: Vec<_> = saved.into_iter().filter_map(|(id, s)| s.map(|s| (id, s))).collect();
        assert_eq!(saved.len(), 2);
        let q = Portals::load(saved, next.unwrap(), |_| Some(shape()));
        assert_eq!(q.get(b), p.get(b));
        assert_eq!(q.at(0, BlockPos::new(0, 41, 10)), Some(b));
        assert_eq!(q.next, c + 1);
        assert_eq!(q.sync.len(), 2, "every block is checked once");
        // A removed top number stays used: the saved next wins.
        let mut r = Portals::load(Vec::new(), c + 1, |_| Some(shape()));
        assert_eq!(r.create(0, shape(), frame(20, 1, 2), "bob"), Some(c + 1));
    }

    #[test]
    fn links_go_both_ways_and_replace_old_ones() {
        let mut p = Portals::default();
        let a = p.create(0, shape(), frame(10, 1, 2), "alice").unwrap();
        let b = p.create(0, shape(), frame(20, 1, 2), "alice").unwrap();
        let c = p.create(1, shape(), frame(20, 1, 2), "alice").unwrap();
        assert!(p.link(a, a).is_empty(), "not to itself");
        assert!(p.link(a, 99).is_empty(), "not to nothing");
        assert_eq!(p.link(a, b), vec![a, b]);
        assert_eq!((p.get(a).unwrap().link, p.get(b).unwrap().link), (Some(b), Some(a)));
        // Linking a elsewhere leaves b unlinked.
        assert_eq!(p.link(a, c), vec![a, b, c]);
        assert_eq!(
            (p.get(a).unwrap().link, p.get(b).unwrap().link, p.get(c).unwrap().link),
            (Some(c), None, Some(a))
        );
        assert_eq!(p.unlink(c), vec![c, a]);
        assert_eq!(p.get(a).unwrap().link, None);
        // Removing a linked portal unlinks its partner.
        p.link(a, b);
        let (_, changed) = p.remove(b, shape()).unwrap();
        assert_eq!(changed, vec![a]);
        assert_eq!(p.get(a).unwrap().link, None);
        // A one-way link from the save is dropped.
        let saved = vec![(
            1,
            SavedPortal {
                planet: 0,
                corner: BlockPos::new(1, 40, 1),
                axis: PortalAxis::X,
                width: 1,
                height: 2,
                owner: "alice".into(),
                link: Some(7),
                fuel: 3,
            },
        )];
        let q = Portals::load(saved, 2, |_| Some(shape()));
        assert_eq!(q.get(1).unwrap().link, None);
    }

    #[test]
    fn tanks_hold_up_to_capacity_and_open_a_linked_side() {
        let mut p = Portals::default();
        let a = p.create(0, shape(), frame(10, 1, 2), "alice").unwrap();
        let b = p.create(0, shape(), frame(20, 1, 2), "alice").unwrap();
        let c = p.create(1, shape(), frame(20, 1, 2), "alice").unwrap();
        assert_eq!(p.add_fuel(a, 400, &RULES), 400);
        assert_eq!(p.add_fuel(a, 400, &RULES), 100, "up to 500");
        assert_eq!(p.add_fuel(a, 1, &RULES), 0);
        assert!(!p.is_open(a, &RULES), "unlinked is dormant");
        p.link(a, b);
        assert!(p.is_open(a, &RULES));
        assert!(!p.is_open(b, &RULES), "each side pays for itself");
        assert_eq!(p.take_fuel(a, 499), 499);
        assert!(p.is_open(a, &RULES), "one shard pays one trip on a planet");
        p.link(a, c);
        assert_eq!(p.trip_cost(a, &RULES), 2);
        assert!(!p.is_open(a, &RULES), "two between planets");
        assert_eq!(p.take_fuel(a, 9), 1);
        assert_eq!(p.take_fuel(a, 9), 0);
    }

    #[test]
    fn travellers_pay_once_per_cooldown() {
        let mut p = Portals::default();
        let a = p.create(0, shape(), frame(10, 1, 2), "alice").unwrap();
        let b = p.create(0, shape(), frame(20, 1, 2), "alice").unwrap();
        p.link(a, b);
        p.add_fuel(a, 2, &RULES);
        assert_eq!(p.charge(a, "bob", 100, &RULES), Charge::Paid(1));
        assert_eq!(p.charge(a, "bob", 200, &RULES), Charge::Free);
        assert_eq!(p.charge(b, "bob", 200, &RULES), Charge::CannotPay, "per portal");
        assert_eq!(p.charge(a, "carol", 200, &RULES), Charge::Paid(1), "per player");
        assert_eq!(p.get(a).unwrap().fuel, 0);
        assert_eq!(p.charge(a, "bob", 100 + RULES.cooldown_ticks - 1, &RULES), Charge::Free);
        assert_eq!(
            p.charge(a, "bob", 100 + RULES.cooldown_ticks, &RULES),
            Charge::CannotPay,
            "due again, and the tank is empty"
        );
        p.forget_old_charges(200 + RULES.cooldown_ticks, &RULES);
        assert!(p.charged.is_empty());
    }
}
