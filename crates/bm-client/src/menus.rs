//! Menus and HUD, drawn with egui over the game.
//!
//! Screens never act on the game themselves: each returns `UiAction`s that
//! `App` carries out (sending messages, connecting, closing). The look follows
//! `docs/art-direction.md`: dark translucent panels, warm gold accents,
//! rounded corners. Player-facing text uses no em dashes.

use crate::icons::{ICON_SIZE, IconSource};
use crate::items::Items;
use crate::skill_screen::grouped;
use bm_core::block::Assembly;
use bm_core::{
    BlockDef, BlockPos, BlockRegistry, Colour, HOTBAR_SLOTS, INVENTORY_SLOTS, ItemId, ItemStack, PlanetShape, Station,
};
use bm_protocol::{ClientMsg, PlanetSummary, PortalView, SlotRef, SparkView, StationView};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use std::collections::HashMap;

pub const GOLD: Color32 = Color32::from_rgb(240, 186, 84);
pub(crate) const GOLD_DIM: Color32 = Color32::from_rgb(150, 116, 62);
pub(crate) const TEXT: Color32 = Color32::from_rgb(236, 228, 212);
pub(crate) const MUTED: Color32 = Color32::from_rgb(170, 162, 150);
pub(crate) const SHORT: Color32 = Color32::from_rgb(235, 104, 92);
const RED: Color32 = Color32::from_rgb(226, 82, 70);
const PANEL: Color32 = Color32::from_rgba_premultiplied(13, 12, 19, 222);
const SLOT_FILL: Color32 = Color32::from_rgba_premultiplied(5, 5, 9, 150);
pub(crate) const SLOT_RIM: Color32 = Color32::from_rgba_unmultiplied_const(120, 110, 92, 140);

const SLOT: f32 = 54.0;
const GAP: f32 = 6.0;

/// What the player asked for this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum UiAction {
    Send(ClientMsg),
    LogIn {
        register: bool,
    },
    CancelLogin,
    CloseMenu,
    /// A change to the character's skills (milestone 12): predicted, then
    /// sent following the latest input.
    Skill(bm_protocol::SkillAction),
}

/// Which screen holds the mouse, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Inventory,
    /// Hand crafting, or the station at this block.
    Crafting(Option<BlockPos>),
    /// The travel menu of the warp shrine at this block.
    Travel(BlockPos),
    /// The storage block at this block, beside the inventory.
    Storage(BlockPos),
    /// The portal one of whose blocks is here (ADR-0009).
    Portal(BlockPos),
    /// The skill screen (milestone 12).
    Skills,
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

/// An item being dragged in the inventory or an open storage block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drag {
    pub from: SlotRef,
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

/// One icon per item id, plus one per (item, colour) for coloured stacks,
/// made as they are first seen (`missing_colours`, `add_coloured`).
#[derive(Default)]
pub struct ItemIcons {
    icons: Vec<Icon>,
    coloured: HashMap<(ItemId, Colour), Icon>,
}

fn icon_from(ctx: &egui::Context, name: String, source: IconSource) -> Icon {
    match source {
        IconSource::Image(rgba) => Icon::Texture(ctx.load_texture(
            name,
            egui::ColorImage::from_rgba_unmultiplied([ICON_SIZE, ICON_SIZE], &rgba),
            egui::TextureOptions::LINEAR,
        )),
        IconSource::Initials { text, color } => Icon::Initials {
            text,
            color: Color32::from_rgb(color[0], color[1], color[2]),
        },
    }
}

impl ItemIcons {
    pub fn build(ctx: &egui::Context, sources: Vec<IconSource>) -> Self {
        let icons = sources
            .into_iter()
            .enumerate()
            .map(|(i, s)| icon_from(ctx, format!("item-{i}"), s))
            .collect();
        Self {
            icons,
            coloured: HashMap::new(),
        }
    }

    /// A stack's icon: its colour's if made, else the item's own.
    fn get(&self, stack: &ItemStack) -> Option<&Icon> {
        if stack.colour != Colour::NATURAL
            && let Some(icon) = self.coloured.get(&(stack.item, stack.colour))
        {
            return Some(icon);
        }
        self.icons.get(stack.item.0 as usize)
    }

    /// The coloured stacks among `stacks` that have no icon of their own
    /// yet, each once.
    pub fn missing_colours<'a>(&self, stacks: impl IntoIterator<Item = &'a ItemStack>) -> Vec<(ItemId, Colour)> {
        let mut out = Vec::new();
        for s in stacks {
            let key = (s.item, s.colour);
            if s.colour != Colour::NATURAL && !self.coloured.contains_key(&key) && !out.contains(&key) {
                out.push(key);
            }
        }
        out
    }

    pub fn add_coloured(&mut self, ctx: &egui::Context, key: (ItemId, Colour), source: IconSource) {
        let name = format!("item-{}-{}", key.0.0, key.1.0);
        self.coloured.insert(key, icon_from(ctx, name, source));
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
        v.faint_bg_color = Color32::from_white_alpha(8);
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
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 80, 64, 110));
    });
}

pub(crate) fn heading(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).heading().color(GOLD));
}

pub(crate) fn window(title: &str) -> egui::Window<'static> {
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
pub(crate) fn paint_stack(
    painter: &egui::Painter,
    rect: Rect,
    stack: &ItemStack,
    items: &Items,
    icons: &ItemIcons,
    tint: f32,
) {
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

pub(crate) fn paint_slot_back(painter: &egui::Painter, rect: Rect, rim: Stroke) {
    painter.rect_filled(rect, CornerRadius::same(8), SLOT_FILL);
    painter.rect_stroke(rect, CornerRadius::same(8), rim, StrokeKind::Inside);
}

/// A small icon with a label beside it, for recipe rows.
pub(crate) fn item_chip(
    ui: &mut Ui,
    stack: &ItemStack,
    items: &Items,
    icons: &ItemIcons,
    size: f32,
    label: egui::RichText,
) {
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
    /// What a key does to the block aimed at ("E", "travel"), shown under
    /// the crosshair.
    pub aim_hint: Option<(&'static str, &'static str)>,
    /// The crop under the crosshair (milestone 16): "Oats: ripe in 20 min".
    pub crop: Option<String>,
    /// Something waits in the reclaim (items from a lapsed claim).
    pub reclaim_waiting: bool,
    /// The aimed block's health, shown under the crosshair.
    pub block_bar: Option<BlockBar>,
    /// A machine's wear left and the most it holds, for a moment after a
    /// spanner hits it (milestone 15); shown in the block bar's place.
    pub wear_bar: Option<(u32, u32)>,
    /// Energy left, 0 to 1 of the bar with no hunger (milestone 10), shown
    /// as a bar over the hotbar.
    pub energy: f32,
    /// Hunger's share of that bar, 0 to 1 (milestone 16): its right end,
    /// shaded.
    pub hunger: f32,
    /// The food buff with its time left, and Full, Sick or Starving, as
    /// tags over the bars (`food_tags`).
    pub food: Vec<(String, Color32)>,
    /// Level, XP into it and XP it needs (milestone 12), shown as a thin
    /// bar under the hotbar.
    pub level: Option<(u32, u64, u64)>,
    /// Health left and the most there is (milestone 13), a bar beside
    /// energy's.
    pub health: (u32, u32),
    /// XP still owed after a defeat: a red stretch on the XP bar.
    pub debt: u64,
    /// Where we are: "Amber Hollows" or "North of Amber Hollows" (milestone
    /// 14), at the top.
    pub region: Option<String>,
    /// The nearest meteor's status and its bar (the fight left), under it.
    pub meteor: Option<(String, f32)>,
    /// The meteor life marker, beside the meteor's status.
    pub life: Option<String>,
    /// Air (milestone 18), while it is not full or protection falls short.
    pub air: Option<AirView>,
}

/// The air bar: what is left, and what the tag over it says.
#[derive(Clone, Debug, PartialEq)]
pub struct AirView {
    /// 0 to 1.
    pub left: f32,
    /// "Acid air: 2 short", "Choking", or "Air" while it comes back.
    pub text: String,
    /// Short of protection (drawn in warning colours).
    pub short: bool,
}

/// What the air bar shows for `s`, or nothing (full, and safe).
pub fn air_view(s: &bm_core::PlayerState) -> Option<AirView> {
    let short = s.shortfall();
    let left = s.air_left();
    if short == 0 && left >= 1.0 {
        return None;
    }
    let text = if s.choking() {
        format!("Choking: {} short", short)
    } else if short > 0 {
        let line = s.air.line.name();
        let mut c = line.chars();
        let line = c
            .next()
            .map(|f| f.to_uppercase().chain(c).collect::<String>())
            .unwrap_or_default();
        format!("{line}: {short} short of {}", s.air.line.ward())
    } else {
        "Air".to_string()
    };
    Some(AirView {
        left,
        text,
        short: short > 0,
    })
}

const XP_BAR: Color32 = Color32::from_rgb(132, 196, 236);
/// A meteor's status and bar.
const METEOR_EMBER: Color32 = Color32::from_rgb(255, 150, 80);
/// XP owed after a defeat, on the XP bar and the skill screen.
pub const DEBT_RED: Color32 = Color32::from_rgb(226, 82, 70);
/// The health bar's fill: a warm red, darker when low.
const HEALTH_FULL: Color32 = Color32::from_rgb(214, 64, 58);
const HEALTH_LOW: Color32 = Color32::from_rgb(150, 34, 34);

/// The energy bar's fill: a warm yellow, turning red when nearly empty.
const ENERGY_FULL: Color32 = Color32::from_rgb(238, 200, 96);
const ENERGY_LOW: Color32 = Color32::from_rgb(214, 96, 70);
/// Hunger's end of the energy bar, and the food tags.
const HUNGER: Color32 = Color32::from_rgb(92, 64, 44);
/// The air bar (milestone 18): pale sky blue, going violet as it runs out.
const AIR_FULL: Color32 = Color32::from_rgb(150, 215, 245);
const AIR_LOW: Color32 = Color32::from_rgb(170, 120, 235);
const AIR_WARN: Color32 = Color32::from_rgb(255, 190, 120);
const FOOD_BUFF: Color32 = Color32::from_rgb(150, 220, 140);
const FOOD_FULL: Color32 = Color32::from_rgb(196, 230, 150);
const FOOD_BAD: Color32 = Color32::from_rgb(230, 120, 96);

/// What food is doing, for the HUD (milestone 16): the buff ("Sated I
/// 9:41"), then Full, Sick or Starving.
pub fn food_tags(s: &bm_core::PlayerState) -> Vec<(String, Color32)> {
    let mut tags = Vec::new();
    if s.buff.left > 0 && s.buff.kind != bm_core::food::Buff::None {
        let secs = s.buff.left / bm_core::TICK_HZ;
        tags.push((
            format!(
                "{} {} {}:{:02}",
                s.buff.kind.name(),
                crate::items::roman(s.buff.rank),
                secs / 60,
                secs % 60
            ),
            FOOD_BUFF,
        ));
    }
    if s.starving() {
        tags.push(("Starving: eat something".into(), FOOD_BAD));
    } else if s.sick > 0 {
        tags.push(("Sick".into(), FOOD_BAD));
    } else if s.is_full() {
        tags.push(("Full".into(), FOOD_FULL));
    }
    tags
}

/// A block's health bar: what is left (0 to 1), and one shield per 100
/// armour of the planet's tier, as Boundless showed it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockBar {
    pub left: f32,
    pub shields: u32,
}

