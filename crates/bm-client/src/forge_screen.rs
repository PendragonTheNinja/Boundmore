//! The forge's window (milestone 17, `docs/milestones/17-forging.md`).
//!
//! Before a session: choose the gear and the deck, and see what the session
//! would start with (computed here from the character's own skills, as the
//! server will). During one: the session as the server last sent it
//! (`SessionView`), and the deck as buttons, each a round (`ForgeUse`). The
//! server rolls every die; nothing here guesses a round's result.

use crate::items::{Items, ingredient_cost, ingredient_effect, kind_title};
use crate::menus::{
    GOLD, GOLD_DIM, ItemIcons, LiveStation, MUTED, SHORT, SLOT_RIM, TEXT, UiAction, heading, idle_banner, item_chip,
    machine_panel, paint_slot_back, paint_stack, thin_bar, window,
};
use bm_core::forge::{
    Action, BuffKind, DECK_STACK, Forger, Gear, IngredientDef, POOL_MAX, SessionStats, TraitKind, ingredient,
};
use bm_core::{BlockPos, INVENTORY_SLOTS, ItemKind, ItemStack, Station};
use bm_protocol::{ClientMsg, ForgeOutcome, ForgeView, SessionView};
use egui::{Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

const BOON: Color32 = Color32::from_rgb(120, 200, 110);
const QUIRK: Color32 = Color32::from_rgb(160, 130, 230);
const FLAW: Color32 = Color32::from_rgb(226, 96, 80);
const STABILITY: Color32 = Color32::from_rgb(110, 170, 230);
const VIGOUR: Color32 = Color32::from_rgb(238, 200, 96);

fn kind_colour(kind: TraitKind) -> Color32 {
    match kind {
        TraitKind::Boon => BOON,
        TraitKind::Quirk => QUIRK,
        TraitKind::Flaw => FLAW,
    }
}

/// What the forge window remembers between frames: the gear and deck being
/// chosen, and the round last asked for (so a second click before the
/// answer sends nothing).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ForgeUi {
    /// The inventory slot of the gear chosen.
    pub gear: Option<u8>,
    /// The deck: inventory slots and how many of each.
    pub deck: Vec<(u8, u16)>,
    /// `rounds` of the session when we last sent `ForgeUse`.
    pub sent: Option<u32>,
}

/// Whether a stack is gear the forge takes: forgeable and not yet forged.
pub fn can_forge(items: &Items, stack: &ItemStack) -> bool {
    items
        .def(stack)
        .is_some_and(|d| Gear::of(d).is_some() && !stack.forge.done)
}

/// The ingredient a stack is, if it is one.
pub fn ingredient_of(items: &Items, stack: &ItemStack) -> Option<&'static IngredientDef> {
    match items.def(stack)?.kind {
        ItemKind::Ingredient(i) => ingredient(i),
        _ => None,
    }
}

/// Pack slots holding gear the forge takes, in slot order.
pub fn gear_slots(items: &Items) -> Vec<u8> {
    (0..INVENTORY_SLOTS)
        .filter(|&i| items.inventory.get(i).is_some_and(|s| can_forge(items, &s)))
        .map(|i| i as u8)
        .collect()
}

/// Pack slots holding forging ingredients, in slot order.
pub fn ingredient_slots(items: &Items) -> Vec<u8> {
    (0..INVENTORY_SLOTS)
        .filter(|&i| {
            items
                .inventory
                .get(i)
                .is_some_and(|s| ingredient_of(items, &s).is_some())
        })
        .map(|i| i as u8)
        .collect()
}

/// What a session with the gear in `slot` would start with, from the
/// character's own skills (no power: coils come later).
pub fn preview(items: &Items, forger: &Forger, slot: u8) -> Option<SessionStats> {
    let stack = items.inventory.get(slot as usize)?;
    let def = items.def(&stack)?;
    Gear::of(def)?;
    Some(forger.stats(def.flex, 0.0))
}

impl ForgeUi {
    /// Drops choices the pack no longer backs (an item moved, used or
    /// forged), and keeps each deck count within its stack and 100, and the
    /// deck within `slots`.
    pub fn tidy(&mut self, items: &Items, slots: usize) {
        if self
            .gear
            .is_some_and(|g| items.inventory.get(g as usize).is_none_or(|s| !can_forge(items, &s)))
        {
            self.gear = None;
        }
        let mut seen = Vec::new();
        self.deck.retain_mut(|(slot, count)| {
            let Some(s) = items.inventory.get(*slot as usize) else {
                return false;
            };
            if ingredient_of(items, &s).is_none() || seen.contains(slot) {
                return false;
            }
            seen.push(*slot);
            *count = (*count).min(s.count).min(DECK_STACK);
            *count > 0
        });
        self.deck.truncate(slots);
    }

