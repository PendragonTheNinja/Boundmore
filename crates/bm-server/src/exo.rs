//! Wandering worlds (milestone 21, `docs/milestones/21-wandering-worlds.md`,
//! ADR-0011): Boundless's exoworlds. Temporary planets that arrive beside a
//! permanent host, stay about five days and leave for good.
//!
//! This module is the pure part: the schedule of each slot, what a new
//! wanderer is (kind, tier, host, name, colours), and its air as it leaves.
//! The game ([`crate::game`]) makes and unmakes the planets.

use bm_core::atmosphere::{Air, AirLine};
use bm_core::colours::ColourPalette;
use bm_core::movement::TICK_HZ;
use bm_worldgen::PlanetKinds;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The first wanderer's planet id. Permanent planets keep their config index
/// as their id (`ServerConfig::validate` keeps them below this), wanderers
/// count up from here and an id is never reused (ADR-0011).
pub const FIRST_EXO_ID: u16 = 1000;

/// The highest air hazard: a world's air climbs to it as it leaves, past any
/// protection (ward 5 and the best food give 10 at most).
pub const LEAVING_HAZARD: u8 = 10;

/// Ticks in an hour of the universe clock.
pub const HOUR: u64 = 3600 * TICK_HZ as u64;

/// The wanderers' tunables (`ServerConfig`, milestone 21's "Decided" list).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExoRules {
    /// Slots running the stream; 0 turns wandering worlds off.
    pub slots: u8,
    /// Ticks between one arrival and the next across all slots.
    pub every: u64,
    /// Shortest and longest stay, ticks.
    pub life: (u64, u64),
    /// How long before leaving the air starts to climb, ticks.
    pub leaving: u64,
    /// Chunks around.
    pub width_chunks: u32,
}

impl Default for ExoRules {
    fn default() -> Self {
        Self {
            slots: 3,
            every: 48 * HOUR,
            life: (113 * HOUR, 128 * HOUR),
            leaving: 30 * 60 * TICK_HZ as u64,
            width_chunks: 48,
        }
    }
}

impl ExoRules {
    /// The time between arrivals in one slot.
    pub fn period(&self) -> u64 {
        self.every * u64::from(self.slots.max(1))
    }

    /// When the first wanderer of `slot` arrives in a universe that gains
    /// wandering worlds at `now`: slot 0 at once, the others staggered.
    pub fn first_arrival(&self, slot: u8, now: u64) -> u64 {
        now + self.every * u64::from(slot)
    }

    /// When the next wanderer of a slot arrives after `prev` (never before it
    /// has left).
    pub fn next_arrival(&self, prev: &ExoRecord) -> u64 {
        (prev.arrives + self.period()).max(prev.leaves)
    }
}

/// One wanderer, as saved (the `exo` table, by slot).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExoRecord {
    /// Its planet id (from [`FIRST_EXO_ID`], never reused).
    pub id: u16,
    pub slot: u8,
    pub name: String,
    /// Its kind in `assets/planets.toml`.
    pub kind: String,
    /// Its tier, which may be above its kind's.
    pub tier: u8,
    /// Its air's line (its kind's, or one given to a low kind raised to 4).
    pub line: AirLine,
    pub seed: u64,
    /// The permanent planet it is reached from.
    pub host: u16,
    /// Universe ticks.
    pub arrives: u64,
    pub leaves: u64,
    /// Its colour recipe's base hue (0 to 15, the palette's hue circle).
    pub hue: u8,
}

/// Where a wanderer is in its life at a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Made, hidden, not yet arrived.
    Coming,
    Up,
    /// In its last stretch: its air is climbing.
    Leaving,
    Gone,
}

impl ExoRecord {
    pub fn phase(&self, tick: u64, rules: &ExoRules) -> Phase {
        if tick < self.arrives {
            Phase::Coming
        } else if tick >= self.leaves {
            Phase::Gone
        } else if tick + rules.leaving >= self.leaves {
            Phase::Leaving
        } else {
            Phase::Up
        }
    }

