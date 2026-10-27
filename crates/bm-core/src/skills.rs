//! Skills and progression, as in Boundless's live game (milestone 12,
//! `docs/milestones/12-skills-and-progression.md`, tuning section 5).
//!
//! The skill tree is data (`assets/skills.toml`, sent in the handshake); what
//! each skill does is named by its `effect` and computed here, so client and
//! server agree on every number. Levels, points and marks follow Boundless's
//! tables exactly.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// XP needed to go from level n to n + 1, for n = 1..49 (index n - 1); from
/// 50 on, [`XP_AFTER_50`] a level (tuning 5.1).
const XP_TO_NEXT: [u64; 49] = [
    500, 600, 800, 1200, 1800, 2500, 3500, 4500, 6000, 7500, 9000, 11000, 13000, 15000, 17000, 20000, 23000, 26000,
    29000, 32000, 35000, 38000, 42000, 46000, 50000, 54000, 58000, 63000, 68000, 73000, 78000, 83000, 88000, 94000,
    100000, 106000, 112000, 118000, 125000, 132000, 139000, 146000, 153000, 160000, 168000, 176000, 184000, 192000,
    200000,
];

/// XP per level past 50.
pub const XP_AFTER_50: u64 = 200_000;

/// The most points one page holds.
pub const PAGE_CAP: u16 = 100;
/// Pages a character may own.
pub const MAX_PAGES: usize = 5;
/// Marks (Boundless's cubits) an extra page costs.
pub const PAGE_PRICE: u32 = 600;
/// Marks a full reset of a page costs, once free resets are over.
pub const RESET_PRICE: u32 = 800;
/// Full resets are free below this level.
pub const FREE_RESET_BELOW: u32 = 20;
/// Marks a plot costs (Boundless: 30 cubits a plot, no discount for more;
/// Nathan, 2026-10-21: marks buy plots, as in Boundless).
pub const PLOT_PRICE: u32 = 30;
/// Most plots one purchase may buy (Boundless's largest coffer: 80).
pub const MAX_PLOTS_AT_ONCE: u32 = 80;

/// Switching pages away from home waits this long (15 minutes, in ticks).
pub const SWITCH_COOLDOWN_TICKS: u64 = 15 * 60 * crate::movement::TICK_HZ as u64;

/// The level reached with `xp` in all, and the XP into it and needed for the
/// next.
pub fn level_of(xp: u64) -> (u32, u64, u64) {
    let mut level = 1;
    let mut left = xp;
    loop {
        let need = if level < 50 {
            XP_TO_NEXT[level as usize - 1]
        } else {
            XP_AFTER_50
        };
        if left < need {
            return (level, left, need);
        }
        left -= need;
        level += 1;
    }
}

/// Total XP to reach `level`.
pub fn xp_for_level(level: u32) -> u64 {
    (1..level.max(1))
        .map(|n| {
            if n < 50 {
                XP_TO_NEXT[n as usize - 1]
            } else {
                XP_AFTER_50
            }
        })
        .sum()
}

/// Skill points earned by `level`: 4 at level 2, 2 a level to 100 at 50,
/// then 4 a level (Release 199).
pub fn points_at(level: u32) -> u32 {
    match level {
        0 | 1 => 0,
        2..=50 => 4 + 2 * (level - 2),
        _ => 100 + 4 * (level - 50),
    }
}

/// Marks given for reaching `level` (from 2): 100 a level to 10, then 5 more
/// each level, 300 at 50 and every level after.
pub fn marks_for_level(level: u32) -> u32 {
    match level {
        0 | 1 => 0,
        2..=10 => 100,
        11..=50 => 100 + 5 * (level - 10),
        _ => 300,
    }
}

/// Cleanse points earned by `level`: one a level up to 50 (Release 197).
pub fn cleanse_at(level: u32) -> u32 {
    level.saturating_sub(1).min(49)
}

/// A skill category, with the level that unlocks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Attributes,
    Mastery,
    Crafting,
    Epic1,
    Endurance,
    Epic2,
    Exploration,
    Epic3,
}

impl Category {
    /// Level at which its skills can be bought (tuning 5.2).
    pub fn unlock_level(self) -> u32 {
        match self {
            Category::Attributes => 1,
            Category::Mastery => 2,
            Category::Crafting => 4,
            Category::Epic1 => 6,
            Category::Endurance => 8,
            Category::Epic2 => 10,
            Category::Exploration => 12,
            Category::Epic3 => 14,
        }
    }
}