    /// Puts the ingredient stack in `slot` into the deck (as many as it
    /// holds, up to 100), if there is room and it is not in already.
    pub fn add(&mut self, items: &Items, slot: u8, slots: usize) {
        let Some(s) = items.inventory.get(slot as usize) else {
            return;
        };
        if ingredient_of(items, &s).is_none() || self.deck.len() >= slots || self.deck.iter().any(|d| d.0 == slot) {
            return;
        }
        self.deck.push((slot, s.count.min(DECK_STACK)));
    }

    /// The message starting the session, once gear and a deck are chosen.
    pub fn begin(&self, pos: BlockPos) -> Option<ClientMsg> {
        let gear = self.gear?;
        (!self.deck.is_empty()).then(|| ClientMsg::ForgeBegin {
            pos,
            gear,
            deck: self.deck.clone(),
        })
    }
}

/// The stability the next use of `d` costs in this session: the least and
/// most (Boundless's 1.00 to 1.10), after shielding and steadying salves;
/// and the chance a gamble salve makes it free.
pub fn stability_cost(view: &SessionView, d: &IngredientDef) -> (i32, i32, f64) {
    let mut base = d.stability;
    let mut free = 0.0;
    for b in &view.buffs {
        match b.kind {
            BuffKind::Shield => base = (base as f64 * 0.5).floor() as i32,
            BuffKind::Gamble(t) => free = t as f64 / 10.0,
            BuffKind::Steady(v) => base = (base - v as i32).max(0),
            _ => {}
        }
    }
    (
        base,
        (base as f64 * 1.1).round() as i32,
        if base > 0 { free } else { 0.0 },
    )
}

/// The stability line shown before a round: "This costs 100 to 110
/// stability; 340 left", with a warning when it could end the item.
pub fn stability_warning(view: &SessionView, d: &IngredientDef) -> (String, bool) {
    let (lo, hi, free) = stability_cost(view, d);
    if hi == 0 {
        return (format!("This costs no stability; {} left", view.stability), false);
    }
    let mut text = if lo == hi {
        format!("This costs {lo} stability; {} left", view.stability)
    } else {
        format!("This costs {lo} to {hi} stability; {} left", view.stability)
    };
    if free > 0.0 {
        text.push_str(&format!(" ({:.0}% chance it costs none)", free * 100.0));
    }
    let risky = hi >= view.stability && !matches!(d.action, Action::Seal | Action::Unmake(_));
    if risky {
        text.push_str(". It could run out: the item would be lost");
    }
    (text, risky)
}

fn item_name(items: &Items, stack: &ItemStack) -> String {
    items.def(stack).map(|d| d.display.clone()).unwrap_or_default()
}

/// What the last round did, in a line: "Rolled: boon hit, 312 points on
/// Mighty; quirk missed. Spent 50 vigour and 104 stability."
pub fn last_round_text(view: &SessionView) -> String {
    if view.rounds == 0 {
        return "Nothing used yet: each ingredient from the deck is one round.".into();
    }
    let last = &view.last;
    let mut parts = Vec::new();
    for kind in TraitKind::ALL {
        let k = kind as usize;
        let Some((_, hit)) = last.rolls[k] else { continue };
        parts.push(format!("{} {}", kind.word(), if hit { "hit" } else { "missed" }));
    }
    let mut text = if parts.is_empty() {
        "No pool rolled".to_string()
    } else {
        format!("Rolled: {}", parts.join(", "))
    };
    for (id, points) in &last.landed {
        text.push_str(&format!("; {points} points on {}", id.def().display));
    }
    text.push('.');
    let mut spent = Vec::new();
    if last.vigour_spent != 0 {
        spent.push(format!("{} vigour", last.vigour_spent));
    }
    if last.stability_spent != 0 {
        spent.push(format!("{} stability", last.stability_spent));
    }
    if !spent.is_empty() {
        text.push_str(&format!(" Spent {}.", spent.join(" and ")));
    }
    text
}

