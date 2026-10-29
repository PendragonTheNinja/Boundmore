//! The sound files, decoded once at start into mono samples.

use crate::Sfx;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// The most variants a sound may have.
pub const MAX_VARIANTS: u32 = 8;

/// One variant, mono.
#[derive(Clone, Debug)]
pub struct Clip {
    pub rate: u32,
    pub samples: Arc<[f32]>,
}

impl Clip {
    pub fn seconds(&self) -> f64 {
        self.samples.len() as f64 / self.rate.max(1) as f64
    }
}

/// Every sound's variants, and the names of those with none.
#[derive(Default)]
pub struct Bank {
    clips: HashMap<Sfx, Vec<Clip>>,
    missing: Vec<&'static str>,
}

impl Bank {
    /// Reads `dir/<name>_<n>.ogg` or `.wav` for every sound, n from 1 up
    /// to the first gap. Unreadable files are skipped with a warning; a
    /// sound with no file at all is listed in `missing`.
    pub fn load(dir: &Path) -> Bank {
        let mut bank = Bank::default();
        for &sfx in Sfx::ALL {
            let mut clips = Vec::new();
            for n in 1..=MAX_VARIANTS {
                let stem = format!("{}_{n}", sfx.name());
                let Some(path) = ["ogg", "wav"]
                    .iter()
                    .map(|ext| dir.join(format!("{stem}.{ext}")))
                    .find(|p| p.is_file())
                else {
                    break;
                };
                match decode(&path) {
                    Ok(c) => clips.push(c),
                    Err(e) => tracing::warn!("cannot read sound {}: {e}", path.display()),
                }
            }
            if clips.is_empty() {
                bank.missing.push(sfx.name());
            } else {
                bank.clips.insert(sfx, clips);
            }
        }
        if !bank.missing.is_empty() {
            tracing::warn!(
                "{} sounds missing from {}: {}",
                bank.missing.len(),
                dir.display(),
                bank.missing.join(", ")
            );
        }
        bank
    }

    pub fn variants(&self, sfx: Sfx) -> &[Clip] {
        self.clips.get(&sfx).map_or(&[], Vec::as_slice)
    }

    /// Sounds with no file.
    pub fn missing(&self) -> &[&'static str] {
        &self.missing
    }
}

fn decode(path: &Path) -> Result<Clip, String> {
    use rodio::Source;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let dec = rodio::Decoder::try_from(file).map_err(|e| e.to_string())?;
    let channels = dec.channels().get() as usize;
    let rate = dec.sample_rate().get();
    let all: Vec<f32> = dec.collect();
    // Mixed down to mono: the game pans it itself.
    let samples: Vec<f32> = all
        .chunks(channels.max(1))
        .map(|f| f.iter().sum::<f32>() / f.len() as f32)
        .collect();
    if samples.is_empty() {
        return Err("no samples".into());
    }
    Ok(Clip {
        rate,
        samples: samples.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sounds")
    }

    #[test]
    fn every_sound_the_game_names_has_a_file_that_reads() {
        let bank = Bank::load(&assets());
        assert!(bank.missing().is_empty(), "missing: {:?}", bank.missing());
        for &s in Sfx::ALL {
            let v = bank.variants(s);
            assert!(!v.is_empty(), "{}", s.name());
            for c in v {
                assert!(
                    c.seconds() > 0.005 && c.seconds() < 4.0,
                    "{} lasts {}",
                    s.name(),
                    c.seconds()
                );
                assert!(c.samples.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            }
        }
    }

    #[test]
    fn a_missing_folder_is_every_sound_missing_never_a_crash() {
        let bank = Bank::load(Path::new("no/such/folder"));
        assert_eq!(bank.missing().len(), Sfx::ALL.len());
        assert!(bank.variants(Sfx::Click).is_empty());
    }
}
