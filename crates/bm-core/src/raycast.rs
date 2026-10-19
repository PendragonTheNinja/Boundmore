//! Voxel ray traversal (Amanatides & Woo). Visits every block a ray passes
//! through, in order, so targeting can never skip a thin wall.

use crate::block::Shape;
use crate::coords::BlockPos;
use glam::DVec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub block: BlockPos,
    /// Unit normal of the face the ray entered through: the block's face, or
    /// for a chiselled block the face of the kept octant it struck. Zero when
    /// the ray started inside what it hit.
    pub normal: [i64; 3],
    pub distance: f64,
    /// Where the ray struck, in world coordinates (on the face it entered
    /// through; the ray's origin when it started inside).
    pub point: DVec3,
}

impl RayHit {
    /// Where a block placed against the hit face would go.
    pub fn adjacent(&self) -> BlockPos {
        self.block.offset(self.normal[0], self.normal[1], self.normal[2])
    }

    /// The octant of the hit block the ray struck (`shape::octant` order):
    /// the half-block just inside the hit point. What a chisel cuts from.
    pub fn octant(&self) -> u8 {
        let base = [self.block.x as f64, self.block.y as f64, self.block.z as f64];
        let half = |a: usize| {
            // Nudged inwards off the face, so a point on the face between two
            // octants picks the one the ray went into.
            let v = self.point[a] - base[a] - self.normal[a] as f64 * 1e-6;
            u8::from(v >= 0.5)
        };
        crate::shape::octant(half(0), half(1), half(2))
    }
}

/// Casts from `origin` along `dir` (need not be normalised) up to `max_dist`
/// blocks, returning the first block for which `hit` is true. Every block it
/// hits is treated as a full cube; [`raycast_shaped`] honours chisel cuts.
pub fn raycast(origin: DVec3, dir: DVec3, max_dist: f64, mut hit: impl FnMut(BlockPos) -> bool) -> Option<RayHit> {
    traverse(origin, dir, max_dist, |cell| hit(cell).then_some(Shape::FULL))
}

/// As [`raycast`], for a world with chiselled blocks (ADR-0008): `hit` gives
/// the shape of a block the ray may hit, or `None` to pass through it. A
/// shaped block is hit only on its kept octants; a ray through the octants
/// cut away goes on to whatever is behind.
pub fn raycast_shaped(
    origin: DVec3,
    dir: DVec3,
    max_dist: f64,
    hit: impl FnMut(BlockPos) -> Option<Shape>,
) -> Option<RayHit> {
    traverse(origin, dir, max_dist, hit)
}

fn traverse(
    origin: DVec3,
    dir: DVec3,
    max_dist: f64,
    mut shape_at: impl FnMut(BlockPos) -> Option<Shape>,
) -> Option<RayHit> {
    let len = dir.length();
    if !len.is_finite() || len == 0.0 || !origin.is_finite() {
        return None;
    }
    let d = dir / len;
    let mut cell = BlockPos::containing(origin.x, origin.y, origin.z);
    let step = [sign(d.x), sign(d.y), sign(d.z)];
    let o = [origin.x, origin.y, origin.z];
    let dv = [d.x, d.y, d.z];
    let c = [cell.x, cell.y, cell.z];
    let mut t_max = [f64::INFINITY; 3];
    let mut t_delta = [f64::INFINITY; 3];
    for a in 0..3 {
        if step[a] != 0 {
            t_delta[a] = 1.0 / dv[a].abs();
            let boundary = if step[a] > 0 { c[a] as f64 + 1.0 } else { c[a] as f64 };
            t_max[a] = (boundary - o[a]) / dv[a];
        }
    }
    let mut normal = [0i64; 3];
    let mut t = 0.0;
    loop {
        if let Some(shape) = shape_at(cell) {
            if shape.is_full() {
                return Some(RayHit {
                    block: cell,
                    normal,
                    distance: t,
                    point: origin + d * t,
                });
            }
            if let Some((bt, bn)) = hit_boxes(origin, d, step, cell, shape)
                && bt <= max_dist
            {
                return Some(RayHit {
                    block: cell,
                    normal: bn,
                    distance: bt,
                    point: origin + d * bt,
                });
            }
        }
        let a = if t_max[0] < t_max[1] {
            if t_max[0] < t_max[2] { 0 } else { 2 }
        } else if t_max[1] < t_max[2] {
            1
        } else {
            2
        };
        t = t_max[a];
        if t > max_dist {
            return None;
        }
        t_max[a] += t_delta[a];
        normal = [0; 3];
        normal[a] = -step[a];
        match a {
            0 => cell.x += step[0],
            1 => cell.y += step[1],
            _ => cell.z += step[2],
        }
    }
}

