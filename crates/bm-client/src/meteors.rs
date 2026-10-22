//! Regions and meteorites as the player sees them (milestone 14,
//! `docs/milestones/14-meteorites.md`): the region line at the top of the
//! screen, discovery notices, and each meteor drawn in the world (the fall,
//! the beam over its site, the portals its waves drop through) with its
//! status line and the meteor life. The server decides everything; this
//! keeps what it said and draws it.

use std::collections::{HashMap, HashSet};
use std::f32::consts::TAU;

use bm_core::PlanetShape;
use bm_core::regions::RegionMap;
use bm_protocol::{MeteorPhase, MeteorView};
use glam::{DVec3, Quat, Vec3};

use crate::render::BoxInstance;

/// A meteor's status shows within this distance of its site (blocks).
const STATUS_RANGE: f64 = 96.0;
/// The beam over a site: how tall, and how wide.
const BEAM_HEIGHT: f32 = 140.0;
const BEAM_WIDTH: f32 = 0.3;
/// The beam marks a site from afar; within this distance it is not drawn,
/// so it never stands in the way of the fight.
const BEAM_NEAR: f32 = 24.0;
/// Ticks a second (the server's clock).
const TICK_HZ: f64 = bm_core::movement::TICK_HZ as f64;

#[derive(Default)]
pub struct Sky {
    map: Option<RegionMap>,
    names: Vec<String>,
    discovered: HashSet<u16>,
    meteors: HashMap<u32, MeteorView>,
    life: Option<(u32, Option<u32>)>,
}

impl Sky {
    /// The planet's regions (on arrival).
    pub fn regions(&mut self, shape: PlanetShape, layout_seed: u64, names: Vec<String>, discovered: Vec<u16>) {
        self.map = Some(RegionMap::new(shape, layout_seed));
        self.names = names;
        self.discovered = discovered.into_iter().collect();
        self.meteors.clear();
        self.life = None;
    }

    /// A discovery: the notice to show.
    pub fn discovered(&mut self, region: u16, xp: u64) -> String {
        self.discovered.insert(region);
        let name = self.names.get(region as usize).map_or("a new region", String::as_str);
        if xp > 0 {
            format!("Discovered {name} (+{xp} XP)")
        } else {
            format!("Discovered {name}")
        }
    }

    pub fn meteor(&mut self, v: MeteorView) {
        self.meteors.insert(v.id, v);
    }

    pub fn gone(&mut self, id: u32) {
        self.meteors.remove(&id);
    }

    pub fn life(&mut self, life: Option<(u32, Option<u32>)>) {
        self.life = life;
    }

    /// Where `pos` is, as the HUD says it: "Amber Hollows" or "North of
    /// Amber Hollows".
    pub fn place(&self, pos: DVec3) -> Option<String> {
        let map = self.map.as_ref()?;
        let place = map.place(pos.x, pos.z);
        let name = self.names.get(place.region)?;
        Some(place.describe(name))
    }

    /// Whether the block at `pos` is a meteor's heart that may be looted
    /// now (opened, not yet taken by us).
    pub fn heart_open(&self, shape: PlanetShape, pos: bm_core::BlockPos) -> bool {
        let pos = shape.wrap_block(pos);
        self.meteors.values().any(|m| {
            m.site == pos && !m.looted && matches!(m.phase, MeteorPhase::Cleared { .. } | MeteorPhase::Dormant { .. })
        })
    }

