//! Items: what an inventory holds.
//!
//! Every placeable block is also an item of the same name (so `stone` the
//! block and `stone` the item are one idea). Everything else (materials such
//! as ore and ingots, tools) is listed in `assets/items.toml`. Like blocks,
//! items are data the server sends to clients in the handshake.

use crate::block::{BlockDef, BlockId, BlockRegistry, Colour, Material};
use crate::movement::{self, GrappleSpec};
use crate::shape::ShapeFamily;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ItemId(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolClass {
    /// Rock, ore, crystal.
    Hammer,
    /// Soil, sand, gravel, snow.
    Shovel,
    /// Wood and leaves.
    Axe,
}

impl ToolClass {
    pub fn suits(self, m: Material) -> bool {
        matches!(
            (self, m),
            (ToolClass::Hammer, Material::Rock | Material::Crystal)
                | (ToolClass::Shovel, Material::Soil)
                | (ToolClass::Axe, Material::Wood | Material::Plant)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ItemKind {
    /// Places this block.
    Block(BlockId),
    /// Crafting material: ore, ingots, sticks.
    Material,
    /// Breaks its materials `speed` times faster than a bare hand, for
    /// `durability` blocks.
    Tool {
        class: ToolClass,
        speed: f32,
        durability: u16,
    },
    /// Cuts blocks into shapes of `family` (ADR-0008): one octant at a time
    /// when `precise`, else an edge's two. One use per cut.
    Chisel {
        family: ShapeFamily,
        precise: bool,
        durability: u16,
    },
    /// Fires a hook that sticks where it strikes, to swing from and climb
    /// (`movement::step`). One use per shot.
    Grapple {
        range: f32,
        shot_speed: f32,
        reel_speed: f32,
        hookshot: bool,
        durability: u16,
    },
    /// Links two portals of the same size (ADR-0009): made at one, used up
    /// at the other. The stack's `tag` is the portal it was made at.
    PortalToken,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemDef {
    pub name: String,
    /// What players see.
    pub display: String,
    pub kind: ItemKind,
    /// Most of this item one slot holds (1 for tools).
    pub stack: u16,
    /// Icon texture for non-block items (`assets/textures/items/<name>.png`).
    pub icon: Option<String>,
}

impl ItemDef {
    pub fn block(&self) -> Option<BlockId> {
        match self.kind {
            ItemKind::Block(b) => Some(b),
            _ => None,
        }
    }

    pub fn tool(&self) -> Option<(ToolClass, f32, u16)> {
        match self.kind {
            ItemKind::Tool {
                class,
                speed,
                durability,
            } => Some((class, speed, durability)),
            _ => None,
        }
    }

    /// `(family, precise, durability)` for a chisel.
    pub fn chisel(&self) -> Option<(ShapeFamily, bool, u16)> {
        match self.kind {
            ItemKind::Chisel {
                family,
                precise,
                durability,
            } => Some((family, precise, durability)),
            _ => None,
        }
    }

    /// What a grapple does. The same on server and client: both widen the
    /// same `f32`s from the catalogue.
    pub fn grapple(&self) -> Option<GrappleSpec> {
        match self.kind {
            ItemKind::Grapple {
                range,
                shot_speed,
                reel_speed,
                hookshot,
                ..
            } => Some(GrappleSpec {
                range: range as f64,
                shot_speed: shot_speed as f64,
                reel_speed: reel_speed as f64,
                hookshot,
            }),
            _ => None,
        }
    }

    /// Uses before it breaks, for anything that wears.
    pub fn durability(&self) -> Option<u16> {
        match self.kind {
            ItemKind::Tool { durability, .. }
            | ItemKind::Chisel { durability, .. }
            | ItemKind::Grapple { durability, .. } => Some(durability),
            _ => None,
        }
    }
}

/// Some number of one item in one slot. `wear` is durability spent (tools).
/// `colour` is part of the item's identity: stacks of different colours never
/// merge, and a placed block takes its stack's colour (ADR-0007). `tag` is
/// what one stack is bound to (a portal token's portal, ADR-0009), 0 for
/// nothing; stacks with different tags never merge either.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: u16,
    pub wear: u16,
    #[serde(default)]
    pub colour: Colour,
    #[serde(default)]
    pub tag: u32,
}

impl ItemStack {
    pub fn new(item: ItemId, count: u16) -> Self {
        Self::coloured(item, Colour::NATURAL, count)
    }

    pub fn coloured(item: ItemId, colour: Colour, count: u16) -> Self {
        Self {
            item,
            count,
            wear: 0,
            colour,
            tag: 0,
        }
    }

    /// A single item bound to `tag` (a portal token's portal).
    pub fn tagged(item: ItemId, tag: u32) -> Self {
        Self {
            tag,
            ..Self::new(item, 1)
        }
    }

    /// Whether `other` can be added onto this stack (same item, colour and
    /// tag, neither worn).
    pub fn stacks_with(&self, other: &ItemStack) -> bool {
        self.item == other.item
            && self.colour == other.colour
            && self.tag == other.tag
            && self.wear == 0
            && other.wear == 0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ItemError {
    #[error("item catalogue is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("item `{0}` is listed twice (placeable blocks are items already)")]
    Duplicate(String),
    #[error("item `{0}` has a stack size of 0")]
    ZeroStack(String),
    #[error("tool `{0}` must stack to 1 and have speed > 0 and durability > 0")]
    BadTool(String),
    #[error("block `{0}` drops `{1}`, which is not an item")]
    UnknownDrop(String, String),
    #[error("no item named `{0}`")]
    Unknown(String),
    #[error("item {0} refers to block {1}, which does not exist")]
    BadBlock(String, u16),
    #[error("too many items")]
    TooMany,
}

#[derive(Deserialize)]
struct ItemFile {
    #[serde(default)]
    item: Vec<ItemEntry>,
}

#[derive(Deserialize)]
struct ItemEntry {
    name: String,
    display: Option<String>,
    #[serde(default = "default_stack")]
    stack: u16,
    #[serde(default)]
    tool: Option<ToolEntry>,
    #[serde(default)]
    chisel: Option<ChiselEntry>,
    #[serde(default)]
    grapple: Option<GrappleEntry>,
    #[serde(default)]
    portal_token: bool,
    icon: Option<String>,
}

#[derive(Deserialize)]
struct GrappleEntry {
    range: f32,
    shot_speed: f32,
    reel_speed: f32,
    #[serde(default)]
    hookshot: bool,
    durability: u16,
}

#[derive(Deserialize)]
struct ChiselEntry {
    family: ShapeFamily,
    #[serde(default)]
    precise: bool,
    durability: u16,
}

#[derive(Deserialize)]
struct ToolEntry {
    class: ToolClass,
    speed: f32,
    durability: u16,
}

fn default_stack() -> u16 {
    100
}

/// `deep_stone` -> `Deep Stone`.
pub fn title_case(name: &str) -> String {
    name.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemRegistry {
    defs: Vec<ItemDef>,
    by_name: HashMap<String, ItemId>,
    /// What each block drops when broken, by block id.
    drops: Vec<Option<ItemId>>,
}

impl ItemRegistry {
    /// The server's catalogue: placeable blocks first, then `items.toml`.
    pub fn build(blocks: &BlockRegistry, items_toml: &str) -> Result<Self, ItemError> {
        let file: ItemFile = toml::from_str(items_toml)?;
        let mut defs = Vec::new();
        for (id, b) in blocks.iter() {
            if b.placeable {
                defs.push(ItemDef {
                    name: b.name.clone(),
                    display: title_case(&b.name),
                    kind: ItemKind::Block(id),
                    stack: 100,
                    icon: None,
                });
            }
        }
        for e in file.item {
            let kind = match (e.tool, e.chisel, e.grapple) {
                (Some(t), None, None) => ItemKind::Tool {
                    class: t.class,
                    speed: t.speed,
                    durability: t.durability,
                },
                (None, Some(c), None) => ItemKind::Chisel {
                    family: c.family,
                    precise: c.precise,
                    durability: c.durability,
                },
                (None, None, Some(g)) => ItemKind::Grapple {
                    range: g.range,
                    shot_speed: g.shot_speed,
                    reel_speed: g.reel_speed,
                    hookshot: g.hookshot,
                    durability: g.durability,
                },
                (None, None, None) if e.portal_token => ItemKind::PortalToken,
                (None, None, None) => ItemKind::Material,
                _ => return Err(ItemError::BadTool(e.name)),
            };
            defs.push(ItemDef {
                display: e.display.unwrap_or_else(|| title_case(&e.name)),
                name: e.name,
                kind,
                stack: e.stack,
                icon: e.icon,
            });
        }
        let reg = Self::from_defs(defs, blocks)?;
        Ok(reg)
    }

    /// The catalogue shipped in `assets/items.toml`, for the built-in blocks.
    pub fn builtin(blocks: &BlockRegistry) -> Self {
        Self::build(blocks, include_str!("../../../assets/items.toml")).expect("assets/items.toml is valid")
    }

    /// Builds and validates a registry from definitions (the server's own, or
    /// the copy a client receives in the handshake).
    pub fn from_defs(defs: Vec<ItemDef>, blocks: &BlockRegistry) -> Result<Self, ItemError> {
        if defs.len() >= u16::MAX as usize {
            return Err(ItemError::TooMany);
        }
        let mut by_name = HashMap::new();
        for (i, d) in defs.iter().enumerate() {
            if d.stack == 0 {
                return Err(ItemError::ZeroStack(d.name.clone()));
            }
            match d.kind {
                ItemKind::Tool { speed, durability, .. } if d.stack != 1 || speed <= 0.0 || durability == 0 => {
                    return Err(ItemError::BadTool(d.name.clone()));
                }
                ItemKind::Chisel { durability, .. } if d.stack != 1 || durability == 0 => {
                    return Err(ItemError::BadTool(d.name.clone()));
                }
                ItemKind::Grapple {
                    range,
                    shot_speed,
                    reel_speed,
                    durability,
                    ..
                } if d.stack != 1
                    || durability == 0
                    || !(range >= movement::ROPE_MIN as f32 && range <= 64.0)
                    || !(shot_speed > 0.0 && shot_speed <= 200.0)
                    || !(reel_speed > 0.0 && reel_speed <= 100.0) =>
                {
                    return Err(ItemError::BadTool(d.name.clone()));
                }
                ItemKind::PortalToken if d.stack != 1 => return Err(ItemError::BadTool(d.name.clone())),
                ItemKind::Block(b) if !blocks.contains(b) => return Err(ItemError::BadBlock(d.name.clone(), b.0)),
                _ => {}
            }
            if by_name.insert(d.name.clone(), ItemId(i as u16)).is_some() {
                return Err(ItemError::Duplicate(d.name.clone()));
            }
        }
        let mut drops = Vec::with_capacity(blocks.len());
        for (_, b) in blocks.iter() {
            drops.push(Self::resolve_drop(b, &by_name)?);
        }
        Ok(Self { defs, by_name, drops })
    }

    fn resolve_drop(b: &BlockDef, by_name: &HashMap<String, ItemId>) -> Result<Option<ItemId>, ItemError> {
        match &b.drops {
            Some(name) if name.is_empty() => Ok(None),
            Some(name) => by_name
                .get(name)
                .copied()
                .map(Some)
                .ok_or_else(|| ItemError::UnknownDrop(b.name.clone(), name.clone())),
            None => Ok(by_name.get(&b.name).copied()),
        }
    }

    pub fn id(&self, name: &str) -> Result<ItemId, ItemError> {
        self.by_name
            .get(name)
            .copied()
            .ok_or_else(|| ItemError::Unknown(name.to_string()))
    }

    pub fn get(&self, id: ItemId) -> Option<&ItemDef> {
        self.defs.get(id.0 as usize)
    }

    pub fn contains(&self, id: ItemId) -> bool {
        (id.0 as usize) < self.defs.len()
    }

    /// The item a block yields when broken, if any.
    pub fn drop_of(&self, block: BlockId) -> Option<ItemId> {
        self.drops.get(block.0 as usize).copied().flatten()
    }

    /// The item that places `block`.
    pub fn item_for_block(&self, block: BlockId) -> Option<ItemId> {
        self.defs
            .iter()
            .position(|d| d.block() == Some(block))
            .map(|i| ItemId(i as u16))
    }

    pub fn defs(&self) -> &[ItemDef] {
        &self.defs
    }

    pub fn iter(&self) -> impl Iterator<Item = (ItemId, &ItemDef)> {
        self.defs.iter().enumerate().map(|(i, d)| (ItemId(i as u16), d))
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// Seconds to break `block` holding `tool` (None for a bare hand). A tool
    /// helps only on the materials its class suits.
    pub fn break_seconds(&self, block: &BlockDef, tool: Option<ItemId>) -> f32 {
        let speed = tool
            .and_then(|t| self.get(t))
            .and_then(|d| d.tool())
            .filter(|(class, _, _)| class.suits(block.material))
            .map(|(_, speed, _)| speed)
            .unwrap_or(1.0);
        block.hardness / speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (BlockRegistry, ItemRegistry) {
        let b = BlockRegistry::builtin();
        let i = ItemRegistry::builtin(&b);
        (b, i)
    }

    #[test]
    fn placeable_blocks_are_items_and_ores_drop_materials() {
        let (b, i) = setup();
        let stone = b.id("stone").unwrap();
        let stone_item = i.id("stone").unwrap();
        assert_eq!(i.get(stone_item).unwrap().block(), Some(stone));
        assert_eq!(i.drop_of(stone), Some(stone_item));
        assert_eq!(
            i.drop_of(b.id("copper_vein").unwrap()),
            Some(i.id("copper_ore").unwrap())
        );
        assert_eq!(i.drop_of(b.id("coal_seam").unwrap()), Some(i.id("coal").unwrap()));
        assert_eq!(i.drop_of(BlockId::AIR), None);
        assert_eq!(i.get(stone_item).unwrap().display, "Stone");
    }

    #[test]
    fn tools_speed_up_only_their_materials() {
        let (b, i) = setup();
        let stone = b.get(b.id("stone").unwrap());
        let soil = b.get(b.id("soil").unwrap());
        let hammer = i.id("stone_hammer").unwrap();
        assert!(i.break_seconds(stone, Some(hammer)) < i.break_seconds(stone, None));
        assert_eq!(i.break_seconds(soil, Some(hammer)), i.break_seconds(soil, None));
        let shovel = i.id("stone_shovel").unwrap();
        assert!(i.break_seconds(soil, Some(shovel)) < soil.hardness);
        let copper = i.id("copper_hammer").unwrap();
        assert!(i.break_seconds(stone, Some(copper)) < i.break_seconds(stone, Some(hammer)));
    }

    #[test]
    fn bad_catalogues_are_refused() {
        let b = BlockRegistry::builtin();
        assert!(matches!(
            ItemRegistry::build(&b, "[[item]]\nname = \"stone\"\n"),
            Err(ItemError::Duplicate(_))
        ));
        assert!(matches!(
            ItemRegistry::build(
                &b,
                "[[item]]\nname = \"pick\"\nstack = 5\ntool = { class = \"hammer\", speed = 2.0, durability = 10 }\n"
            ),
            Err(ItemError::BadTool(_))
        ));
        // The builtin blocks name drops that only the builtin items define.
        assert!(matches!(ItemRegistry::build(&b, ""), Err(ItemError::UnknownDrop(..))));
    }

    #[test]
    fn grapples_are_read_and_checked() {
        let (_, items) = setup();
        let def = items.get(items.id("iron_hookshot").unwrap()).unwrap();
        let g = def.grapple().expect("a grapple");
        assert!(g.hookshot && g.range > 0.0 && g.reel_speed > 0.0);
        assert!(def.durability().is_some());
        assert_eq!(
            items
                .get(items.id("copper_grapple").unwrap())
                .unwrap()
                .grapple()
                .map(|g| g.hookshot),
            Some(false)
        );
        let b = BlockRegistry::builtin();
        let bad = |g: &str| {
            let toml = format!(
                "{}\n[[item]]\nname = \"g\"\nstack = 1\ngrapple = {g}\n",
                include_str!("../../../assets/items.toml")
            );
            matches!(ItemRegistry::build(&b, &toml), Err(ItemError::BadTool(_)))
        };
        assert!(!bad(
            "{ range = 10.0, shot_speed = 50.0, reel_speed = 5.0, durability = 9 }"
        ));
        assert!(bad(
            "{ range = 500.0, shot_speed = 50.0, reel_speed = 5.0, durability = 9 }"
        ));
        assert!(bad(
            "{ range = 10.0, shot_speed = 0.0, reel_speed = 5.0, durability = 9 }"
        ));
        assert!(bad(
            "{ range = 10.0, shot_speed = 50.0, reel_speed = 5.0, durability = 0 }"
        ));
    }

    #[test]
    fn titles() {
        assert_eq!(title_case("deep_stone"), "Deep Stone");
        assert_eq!(title_case("glow_crystal"), "Glow Crystal");
    }
}
