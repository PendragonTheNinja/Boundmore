//! Claims: land owned by an account, in 8×8×8 plots.
//!
//! A claim begins where its owner places a claim stone (the stone's plot) and
//! grows a plot at a time through faces, so it is always one connected piece:
//! a plot is released only if every remaining plot still reaches the stone's.
//! All plots are canonical (`PlotPos::wrap`), so a claim straddling the
//! planet's seam is one claim. Each planet has its own `Claims`; claim ids are
//! unique across the universe (the game hands them out), so an id from one
//! planet never names a claim on another.

use crate::store::SavedClaim;
use bm_core::{BlockPos, PlanetShape, PlotPos};
use bm_protocol::ClaimView;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

#[derive(Clone, Debug, PartialEq)]
pub struct Claim {
    pub id: u32,
    /// Lowercased account name.
    pub owner: String,
    pub owner_name: String,
    /// Canonical position of the claim stone.
    pub stone: BlockPos,
    pub plots: BTreeSet<PlotPos>,
}

impl Claim {
    pub fn view(&self) -> ClaimView {
        ClaimView {
            id: self.id,
            owner: self.owner_name.clone(),
            stone: self.stone,
            plots: self.plots.iter().copied().collect(),
            // Filled in by the game, which keeps the rights.
            rights: Vec::new(),
        }
    }
}

/// Why a claim change was refused, worded for the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimError {
    AlreadyClaimed,
    OwnedBy(String),
    Allowance(u32),
    NotYours,
    NotAdjacent,
    OutsidePlanet,
    StonePlot,
    WouldSplit,
    /// The plot touches the ground kept clear around a warp shrine.
    Shrine,
}

impl std::fmt::Display for ClaimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaimError::AlreadyClaimed => write!(f, "This plot is already claimed"),
            ClaimError::OwnedBy(name) => write!(f, "This land belongs to {name}"),
            ClaimError::Allowance(n) => write!(f, "You have claimed all {n} of your plots"),
            ClaimError::NotYours => write!(f, "That is not your claim"),
            ClaimError::NotAdjacent => write!(f, "A new plot must touch your claim"),
            ClaimError::OutsidePlanet => write!(f, "That plot is outside the planet"),
            ClaimError::StonePlot => write!(f, "The claim stone's plot cannot be released"),
            ClaimError::WouldSplit => write!(f, "Releasing that plot would split your claim"),
            ClaimError::Shrine => write!(f, "The ground around a warp shrine cannot be claimed"),
        }
    }
}

pub struct Claims {
    planet: u16,
    shape: PlanetShape,
    by_id: BTreeMap<u32, Claim>,
    /// Which claim holds each plot.
    plots: HashMap<PlotPos, u32>,
    stones: HashMap<BlockPos, u32>,
    /// Plots nobody may claim (around the warp shrine).
    reserved: BTreeSet<PlotPos>,
    /// Claims changed (or removed) since the last save.
    dirty: BTreeSet<u32>,
}

impl Claims {
    pub fn new(planet: u16, shape: PlanetShape) -> Self {
        Self {
            planet,
            shape,
            by_id: BTreeMap::new(),
            plots: HashMap::new(),
            stones: HashMap::new(),
            reserved: BTreeSet::new(),
            dirty: BTreeSet::new(),
        }
    }

    /// Rebuilds from the save (this planet's claims). A plot claimed twice
    /// (which no rule allows) stays with the claim loaded first.
    pub fn load(planet: u16, shape: PlanetShape, saved: Vec<(u32, SavedClaim)>) -> Self {
        let mut c = Self::new(planet, shape);
        for (id, s) in saved {
            let stone = shape.wrap_block(s.stone);
            let mut plots = BTreeSet::new();
            for p in s.plots {
                let p = p.wrap(shape);
                if c.plots.contains_key(&p) {
                    tracing::error!("claim {id}: plot {p:?} already belongs to another claim; skipped");
                    continue;
                }
                plots.insert(p);
            }
            for p in &plots {
                c.plots.insert(*p, id);
            }
            c.stones.insert(stone, id);
            c.by_id.insert(
                id,
                Claim {
                    id,
                    owner: s.owner,
                    owner_name: s.owner_name,
                    stone,
                    plots,
                },
            );
        }
        c
    }

