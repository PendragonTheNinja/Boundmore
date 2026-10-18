//! Planet generation.
//!
//! Every chunk is a pure function of `(seed, planet shape, config, chunk
//! position)`: any chunk can be generated alone, in any order, on any thread,
//! with no neighbours. Features that cross chunk borders (trees, caves) are
//! decided from world-aligned functions that every chunk evaluates the same
//! way, so a tree cut by a chunk border is still one tree.
//!
//! Generation runs on the **server only** (ADR-0003): ore placement is part of
//! the economy, and a client that could generate terrain could see every vein
//! on the planet.

pub mod noise;

use bm_core::{
    BlockId, BlockPos, BlockRegistry, CHUNK_SIZE, CHUNK_VOLUME, Chunk, ChunkPos, LocalPos, PlanetShape, RegistryError,
};
use noise::{Fbm, hash01};
use serde::{Deserialize, Serialize};

/// Bump on ANY change to what the generator produces for a given seed, and
/// re-pin `terrain_fingerprint_is_pinned` in the same commit. A server refuses
/// to open a planet made by another version (its edited chunks were saved
/// against terrain that would no longer be there).
pub const GENERATOR_VERSION: u32 = 1;

/// Per-planet generation settings, saved with the planet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanetGenConfig {
    pub seed: u64,
    pub sea_level: i64,
    /// Height above sea level where turf gives way to snow.
    pub snow_line: i64,
    /// Peak mountain height above the land base.
    pub mountain_height: f64,
    /// Fraction of suitable columns that grow a tree.
    pub tree_density: f64,
}

impl Default for PlanetGenConfig {
    fn default() -> Self {
        Self {
            seed: 1,
            sea_level: 128,
            snow_line: 112,
            mountain_height: 130.0,
            tree_density: 0.012,
        }
    }
}

struct Ids {
    air: BlockId,
    bedrock: BlockId,
    stone: BlockId,
    deep_stone: BlockId,
    soil: BlockId,
    turf: BlockId,
    sand: BlockId,
    gravel: BlockId,
    snow: BlockId,
    water: BlockId,
    trunk: BlockId,
    foliage: BlockId,
    coal: BlockId,
    copper: BlockId,
    glow: BlockId,
}

impl Ids {
    fn resolve(r: &BlockRegistry) -> Result<Self, RegistryError> {
        Ok(Self {
            air: r.id("air")?,
            bedrock: r.id("bedrock")?,
            stone: r.id("stone")?,
            deep_stone: r.id("deep_stone")?,
            soil: r.id("soil")?,
            turf: r.id("turf")?,
            sand: r.id("sand")?,
            gravel: r.id("gravel")?,
            snow: r.id("snow")?,
            water: r.id("water")?,
            trunk: r.id("trunk")?,
            foliage: r.id("foliage")?,
            coal: r.id("coal_seam")?,
            copper: r.id("copper_vein")?,
            glow: r.id("glow_crystal")?,
        })
    }
}

/// Spacing of the world-aligned lattice that caves and ores are sampled on and
/// interpolated from. Divides 32, so chunk corners land on lattice points and
/// the interpolated value is a pure function of world position.
const COARSE: i64 = 4;
const COARSE_N: usize = (CHUNK_SIZE as i64 / COARSE) as usize + 1;
/// How far outside a chunk a tree's trunk may stand and still reach into it.
const TREE_REACH: i64 = 3;
const TREE_MAX_HEIGHT: i64 = 11;

pub struct Generator {
    shape: PlanetShape,
    cfg: PlanetGenConfig,
    ids: Ids,
    width: f64,
    continent: Fbm,
    hills: Fbm,
    ridges: Fbm,
    mountain_mask: Fbm,
    forest: Fbm,
    caverns: Fbm,
    tunnel_a: Fbm,
    tunnel_b: Fbm,
    coal: Fbm,
    copper: Fbm,
}

/// Cave and ore values at one lattice point.
#[derive(Clone, Copy, Default)]
struct Coarse {
    cavern: f64,
    tunnel: f64,
    coal: f64,
    copper: f64,
}

