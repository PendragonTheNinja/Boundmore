//! Forging (milestone 17, `docs/milestones/17-forging.md`): traits on gear,
//! and the forge session that puts them there.
//!
//! Boundless's Centraforge, rule for rule, from the game's data as the
//! community published it (mayumi's forge simulator: `forge.json`,
//! `ingredientConfig.js`, `ingredientScripts/*.js`, `main.js`; tuning 5.7).
//! Boundmore's names are its own; the numbers are Boundless's. A session
//! follows the simulator's round in the same order (`Session::use_ingredient`)
//! so a rule can be checked against it line by line.
//!
//! Pure data and rules: the server runs sessions (it alone rolls the dice,
//! with a generator saved in the session); server and client both read gear
//! stats through [`Forged`] so prediction stays exact.

use crate::items::{Aoe, BombStats, ItemDef, ItemKind, ItemStack, SlingStats, ToolClass, ToolStats};
use crate::movement::GrappleSpec;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- traits

/// Most traits of one kind an item carries (Boundless: 3 boons, 3 quirks,
/// 3 defects).
pub const TRAITS_PER_KIND: usize = 3;
/// Slots in [`Forged`]: three of each kind.
pub const MAX_TRAITS: usize = 9;
/// A pool never holds more than this (`poolPointsMax`).
pub const POOL_MAX: i32 = 1000;
/// Weight a glaze adds to each trait it leans to (the simulator's
/// `gumWeight`).
pub const GLAZE_WEIGHT: u32 = 10;
/// Effectiveness never exceeds this (players: "a cap of 200%").
pub const MAX_EFFECTIVENESS: f64 = 2.0;
/// Session Stability and Vigour never exceed this (tuning 5.7).
pub const MAX_STAT: i32 = 1800;
/// Deck slots, and with Forgemaster (the forging epic).
pub const DECK_SLOTS: usize = 6;
pub const DECK_SLOTS_EPIC: usize = 10;
/// Most of one ingredient a deck slot holds.
pub const DECK_STACK: u16 = 100;
/// Flexibility Forgemaster adds, and the most any item has.
pub const EPIC_FLEX: u16 = 25;
pub const MAX_FLEX: u16 = 150;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TraitKind {
    Boon,
    Quirk,
    Flaw,
}

impl TraitKind {
    pub const ALL: [TraitKind; 3] = [TraitKind::Boon, TraitKind::Quirk, TraitKind::Flaw];

    fn index(self) -> usize {
        match self {
            TraitKind::Boon => 0,
            TraitKind::Quirk => 1,
            TraitKind::Flaw => 2,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            TraitKind::Boon => "boon",
            TraitKind::Quirk => "quirk",
            TraitKind::Flaw => "flaw",
        }
    }
}

/// A boon's tag, which glazes lean the dice to (Boundless: Ease, Lightness,
/// Special, Effect, Longevity, Glow, Venerable).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Leaning {
    Thrift,
    Swift,
    Odd,
    Force,
    Lasting,
    Glow,
    Elder,
    None,
}

/// What kind of gear an item is, for the forge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Gear {
    Tool,
    Sling,
    Bomb,
    Grapple,
    Spanner,
    Chisel,
}

impl Gear {
    fn bit(self) -> u8 {
        match self {
            Gear::Tool => 1,
            Gear::Sling => 2,
            Gear::Bomb => 4,
            Gear::Grapple => 8,
            Gear::Spanner => 16,
            Gear::Chisel => 32,
        }
    }

    /// The forge's kind of gear for an item, if it can be forged at all
    /// (hammers, shovels, axes, slings, bombs, grapples, spanners, chisels;
    /// not the hand stone or tillers).
    pub fn of(def: &ItemDef) -> Option<Gear> {
        if def.flex == 0 {
            return None;
        }
        match &def.kind {
            ItemKind::Tool(t) => match t.class {
                Some(ToolClass::Hammer | ToolClass::Shovel | ToolClass::Axe) => Some(Gear::Tool),
                Some(ToolClass::Spanner) => Some(Gear::Spanner),
                Some(ToolClass::Tiller) | None => None,
            },
            ItemKind::Sling(_) => Some(Gear::Sling),
            ItemKind::Bomb(_) => Some(Gear::Bomb),
            ItemKind::Grapple { .. } => Some(Gear::Grapple),
            ItemKind::Chisel { .. } => Some(Gear::Chisel),
            _ => None,
        }
    }
}

const ALL: u8 = 63;
const TOOL: u8 = 1;
const SLING: u8 = 2;
const BOMB: u8 = 4;
const GRAPPLE: u8 = 8;
const SPANNER: u8 = 16;
const CHISEL: u8 = 32;

/// Cumulative points for each level.
pub const SCALAR_LEVELS: &[u32] = &[100, 150, 300, 600, 1000, 1500, 2000, 3000, 4000, 5000];
pub const FIVE_LEVELS: &[u32] = &[100, 300, 1000, 2000, 4000];
pub const THREE_LEVELS: &[u32] = &[300, 1000, 2000];
pub const ONE_LEVEL: &[u32] = &[1000];
pub const QUIRK_LEVEL: &[u32] = &[600];

/// Every trait. The numbers are wire and session ids: append only, never
/// reorder (saved stacks use [`TraitDef::name`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum TraitId {
    Thrifty = 1,
    Tiring,
    Quick,
    Sluggish,
    Lucky,
    Unlucky,
    Fierce,
    Mild,
    Mighty,
    Feeble,
    Sturdy,
    Brittle,
    Pushing,
    Soft,
    WideBlast,
    SmallBlast,
    FastShot,
    SlowShot,
    QuickReel,
    SlowReel,
    LongRope,
    ShortRope,
    FarShot,
    ShortShot,
    Mender,
    PoorMender,
    Glowing,
    WideSwing,
    ManyShots,
    HeavyShot,
    LightShot,
    Hookshot,
    NightOwl,
    EarlyBird,
    Bold,
    Desperate,
    Sated,
    Ravenous,
    Loud,
    // Milestone 17 part 2: the body quirks.
    Slippery,
    Bouncing,
    Leaden,
    Sprinter,
    Jumpy,
    Twitchy,
    Gnawing,
    Draining,
}

pub struct TraitDef {
    pub id: TraitId,
    /// Saved name (`iron_hammer!mighty5`).
    pub name: &'static str,
    /// What players see.
    pub display: &'static str,
    pub kind: TraitKind,
    pub leaning: Leaning,
    gear: u8,
    pub levels: &'static [u32],
    /// Traits it cannot share an item with.
    pub excludes: &'static [TraitId],
    /// Item rank a level adds (boons; flaws and quirks scale it).
    rank: &'static [i32],
}

