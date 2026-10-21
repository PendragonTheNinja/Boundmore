//! The skill screen (milestone 12, key K): level and XP, points, cleanses
//! and marks; the skill pages; the tree by category, as Boundless laid it
//! out. Draws `crate::skills`' model and returns `UiAction::Skill`s; it
//! never changes the character itself. Plain words, no em dashes.

use crate::menus::{GOLD, GOLD_DIM, MUTED, SHORT, TEXT, UiAction, window};
use crate::skills::{CATEGORIES, Look, Progress, SkillCard, cards, category_name, spent_in};
use bm_core::skills::{
    Category, FREE_RESET_BELOW, MAX_PAGES, MAX_PLOTS_AT_ONCE, PAGE_CAP, PAGE_PRICE, PLOT_PRICE, RESET_PRICE,
    cleanse_at, level_of, points_at,
};
use bm_protocol::SkillAction;
use egui::{Align, Color32, CornerRadius, Layout, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

/// What the screen remembers between frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillUi {
    pub tab: Category,
    /// "Reset page" was pressed and waits for a yes.
    pub confirm_reset: bool,
    /// How many plots "Buy plots" buys (1 to 80).
    pub plots: u16,
}

impl Default for SkillUi {
    fn default() -> Self {
        Self {
            tab: Category::Attributes,
            confirm_reset: false,
            plots: 1,
        }
    }
}

pub struct SkillView<'a> {
    pub progress: &'a Progress,
    /// Minutes until pages can be switched away from home, while a switch
    /// cools down.
    pub switch_wait: Option<u64>,
    /// Standing inside our own claim: switching is free.
    pub at_home: bool,
}

const XP_FILL: Color32 = Color32::from_rgb(132, 196, 236);
const LOCKED: Color32 = Color32::from_rgb(120, 114, 106);
const PARTNER: Color32 = Color32::from_rgb(196, 160, 236);
const CARD_W: f32 = 318.0;
const CARD_H: f32 = 92.0;

/// 12345 as "12,345".
pub fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn skills(ctx: &egui::Context, v: &SkillView, state: &mut SkillUi, actions: &mut Vec<UiAction>) {
    let mut open = true;
    window("Skills").open(&mut open).show(ctx, |ui| {
        ui.set_width(880.0);
        header(ui, v);
        plots(ui, v, state, actions);
        ui.add_space(4.0);
        pages(ui, v, state, actions);
        ui.add_space(6.0);
        ui.separator();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(176.0);
                categories(ui, v, state);
            });
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.set_width(CARD_W * 2.0 + 14.0);
                skill_cards(ui, v, state.tab, actions);
            });
        });
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Left click learns a rank. Right click takes one back.")
                .color(MUTED)
                .small(),
        );
    });
    if !open {
        actions.push(UiAction::CloseMenu);
    }
}

/// Level, XP, points, cleanses and marks.
fn header(ui: &mut Ui, v: &SkillView) {
    let c = v.progress.character();
    let tree = &v.progress.tree;
    let (level, into, need) = level_of(c.xp);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(380.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Level").color(MUTED).size(16.0));
                ui.label(egui::RichText::new(level.to_string()).color(GOLD).size(30.0).strong());
            });
            let (rect, _) = ui.allocate_exact_size(vec2(360.0, 10.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(34, 32, 38));
            let f = (into as f32 / need.max(1) as f32).clamp(0.0, 1.0);
            if f > 0.0 {
                p.rect_filled(
                    Rect::from_min_size(rect.min, vec2(rect.width() * f, rect.height())),
                    CornerRadius::same(4),
                    XP_FILL,
                );
            }
            p.rect_stroke(
                rect,
                CornerRadius::same(4),
                Stroke::new(1.0, GOLD_DIM),
                StrokeKind::Outside,
            );
            ui.label(
                egui::RichText::new(format!(
                    "{} of {} XP to level {}",
                    grouped(into),
                    grouped(need),
                    level + 1
                ))
                .color(MUTED)
                .small(),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let free = level < FREE_RESET_BELOW;
            stat(
                ui,
                "Marks",
                &grouped(c.marks as u64),
                "Earned by levelling. They buy pages and resets.",
            );
            let cleanse = if free {
                (
                    "Free".to_string(),
                    format!("Taking back ranks is free until level {FREE_RESET_BELOW}."),
                )
            } else {
                (
                    c.cleanses_left().to_string(),
                    format!(
                        "Each takes back one rank. One a level up to 50 ({} earned, {} used).",
                        cleanse_at(level),
                        c.cleanses_used
                    ),
                )
            };
            stat(ui, "Cleanses", &cleanse.0, &cleanse.1);
            stat(
                ui,
                "Skill points",
                &c.points_left(tree).to_string(),
                &format!("{} earned by level {level}, shared by every page.", points_at(level)),
            );
        });
    });
}

