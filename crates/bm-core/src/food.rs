//! Hunger and food, as in Boundless (milestone 16, `docs/milestones/16-food.md`,
//! tuning 3.9).
//!
//! Hunger is a number of points that comes off the energy bar: the bar's
//! usable size is max energy less hunger. Uses, running, sprinting and
//! jumping add to it; food takes it away. One food buff at a time; eating a
//! dish with a buff replaces the old one. Being nearly unhungry ("Full") is
//! what heals out of combat; raw meat makes you sick; full hunger starves.
//!
//! Everything here lives in `PlayerState` as integers and is stepped in
//! `movement::step`, so client and server agree on every tick. Eating
//! happens right after an input both sides agree on (`ClientMsg::Eat`).

use crate::combat::Defence;
use crate::energy::MAX_ENERGY;
use crate::movement::{PlayerState, TICK_HZ};
use serde::{Deserialize, Serialize};

/// Hunger is counted in 1/600 of a point, so 0.5 a second is a whole number
/// per tick and a copper hammer's 0.08 a swing is exact.
pub const HUNGER_SCALE: u32 = 600;

/// Running on the ground: 0.5 a second (a **guess**; Boundless publishes no
/// rate, `docs/feel.md`).
pub const RUN_HUNGER_PER_TICK: u32 = HUNGER_SCALE / 2 / TICK_HZ;
/// Sprinting: 1 a second (a **guess**).
pub const SPRINT_HUNGER_PER_TICK: u32 = HUNGER_SCALE / TICK_HZ;
/// Each jump: 1 (a **guess**).
pub const JUMP_HUNGER: u32 = HUNGER_SCALE;
/// A defeat adds this share of the full bar, in percent (Boundless applies
/// "a hunger penalty"; the size is a **guess**).
pub const DEFEAT_HUNGER_PERCENT: u32 = 25;
/// Full under this share of the full bar, in percent (Boundless's Full Up).
pub const FULL_PERCENT: u32 = 10;
/// Raw meat: 40 s of sickness, 5 damage every 5 s (Boundless).
pub const SICK_TICKS: u16 = (TICK_HZ * 40) as u16;
pub const SICK_DAMAGE: u32 = 5;
/// Starving takes this share of max health every 5 s, in percent (a
/// **guess**: from full to defeated in about four minutes).
pub const STARVE_PERCENT: u32 = 2;
/// At most one bite a second (a **guess**).
pub const EAT_TICKS: u16 = TICK_HZ as u16;

/// How a use turns its energy into hunger (Boundless's per-item "hunger
/// stamina drain", tuning 1 and 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Drain {
    /// Tools, totems, spanners and chisels: energy / 100.
    Tool,
    /// Grapples, slings and bombs: energy / 500.
    Ranged,
}

impl Drain {
    /// Hunger (in 1/600 points) one use costing `energy` adds.
    pub fn hunger(self, energy: u16) -> u32 {
        match self {
            Drain::Tool => energy as u32 * HUNGER_SCALE / 100,
            Drain::Ranged => energy as u32 * HUNGER_SCALE / 500,
        }
    }
}

/// The food buffs (Boundless's names in brackets, tuning 3.9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Buff {
    #[default]
    None,
    /// No hunger gained (Well Fed).
    Sated,
    /// Energy refills faster (Recovery).
    Refreshed,
    /// More max energy (Energise).
    Lively,
    /// More critical hits (Critical Chance).
    Keen,
    /// Uses sometimes cost no durability (Light Touch).
    Gentle,
    /// Armour and resistance (Shield Physical).
    Guarded,
    /// More max health (Invigorate).
    Hearty,
    /// Jump higher, fall softer (Light).
    Springy,
    /// More XP (Fast Learner).
    Studious,
    /// Protection against every kind of hazardous air (milestone 18;
    /// Boundless's atmosphere protection).
    Breathing,
}

impl Buff {
    pub const ALL: [Buff; 10] = [
        Buff::Sated,
        Buff::Refreshed,
        Buff::Lively,
        Buff::Keen,
        Buff::Gentle,
        Buff::Guarded,
        Buff::Hearty,
        Buff::Springy,
        Buff::Studious,
        Buff::Breathing,
    ];