const BAR_WIDTH: f32 = 96.0;
const BAR_HEIGHT: f32 = 8.0;
const BAR_FULL: Color32 = Color32::from_rgb(150, 214, 120);
const SHIELD_FILL: Color32 = Color32::from_rgb(196, 210, 232);
/// The bar sits this far below the screen's centre.
const BAR_DROP: f32 = 30.0;

/// The fill colour for `left`: fresh green, through gold, to red.
pub(crate) fn bar_colour(left: f32) -> Color32 {
    let mix = |a: Color32, b: Color32, t: f32| {
        let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
    };
    if left > 0.5 {
        mix(GOLD, BAR_FULL, (left - 0.5) * 2.0)
    } else {
        mix(RED, GOLD, left * 2.0)
    }
}

/// A small heater shield, `w` wide, its top edge at `top`.
pub(crate) fn paint_shield(painter: &egui::Painter, centre_x: f32, top: f32, w: f32) {
    let h = w * 1.2;
    let (l, r) = (centre_x - w / 2.0, centre_x + w / 2.0);
    let points = vec![
        pos2(l, top),
        pos2(r, top),
        pos2(r, top + h * 0.5),
        pos2(centre_x, top + h),
        pos2(l, top + h * 0.5),
    ];
    painter.add(egui::Shape::convex_polygon(
        points,
        SHIELD_FILL,
        Stroke::new(1.2, Color32::from_black_alpha(200)),
    ));
}

fn block_bar(ctx: &egui::Context, bar: &BlockBar) {
    let shield_w = 11.0;
    let shield_h = shield_w * 1.2;
    let shields_w = bar.shields as f32 * (shield_w + 3.0);
    let width = BAR_WIDTH + if bar.shields > 0 { shields_w + 3.0 } else { 0.0 };
    egui::Area::new(Id::unique("block-health"))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, BAR_DROP))
        .interactable(false)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            // On a dark pill, like the other hints, so it reads on any block.
            egui::Frame::new()
                .fill(Color32::from_black_alpha(140))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(7, 4))
                .show(ui, |ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(width, shield_h), Sense::hover());
                    let painter = ui.painter();
                    // Shields first, at the bar's left, like armour worn over it.
                    for i in 0..bar.shields {
                        let cx = rect.left() + shield_w / 2.0 + i as f32 * (shield_w + 3.0);
                        paint_shield(painter, cx, rect.top(), shield_w);
                    }
                    let back = Rect::from_min_size(
                        pos2(rect.right() - BAR_WIDTH, rect.center().y - BAR_HEIGHT / 2.0),
                        vec2(BAR_WIDTH, BAR_HEIGHT),
                    );
                    painter.rect_filled(back, CornerRadius::same(3), Color32::from_rgb(38, 34, 32));
                    let fill = Rect::from_min_size(back.min, vec2(BAR_WIDTH * bar.left.clamp(0.0, 1.0), BAR_HEIGHT));
                    if fill.width() > 0.5 {
                        painter.rect_filled(fill, CornerRadius::same(3), bar_colour(bar.left));
                    }
                    painter.rect_stroke(
                        back,
                        CornerRadius::same(3),
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(240, 230, 210, 90)),
                        StrokeKind::Outside,
                    );
                });
        });
}

/// A machine's wear under the crosshair after a spanner hit: its bar and
/// "Wear 8,450 / 10,000" beside it.
fn wear_bar(ctx: &egui::Context, (left, max): (u32, u32)) {
    let frac = left as f32 / max.max(1) as f32;
    egui::Area::new(Id::unique("machine-wear"))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, BAR_DROP))
        .interactable(false)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_black_alpha(140))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(7, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(BAR_WIDTH, BAR_HEIGHT), Sense::hover());
                        let painter = ui.painter();
                        painter.rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(38, 34, 32));
                        let fill = Rect::from_min_size(rect.min, vec2(BAR_WIDTH * frac.clamp(0.0, 1.0), BAR_HEIGHT));
                        if fill.width() > 0.5 {
                            painter.rect_filled(fill, CornerRadius::same(3), bar_colour(frac));
                        }
                        painter.rect_stroke(
                            rect,
                            CornerRadius::same(3),
                            Stroke::new(1.0, Color32::from_rgba_unmultiplied(240, 230, 210, 90)),
                            StrokeKind::Outside,
                        );
                        ui.label(egui::RichText::new(wear_text((left, max))).color(TEXT).size(13.0));
                    });
                });
        });
}