/// What a skill does, by name. Each rank adds the same again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Power,
    Dexterity,
    Agility,
    Control,
    Luck,
    Zeal,
    /// +1 rank in effect of every attribute per 5 ranks (2 attribute value a
    /// rank).
    AllAttributes,
    HammerMastery,
    ShovelMastery,
    AxeMastery,
    GrappleMastery,
    ToolCrafting,
    TechCrafting,
    MachineCrafting,
    DecorationCrafting,
    HammerEpic,
    ShovelAxeEpic,
    DamageEpic,
    EnergyEpic,
    EnergyRegenEpic,
    DurabilityEpic,
    LightEpic,
    ChiselEpic,
    DoubleJumpEpic,
    HighJumpEpic,
    PortalEpic,
    /// +1 tool, weapon and tech crafting level.
    GearCraftingEpic,
    /// +1 to one crafting skill's level (tool, tech).
    ToolCraftingEpic,
    TechCraftingEpic,
    /// Milestone 13: health, slings and defences.
    Vitality,
    SlingMastery,
    WeaponCrafting,
    HealthEpic,
    SlingEpic,
    RapidFireEpic,
    ChargingEpic,
    StealthEpic,
    ThreatEpic,
    KineticArmour,
    KineticResistance,
    ImpulseArmour,
    ImpulseResistance,
    FallingArmour,
    HealthRegenEpic,
    DeathEpic,
    WeaponCraftingEpic,
    /// Milestone 13 part 2: bombs.
    BombMastery,
    BombEpic,
    /// Milestone 15: spanners remove 10% more wear and swing 2 faster a rank.
    SpannerMastery,
    /// Milestone 16: cooking levels (+1 a rank; the epics +1 each).
    FoodCrafting,
    FoodCraftingEpic,
    /// Milestone 23 part 2: brewing levels (+1 a rank; Master Brewer +1;
    /// Provisioner adds to cooking and brewing).
    BrewCrafting,
    BrewCraftingEpic,
    /// Milestone 16 part 2: the sowing level (+1 a rank) and its epic (+2,
    /// more yield, stronger fertiliser).
    Farming,
    FarmingEpic,
    /// +1 cooking (and brewing, with brews); excludes the gear epic.
    ConsumableCraftingEpic,
    /// Milestone 17: Intelligence for forging (+10 a rank; its healing part
    /// comes with healing), the forging-ingredient crafting level, and the
    /// forging epic (+25 Flexibility, 4 more deck slots).
    Insight,
    IngredientCrafting,
    ForgingEpic,
    /// Milestone 18: protection against each line of hazardous air.
    AcidWard,
    SparkWard,
    FireWard,
    /// On the tree, not built yet ("Comes later"): cannot be bought.
    Later,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillDef {
    /// Unique, stable: saves store spent ranks by it.
    pub id: String,
    /// What players see.
    pub name: String,
    pub category: Category,
    pub ranks: u8,
    /// Points per rank (epics 5).
    pub cost: u8,
    pub effect: Effect,
    /// What one rank does, in plain words, for the skill screen.
    pub text: String,
    /// Skills that cannot be on the same page as this one (either/or).
    #[serde(default)]
    pub excludes: Vec<String>,
}

impl SkillDef {
    pub fn available(&self) -> bool {
        self.effect != Effect::Later
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("skill catalogue is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("skill `{0}` is listed twice")]
    Duplicate(String),
    #[error("skill `{0}` excludes `{1}`, which is not a skill")]
    UnknownExclusion(String, String),
    #[error("skill `{0}` has no ranks or no cost")]
    Empty(String),
}

/// The tree.
#[derive(Clone, Debug, PartialEq)]
pub struct SkillTree {
    pub skills: Vec<SkillDef>,
    by_id: HashMap<String, usize>,
}

#[derive(Deserialize)]
struct SkillFile {
    skill: Vec<SkillDef>,
}

impl SkillTree {
    pub fn from_toml(src: &str) -> Result<Self, SkillError> {
        let f: SkillFile = toml::from_str(src)?;
        Self::from_defs(f.skill)
    }

    pub fn from_defs(skills: Vec<SkillDef>) -> Result<Self, SkillError> {
        let mut by_id = HashMap::new();
        for (i, s) in skills.iter().enumerate() {
            if s.ranks == 0 || s.cost == 0 {
                return Err(SkillError::Empty(s.id.clone()));
            }
            if by_id.insert(s.id.clone(), i).is_some() {
                return Err(SkillError::Duplicate(s.id.clone()));
            }
        }
        for s in &skills {
            if let Some(x) = s.excludes.iter().find(|x| !by_id.contains_key(*x)) {
                return Err(SkillError::UnknownExclusion(s.id.clone(), x.clone()));
            }
        }
        Ok(Self { skills, by_id })
    }

    pub fn builtin() -> Self {
        Self::from_toml(include_str!("../../../assets/skills.toml")).expect("assets/skills.toml is valid")
    }

    pub fn get(&self, id: &str) -> Option<&SkillDef> {
        self.by_id.get(id).map(|&i| &self.skills[i])
    }
}

/// Spent ranks on one page, by skill id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    pub ranks: BTreeMap<String, u8>,
}

impl Page {
    /// Points this page uses.
    pub fn spent(&self, tree: &SkillTree) -> u32 {
        self.ranks
            .iter()
            .filter_map(|(id, r)| tree.get(id).map(|s| *r as u32 * s.cost as u32))
            .sum()
    }

    pub fn rank(&self, id: &str) -> u8 {
        self.ranks.get(id).copied().unwrap_or(0)
    }
}

/// Everything progression saves about a character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Character {
    pub xp: u64,
    /// Marks to spend (Boundless's cubits; earned by levelling).
    pub marks: u32,
    /// At least one.
    pub pages: Vec<Page>,
    pub active: u8,
    /// Cleanse points used so far (earned: `cleanse_at(level)`).
    pub cleanses_used: u32,
    /// Server tick of the last page switch away from home (0: never).
    pub switched_at: u64,
    /// Plots bought with marks, added to the account's plot allowance.
    #[serde(default)]
    pub plots_bought: u32,
    /// XP debt from defeats (milestone 13): while above 0, XP earned is
    /// halved and pays it down.
    #[serde(default)]
    pub debt: u64,
}

/// A defeat's XP debt: 10% of the XP the next level needs (Boundless Release
/// 195), halved by Second Chance.
pub fn defeat_debt(xp: u64, death_epic: bool) -> u64 {
    let (_, _, need) = level_of(xp);
    let d = need / 10;
    if death_epic { d / 2 } else { d }
}

impl Default for Character {
    fn default() -> Self {
        Self {
            xp: 0,
            marks: 0,
            pages: vec![Page::default()],
            active: 0,
            cleanses_used: 0,
            switched_at: 0,
            plots_bought: 0,
            debt: 0,
        }
    }
}

