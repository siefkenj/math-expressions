//! The elementary antiderivative table (Rubi cluster 1 + pervasive `a + b·x`
//! linear substitution): every row is `∫ g(u) dx = G(u)/b` for linear `u`.
//! The `Apply` arm delegates to `FnDef::antiderivative` in
//! [`crate::special_functions`]; the `Pow` arm handles powers, exponentials,
//! the `1/√(c − b·u²) → asin` row, and integer sin/cos powers (the last via
//! [`trig_power_integral`]'s power-reduction recursion).

use super::util::{apply, depends_on, int, linear_coeff, over};
use crate::expr::Expr;
use crate::normalize::{add, mul, pow};
use crate::num::Number;

/// `∫ fname(u)^n dx` for `n ≥ 0` and `u = a + b·x` linear, via the standard
/// power-reduction recursion. `fname` is `"sin"` or `"cos"`. The result is
/// gate-verified by the caller, so this only needs to be correct, not canonical.
fn trig_power_integral(fname: &str, u: &Expr, b: &Expr, n: i64, x: &str) -> Expr {
    // ∫ f(u)^0 dx = ∫ 1 dx = x.
    if n == 0 {
        return Expr::sym(x);
    }
    // ∫ sin(u) dx = −cos(u)/b ;  ∫ cos(u) dx = sin(u)/b.
    if n == 1 {
        return match fname {
            "sin" => over(mul(vec![int(-1), apply("cos", u.clone())]), b),
            _ => over(apply("sin", u.clone()), b),
        };
    }
    // Boundary term ∓ f(u)^(n−1)·g(u)/(n·b): cofunction `g` and sign differ for
    // sin (−, g=cos) vs cos (+, g=sin).
    let (cofn, sign): (&str, i64) = if fname == "sin" { ("cos", -1) } else { ("sin", 1) };
    let boundary = over(
        mul(vec![
            int(sign),
            pow(apply(fname, u.clone()), int(n - 1)),
            apply(cofn, u.clone()),
            pow(int(n), int(-1)),
        ]),
        b,
    );
    let recursive = mul(vec![
        Expr::Num(Number::rat(n - 1, n)),
        trig_power_integral(fname, u, b, n - 2, x),
    ]);
    add(vec![boundary, recursive])
}

