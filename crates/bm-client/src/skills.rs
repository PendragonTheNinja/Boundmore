//! Skills and progression on the client (milestone 12): the character as
//! the server last said it, our predicted copy with the skill changes we
//! have sent and it has not answered yet, and the skill screen's model (what
//! each skill shows and whether a click does anything). Pure logic, tested
//! without a window; `skill_screen.rs` draws it.
//!
//! Every change is ruled by the same `bm_core::skills::Character` methods
//! the server uses, so a change we predict is one the server accepts unless
//! something we cannot see differs (the page-switch cooldown near its end,
//! whether we stand at home). A change we would refuse is never sent.

use bm_core::Recipe;
use bm_core::movement::Body;
use bm_core::skills::{Category, Character, Effects, Refused, SkillDef, SkillTree};
use bm_protocol::SkillAction;
use std::collections::VecDeque;

/// A sent change still unanswered after this many inputs (two seconds) was
/// refused (the server said why in a notice): it is given up on.
const PENDING_INPUTS: u32 = 120;

/// The categories in the order Boundless's skill screen listed them.
pub const CATEGORIES: [Category; 8] = [
    Category::Attributes,
    Category::Mastery,
    Category::Crafting,
    Category::Epic1,
    Category::Endurance,
    Category::Epic2,
    Category::Exploration,
    Category::Epic3,
];

pub fn category_name(c: Category) -> &'static str {
    match c {
        Category::Attributes => "Attributes",
        Category::Mastery => "Mastery",
        Category::Crafting => "Crafting",
        Category::Epic1 => "Epic I",
        Category::Endurance => "Endurance",
        Category::Epic2 => "Epic II",
        Category::Exploration => "Exploration",
        Category::Epic3 => "Epic III",
    }
}

/// A skill change we sent, following input `at`, with what it was ruled on.
#[derive(Clone, Debug)]
struct Pending {
    at: u32,
    action: SkillAction,
    tick: u64,
    at_home: bool,
}

/// The character, as the server says and as we predict it.
pub struct Progress {
    pub tree: SkillTree,
    truth: Character,
    predicted: Character,
    effects: Effects,
    pending: VecDeque<Pending>,
}

/// One of `Character`'s changes, as `ClientMsg::Skill` names it.
pub fn apply(
    c: &mut Character,
    tree: &SkillTree,
    action: &SkillAction,
    tick: u64,
    at_home: bool,
) -> Result<(), Refused> {
    match action {
        SkillAction::Learn(id) => c.learn(tree, id),
        SkillAction::Cleanse(id) => c.cleanse(id),
        SkillAction::ResetPage => c.reset_page(),
        SkillAction::BuyPage => c.buy_page(),
        SkillAction::SwitchPage(i) => c.switch_page(*i, tick, at_home),
        SkillAction::BuyPlots(n) => c.buy_plots(*n as u32),
    }
}

impl Progress {
    pub fn new(tree: SkillTree, character: Character) -> Self {
        let effects = character.effects(&tree);
        Self {
            tree,
            truth: character.clone(),
            predicted: character,
            effects,
            pending: VecDeque::new(),
        }
    }

    /// The character as we predict it: the server's word plus what it has
    /// not answered yet.
    pub fn character(&self) -> &Character {
        &self.predicted
    }

    /// What the active page does, as predicted.
    pub fn effects(&self) -> &Effects {
        &self.effects
    }

    /// A change following input `at`, at server tick `tick` (our estimate)
    /// and standing `at_home` (inside our own claim) or not. Applied to the
    /// prediction if the rules allow it (then it is to be sent); otherwise
    /// why not, and nothing changes.
    pub fn act(&mut self, at: u32, action: SkillAction, tick: u64, at_home: bool) -> Result<(), Refused> {
        apply(&mut self.predicted, &self.tree, &action, tick, at_home)?;
        self.effects = self.predicted.effects(&self.tree);
        self.pending.push_back(Pending {
            at,
            action,
            tick,
            at_home,
        });
        Ok(())
    }

