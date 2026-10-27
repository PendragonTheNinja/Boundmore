//! Signs on the client (milestone 22 part 4): the written signs the server
//! sent, the sign window (write, or read where one may not), and the text
//! drawn on the signs in the world, in their plane and in perspective.

use crate::world::ClientWorld;
use bm_core::signs::{Align, Ink, SignText, SignView, runs, text_face};
use bm_core::{BlockPos, ChunkPos, ColourPalette};
use egui::epaint::{Mesh, Shape, Tessellator};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Rect, Vec2, pos2};
use glam::{DVec3, Vec3};
use std::collections::HashMap;

/// Signs further than this are not drawn (blocks).
pub const DRAW_DISTANCE: f64 = 32.0;

/// The written signs we hold, by anchor.
#[derive(Default)]
pub struct Signs {
    by_anchor: HashMap<BlockPos, SignView>,
}

impl Signs {
    pub fn set(&mut self, anchor: BlockPos, sign: Option<SignView>) {
        match sign {
            Some(s) => self.by_anchor.insert(anchor, s),
            None => self.by_anchor.remove(&anchor),
        };
    }

    /// Drops the signs anchored in a forgotten chunk.
    pub fn forget_chunk(&mut self, c: ChunkPos) {
        self.by_anchor.retain(|a, _| a.chunk() != c);
    }

    pub fn clear(&mut self) {
        self.by_anchor.clear();
    }

    pub fn get(&self, anchor: BlockPos) -> Option<&SignView> {
        self.by_anchor.get(&anchor)
    }

    pub fn is_empty(&self) -> bool {
        self.by_anchor.is_empty()
    }

    /// The anchor of the written sign covering `pos`, if any.
    pub fn sign_at(&self, world: &ClientWorld, pos: BlockPos) -> Option<BlockPos> {
        if self.by_anchor.contains_key(&pos) {
            return Some(pos);
        }
        let shape = world.shape;
        self.by_anchor.iter().find_map(|(a, v)| {
            if v.across <= 1 && v.up <= 1 {
                return None;
            }
            let (rx, _, rz) = bm_core::signs::panel_right(world.state(*a)?.shape)?;
            (0..v.up as i64).find_map(|j| {
                (0..v.across as i64).find_map(|i| {
                    let b = shape.wrap_block(a.offset(rx * i, j, rz * i));
                    (b == pos).then_some(*a)
                })
            })
        })
    }
}

/// What the sign window is editing.
pub struct SignUi {
    pub text: SignText,
    /// Whether this player may write here (else the window only reads).
    pub writable: bool,
}

/// What the sign window asks for.
pub enum SignAction {
    Write(SignText),
    Close,
}

/// The sign window: a writer where one may write, a reader elsewhere.
pub fn window(ctx: &egui::Context, ui_state: &mut SignUi) -> Option<SignAction> {
    let mut out = None;
    let mut open = true;
    let title = if ui_state.writable { "Write a sign" } else { "A sign" };
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            let t = &mut ui_state.text;
            if ui_state.writable {
                ui.label("On the sign (colours: :#f13: or :#ff1133:, or :#nn: a palette colour)");
                ui.add(
                    egui::TextEdit::multiline(&mut t.text)
                        .desired_rows(4)
                        .desired_width(360.0)
                        .char_limit(bm_core::signs::DISPLAY_CHARS),
                );
                ui.label(format!(
                    "{} of {} characters",
                    t.text.chars().count(),
                    bm_core::signs::DISPLAY_CHARS
                ));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut t.align, Align::Left, "Left");
                    ui.selectable_value(&mut t.align, Align::Centre, "Centre");
                    ui.selectable_value(&mut t.align, Align::Right, "Right");
                    ui.checkbox(&mut t.italic, "Italic");
                });
                ui.separator();
                ui.label("More, for anyone who opens it");
                ui.add(
                    egui::TextEdit::multiline(&mut t.more)
                        .desired_rows(5)
                        .desired_width(360.0)
                        .char_limit(bm_core::signs::MORE_CHARS),
                );
                ui.horizontal(|ui| {
                    if ui.button("Write").clicked() {
                        out = Some(SignAction::Write(t.clone()));
                    }
                    if ui.button("Wipe").clicked() {
                        out = Some(SignAction::Write(SignText::default()));
                    }
                });
            } else {
                let plain: String = runs(&t.text).into_iter().map(|(_, s)| s).collect();
                ui.label(egui::RichText::new(plain).size(18.0));
                if !t.more.trim().is_empty() {
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        ui.label(&t.more);
                    });
                }
            }
        });
    if !open {
        out = Some(SignAction::Close);
    }
    out
}

/// What drawing the signs needs from the frame.
pub struct DrawView<'a> {
    pub world: &'a ClientWorld,
    pub colours: &'a ColourPalette,
    pub eye: DVec3,
    pub forward: Vec3,
    pub fov_y: f32,
    pub screen: Rect,
    /// How bright the sky is now (0 night to 1 day).
    pub sky_light: f32,
}

