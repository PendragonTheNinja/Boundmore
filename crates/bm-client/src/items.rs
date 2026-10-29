//! The client's copy of the player's items: the catalogue and recipes from
//! the welcome, and the inventory the server keeps telling us about.
//!
//! The server is the authority (ADR-0004): the client never changes the
//! inventory itself. It only asks (`MoveItems`, `Discard`, `Craft`) and shows
//! the answer (`Slots`). The checks here exist to avoid asking for moves the
//! server would refuse, using the same `Inventory` rules it runs.

use bm_core::forge::{Action, Gear, Held, IngredientDef, TraitKind};
use bm_core::items::{BombStats, SlingStats, ToolStats};
use bm_core::{
    BlockPos, BlockRegistry, Colour, ColourPalette, INVENTORY_SLOTS, Input, Inventory, ItemDef, ItemError, ItemId,
    ItemKind, ItemRegistry, ItemStack, Recipe, RecipeBook, ShapeFamily, Station, ToolClass,
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
        recipes: Vec<Recipe>,
        mut inventory: Inventory,
        colours: ColourPalette,
        blocks: &BlockRegistry,
    ) -> Result<Self, ItemError> {
        let registry = ItemRegistry::from_defs(defs, blocks)?;
        // Index safety for everything below: a player's 40 smart stacks,
        // every stack valid.
        inventory.group = bm_core::inventory::SMART;
        inventory.sanitize(&registry);
        let recipes = recipes
            .into_iter()
            .filter(|r| {
                registry.contains(r.output)
                    && r.inputs
                        .iter()
                        .all(|input| input.items.iter().all(|i| registry.contains(*i)))
            })
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
    pub fn apply_slots(&mut self, changes: &[(u16, Option<ItemStack>)]) {
        for (slot, stack) in changes {
            let i = *slot as usize;
            if i >= self.inventory.slots.len() {
                continue;
            }
            self.inventory.slots[i] = stack.filter(|s| self.registry.contains(s.item) && s.count > 0);
        }
    }

    pub fn def(&self, stack: &ItemStack) -> Option<&ItemDef> {
        self.registry.get(stack.item)
    }

    /// What visible slot `slot` shows and a hand holding it uses: its
    /// smart stack's active stack (milestone 33).
    pub fn slot(&self, slot: usize) -> Option<ItemStack> {
        self.inventory.held(slot)
    }

    /// The stored index messages name for visible slot `g`: its active
    /// stack, or its first sub-slot when it is empty.
    pub fn at(&self, g: usize) -> u16 {
        self.inventory.active(g).unwrap_or(g * bm_core::inventory::SMART) as u16
    }

    /// The first hotbar slot holding a tool that breaks blocks, if any.
    pub fn first_tool_slot(&self) -> Option<usize> {
        (0..bm_core::HOTBAR_SLOTS).find(|&i| {
            self.tool_of(self.slot(i).as_ref(), &Held::default())
                .is_some_and(|t| t.class != Some(bm_core::items::ToolClass::Spanner))
        })
    }

    /// The slot the left hand starts on: the first slot, unless it is empty
    /// and a tool sits elsewhere on the hotbar (a new character's hand stone
    /// in slot 10), so the first click breaks something.
    pub fn starting_hand(&self) -> usize {
        if self.slot(0).is_some() {
            0
        } else {
            self.first_tool_slot().unwrap_or(0)
        }
    }

    /// The message for dragging visible slot `from` onto `to` (milestone
    /// 33): the whole smart stack (`MoveGroup`: into an empty slot, merged
    /// into one of its family, or swapped), or half of its active stack
    /// (`MoveItems` onto where the server would put it). None when the
    /// server would refuse it anyway.
    pub fn move_request(&self, from: usize, to: usize, half_stack: bool) -> Option<ClientMsg> {
        if from >= INVENTORY_SLOTS || to >= INVENTORY_SLOTS || from == to {
            return None;
        }
        let mut trial = self.inventory.clone();
        if !half_stack {
            trial.move_group(&self.registry, from, to)?;
            return Some(ClientMsg::MoveGroup {
                from: from as u8,
                to: to as u8,
            });
        }
        let src_at = self.inventory.active(from)?;
        let src = self.inventory.get(src_at)?;
        let count = half(src.count);
        let dst = self.inventory.drop_target(&self.registry, src_at, to, count)?;
        trial.move_items(&self.registry, src_at, dst, count)?;
        Some(ClientMsg::MoveItems {
            from: src_at as u16,
            to: dst as u16,
            count,
        })
    }

    /// The message moving one stack (stored index `from`, an opened smart
    /// stack's sub-slot) onto visible slot `to`: what fits where the server
    /// would put it. None when it cannot go there.
    pub fn move_stack_request(&self, from: usize, to: usize) -> Option<ClientMsg> {
        let src = self.inventory.get(from)?;
        let dst = self.inventory.drop_target(&self.registry, from, to, src.count)?;
        let mut trial = self.inventory.clone();
        trial.move_items(&self.registry, from, dst, src.count)?;
        Some(ClientMsg::MoveItems {
            from: from as u16,
            to: dst as u16,
            count: src.count,
        })
    }

    /// Uses left and in all, for a tool (Sturdy and Brittle counted).
    pub fn durability(&self, stack: &ItemStack) -> Option<(u16, u16)> {
        let total = self.def(stack)?.durability_of(stack)?;
        Some((total.saturating_sub(stack.wear), total))
    }

    /// What a held stack does, through its traits (milestone 17): every
    /// prediction reads gear stats here, with `held` built as the server
    /// builds it (`Held::of`), so forged gear predicts exactly.
    pub fn tool_of(&self, stack: Option<&ItemStack>, held: &Held) -> Option<ToolStats> {
        let s = stack?;
        self.def(s)?.tool_of(s, held)
    }

    pub fn sling_of(&self, stack: Option<&ItemStack>, held: &Held) -> Option<SlingStats> {
        let s = stack?;
        self.def(s)?.sling_of(s, held)
    }

    pub fn bomb_of(&self, stack: Option<&ItemStack>, held: &Held) -> Option<BombStats> {
        let s = stack?;
        self.def(s)?.bomb_of(s, held)
    }

    pub fn energy_of(&self, stack: Option<&ItemStack>, held: &Held) -> Option<u16> {
        let s = stack?;
        self.def(s)?.energy_of(s, held)
    }

    /// The grapple spec the movement step is given, as the server builds it:
    /// through the stack's traits with no quirk conditions
    /// (`Held::default`), and its shot's energy by the holder (`held`, as
    /// they stand before the step: Sated and Ravenous).
    pub fn grapple_of(&self, stack: Option<&ItemStack>, held: &Held) -> Option<bm_core::movement::GrappleSpec> {
        let s = stack?;
        let d = self.def(s)?;
        let mut g = d.grapple_of(s, &Held::default())?;
        g.energy = d.energy_of(s, held).unwrap_or(g.energy);
        Some(g)
    }

    /// Light a held stack gives (a torch, a Glowing tool).
    /// The light held in hand per channel (milestone 26).
    pub fn held_rgb_of(&self, stack: Option<&ItemStack>) -> [u8; 3] {
        stack
            .and_then(|s| self.def(s).map(|d| d.held_rgb_of(s)))
            .unwrap_or([0; 3])
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
        // The stats a tooltip shows are the stack's own, through its traits;
        // quirks that need a time of day, health or hunger are left out.
        let held = Held::default();
        match def.kind {
            ItemKind::Tool(t) if t.class == Some(ToolClass::Spanner) => {
                let t = def.tool_of(stack, &held).unwrap_or(t);
                lines.push(spanner_line(&t));
                lines.push(format!(
                    "Breaks nothing; {} hits a second",
                    trim_float((1.0 / bm_core::mining::seconds_per_hit(t.action_speed)) as f32)
                ));
                if let Some((left, total)) = self.durability(stack) {
                    lines.push(format!("{left} of {total} uses left"));
                }
            }
            ItemKind::Tool(t) => {
                let t = def.tool_of(stack, &held).unwrap_or(t);
                lines.push(match t.class {
                    Some(class) => format!("Best on {}; a quarter of the damage on the rest", tool_materials(class)),
                    None => "Works on everything".into(),
                });
                lines.push(format!(
                    "{} damage, {} hits a second",
                    t.damage,
                    trim_float((1.0 / bm_core::mining::seconds_per_hit(t.action_speed)) as f32)
                ));
                if t.crit_chance > 0.0 {
                    lines.push(format!(
                        "{}% chance of a critical hit, {} times the damage",
                        trim_float(t.crit_chance * 100.0),
                        trim_float(1.0 + t.crit_effect)
                    ));
                }
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
                let (range, hookshot) = def
                    .grapple_of(stack, &held)
                    .map_or((range, hookshot), |g| (g.range as f32, g.hookshot));
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
            ItemKind::Food(f) => lines.extend(food_lines(&f)),
            ItemKind::Ingredient(i) => match bm_core::forge::ingredient(i) {
                Some(d) => lines.extend(ingredient_lines(d)),
                None => lines.push("Used in a forge".into()),
            },
            ItemKind::Bucket { fills } => {
                lines.push("Left click on water fills it".into());
                lines.push(format!("{} of {fills} fills left", fills.saturating_sub(stack.wear)));
            }
            ItemKind::Bomb(b) => {
                let b = def.bomb_of(stack, &held).unwrap_or(b);
                lines.push("Hold to light, let go to throw".into());
                lines.push(format!(
                    "{} damage within {} blocks, half at the edge",
                    b.damage,
                    trim_float(b.radius)
                ));
                // The true cadence, as thrown: whole ticks between throws.
                let per_second = bm_core::TICK_HZ as f32 / bm_core::mining::swing_ticks(b.action_speed) as f32;
                lines.push(format!("{} throws a second", trim_float(per_second)));
                lines.push(format!(
                    "A tap drops it at your feet; the fuse burns {} seconds",
                    trim_float(bm_core::combat::BOMB_FUSE_TICKS as f32 / bm_core::TICK_HZ as f32)
                ));
                if b.crit_chance > 0.0 {
                    lines.push(format!(
                        "{}% chance of a critical hit, {} times the damage",
                        trim_float(b.crit_chance * 100.0),
                        trim_float(1.0 + b.crit_effect)
                    ));
                }
                lines.push(format!("{} energy a throw", b.energy));
                if let Some((left, total)) = self.durability(stack) {
                    lines.push(format!("{left} of {total} throws left"));
                }
            }
            ItemKind::Sling(l) => {
                let l = def.sling_of(stack, &held).unwrap_or(l);
                lines.push(format!(
                    "Shoots {} damage up to {} blocks with left click",
                    l.damage,
                    trim_float(l.range)
                ));
                // The true cadence: whole ticks between shots, as fired.
                let per_second = bm_core::TICK_HZ as f32 / bm_core::mining::swing_ticks(l.action_speed) as f32;
                lines.push(format!(
                    "{} shots a second, flying {} blocks a second",
                    trim_float(per_second),
                    trim_float(l.shot_speed)
                ));
                if l.crit_chance > 0.0 {
                    lines.push(format!(
                        "{}% chance of a critical hit, {} times the damage",
                        trim_float(l.crit_chance * 100.0),
                        trim_float(1.0 + l.crit_effect)
                    ));
                }
                lines.push(format!("{} energy a shot", l.energy));
                if let Some((left, total)) = self.durability(stack) {
                    lines.push(format!("{left} of {total} uses left"));
                }
            }
            ItemKind::Atlas => {
                lines.push("A map of one world: right click to open it".into());
                lines.push(match bm_core::atlas::bound(stack.tag) {
                    Some(_) => "Bound to the world it maps".into(),
                    None => "Not bound yet: it will map the first world you open it on".into(),
                });
                if let Some(item) = bm_core::atlas::loaded(stack.tag)
                    && let Some(def) = self.registry.get(item)
                {
                    lines.push(format!("Shows where to find {}", def.display));
                }
            }
            ItemKind::PortalToken => lines.push(match self.token_name(stack.tag) {
                Some(name) => format!("Links to: {name}"),
                None => "Links two portals of the same size".into(),
            }),
            ItemKind::Block(_) if def.held_light > 0 => {
                lines.push("Stands on a floor or hangs on a wall".into());
                lines.push("Lights the way while held".into());
            }
            ItemKind::Block(_) => lines.push("A block you can place".into()),
            ItemKind::Material => lines.push("A crafting material".into()),
        }
        lines.extend(forge_lines(def, stack));
        if stack.count > 1 {
            lines.push(format!("{} in this stack", stack.count));
        }
        lines.join("\n")
    }

    /// How many times `recipe` can be made from what is carried now.
    pub fn craft_max(&self, recipe: &Recipe) -> u16 {
        if recipe.inputs.is_empty() {
            return 0;
        }
        recipe.max_crafts(&self.inventory).min(u16::MAX as u32) as u16
    }

    /// How a recipe input reads in a crafting screen: the item whose icon
    /// it shows (a group's first member carried, else its first), its words
    /// ("Any Rock 3/4", or "3/4" beside a plain item's icon), how many are
    /// carried (a group's members together), and for a group what serves.
    pub fn input_view(&self, input: &Input) -> InputView {
        let have = input.have(&self.inventory);
        let icon = input
            .items
            .iter()
            .copied()
            .find(|i| self.inventory.count(*i) > 0)
            .or(input.items.first().copied())
            .unwrap_or(ItemId(0));
        let count = format!("{have}/{}", input.count);
        let (label, hover) = match &input.group {
            Some(group) => {
                let names: Vec<String> = input
                    .items
                    .iter()
                    .filter_map(|i| self.registry.get(*i).map(|d| d.display.clone()))
                    .collect();
                (
                    format!("{group} {count}"),
                    Some(format!("{group}: {}", names.join(", "))),
                )
            }
            None => (count, None),
        };
        InputView {
            icon,
            label,
            have,
            hover,
        }
    }

    /// The recipes a crafting screen offers, with their indices: in the
    /// hands only hand recipes; a crafting table its own and the hand ones
    /// (milestone 11, as in Boundless); any other machine only its own.
    pub fn recipes_for(&self, station: Station) -> Vec<(u16, &Recipe)> {
        let mut out: Vec<(u16, &Recipe)> = self.recipes.at(station).collect();
        if station == Station::CraftingTable {
            out.extend(self.recipes.at(Station::Hand));
        }
        out
    }

    /// Heat the item gives burnt in a furnace (0: not fuel).
    pub fn fuel(&self, item: ItemId) -> u16 {
        self.registry.get(item).map(|d| d.fuel).unwrap_or(0)
    }

    /// The stored slots holding fuel, in slot order (every stack, inside
    /// smart stacks too).
    pub fn fuel_slots(&self) -> Vec<u16> {
        (0..self.inventory.slots.len())
            .filter(|&i| self.inventory.get(i).is_some_and(|s| self.fuel(s.item) > 0))
            .map(|i| i as u16)
            .collect()
    }

    /// The message putting fuel from pack `slot` into the furnace at `pos`
    /// (the whole stack, or one), or None when the slot holds no fuel.
    pub fn fuel_in(&self, pos: BlockPos, slot: u16, one: bool) -> Option<ClientMsg> {
        let s = self.inventory.get(slot as usize)?;
        (self.fuel(s.item) > 0).then_some(ClientMsg::StationFuel {
            pos,
            slot,
            count: if one { 1 } else { s.count },
            into: true,
        })
    }

    /// The message taking a furnace's fuel back out: into a stack of the
    /// same item with room, else the first empty slot; None when the pack
    /// has no room for any.
    pub fn fuel_out(&self, pos: BlockPos, fuel: &ItemStack) -> Option<ClientMsg> {
        // Where the pack would take it: the first stored slot an add
        // changes (a stack it tops up, a sub-slot of its smart stack, or an
        // empty slot), and how much fits there.
        let mut trial = self.inventory.clone();
        trial.add_stack(&self.registry, *fuel);
        let slot = (0..trial.slots.len()).find(|&i| trial.slots[i] != self.inventory.slots[i])?;
        let before = self.inventory.get(slot).map_or(0, |s| s.count);
        let room = trial.get(slot).map_or(0, |s| s.count) - before;
        Some(ClientMsg::StationFuel {
            pos,
            slot: slot as u16,
            count: fuel.count.min(room),
            into: false,
        })
    }
}

fn tool_materials(class: ToolClass) -> &'static str {
    match class {
        ToolClass::Hammer => "rock, ore and crystal",
        ToolClass::Shovel => "soil, sand, gravel and snow",
        ToolClass::Axe => "wood and leaves",
        ToolClass::Spanner => "machines (repairs wear)",
        ToolClass::Tiller => "soil and turf (tills them for crops) and crops (harvests them)",
    }
}

