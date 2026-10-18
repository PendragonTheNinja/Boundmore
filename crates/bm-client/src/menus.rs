//! Menus and HUD, drawn with egui over the game.
//!
//! Screens never act on the game themselves: each returns `UiAction`s that
//! `App` carries out (sending messages, connecting, closing). The look follows
//! `docs/art-direction.md`: dark translucent panels, warm gold accents,
//! rounded corners. Player-facing text uses no em dashes.

use crate::icons::{ICON_SIZE, IconSource};
use crate::items::Items;
use bm_core::{BlockPos, BlockRegistry, HOTBAR_SLOTS, INVENTORY_SLOTS, ItemStack, Station};
use bm_protocol::{ClientMsg, StationView};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

pub const GOLD: Color32 = Color32::from_rgb(240, 186, 84);
const GOLD_DIM: Color32 = Color32::from_rgb(150, 116, 62);
const TEXT: Color32 = Color32::from_rgb(236, 228, 212);
const MUTED: Color32 = Color32::from_rgb(170, 162, 150);
const SHORT: Color32 = Color32::from_rgb(235, 104, 92);
const RED: Color32 = Color32::from_rgb(226, 82, 70);
const PANEL: Color32 = Color32::from_rgba_premultiplied(13, 12, 19, 222);
const SLOT_FILL: Color32 = Color32::from_rgba_premultiplied(5, 5, 9, 150);
const SLOT_RIM: Color32 = Color32::from_rgba_premultiplied(120, 110, 92, 120);

const SLOT: f32 = 54.0;
const GAP: f32 = 6.0;

/// What the player asked for this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum UiAction {
    Send(ClientMsg),
    LogIn { register: bool },
    CancelLogin,
    CloseMenu,
}

/// Which screen holds the mouse, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Inventory,
    /// Hand crafting, or the station at this block.
    Crafting(Option<BlockPos>),
}

#[derive(Default)]
pub struct LoginForm {
    pub name: String,
    pub password: String,
    pub server: String,
    pub error: Option<String>,
    /// Waiting for the server's answer.
    pub busy: bool,
}

/// An item being dragged in the inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drag {
    pub from: usize,
    pub half: bool,
}

/// A watched station's last state and when it came, to run its progress
/// bar smoothly between updates.
pub struct LiveStation {
    pub view: StationView,
    pub received: f64,
}

pub enum Icon {
    Texture(egui::TextureHandle),
    Initials { text: String, color: Color32 },
}

/// One icon per item id.
#[derive(Default)]
pub struct ItemIcons {
    icons: Vec<Icon>,
}

impl ItemIcons {
    pub fn build(ctx: &egui::Context, sources: Vec<IconSource>) -> Self {
        let icons = sources
            .into_iter()
            .enumerate()
            .map(|(i, s)| match s {
                IconSource::Image(rgba) => Icon::Texture(ctx.load_texture(
                    format!("item-{i}"),
                    egui::ColorImage::from_rgba_unmultiplied([ICON_SIZE, ICON_SIZE], &rgba),
                    egui::TextureOptions::LINEAR,
                )),
                IconSource::Initials { text, color } => Icon::Initials {
                    text,
                    color: Color32::from_rgb(color[0], color[1], color[2]),
                },
            })
            .collect();
        Self { icons }
    }

    fn get(&self, stack: &ItemStack) -> Option<&Icon> {
        self.icons.get(stack.item.0 as usize)
    }
}

/// Text lines shown briefly above the hotbar.
#[derive(Default)]
pub struct Notices {
    list: Vec<(String, f64)>,
}

const NOTICE_SECONDS: f64 = 4.0;
const NOTICE_FADE: f64 = 1.0;
const NOTICES_SHOWN: usize = 4;

impl Notices {
    pub fn push(&mut self, text: String, now: f64) {
        // The same line again restarts its clock instead of stacking.
        self.list.retain(|(t, _)| *t != text);
        self.list.push((text, now));
    }

