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

/// The slot a piece named `name` most likely belongs in, judged from the words pattern makers
/// use ("Back", "Sleeve L", "Collar", "Front Leg R"…); `None` when the name says nothing.
#[must_use]
pub fn slot_for_name(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |stem: &str| words.iter().any(|w| w.starts_with(stem));
    let is = |word: &str| words.contains(&word);
    let left = has("left") || is("l") || is("lt");
    let right = !left && (has("right") || is("r") || is("rt"));
    let back = has("back") || is("bk");
    if has("sleeve") || has("cuff") || is("arm") {
        return Some(if right { "armR" } else { "armL" });
    }
    if has("collar") || has("neck") || has("hood") {
        return Some("neck");
    }
    if has("leg") || has("pant") || has("trouser") || is("shorts") {
        return Some(match (right, back) {
            (false, false) => "legLF",
            (false, true) => "legLB",
            (true, false) => "legRF",
            (true, true) => "legRB",
        });
    }
    if back {
        return Some(if left {
            "backL"
        } else if right {
            "backR"
        } else {
            "back"
        });
    }
    if has("front") || is("fr") {
        return Some(if left {
            "frontL"
        } else if right {
            "frontR"
        } else {
            "front"
        });
    }
    if has("side") || has("gusset") {
        return Some(if right { "right" } else { "left" });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::slot_for_name;

    #[test]
    fn names_choose_slots() {
        for (name, slot) in [
            ("Back", Some("back")),
            ("BACK BODICE", Some("back")),
            ("Front", Some("front")),
            ("Front R", Some("frontR")),
            ("Left Front", Some("frontL")),
            ("Sleeve L", Some("armL")),
            ("Sleeve Right", Some("armR")),
            ("Neck Rib", Some("neck")),
            ("Collar Stand", Some("neck")),
            ("Front Leg R", Some("legRF")),
            ("Back Leg L", Some("legLB")),
            ("Side Panel R", Some("right")),
            ("Piece 3", None),
            ("Pocket", None),
        ] {
            assert_eq!(slot_for_name(name), slot, "{name}");
        }
    }
}
