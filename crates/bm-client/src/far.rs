//! The planet past the view radius (milestone 20): the far copies the server
//! sends, meshed with the near terrain's mesher.
//!
//! A far chunk is meshed once its column and the eight columns around it are
//! all held (so its faces and its light are final, as "light first, mesh
//! once" asks of near chunks). Light is simple: full sky above a column's
//! highest opaque block, dimming a level a block below it to a floor, no
//! block light; day and night still come from the shader's uniform.

use bm_core::{BlockFlags, BlockState, CHUNK_SIZE, ChunkPos, FarChunk, Light, PlanetShape};
use bm_mesh::ChunkMesh;
use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const N: i32 = CHUNK_SIZE;
/// Sky light under a column's top never falls below this (far caves are not
/// drawn dark; overhangs only shade).
const FLOOR_SKY: u8 = 5;

/// Each block column's highest opaque block, as chunk-local y, or -1.
type Tops = [i8; (N * N) as usize];

struct Held {
    copy: Arc<FarChunk>,
    tops: Arc<Tops>,
}

pub struct FarMesh {
    pub pos: ChunkPos,
    version: u64,
    pub mesh: ChunkMesh,
}

pub struct FarWorld {
    shape: PlanetShape,
    flags: BlockFlags,
    held: HashMap<ChunkPos, Held>,
    /// Chunks held per column (x, z).
    columns: HashMap<(i32, i32), u32>,
    dirty: HashSet<ChunkPos>,
    versions: HashMap<ChunkPos, u64>,
    counter: u64,
    in_flight: usize,
    done_tx: Sender<FarMesh>,
    done: Receiver<FarMesh>,
}

impl FarWorld {
    pub fn new(shape: PlanetShape, flags: BlockFlags) -> Self {
        let (done_tx, done) = crossbeam_channel::unbounded();
        Self {
            shape,
            flags,
            held: HashMap::new(),
            columns: HashMap::new(),
            dirty: HashSet::new(),
            versions: HashMap::new(),
            counter: 0,
            in_flight: 0,
            done_tx,
            done,
        }
    }

    pub fn insert(&mut self, pos: ChunkPos, copy: FarChunk) {
        let pos = self.shape.wrap_chunk(pos);
        if pos.y < 0 || pos.y >= self.shape.height_chunks as i32 {
            return;
        }
        let tops = Arc::new(tops(&copy, &self.flags));
        let held = Held {
            copy: Arc::new(copy),
            tops,
        };
        if self.held.insert(pos, held).is_none() {
            *self.columns.entry((pos.x, pos.z)).or_default() += 1;
        }
        self.dirty_around(pos);
    }

    pub fn forget(&mut self, pos: ChunkPos) {
        let pos = self.shape.wrap_chunk(pos);
        if self.held.remove(&pos).is_some() {
            let c = self.columns.entry((pos.x, pos.z)).or_default();
            *c -= 1;
            if *c == 0 {
                self.columns.remove(&(pos.x, pos.z));
            }
            self.dirty.remove(&pos);
            self.versions.remove(&pos);
        }
    }

    /// Chunks held.
    pub fn len(&self) -> usize {
        self.held.len()
    }

    /// Every held chunk of the 3×3 columns around `pos` needs a new mesh:
    /// their faces (next door) or their light (the column's top) may change.
    fn dirty_around(&mut self, pos: ChunkPos) {
        for dz in -1..=1 {
            for dx in -1..=1 {
                let c = self.shape.wrap_chunk(ChunkPos::new(pos.x + dx, 0, pos.z + dz));
                for y in 0..self.shape.height_chunks as i32 {
                    let p = ChunkPos::new(c.x, y, c.z);
                    if self.held.contains_key(&p) {
                        self.dirty.insert(p);
                    }
                }
            }
        }
    }

    fn column_complete(&self, x: i32, z: i32) -> bool {
        self.columns.get(&(x, z)).copied().unwrap_or(0) >= self.shape.height_chunks
    }

    fn ready(&self, p: ChunkPos) -> bool {
        (-1..=1).all(|dz| {
            (-1..=1).all(|dx| {
                let c = self.shape.wrap_chunk(ChunkPos::new(p.x + dx, 0, p.z + dz));
                self.column_complete(c.x, c.z)
            })
        })
    }

