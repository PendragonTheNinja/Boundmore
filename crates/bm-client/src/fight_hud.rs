//! The fighting HUD (milestone 13): the red flash when hurt, creatures'
//! name plates with their health and armour, the numbers that float up,
//! the Draw charge ring, and the defeat screen. Drawn with egui over the
//! game; it acts on nothing itself (the defeat screen's Return is returned
//! to `App`). Player-facing text uses no em dashes.

use crate::menus::{GOLD, MUTED, TEXT, bar_colour, paint_shield};
use crate::render::{projection, view_rotation};
use egui::{Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use glam::Vec3;

/// The red flash at the screen's edges lasts this long after a hit.
pub const FLASH_SECONDS: f64 = 0.5;
/// Hurt creatures show their plate within this many blocks.
pub const PLATE_RANGE: f64 = 24.0;

const ANGRY: Color32 = Color32::from_rgb(240, 96, 80);
const PLATE_W: f32 = 104.0;
const PLATE_H: f32 = 7.0;

/// A creature's name plate.
#[derive(Clone, Debug, PartialEq)]
pub struct Plate {
    /// Where on screen (points), above its head.
    pub at: Pos2,
    /// "Hardy Grazer".
    pub title: String,
    pub angry: bool,
    /// Health left, 0 to 1.
    pub left: f32,
    /// One per 100 armour, as blocks show theirs.
    pub shields: u32,
    pub dead: bool,
}

/// A damage number on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Number {
    pub at: Pos2,
    pub amount: u32,
    pub crit: bool,
    /// Ours, taken (red), rather than dealt.
    pub taken: bool,
    pub alpha: f32,
}

pub struct FightView<'a> {
    /// The hurt flash, 0 (none) to 1 (just hit).
    pub flash: f32,
    pub plates: &'a [Plate],
    pub numbers: &'a [Number],
    /// Draw's charge, 0 to 1, while charging.
    pub charge: Option<f32>,
    /// A lit bomb in hand: how much of its fuse has burned and how hard it
    /// would be thrown now, each 0 to 1.
    pub fuse: Option<(f32, f32)>,
    /// Defeated: by whom (empty if not yet told), the XP debt, and whether
    /// Return was pressed.
    pub defeat: Option<(&'a str, u64, bool)>,
}

/// Where a camera-relative point lands on a screen of `screen` points,
/// for a camera looking along `forward` with vertical field `fov_y`; None
/// behind the camera.
pub fn to_screen(rel: Vec3, forward: Vec3, fov_y: f32, screen: Rect) -> Option<Pos2> {
    let aspect = screen.width() / screen.height().max(1.0);
    let clip = projection(fov_y, aspect) * view_rotation(forward) * rel.extend(1.0);
    if clip.w <= 0.01 {
        return None;
    }
    let (x, y) = (clip.x / clip.w, clip.y / clip.w);
    Some(pos2(
        screen.left() + (x * 0.5 + 0.5) * screen.width(),
        screen.top() + (0.5 - y * 0.5) * screen.height(),
    ))
}

/// Draws the fighting HUD. Returns true when Return was clicked.
pub fn show(ctx: &egui::Context, v: &FightView) -> bool {
    let screen = ctx.content_rect();
    if v.flash > 0.0 {
        flash(ctx, screen, v.flash);
    }
    if !v.plates.is_empty() || !v.numbers.is_empty() || v.charge.is_some() || v.fuse.is_some() {
        egui::Area::new(Id::unique("fight-overlay"))
            .fixed_pos(screen.min)
            .interactable(false)
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                let p = ui.painter();
                for plate in v.plates {
                    paint_plate(p, plate);
                }
                for n in v.numbers {
                    paint_number(p, n);
                }
                if let Some(c) = v.charge {
                    paint_charge(p, screen.center(), c);
                }
                if let Some((burned, power)) = v.fuse {
                    paint_fuse(p, screen.center(), burned, power);
                }
            });
    }
    match v.defeat {
        Some((by, debt, returning)) => defeat(ctx, screen, by, debt, returning),
        None => false,
    }
}

