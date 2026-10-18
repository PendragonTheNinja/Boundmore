//! An isometric diorama built from the painted textures, so they can be
//! judged together the way a player sees them: a hillside of turf over soil
//! over stone, a tree, a snowy crest, a beach with water, a little wall.
//! Faces are shaded flat (top 1.0, left 0.8, right 0.65) like the renderer.

use bm_core::BlockRegistry;

use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::noise::hash3;
use crate::tex::{SIZE, SIZEF, Tex};
use crate::{Family, find};

const X: usize = 10;
const Z: usize = 10;
const Y: usize = 12;
const WATER_LEVEL: usize = 3;

/// Pixels per block along one iso axis step (2:1 dimetric, 2 px per texel).
const STEP_X: f32 = 64.0;
const STEP_Y: f32 = 32.0;
const RISE: f32 = 64.0;
const MARGIN: f32 = 40.0;

struct Scene {
    blocks: Vec<Option<&'static str>>,
}

impl Scene {
    fn get(&self, x: i32, y: i32, z: i32) -> Option<&'static str> {
        if x < 0 || y < 0 || z < 0 || x >= X as i32 || y >= Y as i32 || z >= Z as i32 {
            return None;
        }
        self.blocks[(y as usize * Z + z as usize) * X + x as usize]
    }

    fn set(&mut self, x: usize, y: usize, z: usize, b: &'static str) {
        self.blocks[(y * Z + z) * X + x] = Some(b);
    }
}

/// Ground height of a column: high at the back left, sloping to a beach at
/// the front right.
fn height(x: usize, z: usize) -> usize {
    let (fx, fz) = (x as f32, z as f32);
    let h = 8.6 - 0.4 * (fx + fz) + 0.7 * (fx * 1.3).sin() * (fz * 0.9).cos();
    h.round().clamp(1.0, 9.0) as usize
}

fn build() -> Scene {
    let mut s = Scene {
        blocks: vec![None; X * Y * Z],
    };
    for z in 0..Z {
        for x in 0..X {
            let h = height(x, z);
            s.set(x, 0, z, "bedrock");
            for y in 1..=h {
                let b = if y == h {
                    if h <= WATER_LEVEL {
                        if (x + 2 * z) % 7 == 0 { "gravel" } else { "sand" }
                    } else if h >= 8 {
                        "snow"
                    } else {
                        "turf"
                    }
                } else if y + 2 >= h && h > WATER_LEVEL {
                    "soil"
                } else if y == 1 {
                    "deep_stone"
                } else if h <= WATER_LEVEL + 1 && y + 1 == h {
                    "sand"
                } else {
                    "stone"
                };
                s.set(x, y, z, b);
            }
            for y in h + 1..=WATER_LEVEL {
                s.set(x, y, z, "water");
            }
        }
    }
    // Ore and crystal showing on the cut faces.
    for (x, y, z, b) in [
        (9, 3, 2, "copper_vein"),
        (9, 2, 5, "coal_seam"),
        (4, 2, 9, "coal_seam"),
        (1, 4, 9, "copper_vein"),
        (9, 4, 0, "glow_crystal"),
    ] {
        if s.get(x as i32, y as i32, z as i32).is_some() {
            s.set(x, y, z, b);
        }
    }
    // A tree on the slope.
    let (tx, tz) = (3, 3);
    let base = height(tx, tz) + 1;
    for y in base..base + 4 {
        s.set(tx, y, tz, "trunk");
    }
    for dy in 3..6 {
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                let corner = dx.abs() + dz.abs() == 2;
                if dy == 5 && (corner || dx != 0 && dz != 0) || dy == 3 && corner {
                    continue;
                }
                let (x, z, y) = ((tx as i32 + dx) as usize, (tz as i32 + dz) as usize, base + dy);
                if y < Y && s.get(x as i32, y as i32, z as i32).is_none() {
                    s.set(x, y, z, "foliage");
                }
            }
        }
    }
    // A little wall by the shore: brick, timber and a glass pane.
    let (wx, wz) = (6, 2);
    let g = height(wx, wz) + 1;
    s.set(wx, g, wz, "brick");
    s.set(wx + 1, g, wz, "brick");
    s.set(wx, g + 1, wz, "timber");
    s.set(wx + 1, g + 1, wz, "glass");
    s
}

