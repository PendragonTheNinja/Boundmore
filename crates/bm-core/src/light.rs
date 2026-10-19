//! Sky light and block light, propagated in 3D across chunks and the seam.
//!
//! # Rules
//!
//! Every cell holds two levels, 0 to 15: **sky** light and **block** light.
//! Light is the smallest assignment that satisfies, for every cell:
//!
//! - an opaque cell holds no sky light, and block light equal to its own
//!   emission (a glow crystal is lit; light never passes through it);
//! - a non-opaque cell holds at least its emission, and at least one less than
//!   each of its six neighbours;
//! - sky light falls straight down without loss: a non-opaque cell under a
//!   cell at sky 15 (or in the top layer of the planet, under open sky) is at
//!   sky 15 itself. Sideways, upwards and under cover it loses 1 per block.
//! - **water** (a block that is translucent and not solid, so glass is
//!   excluded) breaks the free fall: sky light entering it from above loses 1
//!   like any other step, so the sea darkens by one level per block of depth
//!   and its floor is dark below fifteen blocks. Block light is not filtered.
//!
//! Below the floor is solid and dark. Above the top of the planet is open sky.
//!
//! # What is lit
//!
//! Light is computed only for **complete columns**: a column (chunk x, z) is
//! lit once every chunk of it is resident, and from then until one of its
//! chunks is forgotten. Light flows between lit columns in both directions.
//! Anything outside the lit set reads as dark. That makes the result a pure
//! function of which columns are lit and what their blocks are, so it never
//! depends on the order chunks arrived in, and a missing neighbour can only
//! make a column darker than the truth until it arrives (dark then brighten),
//! never brighter than the truth after it is gone.
//!
//! # Work
//!
//! Each event does work proportional to the light it changes: a column is lit
//! by a downward sweep for the free fall (uniform chunks in one step) and a
//! breadth-first spread from the cells that can lend light sideways; an edit or
//! a forgotten column removes the light that depended on what went away and
//! refills from the light left at the edge of the removed region (the classic
//! removal-then-refill), visiting only the cells involved. Spreading runs one
//! chunk at a time over its dense array, handing cells across a border to the
//! neighbour's queue, so the inner loop never touches a hash map.
//!
//! # Storage
//!
//! [`ChunkLight`] is one byte per voxel (sky in the high nibble, block light in
//! the low one), or a single value for a chunk that is uniform (open sky above
//! the terrain, solid dark rock below). Dense arrays are shared behind an `Arc`
//! and copied on write, so a mesh job can hold a snapshot cheaply while the map
//! moves on.

use crate::block::BlockFlags;
use crate::chunk::Chunk;
use crate::coords::{BlockPos, CHUNK_SIZE, CHUNK_VOLUME, ChunkPos, LocalPos};
use crate::planet::PlanetShape;
use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::sync::Arc;

/// The brightest light level.
pub const MAX_LIGHT: u8 = 15;

/// One cell's light: sky in the high four bits, block light in the low four.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Light(pub u8);

impl Light {
    pub const DARK: Light = Light(0);
    /// Sky 15, block 0: what lies above the planet.
    pub const OPEN_SKY: Light = Light(0xf0);

    pub const fn new(sky: u8, block: u8) -> Self {
        Light((sky & 15) << 4 | (block & 15))
    }

    pub const fn sky(self) -> u8 {
        self.0 >> 4
    }

    pub const fn block(self) -> u8 {
        self.0 & 15
    }
}

/// The light of one chunk.
#[derive(Clone, Debug)]
pub struct ChunkLight(Repr);

#[derive(Clone, Debug)]
enum Repr {
    Uniform(Light),
    Dense(Arc<[u8; CHUNK_VOLUME]>),
}

fn filled(v: u8) -> Arc<[u8; CHUNK_VOLUME]> {
    let b: Box<[u8; CHUNK_VOLUME]> = vec![v; CHUNK_VOLUME]
        .into_boxed_slice()
        .try_into()
        .expect("exactly one chunk of cells");
    Arc::from(b)
}

impl ChunkLight {
    pub const fn uniform(l: Light) -> Self {
        Self(Repr::Uniform(l))
    }

    pub const fn dark() -> Self {
        Self::uniform(Light::DARK)
    }

    pub const fn open_sky() -> Self {
        Self::uniform(Light::OPEN_SKY)
    }

    /// Builds a chunk's light cell by cell (by [`LocalPos::index`]).
    pub fn from_fn(mut f: impl FnMut(usize) -> Light) -> Self {
        let mut a = filled(0);
        let d = Arc::get_mut(&mut a).expect("freshly made");
        for (i, v) in d.iter_mut().enumerate() {
            *v = f(i).0;
        }
        let mut c = Self(Repr::Dense(a));
        c.compact();
        c
    }

    #[inline]
    pub fn get(&self, p: LocalPos) -> Light {
        self.get_index(p.index())
    }

    #[inline]
    pub fn get_index(&self, i: usize) -> Light {
        match &self.0 {
            Repr::Uniform(l) => *l,
            Repr::Dense(d) => Light(d[i]),
        }
    }

    pub fn set(&mut self, p: LocalPos, l: Light) {
        let i = p.index();
        if self.get_index(i) != l {
            self.dense_mut()[i] = l.0;
        }
    }

    /// The single value of a uniform chunk. Exact after [`ChunkLight::compact`].
    pub fn uniform_value(&self) -> Option<Light> {
        match self.0 {
            Repr::Uniform(l) => Some(l),
            Repr::Dense(_) => None,
        }
    }

    pub fn is_dense(&self) -> bool {
        matches!(self.0, Repr::Dense(_))
    }

    /// Heap bytes this chunk's light owns (shared copies count in full).
    pub fn heap_bytes(&self) -> usize {
        if self.is_dense() { CHUNK_VOLUME } else { 0 }
    }

    fn dense_mut(&mut self) -> &mut [u8; CHUNK_VOLUME] {
        if let Repr::Uniform(l) = self.0 {
            self.0 = Repr::Dense(filled(l.0));
        }
        let Repr::Dense(d) = &mut self.0 else {
            unreachable!("made dense above")
        };
        Arc::make_mut(d)
    }

    /// Collapses a dense chunk whose cells are all equal.
    pub fn compact(&mut self) {
        if let Repr::Dense(d) = &self.0 {
            let v = d[0];
            if d.iter().all(|&x| x == v) {
                self.0 = Repr::Uniform(Light(v));
            }
        }
    }
}

impl PartialEq for ChunkLight {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Repr::Uniform(a), Repr::Uniform(b)) => a == b,
            (Repr::Dense(a), Repr::Dense(b)) => a[..] == b[..],
            _ => (0..CHUNK_VOLUME).all(|i| self.get_index(i) == other.get_index(i)),
        }
    }
}

impl Eq for ChunkLight {}

/// Where the light map reads blocks: the caller's own chunk store, so blocks
/// are never copied. Positions asked for are canonical (wrapped).
pub trait BlockSource {
    fn chunk(&self, pos: ChunkPos) -> Option<&Chunk>;
}

impl<S: BuildHasher> BlockSource for HashMap<ChunkPos, Arc<Chunk>, S> {
    fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.get(&pos).map(|c| &**c)
    }
}

impl<S: BuildHasher> BlockSource for HashMap<ChunkPos, Chunk, S> {
    fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.get(&pos)
    }
}

// Per-id light properties, packed for the inner loops.
const EMIT: u8 = 0x0f;
const OPAQUE: u8 = 0x10;
/// Water: sky light does not fall through it without loss.
const DIMS: u8 = 0x20;

const SKY: usize = 0;
const BLOCK: usize = 1;
const SHIFT: [u32; 2] = [4, 0];

