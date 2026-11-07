use serde::{Deserialize, Serialize};

/// A 2D vector in millimetres. Double precision keeps pattern edits exact over many operations.
pub type Vec2 = glam::DVec2;

/// Stable identifier of an outline point, unique within one piece.
///
/// A segment is identified by the id of its start corner, so sewing and per-edge settings keep
/// pointing at the same edge when points are added elsewhere.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PointId(pub String);

impl PointId {
    /// Creates an id from anything string-like.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<&str> for PointId {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}

/// One point of a piece outline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutlinePoint {
    /// Stable id of the point.
    pub id: PointId,
    /// Position in millimetres.
    pub pos: Vec2,
    /// `true` for a Bézier control point, `false` for a corner the outline passes through.
    pub curve: bool,
}

impl OutlinePoint {
    /// A corner point the outline passes through.
    pub fn corner(id: impl Into<String>, x: f64, y: f64) -> Self {
        Self {
            id: PointId::new(id),
            pos: Vec2::new(x, y),
            curve: false,
        }
    }

    /// A Bézier control point between two corners.
    pub fn curve(id: impl Into<String>, x: f64, y: f64) -> Self {
        Self {
            id: PointId::new(id),
            pos: Vec2::new(x, y),
            curve: true,
        }
    }
}
