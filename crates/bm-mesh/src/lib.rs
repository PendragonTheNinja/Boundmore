//! Chunk meshing.
//!
//! A chunk is meshed from a padded 34³ copy of its blocks (the chunk plus a
//! one-block border from its 26 neighbours), so faces on chunk borders are
//! culled correctly and ambient occlusion reads across seams. Light is padded
//! the same way ([`PaddedLight`]) for smooth lighting: each vertex takes the
//! average light of the open cells in front of its corner, sky and block light
//! kept apart so the shader can dim the sky at night without a re-mesh. The
//! mesher is greedy: coplanar faces of the same block with the same occlusion
//! and the same corner light merge into one quad, which typically cuts terrain
//! quads by 5–10×.
//!
//! Output is a list of [`Quad`]s, converted to packed 8-byte [`Vertex`]es for
//! the GPU. Quads are the unit tests compare: greedy output must cover exactly
//! the faces the naive mesher emits, with the same block, occlusion and light.

use bm_core::{BlockFlags, BlockId, CHUNK_SIZE, Chunk, ChunkLight, Colour, Light, LocalPos, Shape};

pub mod shaped;
pub use shaped::{ShapeTri, template};

const N: usize = CHUNK_SIZE as usize;
const P: usize = N + 2;

/// A chunk plus a one-block border, indexed `x + 1`, `y + 1`, `z + 1`.
pub struct Padded {
    blocks: Box<[BlockId]>,
    /// The centre chunk's colours (faces belong to the centre's blocks, so
    /// the border never needs one), indexed by `LocalPos::index`.
    colours: Box<[u8]>,
    /// Shapes (ADR-0008), indexed like `blocks`; `None` when every block in
    /// reach is a full cube, as almost all are.
    shapes: Option<Box<[Shape]>>,
}

impl Padded {
    #[inline]
    fn idx(x: i32, y: i32, z: i32) -> usize {
        (x + 1) as usize + (z + 1) as usize * P + (y + 1) as usize * P * P
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        self.blocks[Self::idx(x, y, z)]
    }

    /// The shape of a block (invalid shapes read as the full cube).
    #[inline]
    pub fn shape(&self, x: i32, y: i32, z: i32) -> Shape {
        match &self.shapes {
            Some(s) => s[Self::idx(x, y, z)],
            None => Shape::FULL,
        }
    }

    #[inline]
    pub fn full(&self, x: i32, y: i32, z: i32) -> bool {
        self.shapes.as_ref().is_none_or(|s| s[Self::idx(x, y, z)].is_full())
    }

    /// Whether the block hides what is behind it: opaque and uncut (a cut
    /// block is never opaque, ADR-0008).
    #[inline]
    pub fn opaque(&self, flags: &BlockFlags, x: i32, y: i32, z: i32) -> bool {
        flags.opaque(self.get(x, y, z)) && self.full(x, y, z)
    }

    /// The colour of a centre-chunk block.
    #[inline]
    pub fn colour(&self, x: i32, y: i32, z: i32) -> Colour {
        Colour(self.colours[LocalPos::new(x as u8, y as u8, z as u8).index()])
    }

    /// Builds the padded volume. `neighbors[(dy+1)*9 + (dz+1)*3 + (dx+1)]` is
    /// the chunk at offset `(dx, dy, dz)` (index 13, the centre, is ignored).
    /// A missing neighbour reads as `missing`.
    pub fn new(center: &Chunk, neighbors: &[Option<&Chunk>; 27], missing: BlockId) -> Self {
        let mut blocks = vec![missing; P * P * P].into_boxed_slice();
        let cut = |c: &Chunk| c.palette().iter().any(|s| !s.shape.is_full());
        let mut shapes = (cut(center) || neighbors.iter().flatten().any(|c| cut(c)))
            .then(|| vec![Shape::FULL; P * P * P].into_boxed_slice());
        let n = N as i32;
        let off = |v: i32| {
            if v < 0 {
                -1
            } else if v >= n {
                1
            } else {
                0
            }
        };
        for y in -1..=n {
            for z in -1..=n {
                for x in -1..=n {
                    let (dx, dy, dz) = (off(x), off(y), off(z));
                    let src = if (dx, dy, dz) == (0, 0, 0) {
                        Some(center)
                    } else {
                        neighbors[((dy + 1) * 9 + (dz + 1) * 3 + (dx + 1)) as usize]
                    };
                    if let Some(c) = src {
                        let l = LocalPos::new((x - dx * n) as u8, (y - dy * n) as u8, (z - dz * n) as u8);
                        let i = Self::idx(x, y, z);
                        match &mut shapes {
                            Some(shapes) => {
                                let st = c.state(l);
                                blocks[i] = st.id;
                                if st.shape.is_valid() {
                                    shapes[i] = st.shape;
                                }
                            }
                            None => blocks[i] = c.get(l),
                        }
                    }
                }
            }
        }
        let colours = if center.palette().iter().all(|s| s.colour == Colour::NATURAL) {
            vec![0; N * N * N].into_boxed_slice()
        } else {
            center.to_dense_states().iter().map(|s| s.colour.0).collect()
        };
        Self { blocks, colours, shapes }
    }
}

/// A chunk's light plus a one-cell border, indexed like [`Padded`].
pub struct PaddedLight {
    cells: Box<[Light]>,
}

impl PaddedLight {
    /// Every cell at one value.
    pub fn uniform(l: Light) -> Self {
        Self {
            cells: vec![l; P * P * P].into_boxed_slice(),
        }
    }