type Corners = [[[Coarse; 2]; 2]; 2];

impl Generator {
    pub fn new(shape: PlanetShape, cfg: PlanetGenConfig, registry: &BlockRegistry) -> Result<Self, RegistryError> {
        let w = shape.width_blocks();
        let s = cfg.seed;
        let k = |n: u64| s.wrapping_mul(0x2545_f491_4f6c_dd1d).wrapping_add(n << 12);
        Ok(Self {
            shape,
            ids: Ids::resolve(registry)?,
            width: w as f64,
            continent: Fbm::new(k(1), w, 700.0, 3, 0.5),
            hills: Fbm::new(k(2), w, 160.0, 4, 0.5),
            ridges: Fbm::new(k(3), w, 420.0, 5, 0.5),
            mountain_mask: Fbm::new(k(4), w, 900.0, 2, 0.5),
            forest: Fbm::new(k(5), w, 250.0, 2, 0.5),
            caverns: Fbm::new(k(6), w, 64.0, 2, 0.5),
            tunnel_a: Fbm::new(k(7), w, 96.0, 1, 0.5),
            tunnel_b: Fbm::new(k(8), w, 96.0, 1, 0.5),
            coal: Fbm::new(k(9), w, 14.0, 1, 0.5),
            copper: Fbm::new(k(10), w, 12.0, 1, 0.5),
            cfg,
        })
    }

    pub fn shape(&self) -> PlanetShape {
        self.shape
    }

    pub fn config(&self) -> &PlanetGenConfig {
        &self.cfg
    }

    /// Y of the topmost terrain block in a column (before caves and trees).
    pub fn surface_height(&self, x: i64, z: i64) -> i64 {
        let (x, z) = (x as f64, z as f64);
        let w = self.width;
        let c = self.continent.sample2(x, z, w);
        let land = smoothstep(-0.30, 0.05, c);
        let hills = self.hills.sample2(x, z, w);
        let ridge = self.ridges.ridged2(x, z, w);
        let mask = smoothstep(-0.10, 0.30, self.mountain_mask.sample2(x, z, w));
        let sea = self.cfg.sea_level as f64;
        let h = sea - 24.0
            + land * 30.0
            + hills * 14.0 * (0.4 + land)
            + mask * land * ridge * ridge * self.cfg.mountain_height;
        (h.floor() as i64).clamp(4, self.shape.height_blocks() - TREE_MAX_HEIGHT - 4)
    }

    fn coarse_at(&self, x: i64, y: i64, z: i64) -> Coarse {
        let (fx, fy, fz) = (x as f64, y as f64, z as f64);
        let w = self.width;
        let a = self.tunnel_a.sample3(fx, fy * 1.4, fz, w);
        let b = self.tunnel_b.sample3(fx, fy * 1.4, fz, w);
        Coarse {
            cavern: self.caverns.sample3(fx, fy * 1.6, fz, w),
            tunnel: a.abs().max(b.abs()),
            coal: self.coal.sample3(fx, fy, fz, w),
            copper: self.copper.sample3(fx, fy, fz, w),
        }
    }

    fn is_cave(c: &Coarse) -> bool {
        c.cavern > 0.42 || c.tunnel < 0.055
    }

    /// Interpolated cave/ore values at any block, from the world-aligned
    /// lattice. Used for single lookups (tree footings, spawn); chunk
    /// generation computes the same values in bulk.
    fn coarse_interp(&self, x: i64, y: i64, z: i64) -> Coarse {
        let snap = |v: i64| v.div_euclid(COARSE) * COARSE;
        let (x0, y0, z0) = (snap(x), snap(y), snap(z));
        let mut c: Corners = Default::default();
        for (i, cx) in c.iter_mut().enumerate() {
            for (j, cy) in cx.iter_mut().enumerate() {
                for (k, v) in cy.iter_mut().enumerate() {
                    *v = self.coarse_at(x0 + i as i64 * COARSE, y0 + j as i64 * COARSE, z0 + k as i64 * COARSE);
                }
            }
        }
        let t = |v: i64, v0: i64| (v - v0) as f64 / COARSE as f64;
        trilerp(&c, t(x, x0), t(y, y0), t(z, z0))
    }