/// +X, -X, +Y, -Y, +Z, -Z.
const DIRS: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];
const OPPOSITE: [usize; 6] = [1, 0, 3, 2, 5, 4];
const DOWN: usize = 3;
const HORIZONTAL: [usize; 4] = [0, 1, 4, 5];
const NONE: u32 = u32::MAX;
/// Queue entry flag: spread from the cell's current light instead of offering
/// it the bucket's level.
const SPREAD: u16 = 0x8000;
const IDX: u16 = 0x7fff;
/// Every neighbour of a chunk, as a mask over `neighbor_index` order.
const ALL_NEIGHBOURS: u32 = (1 << 27) - 1;
const CENTRE: u32 = 1 << 13;

#[inline]
fn nibble(v: u8, ch: usize) -> u8 {
    v >> SHIFT[ch] & 15
}

#[inline]
fn with_nibble(v: u8, ch: usize, l: u8) -> u8 {
    (v & !(15 << SHIFT[ch])) | l << SHIFT[ch]
}

/// What a cell at level `l` offers its neighbour in direction `dir`.
#[inline]
fn offer(l: u8, dir: usize, ch: usize) -> u8 {
    if ch == SKY && dir == DOWN && l == MAX_LIGHT {
        MAX_LIGHT
    } else {
        l - 1
    }
}

/// The level a cell with properties `p` takes from an offer.
#[inline]
fn admit(offered: u8, p: u8, ch: usize) -> u8 {
    if p & OPAQUE != 0 {
        if ch == BLOCK { offered.min(p & EMIT) } else { 0 }
    } else if ch == SKY && offered == MAX_LIGHT && p & DIMS != 0 {
        MAX_LIGHT - 1
    } else {
        offered
    }
}

/// A cell's light from nothing but itself: its emission, or open sky above
/// the top layer.
#[inline]
fn intrinsic(p: u8, ch: usize, i: usize, top_chunk: bool) -> u8 {
    if ch == BLOCK {
        p & EMIT
    } else if top_chunk && i >> 10 == 31 && p & OPAQUE == 0 {
        MAX_LIGHT
    } else {
        0
    }
}

/// The cell beside `i` in direction `dir`: `Ok` inside the chunk, `Err` the
/// index in the neighbouring chunk.
#[inline]
fn step(i: usize, dir: usize) -> Result<usize, usize> {
    let (x, z, y) = (i & 31, i >> 5 & 31, i >> 10);
    match dir {
        0 if x != 31 => Ok(i + 1),
        0 => Err(i - 31),
        1 if x != 0 => Ok(i - 1),
        1 => Err(i + 31),
        2 if y != 31 => Ok(i + 1024),
        2 => Err(i - 31 * 1024),
        3 if y != 0 => Ok(i - 1024),
        3 => Err(i + 31 * 1024),
        4 if z != 31 => Ok(i + 32),
        4 => Err(i - 31 * 32),
        _ if z != 0 => Ok(i - 32),
        _ => Err(i + 31 * 32),
    }
}

/// The 1024 cells of a chunk's face towards `dir`.
fn face_cells(dir: usize) -> impl Iterator<Item = usize> {
    (0..1024usize).map(move |k| {
        let (a, b) = (k & 31, k >> 5);
        match dir {
            0 => 31 | a << 5 | b << 10,
            1 => a << 5 | b << 10,
            2 => a | b << 5 | 31 << 10,
            3 => a | b << 5,
            4 => a | 31 << 5 | b << 10,
            _ => a | b << 10,
        }
    })
}

/// Which chunks' meshes sample cell `i`: its own and, on a border, the
/// neighbours across it, as bits in `(dy+1)*9 + (dz+1)*3 + (dx+1)` order (the
/// order meshing takes neighbours in).
#[inline]
fn border_mask(i: usize) -> u32 {
    let (x, z, y) = (i & 31, i >> 5 & 31, i >> 10);
    if (1..31).contains(&x) && (1..31).contains(&y) && (1..31).contains(&z) {
        return CENTRE;
    }
    let span = |v: usize| -> u32 {
        match v {
            0 => 0b011,
            31 => 0b110,
            _ => 0b010,
        }
    };
    let (sx, sy, sz) = (span(x), span(y), span(z));
    let mut m = 0;
    for dy in 0..3 {
        for dz in 0..3 {
            for dx in 0..3 {
                if sx >> dx & sy >> dy & sz >> dz & 1 != 0 {
                    m |= 1 << (dy * 9 + dz * 3 + dx);
                }
            }
        }
    }
    m
}

/// A chunk's light, made dense on the first write so read-only visits to a
/// uniform or shared chunk cost nothing.
enum Cells<'a> {
    Lazy(&'a mut ChunkLight),
    Dense(&'a mut [u8; CHUNK_VOLUME]),
    Taken,
}

impl Cells<'_> {
    #[inline]
    fn get(&self, i: usize) -> u8 {
        match self {
            Cells::Lazy(l) => l.get_index(i).0,
            Cells::Dense(d) => d[i],
            Cells::Taken => unreachable!("restored before use"),
        }
    }

    #[inline]
    fn set(&mut self, i: usize, v: u8) {
        if let Cells::Dense(d) = self {
            d[i] = v;
            return;
        }
        if let Cells::Lazy(l) = std::mem::replace(self, Cells::Taken) {
            let d = l.dense_mut();
            d[i] = v;
            *self = Cells::Dense(d);
        }
    }
}

struct Column {
    present: Vec<bool>,
    count: u32,
    lit: bool,
}

struct Slot {
    pos: ChunkPos,
    light: ChunkLight,
    /// Slots of the lit face neighbours, by direction.
    links: [u32; 6],
}

/// A slot's pending propagation.
#[derive(Default)]
struct Work {
    /// Per channel, per offered level: cell indices (with [`SPREAD`]).
    add: [[Vec<u16>; 16]; 2],
    /// Per channel: (cell, level the removed neighbour offered it).
    rem: [Vec<(u16, u8)>; 2],
    in_add: bool,
    in_rem: bool,
    touched: bool,
}

impl Work {
    fn has_add(&self) -> bool {
        self.add.iter().flatten().any(|b| !b.is_empty())
    }
}

/// What one event changed.
#[derive(Default)]
struct Event {
    /// Record each cell's value before its first write instead of marking it
    /// changed, because removal followed by refill often restores it.
    logging: bool,
    log: Vec<(u32, u16, u8)>,
    masks: HashMap<ChunkPos, u32>,
    touched: Vec<u32>,
}

#[inline]
fn record(ev: &mut Event, mask: &mut u32, s: u32, i: usize, old: u8) {
    if ev.logging {
        ev.log.push((s, i as u16, old));
    } else {
        *mask |= border_mask(i);
    }
}

#[inline]
fn push_add(work: &mut [Work], active: &mut Vec<u32>, s: u32, i: usize, ch: usize, level: u8, spread: bool) {
    let w = &mut work[s as usize];
    w.add[ch][level as usize].push(i as u16 | if spread { SPREAD } else { 0 });
    if !w.in_add {
        w.in_add = true;
        active.push(s);
    }
}

#[inline]
fn push_rem(work: &mut [Work], active: &mut Vec<u32>, s: u32, i: usize, ch: usize, offered: u8) {
    let w = &mut work[s as usize];
    w.rem[ch].push((i as u16, offered));
    if !w.in_rem {
        w.in_rem = true;
        active.push(s);
    }
}

/// Light for the resident chunks of one planet.
///
/// Call it alongside your chunk store, after the store has changed:
/// [`LightMap::chunk_loaded`] after inserting a chunk,
/// [`LightMap::chunk_forgotten`] after removing one,
/// [`LightMap::block_changed`] after writing a block. Each returns, sorted and
/// without duplicates, the resident chunks whose meshes sample light that
/// changed (their own or a neighbour's border), which are exactly the ones to
/// re-mesh. Take a mesh job's light with [`LightMap::neighborhood`].
pub struct LightMap {
    shape: PlanetShape,
    props: Box<[u8]>,
    columns: HashMap<(i32, i32), Column>,
    index: HashMap<ChunkPos, u32>,
    slots: Vec<Slot>,
    free: Vec<u32>,
    work: Vec<Work>,
    active_add: Vec<u32>,
    active_rem: Vec<u32>,
    ev: Event,
}

