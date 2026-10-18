//! The planet clock. The server counts ticks and saves the count with the
//! planet, so days continue across restarts; everything about time of day is
//! derived from that one number, identically on server and client.

use crate::movement::TICK_HZ;
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

/// How long a day lasts and how bright night is. Sent to clients in the
/// handshake so the server alone decides.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DayCycle {
    /// Ticks in one full day.
    pub day_ticks: u64,
    /// Tick offset so a fresh planet starts in the morning.
    pub start_offset: u64,
}

impl Default for DayCycle {
    fn default() -> Self {
        let day_ticks = 20 * 60 * TICK_HZ as u64; // 20 minutes
        Self {
            day_ticks,
            start_offset: day_ticks * 3 / 10, // shortly after sunrise
        }
    }
}

/// Brightness of sky light at the darkest point of night, as a fraction of day.
pub const NIGHT_SKY_LIGHT: f32 = 0.16;

impl DayCycle {
    /// Fraction of the day, 0 to 1: 0 is midnight, 0.25 sunrise, 0.5 noon,
    /// 0.75 sunset. `tick` may be fractional (the client interpolates).
    pub fn time_of_day(&self, tick: f64) -> f64 {
        ((tick + self.start_offset as f64) / self.day_ticks as f64).rem_euclid(1.0)
    }

    /// Unit vector towards the sun. It rises in the east (+X), passes high
    /// over the planet at noon, tilted towards +Z so it never sits exactly
    /// overhead, and sets in the west.
    pub fn sun_direction(&self, tick: f64) -> [f64; 3] {
        let a = (self.time_of_day(tick) - 0.25) * TAU;
        let (s, c) = (libm::sin(a), libm::cos(a));
        let v = [c, s * 0.94, s * 0.34 + 0.08];
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / len, v[1] / len, v[2] / len]
    }

    /// Multiplier for sky light: 1 by day, `NIGHT_SKY_LIGHT` by night, with
    /// dawn and dusk easing between as the sun crosses the horizon.
    pub fn sky_light(&self, tick: f64) -> f32 {
        let h = self.sun_direction(tick)[1];
        let t = ((h + 0.12) / 0.32).clamp(0.0, 1.0) as f32;
        let t = t * t * (3.0 - 2.0 * t);
        NIGHT_SKY_LIGHT + (1.0 - NIGHT_SKY_LIGHT) * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(c: &DayCycle, tod: f64) -> f64 {
        tod * c.day_ticks as f64 - c.start_offset as f64
    }

    #[test]
    fn noon_is_bright_and_midnight_dark() {
        let c = DayCycle::default();
        assert!(c.sun_direction(at(&c, 0.5))[1] > 0.9);
        assert!(c.sun_direction(at(&c, 0.0))[1] < -0.9);
        assert_eq!(c.sky_light(at(&c, 0.5)), 1.0);
        assert_eq!(c.sky_light(at(&c, 0.0)), NIGHT_SKY_LIGHT);
    }

    #[test]
    fn sun_rises_in_the_east_and_sets_in_the_west() {
        let c = DayCycle::default();
        let rise = c.sun_direction(at(&c, 0.25));
        let set = c.sun_direction(at(&c, 0.75));
        assert!(rise[0] > 0.9 && rise[1].abs() < 0.1);
        assert!(set[0] < -0.9 && set[1].abs() < 0.1);
    }

    #[test]
    fn fresh_planets_start_in_the_morning() {
        let c = DayCycle::default();
        let tod = c.time_of_day(0.0);
        assert!(tod > 0.25 && tod < 0.4, "{tod}");
        assert!(c.sky_light(0.0) > 0.9);
    }

    #[test]
    fn days_repeat() {
        let c = DayCycle::default();
        let a = c.time_of_day(1234.0);
        let b = c.time_of_day(1234.0 + c.day_ticks as f64 * 3.0);
        assert!((a - b).abs() < 1e-9);
    }
}