    /// Generates one chunk. Positions off the canonical lap are wrapped first.
    pub fn generate(&self, pos: ChunkPos) -> Chunk {
        let pos = self.shape.wrap_chunk(pos);
        if !self.shape.contains_chunk_y(pos.y) {
            return Chunk::empty();
        }
        let origin = pos.origin();
        let ids = &self.ids;
        let sea = self.cfg.sea_level;
        let cs = CHUNK_SIZE as i64;

        // Column heights for the chunk plus a margin: the margin feeds slope
        // tests at the border and trees whose canopy reaches in.
        let m = TREE_REACH + 1;
        let span = (cs + 2 * m) as usize;
        let mut heights = vec![0i64; span * span];
        let mut max_h = i64::MIN;
        for dz in 0..span {
            for dx in 0..span {
                let h = self.surface_height(origin.x + dx as i64 - m, origin.z + dz as i64 - m);
                heights[dz * span + dx] = h;
                max_h = max_h.max(h);
            }
        }
        let height_at = |lx: i64, lz: i64| heights[((lz + m) as usize) * span + (lx + m) as usize];

        // Sky fast path: most chunks on a planet are empty air.
        if origin.y > max_h + TREE_MAX_HEIGHT && origin.y > sea {
            return Chunk::empty();
        }

        // Cave and ore lattice, only if some of the chunk is underground.
        let underground = origin.y <= max_h;
        let mut lattice = vec![Coarse::default(); COARSE_N * COARSE_N * COARSE_N];
        if underground {
            for j in 0..COARSE_N {
                for k in 0..COARSE_N {
                    for i in 0..COARSE_N {
                        lattice[(j * COARSE_N + k) * COARSE_N + i] = self.coarse_at(
                            origin.x + i as i64 * COARSE,
                            origin.y + j as i64 * COARSE,
                            origin.z + k as i64 * COARSE,
                        );
                    }
                }
            }
        }
        let lattice_at = |lx: i64, ly: i64, lz: i64| -> Coarse {
            let (i, j, k) = ((lx / COARSE) as usize, (ly / COARSE) as usize, (lz / COARSE) as usize);
            let g = |di: usize, dj: usize, dk: usize| lattice[((j + dj) * COARSE_N + k + dk) * COARSE_N + i + di];
            let c: Corners = [
                [[g(0, 0, 0), g(0, 0, 1)], [g(0, 1, 0), g(0, 1, 1)]],
                [[g(1, 0, 0), g(1, 0, 1)], [g(1, 1, 0), g(1, 1, 1)]],
            ];
            let t = |v: i64| (v % COARSE) as f64 / COARSE as f64;
            trilerp(&c, t(lx), t(ly), t(lz))
        };

        let seed = self.cfg.seed;
        let top = origin.y + cs - 1;
        let mut blocks = vec![ids.air; CHUNK_VOLUME];
        for lz in 0..cs {
            for lx in 0..cs {
                let h = height_at(lx, lz);
                let (wx, wz) = (origin.x + lx, origin.z + lz);
                let slope = slope_at(&height_at, lx, lz);
                let surface = self.surface_block(h, slope, wx, wz);
                let deep_line = 60 + (hash01(seed ^ 0xdee9, wx, 0, wz) * 5.0) as i64;
                let y_end = top.min(h.max(sea));
                for wy in origin.y..=y_end {
                    let ly = wy - origin.y;
                    let mut b = if wy == 0 || (wy <= 2 && hash01(seed ^ 0xbed, wx, wy, wz) < 0.6 / wy as f64) {
                        ids.bedrock
                    } else if wy > h {
                        ids.water // y_end stops this at sea level
                    } else if wy == h {
                        surface
                    } else if wy > h - 4 && surface != ids.stone {
                        if surface == ids.turf || surface == ids.snow {
                            ids.soil
                        } else {
                            surface
                        }
                    } else if wy < deep_line {
                        ids.deep_stone
                    } else {
                        ids.stone
                    };
                    // Never open a cave into the sea floor: static water would
                    // stand as a wall over the hole.
                    let carveable = b != ids.bedrock && b != ids.water && (h >= sea || wy < h - 6);
                    if underground && carveable {
                        let c = lattice_at(lx, ly, lz);
                        if Self::is_cave(&c) {
                            b = ids.air;
                        } else if b == ids.stone || b == ids.deep_stone {
                            if wy > 12 && wy < h - 5 && c.coal > 0.52 {
                                b = ids.coal;
                            } else if wy < 150 && c.copper > 0.58 {
                                b = ids.copper;
                            } else if wy < sea - 24
                                && ly < cs - 1
                                && Self::is_cave(&lattice_at(lx, ly + 1, lz))
                                && hash01(seed ^ 0x610, wx, wy, wz) < 0.02
                            {
                                b = ids.glow;
                            }
                        }
                    }
                    blocks[LocalPos::new(lx as u8, ly as u8, lz as u8).index()] = b;
                }
            }
        }

        self.place_trees(pos, &mut blocks, &height_at);
        Chunk::from_dense(&blocks)
    }

