//! Props: blocks drawn as a small model instead of a cube (a torch, and the
//! gathering objects: plants, boulders and fungi).
//!
//! A prop is any block with a `mount` (`BlockDef::is_prop`). It is never
//! opaque, so it hides no neighbouring face and casts no occlusion; its own
//! triangles go into the chunk's shaped opaque list ([`ShapedVertex`]) with
//! explicit texture coordinates, because a tilted stick cannot take its
//! texture from its position the way a cube face does. Its block's `model`
//! (`BlockFlags::model`) says which model it is.
//!
//! The torch model, in sixteenths of a block: a stick 2 wide and 9 tall with
//! a flame cap 3 wide from 8 to 11 on top. On the floor it stands upright in
//! the middle of the cell. On a wall it leans out from the face it hangs on
//! ([`WALL_TILT`]), its foot against that face and raised off the floor, as
//! wall torches in Boundless do.
//!
//! Texture layout of the 32 x 32 `torch` texture (pixels; `bm-texgen` paints
//! it as a picture of a torch, so a hand-made replacement keeps the same
//! places): the stick's sides take columns 14..18 of rows 14..32, its foot
//! rows 28..32; the flame's sides take columns 13..19 of rows 6..12 and its
//! top the bright core, columns 14..18 of rows 8..12.
//!
//! Gathering objects (milestone 11) stand on the floor, and each cell's copy
//! is a little different (size, turn, offset), from a hash of its position
//! and block ([`prop_seed`]), so a field of them never looks stamped out.
//! Their textures:
//! - **plant**: two crossed upright quads, about 0.8 wide and 0.9 tall, each
//!   drawn from both sides and showing the whole texture: a picture of the
//!   plant on a clear background (alpha below one half is cut out by the
//!   shader), its foot on the bottom row;
//! - **boulder**: one or two low rocks (irregular six-sided lumps, bulging
//!   at the waist and drawn in at the top), about 0.7 wide and 0.45 tall;
//!   tops take the texture's top left quarter (columns 0..16 of rows 0..16,
//!   the footprint seen from above), sides its lower half (rows 16..32, the
//!   foot on the bottom row, running once round the rock);
//! - **fungus**: a mushroom (a stem and a wider cap) with up to two small
//!   ones at its foot; the cap's top takes columns 0..16 of rows 0..16, its
//!   rim columns 16..32 of rows 0..8, the gills underneath columns 16..32 of
//!   rows 8..16, the stem columns 0..16 of rows 16..32.
//!
//! Light: every model takes the cell's own light (a torch or a glowcap
//! emits, so its cell is lit); a torch's flame is drawn at full block light,
//! since it is the light. Props have no ambient occlusion.

use crate::{Padded, PaddedLight, ShapedVertex};
use bm_core::{BlockFlags, BlockId, CHUNK_SIZE, Light, Model, Mount};

const N: i32 = CHUNK_SIZE;

/// How far a wall torch leans out from its wall, in radians (about 24 deg).
pub const WALL_TILT: f32 = 0.42;

/// A rectangle of the texture, in pixels of the 32 x 32 face:
/// `[u0, v0, u1, v1]`, v running down.
type Rect = [u8; 4];

const STICK_SIDE: Rect = [14, 14, 18, 32];
const STICK_FOOT: Rect = [14, 28, 18, 32];
const STICK_TOP: Rect = [14, 14, 18, 18];
const FLAME_SIDE: Rect = [13, 6, 19, 12];
const FLAME_TOP: Rect = [14, 8, 18, 12];
const FLAME_BOTTOM: Rect = [13, 10, 19, 12];

/// One triangle of a prop model, counter-clockwise seen from outside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropTri {
    /// Corners in block units, relative to the cell's minimum corner.
    pub pos: [[f32; 3]; 3],
    /// The flat normal.
    pub normal: [f32; 3],
    /// Texture coordinates of each corner, in pixels of the 32 x 32 face.
    pub uv: [[u8; 2]; 3],
    /// The cube face whose texture slot and shading it takes (0..6).
    pub tex_face: u8,
    /// Part of the flame (drawn at full block light).
    pub flame: bool,
}

/// An axis-aligned box of the model, in sixteenths, with its texture.
struct Part {
    min: [f32; 3],
    max: [f32; 3],
    side: Rect,
    top: Rect,
    bottom: Option<Rect>,
    flame: bool,
}

fn parts(floor: bool) -> [Part; 2] {
    [
        Part {
            min: [7.0, 0.0, 7.0],
            max: [9.0, 9.0, 9.0],
            side: STICK_SIDE,
            top: STICK_TOP,
            // A floor torch's foot stands on the block below and is never seen.
            bottom: (!floor).then_some(STICK_FOOT),
            flame: false,
        },
        Part {
            min: [6.5, 8.0, 6.5],
            max: [9.5, 11.0, 9.5],
            side: FLAME_SIDE,
            top: FLAME_TOP,
            bottom: Some(FLAME_BOTTOM),
            flame: true,
        },
    ]
}

