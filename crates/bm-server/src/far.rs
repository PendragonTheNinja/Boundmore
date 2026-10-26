//! Far copies of one planet's chunks (milestone 20): made on demand for the
//! players on it, on the loader pool, saved so a later visit reads them.
//!
//! A copy is made from the chunk in memory when it is resident and saved,
//! else from the saved chunk, else from freshly generated terrain (dropped
//! after, never saved as a chunk). Saving a chunk deletes its far copy in the
//! same write (`Store::write`), and the game asks for a new one from the saved
//! bytes ([`FarStore::rebuild`]), so a copy on disk is never older than its
//! chunk. A rebuild bumps the chunk's version; any job of an older version
//! still running is dropped when it returns.

use crate::store::Store;
use bm_core::{BlockFlags, Chunk, ChunkPos, FarChunk};
use bm_protocol::ServerMsg;
use bm_worldgen::Generator;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::Arc;

/// A chunk's far copy as network frames, one per level (index 0 is level 1).
#[derive(Clone)]
pub struct Copy {
    pub frames: [Arc<Vec<u8>>; 2],
}

impl Copy {
    pub fn frame(&self, level: u8) -> &Arc<Vec<u8>> {
        &self.frames[(level.clamp(1, 2) - 1) as usize]
    }
}

/// Where a job takes the chunk from.
enum Source {
    /// The far table, else the saved chunk, else the generator.
    Disk,
    /// A resident chunk.
    Chunk(Chunk),
    /// A chunk just saved, as its saved bytes.
    Saved(Vec<u8>),
}

struct Built {
    pos: ChunkPos,
    version: u32,
    /// `None` when the job could not read its chunk (logged; asked again).
    copy: Option<Copy>,
    /// Level 1 bytes to save, when the copy is new.
    save: Option<Vec<u8>>,
}

pub struct FarStore {
    planet: u16,
    flags: BlockFlags,
    generator: Arc<Generator>,
    store: Store,
    copies: HashMap<ChunkPos, Copy>,
    /// Jobs running per chunk.
    in_flight: HashMap<ChunkPos, u32>,
    running: usize,
    /// Bumped by every rebuild; a job of an older version is dropped.
    versions: HashMap<ChunkPos, u32>,
    done_tx: Sender<Built>,
    done: Receiver<Built>,
    unsaved: Vec<(ChunkPos, Vec<u8>)>,
    /// Copies replaced since `take_changed` (players holding them are sent
    /// the new one).
    changed: Vec<ChunkPos>,
    /// The tick the planet was last seen with players on it.
    last_used: u64,
}

impl FarStore {
    pub fn new(planet: u16, flags: BlockFlags, generator: Arc<Generator>, store: Store) -> Self {
        let (done_tx, done) = crossbeam_channel::unbounded();
        Self {
            planet,
            flags,
            generator,
            store,
            copies: HashMap::new(),
            in_flight: HashMap::new(),
            running: 0,
            versions: HashMap::new(),
            done_tx,
            done,
            unsaved: Vec::new(),
            changed: Vec::new(),
            last_used: 0,
        }
    }

    pub fn get(&self, pos: ChunkPos) -> Option<&Copy> {
        self.copies.get(&pos)
    }

    /// Jobs running now.
    pub fn running(&self) -> usize {
        self.running
    }

    pub fn is_building(&self, pos: ChunkPos) -> bool {
        self.in_flight.contains_key(&pos)
    }

    /// Asks for a copy not held yet: from `resident` (the chunk in memory,
    /// when it is there and saved) or from the disk.
    pub fn request(&mut self, pos: ChunkPos, resident: Option<Chunk>) {
        if self.copies.contains_key(&pos) || self.in_flight.contains_key(&pos) {
            return;
        }
        let src = resident.map_or(Source::Disk, Source::Chunk);
        self.spawn(pos, src);
    }

    /// A chunk was saved as `bytes`: its copy is made again from them.
    pub fn rebuild(&mut self, pos: ChunkPos, bytes: Vec<u8>) {
        *self.versions.entry(pos).or_default() += 1;
        self.spawn(pos, Source::Saved(bytes));
    }