/// Why a change to a character was refused, worded for the player.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("There is no such skill")]
    NoSuchSkill,
    #[error("That skill comes later")]
    ComesLater,
    #[error("That unlocks at level {0}")]
    Locked(u32),
    #[error("That skill is at its highest rank")]
    MaxRank,
    #[error("You have no skill points left")]
    NoPoints,
    #[error("A page holds at most 100 points")]
    PageFull,
    #[error("That cannot go on the same page as {0}")]
    Excluded(String),
    #[error("You have no cleanse points left")]
    NoCleanse,
    #[error("Nothing to take back there")]
    NotSpent,
    #[error("You need {0} marks")]
    Marks(u32),
    #[error("You have every page there is")]
    MaxPages,
    #[error("There is no such page")]
    NoSuchPage,
    #[error("You can switch pages again in {0} minutes")]
    Cooldown(u64),
    #[error("You can buy 1 to 80 plots at once")]
    PlotCount,
}

impl Character {
    pub fn level(&self) -> u32 {
        level_of(self.xp).0
    }

    pub fn page(&self) -> &Page {
        &self.pages[(self.active as usize).min(self.pages.len() - 1)]
    }

    fn page_mut(&mut self) -> &mut Page {
        let i = (self.active as usize).min(self.pages.len() - 1);
        &mut self.pages[i]
    }

    /// Points spent across every page (one pool).
    pub fn spent(&self, tree: &SkillTree) -> u32 {
        self.pages.iter().map(|p| p.spent(tree)).sum()
    }

    pub fn points_left(&self, tree: &SkillTree) -> u32 {
        points_at(self.level()).saturating_sub(self.spent(tree))
    }

    pub fn cleanses_left(&self) -> u32 {
        cleanse_at(self.level()).saturating_sub(self.cleanses_used)
    }

    /// Adds XP; returns the levels gained (marks are added for each). While
    /// in debt from a defeat, half the XP is earned, and all of it pays the
    /// debt down (Boundless Release 195).
    pub fn add_xp(&mut self, xp: u64) -> u32 {
        let xp = if self.debt > 0 {
            self.debt = self.debt.saturating_sub(xp);
            xp / 2
        } else {
            xp
        };
        let before = self.level();
        self.xp = self.xp.saturating_add(xp);
        let after = self.level();
        for l in before + 1..=after {
            self.marks = self.marks.saturating_add(marks_for_level(l));
        }
        after - before
    }

    /// Spends one rank of `id` on the active page.
    pub fn learn(&mut self, tree: &SkillTree, id: &str) -> Result<(), Refused> {
        let s = tree.get(id).ok_or(Refused::NoSuchSkill)?;
        if !s.available() {
            return Err(Refused::ComesLater);
        }
        let unlock = s.category.unlock_level();
        if self.level() < unlock {
            return Err(Refused::Locked(unlock));
        }
        let page = self.page();
        if page.rank(id) >= s.ranks {
            return Err(Refused::MaxRank);
        }
        if let Some(x) = s.excludes.iter().find(|x| page.rank(x) > 0) {
            return Err(Refused::Excluded(tree.get(x).map_or(x.clone(), |d| d.name.clone())));
        }
        if self.points_left(tree) < s.cost as u32 {
            return Err(Refused::NoPoints);
        }
        if page.spent(tree) + s.cost as u32 > PAGE_CAP as u32 {
            return Err(Refused::PageFull);
        }
        *self.page_mut().ranks.entry(id.to_string()).or_insert(0) += 1;
        Ok(())
    }

    /// Takes back one rank of `id` with a cleanse point (free before level
    /// 20, as a full reset is).
    pub fn cleanse(&mut self, id: &str) -> Result<(), Refused> {
        if self.page().rank(id) == 0 {
            return Err(Refused::NotSpent);
        }
        let free = self.level() < FREE_RESET_BELOW;
        if !free && self.cleanses_left() == 0 {
            return Err(Refused::NoCleanse);
        }
        if !free {
            self.cleanses_used += 1;
        }
        let page = self.page_mut();
        let r = page.ranks.get_mut(id).expect("spent");
        *r -= 1;
        if *r == 0 {
            page.ranks.remove(id);
        }
        Ok(())
    }

    /// Clears the active page: free before level 20, else `RESET_PRICE`.
    pub fn reset_page(&mut self) -> Result<(), Refused> {
        if self.level() >= FREE_RESET_BELOW {
            if self.marks < RESET_PRICE {
                return Err(Refused::Marks(RESET_PRICE));
            }
            self.marks -= RESET_PRICE;
        }
        self.page_mut().ranks.clear();
        Ok(())
    }

    pub fn buy_page(&mut self) -> Result<(), Refused> {
        if self.pages.len() >= MAX_PAGES {
            return Err(Refused::MaxPages);
        }
        if self.marks < PAGE_PRICE {
            return Err(Refused::Marks(PAGE_PRICE));
        }
        self.marks -= PAGE_PRICE;
        self.pages.push(Page::default());
        Ok(())
    }

    /// Buys `n` plots (1 to 80) for 30 marks each.
    pub fn buy_plots(&mut self, n: u32) -> Result<(), Refused> {
        if n == 0 || n > MAX_PLOTS_AT_ONCE {
            return Err(Refused::PlotCount);
        }
        let price = n * PLOT_PRICE;
        if self.marks < price {
            return Err(Refused::Marks(price));
        }
        self.marks -= price;
        self.plots_bought = self.plots_bought.saturating_add(n);
        Ok(())
    }

