//! Presence: who else is in the session and what they are looking at.

use serde::{Deserialize, Serialize};

/// Colours handed to collaborators, in turn.
pub const PRESENCE_COLORS: [&str; 8] = [
    "#E8654A", "#3B82F6", "#10B981", "#F59E0B", "#8B5CF6", "#EC4899", "#14B8A6", "#84CC16",
];

/// What one collaborator shares about themselves, sent as awareness state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Colour, `#rrggbb`.
    #[serde(default)]
    pub color: String,
    /// Selected piece, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece: Option<String>,
    /// 3D cursor position in mm, if the pointer is over the garment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<[f32; 3]>,
}

impl Presence {
    /// A presence named `name` with a colour picked from [`PRESENCE_COLORS`] by `seed`.
    #[must_use]
    pub fn named(name: &str, seed: u64) -> Self {
        Self {
            name: name.to_owned(),
            color: PRESENCE_COLORS[(seed % PRESENCE_COLORS.len() as u64) as usize].to_owned(),
            piece: None,
            cursor: None,
        }
    }

    /// The colour as `0xRRGGBB`, or grey when it does not parse.
    #[must_use]
    pub fn rgb(&self) -> u32 {
        self.color
            .strip_prefix('#')
            .filter(|h| h.len() == 6)
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or(0x808080)
    }
}

/// Another collaborator in the session.
#[derive(Clone, Debug, PartialEq)]
pub struct Peer {
    /// Awareness client id of the collaborator.
    pub client: u64,
    /// What they share.
    pub presence: Presence,
}
