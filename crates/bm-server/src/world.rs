//! The server's resident world: loaded chunks, their dirty state, and the
//! background loader that fills them.

use crate::store::Store;
use bm_core::{BlockFlags, BlockId, BlockPos, BlockRegistry, Chunk, ChunkPos, PhysicsWorld, PlanetShape};
use bm_protocol::ServerMsg;
use bm_worldgen::Generator;
use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
        self.frame
            .get_or_insert_with(|| {
                Arc::new(bm_protocol::encode_frame(&ServerMsg::Chunk {
                    pos,
                    data: chunk.encode(),
                }))
            })
            .clone()
    }
}

pub struct World {
    pub shape: PlanetShape,
    pub registry: Arc<BlockRegistry>,
    pub flags: BlockFlags,
    pub water: BlockId,
    chunks: HashMap<ChunkPos, Slot>,
    loading: HashSet<ChunkPos>,
    loader: Loader,
}

impl World {
    pub fn new(shape: PlanetShape, registry: Arc<BlockRegistry>, generator: Arc<Generator>, store: Store) -> Self {
        let water = registry.id("water").expect("catalogue has water");
        Self {
            shape,
            flags: registry.flags(),
            registry,
            water,
            chunks: HashMap::new(),
            loading: HashSet::new(),
            loader: Loader::new(generator, store),
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
    pub fn request(&mut self, p: ChunkPos) {
        let p = self.shape.wrap_chunk(p);
        if self.shape.contains_chunk_y(p.y) && !self.chunks.contains_key(&p) && self.loading.insert(p) {
            self.loader.request(p);
        }
    }

    /// Moves finished loads in. Returns how many arrived.
    pub fn receive_loaded(&mut self, tick: u64) -> usize {
        let mut n = 0;
        while let Ok((p, chunk)) = self.loader.done.try_recv() {
            self.loading.remove(&p);
            self.chunks.insert(
                p,
                Slot {
                    chunk,
                    dirty: false,
                    frame: None,
                    wanted_at: tick,
                },
            );
            n += 1;
        }
        n
    }

    pub fn block(&self, p: BlockPos) -> Option<BlockId> {
        if !self.shape.contains_y(p.y) {
            return Some(BlockId::AIR);
        }
        let p = self.shape.wrap_block(p);
        self.chunks.get(&p.chunk()).map(|s| s.chunk.get(p.local()))
    }

    /// Sets a block in a resident chunk. Returns the old block, or `None` if
    /// the chunk is not resident (the caller must not have allowed the edit).
    pub fn set_block(&mut self, p: BlockPos, id: BlockId) -> Option<BlockId> {
        if !self.shape.contains_y(p.y) {
            return None;
        }
        let p = self.shape.wrap_block(p);
        let slot = self.chunks.get_mut(&p.chunk())?;
        let old = slot.chunk.set(p.local(), id);
        if old != id {
            slot.dirty = true;
            slot.frame = None;
        }
        Some(old)
    }

    /// Takes every dirty chunk's encoded bytes for saving, clearing the flags.
    pub fn take_dirty(&mut self) -> Vec<(ChunkPos, Vec<u8>)> {
        let mut out = Vec::new();
        for (p, slot) in &mut self.chunks {
            if slot.dirty {
                slot.dirty = false;
                out.push((*p, slot.chunk.encode()));
            }
        }
        out
    }

    /// Evicts chunks nobody has wanted since `before`. Dirty chunks are
    /// returned for saving rather than dropped.
    pub fn evict_unwanted(&mut self, before: u64) -> Vec<(ChunkPos, Vec<u8>)> {
        let stale: Vec<ChunkPos> = self
            .chunks
            .iter()
            .filter(|(_, s)| s.wanted_at < before)
            .map(|(p, _)| *p)
            .collect();
        let mut save = Vec::new();
        for p in stale {
            if let Some(s) = self.chunks.remove(&p)
                && s.dirty
            {
                save.push((p, s.chunk.encode()));
            }
        }
        save
    }

    pub fn resident(&self) -> usize {
        self.chunks.len()
    }

    pub fn pending_loads(&self) -> usize {
        self.loading.len()
    }
}

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
}

/// Loads chunks on the rayon pool: the save first, the generator otherwise.
struct Loader {
    generator: Arc<Generator>,
    store: Store,
    done_tx: Sender<(ChunkPos, Chunk)>,
    done: Receiver<(ChunkPos, Chunk)>,
}

impl Loader {
    fn new(generator: Arc<Generator>, store: Store) -> Self {
        let (done_tx, done) = crossbeam_channel::unbounded();
        Self {
            generator,
            store,
            done_tx,
            done,
        }
    }

    fn request(&self, p: ChunkPos) {
        let (g, store, tx) = (self.generator.clone(), self.store.clone(), self.done_tx.clone());
        rayon::spawn(move || {
            // A read error is retried, never answered with fresh terrain: a
            // regenerated chunk that is later edited would be saved over the
            // player's real build.
            let mut attempt = 0u64;
            let saved = loop {
                match store.chunk(p) {
                    Ok(v) => break v,
                    Err(e) => {
                        attempt += 1;
                        tracing::error!("reading chunk {p:?} failed (attempt {attempt}): {e:#}");
                        std::thread::sleep(std::time::Duration::from_millis(100 * attempt.min(50)));
                    }
                }
            };
            let chunk = match saved {
                Some(bytes) => match Chunk::decode(&bytes) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!("saved chunk {p:?} is corrupt ({e}); serving fresh terrain");
                        g.generate(p)
                    }
                },
                None => g.generate(p),
            };
            let _ = tx.send((p, chunk));
        });
    }
}
