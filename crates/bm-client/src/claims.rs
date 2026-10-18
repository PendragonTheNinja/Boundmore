//! What the client knows of claims: every `ClaimView` the server sends, an
//! index from plot to claim, and the outlines claim mode draws.

use bm_core::{PLOT_SIZE, PlanetShape, PlotPos};
use bm_protocol::ClaimView;
use glam::DVec3;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Claims {
    claims: HashMap<u32, ClaimView>,
    by_plot: HashMap<PlotPos, u32>,
    shape: Option<PlanetShape>,
}

impl Claims {
    pub fn new(shape: PlanetShape, list: Vec<ClaimView>) -> Self {
        let mut c = Self {
            shape: Some(shape),
            ..Default::default()
        };
        for v in list {
            c.upsert(v);
        }
        c
    }

    fn wrap(&self, p: PlotPos) -> PlotPos {
        match self.shape {
            Some(s) => p.wrap(s),
            None => p,
        }
    }

    /// A claim created or changed: replaces what we had for it.
    pub fn upsert(&mut self, mut view: ClaimView) {
        self.remove(view.id);
        for p in view.plots.iter_mut() {
            *p = self.wrap(*p);
        }
        for p in &view.plots {
            self.by_plot.insert(*p, view.id);
        }
        self.claims.insert(view.id, view);
    }

    pub fn remove(&mut self, id: u32) {
        if let Some(old) = self.claims.remove(&id) {
            for p in old.plots {
                if self.by_plot.get(&p) == Some(&id) {
                    self.by_plot.remove(&p);
                }
            }
        }
    }

    pub fn at(&self, plot: PlotPos) -> Option<&ClaimView> {
        self.by_plot.get(&self.wrap(plot)).and_then(|id| self.claims.get(id))
    }

    pub fn iter(&self) -> impl Iterator<Item = &ClaimView> {
        self.claims.values()
    }

    /// Plots `owner` holds in all their claims, against the allowance.
    pub fn plots_owned_by(&self, owner: &str) -> usize {
        self.iter().filter(|c| c.owner == owner).map(|c| c.plots.len()).sum()
    }

    /// One of `owner`'s claims that `plot` touches by a face, the only way a
    /// claim may grow.
    pub fn adjacent_claim_of(&self, owner: &str, plot: PlotPos) -> Option<u32> {
        let shape = self.shape?;
        let mut ids: Vec<u32> = plot
            .neighbours(shape)
            .iter()
            .filter_map(|n| self.at(*n))
            .filter(|c| c.owner == owner)
            .map(|c| c.id)
            .collect();
        // Deterministic when a plot touches two of one's own claims.
        ids.sort_unstable();
        ids.first().copied()
    }
}

/// One edge of a plot outline: from `corner` (in plot units, where a plot
/// `p` spans corners `p` to `p + 1`) one plot along `axis` (0 x, 1 y, 2 z).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Edge {
    pub corner: [i32; 3],
    pub axis: usize,
}

impl Edge {
    /// Both ends in blocks, at the lap nearest `eye` (positions are canonical
    /// but the camera may stand across the seam from the claim).
    pub fn endpoints(self, shape: PlanetShape, eye: DVec3) -> [DVec3; 2] {
        let s = PLOT_SIZE as f64;
        let a = DVec3::new(self.corner[0] as f64, self.corner[1] as f64, self.corner[2] as f64) * s;
        let a = shape.nearest_lap(eye, a);
        let mut b = a;
        b[self.axis] += s;
        [a, b]
    }
}