    /// `ServerMsg::Character`: the server's word, after the change that
    /// followed input `at` (0: XP or marks alone). The server answers our
    /// changes in the order sent, so one answered at `at` answers the first
    /// pending there, and any pending before it were refused. Returns the
    /// bodies to set in prediction, oldest first: the server's at `at`, then
    /// each still pending at its own input.
    pub fn server(&mut self, character: Character, at: u32) -> Vec<(u32, Body)> {
        self.truth = character;
        let truth_body = self.truth.effects(&self.tree).body();
        let mut fixes = Vec::new();
        if at > 0 {
            // Refused before it: undone where it was set.
            while self.pending.front().is_some_and(|p| p.at < at) {
                let p = self.pending.pop_front().expect("checked");
                fixes.push((p.at, truth_body));
            }
            if self.pending.front().is_some_and(|p| p.at == at) {
                self.pending.pop_front();
            }
            fixes.push((at, truth_body));
        }
        fixes.extend(self.rebuild());
        fixes
    }

    /// Gives up on changes the server has not answered for
    /// `PENDING_INPUTS` (refused: it said why in a notice) at input
    /// `latest`. Returns the bodies to set, as `server` does.
    pub fn expire(&mut self, latest: u32) -> Vec<(u32, Body)> {
        let truth_body = self.truth.effects(&self.tree).body();
        let mut fixes = Vec::new();
        while self
            .pending
            .front()
            .is_some_and(|p| p.at.saturating_add(PENDING_INPUTS) < latest)
        {
            let p = self.pending.pop_front().expect("checked");
            fixes.push((p.at, truth_body));
        }
        if !fixes.is_empty() {
            fixes.extend(self.rebuild());
        }
        fixes
    }

    /// Whether changes we sent wait for an answer.
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The prediction again from the server's word, with each pending
    /// change re-ruled in order (one the rules now refuse is dropped).
    /// Returns the body after each, at its input.
    fn rebuild(&mut self) -> Vec<(u32, Body)> {
        let mut c = self.truth.clone();
        let mut bodies = Vec::new();
        let mut kept = VecDeque::new();
        for p in self.pending.drain(..) {
            // `Character`'s changes refuse before they change anything.
            let ok = apply(&mut c, &self.tree, &p.action, p.tick, p.at_home).is_ok();
            bodies.push((p.at, c.effects(&self.tree).body()));
            if ok {
                kept.push_back(p);
            }
        }
        self.pending = kept;
        self.predicted = c;
        self.effects = self.predicted.effects(&self.tree);
        bodies
    }

    /// Minutes until pages can be switched away from home at server tick
    /// `tick`, if a switch is cooling down.
    pub fn switch_wait(&self, tick: u64) -> Option<u64> {
        let mut c = self.predicted.clone();
        let other = if c.active == 0 { 1 } else { 0 };
        if c.pages.len() < 2 {
            c.pages.push(Default::default());
        }
        match c.switch_page(other, tick, false) {
            Err(Refused::Cooldown(m)) => Some(m),
            _ => None,
        }
    }
}

/// How a skill is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Can be learned (or is part learned and can take more).
    Open,
    /// At its highest rank.
    Full,
    /// Shown dimmed: it comes later, its category is locked, or its either/or
    /// partner is on this page.
    Greyed,
}

/// One skill on the screen.
#[derive(Clone, Debug)]
pub struct SkillCard<'a> {
    pub def: &'a SkillDef,
    pub rank: u8,
    pub look: Look,
    /// Whether a left click learns a rank.
    pub learnable: bool,
    /// Whether a right click takes one back.
    pub cleansable: bool,
    /// Why it cannot be learned, in plain words (None when it can).
    pub why: Option<String>,
    /// Its either/or partners' names.
    pub partners: Vec<String>,
}

/// The skills of `category` as the screen shows them for `c`.
pub fn cards<'a>(tree: &'a SkillTree, c: &Character, category: Category) -> Vec<SkillCard<'a>> {
    tree.skills
        .iter()
        .filter(|s| s.category == category)
        .map(|def| {
            let rank = c.page().rank(&def.id);
            let learn = c.clone().learn(tree, &def.id);
            let cleansable = c.clone().cleanse(&def.id).is_ok();
            let look = match &learn {
                Ok(()) => Look::Open,
                Err(Refused::MaxRank) => Look::Full,
                Err(Refused::ComesLater | Refused::Locked(_) | Refused::Excluded(_)) => Look::Greyed,
                Err(_) => Look::Open,
            };
            let why = match learn {
                Ok(()) => None,
                Err(Refused::ComesLater) => Some("Comes later".to_string()),
                Err(e) => Some(e.to_string()),
            };
            let partners = def
                .excludes
                .iter()
                .map(|x| tree.get(x).map_or(x.clone(), |d| d.name.clone()))
                .collect();
            SkillCard {
                def,
                rank,
                look,
                learnable: why.is_none(),
                cleansable,
                why,
                partners,
            }
        })
        .collect()
}

