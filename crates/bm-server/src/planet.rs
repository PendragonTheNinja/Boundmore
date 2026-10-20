//! One planet of the universe (ADR-0006): its world, stations, storage and
//! claims, its look and day, and its warp shrine.
//!
//! The shrine stands at the generator's spawn point on a small flat plaza the
//! server cuts when the planet is created. Players arrive standing beside it,
//! inside a guarded box nobody may change or claim, so the arrival spot is
//! always open ground on a solid floor.

use crate::claims::Claims;
use crate::config::PlanetSpec;
use crate::crafting::Stations;
use crate::storage::Storages;
use crate::store::{SavedClaim, SavedPlanet, Store, WriteBatch};
use crate::world::World;
use anyhow::{Context, Result, bail};
use bm_core::{
    BlockId, BlockPos, BlockRegistry, BlockState, Chunk, ChunkPos, DayCycle, ItemRegistry, PlanetLook, PlanetShape,
    PlayerState, PlotPos, RecipeBook,
};
use bm_protocol::{PlanetInfo, PlanetSummary};
use bm_worldgen::{GENERATOR_VERSION, Generator, OreSpec, PlanetGenConfig, PlanetKinds};
use glam::DVec3;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

/// Half-width of the flat plaza cut around a new shrine.
pub const PLAZA_RADIUS: i64 = 6;
/// Air cleared above the plaza (trees, overhangs).
pub const PLAZA_CLEARANCE: i64 = 8;
/// Where arriving players stand, from the shrine (on the plaza, within the
/// default guard).
pub const ARRIVAL_OFFSET: (i64, i64) = (2, 0);

pub struct Planet {
    pub id: u16,
    pub name: String,
    /// The kind's name in `assets/planets.toml`.
    pub kind: String,
    /// The kind's display name.
    pub kind_display: String,
    pub tier: u8,
    pub look: PlanetLook,
    pub tints: Vec<[u8; 3]>,
    pub day: DayCycle,
    pub world: World,
    pub stations: Stations,
    /// Storage blocks with something in them.
    pub storage: Storages,
    pub claims: Claims,
    pub shrine: BlockPos,
    /// What the server built at the shrine, for telling it from what players
    /// built (a lapsing claim leaves the server's blocks alone).
    pub plaza: Plaza,
    pub guard: i64,
    /// Players on it now.
    pub players: usize,
}

impl Planet {
    pub fn info(&self) -> PlanetInfo {
        PlanetInfo {
            id: self.id,
            name: self.name.clone(),
            kind: self.kind_display.clone(),
            tier: self.tier,
            shape: self.world.shape,
            day: self.day,
            look: self.look.clone(),
            tints: self.tints.clone(),
        }
    }

    pub fn summary(&self) -> PlanetSummary {
        PlanetSummary {
            id: self.id,
            name: self.name.clone(),
            kind: self.kind_display.clone(),
            tier: self.tier,
            color: self.look.color,
            width_blocks: self.world.shape.width_blocks() as u32,
            players: self.players.min(u16::MAX as usize) as u16,
        }
    }

    /// Where a player arrives (and a new player starts): standing on the
    /// plaza beside the shrine.
    pub fn arrival(&self) -> PlayerState {
        arrival(self.world.shape, self.shrine)
    }

    /// What the plaza cut put at a block, if it is in the plaza.
    pub fn plaza_state(&self, p: BlockPos) -> Option<BlockState> {
        self.plaza.state(self.world.shape, p)
    }

    /// Whether a block is in the shrine's guarded box (nobody may change it).
    pub fn guarded(&self, p: BlockPos) -> bool {
        guarded(self.world.shape, self.shrine, self.guard, p)
    }
}

pub fn arrival(shape: PlanetShape, shrine: BlockPos) -> PlayerState {
    let (dx, dz) = ARRIVAL_OFFSET;
    PlayerState {
        pos: shape.wrap_point(DVec3::new(
            (shrine.x + dx) as f64 + 0.5,
            shrine.y as f64,
            (shrine.z + dz) as f64 + 0.5,
        )),
        on_ground: true,
        ..Default::default()
    }
}

