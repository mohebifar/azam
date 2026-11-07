//! Basic polygon queries.

use crate::Vec2;

/// Signed area of a closed polygon. Positive means clockwise on screen (y-down).
#[must_use]
pub fn signed_area(poly: &[Vec2]) -> f64 {
    let n = poly.len();
    let twice: f64 = (0..n)
        .map(|i| {
            let (p, q) = (poly[i], poly[(i + 1) % n]);
            p.x * q.y - q.x * p.y
        })
        .sum();
    twice / 2.0
}

/// Whether `q` lies inside a closed polygon (even-odd rule).
#[must_use]
pub fn contains(poly: &[Vec2], q: Vec2) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for (i, &a) in poly.iter().enumerate() {
        let b = poly[j];
        if (a.y > q.y) != (b.y > q.y) && q.x < (b.x - a.x) * (q.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Result of [`distance_to_segment`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentDistance {
    /// Distance from the query point, in mm.
    pub distance: f64,
    /// Position of the closest point along the segment, from 0 at `a` to 1 at `b`.
    pub t: f64,
    /// The closest point on the segment.
    pub closest: Vec2,
}

/// Distance from `p` to the line segment `a`–`b`.
#[must_use]
pub fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> SegmentDistance {
    let d = b - a;
    let len_sq = d.x * d.x + d.y * d.y;
    let t = if len_sq > 0.0 {
        ((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len_sq
    } else {
        0.0
    };
    let t = t.clamp(0.0, 1.0);
    let closest = Vec2::new(a.x + d.x * t, a.y + d.y * t);
    SegmentDistance {
        distance: crate::hypot(p.x - closest.x, p.y - closest.y),
        t,
        closest,
    }
}

/// Axis-aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox {
    /// Smallest x and y.
    pub min: Vec2,
    /// Largest x and y.
    pub max: Vec2,
}

impl BBox {
    /// Width and height.
    #[must_use]
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    /// Centre point.
    #[must_use]
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) / 2.0
    }
}

/// Bounding box of a set of points, or `None` if there are none.
#[must_use]
pub fn bbox(points: impl IntoIterator<Item = Vec2>) -> Option<BBox> {
    points.into_iter().fold(None, |acc, p| {
        Some(match acc {
            None => BBox { min: p, max: p },
            Some(b) => BBox {
                min: b.min.min(p),
                max: b.max.max(p),
            },
        })
    })
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "these tests use exactly representable values")]
mod tests {
    use super::*;

    const SQUARE: [Vec2; 4] = [
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(10.0, 10.0),
        Vec2::new(0.0, 10.0),
    ];

    #[test]
    fn area_sign_follows_winding() {
        assert_eq!(signed_area(&SQUARE), 100.0);
        let mut reversed = SQUARE;
        reversed.reverse();
        assert_eq!(signed_area(&reversed), -100.0);
    }

    #[test]
    fn point_in_polygon() {
        assert!(contains(&SQUARE, Vec2::new(5.0, 5.0)));
        assert!(!contains(&SQUARE, Vec2::new(15.0, 5.0)));
        assert!(!contains(&[], Vec2::ZERO));
    }

    #[test]
    fn bbox_of_points() {
        let b = bbox(SQUARE).expect("points are not empty");
        assert_eq!(b.size(), Vec2::new(10.0, 10.0));
        assert_eq!(b.center(), Vec2::new(5.0, 5.0));
        assert_eq!(bbox(std::iter::empty()), None);
    }
}
