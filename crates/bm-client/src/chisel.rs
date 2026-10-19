//! Chisels on the client (ADR-0008): which octants a click cuts or fills
//! from where the player aims. Whether it may is `bm_core::chisel::apply`,
//! the same rules the server checks (`check_edit`); its answer wins.

use bm_core::{RayHit, Shape, octant};

/// What a left click would cut and a right click would fill back, as masks
/// of octants. `None` where the click would do nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub cut: Option<u8>,
    pub fill: Option<u8>,
}

fn bit(axis: usize) -> u8 {
    // Octant index is x | z << 1 | y << 2.
    [1, 4, 2][axis]
}

fn coords(i: u8) -> [u8; 3] {
    [i & 1, i >> 2 & 1, i >> 1 & 1]
}

/// The octants a chisel acts on from `hit` on a block of `shape`. A precise
/// chisel takes the single octant struck; an ordinary one the edge pair
/// along the struck face's edge nearest the aimed point (falling back to the
/// other pairs through that octant where the nearest is already cut). The
/// fill is the cut-away octant just in front of the struck face, or else the
/// cut-away one nearest the aimed point.
pub fn plan(hit: &RayHit, shape: Shape, precise: bool) -> Plan {
    let Some(axis) = (0..3).find(|&a| hit.normal[a] != 0) else {
        return Plan::default();
    };
    let base = [hit.block.x as f64, hit.block.y as f64, hit.block.z as f64];
    let p: [f64; 3] = std::array::from_fn(|a| (hit.point[a] - base[a]).clamp(0.0, 1.0));
    let (u, v) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    // The edge nearest the point runs along the in-plane axis the point is
    // nearer the middle of; the pair runs along it too.
    let along = if (p[u] - 0.5).abs() >= (p[v] - 0.5).abs() {
        [v, u, axis]
    } else {
        [u, v, axis]
    };
    let pair = |o: u8, want_kept: bool| -> Option<u8> {
        if precise {
            return Some(1 << o);
        }
        along.iter().find_map(|&a| {
            let partner = o ^ bit(a);
            (shape.keeps(partner) == want_kept).then_some(1 << o | 1 << partner)
        })
    };
    let struck = hit.octant();
    let cut = if shape.keeps(struck) { pair(struck, true) } else { None };
    let removed = shape.removed();
    let fill = if removed == 0 {
        None
    } else {
        let mut front = p;
        front[axis] += hit.normal[axis] as f64 * 0.25;
        let in_front = front.iter().all(|c| (0.0..1.0).contains(c)).then(|| {
            let h = |a: usize| u8::from(front[a] >= 0.5);
            octant(h(0), h(1), h(2))
        });
        let target = match in_front {
            Some(o) if !shape.keeps(o) => o,
            _ => (0..8u8)
                .filter(|&o| !shape.keeps(o))
                .min_by(|&a, &b| distance(p, a).total_cmp(&distance(p, b)))
                .expect("something is removed"),
        };
        pair(target, false)
    };
    Plan { cut, fill }
}

/// Distance from a point (block units) to an octant's box.
fn distance(p: [f64; 3], o: u8) -> f64 {
    let c = coords(o);
    (0..3)
        .map(|a| {
            let lo = c[a] as f64 * 0.5;
            let d = (lo - p[a]).max(p[a] - lo - 0.5).max(0.0);
            d * d
        })
        .sum::<f64>()
}

