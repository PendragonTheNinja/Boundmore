//! Chunk meshing.
//!
//! A chunk is meshed from a padded 34³ copy of its blocks (the chunk plus a
//! one-block border from its 26 neighbours), so faces on chunk borders are
//! culled correctly and ambient occlusion reads across seams. The mesher is
//! greedy: coplanar faces of the same block with the same occlusion merge into
//! one quad, which typically cuts terrain quads by 5–10×.
//!
//! Output is a list of [`Quad`]s, converted to packed 8-byte [`Vertex`]es for
//! the GPU. Quads are the unit tests compare: greedy output must cover exactly
//! the faces the naive mesher emits, with the same block and occlusion.

use bm_core::{BlockFlags, BlockId, CHUNK_SIZE, Chunk, LocalPos};

const N: usize = CHUNK_SIZE as usize;
const P: usize = N + 2;

/// A chunk plus a one-block border, indexed `x + 1`, `y + 1`, `z + 1`.
pub struct Padded {
    blocks: Box<[BlockId]>,
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

    /// Builds the padded volume. `neighbors[(dy+1)*9 + (dz+1)*3 + (dx+1)]` is
    /// the chunk at offset `(dx, dy, dz)` (index 13, the centre, is ignored).
    /// A missing neighbour reads as `missing`.
    pub fn new(center: &Chunk, neighbors: &[Option<&Chunk>; 27], missing: BlockId) -> Self {
        let mut blocks = vec![missing; P * P * P].into_boxed_slice();
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
                        blocks[Self::idx(x, y, z)] = c.get(l);
                    }
                }
            }
        }
        Self { blocks }
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
    /// Occlusion 0 (dark) to 3 (open) at corners (u0,v0), (u1,v0), (u1,v1), (u0,v1).
    pub ao: [u8; 4],
}

#[derive(Default, Debug)]
pub struct ChunkQuads {
    pub opaque: Vec<Quad>,
    pub translucent: Vec<Quad>,
}

/// `Some(translucent)` when the face of `me` towards `other` is visible.
#[inline]
fn face_visible(flags: &BlockFlags, me: BlockId, other: BlockId) -> Option<bool> {
    if flags.opaque(me) {
        (!flags.opaque(other)).then_some(false)
    } else if flags.translucent(me) {
        (!flags.opaque(other) && other != me).then_some(true)
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
        flags.opaque(p.get(q[0], q[1], q[2]))
    };
    let corner = |su: i32, sv: i32| vertex_ao(occ(su, 0), occ(0, sv), occ(su, sv));
    [corner(-1, -1), corner(1, -1), corner(1, 1), corner(-1, 1)]
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    block: BlockId,
    ao: [u8; 4],
    translucent: bool,
}

/// Visits every visible face on one layer, one per block side.
fn for_each_face(p: &Padded, flags: &BlockFlags, face: Face, layer: usize, mut f: impl FnMut(usize, usize, Cell)) {
    let (ua, va) = face.plane_axes();
    let d = if face.positive { 1 } else { -1 };
    for v in 0..N {
        for u in 0..N {
            let mut b = [0i32; 3];
            b[face.axis as usize] = layer as i32;
            b[ua] = u as i32;
            b[va] = v as i32;
            let me = p.get(b[0], b[1], b[2]);
            let mut o = b;
            o[face.axis as usize] += d;
            let other = p.get(o[0], o[1], o[2]);
            if let Some(translucent) = face_visible(flags, me, other) {
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
                        ao,
                        translucent,
                    },
                );
            }
        }
    }
}