    /// Makes page `i` active at server tick `now`; free `at_home` (inside
    /// your own claim), otherwise once per `SWITCH_COOLDOWN_TICKS`.
    pub fn switch_page(&mut self, i: u8, now: u64, at_home: bool) -> Result<(), Refused> {
        if i as usize >= self.pages.len() {
            return Err(Refused::NoSuchPage);
        }
        if i == self.active {
            return Ok(());
        }
        if !at_home {
            let ready = self.switched_at.saturating_add(SWITCH_COOLDOWN_TICKS);
            if self.switched_at != 0 && now < ready {
                let ticks_per_minute = 60 * crate::movement::TICK_HZ as u64;
                return Err(Refused::Cooldown((ready - now).div_ceil(ticks_per_minute)));
            }
            self.switched_at = now.max(1);
        }
        self.active = i;
        Ok(())
    }

    /// What the active page does.
    pub fn effects(&self, tree: &SkillTree) -> Effects {
        Effects::of(self.page(), tree)
    }
}

/// What a page does, summed over its ranks. The numbers that use these are
/// below (`skill_damage` and friends) so client and server compute the same.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Effects {
    /// Attribute levels, 0 to 9 (8 ranks plus 5 ranks of the bonus, 0.2
    /// each).
    pub power: f64,
    pub dexterity: f64,
    pub agility: f64,
    pub control: f64,
    pub luck: f64,
    pub zeal: f64,
    /// Mastery ranks: hammer, shovel, axe.
    pub mastery: [u8; 3],
    pub grapple_mastery: u8,
    pub hammer_epic: bool,
    pub shovel_axe_epic: bool,
    pub damage_epic: bool,
    pub energy_epic: bool,
    pub energy_regen_epic: bool,
    pub durability_epic: bool,
    pub light_epic: bool,
    pub chisel_epic: bool,
    pub double_jump: bool,
    pub high_jump: bool,
    pub portal_epic: bool,
    /// Crafting levels.
    pub tool_crafting: u8,
    pub tech_crafting: u8,
    pub machine_crafting: u8,
    pub decoration_crafting: u8,
    /// Milestone 16.
    pub food_crafting: u8,
    /// Milestone 23 part 2.
    pub brew_crafting: u8,
    /// What the brew in effect adds (`Effects::brewed`): damage and action
    /// speed as fractions more, and how much nearer creatures notice you.
    pub brew_damage: f64,
    pub brew_speed: f64,
    pub brew_hush: f64,
    /// The Farming (crop sowing) level and the Farming Epic.
    pub farming: u8,
    pub farming_epic: bool,
    /// Milestone 17: Insight ranks (an attribute: Wholeness adds to it), the
    /// ingredient crafting level, Forgemaster.
    pub insight: f64,
    pub ingredient_crafting: u8,
    pub forging_epic: bool,
    /// Ward ranks against acid, charged and burning air (milestone 18).
    pub wards: [u8; 3],
    /// Milestone 13.
    pub vitality: f64,
    pub sling_mastery: u8,
    pub weapon_crafting: u8,
    pub health_epic: bool,
    pub sling_epic: bool,
    pub rapid_fire: bool,
    pub charging: bool,
    pub stealth_epic: bool,
    pub threat_epic: bool,
    /// Ranks: kinetic armour, kinetic resistance, impulse armour, impulse
    /// resistance, falling armour.
    pub defence: [u8; 5],
    pub health_regen_epic: bool,
    pub death_epic: bool,
    pub bomb_mastery: u8,
    pub bomb_epic: bool,
    pub spanner_mastery: u8,
}

impl Effects {
    pub fn of(page: &Page, tree: &SkillTree) -> Self {
        let mut e = Effects::default();
        let mut all = 0.0;
        let mut gear = 0;
        for (id, &r) in &page.ranks {
            let Some(s) = tree.get(id) else { continue };
            let f = r as f64;
            let on = r > 0;
            match s.effect {
                Effect::Power => e.power += f,
                Effect::Dexterity => e.dexterity += f,
                Effect::Agility => e.agility += f,
                Effect::Control => e.control += f,
                Effect::Luck => e.luck += f,
                Effect::Zeal => e.zeal += f,
                Effect::AllAttributes => all += 0.2 * f,
                Effect::HammerMastery => e.mastery[0] += r,
                Effect::ShovelMastery => e.mastery[1] += r,
                Effect::AxeMastery => e.mastery[2] += r,
                Effect::GrappleMastery => e.grapple_mastery += r,
                Effect::ToolCrafting => e.tool_crafting += r,
                Effect::TechCrafting => e.tech_crafting += r,
                Effect::MachineCrafting => e.machine_crafting += r,
                Effect::DecorationCrafting => e.decoration_crafting += r,
                Effect::HammerEpic => e.hammer_epic |= on,
                Effect::ShovelAxeEpic => e.shovel_axe_epic |= on,
                Effect::DamageEpic => e.damage_epic |= on,
                Effect::EnergyEpic => e.energy_epic |= on,
                Effect::EnergyRegenEpic => e.energy_regen_epic |= on,
                Effect::DurabilityEpic => e.durability_epic |= on,
                Effect::LightEpic => e.light_epic |= on,
                Effect::ChiselEpic => e.chisel_epic |= on,
                Effect::DoubleJumpEpic => e.double_jump |= on,
                Effect::HighJumpEpic => e.high_jump |= on,
                Effect::PortalEpic => e.portal_epic |= on,
                Effect::GearCraftingEpic => gear += r,
                Effect::ToolCraftingEpic => e.tool_crafting += r,
                Effect::TechCraftingEpic => e.tech_crafting += r,
                Effect::Vitality => e.vitality += f,
                Effect::SlingMastery => e.sling_mastery += r,
                Effect::WeaponCrafting | Effect::WeaponCraftingEpic => e.weapon_crafting += r,
                Effect::HealthEpic => e.health_epic |= on,
                Effect::SlingEpic => e.sling_epic |= on,
                Effect::RapidFireEpic => e.rapid_fire |= on,
                Effect::ChargingEpic => e.charging |= on,
                Effect::StealthEpic => e.stealth_epic |= on,
                Effect::ThreatEpic => e.threat_epic |= on,
                Effect::KineticArmour => e.defence[0] += r,
                Effect::KineticResistance => e.defence[1] += r,
                Effect::ImpulseArmour => e.defence[2] += r,
                Effect::ImpulseResistance => e.defence[3] += r,
                Effect::FallingArmour => e.defence[4] += r,
                Effect::HealthRegenEpic => e.health_regen_epic |= on,
                Effect::DeathEpic => e.death_epic |= on,
                Effect::BombMastery => e.bomb_mastery += r,
                Effect::SpannerMastery => e.spanner_mastery += r,
                Effect::BombEpic => e.bomb_epic |= on,
                Effect::FoodCrafting | Effect::FoodCraftingEpic => e.food_crafting += r,
                Effect::ConsumableCraftingEpic => {
                    e.food_crafting += r;
                    e.brew_crafting += r;
                }
                Effect::BrewCrafting | Effect::BrewCraftingEpic => e.brew_crafting += r,
                Effect::Farming => e.farming += r,
                Effect::FarmingEpic => {
                    e.farming += 2 * r;
                    e.farming_epic |= on;
                }
                Effect::Insight => e.insight += f,
                Effect::IngredientCrafting => e.ingredient_crafting += r,
                Effect::ForgingEpic => e.forging_epic |= on,
                Effect::AcidWard => e.wards[0] += r,
                Effect::SparkWard => e.wards[1] += r,
                Effect::FireWard => e.wards[2] += r,
                Effect::Later => {}
            }
        }
        for a in [
            &mut e.power,
            &mut e.dexterity,
            &mut e.agility,
            &mut e.control,
            &mut e.luck,
            &mut e.zeal,
            &mut e.vitality,
            &mut e.insight,
        ] {
            *a += all;
        }
        e.tool_crafting += gear;
        e.tech_crafting += gear;
        e.weapon_crafting += gear;
        e
    }