/// The HUD's reminder while the reclaim holds anything.
pub const RECLAIM_HINT: &str = "Items wait for you at any warp shrine";

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

    // Health and energy: two slim bars side by side across the hotbar's
    // width, just above it; health on the left with its numbers.
    egui::Area::new(Id::unique("energy"))
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -(18.0 + SLOT + 10.0)))
        .interactable(false)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            let (row, _) = ui.allocate_exact_size(vec2(width, 10.0), Sense::hover());
            let painter = ui.painter();
            let half = (width - 10.0) / 2.0;
            let health = Rect::from_min_size(row.min, vec2(half, row.height()));
            let energy = Rect::from_min_size(pos2(row.right() - half, row.top() + 2.0), vec2(half, 6.0));
            for r in [health, energy] {
                painter.rect_filled(r.expand(1.0), CornerRadius::same(3), Color32::from_black_alpha(150));
            }
            let (now, max) = h.health;
            let left = (now as f32 / max.max(1) as f32).clamp(0.0, 1.0);
            if left > 0.0 {
                let fill = Rect::from_min_size(health.min, vec2(health.width() * left, health.height()));
                let colour = if left < 0.25 { HEALTH_LOW } else { HEALTH_FULL };
                painter.rect_filled(fill, CornerRadius::same(3), colour);
            }
            painter.text(
                health.center(),
                Align2::CENTER_CENTER,
                format!("{now} / {max}"),
                FontId::proportional(10.0),
                Color32::from_rgb(255, 240, 232),
            );
            let left = h.energy.clamp(0.0, 1.0);
            let usable = (1.0 - h.hunger).clamp(0.0, 1.0);
            if left > 0.0 {
                let fill = Rect::from_min_size(energy.min, vec2(energy.width() * left, energy.height()));
                let colour = if left < 0.2 * usable { ENERGY_LOW } else { ENERGY_FULL };
                painter.rect_filled(fill, CornerRadius::same(3), colour);
            }
            // Hunger eats the bar from its right end.
            if h.hunger > 0.0 {
                let w = energy.width() * h.hunger.clamp(0.0, 1.0);
                let shade = Rect::from_min_size(pos2(energy.right() - w, energy.top()), vec2(w, energy.height()));
                painter.rect_filled(shade, CornerRadius::same(3), HUNGER);
            }
            // Food tags over the energy bar, right-aligned.
            let mut x = energy.right();
            for (text, colour) in h.food.iter().rev() {
                let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(11.0), *colour);
                let w = galley.size().x;
                let at = pos2(x - w, energy.top() - 16.0);
                painter.rect_filled(
                    Rect::from_min_size(at - vec2(4.0, 1.0), vec2(w + 8.0, galley.size().y + 2.0)),
                    CornerRadius::same(3),
                    Color32::from_black_alpha(140),
                );
                painter.galley(at, galley, *colour);
                x -= w + 14.0;
            }
        });

    // Air (milestone 18): a slim bar over the health bar, with its tag.
    if let Some(air) = &h.air {
        egui::Area::new(Id::unique("air"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -(18.0 + SLOT + 26.0)))
            .interactable(false)
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                let (row, _) = ui.allocate_exact_size(vec2(width, 16.0), Sense::hover());
                let painter = ui.painter();
                let half = (width - 10.0) / 2.0;
                let bar = Rect::from_min_size(pos2(row.left(), row.bottom() - 5.0), vec2(half, 5.0));
                painter.rect_filled(bar.expand(1.0), CornerRadius::same(3), Color32::from_black_alpha(150));
                let left = air.left.clamp(0.0, 1.0);
                if left > 0.0 {
                    let fill = Rect::from_min_size(bar.min, vec2(bar.width() * left, bar.height()));
                    let colour = if air.short && left < 0.3 { AIR_LOW } else { AIR_FULL };
                    painter.rect_filled(fill, CornerRadius::same(3), colour);
                }
                let colour = if air.short { AIR_WARN } else { TEXT };
                let galley = painter.layout_no_wrap(air.text.clone(), FontId::proportional(11.0), colour);
                let at = pos2(bar.left(), bar.top() - galley.size().y - 2.0);
                painter.rect_filled(
                    Rect::from_min_size(at - vec2(4.0, 1.0), galley.size() + vec2(8.0, 2.0)),
                    CornerRadius::same(3),
                    Color32::from_black_alpha(140),
                );
                painter.galley(at, galley, colour);
            });
    }

    // XP: a hairline under the hotbar, the level at its left.
    if let Some((level, into, need)) = h.level {
        egui::Area::new(Id::unique("xp"))
            .anchor(Align2::CENTER_BOTTOM, vec2(-14.0, -4.0))
            .interactable(false)
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(width + 28.0, 13.0), Sense::hover());
                let painter = ui.painter();
                painter.text(
                    pos2(rect.left() + 22.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    level.to_string(),
                    FontId::proportional(12.0),
                    GOLD,
                );
                let bar = Rect::from_min_size(pos2(rect.left() + 28.0, rect.center().y - 1.5), vec2(width, 3.0));
                painter.rect_filled(bar.expand(1.0), CornerRadius::same(2), Color32::from_black_alpha(140));
                let f = (into as f32 / need.max(1) as f32).clamp(0.0, 1.0);
                if f > 0.0 {
                    let fill = Rect::from_min_size(bar.min, vec2(bar.width() * f, bar.height()));
                    painter.rect_filled(fill, CornerRadius::same(2), XP_BAR);
                }
                // A defeat's debt: the XP still owed, in red after the fill.
                if h.debt > 0 {
                    let d = (h.debt as f32 / need.max(1) as f32).clamp(0.0, 1.0 - f).max(0.01);
                    let owed = Rect::from_min_size(
                        pos2(bar.left() + bar.width() * f, bar.top()),
                        vec2(bar.width() * d, bar.height()),
                    );
                    painter.rect_filled(owed, CornerRadius::same(2), DEBT_RED);
                    painter.text(
                        pos2(bar.right() + 6.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        format!("XP debt {}", h.debt),
                        FontId::proportional(11.0),
                        DEBT_RED,
                    );
                }
            });
    }

    if !h.notices.is_empty() {
        egui::Area::new(Id::unique("notices"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -(18.0 + SLOT + 34.0)))
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

    let top_area = |id: &str, y: f32| {
        egui::Area::new(Id::unique(id.to_string()))
            .anchor(Align2::CENTER_TOP, vec2(0.0, y))
            .interactable(false)
            .order(egui::Order::Background)
    };
    // The region at the very top; claims and meteors below it.
    let mut y = 14.0;
    if let Some(region) = h.region.as_ref().filter(|_| h.claim_mode.is_none()) {
        top_area("region", 8.0).show(ctx, |ui| {
            ui.label(
                egui::RichText::new(region)
                    .color(Color32::from_rgb(236, 230, 214))
                    .size(15.0)
                    .strong(),
            );
        });
        y = 34.0;
    }
    if let Some((text, left)) = h.meteor.as_ref().filter(|_| h.claim_mode.is_none()) {
        let at = if h.owner.is_some() { y + 30.0 } else { y };
        top_area("meteor", at).show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_black_alpha(140))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(12, 5))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new(text).color(METEOR_EMBER).size(14.0));
                        if *left > 0.0 {
                            let (rect, _) = ui.allocate_exact_size(vec2(260.0, 6.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 3.0, Color32::from_black_alpha(160));
                            let mut fill = rect;
                            fill.set_width(rect.width() * left.clamp(0.0, 1.0));
                            ui.painter().rect_filled(fill, 3.0, METEOR_EMBER);
                        }
                        if let Some(life) = &h.life {
                            ui.label(egui::RichText::new(life).color(GOLD).size(13.0));
                        }
                    });
                });
        });
    }
    // In claim mode the panel says whose land this is instead.
    if let Some((owner, mine)) = h.owner.as_ref().filter(|_| h.claim_mode.is_none()) {
        top_area("claim-owner", y).show(ctx, |ui| {
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
        });
    }
    if let Some(wear) = h.wear_bar {
        wear_bar(ctx, wear);
    } else if let Some(bar) = &h.block_bar {
        block_bar(ctx, bar);
    }
    if let Some(line) = &h.crop {
        let drop = if h.aim_hint.is_some() { 72.0 } else { 44.0 };
        egui::Area::new(Id::unique("crop-line"))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, drop))
            .interactable(false)
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(130))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(line).color(FOOD_FULL).size(14.0));
                    });
            });
    }
    if let Some((key, what)) = h.aim_hint {
        // Below the health bar when there is one.
        let drop = if h.block_bar.is_some() || h.wear_bar.is_some() {
            62.0
        } else {
            44.0
        };
        egui::Area::new(Id::unique("aim-hint"))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, drop))
            .interactable(false)
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(130))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 7.0;
                            egui::Frame::new()
                                .stroke(Stroke::new(1.5, GOLD))
                                .corner_radius(CornerRadius::same(5))
                                .inner_margin(egui::Margin::symmetric(6, 0))
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new(key).color(GOLD).strong().size(14.0));
                                });
                            ui.label(egui::RichText::new(what).size(15.0));
                        });
                    });
            });
    }
    if h.reclaim_waiting {
        egui::Area::new(Id::unique("reclaim-hint"))
            .anchor(Align2::RIGHT_TOP, vec2(-14.0, 14.0))
            .interactable(false)
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(150))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(10, 3))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(RECLAIM_HINT)
                                .color(TEXT.gamma_multiply(0.85))
                                .size(13.0),
                        );
                    });
            });
    }
    if let Some(help) = &h.claim_mode {
        top_area("claim-help", 14.0).show(ctx, |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("Claim mode").color(GOLD).strong());
                        ui.label(egui::RichText::new(help).size(14.0));
                        if let Some((owner, mine)) = &h.owner {
                            let here = if *mine {
                                "You are standing in your claim.".to_string()
                            } else {
                                format!("You are standing in {owner}'s claim.")
                            };
                            ui.label(egui::RichText::new(here).color(MUTED).size(13.0));
                        }
                    });
                });
        });
    }
}

// ---- the inventory and storage screens ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Slot(SlotRef),
    Discard,
}

/// What the inventory and storage screens draw their slots from: the
/// inventory, and the open chest's slots (empty for the inventory alone).
struct SlotSource<'a> {
    items: &'a Items,
    icons: &'a ItemIcons,
    selected: usize,
    chest: &'a [Option<ItemStack>],
}

impl SlotSource<'_> {
    fn stack(&self, r: SlotRef) -> Option<ItemStack> {
        crate::storage::stack_at(&self.items.inventory.slots, self.chest, r)
    }
}