/// A spanner's first tooltip line: "Repairs machines: 700 wear a hit".
pub fn spanner_line(t: &bm_core::items::ToolStats) -> String {
    format!(
        "Repairs machines: {} wear a hit",
        crate::skill_screen::grouped(t.repair as u64)
    )
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

/// What eating one does, for its tooltip (milestone 16).
pub fn food_lines(f: &bm_core::food::FoodStats) -> Vec<String> {
    use bm_core::brew::Brew;
    if f.brew != Brew::None {
        // A brew (milestone 23 part 2).
        let mut lines = vec!["Left click drinks one".to_string(), f.brew.describe(f.rank)];
        lines.push(match f.brew {
            b if b.instant() => "Works at once; keeps your brew".into(),
            Brew::Lifesaving => "Kept beside your other brew".into(),
            _ => "Replaces any other brew".into(),
        });
        return lines;
    }
    let mut lines = vec!["Left click eats one".to_string()];
    if f.hunger > 0 {
        lines.push(format!("Takes away {} hunger", f.hunger));
    }
    if f.buff != bm_core::food::Buff::None {
        lines.push(format!(
            "{} {}: {} for {} minutes",
            f.buff.name(),
            roman(f.rank),
            f.buff.describe(f.rank),
            f.minutes
        ));
        lines.push("Replaces any other food buff".into());
    }
    if f.sick {
        lines.push("Raw: makes you sick for 40 seconds".into());
    }
    lines
}

/// What forging did to a piece of gear, for its tooltip (milestone 17): its
/// rank and its traits by kind ("Mighty 5: +250 damage and +30%"), or that
/// it can still be forged once. Nothing for what cannot be forged.
pub fn forge_lines(def: &ItemDef, stack: &ItemStack) -> Vec<String> {
    if Gear::of(def).is_none() {
        return Vec::new();
    }
    let f = &stack.forge;
    if !f.done {
        return vec![format!("Rank {}. Can be forged once", def.rank)];
    }
    let mut lines = vec![format!("Forged, rank {}", f.rank(def.rank))];
    let mut any = false;
    for kind in TraitKind::ALL {
        let of_kind: Vec<String> = f
            .traits()
            .filter(|(id, _)| id.def().kind == kind)
            .map(|(id, level)| trait_line(id, level))
            .collect();
        if of_kind.is_empty() {
            continue;
        }
        any = true;
        lines.push(format!("{}:", kind_title(kind)));
        lines.extend(of_kind.into_iter().map(|l| format!("  {l}")));
    }
    if !any {
        lines.push("No traits".into());
    }
    lines
}

/// "Mighty 5: +250 damage and +30%"; a one-level trait has no number.
pub fn trait_line(id: bm_core::forge::TraitId, level: u8) -> String {
    let d = id.def();
    if d.levels.len() == 1 {
        format!("{}: {}", d.display, d.describe(level))
    } else {
        format!("{} {level}: {}", d.display, d.describe(level))
    }
}

/// "Boons", "Quirks", "Flaws".
pub fn kind_title(kind: TraitKind) -> &'static str {
    match kind {
        TraitKind::Boon => "Boons",
        TraitKind::Quirk => "Quirks",
        TraitKind::Flaw => "Flaws",
    }
}

