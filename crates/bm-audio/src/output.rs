//! Playback: a random variant at a slight random pitch, panned and scaled,
//! mixed by `rodio` on its own thread. The device opens on the first play;
//! if there is none, the game stays silent and nothing else changes.

use crate::bank::{Bank, Clip};
use crate::spatial::stereo;
use crate::{Sfx, Volumes};
use std::collections::HashMap;
use std::num::NonZero;
use std::path::Path;
use std::time::{Duration, Instant};

/// The most sounds playing at once; more are dropped, never queued
/// (Boundless lost sounds at its voice limit and kept losing them, topic
/// 22100: here a dropped sound is only ever that one).
pub const MAX_VOICES: usize = 32;
/// The most plays of one sound within `REPEAT_WINDOW`.
pub const MAX_REPEAT: usize = 3;
const REPEAT_WINDOW: Duration = Duration::from_millis(50);
/// Quieter than this is not worth a voice.
const INAUDIBLE: f32 = 0.002;
/// Each play's pitch varies this much either way.
const PITCH_SPREAD: f32 = 0.05;

/// Which volume slider a sound answers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Effects,
    Interface,
}

enum Device {
    /// Not tried yet.
    Unopened,
    Open(rodio::MixerDeviceSink),
    /// None, or turned off (`BOUNDMORE_SILENT`, tests).
    Silent,
}

pub struct Audio {
    bank: Bank,
    device: Device,
    volumes: Volumes,
    /// When each playing sound ends.
    voices: Vec<Instant>,
    recent: HashMap<Sfx, Vec<Instant>>,
    last_variant: HashMap<Sfx, usize>,
    rng: u64,
}

impl Audio {
    /// The sounds in `dir`; the device opens on the first play (none if
    /// `BOUNDMORE_SILENT` is set).
    pub fn new(dir: &Path) -> Audio {
        let silent = std::env::var_os("BOUNDMORE_SILENT").is_some_and(|v| !v.is_empty());
        Audio::with(Bank::load(dir), if silent { Device::Silent } else { Device::Unopened })
    }

    /// The sounds in `dir`, never played: every play is counted and dropped.
    pub fn silent(dir: &Path) -> Audio {
        Audio::with(Bank::load(dir), Device::Silent)
    }