fn project(x: f32, y: f32, z: f32) -> (f32, f32) {
    let ox = MARGIN + Z as f32 * STEP_X;
    let oy = MARGIN + Y as f32 * RISE;
    (ox + (x - z) * STEP_X, oy + (x + z) * STEP_Y - y * RISE)
}

/// Fills the parallelogram `origin + u * a + v * b` (u, v in 0..1) with the
/// texture, shaded by `light`.
fn face(canvas: &mut Canvas, tex: &Tex, origin: (f32, f32), a: (f32, f32), b: (f32, f32), light: f32) {
    let corners = [
        origin,
        (origin.0 + a.0, origin.1 + a.1),
        (origin.0 + b.0, origin.1 + b.1),
        (origin.0 + a.0 + b.0, origin.1 + a.1 + b.1),
    ];
    let minx = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min).floor() as i64;
    let maxx = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max).ceil() as i64;
    let miny = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min).floor() as i64;
    let maxy = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max).ceil() as i64;
    let det = a.0 * b.1 - a.1 * b.0;
    for py in miny..maxy {
        for px in minx..maxx {
            let (dx, dy) = (px as f32 + 0.5 - origin.0, py as f32 + 0.5 - origin.1);
            let u = (dx * b.1 - dy * b.0) / det;
            let v = (a.0 * dy - a.1 * dx) / det;
            if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                continue;
            }
            let tx = ((u * SIZEF) as usize).min(SIZE - 1);
            let ty = ((v * SIZEF) as usize).min(SIZE - 1);
            canvas.blend(px, py, tex.get(tx, ty).scaled(light));
        }
    }
}

pub fn preview(families: &[Family]) -> Canvas {
    let registry = BlockRegistry::builtin();
    let scene = build();
    let w = (2.0 * MARGIN + (X + Z) as f32 * STEP_X) as usize;
    let h = (2.0 * MARGIN + (X + Z) as f32 * STEP_Y + Y as f32 * RISE) as usize;
    let mut canvas = Canvas::new(w, h, Rgba::hex(0x9fd4f4));
    // Sky: a soft vertical gradient.
    for y in 0..h {
        let t = y as f32 / h as f32;
        let c = Rgba::hex(0x6fb4ee).mix(Rgba::hex(0xe8f4ff), t);
        for x in 0..w {
            canvas.px[y * w + x] = c;
        }
    }
    let opaque = |b: Option<&str>| b.is_some_and(|n| registry.get(registry.id(n).unwrap()).opaque);
    // Painter's order: back to front, bottom to top.
    let mut order = Vec::new();
    for y in 0..Y {
        for z in 0..Z {
            for x in 0..X {
                if let Some(b) = scene.get(x as i32, y as i32, z as i32) {
                    order.push((x + y + z, x, y, z, b));
                }
            }
        }
    }
    order.sort_by_key(|o| (o.0, o.2));
    for (_, x, y, z, name) in order {
        let def = registry.get(registry.id(name).unwrap());
        let (ix, iy, iz) = (x as i32, y as i32, z as i32);
        let shows = |n: Option<&str>| !opaque(n) && n != Some(name);
        let pick = |tex_name: &str| {
            let fam = find(families, tex_name);
            let i = hash3(ix as u32, iy as u32, iz as u32) as usize % fam.faces.len();
            fam.faces[i].clone()
        };
        let (fx, fy, fz) = (x as f32, y as f32, z as f32);
        let ax = (STEP_X, STEP_Y);
        let az = (-STEP_X, STEP_Y);
        let down = (0.0, RISE);
        if shows(scene.get(ix, iy + 1, iz)) {
            let t = pick(def.textures.top().unwrap());
            face(&mut canvas, &t, project(fx, fy + 1.0, fz), ax, az, 1.0);
        }
        if shows(scene.get(ix, iy, iz + 1)) {
            let t = pick(def.textures.side().unwrap());
            face(&mut canvas, &t, project(fx, fy + 1.0, fz + 1.0), ax, down, 0.8);
        }
        if shows(scene.get(ix + 1, iy, iz)) {
            let t = pick(def.textures.side().unwrap());
            face(
                &mut canvas,
                &t,
                project(fx + 1.0, fy + 1.0, fz + 1.0),
                (STEP_X, -STEP_Y),
                down,
                0.65,
            );
        }
    }
    canvas
}
