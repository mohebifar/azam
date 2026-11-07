//! Display transform: an ACES-like filmic tone curve and the sRGB transfer function.

use glam::{Mat3, Vec3};

/// sRGB → ACES AP1-ish input transform of Stephen Hill's ACES fit (includes the RRT's
/// saturation adjustment), stored by columns.
const ACES_IN: Mat3 = Mat3::from_cols_array(&[
    0.59719, 0.07600, 0.02840, //
    0.35458, 0.90834, 0.13383, //
    0.04823, 0.01566, 0.83777,
]);
/// Inverse transform back to linear sRGB, stored by columns.
const ACES_OUT: Mat3 = Mat3::from_cols_array(&[
    1.60475, -0.10208, -0.00327, //
    -0.53108, 1.10813, -0.07276, //
    -0.07367, -0.00605, 1.07602,
]);

/// The RRT + ODT curve fit.
fn rrt_odt(v: Vec3) -> Vec3 {
    let a = v * (v + 0.024_578_6) - 0.000_090_537;
    let b = v * (0.983_729 * v + 0.432_951) + 0.238_081;
    a / b
}

/// Maps scene-linear `color` to display-linear 0..1 with the ACES filmic curve (Hill's fit): a
/// soft toe, a long shoulder rolling highlights off to white.
#[must_use]
pub fn aces_filmic(color: [f32; 3]) -> [f32; 3] {
    let c = Vec3::from(color).max(Vec3::ZERO);
    // Hill's fit expects the exposure of the reference ACES pipeline (1.8× brighter than 1:1).
    let mapped = ACES_OUT * rrt_odt(ACES_IN * (c * 1.8));
    mapped.clamp(Vec3::ZERO, Vec3::ONE).into()
}

/// Encodes one display-linear channel (0..1) with the sRGB transfer function.
#[must_use]
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_stays_black_and_white_rolls_off() {
        let black = aces_filmic([0.0; 3]);
        assert!(black.iter().all(|c| c.abs() < 1e-3), "{black:?}");
        let bright = aces_filmic([50.0; 3]);
        assert!(bright.iter().all(|&c| c > 0.98), "{bright:?}");
    }

    #[test]
    fn curve_is_monotonic() {
        let mut last = -1.0;
        for i in 0..200 {
            let v = i as f32 * 0.05;
            let y = aces_filmic([v; 3])[1];
            assert!(y >= last, "{v}: {y} < {last}");
            last = y;
        }
        // Mid grey lands in the middle of the display range.
        let grey = linear_to_srgb(aces_filmic([0.18; 3])[1]);
        assert!((0.35..0.6).contains(&grey), "{grey}");
    }

    #[test]
    fn srgb_endpoints() {
        assert!(linear_to_srgb(0.0).abs() < 1e-7);
        assert!((linear_to_srgb(1.0) - 1.0).abs() < 1e-6);
        assert!((linear_to_srgb(0.214_041) - 0.5).abs() < 1e-3);
    }
}
