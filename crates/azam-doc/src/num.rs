//! Numeric and ordering conventions shared with the stored data.

/// Rounds half-way cases towards positive infinity (`-2.5` → `-2`), the rounding used for drafted
/// dimensions and grade offsets.
#[must_use]
pub(crate) fn round_half_up(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// `-x`, with a zero result always positive so it is written as `0`, not `-0`.
#[must_use]
pub(crate) fn negate(x: f64) -> f64 {
    -x + 0.0
}

/// Largest key that sorts as an array index in stored object order.
const MAX_INDEX_KEY: u64 = u32::MAX as u64 - 1;

/// Whether `key` is a canonical array index (`"0"`, `"17"`, not `"07"`).
fn index_key(key: &str) -> Option<u64> {
    let canonical =
        !key.is_empty() && key.bytes().all(|b| b.is_ascii_digit()) && (key == "0" || !key.starts_with('0'));
    if !canonical {
        return None;
    }
    key.parse().ok().filter(|&v| v <= MAX_INDEX_KEY)
}

/// The key that comes first when keys are inserted into an object in this order: array-index keys
/// first in numeric order, then the others in insertion order.
#[must_use]
pub(crate) fn js_first_key<'a>(keys: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let keys: Vec<&str> = keys.into_iter().collect();
    keys.iter()
        .filter_map(|k| index_key(k).map(|v| (v, *k)))
        .min_by_key(|(v, _)| *v)
        .map(|(_, k)| k)
        .or_else(|| keys.first().copied())
}

/// Sorts `items` by their keys in stored object order (see [`js_first_key`]), keeping insertion
/// order among non-index keys.
pub(crate) fn sort_js_object_order<T>(items: &mut [(String, T)]) {
    items.sort_by_key(|(k, _)| index_key(k).map_or((1, 0), |v| (0, v)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_matches_stored_offsets() {
        assert!((round_half_up(-2.5) - -2.0).abs() < f64::EPSILON);
        assert!((round_half_up(2.5) - 3.0).abs() < f64::EPSILON);
        assert!((round_half_up(0.499_999_999_999_999_94)).abs() < f64::EPSILON);
    }

    #[test]
    fn index_keys_come_first() {
        assert_eq!(js_first_key(["p_3", "12", "4"]), Some("4"));
        assert_eq!(js_first_key(["b", "a", "07"]), Some("b"));
        assert_eq!(js_first_key([]), None);
    }
}
