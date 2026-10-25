//! Recipes: data in `assets/recipes.toml`, referring to items by name.

use crate::block::Station;
use crate::inventory::Inventory;
use crate::items::{ItemError, ItemId, ItemRegistry};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub output: ItemId,
    pub output_count: u16,
    pub inputs: Vec<(ItemId, u16)>,
    pub station: Station,
    /// Seconds per craft at a station (hand recipes are instant).
    pub seconds: f32,
    /// Heat one craft burns at a furnace (milestone 11; 0 elsewhere).
    #[serde(default)]
    pub heat: u16,
    /// XP each craft gives (milestone 12).
    #[serde(default)]
    pub xp: u16,
    /// Spark one craft draws from the machine's grid (milestone 15; 0: none).
    #[serde(default)]
    pub spark: u32,
    /// Wear one craft puts on the machine (milestone 15; 0 at the hand, the
    /// crafting table and furnaces, which do not wear).
    #[serde(default)]
    pub wear: u16,
    /// The crafting skill and level it needs (`skills::Effects::crafting`).
    #[serde(default)]
    pub skill: Option<SkillGate>,
    /// Power the machine needs from its coils (milestone 18; 0: none).
    /// Power above it makes the craft quicker and cheaper
    /// (`spark::power_scale`).
    #[serde(default)]
    pub power: u32,
}

/// A recipe's crafting-skill requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillGate {
    pub name: String,
    pub level: u8,
}

