//! Meteorite rules (milestone 14, `docs/milestones/14-meteorites.md`).
//!
//! What the server decides with, kept here so the numbers live in one place
//! and are tested: when a meteor may fall, how big it is, its waves, and what
//! each player takes from its heart. Numbers marked **guess** are ours (rule
//! numbers refer to the spec); `docs/feel.md` lists them for tuning.

use serde::Deserialize;

use crate::movement::TICK_HZ;

const T: u64 = TICK_HZ as u64;

/// Meteors fall only on planets of this tier and higher (Boundless).
pub const MIN_WORLD_TIER: u8 = 3;
/// Never within this many blocks of a claim or a warp shrine (Boundless: 64 m
/// from a beacon).
pub const CLEARANCE: f64 = 64.0;
/// Players within this distance of the trigger set the meteor's size
/// (players' estimate of Boundless's, about 80 m).
pub const COUNT_RADIUS: f64 = 80.0;
/// A region rests this long after bringing a meteor: 2 to 3 hours
/// (Boundless, players' reports), picked at random in between.
pub const REGION_REST_TICKS: (u64, u64) = (2 * 3600 * T, 3 * 3600 * T);
/// The wandering trigger: squares this many blocks across...
pub const SQUARE: i64 = 300;
/// ...rest this long after being entered (**guess**: players thought about
/// 4 hours)...
pub const SQUARE_REST_TICKS: u64 = 4 * 3600 * T;
/// ...and call a meteor with this chance when entered fresh (**guess**).
pub const SQUARE_CHANCE: f64 = 0.25;
/// At most this many meteors active on one planet (**guess**).
pub const PLANET_CAP: usize = 3;
/// The fall lasts this long (**guess**).
pub const FALL_TICKS: u64 = 6 * T;
/// It lands this far from the trigger (**guess**).
pub const IMPACT_RANGE: (f64, f64) = (60.0, 140.0);
/// The crater and the shell ball (blocks).
pub const CRATER_RADIUS: i64 = 4;
pub const SHELL_RADIUS: i64 = 2;
/// Portals over the site, and how high (**guess**).
pub const PORTALS: usize = 3;
pub const PORTAL_HEIGHT: f64 = 12.0;
/// The meteor's zone: lives are given and kept inside it (**guess**).
pub const ZONE_RADIUS: f64 = 48.0;
/// Outside the zone a life holder has this long to come back (Boundless).
pub const LIFE_GRACE_TICKS: u64 = 10 * T;
/// A pause between one wave cleared and the next dropping (**guess**).
pub const WAVE_PAUSE_TICKS: u64 = 3 * T;
/// An uncleared meteor goes dormant this long after impact (**guess**).
pub const DORMANT_TICKS: u64 = 20 * 60 * T;
/// The heart stays to be looted this long after it opens (**guess**).
pub const HEART_TICKS: u64 = 30 * 60 * T;
/// The site's beam shows this long after impact (**guess**).
pub const BEAM_TICKS: u64 = 30 * 60 * T;
/// Breaking this many shell blocks gives one portal conduit (**guess**).
pub const SHELL_PER_CONDUIT: u32 = 4;
/// Meteor levels run from 1 to this.
pub const MAX_LEVEL: u8 = 6;

/// A meteor's level from the players counted at its start: 1 player is
/// level 1, 2 is 2, 4 is 3, 8 is 4, 16 is 5, 32 or more is 6 (**guess**,
/// shaped to Boundless's 1 to 100 players and levels 1 to 6).
pub fn level_for(players: usize) -> u8 {
    let p = players.max(1);
    (1 + (usize::BITS - 1 - p.leading_zeros()) as u8).min(MAX_LEVEL)
}

/// Waves a meteor of `level` drops (**guess**).
pub fn waves(level: u8) -> u8 {
    2 + level
}

/// Creatures in each wave (**guess**).
pub fn wave_size(level: u8) -> u8 {
    2 + level
}

/// Which creature tier a wave member gets, given the planet's tier range:
/// the last wave leans to the top tier, earlier ones spread evenly; `roll`
/// is in 0..1.
pub fn wave_tier(range: std::ops::RangeInclusive<u8>, last_wave: bool, roll: f64) -> u8 {
    let (lo, hi) = (*range.start(), *range.end());
    let span = (hi - lo) as f64 + 1.0;
    let r = if last_wave {
        1.0 - (1.0 - roll) * (1.0 - roll)
    } else {
        roll
    };
    (lo + (r * span).floor() as u8).min(hi)
}

/// One possible roll on the heart's table.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct LootEntry {
    pub item: String,
    pub weight: u32,
    pub min: u32,
    pub max: u32,
}

/// The heart's loot (`assets/meteors.toml`).
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct MeteorLoot {
    pub wayshards_base: u32,
    pub wayshards_per_level: u32,
    pub tier_bonus: f64,
    pub rolls_min: u32,
    pub rolls_max: u32,
    pub dormant_share: f64,
    pub dormant_rolls: u32,
    pub loot: Vec<LootEntry>,
}

