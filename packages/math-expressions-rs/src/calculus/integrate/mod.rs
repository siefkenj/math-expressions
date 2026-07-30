//! Symbolic (indefinite) integration.
//!
//! Engine order: linearity / constant-slide → the complete rational
//! engine ([`rational`]) → the elementary table with linear inner arguments
//! ([`table`]) → derivative-divides u-substitution ([`usub`], which re-enters
//! the whole pipeline on the substituted integrand). All recursion shares one
//! fuel budget (`max_integration_steps`), and every top-level success must pass
//! the gate `equals(derivative(F, x), f)` — a wrong answer is discarded,
//! never returned.
//!
//! This module is the barrel + pipeline driver: [`integrate`] (public entry
//! and simplify-retry), [`integrate_verified`] (the differentiation gate), and
//! [`integ`] (the stage dispatcher). Shared helpers and the linear-coefficient
//! extractor live in [`util`].

pub(crate) mod rational;

mod table;
mod usub;
mod util;

use crate::assumptions::Assumptions;
use crate::expr::Expr;
use crate::normalize::{add, canonicalize, mul};
use util::depends_on;

/// One antiderivative of `f` with respect to `x` (no `+ C` — the caller's
/// concern, as in Rubi/mathjs). `None` is the honest "no elementary form
/// found within budget" — the caller can still integrate numerically via
/// `integrate_to_precision`.
pub fn integrate(f: &Expr, x: &str, assumptions: &Assumptions) -> Option<Expr> {
    let fc = canonicalize(f);
    if let Some(res) = integrate_verified(&fc, x, assumptions) {
        return Some(res);
    }
    // Retry on a heuristically simplified integrand. `canonicalize` is
    // assumption-free and does no trig/log identities, so a sum like
    // `sin^2 x + cos^2 x + 1` reaches `integ` as an unintegrable term-by-term
    // split even though it collapses to the constant `2`. `simplify` applies the
    // identity layer; if it actually changed the shape, integrating the result
    // can succeed where the raw form could not. This runs only on the failure
    // path, so the common case pays nothing.
    let fs = crate::normalize::simplify(&fc);
    if fs != fc {
        return integrate_verified(&fs, x, assumptions);
    }
    None
}

/// Integrate an already-canonical (or simplified) `fc` and gate the result by
/// differentiation. Returns `None` if no antiderivative is found OR the gate
/// rejects the candidate.
fn integrate_verified(fc: &Expr, x: &str, assumptions: &Assumptions) -> Option<Expr> {
    let mut fuel = crate::resource_limits::current().max_integration_steps;
    let result = integ(fc, x, &mut fuel)?;
    // The gate (plan §2c): verify by differentiation. Accept iff the sampled
    // `equals` OR the certified exact stages (FULL_SIMPLIFY S1: structural
    // cancellation, exact constants, rational normal form) confirm
    // `F' - f ≡ 0`. `equals` runs first because it is cheap and accepts almost
    // every correct candidate; the certified pass then *rescues* sound
    // antiderivatives that sampling wrongly rejects (tolerance/domain
    // artifacts). The disjunction is order-independent, so this costs the old
    // gate's time on the accept path. `is_zero`'s sampling-refuter stage is
    // deliberately not used here: its certified reject duplicates the `equals`
    // reject, and on true zeros it burns its full arbitrary-precision budget
    // before returning Unknown (measured ~35× suite slowdown).
    let df = crate::calculus::diff::derivative(&result, x);
    if !crate::equality::equals(&df, fc, &crate::equality::EqOptions::default()) {
        let residual = Expr::Add(vec![df, Expr::Neg(Box::new(fc.clone()))]);
        if !crate::eval_exact::certified_zero(&residual, assumptions) {
            return None;
        }
    }
    Some(crate::normalize::simplify(&result))
}

/// The stage dispatcher: fuel-count, x-free constant, `Add` linearity, `Mul`
/// coefficient slide, then the rational engine → elementary table → u-sub.
/// `pub(super)` because [`usub`] recurses back into it with the shared fuel.
pub(super) fn integ(e: &Expr, x: &str, fuel: &mut i64) -> Option<Expr> {
    *fuel -= 1;
    if *fuel < 0 {
        return None;
    }
    let xs = Expr::sym(x);
    // ∫ c dx = c·x.
    if !depends_on(e, x) {
        return Some(mul(vec![e.clone(), xs]));
    }
    // Linearity.
    if let Expr::Add(ts) = e {
        let parts: Option<Vec<Expr>> = ts.iter().map(|t| integ(t, x, fuel)).collect();
        return parts.map(add);
    }
    // Slide the x-free coefficient out of a product.
    if let Expr::Mul(fs) = e {
        let (coeff, core): (Vec<Expr>, Vec<Expr>) =
            fs.iter().cloned().partition(|f| !depends_on(f, x));
        if !coeff.is_empty() && !core.is_empty() {
            let inner = integ(&mul(core), x, fuel)?;
            return Some(mul(vec![mul(coeff), inner]));
        }
    }
    // The complete rational engine (I1).
    if let Some((n, d)) = rational::expr_to_ratfun(e, x) {
        if let Some(res) = rational::integrate_rational(&n, &d, x) {
            return Some(res);
        }
    }
    // Elementary table with linear inner arguments (I2).
    if let Some(res) = table::table_match(e, x) {
        return Some(res);
    }
    // Derivative-divides u-substitution (I2), re-entering the pipeline.
    usub::usub(e, x, fuel)
}
