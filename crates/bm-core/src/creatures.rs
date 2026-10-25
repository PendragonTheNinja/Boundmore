//! Creatures as data (milestone 13, `docs/milestones/13-creatures-and-combat.md`,
//! tuning 3.2). Each kind lists, per creature tier, its health, armour, XP,
//! damage and drops (Boundless's numbers where published), plus how it moves
//! and fights (guesses where not; `docs/feel.md`). The catalogue is
//! `assets/creatures.toml`, sent to clients in the handshake; code refers to
//! kinds by name.

use crate::movement::MoveConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The word before a creature's name at each tier (tier 1 has none).
/// Boundmore's own words for Boundless's Stout, Strong, Mighty, Elite.
pub const TIER_WORDS: [&str; 5] = ["", "Hardy", "Tough", "Grim", "Elder"];

/// How a kind behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Behaviour {
    /// Wanders in herds; never starts a fight; charges whoever hurts it or
    /// fires a weapon near it (the grazer).
    Herd,
    /// Runs about; on hostile worlds attacks players it notices, strafing
    /// and shooting (the pelter).
    Ranged,
    /// Never attacks; sprints away from players it notices (the dasher).
    Flee,
    /// Hops about passively; once triggered (hurt, or a player close on a
    /// hostile world) whistles, then throws itself at the target and
    /// explodes; may curl into a ball that cannot be hurt (the popper,
    /// Boundless's hopper; milestone 13 part 2).
    Exploder,
    /// Rests on the ground; when angered flies, strafes above its target and
    /// fires (the drifter, Boundless's cuttletrunk).
    Flyer,
}

/// How a kind attacks.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attack {
    /// No attack.
    None,
    /// Rears for `windup` seconds, then runs at `speed` for up to `range`
    /// blocks, hitting anything within `radius` of its head once, with
    /// knockback `knockback` blocks a second.
    Charge {
        speed: f32,
        range: f32,
        radius: f32,
        windup: f32,
        knockback: f32,
        /// Seconds between charges.
        cooldown: f32,
    },
    /// Shoots `burst` shots `spread` degrees apart, at `speed` blocks a
    /// second, from up to `range` blocks, every `cooldown` seconds.
    Shot {
        speed: f32,
        range: f32,
        cooldown: f32,
        /// Shots a volley holds on tiers 1, 2, 3+ (Boundless: single shots,
        /// then bursts and fans).
        burst: [u8; 3],
        spread: f32,
    },
    /// Whistles for `fuse` seconds, leaps at the target and explodes within
    /// `radius` blocks (impulse damage = the tier's damage, falling off to
    /// half at the edge, never through walls), knocking back at `knockback`
    /// blocks a second. When first hurt it curls into a ball for
    /// `curl` seconds with chance `curl_chance`: it cannot be hurt, and
    /// explodes if it lands hard.
    Explode {
        fuse: f32,
        radius: f32,
        knockback: f32,
        curl: f32,
        curl_chance: f32,
    },
    /// The drifter's kit. It hovers `hover` blocks above its target and fires
    /// bolts (kinetic) at `bolt_speed` every `cooldown` seconds from up to
    /// `range`. Attacks unlock as its health drops (Boundless, players 2019):
    /// lobbed bombs (impulse, `bomb_radius`) below `bomb_below` of its health,
    /// homing bolts below `homing_below`; from `beam_tier` a beam (impulse,
    /// a hit every half second while it lasts); from `lift_tier` it lifts a
    /// player who stays in a marked spot for 1 s and drops them, every
    /// `lift_cooldown` seconds (Boundless: 30 s).
    Barrage {
        bolt_speed: f32,
        range: f32,
        cooldown: f32,
        hover: f32,
        bomb_below: f32,
        bomb_radius: f32,
        homing_below: f32,
        beam_tier: u8,
        lift_tier: u8,
        lift_cooldown: f32,
    },
}

