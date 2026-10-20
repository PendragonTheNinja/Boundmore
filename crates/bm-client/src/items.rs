//! The client's copy of the player's items: the catalogue and recipes from
//! the welcome, and the inventory the server keeps telling us about.
//!
//! The server is the authority (ADR-0004): the client never changes the
//! inventory itself. It only asks (`MoveItems`, `Discard`, `Craft`) and shows
//! the answer (`Slots`). The checks here exist to avoid asking for moves the
//! server would refuse, using the same `Inventory` rules it runs.

use bm_core::{
    BlockRegistry, Colour, ColourPalette, INVENTORY_SLOTS, Inventory, ItemDef, ItemError, ItemKind, ItemRegistry,
    ItemStack, Recipe, RecipeBook, ShapeFamily, ToolClass, ToolStats,
};
use bm_protocol::ClientMsg;
use std::collections::HashMap;

pub struct Items {
    pub registry: ItemRegistry,
    pub recipes: RecipeBook,
    pub inventory: Inventory,
    /// The colour palette from the welcome (ADR-0007), for names.
    pub colours: ColourPalette,
    /// What each portal token links to, by tag, as the server last said
    /// (`TokenNames`): "2 by 3, Verdance, near 812, 140".
    pub token_names: HashMap<u32, String>,
}

/// How much a "half" drag carries: the larger half, so one item still moves.
pub fn half(count: u16) -> u16 {
    count.div_ceil(2)
}

impl Items {
    pub fn new(
        defs: Vec<ItemDef>,
        hand: ToolStats,
        recipes: Vec<Recipe>,
        mut inventory: Inventory,
        colours: ColourPalette,
        blocks: &BlockRegistry,
    ) -> Result<Self, ItemError> {
        let registry = ItemRegistry::from_defs(defs, hand, blocks)?;
        // Index safety for everything below: exactly 40 slots, all valid.
        inventory.sanitize(&registry);
        let recipes = recipes
            .into_iter()
            .filter(|r| registry.contains(r.output) && r.inputs.iter().all(|(i, _)| registry.contains(*i)))
            .collect();
        Ok(Self {
            registry,
            recipes: RecipeBook { recipes },
            inventory,
            colours,
            token_names: HashMap::new(),
        })
    }

    /// Takes in a `TokenNames`: new names are added, known ones renamed.
    pub fn set_token_names(&mut self, names: Vec<(u32, String)>) {
        self.token_names.extend(names);
    }

    /// What the portal token with this tag links to, if the server said.
    pub fn token_name(&self, tag: u32) -> Option<&str> {
        self.token_names.get(&tag).map(String::as_str)
    }

    /// Applies a `Slots` update. Out-of-range slots and unknown items are
    /// ignored rather than trusted.
    pub fn apply_slots(&mut self, changes: &[(u8, Option<ItemStack>)]) {
        for (slot, stack) in changes {
            let i = *slot as usize;
            if i >= INVENTORY_SLOTS {
                continue;
            }
            self.inventory.slots[i] = stack.filter(|s| self.registry.contains(s.item) && s.count > 0);
        }
    }

    pub fn def(&self, stack: &ItemStack) -> Option<&ItemDef> {
        self.registry.get(stack.item)
    }

    pub fn slot(&self, slot: usize) -> Option<ItemStack> {
        self.inventory.get(slot)
    }

    /// The message for dragging slot `from` onto `to` (the whole stack, or
    /// half of it), or None when the server would refuse it anyway: onto
    /// itself, from an empty slot, onto a full stack, or part of a stack onto
    /// a different item (only whole stacks swap).
    pub fn move_request(&self, from: usize, to: usize, half_stack: bool) -> Option<ClientMsg> {
        if from >= INVENTORY_SLOTS || to >= INVENTORY_SLOTS {
            return None;
        }
        let src = self.inventory.get(from)?;
        let count = if half_stack { half(src.count) } else { src.count };
        let mut trial = self.inventory.clone();
        trial.move_items(&self.registry, from, to, count)?;
        Some(ClientMsg::MoveItems {
            from: from as u8,
            to: to as u8,
            count,
        })
    }

