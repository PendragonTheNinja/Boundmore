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

    /// Adds XP; returns the levels gained (marks are added for each).
    pub fn add_xp(&mut self, xp: u64) -> u32 {
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
        ] {
            *a += all;
        }
        e.tool_crafting += gear;
        e.tech_crafting += gear;
        e
    }

    /// The damage multiplier for a tool of `class` (None: the hand stone,
    /// which ignores mastery and tool epics as Boundless's totem did) [tuning
    /// 2.3]: 1 + 0.05 power (x3 with the damage epic) + 0.05 mastery + 0.25
    /// with the tool's epic.
    pub fn damage_multiplier(&self, class: Option<crate::items::ToolClass>) -> f64 {
        use crate::items::ToolClass;
        let power = 0.05 * self.power * if self.damage_epic { 3.0 } else { 1.0 };
        let (mastery, epic) = match class {
            None => (0, false),
            Some(ToolClass::Hammer) => (self.mastery[0], self.hammer_epic),
            Some(ToolClass::Shovel) => (self.mastery[1], self.shovel_axe_epic),
            Some(ToolClass::Axe) => (self.mastery[2], self.shovel_axe_epic),
        };
        1.0 + power + 0.05 * mastery as f64 + if epic { 0.25 } else { 0.0 }
    }

    /// Action speed added to a tool of `class`: 2 a dexterity level, 2 a
    /// mastery rank, and 12 with the tool's epic (players count +22 from
    /// tool skills, tuning 2.5 and its open question 3: the epic's 12 is a
    /// **guess** at the split).
    pub fn action_speed_bonus(&self, class: Option<crate::items::ToolClass>) -> f64 {
        use crate::items::ToolClass;
        let (mastery, epic) = match class {
            None => (0, false),
            Some(ToolClass::Hammer) => (self.mastery[0], self.hammer_epic),
            Some(ToolClass::Shovel) => (self.mastery[1], self.shovel_axe_epic),
            Some(ToolClass::Axe) => (self.mastery[2], self.shovel_axe_epic),
        };
        2.0 * self.dexterity + 2.0 * mastery as f64 + if epic { 12.0 } else { 0.0 }
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

    /// Reach: 4 m plus 1 a control level.
    pub fn reach(&self) -> f64 {
        4.0 + self.control
    }

    /// Multiplier on every bonus-drop chance: 1 + luck / 9 x 1.8 [tuning
    /// 2.8].
    pub fn luck_multiplier(&self) -> f64 {
        1.0 + self.luck / 9.0 * 1.8
    }

    /// Max energy: 600, +150 a zeal level (**guess**), +4000 with the energy
    /// epic (Testing 194 notes, unconfirmed).
    pub fn max_energy(&self) -> u16 {
        let e = 600.0 + 150.0 * self.zeal + if self.energy_epic { 4000.0 } else { 0.0 };
        e.round().min(u16::MAX as f64) as u16
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

    /// Chance a swing does not wear the tool: 30% with the durability epic
    /// (low confidence, tuning 5.4).
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
        }
    }

    /// Crafting level for a gate's skill name (`tool_crafting` ...).
    pub fn crafting(&self, skill: &str) -> u8 {
        match skill {
            "tool_crafting" => self.tool_crafting,
            "tech_crafting" => self.tech_crafting,
            "machine_crafting" => self.machine_crafting,
            "decoration_crafting" => self.decoration_crafting,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ToolClass;

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
        assert_eq!(c.learn(&t, "vitality"), Err(Refused::ComesLater));
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
        assert!((e.reach() - 5.0).abs() < 1e-9);
    }
}
