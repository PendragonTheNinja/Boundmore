//! Brews (milestone 23 part 2, `docs/milestones/23-water-and-brews.md`,
//! research `docs/research/brews-and-breath.md`).
//!
//! A brew is drunk like food (`PlayerState::eat` with `FoodStats::brew`), at
//! an input both sides agree on. Timed brews share one slot, beside the food
//! buff: a second replaces the first. Instant brews never touch the slot,
//! and Lifesaving has a slot of its own. Everything is an integer in
//! `PlayerState` stepped in `movement::step`, so prediction stays exact.

use crate::movement::{PlayerState, TICK_HZ};
use serde::{Deserialize, Serialize};

/// The brews (Boundless's names in brackets).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Brew {
    #[default]
    None,
    /// Health at once (instant healing).
    Restoring,
    /// Health every 5 s for a minute (healing).
    Mending,
    /// Energy at once (instant recovery).
    Rousing,
    /// Energy every second for a minute (recovery).
    Steady,
    /// Runs faster and swings faster (fast).
    Swift,
    /// More damage (strength).
    Brawn,
    /// Creatures notice you later (innocuous).
    Hushed,
    /// Carries a light (luminous).
    Glowing,
    /// Sickness taken away (cleansing).
    Clearing,
    /// Threat taken away from creatures near you (calming).
    Soothing,
    /// Threat added (aggravating).
    Taunting,
    /// Stands you back up when defeated (reviver).
    Lifesaving,
    /// Half the air under water (Kraken).
    Deepwater,
}

impl Brew {
    pub const ALL: [Brew; 13] = [
        Brew::Restoring,
        Brew::Mending,
        Brew::Rousing,
        Brew::Steady,
        Brew::Swift,
        Brew::Brawn,
        Brew::Hushed,
        Brew::Glowing,
        Brew::Clearing,
        Brew::Soothing,
        Brew::Taunting,
        Brew::Lifesaving,
        Brew::Deepwater,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Brew::None => "",
            Brew::Restoring => "Restoring",
            Brew::Mending => "Mending",
            Brew::Rousing => "Rousing",
            Brew::Steady => "Steady",
            Brew::Swift => "Swift",
            Brew::Brawn => "Brawn",
            Brew::Hushed => "Hushed",
            Brew::Glowing => "Glowing",
            Brew::Clearing => "Clearing",
            Brew::Soothing => "Soothing",
            Brew::Taunting => "Taunting",
            Brew::Lifesaving => "Lifesaving",
            Brew::Deepwater => "Deepwater",
        }
    }

    /// Done at once, leaving the brew slot alone.
    pub fn instant(self) -> bool {
        matches!(
            self,
            Brew::Restoring | Brew::Rousing | Brew::Clearing | Brew::Soothing | Brew::Taunting
        )
    }

    /// How long it lasts, in ticks (0 for instant ones). Timed brews last
    /// 10 minutes (Release 199), healing and energy over time 60 s, the air
    /// brew 20 minutes (Boundless's Halloween brews).
    pub fn ticks(self) -> u32 {
        let seconds = match self {
            Brew::Mending | Brew::Steady => 60,
            Brew::Swift | Brew::Brawn | Brew::Hushed | Brew::Glowing | Brew::Lifesaving => 600,
            Brew::Deepwater => 1200,
            _ => 0,
        };
        seconds * TICK_HZ
    }

    /// The grades it comes in (as Boundless's recipe data has them).
    pub fn grades(self) -> std::ops::RangeInclusive<u8> {
        match self {
            // No grades: an empty range.
            Brew::None => std::ops::RangeInclusive::new(1, 0),
            Brew::Hushed | Brew::Glowing => 2..=4,
            Brew::Soothing | Brew::Taunting => 5..=5,
            Brew::Lifesaving | Brew::Deepwater => 1..=1,
            _ => 1..=5,
        }
    }

    /// What it does at `grade`, in plain words (before healing skills).
    pub fn describe(self, grade: u8) -> String {
        let g = grade_index(grade);
        match self {
            Brew::None => String::new(),
            Brew::Restoring => format!("{} health at once", RESTORE[g]),
            Brew::Mending => format!("{} health every 5 s for a minute", MEND[g]),
            Brew::Rousing => format!("{} energy at once", ROUSE[g]),
            Brew::Steady => format!("{} energy a second for a minute", STEADY[g]),
            Brew::Swift => format!(
                "+{} m/s running, +{}% action speed for 10 minutes",
                SWIFT_RUN[g], SWIFT_ACTION[g]
            ),
            Brew::Brawn => format!("+{}% damage for 10 minutes", BRAWN[g]),
            Brew::Hushed => format!("Creatures notice you {}% nearer for 10 minutes", HUSH[g]),
            Brew::Glowing => "You give off light for 10 minutes".into(),
            Brew::Clearing => format!("Takes {} s off sickness", CLEAR[g]),
            Brew::Soothing => "Creatures near you forget you".into(),
            Brew::Taunting => "Creatures near you turn on you".into(),
            Brew::Lifesaving => "If you are defeated in the next 10 minutes, you get straight back up".into(),
            Brew::Deepwater => "Half the air under water for 20 minutes".into(),
        }
    }
}

