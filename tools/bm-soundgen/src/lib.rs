//! Makes every Boundmore sound from code (milestone 30), as `bm-texgen`
//! paints every texture: layered noise, filtered impacts, decaying
//! partials and envelopes, aimed at a soft, warm, rounded fantasy feel.
//! Nothing is sampled or copied from anywhere.
//!
//! Each sound has several variants (`name_1.wav` ...), seeded by its name
//! and number, so a run always makes the same bytes. Any file may be
//! replaced by a hand-made or free one of the same name (`.wav` or `.ogg`).

pub mod dsp;

use dsp::{Filter, Rng, buffer, modes, noise, whoosh};

/// What a struck thing is made of: its partials and how it rings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stuff {
    Stone,
    Wood,
    Soil,
    Plant,
    Metal,
    Glass,
    Crystal,
    Sand,
    Snow,
    Water,
}

/// One sound: its file stem, how many variants, how loud its peak is
/// (relative loudness is baked into the files), and its maker.
pub struct Sound {
    pub name: &'static str,
    pub variants: u32,
    pub peak: f32,
    make: fn(&mut Rng) -> Vec<f32>,
}

impl Sound {
    /// Variant `n` (1-based), finished and ready to write.
    pub fn render(&self, n: u32) -> Vec<f32> {
        let mut rng = Rng::new(seed(self.name, n));
        dsp::finish((self.make)(&mut rng), self.peak)
    }
}