    /// What these skills make of a forger (milestone 17).
    pub fn forger(&self) -> crate::forge::Forger {
        crate::forge::Forger {
            vitality: self.vitality,
            power: self.power,
            insight: self.insight,
            epic: self.forging_epic,
        }
    }

    /// The damage multiplier for a tool of `class` (None: the hand stone,
    /// which ignores mastery and tool epics as Boundless's totem did) [tuning
    /// 2.3]: 1 + 0.05 power (x3 with the damage epic) + 0.05 mastery + 0.25
    /// with the tool's epic.
    pub fn damage_multiplier(&self, class: Option<crate::items::ToolClass>) -> f64 {
        use crate::items::ToolClass;
        // Brawn brews add to Power's share, so the damage epic triples them
        // too (Boundless, research A.4).
        let power = (0.05 * self.power + self.brew_damage) * if self.damage_epic { 3.0 } else { 1.0 };
        let (mastery, epic) = match class {
            None => (0, false),
            Some(ToolClass::Hammer) => (self.mastery[0], self.hammer_epic),
            Some(ToolClass::Shovel) => (self.mastery[1], self.shovel_axe_epic),
            Some(ToolClass::Axe) => (self.mastery[2], self.shovel_axe_epic),
            // A spanner deals no damage; its mastery adds repair.
            Some(ToolClass::Spanner | ToolClass::Tiller) => (0, false),
        };
        1.0 + power + 0.05 * mastery as f64 + if epic { 0.25 } else { 0.0 }
    }

    /// Action speed added to a tool of `class`: 2 a dexterity level, 2 a
    /// mastery rank, and 10 with the tool's epic (Boundless's skill text,
    /// tuning 5.4a).
    pub fn action_speed_bonus(&self, class: Option<crate::items::ToolClass>) -> f64 {
        use crate::items::ToolClass;
        let (mastery, epic) = match class {
            None => (0, false),
            Some(ToolClass::Hammer) => (self.mastery[0], self.hammer_epic),
            Some(ToolClass::Shovel) => (self.mastery[1], self.shovel_axe_epic),
            Some(ToolClass::Axe) => (self.mastery[2], self.shovel_axe_epic),
            Some(ToolClass::Spanner) => (self.spanner_mastery, false),
            Some(ToolClass::Tiller) => (0, false),
        };
        2.0 * self.dexterity + 2.0 * mastery as f64 + if epic { 10.0 } else { 0.0 }
    }

    /// An item's action speed with these skills: its own plus
    /// `action_speed_bonus`'s points, times a Swift brew's (Boundless's fast
    /// brews multiply the total, tuning 2.5).
    pub fn action_speed(&self, base: f32, bonus: f64) -> f32 {
        ((base as f64 + bonus) * (1.0 + self.brew_speed)) as f32
    }

    /// These effects with what the brew in `s` adds (milestone 23 part 2).
    pub fn brewed(mut self, s: &crate::movement::PlayerState) -> Effects {
        self.brew_damage = s.brew_damage();
        self.brew_speed = s.brew_action_speed();
        self.brew_hush = 1.0 - s.brew_noticed_at();
        self
    }

    /// Crit chance added: 0.2 x agility / 9 [tuning 2.3].
    pub fn crit_bonus(&self) -> f64 {
        0.2 * self.agility / 9.0
    }