/// Points spent in `category` on the active page.
pub fn spent_in(tree: &SkillTree, c: &Character, category: Category) -> u32 {
    c.page()
        .ranks
        .iter()
        .filter_map(|(id, r)| {
            tree.get(id)
                .filter(|s| s.category == category)
                .map(|s| *r as u32 * s.cost as u32)
        })
        .sum()
}

/// Why a recipe cannot be made with these skills ("Needs Toolmaking 1"),
/// if it cannot.
pub fn recipe_gate(tree: &SkillTree, effects: &Effects, recipe: &Recipe) -> Option<String> {
    let gate = recipe.skill.as_ref()?;
    if effects.crafting(&gate.name) >= gate.level {
        return None;
    }
    let name = tree
        .get(&gate.name)
        .map_or_else(|| bm_core::items::title_case(&gate.name), |s| s.name.clone());
    Some(format!("Needs {name} {}", gate.level))
}

/// Light level of the light-source epic, as a held torch's (milestone 12,
/// a **guess**: Boundless never published its radius).
pub const INNER_LIGHT: u8 = 10;

/// The light we carry: the item in hand's, or the light-source epic's when
/// that is brighter.
pub fn carried_light(held: u8, effects: &Effects) -> u8 {
    if effects.light_epic {
        held.max(INNER_LIGHT)
    } else {
        held
    }
}

