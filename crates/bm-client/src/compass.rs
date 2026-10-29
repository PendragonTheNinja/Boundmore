//! The compass strip at the top of the screen (polish, 2026-10-29; Boundless
//! had one): the way we face with N, E, S and W, and marks for the planet's
//! warp shrine, our claims and meteors. North is -Z, as everywhere.

use bm_core::PlanetShape;
use egui::{Align2, Color32, FontId, Id, Stroke, pos2, vec2};
use glam::DVec3;

/// How wide the strip is (points) and how many degrees it spans.
pub const WIDTH: f32 = 420.0;
pub const SPAN: f32 = 180.0;
/// Its height, so what sits under it can move down.
pub const HEIGHT: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Shrine,
    Claim,
    Meteor,
}

/// What the strip shows: the bearing we face and the marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompassView {
    /// Degrees east of north we face.
    pub facing: f32,
    /// Each mark's bearing (degrees east of north) and distance (blocks,
    /// level).
    pub marks: Vec<(Mark, f32, f32)>,
}

/// Degrees east of north of a direction (north is -Z).
pub fn bearing(d: DVec3) -> f32 {
    (d.x.atan2(-d.z).to_degrees() as f32).rem_euclid(360.0)
}

/// `to`'s bearing and level distance from `from` (both canonical; the
/// shortest way round the planet).
pub fn toward(shape: PlanetShape, from: DVec3, to: DVec3) -> (f32, f32) {
    let d = shape.delta(from, to);
    (bearing(d), (d.x * d.x + d.z * d.z).sqrt() as f32)
}

/// Where a bearing falls on the strip, -0.5 (left end) to 0.5 (right end),
/// or None outside it.
pub fn place(facing: f32, at: f32) -> Option<f32> {
    let rel = (at - facing + 540.0).rem_euclid(360.0) - 180.0;
    (rel.abs() <= SPAN / 2.0).then_some(rel / SPAN)
}

const TEXT: Color32 = Color32::from_rgb(236, 230, 214);
const SHRINE: Color32 = Color32::from_rgb(240, 186, 84);
const CLAIM: Color32 = Color32::from_rgb(120, 210, 110);
const METEOR: Color32 = Color32::from_rgb(255, 140, 70);