    /// Run and sprint speed: 3.0 and 5.0 m/s plus 0.3 and 0.6 an agility
    /// level [tuning 1.6].
    pub fn run_sprint(&self) -> (f64, f64) {
        (3.0 + 0.3 * self.agility, 5.0 + 0.6 * self.agility)
    }

    /// Reach: 4 m plus 0.75 a control level (Boundless's skill text,
    /// tuning 5.4a).
    pub fn reach(&self) -> f64 {
        4.0 + self.reach_bonus()
    }

    /// Reach added to the server's base reach: 0.75 a control level.
    pub fn reach_bonus(&self) -> f64 {
        0.75 * self.control
    }

    /// Multiplier on every bonus-drop chance: 1 + luck / 9 x 1.8 [tuning
    /// 2.8].
    pub fn luck_multiplier(&self) -> f64 {
        1.0 + self.luck / 9.0 * 1.8
    }

    /// Max energy: 600 + 1400 x (zeal / 9)^1.5, rounded to 10, the curve
    /// that reproduces every rank of Boundless's Zeal text (+50, 100, 120,
    /// 140, 170, 180, 200, 210; 2000 at 9), the same shape as its health
    /// curve; then x3 with the energy epic (tuning 5.4a).
    pub fn max_energy(&self) -> u16 {
        let z = (self.zeal / 9.0).clamp(0.0, 1.0);
        let e = (600.0 + 1400.0 * z * libm::sqrt(z)) / 10.0;
        let e = libm::round(e) * 10.0 * if self.energy_epic { 3.0 } else { 1.0 };
        e.min(u16::MAX as f64) as u16
    }

    /// Refill per tick once the pause is over: 5 (300 a second, a guess)
    /// plus 15 (900 a second) with the regeneration epic.
    pub fn regen_per_tick(&self) -> u16 {
        5 + if self.energy_regen_epic { 15 } else { 0 }
    }

    /// Grapple shot speed added: 5 a mastery rank.
    pub fn grapple_shot_bonus(&self) -> f64 {
        5.0 * self.grapple_mastery as f64
    }

    /// Grapple range added: 5 a mastery rank (Boundless's skill text).
    pub fn grapple_range_bonus(&self) -> f64 {
        5.0 * self.grapple_mastery as f64
    }

    /// Wear saved: 30% with the durability epic (Boundless's skill text:
    /// "reduces the amount of durability lost by 30%"), as a chance a swing
    /// does not wear the tool.
    pub fn wear_saved(&self) -> f64 {
        if self.durability_epic { 0.3 } else { 0.0 }
    }

    /// What the page does to the body, for `PlayerState::body` (movement,
    /// energy, grapples): set by the server at an agreed input.
    pub fn body(&self) -> crate::movement::Body {
        let (run, sprint) = self.run_sprint();
        crate::movement::Body {
            run_bonus: run - 3.0,
            sprint_bonus: sprint - 5.0,
            high_jump: self.high_jump,
            double_jump: self.double_jump,
            energy_bonus: self.max_energy().saturating_sub(crate::energy::MAX_ENERGY),
            regen_bonus: self.regen_per_tick().saturating_sub(crate::energy::REGEN_PER_TICK),
            grapple_shot_bonus: self.grapple_shot_bonus(),
            grapple_range_bonus: self.grapple_range_bonus(),
            health_bonus: self.max_health().saturating_sub(crate::combat::BASE_HEALTH),
            falling_armour: self.defence().falling_armour as u16,
            mending: self.health_regen_epic,
            wards: self.wards.map(crate::atmosphere::ward_points),
            heal_bonus: (10.0 * self.insight).round().clamp(0.0, u16::MAX as f64) as u16,
            buff_bonus: (6.0 * self.zeal).round().clamp(0.0, 90.0) as u16,
            // Creative mode is the server's switch, not a skill: whoever
            // sets the body from the skills keeps the player's own.
            creative: false,
        }
    }

    /// Max health: the vitality curve, x3 with Iron Body (`combat::max_health`).
    pub fn max_health(&self) -> u32 {
        crate::combat::max_health(self.vitality, self.health_epic)
    }

    /// Armour and resistance from the endurance skills (Boundless's skill
    /// text, tuning 5.4a): armour +10 a rank, resistance +5% a rank,
    /// falling armour +20 a rank.
    pub fn defence(&self) -> crate::combat::Defence {
        let d = self.defence;
        crate::combat::Defence {
            kinetic_armour: 10 * d[0] as u32,
            kinetic_resist: 0.05 * d[1] as f32,
            impulse_armour: 10 * d[2] as u32,
            impulse_resist: 0.05 * d[3] as f32,
            falling_armour: 20 * d[4] as u32,
        }
    }

    /// A sling with these skills [tuning 3.3.1]: damage x (1 + 0.05 power
    /// (x3 with the damage epic) + 0.05 a Sling Skill rank + 0.25 with
    /// Sharpshot), action speed + 2 a dexterity level + 2 a rank + 10 with
    /// Sharpshot, crit chance plus agility's.
    pub fn skilled_sling(&self, l: &crate::items::SlingStats) -> crate::items::SlingStats {
        let power = (0.05 * self.power + self.brew_damage) * if self.damage_epic { 3.0 } else { 1.0 };
        let mult = 1.0 + power + 0.05 * self.sling_mastery as f64 + if self.sling_epic { 0.25 } else { 0.0 };
        let speed = 2.0 * self.dexterity + 2.0 * self.sling_mastery as f64 + if self.sling_epic { 10.0 } else { 0.0 };
        crate::items::SlingStats {
            damage: (l.damage as f64 * mult - 1e-9).ceil().max(0.0) as u32,
            action_speed: self.action_speed(l.action_speed, speed),
            crit_chance: (l.crit_chance + self.crit_bonus() as f32).min(1.0),
            ..*l
        }
    }

