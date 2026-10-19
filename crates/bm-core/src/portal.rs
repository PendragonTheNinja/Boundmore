//! Portal frames (ADR-0009): the pure rules shared by server and client.
//!
//! A portal is a filled, upright rectangle of conduit blocks. The server
//! finds it from any one conduit with [`find_frame`] and never takes its
//! shape from a client. Positions passed in may be off the canonical lap;
//! the caller's `is_conduit` wraps them, as `PhysicsWorld` does.

use crate::coords::BlockPos;
use serde::{Deserialize, Serialize};

pub const MIN_WIDTH: u8 = 1;
pub const MAX_WIDTH: u8 = 6;
pub const MIN_HEIGHT: u8 = 2;
pub const MAX_HEIGHT: u8 = 6;

/// What a misshapen frame is told (player-facing).
pub const SHAPE_RULE: &str = "A portal is a filled rectangle of conduits, 1 to 6 wide and 2 to 6 tall";

/// The horizontal axis a frame spans. `X`: it lies in an X-Y plane and is
/// walked through along Z. `Z`: it lies in a Z-Y plane, walked through along X.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortalAxis {
    X,
    Z,
}

/// A portal's rectangle: its lowest corner, the axis it spans, its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortalFrame {
    pub corner: BlockPos,
    pub axis: PortalAxis,
    pub width: u8,
    pub height: u8,
}

impl PortalFrame {
    /// The block `i` along the span and `j` up from the corner.
    pub fn at(&self, i: u8, j: u8) -> BlockPos {
        let (i, j) = (i as i64, j as i64);
        match self.axis {
            PortalAxis::X => self.corner.offset(i, j, 0),
            PortalAxis::Z => self.corner.offset(0, j, i),
        }
    }

