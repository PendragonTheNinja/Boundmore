//! The few building blocks every sound is made of: a seeded random source,
//! noise, filters, decaying partials (modes) and envelopes. Plain `f32`
//! arithmetic in a fixed order, so the same seed makes the same samples.

use std::f32::consts::{PI, TAU};

/// Samples a second. 22,050 keeps the folder small and suits soft sounds
/// (nothing above 11 kHz, which is where harshness lives).
pub const RATE: u32 = 22_050;

/// SplitMix64: small, fast and the same everywhere.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// In `0..1`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// In `-1..1`.
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// `1 ± spread`, for pitch and timing variety.
    pub fn vary(&mut self, spread: f32) -> f32 {
        1.0 + self.signed() * spread
    }
}

/// A mono buffer `seconds` long.
pub fn buffer(seconds: f32) -> Vec<f32> {
    vec![0.0; (seconds * RATE as f32).ceil() as usize]
}

pub fn idx(t: f32) -> usize {
    (t.max(0.0) * RATE as f32) as usize
}

/// Attack then exponential decay, peaking at 1.
pub fn ad(t: f32, attack: f32, decay: f32) -> f32 {
    if t < 0.0 {
        0.0
    } else if t < attack {
        let x = t / attack;
        x * x * (3.0 - 2.0 * x)
    } else {
        (-(t - attack) / decay).exp()
    }
}

/// A smooth hump over `0..len` (zero at both ends).
pub fn hump(t: f32, len: f32) -> f32 {
    if t <= 0.0 || t >= len {
        0.0
    } else {
        let s = (PI * t / len).sin();
        s * s
    }
}

/// A sine partial at `freq` (gliding by `glide` octaves over its life)
/// starting at `t0`, decaying with time constant `decay`.
#[allow(clippy::too_many_arguments)]
pub fn mode(buf: &mut [f32], t0: f32, freq: f32, glide: f32, attack: f32, decay: f32, amp: f32, phase: f32) {
    let start = idx(t0);
    let len = ((decay * 7.0 + attack) * RATE as f32) as usize;
    let mut ph = phase;
    for i in 0..len {
        let Some(s) = buf.get_mut(start + i) else { break };
        let t = i as f32 / RATE as f32;
        let f = freq * (glide * (t / (decay * 3.0 + attack)).min(1.0)).exp2();
        ph += TAU * f / RATE as f32;
        *s += ph.sin() * ad(t, attack, decay) * amp;
    }
}

/// Partials at `ratios` of `base`, each a little detuned, decaying faster the
/// higher they are: a struck object.
pub fn modes(buf: &mut [f32], t0: f32, base: f32, ratios: &[(f32, f32, f32)], amp: f32, rng: &mut Rng) {
    for &(ratio, decay, a) in ratios {
        let f = base * ratio * rng.vary(0.012);
        mode(buf, t0, f, 0.0, 0.0015, decay, a * amp, rng.unit() * TAU);
    }
}

/// White noise shaped by an attack-decay envelope, through a biquad.
#[allow(clippy::too_many_arguments)]
pub fn noise(buf: &mut [f32], t0: f32, attack: f32, decay: f32, amp: f32, filter: Filter, rng: &mut Rng) {
    let start = idx(t0);
    let len = ((decay * 7.0 + attack) * RATE as f32) as usize;
    let mut bq = filter.biquad();
    for i in 0..len {
        let Some(s) = buf.get_mut(start + i) else { break };
        let t = i as f32 / RATE as f32;
        *s += bq.run(rng.signed()) * ad(t, attack, decay) * amp;
    }
}