const R10: &[i32] = &[10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
const R5: &[i32] = &[10, 30, 50, 70, 90];
const R3: &[i32] = &[30, 50, 70];
/// A quirk's rank before Boundless's -0.3 (**guess**).
const RQ: &[i32] = &[40];

macro_rules! t {
    ($id:ident, $name:literal, $display:literal, $kind:ident, $lean:ident, $gear:expr, $levels:expr, $rank:expr, [$($ex:ident),*]) => {
        TraitDef {
            id: TraitId::$id,
            name: $name,
            display: $display,
            kind: TraitKind::$kind,
            leaning: Leaning::$lean,
            gear: $gear,
            levels: $levels,
            rank: $rank,
            excludes: &[$(TraitId::$ex),*],
        }
    };
}

/// Boundless's trait table (`forge.json`), with the traits whose systems
/// exist in Boundmore (milestone 17 part 1), in [`TraitId`] order.
#[rustfmt::skip]
pub static TRAITS: &[TraitDef] = &[
    t!(Thrifty, "thrifty", "Thrifty", Boon, Thrift, ALL, SCALAR_LEVELS, R10, [Tiring]),
    t!(Tiring, "tiring", "Tiring", Flaw, Thrift, ALL, SCALAR_LEVELS, R10, [Thrifty]),
    t!(Quick, "quick", "Quick", Boon, Swift, TOOL | SLING | BOMB | SPANNER, SCALAR_LEVELS, R10, [Sluggish]),
    t!(Sluggish, "sluggish", "Sluggish", Flaw, Swift, TOOL | SLING | BOMB | SPANNER, SCALAR_LEVELS, R10, [Quick]),
    t!(Lucky, "lucky", "Lucky", Boon, Thrift, TOOL | SLING, SCALAR_LEVELS, R10, [Unlucky]),
    t!(Unlucky, "unlucky", "Unlucky", Flaw, Thrift, TOOL | SLING, SCALAR_LEVELS, R10, [Lucky]),
    t!(Fierce, "fierce", "Fierce", Boon, Force, TOOL | SLING, SCALAR_LEVELS, R10, [Mild]),
    t!(Mild, "mild", "Mild", Flaw, Force, TOOL | SLING, SCALAR_LEVELS, R10, [Fierce]),
    t!(Mighty, "mighty", "Mighty", Boon, Force, TOOL | SLING | BOMB, SCALAR_LEVELS, R10, [Feeble]),
    t!(Feeble, "feeble", "Feeble", Flaw, Force, TOOL | SLING | BOMB, SCALAR_LEVELS, R10, [Mighty]),
    t!(Sturdy, "sturdy", "Sturdy", Boon, Lasting, TOOL | SLING | GRAPPLE | SPANNER | CHISEL, SCALAR_LEVELS, R10, [Brittle]),
    t!(Brittle, "brittle", "Brittle", Flaw, Lasting, TOOL | SLING | GRAPPLE | SPANNER | CHISEL, SCALAR_LEVELS, R10, [Sturdy]),
    t!(Pushing, "pushing", "Pushing", Boon, Force, BOMB, SCALAR_LEVELS, R10, [Soft]),
    t!(Soft, "soft", "Soft", Flaw, Force, BOMB, SCALAR_LEVELS, R10, [Pushing]),
    t!(WideBlast, "wide_blast", "Wide Blast", Boon, Lasting, BOMB, SCALAR_LEVELS, R10, [SmallBlast]),
    t!(SmallBlast, "small_blast", "Small Blast", Flaw, Lasting, BOMB, SCALAR_LEVELS, R10, [WideBlast]),
    t!(FastShot, "fast_shot", "Fast Shot", Boon, Swift, SLING | GRAPPLE, SCALAR_LEVELS, R10, [SlowShot]),
    t!(SlowShot, "slow_shot", "Slow Shot", Flaw, Swift, SLING | GRAPPLE, SCALAR_LEVELS, R10, [FastShot]),
    t!(QuickReel, "quick_reel", "Quick Reel", Boon, Force, GRAPPLE, SCALAR_LEVELS, R10, [SlowReel]),
    t!(SlowReel, "slow_reel", "Slow Reel", Flaw, Force, GRAPPLE, SCALAR_LEVELS, R10, [QuickReel]),
    t!(LongRope, "long_rope", "Long Rope", Boon, Lasting, GRAPPLE, SCALAR_LEVELS, R10, [ShortRope]),
    t!(ShortRope, "short_rope", "Short Rope", Flaw, Lasting, GRAPPLE, SCALAR_LEVELS, R10, [LongRope]),
    t!(FarShot, "far_shot", "Far Shot", Boon, Lasting, SLING, SCALAR_LEVELS, R10, [ShortShot]),
    t!(ShortShot, "short_shot", "Short Shot", Flaw, Lasting, SLING, SCALAR_LEVELS, R10, [FarShot]),
    t!(Mender, "mender", "Mender", Boon, Force, SPANNER, SCALAR_LEVELS, R10, [PoorMender]),
    t!(PoorMender, "poor_mender", "Poor Mender", Flaw, Force, SPANNER, SCALAR_LEVELS, R10, [Mender]),
    t!(Glowing, "glowing", "Glowing", Boon, Glow, ALL, THREE_LEVELS, R3, []),
    t!(WideSwing, "wide_swing", "Wide Swing", Boon, Odd, TOOL, FIVE_LEVELS, R5, []),
    t!(ManyShots, "many_shots", "Many Shots", Boon, Odd, SLING, FIVE_LEVELS, R5, []),
    t!(HeavyShot, "heavy_shot", "Heavy Shot", Boon, Force, SLING, FIVE_LEVELS, R5, [LightShot]),
    t!(LightShot, "light_shot", "Light Shot", Boon, Swift, SLING, FIVE_LEVELS, R5, [HeavyShot]),
    t!(Hookshot, "hookshot", "Hookshot", Boon, Odd, GRAPPLE, ONE_LEVEL, &[20], []),
    t!(NightOwl, "night_owl", "Night Owl", Quirk, None, ALL, QUIRK_LEVEL, RQ, [EarlyBird]),
    t!(EarlyBird, "early_bird", "Early Bird", Quirk, None, ALL, QUIRK_LEVEL, RQ, [NightOwl]),
    t!(Bold, "bold", "Bold", Quirk, None, ALL, QUIRK_LEVEL, RQ, [Desperate]),
    t!(Desperate, "desperate", "Desperate", Quirk, None, ALL, QUIRK_LEVEL, RQ, [Bold]),
    t!(Sated, "sated", "Sated", Quirk, None, ALL, QUIRK_LEVEL, RQ, [Ravenous]),
    t!(Ravenous, "ravenous", "Ravenous", Quirk, None, ALL, QUIRK_LEVEL, RQ, [Sated]),
    t!(Loud, "loud", "Loud", Quirk, None, ALL, QUIRK_LEVEL, RQ, []),
    t!(Slippery, "slippery", "Slippery", Quirk, None, ALL, QUIRK_LEVEL, RQ, [Bouncing]),
    t!(Bouncing, "bouncing", "Bouncing", Quirk, None, ALL, QUIRK_LEVEL, RQ, [Slippery]),
    t!(Leaden, "leaden", "Leaden", Quirk, None, ALL, QUIRK_LEVEL, RQ, []),
    t!(Sprinter, "sprinter", "Sprinter", Quirk, None, ALL, QUIRK_LEVEL, RQ, []),
    t!(Jumpy, "jumpy", "Jumpy", Quirk, None, ALL, QUIRK_LEVEL, RQ, []),
    t!(Twitchy, "twitchy", "Twitchy", Quirk, None, ALL, QUIRK_LEVEL, RQ, []),
    t!(Gnawing, "gnawing", "Gnawing", Quirk, None, ALL, QUIRK_LEVEL, RQ, []),
    t!(Draining, "draining", "Draining", Quirk, None, ALL, QUIRK_LEVEL, RQ, []),
];

impl TraitId {
    pub fn def(self) -> &'static TraitDef {
        &TRAITS[self as usize - 1]
    }

    pub fn from_u8(n: u8) -> Option<TraitId> {
        TRAITS.get((n as usize).checked_sub(1)?).map(|d| d.id)
    }

    pub fn by_name(name: &str) -> Option<TraitId> {
        TRAITS.iter().find(|d| d.name == name).map(|d| d.id)
    }
}

impl TraitDef {
    pub fn fits(&self, gear: Gear) -> bool {
        self.gear & gear.bit() != 0
    }

    /// Points for its top level.
    pub fn max_points(&self) -> u32 {
        *self.levels.last().expect("every trait has a level")
    }

    /// The level `points` reach (0: none yet).
    pub fn level_of(&self, points: u32) -> u8 {
        self.levels.iter().take_while(|&&t| points >= t).count() as u8
    }

    /// The item rank this trait at `level` adds (Boundless's multipliers:
    /// a defect counts -0.5, a quirk -0.3 of a boon's).
    pub fn rank(&self, level: u8) -> i32 {
        let Some(&r) = self.rank.get(level.max(1) as usize - 1) else {
            return 0;
        };
        match self.kind {
            TraitKind::Boon => r,
            TraitKind::Flaw => -r / 2,
            TraitKind::Quirk => -(r * 3) / 10,
        }
    }

    /// What it does at `level`, in plain words.
    pub fn describe(&self, level: u8) -> String {
        let l = level as i32;
        let half = (l + 1) / 2 * 10;
        match self.id {
            TraitId::Thrifty => format!("{} less energy a use", 2 * l),
            TraitId::Tiring => format!("{} more energy a use", 2 * l),
            TraitId::Quick => format!("+{} action speed", 5 * l),
            TraitId::Sluggish => format!("-{} action speed", 5 * l),
            TraitId::Lucky => format!("+{}% crit chance", 5 * l),
            TraitId::Unlucky => format!("-{}% crit chance", 5 * l),
            TraitId::Fierce => format!("+{}% crit damage", 10 * l),
            TraitId::Mild => format!("-{}% crit damage", 10 * l),
            TraitId::Mighty => format!("+{} damage and +{half}%", 50 * l),
            TraitId::Feeble => format!("-{} damage and -{half}%", 50 * l),
            TraitId::Sturdy => format!("+{} durability", 200 * l),
            TraitId::Brittle => format!("-{} durability", 200 * l),
            TraitId::Pushing => format!("+{} knockback", 2 * l),
            TraitId::Soft => format!("-{} knockback", 2 * l),
            TraitId::WideBlast => format!("+{:.1} blast radius", 0.3 * l as f64),
            TraitId::SmallBlast => format!("-{:.1} blast radius", 0.3 * l as f64),
            TraitId::FastShot => format!("+{} shot speed", 5 * l),
            TraitId::SlowShot => format!("-{} shot speed", 5 * l),
            TraitId::QuickReel => format!("+{} reel speed", 5 * l),
            TraitId::SlowReel => format!("-{} reel speed", 5 * l),
            TraitId::LongRope => format!("+{} m rope", 3 * l),
            TraitId::ShortRope => format!("-{} m rope", 3 * l),
            TraitId::FarShot => format!("+{} m range", 3 * l),
            TraitId::ShortShot => format!("-{} m range", 3 * l),
            TraitId::Mender => format!("+{} wear mended a hit and +{half}%", 100 * l),
            TraitId::PoorMender => format!("-{} wear mended a hit and -{half}%", 100 * l),
            TraitId::Glowing => format!("glows (light {})", glow_light(level)),
            TraitId::WideSwing => match aoe_of(level) {
                Some(Aoe::ThreeByOne) => "hits a row of three, 40% each".into(),
                Some(Aoe::OneByTwo) => "hits the block below too, 60% each".into(),
                Some(Aoe::Cross) => "hits a cross of five, 40% each".into(),
                Some(Aoe::ThreeByThree) => "hits a square of nine, 40% each".into(),
                Some(Aoe::Random) => "hits 1 to 4 neighbours too, full damage".into(),
                None => String::new(),
            },
            TraitId::ManyShots => {
                let (n, d) = many_shots(level);
                format!("{n} shots at once, {:.0}% damage each", d * 100.0)
            }
            TraitId::HeavyShot => format!("+{}% damage, slower and dropping shots", 10 * l),
            TraitId::LightShot => format!("+{} shot speed, flatter shots", light_speed(level)),
            TraitId::Hookshot => "reels itself in, +20 reel speed".into(),
            TraitId::NightOwl => "+100 and +10% damage at night, less by day".into(),
            TraitId::EarlyBird => "+100 and +10% damage by day, less at night".into(),
            TraitId::Bold => "+10 action speed at 55% health or more".into(),
            TraitId::Desperate => "+10 action speed at 45% health or less".into(),
            TraitId::Sated => "10 less energy a use while you are fed".into(),
            TraitId::Ravenous => "10 less energy a use while you are hungry".into(),
            TraitId::Loud => "creatures notice you from further away".into(),
            TraitId::Slippery => "slippery footing while held; +40 falling armour".into(),
            TraitId::Bouncing => "hard landings bounce you back up".into(),
            TraitId::Leaden => "you sink in water".into(),
            TraitId::Sprinter => "walk 25% slower, sprint bonus 50% bigger".into(),
            TraitId::Jumpy => "jumps by itself now and then; gives a double jump".into(),
            TraitId::Twitchy => "goes off by itself now and then; +400 durability".into(),
            TraitId::Gnawing => "makes you hungry fast; +30% max energy".into(),
            TraitId::Draining => "slowly drains health; +30% max health".into(),
        }
    }
}

/// Held light for a Glowing level (**guess**: Boundless's radius 6, 9, 12).
pub fn glow_light(level: u8) -> u8 {
    match level {
        0 => 0,
        1 => 8,
        2 => 11,
        _ => 14,
    }
}

/// Wide Swing's shape at `level` (Boundless's Effect Adjacent, tuning 5.8).
pub fn aoe_of(level: u8) -> Option<Aoe> {
    match level {
        0 => None,
        1 => Some(Aoe::ThreeByOne),
        2 => Some(Aoe::OneByTwo),
        3 => Some(Aoe::Cross),
        4 => Some(Aoe::ThreeByThree),
        _ => Some(Aoe::Random),
    }
}

/// Many Shots: shots at once and the damage each does.
pub fn many_shots(level: u8) -> (u32, f64) {
    match level {
        0 => (1, 1.0),
        1 => (2, 0.5),
        l => (1 + l.min(5) as u32, 0.4),
    }
}

/// Light Shot's extra shot speed.
fn light_speed(level: u8) -> u32 {
    [0, 10, 12, 15, 20, 30][level.min(5) as usize]
}

/// Many Shots' spread between shots, radians.
pub const MANY_SHOTS_SPREAD: f64 = 0.15;

// ---------------------------------------------------------------- the body

