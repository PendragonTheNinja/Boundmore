//! Sky colours through the day. Linear RGB (the swap chain is sRGB).
//!
//! Three moods are blended by the sun's height: day (deep blue zenith, pale
//! horizon), the golden hour (warm horizon, violet zenith) and night (deep
//! blue-black, stars). The art direction asks for colour shifts rather than
//! plain darkening, so dusk turns warm, then violet, before it turns dark.
//! The daytime colours are the planet's own (`PlanetLook`); dusk and night
//! are shared by every planet.
//!
//! The stars and the other planets of the universe are fixed to the night
//! sky, which turns once a day about the same axis the sun travels around,
//! so they rise and set with it.

use bm_core::{DayCycle, PlanetLook};
use bm_protocol::PlanetSummary;
use glam::{Mat3, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    pub sun_dir: [f32; 3],
    /// 0 at night to 1 by day: how much direct sunlight there is.
    pub daylight: f32,
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    pub sun_color: [f32; 3],
    /// Multiplier for sky light on terrain (`DayCycle::sky_light`).
    pub sky_light: f32,
    pub stars: f32,
    /// Turns the night sky's frame into the world's: a direction fixed to
    /// the stars appears at `sky_rotation * d`.
    pub sky_rotation: Mat3,
}

/// A planet seen in the sky, in world directions for this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyPlanet {
    /// Unit vector towards its centre.
    pub dir: [f32; 3],
    /// Apparent radius, radians.
    pub radius: f32,
    /// Linear RGB.
    pub color: [f32; 3],
}

/// At most this many planets are drawn (the shader's array size).
pub const MAX_SKY_PLANETS: usize = 16;
const DUSK_ZENITH: [f32; 3] = [0.14, 0.12, 0.38];
const DUSK_HORIZON: [f32; 3] = [0.98, 0.42, 0.16];
const NIGHT_ZENITH: [f32; 3] = [0.003, 0.005, 0.022];
const NIGHT_HORIZON: [f32; 3] = [0.016, 0.026, 0.07];
const DAY_SUN: [f32; 3] = [1.0, 0.92, 0.78];
const DUSK_SUN: [f32; 3] = [1.0, 0.45, 0.15];

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Sky {
    /// The sky at a (possibly fractional) server tick, with this planet's
    /// daytime colours.
    pub fn at(day: &DayCycle, look: &PlanetLook, tick: f64) -> Self {
        let d = day.sun_direction(tick);
        let sun_dir = [d[0] as f32, d[1] as f32, d[2] as f32];
        let h = sun_dir[1];
        let daylight = smoothstep(-0.05, 0.25, h);
        // Golden hour peaks with the sun on the horizon and is gone by the
        // time it is a quarter of the way up, or well below.
        let dusk = (1.0 - (h / 0.28).abs()).clamp(0.0, 1.0);
        let night = 1.0 - smoothstep(-0.28, -0.02, h);

        let mut zenith = mix(look.day_zenith, DUSK_ZENITH, dusk * 0.8);
        let mut horizon = mix(look.day_horizon, DUSK_HORIZON, dusk);
        zenith = mix(zenith, NIGHT_ZENITH, night);
        horizon = mix(horizon, NIGHT_HORIZON, night);
        let sun_color = mix(DAY_SUN, DUSK_SUN, dusk);
        let glow = smoothstep(-0.12, 0.05, h);
        Self {
            sun_dir,
            daylight,
            zenith,
            horizon,
            sun_color: [sun_color[0] * glow, sun_color[1] * glow, sun_color[2] * glow],
            sky_light: day.sky_light(tick),
            stars: night,
            sky_rotation: sky_rotation(day, tick),
        }
    }

    /// The other planets of the universe as this sky shows them: every
    /// planet but `here`, at its fixed place among the stars.
    pub fn planets(&self, universe: &[PlanetSummary], here: u16) -> Vec<SkyPlanet> {
        universe
            .iter()
            .filter(|p| p.id != here)
            .take(MAX_SKY_PLANETS)
            .map(|p| SkyPlanet {
                dir: (self.sky_rotation * star_direction(p.id)).to_array(),
                radius: apparent_radius(p.width_blocks),
                color: p.color,
            })
            .collect()
    }
}

/// The axis the sun travels around (`DayCycle::sun_direction`: its path
/// spans +X and (0, 0.94, 0.34)), so the night sky turns with it.
fn sky_axis() -> Vec3 {
    Vec3::X.cross(Vec3::new(0.0, 0.94, 0.34)).normalize()
}

/// The night sky's turn at `tick`: zero at sunrise, a full turn a day.
pub fn sky_rotation(day: &DayCycle, tick: f64) -> Mat3 {
    let angle = (day.time_of_day(tick) - 0.25) * std::f64::consts::TAU;
    Mat3::from_axis_angle(sky_axis(), angle as f32)
}

/// Where a planet sits among the stars: stable for its id, spread around
/// the sky by the golden angle, and kept 18 to 55 degrees off the sun's
/// path so a planet is never lost in the sun's glare for long. Most sit on
/// the side of the sky that stays above the horizon longest.
pub fn star_direction(id: u16) -> Vec3 {
    let h = |salt: u32| {
        let mut x = (id as u32).wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
        x ^= x >> 15;
        x = x.wrapping_mul(0x2C1B_3C6D);
        x ^= x >> 12;
        (x & 0xffff) as f32 / 65535.0
    };
    let around = (id as f32 * 0.618_034 + 0.1 + h(1) * 0.08).fract() * std::f32::consts::TAU;
    let off = (18.0 + 37.0 * h(2)).to_radians();
    // Away from the axis's visible end (it points 20 degrees below the
    // horizon), so three in four tilt towards the end that stays up.
    let off = if id % 4 == 3 { off } else { -off };
    let axis = sky_axis();
    let east = Vec3::X;
    let up = axis.cross(east);
    let on_path = east * around.cos() + up * around.sin();
    (on_path * off.cos() + axis * off.sin()).normalize()
}

