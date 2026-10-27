//! Skills and progression in play (milestone 12,
//! `docs/milestones/12-skills-and-progression.md`): XP from breaking,
//! placing and crafting, levels with their points and marks, and the skill
//! actions, applied on the input timeline so the body a page gives changes
//! at the same input on client and server. The rules themselves are
//! `bm_core::skills`; this applies them.

use super::Game;
use bm_core::block::{BlockDef, Health};
use bm_core::mining::tier_health;
use bm_core::skills::{Character, Effect, PAGE_CAP, SkillTree, marks_for_level, points_at};
use bm_core::{BlockPos, Recipe};
use bm_protocol::{ServerMsg, SkillAction};

/// A character whose XP changed is sent at most this often (4 a second):
/// an area tool breaking nine blocks a swing sends one update, not nine.
pub(super) const CHARACTER_EVERY: u64 = 15;

/// What the light-source epic gives off, like a held torch: **a guess**
/// (Boundless never published its radius; spec, Epics).
pub(super) const INNER_LIGHT: u8 = 10;

/// Sky light (`DayCycle::sky_light`) at or above which the light-source epic
/// goes out under open sky, as Boundless's did in bright places.
const BRIGHT_SKY: f32 = 0.5;

/// XP for breaking `def` on a planet of `tier`: its `mine_xp`, and a
/// gathering object's scaled by the tier's health multiplier (a **guess**
/// at Boundless's per-tier numbers, spec XP sources).
pub(super) fn break_xp(def: &BlockDef, tier: u8) -> u64 {
    let xp = def.mine_xp as f64;
    if def.health == Health::Plant {
        (xp * tier_health(tier)).round() as u64
    } else {
        xp as u64
    }
}

/// A saved character made safe against today's tree: unknown or unbuyable
/// skills go, ranks are capped, and if what is spent no longer fits the
/// points (the tables changed) every page is cleared so it can be spent
/// again, rather than keeping ranks the level cannot pay for.
pub(super) fn sanitize(mut c: Character, tree: &SkillTree) -> Character {
    for page in &mut c.pages {
        page.ranks.retain(|id, r| match tree.get(id) {
            Some(s) if s.available() && *r > 0 => {
                *r = (*r).min(s.ranks);
                true
            }
            _ => false,
        });
    }
    let over_page = c.pages.iter().any(|p| p.spent(tree) > PAGE_CAP as u32);
    if over_page || c.spent(tree) > points_at(c.level()) {
        tracing::warn!("a character spent more points than it has; its pages are cleared");
        for p in &mut c.pages {
            p.ranks.clear();
        }
    }
    c
}

/// "Toolmaking": a skill's name from its id, or its id if the tree lacks it.
fn skill_name(tree: &SkillTree, id: &str) -> String {
    tree.get(id).map_or_else(|| id.to_string(), |s| s.name.clone())
}

/// The name of the first skill with `effect` ("Waywise" for the portal
/// epic).
pub(super) fn effect_name(tree: &SkillTree, effect: Effect) -> String {
    tree.skills
        .iter()
        .find(|s| s.effect == effect)
        .map_or_else(|| format!("{effect:?}"), |s| s.name.clone())
}

impl Game {
    /// Adds XP to a player's character; a level gained is told at once
    /// (with the character), otherwise the change goes out with the next
    /// batch.
    pub(super) fn add_xp(&mut self, id: u32, xp: u64) {
        if xp == 0 {
            return;
        }
        let Some(p) = self.players.get_mut(&id) else { return };
        // Studious food: 10 to 100% more (milestone 16).
        let xp = xp * (100 + p.state.xp_bonus_percent() as u64) / 100;
        let before = p.character.level();
        let gained = p.character.add_xp(xp);
        p.character_dirty = true;
        if gained == 0 {
            return;
        }
        for level in before + 1..=before + gained {
            let points = points_at(level) - points_at(level - 1);
            let marks = marks_for_level(level);
            p.out.send(&ServerMsg::Notice {
                text: format!("You reached level {level}: {points} skill points and {marks} marks"),
            });
        }
        tracing::info!("{} reached level {}", p.name, before + gained);
        let level = (before + gained) as u64;
        p.out.send(&ServerMsg::Character {
            character: p.character.clone(),
            at: 0,
        });
        p.character_dirty = false;
        self.count_at_least(id, "level", level);
    }

    /// Sends every character whose XP changed since it was last sent.
    pub(super) fn send_dirty_characters(&mut self) {
        for p in self.players.values_mut().filter(|p| p.character_dirty) {
            p.character_dirty = false;
            p.out.send(&ServerMsg::Character {
                character: p.character.clone(),
                at: 0,
            });
        }
    }

    /// Whether a player stands inside a claim of their own (switching pages
    /// is free there).
    fn at_home(&self, id: u32) -> bool {
        let Some(p) = self.players.get(&id) else { return false };
        let planet = &self.planets[p.planet];
        let shape = planet.world.shape;
        let feet = BlockPos::containing(p.state.pos.x, p.state.pos.y, p.state.pos.z);
        shape.contains_y(feet.y)
            && planet
                .claims
                .at_block(shape.wrap_block(feet))
                .is_some_and(|c| c.owner == p.account)
    }