/// One face of a box: corners, outward normal, texture rectangle, face index.
type Quad = ([[f32; 3]; 4], [f32; 3], Rect, u8);

/// The faces of a box as quads: four corners counter-clockwise seen from
/// outside (bottom left, bottom right, top right, top left as the texture
/// reads), the outward normal, the texture rectangle and the face index.
fn box_quads(p: &Part) -> Vec<Quad> {
    let ([x0, y0, z0], [x1, y1, z1]) = (p.min, p.max);
    let mut q = vec![
        // +X: seen from +x, right is -z.
        (
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            [1.0, 0.0, 0.0],
            p.side,
            0,
        ),
        // -X: right is +z.
        (
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [-1.0, 0.0, 0.0],
            p.side,
            1,
        ),
        // +Y: seen from above, right is +x and up is -z.
        (
            [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
            [0.0, 1.0, 0.0],
            p.top,
            2,
        ),
        // +Z: right is +x.
        (
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [0.0, 0.0, 1.0],
            p.side,
            4,
        ),
        // -Z: right is -x.
        (
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [0.0, 0.0, -1.0],
            p.side,
            5,
        ),
    ];
    if let Some(bottom) = p.bottom {
        // -Y: seen from below, right is +x and up is +z.
        q.push((
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            [0.0, -1.0, 0.0],
            bottom,
            3,
        ));
    }
    q
}

/// Maps a point (or, with `vector`, a direction) of the upright model in
/// sixteenths to the cell in blocks, for a mount.
fn place(mount: Mount, p: [f32; 3], vector: bool) -> [f32; 3] {
    let [x, mut y, mut z] = p;
    if mount != Mount::Floor {
        // Built as a North torch (the wall at z = 0): tilt about the foot so
        // the top leans towards +z, then set the foot against the wall.
        let (s, c) = WALL_TILT.sin_cos();
        let (pivot_y, pivot_z) = if vector { (0.0, 0.0) } else { (0.0, 8.0) };
        let (dy, dz) = (y - pivot_y, z - pivot_z);
        y = dy * c - dz * s;
        z = dy * s + dz * c;
        if !vector {
            y += 3.0;
            z += 1.0;
        }
    }
    // Turn about the cell's vertical axis so the wall side faces the mount.
    let (cx, cz) = if vector { (x, z) } else { (x - 8.0, z - 8.0) };
    let (rx, rz) = match mount {
        Mount::Floor | Mount::North => (cx, cz),
        Mount::South => (-cx, -cz),
        Mount::East => (-cz, cx),
        Mount::West => (cz, -cx),
    };
    if vector {
        [rx, y, rz]
    } else {
        [(rx + 8.0) / 16.0, y / 16.0, (rz + 8.0) / 16.0]
    }
}

/// The triangles of the torch model for a mount.
pub fn torch_model(mount: Mount) -> Vec<PropTri> {
    let mut out = Vec::new();
    for part in parts(mount == Mount::Floor) {
        for (corners, normal, rect, face) in box_quads(&part) {
            let [u0, v0, u1, v1] = rect;
            let uv = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
            let pos = corners.map(|c| place(mount, c, false));
            let n = place(mount, normal, true);
            for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
                out.push(PropTri {
                    pos: [pos[a], pos[b], pos[c]],
                    normal: n,
                    uv: [uv[a], uv[b], uv[c]],
                    tex_face: face,
                    flame: part.flame,
                });
            }
        }
    }
    out
}

/// The cached model for each mount.
pub fn prop_model(mount: Mount) -> &'static [PropTri] {
    use std::sync::OnceLock;
    static MODELS: [OnceLock<Vec<PropTri>>; 5] = [const { OnceLock::new() }; 5];
    let i = match mount {
        Mount::Floor => 0,
        Mount::North => 1,
        Mount::East => 2,
        Mount::South => 3,
        Mount::West => 4,
    };
    MODELS[i].get_or_init(|| torch_model(mount))
}

// ------------------------------------------------------- gathering objects

