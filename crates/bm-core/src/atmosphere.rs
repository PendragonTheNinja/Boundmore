//! Air on the higher worlds (milestone 18, `docs/milestones/18-higher-worlds.md`).
//!
//! From tier 4 every planet's air belongs to one of three lines (acid,
//! charged, burning) and has a hazard set by its tier. A player's protection
//! against a line is their ward skill for it plus an atmosphere food's
//! points; when protection falls short the body's air runs down, and then
//! they choke. Everything here runs inside `movement::step`, on the server
//! and in the client's prediction alike, from state both sides agree on: the
//! planet's air (in `PlayerState`), the wards (in `Body`, set at an agreed
//! input like every skill) and the food buff.

use crate::combat::REGEN_PERIOD_TICKS;
use crate::movement::{PlayerState, TICK_HZ};
use serde::{Deserialize, Serialize};

/// A line of atmosphere (Boundless's caustic, potent and volatile).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AirLine {
    /// Ordinary air: tiers 1 to 3.
    #[default]
    None,
    /// Boundless's caustic (Lush, Corrosive, Toxic).
    Acid,
    /// Boundless's potent (Metal, Shock, Chill).
    Charged,
    /// Boundless's volatile (Coal, Blast, Burn).
    Burning,
}

impl AirLine {
    pub const ALL: [AirLine; 3] = [AirLine::Acid, AirLine::Charged, AirLine::Burning];

    /// Index into `Body::wards` (`None` has none).
    pub fn index(self) -> Option<usize> {
        match self {
            AirLine::None => None,
            AirLine::Acid => Some(0),
            AirLine::Charged => Some(1),
            AirLine::Burning => Some(2),
        }
    }

    /// What players see ("acid air").
    pub fn name(self) -> &'static str {
        match self {
            AirLine::None => "ordinary air",
            AirLine::Acid => "acid air",
            AirLine::Charged => "charged air",
            AirLine::Burning => "burning air",
        }
    }

    /// The ward skill that protects against it.
    pub fn ward(self) -> &'static str {
        match self {
            AirLine::None => "",
            AirLine::Acid => "Acid Ward",
            AirLine::Charged => "Spark Ward",
            AirLine::Burning => "Fire Ward",
        }
    }
}

/// A planet's air: its line and how much protection it needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Air {
    pub line: AirLine,
    pub hazard: u8,
}

impl Air {
    /// The air of a planet of `tier` whose kind has `line`. Tiers 1 to 3
    /// have none whatever the kind says.
    pub fn of(line: AirLine, tier: u8) -> Air {
        let hazard = hazard_for_tier(tier);
        if hazard == 0 || line == AirLine::None {
            return Air::default();
        }
        Air { line, hazard }
    }

    pub fn hazardous(&self) -> bool {
        self.hazard > 0 && self.line != AirLine::None
    }
}

/// Boundless's hazard by world tier (boundlexx world code, R198; tuning
/// 4.13.1): none to tier 3, then 1, 3, 5, 7, 10.
pub fn hazard_for_tier(tier: u8) -> u8 {
    match tier {
        0..=3 => 0,
        4 => 1,
        5 => 3,
        6 => 5,
        7 => 7,
        _ => 10,
    }
}

/// Protection a ward skill gives at `ranks` (Boundless: +1, +1, +1, +2, +2).
pub fn ward_points(ranks: u8) -> u8 {
    const TOTAL: [u8; 6] = [0, 1, 2, 3, 5, 7];
    TOTAL[ranks.min(5) as usize]
}

/// Ticks of air in a full bar: 30 s (**guess**; Boundless's air bar length is
/// unpublished, Measure 1).
pub const AIR_TICKS: u16 = (TICK_HZ * 30) as u16;
/// Air comes back this much a tick when protected: empty to full in about
/// 8 s (**guess**, Measure 3).
pub const AIR_REFILL: u16 = 4;
/// Choking takes this percent of max health every 5 s, however far short
/// the protection falls (players: "you suffocate at the same rate, no
/// matter how many points you have"; forum check 1). The rate is a
/// **guess** (Measure 2).
pub const CHOKE_PERCENT: u32 = 5;
/// Drowning takes this percent of max health every 5 s (milestone 23;
/// Boundless's drowning was a share of max health, t/17849; the rate is a
/// **guess**, Measure 2).
pub const DROWN_PERCENT: u32 = 10;
/// Fog closes to this many blocks when the air is gone (**guess**).
pub const FOG_AT_EMPTY: f32 = 30.0;

impl PlayerState {
    /// Protection against `line` from wards and food.
    pub fn protection(&self, line: AirLine) -> u8 {
        let ward = line.index().map_or(0, |i| self.body.wards[i]);
        ward.saturating_add(self.buff.air_points(line))
    }