/// Red at the screen's edges, fading in towards the middle.
fn flash(ctx: &egui::Context, screen: Rect, strength: f32) {
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, Id::unique("hurt-flash")));
    let bands = 12;
    let depth = screen.width().min(screen.height()) * 0.16;
    for i in 0..bands {
        let t = i as f32 / bands as f32;
        let a = (strength * (1.0 - t) * (1.0 - t) * 70.0) as u8;
        let c = Color32::from_rgba_unmultiplied(200, 20, 16, a);
        let inset = depth * t;
        let w = depth / bands as f32 + 0.5;
        let r = screen.shrink(inset);
        for edge in [
            Rect::from_min_size(r.min, vec2(r.width(), w)),
            Rect::from_min_size(pos2(r.left(), r.bottom() - w), vec2(r.width(), w)),
            Rect::from_min_size(pos2(r.left(), r.top() + w), vec2(w, r.height() - 2.0 * w)),
            Rect::from_min_size(pos2(r.right() - w, r.top() + w), vec2(w, r.height() - 2.0 * w)),
        ] {
            p.rect_filled(edge, CornerRadius::ZERO, c);
        }
    }
}

fn paint_plate(p: &egui::Painter, plate: &Plate) {
    let colour = if plate.dead {
        MUTED
    } else if plate.angry {
        ANGRY
    } else {
        TEXT
    };
    let name = p.layout_no_wrap(plate.title.clone(), FontId::proportional(14.0), colour);
    let shield_w = 9.0;
    let shields_w = plate.shields as f32 * (shield_w + 2.0);
    let width = PLATE_W.max(name.size().x) + if plate.shields > 0 { shields_w + 4.0 } else { 0.0 };
    let back = Rect::from_center_size(plate.at, vec2(width + 14.0, name.size().y + PLATE_H + 14.0));
    p.rect_filled(back, CornerRadius::same(7), Color32::from_black_alpha(130));
    p.galley(pos2(plate.at.x - name.size().x / 2.0, back.top() + 4.0), name, colour);
    if plate.dead {
        return;
    }
    let bar_top = back.bottom() - PLATE_H - 5.0;
    let bar = Rect::from_min_size(pos2(back.right() - 7.0 - PLATE_W, bar_top), vec2(PLATE_W, PLATE_H));
    for i in 0..plate.shields {
        let cx = back.left() + 7.0 + shield_w / 2.0 + i as f32 * (shield_w + 2.0);
        paint_shield(p, cx, bar_top - 2.0, shield_w);
    }
    p.rect_filled(bar, CornerRadius::same(3), Color32::from_rgb(38, 34, 32));
    let fill = Rect::from_min_size(bar.min, vec2(PLATE_W * plate.left.clamp(0.0, 1.0), PLATE_H));
    if fill.width() > 0.5 {
        p.rect_filled(fill, CornerRadius::same(3), bar_colour(plate.left));
    }
    p.rect_stroke(
        bar,
        CornerRadius::same(3),
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(240, 230, 210, 90)),
        StrokeKind::Outside,
    );
}

fn paint_number(p: &egui::Painter, n: &Number) {
    let a = (n.alpha.clamp(0.0, 1.0) * 255.0) as u8;
    let (size, colour) = match (n.taken, n.crit) {
        (true, _) => (22.0, Color32::from_rgba_unmultiplied(255, 84, 70, a)),
        (false, true) => (26.0, Color32::from_rgba_unmultiplied(255, 214, 72, a)),
        (false, false) => (19.0, Color32::from_rgba_unmultiplied(255, 250, 240, a)),
    };
    let text = if n.taken {
        format!("-{}", n.amount)
    } else {
        n.amount.to_string()
    };
    let font = FontId::proportional(size);
    // A dark edge so it reads on snow and sky alike.
    for d in [vec2(1.5, 1.5), vec2(-1.0, 1.0), vec2(1.0, -1.0)] {
        p.text(
            n.at + d,
            Align2::CENTER_CENTER,
            &text,
            font.clone(),
            Color32::from_black_alpha((a as f32 * 0.7) as u8),
        );
    }
    p.text(n.at, Align2::CENTER_CENTER, text, font, colour);
}

