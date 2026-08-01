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
/// complex-principal branch) and evaluate, `None` if non-finite.
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
    finite(eval_complex(&simplify_core(e), &Env::new())?)
}

/// Whether `e` contains a leaf that stands for "no value here" — a blank `＿` or
/// the `{"$":"None"}` special. Such a leaf poisons any constant it is part of.
fn has_undefined_leaf(e: &Expr) -> bool {
    e.any_subexpr(&|x| {
        matches!(x, Expr::Blank | Expr::Const(crate::expr::MathConst::None))
    })
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
        assert!(evaluate_to_constant(&p("1/0")).is_none()); // non-finite, unchanged
    }
}