    /// Starts mesh jobs for dirty chunks whose 3×3 columns are complete,
    /// nearest to `center` first.
    pub fn schedule_meshing(&mut self, center: ChunkPos, max_in_flight: usize) {
        if self.dirty.is_empty() || self.in_flight >= max_in_flight {
            return;
        }
        let shape = self.shape;
        let (ready, waiting): (Vec<ChunkPos>, Vec<ChunkPos>) = self.dirty.iter().partition(|p| self.ready(**p));
        // A waiting chunk is put back by the arrival that completes it.
        for p in waiting {
            self.dirty.remove(&p);
        }
        let mut ready: Vec<(i32, ChunkPos)> = ready
            .into_iter()
            .map(|p| {
                let (dx, dy, dz) = shape.chunk_delta(center, p);
                (dx * dx + dy * dy + dz * dz, p)
            })
            .collect();
        ready.sort_unstable_by_key(|(d, _)| *d);
        let h = shape.height_chunks as i32;
        for (_, p) in ready {
            if self.in_flight >= max_in_flight {
                break;
            }
            self.dirty.remove(&p);
            let mut near: [Option<Arc<FarChunk>>; 27] = Default::default();
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let n = shape.wrap_chunk(p.offset(dx, dy, dz));
                        near[bm_mesh::neighbor_index(dx, dy, dz)] = self.held.get(&n).map(|h| h.copy.clone());
                    }
                }
            }
            // The 3×3 columns' tops, top chunk first, for the light.
            let mut columns: Vec<Vec<Arc<Tops>>> = Vec::with_capacity(9);
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let c = shape.wrap_chunk(ChunkPos::new(p.x + dx, 0, p.z + dz));
                    columns.push(
                        (0..h)
                            .rev()
                            .filter_map(|y| self.held.get(&ChunkPos::new(c.x, y, c.z)).map(|h| h.tops.clone()))
                            .collect(),
                    );
                }
            }
            self.counter += 1;
            let version = self.counter;
            self.versions.insert(p, version);
            let flags = self.flags.clone();
            let tx = self.done_tx.clone();
            self.in_flight += 1;
            rayon::spawn(move || {
                let mesh = mesh(p, &near, &columns, h, &flags);
                let _ = tx.send(FarMesh { pos: p, version, mesh });
            });
        }
    }

    /// Finished meshes that are still current.
    pub fn take_meshes(&mut self) -> Vec<FarMesh> {
        let mut out = Vec::new();
        while let Ok(r) = self.done.try_recv() {
            self.in_flight -= 1;
            if self.versions.get(&r.pos) == Some(&r.version) && self.held.contains_key(&r.pos) {
                out.push(r);
            }
        }
        out
    }
}

/// Each block column's highest opaque block in a copy.
fn tops(copy: &FarChunk, flags: &BlockFlags) -> Tops {
    let mut t = [-1i8; (N * N) as usize];
    let level = copy.level();
    let across = copy.across();
    for cz in 0..across {
        for cx in 0..across {
            let top = (0..across)
                .rev()
                .find(|&cy| flags.opaque(copy.cell(cx, cy, cz).id))
                .map_or(-1, |cy| (((cy + 1) << level) - 1) as i8);
            for z in cz << level..(cz + 1) << level {
                for x in cx << level..(cx + 1) << level {
                    t[x | z << 5] = top;
                }
            }
        }
    }
    t
}

