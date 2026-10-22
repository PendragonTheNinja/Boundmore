//! One planet's resident world: loaded chunks, their dirty state, and the
//! background loader that fills them.

use crate::store::Store;
use bm_core::{
    BlockFlags, BlockId, BlockPos, BlockRegistry, BlockState, Chunk, ChunkPos, PhysicsWorld, PlanetShape, Shape,
};
use bm_protocol::ServerMsg;
use bm_worldgen::Generator;
use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Slot {
    pub chunk: Chunk,
    /// Changed since the last save.
    pub dirty: bool,
    /// The chunk's network frame, cached because every player in range is
    /// sent the same bytes. Cleared by any edit.
    frame: Option<Arc<Vec<u8>>>,
    /// Last tick any player wanted this chunk.
    pub wanted_at: u64,
}

impl Slot {
    pub fn frame(&mut self, pos: ChunkPos) -> Arc<Vec<u8>> {
        let chunk = &self.chunk;
        self.frame.get_or_insert_with(|| chunk_frame(pos, chunk)).clone()
    }
}

pub struct World {
    /// The planet's id (what its saved chunks are keyed by).
    pub planet: u16,
    pub shape: PlanetShape,
    pub registry: Arc<BlockRegistry>,
    pub flags: BlockFlags,
    pub water: BlockId,
    chunks: HashMap<ChunkPos, Slot>,
    loading: HashSet<ChunkPos>,
    loader: Loader,
    /// Chunks whose latest save is not durable yet, with the save that
    /// carries them.
    unconfirmed: HashMap<ChunkPos, u64>,
    saves: Vec<(Receiver<()>, u64, Vec<ChunkPos>)>,
    next_save: u64,
    /// Evicted chunks among `unconfirmed`. A request for one is answered from
    /// here, never from the disk, which may still hold the copy from before
    /// the edit: a quick return (a player warping back) would otherwise load
    /// it, and the next save would put it over the build.
    saving: HashMap<ChunkPos, Chunk>,
    /// Chunks kept resident whoever is near, with how many holds each has
    /// (a lapsing claim works on them; nobody need be on the planet).
    pinned: HashMap<ChunkPos, u32>,
    /// The tick of the last `receive_loaded`.
    last_tick: u64,
    /// Chunks that came into memory since `take_arrived` (their crops catch
    /// up, milestone 16).
    arrived: Vec<ChunkPos>,
}

impl World {
    pub fn new(
        planet: u16,
        shape: PlanetShape,
        registry: Arc<BlockRegistry>,
        generator: Arc<Generator>,
        store: Store,
    ) -> Self {
        let water = registry.id("water").expect("catalogue has water");
        Self {
            planet,
            shape,
            flags: registry.flags(),
            registry,
            water,
            chunks: HashMap::new(),
            loading: HashSet::new(),
            loader: Loader::new(planet, generator, store),
            unconfirmed: HashMap::new(),
            saves: Vec::new(),
            next_save: 0,
            saving: HashMap::new(),
            pinned: HashMap::new(),
            last_tick: 0,
            arrived: Vec::new(),
        }
    }

    /// The planet's generator: what its terrain was before anyone changed it.
    pub fn generator(&self) -> &Arc<Generator> {
        &self.loader.generator
    }

    /// Keeps a chunk resident once loaded (it must still be requested),
    /// even with nobody on the planet, until as many `unpin`s.
    pub fn pin(&mut self, p: ChunkPos) {
        *self.pinned.entry(self.shape.wrap_chunk(p)).or_default() += 1;
    }

    pub fn unpin(&mut self, p: ChunkPos) {
        let p = self.shape.wrap_chunk(p);
        if let Some(n) = self.pinned.get_mut(&p) {
            *n -= 1;
            if *n == 0 {
                self.pinned.remove(&p);
                // Wanted until now: evicted with the rest of the unwanted.
                if let Some(slot) = self.chunks.get_mut(&p) {
                    slot.wanted_at = slot.wanted_at.max(self.last_tick);
                }
            }
        }
    }

    pub fn get(&self, p: ChunkPos) -> Option<&Slot> {
        self.chunks.get(&self.shape.wrap_chunk(p))
    }

    pub fn get_mut(&mut self, p: ChunkPos) -> Option<&mut Slot> {
        self.chunks.get_mut(&self.shape.wrap_chunk(p))
    }

    pub fn is_loaded(&self, p: ChunkPos) -> bool {
        self.chunks.contains_key(&self.shape.wrap_chunk(p))
    }