    /// What players see.
    pub fn name(self) -> &'static str {
        match self {
            Buff::None => "",
            Buff::Sated => "Sated",
            Buff::Refreshed => "Refreshed",
            Buff::Lively => "Lively",
            Buff::Keen => "Keen",
            Buff::Gentle => "Gentle",
            Buff::Guarded => "Guarded",
            Buff::Hearty => "Hearty",
            Buff::Springy => "Springy",
            Buff::Studious => "Studious",
            Buff::Breathing => "Breathing",
        }
    }

    /// The highest rank (Studious has four, the rest five).
    pub fn max_rank(self) -> u8 {
        match self {
            Buff::None => 0,
            Buff::Studious => 4,
            Buff::Breathing => 3,
            _ => 5,
        }
    }

    /// What it does at `rank` (1 based), in plain words.
    pub fn describe(self, rank: u8) -> String {
        let r = (rank.clamp(1, 5) - 1) as usize;
        match self {
            Buff::None => String::new(),
            Buff::Sated => "No hunger".into(),
            Buff::Refreshed => format!("+{} energy a second", REFRESH[r]),
            Buff::Lively => format!("+{}% max energy", PERCENT_UP[r]),
            Buff::Keen => format!("+{}% crit chance", PERCENT_UP[r]),
            Buff::Gentle => format!("-{}% tool wear", GENTLE[r]),
            Buff::Guarded => format!("+{} armour, +{}% resistance", GUARD_ARMOUR[r], GUARD_RESIST[r]),
            Buff::Hearty => format!("+{}% max health", PERCENT_UP[r]),
            Buff::Springy => format!("+{} m jump, +{} falling armour", SPRING_JUMP[r], SPRING_FALL[r]),
            Buff::Studious => format!("+{}% XP", STUDY[r.min(3)]),
            Buff::Breathing => format!("+{} protection against hazardous air", r.min(2) + 1),
        }
    }
}

// Per-rank effects, Boundless's numbers (tuning 3.9).
/// Refreshed: energy a second (Release 223).
const REFRESH: [u16; 5] = [10, 20, 40, 120, 300];
/// Lively, Keen and Hearty: percent more.
const PERCENT_UP: [u32; 5] = [20, 40, 60, 80, 100];
/// Gentle: percent chance a use costs no durability.
const GENTLE: [u32; 5] = [5, 10, 15, 30, 40];
const GUARD_ARMOUR: [u32; 5] = [10, 20, 40, 70, 100];
const GUARD_RESIST: [u32; 5] = [5, 10, 15, 20, 25];
/// Springy: extra jump height in metres, and falling armour.
const SPRING_JUMP: [f64; 5] = [0.5, 0.8, 1.1, 1.5, 2.0];
const SPRING_FALL: [u16; 5] = [50, 75, 100, 150, 200];
/// Studious: percent more XP.
const STUDY: [u32; 4] = [10, 20, 50, 100];

/// The buff a player has: which, its rank and ticks left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoodBuff {
    pub kind: Buff,
    pub rank: u8,
    pub left: u32,
}

impl FoodBuff {
    /// Protection against hazardous air (Breathing: +1, +2, +3).
    pub fn air_points(&self) -> u8 {
        self.rank_of(Buff::Breathing).map_or(0, |r| r.min(2) as u8 + 1)
    }

    /// The rank if `kind` is the active buff.
    fn rank_of(&self, kind: Buff) -> Option<usize> {
        (self.kind == kind && self.left > 0 && self.rank > 0).then(|| (self.rank.min(5) - 1) as usize)
    }
}

/// What eating one does (`assets/items.toml` `food`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FoodStats {
    /// Hunger taken away, in points.
    pub hunger: u16,
    /// Raw meat: sickness.
    pub sick: bool,
    pub buff: Buff,
    pub rank: u8,
    /// How long the buff lasts.
    pub minutes: u16,
}

impl FoodStats {
    pub fn valid(&self) -> bool {
        match self.buff {
            Buff::None => true,
            b => self.rank >= 1 && self.rank <= b.max_rank() && self.minutes > 0 && self.minutes <= 240,
        }
    }
}

/// Why eating was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Still chewing the last bite.
    Chewing,
    /// Nothing to gain: no hunger and no buff to it.
    Full,
    Down,
}

impl PlayerState {
    /// The bar with no hunger: 600 plus skills, times Lively.
    pub fn full_energy(&self) -> u16 {
        let base = MAX_ENERGY.saturating_add(self.body.energy_bonus) as u32;
        let mut up = self.buff.rank_of(Buff::Lively).map_or(0, |r| PERCENT_UP[r]);
        if self.quirks.has(crate::forge::Quirks::GNAWING) {
            up += crate::forge::DRAIN_MAX_PERCENT;
        }
        (base * (100 + up) / 100).min(u16::MAX as u32) as u16
    }