fn mesh(
    p: ChunkPos,
    near: &[Option<Arc<FarChunk>>; 27],
    columns: &[Vec<Arc<Tops>>],
    height_chunks: i32,
    flags: &BlockFlags,
) -> ChunkMesh {
    let centre = near[13].as_ref().expect("centre held");
    if centre.uniform().is_some_and(|s| s.id.is_air()) {
        return ChunkMesh::default();
    }
    // Each block column's top in world y, over the chunk and its border.
    let side = (N + 2) as usize;
    let mut top = vec![i32::MIN; side * side];
    for z in -1..=N {
        for x in -1..=N {
            let (dx, dz) = (x.div_euclid(N), z.div_euclid(N));
            let col = &columns[((dz + 1) * 3 + (dx + 1)) as usize];
            let i = (x.rem_euclid(N) | z.rem_euclid(N) << 5) as usize;
            // Top chunk first: the first with an opaque block in this column.
            let found = col.iter().enumerate().find_map(|(k, t)| {
                let ly = t[i];
                (ly >= 0).then(|| (height_chunks - 1 - k as i32) * N + ly as i32)
            });
            top[(x + 1) as usize + (z + 1) as usize * side] = found.unwrap_or(i32::MIN);
        }
    }
    let state = |x: i32, y: i32, z: i32| -> BlockState {
        let (dx, dy, dz) = (x.div_euclid(N), y.div_euclid(N), z.div_euclid(N));
        let (copy, lx, ly, lz) = match &near[bm_mesh::neighbor_index(dx, dy, dz)] {
            Some(c) => (c, x.rem_euclid(N), y.rem_euclid(N), z.rem_euclid(N)),
            // Off the planet's top or floor: the centre's edge goes on.
            None => (centre, x.clamp(0, N - 1), y.clamp(0, N - 1), z.clamp(0, N - 1)),
        };
        let l = copy.level();
        copy.cell((lx >> l) as usize, (ly >> l) as usize, (lz >> l) as usize)
    };
    let base = p.y * N;
    let light = |x: i32, y: i32, z: i32| -> Light {
        let t = top[(x + 1) as usize + (z + 1) as usize * side];
        let wy = base + y;
        let sky = if wy > t {
            15
        } else {
            15u8.saturating_sub((t - wy).min(15) as u8).max(FLOOR_SKY)
        };
        Light::new(sky, 0)
    };
    bm_mesh::mesh_far(state, light, flags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{BlockRegistry, Chunk, LocalPos};

    fn ground(r: &BlockRegistry, y: i32, height: u8) -> FarChunk {
        // Column chunks: full below the ground chunk, empty above.
        let stone = r.id("stone").unwrap();
        let c = if y < 1 {
            Chunk::filled(stone)
        } else if y == 1 {
            let mut c = Chunk::empty();
            for ly in 0..height {
                for z in 0..32u8 {
                    for x in 0..32u8 {
                        c.set(LocalPos::new(x, ly, z), stone);
                    }
                }
            }
            c
        } else {
            Chunk::empty()
        };
        FarChunk::from_chunk(&c, &r.flags())
    }

    #[test]
    fn a_far_chunk_meshes_once_its_columns_are_complete() {
        let r = BlockRegistry::builtin();
        let shape = PlanetShape::new(8, 3);
        let mut w = FarWorld::new(shape, r.flags());
        let centre = ChunkPos::new(4, 1, 4);
        // Only the centre column: not ready.
        for y in 0..3 {
            w.insert(ChunkPos::new(4, y, 4), ground(&r, y, 10));
        }
        w.schedule_meshing(centre, 64);
        assert_eq!(w.in_flight, 0);
        for z in 3..=5 {
            for x in 3..=5 {
                if (x, z) != (4, 4) {
                    for y in 0..3 {
                        w.insert(ChunkPos::new(x, y, z), ground(&r, y, 10));
                    }
                }
            }
        }
        w.schedule_meshing(centre, 64);
        let mut meshes = Vec::new();
        for _ in 0..200 {
            meshes.extend(w.take_meshes());
            if w.in_flight == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let m = meshes.iter().find(|m| m.pos == centre).expect("the centre meshed");
        // A flat floor at y 10: top faces only, in full sky.
        assert!(!m.mesh.opaque.is_empty());
        assert!(m.mesh.opaque.iter().all(|v| v.position()[1] == 10 && v.sky() == 15));
        // The buried chunk under it shows nothing.
        let under = meshes.iter().find(|m| m.pos == ChunkPos::new(4, 0, 4)).expect("meshed");
        assert!(under.mesh.is_empty());
    }

    #[test]
    fn tops_follow_the_cells() {
        let r = BlockRegistry::builtin();
        let t = tops(&ground(&r, 1, 10), &r.flags());
        assert_eq!(t[0], 9, "level 1 cells: ten blocks fill five cells");
        let t = tops(&ground(&r, 2, 10), &r.flags());
        assert_eq!(t[0], -1);
    }
}
