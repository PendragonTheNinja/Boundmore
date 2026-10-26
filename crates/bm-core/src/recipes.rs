//! Recipes: data in `assets/recipes.toml`, referring to items by name.
//!
//! An input names an item or a group (milestone 19, as Boundless's "Any
//! Rock"): any member of a group serves, mixed members too, taken in the
//! group's order. A recipe's inputs never share an item, so taking each
//! input greedily, member by member, always succeeds when the totals do.

use crate::block::Station;
use crate::inventory::Inventory;
use crate::items::{ItemError, ItemId, ItemRegistry};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub output: ItemId,
    pub output_count: u16,
    pub inputs: Vec<Input>,
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
    /// The input whose colour the output takes (milestone 22, Boundless's
    /// `tintTakenFrom`): crafting picks that colour, that input is taken
    /// only from stacks of it, and the output has it. None: natural output.
    #[serde(default)]
    pub tint: Option<u8>,
}

/// A recipe's crafting-skill requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillGate {
    pub name: String,
    pub level: u8,
}

/// One input of a recipe: an item, or a group any of whose members serve.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    /// The items that serve, in the order they are taken (one for a plain
    /// item).
    pub items: Vec<ItemId>,
    pub count: u16,
    /// A group's name in plain words ("Any Rock"); None for one item.
    pub group: Option<String>,
}

impl Input {
    /// `count` of one item.
    pub fn item(item: ItemId, count: u16) -> Self {
        Self {
            items: vec![item],
            count,
            group: None,
        }
    }

    /// Whether `item` serves this input.
    pub fn accepts(&self, item: ItemId) -> bool {
        self.items.contains(&item)
    }

    /// How many of its items `inv` holds, all members together.
    pub fn have(&self, inv: &Inventory) -> u32 {
        self.items.iter().map(|i| inv.count(*i)).sum()
    }
}

/// The items one craft took, in input order.
pub type Craft = Vec<(ItemId, u16)>;

impl Recipe {
    /// Whether `times` crafts' inputs are all present.
    pub fn affordable(&self, inv: &Inventory, times: u16) -> bool {
        self.max_crafts(inv) >= times as u32
    }

    /// How many crafts `inv` holds the inputs for (inputs never share an
    /// item, so each input's members are simply added up).
    pub fn max_crafts(&self, inv: &Inventory) -> u32 {
        self.inputs
            .iter()
            .map(|i| i.have(inv) / i.count.max(1) as u32)
            .min()
            .unwrap_or(u32::MAX)
    }

    /// The items `times` crafts would take from `inv`, craft by craft: each
    /// input from its members in order, as many of the first as there are,
    /// then the next. None when short.
    pub fn plan(&self, inv: &Inventory, times: u16) -> Option<Vec<Craft>> {
        let mut left: Vec<(ItemId, u32)> = Vec::new();
        let mut crafts = Vec::with_capacity(times as usize);
        for _ in 0..times {
            let mut craft = Craft::new();
            for input in &self.inputs {
                let mut need = input.count;
                for &item in &input.items {
                    if need == 0 {
                        break;
                    }
                    let k = match left.iter().position(|(i, _)| *i == item) {
                        Some(k) => k,
                        None => {
                            left.push((item, inv.count(item)));
                            left.len() - 1
                        }
                    };
                    let n = left[k].1.min(need as u32) as u16;
                    if n > 0 {
                        left[k].1 -= n as u32;
                        need -= n;
                        craft.push((item, n));
                    }
                }
                if need > 0 {
                    return None;
                }
            }
            crafts.push(craft);
        }
        Some(crafts)
    }