/// The quirks of the item in hand that change the body (milestone 17 part
/// 2), carried in `PlayerState::quirks` so the movement step and the bars
/// read them alike on server and client.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Quirks(pub u16);

impl Quirks {
    pub const SLIPPERY: u16 = 1;
    pub const BOUNCING: u16 = 2;
    pub const LEADEN: u16 = 4;
    pub const SPRINTER: u16 = 8;
    pub const JUMPY: u16 = 16;
    pub const TWITCHY: u16 = 32;
    pub const GNAWING: u16 = 64;
    pub const DRAINING: u16 = 128;

    pub fn has(self, flag: u16) -> bool {
        self.0 & flag != 0
    }

    /// The body quirks of a held stack (none for an empty hand).
    pub fn of(stack: Option<&ItemStack>) -> Quirks {
        let Some(s) = stack else { return Quirks(0) };
        let mut q = 0;
        for (id, _) in s.forge.traits() {
            q |= match id {
                TraitId::Slippery => Self::SLIPPERY,
                TraitId::Bouncing => Self::BOUNCING,
                TraitId::Leaden => Self::LEADEN,
                TraitId::Sprinter => Self::SPRINTER,
                TraitId::Jumpy => Self::JUMPY,
                TraitId::Twitchy => Self::TWITCHY,
                TraitId::Gnawing => Self::GNAWING,
                TraitId::Draining => Self::DRAINING,
                _ => 0,
            };
        }
        Quirks(q)
    }

    /// A number in 0..1 from the step clock and a salt: the random quirks'
    /// dice, the same on both sides.
    pub fn roll(clock: u32, salt: u32) -> f64 {
        let mut d = Dice(((clock as u64) << 8) ^ salt as u64 ^ 0x5EED_F0A6_E000_0000);
        d.roll()
    }
}

/// Max energy and max health with Gnawing or Draining in hand (Release
/// 209: +30%).
pub const DRAIN_MAX_PERCENT: u32 = 30;
/// Gnawing's hunger, in food hundredths a tick: as fast as sprinting
/// (**guess**; players found it fast).
pub const GNAWING_HUNGER_PER_TICK: u32 = crate::food::SPRINT_HUNGER_PER_TICK;
/// Draining's health lost every regeneration beat (5 s): a little under
/// what being Full gives back (**guess**; players: "all you need is Full
/// Up").
pub const DRAINING_PER_BEAT: u32 = 5;
/// Slippery's falling armour (Release 209).
pub const SLIPPERY_FALLING_ARMOUR: u32 = 40;

impl crate::movement::PlayerState {
    /// Gnawing and Draining, once a tick (`beat`: the 5 s regeneration
    /// beat). Draining never resets the calm that lets being Full heal.
    pub(crate) fn quirk_tick(&mut self, beat: bool) {
        if self.down {
            return;
        }
        if self.quirks.has(Quirks::GNAWING) {
            self.add_hunger(GNAWING_HUNGER_PER_TICK);
        }
        if beat && self.quirks.has(Quirks::DRAINING) {
            let max = self.max_health();
            self.hurt = (self.hurt + DRAINING_PER_BEAT).min(max);
            if self.hurt >= max {
                self.down = true;
                self.hook = None;
            }
        }
    }
}

// ---------------------------------------------------------------- on items

/// One trait on an item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraitLevel {
    /// [`TraitId`] as a number; 0: an empty slot.
    pub id: u8,
    pub level: u8,
}

/// What forging did to an item (part of [`ItemStack`]): whether it has been
/// forged (an item forges once) and its traits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Forged {
    pub done: bool,
    pub traits: [TraitLevel; MAX_TRAITS],
}

/// Conditions some quirks need, as the holder's side sees them. `None`
/// leaves a condition unmet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Held {
    /// Whether it is night where the holder is.
    pub night: Option<bool>,
    /// Health as a fraction of max.
    pub health: Option<f64>,
    /// Hunger as a fraction of the energy bar.
    pub hunger: Option<f64>,
}

impl Held {
    /// The holder's conditions, judged the same way on server and client:
    /// night when the sun is below the horizon at `tick`, health and hunger
    /// as fractions.
    pub fn of(state: &crate::movement::PlayerState, day: &crate::time::DayCycle, tick: f64) -> Held {
        let full = state.full_energy().max(1) as f64;
        Held {
            night: Some(day.sun_direction(tick)[1] < 0.0),
            health: Some(state.health() as f64 / state.max_health().max(1) as f64),
            hunger: Some(state.hunger_points() as f64 / full),
        }
    }
}

impl Forged {
    pub fn is_plain(&self) -> bool {
        *self == Forged::default()
    }

    pub fn traits(&self) -> impl Iterator<Item = (TraitId, u8)> + '_ {
        self.traits
            .iter()
            .filter_map(|t| TraitId::from_u8(t.id).map(|id| (id, t.level)))
    }

    pub fn level(&self, id: TraitId) -> u8 {
        self.traits().find(|&(t, _)| t == id).map_or(0, |(_, l)| l)
    }

    fn has(&self, id: TraitId) -> bool {
        self.level(id) > 0
    }

    /// Traits as a forge session finishes them: those with at least one
    /// level, boons first, then quirks, then flaws, each in the order they
    /// landed.
    pub fn from_session(traits: &[Vec<(TraitId, u32)>; 3]) -> Forged {
        let mut f = Forged {
            done: true,
            ..Default::default()
        };
        let mut i = 0;
        for kind in traits {
            for &(id, points) in kind {
                let level = id.def().level_of(points);
                if level > 0 && i < MAX_TRAITS {
                    f.traits[i] = TraitLevel { id: id as u8, level };
                    i += 1;
                }
            }
        }
        f
    }

    /// The saved suffix after `!`: `mighty5,quick3` (empty for forged with
    /// no traits).
    pub fn to_suffix(&self) -> String {
        self.traits()
            .map(|(id, l)| format!("{}{l}", id.def().name))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Reads [`Forged::to_suffix`]. A trait no longer known is dropped.
    pub fn from_suffix(s: &str) -> Forged {
        let mut f = Forged {
            done: true,
            ..Default::default()
        };
        let mut i = 0;
        for part in s.split(',').filter(|p| !p.is_empty()) {
            let split = part.find(|c: char| c.is_ascii_digit()).unwrap_or(part.len());
            let (name, level) = part.split_at(split);
            let (Some(id), Ok(level)) = (TraitId::by_name(name), level.parse::<u8>()) else {
                continue;
            };
            let level = level.min(id.def().levels.len() as u8);
            if level > 0 && i < MAX_TRAITS {
                f.traits[i] = TraitLevel { id: id as u8, level };
                i += 1;
            }
        }
        f
    }

    /// The item's rank: its catalogue rank plus its traits'.
    pub fn rank(&self, base: u16) -> i32 {
        base as i32 + self.traits().map(|(id, l)| id.def().rank(l)).sum::<i32>()
    }

    fn scalar(&self, boon: TraitId, flaw: TraitId) -> i32 {
        self.level(boon) as i32 - self.level(flaw) as i32
    }

    /// Mighty and Feeble on a damage: +50 a level and +10% of the base every
    /// two levels (Boundless: ceil(L / 2) x 10%).
    fn damage(&self, base: u32) -> f64 {
        let up = self.level(TraitId::Mighty) as f64;
        let down = self.level(TraitId::Feeble) as f64;
        let pct = ((up / 2.0).ceil() - (down / 2.0).ceil()) * 0.1;
        base as f64 * (1.0 + pct) + 50.0 * (up - down)
    }

    /// Night Owl and Early Bird: +100 and +10% in their half of the day,
    /// -100 and -10% in the other (Release 209).
    fn time_damage(&self, base: u32, held: &Held) -> f64 {
        let Some(night) = held.night else { return 0.0 };
        let swing = 100.0 + 0.1 * base as f64;
        let mut d = 0.0;
        if self.has(TraitId::NightOwl) {
            d += if night { swing } else { -swing };
        }
        if self.has(TraitId::EarlyBird) {
            d += if night { -swing } else { swing };
        }
        d
    }

    fn speed_quirks(&self, held: &Held) -> f32 {
        let mut s = 0.0;
        if let Some(h) = held.health {
            if self.has(TraitId::Bold) && h >= 0.55 {
                s += 10.0;
            }
            if self.has(TraitId::Desperate) && h <= 0.45 {
                s += 10.0;
            }
        }
        s
    }

    /// Energy a use after Thrifty, Tiring, Sated and Ravenous.
    pub fn energy(&self, base: u16, held: &Held) -> u16 {
        let mut e = base as i32 - 2 * self.scalar(TraitId::Thrifty, TraitId::Tiring);
        if let Some(h) = held.hunger {
            if self.has(TraitId::Sated) && h < 0.45 {
                e -= 10;
            }
            if self.has(TraitId::Ravenous) && h > 0.55 {
                e -= 10;
            }
        }
        e.clamp(0, u16::MAX as i32) as u16
    }

    /// Durability after Sturdy and Brittle.
    pub fn durability(&self, base: u16) -> u16 {
        let twitchy = if self.has(TraitId::Twitchy) { 400 } else { 0 };
        (base as i32 + 200 * self.scalar(TraitId::Sturdy, TraitId::Brittle) + twitchy).clamp(1, u16::MAX as i32) as u16
    }

    /// Light held in the hand.
    pub fn held_light(&self, base: u8) -> u8 {
        base.max(glow_light(self.level(TraitId::Glowing)))
    }

    /// How far creatures notice the holder, as a multiple (Loud: **guess**).
    pub fn notice_scale(&self) -> f64 {
        if self.has(TraitId::Loud) { 1.5 } else { 1.0 }
    }

    pub fn tool(&self, base: ToolStats, held: &Held) -> ToolStats {
        if self.is_plain() {
            return base;
        }
        let mut t = base;
        if base.damage > 0 {
            t.damage = (self.damage(base.damage) + self.time_damage(base.damage, held)).max(1.0) as u32;
        }
        t.action_speed =
            (base.action_speed + 5.0 * self.scalar(TraitId::Quick, TraitId::Sluggish) as f32 + self.speed_quirks(held))
                .max(1.0);
        t.crit_chance =
            (base.crit_chance + 0.05 * self.scalar(TraitId::Lucky, TraitId::Unlucky) as f32).clamp(0.0, 1.0);
        t.crit_effect = (base.crit_effect + 0.1 * self.scalar(TraitId::Fierce, TraitId::Mild) as f32).max(0.0);
        t.durability = self.durability(base.durability);
        t.energy = self.energy(base.energy, held);
        if let Some(aoe) = aoe_of(self.level(TraitId::WideSwing)) {
            t.aoe = Some(aoe);
        }
        if base.repair > 0 {
            let up = self.level(TraitId::Mender) as f64;
            let down = self.level(TraitId::PoorMender) as f64;
            let pct = ((up / 2.0).ceil() - (down / 2.0).ceil()) * 0.1;
            t.repair = (base.repair as f64 * (1.0 + pct) + 100.0 * (up - down)).max(1.0) as u32;
        }
        t
    }

    pub fn sling(&self, base: SlingStats, held: &Held) -> SlingStats {
        if self.is_plain() {
            return base;
        }
        let mut s = base;
        let heavy = self.level(TraitId::HeavyShot);
        let light = self.level(TraitId::LightShot);
        let mut damage = self.damage(base.damage) + self.time_damage(base.damage, held);
        damage += base.damage as f64 * 0.1 * heavy as f64;
        s.damage = damage.max(1.0) as u32;
        s.action_speed =
            (base.action_speed + 5.0 * self.scalar(TraitId::Quick, TraitId::Sluggish) as f32 + self.speed_quirks(held))
                .max(1.0);
        let mut speed = base.shot_speed + 5.0 * self.scalar(TraitId::FastShot, TraitId::SlowShot) as f32;
        if heavy > 0 {
            speed -= 10.0;
        }
        speed += light_speed(light) as f32;
        s.shot_speed = speed.max(1.0);
        s.range = (base.range + 3.0 * self.scalar(TraitId::FarShot, TraitId::ShortShot) as f32).max(1.0);
        s.falloff = (base.falloff + 0.1 * heavy as f32) * (1.0 - 0.1 * light as f32);
        s.crit_chance =
            (base.crit_chance + 0.05 * self.scalar(TraitId::Lucky, TraitId::Unlucky) as f32).clamp(0.0, 1.0);
        s.crit_effect = (base.crit_effect + 0.1 * self.scalar(TraitId::Fierce, TraitId::Mild) as f32).max(0.0);
        s.durability = self.durability(base.durability);
        s.energy = self.energy(base.energy, held);
        s
    }

    pub fn bomb(&self, base: BombStats, held: &Held) -> BombStats {
        if self.is_plain() {
            return base;
        }
        let mut b = base;
        b.damage = (self.damage(base.damage) + self.time_damage(base.damage, held)).max(1.0) as u32;
        b.radius = (base.radius + 0.3 * self.scalar(TraitId::WideBlast, TraitId::SmallBlast) as f32).max(0.5);
        b.knockback = (base.knockback + 2.0 * self.scalar(TraitId::Pushing, TraitId::Soft) as f32).max(0.0);
        b.action_speed =
            (base.action_speed + 5.0 * self.scalar(TraitId::Quick, TraitId::Sluggish) as f32 + self.speed_quirks(held))
                .max(1.0);
        b.energy = self.energy(base.energy, held);
        b
    }

    /// A grapple's spec; the arithmetic is the same on server and client.
    pub fn grapple(&self, base: GrappleSpec, held: &Held) -> GrappleSpec {
        if self.is_plain() {
            return base;
        }
        let mut g = base;
        g.range = (base.range + 3.0 * self.scalar(TraitId::LongRope, TraitId::ShortRope) as f64).max(1.0);
        g.shot_speed = (base.shot_speed + 5.0 * self.scalar(TraitId::FastShot, TraitId::SlowShot) as f64).max(1.0);
        let reel = 5.0 * self.scalar(TraitId::QuickReel, TraitId::SlowReel) as f64;
        g.reel_in_speed = (base.reel_in_speed + reel).max(1.0);
        g.reel_out_speed = (base.reel_out_speed + reel).max(1.0);
        if self.has(TraitId::Hookshot) {
            g.hookshot = true;
            g.reel_in_speed += 20.0;
        }
        g.energy = self.energy(base.energy, held);
        g
    }
}