/// The boxes (block units) outlining `mask`: one per octant, or one for an
/// edge pair.
pub fn outline(mask: u8) -> Vec<([f64; 3], [f64; 3])> {
    let boxes: Vec<([f64; 3], [f64; 3])> = (0..8u8)
        .filter(|o| mask & 1 << o != 0)
        .map(|o| {
            let c = coords(o).map(|v| v as f64 * 0.5);
            (c, [c[0] + 0.5, c[1] + 0.5, c[2] + 0.5])
        })
        .collect();
    match boxes.as_slice() {
        [a, b] => vec![(
            std::array::from_fn(|i| a.0[i].min(b.0[i])),
            std::array::from_fn(|i| a.1[i].max(b.1[i])),
        )],
        _ => boxes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{BlockPos, ShapeFamily, edge_pairs};
    use glam::DVec3;

    fn hit(point: [f64; 3], normal: [i64; 3]) -> RayHit {
        RayHit {
            block: BlockPos::new(10, 20, 30),
            normal,
            distance: 1.0,
            point: DVec3::new(10.0 + point[0], 20.0 + point[1], 30.0 + point[2]),
        }
    }

    #[test]
    fn a_precise_chisel_takes_the_octant_struck() {
        // On top, towards +x and -z: the top octant at x 1, z 0.
        let h = hit([0.8, 1.0, 0.3], [0, 1, 0]);
        let p = plan(&h, Shape::FULL, true);
        assert_eq!(p.cut, Some(1 << octant(1, 1, 0)));
        assert_eq!(p.fill, None, "nothing cut to fill");
    }

    #[test]
    fn an_ordinary_chisel_takes_the_pair_along_the_nearest_edge() {
        // On top, near the +x edge (x 0.95), a little off the middle in z:
        // the pair runs along z on the top +x edge.
        let h = hit([0.95, 1.0, 0.4], [0, 1, 0]);
        let want = 1 << octant(1, 1, 0) | 1 << octant(1, 1, 1);
        assert_eq!(plan(&h, Shape::FULL, false).cut, Some(want));
        // Near the -z edge instead: the pair runs along x.
        let h = hit([0.6, 1.0, 0.02], [0, 1, 0]);
        let want = 1 << octant(0, 1, 0) | 1 << octant(1, 1, 0);
        assert_eq!(plan(&h, Shape::FULL, false).cut, Some(want));
        // On a side face (+x), near the top edge: the pair runs along z.
        let h = hit([1.0, 0.97, 0.3], [1, 0, 0]);
        let want = 1 << octant(1, 1, 0) | 1 << octant(1, 1, 1);
        assert_eq!(plan(&h, Shape::FULL, false).cut, Some(want));
        // Every pair a click can make is one the server accepts.
        for x in 0..10 {
            for z in 0..10 {
                let h = hit([x as f64 / 9.0, 1.0, z as f64 / 9.0], [0, 1, 0]);
                let m = plan(&h, Shape::FULL, false).cut.unwrap();
                assert!(edge_pairs().any(|e| e == m), "{m:#010b}");
            }
        }
    }

    #[test]
    fn a_cut_already_made_turns_the_pair_and_fills_from_in_front() {
        let fam = ShapeFamily::Square;
        // The top +x,+z octant gone; aim at the top of its +x,-z neighbour
        // near the +x edge: the pair along z would take the gone one, so the
        // pair turns along x.
        let shape = Shape::new(fam, 1 << octant(1, 1, 1)).unwrap();
        let h = hit([0.95, 1.0, 0.4], [0, 1, 0]);
        let p = plan(&h, shape, false);
        assert_eq!(p.cut, Some(1 << octant(1, 1, 0) | 1 << octant(0, 1, 0)));
        // Fill: the nearest gone octant, with a gone partner if any (none
        // here, so nothing to fill with an ordinary chisel; precise fills it).
        assert_eq!(p.fill, None);
        assert_eq!(plan(&h, shape, true).fill, Some(1 << octant(1, 1, 1)));
        // Aiming at the bottom half's top, under a cut top: the fill is the
        // octant in front.
        let slab = Shape::new(fam, 0b1111_0000).unwrap();
        let h = hit([0.2, 0.5, 0.7], [0, 1, 0]);
        let p = plan(&h, slab, true);
        assert_eq!(p.cut, Some(1 << octant(0, 0, 1)));
        assert_eq!(p.fill, Some(1 << octant(0, 1, 1)));
        let p = plan(&h, slab, false);
        assert!(p.fill.is_some_and(|m| m.count_ones() == 2 && m & 0b1111_0000 == m));
    }

    #[test]
    fn outlines_merge_an_edge_pair() {
        let edge = 1 << octant(1, 1, 0) | 1 << octant(1, 1, 1);
        assert_eq!(outline(edge), vec![([0.5, 0.5, 0.0], [1.0, 1.0, 1.0])]);
        assert_eq!(outline(1).len(), 1);
    }
}