/// A tiny deterministic generator for one cell's variation.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(0x9e37_79b9).wrapping_add(0x7f4a_7c15);
        let mut h = self.0;
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        h = h.wrapping_mul(0x297a_2d39);
        h ^= h >> 15;
        (h >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// The variation seed of a prop: its chunk-local cell and its block, so
/// neighbours differ, and two kinds in one place differ too.
pub fn prop_seed(x: i32, y: i32, z: i32, block: BlockId) -> u32 {
    let mut h = (x as u32).wrapping_mul(73_856_093)
        ^ (y as u32).wrapping_mul(19_349_663)
        ^ (z as u32).wrapping_mul(83_492_791)
        ^ (block.0 as u32).wrapping_mul(2_654_435_761);
    h ^= h >> 13;
    h = h.wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

const PLANT_RECT: Rect = [0, 0, 32, 32];
/// The same picture seen from behind: its left edge stays at the same end.
const PLANT_BACK: Rect = [32, 0, 0, 32];
const BOULDER_TOP: Rect = [0, 0, 16, 16];
const BOULDER_SIDE: Rect = [0, 16, 32, 32];
const CAP_TOP: Rect = [0, 0, 16, 16];
const CAP_RIM: Rect = [16, 0, 32, 8];
const CAP_GILLS: Rect = [16, 8, 32, 16];
const STEM_SIDE: Rect = [0, 16, 16, 32];

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// Pushes a flat quad as two triangles. `corners` run bottom left, bottom
/// right, top right, top left as the texture rectangle reads; the winding
/// is turned, if need be, so the quad faces along `outward`.
fn quad(out: &mut Vec<PropTri>, corners: [[f32; 3]; 4], rect: Rect, outward: [f32; 3], tex_face: u8) {
    let [u0, v0, u1, v1] = rect;
    let mut pos = corners;
    let mut uv = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
    let mut n = unit(cross(sub(pos[1], pos[0]), sub(pos[3], pos[0])));
    if dot(n, outward) < 0.0 {
        pos.swap(1, 3);
        uv.swap(1, 3);
        n = [-n[0], -n[1], -n[2]];
    }
    for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
        out.push(PropTri {
            pos: [pos[a], pos[b], pos[c]],
            normal: n,
            uv: [uv[a], uv[b], uv[c]],
            tex_face,
            flame: false,
        });
    }
}

/// A lump: a box turned by `turn` about the vertical through `centre`
/// (x, z), from `y[0]` to `y[1]`, with half sizes `half` at the bottom and
/// `top` at the top (smaller for a bevelled rock or a domed cap). Its sides
/// take `rects[0]`; with `caps`, its top (`rects[1]`) and its underside
/// (`rects[2]`).
struct Lump {
    centre: [f32; 2],
    turn: f32,
    y: [f32; 2],
    half: [f32; 2],
    top: [f32; 2],
    rects: [Rect; 3],
    caps: (bool, bool),
}

impl Lump {
    fn push(&self, out: &mut Vec<PropTri>) {
        let (s, c) = self.turn.sin_cos();
        let at = |h: [f32; 2], i: usize, y: f32| {
            // Corners counter-clockwise seen from above.
            let (sx, sz) = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)][i];
            let (lx, lz) = (sx * h[0], sz * h[1]);
            [self.centre[0] + lx * c - lz * s, y, self.centre[1] + lx * s + lz * c]
        };
        let [y0, y1] = self.y;
        let [side, top, under] = self.rects;
        let mid = [self.centre[0], (y0 + y1) * 0.5, self.centre[1]];
        for i in 0..4 {
            let j = (i + 1) % 4;
            let corners = [
                at(self.half, j, y0),
                at(self.half, i, y0),
                at(self.top, i, y1),
                at(self.top, j, y1),
            ];
            let face: [f32; 3] = std::array::from_fn(|a| corners.iter().map(|p| p[a]).sum::<f32>() / 4.0);
            quad(out, corners, side, sub(face, mid), 0);
        }
        if self.caps.0 {
            let corners = [
                at(self.top, 3, y1),
                at(self.top, 2, y1),
                at(self.top, 1, y1),
                at(self.top, 0, y1),
            ];
            quad(out, corners, top, [0.0, 1.0, 0.0], 2);
        }
        if self.caps.1 {
            let corners = [
                at(self.half, 0, y0),
                at(self.half, 1, y0),
                at(self.half, 2, y0),
                at(self.half, 3, y0),
            ];
            quad(out, corners, under, [0.0, -1.0, 0.0], 3);
        }
    }
}

/// A plant: two crossed upright quads, each drawn from both sides.
fn plant(out: &mut Vec<PropTri>, r: &mut Rng) {
    let scale = r.range(0.85, 1.0);
    let (foot, crown, height) = (0.36 * scale, 0.44 * scale, 0.9 * scale);
    let centre = [0.5 + r.range(-0.05, 0.05), 0.5 + r.range(-0.05, 0.05)];
    let turn = r.range(0.0, std::f32::consts::FRAC_PI_2);
    for k in 0..2 {
        let a = turn + k as f32 * std::f32::consts::FRAC_PI_2;
        let (s, c) = a.sin_cos();
        let p = |w: f32, y: f32| [centre[0] + c * w, y, centre[1] + s * w];
        let front = [p(-foot, 0.0), p(foot, 0.0), p(crown, height), p(-crown, height)];
        let n = unit(cross(sub(front[1], front[0]), sub(front[3], front[0])));
        quad(out, front, PLANT_RECT, n, 0);
        let back = [front[1], front[0], front[3], front[2]];
        quad(out, back, PLANT_BACK, [-n[0], -n[1], -n[2]], 0);
    }
}

