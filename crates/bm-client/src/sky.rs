//! Sky colours through the day. Linear RGB (the swap chain is sRGB).
//!
//! Three moods are blended by the sun's height: day (deep blue zenith, pale
//! horizon), the golden hour (warm horizon, violet zenith) and night (deep
//! blue-black, stars). The art direction asks for colour shifts rather than
//! plain darkening, so dusk turns warm, then violet, before it turns dark.

use bm_core::DayCycle;

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
}

const DAY_ZENITH: [f32; 3] = [0.08, 0.26, 0.78];
const DAY_HORIZON: [f32; 3] = [0.42, 0.62, 0.90];
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
    /// The sky at a (possibly fractional) server tick.
    pub fn at(day: &DayCycle, tick: f64) -> Self {
        let d = day.sun_direction(tick);
        let sun_dir = [d[0] as f32, d[1] as f32, d[2] as f32];
        let h = sun_dir[1];
        let daylight = smoothstep(-0.05, 0.25, h);
        // Golden hour peaks with the sun on the horizon and is gone by the
        // time it is a quarter of the way up, or well below.
        let dusk = (1.0 - (h / 0.28).abs()).clamp(0.0, 1.0);
        let night = 1.0 - smoothstep(-0.28, -0.02, h);

        let mut zenith = mix(DAY_ZENITH, DUSK_ZENITH, dusk * 0.8);
        let mut horizon = mix(DAY_HORIZON, DUSK_HORIZON, dusk);
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_at(day: &DayCycle, tod: f64) -> f64 {
        tod * day.day_ticks as f64 - day.start_offset as f64
    }

    #[test]
    fn noon_is_blue_and_starless() {
        let d = DayCycle::default();
        let s = Sky::at(&d, tick_at(&d, 0.5));
        assert_eq!(s.daylight, 1.0);
        assert_eq!(s.stars, 0.0);
        assert!(s.zenith[2] > s.zenith[0] * 3.0, "blue sky");
    }

    #[test]
    fn sunset_is_warm_at_the_horizon() {
        let d = DayCycle::default();
        let s = Sky::at(&d, tick_at(&d, 0.75));
        assert!(s.horizon[0] > s.horizon[2], "warm horizon {:?}", s.horizon);
    }

    #[test]
    fn midnight_is_dark_with_stars() {
        let d = DayCycle::default();
        let s = Sky::at(&d, tick_at(&d, 0.0));
        assert_eq!(s.stars, 1.0);
        assert!(s.zenith.iter().all(|c| *c < 0.05));
        assert_eq!(s.daylight, 0.0);
    }

    #[test]
    fn the_sky_changes_smoothly() {
        let d = DayCycle::default();
        let mut prev = Sky::at(&d, 0.0);
        for t in 1..2000 {
            let s = Sky::at(&d, t as f64 * d.day_ticks as f64 / 2000.0);
            for i in 0..3 {
                assert!((s.horizon[i] - prev.horizon[i]).abs() < 0.05, "jump at step {t}");
            }
            prev = s;
        }
    }
}