/// The shrine and the plaza cut around it when the planet was made: the
/// floor one block below the shrine, air above, the shrine itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plaza {
    pub shrine: BlockPos,
    pub shrine_block: BlockId,
    pub floor: BlockId,
}

impl Plaza {
    /// For a planet whose shrine stands on terrain as generated (every
    /// planet made since shrines exist): the floor is the generated ground
    /// below it, or stone where that was not solid, as [`ensure_shrine`]
    /// chooses.
    pub fn new(generator: &Generator, registry: &BlockRegistry, shrine: BlockPos) -> Result<Self> {
        let below = shrine.offset(0, -1, 0);
        let ground = generator.generate(below.chunk()).get(below.local());
        let floor = if registry.get(ground).solid {
            ground
        } else {
            registry.id("stone")?
        };
        Ok(Self {
            shrine,
            shrine_block: registry.id("warp_shrine")?,
            floor,
        })
    }

    /// What the cut put at `p`, if `p` is in the plaza.
    pub fn state(&self, shape: PlanetShape, p: BlockPos) -> Option<BlockState> {
        let w = shape.width_blocks();
        let s = self.shrine;
        if wrapped(s.x, p.x, w).abs() > PLAZA_RADIUS || wrapped(s.z, p.z, w).abs() > PLAZA_RADIUS {
            return None;
        }
        if shape.wrap_block(p) == shape.wrap_block(s) {
            return Some(BlockState::plain(self.shrine_block));
        }
        match p.y - s.y {
            -1 => Some(BlockState::plain(self.floor)),
            dy if (0..PLAZA_CLEARANCE).contains(&dy) => Some(BlockState::AIR),
            _ => None,
        }
    }
}

/// Signed distance from `a` to `b` along a wrapping axis, the short way.
fn wrapped(a: i64, b: i64, w: i64) -> i64 {
    (b - a + w / 2).rem_euclid(w) - w / 2
}

/// The guarded box: `guard` blocks each way across and down from the shrine,
/// twice that up. A guard of 0 is the shrine alone.
pub fn guarded(shape: PlanetShape, shrine: BlockPos, guard: i64, p: BlockPos) -> bool {
    let w = shape.width_blocks();
    let dy = p.y - shrine.y;
    wrapped(shrine.x, p.x, w).abs() <= guard
        && wrapped(shrine.z, p.z, w).abs() <= guard
        && (-guard..=2 * guard).contains(&dy)
}

/// Plots touching the guarded box (none for a guard of 0: the shrine alone
/// needs no claim kept off it, being unbreakable).
pub fn guarded_plots(shape: PlanetShape, shrine: BlockPos, guard: i64) -> BTreeSet<PlotPos> {
    let mut out = BTreeSet::new();
    if guard == 0 {
        return out;
    }
    for dy in -guard..=2 * guard {
        if !shape.contains_y(shrine.y + dy) {
            continue;
        }
        for dz in -guard..=guard {
            for dx in -guard..=guard {
                out.insert(PlotPos::of(shrine.offset(dx, dy, dz), shape));
            }
        }
    }
    out
}

/// A new planet's record from its spec. Days differ between planets: each
/// has its own length and starts at another hour.
pub fn new_record(id: u16, spec: &PlanetSpec, height_chunks: u32, kinds: &PlanetKinds) -> Result<SavedPlanet> {
    let kind = kinds
        .get(&spec.kind)
        .with_context(|| format!("planet {}: no planet kind `{}`", spec.name, spec.kind))?;
    let base = DayCycle::default();
    let day = if id == 0 {
        base
    } else {
        const LENGTHS: [f64; 4] = [1.0, 0.75, 1.5, 1.25];
        let day_ticks = (base.day_ticks as f64 * LENGTHS[id as usize % 4]) as u64;
        let phase = (0.3 + 0.37 * id as f64).fract();
        DayCycle {
            day_ticks,
            start_offset: (day_ticks as f64 * phase) as u64,
        }
    };
    Ok(SavedPlanet {
        generator_version: GENERATOR_VERSION,
        name: spec.name.clone(),
        kind: kind.name.clone(),
        shape: PlanetShape::new(spec.width_chunks, height_chunks),
        generation: kind.generation(spec.seed),
        day,
        shrine: None,
    })
}

