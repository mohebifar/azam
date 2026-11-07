//! Snapping while drawing and dragging: to the corners of every piece, and Shift's 45° steps.

use azam_doc::PieceId;
use azam_geom::Vec2;

use super::hit::{piece_order, shown_points};
use crate::app::AzamApp;

/// Corners closer than this snap, in logical pixels.
const SNAP_PX: f64 = 8.0;
/// Shift constrains directions to multiples of this angle, radians.
const CONSTRAIN_STEP: f64 = std::f64::consts::FRAC_PI_4;

impl AzamApp {
    /// `q` snapped to the nearest corner of any piece within reach (leaving out point `exclude`),
    /// or `q` itself with `free` (Alt) or nothing in reach. Remembers the snap for the overlay.
    pub(crate) fn snap_point(&mut self, q: Vec2, exclude: Option<(&PieceId, usize)>, free: bool) -> Vec2 {
        self.tools.snap = None;
        if free {
            return q;
        }
        let project = &self.session.project;
        let mut best = SNAP_PX / self.pattern_view.zoom;
        let mut found = None;
        for id in piece_order(project) {
            for (i, (p, curve)) in shown_points(project, &id).into_iter().enumerate() {
                if curve || exclude.is_some_and(|(ep, ei)| *ep == id && ei == i) {
                    continue;
                }
                let d = p.distance(q);
                if d < best {
                    best = d;
                    found = Some(p);
                }
            }
        }
        self.tools.snap = found;
        found.unwrap_or(q)
    }
}

/// `q` moved onto the nearest 45° direction from `from`, keeping its distance.
#[must_use]
pub fn constrain_45(from: Vec2, q: Vec2) -> Vec2 {
    let d = q - from;
    let angle = (d.y.atan2(d.x) / CONSTRAIN_STEP).round() * CONSTRAIN_STEP;
    let (s, c) = angle.sin_cos();
    from + Vec2::new(c, s) * d.length()
}

/// The box from `a` to `q` made square (the larger side wins), keeping the drag direction.
#[must_use]
pub fn square_box(a: Vec2, q: Vec2) -> Vec2 {
    let d = q - a;
    let side = d.x.abs().max(d.y.abs());
    let sign = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
    a + Vec2::new(sign(d.x) * side, sign(d.y) * side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_constrains_to_45_degrees() {
        let q = constrain_45(Vec2::ZERO, Vec2::new(10.0, 1.0));
        assert!(q.distance(Vec2::new(10.0_f64.hypot(1.0), 0.0)) < 1e-9);
        let d = constrain_45(Vec2::ZERO, Vec2::new(10.0, 9.0));
        assert!((d.x - d.y).abs() < 1e-9);
    }

    #[test]
    fn square_boxes_keep_their_direction() {
        assert_eq!(
            square_box(Vec2::ZERO, Vec2::new(-10.0, 4.0)),
            Vec2::new(-10.0, 10.0)
        );
    }
}