    /// Builds the padded light from a chunk's light and its neighbours', in
    /// the order [`Padded::new`] takes (index 13 is the chunk itself). `None`
    /// reads as dark: light that is not known yet is never invented.
    pub fn new(lights: &[Option<&ChunkLight>; 27]) -> Self {
        let mut cells = vec![Light::DARK; P * P * P].into_boxed_slice();
        let n = N as i32;
        let off = |v: i32| {
            if v < 0 {
                -1
            } else if v >= n {
                1
            } else {
                0
            }
        };
        for y in -1..=n {
            for z in -1..=n {
                for x in -1..=n {
                    let (dx, dy, dz) = (off(x), off(y), off(z));
                    if let Some(l) = lights[neighbor_index(dx, dy, dz)] {
                        let p = LocalPos::new((x - dx * n) as u8, (y - dy * n) as u8, (z - dz * n) as u8);
                        cells[Padded::idx(x, y, z)] = l.get(p);
                    }
                }
            }
        }
        Self { cells }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> Light {
        self.cells[Padded::idx(x, y, z)]
    }
}

/// Index of the neighbour at `(dx, dy, dz)` in the array [`Padded::new`] takes.
pub const fn neighbor_index(dx: i32, dy: i32, dz: i32) -> usize {
    ((dy + 1) * 9 + (dz + 1) * 3 + (dx + 1)) as usize
}

/// One face direction: the axis it faces along and which way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Face {
    pub axis: u8,
    pub positive: bool,
}

impl Face {
    pub const ALL: [Face; 6] = [
        Face {
            axis: 0,
            positive: true,
        },
        Face {
            axis: 0,
            positive: false,
        },
        Face {
            axis: 1,
            positive: true,
        },
        Face {
            axis: 1,
            positive: false,
        },
        Face {
            axis: 2,
            positive: true,
        },
        Face {
            axis: 2,
            positive: false,
        },
    ];

    /// 0..6, matching the shader: +X, -X, +Y, -Y, +Z, -Z.
    pub fn index(self) -> u32 {
        self.axis as u32 * 2 + if self.positive { 0 } else { 1 }
    }

    /// The two in-plane axes (u, v), chosen so u × v points along +axis.
    fn plane_axes(self) -> (usize, usize) {
        match self.axis {
            0 => (1, 2),
            1 => (2, 0),
            _ => (0, 1),
        }
    }
}

/// A rectangle of faces on one block layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Quad {
    pub face: Face,
    /// Block layer along the face axis (the block the face belongs to).
    pub layer: u8,
    pub u: u8,
    pub v: u8,
    pub w: u8,
    pub h: u8,
    pub block: BlockId,
    pub colour: Colour,
    /// Occlusion 0 (dark) to 3 (open) at corners (u0,v0), (u1,v0), (u1,v1), (u0,v1).
    pub ao: [u8; 4],
    /// Smooth light at the same corners.
    pub light: [Light; 4],
}

#[derive(Default, Debug)]
pub struct ChunkQuads {
    pub opaque: Vec<Quad>,
    pub translucent: Vec<Quad>,
}

/// `Some(translucent)` when the face of the full block `me` towards `other`
/// is visible. A cut neighbour never hides it (ADR-0008).
#[inline]
fn face_visible(flags: &BlockFlags, me: BlockId, other: BlockId, other_full: bool) -> Option<bool> {
    let covers = other_full && flags.opaque(other);
    if flags.opaque(me) {
        (!covers).then_some(false)
    } else if flags.translucent(me) {
        (!covers && !(other == me && other_full)).then_some(true)
    } else {
        None
    }
}

#[inline]
fn vertex_ao(side1: bool, side2: bool, corner: bool) -> u8 {
    if side1 && side2 {
        0
    } else {
        3 - (side1 as u8 + side2 as u8 + corner as u8)
    }
}

/// Occlusion at the four corners of the face of block `b` facing `face`.
fn face_ao(p: &Padded, flags: &BlockFlags, b: [i32; 3], face: Face) -> [u8; 4] {
    let (ua, va) = face.plane_axes();
    let mut front = b;
    front[face.axis as usize] += if face.positive { 1 } else { -1 };
    let occ = |du: i32, dv: i32| {
        let mut q = front;
        q[ua] += du;
        q[va] += dv;
        p.opaque(flags, q[0], q[1], q[2])
    };
    let corner = |su: i32, sv: i32| vertex_ao(occ(su, 0), occ(0, sv), occ(su, sv));
    [corner(-1, -1), corner(1, -1), corner(1, 1), corner(-1, 1)]
}

/// Light at the four corners of the face of block `b` facing `face`: per
/// corner, the average of the open cells among the one in front of the face
/// and the three beside the corner, sky and block light separately. The
/// diagonal cell is left out when both sides are closed, as AO treats it, so
/// light never leaks round a corner through a wall.
fn face_light(p: &Padded, light: &PaddedLight, flags: &BlockFlags, b: [i32; 3], face: Face) -> [Light; 4] {
    let (ua, va) = face.plane_axes();
    let mut front = b;
    front[face.axis as usize] += if face.positive { 1 } else { -1 };
    let cell = |du: i32, dv: i32| {
        let mut q = front;
        q[ua] += du;
        q[va] += dv;
        (p.opaque(flags, q[0], q[1], q[2]), light.get(q[0], q[1], q[2]))
    };
    let centre = light.get(front[0], front[1], front[2]);
    let corner = |su: i32, sv: i32| {
        let (o1, l1) = cell(su, 0);
        let (o2, l2) = cell(0, sv);
        let (oc, lc) = cell(su, sv);
        let mut sum = (centre.sky() as u32, centre.block() as u32);
        let mut count = 1;
        for (open, l) in [(!o1, l1), (!o2, l2), (!oc && !(o1 && o2), lc)] {
            if open {
                sum.0 += l.sky() as u32;
                sum.1 += l.block() as u32;
                count += 1;
            }
        }
        // Rounded, so a block standing in open sky is at 15 all over.
        let avg = |v: u32| ((2 * v + count) / (2 * count)) as u8;
        Light::new(avg(sum.0), avg(sum.1))
    };
    [corner(-1, -1), corner(1, -1), corner(1, 1), corner(-1, 1)]
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    block: BlockId,
    colour: Colour,
    ao: [u8; 4],
    light: [Light; 4],
    translucent: bool,
}

