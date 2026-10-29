//! Sound (milestone 30): the sounds the game names, the files that hold
//! them, where a sound in the world is heard from, and playback.
//!
//! Sounds are data, as textures are: `assets/sounds/<name>_<n>.wav` (or
//! `.ogg`, which wins), made by `tools/bm-soundgen` and replaceable by hand.
//! A missing sound is a warning, never a crash. With no audio device the
//! game is silent and nothing else changes: every play is a no-op.
//!
//! Headless: this crate opens a device only when asked to play, and never
//! touches the game's state.

mod bank;
mod output;
mod sfx;
mod spatial;

pub use bank::{Bank, Clip};
pub use output::{Audio, Bus, MAX_REPEAT, MAX_VOICES};
pub use sfx::{Mat, Sfx};
pub use spatial::{Listener, PAN_MAX, Placement, place, stereo};

use serde::{Deserialize, Serialize};

/// The player's volumes, in percent (the options window).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Volumes {
    pub master: u8,
    /// Everything from the world: tools, steps, creatures, blasts.
    pub effects: u8,
    /// Menus, notices, coin.
    pub interface: u8,
}

impl Default for Volumes {
    fn default() -> Self {
        Volumes {
            master: 80,
            effects: 100,
            interface: 70,
        }
    }
}

impl Volumes {
    /// Held to 0 to 100 (a hand-edited file may hold anything).
    pub fn clamped(self) -> Self {
        Volumes {
            master: self.master.min(100),
            effects: self.effects.min(100),
            interface: self.interface.min(100),
        }
    }

    /// The gain a bus plays at, 0 to 1. Percent is mapped on a curve, so
    /// the slider's middle sounds like half as loud rather than nearly full.
    pub fn gain(self, bus: Bus) -> f32 {
        let bus = match bus {
            Bus::Effects => self.effects,
            Bus::Interface => self.interface,
        };
        curve(self.master) * curve(bus)
    }
}

fn curve(percent: u8) -> f32 {
    let x = percent.min(100) as f32 / 100.0;
    x * x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_multiply_and_hold_to_their_range() {
        let v = Volumes {
            master: 100,
            effects: 50,
            interface: 0,
        };
        assert!((v.gain(Bus::Effects) - 0.25).abs() < 1e-6);
        assert_eq!(v.gain(Bus::Interface), 0.0);
        let odd = Volumes {
            master: 250,
            effects: 101,
            interface: 7,
        }
        .clamped();
        assert_eq!((odd.master, odd.effects, odd.interface), (100, 100, 7));
        let off = Volumes { master: 0, ..v };
        assert_eq!(off.gain(Bus::Effects), 0.0);
    }
}
