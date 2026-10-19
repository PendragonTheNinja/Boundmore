//! Geometry of chiselled blocks (ADR-0008).
//!
//! A cut block is meshed on its own, never greedily merged, from a template:
//! the triangles of its family and removed octants in block units (`0..=1`),
//! built once per shape and cached. Each triangle that lies on one of the
//! block's six faces is tagged with it, so it can be dropped where a full
//! opaque neighbour covers that face; everything else (cut faces, slopes,
//! curves) is always drawn.
//!
//! How each family draws its cuts:
//!
//! - **square**: the kept octants as half-block cubes, faces between kept
//!   octants culled.
//! - **slope**: the convex hull of the block corners whose octant is kept
//!   (each corner of the cube lies in exactly one octant), so removing an
//!   octant removes its corner and the surface slopes across. Where the kept
//!   corners have no volume (a half slab keeps four coplanar corners) the hull
//!   of the kept octants' own corners is used instead.
//! - **bevel**: each removed octant whose kept neighbours inside the block lie
//!   along two or three axes is partly filled back behind a 45 degree face
//!   across it (a chamfer); with fewer it is left open, as square.
//! - **round**: as bevel, with the chamfer a quarter round (a cylinder along an
//!   edge, a sphere at a corner), [`ROUND_SEGMENTS`] segments per quarter.
//!
//! Collision and targeting use the kept octants whatever the family; only the
//! look differs.
//!
//! Inside an octant, `t` runs from 0 at the block's outer face to 1 at the
//! block's middle along each axis, and `s = 1 - t`. A bevel fills
//! `t_a + t_b >= 1` (edge) or `t_x + t_y + t_z >= 1` (corner); a round fills
//! `s_a² + s_b² <= 1` or `|s|² <= 2`. Both meet each outer face of a corner
//! octant in the same profile as an edge's, so neighbouring cuts line up.

use crate::Face;
use bm_core::{Shape, ShapeFamily};
use std::sync::OnceLock;

/// Segments in a quarter round.
pub const ROUND_SEGMENTS: usize = 4;

/// One triangle of a shape template, counter-clockwise seen from outside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeTri {
    /// Corners in block units.
    pub pos: [[f32; 3]; 3],
    /// Unit normals at the corners (equal for flat faces).
    pub normal: [[f32; 3]; 3],
    /// The block face this triangle lies on, if any.
    pub face: Option<Face>,
    /// The face whose texture and shading it takes (0..6, as [`Face::index`]):
    /// the one its normal points most along.
    pub tex_face: u8,
}

/// The triangles of a shape (empty for the full cube, which the greedy
/// mesher draws, and for anything invalid).
pub fn template(shape: Shape) -> &'static [ShapeTri] {
    static TABLES: [OnceLock<Vec<Vec<ShapeTri>>>; 4] = [const { OnceLock::new() }; 4];
    if !shape.is_valid() {
        return &[];
    }
    let Some(family) = shape.family() else {
        return &[];
    };
    let table = TABLES[family as usize].get_or_init(|| (0u16..256).map(|m| build(family, m as u8)).collect());
    &table[shape.removed() as usize]
}

fn build(family: ShapeFamily, removed: u8) -> Vec<ShapeTri> {
    if removed == 0 || removed == 0xff {
        return Vec::new();
    }
    let mut out = Vec::new();
    match family {
        ShapeFamily::Square => octant_cells(removed, None, &mut out),
        ShapeFamily::Bevel => octant_cells(removed, Some(Fill::Bevel), &mut out),
        ShapeFamily::Round => octant_cells(removed, Some(Fill::Round), &mut out),
        ShapeFamily::Slope => slope(removed, &mut out),
    }
    out
}

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 0.0 { [a[0] / l, a[1] / l, a[2] / l] } else { a }
}