    fn surface_block(&self, h: i64, slope: i64, wx: i64, wz: i64) -> BlockId {
        let ids = &self.ids;
        let sea = self.cfg.sea_level;
        if h < sea - 3 {
            if hash01(self.cfg.seed ^ 0x9a7e1, wx, 0, wz) < 0.5 {
                ids.gravel
            } else {
                ids.sand
            }
        } else if h <= sea + 1 {
            ids.sand
        } else if slope >= 3 {
            ids.stone
        } else if h >= sea + self.cfg.snow_line {
            ids.snow
        } else {
            ids.turf
        }
    }

    /// Whether a tree grows from canonical column `(x, z)`, and its trunk height.
    fn tree_at(&self, x: i64, z: i64, h: i64) -> Option<i64> {
        let sea = self.cfg.sea_level;
        if h < sea + 2 || h >= sea + self.cfg.snow_line - 12 {
            return None;
        }
        let forest = smoothstep(-0.2, 0.4, self.forest.sample2(x as f64, z as f64, self.width));
        let density = self.cfg.tree_density * (0.15 + 2.0 * forest);
        if hash01(self.cfg.seed ^ 0x7ee, x, 0, z) >= density {
            return None;
        }
        // The footing must be the surface itself, not the roof of a cave mouth.
        if Self::is_cave(&self.coarse_interp(x, h, z)) || Self::is_cave(&self.coarse_interp(x, h - 1, z)) {
            return None;
        }
        Some(5 + (hash01(self.cfg.seed ^ 0x7ee2, x, 0, z) * 4.0) as i64)
    }

    fn place_trees(&self, pos: ChunkPos, blocks: &mut [BlockId], height_at: &impl Fn(i64, i64) -> i64) {
        let ids = &self.ids;
        let origin = pos.origin();
        let w = self.shape.width_blocks();
        let cs = CHUNK_SIZE as i64;
        let seed = self.cfg.seed;
        for lz in -TREE_REACH..cs + TREE_REACH {
            for lx in -TREE_REACH..cs + TREE_REACH {
                let h = height_at(lx, lz);
                if origin.y > h + TREE_MAX_HEIGHT || origin.y + cs <= h - 2 {
                    continue;
                }
                if slope_at(height_at, lx, lz) >= 3 {
                    continue;
                }
                let (wx, wz) = ((origin.x + lx).rem_euclid(w), (origin.z + lz).rem_euclid(w));
                let Some(trunk) = self.tree_at(wx, wz, h) else { continue };
                let base = h + 1;
                let crown = base + trunk - 1;
                let mut put = |bx: i64, by: i64, bz: i64, id: BlockId, replace: bool| {
                    let (dx, dy, dz) = (bx - origin.x, by - origin.y, bz - origin.z);
                    if (0..cs).contains(&dx) && (0..cs).contains(&dy) && (0..cs).contains(&dz) {
                        let i = LocalPos::new(dx as u8, dy as u8, dz as u8).index();
                        if replace || blocks[i] == ids.air {
                            blocks[i] = id;
                        }
                    }
                };
                let (bx, bz) = (origin.x + lx, origin.z + lz);
                for oy in -2i64..=2 {
                    let r: i64 = if oy >= 1 { 1 } else { 2 };
                    for oz in -r..=r {
                        for ox in -r..=r {
                            let corner = ox.abs() == r && oz.abs() == r;
                            if corner && (oy == 2 || hash01(seed ^ 0x1eaf, wx + ox, crown + oy, wz + oz) < 0.5) {
                                continue;
                            }
                            put(bx + ox, crown + oy, bz + oz, ids.foliage, false);
                        }
                    }
                }
                for y in base..=crown {
                    put(bx, y, bz, ids.trunk, true);
                }
            }
        }
    }

