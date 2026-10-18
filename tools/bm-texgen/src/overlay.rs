//! Crack overlays for hold-to-break: eight stages of one crack network
//! growing out from the middle of the face.

use crate::color::Rgba;
use crate::noise::{Rng, seed};
use crate::tex::{SIZE, Tex};

pub const STAGES: usize = 8;

/// Path length (in pixels from the impact point) each stage reveals.
const REACH: [f32; STAGES] = [2.5, 5.0, 8.0, 11.0, 14.5, 18.5, 23.5, 32.0];

/// For each pixel, how far along the crack network it is, or infinity if
/// the network never reaches it.
fn crack_births() -> Vec<f32> {
    let mut birth = vec![f32::INFINITY; SIZE * SIZE];
    let mut rng = Rng::new(seed("cracks"));
    // (x, y, heading, birth, remaining length, depth)
    let mut stack = Vec::new();
    let arms = 5;
    for i in 0..arms {
        let heading = i as f32 / arms as f32 * std::f32::consts::TAU + rng.range(-0.35, 0.35);
        stack.push((15.5, 15.5, heading, 0.0, rng.range(16.0, 24.0), 0u32));
    }
    while let Some((mut x, mut y, mut h, mut b, len, depth)) = stack.pop() {
        let mut walked = 0.0;
        let mut since_turn = 0.0;
        while walked < len {
            let step = 0.5;
            x += h.cos() * step;
            y += h.sin() * step;
            walked += step;
            since_turn += step;
            b += step;
            if !(0.0..SIZE as f32).contains(&x) || !(0.0..SIZE as f32).contains(&y) {
                break;
            }
            let i = y as usize * SIZE + x as usize;
            birth[i] = birth[i].min(b);
            if since_turn > 2.5 {
                since_turn = 0.0;
                h += rng.range(-0.55, 0.55);
                // Branch now and then; side branches are shorter.
                if depth < 2 && rng.f() < 0.32 {
                    let side = if rng.f() < 0.5 { -1.0 } else { 1.0 };
                    let nh = h + side * rng.range(0.6, 1.2);
                    stack.push((x, y, nh, b, (len - walked) * rng.range(0.4, 0.7), depth + 1));
                }
            }
        }
    }
    birth
}

/// The eight crack stages, smallest first. Dark grooves with a pale lit lip
/// on their lower-right side, so they read on light blocks and dark ones.
pub fn cracks() -> Vec<Tex> {
    let birth = crack_births();
    let core = Rgba::hex(0x1a1020).with_alpha(0.86);
    let lip = Rgba::hex(0xfff2e0).with_alpha(0.34);
    let halo = Rgba::hex(0x1a1020).with_alpha(0.3);
    (0..STAGES)
        .map(|stage| {
            let reach = REACH[stage];
            let mut on = vec![false; SIZE * SIZE];
            for (i, &b) in birth.iter().enumerate() {
                on[i] = b <= reach;
            }
            // Older parts of the network widen in later stages.
            if stage >= 3 {
                let thick = reach * 0.3;
                for y in 0..SIZE {
                    for x in 0..SIZE - 1 {
                        if birth[y * SIZE + x] <= thick {
                            on[y * SIZE + x + 1] = true;
                        }
                    }
                }
            }
            let at = |x: i32, y: i32| {
                (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) && on[y as usize * SIZE + x as usize]
            };
            Tex::from_fn(|x, y| {
                let (ix, iy) = (x as i32, y as i32);
                if at(ix, iy) {
                    core
                } else if at(ix - 1, iy) || at(ix, iy - 1) {
                    lip
                } else if at(ix + 1, iy) || at(ix, iy + 1) {
                    halo
                } else {
                    Rgba::new(0.0, 0.0, 0.0, 0.0)
                }
            })
        })
        .collect()
}