/// The edges outlining a set of plots as one solid: every edge where the
/// surface turns, and none across a flat face or inside the volume, so a big
/// claim reads as one shape rather than a lattice.
pub fn outline(plots: &HashSet<PlotPos>, shape: PlanetShape) -> Vec<Edge> {
    let width = (shape.width_blocks() / PLOT_SIZE) as i32;
    let wrap = |c: [i32; 3]| [c[0].rem_euclid(width), c[1], c[2].rem_euclid(width)];
    let member = |x: i32, y: i32, z: i32| plots.contains(&PlotPos::new(x, y, z).wrap(shape));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for p in plots {
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for du in 0..2 {
                for dv in 0..2 {
                    let mut c = [p.x, p.y, p.z];
                    c[u] += du;
                    c[v] += dv;
                    let edge = Edge { corner: wrap(c), axis };
                    if !seen.insert(edge) {
                        continue;
                    }
                    // The four plots around this edge, in turn round it.
                    let cell = |a: i32, b: i32| {
                        let mut q = edge.corner;
                        q[u] += a;
                        q[v] += b;
                        member(q[0], q[1], q[2])
                    };
                    let ring = [cell(-1, -1), cell(0, -1), cell(0, 0), cell(-1, 0)];
                    let n = ring.iter().filter(|m| **m).count();
                    let diagonal = n == 2 && ring[0] == ring[2];
                    if n == 1 || n == 3 || diagonal {
                        out.push(edge);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::BlockPos;

    fn shape() -> PlanetShape {
        PlanetShape::new(8, 4) // 256 blocks: 32 plots around
    }

    fn set(p: &[(i32, i32, i32)]) -> HashSet<PlotPos> {
        p.iter().map(|&(x, y, z)| PlotPos::new(x, y, z)).collect()
    }

    #[test]
    fn a_single_plot_is_a_cube() {
        assert_eq!(outline(&set(&[(3, 2, 3)]), shape()).len(), 12);
    }

    #[test]
    fn shared_faces_draw_no_lines() {
        // Two plots side by side: a 2x1x1 box is 16 unit edges, none of them
        // on the face the plots share.
        let e = outline(&set(&[(3, 2, 3), (4, 2, 3)]), shape());
        assert_eq!(e.len(), 16);
        assert!(!e.iter().any(|e| e.axis != 0 && e.corner[0] == 4));
        // A solid 2x2x2 block outlines like one cube with each edge halved.
        let cube: Vec<_> = (0..8).map(|i| (i & 1, (i >> 1) & 1, (i >> 2) & 1)).collect();
        assert_eq!(outline(&set(&cube), shape()).len(), 24);
    }

    #[test]
    fn concave_and_diagonal_corners_are_drawn() {
        // An L of three plots: 8 edges round the top, 8 round the bottom and
        // 6 corners, the inner one included.
        assert_eq!(outline(&set(&[(0, 0, 0), (1, 0, 0), (0, 0, 1)]), shape()).len(), 22);
        // Two plots touching only along an edge keep that edge.
        let e = outline(&set(&[(0, 0, 0), (1, 0, 1)]), shape());
        assert!(e.contains(&Edge {
            corner: [1, 0, 1],
            axis: 1
        }));
    }

    #[test]
    fn outlines_join_across_the_seam() {
        let s = shape();
        let e = outline(&set(&[(31, 2, 5), (0, 2, 5)]), s);
        assert_eq!(e.len(), 16, "one box, not two");
        // Drawn from just east of the seam, every edge is within reach.
        let eye = DVec3::new(2.0, 20.0, 44.0);
        for edge in e {
            for p in edge.endpoints(s, eye) {
                assert!((p - eye).length() < 40.0, "{edge:?} at {p}");
            }
        }
    }

    #[test]
    fn plots_index_their_claims_and_growth_needs_a_face() {
        let s = shape();
        let mine = ClaimView {
            id: 7,
            owner: "Pendragon".into(),
            stone: BlockPos::new(0, 16, 0),
            plots: vec![PlotPos::new(0, 2, 0), PlotPos::new(-1, 2, 0)],
        };
        let mut c = Claims::new(s, vec![mine.clone()]);
        assert_eq!(
            c.at(PlotPos::new(31, 2, 0)).map(|v| v.id),
            Some(7),
            "wrapped on arrival"
        );
        assert_eq!(c.plots_owned_by("Pendragon"), 2);
        assert_eq!(c.adjacent_claim_of("Pendragon", PlotPos::new(1, 2, 0)), Some(7));
        assert_eq!(
            c.adjacent_claim_of("Pendragon", PlotPos::new(1, 2, 1)),
            None,
            "diagonal"
        );
        assert_eq!(c.adjacent_claim_of("Wren", PlotPos::new(1, 2, 0)), None);
        // An update replaces the old plots; removal clears the index.
        c.upsert(ClaimView {
            plots: vec![PlotPos::new(0, 2, 0)],
            ..mine
        });
        assert!(c.at(PlotPos::new(31, 2, 0)).is_none());
        c.remove(7);
        assert!(c.at(PlotPos::new(0, 2, 0)).is_none());
    }
}