/// Reach (milestone 12): the server's base plus what Control adds
/// (`Effects::reach` counts from the untrained 4).
pub fn reach(base: f64, effects: &Effects) -> f64 {
    base + effects.reach_bonus()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::skills::xp_for_level;

    fn at_level(level: u32) -> Character {
        Character {
            xp: xp_for_level(level),
            ..Default::default()
        }
    }

    fn card<'a>(cards: &'a [SkillCard], id: &str) -> &'a SkillCard<'a> {
        cards.iter().find(|c| c.def.id == id).unwrap()
    }

    #[test]
    fn the_screen_says_what_is_clickable_and_why_not() {
        let t = SkillTree::builtin();
        let mut c = at_level(8);
        c.learn(&t, "hammer_epic").unwrap();
        let attrs = cards(&t, &c, Category::Attributes);
        // Open, learnable; and Insight comes later.
        assert!(card(&attrs, "power").learnable);
        assert_eq!(card(&attrs, "power").look, Look::Open);
        let v = card(&attrs, "intelligence");
        assert_eq!((v.look, v.learnable), (Look::Greyed, false));
        assert_eq!(v.why.as_deref(), Some("Comes later"));
        // The partner of a learned epic is greyed and says why.
        let epics = cards(&t, &c, Category::Epic1);
        let g = card(&epics, "shovel_axe_epic");
        assert_eq!(g.look, Look::Greyed);
        assert!(!g.learnable);
        assert_eq!(g.partners, vec!["Rockbreaker".to_string()]);
        assert!(g.why.as_deref().unwrap().contains("Rockbreaker"));
        // The learned one can be taken back (free below level 20).
        assert!(card(&epics, "hammer_epic").cleansable);
        assert_eq!(card(&epics, "hammer_epic").look, Look::Full);
        assert!(!card(&attrs, "power").cleansable, "nothing spent there");
        // A locked category greys its skills with the level.
        let later = cards(&t, &c, Category::Epic2);
        let j = card(&later, "double_jump_epic");
        assert_eq!(j.look, Look::Greyed);
        assert_eq!(j.why.as_deref(), Some("That unlocks at level 10"));
        // Out of points: not clickable, but not greyed.
        let mut poor = at_level(2);
        for _ in 0..4 {
            poor.learn(&t, "control").unwrap();
        }
        let attrs = cards(&t, &poor, Category::Attributes);
        let p = card(&attrs, "power");
        assert_eq!((p.look, p.learnable), (Look::Open, false));
        assert_eq!(p.why.as_deref(), Some("You have no skill points left"));
        assert_eq!(spent_in(&t, &poor, Category::Attributes), 4);
    }

    #[test]
    fn predicted_changes_settle_with_the_server() {
        let t = SkillTree::builtin();
        let c = at_level(10);
        let mut p = Progress::new(t.clone(), c.clone());
        p.act(5, SkillAction::Learn("agility".into()), 0, false).unwrap();
        p.act(6, SkillAction::Learn("agility".into()), 0, false).unwrap();
        assert_eq!(p.character().page().rank("agility"), 2);
        // Something we would refuse is not predicted.
        assert_eq!(
            p.act(7, SkillAction::Learn("intelligence".into()), 0, false),
            Err(Refused::ComesLater)
        );
        // The server answers the first: both still show, the second pending.
        let mut one = c.clone();
        one.learn(&t, "agility").unwrap();
        let fixes = p.server(one.clone(), 5);
        assert_eq!(p.character().page().rank("agility"), 2);
        assert_eq!(fixes[0], (5, one.effects(&t).body()));
        assert_eq!(fixes[1].0, 6);
        assert!((fixes[1].1.run_bonus - 0.6).abs() < 1e-9);
        // XP alone changes nothing pending.
        let mut more = one.clone();
        more.add_xp(10);
        assert!(p.server(more.clone(), 0).iter().all(|f| f.0 == 6));
        assert_eq!(p.character().xp, more.xp);
        // The second is refused (a notice, no Character): given up on later,
        // its body undone where it was set.
        assert!(p.expire(100).is_empty());
        let fixes = p.expire(200);
        assert_eq!(p.character().page().rank("agility"), 1);
        assert!(!p.waiting());
        assert_eq!(fixes, vec![(6, one.effects(&t).body())]);
    }

    #[test]
    fn an_answer_after_a_refusal_drops_the_refused() {
        let t = SkillTree::builtin();
        let c = at_level(10);
        let mut p = Progress::new(t.clone(), c.clone());
        p.act(5, SkillAction::Learn("power".into()), 0, false).unwrap();
        p.act(9, SkillAction::Learn("luck".into()), 0, false).unwrap();
        // The server refused the first and accepted the second.
        let mut truth = c.clone();
        truth.learn(&t, "luck").unwrap();
        p.server(truth, 9);
        assert!(!p.waiting());
        assert_eq!(p.character().page().rank("power"), 0);
        assert_eq!(p.character().page().rank("luck"), 1);
    }

    #[test]
    fn the_switch_cooldown_is_shown_while_it_runs() {
        let t = SkillTree::builtin();
        let mut c = at_level(30);
        c.marks = 600;
        c.buy_page().unwrap();
        let mut p = Progress::new(t, c);
        assert_eq!(p.switch_wait(1000), None);
        p.act(3, SkillAction::SwitchPage(1), 1000, false).unwrap();
        assert_eq!(p.switch_wait(1000 + 60 * 60), Some(14));
        assert_eq!(p.switch_wait(1000 + bm_core::skills::SWITCH_COOLDOWN_TICKS), None);
    }

    #[test]
    fn gated_recipes_name_the_skill() {
        let t = SkillTree::builtin();
        let blocks = bm_core::BlockRegistry::builtin();
        let items = bm_core::ItemRegistry::builtin(&blocks);
        let book = bm_core::RecipeBook::builtin(&items);
        let gated = book
            .recipes
            .iter()
            .find(|r| r.skill.as_ref().is_some_and(|g| g.name == "tool_crafting"))
            .expect("a recipe needs tool crafting");
        let free = book.recipes.iter().find(|r| r.skill.is_none()).unwrap();
        let mut c = at_level(10);
        let e = c.effects(&t);
        assert_eq!(recipe_gate(&t, &e, gated).as_deref(), Some("Needs Toolmaking 1"));
        assert_eq!(recipe_gate(&t, &e, free), None);
        c.learn(&t, "tool_crafting").unwrap();
        assert_eq!(recipe_gate(&t, &c.effects(&t), gated), None);
    }

    #[test]
    fn reach_and_light_follow_the_page() {
        let t = SkillTree::builtin();
        let mut c = at_level(20);
        assert_eq!(reach(4.0, &c.effects(&t)), 4.0);
        for _ in 0..3 {
            c.learn(&t, "control").unwrap();
        }
        assert_eq!(reach(4.0, &c.effects(&t)), 6.25);
        assert_eq!(carried_light(0, &c.effects(&t)), 0);
        c.learn(&t, "light_epic").unwrap();
        assert_eq!(carried_light(0, &c.effects(&t)), INNER_LIGHT);
        assert_eq!(carried_light(14, &c.effects(&t)), 14);
    }
}