    fn spawn(&mut self, pos: ChunkPos, src: Source) {
        let version = self.versions.get(&pos).copied().unwrap_or(0);
        *self.in_flight.entry(pos).or_default() += 1;
        self.running += 1;
        let (planet, flags, g, store, tx) = (
            self.planet,
            self.flags.clone(),
            self.generator.clone(),
            self.store.clone(),
            self.done_tx.clone(),
        );
        rayon::spawn(move || {
            let built = build(planet, pos, src, &flags, &g, &store);
            let (copy, save) = match built {
                Some((copy, save)) => (Some(copy), save),
                None => (None, None),
            };
            let _ = tx.send(Built {
                pos,
                version,
                copy,
                save,
            });
        });
    }

    /// Takes finished jobs in. Returns how many copies arrived.
    pub fn receive(&mut self) -> usize {
        let mut n = 0;
        while let Ok(b) = self.done.try_recv() {
            self.running -= 1;
            if let Some(k) = self.in_flight.get_mut(&b.pos) {
                *k -= 1;
                if *k == 0 {
                    self.in_flight.remove(&b.pos);
                }
            }
            if b.version != self.versions.get(&b.pos).copied().unwrap_or(0) {
                continue;
            }
            let Some(copy) = b.copy else { continue };
            if let Some(bytes) = b.save {
                self.unsaved.push((b.pos, bytes));
            }
            if self.copies.insert(b.pos, copy).is_some() {
                self.changed.push(b.pos);
            }
            n += 1;
        }
        n
    }

    /// New copies to save (with the planet's next batch).
    pub fn take_unsaved(&mut self) -> Vec<(ChunkPos, Vec<u8>)> {
        std::mem::take(&mut self.unsaved)
    }

    pub fn take_changed(&mut self) -> Vec<ChunkPos> {
        std::mem::take(&mut self.changed)
    }

    /// Notes whether anyone is on the planet; after `idle_ticks` with nobody
    /// the copies leave memory (they are on disk).
    pub fn tend(&mut self, tick: u64, players: usize, idle_ticks: u64) {
        if players > 0 {
            self.last_used = tick;
        } else if !self.copies.is_empty() && tick.saturating_sub(self.last_used) > idle_ticks {
            self.copies.clear();
        }
    }

    pub fn held(&self) -> usize {
        self.copies.len()
    }
}

fn frames(pos: ChunkPos, level1: &FarChunk, flags: &BlockFlags) -> Copy {
    let frame = |f: &FarChunk| Arc::new(bm_protocol::encode_frame(&ServerMsg::Far { pos, data: f.encode() }));
    Copy {
        frames: [frame(level1), frame(&level1.coarser(flags))],
    }
}

fn build(
    planet: u16,
    pos: ChunkPos,
    src: Source,
    flags: &BlockFlags,
    g: &Generator,
    store: &Store,
) -> Option<(Copy, Option<Vec<u8>>)> {
    let made = |chunk: &Chunk| {
        let f = FarChunk::from_chunk(chunk, flags);
        let bytes = f.encode();
        (frames(pos, &f, flags), Some(bytes))
    };
    match src {
        Source::Chunk(c) => Some(made(&c)),
        Source::Saved(bytes) => match Chunk::decode(&bytes) {
            Ok(c) => Some(made(&c)),
            Err(e) => {
                tracing::error!("far copy of {pos:?} on planet {planet}: saved chunk unreadable ({e})");
                None
            }
        },
        Source::Disk => {
            match store.far(planet, pos) {
                Ok(Some(bytes)) => {
                    if let Ok(f) = FarChunk::decode(&bytes)
                        && f.level() == 1
                    {
                        return Some((frames(pos, &f, flags), None));
                    }
                    // Another format: made again below.
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("reading the far copy of {pos:?} on planet {planet} failed: {e:#}");
                    return None;
                }
            }
            match store.chunk(planet, pos) {
                Ok(Some(bytes)) => match Chunk::decode(&bytes) {
                    Ok(c) => Some(made(&c)),
                    Err(e) => {
                        tracing::error!("far copy of {pos:?} on planet {planet}: saved chunk unreadable ({e})");
                        None
                    }
                },
                Ok(None) => Some(made(&g.generate(pos))),
                Err(e) => {
                    // Never made from fresh terrain when the saved chunk could
                    // not be read: it may hold a build.
                    tracing::warn!("reading chunk {pos:?} of planet {planet} for its far copy failed: {e:#}");
                    None
                }
            }
        }
    }
}