/// A world's element and its elemental creatures' (milestone 18 part 3;
/// Boundless's gem-world elements, tuning 3.3.3). Each is weak to its
/// opposite: corrosion and shock, burn and chill, blast and toxin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    Corrosion,
    Burn,
    Blast,
    Shock,
    Chill,
    Toxin,
}

impl Element {
    pub const ALL: [Element; 6] = [
        Element::Corrosion,
        Element::Burn,
        Element::Blast,
        Element::Shock,
        Element::Chill,
        Element::Toxin,
    ];

    /// On the wire: 1 to 6 (0 is none).
    pub fn code(self) -> u8 {
        Self::ALL.iter().position(|e| *e == self).expect("listed") as u8 + 1
    }

    pub fn from_code(c: u8) -> Option<Element> {
        Self::ALL.get((c as usize).checked_sub(1)?).copied()
    }

    /// The word before an elemental creature's name ("Burning Grim Pelter").
    pub fn word(self) -> &'static str {
        match self {
            Element::Corrosion => "Corroding",
            Element::Burn => "Burning",
            Element::Blast => "Blasting",
            Element::Shock => "Shocking",
            Element::Chill => "Chilling",
            Element::Toxin => "Toxic",
        }
    }
}

/// The chance an elemental creature shrugs off a hit of `attack` (None:
/// physical), as Vansten's table has it (tuning 3.3.3): physical 0.4,
/// itself 0.8, its neighbours 0.6 and 0.3; negative is the chance the hit
/// does double instead (its opposite, -0.3).
pub fn resistance(creature: Element, attack: Option<Element>) -> f32 {
    use Element::*;
    let Some(a) = attack else { return 0.4 };
    let row: [f32; 6] = match creature {
        // Attack order: corrosion, burn, blast, shock, chill, toxin.
        Corrosion => [0.8, 0.3, 0.0, -0.3, 0.0, 0.6],
        Burn => [0.3, 0.8, 0.6, 0.0, -0.3, 0.0],
        Blast => [0.0, 0.6, 0.8, 0.3, 0.0, -0.3],
        Shock => [-0.3, 0.0, 0.3, 0.8, 0.6, 0.0],
        Chill => [0.0, -0.3, 0.0, 0.6, 0.8, 0.3],
        Toxin => [0.6, 0.0, -0.3, 0.0, 0.3, 0.8],
    };
    row[a.code() as usize - 1]
}

/// A creature's health at full: its tier's, tripled by the exotic health
/// buff, doubled again if elemental (milestone 18 part 3). Client and server
/// both use it.
pub fn max_health(base: u32, exotic: Exotic, element: Option<Element>) -> u32 {
    let h = exotic.health(base);
    if element.is_some() { h * 2 } else { h }
}

/// The chance a creature spawning on an elemental world is elemental
/// (**guess**, milestone 18 Measure 7).
pub const ELEMENTAL_CHANCE: f64 = 0.3;

/// An exotic creature's buffs (Boundless: visible decals, 1 to 2 buffs on low
/// worlds, 3 to 4 on high ones [tuning 3.2.2]). A bit set, sent on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Exotic(pub u8);

impl Exotic {
    pub const HEALTH: u8 = 1;
    pub const ARMOUR: u8 = 2;
    pub const DAMAGE: u8 = 4;
    pub const SPEED: u8 = 8;

    pub fn any(self) -> bool {
        self.0 != 0
    }

    pub fn has(self, bit: u8) -> bool {
        self.0 & bit != 0
    }

    /// Health x3 with the health buff (+200%, Boundless).
    pub fn health(self, base: u32) -> u32 {
        if self.has(Self::HEALTH) { base * 3 } else { base }
    }

    /// Armour x2 with the armour buff (+100%, Boundless).
    pub fn armour(self, base: u32) -> u32 {
        if self.has(Self::ARMOUR) { base * 2 } else { base }
    }