    /// Keeps these plots out of every claim from now on. Claims already
    /// holding one keep it (only a new claim or plot is refused).
    pub fn reserve(&mut self, plots: impl IntoIterator<Item = PlotPos>) {
        let shape = self.shape;
        self.reserved.extend(plots.into_iter().map(|p| p.wrap(shape)));
    }

    pub fn get(&self, id: u32) -> Option<&Claim> {
        self.by_id.get(&id)
    }

    /// Every claim on the planet.
    pub fn all(&self) -> impl Iterator<Item = &Claim> {
        self.by_id.values()
    }

    /// The claim holding a block, if any.
    pub fn at_block(&self, p: BlockPos) -> Option<&Claim> {
        self.at(PlotPos::of(p, self.shape))
    }

    pub fn at(&self, plot: PlotPos) -> Option<&Claim> {
        self.plots.get(&plot.wrap(self.shape)).and_then(|id| self.by_id.get(id))
    }

    /// The claim whose stone is at this block.
    pub fn by_stone(&self, p: BlockPos) -> Option<&Claim> {
        self.stones
            .get(&self.shape.wrap_block(p))
            .and_then(|id| self.by_id.get(id))
    }

    /// Plots an account holds across all its claims.
    pub fn used_by(&self, owner: &str) -> u32 {
        self.by_id
            .values()
            .filter(|c| c.owner == owner)
            .map(|c| c.plots.len() as u32)
            .sum()
    }

    /// Whether a claim stone may claim the plot holding `stone`.
    pub fn can_create(&self, owner: &str, stone: BlockPos, allowance: u32) -> Result<(), ClaimError> {
        match self.at_block(stone) {
            Some(c) if c.owner == owner => return Err(ClaimError::AlreadyClaimed),
            Some(c) => return Err(ClaimError::OwnedBy(c.owner_name.clone())),
            None => {}
        }
        if self.reserved.contains(&PlotPos::of(stone, self.shape)) {
            return Err(ClaimError::Shrine);
        }
        if self.used_by(owner) >= allowance {
            return Err(ClaimError::Allowance(allowance));
        }
        Ok(())
    }

    /// Creates claim `id` (unused anywhere in the universe). `allowance` is
    /// what the owner may hold on this planet.
    pub fn create(
        &mut self,
        id: u32,
        owner: &str,
        owner_name: &str,
        stone: BlockPos,
        allowance: u32,
    ) -> Result<u32, ClaimError> {
        self.can_create(owner, stone, allowance)?;
        let stone = self.shape.wrap_block(stone);
        let plot = PlotPos::of(stone, self.shape);
        self.plots.insert(plot, id);
        self.stones.insert(stone, id);
        self.by_id.insert(
            id,
            Claim {
                id,
                owner: owner.to_string(),
                owner_name: owner_name.to_string(),
                stone,
                plots: BTreeSet::from([plot]),
            },
        );
        self.dirty.insert(id);
        Ok(id)
    }

    /// Adds an unclaimed plot touching claim `id` by a face.
    pub fn add_plot(&mut self, owner: &str, id: u32, plot: PlotPos, allowance: u32) -> Result<(), ClaimError> {
        let shape = self.shape;
        let plot = plot.wrap(shape);
        if !plot.in_planet(shape) {
            return Err(ClaimError::OutsidePlanet);
        }
        let claim = self
            .by_id
            .get(&id)
            .filter(|c| c.owner == owner)
            .ok_or(ClaimError::NotYours)?;
        if let Some(other) = self.plots.get(&plot).and_then(|i| self.by_id.get(i)) {
            return Err(if other.owner == owner {
                ClaimError::AlreadyClaimed
            } else {
                ClaimError::OwnedBy(other.owner_name.clone())
            });
        }
        if !plot.neighbours(shape).iter().any(|n| claim.plots.contains(n)) {
            return Err(ClaimError::NotAdjacent);
        }
        if self.reserved.contains(&plot) {
            return Err(ClaimError::Shrine);
        }
        if self.used_by(owner) >= allowance {
            return Err(ClaimError::Allowance(allowance));
        }
        self.by_id.get_mut(&id).expect("checked").plots.insert(plot);
        self.plots.insert(plot, id);
        self.dirty.insert(id);
        Ok(())
    }

