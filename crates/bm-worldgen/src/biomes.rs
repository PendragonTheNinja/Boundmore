//! Biomes (milestone 19, `docs/milestones/19-living-worlds.md`): the land of
//! a planet as a patchwork of biomes chosen by climate, as Boundless's worlds
//! were (research `docs/research/boundless-worldgen.md` sections 1 to 4).
//!
//! The catalogue (`assets/biomes.toml`) holds every biome Boundmore has, each
//! in a tier set with labels. A planet kind names its home set and the
//! labels it allows; when a planet is created it draws its fifteen biomes
//! from the catalogue ([`pick_biomes`], Boundless's 16 / 8 / 4 / 2 / 1 by
//! distance from the home set) and saves them with its settings, so a saved
//! planet never depends on the catalogue again.
//!
//! On the planet, four climate values (altitude, temperature, humidity,
//! vegetation) are laid out in jittered cells about `biome_size` blocks
//! across; each cell takes the biome whose climate targets lie nearest
//! ([`Layout`]). Heights blend across cell borders; cover, rock and trees
//! come from the cell a column belongs to.

use crate::noise::{Fbm, hash01, hash3};
use serde::{Deserialize, Serialize};

/// One biome as a planet saves it: its climate, its land and what covers it.
/// Heights are blocks above (or below, negative) the planet's sea level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BiomeSpec {
    pub name: String,
    /// Climate targets, 0 to 1: altitude, temperature, humidity, vegetation.
    pub climate: [f64; 4],
    /// How strongly it claims cells near its climate (Boundless 0.5 to 1).
    pub weight: f64,
    /// The floor of its land.
    pub base: f64,
    /// Rolling hills on the floor: amplitude and feature size.
    pub hills: f64,
    pub hill_size: f64,
    /// Ridged mountains on the floor: height and feature size.
    pub mountains: f64,
    pub mountain_size: f64,
    /// Land rises in flat steps this tall (0: smooth).
    pub terrace: f64,
    /// Rivers may cut it.
    pub rivers: bool,
    /// Top and the block under it, one to three pairs, patched by noise.
    pub cover: Vec<[String; 2]>,
    /// The rock it is filled with.
    pub rock: String,
    /// Top of steep ground ("": its rock).
    pub cliff: String,
    /// Its shore ("": none, the land meets the water as it is).
    pub beach: String,
    /// Snow tops the land this far above the sea.
    pub snow_above: Option<i64>,
    /// Chance a column grows a tree (where the forest noise allows).
    pub tree_density: f64,
}

impl Default for BiomeSpec {
    fn default() -> Self {
        Self {
            name: String::new(),
            climate: [0.5; 4],
            weight: 1.0,
            base: 4.0,
            hills: 6.0,
            hill_size: 120.0,
            mountains: 0.0,
            mountain_size: 300.0,
            terrace: 0.0,
            rivers: true,
            cover: vec![["turf".into(), "soil".into()]],
            rock: "stone".into(),
            cliff: String::new(),
            beach: "sand".into(),
            snow_above: None,
            tree_density: 0.0,
        }
    }
}

impl BiomeSpec {
    /// Every block it names, to check against the catalogue.
    pub fn blocks(&self) -> impl Iterator<Item = &str> {
        self.cover
            .iter()
            .flat_map(|p| p.iter().map(String::as_str))
            .chain([self.rock.as_str(), self.cliff.as_str(), self.beach.as_str()])
            .filter(|n| !n.is_empty())
    }
}

/// A catalogue entry: a biome, the tier set it belongs to and its labels.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct BiomeEntry {
    /// Boundless's biome folders 1 to 7: gentle to wild.
    pub set: u8,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(flatten)]
    pub spec: BiomeSpec,
}

#[derive(Deserialize)]
struct CatalogueFile {
    #[serde(default)]
    biome: Vec<BiomeEntry>,
}

/// Every biome Boundmore has (`assets/biomes.toml`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BiomeCatalogue {
    pub biomes: Vec<BiomeEntry>,
}

impl BiomeCatalogue {
    pub fn from_toml(source: &str) -> Result<Self, toml::de::Error> {
        let file: CatalogueFile = toml::from_str(source)?;
        Ok(Self { biomes: file.biome })
    }

    pub fn builtin() -> Self {
        Self::from_toml(include_str!("../../../assets/biomes.toml")).expect("assets/biomes.toml is valid")
    }
}

/// Which biomes a planet kind may draw: its home tier set and the labels a
/// biome needs (any of `any`, none of `not`; an empty `any` takes all).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct BiomeChoice {
    pub home: u8,
    #[serde(default)]
    pub any: Vec<String>,
    #[serde(default)]
    pub not: Vec<String>,
    /// How many a planet keeps (Boundless: 15).
    #[serde(default = "fifteen")]
    pub count: usize,
}

fn fifteen() -> usize {
    15
}

impl BiomeChoice {
    fn allows(&self, e: &BiomeEntry) -> bool {
        let any = self.any.is_empty() || e.labels.iter().any(|l| self.any.contains(l));
        any && !e.labels.iter().any(|l| self.not.contains(l))
    }
}

