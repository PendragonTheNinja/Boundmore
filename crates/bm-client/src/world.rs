//! What the client knows of the planet: the chunks the server has sent, the
//! edits it has predicted, and the queue of chunks waiting to be meshed.

use bm_core::{BlockFlags, BlockId, BlockPos, BlockRegistry, Chunk, ChunkPos, PhysicsWorld, PlanetShape};
use bm_mesh::{ChunkMesh, neighbor_index};
use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct ClientWorld {
    pub shape: PlanetShape,
    pub registry: BlockRegistry,
    pub flags: BlockFlags,
    water: BlockId,
    chunks: HashMap<ChunkPos, Arc<Chunk>>,
    /// Edits sent but not yet answered: position -> (edit seq, block before).
    pending: HashMap<BlockPos, (u32, BlockId)>,
    mesher: Mesher,
}

impl ClientWorld {
    pub fn new(shape: PlanetShape, registry: BlockRegistry) -> Self {
        let water = registry.id("water").unwrap_or(BlockId(u16::MAX));
        Self {
            shape,
            flags: registry.flags(),
            registry,
            water,
            chunks: HashMap::new(),
            pending: HashMap::new(),
            mesher: Mesher::new(),
        }
    }

    pub fn insert_chunk(&mut self, pos: ChunkPos, chunk: Chunk) {
        let pos = self.shape.wrap_chunk(pos);
        // Edits are only made in chunks we hold, and the server answers every
        // one on the same ordered stream, so a freshly arrived chunk is
        // already the truth: nothing predicted needs re-applying to it.
        self.chunks.insert(pos, Arc::new(chunk));
        self.dirty_around(pos);
    }

    pub fn forget_chunk(&mut self, pos: ChunkPos) {
        let pos = self.shape.wrap_chunk(pos);
        self.chunks.remove(&pos);
        self.mesher.forget(pos);
    }

    pub fn block(&self, p: BlockPos) -> Option<BlockId> {
        if !self.shape.contains_y(p.y) {
            return Some(BlockId::AIR);
        }
        let p = self.shape.wrap_block(p);
        self.chunks.get(&p.chunk()).map(|c| c.get(p.local()))
    }

    /// Writes a block the server told us about.
    pub fn set_block(&mut self, p: BlockPos, id: BlockId) {
        if !self.shape.contains_y(p.y) {
            return;
        }
        let p = self.shape.wrap_block(p);
        let Some(c) = self.chunks.get_mut(&p.chunk()) else {
            return;
        };
        if Arc::make_mut(c).set(p.local(), id) != id {
            self.dirty_block(p);
        }
    }

    /// Applies an edit locally before the server answers.
    pub fn predict_edit(&mut self, seq: u32, p: BlockPos, id: BlockId) {
        let p = self.shape.wrap_block(p);
        if let Some(before) = self.block(p) {
            self.pending.entry(p).or_insert((seq, before)).0 = seq;
            self.set_block(p, id);
        }
    }

    /// The server's verdict on our edit is the truth for that block.
    pub fn apply_edit_result(&mut self, seq: u32, p: BlockPos, block: BlockId) {
        let p = self.shape.wrap_block(p);
        if let Some((latest, _)) = self.pending.get(&p)
            && *latest == seq
        {
            self.pending.remove(&p);
        }
        // Later predictions at the same block are still in flight; they will
        // be answered too, so only settle when none are.
        if !self.pending.contains_key(&p) {
            self.set_block(p, block);
        }
    }