    /// The status line of the nearest meteor within range, and how full its
    /// bar is (the fight left), for the HUD.
    pub fn status(&self, pos: DVec3, shape: PlanetShape, tick: u64) -> Option<(String, f32)> {
        let (m, _) = self
            .meteors
            .values()
            .map(|m| (m, shape.delta(pos, site_centre(m)).length()))
            .filter(|(_, d)| *d <= STATUS_RANGE)
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        let level = m.level;
        Some(match m.phase {
            MeteorPhase::Falling { .. } => (format!("Meteorite, level {level}: falling"), 1.0),
            MeteorPhase::Fighting {
                wave,
                waves,
                left,
                lives,
            } => {
                let creatures = if left == 1 { "creature" } else { "creatures" };
                let holders = match lives {
                    0 => String::new(),
                    1 => ", 1 life".to_string(),
                    n => format!(", {n} lives"),
                };
                let done = (wave.saturating_sub(1)) as f32 / waves.max(1) as f32;
                (
                    format!("Meteorite, level {level}: wave {wave} of {waves}, {left} {creatures} left{holders}"),
                    1.0 - done,
                )
            }
            MeteorPhase::Cleared { until } => {
                let text = if m.looted {
                    format!("Meteorite, level {level}: beaten")
                } else {
                    format!(
                        "Meteorite, level {level}: beaten. Take your share from its heart ({})",
                        minutes_left(until, tick)
                    )
                };
                (text, 0.0)
            }
            MeteorPhase::Dormant { until } => {
                let text = if m.looted {
                    format!("Meteorite, level {level}: dormant")
                } else {
                    format!(
                        "Meteorite, level {level}: dormant. Its heart still gives a little ({})",
                        minutes_left(until, tick)
                    )
                };
                (text, 0.0)
            }
        })
    }

    /// The meteor life marker: "Meteor life", or the seconds left to get
    /// back into the zone.
    pub fn life_text(&self) -> Option<String> {
        let (_, out) = self.life?;
        Some(match out {
            None => "Meteor life".to_string(),
            Some(ticks) => format!("Meteor life: back in {} s", (ticks as f64 / TICK_HZ).ceil() as u32),
        })
    }

    /// Every meteor's boxes, camera-relative to `eye`: the falling rock and
    /// its trail, the beam over the site, the portals while it fights.
    /// `tick` is the server's (with a fraction), `time` seconds for motion.
    pub fn boxes(&self, eye: DVec3, shape: PlanetShape, tick: f64, time: f32) -> Vec<BoxInstance> {
        let mut out = Vec::new();
        for m in self.meteors.values() {
            let site = site_centre(m);
            if let MeteorPhase::Falling { lands_at } = m.phase {
                let fall = bm_core::meteors::FALL_TICKS as f64;
                let t = (1.0 - (lands_at as f64 - tick) / fall).clamp(0.0, 1.0);
                let from = DVec3::from_array(m.from.map(|v| v as f64));
                let from = shape.nearest_lap(site, from);
                // Falling faster as it comes (t squared).
                let at = from + (site - from) * (t * t);
                let rel = (shape.nearest_lap(eye, at) - eye).as_vec3();
                let dir = (site - from).normalize_or_zero().as_vec3();
                falling_boxes(&mut out, rel, dir, time);
            }
            let rel = (shape.nearest_lap(eye, site) - eye).as_vec3();
            let far = Vec3::new(rel.x, 0.0, rel.z).length() > BEAM_NEAR;
            if far && tick < m.beam_until as f64 && !matches!(m.phase, MeteorPhase::Falling { .. }) {
                site_beam(&mut out, rel, m, time);
            }
            if matches!(m.phase, MeteorPhase::Fighting { .. }) {
                for p in m.portals {
                    let p = DVec3::from_array(p.map(|v| v as f64));
                    let rel = (shape.nearest_lap(eye, p) - eye).as_vec3();
                    portal_boxes(&mut out, rel, time);
                }
            }
        }
        out
    }

    /// Light from meteors: a falling rock glows, an open heart glows.
    pub fn lights(&self, shape: PlanetShape, eye: DVec3) -> Vec<(DVec3, u8)> {
        self.meteors
            .values()
            .filter(|m| matches!(m.phase, MeteorPhase::Fighting { .. }))
            .map(|m| (shape.nearest_lap(eye, site_centre(m)) + DVec3::new(0.0, 2.0, 0.0), 12))
            .collect()
    }
}