    fn with(bank: Bank, device: Device) -> Audio {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64)
            | 1;
        Audio {
            bank,
            device,
            volumes: Volumes::default(),
            voices: Vec::new(),
            recent: HashMap::new(),
            last_variant: HashMap::new(),
            rng: seed,
        }
    }

    pub fn bank(&self) -> &Bank {
        &self.bank
    }

    /// A device is open and playing.
    pub fn audible(&self) -> bool {
        matches!(self.device, Device::Open(_))
    }

    pub fn set_volumes(&mut self, v: Volumes) {
        self.volumes = v.clamped();
    }

    /// An interface sound, flat.
    pub fn ui(&mut self, sfx: Sfx) -> bool {
        self.play(sfx, Bus::Interface, 1.0, 0.0)
    }

    /// Plays `sfx` at `gain` (0 to 1, before the volumes) and `pan` (-1
    /// left to 1 right). True if it went to a voice (or would have, with
    /// no device); false if it was dropped (silent, too many, a repeat, or
    /// no file).
    pub fn play(&mut self, sfx: Sfx, bus: Bus, gain: f32, pan: f32) -> bool {
        let now = Instant::now();
        let gain = gain.clamp(0.0, 1.5) * self.volumes.gain(bus);
        if gain < INAUDIBLE || !gain.is_finite() {
            return false;
        }
        self.voices.retain(|end| *end > now);
        if self.voices.len() >= MAX_VOICES {
            return false;
        }
        {
            let recent = self.recent.entry(sfx).or_default();
            recent.retain(|t| now.duration_since(*t) < REPEAT_WINDOW);
            if recent.len() >= MAX_REPEAT {
                return false;
            }
        }
        let n = self.bank.variants(sfx).len();
        if n == 0 {
            return false;
        }
        // A random variant, never the one just played.
        let last = self.last_variant.get(&sfx).copied();
        let mut pick = (self.next() % n as u64) as usize;
        if n > 1 && Some(pick) == last {
            pick = (pick + 1 + (self.next() % (n as u64 - 1)) as usize) % n;
        }
        self.last_variant.insert(sfx, pick);
        let pitch = 1.0 + (self.unit() * 2.0 - 1.0) * PITCH_SPREAD;
        let clip = self.bank.variants(sfx)[pick].clone();
        self.recent.entry(sfx).or_default().push(now);
        let length = Duration::from_secs_f64(clip.seconds() / pitch as f64);
        self.voices.push(now + length);
        self.output(&clip, gain, pan, pitch);
        true
    }

    fn output(&mut self, clip: &Clip, gain: f32, pan: f32, pitch: f32) {
        if matches!(self.device, Device::Unopened) {
            self.device = match rodio::DeviceSinkBuilder::open_default_sink() {
                Ok(mut sink) => {
                    sink.log_on_drop(false);
                    tracing::info!("sound on");
                    Device::Open(sink)
                }
                Err(e) => {
                    tracing::warn!("no sound: cannot open an audio device ({e}); playing silently");
                    Device::Silent
                }
            };
        }
        let Device::Open(sink) = &self.device else { return };
        let (l, r) = stereo(pan);
        let (l, r) = (l * gain, r * gain);
        let mut data: Vec<rodio::Sample> = Vec::with_capacity(clip.samples.len() * 2);
        for &s in clip.samples.iter() {
            data.push((s * l) as rodio::Sample);
            data.push((s * r) as rodio::Sample);
        }
        // A different pitch is the same samples at a different rate; the
        // mixer resamples to the device.
        let rate = ((clip.rate as f32 * pitch).round() as u32).max(1000);
        let (Some(channels), Some(rate)) = (NonZero::new(2u16), NonZero::new(rate)) else {
            return;
        };
        sink.mixer()
            .add(rodio::buffer::SamplesBuffer::new(channels, rate, data));
    }

    fn next(&mut self) -> u64 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sounds")
    }

    #[test]
    fn without_a_device_every_sound_plays_as_a_no_op() {
        let mut a = Audio::silent(&assets());
        for &s in Sfx::ALL {
            assert!(a.play(s, Bus::Effects, 0.5, -0.3), "{}", s.name());
            a.voices.clear();
        }
        assert!(!a.audible());
    }

    #[test]
    fn a_burst_of_one_sound_is_held_to_a_few_and_voices_to_the_limit() {
        let mut a = Audio::silent(&assets());
        let played = (0..10)
            .filter(|_| a.play(Sfx::BreakStone, Bus::Effects, 1.0, 0.0))
            .count();
        assert_eq!(played, MAX_REPEAT);
        let mut a = Audio::silent(&assets());
        let mut n = 0;
        for _ in 0..4 {
            for &s in Sfx::ALL {
                n += a.play(s, Bus::Effects, 1.0, 0.0) as usize;
            }
        }
        assert_eq!(n, MAX_VOICES);
    }

    #[test]
    fn muted_or_missing_sounds_take_no_voice() {
        let mut a = Audio::silent(&assets());
        a.set_volumes(Volumes {
            interface: 0,
            ..Volumes::default()
        });
        assert!(!a.ui(Sfx::Click));
        assert!(a.play(Sfx::Click, Bus::Effects, 1.0, 0.0));
        let mut none = Audio::silent(Path::new("no/such/folder"));
        assert!(!none.play(Sfx::Click, Bus::Effects, 1.0, 0.0));
    }

    #[test]
    fn variants_do_not_repeat_back_to_back() {
        let mut a = Audio::silent(&assets());
        let mut last = None;
        for _ in 0..40 {
            a.recent.clear();
            a.voices.clear();
            assert!(a.play(Sfx::StepStone, Bus::Effects, 1.0, 0.0));
            let v = a.last_variant[&Sfx::StepStone];
            assert_ne!(Some(v), last);
            last = Some(v);
        }
    }
}