    /// Its air at `tick`: its own until the last stretch, then climbing in
    /// steady steps to [`LEAVING_HAZARD`].
    pub fn air_at(&self, tick: u64, rules: &ExoRules) -> Air {
        let own = Air::of(self.line, self.tier);
        if self.phase(tick, rules) != Phase::Leaving || rules.leaving == 0 {
            return own;
        }
        let start = self.leaves - rules.leaving;
        let done = (tick - start) as f64 / rules.leaving as f64;
        let climb = f64::from(LEAVING_HAZARD.saturating_sub(own.hazard));
        let hazard = own.hazard + (climb * done).ceil() as u8;
        Air {
            line: self.line,
            hazard: hazard.min(LEAVING_HAZARD),
        }
    }
}

/// splitmix64 over a counter: the wanderers' dice. Seeded from the id and
/// the universe, so a universe's stream of wanderers is fixed once it is
/// made, and tests can know what comes.
#[derive(Clone, Debug)]
pub struct Dice(u64);

impl Dice {
    pub fn new(salt: u64, id: u16) -> Self {
        Self(salt ^ (u64::from(id) << 32) ^ 0x3e0_5eed)
    }

    pub fn draw(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// 0 to 1.
    pub fn unit(&mut self) -> f64 {
        (self.draw() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.draw() % n.max(1) as u64) as usize
    }
}

/// Boundless's live mix of tiers (the archive, 2023 to 2024): 10%, 31%,
/// 29%, 30% for tiers 4 to 7.
const TIER_WEIGHTS: [(u8, f64); 4] = [(4, 0.10), (5, 0.31), (6, 0.29), (7, 0.30)];

/// The air line a low kind takes when raised to tier 4, where its kind has
/// none (Boundless raised lush, coal and metal worlds: one per line).
fn raised_line(kind: &str) -> AirLine {
    match kind {
        "arid" => AirLine::Burning,
        "frozen" => AirLine::Charged,
        _ => AirLine::Acid,
    }
}

/// The kinds a wanderer of `tier` may be: tier 4 the tier 1 to 3 kinds,
/// tiers 5 and 6 the tier 4 kinds up to its own, tier 7 the tier 7 kinds
/// (or the tier 6 ones while there are none).
pub fn kinds_for_tier(kinds: &PlanetKinds, tier: u8) -> Vec<&bm_worldgen::PlanetKind> {
    let range = match tier {
        ..=4 => 1..=3,
        7.. if kinds.kinds.iter().any(|k| k.tier >= 7) => 7..=7,
        7.. => 6..=6,
        t => 4..=t,
    };
    kinds.kinds.iter().filter(|k| range.contains(&k.tier)).collect()
}

/// The host tiers for a wanderer of `tier` (the archive: tier 4 from tier 1
/// and 2 hosts, tier 5 from tier 3, tiers 6 and 7 from tiers 4 and 5).
pub fn host_tiers(tier: u8) -> std::ops::RangeInclusive<u8> {
    match tier {
        ..=4 => 1..=2,
        5 => 3..=3,
        _ => 4..=5,
    }
}

/// Picks a host among `planets` (permanent: id and tier): one of the right
/// tiers, else the nearest lower tier there is, else the first planet.
pub fn pick_host(planets: &[(u16, u8)], tier: u8, dice: &mut Dice) -> u16 {
    let want = host_tiers(tier);
    let fits: Vec<u16> = planets.iter().filter(|(_, t)| want.contains(t)).map(|p| p.0).collect();
    if !fits.is_empty() {
        return fits[dice.below(fits.len())];
    }
    let lower = planets.iter().filter(|(_, t)| *t < *want.start()).map(|p| p.1).max();
    match lower {
        Some(t) => {
            let fits: Vec<u16> = planets.iter().filter(|p| p.1 == t).map(|p| p.0).collect();
            fits[dice.below(fits.len())]
        }
        None => planets.first().map_or(0, |p| p.0),
    }
}

/// A wanderer's name: two or three syllables, sometimes a numeral.
pub fn make_name(dice: &mut Dice) -> String {
    const ONSET: [&str; 18] = [
        "b", "d", "k", "l", "m", "n", "r", "s", "t", "v", "z", "th", "dr", "kr", "br", "f", "h", "y",
    ];
    const VOWEL: [&str; 9] = ["a", "e", "i", "o", "u", "a", "e", "ai", "ou"];
    const CODA: [&str; 8] = ["", "", "", "n", "r", "s", "l", "th"];
    const NUMERAL: [&str; 6] = ["II", "III", "IV", "V", "VI", "VII"];
    let syllables = 2 + dice.below(2);
    let mut s = String::new();
    for i in 0..syllables {
        s.push_str(ONSET[dice.below(ONSET.len())]);
        s.push_str(VOWEL[dice.below(VOWEL.len())]);
        if i + 1 == syllables || dice.below(3) == 0 {
            s.push_str(CODA[dice.below(CODA.len())]);
        }
    }
    let mut name: String = s
        .char_indices()
        .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
        .collect();
    if dice.below(4) == 0 {
        name.push(' ');
        name.push_str(NUMERAL[dice.below(NUMERAL.len())]);
    }
    name
}

/// What a new wanderer is, before it is made: its kind, tier, line, host,
/// name, seed and hue. `planets` are the permanent planets (id, tier) and
/// `taken` the names in use.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    id: u16,
    slot: u8,
    salt: u64,
    arrives: u64,
    rules: &ExoRules,
    kinds: &PlanetKinds,
    planets: &[(u16, u8)],
    taken: &[&str],
) -> ExoRecord {
    let mut dice = Dice::new(salt, id);
    let u = dice.unit();
    let mut acc = 0.0;
    let mut tier = 7;
    for (t, w) in TIER_WEIGHTS {
        acc += w;
        if u < acc {
            tier = t;
            break;
        }
    }
    let choices = kinds_for_tier(kinds, tier);
    let kind = if choices.is_empty() {
        &kinds.kinds[0]
    } else {
        choices[dice.below(choices.len())]
    };
    let line = if kind.air == AirLine::None {
        raised_line(&kind.name)
    } else {
        kind.air
    };
    let host = pick_host(planets, tier, &mut dice);
    let mut name = make_name(&mut dice);
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
        name = make_name(&mut dice);
    }
    let (lo, hi) = rules.life;
    let life = lo + (dice.unit() * (hi.saturating_sub(lo)) as f64) as u64;
    ExoRecord {
        id,
        slot,
        name,
        kind: kind.name.clone(),
        tier: tier.max(kind.tier),
        line,
        seed: dice.draw(),
        host,
        arrives,
        leaves: arrives + life.max(1),
        hue: dice.below(HUES) as u8,
    }
}

