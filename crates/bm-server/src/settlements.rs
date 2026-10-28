//! Prestige per claim and settlements (milestone 24 part 4,
//! `docs/milestones/24-coin-and-trade.md`, research
//! `docs/research/prestige-and-settlements.md`).
//!
//! Each claim keeps a tally of the blocks placed in it (`bm_core::prestige`).
//! Settlements are found from the claims of one planet: claims whose plot
//! columns touch join, as in Boundless, except along thin "roads" (a column
//! with fewer than six plot columns among itself and its eight neighbours),
//! which join nothing. A group of claims with 10,000 prestige or more is a
//! settlement; the owner with the most prestige in it is its warden; the one
//! with the most prestige on a planet is its capital.

use bm_core::prestige::{Rank, Tally};
use bm_core::{PlanetShape, PlotPos};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Every claim's tally (claim ids are the universe's), with what changed
/// since the last save.
#[derive(Default)]
pub struct Tallies {
    map: HashMap<u32, Tally>,
    dirty: BTreeSet<u32>,
}

impl Tallies {
    pub fn load(saved: Vec<(u32, Tally)>) -> Self {
        Tallies {
            map: saved.into_iter().collect(),
            dirty: BTreeSet::new(),
        }
    }

    pub fn get(&self, claim: u32) -> Option<&Tally> {
        self.map.get(&claim)
    }

    /// Changes a claim's tally.
    pub fn change(&mut self, claim: u32, f: impl FnOnce(&mut Tally)) {
        let t = self.map.entry(claim).or_default();
        f(t);
        if t.is_empty() {
            self.map.remove(&claim);
        }
        self.dirty.insert(claim);
    }

    /// A claim is gone.
    pub fn forget(&mut self, claim: u32) {
        if self.map.remove(&claim).is_some() {
            self.dirty.insert(claim);
        }
    }

    pub fn take_dirty(&mut self) -> Vec<(u32, Option<Tally>)> {
        std::mem::take(&mut self.dirty)
            .into_iter()
            .map(|id| (id, self.map.get(&id).cloned()))
            .collect()
    }
}

/// A claim as the settlement finder needs it.
pub struct ClaimPlots<'a> {
    pub id: u32,
    pub owner: &'a str,
    pub owner_name: &'a str,
    pub plots: &'a BTreeSet<PlotPos>,
    pub prestige: u64,
}

/// One settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settlement {
    /// Its claims, lowest id first; the first is its key for its name.
    pub claims: Vec<u32>,
    pub prestige: u64,
    pub rank: Rank,
    /// The owner (account) with the most prestige in it, and as shown.
    pub warden: String,
    pub warden_name: String,
    /// The warden's claim with the most prestige there: the key its name
    /// is kept under, so a renamed settlement keeps its name while its
    /// warden does.
    pub key: u32,
    /// The planet's top settlement.
    pub capital: bool,
}

/// Plot columns count as roads below this many among a column and its
/// eight neighbours (Boundless's rule).
pub const ROAD_BELOW: usize = 6;