/// How many draws a set `distance` sets away from the home set gets
/// (Boundless: 16, 8, 4, 2, then 1).
fn draws(distance: u8) -> f64 {
    match distance {
        0 => 16.0,
        1 => 8.0,
        2 => 4.0,
        3 => 2.0,
        _ => 1.0,
    }
}

/// The biomes a new planet keeps: each allowed biome weighted by its set's
/// draws shared among the allowed biomes of that set, then `count` drawn
/// without replacement by `seed` (all of them when there are fewer).
pub fn pick_biomes(catalogue: &BiomeCatalogue, choice: &BiomeChoice, seed: u64) -> Vec<BiomeSpec> {
    let allowed: Vec<&BiomeEntry> = catalogue.biomes.iter().filter(|e| choice.allows(e)).collect();
    let mut pool: Vec<(f64, &BiomeEntry)> = allowed
        .iter()
        .map(|e| {
            let in_set = allowed.iter().filter(|o| o.set == e.set).count() as f64;
            (draws(e.set.abs_diff(choice.home)) / in_set, *e)
        })
        .collect();
    let mut out = Vec::new();
    let mut n = 0;
    while out.len() < choice.count && !pool.is_empty() {
        let total: f64 = pool.iter().map(|p| p.0).sum();
        let mut r = hash01(seed ^ 0xb10e_5eed, n, 0, 0) * total;
        let mut pick = pool.len() - 1;
        for (i, p) in pool.iter().enumerate() {
            if r < p.0 {
                pick = i;
                break;
            }
            r -= p.0;
        }
        out.push(pool.remove(pick).1.spec.clone());
        n += 1;
    }
    out
}

/// The cells of a planet's climate: a whole number of jittered cells around
/// it each way, each holding the biome nearest its climate.
#[derive(Clone, Debug)]
pub struct Layout {
    /// Cells each way around the planet.
    n: i64,
    /// Cell size in blocks (the planet width over `n`).
    size: f64,
    width: f64,
    seed: u64,
    /// Each cell's biome, `[j * n + i]`.
    cells: Vec<u8>,
    warp_x: Fbm,
    warp_z: Fbm,
}

/// A column's nearby cells: up to nine (distance, biome), nearest first.
pub type Near = ([(f64, u8); 9], usize);