/// Buying plots with marks (Nathan, 2026-10-21: as in Boundless, 30 marks a
/// plot, 1 to 80 at once).
fn plots(ui: &mut Ui, v: &SkillView, state: &mut SkillUi, actions: &mut Vec<UiAction>) {
    let c = v.progress.character();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Plots").color(MUTED));
        ui.label(
            egui::RichText::new(format!("{} bought with marks", c.plots_bought))
                .color(TEXT)
                .small(),
        );
        ui.add_space(8.0);
        state.plots = state.plots.clamp(1, MAX_PLOTS_AT_ONCE as u16);
        if ui.add_enabled(state.plots > 1, egui::Button::new("-")).clicked() {
            state.plots -= 1;
        }
        ui.add(
            egui::DragValue::new(&mut state.plots)
                .range(1..=MAX_PLOTS_AT_ONCE as u16)
                .speed(0.2),
        );
        if ui
            .add_enabled(state.plots < MAX_PLOTS_AT_ONCE as u16, egui::Button::new("+"))
            .clicked()
        {
            state.plots += 1;
        }
        let n = state.plots as u32;
        let price = n * PLOT_PRICE;
        let label = if n == 1 {
            format!("Buy 1 plot ({price} marks)")
        } else {
            format!("Buy {n} plots ({price} marks)")
        };
        let resp = ui.add_enabled(c.marks >= price, egui::Button::new(label));
        if resp.clicked() {
            actions.push(UiAction::Skill(SkillAction::BuyPlots(state.plots)));
        }
        resp.on_disabled_hover_text(format!("You need {price} marks"));
        if c.marks < price {
            ui.label(
                egui::RichText::new(format!("You need {price} marks"))
                    .color(SHORT)
                    .small(),
            );
        }
    });
}

/// A small labelled number in a box.
fn stat(ui: &mut Ui, label: &str, value: &str, hover: &str) {
    let (rect, resp) = ui.allocate_exact_size(vec2(116.0, 52.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(8), Color32::from_white_alpha(8));
    p.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(150, 116, 62, 120)),
        StrokeKind::Inside,
    );
    p.text(
        rect.center_top() + vec2(0.0, 5.0),
        egui::Align2::CENTER_TOP,
        value,
        egui::FontId::proportional(21.0),
        GOLD,
    );
    p.text(
        rect.center_bottom() - vec2(0.0, 5.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(12.0),
        MUTED,
    );
    resp.on_hover_text(hover);
}