/// One slot of either grid. Returns whether it was Shift-clicked (with a
/// stack in it); a press that moves is a drag instead.
fn item_slot(
    ui: &mut Ui,
    src: &SlotSource,
    r: SlotRef,
    drag: &mut Option<Drag>,
    targets: &mut Vec<(Rect, Target)>,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SLOT), Sense::click_and_drag());
    targets.push((rect, Target::Slot(r)));
    let hotbar = match r {
        SlotRef::Inventory(i) => (i as usize) < HOTBAR_SLOTS,
        SlotRef::Storage(_) => false,
    };
    let selected = r == SlotRef::Inventory(src.selected as u8);
    let hovering_drop = drag.is_some() && resp.contains_pointer();
    let rim = if hovering_drop || (hotbar && selected) {
        Stroke::new(2.5, GOLD)
    } else if hotbar {
        Stroke::new(1.5, GOLD_DIM)
    } else {
        Stroke::new(1.5, SLOT_RIM)
    };
    let painter = ui.painter();
    paint_slot_back(painter, rect, rim);
    if let SlotRef::Inventory(i) = r
        && hotbar
    {
        painter.text(
            rect.left_top() + vec2(5.0, 3.0),
            Align2::LEFT_TOP,
            ((i + 1) % 10).to_string(),
            FontId::proportional(11.0),
            MUTED,
        );
    }
    let Some(stack) = src.stack(r) else { return false };
    let dragging_this = drag.is_some_and(|d| d.from == r);
    paint_stack(
        painter,
        rect,
        &stack,
        src.items,
        src.icons,
        if dragging_this { 0.35 } else { 1.0 },
    );
    let shift_click = resp.clicked() && ui.input(|i| i.modifiers.shift);
    if resp.drag_started() {
        let half = ui.input(|inp| inp.pointer.button_down(egui::PointerButton::Secondary) || inp.modifiers.shift);
        *drag = Some(Drag { from: r, half });
    }
    if drag.is_none() {
        resp.on_hover_text(src.items.tooltip(&stack));
    }
    shift_click
}

/// The hotbar row and the pack below it. Returns a slot Shift-clicked.
fn inventory_grid(
    ui: &mut Ui,
    src: &SlotSource,
    drag: &mut Option<Drag>,
    targets: &mut Vec<(Rect, Target)>,
) -> Option<SlotRef> {
    let cols = HOTBAR_SLOTS;
    let mut shifted = None;
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
    let mut row = |ui: &mut Ui, first: usize, shifted: &mut Option<SlotRef>| {
        ui.horizontal(|ui| {
            for i in first..first + cols {
                let r = SlotRef::Inventory(i as u8);
                if item_slot(ui, src, r, drag, targets) {
                    *shifted = Some(r);
                }
            }
        });
    };
    row(ui, 0, &mut shifted);
    ui.add_space(10.0);
    ui.label(egui::RichText::new("Pack").color(MUTED).small());
    for r in 1..INVENTORY_SLOTS / cols {
        row(ui, r * cols, &mut shifted);
    }
    ui.spacing_mut().item_spacing = vec2(8.0, 7.0);
    shifted
}

const GRID_WIDTH: f32 = HOTBAR_SLOTS as f32 * SLOT + (HOTBAR_SLOTS - 1) as f32 * GAP;

/// Draws the dragged stack under the pointer and, when it is let go,
/// returns where it was dropped.
fn drag_and_drop(
    ctx: &egui::Context,
    src: &SlotSource,
    drag: &mut Option<Drag>,
    targets: &[(Rect, Target)],
) -> Option<(Drag, Option<Target>)> {
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    if let (Some(d), Some(p)) = (*drag, pointer)
        && let Some(stack) = src.stack(d.from)
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
                paint_stack(ui.painter(), rect, &shown, src.items, src.icons, 1.0);
            });
    }
    let d = (*drag)?;
    if !ctx.input(|i| i.pointer.any_released()) {
        return None;
    }
    *drag = None;
    let target = pointer.and_then(|p| targets.iter().find(|(r, _)| r.contains(p)).map(|(_, t)| *t));
    Some((d, target))
}

pub fn inventory(
    ctx: &egui::Context,
    items: &Items,
    icons: &ItemIcons,
    selected: usize,
    drag: &mut Option<Drag>,
    actions: &mut Vec<UiAction>,
) {
    let src = SlotSource {
        items,
        icons,
        selected,
        chest: &[],
    };
    let mut targets: Vec<(Rect, Target)> = Vec::new();
    let mut open = true;
    window("Inventory").open(&mut open).show(ctx, |ui| {
        inventory_grid(ui, &src, drag, &mut targets);
        ui.add_space(8.0);
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
                    Color32::from_rgba_unmultiplied(70, 24, 22, 170)
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
    let Some((d, target)) = drag_and_drop(ctx, &src, drag, &targets) else {
        return;
    };
    let SlotRef::Inventory(from) = d.from else { return };
    match target {
        Some(Target::Slot(SlotRef::Inventory(to))) => {
            if let Some(msg) = items.move_request(from as usize, to as usize, d.half) {
                actions.push(UiAction::Send(msg));
            }
        }
        Some(Target::Discard) => actions.push(UiAction::Send(ClientMsg::Discard { slot: from })),
        _ => {}
    }
}

/// An open storage block: its name, where it is, and what the server last
/// said is inside (None until the first `Storage` arrives).
pub struct StorageWindow<'a> {
    pub items: &'a Items,
    pub icons: &'a ItemIcons,
    pub selected: usize,
    pub title: String,
    pub pos: BlockPos,
    pub slots: Option<&'a [Option<ItemStack>]>,
}

pub fn storage(ctx: &egui::Context, v: &StorageWindow, drag: &mut Option<Drag>, actions: &mut Vec<UiAction>) {
    let chest = v.slots.unwrap_or(&[]);
    let src = SlotSource {
        items: v.items,
        icons: v.icons,
        selected: v.selected,
        chest,
    };
    let mut targets: Vec<(Rect, Target)> = Vec::new();
    let mut shifted = None;
    let mut open = true;
    let cols = if chest.len() <= 20 { 5 } else { 10 };
    let chest_width = cols as f32 * SLOT + (cols - 1) as f32 * GAP;
    window(&v.title).open(&mut open).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(GRID_WIDTH);
                heading(ui, "Inventory");
                shifted = inventory_grid(ui, &src, drag, &mut targets);
            });
            // A separator here would stretch the window to the screen's height.
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.set_width(chest_width);
                heading(ui, "Inside");
                if v.slots.is_none() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(egui::RichText::new("Opening").color(MUTED));
                    });
                    return;
                }
                ui.label(
                    egui::RichText::new(format!("{} slots", chest.len()))
                        .color(MUTED)
                        .small(),
                );
                ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                for first in (0..chest.len()).step_by(cols) {
                    ui.horizontal(|ui| {
                        for i in first..(first + cols).min(chest.len()) {
                            let r = SlotRef::Storage(i as u8);
                            if item_slot(ui, &src, r, drag, &mut targets) {
                                shifted = Some(r);
                            }
                        }
                    });
                }
            });
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Drag to move a stack. Shift click sends a whole stack across. Right drag or Shift drag moves half.",
            )
            .color(MUTED)
            .small(),
        );
    });
    if !open {
        actions.push(UiAction::CloseMenu);
    }
    let Some(slots) = v.slots else { return };
    let inv = &v.items.inventory.slots;
    let reg = &v.items.registry;
    if let Some(from) = shifted
        && let Some(msg) = crate::storage::shift_request(reg, inv, slots, v.pos, from)
    {
        actions.push(UiAction::Send(msg));
    }
    if let Some((d, Some(Target::Slot(to)))) = drag_and_drop(ctx, &src, drag, &targets)
        && let Some(msg) = crate::storage::move_request(reg, inv, slots, v.pos, d.from, to, d.half)
    {
        actions.push(UiAction::Send(msg));
    }
}

// ---- crafting and stations ----

pub struct CraftingView<'a> {
    pub items: &'a Items,
    pub icons: &'a ItemIcons,
    /// The station looked at: the block that was opened, its kind and
    /// display name.
    pub station: Option<(BlockPos, Station, String)>,
    /// The server's last word on it: for a machine of several blocks, its
    /// `pos` is the machine's own and may not be the block opened.
    pub live: Option<&'a LiveStation>,
    pub now: f64,
    /// The character's skills: recipes with a crafting gate need them
    /// (milestone 12).
    pub progress: &'a crate::skills::Progress,
}

pub fn station_name(blocks: &BlockRegistry, station: Station) -> String {
    blocks
        .iter()
        .find(|(_, d)| d.station == Some(station))
        .map(|(_, d)| bm_core::items::title_case(&d.name))
        .unwrap_or_else(|| format!("{station:?}"))
}

/// The crafting station a block opens: its own, or for a furnace's
/// crucible the furnace it stands on (milestone 11).
pub fn station_of(def: &BlockDef) -> Option<Station> {
    def.station
        .or((def.assembly == Some(Assembly::FurnaceCrucible)).then_some(Station::Furnace))
}