    /// Lines still showing, oldest first, with their opacity.
    pub fn visible(&mut self, now: f64) -> Vec<(String, f32)> {
        self.list.retain(|(_, t)| now - t < NOTICE_SECONDS);
        let skip = self.list.len().saturating_sub(NOTICES_SHOWN);
        self.list[skip..]
            .iter()
            .map(|(text, t)| {
                let left = NOTICE_SECONDS - (now - t);
                (text.clone(), (left / NOTICE_FADE).clamp(0.0, 1.0) as f32)
            })
            .collect()
    }
}

pub fn install_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        use egui::{FontFamily, TextStyle};
        style.text_styles = [
            (TextStyle::Heading, FontId::new(24.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(16.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(16.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(15.0, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 7.0);
        style.spacing.button_padding = vec2(12.0, 5.0);
        style.spacing.window_margin = egui::Margin::same(16);
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(TEXT);
        v.window_fill = PANEL;
        v.panel_fill = PANEL;
        v.window_corner_radius = CornerRadius::same(14);
        v.window_stroke = Stroke::new(1.5, GOLD_DIM);
        v.window_shadow = egui::Shadow {
            offset: [0, 6],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(140),
        };
        v.extreme_bg_color = Color32::from_rgba_premultiplied(4, 4, 8, 210);
        v.faint_bg_color = Color32::from_rgba_premultiplied(255, 255, 255, 8);
        v.selection.bg_fill = Color32::from_rgb(120, 86, 30);
        v.selection.stroke = Stroke::new(1.0, GOLD);
        v.hyperlink_color = GOLD;
        v.error_fg_color = SHORT;
        let round = CornerRadius::same(8);
        for (w, fill, stroke) in [
            (
                &mut v.widgets.inactive,
                Color32::from_rgb(44, 38, 34),
                Stroke::new(1.0, GOLD_DIM),
            ),
            (
                &mut v.widgets.hovered,
                Color32::from_rgb(78, 60, 36),
                Stroke::new(1.5, GOLD),
            ),
            (
                &mut v.widgets.active,
                Color32::from_rgb(120, 86, 34),
                Stroke::new(1.5, GOLD),
            ),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = stroke;
            w.corner_radius = round;
            w.fg_stroke = Stroke::new(1.0, TEXT);
        }
        v.widgets.noninteractive.corner_radius = round;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgba_premultiplied(90, 80, 64, 90));
    });
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).heading().color(GOLD));
}

fn window(title: &str) -> egui::Window<'static> {
    egui::Window::new(egui::RichText::new(title.to_string()).color(GOLD).size(20.0))
        .id(Id::unique(("menu", title.to_string())))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -30.0))
        .collapsible(false)
        .resizable(false)
        .movable(false)
}

// ---- the login screen ----

pub fn login(ctx: &egui::Context, form: &mut LoginForm, actions: &mut Vec<UiAction>) {
    egui::Area::new(Id::unique("login"))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -20.0))
        .show(ctx, |ui| {
            egui::Frame::window(ui.style()).show(ui, |ui| {
                ui.set_width(340.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("Boundmore").size(40.0).color(GOLD));
                    ui.label(egui::RichText::new("Log in to your account, or make a new one.").color(MUTED));
                });
                ui.add_space(10.0);
                let field = |ui: &mut Ui, label: &str, value: &mut String, secret: bool, enabled: bool| {
                    ui.label(egui::RichText::new(label).color(MUTED).small());
                    ui.add_enabled(
                        enabled,
                        egui::TextEdit::singleline(value)
                            .password(secret)
                            .desired_width(f32::INFINITY)
                            .margin(vec2(8.0, 6.0)),
                    )
                };
                let enabled = !form.busy;
                let name = field(ui, "Name", &mut form.name, false, enabled);
                let pass = field(ui, "Password", &mut form.password, true, enabled);
                ui.add_space(2.0);
                let server = field(ui, "Server", &mut form.server, false, enabled);
                if form.name.is_empty() && !form.busy && !ui.memory(|m| m.has_focus(server.id)) {
                    name.request_focus();
                }
                let enter = (name.lost_focus() || pass.lost_focus() || server.lost_focus())
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.add_space(8.0);
                if let Some(e) = &form.error {
                    ui.label(egui::RichText::new(e).color(SHORT));
                    ui.add_space(4.0);
                }
                if form.busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Connecting to {}", form.server));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Cancel").clicked() {
                                actions.push(UiAction::CancelLogin);
                            }
                        });
                    });
                } else {
                    ui.columns(2, |cols| {
                        let full = |ui: &mut Ui, text: &str, primary: bool| {
                            let mut b = egui::Button::new(egui::RichText::new(text).strong());
                            if primary {
                                b = b.fill(Color32::from_rgb(120, 86, 34));
                            }
                            ui.add_sized([ui.available_width(), 34.0], b).clicked()
                        };
                        if full(&mut cols[0], "Log in", true) || enter {
                            actions.push(UiAction::LogIn { register: false });
                        }
                        if full(&mut cols[1], "Create account", false) {
                            actions.push(UiAction::LogIn { register: true });
                        }
                    });
                }
            });
        });
}