    /// A bag of bombs with these skills, as `skilled_sling` with Bomb Skill
    /// (+5% damage, +2 action speed a rank) and Blaster (+25%, +10).
    pub fn skilled_bomb(&self, b: &crate::items::BombStats) -> crate::items::BombStats {
        let power = (0.05 * self.power + self.brew_damage) * if self.damage_epic { 3.0 } else { 1.0 };
        let mult = 1.0 + power + 0.05 * self.bomb_mastery as f64 + if self.bomb_epic { 0.25 } else { 0.0 };
        let speed = 2.0 * self.dexterity + 2.0 * self.bomb_mastery as f64 + if self.bomb_epic { 10.0 } else { 0.0 };
        crate::items::BombStats {
            damage: (b.damage as f64 * mult - 1e-9).ceil().max(0.0) as u32,
            action_speed: self.action_speed(b.action_speed, speed),
            crit_chance: (b.crit_chance + self.crit_bonus() as f32).min(1.0),
            ..*b
        }
    }

    /// How far creatures notice you, as a multiple of their range: half
    /// with Quiet Step (+50% stealth, tuning 5.4a).
    pub fn noticed_at(&self) -> f64 {
        (if self.stealth_epic { 0.5 } else { 1.0 }) * (1.0 - self.brew_hush)
    }

    /// Threat you make, as a multiple: twice with Lure (+100%).
    pub fn threat(&self) -> f64 {
        if self.threat_epic { 2.0 } else { 1.0 }
    }