/// The palette's hue circle (`assets/colours.toml`: 16 hues of 15 shades,
/// then 15 greys).
const HUES: usize = 16;
const HUE_NAMES: [&str; HUES] = [
    "Crimson", "Ember", "Rust", "Amber", "Gold", "Lime", "Moss", "Fern", "Jade", "Teal", "Sky", "Azure", "Cobalt",
    "Violet", "Orchid", "Rose",
];
const GREYS: [&str; 15] = [
    "Coal", "Soot", "Slate", "Ash", "Smoke", "Iron", "Pewter", "Stone", "Fog", "Silver", "Cloud", "Chalk", "Pearl",
    "Bone", "Snow",
];

/// What a block is to the colour recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Rock,
    Wood,
    Growth,
    Soil,
    Sand,
    Frost,
    Glow,
}

fn role(block: &str) -> Role {
    let has = |w: &str| block.contains(w);
    if has("glow") {
        Role::Glow
    } else if has("trunk") {
        Role::Wood
    } else if has("turf") || has("foliage") || has("frond") || has("leaf") || has("leaves") || has("moss") {
        Role::Growth
    } else if has("sand") {
        Role::Sand
    } else if has("snow") || has("ice") || has("frost") {
        Role::Frost
    } else if has("soil") || has("clay") || has("peat") || has("mud") {
        Role::Soil
    } else {
        Role::Rock
    }
}

