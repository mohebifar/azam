//! Sewing sides: a stretch of a piece outline between two points, walked in one direction.

use serde::{Deserialize, Serialize};

use crate::outline::wrap;
use crate::{Outline, PerimeterPoint, PointId};

/// Sides shorter than this are treated as the whole outline, in mm.
const MIN_SIDE_LENGTH: f64 = 1e-6;
/// Default spacing of points along a side polyline, in mm.
pub const DEFAULT_SIDE_STEP_MM: f64 = 5.0;

/// One end of a sewing side: a fraction along the segment that starts at corner `segment`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SideEnd {
    /// Id of the segment's start corner.
    pub segment: PointId,
    /// Fraction of the segment's arc length, from 0 to 1.
    pub t: f64,
}

/// Which way a side runs along the outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SideDirection {
    /// In outline point order.
    Forward,
    /// Against outline point order.
    Backward,
}

impl SideDirection {
    fn sign(self) -> f64 {
        match self {
            Self::Forward => 1.0,
            Self::Backward => -1.0,
        }
    }
}

/// A stretch of outline from `a` to `b`, walked in `direction`. Seams join two sides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SewingSide {
    /// Where the side starts.
    pub a: SideEnd,
    /// Where the side ends.
    pub b: SideEnd,
    /// Which way it runs from `a` to `b`.
    pub direction: SideDirection,
}

/// Perimeter extent of a [`SewingSide`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SideRange {
    /// Perimeter position of the start.
    pub start: f64,
    /// Perimeter position of the end.
    pub end: f64,
    /// Length walked from start to end, in mm.
    pub length: f64,
    /// Direction of travel.
    pub direction: SideDirection,
}

impl SewingSide {
    /// The whole segment that starts at `segment`, optionally walked backwards.
    #[must_use]
    pub fn segment(segment: PointId, reverse: bool) -> Self {
        let (ta, tb, direction) = if reverse {
            (1.0, 0.0, SideDirection::Backward)
        } else {
            (0.0, 1.0, SideDirection::Forward)
        };
        Self {
            a: SideEnd {
                segment: segment.clone(),
                t: ta,
            },
            b: SideEnd { segment, t: tb },
            direction,
        }
    }

    /// Where the side lies on `outline`, or `None` if it names a segment the outline lacks.
    #[must_use]
    pub fn range(&self, outline: &Outline) -> Option<SideRange> {
        let ka = outline.segment_index(&self.a.segment)?;
        let kb = outline.segment_index(&self.b.segment)?;
        let start = outline.perimeter_param(ka, self.a.t);
        let end = outline.perimeter_param(kb, self.b.t);
        let walked = match self.direction {
            SideDirection::Forward => end - start,
            SideDirection::Backward => start - end,
        };
        let mut length = wrap(walked, outline.total);
        if length < MIN_SIDE_LENGTH {
            length = outline.total;
        }
        Some(SideRange {
            start,
            end,
            length,
            direction: self.direction,
        })
    }

    /// Points along the side, about `step` mm apart, including both ends.
    #[must_use]
    pub fn polyline(&self, outline: &Outline, step: f64) -> Vec<PerimeterPoint> {
        let Some(range) = self.range(outline) else {
            return Vec::new();
        };
        let steps = ((range.length / step).ceil() as usize).max(2);
        (0..=steps)
            .map(|i| {
                let along = range.direction.sign() * range.length * i as f64 / steps as f64;
                outline.point_at_perimeter(range.start + along)
            })
            .collect()
    }
}
