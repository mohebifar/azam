//! Grading: per-point offsets that turn the base-size pattern into other sizes.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{OutlinePoint, PointId, Vec2};

/// Name of a size in the size group, such as `"M"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SizeName(pub String);

impl From<&str> for SizeName {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

/// Offset of one point in one size, relative to the base size, in mm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GradeDelta {
    /// Horizontal offset.
    #[serde(default)]
    pub dx: f64,
    /// Vertical offset.
    #[serde(default)]
    pub dy: f64,
}

/// Grade rules of a piece: point id → size → offset.
pub type GradeRules = BTreeMap<PointId, BTreeMap<SizeName, GradeDelta>>;

/// The piece's points in `size`. Returns the base points unchanged when `size` is the base size or
/// the piece has no rules; points without a rule for `size` keep their base position.
#[must_use]
pub fn graded_points<'a>(
    points: &'a [OutlinePoint],
    rules: &GradeRules,
    size: &SizeName,
    base: &SizeName,
) -> Cow<'a, [OutlinePoint]> {
    if size == base || rules.is_empty() {
        return Cow::Borrowed(points);
    }
    Cow::Owned(
        points
            .iter()
            .map(|p| match rules.get(&p.id).and_then(|by_size| by_size.get(size)) {
                Some(d) => OutlinePoint {
                    pos: p.pos + Vec2::new(d.dx, d.dy),
                    ..p.clone()
                },
                None => p.clone(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_rules_for_the_requested_size_only() {
        let pts = vec![
            OutlinePoint::corner("a", 0.0, 0.0),
            OutlinePoint::corner("b", 10.0, 0.0),
        ];
        let mut rules = GradeRules::new();
        rules
            .entry("b".into())
            .or_default()
            .insert("L".into(), GradeDelta { dx: 5.0, dy: 1.0 });

        let large = graded_points(&pts, &rules, &"L".into(), &"M".into());
        assert_eq!(large[0].pos, Vec2::new(0.0, 0.0));
        assert_eq!(large[1].pos, Vec2::new(15.0, 1.0));

        let base = graded_points(&pts, &rules, &"M".into(), &"M".into());
        assert!(matches!(base, Cow::Borrowed(_)));
        let small = graded_points(&pts, &rules, &"S".into(), &"M".into());
        assert_eq!(small[1].pos, Vec2::new(10.0, 0.0));
    }
}