/// "100 boon points", "200 boon, 100 quirk and 50 flaw points".
fn points_text(parts: &[(i32, &str)]) -> String {
    let words: Vec<String> = parts.iter().map(|(n, k)| format!("{} {k}", n.abs())).collect();
    match words.as_slice() {
        [] => String::new(),
        [one] => format!("{one} points"),
        [rest @ .., last] => format!("{} and {last} points", rest.join(", ")),
    }
}

/// What a forging ingredient does and costs, in plain words, for its
/// tooltip and the forge's deck (milestone 17).
pub fn ingredient_lines(d: &IngredientDef) -> Vec<String> {
    vec![ingredient_effect(d), ingredient_cost(d)]
}

/// The ingredient's effect in one line.
pub fn ingredient_effect(d: &IngredientDef) -> String {
    match d.action {
        Action::Tincture {
            boon,
            quirk,
            flaw,
            after,
        } => {
            let named = [(boon, "boon"), (quirk, "quirk"), (flaw, "flaw")];
            let adds: Vec<(i32, &str)> = named.iter().copied().filter(|(n, _)| *n > 0).collect();
            let takes: Vec<(i32, &str)> = named.iter().copied().filter(|(n, _)| *n < 0).collect();
            let mut parts = Vec::new();
            if !adds.is_empty() {
                parts.push(format!("Adds {}", points_text(&adds)));
            }
            if !takes.is_empty() {
                let verb = if parts.is_empty() { "Takes" } else { "takes" };
                parts.push(format!("{verb} away {}", points_text(&takes)));
            }
            let mut text = parts.join(" and ");
            text.push_str(", then every pool rolls");
            if let Some((buff, rounds)) = after {
                text.push_str(&format!("; then {} for {rounds} rounds", buff.describe()));
            }
            text
        }
        Action::Glaze(buff) => format!("For the rest of the session: {} (each one adds more)", buff.describe()),
        Action::Salve(buff, rounds) => format!("For {rounds} rounds: {}", buff.describe()),
        Action::VigourSpur(per) => format!("Ends every buff, giving {per} vigour for each round it had left"),
        Action::PointsSpur(per) => format!("Turns each round of second chances into {per} boon points"),
        Action::BoostSpur => "Turns rounds of second chances into rounds of boon points +50%".into(),
        Action::FlawTurningSpur => "Moves half the flaw pool to the boon pool".into(),
        Action::Wash(kind, share) => format!(
            "Takes the newest {} off, {}% of its points back into the pool, then that pool rolls",
            kind.word(),
            (share * 100.0).round()
        ),
        Action::ShiftWash(kind) => format!("Moves the newest {}'s points to another {}", kind.word(), kind.word()),
        Action::Seal => "Ends the session: the gear goes into the forge's queue with its traits".into(),
        Action::Unmake(chance) => format!(
            "Ends the session: the gear comes back unforged, and each kind of ingredient used comes back with a {}% chance",
            (chance * 100.0).round()
        ),
    }
}

