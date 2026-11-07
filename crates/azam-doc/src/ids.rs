//! Typed identifiers and the deterministic id generator.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use azam_geom::{PointId, SizeName};

macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// Creates an id from anything string-like.
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// The id as a string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self::new(id)
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(
    /// Identifier of a pattern piece, unique within a project.
    PieceId
);
string_id!(
    /// Identifier of a seam (one sewing connection between two groups of sides).
    SeamId
);
string_id!(
    /// Identifier of a fabric.
    FabricId
);
string_id!(
    /// Identifier of a colorway.
    ColorwayId
);
string_id!(
    /// Identifier of a topstitch.
    StitchId
);
string_id!(
    /// Identifier of a point of measure.
    PomId
);
string_id!(
    /// Identifier of a button, buttonhole or comment.
    ItemId
);
string_id!(
    /// Identifier of an internal line or graphic placed on a piece.
    PartId
);

/// Separator between an id's kind prefix and its sequence number.
const SEPARATOR: char = '_';

/// Prefixes of generated ids, one per kind of object.
pub mod prefix {
    /// Pattern pieces.
    pub const PIECE: &str = "pc";
    /// Outline points.
    pub const POINT: &str = "p";
    /// Seams.
    pub const SEAM: &str = "sw";
    /// Fabrics.
    pub const FABRIC: &str = "f";
    /// Topstitches.
    pub const STITCH: &str = "st";
    /// Points of measure.
    pub const POM: &str = "pm";
    /// Notches.
    pub const NOTCH: &str = "n";
    /// Internal lines.
    pub const INTERNAL: &str = "il";
    /// Graphics.
    pub const GRAPHIC: &str = "g";
    /// Buttons.
    pub const BUTTON: &str = "bt";
    /// Buttonholes.
    pub const HOLE: &str = "bh";
    /// Comments.
    pub const COMMENT: &str = "c";
}

/// Generates ids of the form `<prefix>_<n>` from a counter.
///
/// Ids are deterministic: a fresh project starts at 1 and every id consumes the next number, so
/// the same edits always produce the same ids. A generator that is not yet seeded (for example in
/// a project that was just loaded) is seeded past every id of this form already in the project the
/// first time the project needs one, so generated ids never collide with stored ones.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdGen {
    last: u64,
    seeded: bool,
}

impl IdGen {
    /// A seeded generator whose next id has sequence number `last + 1`.
    #[must_use]
    pub fn after(last: u64) -> Self {
        Self { last, seeded: true }
    }

    /// Whether the generator has been seeded.
    #[must_use]
    pub fn is_seeded(&self) -> bool {
        self.seeded
    }

    /// Sequence number of the last id handed out.
    #[must_use]
    pub fn last(&self) -> u64 {
        self.last
    }

    /// A new id with the given kind prefix.
    pub fn fresh(&mut self, prefix: &str) -> String {
        self.last += 1;
        format!("{prefix}{SEPARATOR}{}", self.last)
    }

    /// A seeded generator past every generated-looking id found in `value` (strings and object
    /// keys of the form `<lowercase letters>_<digits>`).
    #[must_use]
    pub fn seeded_from(value: &Value) -> Self {
        Self::after(max_sequence(value))
    }
}

/// Largest sequence number of a generated-looking id anywhere in `value`.
fn max_sequence(value: &Value) -> u64 {
    match value {
        Value::String(s) => sequence_of(s).unwrap_or(0),
        Value::Array(items) => items.iter().map(max_sequence).max().unwrap_or(0),
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| sequence_of(k).unwrap_or(0).max(max_sequence(v)))
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

/// The sequence number of `id` if it looks generated.
fn sequence_of(id: &str) -> Option<u64> {
    let (kind, number) = id.split_once(SEPARATOR)?;
    let looks_generated = !kind.is_empty()
        && kind.bytes().all(|b| b.is_ascii_lowercase())
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit());
    if looks_generated {
        number.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_ids_count_up() {
        let mut ids = IdGen::after(0);
        assert_eq!(ids.fresh("pc"), "pc_1");
        assert_eq!(ids.fresh("p"), "p_2");
    }

    #[test]
    fn seeding_skips_existing_ids() {
        let doc = serde_json::json!({
            "pieces": { "pc_7": { "pts": [{ "id": "p_12" }, { "id": "hps" }] } },
            "fabrics": { "fab_main": {} },
            "name": "a_b_3"
        });
        let mut ids = IdGen::seeded_from(&doc);
        assert_eq!(ids.fresh("p"), "p_13");
    }
}