/// A ring around the crosshair filling as Draw charges.
fn paint_charge(p: &egui::Painter, centre: Pos2, charge: f32) {
    let r = 20.0;
    p.circle_stroke(centre, r, Stroke::new(3.0, Color32::from_black_alpha(110)));
    let n = 48;
    let end = (n as f32 * charge.clamp(0.0, 1.0)) as usize;
    let full = charge >= 1.0;
    let colour = if full { GOLD } else { Color32::from_rgb(250, 236, 200) };
    let points: Vec<Pos2> = (0..=end)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
            centre + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    if points.len() > 1 {
        p.add(egui::Shape::line(
            points,
            Stroke::new(if full { 3.5 } else { 2.5 }, colour),
        ));
    }
}

/// Points along an arc of radius `r` about `centre`, from the top clockwise,
/// `from` to `to` of the way round (0 to 1).
fn arc(centre: Pos2, r: f32, from: f32, to: f32) -> Vec<Pos2> {
    let n = 64;
    let (a, b) = ((from * n as f32) as usize, (to.clamp(0.0, 1.0) * n as f32) as usize);
    (a..=b)
        .map(|i| {
            let t = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
            centre + vec2(t.cos(), t.sin()) * r
        })
        .collect()
}

/// The colour of a fuse with `left` of it to burn (1 just lit): straw
/// yellow, through orange, to red as it runs out.
pub fn fuse_colour(left: f32) -> Color32 {
    let l = left.clamp(0.0, 1.0);
    let (a, b, t) = if l > 0.5 {
        ([255.0, 150.0, 40.0], [255.0, 226.0, 120.0], (l - 0.5) * 2.0)
    } else {
        ([230.0, 40.0, 24.0], [255.0, 150.0, 40.0], l * 2.0)
    };
    let mix = |i: usize| (a[i] + (b[i] - a[i]) * t) as u8;
    Color32::from_rgb(mix(0), mix(1), mix(2))
}

/// A lit bomb around the crosshair: an outer fuse burning down from the
/// top, a spark where it burns and quicker flicker as it runs out, and an
/// inner ring filling as the throw gains strength (full at
/// `combat::THROW_FULL_TICKS`).
fn paint_fuse(p: &egui::Painter, centre: Pos2, burned: f32, power: f32) {
    let r = 24.0;
    let left = 1.0 - burned.clamp(0.0, 1.0);
    p.circle_stroke(centre, r, Stroke::new(5.0, Color32::from_black_alpha(120)));
    let colour = fuse_colour(left);
    let cord = arc(centre, r, burned, 1.0);
    if cord.len() > 1 {
        p.add(egui::Shape::line(cord, Stroke::new(3.5, colour)));
    }
    // The spark at the burning end.
    let at = arc(centre, r, burned, burned)[0];
    p.circle_filled(at, 5.5, Color32::from_rgba_unmultiplied(255, 140, 40, 110));
    p.circle_filled(at, 3.0, Color32::from_rgb(255, 244, 200));
    // Strength of the throw, inside.
    let inner = 15.0;
    p.circle_stroke(centre, inner, Stroke::new(2.5, Color32::from_black_alpha(90)));
    let fill = arc(centre, inner, 0.0, power);
    if fill.len() > 1 {
        let c = if power >= 1.0 {
            GOLD
        } else {
            Color32::from_rgb(250, 236, 200)
        };
        p.add(egui::Shape::line(fill, Stroke::new(2.0, c)));
    }
}