impl LightMap {
    pub fn new(shape: PlanetShape, flags: &BlockFlags) -> Self {
        // Indexed by any u16 id, so the hot loops need no bounds checks; ids
        // past the catalogue read as air, as `BlockFlags` does.
        let mut props = vec![0u8; 1 << 16].into_boxed_slice();
        for (id, p) in props.iter_mut().enumerate().take(flags.len()) {
            let b = crate::BlockId(id as u16);
            let mut v = flags.light(b).min(MAX_LIGHT);
            if flags.opaque(b) {
                v |= OPAQUE;
            } else if flags.translucent(b) && !flags.solid(b) {
                v |= DIMS;
            }
            *p = v;
        }
        Self {
            shape,
            props,
            columns: HashMap::new(),
            index: HashMap::new(),
            slots: Vec::new(),
            free: Vec::new(),
            work: Vec::new(),
            active_add: Vec::new(),
            active_rem: Vec::new(),
            ev: Event::default(),
        }
    }

    pub fn shape(&self) -> PlanetShape {
        self.shape
    }

    /// A chunk was inserted into (or replaced in) `blocks`. Lights its column
    /// if that completes it.
    pub fn chunk_loaded<S: BlockSource + ?Sized>(&mut self, pos: ChunkPos, blocks: &S) -> Vec<ChunkPos> {
        if !self.shape.contains_chunk_y(pos.y) {
            return Vec::new();
        }
        let pos = self.shape.wrap_chunk(pos);
        let h = self.shape.height_chunks;
        let key = (pos.x, pos.z);
        let col = self.columns.entry(key).or_insert_with(|| Column {
            present: vec![false; h as usize],
            count: 0,
            lit: false,
        });
        let y = pos.y as usize;
        let mut changed = Vec::new();
        if col.present[y] {
            // New contents for a chunk already lit: relight its column whole.
            if col.lit {
                col.lit = false;
                changed = self.unlight_column(key, blocks);
            }
        } else {
            col.present[y] = true;
            col.count += 1;
        }
        let col = self.columns.get_mut(&key).expect("inserted above");
        if col.count == h && !col.lit {
            col.lit = true;
            changed.extend(self.light_column(key, blocks));
            changed.sort_unstable();
            changed.dedup();
        }
        changed
    }

    /// A chunk was removed from `blocks`. Its column stops being lit, and the
    /// light it lent its neighbours is taken back.
    pub fn chunk_forgotten<S: BlockSource + ?Sized>(&mut self, pos: ChunkPos, blocks: &S) -> Vec<ChunkPos> {
        if !self.shape.contains_chunk_y(pos.y) {
            return Vec::new();
        }
        let pos = self.shape.wrap_chunk(pos);
        let key = (pos.x, pos.z);
        let Some(col) = self.columns.get_mut(&key) else {
            return Vec::new();
        };
        if !col.present[pos.y as usize] {
            return Vec::new();
        }
        col.present[pos.y as usize] = false;
        col.count -= 1;
        let was_lit = std::mem::replace(&mut col.lit, false);
        if col.count == 0 {
            self.columns.remove(&key);
        }
        if was_lit {
            self.unlight_column(key, blocks)
        } else {
            Vec::new()
        }
    }

    /// The block at `p` in `blocks` has changed.
    pub fn block_changed<S: BlockSource + ?Sized>(&mut self, p: BlockPos, blocks: &S) -> Vec<ChunkPos> {
        if !self.shape.contains_y(p.y) {
            return Vec::new();
        }
        let p = self.shape.wrap_block(p);
        let Some(&s) = self.index.get(&p.chunk()) else {
            return Vec::new();
        };
        self.ev.logging = true;
        let i = p.local().index();
        let old = self.slots[s as usize].light.get_index(i).0;
        if old != 0 {
            self.ev.log.push((s, i as u16, old));
            self.slots[s as usize].light.dense_mut()[i] = 0;
            self.touch(s);
        }
        // Take back everything the old cell lent, then let the new cell and
        // its neighbours offer what they have.
        for ch in [SKY, BLOCK] {
            let l = nibble(old, ch);
            if l > 1 {
                for dir in 0..6 {
                    let o = offer(l, dir, ch);
                    if o > 0
                        && let Some((ns, n)) = self.neighbour(s, i, dir)
                    {
                        push_rem(&mut self.work, &mut self.active_rem, ns, n, ch, o);
                    }
                }
            }
        }
        let top_chunk = p.chunk().y as u32 == self.shape.height_chunks - 1;
        let props = blocks
            .chunk(p.chunk())
            .map_or(OPAQUE, |c| self.props[c.get(p.local()).0 as usize]);
        for ch in [SKY, BLOCK] {
            let e = intrinsic(props, ch, i, top_chunk);
            if e > 0 {
                push_add(&mut self.work, &mut self.active_add, s, i, ch, e, false);
            }
        }
        for dir in 0..6 {
            if let Some((ns, n)) = self.neighbour(s, i, dir) {
                let l = self.slots[ns as usize].light.get_index(n).0;
                for ch in [SKY, BLOCK] {
                    let v = nibble(l, ch);
                    if v > 1 {
                        push_add(&mut self.work, &mut self.active_add, ns, n, ch, v, true);
                    }
                }
            }
        }
        self.run(blocks);
        self.finish()
    }

    /// A chunk's light, if its column is lit.
    pub fn get(&self, pos: ChunkPos) -> Option<&ChunkLight> {
        let pos = self.shape.wrap_chunk(pos);
        self.index.get(&pos).map(|&s| &self.slots[s as usize].light)
    }

    pub fn is_lit(&self, pos: ChunkPos) -> bool {
        self.index.contains_key(&self.shape.wrap_chunk(pos))
    }

    /// The light at a block: open sky above the planet, dark below it and
    /// wherever the column is not lit.
    pub fn light_at(&self, p: BlockPos) -> Light {
        if p.y >= self.shape.height_blocks() {
            return Light::OPEN_SKY;
        }
        if p.y < 0 {
            return Light::DARK;
        }
        let p = self.shape.wrap_block(p);
        self.get(p.chunk()).map_or(Light::DARK, |l| l.get(p.local()))
    }

    /// The light a mesh job for `pos` needs, in the order meshing takes
    /// neighbours (`(dy+1)*9 + (dz+1)*3 + (dx+1)`, the centre at 13). Above
    /// the planet is open sky and below it is dark; `None` is not lit and
    /// meshes as dark. Cheap: dense light is shared, not copied.
    pub fn neighborhood(&self, pos: ChunkPos) -> [Option<ChunkLight>; 27] {
        std::array::from_fn(|k| {
            let (dx, dy, dz) = (k as i32 % 3 - 1, k as i32 / 9 - 1, k as i32 / 3 % 3 - 1);
            let n = pos.offset(dx, dy, dz);
            if n.y >= self.shape.height_chunks as i32 {
                Some(ChunkLight::open_sky())
            } else if n.y < 0 {
                Some(ChunkLight::dark())
            } else {
                self.get(n).cloned()
            }
        })
    }

    /// Chunks with light (every chunk of every lit column).
    pub fn lit_chunks(&self) -> usize {
        self.index.len()
    }

    /// Chunks whose light is stored per voxel rather than as one value.
    pub fn dense_chunks(&self) -> usize {
        self.index
            .values()
            .filter(|&&s| self.slots[s as usize].light.is_dense())
            .count()
    }

    fn resident(&self, p: ChunkPos) -> bool {
        self.columns.get(&(p.x, p.z)).is_some_and(|c| c.present[p.y as usize])
    }

    fn neighbour(&self, s: u32, i: usize, dir: usize) -> Option<(u32, usize)> {
        match step(i, dir) {
            Ok(n) => Some((s, n)),
            Err(n) => {
                let ns = self.slots[s as usize].links[dir];
                (ns != NONE).then_some((ns, n))
            }
        }
    }