    /// Uses left and in all, for a tool.
    pub fn durability(&self, stack: &ItemStack) -> Option<(u16, u16)> {
        let total = self.def(stack)?.durability()?;
        Some((total.saturating_sub(stack.wear), total))
    }

    /// A stack's name with its colour: "Stone, Rich Azure", or plain
    /// "Stone" when natural (or a colour this palette does not hold).
    pub fn name(&self, stack: &ItemStack) -> String {
        let Some(def) = self.def(stack) else {
            return "Unknown item".into();
        };
        match self
            .colours
            .get(stack.colour)
            .filter(|_| stack.colour != Colour::NATURAL)
        {
            Some(c) => format!("{}, {}", def.display, c.name),
            None => def.display.clone(),
        }
    }

    /// The hover text for a stack: its name (with its colour), then what it
    /// does.
    pub fn tooltip(&self, stack: &ItemStack) -> String {
        let Some(def) = self.def(stack) else {
            return "Unknown item".into();
        };
        let mut lines = vec![self.name(stack)];
        match def.kind {
            ItemKind::Tool(t) => {
                lines.push(match t.class {
                    Some(class) => format!("Best on {}", tool_materials(class)),
                    None => "Works on everything".into(),
                });
                lines.push(format!(
                    "{} damage, {} hits a second",
                    t.damage,
                    trim_float((1.0 / bm_core::mining::seconds_per_hit(t.action_speed)) as f32)
                ));
                if let Some((left, total)) = self.durability(stack) {
                    lines.push(format!("{left} of {total} uses left"));
                }
            }
            ItemKind::Chisel { family, precise, .. } => {
                let how = match family {
                    ShapeFamily::Square => "square",
                    ShapeFamily::Slope => "sloped",
                    ShapeFamily::Bevel => "bevelled",
                    ShapeFamily::Round => "rounded",
                };
                let size = if precise { "an eighth" } else { "an edge" };
                lines.push(format!("Cuts blocks into {how} shapes, {size} at a time"));
                lines.push("Right click fills a cut back".into());
                if let Some((left, total)) = self.durability(stack) {
                    lines.push(format!("{left} of {total} uses left"));
                }
            }
            ItemKind::Grapple { range, hookshot, .. } => {
                lines.push(format!(
                    "Fires a hook up to {} blocks with left click",
                    trim_float(range)
                ));
                lines.push(if hookshot {
                    "Pulls you in by itself; click again to let go".into()
                } else {
                    "R reels in, F lets out; click again to let go".into()
                });
                if let Some((left, total)) = self.durability(stack) {
                    lines.push(format!("{left} of {total} uses left"));
                }
            }
            ItemKind::PortalToken => lines.push(match self.token_name(stack.tag) {
                Some(name) => format!("Links to: {name}"),
                None => "Links two portals of the same size".into(),
            }),
            ItemKind::Block(_) => lines.push("A block you can place".into()),
            ItemKind::Material => lines.push("A crafting material".into()),
        }
        if stack.count > 1 {
            lines.push(format!("{} in this stack", stack.count));
        }
        lines.join("\n")
    }

    /// How many times `recipe` can be made from what is carried now.
    pub fn craft_max(&self, recipe: &Recipe) -> u16 {
        recipe
            .inputs
            .iter()
            .map(|(item, n)| (self.inventory.count(*item) / (*n).max(1) as u32).min(u16::MAX as u32) as u16)
            .min()
            .unwrap_or(0)
    }
}

fn tool_materials(class: ToolClass) -> &'static str {
    match class {
        ToolClass::Hammer => "rock, ore and crystal",
        ToolClass::Shovel => "soil, sand, gravel and snow",
        ToolClass::Axe => "wood and leaves",
    }
}