fn hue(i: i64) -> &'static str {
    HUE_NAMES[i.rem_euclid(HUES as i64) as usize]
}

/// A wanderer's natural colours: for each block its kind colours, a list of
/// palette colours drawn from a recipe around `base` (its rock in the base
/// hue's muted shades, its growth in a hue away round the circle, wood and
/// soil beside the base, sand warm, frost pale), from which
/// [`bm_worldgen::pick_colours`] picks the main colour and its variants.
/// Drawn from the whole palette, so wanderers bring colours the permanent
/// planets' lists lack (Boundless: "unique colours to snag").
pub fn colour_choices(
    blocks: impl IntoIterator<Item = String>,
    base: u8,
    seed: u64,
    palette: &ColourPalette,
) -> BTreeMap<String, Vec<String>> {
    let mut dice = Dice::new(seed, u16::from(base));
    let b = i64::from(base);
    // How far round the circle the growth sits, and which way wood leans.
    let growth = b + [4, 5, 6, 8, -4, -5][dice.below(6)];
    let lean = if dice.below(2) == 0 { 1 } else { -1 };
    let mut out = BTreeMap::new();
    for block in blocks {
        let (hues, shades): (Vec<i64>, &[&str]) = match role(&block) {
            Role::Rock => (
                vec![b, b, b + lean],
                &["Muted", "Faded", "Dusky", "Soft", "Dark", "Misty"],
            ),
            Role::Wood => (
                vec![b + lean, b + 2 * lean, b],
                &["Dark", "Dusky", "Deep", "Muted", "Rich"],
            ),
            Role::Growth => (
                vec![growth, growth + 1, growth - 1],
                &["Rich", "Strong", "Bold", "Dusky", "True"],
            ),
            Role::Soil => (vec![b + lean, b, b + 2 * lean], &["Dark", "Dusky", "Muted", "Deep"]),
            Role::Sand => (vec![b, 2, 3, 4], &["Light", "Pale", "Faded", "Soft"]),
            Role::Frost => (vec![b, b + 1], &["Misty", "Pale", "Soft", "Light"]),
            // Glow crystal anywhere round the circle, always bright: a
            // wanderer's crystal colours are what players come for.
            Role::Glow => (
                vec![b + 8, b + 3, b - 5, b + 11],
                &["Bright", "Light", "Pale", "Bright", "True"],
            ),
        };
        let mut names: Vec<String> = Vec::new();
        for _ in 0..6 {
            let h = hues[dice.below(hues.len())];
            let s = shades[dice.below(shades.len())];
            let n = format!("{s} {}", hue(h));
            if !names.contains(&n) {
                names.push(n);
            }
        }
        // Rock now and then keeps a grey among its shades.
        if role(&block) == Role::Rock && dice.below(3) == 0 {
            names.push(GREYS[3 + dice.below(9)].to_string());
        }
        names.retain(|n| palette.id(n).is_some());
        if !names.is_empty() {
            out.insert(block, names);
        }
    }
    out
}