    fn touch(&mut self, s: u32) {
        let w = &mut self.work[s as usize];
        if !w.touched {
            w.touched = true;
            self.ev.touched.push(s);
        }
    }

    fn alloc(&mut self, pos: ChunkPos) -> u32 {
        let s = match self.free.pop() {
            Some(s) => s,
            None => {
                self.slots.push(Slot {
                    pos,
                    light: ChunkLight::dark(),
                    links: [NONE; 6],
                });
                self.work.push(Work::default());
                (self.slots.len() - 1) as u32
            }
        };
        self.slots[s as usize] = Slot {
            pos,
            light: ChunkLight::dark(),
            links: [NONE; 6],
        };
        self.index.insert(pos, s);
        for (dir, &(dx, dy, dz)) in DIRS.iter().enumerate() {
            let n = pos.offset(dx, dy, dz);
            if !self.shape.contains_chunk_y(n.y) {
                continue;
            }
            if let Some(&ns) = self.index.get(&self.shape.wrap_chunk(n)) {
                self.slots[s as usize].links[dir] = ns;
                self.slots[ns as usize].links[OPPOSITE[dir]] = s;
            }
        }
        s
    }

    fn release(&mut self, s: u32) {
        let slot = &mut self.slots[s as usize];
        let links = std::mem::replace(&mut slot.links, [NONE; 6]);
        slot.light = ChunkLight::dark();
        let pos = slot.pos;
        for (dir, &ns) in links.iter().enumerate() {
            if ns != NONE {
                self.slots[ns as usize].links[OPPOSITE[dir]] = NONE;
            }
        }
        self.index.remove(&pos);
        self.work[s as usize] = Work::default();
        self.free.push(s);
    }

    fn column_slots(&self, (x, z): (i32, i32)) -> Vec<u32> {
        (0..self.shape.height_chunks as i32)
            .map(|y| self.index[&ChunkPos::new(x, y, z)])
            .collect()
    }

    fn light_column<S: BlockSource + ?Sized>(&mut self, (cx, cz): (i32, i32), blocks: &S) -> Vec<ChunkPos> {
        let h = self.shape.height_chunks as usize;
        let ids: Vec<u32> = (0..h).map(|y| self.alloc(ChunkPos::new(cx, y as i32, cz))).collect();
        for &s in &ids {
            self.touch(s);
        }
        let top = self.shape.height_blocks();
        let mut cands: Vec<(u32, usize, usize, u8)> = Vec::new();

        // The free fall. `stop` is the lowest cell of each (x, z) at sky 15.
        let mut stop = vec![top; 1024];
        let mut falling = vec![true; 1024];
        let mut n_falling = 1024;
        for cy in (0..h).rev() {
            if n_falling == 0 {
                break;
            }
            let s = ids[cy] as usize;
            let base = cy as i64 * CHUNK_SIZE as i64;
            let Some(chunk) = blocks.chunk(self.slots[s].pos) else {
                break;
            };
            if let Some(b) = chunk.uniform() {
                let p = self.props[b.0 as usize];
                if p & (OPAQUE | DIMS) == 0 {
                    if n_falling == 1024 {
                        self.slots[s].light = ChunkLight::open_sky();
                    } else {
                        let d = self.slots[s].light.dense_mut();
                        for c in (0..1024).filter(|&c| falling[c]) {
                            for ly in 0..32 {
                                d[c | ly << 10] = Light::OPEN_SKY.0;
                            }
                        }
                    }
                    for c in (0..1024).filter(|&c| falling[c]) {
                        stop[c] = base;
                    }
                } else {
                    for c in (0..1024).filter(|&c| falling[c]) {
                        if p & OPAQUE == 0 {
                            cands.push((s as u32, c | 31 << 10, SKY, MAX_LIGHT));
                        }
                    }
                    n_falling = 0;
                }
                continue;
            }
            let props = &self.props;
            let d = self.slots[s].light.dense_mut();
            for c in 0..1024 {
                if !falling[c] {
                    continue;
                }
                for ly in (0..32).rev() {
                    let i = c | ly << 10;
                    let p = props[chunk.get_index(i).0 as usize];
                    if p & (OPAQUE | DIMS) == 0 {
                        d[i] = Light::OPEN_SKY.0;
                        stop[c] = base + ly as i64;
                    } else {
                        if p & OPAQUE == 0 {
                            cands.push((s as u32, i, SKY, MAX_LIGHT));
                        }
                        falling[c] = false;
                        n_falling -= 1;
                        break;
                    }
                }
            }
        }

        // Emitters.
        for &s in &ids {
            let Some(chunk) = blocks.chunk(self.slots[s as usize].pos) else {
                continue;
            };
            if !chunk.palette().iter().any(|b| self.props[b.id.0 as usize] & EMIT != 0) {
                continue;
            }
            for i in 0..CHUNK_VOLUME {
                let e = self.props[chunk.get_index(i).0 as usize] & EMIT;
                if e > 0 {
                    cands.push((s, i, BLOCK, e));
                }
            }
        }
        for (s, i, ch, l) in cands {
            push_add(&mut self.work, &mut self.active_add, s, i, ch, l, false);
        }

        // Sideways from the fall: each (x, z) lends to the cells beside it
        // that the fall did not reach, from its own lowest 15 up to theirs.
        for c in 0..1024 {
            let (x, z) = ((c & 31) as i32, (c >> 5) as i32);
            let s0 = stop[c];
            let mut lim = s0;
            for dir in HORIZONTAL {
                let (dx, _, dz) = DIRS[dir];
                let (nx, nz) = (x + dx, z + dz);
                if (0..32).contains(&nx) && (0..32).contains(&nz) {
                    lim = lim.max(stop[(nx | nz << 5) as usize]);
                    continue;
                }
                // The neighbour column's cells at sky 15 run from its top down
                // without a gap, so walk up until we meet them.
                let ni = ((nx & 31) | (nz & 31) << 5) as usize;
                let mut y = s0;
                while y < top {
                    let ns = self.slots[ids[(y >> 5) as usize] as usize].links[dir];
                    if ns == NONE
                        || self.slots[ns as usize]
                            .light
                            .get_index(ni | ((y & 31) as usize) << 10)
                            .sky()
                            == MAX_LIGHT
                    {
                        break;
                    }
                    y += 1;
                }
                lim = lim.max(y);
            }
            for y in s0..lim {
                let s = ids[(y >> 5) as usize];
                push_add(
                    &mut self.work,
                    &mut self.active_add,
                    s,
                    c | ((y & 31) as usize) << 10,
                    SKY,
                    MAX_LIGHT,
                    true,
                );
            }
        }
        self.run(blocks);

        // What the lit neighbours have to lend across the border.
        for &s in &ids {
            for dir in HORIZONTAL {
                let ns = self.slots[s as usize].links[dir];
                if ns == NONE {
                    continue;
                }
                let theirs = &self.slots[ns as usize].light;
                let ours = &self.slots[s as usize].light;
                if let Some(u) = theirs.uniform_value() {
                    if u.sky() < 2 && u.block() < 2 {
                        continue;
                    }
                    if let Some(v) = ours.uniform_value()
                        && v.sky() + 1 >= u.sky()
                        && v.block() + 1 >= u.block()
                    {
                        continue;
                    }
                }
                for i in face_cells(dir) {
                    let n = step(i, dir).expect_err("face cell");
                    let (a, b) = (theirs.get_index(n).0, ours.get_index(i).0);
                    for ch in [SKY, BLOCK] {
                        let o = nibble(a, ch);
                        if o >= 2 && o - 1 > nibble(b, ch) {
                            self.work[s as usize].add[ch][o as usize - 1].push(i as u16);
                            if !self.work[s as usize].in_add {
                                self.work[s as usize].in_add = true;
                                self.active_add.push(s);
                            }
                        }
                    }
                }
            }
        }
        self.run(blocks);

        for &s in &ids {
            let slot = &self.slots[s as usize];
            if slot.light.uniform_value() != Some(Light::DARK) {
                *self.ev.masks.entry(slot.pos).or_default() |= ALL_NEIGHBOURS;
            }
        }
        self.finish()
    }