pub fn draw(ctx: &egui::Context, v: &CompassView, top: f32) {
    egui::Area::new(Id::unique("compass"))
        .anchor(Align2::CENTER_TOP, vec2(0.0, top))
        .interactable(false)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, HEIGHT), egui::Sense::hover());
            let p = ui.painter();
            // A band that fades out at both ends (a smooth gradient).
            let mut band = egui::Mesh::default();
            let stops = [(0.0, 0u8), (0.22, 120), (0.78, 120), (1.0, 0)];
            for (i, &(t, a)) in stops.iter().enumerate() {
                let x = rect.left() + rect.width() * t;
                let c = Color32::from_black_alpha(a);
                band.colored_vertex(pos2(x, rect.top()), c);
                band.colored_vertex(pos2(x, rect.bottom()), c);
                if i > 0 {
                    let k = (i as u32) * 2;
                    band.add_triangle(k - 2, k - 1, k);
                    band.add_triangle(k - 1, k + 1, k);
                }
            }
            p.add(band);
            let x_of = |f: f32| rect.center().x + f * rect.width();
            let fade = |f: f32| (1.0 - (f.abs() * 2.0 - 0.6).max(0.0) / 0.4).clamp(0.0, 1.0);
            // Ticks every 15 degrees, letters at the eight points.
            for step in 0..24 {
                let deg = step as f32 * 15.0;
                let Some(f) = place(v.facing, deg) else { continue };
                let x = x_of(f);
                let a = fade(f);
                let name = match step {
                    0 => Some(("N", true)),
                    6 => Some(("E", true)),
                    12 => Some(("S", true)),
                    18 => Some(("W", true)),
                    3 => Some(("NE", false)),
                    9 => Some(("SE", false)),
                    15 => Some(("SW", false)),
                    21 => Some(("NW", false)),
                    _ => None,
                };
                match name {
                    Some((text, main)) => {
                        let colour = if text == "N" {
                            Color32::from_rgb(255, 120, 100)
                        } else {
                            TEXT
                        };
                        p.text(
                            pos2(x, rect.center().y),
                            Align2::CENTER_CENTER,
                            text,
                            FontId::proportional(if main { 15.0 } else { 11.0 }),
                            colour.gamma_multiply(a),
                        );
                    }
                    None => {
                        p.line_segment(
                            [pos2(x, rect.bottom() - 7.0), pos2(x, rect.bottom() - 3.0)],
                            Stroke::new(1.0, TEXT.gamma_multiply(0.6 * a)),
                        );
                    }
                }
            }
            // The marks, under the letters; off the strip, pinned to its end.
            for &(mark, at, dist) in &v.marks {
                let (f, pinned) = match place(v.facing, at) {
                    Some(f) => (f, false),
                    None => {
                        let rel = (at - v.facing + 540.0).rem_euclid(360.0) - 180.0;
                        (0.5f32.copysign(rel), true)
                    }
                };
                // Pinned marks: only the shrine, so the way home is never lost.
                if pinned && mark != Mark::Shrine {
                    continue;
                }
                let x = x_of(f * 0.97);
                let y = rect.bottom() + 1.0;
                let a = if pinned { 0.6 } else { 1.0 };
                match mark {
                    Mark::Shrine => {
                        let c = SHRINE.gamma_multiply(a);
                        p.add(egui::Shape::convex_polygon(
                            vec![pos2(x, y - 6.0), pos2(x + 5.0, y), pos2(x, y + 6.0), pos2(x - 5.0, y)],
                            c,
                            Stroke::new(1.0, Color32::from_black_alpha(160)),
                        ));
                        if !pinned {
                            p.text(
                                pos2(x, y + 8.0),
                                Align2::CENTER_TOP,
                                distance(dist),
                                FontId::proportional(10.0),
                                c,
                            );
                        }
                    }
                    Mark::Claim => {
                        p.rect_filled(egui::Rect::from_center_size(pos2(x, y), vec2(8.0, 8.0)), 1.0, CLAIM);
                        p.rect_stroke(
                            egui::Rect::from_center_size(pos2(x, y), vec2(8.0, 8.0)),
                            1.0,
                            Stroke::new(1.0, Color32::from_black_alpha(160)),
                            egui::StrokeKind::Outside,
                        );
                    }
                    Mark::Meteor => {
                        p.circle_filled(pos2(x, y), 4.5, METEOR);
                        p.circle_stroke(pos2(x, y), 4.5, Stroke::new(1.0, Color32::from_black_alpha(160)));
                    }
                }
            }
            // The way we face.
            p.line_segment(
                [
                    pos2(rect.center().x, rect.top()),
                    pos2(rect.center().x, rect.top() + 4.0),
                ],
                Stroke::new(2.0, TEXT),
            );
        });
}

/// "45 m" up close, "1.2 km" far.
fn distance(blocks: f32) -> String {
    if blocks < 1000.0 {
        format!("{} m", blocks.round() as i64)
    } else {
        format!("{:.1} km", blocks / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearings_run_clockwise_from_north() {
        assert_eq!(bearing(DVec3::new(0.0, 0.0, -1.0)), 0.0);
        assert_eq!(bearing(DVec3::new(1.0, 0.0, 0.0)), 90.0);
        assert_eq!(bearing(DVec3::new(0.0, 0.0, 1.0)), 180.0);
        assert_eq!(bearing(DVec3::new(-1.0, 0.0, 0.0)), 270.0);
        // Facing north, east is right of centre and west left; due south is
        // off the strip; facing 350, north is just right of centre.
        assert_eq!(place(0.0, 90.0), Some(0.5));
        assert_eq!(place(0.0, 270.0), Some(-0.5));
        assert_eq!(place(0.0, 180.0 + 1.0), None);
        let f = place(350.0, 0.0).unwrap();
        assert!(f > 0.0 && f < 0.1);
        // The shortest way round the planet's seam.
        let shape = PlanetShape::new(8, 4);
        let (b, d) = toward(shape, DVec3::new(250.0, 40.0, 10.0), DVec3::new(5.0, 40.0, 10.0));
        assert_eq!(b, 90.0, "east across the seam");
        assert!((d - 11.0).abs() < 1e-3);
        assert_eq!(distance(45.4), "45 m");
        assert_eq!(distance(1240.0), "1.2 km");
    }
}