    /// Crafting level for a gate's skill name (`tool_crafting` ...).
    pub fn crafting(&self, skill: &str) -> u8 {
        match skill {
            "tool_crafting" => self.tool_crafting,
            "tech_crafting" => self.tech_crafting,
            "machine_crafting" => self.machine_crafting,
            "decoration_crafting" => self.decoration_crafting,
            "weapon_crafting" => self.weapon_crafting,
            "food_crafting" => self.food_crafting,
            "brew_crafting" => self.brew_crafting,
            "ingredient_crafting" => self.ingredient_crafting,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ToolClass;

    #[test]
    fn combat_skills() {
        let t = SkillTree::builtin();
        let mut c = Character::default();
        c.add_xp(xp_for_level(50));
        for id in [
            "vitality",
            "vitality",
            "slingbow_mastery",
            "kinetic_armour",
            "kinetic_resistance",
            "falling_armour",
        ] {
            c.learn(&t, id).unwrap();
        }
        c.learn(&t, "slingbow_epic").unwrap();
        c.learn(&t, "rapid_fire_epic").unwrap();
        assert!(matches!(c.learn(&t, "charging_epic"), Err(Refused::Excluded(_))));
        let e = c.effects(&t);
        assert_eq!(e.max_health(), 960);
        assert_eq!(e.body().health_bonus, 360);
        assert_eq!(e.body().falling_armour, 20);
        let d = e.defence();
        assert_eq!((d.kinetic_armour, d.kinetic_resist), (10, 0.05));
        let items = crate::items::ItemRegistry::builtin(&crate::block::BlockRegistry::builtin());
        let stone = items.get(items.id("stone_sling").unwrap()).unwrap().sling().unwrap();
        let skilled = e.skilled_sling(&stone);
        // 300 x (1 + 0.05 + 0.25) = 390; action speed 10 + 2 + 10.
        assert_eq!(skilled.damage, 390);
        assert_eq!(skilled.action_speed, 22.0);
        assert_eq!(skilled.energy, 6);
    }

    #[test]
    fn bomb_skills() {
        let t = SkillTree::builtin();
        let mut c = Character::default();
        c.add_xp(xp_for_level(50));
        c.learn(&t, "bomb_mastery").unwrap();
        c.learn(&t, "bomb_epic").unwrap();
        let items = crate::items::ItemRegistry::builtin(&crate::block::BlockRegistry::builtin());
        let bag = items.get(items.id("copper_bombs").unwrap()).unwrap().bomb().unwrap();
        assert_eq!((bag.durability, bag.energy, bag.radius), (200, 90, 3.0));
        let b = c.effects(&t).skilled_bomb(&bag);
        // 900 x (1 + 0.05 + 0.25) = 1170; action speed 50 + 2 + 10.
        assert_eq!((b.damage, b.action_speed), (1170, 62.0));
    }

    #[test]
    fn defeat_debt_halves_xp_until_paid() {
        let mut c = Character::default();
        c.add_xp(xp_for_level(10));
        // Level 10 needs 7500 for the next: a debt of 750.
        c.debt = defeat_debt(c.xp, false);
        assert_eq!(c.debt, 750);
        assert_eq!(defeat_debt(c.xp, true), 375);
        let before = c.xp;
        c.add_xp(500);
        assert_eq!(c.xp - before, 250, "half XP while in debt");
        assert_eq!(c.debt, 250);
        c.add_xp(500);
        assert_eq!(c.debt, 0);
        let before = c.xp;
        c.add_xp(100);
        assert_eq!(c.xp - before, 100, "full XP once paid");
    }

    #[test]
    fn levels_match_boundless() {
        assert_eq!(level_of(0), (1, 0, 500));
        assert_eq!(level_of(499).0, 1);
        assert_eq!(level_of(500), (2, 0, 600));
        assert_eq!(xp_for_level(10), 21_400);
        assert_eq!(xp_for_level(50), 3_304_900);
        assert_eq!(level_of(3_304_900).0, 50);
        assert_eq!(level_of(3_304_900 + 200_000).0, 51);
        assert_eq!(points_at(2), 4);
        assert_eq!(points_at(50), 100);
        assert_eq!(points_at(51), 104);
        assert_eq!((2..=50).map(marks_for_level).sum::<u32>(), 9000);
        assert_eq!(marks_for_level(11), 105);
        assert_eq!(cleanse_at(50), 49);
    }

    #[test]
    fn the_tree_loads() {
        let t = SkillTree::builtin();
        assert!(t.skills.len() > 40);
        assert!(t.get("power").is_some_and(|s| s.ranks == 8 && s.available()));
    }

    fn at_level(level: u32) -> Character {
        Character {
            xp: xp_for_level(level),
            ..Default::default()
        }
    }

    #[test]
    fn spending_rules() {
        let t = SkillTree::builtin();
        let mut c = at_level(1);
        assert_eq!(c.learn(&t, "power"), Err(Refused::NoPoints));
        let mut c2 = at_level(5);
        assert_eq!(c2.learn(&t, "hammer_epic"), Err(Refused::Locked(6)));
        c = at_level(50);
        for _ in 0..8 {
            c.learn(&t, "power").unwrap();
        }
        assert_eq!(c.learn(&t, "power"), Err(Refused::MaxRank));
        c.learn(&t, "hammer_epic").unwrap();
        assert!(matches!(c.learn(&t, "shovel_axe_epic"), Err(Refused::Excluded(_))));
        assert_eq!(c.learn(&t, "augment_crafting"), Err(Refused::ComesLater));
        assert_eq!(c.page().spent(&t), 13);
        // The page cap and the pool.
        let ids: Vec<String> = t
            .skills
            .iter()
            .filter(|s| s.available())
            .map(|s| s.id.clone())
            .collect();
        for id in &ids {
            while c.learn(&t, id).is_ok() {}
        }
        assert!(c.page().spent(&t) <= 100);
        assert_eq!(c.points_left(&t), 100 - c.spent(&t));
    }

    #[test]
    fn pages_resets_and_cleanses() {
        let t = SkillTree::builtin();
        let mut c = at_level(10);
        c.learn(&t, "power").unwrap();
        c.cleanse("power").unwrap();
        assert_eq!(c.cleanses_used, 0, "free below level 20");
        c = at_level(30);
        c.learn(&t, "power").unwrap();
        c.cleanse("power").unwrap();
        assert_eq!(c.cleanses_left(), 28);
        assert_eq!(c.reset_page(), Err(Refused::Marks(800)));
        c.marks = 1400;
        c.reset_page().unwrap();
        c.buy_page().unwrap();
        assert_eq!((c.pages.len(), c.marks), (2, 0));
        assert_eq!(c.buy_page(), Err(Refused::Marks(600)));
        c.switch_page(1, 100, false).unwrap();
        assert!(matches!(c.switch_page(0, 200, false), Err(Refused::Cooldown(15))));
        c.switch_page(0, 200, true).unwrap();
        assert_eq!(c.active, 0);
        assert!(c.switch_page(1, 100 + SWITCH_COOLDOWN_TICKS, false).is_ok());
        // Plots: 30 marks each, 1 to 80 at once.
        c.marks = 100;
        assert_eq!(c.buy_plots(4), Err(Refused::Marks(120)));
        c.buy_plots(3).unwrap();
        assert_eq!((c.plots_bought, c.marks), (3, 10));
        assert_eq!(c.buy_plots(81), Err(Refused::PlotCount));
    }

    #[test]
    fn effects_follow_the_damage_formula() {
        let t = SkillTree::builtin();
        let mut c = at_level(50);
        // The typical miner's build (tuning 2.7): 8 power, 5 all attributes,
        // 5 hammer mastery, hammer epic, damage epic.
        for (id, n) in [("power", 8), ("all_attributes", 5), ("hammer_mastery", 5)] {
            for _ in 0..n {
                c.learn(&t, id).unwrap();
            }
        }
        c.learn(&t, "hammer_epic").unwrap();
        c.learn(&t, "damage_epic").unwrap();
        let e = c.effects(&t);
        assert!((e.power - 9.0).abs() < 1e-9);
        // Tuning 2.3: the maximum is 2.85; copper hits 1140.
        let m = e.damage_multiplier(Some(ToolClass::Hammer));
        assert!((m - 2.85).abs() < 1e-9, "{m}");
        assert_eq!((400.0 * m).round(), 1140.0);
        // The hand stone ignores mastery and the hammer epic: 1 + 1.35.
        assert!((e.damage_multiplier(None) - 2.35).abs() < 1e-9);
        assert!(
            (e.run_sprint().0 - 3.3).abs() < 1e-9,
            "the bonus alone adds one agility level"
        );
        assert!((e.reach() - 4.75).abs() < 1e-9);
        assert!((e.action_speed_bonus(Some(ToolClass::Hammer)) - (2.0 + 10.0 + 10.0)).abs() < 1e-9);
    }

    #[test]
    fn energy_follows_boundless_zeal_text() {
        // Boundless's Zeal ranks add 50, 100, 120, 140, 170, 180, 200, 210.
        let adds = [50u16, 100, 120, 140, 170, 180, 200, 210];
        let mut want = 600;
        for (rank, add) in adds.iter().enumerate() {
            want += add;
            let e = Effects {
                zeal: rank as f64 + 1.0,
                ..Default::default()
            };
            assert_eq!(e.max_energy(), want, "rank {}", rank + 1);
        }
        let full = Effects {
            zeal: 9.0,
            ..Default::default()
        };
        assert_eq!(full.max_energy(), 2000);
        let epic = Effects {
            energy_epic: true,
            ..full
        };
        assert_eq!(epic.max_energy(), 6000, "the energy epic triples it");
        assert_eq!(Effects::default().max_energy(), 600);
    }
}