/// The face a normal points most along; ties go to Y, then X.
fn dominant(n: V3) -> u8 {
    let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
    let axis = if ay >= ax - 1e-4 && ay >= az - 1e-4 {
        1
    } else if ax >= az - 1e-4 {
        0
    } else {
        2
    };
    axis as u8 * 2 + (n[axis] < 0.0) as u8
}

/// Adds triangle `a, b, c`, turned so it winds counter-clockwise about
/// `towards` (the outward direction).
fn tri(out: &mut Vec<ShapeTri>, p: [V3; 3], n: [V3; 3], towards: V3, face: Option<Face>) {
    let g = cross(sub(p[1], p[0]), sub(p[2], p[0]));
    if dot(g, g) < 1e-12 {
        return;
    }
    let (p, n) = if dot(g, towards) < 0.0 {
        ([p[0], p[2], p[1]], [n[0], n[2], n[1]])
    } else {
        (p, n)
    };
    out.push(ShapeTri {
        pos: p,
        normal: n,
        face,
        tex_face: dominant(normalize(g)),
    });
}

/// A flat convex polygon, fanned, with one normal.
fn flat_poly(out: &mut Vec<ShapeTri>, pts: &[V3], normal: V3, face: Option<Face>) {
    for k in 1..pts.len().saturating_sub(1) {
        tri(out, [pts[0], pts[k], pts[k + 1]], [normal; 3], normal, face);
    }
}

// ---- square, bevel and round: per-octant cells ----

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fill {
    Bevel,
    Round,
}

/// What part of an octant is solid in the drawing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cell {
    Kept,
    Empty,
    /// Filled behind a cut along the edge of axis `c` (the other two axes
    /// have kept neighbours).
    Edge { c: usize },
    /// Filled behind a cut across the corner (all three neighbours kept).
    Corner,
}

/// How much of one face of an octant is solid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cover {
    Full,
    Empty,
    /// The profile of the cut, in the face's plane.
    Cap,
}

fn bits(i: u8) -> [u8; 3] {
    [i & 1, i >> 2 & 1, i >> 1 & 1]
}

fn axis_bit(axis: usize) -> u8 {
    [1, 4, 2][axis]
}

fn cells(removed: u8, fill: Option<Fill>) -> [Cell; 8] {
    std::array::from_fn(|i| {
        let i = i as u8;
        if removed & 1 << i == 0 {
            return Cell::Kept;
        }
        if fill.is_none() {
            return Cell::Empty;
        }
        let kept = |a: usize| removed & 1 << (i ^ axis_bit(a)) == 0;
        match (kept(0), kept(1), kept(2)) {
            (true, true, true) => Cell::Corner,
            (false, true, true) => Cell::Edge { c: 0 },
            (true, false, true) => Cell::Edge { c: 1 },
            (true, true, false) => Cell::Edge { c: 2 },
            _ => Cell::Empty,
        }
    })
}

/// Cover of a cell's face along `axis`, on its inner (towards the middle of
/// the block) or outer side.
fn cover(cell: Cell, axis: usize, inner: bool) -> Cover {
    match cell {
        Cell::Kept => Cover::Full,
        Cell::Empty => Cover::Empty,
        Cell::Edge { c } if c == axis => Cover::Cap,
        Cell::Edge { .. } => {
            if inner {
                Cover::Full
            } else {
                Cover::Empty
            }
        }
        Cell::Corner => {
            if inner {
                Cover::Full
            } else {
                Cover::Cap
            }
        }
    }
}

/// An octant's frame: maps `t` (0 at the block's outer face, 1 at its middle)
/// to block units.
#[derive(Clone, Copy)]
struct Frame {
    b: [u8; 3],
}

impl Frame {
    fn pos(self, t: V3) -> V3 {
        std::array::from_fn(|a| if self.b[a] == 0 { 0.5 * t[a] } else { 1.0 - 0.5 * t[a] })
    }