fn grade_index(grade: u8) -> usize {
    (grade.clamp(1, 5) - 1) as usize
}

// Per-grade numbers, Boundless's (research A.2 to A.4).
pub const RESTORE: [u32; 5] = [750, 1500, 3000, 6000, 12000];
/// Every 5 s for 60 s: twelve times this (an inference from Release 199).
pub const MEND: [u32; 5] = [75, 150, 300, 600, 1200];
pub const ROUSE: [u32; 5] = [600, 1200, 2400, 4800, 9600];
/// Energy a second (Release 223).
pub const STEADY: [u32; 5] = [30, 60, 120, 360, 900];
pub const SWIFT_RUN: [f64; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];
pub const SWIFT_ACTION: [u32; 5] = [20, 40, 60, 80, 100];
pub const BRAWN: [u32; 5] = [10, 20, 30, 40, 50];
/// Percent nearer: grades II to IV are Boundmore's (II and IV inferred).
pub const HUSH: [u32; 5] = [20, 30, 40, 55, 70];
/// Light level carried (**guess**: Boundless gave 6 to 12 m).
pub const GLOW: [u8; 5] = [9, 10, 11, 12, 13];
/// Seconds of sickness taken away.
pub const CLEAR: [u32; 5] = [500, 1000, 1500, 2000, 2500];
/// Threat taken away or added by Soothing and Taunting.
pub const THREAT: f64 = 10_000.0;
/// Lifesaving stands you up with this share of max health, in percent, and
/// this share of your energy spent (**guesses**: Boundless gave "a health
/// and energy deficit").
pub const REVIVE_HEALTH_PERCENT: u32 = 25;
pub const REVIVE_SPENT_PERCENT: u32 = 50;

/// The timed brew a player has: which, its grade and ticks left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrewBuff {
    pub kind: Brew,
    pub grade: u8,
    pub left: u32,
}

impl BrewBuff {
    /// The grade index if `kind` is the brew in effect.
    fn grade_of(&self, kind: Brew) -> Option<usize> {
        (self.kind == kind && self.left > 0 && self.grade > 0).then(|| grade_index(self.grade))
    }
}

impl PlayerState {
    /// Healing multiplied by Insight: (100 + 10 a rank)% (Boundless's
    /// (90 + Intelligence) / 100).
    pub fn heal_amount(&self, base: u32) -> u32 {
        (base as u64 * (100 + self.body.heal_bonus as u64) / 100).min(u32::MAX as u64) as u32
    }