/// Everything shared by every planet that opening one needs.
pub struct Catalogue<'a> {
    pub registry: &'a Arc<BlockRegistry>,
    pub items: &'a ItemRegistry,
    pub recipes: &'a RecipeBook,
    pub kinds: &'a PlanetKinds,
}

/// Opens planet `id`: its saved record (created from `spec` if new), its
/// shrine (placed if missing), stations and claims.
pub fn open(
    store: &Store,
    id: u16,
    spec: &PlanetSpec,
    height_chunks: u32,
    guard: u32,
    cat: &Catalogue,
    claims: Vec<(u32, SavedClaim)>,
) -> Result<Planet> {
    let mut saved = match store.planet(id)? {
        Some(s) => s,
        None => {
            let s = new_record(id, spec, height_chunks, cat.kinds)?;
            store.put_planet(id, &s)?;
            tracing::info!("created planet {} ({}), seed {}", s.name, s.kind, s.generation.seed);
            s
        }
    };
    // Version 2 planets generate the same terrain under version 3 (no colours
    // saved means natural everywhere), so they open and are marked current.
    if bm_worldgen::opens_generator_version(saved.generator_version) && saved.generator_version != GENERATOR_VERSION {
        tracing::info!(
            "planet `{}` was generated by generator v{}; it opens unchanged as v{GENERATOR_VERSION}",
            saved.name,
            saved.generator_version
        );
        saved.generator_version = GENERATOR_VERSION;
        store.put_planet(id, &saved)?;
    }
    if saved.generator_version != GENERATOR_VERSION {
        bail!(
            "planet `{}` was generated by generator v{}, this server is v{}; it cannot be opened",
            saved.name,
            saved.generator_version,
            GENERATOR_VERSION
        );
    }
    if saved.name != spec.name {
        tracing::info!(
            "planet {id} is `{}` (saved), not `{}` (config): the saved name wins",
            saved.name,
            spec.name
        );
    }
    if let Some(kind) = cat.kinds.get(&saved.kind) {
        let kind_ores = kind.generation(saved.generation.seed).ores;
        let added = extend_ores(&mut saved.generation, &kind_ores);
        if !added.is_empty() {
            store.put_planet(id, &saved)?;
            tracing::info!("{}: {} added to its ores", saved.name, added.join(", "));
        }
    }
    saved
        .generation
        .check(cat.registry)
        .map_err(|e| anyhow::anyhow!("planet {}: {e}", saved.name))?;
    let generator = Arc::new(Generator::new(saved.shape, saved.generation.clone(), cat.registry)?);
    let shrine = ensure_shrine(store, id, &mut saved, &generator, cat.registry)?;
    let plaza = Plaza::new(&generator, cat.registry, shrine)?;
    let kind = cat.kinds.get(&saved.kind);
    if kind.is_none() {
        tracing::warn!(
            "planet {}: kind `{}` is no longer in planets.toml; drawn with the default look",
            saved.name,
            saved.kind
        );
    }
    let look = kind.map(|k| k.look.clone()).unwrap_or_default();
    let tints = look
        .resolve_tints(cat.registry)
        .map_err(|e| anyhow::anyhow!("planet {}: {e}", saved.name))?;
    let stations = Stations::load(store.stations(id)?, cat.items, cat.recipes);
    let storage = Storages::load(store.storage(id)?, cat.items);
    let mut claims = Claims::load(id, saved.shape, claims);
    let guard = guard as i64;
    claims.reserve(guarded_plots(saved.shape, shrine, guard));
    tracing::info!(
        "planet {id} {} | {} | {}×{} blocks, {} high | seed {} | shrine at {:?} | {} edited chunks on disk",
        saved.name,
        saved.kind,
        saved.shape.width_blocks(),
        saved.shape.width_blocks(),
        saved.shape.height_blocks(),
        saved.generation.seed,
        shrine,
        store.stored_chunks(id)?
    );
    Ok(Planet {
        id,
        kind_display: kind.map_or_else(|| bm_core::items::title_case(&saved.kind), |k| k.display.clone()),
        tier: kind.map_or(1, |k| k.tier),
        name: saved.name,
        kind: saved.kind,
        look,
        tints,
        day: saved.day,
        world: World::new(id, saved.shape, cat.registry.clone(), generator, store.clone()),
        stations,
        storage,
        claims,
        shrine,
        plaza,
        guard,
        players: 0,
    })
}