/// Noise through a band-pass whose centre sweeps from `f0` to `f1` under a
/// hump envelope: air moving (a swing, a throw, a zip).
#[allow(clippy::too_many_arguments)]
pub fn whoosh(buf: &mut [f32], t0: f32, len: f32, f0: f32, f1: f32, q: f32, amp: f32, rng: &mut Rng) {
    let start = idx(t0);
    let n = (len * RATE as f32) as usize;
    let mut bq = Biquad::default();
    for i in 0..n {
        let Some(s) = buf.get_mut(start + i) else { break };
        let t = i as f32 / RATE as f32;
        let x = t / len;
        // Rises then falls in pitch with the hump, as a blade passing by.
        let bend = (PI * x).sin();
        let f = f0 + (f1 - f0) * bend;
        if i % 16 == 0 {
            bq.set(Filter::Band(f, q));
        }
        *s += bq.run(rng.signed()) * hump(t, len) * amp;
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Filter {
    Low(f32),
    High(f32),
    Band(f32, f32),
}

impl Filter {
    fn biquad(self) -> Biquad {
        let mut b = Biquad::default();
        b.set(self);
        b
    }
}

/// An RBJ cookbook biquad.
#[derive(Default)]
pub struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    pub fn set(&mut self, f: Filter) {
        let (freq, q) = match f {
            Filter::Low(c) | Filter::High(c) => (c, 0.707),
            Filter::Band(c, q) => (c, q),
        };
        let w = TAU * freq.clamp(20.0, RATE as f32 * 0.45) / RATE as f32;
        let (sw, cw) = w.sin_cos();
        let alpha = sw / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b, a) = match f {
            Filter::Low(_) => ([(1.0 - cw) / 2.0, 1.0 - cw, (1.0 - cw) / 2.0], [-2.0 * cw, 1.0 - alpha]),
            Filter::High(_) => (
                [(1.0 + cw) / 2.0, -(1.0 + cw), (1.0 + cw) / 2.0],
                [-2.0 * cw, 1.0 - alpha],
            ),
            Filter::Band(..) => ([alpha, 0.0, -alpha], [-2.0 * cw, 1.0 - alpha]),
        };
        self.b = b.map(|v| v / a0);
        self.a = a.map(|v| v / a0);
    }

    pub fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// Filters a whole buffer.
pub fn filter(buf: &mut [f32], f: Filter) {
    let mut bq = f.biquad();
    for s in buf.iter_mut() {
        *s = bq.run(*s);
    }
}

/// The finish every sound gets: no DC, a gentle top cut (soft, warm),
/// a soft limit, 2 ms fades, then scaled to `peak`; trailing silence
/// trimmed.
pub fn finish(mut buf: Vec<f32>, peak: f32) -> Vec<f32> {
    filter(&mut buf, Filter::High(35.0));
    filter(&mut buf, Filter::Low(7_500.0));
    let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-9);
    for s in buf.iter_mut() {
        *s = (*s / max * 1.2).tanh();
    }
    // Trim the tail below -48 dB of the peak, then fade the last 15 ms
    // (2 ms at the start) so nothing clicks.
    let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-9);
    let end = buf.iter().rposition(|s| s.abs() > max * 0.004).map_or(0, |i| i + 1);
    buf.truncate(end.max(64));
    let n = buf.len();
    let head = (RATE as usize / 500).min(n / 2);
    for (i, s) in buf.iter_mut().take(head).enumerate() {
        *s *= i as f32 / head as f32;
    }
    let tail = (RATE as usize * 15 / 1000).min(n / 2);
    for i in 0..tail {
        buf[n - 1 - i] *= i as f32 / tail as f32;
    }
    let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-9);
    for s in buf.iter_mut() {
        *s *= peak / max;
    }
    buf
}

const IMA_STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107,
    118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963,
    1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894,
    6484, 7132, 7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794,
    32767,
];
const IMA_INDEX: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];
/// Bytes in one ADPCM block, and the samples it holds (the header's one,
/// then two a byte).
const ADPCM_BLOCK: usize = 512;
const ADPCM_SAMPLES: usize = (ADPCM_BLOCK - 4) * 2 + 1;

/// The IMA ADPCM state: the predicted sample and the step index.
#[derive(Clone, Copy)]
struct Ima {
    pred: i32,
    index: i32,
}

impl Ima {
    fn encode(&mut self, sample: i32) -> u8 {
        let step = IMA_STEPS[self.index as usize];
        let mut diff = sample - self.pred;
        let mut nibble = 0u8;
        if diff < 0 {
            nibble = 8;
            diff = -diff;
        }
        let (mut s, mut vp) = (step, step >> 3);
        for bit in [4u8, 2, 1] {
            if diff >= s {
                nibble |= bit;
                diff -= s;
                vp += s;
            }
            s >>= 1;
        }
        self.apply(nibble, vp);
        nibble
    }

    fn decode(&mut self, nibble: u8) -> i32 {
        let step = IMA_STEPS[self.index as usize];
        let mut vp = step >> 3;
        if nibble & 4 != 0 {
            vp += step;
        }
        if nibble & 2 != 0 {
            vp += step >> 1;
        }
        if nibble & 1 != 0 {
            vp += step >> 2;
        }
        self.apply(nibble, vp);
        self.pred
    }