    /// Whether `craft` is exactly one craft's inputs: every input's count
    /// made up of items it accepts, and nothing else.
    pub fn matches(&self, craft: &[(ItemId, u16)]) -> bool {
        craft
            .iter()
            .all(|(i, _)| self.inputs.iter().any(|input| input.accepts(*i)))
            && self.inputs.iter().all(|input| {
                craft
                    .iter()
                    .filter(|(i, _)| input.accepts(*i))
                    .map(|(_, n)| *n as u32)
                    .sum::<u32>()
                    == input.count as u32
            })
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
    #[error("recipe for `{0}` names `{1}` in two of its inputs")]
    Overlap(String, String),
    #[error("recipe for `{0}` takes its colour from an input it does not have")]
    Tint(String),
    #[error("recipe group `{0}` is empty, repeats a member, shares an item's name or is listed twice")]
    BadGroup(String),
}

#[derive(Deserialize)]
struct RecipeFile {
    #[serde(default)]
    group: Vec<GroupEntry>,
    #[serde(default)]
    recipe: Vec<RecipeEntry>,
}

/// A recipe group: `name` is what recipes call it, `label` what players see
/// (by default the name in words: `any_rock` is "Any Rock").
#[derive(Deserialize)]
struct GroupEntry {
    name: String,
    #[serde(default)]
    label: Option<String>,
    items: Vec<String>,
}

/// `any_rock` as "Any Rock".
fn words(name: &str) -> String {
    name.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    #[serde(default)]
    tint: Option<u8>,
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
        let mut groups: std::collections::HashMap<String, (String, Vec<ItemId>)> = std::collections::HashMap::new();
        for g in file.group {
            let members = g
                .items
                .iter()
                .map(|n| items.id(n).map_err(|err| RecipeError::Item(g.name.clone(), err)))
                .collect::<Result<Vec<_>, RecipeError>>()?;
            let repeats = members.iter().enumerate().any(|(k, m)| members[..k].contains(m));
            if members.is_empty() || repeats || items.id(&g.name).is_ok() || groups.contains_key(&g.name) {
                return Err(RecipeError::BadGroup(g.name));
            }
            let label = g.label.unwrap_or_else(|| words(&g.name));
            groups.insert(g.name, (label, members));
        }
        let mut recipes = Vec::new();
        for e in file.recipe {
            let id = |n: &str| items.id(n).map_err(|err| RecipeError::Item(e.output.clone(), err));
            let output = id(&e.output)?;
            let inputs = e
                .inputs
                .iter()
                .map(|(n, c)| match groups.get(n) {
                    Some((label, members)) => Ok(Input {
                        items: members.clone(),
                        count: *c,
                        group: Some(label.clone()),
                    }),
                    None => Ok(Input::item(id(n)?, *c)),
                })
                .collect::<Result<Vec<_>, RecipeError>>()?;
            // Only a hand recipe may need nothing (the hand stone, as
            // Boundless's starter tool).
            if (inputs.is_empty() && e.station != Station::Hand) || e.count == 0 || inputs.iter().any(|i| i.count == 0)
            {
                return Err(RecipeError::Empty(e.output));
            }
            // No item serves two inputs: taking stays a simple sum.
            for (k, input) in inputs.iter().enumerate() {
                if let Some(i) = input.items.iter().find(|i| inputs[..k].iter().any(|o| o.accepts(**i))) {
                    let name = items.get(*i).map(|d| d.name.clone()).unwrap_or_default();
                    return Err(RecipeError::Overlap(e.output, name));
                }
            }
            if e.tint.is_some_and(|t| t as usize >= inputs.len()) {
                return Err(RecipeError::Tint(e.output));
            }
            recipes.push(Recipe {
                tint: e.tint,
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
                for d in &t.drops {
                    if d.item == crate::creatures::ELEMENTAL_SHARD {
                        // Elemental creatures drop their element's shard.
                        have.extend(crate::creatures::Element::ALL.map(|e| items.id(e.shard()).unwrap()));
                    } else {
                        have.insert(items.id(&d.item).unwrap());
                    }
                }
            }
        }
        loop {
            let before = have.len();
            // A bucket fills at water (milestone 16).
            if have.contains(&items.id("bucket").unwrap()) {
                have.insert(items.id("water_bucket").unwrap());
            }
            for r in &book.recipes {
                if r.inputs
                    .iter()
                    .all(|input| input.items.iter().any(|i| have.contains(i)))
                {
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
        // Each trunk makes its own timber (milestone 22): pale does not count.
        inv.add(&items, items.id("pale_trunk").unwrap(), 1);
        assert!(r.affordable(&inv, 2));
        assert!(!r.affordable(&inv, 3));
        assert_eq!(r.tint, Some(0), "timber takes the trunk's colour");
    }

    #[test]
    fn unknown_items_are_refused() {
        let blocks = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let src = "[[recipe]]\noutput = \"unobtainium\"\ninputs = [[\"stone\", 1]]\n";
        assert!(matches!(RecipeBook::from_toml(src, &items), Err(RecipeError::Item(..))));
    }

    #[test]
    fn a_group_input_takes_mixed_members_in_group_order() {
        let blocks = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        // (Milestone 22 split the built-in any-rock stones recipe by rock.)
        let src = "[[group]]\nname = \"any_rock\"\nitems = [\"stone\", \"basalt\", \"limestone\"]\n\
                   [[recipe]]\noutput = \"stones\"\ninputs = [[\"any_rock\", 1]]\n";
        let book = RecipeBook::from_toml(src, &items).unwrap();
        let id = |n: &str| items.id(n).unwrap();
        let stones = &book.recipes[0];
        assert_eq!(stones.inputs[0].group.as_deref(), Some("Any Rock"));
        assert_eq!(stones.inputs[0].items, vec![id("stone"), id("basalt"), id("limestone")]);
        let mut inv = Inventory::default();
        inv.add(&items, id("limestone"), 1);
        inv.add(&items, id("basalt"), 2);
        inv.add(&items, id("stone"), 1);
        assert_eq!(stones.max_crafts(&inv), 4);
        assert!(stones.affordable(&inv, 4) && !stones.affordable(&inv, 5));
        // Stone first, then basalt, then limestone.
        let plan = stones.plan(&inv, 4).unwrap();
        assert_eq!(
            plan,
            vec![
                vec![(id("stone"), 1)],
                vec![(id("basalt"), 1)],
                vec![(id("basalt"), 1)],
                vec![(id("limestone"), 1)]
            ]
        );
        assert!(stones.plan(&inv, 5).is_none());
        assert!(stones.matches(&[(id("basalt"), 1)]));
        assert!(!stones.matches(&[(id("sand"), 1)]));
        assert!(!stones.matches(&[(id("basalt"), 2)]));

        // Four of a group, two members in one craft.
        let src = "[[group]]\nname = \"any_rock\"\nitems = [\"stone\", \"basalt\"]\n\
                   [[recipe]]\noutput = \"stones\"\ninputs = [[\"any_rock\", 4], [\"sand\", 1]]\n";
        let book = RecipeBook::from_toml(src, &items).unwrap();
        let r = &book.recipes[0];
        let mut inv = Inventory::default();
        inv.add(&items, id("basalt"), 2);
        inv.add(&items, id("stone"), 2);
        inv.add(&items, id("sand"), 1);
        assert_eq!(
            r.plan(&inv, 1).unwrap(),
            vec![vec![(id("stone"), 2), (id("basalt"), 2), (id("sand"), 1)]]
        );
        assert!(r.matches(&[(id("stone"), 2), (id("basalt"), 2), (id("sand"), 1)]));
        assert!(!r.matches(&[(id("stone"), 3), (id("basalt"), 2), (id("sand"), 1)]));
    }

    #[test]
    fn groups_are_checked_at_load() {
        let blocks = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&blocks);
        let bad = [
            // A member that is not an item.
            "[[group]]\nname = \"any_x\"\nitems = [\"unobtainium\"]\n",
            // A group named as an item.
            "[[group]]\nname = \"stone\"\nitems = [\"basalt\"]\n",
            // Empty, and a repeated member.
            "[[group]]\nname = \"any_x\"\nitems = []\n",
            "[[group]]\nname = \"any_x\"\nitems = [\"stone\", \"stone\"]\n",
            // An item serving two inputs of one recipe.
            "[[group]]\nname = \"any_x\"\nitems = [\"stone\", \"basalt\"]\n\
             [[recipe]]\noutput = \"stones\"\ninputs = [[\"any_x\", 1], [\"stone\", 1]]\n",
        ];
        for src in bad {
            assert!(RecipeBook::from_toml(src, &items).is_err(), "{src}");
        }
    }
}