    /// Hunger in whole points.
    pub fn hunger_points(&self) -> u16 {
        (self.hunger / HUNGER_SCALE).min(u16::MAX as u32) as u16
    }

    /// The most hunger there can be: the full bar.
    fn hunger_cap(&self) -> u32 {
        self.full_energy() as u32 * HUNGER_SCALE
    }

    /// Hunger under 10% of the bar: health comes back out of combat.
    pub fn is_full(&self) -> bool {
        self.hunger * 100 < self.hunger_cap() * FULL_PERCENT
    }

    /// Hunger at the full bar: health drains until you eat.
    pub fn starving(&self) -> bool {
        self.hunger >= self.hunger_cap() && self.hunger_cap() > 0
    }

    /// Adds `units` (1/600 points) of hunger, unless Sated or down.
    pub fn add_hunger(&mut self, units: u32) {
        if self.down || self.buff.rank_of(Buff::Sated).is_some() {
            return;
        }
        self.hunger = self.hunger.saturating_add(units).min(self.hunger_cap());
    }

    /// The penalty for a defeat (added after the buff is checked: a Sated
    /// player still pays it).
    pub fn defeat_hunger(&mut self) {
        let add = self.hunger_cap() / 100 * DEFEAT_HUNGER_PERCENT;
        self.hunger = self.hunger.saturating_add(add).min(self.hunger_cap());
    }

    /// Whether eating `food` now would do anything, and if not, why.
    pub fn can_eat(&self, food: &FoodStats) -> Result<(), Refusal> {
        if self.down {
            return Err(Refusal::Down);
        }
        if self.chew > 0 {
            return Err(Refusal::Chewing);
        }
        if self.hunger == 0 && food.buff == Buff::None {
            return Err(Refusal::Full);
        }
        Ok(())
    }

    /// Eats one `food`: its hunger off, its buff on (replacing the old one;
    /// food with no buff leaves the buff alone), sickness for raw meat.
    pub fn eat(&mut self, food: &FoodStats) -> Result<(), Refusal> {
        self.can_eat(food)?;
        self.hunger = self.hunger.saturating_sub(food.hunger as u32 * HUNGER_SCALE);
        if food.buff != Buff::None {
            self.buff = FoodBuff {
                kind: food.buff,
                rank: food.rank,
                left: food.minutes as u32 * 60 * TICK_HZ,
            };
        }
        if food.sick {
            self.sick = SICK_TICKS;
        }
        self.chew = EAT_TICKS;
        self.clamp_to_maxima();
        Ok(())
    }

    /// One tick of food: the buff and sickness run down. Sickness and
    /// starving hurt every 5 s (`period`).
    pub(crate) fn food_tick(&mut self, period: bool) {
        self.chew = self.chew.saturating_sub(1);
        if self.buff.left > 0 {
            self.buff.left -= 1;
            if self.buff.left == 0 {
                self.buff = FoodBuff::default();
                self.clamp_to_maxima();
            }
        }
        if self.down {
            return;
        }
        let sick = self.sick > 0;
        self.sick = self.sick.saturating_sub(1);
        if period {
            let mut d = 0;
            if sick {
                d += SICK_DAMAGE;
            }
            if self.starving() {
                d += (self.max_health() * STARVE_PERCENT / 100).max(1);
            }
            if d > 0 {
                self.take(d);
            }
        }
    }

    /// A buff ending (or hunger shrinking the bar) never leaves more spent
    /// or lost than there is.
    pub fn clamp_to_maxima(&mut self) {
        self.spent = self.spent.min(self.max_energy());
        self.hunger = self.hunger.min(self.hunger_cap());
        if !self.down {
            self.hurt = self.hurt.min(self.max_health().saturating_sub(1));
        }
    }

    /// Hearty's share more health, in percent.
    pub(crate) fn health_up_percent(&self) -> u32 {
        let draining = if self.quirks.has(crate::forge::Quirks::DRAINING) {
            crate::forge::DRAIN_MAX_PERCENT
        } else {
            0
        };
        self.buff.rank_of(Buff::Hearty).map_or(0, |r| PERCENT_UP[r]) + draining
    }