/// Whether a `Station` answer belongs to the machine opened at `opened`:
/// the same block, or the first block of its group (a four-block machine
/// spans at most three steps; a furnace's base is one below its crucible).
/// Measured with the planet's wrap, so a machine across the seam matches.
pub fn same_machine(shape: PlanetShape, opened: BlockPos, view: BlockPos) -> bool {
    let centre = |p: BlockPos| glam::DVec3::new(p.x as f64, p.y as f64, p.z as f64);
    let d = shape.delta(centre(opened), centre(view));
    d.x.abs() + d.y.abs() + d.z.abs() <= 3.5
}

/// What a fuel drag carries: a pack slot's fuel, or the furnace's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FuelDrag {
    Pack(u8),
    Furnace,
}

pub fn crafting(ctx: &egui::Context, v: &CraftingView, actions: &mut Vec<UiAction>) {
    let title = match &v.station {
        Some((_, _, name)) => name.clone(),
        None => "Crafting".to_string(),
    };
    let kind = v.station.as_ref().map(|s| s.1).unwrap_or(Station::Hand);
    let idle = v.live.and_then(|l| l.view.idle.as_deref());
    let mut open = true;
    window(&title).open(&mut open).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(470.0);
                let hint = match kind {
                    Station::Hand => "Made in your hands, at once. A crafting table makes more.",
                    Station::CraftingTable => {
                        "Hand recipes are made at once. The rest queue here and keep working while you are away."
                    }
                    _ => "Queued here; it keeps working while you are away.",
                };
                ui.label(egui::RichText::new(hint).color(MUTED).small());
                if let Some(why) = idle {
                    idle_banner(ui, why);
                }
                egui::ScrollArea::vertical().max_height(430.0).show(ui, |ui| {
                    for (index, recipe) in v.items.recipes_for(kind) {
                        recipe_row(ui, v, index, recipe, idle.is_some(), actions);
                    }
                });
            });
            if let Some((pos, _, _)) = v.station {
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(270.0);
                    station_panel(ui, v, kind, pos, actions);
                });
            }
        });
    });
    if !open {
        actions.push(UiAction::CloseMenu);
    }
    paint_fuel_drag(ctx, v);
}

/// Why the machine does not work, in a red-edged box.
pub(crate) fn idle_banner(ui: &mut Ui, why: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(80, 26, 24, 170))
        .stroke(Stroke::new(1.5, Color32::from_rgb(150, 70, 60)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(why).color(TEXT).strong());
            ui.label(egui::RichText::new("It makes nothing until then.").color(MUTED).small());
        });
    ui.add_space(4.0);
}

fn recipe_row(
    ui: &mut Ui,
    v: &CraftingView,
    index: u16,
    recipe: &bm_core::Recipe,
    idle: bool,
    actions: &mut Vec<UiAction>,
) {
    let max = v.items.craft_max(recipe);
    // Hand recipes are made in the hands, at a table too; the rest queue at
    // the machine, which must be working.
    let in_hand = recipe.station == Station::Hand;
    // A crafting skill it needs and we lack ("Needs Toolmaking 1").
    let gate = crate::skills::recipe_gate(&v.progress.tree, v.progress.effects(), recipe);
    let blocked = (idle && !in_hand) || gate.is_some();
    egui::Frame::new()
        .fill(Color32::from_white_alpha(7))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let out = ItemStack::new(recipe.output, recipe.output_count);
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::hover());
                paint_slot_back(ui.painter(), rect, Stroke::new(1.0, SLOT_RIM));
                paint_stack(
                    ui.painter(),
                    rect,
                    &out,
                    v.items,
                    v.icons,
                    if blocked { 0.45 } else { 1.0 },
                );
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
                        let colour = if blocked { MUTED } else { TEXT };
                        ui.label(egui::RichText::new(title).strong().color(colour));
                        let mut notes = Vec::new();
                        if in_hand && v.station.is_some() {
                            notes.push("in your hands".to_string());
                        } else if recipe.seconds > 0.0 {
                            notes.push(format!("{} s", recipe.seconds));
                        }
                        if recipe.heat > 0 {
                            notes.push(format!("heat {}", recipe.heat));
                        }
                        if recipe.spark > 0 {
                            notes.push(format!("{} spark", grouped(recipe.spark as u64)));
                        }
                        if !notes.is_empty() {
                            ui.label(egui::RichText::new(notes.join(", ")).color(MUTED).small());
                        }
                        if let Some(why) = &gate {
                            ui.label(egui::RichText::new(why).color(SHORT).small());
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for input in &recipe.inputs {
                            let view = v.items.input_view(input);
                            let colour = if view.have >= input.count as u32 { MUTED } else { SHORT };
                            let label = egui::RichText::new(view.label).color(colour).small();
                            match view.hover {
                                // A group: its icon, its name, and what serves.
                                Some(hover) => {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                                        paint_icon(ui.painter(), rect, v.icons.get(&ItemStack::new(view.icon, 1)), 1.0);
                                        resp.on_hover_text(&hover);
                                        ui.label(label).on_hover_text(hover);
                                    });
                                }
                                None => {
                                    item_chip(ui, &ItemStack::new(view.icon, 1), v.items, v.icons, 22.0, label);
                                }
                            }
                        }
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let station = if in_hand { None } else { v.station.as_ref().map(|s| s.0) };
                    let mut craft = |ui: &mut Ui, label: String, count: u16| {
                        let ok = !blocked && count > 0 && max >= count;
                        if ui.add_enabled(ok, egui::Button::new(label)).clicked() {
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

fn station_panel(ui: &mut Ui, v: &CraftingView, kind: Station, opened: BlockPos, actions: &mut Vec<UiAction>) {
    let live = v.live;
    // The machine's own position once the server has said it.
    let pos = live.map(|l| l.view.pos).unwrap_or(opened);
    if matches!(kind, Station::Furnace | Station::SparkGenerator) {
        fuel_panel(ui, v, kind, pos, live.map(|l| &l.view), actions);
        ui.add_space(12.0);
    }
    if let Some(view) = live.map(|l| &l.view) {
        machine_panel(ui, kind, view);
    }
    if kind == Station::SparkGenerator {
        // A generator makes nothing: its window is its fuel and its store.
        return;
    }
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
                    // Run on between updates, which come a few times a
                    // second; a machine that does not work stands still,
                    // as does a furnace waiting for heat.
                    let waiting = l.view.idle.is_some() || job.progress == 0.0 && r.heat as u32 > l.view.heat;
                    let run = if waiting {
                        0.0
                    } else {
                        ((v.now - l.received) / r.seconds.max(0.1) as f64) as f32
                    };
                    let p = (job.progress + run).min(1.0);
                    ui.add(egui::ProgressBar::new(p).desired_height(10.0).fill(GOLD_DIM));
                    if kind == Station::Furnace && waiting && l.view.idle.is_none() {
                        ui.label(
                            egui::RichText::new(format!("Waiting for heat: needs {}", r.heat))
                                .color(SHORT)
                                .small(),
                        );
                    }
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

/// "Wear 8,450 / 10,000": a machine's wear left and the most it holds
/// (milestone 15).
pub fn wear_text((left, max): (u32, u32)) -> String {
    format!("Wear {} / {}", grouped(left as u64), grouped(max as u64))
}

/// A station window's spark line (milestone 15): a generator's store and
/// what its fuel still brings, or the spark a linked machine can draw.
/// None for a machine whose links reach no generator (its idle text says
/// why).
pub fn spark_text(kind: Station, s: &SparkView) -> Option<String> {
    if kind == Station::SparkGenerator {
        let mut text = format!("Spark {} / {}", grouped(s.stored as u64), grouped(s.capacity as u64));
        if s.burning > 0 {
            text.push_str(&format!(" (burning {} more)", grouped(s.burning as u64)));
        }
        Some(text)
    } else {
        s.linked
            .then(|| format!("Spark on the grid {}", grouped(s.stored as u64)))
    }
}

/// A thin bar `frac` full, coloured like a block's health.
pub(crate) fn thin_bar(ui: &mut Ui, frac: f32, fill: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(240.0), 8.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(38, 34, 32));
    let full = Rect::from_min_size(rect.min, vec2(rect.width() * frac.clamp(0.0, 1.0), rect.height()));
    if full.width() > 0.5 {
        painter.rect_filled(full, CornerRadius::same(3), fill);
    }
}

/// The spark ember of the station window's bars.
const SPARK_GOLD: Color32 = Color32::from_rgb(255, 176, 60);

/// A machine's wear and spark (milestone 15), when it has them. A
/// generator's spark shows by its fuel instead.
pub(crate) fn machine_panel(ui: &mut Ui, kind: Station, view: &StationView) {
    let spark = view
        .spark
        .filter(|_| kind != Station::SparkGenerator)
        .and_then(|s| spark_text(kind, &s).map(|t| (t, s)));
    if view.wear.is_none() && spark.is_none() && view.power.is_none() {
        return;
    }
    heading(ui, "Machine");
    if let Some(wear) = view.wear {
        ui.label(egui::RichText::new(wear_text(wear)).color(TEXT).strong());
        let frac = wear.0 as f32 / wear.1.max(1) as f32;
        thin_bar(ui, frac, bar_colour(frac));
        if wear.0 == 0 {
            ui.label(
                egui::RichText::new("Hit it with a spanner to repair it.")
                    .color(SHORT)
                    .small(),
            );
        }
        ui.add_space(4.0);
    }
    if let Some((text, s)) = spark {
        ui.label(egui::RichText::new(text).color(SPARK_GOLD).strong());
        thin_bar(ui, s.stored as f32 / s.capacity.max(1) as f32, SPARK_GOLD);
    }
    if let Some(text) = view.power.and_then(power_text) {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(text).color(POWER_BLUE).strong());
    }
    ui.add_space(12.0);
}

/// The power line of a machine's window (milestone 18): "Power 1,500 from
/// 15 coils (1 worn out)"; None with no coils.
pub fn power_text(p: bm_protocol::PowerView) -> Option<String> {
    if p.coils == 0 {
        return None;
    }
    let coils = if p.coils == 1 {
        "1 coil".to_string()
    } else {
        format!("{} coils", p.coils)
    };
    let mut text = format!("Power {} from {coils}", grouped(p.power as u64));
    if p.worn > 0 {
        text.push_str(&format!(" ({} worn out)", p.worn));
    }
    Some(text)
}

/// The power line's colour.
const POWER_BLUE: Color32 = Color32::from_rgb(130, 200, 255);

/// A furnace's (or spark generator's) fuel slot and banked heat (or stored
/// spark), and the fuel in the pack to feed it with: click a pack stack to
/// put it all in (right click: one), click the fuel slot to take it back
/// out, or drag either way.
fn fuel_panel(
    ui: &mut Ui,
    v: &CraftingView,
    kind: Station,
    pos: BlockPos,
    view: Option<&StationView>,
    actions: &mut Vec<UiAction>,
) {
    heading(ui, "Fuel");
    let fuel = view.and_then(|w| w.fuel);
    ui.horizontal(|ui| {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SLOT), Sense::click_and_drag());
        let incoming = resp
            .dnd_hover_payload::<FuelDrag>()
            .is_some_and(|p| matches!(*p, FuelDrag::Pack(_)));
        let rim = if incoming {
            Stroke::new(2.5, GOLD)
        } else {
            Stroke::new(1.5, GOLD_DIM)
        };
        paint_slot_back(ui.painter(), rect, rim);
        let dragging_out = egui::DragAndDrop::payload::<FuelDrag>(ui.ctx()).is_some_and(|p| *p == FuelDrag::Furnace);
        match fuel {
            Some(stack) => {
                paint_stack(
                    ui.painter(),
                    rect,
                    &stack,
                    v.items,
                    v.icons,
                    if dragging_out { 0.35 } else { 1.0 },
                );
                resp.dnd_set_drag_payload(FuelDrag::Furnace);
                if resp.clicked()
                    && let Some(msg) = v.items.fuel_out(pos, &stack)
                {
                    actions.push(UiAction::Send(msg));
                }
                resp.clone()
                    .on_hover_text(format!("{}\nClick to take it out.", v.items.tooltip(&stack)));
            }
            None => {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Empty",
                    FontId::proportional(11.0),
                    MUTED,
                );
            }
        }
        if let Some(p) = resp.dnd_release_payload::<FuelDrag>()
            && let FuelDrag::Pack(slot) = *p
            && let Some(msg) = v.items.fuel_in(pos, slot, false)
        {
            actions.push(UiAction::Send(msg));
        }
        ui.vertical(|ui| {
            let burns = fuel.map(|s| v.items.fuel(s.item) as u32 * s.count as u32).unwrap_or(0);
            if kind == Station::SparkGenerator {
                let spark = view.and_then(|w| w.spark).unwrap_or_default();
                let text = spark_text(kind, &spark).unwrap_or_default();
                ui.label(egui::RichText::new(text).color(SPARK_GOLD).strong());
                thin_bar(ui, spark.stored as f32 / spark.capacity.max(1) as f32, SPARK_GOLD);
                ui.label(
                    egui::RichText::new(format!("{} spark more in the fuel slot", grouped(burns as u64)))
                        .color(MUTED)
                        .small(),
                );
                ui.label(
                    egui::RichText::new("Fuel burns into spark until the store is full.")
                        .color(MUTED)
                        .small(),
                );
                return;
            }
            let heat = view.map(|w| w.heat).unwrap_or(0);
            ui.label(egui::RichText::new(format!("Heat {heat}")).color(GOLD).strong());
            ui.label(
                egui::RichText::new(format!("{burns} more in the fuel slot"))
                    .color(MUTED)
                    .small(),
            );
            ui.label(
                egui::RichText::new("Fuel burns only when a recipe needs heat.")
                    .color(MUTED)
                    .small(),
            );
        });
    });
    ui.add_space(4.0);
    let slots = v.items.fuel_slots();
    let frame = egui::Frame::new()
        .fill(Color32::from_white_alpha(5))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(6));
    let (_, dropped) = ui.dnd_drop_zone::<FuelDrag, _>(frame, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new("Fuel in your pack").color(MUTED).small());
        if slots.is_empty() {
            ui.label(
                egui::RichText::new("Nothing here burns. Coal, trunks, timber and sticks do.")
                    .color(MUTED)
                    .small(),
            );
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            for slot in slots {
                let Some(stack) = v.items.inventory.get(slot as usize) else {
                    continue;
                };
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SLOT - 10.0), Sense::click_and_drag());
                paint_slot_back(ui.painter(), rect, Stroke::new(1.0, SLOT_RIM));
                let dragging =
                    egui::DragAndDrop::payload::<FuelDrag>(ui.ctx()).is_some_and(|p| *p == FuelDrag::Pack(slot));
                paint_stack(
                    ui.painter(),
                    rect,
                    &stack,
                    v.items,
                    v.icons,
                    if dragging { 0.35 } else { 1.0 },
                );
                resp.dnd_set_drag_payload(FuelDrag::Pack(slot));
                let one = resp.secondary_clicked();
                if (resp.clicked() || one)
                    && let Some(msg) = v.items.fuel_in(pos, slot, one)
                {
                    actions.push(UiAction::Send(msg));
                }
                resp.on_hover_text(format!(
                    "{}\nClick to put it all in, right click for one.",
                    v.items.tooltip(&stack)
                ));
            }
        });
    });
    if let Some(p) = dropped
        && *p == FuelDrag::Furnace
        && let Some(stack) = fuel
        && let Some(msg) = v.items.fuel_out(pos, &stack)
    {
        actions.push(UiAction::Send(msg));
    }
}

