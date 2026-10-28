//! The atlas window (milestone 27, `bm_core::atlas`): the open atlas's
//! world from above, from the tiles the server sends, fogged where we have
//! not been. The map wraps as the planet does, so it pans forever.

use crate::menus::{GOLD, MUTED, TEXT, UiAction, window};
use bm_core::atlas::{BLOCKS_PER_PIXEL, FOG, TILE_PIXELS, map_pixels};
use bm_core::regions::RegionMap;
use bm_protocol::{AtlasView, ClaimView, SettlementView};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};

/// The map's side on screen, in points.
const CANVAS: f32 = 560.0;
/// Screen points per map pixel at the closer zoom steps (the widest fits
/// the whole planet); the closest shows one point a block.
const STEPS: [f32; 3] = [1.0, 2.0, 4.0];
/// Fog over land not discovered.
const FOG_COLOUR: [u8; 3] = [46, 50, 64];
const MINE: Color32 = Color32::from_rgba_premultiplied(150, 110, 40, 150);
const THEIRS: Color32 = Color32::from_rgba_premultiplied(110, 120, 150, 120);

/// What the window needs from the world around it.
pub struct Here<'a> {
    /// Our planet's id, position (canonical) and yaw, when it is the
    /// atlas's world; claims and settlements are drawn then too.
    pub planet: u16,
    pub pos: [f64; 2],
    pub yaw: f64,
    pub account: &'a str,
    pub claims: Vec<&'a ClaimView>,
    pub settlements: &'a [SettlementView],
}

pub struct AtlasMap {
    pub view: AtlasView,
    regions: RegionMap,
    n: usize,
    /// RGB by `z * n + x`, [`FOG`] until a tile says otherwise.
    pixels: Vec<[u8; 3]>,
    texture: Option<egui::TextureHandle>,
    dirty: bool,
    /// The map pixel at the canvas's centre.
    centre: Vec2,
    /// Screen points per map pixel; 0 until first drawn (fits the planet).
    scale: f32,
    /// Centred on us until panned.
    follow: bool,
}

impl AtlasMap {
    pub fn new(view: AtlasView) -> Self {
        let n = map_pixels(view.shape);
        Self {
            regions: RegionMap::new(view.shape, view.layout_seed),
            n,
            pixels: vec![FOG; n * n],
            texture: None,
            dirty: true,
            centre: vec2(n as f32 / 2.0, n as f32 / 2.0),
            scale: 0.0,
            follow: true,
            view,
        }
    }

    /// A newer view of the same world (a region discovered): keeps the map
    /// and where it looks.
    pub fn update(&mut self, view: AtlasView) {
        self.view = view;
    }

    /// One tile from the server (`ServerMsg::AtlasTile`); ignored if it is
    /// not this world's or does not decode.
    pub fn tile(&mut self, planet: u16, tile: (u16, u16), rgb: &[u8]) {
        if planet != self.view.planet {
            return;
        }
        let Ok(bytes) = lz4_flex::decompress_size_prepended(rgb) else {
            return;
        };
        let n = self.n;
        let (x0, z0) = (tile.0 as usize * TILE_PIXELS, tile.1 as usize * TILE_PIXELS);
        let w = TILE_PIXELS.min(n.saturating_sub(x0));
        let h = TILE_PIXELS.min(n.saturating_sub(z0));
        if bytes.len() != w * h * 3 {
            return;
        }
        for pz in 0..h {
            for px in 0..w {
                let i = (pz * w + px) * 3;
                self.pixels[(z0 + pz) * n + x0 + px] = [bytes[i], bytes[i + 1], bytes[i + 2]];
            }
        }
        self.dirty = true;
    }

    /// How much of the map has arrived.
    #[cfg(test)]
    pub fn known(&self) -> usize {
        self.pixels.iter().filter(|p| **p != FOG).count()
    }

    fn texture(&mut self, ctx: &egui::Context) -> egui::TextureId {
        if self.dirty || self.texture.is_none() {
            let n = self.n;
            let rgba: Vec<Color32> = self
                .pixels
                .iter()
                .enumerate()
                .map(|(i, &p)| {
                    let c = if p == FOG {
                        // A little grain, so fog reads as fog and not as sea.
                        let g = ((i % n) * 7 + (i / n) * 13) % 5;
                        FOG_COLOUR.map(|v| v + g as u8 * 2)
                    } else {
                        p
                    };
                    Color32::from_rgb(c[0], c[1], c[2])
                })
                .collect();
            let image = egui::ColorImage {
                size: [n, n],
                source_size: vec2(n as f32, n as f32),
                pixels: rgba,
            };
            match &mut self.texture {
                Some(t) => t.set(image, egui::TextureOptions::NEAREST),
                None => self.texture = Some(ctx.load_texture("atlas", image, egui::TextureOptions::NEAREST)),
            }
            self.dirty = false;
        }
        self.texture.as_ref().expect("made above").id()
    }