/// Pushes one triangle with explicit texture coordinates, wound to face
/// along `outward`.
fn tri(out: &mut Vec<PropTri>, pos: [[f32; 3]; 3], uv: [[u8; 2]; 3], outward: [f32; 3], tex_face: u8) {
    let mut pos = pos;
    let mut uv = uv;
    let mut n = unit(cross(sub(pos[1], pos[0]), sub(pos[2], pos[0])));
    if dot(n, outward) < 0.0 {
        pos.swap(1, 2);
        uv.swap(1, 2);
        n = [-n[0], -n[1], -n[2]];
    }
    out.push(PropTri {
        pos,
        normal: n,
        uv,
        tex_face,
        flame: false,
    });
}

/// A rock: an irregular six-sided lump in two tiers, bulging a little at
/// its waist and drawn in to a smaller top, so it reads as a stone rather
/// than a box. `half` is its half size along its own axes, `turn` its turn.
fn rock(out: &mut Vec<PropTri>, r: &mut Rng, centre: [f32; 2], half: [f32; 2], height: f32) {
    const SIDES: usize = 6;
    let turn = r.range(0.0, std::f32::consts::TAU);
    let radii: [f32; SIDES] = std::array::from_fn(|_| r.range(0.85, 1.0));
    let waist = r.range(0.45, 0.6);
    let top_scale = r.range(0.5, 0.68);
    // Rings: the foot, the waist (bulging), the top (drawn in).
    let rings = [(0.0, 1.0), (height * waist, 1.04), (height, top_scale)];
    let (s, c) = turn.sin_cos();
    let at = |i: usize, (y, k): (f32, f32)| {
        let a = i as f32 / SIDES as f32 * std::f32::consts::TAU;
        let (lx, lz) = (a.cos() * half[0] * radii[i] * k, a.sin() * half[1] * radii[i] * k);
        [centre[0] + lx * c - lz * s, y, centre[1] + lx * s + lz * c]
    };
    let [u0, v0, u1, v1] = BOULDER_SIDE;
    let v_waist = v1 - ((v1 - v0) as f32 * waist) as u8;
    let mid = [centre[0], height * 0.5, centre[1]];
    for i in 0..SIDES {
        let j = (i + 1) % SIDES;
        let (ua, ub) = (
            u0 + ((u1 - u0) as usize * i / SIDES) as u8,
            u0 + ((u1 - u0) as usize * (i + 1) / SIDES) as u8,
        );
        for (tier, (va, vb)) in [(v1, v_waist), (v_waist, v0)].into_iter().enumerate() {
            let corners = [
                at(i, rings[tier]),
                at(j, rings[tier]),
                at(j, rings[tier + 1]),
                at(i, rings[tier + 1]),
            ];
            let face: [f32; 3] = std::array::from_fn(|a| corners.iter().map(|p| p[a]).sum::<f32>() / 4.0);
            quad(out, corners, [ua, vb, ub, va], sub(face, mid), 0);
        }
    }
    // The top: a fan from its middle, its texture the footprint seen from
    // above.
    let [t0, s0, t1, s1] = BOULDER_TOP;
    let span = half[0].max(half[1]) * 2.0;
    let uv = |p: [f32; 3]| {
        let fu = ((p[0] - centre[0]) / span + 0.5).clamp(0.0, 1.0);
        let fv = ((p[2] - centre[1]) / span + 0.5).clamp(0.0, 1.0);
        [
            t0 + ((t1 - t0) as f32 * fu).round() as u8,
            s0 + ((s1 - s0) as f32 * fv).round() as u8,
        ]
    };
    let apex = [centre[0], height, centre[1]];
    for i in 0..SIDES {
        let (a, b) = (at(i, rings[2]), at((i + 1) % SIDES, rings[2]));
        tri(out, [apex, a, b], [uv(apex), uv(a), uv(b)], [0.0, 1.0, 0.0], 2);
    }
}

/// A boulder: a low rock and, often, a smaller one beside it.
fn boulder(out: &mut Vec<PropTri>, r: &mut Rng) {
    let half = [r.range(0.37, 0.42), r.range(0.31, 0.37)];
    let height = r.range(0.34, 0.46);
    let centre = [0.5 + r.range(-0.03, 0.03), 0.5 + r.range(-0.03, 0.03)];
    rock(out, r, centre, half, height);
    if r.next() < 0.65 {
        let a = r.range(0.0, std::f32::consts::TAU);
        let half = [r.range(0.11, 0.16), r.range(0.1, 0.15)];
        let height = r.range(0.14, 0.24);
        rock(out, r, [0.5 + a.cos() * 0.27, 0.5 + a.sin() * 0.27], half, height);
    }
}