    /// How far protection falls short of this planet's hazard.
    pub fn shortfall(&self) -> u8 {
        if !self.air.hazardous() {
            return 0;
        }
        self.air.hazard.saturating_sub(self.protection(self.air.line))
    }

    /// Out of air with a shortfall: health is going.
    pub fn choking(&self) -> bool {
        self.air_used >= AIR_TICKS && self.shortfall() > 0 && !self.down
    }

    /// Out of air with the eyes under a liquid (milestone 23).
    pub fn drowning(&self) -> bool {
        self.air_used >= AIR_TICKS && self.submerged && !self.down
    }

    /// How many times the air used under water is halved: Leaden (Boundless's
    /// That Sinking Feeling, +50% breath) once. Two halvings use none, as
    /// Boundless's quirk with its air brew did.
    pub fn breath_halvings(&self) -> u8 {
        u8::from(self.quirks.has(crate::forge::Quirks::LEADEN))
    }

    /// Air left, 0 to 1 (for the bar and the fog).
    pub fn air_left(&self) -> f32 {
        f32::from(AIR_TICKS.saturating_sub(self.air_used)) / f32::from(AIR_TICKS)
    }

    /// One tick of air: it is used while short of protection or with the
    /// eyes under a liquid, and comes back otherwise; once it is all used
    /// every regen beat takes health. A player who is down keeps what air
    /// they had.
    pub(crate) fn air_tick(&mut self, period: bool) {
        if self.down {
            return;
        }
        let short = self.shortfall() > 0;
        let leaden = self.quirks.has(crate::forge::Quirks::LEADEN);
        if !short && !self.submerged {
            // Leaden breathes back in at half the rate (Boundless: "much
            // much much longer to breathe in").
            let refill = if leaden { AIR_REFILL / 2 } else { AIR_REFILL };
            self.air_used = self.air_used.saturating_sub(refill);
            return;
        }
        // Bad air takes a tick of air a tick whatever the swimmer carries;
        // under water each halving skips every other tick of the step clock,
        // which both sides count alike.
        let uses = short
            || match self.breath_halvings() {
                0 => true,
                1 => self.clock % 2 == 0,
                _ => false,
            };
        if uses {
            self.air_used = (self.air_used + 1).min(AIR_TICKS);
        }
        if self.air_used >= AIR_TICKS && period {
            let percent = if self.submerged { DROWN_PERCENT } else { CHOKE_PERCENT };
            let d = (self.max_health() * percent / 100).max(1);
            self.take(d);
        }
    }
}

impl PlayerState {
    /// What to tell a player arriving where their protection falls short,
    /// in plain words; nothing where they can breathe.
    pub fn air_warning(&self) -> Option<String> {
        let short = self.shortfall();
        if short == 0 {
            return None;
        }
        let line = self.air.line;
        let have = self.protection(line);
        let food = self.buff.air_points(line);
        // The fewest ward ranks that cover the hazard with the food eaten.
        let need = (0..=5u8).find(|&r| ward_points(r) + food >= self.air.hazard);
        let advice = match need {
            Some(r) => format!("You need {} {r} to breathe it", line.ward()),
            None => format!("Even {} 5 is not enough without food against it", line.ward()),
        };
        Some(format!(
            "The air here is {}. {advice}; your protection is {have} of {}.",
            line.name().trim_end_matches(" air"),
            self.air.hazard
        ))
    }
}

/// How far the fog lets you see: `normal` while you can breathe, closing in
/// toward [`FOG_AT_EMPTY`] as the air runs out while short (Boundless: the
/// fog rolls in right up to you).
pub fn fog_end(normal: f32, s: &PlayerState) -> f32 {
    // Only bad air brings the fog in; breath held under water does not.
    if s.shortfall() == 0 {
        return normal;
    }
    let left = s.air_left();
    let near = FOG_AT_EMPTY.min(normal);
    near + (normal - near) * left * left
}