/// Greedy mesher.
pub fn mesh_quads(p: &Padded, flags: &BlockFlags) -> ChunkQuads {
    let mut out = ChunkQuads::default();
    let mut mask: Vec<Option<Cell>> = vec![None; N * N];
    for face in Face::ALL {
        for layer in 0..N {
            mask.fill(None);
            let mut any = false;
            for_each_face(p, flags, face, layer, |u, v, c| {
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
                        ao: cell.ao,
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
pub fn mesh_quads_naive(p: &Padded, flags: &BlockFlags) -> ChunkQuads {
    let mut out = ChunkQuads::default();
    for face in Face::ALL {
        for layer in 0..N {
            for_each_face(p, flags, face, layer, |u, v, c| {
                let q = Quad {
                    face,
                    layer: layer as u8,
                    u: u as u8,
                    v: v as u8,
                    w: 1,
                    h: 1,
                    block: c.block,
                    ao: c.ao,
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
/// `b`: block id (16 bits); the upper 16 bits are reserved for light.
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
    fn new(pos: [u32; 3], face: Face, ao: u8, block: BlockId) -> Self {
        Self {
            a: pos[0] | pos[1] << 6 | pos[2] << 12 | face.index() << 18 | (ao as u32) << 21,
            b: block.0 as u32,
        }
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
            Vertex::new(corner(0, 0), q.face, q.ao[0], q.block),
            Vertex::new(corner(w, 0), q.face, q.ao[1], q.block),
            Vertex::new(corner(w, h), q.face, q.ao[2], q.block),
            Vertex::new(corner(0, h), q.face, q.ao[3], q.block),
        ];
        if !q.face.positive {
            v.swap(1, 3);
        }
        // Split along the diagonal with the brighter ends, so occlusion
        // gradients show no crease (the classic AO anisotropy fix).
        if (v[0].ao() + v[2].ao()) < (v[1].ao() + v[3].ao()) {
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

/// A chunk's finished mesh.
#[derive(Default, Debug)]
pub struct ChunkMesh {
    pub opaque: Vec<Vertex>,
    pub translucent: Vec<Vertex>,
}

impl ChunkMesh {
    pub fn is_empty(&self) -> bool {
        self.opaque.is_empty() && self.translucent.is_empty()
    }
}

/// Meshes a chunk. Uniform chunks that can show no faces skip the scan.
pub fn mesh_chunk(center: &Chunk, neighbors: &[Option<&Chunk>; 27], missing: BlockId, flags: &BlockFlags) -> ChunkMesh {
    if let Some(b) = center.uniform() {
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
                .and_then(|c| c.uniform())
                .is_some_and(|nb| flags.opaque(nb))
        });
        if flags.opaque(b) && sealed {
            return ChunkMesh::default();
        }
    }
    let padded = Padded::new(center, neighbors, missing);
    let quads = mesh_quads(&padded, flags);
    let mut mesh = ChunkMesh::default();
    quads_to_vertices(&quads.opaque, &mut mesh.opaque);
    quads_to_vertices(&quads.translucent, &mut mesh.translucent);
    mesh
}

/// An all-missing neighbourhood.
pub const NO_NEIGHBORS: [Option<&Chunk>; 27] = [None; 27];

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{BlockRegistry, CHUNK_VOLUME};
    use rand::{Rng, SeedableRng, rngs::StdRng};
    use std::collections::BTreeSet;

    fn reg() -> BlockRegistry {
        BlockRegistry::builtin()
    }

    type UnitFace = (Face, u8, u8, u8, BlockId, [u8; 4]);

    /// Expands quads to unit faces.
    fn raster(quads: &[Quad]) -> BTreeSet<UnitFace> {
        let mut s = BTreeSet::new();
        for q in quads {
            for dv in 0..q.h {
                for du in 0..q.w {
                    assert!(
                        s.insert((q.face, q.layer, q.u + du, q.v + dv, q.block, q.ao)),
                        "overlapping quads"
                    );
                }
            }
        }
        s
    }

    fn random_chunk(rng: &mut StdRng, ids: &[BlockId], fill: f64) -> Chunk {
        let dense: Vec<BlockId> = (0..CHUNK_VOLUME)
            .map(|_| {
                if rng.random_bool(fill) {
                    ids[rng.random_range(0..ids.len())]
                } else {
                    BlockId::AIR
                }
            })
            .collect();
        Chunk::from_dense(&dense)
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
            let (g, n) = (mesh_quads(&p, &flags), mesh_quads_naive(&p, &flags));
            assert_eq!(raster(&g.opaque), raster(&n.opaque));
            assert_eq!(raster(&g.translucent), raster(&n.translucent));
            assert!(g.opaque.len() <= n.opaque.len());
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
        let q = mesh_quads(&Padded::new(&c, &nb, BlockId::AIR), &r.flags());
        // Only the top shows; the sides face more ground.
        assert_eq!(q.opaque.len(), 1, "{:?}", q.opaque);
        assert_eq!((q.opaque[0].w, q.opaque[0].h), (32, 32));
    }

    #[test]
    fn single_block_has_six_open_faces() {
        let r = reg();
        let mut c = Chunk::empty();
        c.set(LocalPos::new(5, 5, 5), r.id("stone").unwrap());
        let q = mesh_quads(&Padded::new(&c, &NO_NEIGHBORS, BlockId::AIR), &r.flags());
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
        let q = mesh_quads(&Padded::new(&c, &NO_NEIGHBORS, BlockId::AIR), &r.flags());
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
        let q = mesh_quads(&Padded::new(&c, &NO_NEIGHBORS, BlockId::AIR), &r.flags());
        assert!(q.opaque.is_empty());
        assert_eq!(q.translucent.len(), 6, "six merged outer walls");
        let mut nb = NO_NEIGHBORS;
        for (i, slot) in nb.iter_mut().enumerate() {
            if i != neighbor_index(0, 1, 0) {
                *slot = Some(&c);
            }
        }
        let q = mesh_quads(&Padded::new(&c, &nb, BlockId::AIR), &r.flags());
        assert_eq!(q.translucent.len(), 1, "only the surface under the air");
    }

    #[test]
    fn vertices_wind_counter_clockwise_from_outside() {
        let r = reg();
        let mut c = Chunk::empty();
        c.set(LocalPos::new(3, 4, 5), r.id("stone").unwrap());
        c.set(LocalPos::new(4, 5, 5), r.id("stone").unwrap()); // forces some AO flips
        let q = mesh_quads(&Padded::new(&c, &NO_NEIGHBORS, BlockId::AIR), &r.flags());
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
    fn indices_follow_the_quad_pattern() {
        assert_eq!(quad_indices(2), vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
    }
}
