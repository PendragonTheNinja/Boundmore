//! Where a sound in the world is heard from: its volume falls with distance
//! from the camera and it pans by direction. Positions are canonical, so
//! the offset goes through `PlanetShape::delta` (ADR-0002): a sound just
//! across the seam is as near as it looks.

use bm_core::PlanetShape;
use glam::DVec3;

/// Within this many blocks a sound plays at full volume.
pub const NEAR: f64 = 2.0;
/// The most a sound pans to one side: never wholly in one ear.
pub const PAN_MAX: f32 = 0.75;

/// The camera: where sounds are heard from.
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    pub shape: PlanetShape,
    pub eye: DVec3,
    /// The camera's forward direction (only its heading is used).
    pub forward: DVec3,
}

/// How a sound from the world plays: its volume (0 to 1) and its pan
/// (-1 left to 1 right).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub gain: f32,
    pub pan: f32,
}

/// A sound at `at`, heard up to `range` blocks away; None past the range.
pub fn place(l: &Listener, at: DVec3, range: f64) -> Option<Placement> {
    let d = l.shape.delta(l.eye, at);
    let dist = d.length();
    if !dist.is_finite() || dist >= range {
        return None;
    }
    let gain = if dist <= NEAR {
        1.0
    } else {
        let x = (dist - NEAR) / (range - NEAR).max(1e-6);
        (1.0 - x) * (1.0 - x)
    };
    let f = DVec3::new(l.forward.x, 0.0, l.forward.z).normalize_or_zero();
    // The camera's right (`Camera::forward` is (sin yaw, _, -cos yaw)).
    let right = DVec3::new(-f.z, 0.0, f.x);
    let flat = DVec3::new(d.x, 0.0, d.z);
    let pan = if flat.length() < 1e-6 {
        0.0
    } else {
        // Very near, a sound is in both ears.
        flat.normalize().dot(right) * dist.min(1.0)
    };
    Some(Placement {
        gain: gain as f32,
        pan: pan as f32 * PAN_MAX,
    })
}

/// Equal-power left and right gains for a pan, 1 and 1 in the middle.
pub fn stereo(pan: f32) -> (f32, f32) {
    let theta = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    let k = std::f32::consts::SQRT_2;
    (theta.cos() * k, theta.sin() * k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener(yaw: f64) -> Listener {
        Listener {
            shape: PlanetShape::new(8, 8),
            eye: DVec3::new(100.0, 50.0, 100.0),
            forward: DVec3::new(yaw.sin(), 0.0, -yaw.cos()),
        }
    }

    #[test]
    fn near_is_loud_far_is_quiet_and_past_the_range_is_silent() {
        let l = listener(0.0);
        let at = |d: f64| place(&l, l.eye + DVec3::new(0.0, 0.0, -d), 32.0);
        assert_eq!(at(1.0).unwrap().gain, 1.0);
        let (g8, g20) = (at(8.0).unwrap().gain, at(20.0).unwrap().gain);
        assert!(1.0 > g8 && g8 > g20 && g20 > 0.0, "{g8} {g20}");
        assert!(at(32.0).is_none());
        assert!(at(100.0).is_none());
    }

    #[test]
    fn a_sound_to_the_right_pans_right_and_turning_round_swaps_it() {
        // Facing -Z (yaw 0), +X is to the right.
        let l = listener(0.0);
        let right = place(&l, l.eye + DVec3::new(5.0, 0.0, 0.0), 32.0).unwrap();
        assert!((right.pan - PAN_MAX).abs() < 1e-6);
        let ahead = place(&l, l.eye + DVec3::new(0.0, 0.0, -5.0), 32.0).unwrap();
        assert!(ahead.pan.abs() < 1e-6);
        let turned = listener(std::f64::consts::PI);
        let left = place(&turned, turned.eye + DVec3::new(5.0, 0.0, 0.0), 32.0).unwrap();
        assert!((left.pan + PAN_MAX).abs() < 1e-6);
        // Straight above: both ears.
        let above = place(&l, l.eye + DVec3::new(0.0, 6.0, 0.0), 32.0).unwrap();
        assert_eq!(above.pan, 0.0);
    }

    #[test]
    fn across_the_seam_is_as_near_as_beside_us() {
        let mut l = listener(0.0);
        let w = l.shape.width_blocks() as f64;
        l.eye = DVec3::new(1.0, 50.0, 10.0);
        // Three blocks to the left, across the seam at x = 0.
        let across = place(&l, DVec3::new(w - 2.0, 50.0, 10.0), 32.0).unwrap();
        let beside = place(&l, DVec3::new(4.0, 50.0, 10.0), 32.0).unwrap();
        assert_eq!(across.gain, beside.gain);
        assert!((across.pan + beside.pan).abs() < 1e-6, "mirror images");
    }

    #[test]
    fn stereo_keeps_the_middle_even_and_power_equal() {
        let (l, r) = stereo(0.0);
        assert!((l - 1.0).abs() < 1e-6 && (r - 1.0).abs() < 1e-6);
        let (l, r) = stereo(1.0);
        assert!(l.abs() < 1e-6 && r > 1.4);
        for p in [-0.7f32, -0.2, 0.3, 0.75] {
            let (l, r) = stereo(p);
            assert!((l * l + r * r - 2.0).abs() < 1e-5);
            assert_eq!(p > 0.0, r > l);
        }
    }
}