    fn apply(&mut self, nibble: u8, vp: i32) {
        self.pred = if nibble & 8 != 0 {
            self.pred - vp
        } else {
            self.pred + vp
        }
        .clamp(-32768, 32767);
        self.index = (self.index + IMA_INDEX[(nibble & 7) as usize]).clamp(0, 88);
    }
}

fn pcm16(s: f32) -> i32 {
    (s.clamp(-1.0, 1.0) * 32767.0).round() as i32
}

/// IMA ADPCM WAV, mono: a quarter the size of 16-bit PCM, which keeps the
/// sound folder small. Every common decoder reads it.
pub fn wav_adpcm(samples: &[f32]) -> Vec<u8> {
    let blocks = samples.len().div_ceil(ADPCM_SAMPLES).max(1);
    let data = (blocks * ADPCM_BLOCK) as u32;
    let mut out = Vec::with_capacity(60 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(4 + 28 + 12 + 8 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&20u32.to_le_bytes());
    out.extend_from_slice(&0x11u16.to_le_bytes()); // IMA ADPCM
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&((RATE as usize * ADPCM_BLOCK / ADPCM_SAMPLES) as u32).to_le_bytes());
    out.extend_from_slice(&(ADPCM_BLOCK as u16).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&(ADPCM_SAMPLES as u16).to_le_bytes());
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    let mut ima = Ima { pred: 0, index: 0 };
    for b in 0..blocks {
        let at = |i: usize| samples.get(b * ADPCM_SAMPLES + i).copied().map_or(0, pcm16);
        // Each block starts from its first sample exactly, keeping the
        // step index the encoder had reached.
        ima.pred = at(0);
        out.extend_from_slice(&(ima.pred as i16).to_le_bytes());
        out.push(ima.index as u8);
        out.push(0);
        for i in (1..ADPCM_SAMPLES).step_by(2) {
            let lo = ima.encode(at(i));
            let hi = ima.encode(at(i + 1));
            out.push(lo | (hi << 4));
        }
    }
    out
}

/// Decodes what `wav_adpcm` wrote (for tests).
pub fn read_adpcm(bytes: &[u8]) -> Vec<f32> {
    let count = u32::from_le_bytes(bytes[48..52].try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(count);
    for block in bytes[60..].chunks(ADPCM_BLOCK) {
        let mut ima = Ima {
            pred: i16::from_le_bytes([block[0], block[1]]) as i32,
            index: block[2] as i32,
        };
        out.push(ima.pred);
        for &byte in &block[4..] {
            out.push(ima.decode(byte & 15));
            out.push(ima.decode(byte >> 4));
        }
    }
    out.truncate(count);
    out.into_iter().map(|v| v as f32 / 32767.0).collect()
}

/// 16-bit mono PCM WAV.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_low_pass_quiets_high_notes_and_keeps_low_ones() {
        let tone = |f: f32| -> f32 {
            let mut b: Vec<f32> = (0..4410).map(|i| (TAU * f * i as f32 / RATE as f32).sin()).collect();
            filter(&mut b, Filter::Low(500.0));
            b[2000..].iter().fold(0.0f32, |m, s| m.max(s.abs()))
        };
        assert!(tone(100.0) > 0.9);
        assert!(tone(5000.0) < 0.05);
    }

    #[test]
    fn adpcm_comes_back_close_to_what_went_in() {
        let mut rng = Rng::new(7);
        let mut b = buffer(0.3);
        mode(&mut b, 0.0, 440.0, 0.0, 0.002, 0.08, 0.6, 0.0);
        noise(&mut b, 0.05, 0.001, 0.03, 0.3, Filter::Low(2_000.0), &mut rng);
        let bytes = wav_adpcm(&b);
        assert_eq!((bytes.len() - 60) % ADPCM_BLOCK, 0);
        let back = read_adpcm(&bytes);
        assert_eq!(back.len(), b.len());
        let err = b.iter().zip(&back).map(|(a, c)| (a - c).abs()).fold(0.0f32, f32::max);
        assert!(err < 0.05, "worst error {err}");
    }

    #[test]
    fn a_wav_has_a_proper_header_and_every_sample() {
        let w = wav(&[0.0, 1.0, -1.0]);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(w.len(), 44 + 6);
        assert_eq!(i16::from_le_bytes([w[46], w[47]]), 32767);
        assert_eq!(i16::from_le_bytes([w[48], w[49]]), -32767);
    }
}
