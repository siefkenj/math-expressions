//! Numeric evaluation of an expression at variable bindings, port of
//! `me.evaluate` / `me.evaluate_to_constant`.

use crate::eval_numeric::complex::{eval_complex, Env};
use crate::expr::Expr;
use crate::normalize::simplify_core;
use crate::ops::variables;
use num_complex::Complex64;
use std::collections::HashMap;

/// Evaluate `e` at real variable bindings, returning its (possibly complex)
/// numeric value. `None` if a needed variable is unbound, the expression is not
/// numerically meaningful, or the result is non-finite (`me.evaluate` returns
/// `null` for e.g. `1/0`). Uses the complex principal branch, matching mathjs:
/// `x^(1/3)` at `x = -8` is `1 + i√3`, not the real root `-2`.
pub fn evaluate(e: &Expr, bindings: &HashMap<String, f64>) -> Option<Complex64> {
    let env: Env = bindings
        .iter()
        .map(|(k, v)| (k.clone(), Complex64::new(*v, 0.0)))
        .collect();
    finite(eval_complex(e, &env)?)
}

/// Evaluate a closed expression to its numeric constant, or `None`. Matches
/// `me.evaluate_to_constant`: `None` if the *original* expression mentions any
/// genuine free variable (the constants `pi`/`e`/`i` don't count, and it does
/// NOT cancel first — so `x − x` is `None`, not `0`); otherwise simplify (real-
/// domain reductions apply, `(-8)^(1/3)` → `-2` — contrast [`evaluate`]'s
/// complex-principal branch) and evaluate.
///
/// `±∞` is a value here, not a failure: an unbounded interval endpoint is
/// ordinary (`[-∞, ∞]` is the default domain of a function curve), and
/// reporting `None` for one reads downstream as `0`, silently turning an
/// unbounded endpoint into a bounded one (DOENET_INTEGRATION item 1). Anything
/// still undecided — a `NaN`, an unevaluable head — remains `None`.
pub fn evaluate_to_constant(e: &Expr) -> Option<Complex64> {
    if variables(e)
        .iter()
        .any(|v| !crate::expr::sym::is_constant_symbol(v))
    {
        return None;
    }
    // A hole (`＿`) or a `{"$":"None"}` leaf makes the value undefined, exactly
    // like a free variable does — you cannot evaluate an expression with an
    // unfilled slot to a constant. This must be checked on the *original* tree,
    // before `simplify_core`, because simplification absorbs the hole and hides
    // it: `0·＿` folds to `0` and `＿/＿` to `1`, so an undefined line's slope
    // would come back `0`/`1` instead of undefined (DoenetML issue #83, item 3b).
    if has_undefined_leaf(e) {
        return None;
    }
    let simplified = simplify_core(e);
    if let Some(v) = signed_infinity(&simplified) {
        return Some(Complex64::new(v, 0.0));
    }
    finite(eval_complex(&simplified, &Env::new())?)
}

/// `±∞` when `e` *is* an infinite constant, after simplification has done the
/// infinity arithmetic (`∞ + 1`, `2·∞`, `1/∞`) symbolically.
///
/// Read here rather than in [`eval_complex`] on purpose. That evaluator is the
/// equality sampler's, where a sampled `∞` would poison a difference into
/// `NaN` and turn "cannot decide" into a confident wrong answer; it declines
/// the infinite constants for that reason and keeps doing so. The constant
/// itself is not ambiguous, and this function's contract is the *value*.
fn signed_infinity(e: &Expr) -> Option<f64> {
    match e {
        Expr::Const(crate::expr::MathConst::Inf) => Some(f64::INFINITY),
        Expr::Const(crate::expr::MathConst::NegInf) => Some(f64::NEG_INFINITY),
        Expr::Neg(x) => signed_infinity(x).map(|v| -v),
        _ => None,
    }
}

/// Whether `e` contains a leaf that stands for "no value here" — a blank `＿` or
/// the `{"$":"None"}` special. Such a leaf poisons any constant it is part of.
fn has_undefined_leaf(e: &Expr) -> bool {
    e.any_subexpr(&|x| matches!(x, Expr::Blank | Expr::Const(crate::expr::MathConst::None)))
}

fn finite(v: Complex64) -> Option<Complex64> {
    (v.re.is_finite() && v.im.is_finite()).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextToAst;
    fn p(s: &str) -> Expr {
        TextToAst::new(Default::default()).convert(s).unwrap()
    }

    /// An expression with an unfilled hole evaluates to undefined, not a number
    /// simplification happened to produce (item 3b: a no-argument line's slope
    /// was `0`/`1` instead of NaN).
    #[test]
    fn undefined_leaves_do_not_evaluate_to_a_constant() {
        assert_eq!(evaluate_to_constant(&p("_")), None);
        assert_eq!(evaluate_to_constant(&p("0*_")), None); // was Some(0)
        assert_eq!(evaluate_to_constant(&p("(_-_)/(_-_)")), None); // was 1
        assert_eq!(evaluate_to_constant(&p("_+3")), None);
    }

    /// The guard is specific to holes — ordinary constants still evaluate.
    #[test]
    fn ordinary_constants_still_evaluate() {
        assert_eq!(evaluate_to_constant(&p("2+3")).unwrap().re, 5.0);
        assert_eq!(evaluate_to_constant(&p("cos(0)")).unwrap().re, 1.0);
    }

    /// An infinite endpoint reports as `±∞`, not as "no value" — the caller
    /// cannot tell those apart, and `null` reads as `0` to `Math.max`
    /// (DOENET_INTEGRATION item 1).
    #[test]
    fn infinity_is_a_value_not_a_failure() {
        assert_eq!(
            evaluate_to_constant(&p("Infinity")).unwrap().re,
            f64::INFINITY
        );
        assert_eq!(
            evaluate_to_constant(&p("-Infinity")).unwrap().re,
            f64::NEG_INFINITY
        );
        // Reached through the arithmetic simplify does on infinities, too.
        assert_eq!(
            evaluate_to_constant(&p("Infinity+1")).unwrap().re,
            f64::INFINITY
        );
        assert_eq!(evaluate_to_constant(&p("1/0")).unwrap().re, f64::INFINITY);
        assert_eq!(evaluate_to_constant(&p("1/Infinity")).unwrap().re, 0.0);
    }

    /// What stays `None` is what is genuinely undecided — an indeterminate
    /// form is not a number, and must not come back as one.
    #[test]
    fn indeterminate_forms_still_decline() {
        assert_eq!(evaluate_to_constant(&p("Infinity-Infinity")), None);
        assert_eq!(evaluate_to_constant(&p("0/0")), None);
        assert_eq!(evaluate_to_constant(&p("NaN")), None);
    }
}