/// sRGB 0-255 to linear 0-1.
fn linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// A wanderer's look: its kind's, its colour from afar its base hue's, and
/// its sky leaning toward that hue.
pub fn look(kind: &bm_core::PlanetLook, base: u8, palette: &ColourPalette) -> bm_core::PlanetLook {
    let rgb = |name: &str| palette.id(name).and_then(|c| palette.get(c)).map(|d| d.rgb.map(linear));
    let mut out = kind.clone();
    let h = hue(i64::from(base));
    if let Some(c) = rgb(&format!("Strong {h}")) {
        out.color = c;
    }
    if let Some(c) = rgb(&format!("Bright {h}")) {
        let mix = |a: [f32; 3], t: f32| [0, 1, 2].map(|i| a[i] * (1.0 - t) + c[i] * t);
        out.day_zenith = mix(kind.day_zenith, 0.3);
        out.day_horizon = mix(kind.day_horizon, 0.35);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::BlockRegistry;

    fn kinds() -> PlanetKinds {
        PlanetKinds::builtin(&BlockRegistry::builtin())
    }

    /// The default universe's permanent planets: id and tier.
    fn planets() -> Vec<(u16, u8)> {
        vec![
            (0, 1),
            (1, 2),
            (2, 2),
            (3, 3),
            (4, 4),
            (5, 4),
            (6, 4),
            (7, 5),
            (8, 5),
            (9, 5),
            (10, 6),
            (11, 6),
            (12, 6),
        ]
    }

    #[test]
    fn one_arrives_every_two_days_three_slots_six_days_apart() {
        let r = ExoRules::default();
        assert_eq!(r.first_arrival(0, 500), 500);
        assert_eq!(r.first_arrival(1, 500), 500 + 48 * HOUR);
        assert_eq!(r.first_arrival(2, 500), 500 + 96 * HOUR);
        let k = kinds();
        let first = draw(FIRST_EXO_ID, 0, 7, 500, &r, &k, &planets(), &[]);
        assert_eq!(r.next_arrival(&first), 500 + 144 * HOUR);
        // Its stay is Boundless's: 113 to 128 hours.
        let stay = first.leaves - first.arrives;
        assert!((113 * HOUR..=128 * HOUR).contains(&stay), "{stay}");
        // So never more than three are up, and from day 4 always two or more.
        let up = |t: u64| {
            (0..3u8)
                .filter(|s| {
                    let mut a = r.first_arrival(*s, 0);
                    let mut id = FIRST_EXO_ID + u16::from(*s);
                    loop {
                        let rec = draw(id, *s, 7, a, &r, &k, &planets(), &[]);
                        if t < rec.arrives {
                            return false;
                        }
                        if t < rec.leaves {
                            return true;
                        }
                        a = r.next_arrival(&rec);
                        id += 3;
                    }
                })
                .count()
        };
        for h in (100..2000).step_by(7) {
            let n = up(h * HOUR);
            assert!((2..=3).contains(&n), "{n} up at hour {h}");
        }
    }

    #[test]
    fn the_mix_of_tiers_is_boundlesss() {
        let k = kinds();
        let r = ExoRules::default();
        let mut by_tier = [0u32; 8];
        for id in 0..2000u16 {
            let rec = draw(FIRST_EXO_ID + id, 0, 11, 0, &r, &k, &planets(), &[]);
            by_tier[rec.tier as usize] += 1;
            let kind = k.get(&rec.kind).unwrap();
            assert!(kind.tier <= rec.tier, "{} is tier {}", rec.kind, kind.tier);
            let host_tier = planets().iter().find(|p| p.0 == rec.host).unwrap().1;
            assert!(
                host_tiers(rec.tier).contains(&host_tier),
                "tier {} from {host_tier}",
                rec.tier
            );
            assert_ne!(rec.line, AirLine::None);
        }
        let share = |t: usize| f64::from(by_tier[t]) / 2000.0;
        assert!((share(4) - 0.10).abs() < 0.03, "{by_tier:?}");
        assert!((share(5) - 0.31).abs() < 0.04, "{by_tier:?}");
        assert!((share(6) - 0.29).abs() < 0.04, "{by_tier:?}");
        assert!((share(7) - 0.30).abs() < 0.04, "{by_tier:?}");
    }

    #[test]
    fn a_raised_low_kind_breathes_its_lines_air() {
        let r = ExoRules::default();
        let rec = ExoRecord {
            id: FIRST_EXO_ID,
            slot: 0,
            name: "Test".into(),
            kind: "temperate".into(),
            tier: 4,
            line: raised_line("temperate"),
            seed: 1,
            host: 0,
            arrives: 0,
            leaves: 100 * HOUR,
            hue: 0,
        };
        assert_eq!(
            rec.air_at(HOUR, &r),
            Air {
                line: AirLine::Acid,
                hazard: 1
            }
        );
    }

    #[test]
    fn the_air_climbs_to_ten_in_the_last_half_hour() {
        let r = ExoRules::default();
        let rec = ExoRecord {
            id: FIRST_EXO_ID,
            slot: 0,
            name: "Test".into(),
            kind: "glacial".into(),
            tier: 5,
            line: AirLine::Charged,
            seed: 1,
            host: 3,
            arrives: 0,
            leaves: 100 * HOUR,
            hue: 0,
        };
        let start = rec.leaves - r.leaving;
        assert_eq!(rec.phase(start - 1, &r), Phase::Up);
        assert_eq!(rec.air_at(start - 1, &r).hazard, 3);
        assert_eq!(rec.phase(start, &r), Phase::Leaving);
        assert_eq!(rec.air_at(start, &r).hazard, 3);
        let mut last = 3;
        for t in (start..rec.leaves).step_by(600) {
            let h = rec.air_at(t, &r).hazard;
            assert!(h >= last && h <= LEAVING_HAZARD);
            last = h;
        }
        assert_eq!(rec.air_at(rec.leaves - 1, &r).hazard, LEAVING_HAZARD);
        assert_eq!(rec.phase(rec.leaves, &r), Phase::Gone);
        assert_eq!(rec.phase(0, &r), Phase::Up);
        let coming = ExoRecord { arrives: 10, ..rec };
        assert_eq!(coming.phase(9, &r), Phase::Coming);
    }

    #[test]
    fn names_are_words_and_never_taken() {
        let mut d = Dice::new(1, 2);
        for _ in 0..200 {
            let n = make_name(&mut d);
            assert!(n.len() >= 3 && n.chars().next().unwrap().is_ascii_uppercase(), "{n}");
        }
        let r = ExoRules::default();
        let k = kinds();
        let a = draw(FIRST_EXO_ID, 0, 3, 0, &r, &k, &planets(), &[]);
        let b = draw(FIRST_EXO_ID, 0, 3, 0, &r, &k, &planets(), &[&a.name]);
        assert_ne!(a.name, b.name);
    }

    #[test]
    fn wanderers_bring_colours_their_kinds_lack() {
        let palette = ColourPalette::builtin();
        let k = kinds();
        let kind = k.get("temperate").unwrap();
        let mut outside = 0;
        for base in 0..16u8 {
            let picks = colour_choices(kind.colours.keys().cloned(), base, 99, &palette);
            assert_eq!(picks.len(), kind.colours.len(), "every block gets colours");
            for (block, names) in &picks {
                assert!(!names.is_empty());
                for n in names {
                    assert!(palette.id(n).is_some(), "{n}");
                    if !kind.colours[block].contains(n) {
                        outside += 1;
                    }
                }
            }
        }
        assert!(outside > 100, "only {outside} colours outside the kind's lists");
        // Growth sits away from the rock's hue.
        let picks = colour_choices(["stone".to_string(), "turf".to_string()], 0, 5, &palette);
        assert!(picks["stone"].iter().all(|n| !picks["turf"].contains(n)));
    }

    #[test]
    fn a_wanderers_look_takes_its_hue() {
        let palette = ColourPalette::builtin();
        let base = bm_core::PlanetLook::default();
        let a = look(&base, 0, &palette);
        let b = look(&base, 10, &palette);
        assert_ne!(a.color, b.color);
        assert_ne!(a.day_zenith, base.day_zenith);
    }
}
