//! Voxel ray traversal (Amanatides & Woo). Visits every block a ray passes
//! through, in order, so targeting can never skip a thin wall.

use crate::coords::BlockPos;
use glam::DVec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub block: BlockPos,
    /// Unit normal of the face the ray entered through. Zero when the ray
    /// started inside the hit block.
    pub normal: [i64; 3],
    pub distance: f64,
}

impl RayHit {
    /// Where a block placed against the hit face would go.
    pub fn adjacent(&self) -> BlockPos {
        self.block.offset(self.normal[0], self.normal[1], self.normal[2])
    }
}

/// Casts from `origin` along `dir` (need not be normalised) up to `max_dist`
/// blocks, returning the first block for which `hit` is true.
pub fn raycast(origin: DVec3, dir: DVec3, max_dist: f64, mut hit: impl FnMut(BlockPos) -> bool) -> Option<RayHit> {
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
        if hit(cell) {
            return Some(RayHit {
                block: cell,
                normal,
                distance: t,
            });
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

    #[test]
    fn degenerate_rays_hit_nothing() {
        assert!(raycast(DVec3::ZERO, DVec3::ZERO, 5.0, |_| true).is_none());
        assert!(raycast(DVec3::splat(f64::NAN), DVec3::X, 5.0, |_| true).is_none());
    }
}