/// The notice for a session's end (`ServerMsg::Forged`).
pub fn outcome_text(items: &Items, outcome: &ForgeOutcome) -> String {
    match outcome {
        ForgeOutcome::Sealed(gear) => format!("Sealed: your {} goes into the forge's queue", item_name(items, gear)),
        ForgeOutcome::Unmade { gear, returned } => {
            let back: u32 = returned.iter().map(|s| s.count as u32).sum();
            let name = item_name(items, gear);
            match back {
                0 => format!("Unmade: your {name} comes back unforged; no ingredients came back"),
                1 => format!("Unmade: your {name} comes back unforged, with 1 ingredient"),
                n => format!("Unmade: your {name} comes back unforged, with {n} ingredients"),
            }
        }
        ForgeOutcome::Destroyed(gear) => format!("Stability ran out: the {} is lost", item_name(items, gear)),
    }
}

/// A buff in plain words: "Boon points +50%: 2 rounds left"; a glaze
/// "Leaning to Force boons (3 glazes)".
pub fn buff_text(b: &bm_core::forge::Buff) -> String {
    let mut what = b.kind.describe();
    if let Some(first) = what.get(..1) {
        what = first.to_uppercase() + &what[1..];
    }
    match b.rounds {
        None if b.count > 1 => format!("{what} ({} glazes)", b.count),
        None => format!("{what} (1 glaze)"),
        Some(1) => format!("{what}: 1 round left"),
        Some(n) => format!("{what}: {n} rounds left"),
    }
}

/// A trait in the session: "Mighty 2, 180 points (next level at 300)".
pub fn session_trait_text(id: bm_core::forge::TraitId, points: u32) -> (String, String) {
    let d = id.def();
    let level = d.level_of(points);
    let head = match d.levels.get(level as usize) {
        Some(next) if level == 0 => format!("{}, {points} points (a level at {next})", d.display),
        Some(next) => format!("{} {level}, {points} points (next level at {next})", d.display),
        None => format!("{} {level}, {points} points (top level)", d.display),
    };
    let does = if level == 0 {
        "No level yet: does nothing".into()
    } else {
        d.describe(level)
    };
    (head, does)
}

pub struct ForgeWindow<'a> {
    pub items: &'a Items,
    pub icons: &'a ItemIcons,
    pub title: String,
    /// The block that was opened.
    pub opened: BlockPos,
    pub live: Option<&'a LiveStation>,
    pub now: f64,
    /// The character as a forger (`Effects::forger`).
    pub forger: Forger,
}

pub fn forge(ctx: &egui::Context, v: &ForgeWindow, state: &mut ForgeUi, actions: &mut Vec<UiAction>) {
    let mut open = true;
    let pos = v.live.map(|l| l.view.pos).unwrap_or(v.opened);
    let empty = ForgeView::default();
    let forge = v.live.and_then(|l| l.view.forge.as_deref()).unwrap_or(&empty);
    window(&v.title).open(&mut open).show(ctx, |ui| {
        ui.set_width(780.0);
        let Some(live) = v.live else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new("Looking inside").color(MUTED));
            });
            return;
        };
        if let Some(why) = &live.view.idle {
            idle_banner(ui, why);
        }
        machine_panel(ui, Station::Forge, &live.view);
        if let Some(name) = &forge.forger {
            ui.label(egui::RichText::new(format!("{name} is forging")).color(TEXT).strong());
            ui.label(
                egui::RichText::new("One session at a time; only its forger can go on with it.")
                    .color(MUTED)
                    .small(),
            );
        } else if let Some(session) = &forge.session {
            session_panel(ui, v, session, pos, state, actions);
        } else if forge.queued.is_none() && live.view.output.is_empty() {
            setup_panel(ui, v, pos, live.view.idle.is_some(), state, actions);
        }
        if let Some((item, progress, left)) = &forge.queued {
            ui.add_space(8.0);
            heading(ui, "In the queue");
            let run = (v.now - live.received) as f32;
            let left = (*left as f32 - run).max(0.0);
            item_chip(
                ui,
                item,
                v.items,
                v.icons,
                32.0,
                egui::RichText::new(format!("{}, {} left", item_name(v.items, item), time_left(left))).color(TEXT),
            );
            ui.add(egui::ProgressBar::new(*progress).desired_height(10.0).fill(GOLD_DIM));
        }
        if !live.view.output.is_empty() {
            ui.add_space(8.0);
            heading(ui, "Ready");
            ui.horizontal_wrapped(|ui| {
                for stack in &live.view.output {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::hover());
                    paint_slot_back(ui.painter(), rect, Stroke::new(1.0, SLOT_RIM));
                    paint_stack(ui.painter(), rect, stack, v.items, v.icons, 1.0);
                    resp.on_hover_text(v.items.tooltip(stack));
                }
            });
            if ui.button(egui::RichText::new("Collect").strong()).clicked() {
                actions.push(UiAction::Send(ClientMsg::CollectStation { pos }));
            }
        }
    });
    if !open {
        actions.push(UiAction::CloseMenu);
    }
}