// ---- items ----

fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Option<&Icon>, tint: f32) {
    let alpha = (tint * 255.0) as u8;
    match icon {
        Some(Icon::Texture(t)) => {
            painter.image(
                t.id(),
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::from_white_alpha(alpha),
            );
        }
        Some(Icon::Initials { text, color }) => {
            let r = rect.shrink(rect.width() * 0.1);
            let cr = CornerRadius::same((r.width() * 0.22) as u8);
            painter.rect_filled(r, cr, color.gamma_multiply(tint));
            // A lighter top-left rim, as the art direction's light falls.
            painter.rect_stroke(
                r,
                cr,
                Stroke::new(1.5, Color32::from_white_alpha((70.0 * tint) as u8)),
                StrokeKind::Inside,
            );
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(r.height() * 0.42),
                Color32::from_white_alpha(alpha),
            );
        }
        None => {}
    }
}

/// Draws one stack in a slot-sized rect: icon, count and wear.
fn paint_stack(painter: &egui::Painter, rect: Rect, stack: &ItemStack, items: &Items, icons: &ItemIcons, tint: f32) {
    let inset = rect.width() * 0.13;
    paint_icon(painter, rect.shrink(inset), icons.get(stack), tint);
    if stack.count > 1 {
        let font = FontId::proportional((rect.height() * 0.3).max(11.0));
        let at = rect.right_bottom() - vec2(4.0, 2.0);
        let text = stack.count.to_string();
        painter.text(
            at + vec2(1.0, 1.0),
            Align2::RIGHT_BOTTOM,
            &text,
            font.clone(),
            Color32::BLACK,
        );
        painter.text(
            at,
            Align2::RIGHT_BOTTOM,
            &text,
            font,
            Color32::WHITE.gamma_multiply(tint),
        );
    }
    if let Some((left, total)) = items.durability(stack)
        && left < total
    {
        let frac = left as f32 / total.max(1) as f32;
        let bar = Rect::from_min_size(rect.left_bottom() + vec2(6.0, -7.0), vec2(rect.width() - 12.0, 3.0));
        painter.rect_filled(bar.expand(1.0), 1, Color32::from_black_alpha(200));
        let colour = if frac > 0.5 {
            Color32::from_rgb(110, 210, 90)
        } else if frac > 0.2 {
            Color32::from_rgb(232, 196, 64)
        } else {
            RED
        };
        painter.rect_filled(
            Rect::from_min_size(bar.min, vec2(bar.width() * frac, bar.height())),
            1,
            colour,
        );
    }
}

fn paint_slot_back(painter: &egui::Painter, rect: Rect, rim: Stroke) {
    painter.rect_filled(rect, CornerRadius::same(8), SLOT_FILL);
    painter.rect_stroke(rect, CornerRadius::same(8), rim, StrokeKind::Inside);
}