    /// Refreshed's extra energy a second.
    pub(crate) fn refresh_per_second(&self) -> u16 {
        self.buff.rank_of(Buff::Refreshed).map_or(0, |r| REFRESH[r])
    }

    /// Springy's extra jump height, in blocks.
    pub fn jump_extra(&self) -> f64 {
        self.buff.rank_of(Buff::Springy).map_or(0.0, |r| SPRING_JUMP[r])
    }

    /// Falling armour from skills and Springy.
    pub fn falling_armour(&self) -> u32 {
        let slippery = if self.quirks.has(crate::forge::Quirks::SLIPPERY) {
            crate::forge::SLIPPERY_FALLING_ARMOUR
        } else {
            0
        };
        self.body.falling_armour as u32
            + self.buff.rank_of(Buff::Springy).map_or(0, |r| SPRING_FALL[r] as u32)
            + slippery
    }

    /// Keen multiplies crit chance: 1.2 to 2.0.
    pub fn crit_multiplier(&self) -> f32 {
        1.0 + self.buff.rank_of(Buff::Keen).map_or(0, |r| PERCENT_UP[r]) as f32 / 100.0
    }

    /// Gentle's chance (0..1) a use costs no durability.
    pub fn wear_skip_chance(&self) -> f64 {
        self.buff.rank_of(Buff::Gentle).map_or(0, |r| GENTLE[r]) as f64 / 100.0
    }

    /// The chance a use costs no durability, with `skill`'s chance (the
    /// durability epic) and Gentle's both rolled: they multiply, as
    /// Boundless's buffs and skills do.
    pub fn wear_spared(&self, skill: f64) -> f64 {
        1.0 - (1.0 - skill) * (1.0 - self.wear_skip_chance())
    }

    /// Studious: percent more XP.
    pub fn xp_bonus_percent(&self) -> u32 {
        self.buff.rank_of(Buff::Studious).map_or(0, |r| STUDY[r.min(3)])
    }