/// Draws every written sign in view onto `painter`.
pub fn draw(painter: &egui::Painter, signs: &Signs, v: &DrawView) {
    let shape = v.world.shape;
    let ctx = painter.ctx();
    for (anchor, view) in &signs.by_anchor {
        let Some(state) = v.world.state(*anchor) else { continue };
        let Some(face) = text_face(state.shape, view.across, view.up) else {
            continue;
        };
        let base = DVec3::new(anchor.x as f64, anchor.y as f64, anchor.z as f64);
        let base = shape.nearest_lap(v.eye, base);
        let corner = |u: f64, w: f64| base + DVec3::from_array(face.at(u, w));
        let centre = corner(face.width / 2.0, face.height / 2.0);
        let to_eye = v.eye - centre;
        if to_eye.length() > DRAW_DISTANCE || to_eye.dot(DVec3::from_array(face.normal)) <= 0.0 {
            continue;
        }
        if hidden(v.world, v.eye, centre) {
            continue;
        }
        let project = |p: DVec3| crate::fight_hud::to_screen((p - v.eye).as_vec3(), v.forward, v.fov_y, v.screen);
        // How many screen points a block of the sign spans, to lay the text
        // out at about its drawn size (rounded to a power of two, so the font
        // atlas sees few sizes).
        let (Some(a), Some(b)) = (project(centre), project(centre + DVec3::from_array(face.right))) else {
            continue;
        };
        let per_block = ((a - b).length()).clamp(8.0, 512.0);
        let scale = 2f32.powi(per_block.log2().round() as i32);
        let def = v.world.registry.get(state.id);
        let colour = ink_colour(v, state, def, base, face.normal);
        let (w, h) = ((face.width as f32) * scale, (face.height as f32) * scale);
        let Some(galley) = fit(ctx, &view.text, colour, v.colours, w, h) else {
            continue;
        };
        let x = match view.text.align {
            Align::Left => 0.0,
            Align::Centre => w / 2.0,
            Align::Right => w,
        };
        let y = ((h - galley.size().y) / 2.0).max(0.0);
        let mut mesh = Mesh::default();
        let mut tess = ctx.fonts(|f| {
            Tessellator::new(
                ctx.pixels_per_point(),
                Default::default(),
                f.font_image_size(),
                Vec::new(),
            )
        });
        tess.tessellate_shape(Shape::galley(pos2(x, y), galley, colour), &mut mesh);
        // Canvas points to the sign's plane, then to the screen.
        let mut ok = true;
        for vert in &mut mesh.vertices {
            let (u, w2) = (vert.pos.x / scale, face.height as f32 - vert.pos.y / scale);
            match project(corner(u as f64, w2 as f64)) {
                Some(p) => vert.pos = p,
                None => ok = false,
            }
        }
        if ok {
            painter.add(Shape::mesh(mesh));
        }
    }
}

/// The text laid out to fit `w` by `h` canvas points: the largest size
/// that fits, wrapped to the width.
fn fit(
    ctx: &egui::Context,
    text: &SignText,
    colour: Color32,
    palette: &ColourPalette,
    w: f32,
    h: f32,
) -> Option<std::sync::Arc<egui::Galley>> {
    if text.text.trim().is_empty() || w < 4.0 || h < 4.0 {
        return None;
    }
    let mut size = (h * 0.9).min(w * 0.5);
    loop {
        let mut job = LayoutJob::default();
        job.wrap.max_width = w;
        job.halign = match text.align {
            Align::Left => egui::Align::LEFT,
            Align::Centre => egui::Align::Center,
            Align::Right => egui::Align::RIGHT,
        };
        for (ink, run) in runs(&text.text) {
            let c = ink.map_or(colour, |i| ink_rgb(i, palette, colour));
            job.append(
                &run,
                0.0,
                TextFormat {
                    font_id: FontId::proportional(size),
                    color: c,
                    italics: text.italic,
                    ..Default::default()
                },
            );
        }
        let galley = ctx.fonts_mut(|f| f.layout_job(job));
        let fits = galley.size().y <= h && galley.size().x <= w + 0.5;
        if fits || size <= 3.0 {
            return Some(galley);
        }
        size *= 0.85;
    }
}

fn ink_rgb(ink: Ink, palette: &ColourPalette, fallback: Color32) -> Color32 {
    match ink {
        Ink::Rgb([r, g, b]) => Color32::from_rgb(r, g, b),
        Ink::Palette(id) => palette
            .get(bm_core::Colour(id))
            .map_or(fallback, |c| Color32::from_rgb(c.rgb[0], c.rgb[1], c.rgb[2])),
    }
}

/// The sign's own ink: light on dark signs and glowing ones, dark on light
/// ones; dimmed as the sign is lit unless it glows.
fn ink_colour(
    v: &DrawView,
    state: bm_core::BlockState,
    def: &bm_core::BlockDef,
    base: DVec3,
    normal: [f64; 3],
) -> Color32 {
    let tint = v
        .colours
        .get(state.colour)
        .filter(|_| def.colourable)
        .map_or(def.color, |c| c.rgb);
    let luma = (0.299 * tint[0] as f32 + 0.587 * tint[1] as f32 + 0.114 * tint[2] as f32) / 255.0;
    if def.glow_text {
        return Color32::from_rgb(255, 244, 214);
    }
    let ink = if luma < 0.45 { 236 } else { 28 };
    let front = base + DVec3::new(0.5, 0.5, 0.5) + DVec3::from_array(normal) * 0.6;
    let l = v.world.light_at(front);
    let lit = (l.sky() as f32 / 15.0 * v.sky_light).max(l.block() as f32 / 15.0);
    let k = 0.25 + 0.75 * lit.clamp(0.0, 1.0);
    let c = (ink as f32 * k) as u8;
    Color32::from_rgb(c, c, (c as f32 * 0.97) as u8)
}

/// Whether something solid and whole stands between the eye and a point.
fn hidden(world: &ClientWorld, eye: DVec3, at: DVec3) -> bool {
    let d = at - eye;
    let len = d.length();
    let steps = (len / 0.25) as usize;
    (1..steps.saturating_sub(2)).any(|k| {
        let p = eye + d * (k as f64 * 0.25 / len);
        let b = BlockPos::containing(p.x, p.y, p.z);
        world
            .state(b)
            .is_some_and(|s| world.flags.opaque(s.id) && s.shape.is_full())
    })
}