fn seed(name: &str, n: u32) -> u64 {
    // FNV-1a: stable across platforms and Rust versions.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes().chain(n.to_le_bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

macro_rules! sound {
    ($name:literal, $n:expr, $peak:expr, $f:expr) => {
        Sound {
            name: $name,
            variants: $n,
            peak: $peak,
            make: $f,
        }
    };
}

/// Every sound the game plays, by file stem.
pub fn sounds() -> Vec<Sound> {
    use Stuff::*;
    vec![
        // Swings, by tool class: air moving past.
        sound!("swing_hammer", 4, 0.42, |r| swing(r, 0.24, 380.0, 900.0, 2.2)),
        sound!("swing_axe", 4, 0.40, |r| swing(r, 0.21, 520.0, 1300.0, 2.6)),
        sound!("swing_shovel", 4, 0.40, |r| swing(r, 0.23, 330.0, 760.0, 1.8)),
        sound!("swing_light", 4, 0.32, |r| swing(r, 0.17, 700.0, 1700.0, 3.0)),
        // A tool landing on a block.
        sound!("hit_stone", 4, 0.62, |r| hit(r, Stone)),
        sound!("hit_wood", 4, 0.62, |r| hit(r, Wood)),
        sound!("hit_soil", 4, 0.62, |r| hit(r, Soil)),
        sound!("hit_plant", 4, 0.55, |r| hit(r, Plant)),
        sound!("hit_metal", 4, 0.55, |r| hit(r, Metal)),
        sound!("hit_glass", 4, 0.50, |r| hit(r, Glass)),
        sound!("hit_crystal", 4, 0.50, |r| hit(r, Crystal)),
        // A block giving way: its body and its pieces.
        sound!("break_stone", 4, 0.78, |r| crumble(r, Stone)),
        sound!("break_wood", 4, 0.78, |r| crumble(r, Wood)),
        sound!("break_soil", 4, 0.75, |r| crumble(r, Soil)),
        sound!("break_plant", 4, 0.68, |r| crumble(r, Plant)),
        sound!("break_metal", 4, 0.70, |r| crumble(r, Metal)),
        sound!("break_glass", 4, 0.66, |r| crumble(r, Glass)),
        sound!("break_crystal", 4, 0.66, |r| crumble(r, Crystal)),
        sound!("place", 4, 0.50, place_soft),
        sound!("place_hard", 4, 0.52, place_hard),
        sound!("chisel", 4, 0.45, chisel),
        sound!("pickup", 4, 0.40, pickup),
        // Footsteps, quiet: heard thousands of times.
        sound!("step_stone", 5, 0.30, |r| step(r, Stone)),
        sound!("step_soil", 5, 0.30, |r| step(r, Soil)),
        sound!("step_wood", 5, 0.30, |r| step(r, Wood)),
        sound!("step_sand", 5, 0.28, |r| step(r, Sand)),
        sound!("step_snow", 5, 0.30, |r| step(r, Snow)),
        sound!("step_water", 5, 0.30, |r| step(r, Water)),
        sound!("jump", 3, 0.28, jump),
        sound!("land", 3, 0.45, land),
        sound!("splash", 3, 0.55, splash),
        sound!("hurt", 4, 0.55, hurt),
        sound!("defeat", 2, 0.55, defeat),
        sound!("creature_hit", 4, 0.58, creature_hit),
        sound!("creature_windup", 3, 0.50, windup),
        sound!("popper_whistle", 2, 0.34, whistle),
        sound!("sling", 3, 0.45, sling),
        sound!("throw", 3, 0.40, |r| swing(r, 0.26, 300.0, 700.0, 1.6)),
        sound!("blast", 3, 0.95, blast),
        sound!("grapple_fire", 3, 0.45, grapple_fire),
        sound!("grapple_stick", 3, 0.50, grapple_stick),
        sound!("grapple_release", 3, 0.35, grapple_release),
        sound!("reel", 4, 0.22, reel),
        sound!("door_open", 3, 0.55, door_open),
        sound!("door_close", 3, 0.58, door_close),
        sound!("click", 4, 0.25, click),
        sound!("craft", 3, 0.42, craft),
        sound!("coin", 3, 0.40, coin),
        sound!("notice", 2, 0.28, notice),
        sound!("warp", 2, 0.55, warp),
        sound!("meteor_land", 2, 0.95, meteor_land),
    ]
}

// ---- materials ----

/// One strike on `stuff`, at `t0`. `size` above 1 is bigger and lower;
/// `amp` scales it.
pub fn strike(buf: &mut [f32], t0: f32, stuff: Stuff, size: f32, amp: f32, rng: &mut Rng) {
    let s = size * rng.vary(0.08);
    match stuff {
        Stuff::Stone => {
            noise(buf, t0, 0.0005, 0.012, 0.9 * amp, Filter::Low(2_600.0 / s), rng);
            dsp::mode(buf, t0, 120.0 / s, -0.4, 0.001, 0.035, 0.8 * amp, 0.0);
            modes(
                buf,
                t0,
                560.0 / s,
                &[(1.0, 0.03, 0.35), (2.31, 0.022, 0.22), (3.87, 0.014, 0.14)],
                amp,
                rng,
            );
        }
        Stuff::Wood => {
            noise(buf, t0, 0.0005, 0.007, 0.6 * amp, Filter::Low(2_400.0 / s), rng);
            modes(
                buf,
                t0,
                330.0 / s,
                &[(1.0, 0.075, 0.6), (2.57, 0.045, 0.3), (4.2, 0.025, 0.15)],
                amp,
                rng,
            );
        }
        Stuff::Soil => {
            noise(buf, t0, 0.002, 0.05, 1.0 * amp, Filter::Low(750.0 / s), rng);
            dsp::mode(buf, t0, 85.0 / s, -0.3, 0.002, 0.05, 0.7 * amp, 0.0);
        }
        Stuff::Plant => {
            // A rustle with a flutter, and sometimes a twig.
            let len = 0.11 * s;
            let start = dsp::idx(t0);
            let mut bq = dsp::Biquad::default();
            bq.set(Filter::Band(1_700.0 / s, 0.8));
            let rate = rng.range(35.0, 60.0);
            for i in 0..(len * 3.0 * dsp::RATE as f32) as usize {
                let Some(o) = buf.get_mut(start + i) else { break };
                let t = i as f32 / dsp::RATE as f32;
                let flutter = 0.6 + 0.4 * (std::f32::consts::TAU * rate * t).sin();
                *o += bq.run(rng.signed()) * dsp::ad(t, 0.008, len) * flutter * 1.6 * amp;
            }
            if rng.unit() < 0.5 {
                noise(
                    buf,
                    t0 + rng.range(0.0, 0.03),
                    0.0003,
                    0.004,
                    0.4 * amp,
                    Filter::Band(3_500.0, 1.5),
                    rng,
                );
            }
        }
        Stuff::Metal => {
            noise(buf, t0, 0.0003, 0.004, 0.4 * amp, Filter::High(1_500.0), rng);
            modes(
                buf,
                t0,
                620.0 / s,
                &[
                    (1.0, 0.35, 0.45),
                    (2.76, 0.22, 0.3),
                    (5.4, 0.13, 0.18),
                    (8.93, 0.08, 0.08),
                ],
                amp,
                rng,
            );
        }
        Stuff::Glass => {
            noise(buf, t0, 0.0003, 0.003, 0.35 * amp, Filter::High(2_500.0), rng);
            modes(
                buf,
                t0,
                1_350.0 / s,
                &[
                    (1.0, 0.2, 0.4),
                    (2.32, 0.14, 0.25),
                    (4.25, 0.08, 0.14),
                    (6.63, 0.05, 0.08),
                ],
                amp,
                rng,
            );
        }
        Stuff::Crystal => {
            noise(buf, t0, 0.0005, 0.004, 0.25 * amp, Filter::Band(3_000.0, 1.0), rng);
            modes(
                buf,
                t0,
                880.0 / s,
                &[
                    (1.0, 0.55, 0.45),
                    (2.0, 0.38, 0.25),
                    (3.01, 0.24, 0.15),
                    (4.16, 0.15, 0.1),
                ],
                amp,
                rng,
            );
        }
        Stuff::Sand => {
            noise(buf, t0, 0.012, 0.06 * s, 0.9 * amp, Filter::Band(1_000.0 / s, 0.7), rng);
        }
        Stuff::Snow => {
            // A crunch: many tiny grains packed close.
            for _ in 0..14 {
                let t = t0 + rng.range(0.0, 0.09 * s);
                noise(
                    buf,
                    t,
                    0.0003,
                    0.003,
                    rng.range(0.3, 0.9) * amp,
                    Filter::Band(2_200.0, 0.9),
                    rng,
                );
            }
            noise(buf, t0, 0.004, 0.04, 0.4 * amp, Filter::Low(600.0), rng);
        }
        Stuff::Water => {
            noise(buf, t0, 0.006, 0.07 * s, 0.7 * amp, Filter::Band(900.0, 0.8), rng);
            for _ in 0..3 {
                let t = t0 + rng.range(0.0, 0.08);
                let f = rng.range(500.0, 1_100.0) / s;
                dsp::mode(buf, t, f, 0.8, 0.002, 0.012, 0.35 * amp, 0.0);
            }
        }
    }
}

fn swing(r: &mut Rng, len: f32, f0: f32, f1: f32, q: f32) -> Vec<f32> {
    let len = len * r.vary(0.08);
    let mut b = buffer(len + 0.05);
    whoosh(&mut b, 0.0, len, f0 * r.vary(0.07), f1 * r.vary(0.07), q, 1.0, r);
    b
}

fn hit(r: &mut Rng, stuff: Stuff) -> Vec<f32> {
    let mut b = buffer(0.6);
    strike(&mut b, 0.0, stuff, 1.0, 1.0, r);
    b
}

/// A block giving way: a deeper body, then pieces falling.
fn crumble(r: &mut Rng, stuff: Stuff) -> Vec<f32> {
    let mut b = buffer(1.0);
    strike(&mut b, 0.0, stuff, 1.35, 1.0, r);
    let pieces = match stuff {
        Stuff::Plant => 3,
        Stuff::Glass | Stuff::Crystal => 9,
        _ => 6,
    };
    for i in 0..pieces {
        let t = 0.02 + (i as f32 / pieces as f32) * 0.25 + r.range(0.0, 0.04);
        let size = r.range(0.45, 0.8);
        strike(
            &mut b,
            t,
            stuff,
            size,
            r.range(0.2, 0.5) * (1.0 - i as f32 / (pieces as f32 * 1.4)),
            r,
        );
    }
    if matches!(stuff, Stuff::Stone | Stuff::Soil) {
        // The settling dust.
        noise(&mut b, 0.03, 0.03, 0.12, 0.25, Filter::Low(1_200.0), r);
    }
    b
}

fn place_soft(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.3);
    noise(&mut b, 0.0, 0.002, 0.03, 0.8, Filter::Low(900.0 * r.vary(0.1)), r);
    dsp::mode(&mut b, 0.0, 140.0 * r.vary(0.08), -0.3, 0.002, 0.045, 0.8, 0.0);
    b
}

