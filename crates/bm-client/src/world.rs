//! What the client knows of the planet: the chunks the server has sent, the
//! edits it has predicted, and the queue of chunks waiting to be meshed.

use bm_core::{
    BlockFlags, BlockId, BlockPos, BlockRegistry, BlockState, Chunk, ChunkLight, ChunkPos, LightMap, PhysicsWorld,
    PlanetShape, Shape,
};
use bm_mesh::{ChunkMesh, neighbor_index};
use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Most broken blocks remembered between frames (a blast breaks many).
const MAX_BROKEN: usize = 64;

pub struct ClientWorld {
    pub shape: PlanetShape,
    pub registry: BlockRegistry,
    pub flags: BlockFlags,
    chunks: HashMap<ChunkPos, Arc<Chunk>>,
    /// Edits sent but not yet answered: position -> (edit seq, block before).
    pending: HashMap<BlockPos, (u32, BlockState)>,
    /// Sky and block light for the resident chunks (bm-core `light.rs`).
    light: LightMap,
    mesher: Mesher,
    /// Blocks that just broke (became air), with what they were: the frame
    /// takes them for break chips (`Effects::chips`).
    pub broken: Vec<(BlockPos, BlockState)>,
}

impl ClientWorld {
    /// `colours`: the palette painted glow crystals light in (milestone 26).
    pub fn new(shape: PlanetShape, registry: BlockRegistry, colours: &bm_core::ColourPalette) -> Self {
        Self {
            shape,
            flags: registry.flags(),
            chunks: HashMap::new(),
            pending: HashMap::new(),
            light: LightMap::new(shape, &registry.flags()).with_colours(colours),
            registry,
            mesher: Mesher::new(),
            broken: Vec::new(),
        }
    }

    pub fn insert_chunk(&mut self, pos: ChunkPos, mut chunk: Chunk) {
        let pos = self.shape.wrap_chunk(pos);
        // A shape the game cannot make reads as the full cube, as on the
        // server (ADR-0008).
        chunk.fix_invalid_shapes();
        // Edits are only made in chunks we hold, and the server answers every
        // one on the same ordered stream, so a freshly arrived chunk is
        // already the truth: nothing predicted needs re-applying to it.
        self.chunks.insert(pos, Arc::new(chunk));
        self.dirty_around(pos);
        // Light arrives when the chunk completes its column; the light map
        // says which chunks (here and next door) changed and need a re-mesh.
        let relit = self.light.chunk_loaded(pos, &self.chunks);
        self.mesher.dirty.extend(relit);
    }

    pub fn forget_chunk(&mut self, pos: ChunkPos) {
        let pos = self.shape.wrap_chunk(pos);
        self.chunks.remove(&pos);
        self.mesher.forget(pos);
        let relit = self.light.chunk_forgotten(pos, &self.chunks);
        self.mesher.dirty.extend(relit);
    }

    pub fn block(&self, p: BlockPos) -> Option<BlockId> {
        if !self.shape.contains_y(p.y) {
            return Some(BlockId::AIR);
        }
        let p = self.shape.wrap_block(p);
        self.chunks.get(&p.chunk()).map(|c| c.get(p.local()))
    }

    pub fn state(&self, p: BlockPos) -> Option<BlockState> {
        if !self.shape.contains_y(p.y) {
            return Some(BlockState::AIR);
        }
        let p = self.shape.wrap_block(p);
        self.chunks.get(&p.chunk()).map(|c| c.state(p.local()))
    }

    /// A block's cut, as the server answers it (invariant 3): a shape the
    /// game cannot make, or an unloaded block, reads as the full cube.
    pub fn block_shape(&self, p: BlockPos) -> Shape {
        match self.state(p) {
            Some(s) if s.shape.is_valid() => s.shape,
            _ => Shape::FULL,
        }
    }

    /// Writes a block the server told us about.
    pub fn set_block(&mut self, p: BlockPos, mut state: BlockState) {
        if !self.shape.contains_y(p.y) {
            return;
        }
        if !state.shape.is_valid() {
            state.shape = Shape::FULL;
        }
        let p = self.shape.wrap_block(p);
        let Some(c) = self.chunks.get_mut(&p.chunk()) else {
            return;
        };
        let old = Arc::make_mut(c).set_state(p.local(), state);
        if old != state {
            if !old.id.is_air() && state.id.is_air() && self.broken.len() < MAX_BROKEN {
                self.broken.push((p, old));
            }
            self.dirty_block(p);
            // A new colour needs a re-mesh; a new type, or a cut that makes
            // or unmakes a whole cube (cut blocks are not opaque), relights.
            if old.id != state.id || old.shape.is_full() != state.shape.is_full() {
                let relit = self.light.block_changed(p, &self.chunks);
                self.mesher.dirty.extend(relit);
            }
        }
    }

    /// Applies an edit locally before the server answers.
    pub fn predict_edit(&mut self, seq: u32, p: BlockPos, state: BlockState) {
        let p = self.shape.wrap_block(p);
        if let Some(before) = self.state(p) {
            self.pending.entry(p).or_insert((seq, before)).0 = seq;
            self.set_block(p, state);
        }
    }

