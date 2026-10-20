//! Props: blocks drawn as a small model instead of a cube (a torch).
//!
//! A prop is any block with a `mount` (`BlockDef::is_prop`). It is never
//! opaque, so it hides no neighbouring face and casts no occlusion; its own
//! triangles go into the chunk's shaped opaque list ([`ShapedVertex`]) with
//! explicit texture coordinates, because a tilted stick cannot take its
//! texture from its position the way a cube face does.
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
//! Light: the stick takes the cell's own light (a torch emits, so its cell is
//! lit); the flame is drawn at full block light, since it is the light.

use crate::{Padded, PaddedLight, ShapedVertex};
use bm_core::{BlockFlags, CHUNK_SIZE, Light, Mount};

const N: i32 = CHUNK_SIZE as i32;

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

/// The faces of a box as quads: four corners counter-clockwise seen from
/// outside (bottom left, bottom right, top right, top left as the texture
/// reads), the outward normal, the texture rectangle and the face index.
fn box_quads(p: &Part) -> Vec<([[f32; 3]; 4], [f32; 3], Rect, u8)> {
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

/// The triangles of every prop in the chunk.
pub fn mesh_props(p: &Padded, light: &PaddedLight, flags: &BlockFlags, out: &mut Vec<ShapedVertex>) {
    for y in 0..N {
        for z in 0..N {
            for x in 0..N {
                let me = p.get(x, y, z);
                let Some(mount) = flags.mount(me) else {
                    continue;
                };
                let own = light.get(x, y, z);
                let colour = p.colour(x, y, z);
                let base = [x as f32, y as f32, z as f32];
                for t in prop_model(mount) {
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
        for (v, t) in mesh.shaped_opaque.iter().zip(model.iter().flat_map(|t| [t, t, t])) {
            let want = if t.flame { 15 } else { 10 };
            assert_eq!(v.light(), Light::new(0, want), "{t:?}");
        }
        // The floor beside the torch is lit by it.
        assert!(mesh.opaque.iter().any(|v| v.face() == 2 && v.block_light() >= 9));
    }
}