/// Stats of a stack: its item's catalogue stats through its traits. Every
/// place that reads gear stats for a held stack goes through these.
impl ItemDef {
    pub fn tool_of(&self, stack: &ItemStack, held: &Held) -> Option<ToolStats> {
        self.tool().map(|t| stack.forge.tool(t, held))
    }

    pub fn sling_of(&self, stack: &ItemStack, held: &Held) -> Option<SlingStats> {
        self.sling().map(|s| stack.forge.sling(s, held))
    }

    pub fn bomb_of(&self, stack: &ItemStack, held: &Held) -> Option<BombStats> {
        self.bomb().map(|b| stack.forge.bomb(b, held))
    }

    pub fn grapple_of(&self, stack: &ItemStack, held: &Held) -> Option<GrappleSpec> {
        self.grapple().map(|g| stack.forge.grapple(g, held))
    }

    pub fn energy_of(&self, stack: &ItemStack, held: &Held) -> Option<u16> {
        self.energy().map(|e| stack.forge.energy(e, held))
    }

    pub fn durability_of(&self, stack: &ItemStack) -> Option<u16> {
        match self.kind {
            // Bombs carry no Sturdy (Boundless's table).
            ItemKind::Bomb(_) => self.durability(),
            _ => self.durability().map(|d| stack.forge.durability(d)),
        }
    }

    pub fn held_light_of(&self, stack: &ItemStack) -> u8 {
        stack.forge.held_light(self.held_light)
    }
}

// ---------------------------------------------------------------- the forger

/// A session's fixed stats, from the forger and the gear.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionStats {
    pub stability: i32,
    pub vigour: i32,
    pub effectiveness: f64,
}

/// A forger's attribute ranks (each counting Wholeness's fifth of a rank).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Forger {
    pub vitality: f64,
    pub power: f64,
    pub insight: f64,
    /// Forgemaster, the forging epic.
    pub epic: bool,
}

impl Forger {
    /// Stability: 900 + 100 a Vitality rank (wiki Vitality Bonus table).
    pub fn stability(&self) -> i32 {
        ((900.0 + 100.0 * self.vitality).round() as i32).min(MAX_STAT)
    }

    /// Vigour: 200 a Power rank (game skill text).
    pub fn vigour(&self) -> i32 {
        ((200.0 * self.power).round() as i32).min(MAX_STAT)
    }

    /// Intelligence: 10 + 10 an Insight rank (wiki Intelligence Bonus table).
    pub fn intelligence(&self) -> f64 {
        10.0 + 10.0 * self.insight
    }

    pub fn deck_slots(&self) -> usize {
        if self.epic { DECK_SLOTS_EPIC } else { DECK_SLOTS }
    }

    /// The item's Flexibility as this forger works it.
    pub fn flexibility(&self, item_flex: u16) -> u16 {
        (item_flex + if self.epic { EPIC_FLEX } else { 0 }).min(MAX_FLEX)
    }

    pub fn stats(&self, item_flex: u16, power: f64) -> SessionStats {
        SessionStats {
            stability: self.stability(),
            vigour: self.vigour(),
            effectiveness: effectiveness(self.flexibility(item_flex), self.intelligence(), power),
        }
    }
}

/// Effectiveness (**guess**, `docs/milestones/17-forging.md`): Boundless's
/// forge constants in their plain reading, scaled to the one measured line
/// (a diamond hammer with a full forging build: 92% with no coils, 6% a
/// coil). `flex` already includes Forgemaster; `power` is the forge's power
/// from coils (0 until coils exist).
pub fn effectiveness(flex: u16, intelligence: f64, power: f64) -> f64 {
    let f = flex.min(MAX_FLEX) as f64 / MAX_FLEX as f64;
    let insight = 0.2 + 0.8 * (intelligence / (30.0 + 105.0 * (1.0 - f))).min(1.0);
    let flexibility = 0.34 * (0.5 + 4.5 * f * f);
    let power = power.max(0.0) / (1000.0 + 13500.0 * (1.0 - f));
    (flexibility * insight + power).min(MAX_EFFECTIVENESS)
}

/// What sealing costs and gives, by the item's rank (Boundless's `forge.json`
/// levels; reading its "value" as rank is a **guess**).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueBand {
    pub seconds: u32,
    pub spark: u32,
    pub wear: u32,
    pub xp: u32,
}

pub fn queue_band(rank: i32) -> QueueBand {
    const BANDS: [(i32, QueueBand); 6] = [
        (
            200,
            QueueBand {
                seconds: 120,
                spark: 90,
                wear: 4,
                xp: 5,
            },
        ),
        (
            500,
            QueueBand {
                seconds: 240,
                spark: 180,
                wear: 8,
                xp: 10,
            },
        ),
        (
            1000,
            QueueBand {
                seconds: 600,
                spark: 360,
                wear: 12,
                xp: 20,
            },
        ),
        (
            2000,
            QueueBand {
                seconds: 1200,
                spark: 720,
                wear: 24,
                xp: 50,
            },
        ),
        (
            5000,
            QueueBand {
                seconds: 2400,
                spark: 1440,
                wear: 40,
                xp: 75,
            },
        ),
        (
            i32::MAX,
            QueueBand {
                seconds: 3600,
                spark: 2880,
                wear: 60,
                xp: 100,
            },
        ),
    ];
    BANDS
        .iter()
        .find(|(max, _)| rank <= *max)
        .expect("the last band is unbounded")
        .1
}

// ---------------------------------------------------------------- ingredients

/// A timed or lasting effect in a session. Same kind, same buff: a second
/// glaze of a leaning stacks onto the first (Boundless's buff by name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuffKind {
    /// +10 weight per glaze to boons of this leaning.
    Glaze(Leaning),
    /// +10 weight per glaze to traits already on the item.
    Keeping,
    /// +10 weight per glaze to traits not on the item.
    Seeking,
    /// Boon points +50%.
    Boost,
    FlawGuard,
    QuirkGuard,
    /// Stability cost x0.5.
    Shield,
    /// A chance (in tenths) an ingredient costs no stability: 4 or 6.
    Gamble(u8),
    /// Vigour a round: 15 or 50.
    Vigour(u8),
    /// Stability cost less a round: 35 or 50.
    Steady(u8),
    /// Boon misses and quirk or flaw hits are rerolled up to twice.
    Fate,
    /// Flaw points added become fate rounds, one per 100.
    Turnabout,
    /// Vigour lost a round: 20 or 50 (sapping tincture).
    Sapping(u8),
    /// Flaw points a round: 80 or 180 (tainted tincture).
    Tainted(u8),
}

impl BuffKind {
    pub fn describe(self) -> String {
        match self {
            BuffKind::Glaze(l) => format!("leaning to {l:?} boons"),
            BuffKind::Keeping => "leaning to traits it has".into(),
            BuffKind::Seeking => "leaning to new traits".into(),
            BuffKind::Boost => "boon points +50%".into(),
            BuffKind::FlawGuard => "flaw points -50%".into(),
            BuffKind::QuirkGuard => "quirk points -50%".into(),
            BuffKind::Shield => "stability costs halved".into(),
            BuffKind::Gamble(t) => format!("{}% chance of no stability cost", t as u32 * 10),
            BuffKind::Vigour(v) => format!("+{v} vigour a round"),
            BuffKind::Steady(v) => format!("{v} less stability a use"),
            BuffKind::Fate => "second chances on the dice".into(),
            BuffKind::Turnabout => "flaw points become second chances".into(),
            BuffKind::Sapping(v) => format!("-{v} vigour a round"),
            BuffKind::Tainted(v) => format!("+{v} flaw points a round"),
        }
    }