    /// Drinks a brew of `kind` at `grade`; called by `eat`.
    pub(crate) fn drink(&mut self, kind: Brew, grade: u8) {
        let g = grade_index(grade);
        match kind {
            Brew::None => {}
            Brew::Restoring => self.hurt = self.hurt.saturating_sub(self.heal_amount(RESTORE[g])),
            Brew::Rousing => self.spent = self.spent.saturating_sub(ROUSE[g].min(u16::MAX as u32) as u16),
            Brew::Clearing => self.sick = (self.sick as u32).saturating_sub(CLEAR[g] * TICK_HZ) as u16,
            // The server moves the creatures' threat; nothing on the body.
            Brew::Soothing | Brew::Taunting => {}
            Brew::Lifesaving => self.revive = self.buff_ticks(kind.ticks()),
            // Healing and energy over time keep their minute (an inference:
            // Zeal would otherwise add to what they give).
            Brew::Mending | Brew::Steady => {
                self.brew = BrewBuff {
                    kind,
                    grade,
                    left: kind.ticks(),
                };
            }
            _ => {
                self.brew = BrewBuff {
                    kind,
                    grade,
                    left: self.buff_ticks(kind.ticks()),
                };
            }
        }
    }

    /// One tick of brews: they run down; Mending heals every regen beat,
    /// Steady gives energy every second.
    pub(crate) fn brew_tick(&mut self) {
        self.revive = self.revive.saturating_sub(1);
        if self.brew.left == 0 {
            return;
        }
        if !self.down {
            // `beat` has already moved on this tick.
            if let Some(g) = self.brew.grade_of(Brew::Mending)
                && self.beat == 0
            {
                self.hurt = self.hurt.saturating_sub(self.heal_amount(MEND[g]));
            }
            if let Some(g) = self.brew.grade_of(Brew::Steady)
                && self.beat % TICK_HZ as u16 == 0
            {
                self.spent = self.spent.saturating_sub(STEADY[g] as u16);
            }
        }
        self.brew.left -= 1;
        if self.brew.left == 0 {
            self.brew = BrewBuff::default();
        }
    }

    /// Swift's extra run and sprint speed, blocks a second.
    pub fn brew_run_bonus(&self) -> f64 {
        self.brew.grade_of(Brew::Swift).map_or(0.0, |g| SWIFT_RUN[g])
    }

    /// Swift's action speed, as a fraction more (0.2 to 1).
    pub fn brew_action_speed(&self) -> f64 {
        self.brew
            .grade_of(Brew::Swift)
            .map_or(0.0, |g| SWIFT_ACTION[g] as f64 / 100.0)
    }

    /// Brawn's damage, as a fraction more (0.1 to 0.5).
    pub fn brew_damage(&self) -> f64 {
        self.brew.grade_of(Brew::Brawn).map_or(0.0, |g| BRAWN[g] as f64 / 100.0)
    }

    /// How far creatures notice you, as a multiple (Hushed: 0.3 to 0.7).
    pub fn brew_noticed_at(&self) -> f64 {
        self.brew
            .grade_of(Brew::Hushed)
            .map_or(1.0, |g| 1.0 - HUSH[g] as f64 / 100.0)
    }

    /// Glowing's carried light level (0 without it).
    pub fn brew_light(&self) -> u8 {
        self.brew.grade_of(Brew::Glowing).map_or(0, |g| GLOW[g])
    }

    /// Deepwater halves the air used under water.
    pub fn deepwater(&self) -> bool {
        self.brew.grade_of(Brew::Deepwater).is_some()
    }