    /// Damage x1.5 with the damage buff: a **guess** (Boundless never
    /// published it).
    pub fn damage(self, base: u32) -> u32 {
        if self.has(Self::DAMAGE) { base + base / 2 } else { base }
    }

    /// Speed x1.3 with the speed buff: a **guess**.
    pub fn speed(self) -> f64 {
        if self.has(Self::SPEED) { 1.3 } else { 1.0 }
    }

    /// XP x1.5 for any exotic: a **guess** ("extra XP", Boundless wiki).
    pub fn xp(self, base: u32) -> u32 {
        if self.any() { base + base / 2 } else { base }
    }

    /// Extra drop rolls for any exotic: one more at least and at most (a
    /// **guess**; "more drops", Boundless wiki).
    pub fn extra_rolls(self) -> u8 {
        u8::from(self.any())
    }
}

/// Chance a spawned creature is exotic: Boundless's wiki gave 20% for
/// spitters, and Testing 218 made exotics rarer on low worlds; 10% up to
/// world tier 4 and 20% above is a **guess**.
pub fn exotic_chance(world_tier: u8) -> f64 {
    if world_tier <= 4 { 0.10 } else { 0.20 }
}

/// Rolls an exotic's buffs on a world of `world_tier`: 1 or 2 up to tier 4,
/// 3 or 4 above (Testing 218), each buff different. `rand` gives 0..1.
pub fn roll_exotic(world_tier: u8, mut rand: impl FnMut() -> f64) -> Exotic {
    if rand() >= exotic_chance(world_tier) {
        return Exotic::default();
    }
    let n = if world_tier <= 4 { 1 } else { 3 } + u32::from(rand() < 0.5);
    let mut bits = 0u8;
    while bits.count_ones() < n {
        bits |= 1 << ((rand() * 4.0) as u8).min(3);
    }
    Exotic(bits)
}

/// One weighted entry in a drop pool [tuning 3.2.4]: picked with chance
/// weight / total, giving 1 to `max`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Drop {
    pub item: String,
    pub weight: u16,
    pub max: u16,
}

/// A kind at one tier.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureTier {
    pub health: u32,
    pub armour: u32,
    pub xp: u32,
    /// Damage one hit deals before the player's armour.
    #[serde(default)]
    pub damage: u32,
    /// Rolls on the pool per kill, uniformly from min to max.
    pub drops_min: u8,
    pub drops_max: u8,
    pub drops: Vec<Drop>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureDef {
    pub name: String,
    /// What players see (title case of the name if unset).
    #[serde(default)]
    pub display: String,
    pub behaviour: Behaviour,
    pub attack: Attack,
    /// Body: half its width and its height, in blocks.
    pub half_width: f32,
    pub height: f32,
    /// Blocks a second when wandering, and when running (fleeing, chasing).
    pub walk_speed: f32,
    pub run_speed: f32,
    /// Notices a player within this many blocks.
    pub notice: f32,
    /// Gives up on a target this far away, or unseen for `forget` seconds.
    pub give_up: f32,
    pub forget: f32,
    /// Lowest world tier it lives on.
    #[serde(default = "one")]
    pub min_world_tier: u8,
    /// Relative chance a spawn picks this kind, and how many come together.
    pub spawn_weight: u16,
    pub group: [u8; 2],
    /// Tiers 1 up; a kind may stop early (the dasher at 3).
    pub tiers: Vec<CreatureTier>,
}

fn one() -> u8 {
    1
}

impl CreatureDef {
    /// The highest tier this kind has.
    pub fn top_tier(&self) -> u8 {
        self.tiers.len() as u8
    }

    /// Stats at `tier` (1-based), clamped to the tiers it has.
    pub fn tier(&self, tier: u8) -> &CreatureTier {
        let i = (tier.max(1) as usize - 1).min(self.tiers.len() - 1);
        &self.tiers[i]
    }