    /// Where map pixel `m` is on the canvas: the copy nearest the centre.
    fn to_screen(&self, rect: Rect, m: Vec2) -> Pos2 {
        let n = self.n as f32;
        let d = |a: f32, b: f32| (a - b + n / 2.0).rem_euclid(n) - n / 2.0;
        rect.center() + vec2(d(m.x, self.centre.x), d(m.y, self.centre.y)) * self.scale
    }

    /// Draws the window. Closing it pushes `UiAction::CloseMenu`.
    pub fn show(&mut self, ctx: &egui::Context, here: &Here, actions: &mut Vec<UiAction>) {
        let n = self.n as f32;
        let fit = CANVAS / n;
        if self.scale == 0.0 {
            self.scale = fit;
        }
        let on_it = here.planet == self.view.planet;
        let me = vec2(
            here.pos[0] as f32 / BLOCKS_PER_PIXEL as f32,
            here.pos[1] as f32 / BLOCKS_PER_PIXEL as f32,
        );
        if self.follow && on_it {
            self.centre = me;
        }
        let tex = self.texture(ctx);
        let mut open = true;
        let title = format!("Atlas: {}", self.view.name);
        window(&title).open(&mut open).show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(vec2(CANVAS, CANVAS), Sense::click_and_drag());
            if response.dragged() {
                self.centre -= response.drag_delta() / self.scale;
                self.centre = vec2(self.centre.x.rem_euclid(n), self.centre.y.rem_euclid(n));
                self.follow = false;
            }
            if response.hovered() {
                let scroll: f32 = ui.input(|i| {
                    i.events
                        .iter()
                        .map(|e| match e {
                            egui::Event::MouseWheel { delta, .. } => delta.y,
                            _ => 0.0,
                        })
                        .sum()
                });
                if scroll != 0.0 {
                    let mut steps: Vec<f32> = std::iter::once(fit)
                        .chain(STEPS.into_iter().filter(|&s| s > fit))
                        .collect();
                    steps.dedup();
                    let at = steps.iter().position(|&s| (s - self.scale).abs() < 1e-3).unwrap_or(0);
                    let to = if scroll > 0.0 {
                        (at + 1).min(steps.len() - 1)
                    } else {
                        at.saturating_sub(1)
                    };
                    self.scale = steps[to];
                }
            }
            let painter = ui.painter_at(rect);
            painter.rect_filled(
                rect,
                0.0,
                Color32::from_rgb(FOG_COLOUR[0], FOG_COLOUR[1], FOG_COLOUR[2]),
            );
            // The map, repeated so it wraps; the painter clips to the canvas.
            let size = n * self.scale;
            let origin = rect.center() - self.centre * self.scale;
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            let copies = (CANVAS / size).ceil() as i32 + 1;
            for kz in -copies..=copies {
                for kx in -copies..=copies {
                    let min = origin + vec2(kx as f32, kz as f32) * size;
                    let r = Rect::from_min_size(min, vec2(size, size));
                    if r.intersects(rect) {
                        painter.image(tex, r, uv, Color32::WHITE);
                    }
                }
            }
            if on_it {
                self.draw_claims(&painter, rect, here);
            }
            // The shrine.
            let s = self.to_screen(
                rect,
                vec2(
                    self.view.shrine.x as f32 / BLOCKS_PER_PIXEL as f32,
                    self.view.shrine.z as f32 / BLOCKS_PER_PIXEL as f32,
                ),
            );
            if rect.contains(s) {
                let d = 5.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![s + vec2(0.0, -d), s + vec2(d, 0.0), s + vec2(0.0, d), s + vec2(-d, 0.0)],
                    Color32::from_rgb(196, 168, 255),
                    Stroke::new(1.0, Color32::BLACK),
                ));
            }
            // Discovered regions' names at their centres.
            for &r in &self.view.discovered {
                let Some(name) = self.view.region_names.get(r as usize) else {
                    continue;
                };
                if r as usize >= self.regions.len() {
                    continue;
                }
                let [x, z] = self.regions.centre(r as usize);
                let at = self.to_screen(
                    rect,
                    vec2(x as f32 / BLOCKS_PER_PIXEL as f32, z as f32 / BLOCKS_PER_PIXEL as f32),
                );
                if rect.expand(60.0).contains(at) {
                    shadowed(&painter, at, name, 15.0, TEXT);
                }
            }
            // Us, an arrow the way we face.
            if on_it {
                let at = self.to_screen(rect, me);
                let f = vec2(here.yaw.sin() as f32, -here.yaw.cos() as f32);
                let side = vec2(-f.y, f.x);
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        at + f * 9.0,
                        at - f * 5.0 + side * 6.0,
                        at - f * 2.0,
                        at - f * 5.0 - side * 6.0,
                    ],
                    GOLD,
                    Stroke::new(1.5, Color32::BLACK),
                ));
            }
            ui.horizontal(|ui| {
                if on_it && ui.button("Centre on me").clicked() {
                    self.follow = true;
                }
                let note = if self.view.discovered.is_empty() {
                    "Discover a region (reach its centre) to see it on the map".to_string()
                } else {
                    format!(
                        "{} of {} regions discovered. Scroll to zoom, drag to look around.",
                        self.view.discovered.len(),
                        self.regions.len()
                    )
                };
                ui.label(egui::RichText::new(note).color(MUTED).small());
            });
        });
        if !open {
            actions.push(UiAction::CloseMenu);
        }
    }

    fn draw_claims(&self, painter: &egui::Painter, rect: Rect, here: &Here) {
        let plot = bm_core::plots::PLOT_SIZE as f32 / BLOCKS_PER_PIXEL as f32;
        for c in &here.claims {
            let fill = if c.owner == here.account { MINE } else { THEIRS };
            for p in &c.plots {
                let at = self.to_screen(rect, vec2(p.x as f32 * plot, p.z as f32 * plot));
                let r = Rect::from_min_size(at, Vec2::splat(plot * self.scale));
                if r.intersects(rect) {
                    painter.rect_filled(r, 0.0, fill);
                }
            }
        }
        for s in here.settlements {
            let stones: Vec<_> = here
                .claims
                .iter()
                .filter(|c| s.claims.contains(&c.id))
                .map(|c| c.stone)
                .collect();
            let Some(first) = stones.first() else { continue };
            let at = self.to_screen(
                rect,
                vec2(
                    first.x as f32 / BLOCKS_PER_PIXEL as f32,
                    first.z as f32 / BLOCKS_PER_PIXEL as f32,
                ),
            );
            if rect.contains(at) {
                let capital = if s.capital { ", the capital" } else { "" };
                shadowed(
                    painter,
                    at + vec2(0.0, -14.0),
                    &format!("{} ({}{capital})", s.name, s.rank),
                    13.0,
                    GOLD,
                );
            }
        }
    }
}