    pub fn is_bad(self) -> bool {
        matches!(self, BuffKind::Sapping(_) | BuffKind::Tainted(_))
    }
}

/// What an ingredient does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Points to the boon, quirk and flaw pools (boon before Effectiveness),
    /// then every pool rolls; maybe a debuff for some rounds.
    Tincture {
        boon: i32,
        quirk: i32,
        flaw: i32,
        after: Option<(BuffKind, u16)>,
    },
    /// A lasting buff.
    Glaze(BuffKind),
    /// A buff for some rounds.
    Salve(BuffKind, u16),
    /// Ends every buff for this much vigour per round each had left (a
    /// lasting one counts 1).
    VigourSpur(i32),
    /// Fate rounds become this many boon points each.
    PointsSpur(i32),
    /// Fate rounds become boost rounds.
    BoostSpur,
    /// Half the flaw pool to the boon pool.
    FlawTurningSpur,
    /// The newest trait of a kind back into its pool, this share of its
    /// points (as far as the pool has room); then that pool rolls.
    Wash(TraitKind, f64),
    /// The newest trait of a kind's points move to another trait of it.
    ShiftWash(TraitKind),
    /// Ends the session: the gear goes into the queue.
    Seal,
    /// Ends it: the gear back unforged, each kind of ingredient used back
    /// with this chance.
    Unmake(f64),
}

pub struct IngredientDef {
    /// The item's name in `assets/items.toml` (`ingredient = "..."`).
    pub name: &'static str,
    pub vigour: i32,
    pub stability: i32,
    pub action: Action,
}

const fn tincture(name: &'static str, vigour: i32, boon: i32, quirk: i32, flaw: i32) -> IngredientDef {
    IngredientDef {
        name,
        vigour,
        stability: 100,
        action: Action::Tincture {
            boon,
            quirk,
            flaw,
            after: None,
        },
    }
}

const fn salve(name: &'static str, vigour: i32, buff: BuffKind, rounds: u16) -> IngredientDef {
    IngredientDef {
        name,
        vigour,
        stability: 100,
        action: Action::Salve(buff, rounds),
    }
}

const fn glaze(name: &'static str, buff: BuffKind) -> IngredientDef {
    IngredientDef {
        name,
        vigour: 50,
        stability: 0,
        action: Action::Glaze(buff),
    }
}

const fn free(name: &'static str, vigour: i32, action: Action) -> IngredientDef {
    IngredientDef {
        name,
        vigour,
        stability: 0,
        action,
    }
}

/// Every ingredient, Boundless's numbers (`ingredientConfig.js`). Stability
/// costs of glazes, spurs and washes are not in the config: 0, as in the
/// simulator.
pub static INGREDIENTS: &[IngredientDef] = &[
    tincture("pure_tincture_1", 0, 100, 0, 0),
    tincture("pure_tincture_2", 50, 200, 0, 0),
    tincture("pure_tincture_3", 100, 400, 0, 0),
    tincture("rough_tincture_1", 0, 200, 0, 80),
    tincture("rough_tincture_2", 50, 400, 0, 160),
    tincture("rough_tincture_3", 100, 1000, 0, 400),
    tincture("wild_tincture_1", 0, 200, 100, 50),
    tincture("wild_tincture_2", 50, 400, 200, 100),
    tincture("wild_tincture_3", 100, 1000, 500, 200),
    IngredientDef {
        name: "sapping_tincture_1",
        vigour: 50,
        stability: 100,
        action: Action::Tincture {
            boon: 400,
            quirk: 160,
            flaw: 0,
            after: Some((BuffKind::Sapping(20), 3)),
        },
    },
    IngredientDef {
        name: "sapping_tincture_2",
        vigour: 100,
        stability: 100,
        action: Action::Tincture {
            boon: 1000,
            quirk: 400,
            flaw: 0,
            after: Some((BuffKind::Sapping(50), 3)),
        },
    },
    IngredientDef {
        name: "tainted_tincture_1",
        vigour: 50,
        stability: 100,
        action: Action::Tincture {
            boon: 400,
            quirk: 0,
            flaw: 0,
            after: Some((BuffKind::Tainted(80), 3)),
        },
    },
    IngredientDef {
        name: "tainted_tincture_2",
        vigour: 100,
        stability: 100,
        action: Action::Tincture {
            boon: 1000,
            quirk: 0,
            flaw: 0,
            after: Some((BuffKind::Tainted(180), 3)),
        },
    },
    tincture("mending_tincture_1", 50, -150, 0, -150),
    tincture("mending_tincture_2", 100, -400, 0, -400),
    tincture("wild_mending_tincture_1", 50, -100, 50, -150),
    tincture("wild_mending_tincture_2", 100, -400, 150, -250),
    tincture("calming_tincture_1", 50, -150, -150, 0),
    tincture("calming_tincture_2", 100, -400, -400, 0),
    tincture("rough_calming_tincture_1", 50, -100, -150, 50),
    tincture("rough_calming_tincture_2", 100, -250, -400, 150),
    glaze("thrift_glaze", BuffKind::Glaze(Leaning::Thrift)),
    glaze("swift_glaze", BuffKind::Glaze(Leaning::Swift)),
    glaze("odd_glaze", BuffKind::Glaze(Leaning::Odd)),
    glaze("force_glaze", BuffKind::Glaze(Leaning::Force)),
    glaze("lasting_glaze", BuffKind::Glaze(Leaning::Lasting)),
    glaze("glow_glaze", BuffKind::Glaze(Leaning::Glow)),
    glaze("elder_glaze", BuffKind::Glaze(Leaning::Elder)),
    glaze("keeping_glaze", BuffKind::Keeping),
    glaze("seeking_glaze", BuffKind::Seeking),
    salve("shield_salve_1", 50, BuffKind::Shield, 2),
    salve("shield_salve_2", 100, BuffKind::Shield, 4),
    salve("boost_salve_1", 50, BuffKind::Boost, 2),
    salve("boost_salve_2", 100, BuffKind::Boost, 4),
    salve("flaw_guard_salve_1", 50, BuffKind::FlawGuard, 2),
    salve("flaw_guard_salve_2", 100, BuffKind::FlawGuard, 4),
    salve("quirk_guard_salve_1", 50, BuffKind::QuirkGuard, 2),
    salve("quirk_guard_salve_2", 100, BuffKind::QuirkGuard, 4),
    salve("vigour_salve_1", 0, BuffKind::Vigour(15), 5),
    salve("vigour_salve_2", 100, BuffKind::Vigour(50), 5),
    salve("steady_salve_1", 100, BuffKind::Steady(35), 5),
    salve("steady_salve_2", 200, BuffKind::Steady(50), 5),
    salve("fate_salve_1", 50, BuffKind::Fate, 3),
    salve("fate_salve_2", 100, BuffKind::Fate, 5),
    salve("turnabout_salve", 200, BuffKind::Turnabout, 5),
    salve("gamble_salve_1", 50, BuffKind::Gamble(4), 3),
    salve("gamble_salve_2", 100, BuffKind::Gamble(6), 3),
    free("flaw_turning_spur", 200, Action::FlawTurningSpur),
    free("vigour_spur_1", 100, Action::VigourSpur(30)),
    free("vigour_spur_2", 200, Action::VigourSpur(50)),
    free("boost_spur", 100, Action::BoostSpur),
    free("points_spur_1", 100, Action::PointsSpur(100)),
    free("points_spur_2", 200, Action::PointsSpur(200)),
    free("boon_wash_1", 100, Action::Wash(TraitKind::Boon, 0.5)),
    free("boon_wash_2", 200, Action::Wash(TraitKind::Boon, 1.0)),
    free("flaw_wash_1", 100, Action::Wash(TraitKind::Flaw, 1.0)),
    free("flaw_wash_2", 200, Action::Wash(TraitKind::Flaw, 0.5)),
    free("quirk_wash_1", 100, Action::Wash(TraitKind::Quirk, 1.0)),
    free("quirk_wash_2", 200, Action::Wash(TraitKind::Quirk, 0.5)),
    free("boon_shift_wash", 100, Action::ShiftWash(TraitKind::Boon)),
    free("flaw_shift_wash", 100, Action::ShiftWash(TraitKind::Flaw)),
    free("quirk_shift_wash", 100, Action::ShiftWash(TraitKind::Quirk)),
    free("sealing_wax", 0, Action::Seal),
    free("unmaking_wax_1", 0, Action::Unmake(0.25)),
    free("unmaking_wax_2", 50, Action::Unmake(0.5)),
    free("unmaking_wax_3", 200, Action::Unmake(0.75)),
];

/// An ingredient by index into [`INGREDIENTS`] (an item's
/// `ItemKind::Ingredient`).
pub fn ingredient_by_name(name: &str) -> Option<u8> {
    INGREDIENTS.iter().position(|d| d.name == name).map(|i| i as u8)
}

pub fn ingredient(index: u8) -> Option<&'static IngredientDef> {
    INGREDIENTS.get(index as usize)
}

// ---------------------------------------------------------------- the dice

/// The session's dice: SplitMix64, saved with the session so a restart
/// rolls on exactly where it left off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dice(pub u64);

impl Dice {
    /// A number in 0..1.
    pub fn roll(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }

    /// One of `weights` at random by weight, as the simulator's
    /// `randomWeighted`; None if there is none.
    fn weighted<T: Copy>(&mut self, weights: &[(T, u32)]) -> Option<T> {
        let total: u32 = weights.iter().map(|w| w.1).sum();
        if total == 0 {
            return None;
        }
        let mut r = self.roll() * total as f64;
        for &(t, w) in weights {
            if r < w as f64 {
                return Some(t);
            }
            r -= w as f64;
        }
        weights.last().map(|w| w.0)
    }
}

// ---------------------------------------------------------------- a session

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Buff {
    pub kind: BuffKind,
    /// Glazes of this kind used (lasting buffs).
    pub count: u16,
    /// Rounds left; None: lasts the session.
    pub rounds: Option<u16>,
    /// Added this round: it acts from the next.
    pub fresh: bool,
}

/// One deck slot: an ingredient and how many are left.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeckSlot {
    pub stack: ItemStack,
    /// Index into [`INGREDIENTS`].
    pub ingredient: u8,
}

