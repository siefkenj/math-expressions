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

/// Evaluate `e` at many values of a single variable, in one pass.
///
/// Sampling a function — plotting it, bracketing its extrema, hunting a root —
/// asks for the same expression at thousands of points, and [`evaluate`] is the
/// wrong shape for that: it rebuilds the environment per point, and across the
/// wasm boundary each call also marshals the variable names. Measured on
/// `x²−3x+1`, that overhead is ~1.2µs a point against ~6ns of actual
/// arithmetic. Here it is paid once.
///
/// Other variables are left unbound; substitute them first if the expression
/// has any. A point that does not evaluate to a finite real — unbound variable,
/// pole, complex value, outside a real branch — comes back as `NaN` rather than
/// being dropped, because a sampler wants one result per point it asked about
/// and `NaN` is the gap marker its consumers already handle.
pub fn evaluate_many(e: &Expr, var: &str, values: &[f64]) -> Vec<f64> {
    let mut env = Env::new();
    // The binding's key never changes, so insert it once and overwrite its
    // value each point — the loop this feature exists to speed up should not
    // allocate a fresh `String` per point.
    env.insert(var.to_string(), Complex64::new(0.0, 0.0));
    values
        .iter()
        .map(|&x| {
            *env.get_mut(var).expect("inserted above") = Complex64::new(x, 0.0);
            match eval_complex(e, &env).and_then(finite) {
                // The imaginary part is compared against the real one's scale,
                // the same tolerance the single-point wasm entry point applies.
                Some(v) if v.im.abs() <= 1e-10 * v.re.abs().max(1.0) => v.re,
                _ => f64::NAN,
            }
        })
        .collect()
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
    if is_nan_constant(&simplified) {
        return Some(Complex64::new(f64::NAN, 0.0));
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

/// Whether simplification *proved* the value is NaN (`0/0`, `∞ − ∞`).
///
/// This is a value, not a failure to decide, and is reported as one for the
/// same reason [`signed_infinity`] is: `None` crosses to JS as `null`, and
/// `Math.abs(null)` is `0`, so an undefined intercept would read as the origin
/// — a real point — instead of as no value at all.
///
/// Read from the *simplified tree*, deliberately, and not by relaxing
/// [`finite`]. A NaN that simplification derived is a conclusion; a NaN that
/// falls out of [`eval_complex`] may only mean the sampler could not evaluate
/// there, and returning that as a value would turn "cannot decide" into a
/// confident wrong answer. The undecidable cases — free variables, and the
/// holes rejected by [`has_undefined_leaf`] — still return `None`.
fn is_nan_constant(e: &Expr) -> bool {
    match e {
        Expr::Const(crate::expr::MathConst::NaN) => true,
        Expr::Neg(x) => is_nan_constant(x),
        _ => false,
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

    /// An indeterminate form *evaluates* — to NaN — and is reported as that
    /// value, for the same reason `±∞` is: `None` crosses to JS as `null`, and
    /// `Math.abs(null)` is `0`, so an undefined result would read as a real
    /// point at the origin rather than as no value.
    #[test]
    fn indeterminate_forms_evaluate_to_nan() {
        // No text spelling for NaN — `"NaN"` parses as the product `N·a·N` —
        // so these are reached through the arithmetic, which is how DoenetML
        // reaches them too.
        for s in [
            "0/0",
            "Infinity-Infinity",
            "-Infinity+Infinity",
            "0*Infinity",
            "Infinity/Infinity",
            "0^0",
            "Infinity^0",
            "1^Infinity",
        ] {
            assert!(
                evaluate_to_constant(&p(s)).is_some_and(|c| c.re.is_nan()),
                "{s} should evaluate to NaN, got {:?}",
                evaluate_to_constant(&p(s)).map(|c| c.re)
            );
        }
    }

    /// What stays `None` is what is genuinely *undecided* rather than computed:
    /// a free variable, and the holes `has_undefined_leaf` rejects. This is the
    /// line [`is_nan_constant`] must not cross.
    #[test]
    fn undecidable_values_still_decline() {
        assert_eq!(evaluate_to_constant(&p("x")), None);
        assert_eq!(evaluate_to_constant(&p("x-x+1")), None);
    }
}