    /// "Hardy Grazer".
    pub fn title(&self, tier: u8) -> String {
        let word = TIER_WORDS.get(tier.max(1) as usize - 1).copied().unwrap_or("");
        if word.is_empty() {
            self.display.clone()
        } else {
            format!("{word} {}", self.display)
        }
    }

    /// The movement settings its body moves by (`movement::step`, the same
    /// physics as players: it collides, swims, falls and wraps).
    pub fn move_config(&self) -> MoveConfig {
        MoveConfig {
            walk_speed: self.walk_speed as f64,
            sprint_speed: self.run_speed as f64,
            swim_speed: (self.walk_speed as f64).max(2.0),
            half_width: self.half_width as f64,
            height: self.height as f64,
            eye_height: self.height as f64 * 0.8,
            ..MoveConfig::default()
        }
    }
}

/// Creature tiers a world of `world_tier` holds: 1 up to one below the
/// world's, never below 1 (Boundless: tier 3 worlds hold tiers 1 and 2,
/// tier 6 and up 1 to 5 [tuning 3.2.1]).
pub fn tiers_on(world_tier: u8) -> std::ops::RangeInclusive<u8> {
    1..=world_tier.saturating_sub(1).clamp(1, 5)
}

/// Whether creatures on a world of `world_tier` attack unprovoked
/// (Boundless: tier 1 worlds are passive).
pub fn hostile_world(world_tier: u8) -> bool {
    world_tier >= 2
}

/// Below this level creatures leave you alone unless you attack them
/// (Boundless's new-player protection).
pub const PROTECTED_BELOW_LEVEL: u32 = 5;

/// Rolls a kill's drops: `drops_min..=drops_max` rolls (more with luck,
/// `luck` the multiplier from `skills::Effects::luck_multiplier`), each
/// picking an entry by weight and giving 1 to its max. `rand` gives numbers
/// in 0..1. How luck counts on creatures is a **guess**: it scales the number
/// of rolls, as it scales bonus chances on blocks.
pub fn roll_drops(t: &CreatureTier, luck: f64, mut rand: impl FnMut() -> f64) -> Vec<(String, u16)> {
    let total: u32 = t.drops.iter().map(|d| d.weight as u32).sum();
    if total == 0 || t.drops_max == 0 {
        return Vec::new();
    }
    let span = (t.drops_max - t.drops_min) as f64 + 1.0;
    let rolls = t.drops_min as f64 + (rand() * span).floor().min(span - 1.0);
    let rolls = (rolls * luck.max(1.0)).round() as u32;
    let mut out: Vec<(String, u16)> = Vec::new();
    for _ in 0..rolls {
        let mut pick = (rand() * total as f64) as u32;
        let Some(d) = t.drops.iter().find(|d| {
            if pick < d.weight as u32 {
                true
            } else {
                pick -= d.weight as u32;
                false
            }
        }) else {
            continue;
        };
        let n = 1 + (rand() * d.max as f64).floor().min(d.max as f64 - 1.0) as u16;
        match out.iter_mut().find(|(name, _)| *name == d.item) {
            Some((_, c)) => *c += n,
            None => out.push((d.item.clone(), n)),
        }
    }
    out
}

#[derive(Debug, thiserror::Error)]
pub enum CreatureError {
    #[error("creature catalogue is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("creature `{0}` is listed twice")]
    Duplicate(String),
    #[error("creature `{0}` needs 1 to 5 tiers, a body, speeds and ranges above 0")]
    Bad(String),
    #[error("creature `{0}` drops `{1}`, which is not an item")]
    UnknownItem(String, String),
}

#[derive(Deserialize)]
struct CreatureFile {
    creature: Vec<CreatureDef>,
}

/// Every creature kind, by id (its place in the list, sent on the wire).
#[derive(Clone, Debug, PartialEq)]
pub struct CreatureKinds {
    defs: Vec<CreatureDef>,
    by_name: HashMap<String, u16>,
}

impl CreatureKinds {
    pub fn from_toml(src: &str) -> Result<Self, CreatureError> {
        let f: CreatureFile = toml::from_str(src)?;
        Self::from_defs(f.creature)
    }