impl Layout {
    pub fn new(biomes: &[BiomeSpec], width: i64, biome_size: f64, seed: u64) -> Self {
        let n = ((width as f64 / biome_size.max(32.0)).round() as i64).max(2);
        let size = width as f64 / n as f64;
        // Climate varies smoothly over a few cells, with each cell's own
        // jitter on top, so neighbours are related but not alike.
        let fields: Vec<Fbm> = (0..4)
            .map(|f| Fbm::new(seed ^ (0xc11a_7e00 + f), width, size * 2.5, 2, 0.5))
            .collect();
        // Raw climate per cell, then each field spread evenly over 0 to 1 by
        // rank, so a planet's few cells reach every climate its biomes want
        // instead of bunching in the middle.
        let count = (n * n) as usize;
        let mut raw = vec![[0.0f64; 4]; count];
        for j in 0..n {
            for i in 0..n {
                let (cx, cz) = Self::centre_of(seed, size, i, j);
                raw[(j * n + i) as usize] = std::array::from_fn(|f| {
                    let smooth = fields[f].sample2(cx, cz, width as f64);
                    let own = hash01(seed ^ 0xc1e11, i, f as i64, j) - 0.5;
                    smooth + own * 0.5
                });
            }
        }
        let mut ranked = vec![[0.0f64; 4]; count];
        for f in 0..4 {
            let mut order: Vec<usize> = (0..count).collect();
            order.sort_by(|a, b| raw[*a][f].total_cmp(&raw[*b][f]).then(a.cmp(b)));
            for (rank, c) in order.into_iter().enumerate() {
                ranked[c][f] = (rank as f64 + 0.5) / count as f64;
            }
        }
        let mut cells = Vec::with_capacity(count);
        for j in 0..n {
            for i in 0..n {
                let climate = ranked[(j * n + i) as usize];
                let best = biomes
                    .iter()
                    .enumerate()
                    .map(|(b, spec)| {
                        let d: f64 = spec
                            .climate
                            .iter()
                            .zip(&climate)
                            .map(|(t, c)| (t - c) * (t - c))
                            .sum::<f64>()
                            .sqrt();
                        (d / spec.weight.max(0.05), b)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map_or(0, |(_, b)| b as u8);
                cells.push(best);
            }
        }
        Self {
            n,
            size,
            width: width as f64,
            seed,
            cells,
            warp_x: Fbm::new(seed ^ 0x3a2b_0001, width, size * 0.7, 2, 0.5),
            warp_z: Fbm::new(seed ^ 0x3a2b_0002, width, size * 0.7, 2, 0.5),
        }
    }

    /// The jittered centre of cell `(i, j)`, in blocks.
    fn centre_of(seed: u64, size: f64, i: i64, j: i64) -> (f64, f64) {
        let h = hash3(seed ^ 0xce11, i, 0, j);
        let jx = 0.15 + 0.7 * ((h & 0xffff) as f64 / 65536.0);
        let jz = 0.15 + 0.7 * (((h >> 16) & 0xffff) as f64 / 65536.0);
        ((i as f64 + jx) * size, (j as f64 + jz) * size)
    }

    /// The cells near column `(x, z)`: the nearest nine, with their distances
    /// from the column (warped, so borders wander), nearest first.
    pub fn near(&self, x: f64, z: f64) -> Near {
        let amp = self.size * 0.3;
        let wx = x + self.warp_x.sample2(x, z, self.width) * amp;
        let wz = z + self.warp_z.sample2(x, z, self.width) * amp;
        let ci = (wx / self.size).floor() as i64;
        let cj = (wz / self.size).floor() as i64;
        let mut out = [(f64::MAX, 0u8); 9];
        let mut k = 0;
        for dj in -1..=1 {
            for di in -1..=1 {
                let (i, j) = (ci + di, cj + dj);
                let (ri, rj) = (i.rem_euclid(self.n), j.rem_euclid(self.n));
                // The centre of the cell as seen from this column: wrapped
                // cells keep their own jitter, shifted by whole laps.
                let (cx, cz) = Self::centre_of(self.seed, self.size, ri, rj);
                let (cx, cz) = (cx + (i - ri) as f64 * self.size, cz + (j - rj) as f64 * self.size);
                let d = ((wx - cx).powi(2) + (wz - cz).powi(2)).sqrt();
                out[k] = (d, self.cells[(rj * self.n + ri) as usize]);
                k += 1;
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        (out, k)
    }

    /// The biome of the cell column `(x, z)` belongs to.
    pub fn biome_at(&self, x: f64, z: f64) -> u8 {
        self.near(x, z).0[0].1
    }

    /// The cells' biomes, `[j * n + i]`, and `n` (tests).
    pub fn cells(&self) -> (&[u8], i64) {
        (&self.cells, self.n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_loads_and_names_real_blocks() {
        let cat = BiomeCatalogue::builtin();
        assert!(cat.biomes.len() >= 20, "{}", cat.biomes.len());
        let blocks = bm_core::BlockRegistry::builtin();
        for e in &cat.biomes {
            for b in e.spec.blocks() {
                assert!(blocks.id(b).is_ok(), "biome `{}` names `{b}`", e.spec.name);
            }
            assert!(!e.spec.cover.is_empty(), "{}", e.spec.name);
            assert!((1..=7).contains(&e.set), "{}", e.spec.name);
        }
    }

    #[test]
    fn a_planet_draws_mostly_from_its_home_set() {
        let cat = BiomeCatalogue::builtin();
        let choice = BiomeChoice {
            home: 1,
            any: Vec::new(),
            not: Vec::new(),
            count: 8,
        };
        let mut home = 0;
        let mut total = 0;
        for seed in 0..40 {
            let picked = pick_biomes(&cat, &choice, seed);
            assert_eq!(picked.len(), 8);
            let names: std::collections::BTreeSet<_> = picked.iter().map(|b| &b.name).collect();
            assert_eq!(names.len(), 8, "no biome twice");
            for b in &picked {
                total += 1;
                if cat.biomes.iter().any(|e| e.spec.name == b.name && e.set == 1) {
                    home += 1;
                }
            }
        }
        assert!(home * 3 > total, "{home} of {total} from the home set");
        assert_eq!(pick_biomes(&cat, &choice, 9), pick_biomes(&cat, &choice, 9));
    }

    #[test]
    fn labels_filter_the_draw() {
        let cat = BiomeCatalogue::builtin();
        let choice = BiomeChoice {
            home: 2,
            any: vec!["desert".into()],
            not: vec!["cold".into()],
            count: 15,
        };
        for b in pick_biomes(&cat, &choice, 3) {
            let e = cat.biomes.iter().find(|e| e.spec.name == b.name).unwrap();
            assert!(e.labels.iter().any(|l| l == "desert"), "{}", b.name);
            assert!(!e.labels.iter().any(|l| l == "cold"), "{}", b.name);
        }
    }

    #[test]
    fn cells_tile_the_planet_and_borders_wander() {
        let cat = BiomeCatalogue::builtin();
        let biomes: Vec<BiomeSpec> = cat.biomes.iter().take(10).map(|e| e.spec.clone()).collect();
        let w = 1536;
        let l = Layout::new(&biomes, w, 320.0, 42);
        // The seam: a column and its lap agree.
        for (x, z) in [(0.0, 5.0), (3.0, 1500.0), (700.0, 0.0)] {
            assert_eq!(l.biome_at(x, z), l.biome_at(x + w as f64, z));
            assert_eq!(l.biome_at(x, z), l.biome_at(x, z - w as f64));
        }
        // More than one biome on a planet.
        let (cells, n) = l.cells();
        assert_eq!(n, 5);
        let kinds: std::collections::BTreeSet<_> = cells.iter().collect();
        assert!(kinds.len() >= 3, "{kinds:?}");
    }
}