/// One mushroom, `scale` of full size, standing at `centre`: a stem (its
/// top hidden under the cap) and a domed cap showing its gills beneath.
fn mushroom(out: &mut Vec<PropTri>, r: &mut Rng, centre: [f32; 2], scale: f32) {
    let turn = r.range(0.0, std::f32::consts::FRAC_PI_2);
    let stem = r.range(0.06, 0.085) * scale;
    let cap = r.range(0.22, 0.28) * scale;
    let thick = r.range(0.12, 0.17) * scale;
    let cap_y = r.range(0.24, 0.36) * scale;
    Lump {
        centre,
        turn,
        y: [0.0, cap_y],
        half: [stem, stem],
        top: [stem, stem],
        rects: [STEM_SIDE; 3],
        caps: (false, false),
    }
    .push(out);
    Lump {
        centre,
        turn,
        y: [cap_y, cap_y + thick],
        half: [cap, cap],
        top: [cap * 0.6, cap * 0.6],
        rects: [CAP_RIM, CAP_TOP, CAP_GILLS],
        caps: (true, true),
    }
    .push(out);
}

/// A fungus: one mushroom and up to two small ones at its foot.
fn fungus(out: &mut Vec<PropTri>, r: &mut Rng) {
    let a = r.range(0.0, std::f32::consts::TAU);
    let (s, c) = a.sin_cos();
    let scale = r.range(0.9, 1.05);
    mushroom(out, r, [0.5 + c * 0.06, 0.5 + s * 0.06], scale);
    let small = (r.next() * 3.0) as usize;
    for k in 0..small {
        let b = a + std::f32::consts::PI + (k as f32 - 0.5) * 1.3 + r.range(-0.2, 0.2);
        let (s, c) = b.sin_cos();
        let scale = r.range(0.48, 0.6);
        mushroom(out, r, [0.5 + c * 0.26, 0.5 + s * 0.26], scale);
    }
}

/// Appends the triangles of a gathering object's model for its variation
/// `seed` ([`prop_seed`]). A torch adds nothing: its model is
/// [`prop_model`].
pub fn flora_model(model: Model, seed: u32, out: &mut Vec<PropTri>) {
    let mut r = Rng(seed);
    match model {
        Model::Torch => {}
        Model::Plant => plant(out, &mut r),
        Model::Boulder => boulder(out, &mut r),
        Model::Fungus => fungus(out, &mut r),
    }
}