/// The nearest kept octant of `shape` at `cell` the ray strikes: its
/// distance and entry normal (zero if the ray starts inside it).
fn hit_boxes(origin: DVec3, d: DVec3, step: [i64; 3], cell: BlockPos, shape: Shape) -> Option<(f64, [i64; 3])> {
    let base = DVec3::new(cell.x as f64, cell.y as f64, cell.z as f64);
    let mut best: Option<(f64, [i64; 3])> = None;
    for (l, h) in shape.boxes() {
        let (lo, hi) = (base + DVec3::from_array(l), base + DVec3::from_array(h));
        let (mut near, mut far, mut axis) = (f64::NEG_INFINITY, f64::INFINITY, usize::MAX);
        let mut miss = false;
        for a in 0..3 {
            if d[a] == 0.0 {
                if origin[a] < lo[a] || origin[a] > hi[a] {
                    miss = true;
                }
                continue;
            }
            let (t0, t1) = ((lo[a] - origin[a]) / d[a], (hi[a] - origin[a]) / d[a]);
            let (t0, t1) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
            if t0 > near {
                near = t0;
                axis = a;
            }
            far = far.min(t1);
        }
        if miss || near > far || far < 0.0 {
            continue;
        }
        let (t, n) = if near < 0.0 || axis == usize::MAX {
            (0.0, [0; 3])
        } else {
            let mut n = [0i64; 3];
            n[axis] = -step[axis];
            (near, n)
        };
        if best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, n));
        }
    }
    best
}

fn sign(v: f64) -> i64 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_a_block_straight_ahead_with_the_right_face() {
        let target = BlockPos::new(5, 0, 0);
        let h = raycast(DVec3::new(0.5, 0.5, 0.5), DVec3::X, 10.0, |p| p == target).unwrap();
        assert_eq!(h.block, target);
        assert_eq!(h.normal, [-1, 0, 0]);
        assert!((h.distance - 4.5).abs() < 1e-9);
        assert_eq!(h.adjacent(), BlockPos::new(4, 0, 0));
    }

    #[test]
    fn respects_max_distance() {
        let target = BlockPos::new(0, -9, 0);
        assert!(raycast(DVec3::new(0.5, 0.5, 0.5), -DVec3::Y, 5.0, |p| p == target).is_none());
    }

    #[test]
    fn diagonal_rays_visit_a_connected_path() {
        let mut visited = Vec::new();
        raycast(DVec3::new(-0.3, 0.2, 0.7), DVec3::new(1.0, 0.37, -0.81), 20.0, |p| {
            visited.push(p);
            false
        });
        for w in visited.windows(2) {
            let d = (w[1].x - w[0].x).abs() + (w[1].y - w[0].y).abs() + (w[1].z - w[0].z).abs();
            assert_eq!(d, 1, "each step moves to a face neighbour");
        }
        assert!(visited.len() > 20);
    }

    fn slab() -> Shape {
        Shape::new(crate::shape::ShapeFamily::Square, 0xf0).unwrap()
    }

    #[test]
    fn rays_pass_through_cut_octants_and_hit_kept_ones() {
        let target = BlockPos::new(5, 0, 0);
        let behind = BlockPos::new(7, 0, 0);
        let world = |p: BlockPos| {
            if p == target {
                Some(slab())
            } else if p == behind {
                Some(Shape::FULL)
            } else {
                None
            }
        };
        // Through the upper half (cut away): on to the block behind.
        let h = raycast_shaped(DVec3::new(0.5, 0.75, 0.5), DVec3::X, 10.0, world).unwrap();
        assert_eq!((h.block, h.normal), (behind, [-1, 0, 0]));
        assert!((h.point - DVec3::new(7.0, 0.75, 0.5)).length() < 1e-9);
        // Through the lower half: the slab's side.
        let h = raycast_shaped(DVec3::new(0.5, 0.25, 0.75), DVec3::X, 10.0, world).unwrap();
        assert_eq!((h.block, h.normal), (target, [-1, 0, 0]));
        assert!((h.distance - 4.5).abs() < 1e-9);
        assert_eq!(h.octant(), crate::shape::octant(0, 0, 1));
        // Down from above: through the cut half onto the slab's top, at 0.5.
        let h = raycast_shaped(DVec3::new(5.8, 3.0, 0.2), -DVec3::Y, 10.0, world).unwrap();
        assert_eq!((h.block, h.normal), (target, [0, 1, 0]));
        assert!((h.point.y - 0.5).abs() < 1e-9 && (h.distance - 2.5).abs() < 1e-9);
        assert_eq!(h.octant(), crate::shape::octant(1, 0, 0));
        // A diagonal ray that crosses the cut corner misses it.
        let corner = Shape::new(crate::shape::ShapeFamily::Square, 1 << crate::shape::octant(0, 1, 0)).unwrap();
        let only = |p: BlockPos| (p == target).then_some(corner);
        let h = raycast_shaped(DVec3::new(4.9, 0.95, 0.25), DVec3::new(1.0, -0.2, 0.0), 10.0, only).unwrap();
        assert_eq!(h.normal, [-1, 0, 0]);
        assert!(h.point.x > 5.5 - 1e-9, "went past the cut octant: {:?}", h.point);
    }

    #[test]
    fn plain_hits_report_where_they_struck() {
        let h = raycast(DVec3::new(0.5, 0.2, 0.9), DVec3::new(1.0, 0.1, 0.0), 10.0, |p| p.x == 3).unwrap();
        assert!((h.point.x - 3.0).abs() < 1e-9);
        assert_eq!(h.octant(), crate::shape::octant(0, 0, 1));
    }

    #[test]
    fn degenerate_rays_hit_nothing() {
        assert!(raycast(DVec3::ZERO, DVec3::ZERO, 5.0, |_| true).is_none());
        assert!(raycast(DVec3::splat(f64::NAN), DVec3::X, 5.0, |_| true).is_none());
    }
}