    /// Releases one plot of the owner's. Returns the claim it left.
    pub fn release_plot(&mut self, owner: &str, plot: PlotPos) -> Result<u32, ClaimError> {
        let shape = self.shape;
        let plot = plot.wrap(shape);
        let claim = self.at(plot).ok_or(ClaimError::NotYours)?;
        if claim.owner != owner {
            return Err(ClaimError::NotYours);
        }
        let home = PlotPos::of(claim.stone, shape);
        if plot == home {
            return Err(ClaimError::StonePlot);
        }
        // Everything left must still reach the stone's plot.
        let mut seen = BTreeSet::from([home]);
        let mut queue = VecDeque::from([home]);
        while let Some(p) = queue.pop_front() {
            for n in p.neighbours(shape) {
                if n != plot && claim.plots.contains(&n) && seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        if seen.len() != claim.plots.len() - 1 {
            return Err(ClaimError::WouldSplit);
        }
        let id = claim.id;
        self.by_id.get_mut(&id).expect("present").plots.remove(&plot);
        self.plots.remove(&plot);
        self.dirty.insert(id);
        Ok(id)
    }

    /// Removes the claim whose stone is at `stone` (its stone was broken).
    pub fn remove_by_stone(&mut self, stone: BlockPos) -> Option<u32> {
        let id = self.stones.remove(&self.shape.wrap_block(stone))?;
        if let Some(c) = self.by_id.remove(&id) {
            for p in &c.plots {
                self.plots.remove(p);
            }
        }
        self.dirty.insert(id);
        Some(id)
    }

    pub fn views(&self) -> Vec<ClaimView> {
        self.by_id.values().map(Claim::view).collect()
    }

    /// Changed claims for the save; None for a removed one.
    pub fn take_dirty(&mut self) -> Vec<(u32, Option<SavedClaim>)> {
        std::mem::take(&mut self.dirty)
            .into_iter()
            .map(|id| {
                let saved = self.by_id.get(&id).map(|c| SavedClaim {
                    planet: self.planet,
                    owner: c.owner.clone(),
                    owner_name: c.owner_name.clone(),
                    stone: c.stone,
                    plots: c.plots.iter().copied().collect(),
                });
                (id, saved)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape() -> PlanetShape {
        PlanetShape::new(8, 4) // 256 blocks around: 32 plots
    }

    #[test]
    fn a_stone_claims_its_plot_once() {
        let mut c = Claims::new(0, shape());
        let id = c.create(1, "alice", "Alice", BlockPos::new(3, 20, 3), 64).unwrap();
        assert_eq!(c.at_block(BlockPos::new(7, 23, 0)).unwrap().id, id);
        assert!(c.at_block(BlockPos::new(8, 20, 3)).is_none());
        assert_eq!(
            c.create(2, "alice", "Alice", BlockPos::new(4, 20, 3), 64),
            Err(ClaimError::AlreadyClaimed)
        );
        assert_eq!(
            c.create(3, "bob", "Bob", BlockPos::new(4, 20, 3), 64),
            Err(ClaimError::OwnedBy("Alice".into()))
        );
        assert_eq!(c.by_stone(BlockPos::new(3, 20, 3)).unwrap().id, id);
        assert_eq!(c.remove_by_stone(BlockPos::new(3, 20, 3)), Some(id));
        assert!(c.at_block(BlockPos::new(3, 20, 3)).is_none());
        assert_eq!(c.take_dirty(), vec![(id, None)]);
    }

    #[test]
    fn claims_grow_through_faces_within_the_allowance_and_across_the_seam() {
        let mut c = Claims::new(0, shape());
        let id = c.create(4, "alice", "Alice", BlockPos::new(0, 0, 0), 3).unwrap();
        assert_eq!(
            c.add_plot("alice", id, PlotPos::new(1, 1, 0), 3),
            Err(ClaimError::NotAdjacent)
        );
        assert_eq!(
            c.add_plot("bob", id, PlotPos::new(1, 0, 0), 3),
            Err(ClaimError::NotYours)
        );
        assert_eq!(
            c.add_plot("alice", id, PlotPos::new(0, -1, 0), 3),
            Err(ClaimError::OutsidePlanet)
        );
        // Plot -1 is plot 31: the claim crosses the seam.
        c.add_plot("alice", id, PlotPos::new(-1, 0, 0), 3).unwrap();
        assert_eq!(c.at(PlotPos::new(31, 0, 0)).unwrap().id, id);
        c.add_plot("alice", id, PlotPos::new(1, 0, 0), 3).unwrap();
        assert_eq!(c.used_by("alice"), 3);
        assert_eq!(
            c.add_plot("alice", id, PlotPos::new(0, 1, 0), 3),
            Err(ClaimError::Allowance(3))
        );
        assert_eq!(
            c.create(5, "alice", "Alice", BlockPos::new(100, 0, 100), 3),
            Err(ClaimError::Allowance(3))
        );
        let bob = c.create(6, "bob", "Bob", BlockPos::new(16, 0, 0), 3).unwrap();
        assert_eq!(
            c.add_plot("bob", bob, PlotPos::new(1, 0, 0), 3),
            Err(ClaimError::OwnedBy("Alice".into()))
        );
    }

    #[test]
    fn releasing_never_splits_a_claim_or_frees_the_stone() {
        let mut c = Claims::new(0, shape());
        let id = c.create(7, "alice", "Alice", BlockPos::new(0, 0, 0), 64).unwrap();
        // A line of plots: 0 - 1 - 2.
        c.add_plot("alice", id, PlotPos::new(1, 0, 0), 64).unwrap();
        c.add_plot("alice", id, PlotPos::new(2, 0, 0), 64).unwrap();
        assert_eq!(
            c.release_plot("alice", PlotPos::new(0, 0, 0)),
            Err(ClaimError::StonePlot)
        );
        assert_eq!(
            c.release_plot("alice", PlotPos::new(1, 0, 0)),
            Err(ClaimError::WouldSplit)
        );
        assert_eq!(c.release_plot("bob", PlotPos::new(2, 0, 0)), Err(ClaimError::NotYours));
        // Closing a ring makes the middle releasable.
        c.add_plot("alice", id, PlotPos::new(0, 0, 1), 64).unwrap();
        c.add_plot("alice", id, PlotPos::new(1, 0, 1), 64).unwrap();
        c.add_plot("alice", id, PlotPos::new(2, 0, 1), 64).unwrap();
        assert_eq!(c.release_plot("alice", PlotPos::new(1, 0, 0)), Ok(id));
        assert!(c.at(PlotPos::new(1, 0, 0)).is_none());
        assert_eq!(c.get(id).unwrap().plots.len(), 5);
    }

    #[test]
    fn reserved_plots_cannot_be_claimed() {
        let mut c = Claims::new(0, shape());
        c.reserve([PlotPos::new(1, 0, 0)]);
        assert_eq!(
            c.create(1, "alice", "Alice", BlockPos::new(9, 0, 0), 64),
            Err(ClaimError::Shrine)
        );
        let id = c.create(2, "alice", "Alice", BlockPos::new(0, 0, 0), 64).unwrap();
        assert_eq!(
            c.add_plot("alice", id, PlotPos::new(1, 0, 0), 64),
            Err(ClaimError::Shrine)
        );
    }

    #[test]
    fn claims_survive_a_save_round_trip() {
        let mut c = Claims::new(0, shape());
        let id = c.create(8, "alice", "Alice", BlockPos::new(-3, 9, 260), 64).unwrap();
        c.add_plot("alice", id, PlotPos::new(31, 2, 0), 64).unwrap();
        let saved: Vec<_> = c.take_dirty().into_iter().map(|(i, s)| (i, s.unwrap())).collect();
        assert!(saved.iter().all(|(_, s)| s.planet == 0));
        let d = Claims::load(0, shape(), saved);
        assert_eq!(d.get(id), c.get(id));
        assert_eq!(d.by_stone(BlockPos::new(253, 9, 4)).unwrap().id, id);
        assert_eq!(d.views(), c.views());
    }
}
