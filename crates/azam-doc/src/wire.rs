//! File-format helpers: the stored shape of outline points, line points and lenient maps.
//!
//! Stored points are `{ "id", "x", "y", "c": 1 }`, where `c` marks a curve point and is omitted
//! for corners. Geometry types use a vector and a boolean instead, so these helpers convert.

use std::collections::BTreeMap;

use azam_geom::{OutlinePoint, PointId, Vec2};
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

/// A stored curve flag: `1`/`true` mark a curve point; `0`, `false` and `null` a corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CurveFlag(pub bool);

impl Serialize for CurveFlag {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(u8::from(self.0))
    }
}

impl<'de> Deserialize<'de> for CurveFlag {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self(truthy(&serde_json::Value::deserialize(d)?)))
    }
}

impl CurveFlag {
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "signature required by serde")]
    pub(crate) fn is_corner(&self) -> bool {
        !self.0
    }
}

/// Whether a stored value counts as set.
fn truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Null => false,
        _ => true,
    }
}

#[derive(Serialize, Deserialize)]
struct PointWire {
    id: PointId,
    x: f64,
    y: f64,
    #[serde(default, skip_serializing_if = "CurveFlag::is_corner")]
    c: CurveFlag,
}

impl From<&OutlinePoint> for PointWire {
    fn from(p: &OutlinePoint) -> Self {
        Self {
            id: p.id.clone(),
            x: p.pos.x,
            y: p.pos.y,
            c: CurveFlag(p.curve),
        }
    }
}

impl From<PointWire> for OutlinePoint {
    fn from(p: PointWire) -> Self {
        Self {
            id: p.id,
            pos: Vec2::new(p.x, p.y),
            curve: p.c.0,
        }
    }
}

/// `#[serde(with)]` module for a piece outline (`Vec<OutlinePoint>`) in the stored point shape.
pub(crate) mod outline_points {
    use super::{Deserialize, Deserializer, OutlinePoint, PointWire, Serialize, Serializer};

    pub(crate) fn serialize<S: Serializer>(pts: &[OutlinePoint], s: S) -> Result<S::Ok, S::Error> {
        let wire: Vec<PointWire> = pts.iter().map(PointWire::from).collect();
        wire.serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<OutlinePoint>, D::Error> {
        let wire = Vec::<PointWire>::deserialize(d)?;
        Ok(wire.into_iter().map(OutlinePoint::from).collect())
    }
}

/// A map from segment id to millimetres that skips `null` entries when reading, as stored files
/// may clear an entry by setting it to `null`.
pub(crate) mod lenient_mm_map {
    use super::{BTreeMap, Deserialize, Deserializer, PointId, Serialize, Serializer, de};

    #[allow(clippy::ref_option, reason = "signature required by serde")]
    pub(crate) fn serialize<S: Serializer>(
        map: &Option<BTreeMap<PointId, f64>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        map.serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<BTreeMap<PointId, f64>>, D::Error> {
        let raw = Option::<BTreeMap<PointId, Option<serde_json::Value>>>::deserialize(d)?;
        raw.map(|m| {
            m.into_iter()
                .filter_map(|(k, v)| match v {
                    None | Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::Number(n)) => Some(
                        n.as_f64()
                            .map(|x| (k, x))
                            .ok_or_else(|| de::Error::custom("allowance is not a finite number")),
                    ),
                    Some(other) => Some(Err(de::Error::custom(format!(
                        "allowance must be a number, found {other}"
                    )))),
                })
                .collect()
        })
        .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct Holder {
        #[serde(with = "outline_points")]
        pts: Vec<OutlinePoint>,
    }

    #[test]
    fn points_use_the_stored_shape() {
        let json = r#"{"pts":[{"id":"a","x":1,"y":2},{"id":"b","x":3,"y":4,"c":1},{"id":"d","x":0,"y":0,"c":true}]}"#;
        let h: Holder = serde_json::from_str(json).expect("valid points");
        assert!(!h.pts[0].curve && h.pts[1].curve && h.pts[2].curve);
        let back = serde_json::to_value(&h).expect("serializable");
        assert_eq!(back["pts"][0].get("c"), None);
        assert_eq!(back["pts"][1]["c"], 1);
    }
}