/// Visits every visible face on one layer, one per block side.
fn for_each_face(
    p: &Padded,
    light: &PaddedLight,
    flags: &BlockFlags,
    face: Face,
    layer: usize,
    mut f: impl FnMut(usize, usize, Cell),
) {
    let (ua, va) = face.plane_axes();
    let d = if face.positive { 1 } else { -1 };
    for v in 0..N {
        for u in 0..N {
            let mut b = [0i32; 3];
            b[face.axis as usize] = layer as i32;
            b[ua] = u as i32;
            b[va] = v as i32;
            let me = p.get(b[0], b[1], b[2]);
            // Cut blocks are drawn on their own (`mesh_shaped`).
            if !p.full(b[0], b[1], b[2]) {
                continue;
            }
            let mut o = b;
            o[face.axis as usize] += d;
            let other = p.get(o[0], o[1], o[2]);
            if let Some(translucent) = face_visible(flags, me, other, p.full(o[0], o[1], o[2])) {
                let ao = if translucent {
                    [3; 4]
                } else {
                    face_ao(p, flags, b, face)
                };
                f(
                    u,
                    v,
                    Cell {
                        block: me,
                        colour: p.colour(b[0], b[1], b[2]),
                        ao,
                        light: face_light(p, light, flags, b, face),
                        translucent,
                    },
                );
            }
        }
    }
}

/// Greedy mesher.
pub fn mesh_quads(p: &Padded, light: &PaddedLight, flags: &BlockFlags) -> ChunkQuads {
    let mut out = ChunkQuads::default();
    let mut mask: Vec<Option<Cell>> = vec![None; N * N];
    for face in Face::ALL {
        for layer in 0..N {
            mask.fill(None);
            let mut any = false;
            for_each_face(p, light, flags, face, layer, |u, v, c| {
                mask[v * N + u] = Some(c);
                any = true;
            });
            if !any {
                continue;
            }
            for v in 0..N {
                let mut u = 0;
                while u < N {
                    let Some(cell) = mask[v * N + u] else {
                        u += 1;
                        continue;
                    };
                    let mut w = 1;
                    while u + w < N && mask[v * N + u + w] == Some(cell) {
                        w += 1;
                    }
                    let mut h = 1;
                    'grow: while v + h < N {
                        for k in 0..w {
                            if mask[(v + h) * N + u + k] != Some(cell) {
                                break 'grow;
                            }
                        }
                        h += 1;
                    }
                    for dv in 0..h {
                        mask[(v + dv) * N + u..(v + dv) * N + u + w].fill(None);
                    }
                    let q = Quad {
                        face,
                        layer: layer as u8,
                        u: u as u8,
                        v: v as u8,
                        w: w as u8,
                        h: h as u8,
                        block: cell.block,
                        colour: cell.colour,
                        ao: cell.ao,
                        light: cell.light,
                    };
                    if cell.translucent {
                        out.translucent.push(q)
                    } else {
                        out.opaque.push(q)
                    }
                    u += w;
                }
            }
        }
    }
    out
}

/// One quad per visible face. The reference the greedy mesher is tested
/// against; never used in the game.
pub fn mesh_quads_naive(p: &Padded, light: &PaddedLight, flags: &BlockFlags) -> ChunkQuads {
    let mut out = ChunkQuads::default();
    for face in Face::ALL {
        for layer in 0..N {
            for_each_face(p, light, flags, face, layer, |u, v, c| {
                let q = Quad {
                    face,
                    layer: layer as u8,
                    u: u as u8,
                    v: v as u8,
                    w: 1,
                    h: 1,
                    block: c.block,
                    colour: c.colour,
                    ao: c.ao,
                    light: c.light,
                };
                if c.translucent {
                    out.translucent.push(q)
                } else {
                    out.opaque.push(q)
                }
            });
        }
    }
    out
}

/// A packed vertex, 8 bytes.
///
/// `a`: x (6 bits) | y << 6 | z << 12 | face << 18 (3 bits) | ao << 21 (2 bits)
/// `b`: block id (16 bits) | sky light << 16 (4 bits) | block light << 20
/// (4 bits) | colour << 24 (8 bits, palette id, 0 natural; ADR-0007).
///
/// Sky and block light are separate so the shader can scale sky light by the
/// time of day (and leave glow crystals alone) without anything re-meshing.
///
/// Positions are chunk-local corners in `0..=32`. The shader rebuilds texture
/// coordinates from position and face, so quads of any size tile.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub a: u32,
    pub b: u32,
}

impl Vertex {
    pub fn new(pos: [u32; 3], face: Face, ao: u8, block: BlockId, light: Light) -> Self {
        Self {
            a: pos[0] | pos[1] << 6 | pos[2] << 12 | face.index() << 18 | (ao as u32) << 21,
            b: block.0 as u32 | (light.sky() as u32) << 16 | (light.block() as u32) << 20,
        }
    }

    /// The same vertex in a palette colour (ADR-0007).
    pub fn with_colour(self, c: Colour) -> Self {
        Self {
            b: (self.b & 0x00ff_ffff) | (c.0 as u32) << 24,
            ..self
        }
    }

    pub fn block(self) -> BlockId {
        BlockId(self.b as u16)
    }

    pub fn colour(self) -> Colour {
        Colour((self.b >> 24) as u8)
    }

    pub fn sky(self) -> u8 {
        (self.b >> 16 & 15) as u8
    }

    pub fn block_light(self) -> u8 {
        (self.b >> 20 & 15) as u8
    }

    pub fn light(self) -> Light {
        Light::new(self.sky(), self.block_light())
    }

    /// For choosing the quad diagonal: occlusion first, light to break ties.
    fn brightness(self) -> u32 {
        self.ao() as u32 * 64 + self.sky() as u32 + self.block_light() as u32
    }

    pub fn position(self) -> [u32; 3] {
        [self.a & 63, self.a >> 6 & 63, self.a >> 12 & 63]
    }

    pub fn face(self) -> u32 {
        self.a >> 18 & 7
    }

    pub fn ao(self) -> u8 {
        (self.a >> 21 & 3) as u8
    }
}

