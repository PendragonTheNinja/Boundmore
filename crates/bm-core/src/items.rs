//! Items: what an inventory holds.
//!
//! Every placeable block is also an item of the same name (so `stone` the
//! block and `stone` the item are one idea). Everything else (materials such
//! as ore and ingots, tools) is listed in `assets/items.toml`. Like blocks,
//! items are data the server sends to clients in the handshake.

use crate::block::{BlockDef, BlockId, BlockRegistry, Colour, Material, Mount};
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

/// A shape that one swing hits beyond the hit block (Boundless's area of
/// effect forge boon [tuning 2.5a]; `mining::aoe_blocks`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Aoe {
    /// The hit block and the one below, 60% each.
    #[serde(rename = "1x2")]
    OneByTwo,
    /// A row of three across the view, 40% each.
    #[serde(rename = "3x1")]
    ThreeByOne,
    /// A plus of five facing the player, 40% each.
    #[serde(rename = "cross")]
    Cross,
    /// A square of nine facing the player, 40% each.
    #[serde(rename = "3x3")]
    ThreeByThree,
    /// The hit block and 1 to 4 random neighbours, full damage.
    #[serde(rename = "random")]
    Random,
}

impl Aoe {
    /// Damage each block in the shape takes, as a fraction of a single hit.
    pub fn multiplier(self) -> f64 {
        match self {
            Aoe::OneByTwo => 0.6,
            Aoe::ThreeByOne | Aoe::Cross | Aoe::ThreeByThree => 0.4,
            Aoe::Random => 1.0,
        }
    }
}

/// What a tool does to blocks [tuning 2.1]. A bare hand breaks nothing
/// (Nathan, 2026-10-20: as in Boundless). Every swing
/// costs one durability.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolStats {
    /// Hammer, shovel or axe: full damage on what it suits, a quarter on the
    /// rest. None suits everything (the hand stone).
    pub class: Option<ToolClass>,
    /// Damage per swing, before armour.
    pub damage: u32,
    /// Sets the time between swings (`mining::seconds_per_hit`).
    pub action_speed: f32,
    /// Chance (0..1) a swing is a critical hit.
    pub crit_chance: f32,
    /// A critical hit deals `1 + crit_effect` times the damage.
    pub crit_effect: f32,
    /// Swings before it breaks.
    pub durability: u16,
    /// Energy a swing costs (`energy`); Boundless's tools all cost damage
    /// / 50 (tuning 2.1).
    pub energy: u16,
    /// The shape one swing hits; None: one block.
    pub aoe: Option<Aoe>,
}