/// Appends the ores a planet's kind lists after the planet's own saved ores
/// (ADR-0009: M8's wayshards), when the saved list is a strict prefix of the
/// kind's. Ores are tried in order and the first match wins, so earlier ores
/// stay exactly where they were; only rock no earlier ore claimed can become
/// a new ore, and only in chunks never saved. Never reorders or removes: a
/// list that is not a prefix is left alone. Returns the blocks appended.
pub fn extend_ores(generation: &mut PlanetGenConfig, kind_ores: &[OreSpec]) -> Vec<String> {
    let n = generation.ores.len();
    if kind_ores.len() <= n || kind_ores[..n] != generation.ores[..] {
        return Vec::new();
    }
    let new = &kind_ores[n..];
    generation.ores.extend_from_slice(new);
    new.iter().map(|o| o.block.clone()).collect()
}

/// Makes sure the planet's shrine stands, placing it (and cutting its plaza)
/// if not. Idempotent: a standing shrine is left alone. Runs before the game
/// starts, writing straight to the store, so no chunk stays resident.
///
/// The plaza only changes chunks nobody has edited: on an old planet whose
/// spawn players built on, the shrine takes its one block and the builds
/// around it stay.
pub fn ensure_shrine(
    store: &Store,
    id: u16,
    saved: &mut SavedPlanet,
    generator: &Generator,
    registry: &BlockRegistry,
) -> Result<BlockPos> {
    let shape = saved.shape;
    let block = registry.id("warp_shrine")?;
    let shrine = shape.wrap_block(saved.shrine.unwrap_or_else(|| generator.spawn_point()));
    let mut plaza = PlazaCut {
        store,
        id,
        generator,
        shape,
        chunks: HashMap::new(),
    };
    if plaza.get(shrine)? != block {
        let ground = plaza.get(shrine.offset(0, -1, 0))?;
        let floor = if registry.get(ground).solid {
            ground
        } else {
            registry.id("stone")?
        };
        for dz in -PLAZA_RADIUS..=PLAZA_RADIUS {
            for dx in -PLAZA_RADIUS..=PLAZA_RADIUS {
                plaza.set(shrine.offset(dx, -1, dz), floor, false)?;
                for dy in 0..PLAZA_CLEARANCE {
                    plaza.set(shrine.offset(dx, dy, dz), BlockId::AIR, false)?;
                }
            }
        }
        // The arrival spot is open whatever was built there.
        let (ax, az) = ARRIVAL_OFFSET;
        plaza.set(shrine.offset(ax, -1, az), floor, true)?;
        plaza.set(shrine.offset(ax, 0, az), BlockId::AIR, true)?;
        plaza.set(shrine.offset(ax, 1, az), BlockId::AIR, true)?;
        plaza.set(shrine, block, true)?;
        let batch = WriteBatch {
            chunks: plaza
                .chunks
                .iter()
                .filter(|(_, e)| e.changed)
                .map(|(p, e)| (id, *p, e.chunk.encode()))
                .collect(),
            ..Default::default()
        };
        store.write(&batch)?;
        tracing::info!("placed the warp shrine of planet {} at {shrine:?}", saved.name);
    }
    if saved.shrine != Some(shrine) {
        saved.shrine = Some(shrine);
        store.put_planet(id, saved)?;
    }
    Ok(shrine)
}