    /// Guarded's armour and resistance added to `d`.
    pub fn guarded(&self, mut d: Defence) -> Defence {
        if let Some(r) = self.buff.rank_of(Buff::Guarded) {
            d.kinetic_armour += GUARD_ARMOUR[r];
            d.impulse_armour += GUARD_ARMOUR[r];
            let add = GUARD_RESIST[r] as f32 / 100.0;
            d.kinetic_resist = (d.kinetic_resist + add).min(1.0);
            d.impulse_resist = (d.impulse_resist + add).min(1.0);
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::REGEN_PERIOD_TICKS;
    use crate::movement::Body;

    fn meat() -> FoodStats {
        FoodStats {
            hunger: 400,
            ..Default::default()
        }
    }

    fn broth() -> FoodStats {
        FoodStats {
            hunger: 1200,
            buff: Buff::Sated,
            rank: 1,
            minutes: 10,
            ..Default::default()
        }
    }

    #[test]
    fn hunger_shrinks_the_bar_and_food_restores_it() {
        let mut s = PlayerState::default();
        assert_eq!(s.max_energy(), 600);
        s.add_hunger(250 * HUNGER_SCALE);
        assert_eq!(s.max_energy(), 350);
        assert_eq!(s.energy(), 350);
        s.eat(&meat()).unwrap();
        assert_eq!(s.hunger, 0, "cooked meat takes 400 off");
        assert_eq!(s.max_energy(), 600);
        // Never more than the bar.
        s.add_hunger(10_000 * HUNGER_SCALE);
        assert_eq!(s.max_energy(), 0);
        assert!(s.starving());
    }

    #[test]
    fn uses_cost_hunger_by_their_kind() {
        // Copper hammer: 8 energy, 0.08 hunger; a copper grapple 60, 0.12.
        assert_eq!(Drain::Tool.hunger(8), 48);
        assert_eq!(Drain::Ranged.hunger(60), 72);
        let mut s = PlayerState::default();
        assert!(s.try_spend(8, Drain::Tool));
        assert_eq!(s.hunger, 48);
        // A refused use costs no hunger.
        s.spent = 600;
        assert!(!s.try_spend(8, Drain::Tool));
        assert_eq!(s.hunger, 48);
    }

    #[test]
    fn eating_rules() {
        let mut s = PlayerState::default();
        assert_eq!(s.eat(&meat()), Err(Refusal::Full), "nothing to gain");
        s.eat(&broth()).unwrap();
        assert_eq!(s.buff.kind, Buff::Sated);
        assert_eq!(s.eat(&broth()), Err(Refusal::Chewing));
        for _ in 0..EAT_TICKS {
            s.food_tick(false);
        }
        // Sated: no hunger gained.
        s.add_hunger(1000);
        assert_eq!(s.hunger, 0);
        // Plain food leaves the buff; a buff dish replaces it.
        s.buff.kind = Buff::Keen;
        s.hunger = 100 * HUNGER_SCALE;
        s.eat(&meat()).unwrap();
        assert_eq!(s.buff.kind, Buff::Keen);
        for _ in 0..EAT_TICKS {
            s.food_tick(false);
        }
        s.eat(&broth()).unwrap();
        assert_eq!(s.buff.kind, Buff::Sated);
        assert_eq!(s.buff.left, 10 * 60 * TICK_HZ);
        // It runs out.
        for _ in 0..s.buff.left {
            s.food_tick(false);
        }
        assert_eq!(s.buff, FoodBuff::default());
    }

    #[test]
    fn raw_meat_makes_you_sick() {
        let mut s = PlayerState {
            hunger: 300 * HUNGER_SCALE,
            ..Default::default()
        };
        s.eat(&FoodStats {
            hunger: 100,
            sick: true,
            ..Default::default()
        })
        .unwrap();
        let mut hurt = 0;
        for t in 1..=SICK_TICKS as u32 + 600 {
            s.food_tick(t % REGEN_PERIOD_TICKS as u32 == 0);
            hurt = s.hurt;
        }
        assert_eq!(hurt, SICK_DAMAGE * 8, "5 every 5 s for 40 s");
    }

    #[test]
    fn starving_hurts_until_defeat() {
        let mut s = PlayerState::default();
        s.add_hunger(u32::MAX);
        let mut periods = 0;
        while !s.down {
            s.food_tick(true);
            periods += 1;
        }
        assert_eq!(periods, 50, "2% of 600 every 5 s");
    }

    #[test]
    fn full_is_under_a_tenth() {
        let mut s = PlayerState::default();
        assert!(s.is_full());
        s.add_hunger(59 * HUNGER_SCALE);
        assert!(s.is_full());
        s.add_hunger(HUNGER_SCALE);
        assert!(!s.is_full());
    }

    #[test]
    fn buffs_change_the_body() {
        let mut s = PlayerState {
            body: Body {
                falling_armour: 10,
                ..Default::default()
            },
            ..Default::default()
        };
        let set = |s: &mut PlayerState, kind, rank| {
            s.buff = FoodBuff { kind, rank, left: 100 };
        };
        set(&mut s, Buff::Lively, 2);
        assert_eq!(s.max_energy(), 840);
        set(&mut s, Buff::Hearty, 5);
        assert_eq!(s.max_health(), 1200);
        set(&mut s, Buff::Keen, 1);
        assert!((s.crit_multiplier() - 1.2).abs() < 1e-6);
        set(&mut s, Buff::Gentle, 4);
        assert!((s.wear_skip_chance() - 0.3).abs() < 1e-9);
        set(&mut s, Buff::Studious, 4);
        assert_eq!(s.xp_bonus_percent(), 100);
        set(&mut s, Buff::Springy, 1);
        assert_eq!(s.falling_armour(), 60);
        assert!((s.jump_extra() - 0.5).abs() < 1e-9);
        set(&mut s, Buff::Guarded, 3);
        let d = s.guarded(Defence::default());
        assert_eq!(d.kinetic_armour, 40);
        assert!((d.impulse_resist - 0.15).abs() < 1e-6);
        set(&mut s, Buff::Refreshed, 5);
        assert_eq!(s.refresh_per_second(), 300);
    }

    #[test]
    fn a_buff_ending_never_leaves_more_lost_than_there_is() {
        let mut s = PlayerState {
            buff: FoodBuff {
                kind: Buff::Hearty,
                rank: 5,
                left: 1,
            },
            hurt: 1000,
            ..Default::default()
        };
        s.food_tick(false);
        assert_eq!(s.max_health(), 600);
        assert_eq!(s.hurt, 599, "one health left, not defeated");
        assert!(!s.down);
    }

    #[test]
    fn defeat_costs_a_quarter_of_the_bar() {
        let mut s = PlayerState::default();
        s.defeat_hunger();
        assert_eq!(s.hunger_points(), 150);
    }
}