impl ToolStats {
    fn valid(&self) -> bool {
        self.damage > 0
            && self.action_speed.is_finite()
            && self.action_speed > 0.0
            && (0.0..=1.0).contains(&self.crit_chance)
            && self.crit_effect.is_finite()
            && self.crit_effect >= 0.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ItemKind {
    /// Places this block.
    Block(BlockId),
    /// Crafting material: ore, ingots, sticks.
    Material,
    /// Breaks blocks (`mining`).
    Tool(ToolStats),
    /// Cuts blocks into shapes of `family` (ADR-0008): one octant at a time
    /// when `precise`, else an edge's two. One use per cut.
    Chisel {
        family: ShapeFamily,
        precise: bool,
        durability: u16,
        /// Energy a cut costs (`energy`).
        energy: u16,
    },
    /// Fires a hook that sticks where it strikes, to swing from and climb
    /// (`movement::step`). One use per shot.
    Grapple {
        range: f32,
        shot_speed: f32,
        reel_in: f32,
        reel_out: f32,
        hookshot: bool,
        durability: u16,
        /// Energy a shot costs (`energy`).
        energy: u16,
    },
    /// Links two portals of the same size (ADR-0009): made at one, used up
    /// at the other. The stack's `tag` is the portal it was made at.
    PortalToken,
    /// Shoots (milestone 13, Boundless's slingbows [tuning 3.1.1]). One use
    /// per shot.
    Sling(SlingStats),
    /// A bag of bombs (milestone 13 part 2, tuning 3.1.3): hold to light,
    /// release to throw; one use per throw.
    Bomb(BombStats),
}

/// What a bag of bombs does [tuning 3.1.3]: area (impulse) damage within
/// `radius`, knockback, 200 throws a bag.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BombStats {
    pub damage: u32,
    /// Blast radius in blocks.
    pub radius: f32,
    /// Blocks a second the blast pushes creatures away.
    pub knockback: f32,
    /// Sets the time between throws (the swing curve).
    pub action_speed: f32,
    /// Speed of a full throw, blocks a second (25 for every bag).
    pub throw_speed: f32,
    pub crit_chance: f32,
    pub crit_effect: f32,
    pub durability: u16,
    /// Energy a throw costs: damage / 10 for every Boundless bag.
    pub energy: u16,
}

impl BombStats {
    fn valid(&self) -> bool {
        self.damage > 0
            && self.durability > 0
            && self.radius > 0.0
            && self.radius <= 16.0
            && self.action_speed.is_finite()
            && self.action_speed > 0.0
            && self.throw_speed > 0.0
            && self.throw_speed <= 100.0
            && (0.0..=1.0).contains(&self.crit_chance)
            && self.crit_effect >= 0.0
    }
}

/// What a sling does [tuning 3.1.1]: every shot is kinetic, breaks on what it
/// hits, and needs no ammunition.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlingStats {
    pub damage: u32,
    /// Sets the time between shots (`mining::seconds_per_hit`, the same curve).
    pub action_speed: f32,
    /// Blocks a second.
    pub shot_speed: f32,
    /// Blocks a shot flies before it is spent.
    pub range: f32,
    /// How much a shot drops: gravity on it is `falloff x combat::SHOT_GRAVITY`.
    pub falloff: f32,
    pub crit_chance: f32,
    pub crit_effect: f32,
    pub durability: u16,
    /// Energy a shot costs: damage / 50 for every Boundless slingbow.
    pub energy: u16,
}

impl SlingStats {
    fn valid(&self) -> bool {
        self.damage > 0
            && self.durability > 0
            && self.action_speed.is_finite()
            && self.action_speed > 0.0
            && self.shot_speed > 0.0
            && self.shot_speed <= 200.0
            && self.range > 0.0
            && self.range <= 128.0
            && (0.0..=4.0).contains(&self.falloff)
            && (0.0..=1.0).contains(&self.crit_chance)
            && self.crit_effect >= 0.0
    }
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
    /// Light it gives off around a player holding it in the selected hotbar
    /// slot (a torch: its prop's light), 0 for none.
    #[serde(default)]
    pub held_light: u8,
    /// Heat it gives burnt in a furnace's fuel slot (0: not fuel; tuning
    /// 4.7: soft coal 100, trunk 30, timber 15, stick 7).
    #[serde(default)]
    pub fuel: u16,
}

impl ItemDef {
    pub fn block(&self) -> Option<BlockId> {
        match self.kind {
            ItemKind::Block(b) => Some(b),
            _ => None,
        }
    }

    pub fn tool(&self) -> Option<ToolStats> {
        match self.kind {
            ItemKind::Tool(t) => Some(t),
            _ => None,
        }
    }

    pub fn bomb(&self) -> Option<BombStats> {
        match self.kind {
            ItemKind::Bomb(b) => Some(b),
            _ => None,
        }
    }