/// `2.5` -> "2.5", `4.0` -> "4".
fn trim_float(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Up to two letters for an icon with no picture: the first letter of the
/// first two words ("Copper Ore" -> "CO"), or the first two of a single word
/// ("Coal" -> "Co").
pub fn initials(display: &str) -> String {
    let words: Vec<&str> = display.split_whitespace().collect();
    match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(2).collect(),
        [a, b, ..] => a
            .chars()
            .take(1)
            .chain(b.chars().take(1))
            .collect::<String>()
            .to_uppercase(),
    }
}

/// A stable, saturated colour for an item with no picture, so each item
/// keeps its colour between runs and players.
pub fn fallback_colour(name: &str) -> [u8; 3] {
    // FNV-1a: tiny and stable across platforms and releases.
    let mut h: u32 = 0x811c_9dc5;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    let hue = (h % 360) as f32;
    hsv_to_rgb(hue, 0.55, 0.62)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b].map(|v| ((v + m) * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Items {
        let blocks = BlockRegistry::builtin();
        let reg = ItemRegistry::builtin(&blocks);
        let book = RecipeBook::builtin(&reg);
        Items::new(
            reg.defs().to_vec(),
            reg.hand(),
            book.recipes,
            Inventory::default(),
            ColourPalette::builtin(),
            &blocks,
        )
        .unwrap()
    }

    fn stack(i: &Items, name: &str, count: u16) -> Option<ItemStack> {
        Some(ItemStack::new(i.registry.id(name).unwrap(), count))
    }

    #[test]
    fn slot_updates_apply_and_bad_ones_are_ignored() {
        let mut i = items();
        let stone = stack(&i, "stone", 5);
        i.apply_slots(&[(3, stone), (200, stone)]);
        assert_eq!(i.slot(3), stone);
        i.apply_slots(&[(3, None)]);
        assert_eq!(i.slot(3), None);
        let bogus = Some(ItemStack::new(bm_core::ItemId(60000), 1));
        i.apply_slots(&[
            (4, bogus),
            (5, Some(ItemStack::new(i.registry.id("stone").unwrap(), 0))),
        ]);
        assert_eq!((i.slot(4), i.slot(5)), (None, None));
    }

    #[test]
    fn a_short_inventory_from_the_server_is_padded() {
        let blocks = BlockRegistry::builtin();
        let reg = ItemRegistry::builtin(&blocks);
        let inv = Inventory { slots: vec![None; 3] };
        let i = Items::new(
            reg.defs().to_vec(),
            reg.hand(),
            vec![],
            inv,
            ColourPalette::builtin(),
            &blocks,
        )
        .unwrap();
        assert_eq!(i.inventory.slots.len(), INVENTORY_SLOTS);
    }

    #[test]
    fn drags_follow_the_servers_move_rules() {
        let mut i = items();
        i.inventory.slots[0] = stack(&i, "stone", 9);
        i.inventory.slots[1] = stack(&i, "soil", 4);
        i.inventory.slots[2] = stack(&i, "stone", 100);
        let mv = |from: u8, to: u8, count: u16| Some(ClientMsg::MoveItems { from, to, count });
        // Whole stack onto an empty slot, and half of it (rounded up).
        assert_eq!(i.move_request(0, 7, false), mv(0, 7, 9));
        assert_eq!(i.move_request(0, 7, true), mv(0, 7, 5));
        // Whole stacks of different items swap; half stacks cannot.
        assert_eq!(i.move_request(0, 1, false), mv(0, 1, 9));
        assert_eq!(i.move_request(0, 1, true), None);
        // Nothing to move, onto itself, onto a full stack, off the end.
        assert_eq!(i.move_request(5, 6, false), None);
        assert_eq!(i.move_request(0, 0, false), None);
        assert_eq!(i.move_request(0, 2, false), None);
        assert_eq!(i.move_request(0, 40, false), None);
        // Merging asks for the whole stack; the server moves what fits.
        assert_eq!(i.move_request(2, 0, false), mv(2, 0, 100));
        assert_eq!(half(1), 1);
        assert_eq!(half(7), 4);
    }

    #[test]
    fn tools_show_their_wear() {
        let i = items();
        let mut hammer = stack(&i, "stone_hammer", 1).unwrap();
        hammer.wear = 30;
        assert_eq!(i.durability(&hammer), Some((570, 600)));
        let tip = i.tooltip(&hammer);
        assert!(tip.starts_with("Stone Hammer\n"), "{tip}");
        assert!(
            tip.contains("400 damage, 1.22 hits a second") && tip.contains("570 of 600 uses left"),
            "{tip}"
        );
        assert_eq!(i.durability(&stack(&i, "stone", 1).unwrap()), None);
        assert!(!tip.contains('\u{2014}'), "no em dashes in player text");
    }

    #[test]
    fn tooltips_name_the_colour_after_the_item() {
        let i = items();
        let stone = i.registry.id("stone").unwrap();
        let azure = i.colours.id("Rich Azure").unwrap();
        let blue = ItemStack::coloured(stone, azure, 3);
        assert_eq!(i.name(&blue), "Stone, Rich Azure");
        let tip = i.tooltip(&blue);
        assert!(tip.starts_with("Stone, Rich Azure\nA block you can place"), "{tip}");
        assert_eq!(i.name(&ItemStack::new(stone, 1)), "Stone", "natural says nothing");
        // An id the palette does not hold reads as natural, never a panic.
        let short = Items {
            colours: ColourPalette::from_defs(i.colours.colours[..3].to_vec()).unwrap(),
            ..items()
        };
        assert_eq!(short.name(&ItemStack::coloured(stone, Colour(200), 1)), "Stone");
        assert!(!tip.contains('\u{2014}'));
    }

    #[test]
    fn token_tooltips_name_where_they_lead() {
        let mut i = items();
        let token = i.registry.id("portal_token").unwrap();
        let bound = ItemStack::tagged(token, 17);
        assert_eq!(
            i.tooltip(&bound),
            "Portal Token\nLinks two portals of the same size",
            "before the server names it"
        );
        i.set_token_names(vec![(17, "2 by 3, Verdance, near 812, 140".into())]);
        assert_eq!(
            i.tooltip(&bound),
            "Portal Token\nLinks to: 2 by 3, Verdance, near 812, 140"
        );
        // Another token keeps the general line; a later name replaces one.
        assert!(i.tooltip(&ItemStack::tagged(token, 18)).ends_with("same size"));
        i.set_token_names(vec![(17, "1 by 2, Saffra, near 5, 6".into())]);
        assert!(i.tooltip(&bound).ends_with("Links to: 1 by 2, Saffra, near 5, 6"));
        assert!(!i.tooltip(&bound).contains('\u{2014}'));
    }

    #[test]
    fn craft_max_counts_every_colour() {
        let mut i = items();
        let hammer = i.registry.id("stone_hammer").unwrap();
        let r = i.recipes.recipes.iter().find(|r| r.output == hammer).unwrap().clone();
        let stone = i.registry.id("stone").unwrap();
        i.inventory.slots[0] = Some(ItemStack::coloured(stone, Colour(5), 2));
        i.inventory.slots[1] = Some(ItemStack::new(stone, 1));
        i.inventory.slots[2] = stack(&i, "timber", 2);
        assert_eq!(i.craft_max(&r), 1, "two blue and one natural stone make three");
    }

    #[test]
    fn craft_max_is_limited_by_the_scarcest_input() {
        let mut i = items();
        let hammer = i.registry.id("stone_hammer").unwrap();
        let r = i.recipes.recipes.iter().find(|r| r.output == hammer).unwrap().clone();
        assert_eq!(i.craft_max(&r), 0);
        i.inventory.slots[0] = stack(&i, "stone", 10);
        i.inventory.slots[1] = stack(&i, "timber", 5);
        assert_eq!(i.craft_max(&r), 2); // 3 stone and 2 timber each
    }

    #[test]
    fn fallback_icons_have_initials_and_stable_colours() {
        assert_eq!(initials("Copper Ore"), "CO");
        assert_eq!(initials("Coal"), "Co");
        assert_eq!(initials(""), "?");
        assert_eq!(fallback_colour("coal"), fallback_colour("coal"));
        assert_ne!(fallback_colour("coal"), fallback_colour("copper_ore"));
    }
}