    /// A skill action, right after input `at` was simulated: on success the
    /// body changes from the next input, as the client predicts, and the
    /// character goes back with `at`; refused, a notice says why and the
    /// character goes back so the client can undo its guess.
    pub(super) fn skill(&mut self, id: u32, at: u32, action: SkillAction) {
        let at_home = matches!(action, SkillAction::SwitchPage(_)) && self.at_home(id);
        let now = self.tick;
        let tree = &self.skills;
        let Some(p) = self.players.get_mut(&id) else { return };
        let mut c = p.character.clone();
        // Skill points a learn spends (milestone 24 feats).
        let spends = match &action {
            SkillAction::Learn(skill) => tree.get(skill).map_or(0, |d| d.cost as u64),
            _ => 0,
        };
        let result = match &action {
            SkillAction::Learn(skill) => c.learn(tree, skill),
            SkillAction::Cleanse(skill) => c.cleanse(skill),
            SkillAction::ResetPage => c.reset_page(),
            SkillAction::BuyPage => c.buy_page(),
            SkillAction::SwitchPage(i) => c.switch_page(*i, now, at_home),
            SkillAction::BuyPlots(n) => c.buy_plots(*n as u32),
        };
        match result {
            Ok(()) => {
                p.effects = c.effects(tree);
                // Creative mode is the player's switch, not a skill's.
                p.state.body = bm_core::movement::Body {
                    creative: p.state.body.creative,
                    ..p.effects.body()
                };
                // Less max health (a page without Toughness): never below 1.
                if !p.state.down {
                    p.state.hurt = p.state.hurt.min(p.state.max_health() - 1);
                }
                p.character = c;
                p.character_dirty = false;
                tracing::debug!("{} skill action {action:?} at input {at}", p.name);
                p.out.send(&ServerMsg::Character {
                    character: p.character.clone(),
                    at,
                });
                self.count(id, "skill_points", spends);
            }
            Err(e) => self.skill_refused(id, at, e.to_string()),
        }
    }

    pub(super) fn skill_refused(&mut self, id: u32, at: u32, text: String) {
        let Some(p) = self.players.get_mut(&id) else { return };
        p.out.send(&ServerMsg::Notice { text });
        p.character_dirty = false;
        p.out.send(&ServerMsg::Character {
            character: p.character.clone(),
            at,
        });
    }

    /// Why this player may not craft `recipe` yet ("Needs Toolmaking 1"),
    /// from the crafting levels of their active page.
    pub(super) fn craft_gate(&self, id: u32, recipe: &Recipe) -> Option<String> {
        let gate = recipe.skill.as_ref()?;
        let p = self.players.get(&id)?;
        (p.effects.crafting(&gate.name) < gate.level)
            .then(|| format!("Needs {} {}", skill_name(&self.skills, &gate.name), gate.level))
    }

    /// The light-source epic shines unless the player stands in daylight
    /// under open sky: no opaque block anywhere above their head (the
    /// server keeps no light map; this is its cheap stand-in for "bright").
    pub(super) fn update_inner_light(&mut self) {
        let tick = self.tick;
        let m = self.movement;
        let lit: Vec<(u32, bool)> = self
            .players
            .iter()
            .map(|(id, p)| {
                // A Glowing brew (milestone 23 part 2) shines like the epic.
                if !p.effects.light_epic && p.state.brew_light() == 0 {
                    return (*id, false);
                }
                let planet = &self.planets[p.planet];
                let world = &planet.world;
                let eye = p.state.eye(&m);
                let head = world.shape.wrap_block(BlockPos::containing(eye.x, eye.y, eye.z));
                let day = planet.day.sky_light(tick as f64) >= BRIGHT_SKY;
                let open = (head.y + 1..).take_while(|&y| world.shape.contains_y(y)).all(|y| {
                    world
                        .block(BlockPos::new(head.x, y, head.z))
                        .is_none_or(|b| !world.flags.opaque(b))
                });
                (*id, !(day && open))
            })
            .collect();
        for (id, on) in lit {
            if let Some(p) = self.players.get_mut(&id) {
                p.inner_light = on;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::BlockRegistry;
    use bm_core::skills::{Page, xp_for_level};

    #[test]
    fn gathering_xp_scales_with_the_tier() {
        let b = BlockRegistry::builtin();
        let plant = b
            .defs()
            .iter()
            .find(|d| d.health == Health::Plant && d.mine_xp > 0)
            .unwrap();
        let x = plant.mine_xp as u64;
        assert_eq!(break_xp(plant, 1), x);
        assert_eq!(break_xp(plant, 4), 2 * x);
        assert_eq!(break_xp(plant, 8), 6 * x);
        let rock = b.get(b.id("stone").unwrap());
        assert_eq!(break_xp(rock, 6), rock.mine_xp as u64, "rock is not scaled");
    }

    #[test]
    fn saved_characters_are_made_safe() {
        let t = SkillTree::builtin();
        let mut page = Page::default();
        page.ranks.insert("power".into(), 50);
        page.ranks.insert("no_such_skill".into(), 1);
        page.ranks.insert("augment_crafting".into(), 1);
        let c = Character {
            xp: xp_for_level(50),
            pages: vec![page],
            ..Default::default()
        };
        let c = sanitize(c, &t);
        assert_eq!(c.pages[0].ranks.len(), 1);
        assert_eq!(c.pages[0].rank("power"), 8, "capped at its ranks");
        // More spent than the level pays for: cleared.
        let low = Character { xp: 0, ..c.clone() };
        assert!(sanitize(low, &t).pages[0].ranks.is_empty());
    }
}