/// The fuel being dragged, under the pointer.
fn paint_fuel_drag(ctx: &egui::Context, v: &CraftingView) {
    let Some(p) = egui::DragAndDrop::payload::<FuelDrag>(ctx) else {
        return;
    };
    let stack = match *p {
        FuelDrag::Pack(slot) => v.items.inventory.get(slot as usize),
        FuelDrag::Furnace => v.live.and_then(|l| l.view.fuel),
    };
    let (Some(stack), Some(at)) = (stack, ctx.input(|i| i.pointer.latest_pos())) else {
        return;
    };
    egui::Area::new(Id::unique("fuel drag"))
        .order(egui::Order::Tooltip)
        .fixed_pos(at - Vec2::splat(SLOT / 2.0))
        .interactable(false)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(SLOT), Sense::hover());
            paint_stack(ui.painter(), rect, &stack, v.items, v.icons, 1.0);
        });
}

// ---- portals ----

/// The portal window: the portal E was pressed on (`pos`) and the server's
/// last word on it (None until the first `Portal` arrives).
pub struct PortalWindow<'a> {
    pub items: &'a Items,
    pub icons: &'a ItemIcons,
    pub pos: BlockPos,
    pub view: Option<&'a PortalView>,
}

const OPEN_GREEN: Color32 = Color32::from_rgb(150, 214, 120);
const PORTAL_VIOLET: Color32 = Color32::from_rgb(168, 140, 255);

fn action_button(ui: &mut Ui, label: &str, enabled: bool, why_not: &str) -> bool {
    let resp = ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).strong()).min_size(vec2(0.0, 30.0)),
    );
    if enabled {
        resp.clicked()
    } else {
        resp.on_disabled_hover_text(why_not);
        false
    }
}

