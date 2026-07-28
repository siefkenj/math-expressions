//! The eight three-valued predicates of `lib/assumptions/element_of_sets.js` —
//! thin wrappers that canonicalize their input and read one field off the
//! inferred [`Facts`](super::facts::Facts).

use super::infer::facts;
use super::{Assumptions, MaybeBool};
use crate::expr::Expr;
use crate::normalize::canonicalize;

pub fn is_integer(e: &Expr, a: &Assumptions) -> MaybeBool {
    facts(&canonicalize(e), a).integer
}
pub fn is_real(e: &Expr, a: &Assumptions) -> MaybeBool {
    facts(&canonicalize(e), a).real
}
pub fn is_complex(e: &Expr, a: &Assumptions) -> MaybeBool {
    facts(&canonicalize(e), a).complex
}
pub fn is_nonzero(e: &Expr, a: &Assumptions) -> MaybeBool {
    facts(&canonicalize(e), a).nonzero
}
pub fn is_nonnegative(e: &Expr, a: &Assumptions) -> MaybeBool {
    facts(&canonicalize(e), a).nonneg
}
pub fn is_positive(e: &Expr, a: &Assumptions) -> MaybeBool {
    facts(&canonicalize(e), a).positive
}
pub fn is_negative(e: &Expr, a: &Assumptions) -> MaybeBool {
    facts(&canonicalize(e), a).negative
}
pub fn is_nonpositive(e: &Expr, a: &Assumptions) -> MaybeBool {
    facts(&canonicalize(e), a).nonpos
}