    fn dirty_block(&mut self, p: BlockPos) {
        let c = p.chunk();
        self.mesher.dirty.insert(c);
        let l = p.local();
        // Only neighbours that sample this block's border need a re-mesh.
        let near = |v: u8| -> &'static [i32] {
            match v {
                0 => &[-1, 0],
                31 => &[0, 1],
                _ => &[0],
            }
        };
        for &dx in near(l.x) {
            for &dy in near(l.y) {
                for &dz in near(l.z) {
                    let n = self.shape.wrap_chunk(c.offset(dx, dy, dz));
                    if self.chunks.contains_key(&n) {
                        self.mesher.dirty.insert(n);
                    }
                }
            }
        }
    }

    fn dirty_around(&mut self, pos: ChunkPos) {
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let n = self.shape.wrap_chunk(pos.offset(dx, dy, dz));
                    if self.chunks.contains_key(&n) {
                        self.mesher.dirty.insert(n);
                    }
                }
            }
        }
    }

    /// Starts mesh jobs for dirty chunks whose neighbourhood is complete,
    /// nearest to `center` first.
    pub fn schedule_meshing(&mut self, center: ChunkPos, max_in_flight: usize) {
        if self.mesher.dirty.is_empty() || self.mesher.in_flight >= max_in_flight {
            return;
        }
        let shape = self.shape;
        // Chunks still missing a neighbour leave the queue: the neighbour's
        // arrival puts them back (`dirty_around`), so they are not rescanned
        // every frame while they wait at the edge of view.
        let (ready, waiting): (Vec<ChunkPos>, Vec<ChunkPos>) =
            self.mesher.dirty.iter().partition(|p| self.neighbourhood_complete(**p));
        for p in waiting {
            self.mesher.dirty.remove(&p);
        }
        let mut ready: Vec<(i32, ChunkPos)> = ready
            .into_iter()
            .map(|p| {
                let (dx, dy, dz) = shape.chunk_delta(center, p);
                (dx * dx + dy * dy + dz * dz, p)
            })
            .collect();
        ready.sort_unstable_by_key(|(d, _)| *d);
        for (_, p) in ready {
            if self.mesher.in_flight >= max_in_flight {
                break;
            }
            self.mesher.dirty.remove(&p);
            let mut nb: [Option<Arc<Chunk>>; 27] = Default::default();
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let n = shape.wrap_chunk(p.offset(dx, dy, dz));
                        nb[neighbor_index(dx, dy, dz)] = self.chunks.get(&n).cloned();
                    }
                }
            }
            let version = self.mesher.next_version(p);
            let flags = self.flags.clone();
            let tx = self.mesher.done_tx.clone();
            self.mesher.in_flight += 1;
            rayon::spawn(move || {
                let center = nb[13].clone().expect("centre present");
                let refs: [Option<&Chunk>; 27] = std::array::from_fn(|i| nb[i].as_deref());
                let mesh = bm_mesh::mesh_chunk(&center, &refs, BlockId::AIR, &flags);
                let _ = tx.send(MeshResult { pos: p, version, mesh });
            });
        }
    }

    /// Finished meshes that are still current.
    pub fn take_meshes(&mut self) -> Vec<MeshResult> {
        let mut out = Vec::new();
        while let Ok(r) = self.mesher.done.try_recv() {
            self.mesher.in_flight -= 1;
            if self.mesher.versions.get(&r.pos) == Some(&r.version) && self.chunks.contains_key(&r.pos) {
                out.push(r);
            }
        }
        out
    }

    pub fn dirty_count(&self) -> usize {
        self.mesher.dirty.len()
    }

    fn neighbourhood_complete(&self, p: ChunkPos) -> bool {
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let n = p.offset(dx, dy, dz);
                    // Above the sky and below the floor there is nothing to wait for.
                    if !self.shape.contains_chunk_y(n.y) {
                        continue;
                    }
                    if !self.chunks.contains_key(&self.shape.wrap_chunk(n)) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

impl PhysicsWorld for ClientWorld {
    // Must match the server's rules exactly, or prediction disagrees.
    fn solid(&self, p: BlockPos) -> bool {
        if p.y < 0 {
            return true;
        }
        match self.block(p) {
            Some(b) => self.flags.solid(b),
            None => true,
        }
    }

    fn liquid(&self, p: BlockPos) -> bool {
        self.block(p) == Some(self.water)
    }
}

pub struct MeshResult {
    pub pos: ChunkPos,
    version: u64,
    pub mesh: ChunkMesh,
}

struct Mesher {
    dirty: HashSet<ChunkPos>,
    versions: HashMap<ChunkPos, u64>,
    counter: u64,
    in_flight: usize,
    done_tx: Sender<MeshResult>,
    done: Receiver<MeshResult>,
}

impl Mesher {
    fn new() -> Self {
        let (done_tx, done) = crossbeam_channel::unbounded();
        Self {
            dirty: HashSet::new(),
            versions: HashMap::new(),
            counter: 0,
            in_flight: 0,
            done_tx,
            done,
        }
    }

    fn next_version(&mut self, p: ChunkPos) -> u64 {
        self.counter += 1;
        self.versions.insert(p, self.counter);
        self.counter
    }

    fn forget(&mut self, p: ChunkPos) {
        self.dirty.remove(&p);
        self.versions.remove(&p);
    }
}