fn place_hard(r: &mut Rng) -> Vec<f32> {
    let mut b = place_soft(r);
    strike(&mut b, 0.004, Stuff::Wood, 1.6, 0.5, r);
    b
}

fn chisel(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.3);
    strike(&mut b, 0.0, Stuff::Stone, 0.55, 0.7, r);
    noise(
        &mut b,
        0.01,
        0.01,
        0.03,
        0.35,
        Filter::Band(3_200.0 * r.vary(0.1), 2.0),
        r,
    );
    b
}

fn pickup(r: &mut Rng) -> Vec<f32> {
    // A soft rising pop.
    let mut b = buffer(0.3);
    let f = 420.0 * r.vary(0.05);
    dsp::mode(&mut b, 0.0, f, 0.9, 0.004, 0.05, 0.8, 0.0);
    dsp::mode(&mut b, 0.035, f * 1.5, 0.3, 0.004, 0.06, 0.45, 0.0);
    b
}

fn step(r: &mut Rng, stuff: Stuff) -> Vec<f32> {
    let mut b = buffer(0.35);
    let size = match stuff {
        Stuff::Wood => 1.25,
        Stuff::Stone => 1.6,
        _ => 1.0,
    };
    strike(&mut b, 0.0, stuff, size, 1.0, r);
    if matches!(stuff, Stuff::Soil) {
        // Turf and soil: a little grass in it.
        strike(&mut b, 0.005, Stuff::Plant, 1.2, 0.25, r);
    }
    b
}