pub fn portal(ctx: &egui::Context, v: &PortalWindow, actions: &mut Vec<UiAction>) {
    let mut open = true;
    window("Portal").open(&mut open).show(ctx, |ui| {
        ui.set_width(420.0);
        let Some(view) = v.view else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new("Opening").color(MUTED));
            });
            return;
        };
        let send = |actions: &mut Vec<UiAction>, m: Option<ClientMsg>| {
            if let Some(m) = m {
                actions.push(UiAction::Send(m));
            }
        };
        let b = crate::portal::buttons(v.items, view, v.pos);
        let is_open = view.open;

        // What it is and where it leads.
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(crate::portal::size_text(view.width, view.height))
                    .strong()
                    .size(19.0),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (text, colour) = if is_open {
                    ("Open", OPEN_GREEN)
                } else {
                    ("Dormant", MUTED)
                };
                egui::Frame::new()
                    .stroke(Stroke::new(1.5, colour))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(8, 1))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(text).color(colour).strong());
                    });
            });
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Leads to").color(MUTED));
            match &view.link {
                Some(to) => ui.label(egui::RichText::new(to).color(PORTAL_VIOLET).strong()),
                None => ui.label(egui::RichText::new("Not linked").color(MUTED)),
            };
        });
        if let Some(why) = crate::portal::dormant_because(view) {
            ui.label(egui::RichText::new(why).color(SHORT).small());
        }

        // The tank.
        ui.add_space(8.0);
        heading(ui, "Tank");
        let frac = view.fuel as f32 / view.capacity.max(1) as f32;
        ui.add(
            egui::ProgressBar::new(frac.clamp(0.0, 1.0))
                .desired_height(16.0)
                .fill(Color32::from_rgb(118, 92, 168))
                .text(
                    egui::RichText::new(format!("{} of {} wayshards", view.fuel, view.capacity))
                        .color(TEXT)
                        .small(),
                ),
        );
        ui.label(egui::RichText::new(crate::portal::trip_text(view)).color(MUTED).small());
        ui.horizontal(|ui| {
            let carried = crate::portal::wayshards_carried(v.items);
            let label = if carried > 0 {
                format!("Add fuel ({carried})")
            } else {
                "Add fuel".into()
            };
            let why = if carried == 0 {
                "You carry no wayshards"
            } else {
                "The tank is full"
            };
            if action_button(ui, &label, !b.add_fuel.is_empty(), why) {
                actions.extend(b.add_fuel.iter().cloned().map(UiAction::Send));
            }
            let why = if view.fuel == 0 {
                "The tank is empty"
            } else {
                "No room in your pack"
            };
            if action_button(ui, "Take all", b.take_all.is_some(), why) {
                send(actions, b.take_all.clone());
            }
            if let Some(ClientMsg::Portal {
                action: bm_protocol::PortalAction::TakeFuel { count },
                ..
            }) = b.take_stack
                && action_button(ui, &format!("Take {count}"), true, "")
            {
                send(actions, b.take_stack.clone());
            }
        });

        // Linking.
        ui.add_space(8.0);
        heading(ui, "Link");
        ui.horizontal(|ui| {
            if action_button(ui, "Make token", b.make_token.is_some(), "No room in your pack") {
                send(actions, b.make_token.clone());
            }
            if action_button(ui, "Unlink", b.unlink.is_some(), "It is not linked") {
                send(actions, b.unlink.clone());
            }
        });
        ui.label(
            egui::RichText::new("Take a token to another portal of the same size and link it there.")
                .color(MUTED)
                .small(),
        );
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Link with a token").color(MUTED));
        let tokens = crate::portal::tokens(v.items, view);
        if tokens.is_empty() {
            ui.label(egui::RichText::new("You carry no portal tokens.").color(MUTED).small());
        }
        for t in tokens {
            ui.horizontal(|ui| {
                let stack = v.items.slot(t.slot as usize);
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                if let Some(stack) = stack {
                    paint_icon(ui.painter(), rect, v.icons.get(&stack), 1.0);
                }
                let resp = ui.add_enabled(
                    t.refused.is_none(),
                    egui::Button::new(egui::RichText::new(&t.name)).min_size(vec2(0.0, 28.0)),
                );
                match t.refused {
                    Some(why) => {
                        resp.on_disabled_hover_text(format!(
                            "{why}: links only to a {} portal",
                            crate::portal::size_text(view.width, view.height)
                        ));
                    }
                    None => {
                        if resp
                            .on_hover_text("Uses the token up and links the two portals")
                            .clicked()
                        {
                            actions.push(UiAction::Send(crate::portal::link_request(v.pos, t.slot)));
                        }
                    }
                }
            });
        }
    });
    if !open {
        actions.push(UiAction::CloseMenu);
    }
}

// ---- travel ----

fn planet_colour(c: [f32; 3]) -> Color32 {
    let s = c.map(crate::textures::linear_to_srgb);
    Color32::from_rgb(s[0], s[1], s[2])
}

/// A small planet: a disc in its colour, lit from the upper left, with a
/// thin atmosphere ring. Matches how planets look in the sky.
fn paint_planet(painter: &egui::Painter, centre: egui::Pos2, radius: f32, colour: [f32; 3]) {
    let base = planet_colour(colour);
    let air = planet_colour([colour[0] * 0.45 + 0.4, colour[1] * 0.45 + 0.46, colour[2] * 0.45 + 0.55]);
    painter.circle_filled(centre, radius + 2.5, air.gamma_multiply(0.25));
    painter.circle_filled(centre, radius, base.gamma_multiply(0.35));
    // The lit side: discs stepping towards the light.
    for (k, f) in [(0.86f32, 0.6f32), (0.7, 0.8), (0.5, 1.0)] {
        let shift = vec2(-1.0, -0.8) * radius * (1.0 - k) * 0.9;
        painter.circle_filled(centre + shift, radius * k, base.gamma_multiply(f));
    }
    painter.circle_stroke(centre, radius, Stroke::new(1.0, air.gamma_multiply(0.7)));
}

fn players_there(n: u16) -> String {
    match n {
        0 => "Nobody there".into(),
        1 => "1 player".into(),
        n => format!("{n} players"),
    }
}

pub struct TravelView<'a> {
    pub planets: &'a [PlanetSummary],
    /// The planet we are on.
    pub here: u16,
    /// The shrine the menu was opened at.
    pub shrine: BlockPos,
    pub items: &'a Items,
    pub icons: &'a ItemIcons,
    /// What waits in the reclaim, as the server last said.
    pub reclaim: &'a [ItemStack],
}

/// The warp shrine window's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShrineTab {
    #[default]
    Worlds,
    Reclaim,
}

/// A planet's kind and tier, and its air when that needs protection:
/// "Rustland, tier 4, charged air (protection 1)".
pub fn planet_line(kind: &str, tier: u8, air: bm_core::atmosphere::Air) -> String {
    if air.hazardous() {
        format!("{kind}, tier {tier}, {} (protection {})", air.line.name(), air.hazard)
    } else {
        format!("{kind}, tier {tier}")
    }
}

pub fn travel(ctx: &egui::Context, v: &TravelView, tab: &mut ShrineTab, actions: &mut Vec<UiAction>) {
    let mut open = true;
    window("Warp Shrine").open(&mut open).show(ctx, |ui| {
        ui.set_width(430.0);
        ui.horizontal(|ui| {
            ui.selectable_value(tab, ShrineTab::Worlds, egui::RichText::new("Worlds").strong());
            let waiting: u32 = v.reclaim.iter().map(|s| s.count as u32).sum();
            let label = if waiting > 0 {
                format!("Reclaim ({waiting})")
            } else {
                "Reclaim".into()
            };
            ui.selectable_value(tab, ShrineTab::Reclaim, egui::RichText::new(label).strong());
        });
        ui.add_space(4.0);
        match tab {
            ShrineTab::Worlds => worlds_tab(ui, v, actions),
            ShrineTab::Reclaim => reclaim_tab(ui, v, actions),
        }
    });
    if !open {
        actions.push(UiAction::CloseMenu);
    }
}

fn reclaim_tab(ui: &mut Ui, v: &TravelView, actions: &mut Vec<UiAction>) {
    ui.label(
        egui::RichText::new("What was kept for you when a claim of yours lapsed. Any warp shrine holds it.")
            .color(MUTED)
            .small(),
    );
    ui.add_space(6.0);
    if v.reclaim.is_empty() {
        ui.label(egui::RichText::new("Nothing waits for you.").color(MUTED));
        return;
    }
    let cols = 7;
    egui::ScrollArea::vertical().max_height(330.0).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(GAP, GAP);
        for row in v.reclaim.chunks(cols) {
            ui.horizontal(|ui| {
                for stack in row {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SLOT), Sense::hover());
                    paint_slot_back(ui.painter(), rect, Stroke::new(1.5, SLOT_RIM));
                    paint_stack(ui.painter(), rect, stack, v.items, v.icons, 1.0);
                    resp.on_hover_text(v.items.tooltip(stack));
                }
            });
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                [140.0, 32.0],
                egui::Button::new(egui::RichText::new("Take what fits").strong()).fill(Color32::from_rgb(120, 86, 34)),
            )
            .clicked()
        {
            actions.push(UiAction::Send(ClientMsg::CollectReclaim { shrine: v.shrine }));
        }
        let stacks = v.reclaim.len();
        ui.label(
            egui::RichText::new(if stacks == 1 {
                "1 stack waiting".to_string()
            } else {
                format!("{stacks} stacks waiting")
            })
            .color(MUTED)
            .small(),
        );
    });
}