/// Every quad becomes four vertices; draw with [`quad_indices`].
pub fn quads_to_vertices(quads: &[Quad], out: &mut Vec<Vertex>) {
    out.reserve(quads.len() * 4);
    for q in quads {
        let (ua, va) = q.face.plane_axes();
        let mut base = [0u32; 3];
        base[q.face.axis as usize] = q.layer as u32 + q.face.positive as u32;
        base[ua] = q.u as u32;
        base[va] = q.v as u32;
        let corner = |du: u32, dv: u32| {
            let mut p = base;
            p[ua] += du;
            p[va] += dv;
            p
        };
        let (w, h) = (q.w as u32, q.h as u32);
        // Counter-clockwise seen from outside. u × v points along +axis, so
        // the (u, v) order is CCW for positive faces and reversed for negative.
        let mut v = [
            Vertex::new(corner(0, 0), q.face, q.ao[0], q.block, q.light[0]).with_colour(q.colour),
            Vertex::new(corner(w, 0), q.face, q.ao[1], q.block, q.light[1]).with_colour(q.colour),
            Vertex::new(corner(w, h), q.face, q.ao[2], q.block, q.light[2]).with_colour(q.colour),
            Vertex::new(corner(0, h), q.face, q.ao[3], q.block, q.light[3]).with_colour(q.colour),
        ];
        if !q.face.positive {
            v.swap(1, 3);
        }
        // Split along the diagonal with the brighter ends, so occlusion and
        // light gradients show no crease (the classic AO anisotropy fix).
        if v[0].brightness() + v[2].brightness() < v[1].brightness() + v[3].brightness() {
            v.rotate_left(1);
        }
        out.extend_from_slice(&v);
    }
}

/// Index buffer pattern for `quads` quads drawn from [`quads_to_vertices`].
pub fn quad_indices(quads: usize) -> Vec<u32> {
    (0..quads as u32)
        .flat_map(|q| {
            let b = q * 4;
            [b, b + 1, b + 2, b, b + 2, b + 3]
        })
        .collect()
}

/// A chunk's finished mesh: greedy quads of full cubes (four [`Vertex`]es
/// each, drawn with [`quad_indices`]) and triangles of cut blocks (three
/// [`ShapedVertex`]es each, drawn as a plain list).
#[derive(Default, Debug)]
pub struct ChunkMesh {
    pub opaque: Vec<Vertex>,
    pub translucent: Vec<Vertex>,
    pub shaped_opaque: Vec<ShapedVertex>,
    pub shaped_translucent: Vec<ShapedVertex>,
}

impl ChunkMesh {
    pub fn is_empty(&self) -> bool {
        self.opaque.is_empty()
            && self.translucent.is_empty()
            && self.shaped_opaque.is_empty()
            && self.shaped_translucent.is_empty()
    }
}

/// Fixed-point steps per block in a [`ShapedVertex`] position.
pub const SHAPED_UNITS: f32 = 256.0;

/// A vertex of a cut block, 16 bytes. Cuts need positions finer than the
/// cube vertex's whole corners, and a normal.
///
/// `xy`: x | y << 16, chunk-local in 1/[`SHAPED_UNITS`] of a block (0..=8192).
/// `z`: z | texture face << 16 (3 bits, as [`Face::index`]) | ao << 19 (2 bits).
/// `normal`: x, y, z as signed bytes (`unpack4x8snorm`).
/// `b`: as [`Vertex`]: block id | sky << 16 | block light << 20 | colour << 24.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShapedVertex {
    pub xy: u32,
    pub z: u32,
    pub normal: u32,
    pub b: u32,
}

impl ShapedVertex {
    /// `pos` in blocks, chunk-local; `normal` a unit vector.
    pub fn new(pos: [f32; 3], normal: [f32; 3], tex_face: u8, ao: u8, block: BlockId, light: Light, c: Colour) -> Self {
        let q = |v: f32| (v * SHAPED_UNITS).round().clamp(0.0, 65535.0) as u32;
        let n = |v: f32| ((v.clamp(-1.0, 1.0) * 127.0).round() as i8) as u8 as u32;
        Self {
            xy: q(pos[0]) | q(pos[1]) << 16,
            z: q(pos[2]) | (tex_face as u32 & 7) << 16 | (ao as u32 & 3) << 19,
            normal: n(normal[0]) | n(normal[1]) << 8 | n(normal[2]) << 16,
            b: block.0 as u32 | (light.sky() as u32) << 16 | (light.block() as u32) << 20 | (c.0 as u32) << 24,
        }
    }

    /// Position in 1/[`SHAPED_UNITS`] of a block.
    pub fn position(self) -> [u32; 3] {
        [self.xy & 0xffff, self.xy >> 16, self.z & 0xffff]
    }

    pub fn normal(self) -> [f32; 3] {
        let c = |s: u32| (self.normal >> s) as u8 as i8 as f32 / 127.0;
        [c(0), c(8), c(16)]
    }

    pub fn tex_face(self) -> u32 {
        self.z >> 16 & 7
    }

    pub fn ao(self) -> u8 {
        (self.z >> 19 & 3) as u8
    }

    pub fn block(self) -> BlockId {
        BlockId(self.b as u16)
    }

    pub fn colour(self) -> Colour {
        Colour((self.b >> 24) as u8)
    }

    pub fn light(self) -> Light {
        Light::new((self.b >> 16 & 15) as u8, (self.b >> 20 & 15) as u8)
    }
}

/// Occlusion (0 dark to 3 open) and light at a point of a cut block's
/// surface: the cells around the point half a block out along `n`, weighted
/// by nearness. Opaque cells occlude and lend no light. For a cube corner
/// this is the classic four cells in front of it.
fn shaped_sample(p: &Padded, light: &PaddedLight, flags: &BlockFlags, b: [i32; 3], q: [f32; 3], n: [f32; 3]) -> (u8, Light) {
    let s: [f32; 3] = std::array::from_fn(|a| b[a] as f32 + q[a] + n[a] * 0.5 - 0.5);
    let c0: [i32; 3] = s.map(|v| v.floor() as i32);
    let f: [f32; 3] = std::array::from_fn(|a| s[a] - c0[a] as f32);
    let (mut occ, mut open, mut sky, mut blk) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for k in 0..8 {
        let d = [k & 1, k >> 1 & 1, k >> 2 & 1];
        let w: f32 = (0..3).map(|a| if d[a] == 1 { f[a] } else { 1.0 - f[a] }).product();
        if w <= 0.0 {
            continue;
        }
        let c: [i32; 3] = std::array::from_fn(|a| (c0[a] + d[a]).clamp(-1, N as i32));
        if p.opaque(flags, c[0], c[1], c[2]) {
            occ += w;
        } else {
            let l = light.get(c[0], c[1], c[2]);
            open += w;
            sky += w * l.sky() as f32;
            blk += w * l.block() as f32;
        }
    }
    let ao = (3.0 - (occ * 4.0).round()).clamp(0.0, 3.0) as u8;
    let l = if open > 1e-3 {
        Light::new((sky / open).round() as u8, (blk / open).round() as u8)
    } else {
        light.get(b[0], b[1], b[2])
    };
    (ao, l)
}