    /// A direction given in `t`.
    fn dir(self, v: V3) -> V3 {
        normalize(std::array::from_fn(|a| if self.b[a] == 0 { v[a] } else { -v[a] }))
    }
}

fn other_axes(c: usize) -> (usize, usize) {
    match c {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

/// `(sin, cos)` of `k` quarter-round steps: the shared samples every cap and
/// curve uses, so they meet exactly.
fn arc(k: usize) -> (f32, f32) {
    let a = std::f32::consts::FRAC_PI_2 * k as f32 / ROUND_SEGMENTS as f32;
    let (s, c) = a.sin_cos();
    // Exact ends, so the curve meets the octant's corners.
    match k {
        0 => (0.0, 1.0),
        k if k == ROUND_SEGMENTS => (1.0, 0.0),
        _ => (s, c),
    }
}

/// The 2D outline (in `t` of the face's two axes) of a face's cover.
fn outline(cover: Cover, fill: Option<Fill>) -> Vec<[f32; 2]> {
    match (cover, fill) {
        (Cover::Empty, _) => Vec::new(),
        (Cover::Full, _) | (Cover::Cap, None) => vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        (Cover::Cap, Some(Fill::Bevel)) => vec![[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        (Cover::Cap, Some(Fill::Round)) => {
            // s_a² + s_b² <= 1: the middle corner, then the arc.
            let mut v = vec![[1.0, 1.0]];
            v.extend((0..=ROUND_SEGMENTS).map(|k| {
                let (s, c) = arc(k);
                [1.0 - s, 1.0 - c]
            }));
            v
        }
    }
}

fn octant_cells(removed: u8, fill: Option<Fill>, out: &mut Vec<ShapeTri>) {
    let cells = cells(removed, fill);
    for i in 0..8u8 {
        let cell = cells[i as usize];
        let f = Frame { b: bits(i) };
        // Faces on the octant's six sides.
        for axis in 0..3 {
            for inner in [true, false] {
                let mine = cover(cell, axis, inner);
                let shown = if inner {
                    let j = i ^ axis_bit(axis);
                    let theirs = cover(cells[j as usize], axis, true);
                    match (mine, theirs) {
                        (Cover::Full, Cover::Full) | (Cover::Cap, Cover::Cap) => Cover::Empty,
                        (m, _) => m,
                    }
                } else {
                    mine
                };
                if shown == Cover::Empty {
                    continue;
                }
                let (a, b) = other_axes(axis);
                let tc = if inner { 1.0 } else { 0.0 };
                let pts: Vec<V3> = outline(shown, fill)
                    .into_iter()
                    .map(|[ta, tb]| {
                        let mut t = [0.0; 3];
                        t[a] = ta;
                        t[b] = tb;
                        t[axis] = tc;
                        f.pos(t)
                    })
                    .collect();
                let mut e = [0.0; 3];
                e[axis] = if inner { 1.0 } else { -1.0 };
                let normal = f.dir(e);
                let face = (!inner).then_some(Face {
                    axis: axis as u8,
                    positive: f.b[axis] == 1,
                });
                flat_poly(out, &pts, normal, face);
            }
        }
        // The cut surface itself.
        match (cell, fill) {
            (Cell::Edge { c }, Some(Fill::Bevel)) => {
                let (a, b) = other_axes(c);
                let at = |ta: f32, tb: f32, tc: f32| {
                    let mut t = [0.0; 3];
                    t[a] = ta;
                    t[b] = tb;
                    t[c] = tc;
                    f.pos(t)
                };
                let mut n = [0.0; 3];
                n[a] = -1.0;
                n[b] = -1.0;
                let pts = [at(1.0, 0.0, 0.0), at(0.0, 1.0, 0.0), at(0.0, 1.0, 1.0), at(1.0, 0.0, 1.0)];
                flat_poly(out, &pts, f.dir(n), None);
            }
            (Cell::Corner, Some(Fill::Bevel)) => {
                let pts = [f.pos([1.0, 0.0, 0.0]), f.pos([0.0, 1.0, 0.0]), f.pos([0.0, 0.0, 1.0])];
                flat_poly(out, &pts, f.dir([-1.0, -1.0, -1.0]), None);
            }
            (Cell::Edge { c }, Some(Fill::Round)) => {
                let (a, b) = other_axes(c);
                // In `s`: (s_a, s_b) = (sin, cos) of each step; normal along s.
                let point = |k: usize, tc: f32| {
                    let (sa, sb) = arc(k);
                    let mut t = [0.0; 3];
                    t[a] = 1.0 - sa;
                    t[b] = 1.0 - sb;
                    t[c] = tc;
                    let mut n = [0.0; 3];
                    n[a] = -sa;
                    n[b] = -sb;
                    (f.pos(t), f.dir(n))
                };
                for k in 0..ROUND_SEGMENTS {
                    let (p0, n0) = point(k, 0.0);
                    let (p1, n1) = point(k + 1, 0.0);
                    let (p2, n2) = point(k + 1, 1.0);
                    let (p3, n3) = point(k, 1.0);
                    let towards = normalize([
                        n0[0] + n1[0] + n2[0] + n3[0],
                        n0[1] + n1[1] + n2[1] + n3[1],
                        n0[2] + n1[2] + n2[2] + n3[2],
                    ]);
                    tri(out, [p0, p1, p2], [n0, n1, n2], towards, None);
                    tri(out, [p0, p2, p3], [n0, n2, n3], towards, None);
                }
            }
            (Cell::Corner, Some(Fill::Round)) => sphere_patch(f, out),
            _ => {}
        }
    }
}

/// The corner's quarter round: `|s|² = 2` inside the octant, in rows of
/// constant `s_z` from the outer face (`s_z = 1`, a full quarter arc) to the
/// middle (`s_z = 0`, one point). Row ends lie on the other two outer faces'
/// arcs, sampled as their caps are.
fn sphere_patch(f: Frame, out: &mut Vec<ShapeTri>) {
    let n = ROUND_SEGMENTS;
    let vert = |s: V3| {
        let t = [1.0 - s[0], 1.0 - s[1], 1.0 - s[2]];
        (f.pos(t), f.dir([-s[0], -s[1], -s[2]]))
    };
    let rows: Vec<Vec<(V3, V3)>> = (0..=n)
        .map(|i| {
            let (w, sz) = arc(i);
            let count = n - i;
            (0..=count)
                .map(|j| {
                    let s = if i == 0 {
                        let (a, b) = arc(n - j);
                        [a, b, 1.0]
                    } else if j == 0 {
                        [1.0, w, sz]
                    } else if j == count {
                        [w, 1.0, sz]
                    } else {
                        let r = (1.0 + w * w).sqrt();
                        let (p0, p1) = (w.atan2(1.0), std::f32::consts::FRAC_PI_2 - w.atan2(1.0));
                        let psi = p0 + (p1 - p0) * j as f32 / count as f32;
                        [r * psi.cos(), r * psi.sin(), sz]
                    };
                    vert(s)
                })
                .collect()
        })
        .collect();
    let mut add = |a: (V3, V3), b: (V3, V3), c: (V3, V3)| {
        let towards = normalize([
            a.1[0] + b.1[0] + c.1[0],
            a.1[1] + b.1[1] + c.1[1],
            a.1[2] + b.1[2] + c.1[2],
        ]);
        tri(out, [a.0, b.0, c.0], [a.1, b.1, c.1], towards, None);
    };
    for i in 0..n {
        let (r0, r1) = (&rows[i], &rows[i + 1]);
        for j in 0..r1.len() {
            add(r0[j], r0[j + 1], r1[j]);
            if j + 1 < r1.len() {
                add(r0[j + 1], r1[j + 1], r1[j]);
            }
        }
    }
}

// ---- slope: a convex hull ----

fn corner_of(i: u8) -> V3 {
    let b = bits(i);
    [b[0] as f32, b[1] as f32, b[2] as f32]
}

fn slope(removed: u8, out: &mut Vec<ShapeTri>) {
    let corners: Vec<V3> = (0..8u8).filter(|i| removed & 1 << i == 0).map(corner_of).collect();
    if hull(&corners, out) {
        return;
    }
    // No volume in the kept corners: hull the kept octants instead.
    let mut pts: Vec<V3> = Vec::new();
    for i in (0..8u8).filter(|i| removed & 1 << i == 0) {
        let b = bits(i);
        for k in 0..8u8 {
            let d = bits(k);
            let p: V3 = std::array::from_fn(|a| (b[a] as f32 + d[a] as f32) * 0.5);
            if !pts.contains(&p) {
                pts.push(p);
            }
        }
    }
    let ok = hull(&pts, out);
    debug_assert!(ok, "kept octants always have volume");
}

/// Adds the convex hull of `pts` (block units); false if it has no volume.
fn hull(pts: &[V3], out: &mut Vec<ShapeTri>) -> bool {
    const EPS: f32 = 1e-4;
    let n = pts.len();
    let mut planes: Vec<(V3, f32)> = Vec::new();
    let mut solid = false;
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let g = cross(sub(pts[j], pts[i]), sub(pts[k], pts[i]));
                if dot(g, g) < 1e-8 {
                    continue;
                }
                let g = normalize(g);
                let d: Vec<f32> = pts.iter().map(|p| dot(g, sub(*p, pts[i]))).collect();
                let above = d.iter().any(|&v| v > EPS);
                let below = d.iter().any(|&v| v < -EPS);
                if above || below {
                    solid = true;
                }
                let normal = match (above, below) {
                    (false, true) => g,
                    (true, false) => [-g[0], -g[1], -g[2]],
                    _ => continue,
                };
                let offset = dot(normal, pts[i]);
                let same = |(m, o): &(V3, f32)| dot(*m, normal) > 1.0 - 1e-4 && (o - offset).abs() < EPS;
                if !planes.iter().any(same) {
                    planes.push((normal, offset));
                }
            }
        }
    }
    if !solid {
        return false;
    }
    for (normal, offset) in planes {
        let on: Vec<V3> = pts
            .iter()
            .copied()
            .filter(|p| (dot(normal, *p) - offset).abs() < EPS)
            .collect();
        // Sort around the centre, counter-clockwise about the normal.
        let c = on.iter().fold([0.0; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]);
        let c = [c[0] / on.len() as f32, c[1] / on.len() as f32, c[2] / on.len() as f32];
        let u = normalize(sub(on[0], c));
        let v = cross(normal, u);
        let mut ring: Vec<(f32, V3)> = on
            .iter()
            .map(|p| {
                let d = sub(*p, c);
                (dot(d, v).atan2(dot(d, u)), *p)
            })
            .collect();
        ring.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Drop points in the middle of an edge, so the fan has no slivers.
        let ring: Vec<V3> = ring.into_iter().map(|r| r.1).collect();
        let m = ring.len();
        let corners: Vec<V3> = (0..m)
            .filter(|&q| {
                let (a, b, p) = (ring[(q + m - 1) % m], ring[(q + 1) % m], ring[q]);
                let g = cross(sub(p, a), sub(b, p));
                dot(g, g) > 1e-8
            })
            .map(|q| ring[q])
            .collect();
        let face = block_face(normal, offset);
        flat_poly(out, &corners, normal, face);
    }
    true
}

/// The block face a plane is, if it is one.
fn block_face(normal: V3, offset: f32) -> Option<Face> {
    (0..3).find_map(|a| {
        if (normal[a].abs() - 1.0).abs() > 1e-4 {
            return None;
        }
        let positive = normal[a] > 0.0;
        let on = if positive { (offset - 1.0).abs() < 1e-4 } else { offset.abs() < 1e-4 };
        on.then_some(Face { axis: a as u8, positive })
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Whether a point (block units) is inside the drawn solid, from the
    /// families' definitions (independently of the triangles).
    pub fn inside(family: ShapeFamily, removed: u8, p: V3) -> bool {
        if p.iter().any(|&v| !(0.0..=1.0).contains(&v)) {
            return false;
        }
        let o = |v: f32| (v >= 0.5) as u8;
        let i = o(p[0]) | o(p[2]) << 1 | o(p[1]) << 2;
        if removed & 1 << i == 0 && family != ShapeFamily::Slope {
            return true;
        }
        let b = bits(i);
        let t: V3 = std::array::from_fn(|a| if b[a] == 0 { p[a] * 2.0 } else { (1.0 - p[a]) * 2.0 });
        let cells = cells(removed, Some(Fill::Bevel));
        match family {
            ShapeFamily::Square => false,
            ShapeFamily::Bevel | ShapeFamily::Round => {
                let round = family == ShapeFamily::Round;
                let s: V3 = std::array::from_fn(|a| 1.0 - t[a]);
                match cells[i as usize] {
                    Cell::Edge { c } => {
                        let (a, b) = other_axes(c);
                        if round { s[a] * s[a] + s[b] * s[b] <= 1.0 } else { t[a] + t[b] >= 1.0 }
                    }
                    Cell::Corner => {
                        if round { dot(s, s) <= 2.0 } else { t[0] + t[1] + t[2] >= 1.0 }
                    }
                    _ => false,
                }
            }
            ShapeFamily::Slope => {
                // Inside the hull: behind every face plane.
                let tris = template(Shape::new(family, removed).unwrap());
                tris.iter().all(|tr| {
                    let n = normalize(cross(sub(tr.pos[1], tr.pos[0]), sub(tr.pos[2], tr.pos[0])));
                    dot(n, sub(p, tr.pos[0])) <= 1e-5
                })
            }
        }
    }

    fn area_normal(t: &ShapeTri) -> V3 {
        let g = cross(sub(t.pos[1], t.pos[0]), sub(t.pos[2], t.pos[0]));
        [g[0] * 0.5, g[1] * 0.5, g[2] * 0.5]
    }

    /// Volume by the divergence theorem; only meaningful for a closed surface.
    fn volume(tris: &[ShapeTri]) -> f32 {
        tris.iter()
            .map(|t| dot(t.pos[0], cross(t.pos[1], t.pos[2])) / 6.0)
            .sum()
    }

    fn masks() -> impl Iterator<Item = u8> {
        1u8..0xff
    }

    #[test]
    fn every_shape_is_a_closed_surface_wound_outwards() {
        for family in ShapeFamily::ALL {
            for m in masks() {
                let tris = template(Shape::new(family, m).unwrap());
                assert!(!tris.is_empty(), "{family:?} {m:#010b}");
                // A closed surface's area-weighted normals cancel.
                let sum = tris.iter().fold([0.0f32; 3], |s, t| {
                    let a = area_normal(t);
                    [s[0] + a[0], s[1] + a[1], s[2] + a[2]]
                });
                assert!(dot(sum, sum).sqrt() < 1e-4, "{family:?} {m:#010b} is open: {sum:?}");
                let v = volume(tris);
                let kept = (8 - m.count_ones()) as f32 / 8.0;
                match family {
                    ShapeFamily::Square => assert!((v - kept).abs() < 1e-4, "{family:?} {m:#010b}: {v}"),
                    // Fills only add, never past the cube.
                    ShapeFamily::Bevel | ShapeFamily::Round => {
                        assert!(v >= kept - 1e-4 && v <= 1.0 + 1e-4, "{family:?} {m:#010b}: {v}")
                    }
                    ShapeFamily::Slope => assert!(v > 0.01 && v <= 1.0 + 1e-4, "{family:?} {m:#010b}: {v}"),
                }
                for t in tris {
                    let g = normalize(area_normal(t));
                    for n in t.normal {
                        assert!(dot(g, n) > 0.3, "{family:?} {m:#010b}: normal against winding");
                    }
                    assert!(t.pos.iter().flatten().all(|v| (0.0..=1.0).contains(v)));
                }
            }
        }
    }

    #[test]
    fn square_cuts_have_exactly_matched_edges() {
        // No cracks: every edge is shared with one running the other way.
        for m in masks() {
            let tris = template(Shape::new(ShapeFamily::Square, m).unwrap());
            let key = |p: V3| p.map(|v| (v * 64.0).round() as i32);
            let mut edges = std::collections::HashMap::new();
            for t in tris {
                for k in 0..3 {
                    *edges.entry((key(t.pos[k]), key(t.pos[(k + 1) % 3]))).or_insert(0i32) += 1;
                }
            }
            for ((a, b), n) in &edges {
                assert_eq!(edges.get(&(*b, *a)), Some(n), "{m:#010b}: edge {a:?} {b:?}");
            }
        }
    }

    #[test]
    fn no_face_is_inside_the_solid() {
        for family in [ShapeFamily::Square, ShapeFamily::Bevel, ShapeFamily::Slope] {
            for m in masks() {
                for t in template(Shape::new(family, m).unwrap()) {
                    let c: V3 = std::array::from_fn(|a| (t.pos[0][a] + t.pos[1][a] + t.pos[2][a]) / 3.0);
                    let n = normalize(area_normal(t));
                    let e = 0.003;
                    let back: V3 = std::array::from_fn(|a| c[a] - n[a] * e);
                    let front: V3 = std::array::from_fn(|a| c[a] + n[a] * e);
                    assert!(inside(family, m, back), "{family:?} {m:#010b}: nothing behind {c:?}");
                    assert!(!inside(family, m, front), "{family:?} {m:#010b}: buried face at {c:?}");
                }
            }
        }
    }

    #[test]
    fn faces_on_the_block_boundary_are_tagged() {
        for family in ShapeFamily::ALL {
            for m in masks() {
                for t in template(Shape::new(family, m).unwrap()) {
                    let on = (0..3).find(|&a| t.pos.iter().all(|p| p[a] == 0.0) || t.pos.iter().all(|p| p[a] == 1.0));
                    match t.face {
                        Some(f) => {
                            let a = f.axis as usize;
                            let want = if f.positive { 1.0 } else { 0.0 };
                            assert!(t.pos.iter().all(|p| p[a] == want), "{family:?} {m:#010b}");
                        }
                        None => assert!(on.is_none(), "{family:?} {m:#010b}: untagged boundary face"),
                    }
                }
            }
        }
    }

    #[test]
    fn the_families_draw_a_corner_cut_differently() {
        let corner = 1 << 7; // the top (1, 1, 1) octant
        let count = |f| template(Shape::new(f, corner).unwrap()).len();
        // Slope: the cube with one corner sliced off through its three
        // neighbours: three squares, three triangles cut from squares, one
        // big triangle. Seven faces.
        let slope = template(Shape::new(ShapeFamily::Slope, corner).unwrap());
        let diagonal: Vec<_> = slope.iter().filter(|t| t.face.is_none()).collect();
        assert_eq!(diagonal.len(), 1, "one slanted triangle");
        let n = normalize(area_normal(diagonal[0]));
        assert!(n.iter().all(|&v| (v - 1.0 / 3f32.sqrt()).abs() < 1e-4));
        assert!(count(ShapeFamily::Round) > count(ShapeFamily::Bevel));
        // Round stays modest.
        for m in masks() {
            assert!(template(Shape::new(ShapeFamily::Round, m).unwrap()).len() <= 260, "{m:#010b}");
        }
    }
}