    /// Lifesaving: instead of falling, back up at once. Returns true if it
    /// did (the brew is used).
    pub(crate) fn lifesave(&mut self) -> bool {
        if self.revive == 0 {
            return false;
        }
        self.revive = 0;
        let max = self.max_health();
        self.hurt = max - (max * REVIVE_HEALTH_PERCENT / 100).max(1);
        self.spent = self
            .spent
            .max((self.max_energy() as u32 * REVIVE_SPENT_PERCENT / 100) as u16);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::REGEN_PERIOD_TICKS;
    use crate::food::FoodStats;
    use crate::movement::{MoveConfig, PlayerInput, step};
    use crate::{BlockPos, PhysicsWorld, PlanetShape};
    use glam::DVec3;

    struct Floor;
    impl PhysicsWorld for Floor {
        fn solid(&self, p: BlockPos) -> bool {
            p.y < 10
        }
        fn liquid(&self, _: BlockPos) -> bool {
            false
        }
    }

    fn brew(kind: Brew, grade: u8) -> FoodStats {
        FoodStats {
            brew: kind,
            rank: grade,
            ..Default::default()
        }
    }

    fn standing() -> PlayerState {
        PlayerState {
            pos: DVec3::new(5.5, 10.0, 5.5),
            ..Default::default()
        }
    }

    fn run(s: &mut PlayerState, input: PlayerInput, ticks: u32) {
        let cfg = MoveConfig::default();
        for _ in 0..ticks {
            step(s, &input, &cfg, None, PlanetShape::new(16, 8), &Floor);
        }
    }

    #[test]
    fn every_grade_is_valid_food() {
        for kind in Brew::ALL {
            for g in kind.grades() {
                assert!(brew(kind, g).valid(), "{kind:?} {g}");
            }
            let below = *kind.grades().start() - 1;
            assert!(!brew(kind, below).valid() || below == 0 && !brew(kind, 0).valid());
            assert!(
                !brew(kind, kind.grades().end() + 1).valid(),
                "{kind:?} above its grades"
            );
        }
    }

    #[test]
    fn restoring_heals_at_once_and_insight_adds_a_tenth_a_rank() {
        let mut s = standing();
        s.body.health_bonus = 10_000;
        s.hurt = 2000;
        s.eat(&brew(Brew::Restoring, 1)).unwrap();
        assert_eq!(s.hurt, 1250);
        assert_eq!(s.brew, BrewBuff::default(), "instant: the slot is untouched");
        s.chew = 0;
        s.body.heal_bonus = 30;
        s.hurt = 2000;
        s.eat(&brew(Brew::Restoring, 1)).unwrap();
        assert_eq!(s.hurt, 2000 - 975, "750 x 1.3");
    }

    #[test]
    fn mending_heals_every_beat_for_a_minute() {
        let mut s = standing();
        s.body.health_bonus = 100_000;
        s.hurt = 50_000;
        // Half hungry: no regeneration from a full belly, only the brew.
        s.hunger = 300 * crate::food::HUNGER_SCALE;
        s.eat(&brew(Brew::Mending, 2)).unwrap();
        run(&mut s, PlayerInput::default(), 70 * crate::TICK_HZ);
        let beats = 60 * crate::TICK_HZ / u32::from(REGEN_PERIOD_TICKS);
        assert_eq!(50_000 - s.hurt, 150 * beats, "twelve beats of 150");
        assert_eq!(s.brew, BrewBuff::default(), "gone after a minute");
    }

    #[test]
    fn rousing_and_steady_give_energy() {
        let mut s = standing();
        s.spent = 500;
        s.eat(&brew(Brew::Rousing, 1)).unwrap();
        assert_eq!(s.spent, 0, "600 at once");
        s.chew = 0;
        s.body.energy_bonus = 10_000;
        s.spent = 9_000;
        s.rest = u16::MAX;
        s.eat(&brew(Brew::Steady, 1)).unwrap();
        run(&mut s, PlayerInput::default(), 10 * crate::TICK_HZ);
        assert!(s.spent <= 9_000 - 10 * 30, "{}", s.spent);
    }

    #[test]
    fn a_second_timed_brew_replaces_the_first_and_an_instant_one_leaves_it() {
        let mut s = standing();
        s.eat(&brew(Brew::Swift, 3)).unwrap();
        assert_eq!(s.brew_run_bonus(), 1.0);
        assert!((s.brew_action_speed() - 0.6).abs() < 1e-9);
        s.chew = 0;
        s.eat(&brew(Brew::Brawn, 5)).unwrap();
        assert_eq!(s.brew_run_bonus(), 0.0, "replaced");
        assert!((s.brew_damage() - 0.5).abs() < 1e-9);
        s.chew = 0;
        s.hurt = 100;
        s.eat(&brew(Brew::Restoring, 1)).unwrap();
        assert!((s.brew_damage() - 0.5).abs() < 1e-9, "kept");
        s.chew = 0;
        s.eat(&brew(Brew::Lifesaving, 1)).unwrap();
        assert!((s.brew_damage() - 0.5).abs() < 1e-9, "its own slot");
        assert!(s.revive > 0);
        run(&mut s, PlayerInput::default(), Brew::Brawn.ticks());
        assert_eq!(s.brew_damage(), 0.0, "ran out");
        assert_eq!(s.revive, 0);
    }

    #[test]
    fn swift_runs_faster() {
        let (yaw, _) = PlayerInput::from_angles(std::f64::consts::FRAC_PI_2, 0.0);
        let east = PlayerInput {
            forward: 1,
            yaw,
            ..Default::default()
        };
        let mut plain = standing();
        let mut quick = standing();
        quick.eat(&brew(Brew::Swift, 5)).unwrap();
        run(&mut plain, east, 120);
        run(&mut quick, east, 120);
        assert!(
            (quick.vel.x - (plain.vel.x + 2.0)).abs() < 1e-6,
            "{} vs {}",
            quick.vel.x,
            plain.vel.x
        );
    }

    #[test]
    fn lifesaving_stands_you_up_instead_of_falling() {
        let mut s = standing();
        s.eat(&brew(Brew::Lifesaving, 1)).unwrap();
        s.take(s.max_health() * 2);
        assert!(!s.down, "back up");
        assert_eq!(s.health(), s.max_health() * REVIVE_HEALTH_PERCENT / 100);
        assert_eq!(s.revive, 0, "used");
        s.take(s.max_health());
        assert!(s.down, "only once");
    }

    /// Zeal: buffs 6% longer a rank, debuffs 6% shorter (Boundless's skill
    /// text); healing over time keeps its minute.
    #[test]
    fn zeal_lengthens_buffs_and_shortens_sickness() {
        let mut s = standing();
        s.body.buff_bonus = 48;
        s.eat(&brew(Brew::Swift, 1)).unwrap();
        assert_eq!(s.brew.left, Brew::Swift.ticks() * 148 / 100);
        s.chew = 0;
        s.eat(&brew(Brew::Mending, 1)).unwrap();
        assert_eq!(s.brew.left, Brew::Mending.ticks());
        s.chew = 0;
        s.hunger = 1000 * crate::food::HUNGER_SCALE;
        s.eat(&FoodStats {
            hunger: 10,
            sick: true,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(s.sick as u32, crate::food::SICK_TICKS as u32 * 52 / 100);
    }

    #[test]
    fn clearing_takes_sickness_away() {
        let mut s = standing();
        s.sick = crate::food::SICK_TICKS;
        s.eat(&brew(Brew::Clearing, 1)).unwrap();
        assert_eq!(s.sick, 0);
    }

    #[test]
    fn deepwater_halves_the_breath_and_with_leaden_none_is_used() {
        let mut s = standing();
        assert_eq!(s.breath_halvings(), 0);
        s.eat(&brew(Brew::Deepwater, 1)).unwrap();
        assert_eq!(s.breath_halvings(), 1);
        s.quirks = crate::forge::Quirks(crate::forge::Quirks::LEADEN);
        assert_eq!(s.breath_halvings(), 2);
    }

    #[test]
    fn hushed_and_glowing_by_grade() {
        let mut s = standing();
        s.eat(&brew(Brew::Hushed, 4)).unwrap();
        assert!((s.brew_noticed_at() - 0.45).abs() < 1e-9);
        s.chew = 0;
        s.eat(&brew(Brew::Glowing, 2)).unwrap();
        assert_eq!(s.brew_light(), 10);
        assert_eq!(s.brew_noticed_at(), 1.0);
    }
}
