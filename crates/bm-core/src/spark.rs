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

/// Power one working coil gives (Boundless: +100; advanced coils +300).
pub const COIL_POWER: u32 = 100;
pub const ADVANCED_COIL_POWER: u32 = 300;

/// The most coils that count for one machine (Boundless: 24).
pub const MAX_COILS: usize = 24;

/// Wear a coil holds when mended (**guess**, as a machine's: Boundless's is
/// unknown, tuning 4.13.2).
pub const COIL_WEAR: u32 = MACHINE_WEAR;

/// The need a recipe that needs no power is read as, for the power curve
/// (**guess**: fitted so the cut stops growing at about 16 advanced coils,
/// as players saw; tuning 4.13.2).
pub const UNPOWERED_NEED: u32 = 2_400;

/// The share of a recipe's base time (and spark) a craft takes with `power`
/// against a need of `need`, or None when the power falls short. Boundless's
/// early-access formula (tuning 4.13.2): every full 100 power above the need
/// counts a step; the first step cuts 50%, and each step (the first
/// included) adds 1,250 / need percent more; never below 25% of the base.
pub fn power_scale(need: u32, power: u32) -> Option<f64> {
    if power < need {
        return None;
    }
    let steps = (power - need) / 100;
    if steps == 0 {
        return Some(1.0);
    }
    let read_need = if need == 0 { UNPOWERED_NEED } else { need };
    let cut = 50.0 + steps as f64 * 1250.0 / read_need as f64;
    Some((1.0 - cut.min(75.0) / 100.0).max(0.25))
}

#[cfg(test)]
mod power_tests {
    use super::*;

    #[test]
    fn power_follows_the_guide_and_the_measured_series() {
        assert_eq!(power_scale(1500, 1400), None, "short of the need");
        assert_eq!(power_scale(1500, 1599), Some(1.0), "less than a step");
        // The guide's worked example: need 1,500, 600 above: 55% cut.
        let s = power_scale(1500, 2100).unwrap();
        assert!((s - 0.45).abs() < 0.001, "{s}");
        // The marble series (need 800, advanced coils): 51%, 36%, 26%, 25%
        // of base measured; the curve gives within a few points.
        for (coils, measured) in [(3, 0.51), (6, 0.36), (12, 0.26), (24, 0.25)] {
            let s = power_scale(800, coils * ADVANCED_COIL_POWER).unwrap();
            assert!((s - measured).abs() < 0.04, "{coils} coils: {s} vs {measured}");
        }
        // No need: one coil halves the time; about 16 advanced coils reach
        // the floor.
        let one = power_scale(0, 100).unwrap();
        assert!((one - 0.4948).abs() < 0.001, "{one}");
        assert!(power_scale(0, 15 * ADVANCED_COIL_POWER).unwrap() > 0.25);
        assert_eq!(power_scale(0, 17 * ADVANCED_COIL_POWER), Some(0.25));
    }
}

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
        // Compacted coal (milestone 18): soft 1,500 in 150 s, medium 4,500
        // in 300, hard 15,000 in 500.
        1500 => 150.0,
        4500 => 300.0,
        15000 => 500.0,
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