fn jump(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.25);
    whoosh(&mut b, 0.0, 0.16, 250.0, 600.0 * r.vary(0.1), 1.4, 0.8, r);
    noise(&mut b, 0.0, 0.002, 0.02, 0.4, Filter::Low(700.0), r);
    b
}

fn land(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.4);
    dsp::mode(&mut b, 0.0, 75.0 * r.vary(0.08), -0.5, 0.002, 0.07, 1.0, 0.0);
    noise(&mut b, 0.0, 0.002, 0.05, 0.7, Filter::Low(700.0), r);
    b
}

fn splash(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.9);
    noise(&mut b, 0.0, 0.01, 0.16, 1.0, Filter::Band(1_100.0, 0.6), r);
    for _ in 0..8 {
        let t = r.range(0.02, 0.4);
        dsp::mode(&mut b, t, r.range(500.0, 1_300.0), 0.9, 0.002, 0.014, 0.3, 0.0);
    }
    b
}

fn hurt(r: &mut Rng) -> Vec<f32> {
    // A muffled thump and a short falling tone: felt, not shrill.
    let mut b = buffer(0.4);
    noise(&mut b, 0.0, 0.002, 0.04, 0.8, Filter::Low(600.0), r);
    let f = 260.0 * r.vary(0.06);
    dsp::mode(&mut b, 0.0, f, -0.7, 0.006, 0.08, 0.6, 0.0);
    dsp::mode(&mut b, 0.0, f * 2.0, -0.7, 0.006, 0.05, 0.2, 0.0);
    b
}

fn defeat(r: &mut Rng) -> Vec<f32> {
    // Three falling soft notes (a minor triad, downward).
    let mut b = buffer(2.2);
    let root = 330.0 * r.vary(0.03);
    for (i, ratio) in [1.5f32, 1.2, 1.0].into_iter().enumerate() {
        let t = i as f32 * 0.28;
        for (h, a) in [(1.0, 0.6), (2.0, 0.18), (3.0, 0.07)] {
            dsp::mode(&mut b, t, root * ratio * h, 0.0, 0.03, 0.45, a, 0.0);
        }
    }
    b
}

fn creature_hit(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.35);
    noise(&mut b, 0.0, 0.001, 0.035, 0.9, Filter::Low(1_000.0 * r.vary(0.1)), r);
    dsp::mode(&mut b, 0.0, 190.0 * r.vary(0.1), -0.6, 0.002, 0.05, 0.7, 0.0);
    b
}

fn windup(r: &mut Rng) -> Vec<f32> {
    // A low rising growl with a tremble.
    let len = 0.55 * r.vary(0.08);
    let mut b = buffer(len + 0.1);
    let f0 = 85.0 * r.vary(0.1);
    let mut ph = 0.0f32;
    let mut bq = dsp::Biquad::default();
    bq.set(Filter::Low(500.0));
    for (i, o) in b.iter_mut().enumerate() {
        let t = i as f32 / dsp::RATE as f32;
        let f = f0 * (1.0 + 0.8 * (t / len).min(1.0));
        ph += std::f32::consts::TAU * f / dsp::RATE as f32;
        let tremble = 0.7 + 0.3 * (std::f32::consts::TAU * 13.0 * t).sin();
        let voice = ph.sin() + 0.5 * (2.0 * ph).sin() + 0.25 * (3.0 * ph).sin();
        *o = bq.run(voice + 0.3 * r.signed()) * dsp::hump(t, len) * tremble;
    }
    b
}