    fn unlight_column<S: BlockSource + ?Sized>(&mut self, key: (i32, i32), blocks: &S) -> Vec<ChunkPos> {
        self.ev.logging = true;
        let ids = self.column_slots(key);
        for &s in &ids {
            let slot = &self.slots[s as usize];
            if slot.light.uniform_value() == Some(Light::DARK) {
                continue;
            }
            *self.ev.masks.entry(slot.pos).or_default() |= ALL_NEIGHBOURS;
            for dir in HORIZONTAL {
                let ns = slot.links[dir];
                if ns == NONE || self.slots[ns as usize].light.uniform_value() == Some(Light::DARK) {
                    continue;
                }
                for i in face_cells(dir) {
                    let a = slot.light.get_index(i).0;
                    for ch in [SKY, BLOCK] {
                        let l = nibble(a, ch);
                        if l >= 2 {
                            let n = step(i, dir).expect_err("face cell");
                            push_rem(&mut self.work, &mut self.active_rem, ns, n, ch, l - 1);
                        }
                    }
                }
            }
        }
        // Unlinked first, so the refill cannot flow back in.
        for &s in &ids {
            self.release(s);
        }
        self.run(blocks);
        self.finish()
    }

    fn run<S: BlockSource + ?Sized>(&mut self, blocks: &S) {
        // Every removal finishes before any refill starts; refills never
        // queue removals, so the phases stay apart.
        loop {
            if let Some(s) = self.active_rem.pop() {
                self.process_rem(s, blocks);
            } else if let Some(s) = self.active_add.pop() {
                self.process_add(s, blocks);
            } else {
                break;
            }
        }
    }

