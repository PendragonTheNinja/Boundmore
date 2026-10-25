//! A top-down picture of a whole planet: each column's top block in its
//! natural colour, shaded by height and by the slope's light, water tinted
//! by depth. For judging land by eye (milestone 19).
//!
//! `cargo run --release -p bm-worldgen --example planet_map -- KIND SEED WIDTH_CHUNKS OUT.ppm`

use bm_core::{BlockRegistry, CHUNK_SIZE, ChunkPos, LocalPos, PlanetShape};
use bm_worldgen::{Generator, PlanetKinds};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let kind = args.get(1).map_or("temperate", String::as_str);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(42);
    let width: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(48);
    let out = args.get(4).map_or("planet.ppm", String::as_str);
    let blocks = BlockRegistry::builtin();
    let kinds = PlanetKinds::builtin(&blocks);
    let cfg = kinds.get(kind).expect("a planet kind").generation(seed);
    let names: Vec<&str> = cfg.biomes.iter().map(|b| b.name.as_str()).collect();
    eprintln!("{kind} seed {seed}: {} biomes {names:?}", names.len());
    let shape = PlanetShape::new(width, 12);
    let g = Generator::new(shape, cfg.clone(), &blocks).expect("valid settings");
    let w = shape.width_blocks() as usize;
    let cs = CHUNK_SIZE as i64;
    let water = blocks.id(&cfg.water).unwrap();
    let mut top = vec![(0i64, [0u8; 3], false); w * w];
    for cz in 0..width as i32 {
        for cx in 0..width as i32 {
            let mut left = cs * cs;
            let mut found = vec![false; (cs * cs) as usize];
            for cy in (0..12).rev() {
                let c = g.generate(ChunkPos::new(cx, cy, cz));
                if c.uniform() == Some(bm_core::BlockId(0)) {
                    continue;
                }
                for lz in 0..cs {
                    for lx in 0..cs {
                        let i = (lz * cs + lx) as usize;
                        if found[i] {
                            continue;
                        }
                        for ly in (0..cs).rev() {
                            let s = c.state(LocalPos::new(lx as u8, ly as u8, lz as u8));
                            if s.id.0 == 0 {
                                continue;
                            }
                            let def = blocks.get(s.id);
                            let col = def.top_color.unwrap_or(def.color);
                            let (x, z) = (cx as i64 * cs + lx, cz as i64 * cs + lz);
                            top[z as usize * w + x as usize] = (cy as i64 * cs + ly, col, s.id == water);
                            found[i] = true;
                            left -= 1;
                            break;
                        }
                    }
                }
                if left == 0 {
                    break;
                }
            }
        }
    }
    let mut f = std::fs::File::create(out).unwrap();
    writeln!(f, "P6 {w} {w} 255").unwrap();
    let sea = cfg.sea_level;
    let mut px = Vec::with_capacity(w * w * 3);
    for z in 0..w {
        for x in 0..w {
            let (h, col, wet) = top[z * w + x];
            let hw = top[z * w + (x + w - 1) % w].0;
            let hn = top[((z + w - 1) % w) * w + x].0;
            let light = (1.0 + ((h - hw) + (h - hn)) as f64 * 0.08).clamp(0.6, 1.4);
            let lift = 0.85 + ((h - sea) as f64 / 300.0).clamp(-0.2, 0.3);
            for ch in col {
                let v = if wet { ch as f64 * 0.9 } else { ch as f64 * light * lift };
                px.push(v.clamp(0.0, 255.0) as u8);
            }
        }
    }
    f.write_all(&px).unwrap();
}