/// A small icon with a label beside it, for recipe rows.
fn item_chip(ui: &mut Ui, stack: &ItemStack, items: &Items, icons: &ItemIcons, size: f32, label: egui::RichText) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        paint_icon(ui.painter(), rect, icons.get(stack), 1.0);
        resp.on_hover_text(items.tooltip(&ItemStack { count: 1, ..*stack }));
        ui.label(label);
    });
}

// ---- the HUD ----

pub struct Hud<'a> {
    pub items: &'a Items,
    pub icons: &'a ItemIcons,
    pub selected: usize,
    pub notices: Vec<(String, f32)>,
    /// Shown small at the top: whose land this is.
    pub owner: Option<(String, bool)>,
    /// Claim mode's help and allowance, when it is on.
    pub claim_mode: Option<String>,
}

pub fn hud(ctx: &egui::Context, h: &Hud) {
    let width = HOTBAR_SLOTS as f32 * SLOT + (HOTBAR_SLOTS - 1) as f32 * GAP;
    egui::Area::new(Id::unique("hotbar"))
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -18.0))
        .interactable(false)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(width, SLOT + 6.0), Sense::hover());
            let painter = ui.painter();
            for i in 0..HOTBAR_SLOTS {
                let selected = i == h.selected;
                let x = rect.left() + i as f32 * (SLOT + GAP);
                // The selected slot rises a little, like a card lifted from a hand.
                let y = rect.top() + if selected { 0.0 } else { 6.0 };
                let r = Rect::from_min_size(pos2(x, y), Vec2::splat(SLOT));
                let rim = if selected {
                    Stroke::new(3.0, GOLD)
                } else {
                    Stroke::new(1.5, SLOT_RIM)
                };
                paint_slot_back(painter, r, rim);
                painter.text(
                    r.left_top() + vec2(5.0, 3.0),
                    Align2::LEFT_TOP,
                    ((i + 1) % 10).to_string(),
                    FontId::proportional(11.0),
                    if selected { GOLD } else { MUTED.gamma_multiply(0.8) },
                );
                if let Some(stack) = h.items.slot(i) {
                    paint_stack(painter, r, &stack, h.items, h.icons, 1.0);
                }
            }
        });

    if !h.notices.is_empty() {
        egui::Area::new(Id::unique("notices"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -(18.0 + SLOT + 26.0)))
            .interactable(false)
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    for (text, alpha) in &h.notices {
                        egui::Frame::new()
                            .fill(Color32::from_black_alpha((150.0 * alpha) as u8))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(egui::Margin::symmetric(14, 5))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(text).color(TEXT.gamma_multiply(*alpha)).size(17.0));
                            });
                    }
                });
            });
    }

    if h.owner.is_some() || h.claim_mode.is_some() {
        egui::Area::new(Id::unique("claim-info"))
            .anchor(Align2::CENTER_TOP, vec2(0.0, 14.0))
            .interactable(false)
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    if let Some((owner, mine)) = &h.owner {
                        egui::Frame::new()
                            .fill(Color32::from_black_alpha(120))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(egui::Margin::symmetric(12, 3))
                            .show(ui, |ui| {
                                let (text, colour) = if *mine {
                                    ("Your claim".to_string(), GOLD)
                                } else {
                                    (format!("{owner}'s claim"), Color32::from_rgb(240, 150, 140))
                                };
                                ui.label(egui::RichText::new(text).color(colour).size(14.0));
                            });
                    }
                    if let Some(help) = &h.claim_mode {
                        egui::Frame::window(ui.style())
                            .inner_margin(egui::Margin::symmetric(14, 8))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Claim mode").color(GOLD).strong());
                                ui.label(egui::RichText::new(help).size(14.0));
                            });
                    }
                });
            });
    }
}

// ---- the inventory screen ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Slot(usize),
    Discard,
}