/// The text the defeat screen shows.
pub fn defeat_lines(by: &str, debt: u64) -> (String, Option<String>) {
    let title = if by.is_empty() {
        "You were defeated".to_string()
    } else {
        format!("You were defeated by {by}")
    };
    let debt = (debt > 0).then(|| format!("XP debt: {debt}. You earn half XP until it is paid"));
    (title, debt)
}

fn defeat(ctx: &egui::Context, screen: Rect, by: &str, debt: u64, returning: bool) -> bool {
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, Id::unique("defeat-dim")));
    p.rect_filled(
        screen,
        CornerRadius::ZERO,
        Color32::from_rgba_unmultiplied(40, 4, 6, 120),
    );
    let (title, debt) = defeat_lines(by, debt);
    let mut pressed = false;
    egui::Area::new(Id::unique("defeat"))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -40.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(egui::Margin::symmetric(36, 24))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.set_min_width(380.0);
                        ui.label(egui::RichText::new(title).size(26.0).color(ANGRY).strong());
                        ui.add_space(8.0);
                        match &debt {
                            Some(d) => ui.label(egui::RichText::new(d).size(15.0).color(TEXT)),
                            None => ui.label(egui::RichText::new("Nothing you carry is lost").size(15.0).color(MUTED)),
                        };
                        ui.label(
                            egui::RichText::new("You come back on safe ground nearby")
                                .size(13.0)
                                .color(MUTED),
                        );
                        ui.add_space(14.0);
                        let label = if returning { "Returning..." } else { "Return" };
                        let button = egui::Button::new(egui::RichText::new(label).size(18.0).color(GOLD))
                            .min_size(vec2(160.0, 36.0));
                        let r = ui.add_enabled(!returning, button);
                        pressed = r.clicked();
                        // Enter returns too.
                        if !returning && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            pressed = true;
                        }
                        let _ = ui.allocate_exact_size(vec2(1.0, 2.0), Sense::hover());
                    });
                });
        });
    pressed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_ahead_land_on_screen_and_behind_do_not() {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 720.0));
        let fwd = Vec3::new(0.0, 0.0, -1.0);
        let fov = 75f32.to_radians();
        let centre = to_screen(Vec3::new(0.0, 0.0, -10.0), fwd, fov, screen).unwrap();
        assert!((centre - screen.center()).length() < 0.5);
        let up = to_screen(Vec3::new(0.0, 2.0, -10.0), fwd, fov, screen).unwrap();
        assert!(up.y < centre.y, "higher in the world is higher on screen");
        let right = to_screen(Vec3::new(2.0, 0.0, -10.0), fwd, fov, screen).unwrap();
        assert!(right.x > centre.x);
        assert_eq!(to_screen(Vec3::new(0.0, 0.0, 10.0), fwd, fov, screen), None);
    }

    #[test]
    fn the_fuse_reddens_as_it_burns() {
        let fresh = fuse_colour(1.0);
        let late = fuse_colour(0.05);
        assert!(fresh.g() > 200 && late.g() < 60, "{fresh:?} {late:?}");
        let top = arc(Pos2::ZERO, 10.0, 0.0, 0.0)[0];
        assert!((top - pos2(0.0, -10.0)).length() < 1e-4, "from the top: {top:?}");
        assert_eq!(arc(Pos2::ZERO, 10.0, 0.0, 1.0).len(), 65);
    }

    #[test]
    fn the_defeat_screen_says_who_and_what_it_costs() {
        let (t, d) = defeat_lines("Hardy Grazer", 750);
        assert_eq!(t, "You were defeated by Hardy Grazer");
        assert_eq!(d.as_deref(), Some("XP debt: 750. You earn half XP until it is paid"));
        let (t, d) = defeat_lines("", 0);
        assert_eq!(t, "You were defeated");
        assert_eq!(d, None);
        assert!(!t.contains('\u{2014}'));
    }
}
