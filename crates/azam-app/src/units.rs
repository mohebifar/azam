//! Display units (the project's: mm, cm or inches). Everything is stored in millimetres; lengths
//! are converted only where they are shown or typed.

use azam_doc::Units;

/// How many of `units` make one millimetre.
#[must_use]
pub fn per_mm(units: Units) -> f64 {
    match units {
        Units::Mm => 1.0,
        Units::Cm => 0.1,
        Units::In => 1.0 / 25.4,
    }
}

/// The unit's short name: `mm`, `cm`, `in`.
#[must_use]
pub const fn name(units: Units) -> &'static str {
    match units {
        Units::Mm => "mm",
        Units::Cm => "cm",
        Units::In => "in",
    }
}

/// `mm` in `units`, as a number.
#[must_use]
pub fn value(units: Units, mm: f64) -> f64 {
    mm * per_mm(units)
}

/// `mm` shown in `units` with its name: `12.3 cm`, `4.84 in`, `123 mm`.
#[must_use]
pub fn text(units: Units, mm: f64) -> format_args_free::Len {
    format_args_free::Len(units, mm)
}

/// The number alone, to the precision the unit reads at.
#[must_use]
pub fn number(units: Units, mm: f64) -> String {
    let v = value(units, mm);
    let v = if v.abs() < 0.5 * 10f64.powi(-decimals(units)) {
        0.0
    } else {
        v
    };
    format!("{v:.*}", decimals(units) as usize)
}

/// Decimals a length reads to in `units`.
const fn decimals(units: Units) -> i32 {
    match units {
        Units::Mm => 0,
        Units::Cm => 1,
        Units::In => 2,
    }
}

/// Display helpers that format without allocating until written.
pub mod format_args_free {
    use super::{Units, name, number};

    /// A length shown with its unit.
    #[derive(Clone, Copy, Debug)]
    pub struct Len(pub Units, pub f64);

    impl std::fmt::Display for Len {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{} {}", number(self.0, self.1), name(self.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_read_in_each_unit() {
        assert_eq!(text(Units::Cm, 123.0).to_string(), "12.3 cm");
        assert_eq!(text(Units::In, 25.4).to_string(), "1.00 in");
        assert_eq!(text(Units::Mm, 12.4).to_string(), "12 mm");
        assert_eq!(number(Units::Cm, -0.01), "0.0", "no negative zero");
    }
}