pub fn inventory(
    ctx: &egui::Context,
    items: &Items,
    icons: &ItemIcons,
    selected: usize,
    drag: &mut Option<Drag>,
    actions: &mut Vec<UiAction>,
) {
    let mut targets: Vec<(Rect, Target)> = Vec::new();
    let mut open = true;
    window("Inventory").open(&mut open).show(ctx, |ui| {
        let cols = HOTBAR_SLOTS;
        let painter_slot = |ui: &mut Ui, i: usize, targets: &mut Vec<(Rect, Target)>, drag: &mut Option<Drag>| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SLOT), Sense::click_and_drag());
            targets.push((rect, Target::Slot(i)));
            let hot = i < HOTBAR_SLOTS;
            let hovering_drop = drag.is_some() && resp.contains_pointer();
            let rim = if hovering_drop || (hot && i == selected) {
                Stroke::new(2.5, GOLD)
            } else if hot {
                Stroke::new(1.5, GOLD_DIM)
            } else {
                Stroke::new(1.5, SLOT_RIM)
            };
            let painter = ui.painter();
            paint_slot_back(painter, rect, rim);
            if hot {
                painter.text(
                    rect.left_top() + vec2(5.0, 3.0),
                    Align2::LEFT_TOP,
                    ((i + 1) % 10).to_string(),
                    FontId::proportional(11.0),
                    MUTED,
                );
            }
            let Some(stack) = items.slot(i) else { return };
            let dragging_this = drag.is_some_and(|d| d.from == i);
            paint_stack(
                painter,
                rect,
                &stack,
                items,
                icons,
                if dragging_this { 0.35 } else { 1.0 },
            );
            if resp.drag_started() {
                let half =
                    ui.input(|inp| inp.pointer.button_down(egui::PointerButton::Secondary) || inp.modifiers.shift);
                *drag = Some(Drag { from: i, half });
            }
            if drag.is_none() {
                resp.on_hover_text(items.tooltip(&stack));
            }
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Hotbar").color(GOLD).small());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Keys 1 to 0 pick a hotbar slot")
                        .color(MUTED)
                        .small(),
                );
            });
        });
        ui.spacing_mut().item_spacing = vec2(GAP, GAP);
        ui.horizontal(|ui| {
            for i in 0..cols {
                painter_slot(ui, i, &mut targets, drag);
            }
        });
        ui.add_space(10.0);
        ui.label(egui::RichText::new("Pack").color(MUTED).small());
        for row in 1..INVENTORY_SLOTS / cols {
            ui.horizontal(|ui| {
                for c in 0..cols {
                    painter_slot(ui, row * cols + c, &mut targets, drag);
                }
            });
        }
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing = vec2(8.0, 7.0);
        ui.horizontal(|ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(180.0, 40.0), Sense::hover());
            targets.push((rect, Target::Discard));
            let over = drag.is_some() && resp.contains_pointer();
            let painter = ui.painter();
            painter.rect_filled(
                rect,
                CornerRadius::same(8),
                if over {
                    Color32::from_rgb(110, 34, 30)
                } else {
                    Color32::from_rgba_premultiplied(40, 14, 14, 160)
                },
            );
            painter.rect_stroke(
                rect,
                CornerRadius::same(8),
                Stroke::new(1.5, if over { RED } else { Color32::from_rgb(120, 60, 52) }),
                StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Throw away",
                FontId::proportional(15.0),
                if over { TEXT } else { MUTED },
            );
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Drag to move a stack. Right drag or Shift drag moves half.")
                        .color(MUTED)
                        .small(),
                );
                ui.label(
                    egui::RichText::new("Drop on Throw away to get rid of the whole stack.")
                        .color(MUTED)
                        .small(),
                );
            });
        });
    });
    if !open {
        actions.push(UiAction::CloseMenu);
    }

    // The dragged stack follows the pointer, over everything.
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    if let (Some(d), Some(p)) = (*drag, pointer)
        && let Some(stack) = items.slot(d.from)
    {
        let shown = ItemStack {
            count: if d.half {
                crate::items::half(stack.count)
            } else {
                stack.count
            },
            ..stack
        };
        egui::Area::new(Id::unique("dragged"))
            .order(egui::Order::Tooltip)
            .fixed_pos(p - Vec2::splat(SLOT / 2.0))
            .interactable(false)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(SLOT), Sense::hover());
                paint_stack(ui.painter(), rect, &shown, items, icons, 1.0);
            });
    }
    if let Some(d) = *drag
        && ctx.input(|i| i.pointer.any_released())
    {
        *drag = None;
        let target = pointer.and_then(|p| targets.iter().find(|(r, _)| r.contains(p)).map(|(_, t)| *t));
        match target {
            Some(Target::Slot(to)) => {
                if let Some(msg) = items.move_request(d.from, to, d.half) {
                    actions.push(UiAction::Send(msg));
                }
            }
            Some(Target::Discard) => actions.push(UiAction::Send(ClientMsg::Discard { slot: d.from as u8 })),
            None => {}
        }
    }
}