/// The triangles of every prop in the chunk.
pub fn mesh_props(p: &Padded, light: &PaddedLight, flags: &BlockFlags, out: &mut Vec<ShapedVertex>) {
    let mut scratch = Vec::new();
    for y in 0..N {
        for z in 0..N {
            for x in 0..N {
                let me = p.get(x, y, z);
                let Some(mount) = flags.mount(me) else {
                    continue;
                };
                let tris = match flags.model(me) {
                    Model::Torch => prop_model(mount),
                    model => {
                        scratch.clear();
                        flora_model(model, prop_seed(x, y, z, me), &mut scratch);
                        &scratch[..]
                    }
                };
                let own = light.get(x, y, z);
                let colour = p.colour(x, y, z);
                let base = [x as f32, y as f32, z as f32];
                for t in tris {
                    let l = if t.flame { Light::new(own.sky(), 15) } else { own };
                    for k in 0..3 {
                        let pos = std::array::from_fn(|a| base[a] + t.pos[k][a]);
                        out.push(ShapedVertex::new(pos, t.normal, t.tex_face, 3, me, l, colour).with_uv(t.uv[k]));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NO_NEIGHBORS, SHAPED_UNITS, mesh_chunk, mesh_chunk_lit};
    use bm_core::{BlockId, BlockRegistry, Chunk, ChunkLight, ChunkPos, Colour, LightMap, LocalPos, PlanetShape};
    use std::collections::HashMap;

    const MOUNTS: [Mount; 5] = [Mount::Floor, Mount::North, Mount::East, Mount::South, Mount::West];

    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn corners(m: &[PropTri]) -> Vec<[f32; 3]> {
        m.iter().flat_map(|t| t.pos).collect()
    }

    fn mean(pts: &[[f32; 3]], f: impl Fn([f32; 3]) -> f32) -> f32 {
        pts.iter().map(|&p| f(p)).sum::<f32>() / pts.len() as f32
    }

    #[test]
    fn models_have_the_expected_triangles_inside_the_cell() {
        for m in MOUNTS {
            let model = torch_model(m);
            // The stick has 5 faces on the floor (its foot is hidden) and 6
            // on a wall, the flame 6; two triangles each.
            let want = if m == Mount::Floor { (5 + 6) * 2 } else { (6 + 6) * 2 };
            assert_eq!(model.len(), want, "{m:?}");
            for t in &model {
                for p in t.pos {
                    assert!(
                        p.iter().all(|v| (0.0..=1.0).contains(v)),
                        "{m:?} corner {p:?} leaves the cell"
                    );
                }
                assert!(t.uv.iter().all(|uv| uv[0] <= 32 && uv[1] <= 32));
                assert!(
                    (dot(t.normal, t.normal) - 1.0).abs() < 1e-4,
                    "{m:?} normal {:?}",
                    t.normal
                );
            }
        }
    }

    #[test]
    fn triangles_wind_counter_clockwise_from_outside() {
        for m in MOUNTS {
            for t in torch_model(m) {
                let n = cross(sub(t.pos[1], t.pos[0]), sub(t.pos[2], t.pos[0]));
                assert!(dot(n, t.normal) > 0.0, "{m:?} {t:?}");
            }
        }
    }

    #[test]
    fn floor_torch_stands_upright_in_the_middle() {
        let pts = corners(&torch_model(Mount::Floor));
        assert!((mean(&pts, |p| p[0]) - 0.5).abs() < 1e-4);
        assert!((mean(&pts, |p| p[2]) - 0.5).abs() < 1e-4);
        let lo = pts.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        let hi = pts.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        assert_eq!(lo, 0.0, "stands on the floor");
        assert!((hi - 11.0 / 16.0).abs() < 1e-4, "{hi}");
        // Thin: nothing wider than the flame.
        for p in pts {
            assert!((p[0] - 0.5).abs() <= 1.5 / 16.0 + 1e-4 && (p[2] - 0.5).abs() <= 1.5 / 16.0 + 1e-4);
        }
    }

    #[test]
    fn wall_torches_hang_against_their_support_and_lean_out() {
        for m in [Mount::North, Mount::East, Mount::South, Mount::West] {
            let model = torch_model(m);
            let (dx, _, dz) = m.support();
            // How far a point is from the support's face, into the cell.
            let from_wall = move |p: [f32; 3]| match (dx, dz) {
                (0, -1) => p[2],
                (0, 1) => 1.0 - p[2],
                (1, 0) => 1.0 - p[0],
                _ => p[0],
            };
            let pts = corners(&model);
            let nearest = pts.iter().map(|&p| from_wall(p)).fold(f32::MAX, f32::min);
            assert!(nearest < 0.02, "{m:?} foot {nearest} from the wall");
            let flame: Vec<_> = model.iter().filter(|t| t.flame).flat_map(|t| t.pos).collect();
            let stick: Vec<_> = model.iter().filter(|t| !t.flame).flat_map(|t| t.pos).collect();
            assert!(
                mean(&flame, from_wall) > mean(&stick, from_wall) + 0.1,
                "{m:?} does not lean out"
            );
            assert!(mean(&flame, from_wall) < 0.6, "{m:?} strays from its wall");
            let lo = pts.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
            assert!(lo > 0.1, "{m:?} lowest point {lo}: raised off the floor");
            let along = if dx == 0 {
                mean(&pts, |p| p[0])
            } else {
                mean(&pts, |p| p[2])
            };
            assert!((along - 0.5).abs() < 1e-3, "{m:?} is centred along its wall");
        }
    }

    #[test]
    fn uv_survives_packing() {
        let v = ShapedVertex::new(
            [1.5, 2.0, 3.0],
            [0.0, 1.0, 0.0],
            2,
            3,
            BlockId(7),
            Light::new(4, 9),
            Colour(5),
        );
        assert_eq!(v.uv(), None);
        let u = v.with_uv([32, 17]);
        assert_eq!(u.uv(), Some([32, 17]));
        assert_eq!(u.position(), v.position());
        assert_eq!(
            (u.tex_face(), u.ao(), u.block(), u.light()),
            (2, 3, BlockId(7), Light::new(4, 9))
        );
        assert_eq!(u.colour(), Colour(5));
        assert!((u.normal()[1] - 1.0).abs() < 1e-2);
    }

    #[test]
    fn a_torch_is_drawn_as_a_model_and_hides_nothing() {
        let r = BlockRegistry::builtin();
        let mut c = Chunk::empty();
        c.set(LocalPos::new(4, 4, 6), r.id("stone").unwrap());
        c.set(LocalPos::new(4, 5, 6), r.id("torch").unwrap());
        let mesh = mesh_chunk(&c, &NO_NEIGHBORS, BlockId::AIR, &r.flags());
        // The stone keeps all six faces (its top under the torch too), and
        // the torch casts no occlusion on it.
        assert_eq!(mesh.opaque.len(), 6 * 4);
        assert!(mesh.opaque.iter().all(|v| v.ao() == 3));
        assert_eq!(mesh.shaped_opaque.len(), torch_model(Mount::Floor).len() * 3);
        let torch = r.id("torch").unwrap();
        for v in &mesh.shaped_opaque {
            assert_eq!((v.block(), v.ao()), (torch, 3));
            assert!(v.uv().is_some());
            let p = v.position().map(|q| q as f32 / SHAPED_UNITS);
            assert!(
                (4.0..=5.0).contains(&p[0]) && (5.0..=6.0).contains(&p[1]) && (6.0..=7.0).contains(&p[2]),
                "{p:?}"
            );
        }
    }

    #[test]
    fn wall_variants_mesh_against_their_side() {
        let r = BlockRegistry::builtin();
        for (name, m) in [
            ("torch_north", Mount::North),
            ("torch_east", Mount::East),
            ("torch_south", Mount::South),
            ("torch_west", Mount::West),
        ] {
            let mut c = Chunk::empty();
            c.set(LocalPos::new(8, 8, 8), r.id(name).unwrap());
            let mesh = mesh_chunk(&c, &NO_NEIGHBORS, BlockId::AIR, &r.flags());
            assert_eq!(mesh.shaped_opaque.len(), torch_model(m).len() * 3, "{name}");
            let n = mesh.shaped_opaque.len() as f32;
            let off = |a: usize| {
                mesh.shaped_opaque
                    .iter()
                    .map(|v| v.position()[a] as f32 / SHAPED_UNITS)
                    .sum::<f32>()
                    / n
                    - 8.5
            };
            // The model sits on its support's side of the cell.
            let (dx, _, dz) = m.support();
            if dx != 0 {
                assert!(off(0) * dx as f32 > 0.1, "{name} x {}", off(0));
            } else {
                assert!(off(2) * dz as f32 > 0.1, "{name} z {}", off(2));
            }
        }
    }

    #[test]
    fn the_stick_takes_its_cells_light_and_the_flame_glows() {
        let r = BlockRegistry::builtin();
        let (stone, torch) = (r.id("stone").unwrap(), r.id("torch").unwrap());
        // A sealed stone room with a torch on its floor.
        let shape = PlanetShape::new(4, 1);
        let mut world = HashMap::new();
        for x in 0..4 {
            for z in 0..4 {
                world.insert(ChunkPos::new(x, 0, z), Chunk::empty());
            }
        }
        let pos = ChunkPos::new(0, 0, 0);
        let c = world.get_mut(&pos).unwrap();
        for x in 8..17u8 {
            for y in 8..15u8 {
                for z in 8..17u8 {
                    if x == 8 || x == 16 || y == 8 || y == 14 || z == 8 || z == 16 {
                        c.set(LocalPos::new(x, y, z), stone);
                    }
                }
            }
        }
        c.set(LocalPos::new(12, 9, 12), torch);
        let mut map = LightMap::new(shape, &r.flags());
        let keys: Vec<ChunkPos> = world.keys().copied().collect();
        for p in keys {
            map.chunk_loaded(p, &world);
        }
        let nb: [Option<&Chunk>; 27] = std::array::from_fn(|k| {
            let (dx, dy, dz) = (k as i32 % 3 - 1, k as i32 / 9 - 1, k as i32 / 3 % 3 - 1);
            world.get(&shape.wrap_chunk(pos.offset(dx, dy, dz)))
        });
        let lights = map.neighborhood(pos);
        let refs: [Option<&ChunkLight>; 27] = std::array::from_fn(|i| lights[i].as_ref());
        let mesh = mesh_chunk_lit(&world[&pos], &nb, &refs, BlockId::AIR, &r.flags());
        let model = torch_model(Mount::Floor);
        assert_eq!(mesh.shaped_opaque.len(), model.len() * 3);
        // The stick takes the torch's own warm light; the flame glows white.
        let warm = Light::rgb(0, map.emission_of(bm_core::BlockState::plain(torch)));
        assert_eq!(warm.block(), 10);
        assert!(warm.block_rgb()[2] < 10, "warm: less blue");
        for (v, t) in mesh.shaped_opaque.iter().zip(model.iter().flat_map(|t| [t, t, t])) {
            let want = if t.flame { Light::new(0, 15) } else { warm };
            assert_eq!(v.light(), want, "{t:?}");
        }
        // The floor beside the torch is lit by it.
        assert!(mesh.opaque.iter().any(|v| v.face() == 2 && v.block_light() >= 9));
    }

    fn flora(model: Model, seed: u32) -> Vec<PropTri> {
        let mut v = Vec::new();
        flora_model(model, seed, &mut v);
        v
    }

    fn bounds(m: &[PropTri]) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in corners(m) {
            for a in 0..3 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a]);
            }
        }
        (lo, hi)
    }

    const SEEDS: std::ops::Range<u32> = 0..400;

    #[test]
    fn flora_models_have_their_triangles_inside_the_cell_facing_out() {
        for model in [Model::Plant, Model::Boulder, Model::Fungus] {
            for seed in SEEDS {
                let m = flora(model, seed);
                let n = m.len();
                match model {
                    // Two crossed quads, both sides.
                    Model::Plant => assert_eq!(n, 2 * 2 * 2),
                    // One or two rocks of six sides in two tiers and a
                    // six-triangle top.
                    Model::Boulder => assert!(n == 30 || n == 60, "{n}"),
                    // One to three mushrooms: a stem's four sides, a cap's
                    // four sides, top and gills.
                    _ => assert!([20, 40, 60].contains(&n), "{n}"),
                }
                for t in &m {
                    for p in t.pos {
                        assert!(
                            p.iter().all(|v| (0.0..=1.0).contains(v)),
                            "{model:?} {seed} corner {p:?} leaves the cell"
                        );
                    }
                    assert!(t.uv.iter().all(|uv| uv[0] <= 32 && uv[1] <= 32));
                    assert!((dot(t.normal, t.normal) - 1.0).abs() < 1e-3);
                    let wind = cross(sub(t.pos[1], t.pos[0]), sub(t.pos[2], t.pos[0]));
                    assert!(dot(wind, t.normal) > 0.0, "{model:?} {seed} winds inward");
                    assert!(!t.flame);
                }
                let (lo, hi) = bounds(&m);
                assert_eq!(lo[1], 0.0, "{model:?} stands on the floor");
                let (w, d) = (hi[0] - lo[0], hi[2] - lo[2]);
                match model {
                    Model::Plant => {
                        assert!((0.75..=0.91).contains(&hi[1]), "plant height {}", hi[1]);
                        assert!(w.max(d) > 0.5 && w.max(d) < 0.96, "plant width {w} {d}");
                    }
                    Model::Boulder => {
                        assert!((0.33..=0.47).contains(&hi[1]), "boulder height {}", hi[1]);
                        assert!(w.max(d) > 0.5 && w.max(d) < 0.98, "boulder width {w} {d}");
                    }
                    _ => {
                        assert!((0.3..=0.6).contains(&hi[1]), "fungus height {}", hi[1]);
                        assert!(w.max(d) > 0.38, "fungus width {w} {d}");
                    }
                }
            }
        }
    }

    #[test]
    fn plants_show_both_sides_of_the_same_picture() {
        let m = flora(Model::Plant, 7);
        // Each quad has a back twin: the same corners, the opposite normal,
        // and the same texture coordinate at each corner.
        for pair in m.chunks(4) {
            let (front, back) = (&pair[..2], &pair[2..]);
            assert!(dot(front[0].normal, back[0].normal) < -0.99);
            for f in front {
                for (k, p) in f.pos.iter().enumerate() {
                    let twin = back
                        .iter()
                        .flat_map(|b| b.pos.iter().zip(b.uv))
                        .find(|(q, _)| sub(**q, *p).iter().all(|v| v.abs() < 1e-6))
                        .expect("the back has the corner");
                    assert_eq!(twin.1, f.uv[k], "the picture is not mirrored at {p:?}");
                }
            }
        }
    }

    #[test]
    fn neighbouring_flora_differ() {
        for model in [Model::Plant, Model::Boulder, Model::Fungus] {
            let b = BlockId(40);
            let here = flora(model, prop_seed(3, 5, 3, b));
            assert_eq!(here, flora(model, prop_seed(3, 5, 3, b)), "deterministic");
            for (x, z) in [(4, 3), (3, 4), (2, 3)] {
                assert_ne!(
                    here,
                    flora(model, prop_seed(x, 5, z, b)),
                    "{model:?} repeats at {x} {z}"
                );
            }
            assert_ne!(here, flora(model, prop_seed(3, 5, 3, BlockId(41))));
        }
        // A fungus patch has single mushrooms and clusters.
        let sizes: std::collections::BTreeSet<usize> = SEEDS.map(|s| flora(Model::Fungus, s).len()).collect();
        assert_eq!(sizes.len(), 3, "{sizes:?}");
    }

    #[test]
    fn flora_is_drawn_as_models_lit_by_their_cell_and_hides_nothing() {
        let r = BlockRegistry::builtin();
        let stone = r.id("stone").unwrap();
        for (name, model) in [
            ("spikeleaf", Model::Plant),
            ("boulder", Model::Boulder),
            ("glowcap", Model::Fungus),
        ] {
            let id = r.id(name).unwrap();
            assert_eq!(r.flags().model(id), model, "{name}");
            let mut c = Chunk::empty();
            c.set(LocalPos::new(4, 4, 6), stone);
            c.set(LocalPos::new(4, 5, 6), id);
            let mesh = mesh_chunk(&c, &NO_NEIGHBORS, BlockId::AIR, &r.flags());
            assert_eq!(mesh.opaque.len(), 6 * 4, "{name}: the stone keeps its faces");
            assert!(mesh.opaque.iter().all(|v| v.ao() == 3), "{name} casts no occlusion");
            let want = flora(model, prop_seed(4, 5, 6, id)).len() * 3;
            assert_eq!(mesh.shaped_opaque.len(), want, "{name}");
            for v in &mesh.shaped_opaque {
                assert_eq!((v.block(), v.ao()), (id, 3));
                assert!(v.uv().is_some());
                assert_eq!(v.light(), Light::OPEN_SKY, "{name} takes its cell's light");
                let p = v.position().map(|q| q as f32 / SHAPED_UNITS);
                assert!(
                    (4.0..=5.0).contains(&p[0]) && (5.0..=6.0).contains(&p[1]) && (6.0..=7.0).contains(&p[2]),
                    "{name} {p:?}"
                );
            }
        }
    }
}