    /// A safe place to stand: dry land near the planet's origin, not in a cave.
    pub fn spawn_point(&self) -> BlockPos {
        let sea = self.cfg.sea_level;
        let w = self.shape.width_blocks();
        for r in 0..w / 16 {
            for i in -r..=r {
                for (x, z) in [(i, -r), (i, r), (-r, i), (r, i)] {
                    let (x, z) = ((x * 8).rem_euclid(w), (z * 8).rem_euclid(w));
                    let h = self.surface_height(x, z);
                    if h > sea + 2 && h < sea + 40 && !Self::is_cave(&self.coarse_interp(x, h, z)) {
                        return BlockPos::new(x, h + 1, z);
                    }
                }
            }
        }
        BlockPos::new(0, self.shape.height_blocks() - TREE_MAX_HEIGHT, 0)
    }
}

fn slope_at(height_at: &impl Fn(i64, i64) -> i64, lx: i64, lz: i64) -> i64 {
    (height_at(lx + 1, lz) - height_at(lx - 1, lz))
        .abs()
        .max((height_at(lx, lz + 1) - height_at(lx, lz - 1)).abs())
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn trilerp(c: &Corners, tx: f64, ty: f64, tz: f64) -> Coarse {
    let l = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let f = |g: fn(&Coarse) -> f64| {
        let x00 = l(g(&c[0][0][0]), g(&c[1][0][0]), tx);
        let x01 = l(g(&c[0][0][1]), g(&c[1][0][1]), tx);
        let x10 = l(g(&c[0][1][0]), g(&c[1][1][0]), tx);
        let x11 = l(g(&c[0][1][1]), g(&c[1][1][1]), tx);
        l(l(x00, x10, ty), l(x01, x11, ty), tz)
    };
    Coarse {
        cavern: f(|c| c.cavern),
        tunnel: f(|c| c.tunnel),
        coal: f(|c| c.coal),
        copper: f(|c| c.copper),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generator() -> Generator {
        let cfg = PlanetGenConfig {
            seed: 42,
            ..Default::default()
        };
        Generator::new(PlanetShape::new(64, 12), cfg, &BlockRegistry::builtin()).unwrap()
    }

    fn fnv(bytes: &[u8], mut h: u64) -> u64 {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h
    }

    #[test]
    fn same_seed_gives_identical_chunks() {
        let (a, b) = (generator(), generator());
        for p in [
            ChunkPos::new(0, 3, 0),
            ChunkPos::new(17, 4, -5),
            ChunkPos::new(63, 2, 63),
        ] {
            assert_eq!(a.generate(p).encode(), b.generate(p).encode());
        }
    }

    #[test]
    fn chunks_wrap_with_the_planet() {
        let g = generator();
        assert_eq!(
            g.generate(ChunkPos::new(3, 4, 5)),
            g.generate(ChunkPos::new(3 + 64, 4, 5 - 128))
        );
    }

    /// Pins what the generator produces. If this fails the terrain changed:
    /// bump GENERATOR_VERSION and re-pin, in the same commit.
    #[test]
    fn terrain_fingerprint_is_pinned() {
        let g = generator();
        let mut h = 0xcbf2_9ce4_8422_2325;
        for p in [
            ChunkPos::new(0, 3, 0),
            ChunkPos::new(0, 4, 0),
            ChunkPos::new(10, 3, 20),
            ChunkPos::new(40, 5, 7),
            ChunkPos::new(63, 2, 1),
        ] {
            h = fnv(&g.generate(p).encode(), h);
        }
        assert_eq!(GENERATOR_VERSION, 1);
        assert_eq!(h, PINNED, "terrain changed: got {h:#x}");
    }
    const PINNED: u64 = 0x7d6bf6bb16c76ecf;

    #[test]
    fn interpolated_lookup_matches_bulk_generation() {
        // Tree footings use coarse_interp; chunks use the bulk lattice. They
        // must agree or a tree could stand over a cave the chunk carved.
        let g = generator();
        let pos = ChunkPos::new(9, 3, 4);
        let chunk = g.generate(pos);
        let o = pos.origin();
        let mut checked = 0;
        for i in 0..CHUNK_VOLUME {
            let l = LocalPos::from_index(i);
            if i % 97 != 0 {
                continue;
            }
            let (wx, wy, wz) = (o.x + l.x as i64, o.y + l.y as i64, o.z + l.z as i64);
            let h = g.surface_height(wx, wz);
            if wy < h - 6 && wy > 3 && chunk.get(l) == BlockId::AIR {
                assert!(Generator::is_cave(&g.coarse_interp(wx, wy, wz)));
                checked += 1;
            }
        }
        assert!(checked > 0, "sample hit no cave air; pick another chunk");
    }

    #[test]
    fn surface_tiles_across_the_seam() {
        let g = generator();
        let w = g.shape().width_blocks();
        for i in 0..200 {
            assert_eq!(g.surface_height(i * 7, i), g.surface_height(i * 7 + w, i - w));
        }
    }

    #[test]
    fn world_has_sea_land_and_mountains() {
        let g = generator();
        let w = g.shape().width_blocks();
        let sea = g.config().sea_level;
        let (mut water, mut land, mut high) = (0, 0, 0);
        for z in (0..w).step_by(16) {
            for x in (0..w).step_by(16) {
                let h = g.surface_height(x, z);
                if h < sea {
                    water += 1
                } else {
                    land += 1
                }
                if h > sea + 60 {
                    high += 1;
                }
            }
        }
        let total = (water + land) as f64;
        assert!(water as f64 / total > 0.1, "water {water} land {land}");
        assert!(land as f64 / total > 0.3, "water {water} land {land}");
        assert!(high > 0, "no mountains");
    }

    #[test]
    fn floor_is_bedrock_and_sky_is_air() {
        let g = generator();
        let bedrock = BlockRegistry::builtin().id("bedrock").unwrap();
        let floor = g.generate(ChunkPos::new(5, 0, 5));
        for x in 0..32 {
            for z in 0..32 {
                assert_eq!(floor.get(LocalPos::new(x, 0, z)), bedrock);
            }
        }
        assert_eq!(g.generate(ChunkPos::new(5, 11, 5)).uniform(), Some(BlockId::AIR));
        assert_eq!(g.generate(ChunkPos::new(5, -1, 5)).uniform(), Some(BlockId::AIR));
    }

    #[test]
    fn trees_grow_and_survive_chunk_borders() {
        let g = generator();
        let r = BlockRegistry::builtin();
        let trunk = r.id("trunk").unwrap();
        let foliage = r.id("foliage").unwrap();
        let mut trees = 0;
        let mut canopy_only = 0;
        for cz in 0..12 {
            for cx in 0..12 {
                for cy in 3..8 {
                    let c = g.generate(ChunkPos::new(cx, cy, cz));
                    let (t, f) = (c.palette().contains(&trunk), c.palette().contains(&foliage));
                    if t && f {
                        trees += 1;
                    } else if f {
                        canopy_only += 1;
                    }
                }
            }
        }
        assert!(trees > 0, "no trees in a 12x12 chunk area");
        // Canopy reaching over a border without its trunk proves cross-chunk
        // placement happens; it must not be the common case.
        assert!(canopy_only < trees * 3, "trees {trees} canopy-only {canopy_only}");
    }

    #[test]
    fn spawn_is_on_dry_land() {
        let g = generator();
        assert!(g.spawn_point().y > g.config().sea_level);
    }
}