impl Recipe {
    /// Whether `times` crafts' inputs are all present.
    pub fn affordable(&self, inv: &Inventory, times: u16) -> bool {
        self.inputs
            .iter()
            .all(|(item, n)| inv.count(*item) >= *n as u32 * times as u32)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecipeError {
    #[error("recipe catalogue is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("recipe for `{0}`: {1}")]
    Item(String, ItemError),
    #[error("recipe for `{0}` makes nothing, or needs nothing at a station")]
    Empty(String),
}

#[derive(Deserialize)]
struct RecipeFile {
    #[serde(default)]
    recipe: Vec<RecipeEntry>,
}

#[derive(Deserialize)]
struct RecipeEntry {
    output: String,
    #[serde(default = "one")]
    count: u16,
    inputs: Vec<(String, u16)>,
    #[serde(default = "hand")]
    station: Station,
    #[serde(default)]
    seconds: f32,
    #[serde(default)]
    heat: u16,
    #[serde(default)]
    xp: u16,
    #[serde(default)]
    skill: Option<SkillGate>,
    #[serde(default)]
    spark: u32,
    #[serde(default)]
    wear: u16,
    #[serde(default)]
    power: u32,
}

fn one() -> u16 {
    1
}

fn hand() -> Station {
    Station::Hand
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeBook {
    pub recipes: Vec<Recipe>,
}

impl RecipeBook {
    pub fn from_toml(source: &str, items: &ItemRegistry) -> Result<Self, RecipeError> {
        let file: RecipeFile = toml::from_str(source)?;
        let mut recipes = Vec::new();
        for e in file.recipe {
            let id = |n: &str| items.id(n).map_err(|err| RecipeError::Item(e.output.clone(), err));
            let output = id(&e.output)?;
            let inputs = e
                .inputs
                .iter()
                .map(|(n, c)| Ok((id(n)?, *c)))
                .collect::<Result<Vec<_>, RecipeError>>()?;
            // Only a hand recipe may need nothing (the hand stone, as
            // Boundless's starter tool).
            if (inputs.is_empty() && e.station != Station::Hand) || e.count == 0 || inputs.iter().any(|(_, c)| *c == 0)
            {
                return Err(RecipeError::Empty(e.output));
            }
            recipes.push(Recipe {
                output,
                output_count: e.count,
                inputs,
                station: e.station,
                seconds: if e.station == Station::Hand {
                    0.0
                } else {
                    e.seconds.max(0.0)
                },
                heat: if e.station == Station::Furnace { e.heat } else { 0 },
                xp: e.xp,
                skill: e.skill.clone(),
                spark: e.spark,
                wear: if crate::spark::wears(e.station) { e.wear } else { 0 },
                power: e.power,
            });
        }
        Ok(Self { recipes })
    }

    pub fn builtin(items: &ItemRegistry) -> Self {
        Self::from_toml(include_str!("../../../assets/recipes.toml"), items).expect("assets/recipes.toml is valid")
    }

    pub fn get(&self, index: u16) -> Option<&Recipe> {
        self.recipes.get(index as usize)
    }

    /// Recipes made at `station`, with their indices.
    pub fn at(&self, station: Station) -> impl Iterator<Item = (u16, &Recipe)> {
        self.recipes
            .iter()
            .enumerate()
            .filter(move |(_, r)| r.station == station)
            .map(|(i, r)| (i as u16, r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockRegistry;

    #[test]
    fn builtin_recipes_load_and_reach_copper_tools() {
        let blocks = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let book = RecipeBook::builtin(&items);
        let makes = |name: &str| {
            let id = items.id(name).unwrap();
            book.recipes
                .iter()
                .find(|r| r.output == id)
                .unwrap_or_else(|| panic!("no recipe for {name}"))
        };
        // Boundless's chain (milestone 11).
        assert_eq!(makes("timber").station, Station::Hand);
        assert_eq!(makes("crafting_table").station, Station::Hand);
        assert_eq!(makes("stones").station, Station::CraftingTable);
        assert_eq!(makes("stone_hammer").station, Station::CraftingTable);
        assert_eq!(makes("workbench").station, Station::CraftingTable);
        assert_eq!(makes("workbench").output_count, 4);
        assert_eq!(makes("copper_ingot").station, Station::Furnace);
        assert_eq!(makes("copper_ingot").heat, 100);
        assert_eq!(makes("sackcloth").station, Station::Extractor);
        assert_eq!(makes("copper_hammer").station, Station::Workbench);
        assert!(makes("claim_stone").inputs.len() >= 2);

        // Everything a recipe makes can be reached from what the world
        // yields (block drops and bonus drops) and recipes needing nothing.
        let mut have: std::collections::HashSet<ItemId> = std::collections::HashSet::new();
        for (id, _) in blocks.iter() {
            have.extend(items.drop_of(id));
            have.extend(items.bonus_of(id).iter().map(|(i, _)| *i));
        }
        // Creature drops (milestone 13): bone for glue, tallow for torches.
        for (_, c) in crate::creatures::CreatureKinds::builtin().iter() {
            for t in &c.tiers {
                have.extend(t.drops.iter().map(|d| items.id(&d.item).unwrap()));
            }
        }
        loop {
            let before = have.len();
            // A bucket fills at water (milestone 16).
            if have.contains(&items.id("bucket").unwrap()) {
                have.insert(items.id("water_bucket").unwrap());
            }
            for r in &book.recipes {
                if r.inputs.iter().all(|(i, _)| have.contains(i)) {
                    have.insert(r.output);
                }
            }
            if have.len() == before {
                break;
            }
        }
        for r in &book.recipes {
            let name = &items.get(r.output).unwrap().name;
            assert!(have.contains(&r.output), "{name} cannot be reached");
        }
        for name in [
            "copper_hammer",
            "iron_grapple",
            "sackcloth",
            "glue",
            "extractor",
            "torch",
            "tallow_torch",
            "iron_sling",
            "copper_bombs",
        ] {
            assert!(have.contains(&items.id(name).unwrap()), "{name}");
        }
    }

    #[test]
    fn affordability_counts_every_input() {
        let blocks = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let book = RecipeBook::builtin(&items);
        let timber = items.id("timber").unwrap();
        let r = book.recipes.iter().find(|r| r.output == timber).unwrap();
        let mut inv = Inventory::default();
        assert!(!r.affordable(&inv, 1));
        inv.add(&items, items.id("trunk").unwrap(), 2);
        assert!(r.affordable(&inv, 2));
        assert!(!r.affordable(&inv, 3));
    }

    #[test]
    fn unknown_items_are_refused() {
        let blocks = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let src = "[[recipe]]\noutput = \"unobtainium\"\ninputs = [[\"stone\", 1]]\n";
        assert!(matches!(RecipeBook::from_toml(src, &items), Err(RecipeError::Item(..))));
    }
}