fn site_centre(m: &MeteorView) -> DVec3 {
    DVec3::new(m.site.x as f64 + 0.5, m.site.y as f64 + 0.5, m.site.z as f64 + 0.5)
}

fn minutes_left(until: u64, tick: u64) -> String {
    let m = (until.saturating_sub(tick) as f64 / TICK_HZ / 60.0).ceil() as u32;
    if m <= 1 {
        "a minute left".into()
    } else {
        format!("{m} minutes left")
    }
}

fn srgb(rgb: u32) -> Vec3 {
    Vec3::from(crate::colour::srgb_to_linear([
        (rgb >> 16) as u8,
        (rgb >> 8) as u8,
        rgb as u8,
    ]))
}

fn glowing(center: Vec3, half: Vec3, rot: Quat, colour: Vec3, glow: f32) -> BoxInstance {
    BoxInstance {
        center: center.to_array(),
        half_size: half.max(Vec3::ZERO).to_array(),
        color: [colour.x, colour.y, colour.z, 1.0],
        rotation: rot.normalize().to_array(),
        light: [1.0, 1.0, 1.0, glow],
    }
}

/// A falling meteor at `rel`, moving along `dir`: a tumbling dark rock in a
/// ball of fire, with a trail of embers behind it.
fn falling_boxes(out: &mut Vec<BoxInstance>, rel: Vec3, dir: Vec3, time: f32) {
    let spin = Quat::from_rotation_y(time * 2.3) * Quat::from_rotation_x(time * 1.7);
    out.push(glowing(rel, Vec3::splat(1.1), spin, srgb(0x2c2244), 0.35));
    out.push(glowing(rel, Vec3::splat(1.5), spin.inverse(), srgb(0xff8a2a), 1.0));
    out.push(glowing(rel, Vec3::splat(1.85), spin, srgb(0xffd070), 0.9));
    for i in 1..14 {
        let back = rel - dir * (i as f32 * 1.6);
        let size = 1.3 * (1.0 - i as f32 / 14.0) + 0.15;
        let flick = 0.8 + 0.2 * (time * 23.0 + i as f32).sin();
        let colour = if i < 5 { srgb(0xffb050) } else { srgb(0xc8481c) };
        out.push(glowing(
            back,
            Vec3::splat(size * flick),
            Quat::from_rotation_z(time * 3.0 + i as f32),
            colour,
            1.0,
        ));
    }
}

/// The beam over a meteor's site, seen from far away: ember while it
/// fights, gold once beaten, pale violet when dormant.
fn site_beam(out: &mut Vec<BoxInstance>, rel: Vec3, m: &MeteorView, time: f32) {
    let colour = match m.phase {
        MeteorPhase::Fighting { .. } => srgb(0xff7a3a),
        MeteorPhase::Cleared { .. } => srgb(0xffd25a),
        _ => srgb(0xb7a6e8),
    };
    let pulse = 0.85 + 0.15 * (time * 2.0).sin();
    let mid = rel + Vec3::new(0.0, BEAM_HEIGHT * 0.5 + 1.5, 0.0);
    out.push(glowing(
        mid,
        Vec3::new(BEAM_WIDTH * pulse, BEAM_HEIGHT * 0.5, BEAM_WIDTH * pulse),
        Quat::from_rotation_y(time * 0.5),
        colour,
        0.85,
    ));
    out.push(glowing(
        mid,
        Vec3::new(BEAM_WIDTH * 0.35, BEAM_HEIGHT * 0.5 + 0.1, BEAM_WIDTH * 0.35),
        Quat::IDENTITY,
        srgb(0xfff4d8),
        1.0,
    ));
}