    /// Every block of the rectangle, row by row from the bottom.
    pub fn blocks(&self) -> impl Iterator<Item = BlockPos> + '_ {
        (0..self.height).flat_map(move |j| (0..self.width).map(move |i| self.at(i, j)))
    }

    /// Whether `p` (on the same lap as `corner`) is part of the rectangle.
    pub fn contains(&self, p: BlockPos) -> bool {
        let d = (p.x - self.corner.x, p.y - self.corner.y, p.z - self.corner.z);
        let (along, across) = match self.axis {
            PortalAxis::X => (d.0, d.2),
            PortalAxis::Z => (d.2, d.0),
        };
        across == 0 && (0..self.width as i64).contains(&along) && (0..self.height as i64).contains(&d.1)
    }

    /// The two horizontal unit directions out of the rectangle's faces,
    /// `[+, -]` along the axis it is walked through.
    pub fn normals(&self) -> [(i64, i64); 2] {
        match self.axis {
            PortalAxis::X => [(0, 1), (0, -1)],
            PortalAxis::Z => [(1, 0), (-1, 0)],
        }
    }

    /// Where a traveller arriving through this portal could stand, best
    /// first: the block in front of the bottom middle on each face (for a
    /// one-wide frame, on all four sides). The caller takes the first with
    /// room for a body and ground under it.
    pub fn exit_feet(&self) -> Vec<(BlockPos, (i64, i64))> {
        let base = self.at((self.width - 1) / 2, 0);
        let mut dirs = self.normals().to_vec();
        if self.width == 1 {
            dirs.extend(match self.axis {
                PortalAxis::X => [(1, 0), (-1, 0)],
                PortalAxis::Z => [(0, 1), (0, -1)],
            });
        }
        dirs.into_iter()
            .map(|(dx, dz)| (base.offset(dx, 0, dz), (dx, dz)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("not a portal conduit")]
    NotConduit,
    /// Not flat, not upright, not filled, or too small or large.
    #[error("{SHAPE_RULE}")]
    Shape,
}

/// The rectangle the conduit at `at` belongs to: every conduit connected to
/// it (sharing a face), which must fill an upright rectangle within the size
/// limits exactly. A single column is taken as spanning X.
pub fn find_frame(at: BlockPos, is_conduit: impl Fn(BlockPos) -> bool) -> Result<PortalFrame, FrameError> {
    if !is_conduit(at) {
        return Err(FrameError::NotConduit);
    }
    // More connected conduits than the largest frame means it is no frame.
    let limit = MAX_WIDTH as usize * MAX_HEIGHT as usize;
    let mut seen = vec![at];
    let mut i = 0;
    while i < seen.len() {
        let p = seen[i];
        i += 1;
        for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
            let q = p.offset(dx, dy, dz);
            if !seen.contains(&q) && is_conduit(q) {
                if seen.len() == limit {
                    return Err(FrameError::Shape);
                }
                seen.push(q);
            }
        }
    }
    let lo = |f: fn(&BlockPos) -> i64| seen.iter().map(f).min().expect("not empty");
    let hi = |f: fn(&BlockPos) -> i64| seen.iter().map(f).max().expect("not empty");
    let (x0, x1, y0, y1, z0, z1) = (
        lo(|p| p.x),
        hi(|p| p.x),
        lo(|p| p.y),
        hi(|p| p.y),
        lo(|p| p.z),
        hi(|p| p.z),
    );
    let (axis, width) = match (x1 - x0, z1 - z0) {
        (w, 0) => (PortalAxis::X, w + 1),
        (0, w) => (PortalAxis::Z, w + 1),
        _ => return Err(FrameError::Shape),
    };
    let height = y1 - y0 + 1;
    if !(MIN_WIDTH as i64..=MAX_WIDTH as i64).contains(&width)
        || !(MIN_HEIGHT as i64..=MAX_HEIGHT as i64).contains(&height)
        || seen.len() as i64 != width * height
    {
        return Err(FrameError::Shape);
    }
    Ok(PortalFrame {
        corner: BlockPos::new(x0, y0, z0),
        axis,
        width: width as u8,
        height: height as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn frame_of(blocks: &[(i64, i64, i64)], at: (i64, i64, i64)) -> Result<PortalFrame, FrameError> {
        let set: HashSet<BlockPos> = blocks.iter().map(|&(x, y, z)| BlockPos::new(x, y, z)).collect();
        find_frame(BlockPos::new(at.0, at.1, at.2), |p| set.contains(&p))
    }

    fn rect(x: i64, z: i64, w: i64, h: i64, along_z: bool) -> Vec<(i64, i64, i64)> {
        let mut v = Vec::new();
        for j in 0..h {
            for i in 0..w {
                v.push(if along_z {
                    (x, 10 + j, z + i)
                } else {
                    (x + i, 10 + j, z)
                });
            }
        }
        v
    }

    #[test]
    fn rectangles_are_found_from_any_conduit() {
        let blocks = rect(5, 7, 2, 3, false);
        for &b in &blocks {
            let f = frame_of(&blocks, b).unwrap();
            assert_eq!(
                (f.corner, f.axis, f.width, f.height),
                (BlockPos::new(5, 10, 7), PortalAxis::X, 2, 3)
            );
            assert_eq!(f.blocks().count(), 6);
            assert!(blocks.iter().all(|&(x, y, z)| f.contains(BlockPos::new(x, y, z))));
        }
        let f = frame_of(&rect(5, 7, 4, 2, true), (5, 11, 9)).unwrap();
        assert_eq!((f.axis, f.width, f.height), (PortalAxis::Z, 4, 2));
        assert!(!f.contains(BlockPos::new(6, 10, 7)));
        let column = frame_of(&rect(0, 0, 1, 2, false), (0, 10, 0)).unwrap();
        assert_eq!((column.width, column.height), (1, 2));
        assert_eq!(column.exit_feet().len(), 4, "a one-wide frame can be left on any side");
    }

    #[test]
    fn misshapen_frames_are_refused() {
        // Too short, too wide, too tall.
        assert_eq!(frame_of(&rect(0, 0, 3, 1, false), (0, 10, 0)), Err(FrameError::Shape));
        assert_eq!(frame_of(&rect(0, 0, 7, 2, false), (0, 10, 0)), Err(FrameError::Shape));
        assert_eq!(frame_of(&rect(0, 0, 2, 7, false), (0, 10, 0)), Err(FrameError::Shape));
        // A hole, an L, a bent frame.
        let mut holed = rect(0, 0, 3, 3, false);
        holed.retain(|&b| b != (1, 11, 0));
        assert_eq!(frame_of(&holed, (0, 10, 0)), Err(FrameError::Shape));
        let mut l = rect(0, 0, 1, 3, false);
        l.push((1, 10, 0));
        assert_eq!(frame_of(&l, (0, 10, 0)), Err(FrameError::Shape));
        let mut bent = rect(0, 0, 2, 2, false);
        bent.push((1, 10, 1));
        bent.push((1, 11, 1));
        assert_eq!(frame_of(&bent, (0, 10, 0)), Err(FrameError::Shape));
        // A huge slab stops early instead of searching it all.
        let slab: Vec<_> = (0..40).flat_map(|x| (0..40).map(move |y| (x, y, 0))).collect();
        assert_eq!(frame_of(&slab, (0, 0, 0)), Err(FrameError::Shape));
        assert_eq!(frame_of(&[], (0, 0, 0)), Err(FrameError::NotConduit));
    }

    #[test]
    fn exits_stand_in_front_of_the_bottom_middle() {
        let f = frame_of(&rect(5, 7, 3, 3, false), (5, 10, 7)).unwrap();
        assert_eq!(
            f.exit_feet(),
            vec![(BlockPos::new(6, 10, 8), (0, 1)), (BlockPos::new(6, 10, 6), (0, -1))]
        );
    }
}