/// "3 min 20 s", "45 s".
fn time_left(secs: f32) -> String {
    let s = secs.ceil() as u32;
    match (s / 60, s % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} s"),
    }
}

/// A pack stack as a clickable slot; `chosen` gives it a gold rim.
fn pick_slot(ui: &mut Ui, v: &ForgeWindow, stack: &ItemStack, chosen: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::click());
    let rim = if chosen {
        Stroke::new(2.5, GOLD)
    } else if resp.hovered() {
        Stroke::new(1.5, GOLD_DIM)
    } else {
        Stroke::new(1.0, SLOT_RIM)
    };
    paint_slot_back(ui.painter(), rect, rim);
    paint_stack(ui.painter(), rect, stack, v.items, v.icons, 1.0);
    resp.on_hover_text(v.items.tooltip(stack))
}

fn setup_panel(
    ui: &mut Ui,
    v: &ForgeWindow,
    pos: BlockPos,
    idle: bool,
    state: &mut ForgeUi,
    actions: &mut Vec<UiAction>,
) {
    let slots = v.forger.deck_slots();
    state.tidy(v.items, slots);
    ui.label(
        egui::RichText::new(
            "Put in a piece of gear and a deck of ingredients. Each ingredient used is one round; \
             a sealing wax finishes the item.",
        )
        .color(MUTED)
        .small(),
    );
    ui.add_space(6.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(380.0);
            heading(ui, "Gear");
            let gear = gear_slots(v.items);
            if gear.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "Nothing in your pack can be forged. Tools, slings, bombs, grapples, spanners and chisels \
                         can, once each.",
                    )
                    .color(MUTED),
                );
            }
            ui.horizontal_wrapped(|ui| {
                for slot in gear {
                    let Some(stack) = v.items.inventory.get(slot as usize) else {
                        continue;
                    };
                    if pick_slot(ui, v, &stack, state.gear == Some(slot)).clicked() {
                        state.gear = if state.gear == Some(slot) { None } else { Some(slot) };
                    }
                }
            });
            ui.add_space(8.0);
            heading(ui, "Ingredients in your pack");
            let ingredients = ingredient_slots(v.items);
            if ingredients.is_empty() {
                ui.label(egui::RichText::new("No forging ingredients in your pack.").color(MUTED));
            }
            ui.horizontal_wrapped(|ui| {
                for slot in ingredients {
                    let Some(stack) = v.items.inventory.get(slot as usize) else {
                        continue;
                    };
                    let chosen = state.deck.iter().any(|d| d.0 == slot);
                    if pick_slot(ui, v, &stack, chosen).clicked() {
                        if chosen {
                            state.deck.retain(|d| d.0 != slot);
                        } else {
                            state.add(v.items, slot, slots);
                        }
                    }
                }
            });
            ui.label(
                egui::RichText::new("Click one to put it in the deck, again to take it out.")
                    .color(MUTED)
                    .small(),
            );
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_width(370.0);
            heading(ui, &format!("Deck, {} of {slots}", state.deck.len()));
            if state.deck.is_empty() {
                ui.label(egui::RichText::new("Empty").color(MUTED));
            }
            let mut remove = None;
            for (i, (slot, count)) in state.deck.iter_mut().enumerate() {
                let Some(stack) = v.items.inventory.get(*slot as usize) else {
                    continue;
                };
                ui.horizontal(|ui| {
                    item_chip(
                        ui,
                        &stack,
                        v.items,
                        v.icons,
                        26.0,
                        egui::RichText::new(format!("{} x{count}", item_name(v.items, &stack))).color(TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Take out").clicked() {
                            remove = Some(i);
                        }
                        let most = stack.count.min(DECK_STACK);
                        for (label, step) in [("+10", 10i32), ("+1", 1), ("-1", -1), ("-10", -10)] {
                            if ui.small_button(label).clicked() {
                                *count = (*count as i32 + step).clamp(1, most as i32) as u16;
                            }
                        }
                    });
                });
            }
            if let Some(i) = remove {
                state.deck.remove(i);
            }
            ui.add_space(8.0);
            heading(ui, "The session starts with");
            match state.gear.and_then(|g| preview(v.items, &v.forger, g)) {
                Some(stats) => {
                    let flex = state
                        .gear
                        .and_then(|g| v.items.inventory.get(g as usize))
                        .and_then(|s| v.items.def(&s).map(|d| v.forger.flexibility(d.flex)))
                        .unwrap_or(0);
                    stat_line(
                        ui,
                        "Effectiveness",
                        &format!("{:.0}%", stats.effectiveness * 100.0),
                        GOLD,
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "Boon points are multiplied by it. From your Insight and the item's Flexibility ({flex})."
                        ))
                        .color(MUTED)
                        .small(),
                    );
                    stat_line(ui, "Stability", &stats.stability.to_string(), STABILITY);
                    ui.label(
                        egui::RichText::new("From Vitality. Run out and the item is lost.")
                            .color(MUTED)
                            .small(),
                    );
                    stat_line(ui, "Vigour", &stats.vigour.to_string(), VIGOUR);
                    ui.label(
                        egui::RichText::new("From Power. Most ingredients past the first tier spend it.")
                            .color(MUTED)
                            .small(),
                    );
                }
                None => {
                    ui.label(egui::RichText::new("Choose the gear to see.").color(MUTED));
                }
            }
            let sealing = state.deck.iter().any(|(slot, _)| {
                v.items
                    .inventory
                    .get(*slot as usize)
                    .and_then(|s| ingredient_of(v.items, &s))
                    .is_some_and(|d| matches!(d.action, Action::Seal))
            });
            if !state.deck.is_empty() && !sealing {
                ui.label(
                    egui::RichText::new("No sealing wax in the deck: you could not finish the item.")
                        .color(SHORT)
                        .small(),
                );
            }
            ui.add_space(8.0);
            let msg = state.begin(pos).filter(|_| !idle);
            let why = if idle {
                "The forge does not work yet"
            } else if state.gear.is_none() {
                "Choose the gear first"
            } else {
                "Put at least one ingredient in the deck"
            };
            let resp = ui.add_enabled(
                msg.is_some(),
                egui::Button::new(egui::RichText::new("Begin forging").strong()),
            );
            let resp = if msg.is_none() {
                resp.on_disabled_hover_text(why)
            } else {
                resp
            };
            if resp.clicked()
                && let Some(m) = msg
            {
                state.sent = None;
                actions.push(UiAction::Send(m));
            }
        });
    });
}