    /// The server's verdict on our edit is the truth for that block.
    pub fn apply_edit_result(&mut self, seq: u32, p: BlockPos, block: BlockState) {
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

    /// Undoes our prediction `seq` at `p` when the server has told us
    /// otherwise without an `EditResult` for it (a swing it answered with
    /// `BlockHealth`: the block still stands). A later prediction at the
    /// block is left to its own answer.
    pub fn revert_prediction(&mut self, seq: u32, p: BlockPos) {
        let p = self.shape.wrap_block(p);
        if let Some(&(latest, before)) = self.pending.get(&p)
            && latest == seq
        {
            self.pending.remove(&p);
            self.set_block(p, before);
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
        // Light first, mesh once: a chunk meshed before its column is lit
        // would be drawn dark and meshed again moments later.
        let (ready, waiting): (Vec<ChunkPos>, Vec<ChunkPos>) = self
            .mesher
            .dirty
            .iter()
            .partition(|p| self.light.is_lit(**p) && self.neighbourhood_complete(**p));
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
            let lights = self.light.neighborhood(p);
            let version = self.mesher.next_version(p);
            let flags = self.flags.clone();
            let tx = self.mesher.done_tx.clone();
            self.mesher.in_flight += 1;
            rayon::spawn(move || {
                let center = nb[13].clone().expect("centre present");
                let refs: [Option<&Chunk>; 27] = std::array::from_fn(|i| nb[i].as_deref());
                let light_refs: [Option<&ChunkLight>; 27] = std::array::from_fn(|i| lights[i].as_ref());
                let mesh = bm_mesh::mesh_chunk_lit(&center, &refs, &light_refs, BlockId::AIR, &flags);
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

    /// Chunks held, and edits still waiting for the server's answer.
    #[cfg(test)]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    #[cfg(test)]
    pub fn pending_edits(&self) -> usize {
        self.pending.len()
    }

    /// The light at a point (the block holding it), for lighting what is
    /// not meshed (creatures, players, shots) as terrain is lit. A solid
    /// block holds no light of its own, so the point is lifted out of one
    /// (a body sunk a little into the ground) when the block above is open.
    pub fn light_at(&self, p: glam::DVec3) -> bm_core::Light {
        let b = BlockPos::containing(p.x, p.y, p.z);
        let l = self.light.light_at(b);
        if l.0 == 0 && self.block(b).is_some_and(|id| self.flags.opaque(id)) {
            return self.light.light_at(b.offset(0, 1, 0));
        }
        l
    }

    pub fn dirty_count(&self) -> usize {
        self.mesher.dirty.len()
    }

    /// Whether the chunks a player at `feet` could touch in the next moments
    /// are resident.
    pub fn has_chunks_around(&self, feet: glam::DVec3) -> bool {
        let c = BlockPos::containing(feet.x, feet.y, feet.z).chunk();
        (-1..=1).all(|dy| {
            (-1..=1).all(|dz| {
                (-1..=1).all(|dx| {
                    let n = c.offset(dx, dy, dz);
                    !self.shape.contains_chunk_y(n.y) || self.chunks.contains_key(&self.shape.wrap_chunk(n))
                })
            })
        })
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
        self.block(p).is_some_and(|b| self.flags.liquid(b))
    }

    fn burning(&self, p: BlockPos) -> bool {
        self.block(p).is_some_and(|b| self.flags.burns(b))
    }

    /// Soft ground softens a fall (milestone 13). Unloaded reads as hard.
    fn soft(&self, p: BlockPos) -> bool {
        self.block(p).is_some_and(|b| self.flags.soft(b))
    }

    fn shape(&self, p: BlockPos) -> Shape {
        self.block_shape(p)
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

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{LocalPos, ShapeFamily};

    #[test]
    fn the_physics_shape_is_the_servers() {
        let reg = BlockRegistry::builtin();
        let stone = reg.id("stone").unwrap();
        let mut w = ClientWorld::new(PlanetShape::new(4, 1), reg, &bm_core::ColourPalette::builtin());
        let cut = Shape::new(ShapeFamily::Slope, 0b1100_0000).unwrap();
        let mut c = Chunk::empty();
        c.set_state(
            LocalPos::new(1, 2, 3),
            BlockState {
                shape: cut,
                ..BlockState::plain(stone)
            },
        );
        c.set_state(
            LocalPos::new(4, 5, 6),
            BlockState {
                shape: Shape(0x0f01),
                ..BlockState::plain(stone)
            },
        );
        w.insert_chunk(ChunkPos::new(0, 0, 0), c);
        assert_eq!(w.shape(BlockPos::new(1, 2, 3)), cut);
        assert_eq!(w.shape(BlockPos::new(4, 5, 6)), Shape::FULL, "invalid reads as full");
        assert_eq!(w.shape(BlockPos::new(40, 5, 6)), Shape::FULL, "unloaded reads as full");
        // Across the seam, the same block.
        assert_eq!(w.shape(BlockPos::new(1 + 128, 2, 3)), cut);
        // A cut written by the server is kept; a bad one is not.
        w.set_block(
            BlockPos::new(7, 7, 7),
            BlockState {
                shape: Shape(0x0f01),
                ..BlockState::plain(stone)
            },
        );
        assert_eq!(w.state(BlockPos::new(7, 7, 7)).unwrap().shape, Shape::FULL);
    }

    #[test]
    fn a_break_the_server_did_not_make_is_undone() {
        let registry = BlockRegistry::builtin();
        let stone = registry.id("stone").unwrap();
        let mut w = ClientWorld::new(PlanetShape::new(4, 2), registry, &bm_core::ColourPalette::builtin());
        let mut c = Chunk::empty();
        c.set_state(LocalPos::new(1, 2, 3), BlockState::plain(stone));
        w.insert_chunk(ChunkPos::new(0, 0, 0), c);
        let p = BlockPos::new(1, 2, 3);
        w.predict_edit(7, p, BlockState::AIR);
        assert_eq!(w.block(p), Some(BlockId::AIR));
        // Another seq's answer does not undo it.
        w.revert_prediction(6, p);
        assert_eq!(w.block(p), Some(BlockId::AIR));
        w.revert_prediction(7, p);
        assert_eq!(w.block(p), Some(stone));
        assert_eq!(w.pending_edits(), 0);
    }
}