/// A creature portal in the air: two counter-turning violet rings round a
/// dark core.
fn portal_boxes(out: &mut Vec<BoxInstance>, rel: Vec3, time: f32) {
    out.push(glowing(
        rel,
        Vec3::new(0.9, 0.9, 0.06),
        Quat::from_rotation_x(TAU / 4.0),
        srgb(0x1a0f2a),
        0.2,
    ));
    for k in 0..10 {
        let a = k as f32 / 10.0 * TAU + time * 1.4;
        let p = rel + Vec3::new(a.cos() * 1.6, 0.0, a.sin() * 1.6);
        out.push(glowing(
            p,
            Vec3::splat(0.22),
            Quat::from_rotation_y(a),
            srgb(0xb46cff),
            1.0,
        ));
        let b = -(k as f32 / 10.0 * TAU) - time * 2.1;
        let q = rel + Vec3::new(b.cos() * 1.1, 0.15 * (time * 3.0 + k as f32).sin(), b.sin() * 1.1);
        out.push(glowing(
            q,
            Vec3::splat(0.14),
            Quat::from_rotation_y(b),
            srgb(0xe8c6ff),
            1.0,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::BlockPos;

    fn view(phase: MeteorPhase, looted: bool) -> MeteorView {
        MeteorView {
            id: 1,
            from: [0.0, 400.0, 0.0],
            site: BlockPos::new(100, 140, 100),
            level: 2,
            phase,
            portals: [[100.0, 152.0, 100.0]; 3],
            beam_until: 10_000,
            looted,
        }
    }

    #[test]
    fn the_hud_names_regions_and_meteors_nearby() {
        let shape = PlanetShape::new(64, 8);
        let mut sky = Sky::default();
        let names: Vec<String> = (0..49).map(|i| format!("Region {i}")).collect();
        sky.regions(shape, 5, names, vec![]);
        let map = RegionMap::new(shape, 5);
        let [x, z] = map.centre(7);
        assert_eq!(sky.place(DVec3::new(x, 140.0, z)).unwrap(), "Region 7");
        assert_eq!(sky.place(DVec3::new(x, 140.0, z - 90.0)).unwrap(), "North of Region 7");
        assert_eq!(sky.discovered(7, 150), "Discovered Region 7 (+150 XP)");

        let near = DVec3::new(110.0, 140.0, 100.0);
        assert_eq!(sky.status(near, shape, 0), None);
        sky.meteor(view(
            MeteorPhase::Fighting {
                wave: 2,
                waves: 4,
                left: 1,
                lives: 2,
            },
            false,
        ));
        let (text, bar) = sky.status(near, shape, 0).unwrap();
        assert_eq!(text, "Meteorite, level 2: wave 2 of 4, 1 creature left, 2 lives");
        assert!((bar - 0.75).abs() < 1e-6);
        assert!(
            sky.status(DVec3::new(400.0, 140.0, 100.0), shape, 0).is_none(),
            "far away"
        );
        assert!(
            !sky.heart_open(shape, BlockPos::new(100, 140, 100)),
            "sealed while it fights"
        );
        sky.meteor(view(MeteorPhase::Cleared { until: 3600 * 2 }, false));
        assert!(sky.heart_open(shape, BlockPos::new(100, 140, 100)));
        assert!(sky.status(near, shape, 0).unwrap().0.contains("Take your share"));
        sky.meteor(view(MeteorPhase::Cleared { until: 3600 * 2 }, true));
        assert!(!sky.heart_open(shape, BlockPos::new(100, 140, 100)), "taken");
        sky.life(Some((1, Some(150))));
        assert_eq!(sky.life_text().unwrap(), "Meteor life: back in 3 s");
    }

    #[test]
    fn a_falling_meteor_is_drawn_between_sky_and_site() {
        let shape = PlanetShape::new(64, 8);
        let mut sky = Sky::default();
        sky.meteor(view(MeteorPhase::Falling { lands_at: 360 }, false));
        let eye = DVec3::new(100.5, 141.0, 120.5);
        let start = sky.boxes(eye, shape, 0.0, 0.0);
        let end = sky.boxes(eye, shape, 359.0, 0.0);
        assert!(!start.is_empty());
        assert!(start[0].center[1] > end[0].center[1] + 100.0, "it falls");
    }
}
