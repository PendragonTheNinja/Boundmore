//! Energy, as in Boundless (milestone 10, `docs/milestones/10-energy.md`,
//! tuning 1.7): every swing, grapple shot and chisel cut costs energy from a
//! bar of [`MAX_ENERGY`]. It refills only after [`PAUSE_TICKS`] without a
//! use, then at [`REGEN_PER_TICK`]. A use that costs more than is left does
//! nothing but restart the pause (Boundless Release 199: holding the button
//! at zero keeps the bar from refilling).
//!
//! Energy lives in `PlayerState` as integers and is stepped in
//! `movement::step`, so client and server agree on every tick. Swings and
//! chisel cuts spend it right after the input they follow, on both sides.

use crate::movement::{PlayerState, TICK_HZ};

/// A full bar (Boundless's base; Zeal raises it once skills exist).
pub const MAX_ENERGY: u16 = 600;

/// Ticks after a use before the bar refills: 3.5 s (players timed 3 to 4 s).
pub const PAUSE_TICKS: u16 = (TICK_HZ * 7 / 2) as u16;

/// Refill per tick once the pause is over: 300 a second, empty to full in
/// 2 s. Boundless's base rate is not documented: a guess (`docs/feel.md`).
pub const REGEN_PER_TICK: u16 = 5;

impl PlayerState {
    /// Energy left.
    pub fn energy(&self) -> u16 {
        MAX_ENERGY.saturating_sub(self.spent)
    }

    /// Spends `cost` if there is that much left (and restarts the pause
    /// either way). Returns whether it was spent: a use with too little
    /// energy left does nothing.
    pub fn try_spend(&mut self, cost: u16) -> bool {
        self.rest = PAUSE_TICKS;
        if cost > self.energy() {
            return false;
        }
        self.spent += cost;
        true
    }

    /// One tick of rest: the pause runs down, then the bar refills.
    pub(crate) fn recover(&mut self) {
        if self.rest > 0 {
            self.rest -= 1;
        } else {
            self.spent = self.spent.saturating_sub(REGEN_PER_TICK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spending_and_refilling() {
        let mut s = PlayerState::default();
        assert_eq!(s.energy(), MAX_ENERGY, "a fresh body is rested");
        // A copper hammer (8 a swing) swings 75 times from full.
        let swings = (0..1000).take_while(|_| s.try_spend(8)).count();
        assert_eq!(swings, 75);
        assert_eq!(s.energy(), 0);
        // Too tired: nothing spent, the pause starts again.
        s.rest = 3;
        assert!(!s.try_spend(8));
        assert_eq!(s.rest, PAUSE_TICKS);
        // The pause, then the refill.
        for _ in 0..PAUSE_TICKS {
            s.recover();
        }
        assert_eq!(s.energy(), 0, "nothing back during the pause");
        s.recover();
        assert_eq!(s.energy(), REGEN_PER_TICK);
        for _ in 0..TICK_HZ * 2 {
            s.recover();
        }
        assert_eq!(s.energy(), MAX_ENERGY, "full within 2 s");
    }
}
