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

/// The most wear any machine holds (the refinery's, compactor's and
/// forge's); [`machine_wear`] gives each kind's.
pub const MACHINE_WEAR: u32 = 10_000;

/// Wear a machine of `kind` holds when fully repaired: Boundless's own
/// durabilities, read off the game's debug view (forum check 4b): workbench
/// and extractor 2,000, mixer 4,000, spark generator 8,000, refinery,
/// compactor and forge 10,000.
pub fn machine_wear(kind: Station) -> u32 {
    match kind {
        Station::Workbench | Station::Extractor => 2_000,
        Station::Mixer => 4_000,
        Station::SparkGenerator => 8_000,
        _ => MACHINE_WEAR,
    }
}

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

/// Wear a coil holds when mended, basic or advanced alike (players: "all
/// coils have the same total, around 20k"; forum check 4b, a low-medium
/// confidence number).
pub const COIL_WEAR: u32 = 20_000;

/// Wear each working coil loses for a craft that wears its machine of
/// `kind` by `wear`: the same share of its own durability (Boundless's
/// coil and machine bars fall together; forum check 4b).
pub fn coil_wear(kind: Station, wear: u32) -> u32 {
    ((wear as u64 * COIL_WEAR as u64).div_ceil(machine_wear(kind) as u64)) as u32
}

/// Wear a spanner hit of `amount` on a machine of `kind` mends on each of
/// its coils: the same share (Boundless since Release 235, one bar for the
/// machine and its coils).
pub fn coil_mend(kind: Station, amount: u32) -> u32 {
    ((amount as u64 * COIL_WEAR as u64) / machine_wear(kind) as u64) as u32
}

/// The cheapest a craft gets with power: 46.75% of its base time and spark
/// (Boundless since Release 198, measured; forum check 4a).
pub const POWER_FLOOR: f64 = 0.4675;

/// Measured multipliers for a recipe that needs no power, by power: one
/// advanced coil (300) a step, refined titanium in the refinery (Arkhainn,
/// forum t/20845), with Simoyd's 100-power point. Past the last, the floor.
const UNPOWERED: [(u32, f64); 18] = [
    (0, 1.0),
    (100, 0.82),
    (300, 0.7662),
    (600, 0.6969),
    (900, 0.6405),
    (1200, 0.5951),
    (1500, 0.5594),
    (1800, 0.5317),
    (2100, 0.5108),
    (2400, 0.4955),
    (2700, 0.4847),
    (3000, 0.4773),
    (3300, 0.4726),
    (3600, 0.4699),
    (3900, 0.4684),
    (4200, 0.4678),
    (4500, 0.4676),
    (4800, POWER_FLOOR),
];

/// Measured multipliers for a recipe that needs power, by power above the
/// need: refined marble (need 800) at 100 and 1,000 over (forum check 4a);
/// from 2,800 over it joins the table above.
const POWERED: [(u32, f64); 4] = [(0, 1.0), (100, 0.956), (1000, 0.666), (2800, 0.482)];

/// Reads a measured table at `x`, straight lines between its points.
fn read_table(table: &[(u32, f64)], x: u32) -> f64 {
    let mut last = table[0];
    for &(at, m) in table {
        if x <= at {
            if at == last.0 {
                return m;
            }
            let t = (x - last.0) as f64 / (at - last.0) as f64;
            return last.1 + (m - last.1) * t;
        }
        last = (at, m);
    }
    last.1
}

/// The share of a recipe's base time and spark a craft takes with `power`
/// against a need of `need`, or None when the power falls short. One
/// multiplier for both, from Boundless's measured tables (forum check 4a):
/// only power above the need counts, a powered recipe starts slower, and
/// nothing goes below [`POWER_FLOOR`].
pub fn power_scale(need: u32, power: u32) -> Option<f64> {
    if power < need {
        return None;
    }
    let over = power - need;
    let m = if need == 0 || over >= 2_800 {
        read_table(&UNPOWERED, over)
    } else {
        read_table(&POWERED, over)
    };
    Some(m.max(POWER_FLOOR))
}

#[cfg(test)]
mod power_tests {
    use super::*;

    #[test]
    fn power_follows_the_measured_tables() {
        assert_eq!(power_scale(1500, 1400), None, "short of the need");
        assert_eq!(power_scale(1500, 1500), Some(1.0), "just enough");
        // Refined titanium (no need), advanced coils.
        for (coils, measured) in [(1, 0.7662), (2, 0.6969), (4, 0.5951), (8, 0.4955), (12, 0.4699)] {
            let s = power_scale(0, coils * ADVANCED_COIL_POWER).unwrap();
            assert!((s - measured).abs() < 1e-9, "{coils} coils: {s}");
        }
        assert_eq!(power_scale(0, 24 * ADVANCED_COIL_POWER), Some(POWER_FLOOR));
        // Refined marble (need 800): 0.956, 0.666, 0.482, 0.468, 0.4676.
        for (over, measured) in [
            (100, 0.956),
            (1000, 0.666),
            (2800, 0.482),
            (4000, 0.468),
            (6400, 0.4676),
        ] {
            let s = power_scale(800, 800 + over).unwrap();
            assert!((s - measured).abs() < 0.002, "{over} over: {s}");
        }
        // More power never costs more.
        let mut last = 1.0;
        for p in (0..8000).step_by(100) {
            let s = power_scale(0, p).unwrap();
            assert!(s <= last + 1e-12);
            last = s;
        }
    }

    #[test]
    fn coils_wear_and_mend_with_their_machine() {
        // A workbench (2,000) worn by 110 loses 5.5%; so does each coil.
        assert_eq!(coil_wear(Station::Workbench, 110), 1_100);
        assert_eq!(coil_wear(Station::Refinery, 100), 200);
        assert_eq!(coil_mend(Station::Mixer, 2_000), 10_000);
        assert_eq!(machine_wear(Station::Forge), 10_000);
        assert_eq!(machine_wear(Station::Extractor), 2_000);
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
