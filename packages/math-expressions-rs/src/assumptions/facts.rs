//! [`Facts`] — what is known about one (sub)expression, as eight three-valued
//! predicates, plus the constant-leaf fact tables.

use super::MaybeBool;
use crate::num::Number;

/// What is known about one (sub)expression. Every field is three-valued.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Facts {
    pub(super) integer: MaybeBool,
    pub(super) real: MaybeBool,
    pub(super) complex: MaybeBool,
    pub(super) nonzero: MaybeBool,
    pub(super) nonneg: MaybeBool,
    pub(super) positive: MaybeBool,
    pub(super) negative: MaybeBool,
    pub(super) nonpos: MaybeBool,
}

impl Facts {
    /// Everything unknown.
    pub(super) fn unknown() -> Facts {
        Facts::default()
    }

    /// Facts of an exact number.
    pub(super) fn of_number(n: &Number) -> Facts {
        let v = n.to_f64();
        if v.is_nan() {
            return Facts::unknown();
        }
        let is_int = match n {
            Number::Int(_) => true,
            Number::Rat(..) => false,
            Number::Big(_) => n.magnitude_log10().is_some() && is_big_int(n),
            Number::Float(_) => v.fract() == 0.0,
        };
        Facts {
            integer: Some(is_int),
            real: Some(true),
            complex: Some(true),
            nonzero: Some(v != 0.0),
            nonneg: Some(v >= 0.0),
            positive: Some(v > 0.0),
            negative: Some(v < 0.0),
            nonpos: Some(v <= 0.0),
        }
    }

    /// A real, positive, non-integer constant (`pi`, `e`).
    pub(super) fn positive_transcendental() -> Facts {
        Facts {
            integer: Some(false),
            real: Some(true),
            complex: Some(true),
            nonzero: Some(true),
            nonneg: Some(true),
            positive: Some(true),
            negative: Some(false),
            nonpos: Some(false),
        }
    }

    /// The imaginary unit: complex, not real; sign predicates are all false
    /// (JS reports F, not undefined, for `i`).
    pub(super) fn imaginary_unit() -> Facts {
        Facts {
            integer: Some(false),
            real: Some(false),
            complex: Some(true),
            nonzero: Some(true),
            nonneg: Some(false),
            positive: Some(false),
            negative: Some(false),
            nonpos: Some(false),
        }
    }
}

fn is_big_int(n: &Number) -> bool {
    matches!(n, Number::Big(b) if matches!(&**b, crate::num::BigNumber::Int(_)))
}
