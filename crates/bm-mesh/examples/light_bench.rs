//! Light and lit-meshing timings on a generated planet.
//!
//! `cargo run --release -p bm-mesh --example light_bench`
//!
//! Streams a square of columns into a `LightMap` chunk by chunk, as the client
//! receives them, and reports the time to light each column as it completes;
//! then times single block edits at and under the surface, and meshing with
//! and without light.

use bm_core::{BlockId, BlockPos, BlockRegistry, Chunk, ChunkLight, ChunkPos, LightMap, PlanetShape};
use bm_mesh::{mesh_chunk, mesh_chunk_lit, neighbor_index};
use bm_worldgen::{Generator, PlanetGenConfig};
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Columns each side of the centre.
const RADIUS: i32 = 5;
const EDITS: usize = 4000;

fn main() {
    let registry = BlockRegistry::builtin();
    let flags = registry.flags();
    let shape = PlanetShape::new(64, 12);
    let cfg = PlanetGenConfig {
        seed: 42,
        ..PlanetGenConfig::default()
    };
    let generator = Generator::new(shape, cfg, &registry).expect("catalogue has the generator's blocks");
    let spawn = generator.spawn_point().chunk();
    let h = shape.height_chunks as i32;

    // Nearest columns first, the way the server streams them; each column's
    // chunks in a scrambled order, since they arrive one by one.
    let mut cols: Vec<(i32, i32)> = (-RADIUS..=RADIUS)
        .flat_map(|dx| (-RADIUS..=RADIUS).map(move |dz| (dx, dz)))
        .collect();
    cols.sort_by_key(|&(dx, dz)| dx * dx + dz * dz);
    let t = Instant::now();
    let mut world: HashMap<ChunkPos, Chunk> = HashMap::new();
    for &(dx, dz) in &cols {
        for y in 0..h {
            let p = shape.wrap_chunk(ChunkPos::new(spawn.x + dx, y, spawn.z + dz));
            world.insert(p, generator.generate(p));
        }
    }
    println!(
        "generated {} chunks ({} columns of {h}) in {:.1} s",
        world.len(),
        cols.len(),
        t.elapsed().as_secs_f64()
    );

    let mut rng = StdRng::seed_from_u64(1);
    let mut map = LightMap::new(shape, &flags);
    let mut column_times = Vec::new();
    let mut changed_total = 0;
    for &(dx, dz) in &cols {
        let mut ys: Vec<i32> = (0..h).collect();
        for i in (1..ys.len()).rev() {
            ys.swap(i, rng.random_range(0..=i));
        }
        let mut spent = Duration::ZERO;
        for y in ys {
            let p = shape.wrap_chunk(ChunkPos::new(spawn.x + dx, y, spawn.z + dz));
            let t = Instant::now();
            changed_total += map.chunk_loaded(p, &world).len();
            spent += t.elapsed();
        }
        column_times.push(spent);
    }
    report("light one column (all its chunk arrivals)", &mut column_times);
    println!(
        "  {} chunks lit, {} dense ({:.1} MB), {} re-mesh requests",
        map.lit_chunks(),
        map.dense_chunks(),
        map.dense_chunks() as f64 * 32768.0 / 1e6,
        changed_total
    );

    // Edits inside the square, so every one has lit neighbours.
    let inner = (RADIUS - 1) as i64 * 32;
    let origin = spawn.origin();
    let block_at = |world: &HashMap<ChunkPos, Chunk>, p: BlockPos| {
        let p = shape.wrap_block(p);
        world[&p.chunk()].get(p.local())
    };
    let stone = registry.id("stone").unwrap();
    let glow = registry.id("glow_crystal").unwrap();
    let mut edit_times = Vec::new();
    let mut remeshes = 0;
    for n in 0..EDITS {
        let x = origin.x + rng.random_range(-inner..inner);
        let z = origin.z + rng.random_range(-inner..inner);
        let mut y = shape.height_blocks() - 1;
        while y > 0 && block_at(&world, BlockPos::new(x, y, z)).is_air() {
            y -= 1;
        }
        let (p, id) = match n % 4 {
            0 => (BlockPos::new(x, y, z), BlockId::AIR),
            1 => (BlockPos::new(x, y + 1, z), stone),
            2 => (BlockPos::new(x, y - 6, z), BlockId::AIR),
            _ => (BlockPos::new(x, y - 3, z), glow),
        };
        if !shape.contains_y(p.y) {
            continue;
        }
        let p = shape.wrap_block(p);
        if world.get_mut(&p.chunk()).unwrap().set(p.local(), id) == id {
            continue;
        }
        let t = Instant::now();
        remeshes += map.block_changed(p, &world).len();
        edit_times.push(t.elapsed());
    }
    let count = edit_times.len();
    report("one block edit", &mut edit_times);
    println!("  {:.1} chunks to re-mesh per edit", remeshes as f64 / count as f64);

    // Meshing the inner columns, unlit and lit.
    let mut plain = Vec::new();
    let mut lit = Vec::new();
    for &(dx, dz) in cols.iter().filter(|(dx, dz)| dx.abs() < RADIUS && dz.abs() < RADIUS) {
        for y in 0..h {
            let p = shape.wrap_chunk(ChunkPos::new(spawn.x + dx, y, spawn.z + dz));
            let mut nb: [Option<&Chunk>; 27] = [None; 27];
            for ny in -1..=1 {
                for nz in -1..=1 {
                    for nx in -1..=1 {
                        nb[neighbor_index(nx, ny, nz)] = world.get(&shape.wrap_chunk(p.offset(nx, ny, nz)));
                    }
                }
            }
            let t = Instant::now();
            let a = mesh_chunk(&world[&p], &nb, BlockId::AIR, &flags);
            let ta = t.elapsed();
            let t = Instant::now();
            let lights = map.neighborhood(p);
            let refs: [Option<&ChunkLight>; 27] = std::array::from_fn(|i| lights[i].as_ref());
            let b = mesh_chunk_lit(&world[&p], &nb, &refs, BlockId::AIR, &flags);
            let tb = t.elapsed();
            if !a.is_empty() {
                plain.push(ta);
                lit.push(tb);
                assert!(a.opaque.len() <= b.opaque.len(), "light only splits quads");
            }
        }
    }
    report("mesh a non-empty chunk, full bright", &mut plain);
    report("mesh a non-empty chunk, lit", &mut lit);
}

fn report(what: &str, times: &mut [Duration]) {
    times.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    let mean = times.iter().map(|&d| ms(d)).sum::<f64>() / times.len() as f64;
    let at = |q: f64| ms(times[((times.len() - 1) as f64 * q) as usize]);
    println!(
        "{what}: n {}, mean {mean:.3} ms, median {:.3}, p95 {:.3}, p99 {:.3}, max {:.3}",
        times.len(),
        at(0.5),
        at(0.95),
        at(0.99),
        at(1.0)
    );
}