/// The settlements among `claims` on a planet of `shape`.
pub fn find(claims: &[ClaimPlots], shape: PlanetShape) -> Vec<Settlement> {
    // Columns (x, z) and who holds them.
    let col = |p: PlotPos| {
        let w = p.wrap(shape);
        (w.x, w.z)
    };
    let mut held: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, c) in claims.iter().enumerate() {
        let cols: BTreeSet<(i32, i32)> = c.plots.iter().map(|&p| col(p)).collect();
        for k in cols {
            held.entry(k).or_default().push(i);
        }
    }
    let at = |x: i32, z: i32, dx: i32, dz: i32| {
        col(PlotPos {
            x: x + dx,
            y: 0,
            z: z + dz,
        })
    };
    let road = |&(x, z): &(i32, i32)| {
        let mut n = 0;
        for dx in -1..=1 {
            for dz in -1..=1 {
                if held.contains_key(&at(x, z, dx, dz)) {
                    n += 1;
                }
            }
        }
        n < ROAD_BELOW
    };
    // Union-find over claims: two claims join where their non-road columns
    // are the same or side by side.
    let mut parent: Vec<usize> = (0..claims.len()).collect();
    fn root(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            p[r] = p[p[r]];
            r = p[r];
        }
        r
    }
    let mut joined = vec![false; claims.len()];
    for (&(x, z), here) in &held {
        if road(&(x, z)) {
            continue;
        }
        for &i in here {
            joined[i] = true;
        }
        let mut near = here.clone();
        for (dx, dz) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            let n = at(x, z, dx, dz);
            if let Some(o) = held.get(&n)
                && !road(&n)
            {
                near.extend(o.iter().copied());
            }
        }
        for w in near.windows(2) {
            let (a, b) = (root(&mut parent, w[0]), root(&mut parent, w[1]));
            if a != b {
                parent[a] = b;
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, &joins) in joined.iter().enumerate() {
        // A claim all of road joins nothing, but stands alone.
        let r = if joins {
            root(&mut parent, i)
        } else {
            i + claims.len()
        };
        groups.entry(r).or_default().push(i);
    }
    let mut out: Vec<Settlement> = groups
        .into_values()
        .filter_map(|members| {
            let prestige: u64 = members.iter().map(|&i| claims[i].prestige).sum();
            let rank = Rank::of(prestige)?;
            let mut by_owner: BTreeMap<&str, u64> = BTreeMap::new();
            for &i in &members {
                *by_owner.entry(claims[i].owner).or_insert(0) += claims[i].prestige;
            }
            // The most prestige; ties to the owner first by name.
            let (&warden, _) = by_owner
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .expect("not empty");
            let top = members
                .iter()
                .filter(|&&i| claims[i].owner == warden)
                .max_by_key(|&&i| (claims[i].prestige, std::cmp::Reverse(claims[i].id)))
                .copied()
                .expect("the warden has one");
            let mut ids: Vec<u32> = members.iter().map(|&i| claims[i].id).collect();
            ids.sort_unstable();
            Some(Settlement {
                claims: ids,
                prestige,
                rank,
                warden: warden.to_string(),
                warden_name: claims[top].owner_name.to_string(),
                key: claims[top].id,
                capital: false,
            })
        })
        .collect();
    if let Some(best) = out
        .iter_mut()
        .max_by(|a, b| a.prestige.cmp(&b.prestige).then(b.key.cmp(&a.key)))
    {
        best.capital = true;
    }
    out.sort_by_key(|s| s.key);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape() -> PlanetShape {
        PlanetShape::new(64, 8)
    }

    /// A rectangle of plot columns at y 10.
    fn block(x0: i32, z0: i32, w: i32, d: i32) -> BTreeSet<PlotPos> {
        let mut s = BTreeSet::new();
        for x in x0..x0 + w {
            for z in z0..z0 + d {
                s.insert(PlotPos { x, y: 10, z });
            }
        }
        s
    }

    fn claim<'a>(id: u32, owner: &'a str, plots: &'a BTreeSet<PlotPos>, prestige: u64) -> ClaimPlots<'a> {
        ClaimPlots {
            id,
            owner,
            owner_name: owner,
            plots,
            prestige,
        }
    }

    #[test]
    fn touching_claims_join_and_the_richest_owner_is_warden() {
        let (a, b, far) = (block(0, 0, 3, 3), block(3, 0, 3, 3), block(30, 30, 3, 3));
        let s = find(
            &[
                claim(1, "alice", &a, 8_000),
                claim(2, "bob", &b, 30_000),
                claim(3, "carol", &far, 12_000),
            ],
            shape(),
        );
        assert_eq!(s.len(), 2);
        let main = s.iter().find(|s| s.claims == vec![1, 2]).expect("joined");
        assert_eq!(
            (main.prestige, main.rank, main.warden.as_str()),
            (38_000, Rank::Outpost, "bob")
        );
        assert!(main.capital, "the planet's top one");
        let lone = s.iter().find(|s| s.claims == vec![3]).unwrap();
        assert!(!lone.capital);
    }

    #[test]
    fn small_groups_are_not_settlements_and_roads_join_nothing() {
        let a = block(0, 0, 3, 3);
        let small = find(&[claim(1, "alice", &a, 9_999)], shape());
        assert!(small.is_empty(), "under 10,000");
        // Two towns linked by a one-plot-wide road of a third claim: three
        // separate groups, not one.
        let (west, east) = (block(0, 0, 3, 3), block(10, 0, 3, 3));
        let road: BTreeSet<PlotPos> = (3..10).map(|x| PlotPos { x, y: 10, z: 1 }).collect();
        let s = find(
            &[
                claim(1, "alice", &west, 20_000),
                claim(2, "bob", &east, 20_000),
                claim(3, "carol", &road, 20_000),
            ],
            shape(),
        );
        assert_eq!(s.len(), 3, "{s:?}");
    }

    #[test]
    fn claims_join_across_the_planets_seam() {
        let w = (shape().width_blocks() >> bm_core::plots::PLOT_BITS) as i32;
        let (a, b) = (block(w - 3, 0, 3, 3), block(0, 0, 3, 3));
        let s = find(&[claim(1, "alice", &a, 6_000), claim(2, "bob", &b, 6_000)], shape());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].claims, vec![1, 2]);
    }
}