fn worlds_tab(ui: &mut Ui, v: &TravelView, actions: &mut Vec<UiAction>) {
    let mut list: Vec<&PlanetSummary> = v.planets.iter().collect();
    list.sort_by_key(|p| (p.id != v.here, p.tier, p.id));
    ui.label(
        egui::RichText::new("The shrine opens a way to any world it knows.")
            .color(MUTED)
            .small(),
    );
    ui.add_space(4.0);
    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        for p in list {
            let here = p.id == v.here;
            egui::Frame::new()
                .fill(if here {
                    Color32::from_rgba_unmultiplied(240, 186, 84, 18)
                } else {
                    Color32::from_white_alpha(7)
                })
                .stroke(if here { Stroke::new(1.0, GOLD_DIM) } else { Stroke::NONE })
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
                        paint_planet(ui.painter(), rect.center(), 17.0, p.color);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(egui::RichText::new(&p.name).strong().size(17.0));
                            ui.label(
                                egui::RichText::new(planet_line(&p.kind, p.tier, p.air))
                                    .color(if p.air.hazardous() { AIR_WARN } else { MUTED })
                                    .small(),
                            );
                            ui.label(egui::RichText::new(players_there(p.players)).color(MUTED).small());
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if here {
                                ui.label(egui::RichText::new("You are here").color(GOLD));
                            } else if ui
                                .add_sized(
                                    [86.0, 32.0],
                                    egui::Button::new(egui::RichText::new("Travel").strong())
                                        .fill(Color32::from_rgb(120, 86, 34)),
                                )
                                .clicked()
                            {
                                actions.push(UiAction::Send(ClientMsg::Warp {
                                    shrine: v.shrine,
                                    planet: p.id,
                                }));
                                actions.push(UiAction::CloseMenu);
                            }
                        });
                    });
                });
        }
    });
}

/// Where we are going, for the screen shown on the way.
#[derive(Clone, Debug, PartialEq)]
pub struct Arrival {
    pub name: String,
    pub kind: String,
    pub tier: u8,
    pub color: [f32; 3],
}

/// The screen shown while the ground of a new planet arrives. `alpha` fades
/// it out once we have landed; `time` turns the little planet's light.
pub fn travelling(ctx: &egui::Context, a: &Arrival, alpha: f32, time: f64) {
    let screen = ctx.content_rect();
    egui::Area::new(Id::unique("travelling"))
        .fixed_pos(screen.min)
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter();
            // Opaque on the way (the old planet is gone, the new one not yet
            // here), fading as we land.
            painter.rect_filled(
                screen,
                0,
                Color32::from_rgba_unmultiplied(6, 5, 12, (255.0 * alpha) as u8),
            );
            let centre = screen.center() - vec2(0.0, 60.0);
            // A slow ring of motes around the destination.
            for i in 0..12 {
                let t = time as f32 * 0.6 + i as f32 * std::f32::consts::TAU / 12.0;
                let p = centre + vec2(t.cos(), t.sin() * 0.35) * 92.0;
                let fade = 0.35 + 0.65 * (0.5 + 0.5 * (t * 0.5).sin());
                painter.circle_filled(p, 2.5, GOLD.gamma_multiply(fade * alpha));
            }
            paint_planet_faded(painter, centre, 46.0, a.color, alpha);
            let text = |y: f32, s: &str, size: f32, colour: Color32| {
                painter.text(
                    pos2(screen.center().x, centre.y + y),
                    Align2::CENTER_CENTER,
                    s,
                    FontId::proportional(size),
                    colour.gamma_multiply(alpha),
                );
            };
            text(84.0, "Travelling to", 16.0, MUTED);
            text(118.0, &a.name, 36.0, GOLD);
            text(152.0, &format!("{}, tier {}", a.kind, a.tier), 15.0, MUTED);
        });
}

fn paint_planet_faded(painter: &egui::Painter, centre: egui::Pos2, radius: f32, colour: [f32; 3], alpha: f32) {
    if alpha >= 0.999 {
        paint_planet(painter, centre, radius, colour);
    } else {
        // Fade the whole planet by darkening its colour towards the backdrop.
        paint_planet(painter, centre, radius, colour.map(|c| c * alpha));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_power_line_counts_coils() {
        use bm_protocol::PowerView;
        assert_eq!(power_text(PowerView::default()), None);
        assert_eq!(
            power_text(PowerView {
                power: 1500,
                coils: 16,
                worn: 1
            })
            .as_deref(),
            Some("Power 1,500 from 16 coils (1 worn out)")
        );
        assert_eq!(
            power_text(PowerView {
                power: 100,
                coils: 1,
                worn: 0
            })
            .as_deref(),
            Some("Power 100 from 1 coil")
        );
    }

    #[test]
    fn the_air_bar_shows_only_when_it_matters_and_says_why() {
        use bm_core::atmosphere::{AIR_TICKS, Air, AirLine};
        let mut s = bm_core::PlayerState::default();
        assert_eq!(air_view(&s), None, "ordinary air, full: no bar");
        s.air = Air::of(AirLine::Charged, 5);
        let v = air_view(&s).unwrap();
        assert_eq!((v.left, v.short), (1.0, true));
        assert_eq!(v.text, "Charged air: 3 short of Spark Ward");
        s.air_used = AIR_TICKS;
        assert_eq!(air_view(&s).unwrap().text, "Choking: 3 short");
        s.body.wards = [0, 3, 0];
        let v = air_view(&s).unwrap();
        assert_eq!(
            (v.text.as_str(), v.short, v.left),
            ("Air", false, 0.0),
            "safe, coming back"
        );
        assert_eq!(
            planet_line("Rustland", 4, Air::of(AirLine::Charged, 4)),
            "Rustland, tier 4, charged air (protection 1)"
        );
        assert_eq!(planet_line("Arid", 2, Air::default()), "Arid, tier 2");
    }

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
        assert_eq!(station_name(&b, Station::CraftingTable), "Crafting Table");
        assert_eq!(station_name(&b, Station::Extractor), "Extractor");
    }

    #[test]
    fn a_crucible_opens_its_furnace_and_stones_open_nothing() {
        let b = BlockRegistry::builtin();
        let of = |n: &str| station_of(b.get(b.id(n).unwrap()));
        assert_eq!(of("furnace_crucible"), Some(Station::Furnace));
        assert_eq!(of("furnace"), Some(Station::Furnace));
        assert_eq!(of("crafting_table"), Some(Station::CraftingTable));
        assert_eq!(of("extractor"), Some(Station::Extractor));
        assert_eq!(of("stone"), None);
    }

    #[test]
    fn an_answer_for_any_block_of_the_machine_is_taken() {
        let shape = PlanetShape::new(8, 4);
        let w = shape.width_blocks();
        let p = |x, y, z| BlockPos::new(x, y, z);
        assert!(same_machine(shape, p(10, 5, 10), p(10, 5, 10)));
        // A furnace opened by its crucible answers with its base.
        assert!(same_machine(shape, p(10, 6, 10), p(10, 5, 10)));
        // The far end of a line of four.
        assert!(same_machine(shape, p(13, 5, 10), p(10, 5, 10)));
        assert!(!same_machine(shape, p(15, 5, 10), p(10, 5, 10)));
        // Across the seam, the wrap counts.
        assert!(same_machine(shape, p(0, 5, 10), p(w - 2, 5, 10)));
    }

    #[test]
    fn wear_and_spark_read_in_plain_numbers() {
        assert_eq!(wear_text((8_450, 10_000)), "Wear 8,450 / 10,000");
        assert_eq!(wear_text((0, 10_000)), "Wear 0 / 10,000");
        let generator = SparkView {
            stored: 3_200,
            capacity: 5_000,
            linked: true,
            burning: 60,
        };
        assert_eq!(
            spark_text(Station::SparkGenerator, &generator).as_deref(),
            Some("Spark 3,200 / 5,000 (burning 60 more)")
        );
        let idle = SparkView {
            burning: 0,
            ..generator
        };
        assert_eq!(
            spark_text(Station::SparkGenerator, &idle).as_deref(),
            Some("Spark 3,200 / 5,000")
        );
        assert_eq!(
            spark_text(Station::Mixer, &idle).as_deref(),
            Some("Spark on the grid 3,200")
        );
        let cut_off = SparkView { linked: false, ..idle };
        assert_eq!(spark_text(Station::Mixer, &cut_off), None, "the idle text says why");
    }

    #[test]
    fn spark_machines_are_named_and_opened() {
        let b = BlockRegistry::builtin();
        assert_eq!(station_name(&b, Station::Mixer), "Mixer");
        assert_eq!(station_name(&b, Station::SparkGenerator), "Spark Generator");
        let of = |n: &str| station_of(b.get(b.id(n).unwrap()));
        assert_eq!(of("iron_furnace_crucible"), Some(Station::Furnace));
        assert_eq!(of("iron_furnace"), Some(Station::Furnace));
        assert_eq!(of("compactor"), Some(Station::Compactor));
        assert_eq!(of("spark_link"), None);
    }

    #[test]
    fn players_are_counted_in_words() {
        assert_eq!(players_there(0), "Nobody there");
        assert_eq!(players_there(1), "1 player");
        assert_eq!(players_there(12), "12 players");
    }
}