    fn process_add<S: BlockSource + ?Sized>(&mut self, s: u32, blocks: &S) {
        let su = s as usize;
        let mut w = std::mem::take(&mut self.work[su]);
        w.in_add = false;
        if !w.touched {
            w.touched = true;
            self.ev.touched.push(s);
        }
        let pos = self.slots[su].pos;
        let links = self.slots[su].links;
        let Some(chunk) = blocks.chunk(pos) else {
            // Not in the store although its column is lit: nothing to light.
            w.add.iter_mut().flatten().for_each(Vec::clear);
            self.work[su] = w;
            return;
        };
        let props = &self.props;
        let uniform = chunk.uniform().map(|b| props[b.0 as usize]);
        let prop = |i: usize| uniform.unwrap_or_else(|| props[chunk.get_index(i).0 as usize]);
        let ev = &mut self.ev;
        let work = &mut self.work;
        let active = &mut self.active_add;
        let mut cells = Cells::Lazy(&mut self.slots[su].light);
        let mut mask = 0u32;
        loop {
            for ch in [SKY, BLOCK] {
                for level in (1..16).rev() {
                    while let Some(e) = w.add[ch][level].pop() {
                        let i = (e & IDX) as usize;
                        let old = cells.get(i);
                        let cur = nibble(old, ch);
                        let l = if e & SPREAD != 0 {
                            cur
                        } else {
                            let v = admit(level as u8, prop(i), ch);
                            if v <= cur {
                                continue;
                            }
                            cells.set(i, with_nibble(old, ch, v));
                            record(ev, &mut mask, s, i, old);
                            v
                        };
                        if l <= 1 {
                            continue;
                        }
                        for (dir, &ns) in links.iter().enumerate() {
                            let o = offer(l, dir, ch);
                            match step(i, dir) {
                                Ok(n) => {
                                    let nold = cells.get(n);
                                    let v = admit(o, prop(n), ch);
                                    if v > nibble(nold, ch) {
                                        cells.set(n, with_nibble(nold, ch, v));
                                        record(ev, &mut mask, s, n, nold);
                                        w.add[ch][v as usize].push(n as u16 | SPREAD);
                                    }
                                }
                                Err(n) => {
                                    if ns != NONE {
                                        debug_assert_ne!(ns, s);
                                        push_add(work, active, ns, n, ch, o, false);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // A spread entry can find its cell raised since it was queued and
            // fill a bucket already passed; sweep again until all are empty.
            if !w.has_add() {
                break;
            }
        }
        if mask != 0 {
            *ev.masks.entry(pos).or_default() |= mask;
        }
        self.work[su] = w;
    }

    fn process_rem<S: BlockSource + ?Sized>(&mut self, s: u32, blocks: &S) {
        let su = s as usize;
        let mut w = std::mem::take(&mut self.work[su]);
        w.in_rem = false;
        if !w.touched {
            w.touched = true;
            self.ev.touched.push(s);
        }
        let pos = self.slots[su].pos;
        let links = self.slots[su].links;
        let top_chunk = pos.y as u32 == self.shape.height_chunks - 1;
        let Some(chunk) = blocks.chunk(pos) else {
            w.rem.iter_mut().for_each(Vec::clear);
            self.work[su] = w;
            return;
        };
        let props = &self.props;
        let uniform = chunk.uniform().map(|b| props[b.0 as usize]);
        let prop = |i: usize| uniform.unwrap_or_else(|| props[chunk.get_index(i).0 as usize]);
        let ev = &mut self.ev;
        let work = &mut self.work;
        let active = &mut self.active_rem;
        let mut cells = Cells::Lazy(&mut self.slots[su].light);
        let mut mask = 0u32;
        for ch in [SKY, BLOCK] {
            while let Some((e, o)) = w.rem[ch].pop() {
                let i = e as usize;
                let old = cells.get(i);
                let cur = nibble(old, ch);
                if cur == 0 {
                    continue;
                }
                let p = prop(i);
                // Lit by something other than what went away: it refills.
                if p & OPAQUE != 0 || cur > admit(o, p, ch) {
                    w.add[ch][cur as usize].push(i as u16 | SPREAD);
                    continue;
                }
                cells.set(i, with_nibble(old, ch, 0));
                record(ev, &mut mask, s, i, old);
                // Even offering nothing, the neighbours must be visited: a
                // brighter one among them is what refills this cell.
                for (dir, &ns) in links.iter().enumerate() {
                    let oo = offer(cur, dir, ch);
                    match step(i, dir) {
                        Ok(n) => w.rem[ch].push((n as u16, oo)),
                        Err(n) => {
                            if ns != NONE {
                                push_rem(work, active, ns, n, ch, oo);
                            }
                        }
                    }
                }
                let e = intrinsic(p, ch, i, top_chunk);
                if e > 0 {
                    w.add[ch][e as usize].push(i as u16);
                }
            }
        }
        if mask != 0 {
            *ev.masks.entry(pos).or_default() |= mask;
        }
        if w.has_add() && !w.in_add {
            w.in_add = true;
            self.active_add.push(s);
        }
        self.work[su] = w;
    }

    /// Ends an event: works out what changed, compacts what was touched, and
    /// names the resident chunks to re-mesh.
    fn finish(&mut self) -> Vec<ChunkPos> {
        let mut ev = std::mem::take(&mut self.ev);
        if ev.logging {
            // Stable, so the first entry for a cell holds its value before the event.
            ev.log.sort_by_key(|&(s, i, _)| (s, i));
            let mut last = None;
            for &(s, i, old) in &ev.log {
                if last == Some((s, i)) {
                    continue;
                }
                last = Some((s, i));
                let slot = &self.slots[s as usize];
                if slot.light.get_index(i as usize).0 != old {
                    *ev.masks.entry(slot.pos).or_default() |= border_mask(i as usize);
                }
            }
        }
        for &s in &ev.touched {
            self.work[s as usize].touched = false;
            self.slots[s as usize].light.compact();
        }
        let mut out = HashSet::new();
        for (&pos, &mask) in &ev.masks {
            for k in 0..27 {
                if mask >> k & 1 == 0 {
                    continue;
                }
                let n = pos.offset(k % 3 - 1, k / 9 - 1, k / 3 % 3 - 1);
                if !self.shape.contains_chunk_y(n.y) {
                    continue;
                }
                let n = self.shape.wrap_chunk(n);
                if self.resident(n) {
                    out.insert(n);
                }
            }
        }
        // Keep the buffers' capacity for the next event.
        ev.log.clear();
        ev.masks.clear();
        ev.touched.clear();
        ev.logging = false;
        self.ev = ev;
        let mut out: Vec<ChunkPos> = out.into_iter().collect();
        out.sort_unstable();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockId, BlockRegistry};
    use rand::seq::SliceRandom;
    use rand::{Rng, SeedableRng, rngs::StdRng};
    use std::collections::VecDeque;

    type World = HashMap<ChunkPos, Chunk>;

    struct Ids {
        air: BlockId,
        stone: BlockId,
        glow: BlockId,
        water: BlockId,
        glass: BlockId,
    }

    fn setup() -> (BlockRegistry, Ids) {
        let r = BlockRegistry::builtin();
        let ids = Ids {
            air: BlockId::AIR,
            stone: r.id("stone").unwrap(),
            glow: r.id("glow_crystal").unwrap(),
            water: r.id("water").unwrap(),
            glass: r.id("glass").unwrap(),
        };
        (r, ids)
    }

    fn build(shape: PlanetShape, cols: &[(i32, i32)], f: impl Fn(i64, i64, i64) -> BlockId) -> World {
        let mut w = World::new();
        for &(cx, cz) in cols {
            for cy in 0..shape.height_chunks as i32 {
                let pos = ChunkPos::new(cx, cy, cz);
                let dense: Vec<BlockId> = (0..CHUNK_VOLUME)
                    .map(|i| {
                        let b = pos.block(LocalPos::from_index(i));
                        f(b.x, b.y, b.z)
                    })
                    .collect();
                w.insert(pos, Chunk::from_dense(&dense));
            }
        }
        w
    }

    fn all_columns(shape: PlanetShape) -> Vec<(i32, i32)> {
        let w = shape.width_chunks as i32;
        (0..w).flat_map(|x| (0..w).map(move |z| (x, z))).collect()
    }

    /// Hilly ground with caves, overhangs, ponds, glass and glow crystals.
    fn rough_world(shape: PlanetShape, ids: &Ids, cols: &[(i32, i32)], seed: u64) -> World {
        let mut rng = StdRng::seed_from_u64(seed);
        let feats: Vec<(i64, i64, i64, i64, BlockId)> = (0..60)
            .map(|_| {
                let kind = [ids.air, ids.air, ids.stone, ids.water, ids.glass][rng.random_range(0..5)];
                (
                    rng.random_range(0..shape.width_blocks()),
                    rng.random_range(4..shape.height_blocks() - 4),
                    rng.random_range(0..shape.width_blocks()),
                    rng.random_range(2..7),
                    kind,
                )
            })
            .collect();
        let glows: HashSet<(i64, i64, i64)> = (0..80)
            .map(|_| {
                (
                    rng.random_range(0..shape.width_blocks()),
                    rng.random_range(1..shape.height_blocks() - 1),
                    rng.random_range(0..shape.width_blocks()),
                )
            })
            .collect();
        let w = shape.width_blocks();
        build(shape, cols, |x, y, z| {
            if glows.contains(&(x, y, z)) {
                return ids.glow;
            }
            for &(fx, fy, fz, r, kind) in &feats {
                let d = |a: i64, b: i64| {
                    let d = (a - b).rem_euclid(w);
                    d.min(w - d)
                };
                if d(x, fx) <= r && (y - fy).abs() <= r / 2 && d(z, fz) <= r {
                    return kind;
                }
            }
            let h = 28 + ((x * 7 + z * 3) % 11) + ((x / 9 + z / 5) % 5);
            if y < h { ids.stone } else { ids.air }
        })
    }

    /// An independent solver for the rules in the module docs: plain
    /// relaxation over one dense array of the lit columns.
    fn reference(
        shape: PlanetShape,
        flags: &BlockFlags,
        world: &World,
        lit: &[(i32, i32)],
    ) -> HashMap<ChunkPos, ChunkLight> {
        let w = shape.width_blocks();
        let h = shape.height_blocks();
        let n = (w * w * h) as usize;
        let at = |x: i64, y: i64, z: i64| (x + z * w + y * w * w) as usize;
        let lit: HashSet<(i32, i32)> = lit.iter().copied().collect();
        let mut present = vec![false; n];
        let mut opaque = vec![false; n];
        let mut water = vec![false; n];
        let mut emit = vec![0u8; n];
        for (&pos, chunk) in world {
            if !lit.contains(&(pos.x, pos.z)) {
                continue;
            }
            for i in 0..CHUNK_VOLUME {
                let b = pos.block(LocalPos::from_index(i));
                let id = chunk.get_index(i);
                let k = at(b.x, b.y, b.z);
                present[k] = true;
                opaque[k] = flags.opaque(id);
                water[k] = !flags.opaque(id) && flags.translucent(id) && !flags.solid(id);
                emit[k] = flags.light(id);
            }
        }
        let mut out = HashMap::new();
        let mut levels = [vec![0u8; n], vec![0u8; n]];
        for (channel, level) in levels.iter_mut().enumerate() {
            let mut q = VecDeque::new();
            for k in 0..n {
                if !present[k] {
                    continue;
                }
                let y = k as i64 / (w * w);
                let v = if channel == 1 {
                    emit[k]
                } else if y == h - 1 && !opaque[k] {
                    if water[k] { 14 } else { 15 }
                } else {
                    0
                };
                if v > 0 {
                    level[k] = v;
                    q.push_back(k);
                }
            }
            while let Some(k) = q.pop_front() {
                let l = level[k];
                if l <= 1 {
                    continue;
                }
                let (x, y, z) = (k as i64 % w, k as i64 / (w * w), k as i64 / w % w);
                for (dx, dy, dz) in DIRS {
                    let ny = y + dy as i64;
                    if ny < 0 || ny >= h {
                        continue;
                    }
                    let nk = at((x + dx as i64).rem_euclid(w), ny, (z + dz as i64).rem_euclid(w));
                    if !present[nk] || opaque[nk] {
                        continue;
                    }
                    let falls = channel == 0 && dy == -1 && l == 15 && !water[nk];
                    let v = if falls { 15 } else { l - 1 };
                    if v > level[nk] {
                        level[nk] = v;
                        q.push_back(nk);
                    }
                }
            }
        }
        for &pos in world.keys() {
            if !lit.contains(&(pos.x, pos.z)) {
                continue;
            }
            let light = ChunkLight::from_fn(|i| {
                let b = pos.block(LocalPos::from_index(i));
                let k = at(b.x, b.y, b.z);
                Light::new(levels[0][k], levels[1][k])
            });
            out.insert(pos, light);
        }
        out
    }

    fn assert_matches(map: &LightMap, want: &HashMap<ChunkPos, ChunkLight>, world: &World) {
        for &pos in world.keys() {
            match (map.get(pos), want.get(&pos)) {
                (None, None) => {}
                (Some(a), Some(b)) if a == b => {}
                (Some(a), Some(b)) => {
                    let i = (0..CHUNK_VOLUME).find(|&i| a.get_index(i) != b.get_index(i)).unwrap();
                    panic!(
                        "chunk {pos:?} cell {:?}: got {:?}, want {:?}",
                        pos.block(LocalPos::from_index(i)),
                        a.get_index(i),
                        b.get_index(i)
                    );
                }
                (a, b) => panic!("chunk {pos:?}: lit {} but should be {}", a.is_some(), b.is_some()),
            }
        }
    }

    fn load(map: &mut LightMap, world: &World, order: &[ChunkPos]) {
        for &p in order {
            map.chunk_loaded(p, world);
        }
    }

    fn sorted_keys(world: &World) -> Vec<ChunkPos> {
        let mut k: Vec<ChunkPos> = world.keys().copied().collect();
        k.sort();
        k
    }

    /// The chunks whose padded light differs between two snapshots.
    fn expected_changes(shape: PlanetShape, before: &HashMap<ChunkPos, ChunkLight>, map: &LightMap) -> Vec<ChunkPos> {
        let mut out = HashSet::new();
        for (&pos, old) in before {
            let new = map.get(pos).unwrap();
            if old == new {
                continue;
            }
            for i in 0..CHUNK_VOLUME {
                if old.get_index(i) != new.get_index(i) {
                    let m = border_mask(i);
                    for k in 0..27 {
                        if m >> k & 1 != 0 {
                            let n = pos.offset(k % 3 - 1, k / 9 - 1, k / 3 % 3 - 1);
                            if shape.contains_chunk_y(n.y) {
                                out.insert(shape.wrap_chunk(n));
                            }
                        }
                    }
                }
            }
        }
        let mut v: Vec<ChunkPos> = out.into_iter().collect();
        v.sort();
        v
    }

    fn snapshot(map: &LightMap, world: &World) -> HashMap<ChunkPos, ChunkLight> {
        world.keys().map(|&p| (p, map.get(p).unwrap().clone())).collect()
    }

    #[test]
    fn incremental_edits_match_a_full_recompute() {
        let (r, ids) = setup();
        let flags = r.flags();
        let shape = PlanetShape::new(4, 2);
        let cols = all_columns(shape);
        let mut world = rough_world(shape, &ids, &cols, 7);
        let mut map = LightMap::new(shape, &flags);
        load(&mut map, &world, &sorted_keys(&world));
        assert_matches(&map, &reference(shape, &flags, &world, &cols), &world);

        let mut rng = StdRng::seed_from_u64(11);
        let kinds = [
            ids.air, ids.air, ids.air, ids.stone, ids.stone, ids.glow, ids.water, ids.glass,
        ];
        // Edits cluster on chunk borders and the seam, where the bugs live.
        let near_border = |rng: &mut StdRng, w: i64| -> i64 {
            let b: i64 = [0, 32, 64, 96][rng.random_range(0..4)];
            (b + rng.random_range(-3..3)).rem_euclid(w)
        };
        for n in 0..400 {
            let x = near_border(&mut rng, shape.width_blocks());
            let z = if rng.random_bool(0.5) {
                near_border(&mut rng, shape.width_blocks())
            } else {
                rng.random_range(0..shape.width_blocks())
            };
            let y = if rng.random_bool(0.3) {
                rng.random_range(29..35)
            } else {
                rng.random_range(1..60)
            };
            let p = BlockPos::new(x, y, z);
            let id = kinds[rng.random_range(0..kinds.len())];
            if world.get_mut(&p.chunk()).unwrap().set(p.local(), id) == id {
                continue;
            }
            let before = snapshot(&map, &world);
            let changed = map.block_changed(p, &world);
            assert_eq!(changed, expected_changes(shape, &before, &map), "edit {n} at {p:?}");
            if n % 40 == 39 {
                assert_matches(&map, &reference(shape, &flags, &world, &cols), &world);
            }
        }
        assert_matches(&map, &reference(shape, &flags, &world, &cols), &world);
    }

    #[test]
    fn load_order_does_not_matter() {
        let (r, ids) = setup();
        let flags = r.flags();
        let shape = PlanetShape::new(8, 2);
        // A 3 × 3 block of columns straddling the seam in X.
        let cols: Vec<(i32, i32)> = [7, 0, 1].iter().flat_map(|&x| [3, 4, 5].map(|z| (x, z))).collect();
        let world = rough_world(shape, &ids, &cols, 3);
        let want = reference(shape, &flags, &world, &cols);
        let mut rng = StdRng::seed_from_u64(5);
        let mut orders = vec![sorted_keys(&world)];
        orders.push(orders[0].iter().rev().copied().collect());
        for _ in 0..4 {
            let mut o = orders[0].clone();
            o.shuffle(&mut rng);
            orders.push(o);
        }
        for (k, order) in orders.iter().enumerate() {
            let mut map = LightMap::new(shape, &flags);
            for (j, &p) in order.iter().enumerate() {
                map.chunk_loaded(p, &world);
                // On the way, light must equal the truth for the columns
                // complete so far, never something left over from before.
                if k == 2 && j % 3 == 2 {
                    let complete: Vec<(i32, i32)> = cols
                        .iter()
                        .copied()
                        .filter(|&(x, z)| (0..2).all(|y| order[..=j].contains(&ChunkPos::new(x, y, z))))
                        .collect();
                    assert_matches(&map, &reference(shape, &flags, &world, &complete), &world);
                }
            }
            assert_matches(&map, &want, &world);
        }
    }

    #[test]
    fn forgetting_takes_back_borrowed_light() {
        let (r, ids) = setup();
        let flags = r.flags();
        let shape = PlanetShape::new(8, 2);
        let cols: Vec<(i32, i32)> = [7, 0, 1].iter().flat_map(|&x| [3, 4, 5].map(|z| (x, z))).collect();
        let world = rough_world(shape, &ids, &cols, 9);
        let mut map = LightMap::new(shape, &flags);
        load(&mut map, &world, &sorted_keys(&world));

        // The middle column goes; its neighbours lose what it lent them.
        let mut gone = world.clone();
        gone.remove(&ChunkPos::new(0, 1, 4));
        let changed = map.chunk_forgotten(ChunkPos::new(0, 1, 4), &gone);
        assert!(!changed.contains(&ChunkPos::new(0, 1, 4)));
        assert!(
            changed.contains(&ChunkPos::new(0, 0, 4)),
            "the rest of its column darkens"
        );
        assert!(changed.contains(&ChunkPos::new(1, 1, 4)));
        let rest: Vec<(i32, i32)> = cols.iter().copied().filter(|&c| c != (0, 4)).collect();
        assert_matches(&map, &reference(shape, &flags, &gone, &rest), &gone);

        // It comes back; so does the light.
        map.chunk_loaded(ChunkPos::new(0, 1, 4), &world);
        assert_matches(&map, &reference(shape, &flags, &world, &cols), &world);

        // A chunk sent again with new contents relights its column.
        let mut edited = world.clone();
        let c = edited.get_mut(&ChunkPos::new(1, 0, 3)).unwrap();
        for i in 0..CHUNK_VOLUME {
            if LocalPos::from_index(i).y > 20 {
                c.set(LocalPos::from_index(i), ids.air);
            }
        }
        map.chunk_loaded(ChunkPos::new(1, 0, 3), &edited);
        assert_matches(&map, &reference(shape, &flags, &edited, &cols), &edited);
    }

    #[test]
    fn light_crosses_the_seam() {
        let (r, ids) = setup();
        let flags = r.flags();
        let shape = PlanetShape::new(4, 1);
        let w = shape.width_blocks();
        // Solid rock with a sealed tunnel along X through the seam, lit by a
        // crystal at its east end and by a shaft to the sky west of the seam.
        let world = build(shape, &all_columns(shape), |x, y, z| {
            let tunnel = (y == 10 || y == 11) && z == 10 && (x >= w - 20 || x < 20);
            let shaft = x == w - 10 && z == 10 && y > 10;
            if x == w - 1 && y == 10 && z == 10 {
                ids.glow
            } else if tunnel || shaft {
                ids.air
            } else {
                ids.stone
            }
        });
        let mut map = LightMap::new(shape, &flags);
        load(&mut map, &world, &sorted_keys(&world));
        let at = |x: i64, y: i64| map.light_at(BlockPos::new(x, y, 10));
        assert_eq!(at(w - 1, 10).block(), 12);
        assert_eq!(at(0, 10).block(), 11);
        assert_eq!(at(1, 10).block(), 10);
        assert_eq!(at(5, 11).block(), 5);
        // Sky down the shaft at full strength, then 1 less per block along
        // the tunnel and over the seam.
        assert_eq!(at(w - 10, 11).sky(), 15);
        assert_eq!(at(w - 1, 11).sky(), 15 - 9);
        assert_eq!(at(0, 11).sky(), 15 - 10);
        assert_eq!(at(3, 11).sky(), 15 - 13);
        assert_matches(&map, &reference(shape, &flags, &world, &all_columns(shape)), &world);
    }

    #[test]
    fn a_sealed_cave_is_dark_until_opened() {
        let (r, ids) = setup();
        let flags = r.flags();
        let shape = PlanetShape::new(4, 2);
        let cols = all_columns(shape);
        let cave = |x: i64, y: i64, z: i64| (26..40).contains(&x) && (10..20).contains(&y) && (26..40).contains(&z);
        let mut world = build(
            shape,
            &cols,
            |x, y, z| {
                if cave(x, y, z) || y >= 40 { ids.air } else { ids.stone }
            },
        );
        let mut map = LightMap::new(shape, &flags);
        load(&mut map, &world, &sorted_keys(&world));
        for x in 26..40 {
            for y in 10..20 {
                for z in 26..40 {
                    assert_eq!(map.light_at(BlockPos::new(x, y, z)), Light::DARK);
                }
            }
        }
        // A shaft from the surface into the cave, dug from the top down.
        for y in (20..40).rev() {
            let p = BlockPos::new(31, y, 33);
            world.get_mut(&p.chunk()).unwrap().set(p.local(), ids.air);
            map.block_changed(p, &world);
        }
        assert_eq!(
            map.light_at(BlockPos::new(31, 10, 33)).sky(),
            15,
            "straight down the shaft"
        );
        assert_eq!(map.light_at(BlockPos::new(35, 10, 33)).sky(), 11);
        assert_eq!(map.light_at(BlockPos::new(26, 19, 26)).sky(), 15 - 5 - 7);
        assert_matches(&map, &reference(shape, &flags, &world, &cols), &world);
        // Sealed again, it goes dark again.
        let p = BlockPos::new(31, 30, 33);
        world.get_mut(&p.chunk()).unwrap().set(p.local(), ids.stone);
        map.block_changed(p, &world);
        assert_eq!(map.light_at(BlockPos::new(31, 10, 33)), Light::DARK);
        assert_matches(&map, &reference(shape, &flags, &world, &cols), &world);
    }

    #[test]
    fn emitters_fall_off_one_per_block() {
        let (r, ids) = setup();
        let flags = r.flags();
        let shape = PlanetShape::new(4, 2);
        let cols = all_columns(shape);
        let c = (32i64, 32i64, 32i64); // on a corner shared by eight chunks
        let mut world = build(shape, &cols, |x, y, z| {
            let inside = (14..50).contains(&x) && (14..50).contains(&y) && (14..50).contains(&z);
            if (x, y, z) == c {
                ids.glow
            } else if inside {
                ids.air
            } else {
                ids.stone
            }
        });
        let mut map = LightMap::new(shape, &flags);
        load(&mut map, &world, &sorted_keys(&world));
        let check = |map: &LightMap, e: i64| {
            for x in 14..50 {
                for y in 14..50 {
                    for z in 14..50 {
                        let d = (x - c.0).abs() + (y - c.1).abs() + (z - c.2).abs();
                        let l = map.light_at(BlockPos::new(x, y, z));
                        assert_eq!(l.block() as i64, (e - d).max(0), "at {x} {y} {z}");
                        assert_eq!(l.sky(), 0);
                    }
                }
            }
        };
        check(&map, 12);
        // Taken away, then put back by an edit.
        let p = BlockPos::new(c.0, c.1, c.2);
        world.get_mut(&p.chunk()).unwrap().set(p.local(), ids.air);
        map.block_changed(p, &world);
        check(&map, 0);
        world.get_mut(&p.chunk()).unwrap().set(p.local(), ids.glow);
        map.block_changed(p, &world);
        check(&map, 12);
    }

    #[test]
    fn water_dims_the_sky_and_glass_does_not() {
        let (r, ids) = setup();
        let flags = r.flags();
        let shape = PlanetShape::new(4, 2);
        let cols = all_columns(shape);
        let world = build(shape, &cols, |x, y, z| {
            if y < 20 {
                ids.stone
            } else if y < 40 && x < 64 {
                ids.water
            } else if y < 40 && (x, z) == (96, 96) {
                ids.glass
            } else if y < 40 {
                ids.stone
            } else {
                ids.air
            }
        });
        let mut map = LightMap::new(shape, &flags);
        load(&mut map, &world, &sorted_keys(&world));
        for y in 20..40 {
            let depth = 40 - y;
            assert_eq!(map.light_at(BlockPos::new(10, y, 10)).sky() as i64, (15 - depth).max(0));
            assert_eq!(map.light_at(BlockPos::new(96, y, 96)).sky(), 15);
        }
        assert_matches(&map, &reference(shape, &flags, &world, &cols), &world);

        // A sea deep enough that whole chunks are water, beside dry land.
        let shape = PlanetShape::new(4, 3);
        let cols = all_columns(shape);
        let world = build(shape, &cols, |x, y, _| {
            if y < 32 || (x >= 64 && y < 70) {
                ids.stone
            } else if y < 64 {
                ids.water
            } else {
                ids.air
            }
        });
        let mut map = LightMap::new(shape, &flags);
        load(&mut map, &world, &sorted_keys(&world));
        assert_eq!(map.light_at(BlockPos::new(10, 63, 10)).sky(), 14);
        assert_eq!(map.light_at(BlockPos::new(10, 50, 10)).sky(), 1);
        assert_eq!(map.light_at(BlockPos::new(10, 40, 10)), Light::DARK);
        assert_matches(&map, &reference(shape, &flags, &world, &cols), &world);
    }

    #[test]
    fn uniform_chunks_store_one_value() {
        let (r, ids) = setup();
        let flags = r.flags();
        let shape = PlanetShape::new(4, 3);
        let cols = all_columns(shape);
        let world = build(shape, &cols, |_, y, _| if y < 40 { ids.stone } else { ids.air });
        let mut map = LightMap::new(shape, &flags);
        load(&mut map, &world, &sorted_keys(&world));
        assert_eq!(
            map.get(ChunkPos::new(1, 2, 1)).unwrap().uniform_value(),
            Some(Light::OPEN_SKY)
        );
        assert_eq!(
            map.get(ChunkPos::new(1, 0, 1)).unwrap().uniform_value(),
            Some(Light::DARK)
        );
        assert_eq!(map.dense_chunks(), 16, "only the surface chunks");
        assert_eq!(map.lit_chunks(), 48);
    }

    #[test]
    fn incomplete_columns_stay_dark() {
        let (r, ids) = setup();
        let flags = r.flags();
        let shape = PlanetShape::new(4, 2);
        let world = build(
            shape,
            &[(0, 0), (1, 0)],
            |_, y, _| if y < 40 { ids.stone } else { ids.air },
        );
        let mut map = LightMap::new(shape, &flags);
        assert!(map.chunk_loaded(ChunkPos::new(0, 1, 0), &world).is_empty());
        assert!(!map.is_lit(ChunkPos::new(0, 1, 0)));
        assert_eq!(map.light_at(BlockPos::new(3, 50, 3)), Light::DARK);
        assert_eq!(
            map.light_at(BlockPos::new(3, 70, 3)),
            Light::OPEN_SKY,
            "above the planet"
        );
        let changed = map.chunk_loaded(ChunkPos::new(0, 0, 0), &world);
        assert_eq!(changed, vec![ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        assert_eq!(map.light_at(BlockPos::new(3, 50, 3)), Light::OPEN_SKY);
        // A neighbour that is resident but not lit is still told to re-mesh
        // when the light it samples at the border changes.
        map.chunk_loaded(ChunkPos::new(1, 1, 0), &world);
        let p = BlockPos::new(31, 45, 3);
        assert!(map.block_changed(p, &world).is_empty(), "nothing changed");
    }
}