/// The triangles of every cut block in the chunk, from their templates
/// ([`shaped::template`]). A triangle on the block's boundary is dropped
/// where the neighbour is a full opaque block (or, for a translucent block,
/// a full block of its own kind).
pub fn mesh_shaped(p: &Padded, light: &PaddedLight, flags: &BlockFlags, mesh: &mut ChunkMesh) {
    if p.shapes.is_none() {
        return;
    }
    for y in 0..N as i32 {
        for z in 0..N as i32 {
            for x in 0..N as i32 {
                let shape = p.shape(x, y, z);
                if shape.is_full() {
                    continue;
                }
                let me = p.get(x, y, z);
                let out = if flags.opaque(me) {
                    &mut mesh.shaped_opaque
                } else if flags.translucent(me) {
                    &mut mesh.shaped_translucent
                } else {
                    continue;
                };
                let colour = p.colour(x, y, z);
                let b = [x, y, z];
                for t in template(shape) {
                    if let Some(f) = t.face {
                        let mut o = b;
                        o[f.axis as usize] += if f.positive { 1 } else { -1 };
                        let other = p.get(o[0], o[1], o[2]);
                        let full = p.full(o[0], o[1], o[2]);
                        if full && (flags.opaque(other) || (other == me && flags.translucent(me))) {
                            continue;
                        }
                    }
                    for k in 0..3 {
                        let (ao, l) = shaped_sample(p, light, flags, b, t.pos[k], t.normal[k]);
                        let pos = [
                            x as f32 + t.pos[k][0],
                            y as f32 + t.pos[k][1],
                            z as f32 + t.pos[k][2],
                        ];
                        out.push(ShapedVertex::new(pos, t.normal[k], t.tex_face, ao, me, l, colour));
                    }
                }
            }
        }
    }
}

/// Meshes a chunk at full daylight (sky 15 everywhere), for callers without
/// a light map.
pub fn mesh_chunk(center: &Chunk, neighbors: &[Option<&Chunk>; 27], missing: BlockId, flags: &BlockFlags) -> ChunkMesh {
    mesh_with(center, neighbors, missing, flags, || {
        PaddedLight::uniform(Light::OPEN_SKY)
    })
}

/// Meshes a chunk with smooth lighting. `lights` is in the same order as
/// `neighbors` (index 13 is the chunk's own light); take it from
/// `LightMap::neighborhood`. Unknown light (`None`) meshes as dark.
pub fn mesh_chunk_lit(
    center: &Chunk,
    neighbors: &[Option<&Chunk>; 27],
    lights: &[Option<&ChunkLight>; 27],
    missing: BlockId,
    flags: &BlockFlags,
) -> ChunkMesh {
    mesh_with(center, neighbors, missing, flags, || PaddedLight::new(lights))
}

/// Uniform chunks that can show no faces skip the scan (and the light).
fn mesh_with(
    center: &Chunk,
    neighbors: &[Option<&Chunk>; 27],
    missing: BlockId,
    flags: &BlockFlags,
    light: impl FnOnce() -> PaddedLight,
) -> ChunkMesh {
    // Uniform in one full cube: the scan can often be skipped.
    let uniform = |c: &Chunk| match c.palette() {
        [s] if s.shape.is_full() => Some(s.id),
        _ => None,
    };
    if let Some(b) = uniform(center) {
        if b.is_air() {
            return ChunkMesh::default();
        }
        let faces = [
            neighbor_index(0, -1, 0),
            neighbor_index(0, 1, 0),
            neighbor_index(-1, 0, 0),
            neighbor_index(1, 0, 0),
            neighbor_index(0, 0, -1),
            neighbor_index(0, 0, 1),
        ];
        let sealed = faces.iter().all(|&i| {
            neighbors[i]
                .and_then(uniform)
                .is_some_and(|nb| flags.opaque(nb))
        });
        if flags.opaque(b) && sealed {
            return ChunkMesh::default();
        }
    }
    let padded = Padded::new(center, neighbors, missing);
    let light = light();
    let quads = mesh_quads(&padded, &light, flags);
    let mut mesh = ChunkMesh::default();
    quads_to_vertices(&quads.opaque, &mut mesh.opaque);
    quads_to_vertices(&quads.translucent, &mut mesh.translucent);
    mesh_shaped(&padded, &light, flags, &mut mesh);
    mesh
}