/// "Costs 50 vigour and 100 to 110 stability", "Costs nothing".
pub fn ingredient_cost(d: &IngredientDef) -> String {
    let mut parts = Vec::new();
    if d.vigour > 0 {
        parts.push(format!("{} vigour", d.vigour));
    }
    if d.stability > 0 {
        parts.push(format!(
            "{} to {} stability",
            d.stability,
            (d.stability as f64 * 1.1).round() as i32
        ));
    }
    if parts.is_empty() {
        "Costs nothing".into()
    } else {
        format!("Costs {}", parts.join(" and "))
    }
}

/// A recipe input as a crafting screen shows it ([`Items::input_view`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputView {
    pub icon: ItemId,
    pub label: String,
    pub have: u32,
    /// A group's members, for its tooltip.
    pub hover: Option<String>,
}

/// 1 to 5 as I to V, for buff ranks.
pub fn roman(n: u8) -> &'static str {
    ["", "I", "II", "III", "IV", "V"].get(n as usize).copied().unwrap_or("")
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

    /// A new character's first click breaks something (audit 2026-10-29):
    /// with slot 1 empty, the left hand starts on the hand stone in slot 10.
    #[test]
    fn the_left_hand_starts_on_a_tool() {
        let mut i = items();
        assert_eq!(i.starting_hand(), 0, "nothing carried: slot 1");
        i.inventory.slots[9 * bm_core::inventory::SMART] = stack(&i, "hand_stone", 1);
        assert_eq!((i.starting_hand(), i.first_tool_slot()), (9, Some(9)));
        i.inventory.slots[0] = stack(&i, "stone", 5);
        assert_eq!(i.starting_hand(), 0, "something in slot 1 stays chosen");
    }

    #[test]
    fn slot_updates_apply_and_bad_ones_are_ignored() {
        let mut i = items();
        let stone = stack(&i, "stone", 5);
        // Stored indices: visible slot 3's first stack is 27.
        i.apply_slots(&[(27, stone), (4000, stone)]);
        assert_eq!(i.slot(3), stone);
        i.apply_slots(&[(27, None)]);
        assert_eq!(i.slot(3), None);
        let bogus = Some(ItemStack::new(bm_core::ItemId(60000), 1));
        i.apply_slots(&[
            (36, bogus),
            (45, Some(ItemStack::new(i.registry.id("stone").unwrap(), 0))),
        ]);
        assert_eq!((i.slot(4), i.slot(5)), (None, None));
    }

    #[test]
    fn a_short_inventory_from_the_server_is_padded() {
        let blocks = BlockRegistry::builtin();
        let reg = ItemRegistry::builtin(&blocks);
        let inv = Inventory::row(vec![None; 3]);
        let i = Items::new(reg.defs().to_vec(), vec![], inv, ColourPalette::builtin(), &blocks).unwrap();
        assert_eq!(i.inventory.slots.len(), bm_core::inventory::PACK_SLOTS);
    }

    #[test]
    fn drags_follow_the_servers_move_rules() {
        let mut i = items();
        let g = |v: usize| v * bm_core::inventory::SMART;
        i.inventory.slots[g(0)] = stack(&i, "stone", 9);
        i.inventory.slots[g(1)] = stack(&i, "soil", 4);
        i.inventory.slots[g(2)] = stack(&i, "stone", 100);
        let group = |from: u8, to: u8| Some(ClientMsg::MoveGroup { from, to });
        let mv = |from: usize, to: usize, count: u16| {
            Some(ClientMsg::MoveItems {
                from: from as u16,
                to: to as u16,
                count,
            })
        };
        // A whole smart stack onto an empty slot, a different one (swap) or
        // one of its family (merge).
        assert_eq!(i.move_request(0, 7, false), group(0, 7));
        assert_eq!(i.move_request(0, 1, false), group(0, 1));
        assert_eq!(i.move_request(2, 0, false), group(2, 0));
        // Half of the active stack: onto an empty slot, into a smart stack of
        // its family, never into another family's.
        assert_eq!(i.move_request(0, 7, true), mv(g(0), g(7), 5));
        assert_eq!(i.move_request(2, 0, true), mv(g(2), g(0), 50));
        assert_eq!(i.move_request(0, 1, true), None);
        // Nothing to move, onto itself, off the end.
        assert_eq!(i.move_request(5, 6, false), None);
        assert_eq!(i.move_request(0, 0, false), None);
        assert_eq!(i.move_request(0, 40, false), None);
        // One stack out of an opened smart stack.
        i.inventory.slots[g(0) + 1] = stack(&i, "stone", 3);
        assert_eq!(i.move_stack_request(g(0) + 1, 7), mv(g(0) + 1, g(7), 3));
        assert_eq!(i.move_stack_request(g(0) + 1, 1), None, "soil's slot takes no stone");
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
        assert!(tip.contains("Best on rock, ore and crystal"), "{tip}");
        assert!(!tip.contains("critical"), "stone never crits: {tip}");
        assert_eq!(i.durability(&stack(&i, "stone", 1).unwrap()), None);
        assert!(!tip.contains('\u{2014}'), "no em dashes in player text");
        let iron = i.tooltip(&stack(&i, "iron_hammer", 1).unwrap());
        assert!(
            iron.contains("700 damage, 1.1 hits a second")
                && iron.contains("20% chance of a critical hit, 1.2 times the damage"),
            "{iron}"
        );
        let hand = i.tooltip(&stack(&i, "hand_stone", 1).unwrap());
        assert!(
            hand.contains("Works on everything") && hand.contains("200 damage"),
            "{hand}"
        );
        let torch = i.tooltip(&stack(&i, "torch", 4).unwrap());
        assert!(
            torch.contains("hangs on a wall") && torch.contains("while held"),
            "{torch}"
        );
        let mut sling = stack(&i, "iron_sling", 1).unwrap();
        sling.wear = 100;
        let sling = i.tooltip(&sling);
        let per_second = 60.0 / bm_core::mining::swing_ticks(10.0) as f32;
        assert!(
            sling.starts_with("Heavy Iron Sling\n")
                && sling.contains("Shoots 600 damage up to 20 blocks")
                && sling.contains(&format!(
                    "{} shots a second, flying 35 blocks a second",
                    trim_float(per_second)
                ))
                && sling.contains("20% chance of a critical hit, 1.2 times the damage")
                && sling.contains("12 energy a shot")
                && sling.contains("800 of 900 uses left"),
            "{sling}"
        );
        let stone = i.tooltip(&stack(&i, "stone_sling", 1).unwrap());
        assert!(
            stone.contains("6 energy a shot") && !stone.contains("critical"),
            "{stone}"
        );
        let mut bag = stack(&i, "copper_bombs", 1).unwrap();
        bag.wear = 10;
        let bombs = i.tooltip(&bag);
        let per_second = bm_core::TICK_HZ as f32 / bm_core::mining::swing_ticks(50.0) as f32;
        assert!(
            bombs.starts_with("Bag of Copper Bombs\nHold to light, let go to throw\n")
                && bombs.contains("900 damage within 3 blocks")
                && bombs.contains(&format!("{} throws a second", trim_float(per_second)))
                && bombs.contains("the fuse burns 3 seconds")
                && bombs.contains("90 energy a throw")
                && bombs.contains("190 of 200 throws left"),
            "{bombs}"
        );
        for t in [iron, hand, torch, sling, stone, bombs] {
            assert!(!t.contains('\u{2014}'));
        }
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
    fn a_group_input_shows_its_name_and_the_members_together() {
        // (Milestone 22: the stone hammer takes Any Stones.)
        let mut i = items();
        let id = |n: &str| i.registry.id(n).unwrap();
        let (basalt, limestone) = (id("basalt_stones"), id("limestone_stones"));
        let r = i
            .recipes
            .recipes
            .iter()
            .find(|r| r.output == id("stone_hammer"))
            .unwrap()
            .clone();
        let v = i.input_view(&r.inputs[0]);
        assert_eq!((v.label.as_str(), v.have, v.icon), ("Any Stones 0/4", 0, id("stones")));
        assert_eq!(
            v.hover.as_deref(),
            Some("Any Stones: Stones, Basalt Stones, Limestone Stones")
        );
        i.inventory.add(&i.registry, limestone, 2);
        i.inventory.add(&i.registry, basalt, 3);
        let v = i.input_view(&r.inputs[0]);
        assert_eq!((v.label.as_str(), v.have, v.icon), ("Any Stones 5/4", 5, basalt));
        assert!(!v.label.contains('\u{2014}'));
        // A plain item reads as its count only, beside its icon.
        let v = i.input_view(&r.inputs[1]);
        assert_eq!((v.label.as_str(), v.hover.clone()), ("0/4", None));
        assert_eq!(r.inputs[2].group.as_deref(), Some("Any Leaves"));
        i.inventory.add(&i.registry, id("stick"), 4);
        i.inventory.add(&i.registry, id("foliage"), 4);
        assert_eq!(i.craft_max(&r), 1);
    }

    #[test]
    fn craft_max_counts_every_colour() {
        let mut i = items();
        let hammer = i.registry.id("stone_hammer").unwrap();
        let r = i.recipes.recipes.iter().find(|r| r.output == hammer).unwrap().clone();
        // A stone hammer takes 4 foliage (and 4 stones and 4 sticks).
        let foliage = i.registry.id("foliage").unwrap();
        i.inventory.slots[0] = Some(ItemStack::coloured(foliage, Colour(5), 3));
        i.inventory.slots[1] = Some(ItemStack::new(foliage, 1));
        i.inventory.slots[2] = stack(&i, "stones", 4);
        i.inventory.slots[3] = stack(&i, "stick", 4);
        assert_eq!(i.craft_max(&r), 1, "three blue and one natural foliage make four");
    }

    #[test]
    fn craft_max_is_limited_by_the_scarcest_input() {
        let mut i = items();
        let hammer = i.registry.id("stone_hammer").unwrap();
        let r = i.recipes.recipes.iter().find(|r| r.output == hammer).unwrap().clone();
        assert_eq!(i.craft_max(&r), 0);
        i.inventory.slots[0] = stack(&i, "stones", 12);
        i.inventory.slots[1] = stack(&i, "stick", 9);
        i.inventory.slots[2] = stack(&i, "foliage", 20);
        assert_eq!(i.craft_max(&r), 2); // 4 stones, 4 sticks, 4 foliage each
    }

    #[test]
    fn hands_offer_hand_recipes_and_a_table_offers_both() {
        let i = items();
        let stations = |s: Station| -> std::collections::BTreeSet<Station> {
            i.recipes_for(s).iter().map(|(_, r)| r.station).collect()
        };
        assert_eq!(stations(Station::Hand), [Station::Hand].into());
        assert_eq!(
            stations(Station::CraftingTable),
            [Station::Hand, Station::CraftingTable].into()
        );
        assert_eq!(stations(Station::Furnace), [Station::Furnace].into());
        assert_eq!(stations(Station::Extractor), [Station::Extractor].into());
        // Indices stay those of the book, so a craft names the right one.
        for (index, r) in i.recipes_for(Station::CraftingTable) {
            assert_eq!(i.recipes.get(index), Some(r));
        }
        let name = |(_, r): &(u16, &Recipe)| i.registry.get(r.output).unwrap().name.clone();
        let hand: Vec<String> = i.recipes_for(Station::Hand).iter().map(name).collect();
        assert!(hand.contains(&"crafting_table".to_string()));
        assert!(!hand.contains(&"stones".to_string()), "stones need a table");
    }

    #[test]
    fn forged_gear_lists_its_traits_and_shows_its_forged_stats() {
        use bm_core::forge::Forged;
        let i = items();
        let iron = stack(&i, "iron_hammer", 1).unwrap();
        let plain = i.tooltip(&iron);
        assert!(plain.ends_with("Rank 30. Can be forged once"), "{plain}");
        let forged = ItemStack {
            forge: Forged::from_suffix("mighty5,quick3,night_owl1,unlucky1"),
            ..iron
        };
        let tip = i.tooltip(&forged);
        // Mighty 5: 700 x 1.3 + 250; Quick 3: action speed +15.
        let per_second = 1.0 / bm_core::mining::seconds_per_hit(def_speed(&i, "iron_hammer") + 15.0);
        assert!(
            tip.contains(&format!("1160 damage, {} hits a second", trim_float(per_second as f32))),
            "{tip}"
        );
        assert!(tip.contains("15% chance of a critical hit"), "Unlucky 1: {tip}");
        let rank = forged.forge.rank(30);
        assert!(tip.contains(&format!("Forged, rank {rank}")), "{tip}");
        assert!(
            tip.contains("Boons:\n  Mighty 5: +250 damage and +30%\n  Quick 3: +15 action speed\n"),
            "{tip}"
        );
        assert!(
            tip.contains("Quirks:\n  Night Owl: +100 and +10% damage at night"),
            "{tip}"
        );
        assert!(tip.contains("Flaws:\n  Unlucky 1: -5% crit chance"), "{tip}");
        // Forged with no traits; a Sturdy grapple's range and uses.
        let none = ItemStack {
            forge: Forged::from_suffix(""),
            ..iron
        };
        assert!(i.tooltip(&none).ends_with("No traits"));
        let g = ItemStack {
            forge: Forged::from_suffix("long_rope2,sturdy1"),
            ..stack(&i, "copper_grapple", 1).unwrap()
        };
        let gt = i.tooltip(&g);
        assert!(
            gt.contains("Fires a hook up to 16 blocks") && gt.contains("700 of 700 uses left"),
            "{gt}"
        );
        // Not gear: nothing about forging.
        assert!(!i.tooltip(&stack(&i, "stone", 1).unwrap()).contains("forged"));
        assert!(!i.tooltip(&stack(&i, "hand_stone", 1).unwrap()).contains("forged"));
        for t in [plain, tip, gt] {
            assert!(!t.contains('\u{2014}'));
        }
    }

    fn def_speed(i: &Items, name: &str) -> f32 {
        i.registry
            .get(i.registry.id(name).unwrap())
            .unwrap()
            .tool()
            .unwrap()
            .action_speed
    }

    #[test]
    fn ingredients_say_what_they_do_and_cost() {
        let i = items();
        let tip = |n: &str| i.tooltip(&stack(&i, n, 3).unwrap());
        assert_eq!(
            tip("pure_tincture_1"),
            "Pure Tincture I\nAdds 100 boon points, then every pool rolls\nCosts 100 to 110 stability\n3 in this stack"
        );
        assert!(
            tip("wild_tincture_1").contains("Adds 200 boon, 100 quirk and 50 flaw points"),
            "{}",
            tip("wild_tincture_1")
        );
        assert!(tip("mending_tincture_1").contains("Takes away 150 boon and 150 flaw points"));
        assert!(tip("force_glaze").contains("For the rest of the session: leaning to Force boons"));
        assert!(tip("force_glaze").contains("Costs 50 vigour\n"));
        assert!(tip("boost_salve_1").contains("For 2 rounds: boon points +50%"));
        assert!(tip("boost_salve_1").contains("Costs 50 vigour and 100 to 110 stability"));
        assert!(tip("sealing_wax").contains("Ends the session") && tip("sealing_wax").contains("Costs nothing"));
        assert!(tip("unmaking_wax_1").contains("a 25% chance"));
        for d in bm_core::forge::INGREDIENTS {
            let t = ingredient_lines(d).join("\n");
            assert!(!t.contains('\u{2014}') && !t.is_empty(), "{}", d.name);
        }
    }

    #[test]
    fn spanners_say_what_they_repair() {
        let i = items();
        for (name, repair) in [("stone_spanner", 100), ("copper_spanner", 400), ("iron_spanner", 700)] {
            let id = i.registry.id(name).unwrap();
            let tip = i.tooltip(&ItemStack::new(id, 1));
            let want = format!("Repairs machines: {repair} wear a hit");
            assert!(tip.lines().nth(1) == Some(want.as_str()), "{name}: {tip}");
            assert!(!tip.contains("damage"), "{name}: {tip}");
        }
    }

    #[test]
    fn fuel_goes_in_from_the_pack_and_comes_back_out_where_it_fits() {
        let mut i = items();
        let pos = BlockPos::new(4, 5, 6);
        i.inventory.slots[18] = stack(&i, "coal", 9);
        i.inventory.slots[27] = stack(&i, "stone", 9);
        i.inventory.slots[45] = stack(&i, "stick", 4);
        assert_eq!(i.fuel_slots(), vec![18, 45], "coal and sticks burn, stone does not");
        assert_eq!(
            i.fuel_in(pos, 18, false),
            Some(ClientMsg::StationFuel {
                pos,
                slot: 18,
                count: 9,
                into: true
            })
        );
        assert_eq!(
            i.fuel_in(pos, 45, true),
            Some(ClientMsg::StationFuel {
                pos,
                slot: 45,
                count: 1,
                into: true
            })
        );
        assert_eq!(i.fuel_in(pos, 27, false), None);
        assert_eq!(i.fuel_in(pos, 63, false), None);
        // Out: onto the coal stack there, else into the coal's smart stack,
        // else into the first empty slot.
        let coal = stack(&i, "coal", 30).unwrap();
        let out = |i: &Items| match i.fuel_out(pos, &coal) {
            Some(ClientMsg::StationFuel { slot, count, into, .. }) => {
                assert!(!into);
                Some((slot, count))
            }
            _ => None,
        };
        assert_eq!(out(&i), Some((18, 30)));
        i.inventory.slots[18] = stack(&i, "coal", 100);
        assert_eq!(out(&i), Some((19, 30)));
        i.inventory.slots[18] = None;
        assert_eq!(out(&i), Some((0, 30)), "no coal carried: the first empty slot");
        for s in 0..i.inventory.slots.len() {
            i.inventory.slots[s] = stack(&i, "stone", 1);
        }
        assert_eq!(out(&i), None, "a full pack takes nothing");
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