/// The page tabs, buying a page, resetting one, and the switch cooldown.
fn pages(ui: &mut Ui, v: &SkillView, state: &mut SkillUi, actions: &mut Vec<UiAction>) {
    let c = v.progress.character();
    let tree = &v.progress.tree;
    let level = c.level();
    let can_switch = v.at_home || v.switch_wait.is_none();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Pages").color(MUTED));
        for i in 0..c.pages.len() {
            let active = i == c.active as usize;
            let spent = c.pages[i].spent(tree);
            let text = egui::RichText::new(format!("Page {}  ({spent})", i + 1)).strong();
            let resp = ui.add(
                egui::Button::new(if active { text.color(Color32::BLACK) } else { text })
                    .fill(if active { GOLD } else { Color32::from_rgb(44, 38, 34) })
                    .min_size(vec2(0.0, 28.0)),
            );
            if active {
                resp.on_hover_text("The page in use. Its skills are the ones that work.");
            } else if !can_switch {
                resp.on_hover_text("Switching waits for the cooldown, or is free inside your own claim.");
            } else if resp.clicked() {
                actions.push(UiAction::Skill(SkillAction::SwitchPage(i as u8)));
            } else {
                resp.on_hover_text("Use this page instead");
            }
        }
        if c.pages.len() < MAX_PAGES {
            let ok = c.marks >= PAGE_PRICE;
            let resp = ui.add_enabled(
                ok,
                egui::Button::new(format!("New page ({PAGE_PRICE} marks)")).min_size(vec2(0.0, 28.0)),
            );
            if resp.clicked() {
                actions.push(UiAction::Skill(SkillAction::BuyPage));
            }
            resp.on_disabled_hover_text(format!("You need {PAGE_PRICE} marks"));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let free = level < FREE_RESET_BELOW;
            let label = if free {
                "Reset page (free)".to_string()
            } else {
                format!("Reset page ({RESET_PRICE} marks)")
            };
            let ok = (free || c.marks >= RESET_PRICE) && !c.page().ranks.is_empty();
            let resp = ui.add_enabled(ok && !state.confirm_reset, egui::Button::new(label));
            if resp.clicked() {
                state.confirm_reset = true;
            }
            ui.label(
                egui::RichText::new(format!("{} of {PAGE_CAP} points on this page", c.page().spent(tree)))
                    .color(MUTED)
                    .small(),
            );
        });
    });
    if let Some(m) = v.switch_wait.filter(|_| !v.at_home) {
        let text = format!(
            "You can switch pages again in {m} minute{}, or at once inside your own claim.",
            if m == 1 { "" } else { "s" }
        );
        ui.label(egui::RichText::new(text).color(SHORT).small());
    }
    if state.confirm_reset {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(80, 26, 24, 170))
            .stroke(Stroke::new(1.5, Color32::from_rgb(150, 70, 60)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let cost = if level < FREE_RESET_BELOW {
                        "It is free until level 20.".to_string()
                    } else {
                        format!("It costs {RESET_PRICE} marks.")
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "Take back every skill on page {}? {cost}",
                            c.active as usize + 1
                        ))
                        .color(TEXT),
                    );
                    if ui.button("Reset it").clicked() {
                        actions.push(UiAction::Skill(SkillAction::ResetPage));
                        state.confirm_reset = false;
                    }
                    if ui.button("Keep it").clicked() {
                        state.confirm_reset = false;
                    }
                });
            });
    }
}

/// The category list on the left, Boundless's order.
fn categories(ui: &mut Ui, v: &SkillView, state: &mut SkillUi) {
    let c = v.progress.character();
    let level = c.level();
    for cat in CATEGORIES {
        let unlock = cat.unlock_level();
        let open = level >= unlock;
        let chosen = state.tab == cat;
        let spent = spent_in(&v.progress.tree, c, cat);
        let (rect, resp) = ui.allocate_exact_size(vec2(176.0, 38.0), Sense::click());
        let p = ui.painter();
        let fill = if chosen {
            Color32::from_rgb(78, 60, 36)
        } else if resp.hovered() {
            Color32::from_rgb(52, 44, 36)
        } else {
            Color32::from_white_alpha(6)
        };
        p.rect_filled(rect, CornerRadius::same(8), fill);
        if chosen {
            p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.5, GOLD), StrokeKind::Inside);
        }
        let name_colour = if open { TEXT } else { LOCKED };
        p.text(
            rect.left_top() + vec2(10.0, 3.0),
            egui::Align2::LEFT_TOP,
            category_name(cat),
            egui::FontId::proportional(15.0),
            name_colour,
        );
        let sub = if open {
            format!("from level {unlock}")
        } else {
            format!("unlocks at level {unlock}")
        };
        p.text(
            rect.left_bottom() + vec2(10.0, -3.0),
            egui::Align2::LEFT_BOTTOM,
            sub,
            egui::FontId::proportional(12.0),
            if open { MUTED } else { LOCKED },
        );
        if spent > 0 {
            p.text(
                rect.right_center() - vec2(10.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                spent.to_string(),
                egui::FontId::proportional(17.0),
                GOLD,
            );
        }
        if resp.clicked() {
            state.tab = cat;
        }
        ui.add_space(1.0);
    }
}

/// The chosen category's skills as cards, two to a row.
fn skill_cards(ui: &mut Ui, v: &SkillView, tab: Category, actions: &mut Vec<UiAction>) {
    let c = v.progress.character();
    let list = cards(&v.progress.tree, c, tab);
    let unlock = tab.unlock_level();
    let title = if c.level() >= unlock {
        format!("{}  (level {unlock})", category_name(tab))
    } else {
        format!("{}  (unlocks at level {unlock})", category_name(tab))
    };
    ui.label(egui::RichText::new(title).color(GOLD).size(18.0));
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for row in list.chunks(2) {
                ui.horizontal(|ui| {
                    for card in row {
                        skill_card(ui, card, actions);
                    }
                });
            }
        });
}

