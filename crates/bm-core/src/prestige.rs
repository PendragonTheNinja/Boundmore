//! Prestige (milestone 24 part 4, `docs/milestones/24-coin-and-trade.md`,
//! research `docs/research/prestige-and-settlements.md`).
//!
//! A claim's prestige comes from the blocks placed in it (untouched ground
//! counts nothing): each kind of block by its rank's points, full for its
//! first blocks and less after, plus bonuses for the variety of kinds and of
//! chisel shapes. Boundless's own formula was never published; this is
//! Boundmore's, shaped by its rules (Nathan, 2026-10-28). Adding a block,
//! chiselling one or adding a plot never lowers it. Every number is a
//! **guess** to calibrate (Measure list).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Points one block of `rank` is worth: 2 + 2^(rank / 3), so a natural
/// block (rank 1) about 3, refined rock (4) about 4.5, a carved block (12)
/// 18 and an advanced coil (24) 258 (the research measured about 280 for
/// Boundless's advanced coil).
pub fn points(rank: u8) -> f64 {
    if rank == 0 {
        return 0.0;
    }
    2.0 + libm::pow(2.0, rank as f64 / 3.0)
}

/// Blocks of one kind that count in full; past this each counts less (the
/// square root of the rest, times [`REPEAT_SCALE`]).
pub const FULL_BLOCKS: u32 = 200;
pub const REPEAT_SCALE: f64 = 20.0;
/// A kind counts toward variety from this many blocks.
pub const VARIETY_MIN: u32 = 20;
/// Bonus per kind of block that counts toward variety.
pub const VARIETY_BONUS: u64 = 100;
/// Bonus per different chisel shape used, and per chiselled block (up to
/// [`CHISEL_COUNT_CAP`]).
pub const SHAPE_BONUS: u64 = 50;
pub const CHISEL_COUNT_CAP: u32 = 1000;

/// How many blocks of one kind count: all of the first [`FULL_BLOCKS`],
/// then the square root of the rest.
pub fn counted(n: u32) -> f64 {
    let full = n.min(FULL_BLOCKS) as f64;
    let rest = n.saturating_sub(FULL_BLOCKS) as f64;
    full + REPEAT_SCALE * libm::sqrt(rest)
}

/// What a claim holds toward its prestige.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    /// Placed blocks by block name.
    pub blocks: BTreeMap<String, u32>,
    /// Different chisel shapes used (their shape codes).
    pub shapes: BTreeSet<u16>,
    /// Blocks chiselled.
    pub chiselled: u32,
}

impl Tally {
    pub fn placed(&mut self, name: &str) {
        *self.blocks.entry(name.to_string()).or_insert(0) += 1;
    }

    /// A block of this kind went (broken): taken off if any were counted.
    pub fn removed(&mut self, name: &str) {
        if let Some(n) = self.blocks.get_mut(name) {
            *n -= 1;
            if *n == 0 {
                self.blocks.remove(name);
            }
        }
    }

    pub fn chiselled(&mut self, shape: u16) {
        self.shapes.insert(shape);
        self.chiselled = self.chiselled.saturating_add(1);
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.shapes.is_empty() && self.chiselled == 0
    }

    /// The prestige, with `rank` giving each block name's rank.
    pub fn score(&self, rank: impl Fn(&str) -> u8) -> u64 {
        let mut total = 0.0;
        let mut kinds = 0u64;
        for (name, &n) in &self.blocks {
            let r = rank(name);
            total += points(r) * counted(n);
            if r > 0 && n >= VARIETY_MIN {
                kinds += 1;
            }
        }
        total as u64
            + kinds * VARIETY_BONUS
            + self.shapes.len() as u64 * SHAPE_BONUS
            + self.chiselled.min(CHISEL_COUNT_CAP) as u64
    }
}

/// A settlement's rank by its prestige (Boundless's thresholds), and the
/// base footfall it pays a visit (Boundless's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Rank {
    Outpost,
    Hamlet,
    Village,
    Town,
    City,
    GreatCity,
}