// ---- crafting and stations ----

pub struct CraftingView<'a> {
    pub items: &'a Items,
    pub icons: &'a ItemIcons,
    /// The station looked at, its kind and display name.
    pub station: Option<(BlockPos, Station, String)>,
    pub live: Option<&'a LiveStation>,
    pub now: f64,
}

pub fn station_name(blocks: &BlockRegistry, station: Station) -> String {
    blocks
        .iter()
        .find(|(_, d)| d.station == Some(station))
        .map(|(_, d)| bm_core::items::title_case(&d.name))
        .unwrap_or_else(|| format!("{station:?}"))
}

pub fn crafting(ctx: &egui::Context, v: &CraftingView, actions: &mut Vec<UiAction>) {
    let title = match &v.station {
        Some((_, _, name)) => name.clone(),
        None => "Crafting".to_string(),
    };
    let kind = v.station.as_ref().map(|s| s.1).unwrap_or(Station::Hand);
    let mut open = true;
    window(&title).open(&mut open).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(470.0);
                if kind == Station::Hand {
                    ui.label(egui::RichText::new("Made in your hands, at once.").color(MUTED).small());
                } else {
                    ui.label(
                        egui::RichText::new("Queued here; it keeps working while you are away.")
                            .color(MUTED)
                            .small(),
                    );
                }
                egui::ScrollArea::vertical().max_height(430.0).show(ui, |ui| {
                    for (index, recipe) in v.items.recipes.at(kind) {
                        recipe_row(ui, v, index, recipe, actions);
                    }
                });
            });
            if let Some((pos, _, _)) = v.station {
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(250.0);
                    station_panel(ui, v, pos, actions);
                });
            }
        });
    });
    if !open {
        actions.push(UiAction::CloseMenu);
    }
}

fn recipe_row(ui: &mut Ui, v: &CraftingView, index: u16, recipe: &bm_core::Recipe, actions: &mut Vec<UiAction>) {
    let max = v.items.craft_max(recipe);
    egui::Frame::new()
        .fill(Color32::from_rgba_premultiplied(255, 255, 255, 6))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let out = ItemStack::new(recipe.output, recipe.output_count);
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::hover());
                paint_slot_back(ui.painter(), rect, Stroke::new(1.0, SLOT_RIM));
                paint_stack(ui.painter(), rect, &out, v.items, v.icons, 1.0);
                resp.on_hover_text(v.items.tooltip(&ItemStack { count: 1, ..out }));
                ui.vertical(|ui| {
                    let name = v
                        .items
                        .registry
                        .get(recipe.output)
                        .map(|d| d.display.clone())
                        .unwrap_or_default();
                    let mut title = name;
                    if recipe.output_count > 1 {
                        title = format!("{title} x{}", recipe.output_count);
                    }
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(title).strong());
                        if recipe.seconds > 0.0 {
                            ui.label(
                                egui::RichText::new(format!("{} s each", recipe.seconds))
                                    .color(MUTED)
                                    .small(),
                            );
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (item, need) in &recipe.inputs {
                            let have = v.items.inventory.count(*item);
                            let colour = if have >= *need as u32 { MUTED } else { SHORT };
                            item_chip(
                                ui,
                                &ItemStack::new(*item, 1),
                                v.items,
                                v.icons,
                                22.0,
                                egui::RichText::new(format!("{have}/{need}")).color(colour).small(),
                            );
                        }
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let station = v.station.as_ref().map(|s| s.0);
                    let mut craft = |ui: &mut Ui, label: String, count: u16| {
                        if ui
                            .add_enabled(count > 0 && max >= count, egui::Button::new(label))
                            .clicked()
                        {
                            actions.push(UiAction::Send(ClientMsg::Craft {
                                recipe: index,
                                count,
                                station,
                            }));
                        }
                    };
                    craft(ui, format!("Max ({max})"), max);
                    craft(ui, "5".into(), 5);
                    craft(ui, "Craft".into(), 1);
                });
            });
        });
}