impl MeteorLoot {
    pub fn from_toml(source: &str) -> Result<Self, String> {
        let t: Self = toml::from_str(source).map_err(|e| e.to_string())?;
        if t.loot.is_empty() || t.loot.iter().any(|e| e.weight == 0 || e.min == 0 || e.min > e.max) {
            return Err("every loot entry needs a weight and 1 <= min <= max".into());
        }
        if t.rolls_min > t.rolls_max {
            return Err("rolls_min is above rolls_max".into());
        }
        Ok(t)
    }

    pub fn builtin() -> Self {
        Self::from_toml(include_str!("../../../assets/meteors.toml")).expect("assets/meteors.toml is valid")
    }

    /// Wayshards one player takes from a meteor of `level` on a planet of
    /// `world_tier`, full or dormant.
    pub fn wayshards(&self, level: u8, world_tier: u8, dormant: bool) -> u32 {
        let base = (self.wayshards_base + self.wayshards_per_level * level as u32) as f64;
        let bonus = 1.0 + self.tier_bonus * world_tier.saturating_sub(MIN_WORLD_TIER) as f64;
        let share = if dormant { self.dormant_share } else { 1.0 };
        (base * bonus * share).round().max(1.0) as u32
    }

    /// One player's loot: (item name, count), wayshards first. `rand` gives
    /// numbers in 0..1.
    pub fn roll(&self, level: u8, world_tier: u8, dormant: bool, mut rand: impl FnMut() -> f64) -> Vec<(String, u32)> {
        let mut out = vec![("wayshard".to_string(), self.wayshards(level, world_tier, dormant))];
        let rolls = if dormant {
            self.dormant_rolls
        } else {
            let span = (self.rolls_max - self.rolls_min + 1) as f64;
            self.rolls_min + ((rand() * span).floor() as u32).min(self.rolls_max - self.rolls_min)
        };
        let total: u32 = self.loot.iter().map(|e| e.weight).sum();
        for _ in 0..rolls {
            let mut pick = rand() * total as f64;
            let entry = self
                .loot
                .iter()
                .find(|e| {
                    pick -= e.weight as f64;
                    pick < 0.0
                })
                .unwrap_or(&self.loot[self.loot.len() - 1]);
            let span = (entry.max - entry.min + 1) as f64;
            let n = entry.min + ((rand() * span).floor() as u32).min(entry.max - entry.min);
            match out.iter_mut().find(|(item, _)| *item == entry.item) {
                Some((_, c)) => *c += n,
                None => out.push((entry.item.clone(), n)),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_double_with_the_players() {
        let got: Vec<u8> = [0, 1, 2, 3, 4, 7, 8, 16, 31, 32, 100].map(level_for).to_vec();
        assert_eq!(got, vec![1, 1, 2, 2, 3, 3, 4, 5, 5, 6, 6]);
        assert_eq!(waves(1), 3);
        assert_eq!(wave_size(6), 8);
    }

    #[test]
    fn the_last_wave_leans_to_the_top_tier() {
        let mean = |last| {
            (0..1000)
                .map(|i| wave_tier(1..=3, last, (i as f64 + 0.5) / 1000.0) as f64)
                .sum::<f64>()
                / 1000.0
        };
        assert!((mean(false) - 2.0).abs() < 0.05);
        assert!(mean(true) > 2.3);
        assert_eq!(wave_tier(2..=2, true, 0.99), 2);
        assert_eq!(wave_tier(1..=3, false, 0.999_999), 3);
    }

    #[test]
    fn the_heart_pays_wayshards_first_and_dormant_pays_less() {
        let t = MeteorLoot::builtin();
        assert_eq!(t.wayshards(1, 3, false), 7);
        assert_eq!(t.wayshards(6, 3, false), 22);
        assert_eq!(t.wayshards(1, 4, false), 9, "+25% a tier above 3");
        assert_eq!(t.wayshards(1, 3, true), 2);
        let mut s = 1u64;
        let mut rand = || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        for _ in 0..200 {
            let full = t.roll(2, 3, false, &mut rand);
            assert_eq!(full[0], ("wayshard".to_string(), 10));
            assert!(
                (2..=4).contains(&full.len()),
                "1 to 3 rolls (merged if repeated): {full:?}"
            );
            let dormant = t.roll(2, 3, true, &mut rand);
            assert!(dormant.len() <= 2, "one roll: {dormant:?}");
            for (item, n) in full.iter().skip(1).chain(dormant.iter().skip(1)) {
                let e = t.loot.iter().find(|e| &e.item == item).expect("from the table");
                assert!(*n >= e.min, "{item} {n}");
            }
        }
    }

    #[test]
    fn every_loot_item_exists() {
        let blocks = crate::BlockRegistry::builtin();
        let items = crate::ItemRegistry::builtin(&blocks);
        for e in &MeteorLoot::builtin().loot {
            assert!(items.id(&e.item).is_ok(), "meteor loot `{}` is no item", e.item);
        }
        assert!(items.id("wayshard").is_ok());
    }
}