    /// Asks for a chunk to be loaded (from the save or the generator). Does
    /// nothing if it is resident or already on its way.
    pub fn request(&mut self, p: ChunkPos, tick: u64) {
        let p = self.shape.wrap_chunk(p);
        if !self.shape.contains_chunk_y(p.y) || self.chunks.contains_key(&p) || self.loading.contains(&p) {
            return;
        }
        if let Some(chunk) = self.saving.remove(&p) {
            // Still on its way to the disk: the copy here is the true one,
            // and the save in flight writes these same bytes.
            self.chunks.insert(
                p,
                Slot {
                    chunk,
                    dirty: false,
                    frame: None,
                    wanted_at: tick,
                },
            );
            self.arrived.push(p);
            return;
        }
        self.loading.insert(p);
        self.loader.request(p);
    }

    /// Moves finished loads in. Returns how many arrived. With `keep` false
    /// (nobody is on the planet) they are dropped instead: nothing was
    /// changed in them, and an empty planet holds no chunks.
    /// Pinned chunks are always kept.
    pub fn receive_loaded(&mut self, tick: u64, keep: bool) -> usize {
        self.last_tick = tick;
        let mut n = 0;
        while let Ok((p, chunk, frame)) = self.loader.done.try_recv() {
            self.loading.remove(&p);
            if !keep && !self.pinned.contains_key(&p) {
                continue;
            }
            self.chunks.insert(
                p,
                Slot {
                    chunk,
                    dirty: false,
                    frame: Some(frame),
                    wanted_at: tick,
                },
            );
            self.arrived.push(p);
            n += 1;
        }
        n
    }

    /// Chunks that came into memory since the last call.
    pub fn take_arrived(&mut self) -> Vec<ChunkPos> {
        std::mem::take(&mut self.arrived)
    }

    pub fn block(&self, p: BlockPos) -> Option<BlockId> {
        if !self.shape.contains_y(p.y) {
            return Some(BlockId::AIR);
        }
        let p = self.shape.wrap_block(p);
        self.chunks.get(&p.chunk()).map(|s| s.chunk.get(p.local()))
    }

    /// The full state (type, colour, shape) of a resident block.
    pub fn state(&self, p: BlockPos) -> Option<BlockState> {
        if !self.shape.contains_y(p.y) {
            return Some(BlockState::AIR);
        }
        let p = self.shape.wrap_block(p);
        self.chunks.get(&p.chunk()).map(|s| s.chunk.state(p.local()))
    }

    /// Sets a plain block (natural colour, full shape) in a resident chunk.
    /// Returns the old block, or `None` if the chunk is not resident (the
    /// caller must not have allowed the edit).
    pub fn set_block(&mut self, p: BlockPos, id: BlockId) -> Option<BlockId> {
        self.set_state(p, BlockState::plain(id)).map(|s| s.id)
    }

    /// Sets a block state in a resident chunk; see [`World::set_block`].
    pub fn set_state(&mut self, p: BlockPos, state: BlockState) -> Option<BlockState> {
        if !self.shape.contains_y(p.y) {
            return None;
        }
        let p = self.shape.wrap_block(p);
        let slot = self.chunks.get_mut(&p.chunk())?;
        let old = slot.chunk.set_state(p.local(), state);
        if old != state {
            slot.dirty = true;
            slot.frame = None;
        }
        Some(old)
    }

    /// Takes every dirty chunk's encoded bytes for saving, clearing the
    /// flags. `saved` fires (or disconnects) once the batch is durable.
    pub fn take_dirty(&mut self, saved: &Receiver<()>) -> Vec<(ChunkPos, Vec<u8>)> {
        let mut out = Vec::new();
        for (p, slot) in &mut self.chunks {
            if slot.dirty {
                slot.dirty = false;
                out.push((*p, slot.chunk.encode()));
            }
        }
        self.track(saved, &out);
        out
    }

    fn track(&mut self, saved: &Receiver<()>, chunks: &[(ChunkPos, Vec<u8>)]) {
        if chunks.is_empty() {
            return;
        }
        self.next_save += 1;
        for (p, _) in chunks {
            self.unconfirmed.insert(*p, self.next_save);
        }
        self.saves
            .push((saved.clone(), self.next_save, chunks.iter().map(|(p, _)| *p).collect()));
    }

    /// Evicts chunks nobody has wanted since `before` (`u64::MAX`: all but
    /// the pinned).
    /// Dirty chunks are returned for saving, in the batch `saved` belongs to;
    /// any whose save is not durable yet is held until it is.
    pub fn evict_unwanted(&mut self, before: u64, saved: &Receiver<()>) -> Vec<(ChunkPos, Vec<u8>)> {
        self.forget_saved();
        let stale: Vec<ChunkPos> = self
            .chunks
            .iter()
            .filter(|(p, s)| s.wanted_at < before && !self.pinned.contains_key(p))
            .map(|(p, _)| *p)
            .collect();
        let mut save = Vec::new();
        let mut held = Vec::new();
        for p in stale {
            let Some(s) = self.chunks.remove(&p) else { continue };
            if s.dirty {
                save.push((p, s.chunk.encode()));
            }
            if s.dirty || self.unconfirmed.contains_key(&p) {
                held.push((p, s.chunk));
            }
        }
        self.track(saved, &save);
        self.saving.extend(held);
        save
    }