/// The elementary table (Rubi cluster 1 + pervasive `a + b·x` linear
/// substitution): every row is `∫ g(u) dx = G(u)/b` for linear `u`.
pub(super) fn table_match(e: &Expr, x: &str) -> Option<Expr> {
    match e {
        // (a+bx)^n and c^(a+bx).
        Expr::Pow(base0, exp0) => {
            // `sqrt(w)^k` is `w^(k/2)`: unify so the power rows see one
            // spelling (canonical form keeps sqrt as an application).
            let (base, exp): (Expr, Expr) = match (&**base0, &**exp0) {
                (Expr::Apply(h, args), Expr::Num(n))
                    if matches!(&**h, Expr::Sym(s) if s.name() == "sqrt")
                        && args.len() == 1 =>
                {
                    (args[0].clone(), Expr::Num(n.mul(&Number::rat(1, 2))))
                }
                _ => ((**base0).clone(), (**exp0).clone()),
            };
            let (base, exp) = (&base, &exp);
            // Power of a linear argument with an x-free exponent.
            if let Some(b) = linear_coeff(base, x) {
                if !depends_on(exp, x) {
                    if matches!(exp, Expr::Num(n) if n.to_f64() == -1.0) {
                        return Some(over(apply("log", base.clone()), &b));
                    }
                    // u^n → u^(n+1)/(n+1): exponent must be a number ≠ −1.
                    if let Expr::Num(n) = exp {
                        let n1 = n.add(&Number::Int(1));
                        if !n1.is_zero() {
                            let f = mul(vec![
                                pow(base.clone(), Expr::Num(n1.clone())),
                                pow(Expr::Num(n1), int(-1)),
                            ]);
                            return Some(over(f, &b));
                        }
                    }
                }
            }
            // Exponential: c^u, x-free base.
            if !depends_on(base, x) {
                if let Some(b) = linear_coeff(exp, x) {
                    let is_e = matches!(base, Expr::Const(crate::expr::MathConst::E))
                        || matches!(base, Expr::Sym(s) if s.name() == "e");
                    if is_e {
                        return Some(over(e.clone(), &b));
                    }
                    if matches!(base, Expr::Num(n) if n.is_positive() && !n.is_one()) {
                        let f = mul(vec![
                            e.clone(),
                            pow(apply("log", base.clone()), int(-1)),
                        ]);
                        return Some(over(f, &b));
                    }
                }
            }
            // 1/√(c − b·u²) → asin(u·√(b/c))/√b (the inverse-trig table row
            // in its canonical Pow clothing).
            if matches!(exp, Expr::Num(n) if n.to_f64() == -0.5) {
                if let Some((c, b_coef, u, ub)) = concave_quadratic(base, x) {
                    let ratio = &b_coef / &c;
                    let s = super::rational::sqrt_expr(&ratio);
                    let inv_sqrt_b = pow(super::rational::sqrt_expr(&b_coef), int(-1));
                    let f = mul(vec![
                        inv_sqrt_b,
                        apply("asin", mul(vec![u, s])),
                    ]);
                    return Some(over(f, &ub));
                }
            }
            // sec²/csc² in canonical clothing: cos(u)^(−2), sin(u)^(−2).
            if let (Expr::Apply(h, args), Expr::Num(Number::Int(-2))) = (base, exp) {
                if let (Expr::Sym(f), [u]) = (&**h, args.as_slice()) {
                    if let Some(b) = linear_coeff(u, x) {
                        match f.name().as_str() {
                            "cos" => return Some(over(apply("tan", u.clone()), &b)),
                            "sin" => {
                                let cot = mul(vec![
                                    int(-1),
                                    apply("cos", u.clone()),
                                    pow(apply("sin", u.clone()), int(-1)),
                                ]);
                                return Some(over(cot, &b));
                            }
                            _ => {}
                        }
                    }
                }
            }
            // Positive integer powers of sin/cos with a linear argument, by the
            // reduction  ∫sinⁿ(u) dx = −sinⁿ⁻¹(u)·cos(u)/(n·b) + (n−1)/n·∫sinⁿ⁻²(u) dx
            // (and the sign-flipped cos analogue), bottoming out at ∫1 = x and
            // ∫sin(u) dx = −cos(u)/b. The `∫sin²x` case is why an un-simplified
            // `sin²x + cos²x` used to fail entirely.
            if let (Expr::Apply(h, args), Expr::Num(Number::Int(n))) = (base, exp) {
                // Bounded: the reduction expands to ~n/2 terms in one shot
                // (outside the step-fuel loop), so refuse absurd exponents rather
                // than build a huge tree. 16 covers every realistic case.
                if (2..=16).contains(n) {
                    if let (Expr::Sym(f), [u]) = (&**h, args.as_slice()) {
                        let name = f.name();
                        if name == "sin" || name == "cos" {
                            if let Some(b) = linear_coeff(u, x) {
                                return Some(trig_power_integral(&name, u, &b, *n, x));
                            }
                        }
                    }
                }
            }
            None
        }
        Expr::Apply(head, args) => {
            // The elementary antiderivative table is `FnDef::antiderivative`
            // in `crate::special_functions` (alias-aware: `arctan` finds `atan`).
            let (Expr::Sym(f), [u]) = (&**head, args.as_slice()) else {
                return None;
            };
            let builder = crate::special_functions::antiderivative_builder(&f.name())?;
            let b = linear_coeff(u, x)?;
            Some(over(builder(u.clone()), &b))
        }
        _ => None,
    }
}

/// Match `c − b·w²` with rational `c, b > 0` and `w` linear in `x`.
/// Returns `(c, b, w, linear coefficient of w)`.
fn concave_quadratic(
    base: &Expr,
    x: &str,
) -> Option<(
    num_rational::BigRational,
    num_rational::BigRational,
    Expr,
    Expr,
)> {
    use num_rational::BigRational;
    use num_traits::{One, Signed};
    let Expr::Add(ts) = base else { return None };
    let mut c: Option<BigRational> = None;
    let mut quad: Option<(BigRational, Expr)> = None;
    for t in ts {
        match t {
            Expr::Num(n) => {
                if c.is_some() {
                    return None;
                }
                c = n.to_bigrational();
            }
            // A bare `w²` term: coefficient +1. (Rejected below by the
            // `a < 0` check — `c + w²` is convex, not the `c − b·w²` shape —
            // but recorded here so a *second* quadratic term is still caught.)
            Expr::Pow(w, k) if matches!(&**k, Expr::Num(Number::Int(2))) => {
                if quad.is_some() {
                    return None;
                }
                quad = Some((BigRational::one(), (**w).clone()));
            }
            Expr::Mul(fs) => {
                let mut coeff: Option<BigRational> = None;
                let mut w: Option<Expr> = None;
                for f in fs {
                    match f {
                        Expr::Num(n) => coeff = n.to_bigrational(),
                        Expr::Pow(b, k) if matches!(&**k, Expr::Num(Number::Int(2))) => {
                            w = Some((**b).clone())
                        }
                        _ => return None,
                    }
                }
                if quad.is_some() {
                    return None;
                }
                quad = Some((coeff?, w?));
            }
            _ => return None,
        }
    }
    let (a, w) = quad?;
    let c = c?;
    // c − b·w²: need c > 0, a < 0.
    if !c.is_positive() || !a.is_negative() {
        return None;
    }
    let ub = linear_coeff(&w, x)?;
    Some((c, -a, w, ub))
}