/// How a round went, for the forger's screen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoundReport {
    /// The dice on each pool that rolled (boon, quirk, flaw) and whether it
    /// hit.
    pub rolls: [Option<(f64, bool)>; 3],
    /// Points that landed on a trait this round.
    pub landed: Vec<(TraitId, u32)>,
    pub vigour_spent: i32,
    pub stability_spent: i32,
}

/// How a round ended the session, if it did.
#[derive(Clone, Debug, PartialEq)]
pub enum Ended {
    /// Sealed: the gear goes into the queue with these traits.
    Sealed(Forged),
    /// Unmade: the gear back unforged; these ingredients come back (one of
    /// each kind used, with how many of it).
    Unmade(Vec<(ItemStack, u16)>),
    /// Stability ran out: the gear and every ingredient used are lost.
    Destroyed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// No such deck slot, or none left in it.
    Empty,
    /// Its vigour cost is more than the vigour left.
    NoVigour,
    /// Not an ingredient.
    NotAnIngredient,
}

/// A forging session: one piece of gear, a deck, and everything that has
/// happened. Lives in the forge's station record; saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// The gear being forged (unforged until sealed).
    pub item: ItemStack,
    pub gear: Gear,
    pub stats: SessionStats,
    pub stability: i32,
    pub vigour: i32,
    /// Boon, quirk and flaw pools.
    pub pools: [i32; 3],
    /// Traits by kind (boon, quirk, flaw), in the order they landed, with
    /// their points.
    pub traits: [Vec<(TraitId, u32)>; 3],
    pub buffs: Vec<Buff>,
    pub deck: Vec<DeckSlot>,
    /// Every ingredient used, in order (deck slot stacks of one).
    pub used: Vec<ItemStack>,
    pub dice: Dice,
    pub rounds: u32,
    pub last: RoundReport,
}

/// Working state of one round (the simulator's per-round fields).
#[derive(Default)]
struct Round {
    add: [i32; 3],
    enact: bool,
    roll: [bool; 3],
    cost_vigour: i32,
    cost_stability: i32,
    report: RoundReport,
}

impl Session {
    pub fn new(item: ItemStack, gear: Gear, stats: SessionStats, deck: Vec<DeckSlot>, seed: u64) -> Self {
        Self {
            item,
            gear,
            stats,
            stability: stats.stability,
            vigour: stats.vigour,
            pools: [0; 3],
            traits: Default::default(),
            buffs: Vec::new(),
            deck,
            used: Vec::new(),
            dice: Dice(seed),
            rounds: 0,
            last: RoundReport::default(),
        }
    }

    /// The traits as they would be sealed now.
    pub fn forged(&self) -> Forged {
        Forged::from_session(&self.traits)
    }

    fn buff(&self, kind: BuffKind) -> Option<&Buff> {
        self.buffs.iter().find(|b| b.kind == kind)
    }

