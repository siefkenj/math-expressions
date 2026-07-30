//! Builder shorthand shared by the antiderivative closures and `eval1`
//! definitions in the family files. `int`/`apply` are the same helpers the
//! integrator uses, so the built antiderivative shapes are identical.

use crate::expr::Expr;
use num_complex::Complex64;

pub(crate) fn int(i: i64) -> Expr {
    Expr::Num(crate::num::Number::Int(i))
}

pub(crate) fn apply(name: &str, arg: Expr) -> Expr {
    Expr::Apply(Box::new(Expr::sym(name)), vec![arg])
}

/// Apply a real function to a (near-)real complex value, else `None`.
/// Shared by the `eval1` closures of real-only functions (floor/ceil/…).
pub(crate) fn real_only(z: Complex64, f: fn(f64) -> f64) -> Option<Complex64> {
    if z.im.abs() < 1e-9 {
        Some(Complex64::new(f(z.re), 0.0))
    } else {
        None
    }
}