    /// The catalogue shipped in `assets/creatures.toml`.
    pub fn builtin() -> Self {
        Self::from_toml(include_str!("../../../assets/creatures.toml")).expect("assets/creatures.toml is valid")
    }

    pub fn from_defs(mut defs: Vec<CreatureDef>) -> Result<Self, CreatureError> {
        let mut by_name = HashMap::new();
        for (i, d) in defs.iter_mut().enumerate() {
            if d.display.is_empty() {
                d.display = crate::items::title_case(&d.name);
            }
            let ok = (1..=5).contains(&d.tiers.len())
                && d.half_width > 0.0
                && d.half_width <= 2.0
                && d.height > 0.0
                && d.height <= 4.0
                && d.walk_speed > 0.0
                && d.run_speed >= d.walk_speed
                && d.run_speed <= 40.0
                && d.notice > 0.0
                && d.give_up >= d.notice
                && d.group[0] >= 1
                && d.group[1] >= d.group[0]
                && d.tiers.iter().all(|t| t.health > 0 && t.drops_max >= t.drops_min);
            if !ok {
                return Err(CreatureError::Bad(d.name.clone()));
            }
            if by_name.insert(d.name.clone(), i as u16).is_some() {
                return Err(CreatureError::Duplicate(d.name.clone()));
            }
        }
        Ok(Self { defs, by_name })
    }

    /// Checks every drop names an item (`known`): a startup error otherwise.
    pub fn check_items(&self, known: impl Fn(&str) -> bool) -> Result<(), CreatureError> {
        for d in &self.defs {
            for t in &d.tiers {
                if let Some(x) = t.drops.iter().find(|x| !known(&x.item)) {
                    return Err(CreatureError::UnknownItem(d.name.clone(), x.item.clone()));
                }
            }
        }
        Ok(())
    }

    pub fn get(&self, id: u16) -> Option<&CreatureDef> {
        self.defs.get(id as usize)
    }

    pub fn id(&self, name: &str) -> Option<u16> {
        self.by_name.get(name).copied()
    }

    pub fn defs(&self) -> &[CreatureDef] {
        &self.defs
    }

    pub fn iter(&self) -> impl Iterator<Item = (u16, &CreatureDef)> {
        self.defs.iter().enumerate().map(|(i, d)| (i as u16, d))
    }
}

#[cfg(test)]
mod element_tests {
    use super::*;