fn whistle(r: &mut Rng) -> Vec<f32> {
    // Rising, with vibrato: a warning, never a screech.
    let len = 1.0;
    let mut b = buffer(len + 0.05);
    let f0 = 950.0 * r.vary(0.04);
    let mut ph = 0.0f32;
    for (i, o) in b.iter_mut().enumerate() {
        let t = i as f32 / dsp::RATE as f32;
        let f = f0 * (1.0 + 0.7 * t / len) * (1.0 + 0.02 * (std::f32::consts::TAU * 7.0 * t).sin());
        ph += std::f32::consts::TAU * f / dsp::RATE as f32;
        let env = (t / 0.06).min(1.0) * (1.0 - ((t - len + 0.05) / 0.05).clamp(0.0, 1.0));
        *o = (ph.sin() + 0.15 * (2.0 * ph).sin()) * env;
    }
    noise(&mut b, 0.0, 0.05, 0.6, 0.05, Filter::Band(f0 * 1.3, 4.0), r);
    b
}

fn sling(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.35);
    dsp::mode(&mut b, 0.0, 210.0 * r.vary(0.06), -0.6, 0.002, 0.06, 0.7, 0.0);
    whoosh(&mut b, 0.01, 0.18, 600.0, 1_400.0, 2.5, 0.6, r);
    b
}

fn blast(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(2.2);
    // Body: low noise whose top falls, and a dropping sine.
    let start_cut = 2_200.0 * r.vary(0.1);
    let mut bq = dsp::Biquad::default();
    for (i, o) in b.iter_mut().enumerate() {
        let t = i as f32 / dsp::RATE as f32;
        if i % 32 == 0 {
            bq.set(Filter::Low(start_cut * (-t * 3.0).exp() + 120.0));
        }
        *o += bq.run(r.signed()) * dsp::ad(t, 0.004, 0.35) * 1.4;
    }
    dsp::mode(&mut b, 0.0, 55.0 * r.vary(0.08), -1.0, 0.003, 0.25, 1.0, 0.0);
    for _ in 0..14 {
        let t = r.range(0.05, 0.7);
        noise(
            &mut b,
            t,
            0.0005,
            0.006,
            r.range(0.05, 0.2),
            Filter::Band(1_600.0, 1.0),
            r,
        );
    }
    b
}

fn grapple_fire(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.4);
    noise(&mut b, 0.0, 0.0005, 0.005, 0.5, Filter::High(1_800.0), r);
    whoosh(&mut b, 0.0, 0.26, 900.0, 2_600.0 * r.vary(0.08), 3.0, 0.8, r);
    b
}

fn grapple_stick(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.5);
    strike(&mut b, 0.0, Stuff::Wood, 0.9, 0.9, r);
    strike(&mut b, 0.0, Stuff::Metal, 1.4, 0.25, r);
    b
}

fn grapple_release(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.3);
    noise(&mut b, 0.0, 0.0005, 0.004, 0.5, Filter::Band(2_200.0, 1.2), r);
    whoosh(&mut b, 0.0, 0.12, 1_400.0, 700.0, 2.0, 0.4, r);
    b
}

fn reel(r: &mut Rng) -> Vec<f32> {
    // One ratchet tick: the game plays them as the rope comes in.
    let mut b = buffer(0.15);
    noise(
        &mut b,
        0.0,
        0.0003,
        0.003,
        0.7,
        Filter::Band(2_800.0 * r.vary(0.1), 1.5),
        r,
    );
    strike(&mut b, 0.0, Stuff::Metal, 2.2, 0.15, r);
    b
}

fn creak(b: &mut [f32], t0: f32, len: f32, f0: f32, r: &mut Rng) {
    // A hinge: a band of noise pulsed by a slow, uneven stick-slip.
    let start = dsp::idx(t0);
    let mut bq = dsp::Biquad::default();
    let mut ph = 0.0f32;
    for i in 0..(len * dsp::RATE as f32) as usize {
        let Some(o) = b.get_mut(start + i) else { break };
        let t = i as f32 / dsp::RATE as f32;
        let f = f0 * (1.0 + 0.25 * (t / len));
        if i % 32 == 0 {
            bq.set(Filter::Band(f * 3.0, 6.0));
        }
        ph += std::f32::consts::TAU * f * 0.12 / dsp::RATE as f32;
        let pulse = (0.5 + 0.5 * ph.sin()).powi(3);
        *o += bq.run(r.signed()) * pulse * dsp::hump(t, len) * 2.0;
    }
}