fn stat_line(ui: &mut Ui, name: &str, value: &str, colour: Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(name).color(TEXT).strong());
        ui.label(egui::RichText::new(value).color(colour).strong().size(16.0));
    });
}

/// A pool's bar out of 1,000, with the last round's roll marked on it.
fn pool_bar(ui: &mut Ui, kind: TraitKind, points: i32, roll: Option<(f64, bool)>) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} pool", kind_title(kind).trim_end_matches('s')))
                .color(TEXT)
                .small(),
        );
        ui.label(
            egui::RichText::new(format!("{points} / {POOL_MAX}"))
                .color(MUTED)
                .small(),
        );
        if let Some((r, hit)) = roll {
            let (text, colour) = if hit {
                ("hit", kind_colour(kind))
            } else {
                ("missed", MUTED)
            };
            ui.label(
                egui::RichText::new(format!("rolled {:.0}: {text}", r * 1000.0))
                    .color(colour)
                    .small(),
            );
        }
    });
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(330.0), 14.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(38, 34, 32));
    let frac = (points as f32 / POOL_MAX as f32).clamp(0.0, 1.0);
    if frac > 0.0 {
        let full = Rect::from_min_size(rect.min, vec2(rect.width() * frac, rect.height()));
        painter.rect_filled(full, CornerRadius::same(4), kind_colour(kind).gamma_multiply(0.85));
    }
    if let Some((r, hit)) = roll {
        let x = rect.left() + rect.width() * r.clamp(0.0, 1.0) as f32;
        let colour = if hit { Color32::WHITE } else { Color32::from_gray(150) };
        painter.line_segment(
            [pos2(x, rect.top() - 3.0), pos2(x, rect.bottom() + 3.0)],
            Stroke::new(2.0, colour),
        );
    }
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, Color32::from_white_alpha(30)),
        StrokeKind::Inside,
    );
    ui.add_space(2.0);
}

