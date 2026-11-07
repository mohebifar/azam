//! Arrangement slots around the avatar, as far as the document needs them.

/// Kind of body part a slot wraps around.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlotKind {
    /// Around the torso; the slot angle is measured around the body.
    Body,
    /// Around an arm.
    Arm,
    /// Around the neck.
    Neck,
    /// Around a leg.
    Leg,
}

/// Every slot id with its kind.
const SLOTS: [(&str, SlotKind); 15] = [
    ("front", SlotKind::Body),
    ("frontL", SlotKind::Body),
    ("frontR", SlotKind::Body),
    ("left", SlotKind::Body),
    ("right", SlotKind::Body),
    ("backL", SlotKind::Body),
    ("backR", SlotKind::Body),
    ("back", SlotKind::Body),
    ("armL", SlotKind::Arm),
    ("armR", SlotKind::Arm),
    ("neck", SlotKind::Neck),
    ("legLF", SlotKind::Leg),
    ("legLB", SlotKind::Leg),
    ("legRF", SlotKind::Leg),
    ("legRB", SlotKind::Leg),
];

/// Kind of slot `id`; unknown slots count as the front body slot.
#[must_use]
pub fn slot_kind(id: &str) -> SlotKind {
    SLOTS
        .iter()
        .find(|(s, _)| *s == id)
        .map_or(SlotKind::Body, |&(_, kind)| kind)
}

/// The slot on the other side of the body, for slots that have one.
#[must_use]
pub fn mirror_slot(id: &str) -> Option<&'static str> {
    Some(match id {
        "frontL" => "frontR",
        "frontR" => "frontL",
        "left" => "right",
        "right" => "left",
        "backL" => "backR",
        "backR" => "backL",
        "armL" => "armR",
        "armR" => "armL",
        "legLF" => "legRF",
        "legRF" => "legLF",
        "legLB" => "legRB",
        "legRB" => "legLB",
        _ => return None,
    })
}
