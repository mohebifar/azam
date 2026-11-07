//! Procedural fabric weaves: the brightness pattern the viewport draws (`mesh.wgsl`'s `weave`),
//! evaluated at a surface point's pattern coordinates.

use std::f32::consts::TAU;

use glam::Vec2;

use crate::Weave;

/// A thread wave across `t` (repeats per unit), faded by `fade`.
fn wave(t: f32, fade: f32) -> f32 {
    (t * TAU).sin() * fade
}

/// 1 where `x >= edge`, else 0.
fn step(edge: f32, x: f32) -> f32 {
    if x >= edge { 1.0 } else { 0.0 }
}

/// GLSL's `fract`: `x - floor(x)`.
fn fract(x: f32) -> f32 {
    x - x.floor()
}

/// Brightness change of `kind` at pattern point `uv` (mm), around 1, for a weave repeating every
/// `repeat_mm` seen with a pixel footprint of `footprint_mm`.
///
/// Threads fade to their average before they get finer than about three pixels (safely above
/// the Nyquist limit): earlier than the viewport fades them, since the averaged samples of a path
/// tracer would otherwise turn the leftover waves into moiré.
#[must_use]
pub(crate) fn weave(uv: Vec2, kind: Weave, repeat_mm: f32, footprint_mm: f32) -> f32 {
    let repeat = repeat_mm.max(0.1);
    let p = uv / repeat;
    let width = footprint_mm / repeat;
    // How much of a wave with `freq` repeats per weave repeat survives.
    let fade = |freq: f32| (1.0 - (width * freq - 0.1) * 4.0).clamp(0.0, 1.0);
    let diagonal = std::f32::consts::SQRT_2;
    match kind {
        Weave::None => 1.0,
        Weave::Plain => 1.0 + 0.05 * wave(p.x, fade(1.0)) * wave(p.y, fade(1.0)),
        Weave::Twill => 1.0 + 0.07 * wave(p.x + p.y, fade(diagonal)),
        Weave::Denim => 1.0 + 0.10 * wave(p.x + p.y, fade(diagonal)) + 0.04 * wave(p.y * 0.37, fade(0.37)),
        Weave::Satin => 1.0 + 0.015 * wave(p.x * 0.5, fade(0.5)),
        Weave::Knit => {
            let v = (fract(p.x) - 0.5).abs() * 2.0;
            1.0 + 0.08 * fade(2.0) * ((fract(p.y * 2.0 + v) - 0.5).abs() * 2.0 - 0.5)
        }
        Weave::Rib => 1.0 + 0.14 * wave(p.x, fade(1.0)),
        Weave::Herringbone => {
            let flip = if fract(p.x * 0.25) < 0.5 { 1.0 } else { -1.0 };
            1.0 + 0.08 * wave(p.y + p.x * flip, fade(diagonal))
        }
        Weave::Corduroy => 1.0 + 0.22 * wave(p.x, fade(1.0)).max(-0.4),
        Weave::Check => {
            let a = step(0.5, fract(p.x * 0.125));
            let b = step(0.5, fract(p.y * 0.125));
            1.0 - 0.18 * (a + b) * 0.5 - 0.1 * a * b
        }
        Weave::Stripe => 1.0 - 0.22 * step(0.5, fract(p.x * 0.1)),
        Weave::Plaid => {
            let a = step(0.7, fract(p.x * 0.08)) + 0.5 * step(0.92, fract(p.x * 0.08 + 0.3));
            let b = step(0.7, fract(p.y * 0.08)) + 0.5 * step(0.92, fract(p.y * 0.08 + 0.3));
            1.0 - 0.16 * (a + b)
        }
        Weave::Fleece => 1.0 + 0.04 * wave(p.x * 2.3 + (p.y * 1.7).sin(), fade(2.3)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Weave; 13] = [
        Weave::None,
        Weave::Plain,
        Weave::Twill,
        Weave::Denim,
        Weave::Satin,
        Weave::Knit,
        Weave::Rib,
        Weave::Herringbone,
        Weave::Corduroy,
        Weave::Check,
        Weave::Stripe,
        Weave::Plaid,
        Weave::Fleece,
    ];

    #[test]
    fn weaves_stay_near_one() {
        for kind in ALL {
            for i in 0..200 {
                let uv = Vec2::new(i as f32 * 0.37, i as f32 * 0.91);
                let w = weave(uv, kind, 1.6, 0.1);
                assert!((0.5..=1.3).contains(&w), "{kind:?} {w}");
            }
        }
    }

    #[test]
    fn fine_threads_fade_to_their_average() {
        // Seen from far away (a footprint of many threads) a twill is flat.
        let w = weave(Vec2::new(0.3, 0.1), Weave::Twill, 1.6, 50.0);
        assert!((w - 1.0).abs() < 1e-6);
        // Up close it varies.
        let close: Vec<f32> = (0..10)
            .map(|i| weave(Vec2::new(i as f32 * 0.2, 0.0), Weave::Twill, 1.6, 0.05))
            .collect();
        assert!(close.iter().any(|w| (w - 1.0).abs() > 0.03));
    }

    #[test]
    fn names_follow_the_viewport() {
        assert_eq!(Weave::from_name("jersey"), Weave::Knit);
        assert_eq!(Weave::from_name("denim"), Weave::Denim);
    }
}