    /// Forgets saves that have finished, and drops the evicted chunks they
    /// were the last save of.
    pub fn forget_saved(&mut self) {
        let (unconfirmed, saving) = (&mut self.unconfirmed, &mut self.saving);
        self.saves.retain(|(rx, n, chunks)| {
            if matches!(rx.try_recv(), Err(crossbeam_channel::TryRecvError::Empty)) {
                return true;
            }
            for p in chunks {
                if unconfirmed.get(p) == Some(n) {
                    unconfirmed.remove(p);
                    saving.remove(p);
                }
            }
            false
        });
    }

    /// Evicted chunks still waiting for their save.
    pub fn held_for_saving(&self) -> usize {
        self.saving.len()
    }

    pub fn resident(&self) -> usize {
        self.chunks.len()
    }

    pub fn pending_loads(&self) -> usize {
        self.loading.len()
    }
}

/// Whether an invalid saved shape has been logged (once per run is enough to
/// notice; the block reads as full either way).
static BAD_SHAPE_LOGGED: AtomicBool = AtomicBool::new(false);

impl PhysicsWorld for World {
    /// Unloaded chunks are solid: a player can never fall into terrain the
    /// server has not loaded yet (the client agrees, so prediction matches).
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

    /// Soft ground softens a fall (milestone 13). Unloaded reads as hard.
    fn soft(&self, p: BlockPos) -> bool {
        self.block(p).is_some_and(|b| self.flags.soft(b))
    }

    /// Chiselled blocks collide on their kept octants (ADR-0008). Unloaded
    /// reads as full, matching `solid`.
    fn shape(&self, p: BlockPos) -> Shape {
        self.state(p).map_or(Shape::FULL, |s| s.shape)
    }
}

/// The network frame of a chunk (the same bytes for every player).
fn chunk_frame(pos: ChunkPos, chunk: &Chunk) -> Arc<Vec<u8>> {
    Arc::new(bm_protocol::encode_frame(&ServerMsg::Chunk {
        pos,
        data: chunk.encode(),
    }))
}

/// A loaded chunk with its frame, encoded on the loader thread so the game
/// thread only copies it out.
type Loaded = (ChunkPos, Chunk, Arc<Vec<u8>>);

/// Loads chunks on the rayon pool: the save first, the generator otherwise.
struct Loader {
    planet: u16,
    generator: Arc<Generator>,
    store: Store,
    done_tx: Sender<Loaded>,
    done: Receiver<Loaded>,
}

impl Loader {
    fn new(planet: u16, generator: Arc<Generator>, store: Store) -> Self {
        let (done_tx, done) = crossbeam_channel::unbounded();
        Self {
            planet,
            generator,
            store,
            done_tx,
            done,
        }
    }

    fn request(&self, p: ChunkPos) {
        let (planet, g, store, tx) = (
            self.planet,
            self.generator.clone(),
            self.store.clone(),
            self.done_tx.clone(),
        );
        rayon::spawn(move || {
            // A read error is retried, never answered with fresh terrain: a
            // regenerated chunk that is later edited would be saved over the
            // player's real build.
            let mut attempt = 0u64;
            let saved = loop {
                match store.chunk(planet, p) {
                    Ok(v) => break v,
                    Err(e) => {
                        attempt += 1;
                        tracing::error!("reading chunk {p:?} of planet {planet} failed (attempt {attempt}): {e:#}");
                        std::thread::sleep(std::time::Duration::from_millis(100 * attempt.min(50)));
                    }
                }
            };
            let chunk = match saved {
                Some(bytes) => match Chunk::decode(&bytes) {
                    Ok(mut c) => {
                        let bad = c.fix_invalid_shapes();
                        if bad > 0 && !BAD_SHAPE_LOGGED.swap(true, Ordering::Relaxed) {
                            tracing::warn!(
                                "saved chunk {p:?} of planet {planet} has {bad} invalid shapes; read as full blocks \
                                 (logged once)"
                            );
                        }
                        c
                    }
                    Err(e) => {
                        tracing::error!("saved chunk {p:?} is corrupt ({e}); serving fresh terrain");
                        g.generate(p)
                    }
                },
                None => g.generate(p),
            };
            let frame = chunk_frame(p, &chunk);
            let _ = tx.send((p, chunk, frame));
        });
    }
}