fn door_open(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.7);
    strike(&mut b, 0.0, Stuff::Metal, 2.4, 0.25, r);
    creak(&mut b, 0.03, 0.4 * r.vary(0.1), 210.0 * r.vary(0.1), r);
    b
}

fn door_close(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.6);
    creak(&mut b, 0.0, 0.18, 240.0 * r.vary(0.1), r);
    strike(&mut b, 0.17, Stuff::Wood, 1.8, 1.0, r);
    strike(&mut b, 0.18, Stuff::Metal, 2.4, 0.2, r);
    b
}

fn click(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.12);
    modes(
        &mut b,
        0.0,
        1_150.0 * r.vary(0.04),
        &[(1.0, 0.012, 0.8), (2.4, 0.007, 0.3)],
        1.0,
        r,
    );
    b
}

fn chime(b: &mut [f32], t0: f32, f: f32, decay: f32, amp: f32) {
    for (h, a) in [(1.0, 0.6), (2.0, 0.2), (3.0, 0.08)] {
        dsp::mode(b, t0, f * h, 0.0, 0.004, decay / h, a * amp, 0.0);
    }
}

fn craft(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(1.0);
    strike(&mut b, 0.0, Stuff::Wood, 1.2, 0.5, r);
    let f = 523.0 * r.vary(0.02);
    chime(&mut b, 0.05, f, 0.3, 0.6);
    chime(&mut b, 0.16, f * 1.5, 0.35, 0.6);
    b
}

fn coin(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.9);
    let f = 1_500.0 * r.vary(0.03);
    modes(
        &mut b,
        0.0,
        f,
        &[(1.0, 0.25, 0.5), (1.52, 0.2, 0.35), (2.7, 0.1, 0.15)],
        1.0,
        r,
    );
    modes(&mut b, 0.08, f * 1.33, &[(1.0, 0.3, 0.5), (1.52, 0.22, 0.3)], 0.8, r);
    b
}

fn notice(r: &mut Rng) -> Vec<f32> {
    let mut b = buffer(0.6);
    chime(&mut b, 0.0, 660.0 * r.vary(0.02), 0.2, 1.0);
    b
}

fn warp(r: &mut Rng) -> Vec<f32> {
    // A shimmer rising through a soft whoosh.
    let len = 1.6;
    let mut b = buffer(len + 0.3);
    whoosh(&mut b, 0.0, len, 300.0, 1_800.0, 1.2, 0.6, r);
    let base = 220.0 * r.vary(0.03);
    for k in 0..6 {
        let t = k as f32 * 0.16;
        dsp::mode(&mut b, t, base * (1.0 + k as f32 * 0.25), 1.0, 0.08, 0.35, 0.25, 0.0);
    }
    b
}

fn meteor_land(r: &mut Rng) -> Vec<f32> {
    let mut b = blast(r);
    b.resize(b.len() + dsp::RATE as usize, 0.0);
    // A long rumble after.
    noise(&mut b, 0.1, 0.2, 0.7, 0.6, Filter::Low(160.0), r);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_code_makes_the_same_sounds_and_variants_differ() {
        for s in sounds() {
            let a = s.render(1);
            assert_eq!(a, s.render(1), "{} is not deterministic", s.name);
            assert!(!a.is_empty() && a.iter().all(|v| v.is_finite()), "{}", s.name);
            let peak = a.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!((peak - s.peak).abs() < 0.01, "{} peaks at {peak}", s.name);
            if s.variants > 1 {
                assert_ne!(a, s.render(2), "{} variants are identical", s.name);
            }
        }
    }

    #[test]
    fn names_are_unique_and_sounds_are_short() {
        let all = sounds();
        let mut names: Vec<_> = all.iter().map(|s| s.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), all.len());
        for s in &all {
            let secs = s.render(1).len() as f32 / dsp::RATE as f32;
            assert!(secs < 3.5, "{} lasts {secs} s", s.name);
        }
    }
}