fn session_panel(
    ui: &mut Ui,
    v: &ForgeWindow,
    s: &SessionView,
    pos: BlockPos,
    state: &mut ForgeUi,
    actions: &mut Vec<UiAction>,
) {
    let waiting = state.sent == Some(s.rounds);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(350.0);
            item_chip(
                ui,
                &s.item,
                v.items,
                v.icons,
                40.0,
                egui::RichText::new(format!("{}, round {}", item_name(v.items, &s.item), s.rounds + 1))
                    .color(TEXT)
                    .strong()
                    .size(16.0),
            );
            stat_line(
                ui,
                "Effectiveness",
                &format!("{:.0}%", s.stats.effectiveness * 100.0),
                GOLD,
            );
            ui.label(
                egui::RichText::new(format!("Stability {} / {}", s.stability.max(0), s.stats.stability))
                    .color(TEXT)
                    .small(),
            );
            thin_bar(ui, s.stability as f32 / s.stats.stability.max(1) as f32, STABILITY);
            ui.label(
                egui::RichText::new(format!("Vigour {} / {}", s.vigour.max(0), s.stats.vigour))
                    .color(TEXT)
                    .small(),
            );
            thin_bar(ui, s.vigour as f32 / s.stats.vigour.max(1) as f32, VIGOUR);
            ui.add_space(6.0);
            for kind in TraitKind::ALL {
                let k = kind as usize;
                pool_bar(ui, kind, s.pools[k], s.last.rolls[k]);
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(last_round_text(s)).color(MUTED).small());
            ui.add_space(6.0);
            heading(ui, "Buffs");
            if s.buffs.is_empty() {
                ui.label(egui::RichText::new("None").color(MUTED).small());
            }
            for b in &s.buffs {
                let colour = if b.kind.is_bad() { SHORT } else { TEXT };
                ui.label(egui::RichText::new(buff_text(b)).color(colour).small());
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_width(400.0);
            heading(ui, "Traits");
            let mut any = false;
            for kind in TraitKind::ALL {
                let list = &s.traits[kind as usize];
                if list.is_empty() {
                    continue;
                }
                any = true;
                ui.label(egui::RichText::new(kind_title(kind)).color(kind_colour(kind)).strong());
                for &(id, points) in list {
                    let (head, does) = session_trait_text(id, points);
                    ui.label(egui::RichText::new(head).color(TEXT).small());
                    ui.label(egui::RichText::new(format!("  {does}")).color(MUTED).small());
                }
            }
            if !any {
                ui.label(egui::RichText::new("None yet").color(MUTED).small());
            }
            ui.add_space(6.0);
            heading(ui, "Deck");
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (i, slot) in s.deck.iter().enumerate() {
                    deck_button(ui, v, s, i, slot, waiting, pos, state, actions);
                }
            });
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn deck_button(
    ui: &mut Ui,
    v: &ForgeWindow,
    s: &SessionView,
    i: usize,
    slot: &bm_core::forge::DeckSlot,
    waiting: bool,
    pos: BlockPos,
    state: &mut ForgeUi,
    actions: &mut Vec<UiAction>,
) {
    let Some(d) = ingredient(slot.ingredient) else { return };
    let short = d.vigour > 0 && d.vigour > s.vigour;
    let usable = slot.stack.count > 0 && !short && !waiting;
    let (warning, risky) = stability_warning(s, d);
    let tint = if usable { 1.0 } else { 0.45 };
    let frame = egui::Frame::new()
        .fill(Color32::from_white_alpha(if usable { 9 } else { 4 }))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(6));
    let resp = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                paint_slot_back(ui.painter(), rect, Stroke::new(1.0, SLOT_RIM));
                paint_stack(ui.painter(), rect, &slot.stack, v.items, v.icons, tint);
                ui.vertical(|ui| {
                    let colour = if usable { TEXT } else { MUTED };
                    ui.label(
                        egui::RichText::new(format!("{} x{}", item_name(v.items, &slot.stack), slot.stack.count))
                            .color(colour)
                            .strong(),
                    );
                    let cost = ingredient_cost(d);
                    let cost_colour = if short { SHORT } else { MUTED };
                    ui.label(egui::RichText::new(cost).color(cost_colour).small());
                    if short {
                        ui.label(egui::RichText::new("Not enough vigour").color(SHORT).small());
                    } else if risky {
                        ui.label(
                            egui::RichText::new("Could use up the last of the stability")
                                .color(SHORT)
                                .small(),
                        );
                    }
                });
            });
        })
        .response;
    let resp = ui
        .interact(resp.rect, ui.scope_id().with(("deck", i)), Sense::click())
        .on_hover_text(format!("{}\n{}", ingredient_effect(d), warning));
    if usable && resp.hovered() {
        ui.painter().rect_stroke(
            resp.rect,
            CornerRadius::same(8),
            Stroke::new(1.5, GOLD_DIM),
            StrokeKind::Inside,
        );
    }
    if usable && resp.clicked() {
        state.sent = Some(s.rounds);
        actions.push(UiAction::Send(ClientMsg::ForgeUse {
            pos,
            slot: i as u8,
            round: s.rounds,
        }));
    }
    ui.add_space(3.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::forge::{Buff, DeckSlot, Session, ingredient_by_name};
    use bm_core::{BlockRegistry, ColourPalette, Inventory, ItemRegistry, RecipeBook};

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

    fn put(i: &mut Items, slot: usize, name: &str, count: u16) {
        i.inventory.slots[slot] = Some(ItemStack::new(i.registry.id(name).unwrap(), count));
    }

    #[test]
    fn the_forge_takes_unforged_gear_and_ingredients() {
        let mut i = items();
        put(&mut i, 0, "iron_hammer", 1);
        put(&mut i, 1, "stone", 40);
        put(&mut i, 2, "pure_tincture_1", 150);
        put(&mut i, 3, "hand_stone", 1);
        put(&mut i, 4, "copper_grapple", 1);
        put(&mut i, 5, "sealing_wax", 3);
        let mut forged = ItemStack::new(i.registry.id("stone_hammer").unwrap(), 1);
        forged.forge.done = true;
        i.inventory.slots[6] = Some(forged);
        assert_eq!(
            gear_slots(&i),
            vec![0, 4],
            "not stone, the hand stone or a forged hammer"
        );
        assert_eq!(ingredient_slots(&i), vec![2, 5]);

        let mut ui = ForgeUi::default();
        assert_eq!(ui.begin(BlockPos::new(1, 2, 3)), None);
        ui.gear = Some(0);
        ui.add(&i, 2, 6);
        ui.add(&i, 2, 6);
        ui.add(&i, 1, 6);
        ui.add(&i, 5, 6);
        assert_eq!(
            ui.deck,
            vec![(2, 100), (5, 3)],
            "at most 100, once each, ingredients only"
        );
        assert_eq!(
            ui.begin(BlockPos::new(1, 2, 3)),
            Some(ClientMsg::ForgeBegin {
                pos: BlockPos::new(1, 2, 3),
                gear: 0,
                deck: vec![(2, 100), (5, 3)],
            })
        );
        // The pack changes under the choices: they follow it.
        put(&mut i, 2, "pure_tincture_1", 7);
        i.inventory.slots[5] = None;
        i.inventory.slots[0] = None;
        ui.tidy(&i, 6);
        assert_eq!((ui.gear, ui.deck.clone()), (None, vec![(2, 7)]));
        // A full deck takes no more.
        let mut full = ForgeUi::default();
        full.add(&i, 2, 0);
        assert!(full.deck.is_empty());
    }

    #[test]
    fn the_preview_is_the_forgers_own_stats_for_the_item() {
        let mut i = items();
        put(&mut i, 0, "iron_hammer", 1);
        put(&mut i, 1, "stone", 1);
        let new = Forger::default();
        let s = preview(&i, &new, 0).unwrap();
        assert_eq!(s, new.stats(110, 0.0));
        assert_eq!((s.stability, s.vigour), (900, 0));
        assert_eq!(
            (s.effectiveness * 100.0).round(),
            34.0,
            "a new character, copper or iron"
        );
        let full = Forger {
            vitality: 9.0,
            power: 9.0,
            insight: 9.0,
            epic: true,
        };
        let f = preview(&i, &full, 0).unwrap();
        assert_eq!((f.stability, f.vigour), (1800, 1800));
        assert!(f.effectiveness > s.effectiveness);
        assert_eq!(preview(&i, &new, 1), None);
    }

    fn session(i: &Items, deck: &[(&str, u16)]) -> Session {
        let hammer = ItemStack::new(i.registry.id("iron_hammer").unwrap(), 1);
        let deck = deck
            .iter()
            .map(|(n, c)| DeckSlot {
                stack: ItemStack::new(i.registry.id(n).unwrap(), *c),
                ingredient: ingredient_by_name(n).unwrap(),
            })
            .collect();
        let stats = SessionStats {
            stability: 300,
            vigour: 100,
            effectiveness: 1.0,
        };
        Session::new(hammer, Gear::Tool, stats, deck, 7)
    }

    #[test]
    fn the_stability_cost_is_told_before_each_round() {
        let i = items();
        let s = session(&i, &[("pure_tincture_1", 5), ("sealing_wax", 1)]);
        let mut view = SessionView::of(&s);
        let pure = ingredient(ingredient_by_name("pure_tincture_1").unwrap()).unwrap();
        let wax = ingredient(ingredient_by_name("sealing_wax").unwrap()).unwrap();
        assert_eq!(stability_cost(&view, pure), (100, 110, 0.0));
        assert_eq!(
            stability_warning(&view, pure),
            ("This costs 100 to 110 stability; 300 left".into(), false)
        );
        assert_eq!(stability_warning(&view, wax).0, "This costs no stability; 300 left");
        view.stability = 105;
        let (text, risky) = stability_warning(&view, pure);
        assert!(risky && text.ends_with("the item would be lost"), "{text}");
        view.buffs.push(Buff {
            kind: BuffKind::Shield,
            count: 1,
            rounds: Some(2),
            fresh: false,
        });
        view.buffs.push(Buff {
            kind: BuffKind::Gamble(4),
            count: 1,
            rounds: Some(2),
            fresh: false,
        });
        assert_eq!(stability_cost(&view, pure), (50, 55, 0.4));
        assert!(stability_warning(&view, pure).0.contains("40% chance it costs none"));
    }

    #[test]
    fn rounds_and_endings_read_in_plain_words() {
        let mut i = items();
        let mut s = session(&i, &[("pure_tincture_1", 20), ("sealing_wax", 1)]);
        assert!(last_round_text(&SessionView::of(&s)).starts_with("Nothing used yet"));
        s.use_ingredient(0).unwrap();
        let text = last_round_text(&SessionView::of(&s));
        assert!(text.starts_with("Rolled: boon "), "{text}");
        assert!(text.contains("Spent 1"), "pure tincture costs stability only: {text}");
        let hammer = s.item;
        assert_eq!(
            outcome_text(&i, &ForgeOutcome::Sealed(hammer)),
            "Sealed: your Iron Hammer goes into the forge's queue"
        );
        assert_eq!(
            outcome_text(&i, &ForgeOutcome::Destroyed(hammer)),
            "Stability ran out: the Iron Hammer is lost"
        );
        put(&mut i, 0, "pure_tincture_1", 1);
        let back = vec![ItemStack::new(i.registry.id("pure_tincture_1").unwrap(), 3)];
        assert_eq!(
            outcome_text(
                &i,
                &ForgeOutcome::Unmade {
                    gear: hammer,
                    returned: back
                }
            ),
            "Unmade: your Iron Hammer comes back unforged, with 3 ingredients"
        );
        let glaze = Buff {
            kind: BuffKind::Glaze(bm_core::forge::Leaning::Force),
            count: 3,
            rounds: None,
            fresh: false,
        };
        assert_eq!(buff_text(&glaze), "Leaning to Force boons (3 glazes)");
        let boost = Buff {
            kind: BuffKind::Boost,
            count: 1,
            rounds: Some(2),
            fresh: false,
        };
        assert_eq!(buff_text(&boost), "Boon points +50%: 2 rounds left");
        let (head, does) = session_trait_text(bm_core::forge::TraitId::Mighty, 180);
        assert_eq!(head, "Mighty 2, 180 points (next level at 300)");
        assert_eq!(does, "+100 damage and +10%");
        let (head, does) = session_trait_text(bm_core::forge::TraitId::Quick, 60);
        assert_eq!(head, "Quick, 60 points (a level at 100)");
        assert!(does.starts_with("No level yet"));
        assert!(
            session_trait_text(bm_core::forge::TraitId::Hookshot, 1000)
                .0
                .ends_with("(top level)")
        );
        for t in [text, head, buff_text(&glaze)] {
            assert!(!t.contains('\u{2014}'));
        }
    }
}