struct PlazaChunk {
    chunk: Chunk,
    /// Saved before: a player changed it.
    edited: bool,
    changed: bool,
}

/// The chunks around a shrine being placed.
struct PlazaCut<'a> {
    store: &'a Store,
    id: u16,
    generator: &'a Generator,
    shape: PlanetShape,
    chunks: HashMap<ChunkPos, PlazaChunk>,
}

impl PlazaCut<'_> {
    fn entry(&mut self, p: BlockPos) -> Result<&mut PlazaChunk> {
        let c = p.chunk();
        if !self.chunks.contains_key(&c) {
            let e = match self.store.chunk(self.id, c)? {
                Some(bytes) => PlazaChunk {
                    chunk: Chunk::decode(&bytes)
                        .map_err(|e| anyhow::anyhow!("saved chunk {c:?} of planet {}: {e}", self.id))?,
                    edited: true,
                    changed: false,
                },
                None => PlazaChunk {
                    chunk: self.generator.generate(c),
                    edited: false,
                    changed: false,
                },
            };
            self.chunks.insert(c, e);
        }
        Ok(self.chunks.get_mut(&c).expect("inserted"))
    }

    fn get(&mut self, p: BlockPos) -> Result<BlockId> {
        if !self.shape.contains_y(p.y) {
            return Ok(BlockId::AIR);
        }
        let p = self.shape.wrap_block(p);
        Ok(self.entry(p)?.chunk.get(p.local()))
    }

    /// Sets a block; in a chunk a player edited, only if `always`.
    fn set(&mut self, p: BlockPos, b: BlockId, always: bool) -> Result<()> {
        if !self.shape.contains_y(p.y) {
            return Ok(());
        }
        let p = self.shape.wrap_block(p);
        let e = self.entry(p)?;
        if (always || !e.edited) && e.chunk.get(p.local()) != b {
            e.chunk.set(p.local(), b);
            e.changed = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guard_wraps_and_reaches_up_twice_as_far() {
        let shape = PlanetShape::new(8, 4);
        let s = BlockPos::new(1, 50, 255);
        assert!(guarded(shape, s, 4, BlockPos::new(253, 46, 3)), "across the seam");
        assert!(guarded(shape, s, 4, BlockPos::new(1, 58, 255)));
        assert!(!guarded(shape, s, 4, BlockPos::new(1, 59, 255)));
        assert!(!guarded(shape, s, 4, BlockPos::new(1, 45, 255)));
        assert!(!guarded(shape, s, 4, BlockPos::new(6, 50, 255)));
        assert!(guarded(shape, s, 0, s) && !guarded(shape, s, 0, s.offset(1, 0, 0)));
        assert!(guarded_plots(shape, s, 0).is_empty());
        let plots = guarded_plots(shape, s, 4);
        assert!(plots.contains(&PlotPos::of(BlockPos::new(253, 46, 3), shape)));
        assert!(plots.iter().all(|p| *p == p.wrap(shape)));
    }

    #[test]
    fn new_ores_are_only_ever_appended() {
        let registry = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&registry);
        let kind = kinds.get("arid").unwrap().generation(5);
        assert_eq!(kind.ores.last().unwrap().block, "wayshard_vein");
        // A planet made before wayshards gains them, once.
        let mut old = kind.clone();
        old.ores.pop();
        assert_eq!(extend_ores(&mut old, &kind.ores), vec!["wayshard_vein".to_string()]);
        assert_eq!(old, kind);
        assert!(extend_ores(&mut old, &kind.ores).is_empty(), "already there");
        // A list that is not a prefix (reordered, or an ore of its own) is left alone.
        let mut odd = kind.clone();
        odd.ores.truncate(1);
        odd.ores[0].threshold += 0.01;
        let before = odd.clone();
        assert!(extend_ores(&mut odd, &kind.ores).is_empty());
        assert_eq!(odd, before);
        // A longer saved list is never cut.
        let mut more = kind.clone();
        more.ores.push(kind.ores[0].clone());
        assert!(extend_ores(&mut more, &kind.ores).is_empty());
        assert_eq!(more.ores.len(), kind.ores.len() + 1);
    }

    #[test]
    fn an_old_planet_gains_wayshards_on_opening() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("u.redb")).unwrap();
        let registry = Arc::new(BlockRegistry::builtin());
        let kinds = PlanetKinds::builtin(&registry);
        let items = ItemRegistry::builtin(&registry);
        let recipes = RecipeBook::builtin(&items);
        let spec = PlanetSpec::new("Old", "temperate", 3, 16);
        let mut saved = new_record(0, &spec, 12, &kinds).unwrap();
        let full = saved.generation.ores.clone();
        saved.generation.ores.pop();
        store.put_planet(0, &saved).unwrap();
        let cat = Catalogue {
            registry: &registry,
            items: &items,
            recipes: &recipes,
            kinds: &kinds,
        };
        open(&store, 0, &spec, 12, 4, &cat, Vec::new()).unwrap();
        assert_eq!(store.planet(0).unwrap().unwrap().generation.ores, full, "saved");
    }

    #[test]
    fn a_shrine_is_placed_once_on_a_plaza() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("u.redb")).unwrap();
        let registry = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&registry);
        let mut saved = new_record(1, &PlanetSpec::new("A", "temperate", 3, 16), 12, &kinds).unwrap();
        let g = Generator::new(saved.shape, saved.generation.clone(), &registry).unwrap();
        let s = ensure_shrine(&store, 1, &mut saved, &g, &registry).unwrap();
        assert_eq!(saved.shrine, Some(s));
        assert_eq!(store.planet(1).unwrap().unwrap().shrine, Some(s));
        let stored = store.stored_chunks(1).unwrap();
        assert!(stored > 0);
        let shape = saved.shape;
        let block_at = |p: BlockPos| {
            let p = shape.wrap_block(p);
            let bytes = store.chunk(1, p.chunk()).unwrap();
            bytes
                .map_or_else(|| g.generate(p.chunk()), |b| Chunk::decode(&b).unwrap())
                .get(p.local())
        };
        assert_eq!(block_at(s), registry.id("warp_shrine").unwrap());
        let a = arrival(saved.shape, s);
        let feet = BlockPos::containing(a.pos.x, a.pos.y, a.pos.z);
        assert!(registry.get(block_at(feet.offset(0, -1, 0))).solid, "stands on ground");
        assert!(
            block_at(feet).is_air() && block_at(feet.offset(0, 1, 0)).is_air(),
            "room to stand"
        );
        // Again: nothing changes.
        ensure_shrine(&store, 1, &mut saved, &g, &registry).unwrap();
        assert_eq!(store.stored_chunks(1).unwrap(), stored);

        // The plaza as remembered is the plaza as cut.
        let plaza = Plaza::new(&g, &registry, s).unwrap();
        for dy in -3..PLAZA_CLEARANCE + 2 {
            for dz in -PLAZA_RADIUS - 1..=PLAZA_RADIUS + 1 {
                for dx in -PLAZA_RADIUS - 1..=PLAZA_RADIUS + 1 {
                    let p = s.offset(dx, dy, dz);
                    let inside =
                        dx.abs() <= PLAZA_RADIUS && dz.abs() <= PLAZA_RADIUS && (-1..PLAZA_CLEARANCE).contains(&dy);
                    match plaza.state(shape, p) {
                        Some(st) => {
                            assert!(inside, "{dx} {dy} {dz}");
                            assert_eq!(st.id, block_at(p), "{dx} {dy} {dz}");
                        }
                        None => assert!(!inside, "{dx} {dy} {dz}"),
                    }
                }
            }
        }
        // Across the seam too.
        let lap = s.offset(shape.width_blocks(), 0, 0);
        assert_eq!(
            plaza.state(shape, lap).map(|st| st.id),
            Some(registry.id("warp_shrine").unwrap())
        );
    }
}