/// Ticks between choking hits (the regen beat).
pub const CHOKE_PERIOD_TICKS: u16 = REGEN_PERIOD_TICKS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::food::{Buff, FoodBuff};
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

    fn body_on(air: Air) -> PlayerState {
        PlayerState {
            pos: DVec3::new(5.5, 10.0, 5.5),
            air,
            ..Default::default()
        }
    }

    fn run(s: &mut PlayerState, ticks: u32) {
        let shape = PlanetShape::new(4, 4);
        for _ in 0..ticks {
            step(s, &PlayerInput::default(), &MoveConfig::default(), None, shape, &Floor);
        }
    }

    #[test]
    fn the_fog_closes_in_as_the_air_runs_out() {
        let mut s = body_on(Air::of(AirLine::Acid, 4));
        assert_eq!(fog_end(200.0, &s), 200.0, "full air: normal fog");
        s.air_used = AIR_TICKS / 2;
        let half = fog_end(200.0, &s);
        assert!(half < 200.0 && half > FOG_AT_EMPTY, "{half}");
        s.air_used = AIR_TICKS;
        assert_eq!(fog_end(200.0, &s), FOG_AT_EMPTY);
        assert_eq!(fog_end(20.0, &s), 20.0, "never further than normal");
        let safe = body_on(Air::default());
        assert_eq!(fog_end(200.0, &safe), 200.0);
    }

    #[test]
    fn the_warning_says_what_ward_is_needed() {
        let mut s = body_on(Air::of(AirLine::Burning, 6));
        assert_eq!(
            s.air_warning().as_deref(),
            Some("The air here is burning. You need Fire Ward 4 to breathe it; your protection is 0 of 5.")
        );
        s.body.wards = [0, 0, ward_points(4)];
        assert_eq!(s.air_warning(), None);
        let mut t = body_on(Air::of(AirLine::Acid, 8));
        t.body.wards = [ward_points(5), 0, 0];
        assert!(t.air_warning().unwrap().contains("Even Acid Ward 5 is not enough"));
    }

    #[test]
    fn hazard_follows_the_tier_and_wards_follow_boundless() {
        assert_eq!(
            (1..=8).map(hazard_for_tier).collect::<Vec<_>>(),
            [0, 0, 0, 1, 3, 5, 7, 10]
        );
        assert_eq!((0..=5).map(ward_points).collect::<Vec<_>>(), [0, 1, 2, 3, 5, 7]);
        assert_eq!(Air::of(AirLine::Acid, 3), Air::default(), "tier 3 air is ordinary");
        assert_eq!(Air::of(AirLine::None, 6), Air::default());
        assert_eq!(Air::of(AirLine::Burning, 6).hazard, 5);
    }

    #[test]
    fn ordinary_air_never_runs_down() {
        let mut s = body_on(Air::default());
        assert_eq!(s.air_used, 0, "a new body has full air");
        run(&mut s, 3000);
        assert_eq!((s.air_used, s.hurt), (0, 0));
    }

    #[test]
    fn short_of_protection_the_air_runs_out_and_then_choking_hurts() {
        let mut s = body_on(Air::of(AirLine::Acid, 4));
        assert_eq!(s.shortfall(), 1);
        run(&mut s, u32::from(AIR_TICKS) - 1);
        assert!(s.air_used < AIR_TICKS && s.hurt == 0, "air lasts 30 s");
        run(&mut s, u32::from(CHOKE_PERIOD_TICKS) + 1);
        assert!(s.choking());
        let before = s.hurt;
        assert!(before > 0);
        run(&mut s, u32::from(CHOKE_PERIOD_TICKS));
        assert_eq!(
            s.hurt - before,
            s.max_health() * CHOKE_PERCENT / 100,
            "one beat at one short"
        );
        // Run to defeat: about 20 beats at one short.
        run(&mut s, 25 * u32::from(CHOKE_PERIOD_TICKS));
        assert!(s.down, "choking defeats");
        let kept = s.air_used;
        run(&mut s, 100);
        assert_eq!(s.air_used, kept, "a body that is down keeps its air");
        s.revive(100);
        assert_eq!(s.air_used, 0, "revived with full air");
    }

    #[test]
    fn any_shortfall_chokes_alike() {
        let mut one = body_on(Air::of(AirLine::Charged, 6));
        one.body.wards = [0, ward_points(4), 0];
        assert_eq!(one.shortfall(), 0, "four ranks (five points) meet tier 6");
        one.body.wards = [7, 3, 7];
        assert_eq!(one.shortfall(), 2, "only the line's own ward counts");
        one.air_used = AIR_TICKS;
        run(&mut one, u32::from(CHOKE_PERIOD_TICKS));
        let before = one.hurt;
        run(&mut one, u32::from(CHOKE_PERIOD_TICKS));
        assert_eq!(
            one.hurt - before,
            one.max_health() * CHOKE_PERCENT / 100,
            "two short chokes as one short"
        );
    }

    #[test]
    fn wards_and_food_together_protect_and_the_air_comes_back() {
        let mut s = body_on(Air::of(AirLine::Burning, 5));
        s.body.wards = [0, 0, 2];
        assert_eq!(s.shortfall(), 1);
        run(&mut s, 600);
        assert_eq!(s.air_used, 600);
        s.buff = FoodBuff {
            kind: Buff::BurningAir,
            rank: 1,
            left: 60 * 60,
        };
        assert_eq!(s.protection(AirLine::Burning), 3);
        assert_eq!(s.protection(AirLine::Acid), 0, "food covers its own line only");
        run(&mut s, 150);
        assert_eq!(s.air_used, 0, "back to full in a few seconds");
        assert_eq!(s.hurt, 0);
    }
}
