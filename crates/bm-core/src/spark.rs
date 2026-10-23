//! Spark and machine wear (milestone 15, `docs/milestones/15-spark.md`).
//!
//! Spark generators burn fuel into spark and store it; spark links carry it
//! to machines; recipes spend it; machines wear as they craft and spanners
//! repair them. The numbers live here so client and server agree; guesses
//! are marked and listed in `docs/feel.md`.

use crate::Station;

/// Spark one generator block stores (**guess**: Boundless's is not
/// published). Joined generators add their stores.
pub const GENERATOR_CAPACITY: u32 = 5_000;

/// Spark goes through at most this many links between a generator and a
/// machine (Boundless, wiki Spark Link).
pub const MAX_LINKS: usize = 100;

/// Wear a machine holds when fully repaired (**guess**: players needed 10
/// to 15 hits of a 10,000 gem spanner for a machine with its coils).
pub const MACHINE_WEAR: u32 = 10_000;

/// Spark one refund item holds when a generator with spark is broken
/// (Boundless's "Unused Spark", 1,000 each since Release 242).
pub const SPARK_PER_ITEM: u32 = 1_000;

/// Spanner Mastery: wear removed per hit grows by this share a rank (and
/// action speed by 2 a rank, as every mastery).
pub const SPANNER_MASTERY_REPAIR: f64 = 0.10;

/// Whether machines of this kind wear as they craft. The hand, the crafting
/// table, furnaces and generators do not (Boundless's did not).
pub fn wears(station: Station) -> bool {
    matches!(
        station,
        Station::Workbench
            | Station::Extractor
            | Station::Mixer
            | Station::Refinery
            | Station::Compactor
            | Station::Forge
    )
}

/// Seconds a fuel item worth `spark` takes to burn in a generator
/// (Boundless's `fuelData`, tuning 4.7): stick 7 in 1 s, timber 15 in 3.5,
/// peaty soil 20 and trunk 30 in 7, soft coal 100 in 10, medium 250 in 20,
/// hard 800 in 30. Anything else burns at 10 spark a second.
pub fn burn_seconds(spark: u16) -> f32 {
    match spark {
        0 => 0.0,
        7 => 1.0,
        15 => 3.5,
        20 | 30 => 7.0,
        100 => 10.0,
        150 => 15.0,
        250 => 20.0,
        800 => 30.0,
        s => s as f32 / 10.0,
    }
}

/// A generator group's store: `blocks` generators hold `blocks x
/// GENERATOR_CAPACITY`.
pub fn capacity(blocks: usize) -> u32 {
    (blocks as u32).saturating_mul(GENERATOR_CAPACITY)
}

/// Wear one spanner hit removes, with `mastery` ranks of Spanner Mastery.
pub fn repair_per_hit(repair: u32, mastery: u8) -> u32 {
    (repair as f64 * (1.0 + SPANNER_MASTERY_REPAIR * mastery as f64)).round() as u32
}

/// The heat a furnace burns for a recipe of `heat`, given its base's
/// `heat_share` (stone 1.0, iron 0.5), never less than 1 for a recipe that
/// needs heat.
pub fn furnace_heat(heat: u16, heat_share: f32) -> u16 {
    if heat == 0 {
        return 0;
    }
    ((heat as f32 * heat_share).round() as u16).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuels_burn_as_boundless_timed_them() {
        assert_eq!(burn_seconds(100), 10.0, "soft coal");
        assert_eq!(burn_seconds(30), 7.0, "trunk");
        assert_eq!(burn_seconds(15), 3.5, "timber");
        assert_eq!(burn_seconds(7), 1.0, "stick");
        assert_eq!(burn_seconds(0), 0.0);
    }

    #[test]
    fn only_crafting_machines_wear() {
        for s in [
            Station::Workbench,
            Station::Extractor,
            Station::Mixer,
            Station::Refinery,
            Station::Compactor,
            Station::Forge,
        ] {
            assert!(wears(s), "{s:?}");
        }
        for s in [
            Station::Hand,
            Station::CraftingTable,
            Station::Furnace,
            Station::SparkGenerator,
        ] {
            assert!(!wears(s), "{s:?}");
        }
    }

    #[test]
    fn spanners_and_iron_furnaces_follow_the_rules() {
        assert_eq!(repair_per_hit(700, 0), 700);
        assert_eq!(repair_per_hit(700, 5), 1050, "+10% a rank");
        assert_eq!(furnace_heat(100, 0.5), 50, "an iron furnace halves heat");
        assert_eq!(furnace_heat(150, 1.0), 150);
        assert_eq!(furnace_heat(0, 0.5), 0);
        assert_eq!(capacity(3), 15_000);
    }
}