impl Rank {
    /// None below 10,000: not a settlement.
    pub fn of(prestige: u64) -> Option<Rank> {
        Some(match prestige {
            0..10_000 => return None,
            10_000..50_000 => Rank::Outpost,
            50_000..250_000 => Rank::Hamlet,
            250_000..1_250_000 => Rank::Village,
            1_250_000..6_500_000 => Rank::Town,
            6_500_000..32_000_000 => Rank::City,
            _ => Rank::GreatCity,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Rank::Outpost => "Outpost",
            Rank::Hamlet => "Hamlet",
            Rank::Village => "Village",
            Rank::Town => "Town",
            Rank::City => "City",
            Rank::GreatCity => "Great City",
        }
    }

    /// Base footfall coin a visit pays (Boundless's).
    pub fn footfall(self) -> u64 {
        match self {
            Rank::Outpost => 20,
            Rank::Hamlet => 30,
            Rank::Village => 40,
            Rank::Town => 60,
            Rank::City => 80,
            Rank::GreatCity => 100,
        }
    }
}

/// A claim earns footfall only with this much prestige of its own
/// (Boundless's Release 211 rule).
pub const FOOTFALL_MIN_PRESTIGE: u64 = 10_000;

/// Footfall one visit pays: the rank's base, doubled for a visitor not paid
/// here in the last 5 days, and for one paid 1 to 5 days ago 20% to 100% of
/// that by whole days since (Boundless's, players' measurements); nothing
/// within a day. Then stepped down by the claim's visitors in the last day
/// (**guess** at the steps: players saw 120 become about 84).
pub fn footfall(rank: Rank, days_since_paid: Option<u32>, visitors_today: u32) -> u64 {
    let full = rank.footfall() * 2;
    let share = match days_since_paid {
        None => 100,
        Some(0) => return 0,
        Some(d) if d >= 5 => 100,
        Some(d) => 20 * d as u64,
    };
    let busy = match visitors_today {
        0..=5 => 100,
        6..=20 => 70,
        21..=50 => 50,
        _ => 30,
    };
    full * share / 100 * busy / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rank(name: &str) -> u8 {
        match name {
            "stone" => 1,
            "refined_stone" => 4,
            "carved_stone" => 12,
            "advanced_mixer_coil" => 24,
            "air" => 0,
            _ => 1,
        }
    }

    #[test]
    fn points_rise_with_rank() {
        assert_eq!(points(0), 0.0);
        assert!((points(1) - 3.26).abs() < 0.01);
        assert!((points(12) - 18.0).abs() < 1e-9);
        assert!((points(24) - 258.0).abs() < 1e-9);
    }

    #[test]
    fn repeats_count_less_and_adding_never_lowers_it() {
        assert_eq!(counted(150), 150.0);
        assert_eq!(counted(300), 200.0 + 20.0 * 10.0);
        let mut t = Tally::default();
        let mut last = 0;
        for i in 0..1000 {
            t.placed(if i % 3 == 0 { "carved_stone" } else { "stone" });
            if i % 50 == 0 {
                t.chiselled((i % 7) as u16);
            }
            let s = t.score(rank);
            assert!(s >= last, "lowered at {i}");
            last = s;
        }
        // A tower of one kind scores less than the same number spread out.
        let mut one = Tally::default();
        let mut mixed = Tally::default();
        for i in 0..1200 {
            one.placed("carved_stone");
            mixed.placed(["carved_stone", "refined_stone", "stone"][i % 3]);
        }
        let carved_only = one.score(rank);
        let mut carved_spread = Tally::default();
        for _ in 0..400 {
            carved_spread.placed("carved_stone");
        }
        assert!(carved_spread.score(rank) * 3 > carved_only, "repeats count less");
        assert!(mixed.score(rank) > 0);
        one.removed("carved_stone");
        assert_eq!(one.blocks["carved_stone"], 1199);
        one.removed("nothing");
    }

    #[test]
    fn ranks_and_footfall_follow_boundless() {
        assert_eq!(Rank::of(9_999), None);
        assert_eq!(Rank::of(10_000), Some(Rank::Outpost));
        assert_eq!(Rank::of(300_000), Some(Rank::Village));
        assert_eq!(Rank::of(40_000_000), Some(Rank::GreatCity));
        assert_eq!(footfall(Rank::Outpost, None, 1), 40, "fresh: twice the base");
        assert_eq!(footfall(Rank::Outpost, Some(0), 1), 0, "once a day");
        assert_eq!(footfall(Rank::Outpost, Some(1), 1), 8, "20% the next day");
        assert_eq!(footfall(Rank::Outpost, Some(6), 1), 40);
        assert_eq!(footfall(Rank::Town, None, 10), 84, "busy: 120 to 84");
    }
}
