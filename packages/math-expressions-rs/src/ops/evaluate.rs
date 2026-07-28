//! Numeric evaluation of an expression at variable bindings, port of
//! `me.evaluate` / `me.evaluate_to_constant`.

use crate::eval_numerical::{eval_complex, Env};
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
        .any(|v| !crate::sym::is_constant_symbol(v))
    {
        return None;
    }
    finite(eval_complex(&simplify_core(e), &Env::new())?)
}

fn finite(v: Complex64) -> Option<Complex64> {
    (v.re.is_finite() && v.im.is_finite()).then_some(v)
}
