//! Literal-matrix helpers used by the `mul`/`pow` smart constructors:
//! matrix-valued detection, the identity, and symbolic literal multiplication.

use super::{add, mul};
use crate::expr::{Expr, Mat};

/// Is this canonical factor matrix-valued (a literal matrix, an unevaluated
/// matrix power, or an unfoldable matrix product)? Such factors must not
/// commute past each other and are excluded from scalar-only rewrites.
pub(crate) fn is_matrix_valued(e: &Expr) -> bool {
    match e {
        Expr::Matrix(_) => true,
        Expr::Pow(b, _) => matches!(**b, Expr::Matrix(_)),
        Expr::Mul(fs) => fs.iter().any(is_matrix_valued),
        _ => false,
    }
}

/// The n×n identity matrix.
pub(crate) fn identity_matrix(n: u32) -> Expr {
    Expr::Matrix(Mat::generate(n, n, |r, c| Expr::int(i64::from(r == c))))
}

/// Multiply two literal matrices symbolically (entries built with the smart
/// constructors). `None` on dimension mismatch or when the work exceeds
/// `limits.max_expand_terms` (the caller keeps the product unevaluated).
pub(crate) fn matmul_literal(a: &Expr, b: &Expr) -> Option<Expr> {
    let (Expr::Matrix(ma), Expr::Matrix(mb)) = (a, b) else {
        return None;
    };
    if ma.cols() != mb.rows() {
        return None;
    }
    let (r1, c1, c2) = (ma.rows() as usize, ma.cols() as usize, mb.cols() as usize);
    if r1.saturating_mul(c1).saturating_mul(c2) > crate::resource_limits::current().max_expand_terms
    {
        return None;
    }
    // `i < r1`, `j < c2` and `k < c1`, so both flat indices are within
    // `rows * cols` — in bounds by `Mat`'s invariant, with no length check of
    // our own to get right.
    let (ea, eb) = (ma.entries(), mb.entries());
    Some(Expr::Matrix(Mat::generate(ma.rows(), mb.cols(), |i, j| {
        let (i, j) = (i as usize, j as usize);
        add((0..c1)
            .map(|k| mul(vec![ea[i * c1 + k].clone(), eb[k * c2 + j].clone()]))
            .collect())
    })))
}