fn shadowed(painter: &egui::Painter, at: Pos2, text: &str, size: f32, colour: Color32) {
    let font = FontId::proportional(size);
    painter.text(
        at + vec2(1.0, 1.0),
        Align2::CENTER_CENTER,
        text,
        font.clone(),
        Color32::BLACK,
    );
    painter.text(at, Align2::CENTER_CENTER, text, font, colour);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> AtlasView {
        AtlasView {
            slot: 1,
            planet: 3,
            name: "Hearthmoor".into(),
            shape: bm_core::PlanetShape::new(20, 12),
            layout_seed: 7,
            region_names: vec!["Wold".into()],
            discovered: vec![0],
            shrine: bm_core::BlockPos::new(0, 0, 0),
            closed: false,
        }
    }

    #[test]
    fn tiles_land_where_they_belong_and_short_tiles_fit() {
        let mut m = AtlasMap::new(view());
        assert_eq!(m.n, 160, "20 chunks, 4 blocks a pixel");
        assert_eq!(m.known(), 0);
        // The last tile in x is 32 pixels wide.
        let px = vec![200u8; 32 * 64 * 3];
        m.tile(3, (2, 0), &lz4_flex::compress_prepend_size(&px));
        assert_eq!(m.known(), 32 * 64);
        assert_eq!(m.pixels[128], [200; 3]);
        assert_eq!(m.pixels[127], FOG);
        // Another world's tile, or a wrong size, changes nothing.
        m.tile(4, (0, 0), &lz4_flex::compress_prepend_size(&px));
        m.tile(3, (0, 0), &lz4_flex::compress_prepend_size(&px));
        assert_eq!(m.known(), 32 * 64);
    }

    #[test]
    fn the_map_wraps_to_the_nearest_copy() {
        let mut m = AtlasMap::new(view());
        m.scale = 2.0;
        m.centre = vec2(2.0, 80.0);
        let rect = Rect::from_center_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
        // Pixel 158 is 4 pixels west of 2 across the seam, not 156 east.
        assert_eq!(m.to_screen(rect, vec2(158.0, 80.0)), pos2(-8.0, 0.0));
    }
}
