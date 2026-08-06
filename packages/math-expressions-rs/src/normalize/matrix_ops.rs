//! Literal-matrix helpers used by the `mul`/`pow` smart constructors:
//! matrix-valued detection, the identity, and symbolic literal multiplication.

use super::{add, mul};
use crate::expr::{Expr, Mat, SeqKind};

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

/// Is this factor a coordinate vector — something a matrix *multiplies* rather
/// than something that scales it?
///
/// `M·(e,f)` used to partition `(e,f)` into `mul`'s scalar segment, which
/// distributed it into every entry and produced a matrix of `a·(e,f)`. A vector
/// is not a scalar; it belongs in the ordered segment beside the matrices, so
/// the product either contracts (in [`matvec_literal`], under `expand`) or
/// stays written as it was.
pub(crate) fn is_vector_valued(e: &Expr) -> bool {
    matches!(e, Expr::Seq(k, _)
        if matches!(k, SeqKind::Tuple | SeqKind::Array | SeqKind::Vector | SeqKind::AltVector))
}

/// `M · v` for a literal matrix and a coordinate vector: the contraction
/// `(Σ a₁ⱼ vⱼ, …)`, in **the vector's own container kind**, so a tuple comes
/// back a tuple and `⟨p,q⟩` comes back `⟨…⟩`.
///
/// `None` when the shapes do not conform (`v` is read as a column, so the
/// matrix must have as many columns as `v` has entries) or when the work would
/// exceed the expansion cap. A vector on the *left* is not handled at all: as a
/// column it is not conformable with a matrix on the right, and silently
/// transposing it would answer a question the author did not ask.
pub(crate) fn matvec_literal(m: &Expr, v: &Expr) -> Option<Expr> {
    let (Expr::Matrix(ma), Expr::Seq(kind, comps)) = (m, v) else {
        return None;
    };
    if ma.cols() as usize != comps.len() {
        return None;
    }
    let (rows, cols) = (ma.rows() as usize, ma.cols() as usize);
    if rows.saturating_mul(cols) > crate::resource_limits::current().max_expand_terms {
        return None;
    }
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut terms = Vec::with_capacity(cols);
        for c in 0..cols {
            terms.push(mul(vec![
                ma.get(r as u32, c as u32)?.clone(),
                comps[c].clone(),
            ]));
        }
        out.push(add(terms));
    }
    Some(Expr::Seq(*kind, out))
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