    /// Acting buffs (not added this round).
    fn acting(&self) -> impl Iterator<Item = Buff> + '_ {
        self.buffs.iter().copied().filter(|b| !b.fresh)
    }

    /// The simulator's `addBuff`.
    fn add_buff(&mut self, kind: BuffKind, rounds: Option<u16>) {
        if let Some(b) = self.buffs.iter_mut().find(|b| b.kind == kind) {
            match rounds {
                None => {
                    b.count += 1;
                    b.rounds = None;
                }
                Some(r) => b.rounds = b.rounds.map(|n| n + r),
            }
        } else {
            self.buffs.push(Buff {
                kind,
                count: 1,
                rounds: rounds.map(|r| 1 + r),
                fresh: true,
            });
        }
    }

    fn remove_buff(&mut self, kind: BuffKind) {
        self.buffs.retain(|b| b.kind != kind);
    }

    /// Whether a trait is on the item (any kind).
    fn on_item(&self, id: TraitId) -> bool {
        self.traits.iter().flatten().any(|&(t, _)| t == id)
    }

    /// The simulator's `getTraitWeights`: every eligible trait of a kind
    /// with its weight.
    pub fn weights(&self, kind: TraitKind) -> Vec<(TraitId, u32)> {
        let current = &self.traits[kind.index()];
        let excluded: Vec<TraitId> = self
            .traits
            .iter()
            .flatten()
            .flat_map(|&(t, _)| t.def().excludes.iter().copied())
            .collect();
        let mut w: Vec<(TraitId, u32)> = TRAITS
            .iter()
            .filter(|d| d.kind == kind && d.fits(self.gear) && !excluded.contains(&d.id))
            .filter(|d| match current.iter().find(|(t, _)| *t == d.id) {
                None => current.len() < TRAITS_PER_KIND,
                Some(&(_, points)) => points < d.max_points(),
            })
            .map(|d| (d.id, 1))
            .collect();
        for b in self.acting() {
            let add = b.count as u32 * GLAZE_WEIGHT;
            for (t, weight) in &mut w {
                let d = t.def();
                let leans = match b.kind {
                    BuffKind::Glaze(l) => d.kind == TraitKind::Boon && d.leaning == l,
                    BuffKind::Keeping => self.on_item(*t),
                    BuffKind::Seeking => !self.on_item(*t),
                    _ => false,
                };
                if leans {
                    *weight += add;
                }
            }
        }
        w
    }

    /// The simulator's `buffTraitPoints` buffs on points about to be added.
    fn buff_points(&self, add: &mut [i32; 3]) {
        for b in self.acting() {
            match b.kind {
                BuffKind::Boost => add[0] += (add[0] as f64 * 0.5).floor() as i32,
                BuffKind::QuirkGuard => add[1] -= (add[1] as f64 * 0.5).floor() as i32,
                BuffKind::FlawGuard => add[2] -= (add[2] as f64 * 0.5).floor() as i32,
                _ => {}
            }
        }
    }

    /// The simulator's `ingredientCost` buffs.
    fn buff_cost(&mut self, stability: &mut i32) {
        let acting: Vec<Buff> = self.acting().collect();
        for b in acting {
            match b.kind {
                BuffKind::Shield => *stability = (*stability as f64 * 0.5).floor() as i32,
                BuffKind::Gamble(tenths) if self.dice.roll() <= tenths as f64 / 10.0 => *stability = 0,
                _ => {}
            }
        }
    }

    /// The simulator's `roundEnd` buffs.
    fn buffs_round_end(&mut self, round: &mut Round) {
        let acting: Vec<Buff> = self.acting().collect();
        for b in acting {
            match b.kind {
                BuffKind::Turnabout => {
                    if round.add[2] > 0 {
                        let r = (round.add[2] as f64 / 100.0).ceil() as u16;
                        self.add_buff(BuffKind::Fate, Some(r));
                    }
                }
                BuffKind::Vigour(v) => self.vigour += v as i32,
                // Never below no cost: the simulator lets it go negative,
                // which would give stability back (a difference on purpose).
                BuffKind::Steady(v) => round.cost_stability = (round.cost_stability - v as i32).max(0),
                BuffKind::Sapping(v) => self.vigour -= v as i32,
                _ => {}
            }
        }
    }

    /// The simulator's `tryToHitBar`: points that hit (0 for a miss).
    fn try_hit(&mut self, kind: TraitKind, round: &mut Round) -> u32 {
        let k = kind.index();
        let p = self.pools[k] as f64 / POOL_MAX as f64;
        if p <= 0.0 {
            return 0;
        }
        let mut r = self.dice.roll();
        let mut hit = r <= p;
        // Fate rerolls boon misses and quirk or flaw hits.
        let want = kind == TraitKind::Boon;
        if hit != want
            && self
                .buff(BuffKind::Fate)
                .is_some_and(|b| b.rounds.is_none_or(|n| n > 0))
        {
            let mut rerolls = 2;
            while hit != want && rerolls > 0 {
                r = self.dice.roll();
                hit = r <= p;
                rerolls -= 1;
            }
        }
        round.report.rolls[k] = Some((r, hit));
        if hit { (r * 1000.0).floor() as u32 } else { 0 }
    }

    /// The simulator's `rollTraitBar`.
    fn roll_pool(&mut self, kind: TraitKind, round: &mut Round) {
        let points = self.try_hit(kind, round);
        if points == 0 {
            return;
        }
        let weights = self.weights(kind);
        let Some(t) = self.dice.weighted(&weights) else {
            round.report.rolls[kind.index()] = None;
            return;
        };
        self.pools[kind.index()] -= points as i32;
        self.add_trait_points(kind, t, points);
        round.report.landed.push((t, points));
    }

    fn add_trait_points(&mut self, kind: TraitKind, t: TraitId, points: u32) {
        let list = &mut self.traits[kind.index()];
        match list.iter_mut().find(|(id, _)| *id == t) {
            Some(e) => e.1 += points,
            None => list.push((t, points)),
        }
    }

    /// Uses one of the ingredient in deck slot `slot`: one round. Ok(None)
    /// while the session goes on.
    pub fn use_ingredient(&mut self, slot: usize) -> Result<Option<Ended>, Refused> {
        let ds = *self
            .deck
            .get(slot)
            .filter(|d| d.stack.count > 0)
            .ok_or(Refused::Empty)?;
        let def = ingredient(ds.ingredient).ok_or(Refused::NotAnIngredient)?;
        if def.vigour > 0 && def.vigour > self.vigour {
            return Err(Refused::NoVigour);
        }
        self.deck[slot].stack.count -= 1;
        let mut one = ds.stack;
        one.count = 1;
        self.rounds += 1;

        // beginRound, then the ingredient's own costs and effect.
        let mut round = Round {
            cost_vigour: def.vigour,
            ..Default::default()
        };
        let mut stab = def.stability;
        self.buff_cost(&mut stab);
        round.cost_stability = stab;
        let mut ended = None;
        match def.action {
            Action::Tincture {
                boon,
                quirk,
                flaw,
                after,
            } => {
                let mut add = [boon, quirk, flaw];
                if add[0] >= 0 {
                    add[0] = (add[0] as f64 * self.stats.effectiveness).floor() as i32;
                }
                self.buff_points(&mut add);
                for (sum, a) in round.add.iter_mut().zip(add) {
                    *sum += a;
                }
                round.enact = true;
                round.roll = [true; 3];
                if let Some((buff, rounds)) = after {
                    self.add_buff(buff, Some(rounds));
                }
            }
            Action::Glaze(buff) => self.add_buff(buff, None),
            Action::Salve(buff, rounds) => self.add_buff(buff, Some(rounds)),
            Action::VigourSpur(per) => {
                let kinds: Vec<BuffKind> = self.buffs.iter().map(|b| b.kind).collect();
                for kind in kinds {
                    let Some(b) = self.buff(kind).copied() else { continue };
                    self.vigour += b.rounds.map_or(1, |r| r as i32) * per;
                    self.buffs_round_end(&mut round);
                    self.remove_buff(kind);
                }
            }
            Action::PointsSpur(per) => {
                if let Some(b) = self.buff(BuffKind::Fate).copied() {
                    self.pools[0] += b.rounds.unwrap_or(1) as i32 * per;
                    self.remove_buff(BuffKind::Fate);
                }
            }
            Action::BoostSpur => {
                if let Some(b) = self.buff(BuffKind::Fate).copied() {
                    self.add_buff(BuffKind::Boost, Some(b.rounds.unwrap_or(1)));
                    self.remove_buff(BuffKind::Fate);
                }
            }
            Action::FlawTurningSpur => {
                let half = (self.pools[2] as f64 * 0.5).floor() as i32;
                self.pools[0] += half;
                self.pools[2] -= half;
            }
            Action::Wash(kind, share) => {
                let k = kind.index();
                let room = (POOL_MAX - self.pools[k]).max(0) as u32;
                if let Some(&(_, points)) = self.traits[k].last() {
                    let take = points.min(room);
                    if take < points {
                        self.traits[k].last_mut().expect("checked").1 -= take;
                    } else {
                        self.traits[k].pop();
                    }
                    self.pools[k] += (take as f64 * share).floor() as i32;
                    round.roll[k] = true;
                }
            }
            Action::ShiftWash(kind) => {
                let k = kind.index();
                if let Some((old, points)) = self.traits[k].pop() {
                    let w: Vec<(TraitId, u32)> = self
                        .weights(kind)
                        .into_iter()
                        .filter(|(t, _)| *t != old && !self.traits[k].iter().any(|(o, _)| o == t))
                        .collect();
                    match self.dice.weighted(&w) {
                        Some(new) => {
                            self.add_trait_points(kind, new, points);
                            round.report.landed.push((new, points));
                        }
                        // Nowhere to go: it stays.
                        None => self.traits[k].push((old, points)),
                    }
                }
            }
            Action::Seal => ended = Some(Ended::Sealed(self.forged())),
            Action::Unmake(chance) => {
                let mut kinds: Vec<(ItemStack, u16)> = Vec::new();
                for s in &self.used {
                    match kinds.iter_mut().find(|(k, _)| k.stacks_with(s)) {
                        Some(k) => k.1 += 1,
                        None => kinds.push((*s, 1)),
                    }
                }
                let back = kinds.into_iter().filter(|_| self.dice.roll() <= chance).collect();
                ended = Some(Ended::Unmade(back));
            }
        }
        if !matches!(def.action, Action::Seal | Action::Unmake(_)) {
            self.used.push(one);
        }

        // endRound.
        let acting: Vec<Buff> = self.acting().collect();
        for b in acting {
            if let BuffKind::Tainted(v) = b.kind {
                let mut add = [0, 0, v as i32];
                self.buff_points(&mut add);
                round.add[2] += add[2];
            }
        }
        if round.enact {
            for (pool, a) in self.pools.iter_mut().zip(round.add) {
                *pool += a;
            }
        }
        for p in &mut self.pools {
            *p = (*p).clamp(0, POOL_MAX);
        }
        // The simulator's order: quirk, flaw, boon.
        for kind in [TraitKind::Quirk, TraitKind::Flaw, TraitKind::Boon] {
            if round.roll[kind.index()] {
                self.roll_pool(kind, &mut round);
            }
        }
        self.buffs_round_end(&mut round);
        for b in &mut self.buffs {
            b.fresh = false;
            if let Some(r) = &mut b.rounds {
                *r = r.saturating_sub(1);
            }
        }
        self.buffs.retain(|b| b.rounds.is_none_or(|r| r > 0));
        let dice = 1.0 + self.dice.roll() * 0.1;
        let stab = (round.cost_stability as f64 * dice).round() as i32;
        self.vigour -= round.cost_vigour;
        self.stability -= stab;
        round.report.vigour_spent = round.cost_vigour;
        round.report.stability_spent = stab;
        self.last = round.report;
        if ended.is_none() && self.stability <= 0 {
            ended = Some(Ended::Destroyed);
        }
        Ok(ended)
    }

    /// What is left in the deck, to give back when the session ends.
    pub fn leftovers(&self) -> Vec<ItemStack> {
        self.deck.iter().map(|d| d.stack).filter(|s| s.count > 0).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemId;

    fn slot(name: &str, count: u16) -> DeckSlot {
        let i = ingredient_by_name(name).unwrap();
        DeckSlot {
            stack: ItemStack::new(ItemId(1000 + i as u16), count),
            ingredient: i,
        }
    }

    fn gear() -> ItemStack {
        ItemStack::new(ItemId(1), 1)
    }

    fn stats(eff: f64) -> SessionStats {
        SessionStats {
            stability: 1800,
            vigour: 1800,
            effectiveness: eff,
        }
    }

    #[test]
    fn every_trait_id_is_its_place_in_the_table() {
        for (i, d) in TRAITS.iter().enumerate() {
            assert_eq!(d.id as usize, i + 1, "{}", d.name);
            assert_eq!(TraitId::from_u8(i as u8 + 1), Some(d.id));
            assert_eq!(TraitId::by_name(d.name), Some(d.id));
            for e in d.excludes {
                assert!(
                    e.def().excludes.contains(&d.id),
                    "{} and {} exclude each other",
                    d.name,
                    e.def().name
                );
            }
        }
        assert_eq!(TraitId::from_u8(0), None);
    }

    #[test]
    fn levels_are_cumulative_thresholds() {
        let d = TraitId::Mighty.def();
        assert_eq!(d.level_of(99), 0);
        assert_eq!(d.level_of(100), 1);
        assert_eq!(d.level_of(1499), 5);
        assert_eq!(d.level_of(9999), 10);
        assert_eq!(TraitId::NightOwl.def().level_of(600), 1);
    }

    #[test]
    fn effectiveness_follows_the_diamond_line_and_the_cap() {
        // A diamond hammer (80) with Forgemaster and full Insight (9 ranks).
        let full = Forger {
            insight: 9.0,
            epic: true,
            ..Default::default()
        };
        let e = |coils: f64| effectiveness(full.flexibility(80), full.intelligence(), 300.0 * coils);
        assert!((e(0.0) - 0.92).abs() < 0.01);
        assert!((e(17.0) - 1.94).abs() < 0.02, "{}", e(17.0));
        assert!(e(18.0) > 1.98 && e(30.0) == 2.0);
        // No coils: softer takes traits more readily; a new character less.
        let new = Forger::default();
        let wood = effectiveness(150, new.intelligence(), 0.0);
        assert!((wood - 0.79).abs() < 0.01);
        assert!(effectiveness(110, new.intelligence(), 0.0) < wood);
        assert!((effectiveness(110, full.intelligence(), 0.0) - 0.99).abs() < 0.01);
    }

    #[test]
    fn forger_stats_follow_the_skill_tables() {
        let f = Forger {
            vitality: 8.0,
            power: 3.0,
            insight: 8.0,
            epic: false,
        };
        assert_eq!(f.stability(), 1700);
        assert_eq!(f.vigour(), 600);
        assert_eq!(f.intelligence(), 90.0);
        assert_eq!(
            Forger {
                vitality: 9.0,
                power: 9.0,
                ..f
            }
            .vigour(),
            1800
        );
        assert_eq!(Forger::default().stability(), 900);
        assert_eq!(Forger::default().deck_slots(), 6);
        assert_eq!(Forger { epic: true, ..f }.flexibility(140), 150);
    }

    #[test]
    fn a_pure_tincture_fills_the_boon_pool_by_effectiveness_and_rolls() {
        let mut s = Session::new(gear(), Gear::Tool, stats(1.5), vec![slot("pure_tincture_1", 10)], 7);
        let before = s.dice;
        assert_eq!(s.use_ingredient(0), Ok(None));
        // 100 x 1.5 = 150, less what moved to a trait.
        let landed: u32 = s.last.landed.iter().map(|l| l.1).sum();
        assert_eq!(s.pools[0] as u32 + landed, 150);
        assert_eq!(s.pools[1], 0);
        assert!(
            s.last.rolls[0].is_some() && s.last.rolls[1].is_none(),
            "empty pools do not roll"
        );
        assert!(s.dice != before);
        assert_eq!(s.deck[0].stack.count, 9);
        assert!((100..=110).contains(&s.last.stability_spent));
        assert_eq!(s.stability, 1800 - s.last.stability_spent);
        assert_eq!(s.vigour, 1800);
    }

    #[test]
    fn a_hit_moves_floor_r_times_1000_points_to_an_eligible_trait() {
        let mut s = Session::new(gear(), Gear::Grapple, stats(2.0), vec![slot("rough_tincture_3", 50)], 3);
        let mut seen = false;
        for _ in 0..10 {
            s.use_ingredient(0).unwrap();
            if let Some((r, true)) = s.last.rolls[0] {
                seen = true;
                assert_eq!(
                    s.last
                        .landed
                        .iter()
                        .find(|l| l.0.def().kind == TraitKind::Boon)
                        .unwrap()
                        .1,
                    (r * 1000.0).floor() as u32
                );
            }
        }
        assert!(seen);
        for (t, _) in s.traits.iter().flatten() {
            assert!(t.def().fits(Gear::Grapple), "{:?}", t);
        }
    }

    #[test]
    fn at_most_three_of_a_kind_and_never_a_boon_beside_its_flaw() {
        let mut s = Session::new(gear(), Gear::Tool, stats(2.0), vec![slot("rough_tincture_3", 100)], 11);
        for _ in 0..100 {
            if s.use_ingredient(0).unwrap().is_some() {
                break;
            }
        }
        for k in &s.traits {
            assert!(k.len() <= 3);
        }
        for &(t, _) in s.traits.iter().flatten() {
            for e in t.def().excludes {
                assert!(!s.on_item(*e), "{t:?} beside {e:?}");
            }
        }
    }

    #[test]
    fn glazes_lean_the_weights_by_ten_each_from_the_next_round() {
        let mut s = Session::new(
            gear(),
            Gear::Tool,
            stats(1.0),
            vec![slot("force_glaze", 2), slot("seeking_glaze", 1)],
            1,
        );
        let base: Vec<_> = s.weights(TraitKind::Boon);
        assert!(base.iter().all(|w| w.1 == 1));
        s.use_ingredient(0).unwrap();
        s.use_ingredient(0).unwrap();
        let w = s.weights(TraitKind::Boon);
        let of = |t| w.iter().find(|x| x.0 == t).unwrap().1;
        assert_eq!(of(TraitId::Mighty), 21);
        assert_eq!(of(TraitId::Quick), 1);
        assert_eq!(s.buffs[0].count, 2);
        assert_eq!(s.stability, 1800, "glazes cost no stability");
        assert_eq!(s.vigour, 1700);
        s.use_ingredient(1).unwrap();
        let w = s.weights(TraitKind::Boon);
        assert_eq!(w.iter().find(|x| x.0 == TraitId::Mighty).unwrap().1, 31);
    }

    #[test]
    fn salves_last_their_rounds_after_the_one_they_are_used_in() {
        let mut s = Session::new(
            gear(),
            Gear::Tool,
            stats(1.0),
            vec![slot("shield_salve_1", 1), slot("pure_tincture_1", 5)],
            5,
        );
        s.use_ingredient(0).unwrap();
        assert_eq!(s.buffs[0].rounds, Some(2));
        s.use_ingredient(1).unwrap();
        assert!(s.last.stability_spent <= 55, "halved: {}", s.last.stability_spent);
        s.use_ingredient(1).unwrap();
        assert!(s.last.stability_spent <= 55);
        assert!(s.buffs.is_empty());
        s.use_ingredient(1).unwrap();
        assert!(s.last.stability_spent >= 100);
    }

    #[test]
    fn fate_rerolls_a_boon_miss() {
        // With a tiny pool nearly every roll misses; fate tries twice more.
        let mut with = 0;
        let mut without = 0;
        for seed in 0..400 {
            let mut a = Session::new(
                gear(),
                Gear::Tool,
                stats(0.3),
                vec![slot("fate_salve_2", 1), slot("pure_tincture_1", 1)],
                seed,
            );
            a.use_ingredient(0).unwrap();
            a.use_ingredient(1).unwrap();
            with += a.last.rolls[0].is_some_and(|r| r.1) as u32;
            let mut b = Session::new(gear(), Gear::Tool, stats(0.3), vec![slot("pure_tincture_1", 1)], seed);
            b.use_ingredient(0).unwrap();
            without += b.last.rolls[0].is_some_and(|r| r.1) as u32;
        }
        assert!(with > without * 2, "{with} vs {without}");
    }

    #[test]
    fn tainted_adds_flaw_points_on_tincture_rounds_only() {
        let mut s = Session::new(
            gear(),
            Gear::Tool,
            stats(0.0),
            vec![
                slot("tainted_tincture_1", 1),
                slot("thrift_glaze", 1),
                slot("pure_tincture_1", 1),
            ],
            9,
        );
        s.use_ingredient(0).unwrap();
        let after_first = s.pools[2] + s.traits[2].iter().map(|t| t.1 as i32).sum::<i32>();
        assert_eq!(after_first, 0, "the debuff acts from the next round");
        s.use_ingredient(1).unwrap();
        assert_eq!(s.pools[2], 0, "no tincture: nothing enacted");
        s.use_ingredient(2).unwrap();
        assert_eq!(s.pools[2] + s.traits[2].iter().map(|t| t.1 as i32).sum::<i32>(), 80);
    }

    #[test]
    fn vigour_is_refused_when_short_and_sapping_drains_it() {
        let st = SessionStats {
            stability: 1800,
            vigour: 120,
            effectiveness: 1.0,
        };
        let mut s = Session::new(
            gear(),
            Gear::Tool,
            st,
            vec![slot("sapping_tincture_1", 3), slot("pure_tincture_1", 3)],
            2,
        );
        s.use_ingredient(0).unwrap();
        assert_eq!(s.vigour, 70);
        s.use_ingredient(1).unwrap();
        assert_eq!(s.vigour, 50, "drained 20");
        s.use_ingredient(1).unwrap();
        assert_eq!(s.vigour, 30);
        assert_eq!(s.use_ingredient(0), Err(Refused::NoVigour));
        assert_eq!(s.deck[0].stack.count, 2, "a refused ingredient is not used");
    }

    #[test]
    fn running_out_of_stability_destroys_the_gear() {
        let st = SessionStats {
            stability: 250,
            vigour: 0,
            effectiveness: 1.0,
        };
        let mut s = Session::new(gear(), Gear::Tool, st, vec![slot("pure_tincture_1", 5)], 4);
        assert_eq!(s.use_ingredient(0), Ok(None));
        assert_eq!(s.use_ingredient(0), Ok(None));
        assert_eq!(s.use_ingredient(0), Ok(Some(Ended::Destroyed)));
        assert_eq!(s.leftovers()[0].count, 2);
    }

    #[test]
    fn a_wash_takes_the_newest_boon_back_and_rolls() {
        let mut s = Session::new(gear(), Gear::Tool, stats(1.0), vec![slot("boon_wash_2", 1)], 8);
        s.traits[0] = vec![(TraitId::Quick, 300), (TraitId::Mighty, 400)];
        s.pools[0] = 700;
        s.use_ingredient(0).unwrap();
        // Room for 300 of Mighty's 400: 100 stays, the pool fills, then rolls.
        let mighty = s.traits[0].iter().find(|t| t.0 == TraitId::Mighty).map_or(0, |t| t.1);
        let landed: u32 = s.last.landed.iter().map(|l| l.1).sum();
        assert_eq!(s.pools[0] as u32 + landed, 1000);
        assert!(mighty >= 100);
        assert!(s.last.rolls[0].is_some());
    }

    #[test]
    fn a_shift_wash_moves_points_to_another_trait() {
        let mut s = Session::new(gear(), Gear::Tool, stats(1.0), vec![slot("boon_shift_wash", 1)], 8);
        s.traits[0] = vec![(TraitId::Quick, 300)];
        s.use_ingredient(0).unwrap();
        assert_eq!(s.traits[0].len(), 1);
        assert_ne!(s.traits[0][0].0, TraitId::Quick);
        assert_eq!(s.traits[0][0].1, 300);
    }

    #[test]
    fn spurs_turn_buffs_into_vigour_and_flaws_into_boons() {
        let mut s = Session::new(
            gear(),
            Gear::Tool,
            stats(1.0),
            vec![
                slot("shield_salve_2", 1),
                slot("vigour_spur_1", 1),
                slot("flaw_turning_spur", 1),
            ],
            8,
        );
        s.use_ingredient(0).unwrap();
        let v = s.vigour;
        s.use_ingredient(1).unwrap();
        assert!(s.buffs.is_empty());
        assert_eq!(s.vigour, v - 100 + 4 * 30);
        s.pools = [100, 0, 501];
        s.use_ingredient(2).unwrap();
        assert_eq!(s.pools, [350, 0, 251]);
    }

    #[test]
    fn sealing_finishes_and_unmaking_returns_some_ingredients() {
        let mut s = Session::new(
            gear(),
            Gear::Tool,
            stats(1.0),
            vec![slot("sealing_wax", 1), slot("unmaking_wax_3", 1)],
            8,
        );
        s.traits[0] = vec![(TraitId::Quick, 650)];
        match s.clone().use_ingredient(0) {
            Ok(Some(Ended::Sealed(f))) => assert_eq!(f.level(TraitId::Quick), 4),
            other => panic!("{other:?}"),
        }
        let mut back_any = false;
        for seed in 0..20 {
            let mut u = Session::new(
                gear(),
                Gear::Tool,
                stats(1.0),
                vec![slot("pure_tincture_1", 5), slot("unmaking_wax_3", 1)],
                seed,
            );
            u.use_ingredient(0).unwrap();
            u.use_ingredient(0).unwrap();
            match u.use_ingredient(1) {
                Ok(Some(Ended::Unmade(back))) => {
                    if let Some((s, n)) = back.first() {
                        back_any = true;
                        assert_eq!((s.count, *n), (1, 2), "all of a kind or none");
                    }
                }
                other => panic!("{other:?}"),
            }
        }
        assert!(back_any);
    }

    #[test]
    fn the_same_seed_forges_the_same() {
        let run = |seed| {
            let mut s = Session::new(gear(), Gear::Sling, stats(1.6), vec![slot("wild_tincture_1", 20)], seed);
            for _ in 0..12 {
                let _ = s.use_ingredient(0);
            }
            s
        };
        assert_eq!(run(42), run(42));
        assert_ne!(run(42).traits, run(43).traits);
    }

    #[test]
    fn traits_change_tool_stats_as_boundless_does() {
        let base = ToolStats {
            class: Some(ToolClass::Hammer),
            damage: 1000,
            action_speed: 30.0,
            crit_chance: 0.1,
            crit_effect: 0.2,
            durability: 500,
            energy: 20,
            aoe: None,
            repair: 0,
        };
        let f = Forged::from_suffix("mighty3,quick2,thrifty10,wide_swing4,sturdy1");
        let t = f.tool(base, &Held::default());
        assert_eq!(t.damage, 1000 + 200 + 150);
        assert_eq!(t.action_speed, 40.0);
        assert_eq!(t.energy, 0);
        assert_eq!(t.aoe, Some(Aoe::ThreeByThree));
        assert_eq!(t.durability, 700);
        let night = Held {
            night: Some(true),
            ..Default::default()
        };
        let owl = Forged::from_suffix("night_owl1");
        assert_eq!(owl.tool(base, &night).damage, 1200);
        assert_eq!(
            owl.tool(
                base,
                &Held {
                    night: Some(false),
                    ..night
                }
            )
            .damage,
            800
        );
        assert_eq!(owl.tool(base, &Held::default()).damage, 1000, "unknown time: no change");
        assert_eq!(Forged::default().tool(base, &night), base);
    }

    #[test]
    fn grapple_traits_and_hookshot() {
        let base = GrappleSpec {
            range: 20.0,
            shot_speed: 60.0,
            reel_in_speed: 10.0,
            reel_out_speed: 10.0,
            hookshot: false,
            energy: 10,
        };
        let g = Forged::from_suffix("long_rope2,hookshot1").grapple(base, &Held::default());
        assert_eq!(g.range, 26.0);
        assert!(g.hookshot);
        assert_eq!(g.reel_in_speed, 30.0);
        assert_eq!(g.reel_out_speed, 10.0);
    }

    #[test]
    fn suffixes_round_trip_and_rank_counts_flaws_and_quirks() {
        let f = Forged::from_suffix("mighty5,loud1,brittle2,nonsense3");
        assert!(f.done);
        assert_eq!(Forged::from_suffix(&f.to_suffix()), f);
        assert_eq!(f.to_suffix(), "mighty5,loud1,brittle2");
        assert_eq!(f.rank(30), 30 + 50 - 12 - 10);
        let empty = Forged::from_suffix("");
        assert!(empty.done && !empty.is_plain());
        assert_eq!(queue_band(100).seconds, 120);
        assert_eq!(queue_band(501).seconds, 600);
    }

    #[test]
    fn every_ingredient_name_is_unique() {
        for (i, d) in INGREDIENTS.iter().enumerate() {
            assert_eq!(ingredient_by_name(d.name), Some(i as u8));
        }
    }
}