/// Apparent radius in radians: a planet the size of a 2048-block one is
/// four degrees across, larger ones grow with the square root of width.
pub fn apparent_radius(width_blocks: u32) -> f32 {
    let r = 2.0 * (width_blocks.max(1) as f32 / 2048.0).sqrt();
    r.clamp(0.8, 6.0).to_radians()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_at(day: &DayCycle, tod: f64) -> f64 {
        tod * day.day_ticks as f64 - day.start_offset as f64
    }

    fn look() -> PlanetLook {
        PlanetLook::default()
    }

    fn summary(id: u16, width: u32) -> PlanetSummary {
        PlanetSummary {
            id,
            name: format!("P{id}"),
            kind: "Temperate".into(),
            tier: 1,
            color: [0.5, 0.4, 0.3],
            width_blocks: width,
            players: 0,
        }
    }

    #[test]
    fn noon_is_blue_and_starless() {
        let d = DayCycle::default();
        let s = Sky::at(&d, &look(), tick_at(&d, 0.5));
        assert_eq!(s.daylight, 1.0);
        assert_eq!(s.stars, 0.0);
        assert!(s.zenith[2] > s.zenith[0] * 3.0, "blue sky");
    }

    #[test]
    fn sunset_is_warm_at_the_horizon() {
        let d = DayCycle::default();
        let s = Sky::at(&d, &look(), tick_at(&d, 0.75));
        assert!(s.horizon[0] > s.horizon[2], "warm horizon {:?}", s.horizon);
    }

    #[test]
    fn midnight_is_dark_with_stars() {
        let d = DayCycle::default();
        let s = Sky::at(&d, &look(), tick_at(&d, 0.0));
        assert_eq!(s.stars, 1.0);
        assert!(s.zenith.iter().all(|c| *c < 0.05));
        assert_eq!(s.daylight, 0.0);
    }

    #[test]
    fn the_sky_changes_smoothly() {
        let d = DayCycle::default();
        let mut prev = Sky::at(&d, &look(), 0.0);
        for t in 1..2000 {
            let s = Sky::at(&d, &look(), t as f64 * d.day_ticks as f64 / 2000.0);
            for i in 0..3 {
                assert!((s.horizon[i] - prev.horizon[i]).abs() < 0.05, "jump at step {t}");
            }
            prev = s;
        }
    }

    #[test]
    fn the_day_sky_is_the_planets_own() {
        let d = DayCycle::default();
        let arid = PlanetLook {
            day_zenith: [0.2, 0.32, 0.62],
            day_horizon: [0.85, 0.70, 0.50],
            ..Default::default()
        };
        let noon = tick_at(&d, 0.5);
        let a = Sky::at(&d, &arid, noon);
        let t = Sky::at(&d, &look(), noon);
        assert_ne!(a.horizon, t.horizon);
        assert!(a.horizon[0] > a.horizon[2], "a warm horizon {:?}", a.horizon);
        // Night is shared.
        let midnight = tick_at(&d, 0.0);
        let (a, b) = (
            Sky::at(&d, &arid, midnight).zenith,
            Sky::at(&d, &look(), midnight).zenith,
        );
        assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-5), "{a:?} {b:?}");
    }

    #[test]
    fn the_night_sky_turns_with_the_sun() {
        let d = DayCycle::default();
        for tod in [0.0, 0.1, 0.3, 0.5, 0.77, 0.9] {
            let tick = tick_at(&d, tod);
            let sun = d.sun_direction(tick);
            let sun = Vec3::new(sun[0] as f32, sun[1] as f32, sun[2] as f32);
            // The sun is a fixed point of the night sky (up to its small
            // tilt off the turning circle).
            let fixed = sky_rotation(&d, tick) * sky_rotation(&d, tick_at(&d, 0.25)).transpose() * Vec3::X;
            assert!(fixed.dot(sun) > 0.99, "{tod}: {fixed} vs {sun}");
        }
    }

    #[test]
    fn planets_are_placed_stably_apart_and_off_the_suns_path() {
        let dirs: Vec<Vec3> = (0..8).map(star_direction).collect();
        assert_eq!(dirs, (0..8).map(star_direction).collect::<Vec<_>>(), "stable");
        for (i, a) in dirs.iter().enumerate() {
            assert!((a.length() - 1.0).abs() < 1e-4);
            // At least 15 degrees from the sun's circle.
            assert!(
                a.dot(sky_axis()).abs() > 15f32.to_radians().sin(),
                "{i} on the sun's path"
            );
            for b in &dirs[i + 1..] {
                // Never overlapping, even at the largest size.
                assert!(a.angle_between(*b) > 2.0 * apparent_radius(u32::MAX), "{i} overlaps");
            }
        }
    }

    #[test]
    fn the_sky_shows_every_other_planet_sized_by_width() {
        let d = DayCycle::default();
        let sky = Sky::at(&d, &look(), 0.0);
        let universe = [summary(0, 2048), summary(1, 4096), summary(2, 1024)];
        let p = sky.planets(&universe, 0);
        assert_eq!(p.len(), 2, "not our own planet");
        assert!(p[0].radius > p[1].radius, "wider looks bigger");
        assert!((apparent_radius(2048) - 2f32.to_radians()).abs() < 1e-6);
        // The same planet moves with the stars over the night.
        let later = Sky::at(&d, &look(), d.day_ticks as f64 / 4.0).planets(&universe, 0);
        assert!(Vec3::from(p[0].dir).angle_between(Vec3::from(later[0].dir)) > 0.5);
    }
}