fn skill_card(ui: &mut Ui, card: &SkillCard, actions: &mut Vec<UiAction>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::click());
    let greyed = card.look == Look::Greyed;
    let learned = card.rank > 0;
    let p = ui.painter();
    let fill = match (greyed, learned) {
        (true, _) => Color32::from_rgba_unmultiplied(20, 19, 24, 200),
        (false, true) => Color32::from_rgba_unmultiplied(70, 54, 30, 200),
        (false, false) => Color32::from_white_alpha(10),
    };
    p.rect_filled(rect, CornerRadius::same(10), fill);
    let rim = if resp.hovered() && (card.learnable || card.cleansable) {
        Stroke::new(1.5, GOLD)
    } else if learned {
        Stroke::new(1.2, GOLD_DIM)
    } else {
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 80, 64, 110))
    };
    p.rect_stroke(rect, CornerRadius::same(10), rim, StrokeKind::Inside);

    let inner = rect.shrink2(vec2(12.0, 8.0));
    let name_colour = match (greyed, learned) {
        (true, _) => LOCKED,
        (false, true) => GOLD,
        (false, false) => TEXT,
    };
    p.text(
        inner.left_top(),
        egui::Align2::LEFT_TOP,
        &card.def.name,
        egui::FontId::proportional(17.0),
        name_colour,
    );
    let cost = if card.def.cost == 1 {
        "1 point".to_string()
    } else {
        format!("{} points", card.def.cost)
    };
    p.text(
        inner.right_top(),
        egui::Align2::RIGHT_TOP,
        cost,
        egui::FontId::proportional(12.0),
        if greyed { LOCKED } else { MUTED },
    );
    // Rank pips.
    let pip_y = inner.top() + 30.0;
    for i in 0..card.def.ranks {
        let centre = pos2(inner.left() + 6.0 + i as f32 * 16.0, pip_y);
        if i < card.rank {
            p.circle_filled(centre, 5.5, GOLD);
        } else {
            p.circle_stroke(
                centre,
                5.0,
                Stroke::new(1.3, if greyed { LOCKED.gamma_multiply(0.7) } else { GOLD_DIM }),
            );
        }
    }
    p.text(
        pos2(inner.left() + 6.0 + card.def.ranks as f32 * 16.0, pip_y),
        egui::Align2::LEFT_CENTER,
        format!("{}/{}", card.rank, card.def.ranks),
        egui::FontId::proportional(12.0),
        MUTED,
    );
    // What a rank does, wrapped in the card.
    let text_colour = if greyed { LOCKED } else { TEXT.gamma_multiply(0.92) };
    let galley = p.layout(
        card.def.text.clone(),
        egui::FontId::proportional(13.0),
        text_colour,
        inner.width(),
    );
    p.galley(pos2(inner.left(), pip_y + 10.0), galley, text_colour);
    // The foot: either/or, comes later, locked, or why not.
    let foot = if !card.partners.is_empty() && card.look != Look::Full {
        Some((format!("Either this or {}", card.partners.join(" or ")), PARTNER))
    } else {
        None
    };
    let reason = match card.why.as_deref() {
        Some("Comes later") => Some(("Comes later".to_string(), LOCKED)),
        Some(why) if greyed && !why.starts_with("That cannot go") => Some((why.to_string(), SHORT)),
        _ => None,
    };
    if let Some((text, colour)) = reason.or(foot) {
        p.text(
            inner.left_bottom(),
            egui::Align2::LEFT_BOTTOM,
            text,
            egui::FontId::proportional(12.0),
            colour,
        );
    }

    let hover = match (&card.why, card.cleansable) {
        (None, true) => "Left click: learn a rank. Right click: take one back.".to_string(),
        (None, false) => "Left click: learn a rank.".to_string(),
        (Some(why), true) => format!("{why}. Right click: take one back."),
        (Some(why), false) => why.clone(),
    };
    let resp = resp.on_hover_text(hover);
    if resp.clicked() && card.learnable {
        actions.push(UiAction::Skill(SkillAction::Learn(card.def.id.clone())));
    }
    if resp.secondary_clicked() && card.cleansable {
        actions.push(UiAction::Skill(SkillAction::Cleanse(card.def.id.clone())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_grouped() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1450), "1,450");
        assert_eq!(grouped(3_304_900), "3,304,900");
    }
}