fn station_panel(ui: &mut Ui, v: &CraftingView, pos: BlockPos, actions: &mut Vec<UiAction>) {
    let live = v.live.filter(|l| l.view.pos == pos);
    heading(ui, "Queue");
    match live {
        None => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new("Looking inside").color(MUTED));
            });
        }
        Some(l) if l.view.jobs.is_empty() => {
            ui.label(egui::RichText::new("Nothing is being made.").color(MUTED));
        }
        Some(l) => {
            for (i, job) in l.view.jobs.iter().enumerate() {
                let Some(r) = v.items.recipes.get(job.recipe) else {
                    continue;
                };
                ui.horizontal(|ui| {
                    let name = v
                        .items
                        .registry
                        .get(r.output)
                        .map(|d| d.display.clone())
                        .unwrap_or_default();
                    item_chip(
                        ui,
                        &ItemStack::new(r.output, 1),
                        v.items,
                        v.icons,
                        28.0,
                        egui::RichText::new(format!("{name}, {} to go", job.remaining)),
                    );
                });
                if i == 0 {
                    // Run on between updates, which come a few times a second.
                    let p = (job.progress + ((v.now - l.received) / r.seconds.max(0.1) as f64) as f32).min(1.0);
                    ui.add(egui::ProgressBar::new(p).desired_height(10.0).fill(GOLD_DIM));
                }
            }
        }
    }
    ui.add_space(12.0);
    heading(ui, "Ready");
    let output = live.map(|l| l.view.output.as_slice()).unwrap_or(&[]);
    if output.is_empty() {
        ui.label(egui::RichText::new("Nothing finished yet.").color(MUTED));
    } else {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            for stack in output {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SLOT - 6.0), Sense::hover());
                paint_slot_back(ui.painter(), rect, Stroke::new(1.0, SLOT_RIM));
                paint_stack(ui.painter(), rect, stack, v.items, v.icons, 1.0);
                resp.on_hover_text(v.items.tooltip(stack));
            }
        });
    }
    ui.add_space(6.0);
    if ui
        .add_enabled(
            !output.is_empty(),
            egui::Button::new(egui::RichText::new("Collect").strong()),
        )
        .clicked()
    {
        actions.push(UiAction::Send(ClientMsg::CollectStation { pos }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_fade_and_expire() {
        let mut n = Notices::default();
        n.push("Your inventory is full".into(), 0.0);
        n.push("b".into(), 1.0);
        assert_eq!(n.visible(0.5).len(), 2);
        let v = n.visible(3.5);
        assert_eq!(v[0].1, 0.5, "the first is half faded");
        assert_eq!(v[1].1, 1.0);
        assert_eq!(n.visible(4.5).len(), 1);
        // Repeats restart rather than pile up.
        n.push("b".into(), 4.5);
        n.push("b".into(), 4.6);
        assert_eq!(n.visible(4.7).len(), 1);
        for i in 0..10 {
            n.push(format!("line {i}"), 5.0);
        }
        assert_eq!(n.visible(5.0).len(), NOTICES_SHOWN);
    }

    #[test]
    fn stations_are_named_from_the_catalogue() {
        let b = BlockRegistry::builtin();
        assert_eq!(station_name(&b, Station::Workbench), "Workbench");
        assert_eq!(station_name(&b, Station::Furnace), "Furnace");
    }
}