    #[test]
    fn each_element_resists_itself_and_fears_its_opposite() {
        for e in Element::ALL {
            assert_eq!(resistance(e, Some(e)), 0.8);
            assert_eq!(resistance(e, None), 0.4, "physical");
            assert_eq!(Element::from_code(e.code()), Some(e));
            let weak: Vec<Element> = Element::ALL
                .into_iter()
                .filter(|a| resistance(e, Some(*a)) < 0.0)
                .collect();
            assert_eq!(weak.len(), 1, "{e:?} has one weakness");
            assert_eq!(resistance(weak[0], Some(e)), -0.3, "opposites pair up");
        }
        assert_eq!(resistance(Element::Burn, Some(Element::Chill)), -0.3);
        assert_eq!(Element::from_code(0), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_matches_boundless() {
        let k = CreatureKinds::builtin();
        let items = crate::items::ItemRegistry::builtin(&crate::block::BlockRegistry::builtin());
        k.check_items(|n| items.id(n).is_ok()).unwrap();
        // tuning 3.2.2 (HP / armour / XP).
        let grazer = k.get(k.id("grazer").unwrap()).unwrap();
        let hp: Vec<_> = grazer.tiers.iter().map(|t| (t.health, t.armour, t.xp)).collect();
        assert_eq!(
            hp,
            [
                (1500, 0, 60),
                (3500, 0, 140),
                (9000, 0, 240),
                (25000, 0, 360),
                (60000, 0, 540)
            ]
        );
        let pelter = k.get(k.id("pelter").unwrap()).unwrap();
        assert_eq!(pelter.tier(5).armour, 1000);
        assert_eq!(pelter.tier(2).xp, 240);
        let dasher = k.get(k.id("dasher").unwrap()).unwrap();
        assert_eq!(dasher.top_tier(), 3);
        assert_eq!(dasher.tier(3).health, 5000);
        assert_eq!(dasher.title(2), "Hardy Dasher");
        assert_eq!(grazer.title(1), "Grazer");
    }

    #[test]
    fn part_two_matches_boundless() {
        let k = CreatureKinds::builtin();
        // tuning 3.2.2: hopper and cuttletrunk (HP / armour / XP).
        let popper = k.get(k.id("popper").unwrap()).unwrap();
        let p: Vec<_> = popper.tiers.iter().map(|t| (t.health, t.armour, t.xp)).collect();
        assert_eq!(
            p,
            [
                (1000, 0, 140),
                (2500, 100, 240),
                (4500, 200, 360),
                (7000, 300, 540),
                (10000, 400, 840)
            ]
        );
        assert_eq!((popper.tier(1).damage, popper.tier(2).damage), (400, 1000));
        let drifter = k.get(k.id("drifter").unwrap()).unwrap();
        let d: Vec<_> = drifter.tiers.iter().map(|t| (t.health, t.armour, t.xp)).collect();
        assert_eq!(
            d,
            [
                (1500, 100, 240),
                (3000, 350, 360),
                (5000, 750, 540),
                (10000, 1300, 840),
                (20000, 2000, 1200)
            ]
        );
        assert_eq!((popper.min_world_tier, drifter.min_world_tier), (3, 3));
        assert!(popper.tier(1).drops.iter().any(|d| d.item == "volatile_blood"));
    }

    #[test]
    fn exotics() {
        let e = Exotic(Exotic::HEALTH | Exotic::ARMOUR);
        assert_eq!((e.health(1000), e.armour(100), e.damage(100)), (3000, 200, 100));
        assert_eq!(e.xp(140), 210);
        assert_eq!(Exotic::default().xp(140), 140);
        let mut seed = 7u64;
        let mut rand = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let (mut exotic, mut most) = (0, 0);
        for _ in 0..10_000 {
            let x = roll_exotic(3, &mut rand);
            if x.any() {
                exotic += 1;
                most = most.max(x.0.count_ones());
            }
        }
        assert!((800..1200).contains(&exotic), "about 10% on a tier 3 world: {exotic}");
        assert_eq!(most, 2, "1 or 2 buffs on low worlds");
    }

    #[test]
    fn world_tiers() {
        assert_eq!(tiers_on(1), 1..=1);
        assert_eq!(tiers_on(2), 1..=1);
        assert_eq!(tiers_on(3), 1..=2);
        assert_eq!(tiers_on(8), 1..=5);
        assert!(!hostile_world(1));
    }

    #[test]
    fn drops_follow_the_weights() {
        let k = CreatureKinds::builtin();
        let grazer = k.get(k.id("grazer").unwrap()).unwrap();
        let mut seed = 1u64;
        let mut rand = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut counts: HashMap<String, u32> = HashMap::new();
        let mut kills_with_nothing = 0;
        for _ in 0..4000 {
            let d = roll_drops(grazer.tier(1), 1.0, &mut rand);
            if d.is_empty() {
                kills_with_nothing += 1;
            }
            for (n, c) in d {
                *counts.entry(n).or_default() += c as u32;
            }
        }
        assert_eq!(kills_with_nothing, 0, "tier 1 always rolls at least once");
        // meat 10/2 against bone 7/1: meat comes about 10/7 x 1.5 as often.
        let ratio = counts["raw_meat"] as f64 / counts["bone"] as f64;
        assert!((1.8..2.5).contains(&ratio), "{ratio}");
    }
}