/// An all-missing neighbourhood.
pub const NO_NEIGHBORS: [Option<&Chunk>; 27] = [None; 27];

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{BlockPos, BlockRegistry, BlockState, CHUNK_VOLUME, ChunkPos, LightMap, PlanetShape};
    use rand::{Rng, SeedableRng, rngs::StdRng};
    use std::collections::BTreeSet;
    use std::collections::HashMap;

    fn reg() -> BlockRegistry {
        BlockRegistry::builtin()
    }

    type UnitFace = (Face, u8, u8, u8, BlockId, Colour, [u8; 4], [Light; 4]);

    fn bright() -> PaddedLight {
        PaddedLight::uniform(Light::OPEN_SKY)
    }

    /// Expands quads to unit faces.
    fn raster(quads: &[Quad]) -> BTreeSet<UnitFace> {
        let mut s = BTreeSet::new();
        for q in quads {
            for dv in 0..q.h {
                for du in 0..q.w {
                    assert!(
                        s.insert((q.face, q.layer, q.u + du, q.v + dv, q.block, q.colour, q.ao, q.light)),
                        "overlapping quads"
                    );
                }
            }
        }
        s
    }

    /// Random blocks of `ids`, a third of them in one of three colours, so
    /// the greedy mesher is checked never to merge across colours.
    fn random_chunk(rng: &mut StdRng, ids: &[BlockId], fill: f64) -> Chunk {
        let dense: Vec<BlockState> = (0..CHUNK_VOLUME)
            .map(|_| {
                if rng.random_bool(fill) {
                    let id = ids[rng.random_range(0..ids.len())];
                    let colour = if rng.random_bool(0.33) {
                        Colour(rng.random_range(1..4))
                    } else {
                        Colour::NATURAL
                    };
                    BlockState::coloured(id, colour)
                } else {
                    BlockState::AIR
                }
            })
            .collect();
        Chunk::from_dense_states(&dense)
    }

    /// Patchy light: a few values in blocks of cells, so faces still merge
    /// in places and split at every change.
    fn random_light(rng: &mut StdRng) -> ChunkLight {
        let levels = [Light::OPEN_SKY, Light::new(14, 3), Light::new(6, 0), Light::new(0, 11)];
        let cells: Vec<Light> = (0..512).map(|_| levels[rng.random_range(0..levels.len())]).collect();
        ChunkLight::from_fn(|i| {
            let p = LocalPos::from_index(i);
            cells[(p.x as usize / 4) | (p.z as usize / 4) << 3 | (p.y as usize / 4) << 6]
        })
    }

    #[test]
    fn greedy_covers_exactly_the_naive_faces() {
        let r = reg();
        let flags = r.flags();
        let ids = [
            r.id("stone").unwrap(),
            r.id("turf").unwrap(),
            r.id("water").unwrap(),
            r.id("glass").unwrap(),
        ];
        let mut rng = StdRng::seed_from_u64(9);
        for fill in [0.05, 0.4, 0.8, 0.97] {
            let center = random_chunk(&mut rng, &ids, fill);
            let others: Vec<Chunk> = (0..27).map(|_| random_chunk(&mut rng, &ids, fill)).collect();
            let mut nb: [Option<&Chunk>; 27] = [None; 27];
            for (i, c) in others.iter().enumerate() {
                if i % 4 != 0 {
                    nb[i] = Some(c);
                }
            }
            let p = Padded::new(&center, &nb, BlockId::AIR);
            let lights: Vec<ChunkLight> = (0..27).map(|_| random_light(&mut rng)).collect();
            let mut lnb: [Option<&ChunkLight>; 27] = [None; 27];
            for (i, l) in lights.iter().enumerate() {
                if i % 5 != 1 {
                    lnb[i] = Some(l);
                }
            }
            for light in [bright(), PaddedLight::new(&lnb)] {
                let (g, n) = (mesh_quads(&p, &light, &flags), mesh_quads_naive(&p, &light, &flags));
                assert_eq!(raster(&g.opaque), raster(&n.opaque));
                assert_eq!(raster(&g.translucent), raster(&n.translucent));
                assert!(g.opaque.len() <= n.opaque.len());
            }
        }
    }

    #[test]
    fn flat_ground_merges_to_one_quad() {
        let r = reg();
        let stone = r.id("stone").unwrap();
        let mut dense = vec![BlockId::AIR; CHUNK_VOLUME];
        for (i, b) in dense.iter_mut().enumerate() {
            if LocalPos::from_index(i).y < 10 {
                *b = stone;
            }
        }
        let c = Chunk::from_dense(&dense);
        let solid = Chunk::filled(stone);
        let mut nb = NO_NEIGHBORS;
        nb[neighbor_index(0, -1, 0)] = Some(&solid);
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            nb[neighbor_index(dx, 0, dz)] = Some(&c);
        }
        let q = mesh_quads(&Padded::new(&c, &nb, BlockId::AIR), &bright(), &r.flags());
        // Only the top shows; the sides face more ground.
        assert_eq!(q.opaque.len(), 1, "{:?}", q.opaque);
        assert_eq!((q.opaque[0].w, q.opaque[0].h), (32, 32));
    }

    #[test]
    fn single_block_has_six_open_faces() {
        let r = reg();
        let mut c = Chunk::empty();
        c.set(LocalPos::new(5, 5, 5), r.id("stone").unwrap());
        let q = mesh_quads(&Padded::new(&c, &NO_NEIGHBORS, BlockId::AIR), &bright(), &r.flags());
        assert_eq!(q.opaque.len(), 6);
        assert!(q.opaque.iter().all(|q| q.ao == [3; 4]));
    }

    #[test]
    fn edge_occlusion_darkens_the_right_corners() {
        let r = reg();
        let stone = r.id("stone").unwrap();
        let mut c = Chunk::empty();
        c.set(LocalPos::new(5, 5, 5), stone);
        c.set(LocalPos::new(6, 6, 5), stone); // over the +X edge of the top face
        let q = mesh_quads(&Padded::new(&c, &NO_NEIGHBORS, BlockId::AIR), &bright(), &r.flags());
        let top = q
            .opaque
            .iter()
            .find(|q| {
                q.face
                    == Face {
                        axis: 1,
                        positive: true,
                    }
                    && q.layer == 5
            })
            .unwrap();
        // +Y plane axes are (u = z, v = x): the corners at v = x + 1 darken.
        assert_eq!(top.ao, [3, 3, 2, 2]);
    }

    #[test]
    fn water_shows_its_surface_but_no_internal_faces() {
        let r = reg();
        let c = Chunk::filled(r.id("water").unwrap());
        let q = mesh_quads(&Padded::new(&c, &NO_NEIGHBORS, BlockId::AIR), &bright(), &r.flags());
        assert!(q.opaque.is_empty());
        assert_eq!(q.translucent.len(), 6, "six merged outer walls");
        let mut nb = NO_NEIGHBORS;
        for (i, slot) in nb.iter_mut().enumerate() {
            if i != neighbor_index(0, 1, 0) {
                *slot = Some(&c);
            }
        }
        let q = mesh_quads(&Padded::new(&c, &nb, BlockId::AIR), &bright(), &r.flags());
        assert_eq!(q.translucent.len(), 1, "only the surface under the air");
    }

    #[test]
    fn vertices_wind_counter_clockwise_from_outside() {
        let r = reg();
        let mut c = Chunk::empty();
        c.set(LocalPos::new(3, 4, 5), r.id("stone").unwrap());
        c.set(LocalPos::new(4, 5, 5), r.id("stone").unwrap()); // forces some AO flips
        let q = mesh_quads(&Padded::new(&c, &NO_NEIGHBORS, BlockId::AIR), &bright(), &r.flags());
        let mut verts = Vec::new();
        quads_to_vertices(&q.opaque, &mut verts);
        let normals: [[i64; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
        for quad in verts.chunks(4) {
            let p: Vec<[i64; 3]> = quad.iter().map(|v| v.position().map(|c| c as i64)).collect();
            let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
            let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let want = normals[quad[0].face() as usize];
            assert!(
                n[0] * want[0] + n[1] * want[1] + n[2] * want[2] > 0,
                "face {} wound clockwise",
                quad[0].face()
            );
        }
    }

    #[test]
    fn uniform_chunks_short_circuit() {
        let r = reg();
        let flags = r.flags();
        assert!(mesh_chunk(&Chunk::empty(), &NO_NEIGHBORS, BlockId::AIR, &flags).is_empty());
        let stone = Chunk::filled(r.id("stone").unwrap());
        let nb: [Option<&Chunk>; 27] = [Some(&stone); 27];
        assert!(mesh_chunk(&stone, &nb, BlockId::AIR, &flags).is_empty());
        assert_eq!(
            mesh_chunk(&stone, &NO_NEIGHBORS, BlockId::AIR, &flags).opaque.len(),
            6 * 4
        );
    }

    #[test]
    fn vertex_light_packing_round_trips() {
        for sky in 0..16 {
            for block in 0..16 {
                let face = Face::ALL[(sky + block) as usize % 6];
                let v =
                    Vertex::new([32, 17, 5], face, 2, BlockId(0xbeef), Light::new(sky, block)).with_colour(Colour(201));
                assert_eq!(v.colour(), Colour(201));
                assert_eq!((v.sky(), v.block_light()), (sky, block));
                assert_eq!(v.light(), Light::new(sky, block));
                assert_eq!(v.block(), BlockId(0xbeef));
                assert_eq!((v.position(), v.face(), v.ao()), ([32, 17, 5], face.index(), 2));
                assert_eq!(
                    v.with_colour(Colour::NATURAL).b >> 24,
                    0,
                    "natural colour clears the top byte"
                );
            }
        }
    }

    #[test]
    fn full_bright_meshing_is_open_sky() {
        let r = reg();
        let mut c = Chunk::empty();
        c.set(LocalPos::new(3, 4, 5), r.id("stone").unwrap());
        let mesh = mesh_chunk(&c, &NO_NEIGHBORS, BlockId::AIR, &r.flags());
        assert!(mesh.opaque.iter().all(|v| v.light() == Light::OPEN_SKY));
    }

    /// A world on a one-chunk-high planet, lit by the real light map.
    fn lit_world(blocks: &[(BlockPos, BlockId)]) -> (HashMap<ChunkPos, Chunk>, LightMap) {
        let shape = PlanetShape::new(4, 1);
        let mut world = HashMap::new();
        for x in 0..4 {
            for z in 0..4 {
                world.insert(ChunkPos::new(x, 0, z), Chunk::empty());
            }
        }
        for &(p, id) in blocks {
            world.get_mut(&p.chunk()).unwrap().set(p.local(), id);
        }
        let mut map = LightMap::new(shape, &reg().flags());
        let keys: Vec<ChunkPos> = world.keys().copied().collect();
        for p in keys {
            map.chunk_loaded(p, &world);
        }
        (world, map)
    }

    fn mesh_lit(world: &HashMap<ChunkPos, Chunk>, map: &LightMap, pos: ChunkPos) -> ChunkMesh {
        let shape = map.shape();
        let nb: [Option<&Chunk>; 27] = std::array::from_fn(|k| {
            let (dx, dy, dz) = (k as i32 % 3 - 1, k as i32 / 9 - 1, k as i32 / 3 % 3 - 1);
            world.get(&shape.wrap_chunk(pos.offset(dx, dy, dz)))
        });
        let lights = map.neighborhood(pos);
        let refs: [Option<&ChunkLight>; 27] = std::array::from_fn(|i| lights[i].as_ref());
        mesh_chunk_lit(&world[&pos], &nb, &refs, BlockId::AIR, &reg().flags())
    }

    #[test]
    fn a_block_in_open_sky_is_lit_at_every_vertex() {
        let stone = reg().id("stone").unwrap();
        // On a chunk corner, so every face samples a neighbour's light.
        let (world, map) = lit_world(&[(BlockPos::new(31, 16, 31), stone)]);
        let mesh = mesh_lit(&world, &map, ChunkPos::new(0, 0, 0));
        assert_eq!(mesh.opaque.len(), 6 * 4);
        assert!(
            mesh.opaque.iter().all(|v| v.light() == Light::OPEN_SKY),
            "{:?}",
            mesh.opaque
        );
    }

    #[test]
    fn glow_lights_the_faces_near_it() {
        let r = reg();
        let (stone, glow) = (r.id("stone").unwrap(), r.id("glow_crystal").unwrap());
        // A sealed stone room with a crystal on its floor.
        let mut blocks = Vec::new();
        for x in 8..17 {
            for y in 8..15 {
                for z in 8..17 {
                    let wall = x == 8 || x == 16 || y == 8 || y == 14 || z == 8 || z == 16;
                    if wall {
                        blocks.push((BlockPos::new(x, y, z), stone));
                    }
                }
            }
        }
        blocks.push((BlockPos::new(12, 9, 12), glow));
        let (world, map) = lit_world(&blocks);
        let mesh = mesh_lit(&world, &map, ChunkPos::new(0, 0, 0));
        // Faces inside the room see only the crystal; faces outside only the sky.
        for v in &mesh.opaque {
            assert_eq!(v.sky() == 0, v.block_light() > 0, "{v:?} at {:?}", v.position());
        }
        // The crystal's own top face: the cells over it are at 11 and 10.
        let top: Vec<&Vertex> = mesh
            .opaque
            .iter()
            .filter(|v| v.face() == 2 && v.position()[1] == 10)
            .collect();
        assert_eq!(top.len(), 4);
        assert!(top.iter().all(|v| v.block_light() == 10), "{top:?}");
    }

    fn cut(r: &BlockRegistry, name: &str, family: bm_core::ShapeFamily, removed: u8) -> BlockState {
        BlockState {
            shape: Shape::new(family, removed).unwrap(),
            ..BlockState::plain(r.id(name).unwrap())
        }
    }

    #[test]
    fn a_cut_block_is_meshed_on_its_own_and_its_neighbours_show_through() {
        let r = reg();
        let stone = r.id("stone").unwrap();
        let mut c = Chunk::empty();
        // A half slab (top four removed) on a stone, with a stone beside it.
        let top = 0b1111_0000;
        c.set_state(LocalPos::new(5, 5, 5), cut(&r, "stone", bm_core::ShapeFamily::Square, top));
        c.set(LocalPos::new(5, 4, 5), stone);
        c.set(LocalPos::new(6, 5, 5), stone);
        let mesh = mesh_chunk(&c, &NO_NEIGHBORS, BlockId::AIR, &r.flags());
        assert!(mesh.translucent.is_empty() && mesh.shaped_translucent.is_empty());
        // The stones show all their faces except the one... none: the slab
        // is not opaque, so both draw the face towards it.
        assert_eq!(mesh.opaque.len(), 12 * 4, "two stones, six faces each");
        // The slab's own faces: no bottom (stone below), no +X (stone beside);
        // top (two triangles per octant quarter), -X, ±Z.
        let tris = mesh.shaped_opaque.chunks(3).count();
        let all = template(Shape::new(bm_core::ShapeFamily::Square, top).unwrap()).len();
        let culled = template(Shape::new(bm_core::ShapeFamily::Square, top).unwrap())
            .iter()
            .filter(|t| matches!(t.face, Some(f) if (f.axis, f.positive) == (1, false) || (f.axis, f.positive) == (0, true)))
            .count();
        assert_eq!(tris, all - culled);
        assert!(culled > 0);
        // Every vertex is within the slab.
        for v in &mesh.shaped_opaque {
            let [x, y, z] = v.position().map(|u| u as f32 / SHAPED_UNITS);
            assert!((5.0..=6.0).contains(&x) && (5.0..=5.5).contains(&y) && (5.0..=6.0).contains(&z));
            assert_eq!(v.block(), stone);
        }
    }

    #[test]
    fn cut_glass_drops_faces_against_whole_glass_but_not_against_cut_glass() {
        let r = reg();
        let glass = r.id("glass").unwrap();
        let shape = 0b1000_0000;
        let count = |nb: BlockState| {
            let mut c = Chunk::empty();
            c.set_state(LocalPos::new(5, 5, 5), cut(&r, "glass", bm_core::ShapeFamily::Bevel, shape));
            c.set_state(LocalPos::new(4, 5, 5), nb);
            let m = mesh_chunk(&c, &NO_NEIGHBORS, BlockId::AIR, &r.flags());
            assert!(m.shaped_opaque.is_empty());
            m.shaped_translucent.len()
        };
        let whole = count(BlockState::plain(glass));
        let cut_beside = count(cut(&r, "glass", bm_core::ShapeFamily::Bevel, 1));
        let alone = count(BlockState::AIR);
        assert!(whole < alone);
        assert_eq!(cut_beside, alone);
    }

    #[test]
    fn chunks_of_one_cut_state_are_not_skipped() {
        let r = reg();
        let s = cut(&r, "stone", bm_core::ShapeFamily::Slope, 1);
        let c = Chunk::filled_state(s);
        let nb: [Option<&Chunk>; 27] = [Some(&c); 27];
        let m = mesh_chunk(&c, &nb, BlockId::AIR, &r.flags());
        assert!(!m.shaped_opaque.is_empty());
        assert!(m.opaque.is_empty(), "no cube faces in a chunk of cut blocks");
    }

    #[test]
    fn shaped_vertices_pack_and_light_like_cubes() {
        let v = ShapedVertex::new(
            [31.5, 0.25, 17.0],
            [0.0, -0.7071, 0.7071],
            5,
            2,
            BlockId(0xbeef),
            Light::new(9, 4),
            Colour(77),
        );
        assert_eq!(v.position(), [31 * 256 + 128, 64, 17 * 256]);
        assert_eq!((v.tex_face(), v.ao(), v.block(), v.light(), v.colour()), (5, 2, BlockId(0xbeef), Light::new(9, 4), Colour(77)));
        let n = v.normal();
        assert!((n[1] + 0.7071).abs() < 0.01 && (n[2] - 0.7071).abs() < 0.01 && n[0] == 0.0);
        assert_eq!(std::mem::size_of::<ShapedVertex>(), 16);
        // A cut block in open sky is lit at every vertex, and open.
        let mut c = Chunk::empty();
        let r = reg();
        c.set_state(LocalPos::new(5, 5, 5), cut(&r, "stone", bm_core::ShapeFamily::Round, 0b1100_0000));
        let m = mesh_chunk(&c, &NO_NEIGHBORS, BlockId::AIR, &r.flags());
        assert!(m.shaped_opaque.iter().all(|v| v.light() == Light::OPEN_SKY && v.ao() == 3));
    }

    #[test]
    fn a_cut_beside_a_wall_is_shaded_by_it() {
        let r = reg();
        let stone = r.id("stone").unwrap();
        let mut c = Chunk::empty();
        c.set_state(LocalPos::new(5, 5, 5), cut(&r, "stone", bm_core::ShapeFamily::Square, 0b1111_0000));
        for z in 4..7 {
            for y in 5..8 {
                c.set(LocalPos::new(4, y, z), stone);
            }
        }
        let m = mesh_chunk(&c, &NO_NEIGHBORS, BlockId::AIR, &r.flags());
        // On the slab's top, corners against the wall darken.
        let top: Vec<_> = m.shaped_opaque.iter().filter(|v| v.tex_face() == 2).collect();
        assert!(top.iter().any(|v| v.ao() < 3) && top.iter().any(|v| v.ao() == 3));
        for v in top {
            assert_eq!(v.ao() < 3, v.position()[0] == 5 * 256, "{v:?}");
        }
    }

    #[test]
    fn indices_follow_the_quad_pattern() {
        assert_eq!(quad_indices(2), vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
    }
}