    pub fn sling(&self) -> Option<SlingStats> {
        match self.kind {
            ItemKind::Sling(l) => Some(l),
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
                ..
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
                reel_in,
                reel_out,
                hookshot,
                energy,
                ..
            } => Some(GrappleSpec {
                range: range as f64,
                shot_speed: shot_speed as f64,
                reel_in_speed: reel_in as f64,
                reel_out_speed: reel_out as f64,
                hookshot,
                energy,
            }),
            _ => None,
        }
    }

    /// Energy one use costs (a swing, a shot, a cut), for anything that
    /// tires you (`energy`).
    pub fn energy(&self) -> Option<u16> {
        match self.kind {
            ItemKind::Tool(ToolStats { energy, .. })
            | ItemKind::Chisel { energy, .. }
            | ItemKind::Grapple { energy, .. }
            | ItemKind::Sling(SlingStats { energy, .. })
            | ItemKind::Bomb(BombStats { energy, .. }) => Some(energy),
            _ => None,
        }
    }

    /// Uses before it breaks, for anything that wears.
    pub fn durability(&self) -> Option<u16> {
        match self.kind {
            ItemKind::Tool(ToolStats { durability, .. })
            | ItemKind::Chisel { durability, .. }
            | ItemKind::Grapple { durability, .. }
            | ItemKind::Sling(SlingStats { durability, .. })
            | ItemKind::Bomb(BombStats { durability, .. }) => Some(durability),
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
    #[error("tool `{0}` must stack to 1, with damage, action speed and durability above 0 and a crit chance in 0..1")]
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
    #[serde(default)]
    sling: Option<SlingEntry>,
    #[serde(default)]
    bomb: Option<BombEntry>,
    icon: Option<String>,
    #[serde(default)]
    fuel: u16,
}

#[derive(Deserialize)]
struct GrappleEntry {
    range: f32,
    shot_speed: f32,
    reel_in: f32,
    reel_out: f32,
    #[serde(default)]
    hookshot: bool,
    durability: u16,
    energy: u16,
}

#[derive(Deserialize)]
struct SlingEntry {
    damage: u32,
    action_speed: f32,
    shot_speed: f32,
    range: f32,
    falloff: f32,
    #[serde(default)]
    crit_chance: f32,
    #[serde(default = "default_crit_effect")]
    crit_effect: f32,
    durability: u16,
}

#[derive(Deserialize)]
struct BombEntry {
    damage: u32,
    radius: f32,
    knockback: f32,
    #[serde(default = "bomb_action_speed")]
    action_speed: f32,
    #[serde(default = "bomb_throw_speed")]
    throw_speed: f32,
    #[serde(default = "bomb_crit")]
    crit_chance: f32,
    #[serde(default = "default_crit_effect")]
    crit_effect: f32,
    #[serde(default = "bomb_durability")]
    durability: u16,
}

fn bomb_action_speed() -> f32 {
    50.0
}
fn bomb_throw_speed() -> f32 {
    25.0
}
fn bomb_crit() -> f32 {
    0.1
}
fn bomb_durability() -> u16 {
    200
}

#[derive(Deserialize)]
struct ChiselEntry {
    family: ShapeFamily,
    #[serde(default)]
    precise: bool,
    durability: u16,
    /// Boundless's basic chisels cost 10 a cut (tuning 4.9).
    #[serde(default = "chisel_energy")]
    energy: u16,
}

fn chisel_energy() -> u16 {
    10
}

#[derive(Deserialize)]
struct ToolEntry {
    #[serde(default)]
    class: Option<ToolClass>,
    damage: u32,
    action_speed: f32,
    #[serde(default)]
    crit_chance: f32,
    #[serde(default = "default_crit_effect")]
    crit_effect: f32,
    #[serde(default)]
    durability: u16,
    #[serde(default)]
    aoe: Option<Aoe>,
    /// Energy per swing; unset, damage / 50 as for every Boundless tool.
    #[serde(default)]
    energy: Option<u16>,
}

impl ToolEntry {
    fn stats(&self) -> ToolStats {
        ToolStats {
            class: self.class,
            damage: self.damage,
            action_speed: self.action_speed,
            crit_chance: self.crit_chance,
            crit_effect: self.crit_effect,
            durability: self.durability,
            aoe: self.aoe,
            energy: self.energy.unwrap_or((self.damage / 50) as u16),
        }
    }
}

fn default_crit_effect() -> f32 {
    0.2
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
    /// Extra yields by block id: (item, expected count per tier).
    bonus: Vec<Vec<(ItemId, [f32; 8])>>,
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
                    held_light: if b.is_prop() && b.model == crate::block::Model::Torch {
                        b.light
                    } else {
                        0
                    },
                    fuel: b.fuel,
                });
            }
        }
        for e in file.item {
            let kind = match (e.tool, e.chisel, e.grapple) {
                (Some(t), None, None) => ItemKind::Tool(t.stats()),
                (None, Some(c), None) => ItemKind::Chisel {
                    family: c.family,
                    precise: c.precise,
                    durability: c.durability,
                    energy: c.energy,
                },
                (None, None, Some(g)) => ItemKind::Grapple {
                    range: g.range,
                    shot_speed: g.shot_speed,
                    reel_in: g.reel_in,
                    reel_out: g.reel_out,
                    hookshot: g.hookshot,
                    durability: g.durability,
                    energy: g.energy,
                },
                (None, None, None) if e.sling.is_some() => {
                    let l = e.sling.as_ref().expect("checked");
                    ItemKind::Sling(SlingStats {
                        damage: l.damage,
                        action_speed: l.action_speed,
                        shot_speed: l.shot_speed,
                        range: l.range,
                        falloff: l.falloff,
                        crit_chance: l.crit_chance,
                        crit_effect: l.crit_effect,
                        durability: l.durability,
                        energy: (l.damage / 50) as u16,
                    })
                }
                (None, None, None) if e.bomb.is_some() => {
                    let b = e.bomb.as_ref().expect("checked");
                    ItemKind::Bomb(BombStats {
                        damage: b.damage,
                        radius: b.radius,
                        knockback: b.knockback,
                        action_speed: b.action_speed,
                        throw_speed: b.throw_speed,
                        crit_chance: b.crit_chance,
                        crit_effect: b.crit_effect,
                        durability: b.durability,
                        energy: (b.damage / 10) as u16,
                    })
                }
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
                held_light: 0,
                fuel: e.fuel,
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
                ItemKind::Tool(t) if d.stack != 1 || t.durability == 0 || !t.valid() => {
                    return Err(ItemError::BadTool(d.name.clone()));
                }
                ItemKind::Chisel { durability, .. } if d.stack != 1 || durability == 0 => {
                    return Err(ItemError::BadTool(d.name.clone()));
                }
                ItemKind::Grapple {
                    range,
                    shot_speed,
                    reel_in,
                    reel_out,
                    durability,
                    ..
                } if d.stack != 1
                    || durability == 0
                    || !(range >= movement::ROPE_MIN as f32 && range <= 64.0)
                    || !(shot_speed > 0.0 && shot_speed <= 200.0)
                    || !(reel_in > 0.0 && reel_in <= 100.0)
                    || !(reel_out > 0.0 && reel_out <= 100.0) =>
                {
                    return Err(ItemError::BadTool(d.name.clone()));
                }
                ItemKind::PortalToken if d.stack != 1 => return Err(ItemError::BadTool(d.name.clone())),
                ItemKind::Sling(l) if d.stack != 1 || !l.valid() => return Err(ItemError::BadTool(d.name.clone())),
                ItemKind::Bomb(b) if d.stack != 1 || !b.valid() => return Err(ItemError::BadTool(d.name.clone())),
                ItemKind::Block(b) if !blocks.contains(b) => return Err(ItemError::BadBlock(d.name.clone(), b.0)),
                _ => {}
            }
            if by_name.insert(d.name.clone(), ItemId(i as u16)).is_some() {
                return Err(ItemError::Duplicate(d.name.clone()));
            }
        }
        let mut drops = Vec::with_capacity(blocks.len());
        let mut bonus = Vec::with_capacity(blocks.len());
        for (_, b) in blocks.iter() {
            drops.push(Self::resolve_drop(b, &by_name)?);
            let mut extra = Vec::new();
            for x in &b.bonus {
                let item = by_name
                    .get(&x.item)
                    .copied()
                    .ok_or_else(|| ItemError::UnknownDrop(b.name.clone(), x.item.clone()))?;
                extra.push((item, x.per_tier));
            }
            bonus.push(extra);
        }
        Ok(Self {
            defs,
            by_name,
            drops,
            bonus,
        })
    }

    /// The stats a swing uses holding `item`; None for a bare hand or
    /// anything not a tool, which cannot break blocks.
    pub fn swing_tool(&self, item: Option<ItemId>) -> Option<ToolStats> {
        item.and_then(|i| self.get(i)).and_then(|d| d.tool())
    }

    /// Extra yields of `block`: (item, expected count on each tier 1 to 8).
    pub fn bonus_of(&self, block: BlockId) -> &[(ItemId, [f32; 8])] {
        self.bonus.get(block.0 as usize).map_or(&[], |v| v.as_slice())
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

    /// The block that places `item` with `mount`: the item's own block when
    /// its mount matches, else a variant with that mount that drops the item
    /// (`torch_north` for `torch`). None for anything but a prop, or a mount
    /// it has no variant for.
    pub fn prop_block(&self, blocks: &BlockRegistry, item: ItemId, mount: Mount) -> Option<BlockId> {
        let own = self.get(item)?.block()?;
        let def = blocks.get(own);
        def.mount?;
        if def.mount == Some(mount) {
            return Some(own);
        }
        blocks
            .iter()
            .find(|(id, b)| b.mount == Some(mount) && self.drop_of(*id) == Some(item))
            .map(|(id, _)| id)
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

    const HAND: &str = "";

    #[test]
    fn tools_have_boundless_stats() {
        let (_, i) = setup();
        let copper = i.get(i.id("copper_hammer").unwrap()).unwrap().tool().unwrap();
        assert_eq!(
            (copper.damage, copper.action_speed, copper.durability),
            (400, 30.0, 700),
            "tuning 2.1"
        );
        let iron = i.get(i.id("iron_axe").unwrap()).unwrap().tool().unwrap();
        assert_eq!((iron.damage, iron.crit_chance, iron.durability), (700, 0.2, 900));
        assert_eq!(iron.class, Some(ToolClass::Axe));
        assert!(
            i.get(i.id("hand_stone").unwrap())
                .unwrap()
                .tool()
                .unwrap()
                .class
                .is_none()
        );
        // A bare hand, or anything that is not a tool, cannot swing.
        assert_eq!(i.swing_tool(Some(i.id("stone").unwrap())), None);
        assert_eq!(i.swing_tool(None), None);
    }

    #[test]
    fn torches_light_the_hand_and_seams_give_bonus_ore() {
        let (b, i) = setup();
        let torch = i.get(i.id("torch").unwrap()).unwrap();
        assert!(torch.held_light > 0);
        assert_eq!(i.get(i.id("glow_crystal").unwrap()).unwrap().held_light, 0);
        let vein = b.id("copper_vein").unwrap();
        let ore = i.id("copper_ore").unwrap();
        assert!(i.bonus_of(vein).iter().any(|&(item, per)| item == ore && per[0] == 0.5));
        let flint = i.id("flint").unwrap();
        assert!(
            i.bonus_of(b.id("gravel").unwrap())
                .iter()
                .any(|&(item, _)| item == flint)
        );
        assert!(i.bonus_of(b.id("soil").unwrap()).is_empty());
    }

    #[test]
    fn uses_cost_boundless_energy() {
        // Tools damage / 50, grapples per shot, chisels 10 (tuning 2.1, 1.1,
        // 4.9; milestone 10).
        let (_, i) = setup();
        let cost = |n: &str| i.get(i.id(n).unwrap()).unwrap().energy();
        assert_eq!(cost("hand_stone"), Some(4));
        assert_eq!(cost("wood_axe"), Some(6));
        assert_eq!(cost("stone_hammer"), Some(8));
        assert_eq!(cost("copper_shovel"), Some(8));
        assert_eq!(cost("iron_hammer"), Some(14));
        assert_eq!(cost("copper_grapple"), Some(60));
        assert_eq!(cost("iron_grapple"), Some(80));
        assert_eq!(cost("square_chisel"), Some(10));
        assert_eq!(cost("stone"), None);
    }

    #[test]
    fn torches_have_a_variant_for_every_face() {
        let (b, i) = setup();
        let torch = i.id("torch").unwrap();
        assert_eq!(i.prop_block(&b, torch, Mount::Floor), Some(b.id("torch").unwrap()));
        assert_eq!(i.prop_block(&b, torch, Mount::West), Some(b.id("torch_west").unwrap()));
        assert_eq!(i.drop_of(b.id("torch_north").unwrap()), Some(torch));
        assert_eq!(i.prop_block(&b, i.id("stone").unwrap(), Mount::Floor), None);
    }

    #[test]
    fn bad_catalogues_are_refused() {
        let b = BlockRegistry::builtin();
        assert!(matches!(
            ItemRegistry::build(&b, &format!("{HAND}[[item]]\nname = \"stone\"\n")),
            Err(ItemError::Duplicate(_))
        ));
        assert!(matches!(
            ItemRegistry::build(
                &b,
                &format!(
                    "{HAND}[[item]]\nname = \"pick\"\nstack = 5\ntool = {{ class = \"hammer\", damage = 10, action_speed = 20.0, durability = 10 }}\n"
                )
            ),
            Err(ItemError::BadTool(_))
        ));
        assert!(matches!(
            ItemRegistry::build(
                &b,
                &format!(
                    "{HAND}[[item]]\nname = \"pick\"\nstack = 1\ntool = {{ class = \"hammer\", damage = 10, action_speed = 20.0, crit_chance = 2.0, durability = 10 }}\n"
                )
            ),
            Err(ItemError::BadTool(_))
        ));
        // The builtin blocks name drops that only the builtin items define.
        assert!(matches!(ItemRegistry::build(&b, HAND), Err(ItemError::UnknownDrop(..))));
    }

    #[test]
    fn grapples_are_read_and_checked() {
        let (_, items) = setup();
        let def = items.get(items.id("copper_grapple").unwrap()).unwrap();
        let g = def.grapple().expect("a grapple");
        assert_eq!(
            (g.range, g.shot_speed, g.reel_in_speed, g.reel_out_speed),
            (10.0, 45.0, 5.0, 15.0)
        );
        assert!(!g.hookshot, "no base grapple reels itself (tuning 1.1)");
        assert!(def.durability().is_some());
        let iron = items.get(items.id("iron_grapple").unwrap()).unwrap();
        let g = iron.grapple().expect("a grapple");
        assert_eq!(
            (g.range, g.shot_speed, g.reel_in_speed, g.reel_out_speed),
            (20.0, 30.0, 2.0, 10.0)
        );
        assert_eq!(iron.durability(), Some(1000));
        let b = BlockRegistry::builtin();
        let bad = |g: &str| {
            let toml = format!(
                "{}\n[[item]]\nname = \"g\"\nstack = 1\ngrapple = {g}\n",
                include_str!("../../../assets/items.toml")
            );
            matches!(ItemRegistry::build(&b, &toml), Err(ItemError::BadTool(_)))
        };
        assert!(!bad(
            "{ range = 10.0, shot_speed = 50.0, reel_in = 5.0, reel_out = 5.0, durability = 9, energy = 60 }"
        ));
        assert!(bad(
            "{ range = 500.0, shot_speed = 50.0, reel_in = 5.0, reel_out = 5.0, durability = 9, energy = 60 }"
        ));
        assert!(bad(
            "{ range = 10.0, shot_speed = 0.0, reel_in = 5.0, reel_out = 5.0, durability = 9, energy = 60 }"
        ));
        assert!(bad(
            "{ range = 10.0, shot_speed = 50.0, reel_in = 5.0, reel_out = 5.0, durability = 0, energy = 60 }"
        ));
    }

    #[test]
    fn titles() {
        assert_eq!(title_case("deep_stone"), "Deep Stone");
        assert_eq!(title_case("glow_crystal"), "Glow Crystal");
    }
}
