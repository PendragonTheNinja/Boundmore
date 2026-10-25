//! Chunk generation time per planet kind.
//!
//! `cargo run --release -p bm-worldgen --example gen_timing [-- --single]`
//! (`--single`: each kind without its biomes, as planets made before
//! generator 4)
//!
//! Generates the same 16×16-column, 8-chunk-tall block of a 2048-block planet
//! for each kind on one thread and prints the mean time per chunk (every chunk,
//! and the ones that hold terrain rather than empty sky).

use bm_core::{BlockRegistry, ChunkPos, PlanetShape};
use bm_worldgen::{Generator, PlanetGenConfig};
use std::time::Instant;

fn main() {
    let blocks = BlockRegistry::builtin();
    let mut configs: Vec<(String, PlanetGenConfig)> = vec![("default".into(), PlanetGenConfig::default())];
    configs.extend(kinds(&blocks));
    // Each kind as planets made before generator 4 hold it: one landscape.
    if std::env::args().any(|a| a == "--single") {
        for (name, cfg) in &mut configs {
            name.push_str(" (single)");
            cfg.biomes.clear();
        }
    }
    for (name, mut cfg) in configs {
        cfg.seed = 42;
        let g = Generator::new(PlanetShape::new(64, 12), cfg, &blocks).unwrap();
        // Warm up.
        for c in 0..64 {
            std::hint::black_box(g.generate(ChunkPos::new(c % 8, 3 + c / 32, c / 8)));
        }
        let (mut total, mut n, mut solid_t, mut solid_n) = (0.0, 0, 0.0, 0);
        for cz in 0..16 {
            for cx in 0..16 {
                for cy in 0..8 {
                    let t = Instant::now();
                    let c = g.generate(ChunkPos::new(cx, cy, cz));
                    let dt = t.elapsed().as_secs_f64();
                    total += dt;
                    n += 1;
                    if c.uniform().is_none() {
                        solid_t += dt;
                        solid_n += 1;
                    }
                    std::hint::black_box(c);
                }
            }
        }
        println!(
            "{name:>10}: {:.3} ms/chunk over {n}, {:.3} ms/chunk over {solid_n} with terrain",
            total * 1e3 / n as f64,
            solid_t * 1e3 / solid_n.max(1) as f64
        );
    }
}

fn kinds(blocks: &BlockRegistry) -> Vec<(String, PlanetGenConfig)> {
    bm_worldgen::PlanetKinds::builtin(blocks)
        .kinds
        .iter()
        .map(|k| (k.name.clone(), k.generation(42)))
        .collect()
}
