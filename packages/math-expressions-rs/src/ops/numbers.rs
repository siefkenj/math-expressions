//! Numeric folding and rounding passes over the expression tree:
//! `evaluate_numbers`, rational-fraction cancellation (`reduce_rational`), and
//! the display-rounding passes (`round_numbers_*`, `set_small_zero`,
//! `constants_to_floats`).

use crate::expr::map_children;
use crate::expr::Expr;
use crate::normalize::{canonicalize, present};
use crate::num::{Number, Spelling};
use std::collections::BTreeSet;

/// Fold numeric subexpressions (`4 + x − 2` → `x + 2`) — the port of
/// `me.evaluate_numbers`. Ours is the exact canonical fold: rationals stay
/// exact where the JS produces floats; the ordering is the canonical one.
///
/// **Numeric only.** The canonical layer also collects like terms, and running
/// it unmodified here made `x² + 3x²` come back as `4x²` — a correct
/// simplification, but not a *numeric* one. It is the whole content of
/// DoenetML's `simplify="numbers"`, which is specified as "fold numeric
/// constants, leave the symbolic structure alone"; with like terms collected
/// that attribute was indistinguishable from `simplify="full"`. So the sum
/// constructor runs with collection switched off — see
/// [`without_like_term_collection`](crate::normalize::without_like_term_collection)
/// for exactly what that does and does not suppress.
pub fn evaluate_numbers(e: &Expr) -> Expr {
    crate::normalize::without_like_term_collection(|| present(&canonicalize(e)))
}

/// [`evaluate_numbers`] plus the special-value folds, so a function applied to
/// a numeric argument evaluates: `sin(0) + 2` → `2`. This is the
/// `evaluate_functions` option of the JS `evaluate_numbers`, and what
/// DoenetML's `simplify="full"` needs.
///
/// Two passes are added on top of [`evaluate_numbers`]:
/// [`fold_special_values`](crate::normalize::fold_special_values) for the exact
/// identities (`sin(0) → 0`), then
/// [`fold_numeric_applications_approx`](crate::normalize::fold_numeric_applications_approx)
/// for the rest, which evaluates a function of numeric arguments to a float
/// when it has no exact value (`log(31) → 3.4339…`).
///
/// The float step is what "evaluate functions" means to the callers — `<round>`
/// asks for this precisely so it has a number to round — and it is why this
/// pass is *not* part of `simplify`, which must not trade an exact value for a
/// float. Nothing with a free variable is touched: every argument has to be a
/// number already, so `f(x)` is never "evaluated" at a guessed point.
///
/// Like-term collection stays suppressed for the same reason
/// [`evaluate_numbers`] suppresses it: the difference between this and plain
/// `evaluate_numbers` should be function evaluation and nothing else.
pub fn evaluate_numbers_evaluate_functions(e: &Expr) -> Expr {
    crate::normalize::without_like_term_collection(|| {
        let folded = crate::normalize::fold_special_values(e);
        present(&crate::normalize::fold_numeric_applications_approx(&folded))
    })
}

/// Cancel common polynomial factors in fractions — the port of
/// `me.reduce_rational` (`(x²−1)/(x−1)` → `x+1`, `(x²−5x+6)/(x²−4)` →
/// `(x−3)/(x+2)`, multivariate included). Applied bottom-up at every node;
/// non-polynomial fractions (`sin x / x`) are left unchanged. Backed by the
/// polynomial layer (recursive dense GCD over ℚ, bounded by resource limits).
pub fn reduce_rational(e: &Expr) -> Expr {
    let canon = canonicalize(e);
    // Bottom-up reduction, then re-canonicalize so in-place reductions merge
    // with their surroundings (`1 + (x²−1)/(x−1)` → `x + 2`).
    present(&canonicalize(&reduce_node(&canon)))
}

/// The spelling a value computed from `e` should read back with: `Decimal` as
/// soon as any number in `e` is decimal, `Fraction` otherwise. The polynomial
/// layer works in `BigRational`, which carries no spelling, so a pass that
/// round-trips through it has to restore one — otherwise
/// `(1.5x + 1.5)/(x + 1)` reduces to `3/2` instead of `1.5`.
fn spelling_of(e: &Expr) -> Spelling {
    match e {
        Expr::Num(n) => n.spelling(),
        _ => e
            .children()
            .into_iter()
            .fold(Spelling::Fraction, |acc, c| acc.join(spelling_of(c))),
    }
}

/// `e` with every number re-spelled. Sound only alongside [`spelling_of`],
/// which is why the two are used as a pair.
fn respell(e: &Expr, spelling: Spelling) -> Expr {
    map_numbers(e, &|n| n.with_spelling(spelling))
}

fn reduce_node(e: &Expr) -> Expr {
    let e = map_children(e, reduce_node);
    let Expr::Mul(factors) = &e else { return e };
    let spelling = spelling_of(&e);

    // Split canonical `Mul` factors into numerator parts and denominator
    // bases: a factor `Pow(b, −k)` (integer k>0) contributes `b^k` below.
    let mut num_parts: Vec<Expr> = Vec::new();
    let mut den_parts: Vec<Expr> = Vec::new();
    for f in factors {
        if let Expr::Pow(b, x) = f {
            if let Expr::Num(Number::Int(k)) = &**x {
                if *k < 0 {
                    den_parts.push(crate::normalize::pow(
                        (**b).clone(),
                        Expr::Num(Number::Int(-k)),
                    ));
                    continue;
                }
            }
        }
        num_parts.push(f.clone());
    }
    if den_parts.is_empty() {
        return e;
    }
    let num = crate::normalize::mul(num_parts);
    let den = crate::normalize::mul(den_parts);

    // Common variable list (order fixed by BTreeSet). Constant symbols are
    // rejected by the converter, so `pi/x` style fractions pass through.
    let mut vars = BTreeSet::new();
    collect_var_names(&num, &mut vars);
    collect_var_names(&den, &mut vars);
    let vars: Vec<String> = vars.into_iter().collect();
    if vars.is_empty() {
        return e; // pure numeric fraction — Number arithmetic already reduced it
    }

    let (Some(pn), Some(pd)) = (
        crate::polynomials::expr_to_poly(&num, &vars),
        crate::polynomials::expr_to_poly(&den, &vars),
    ) else {
        return e;
    };
    let Some(g) = crate::polynomials::gcd(&pn, &pd, vars.len()) else {
        return e;
    };
    if crate::polynomials::is_trivial(&g) {
        return e;
    }
    let (Some(qn), Some(qd)) = (
        crate::polynomials::exact_div_top(&pn, &g, vars.len()),
        crate::polynomials::exact_div_top(&pd, &g, vars.len()),
    ) else {
        return e;
    };
    // Normalize the quotients' rational content into a single scalar on the
    // numerator, so `(2x+4)/2` comes out as `x+2` rather than `½·(2x+4)`.
    let (cn, qn) = crate::polynomials::strip_rational_content(&qn);
    let (cd, qd) = crate::polynomials::strip_rational_content(&qd);
    let scalar = Expr::Num(Number::from_bigrational_spelled(cn / cd, spelling));
    let new_num = crate::normalize::mul(vec![
        scalar,
        respell(&crate::polynomials::poly_to_expr(&qn, &vars), spelling),
    ]);
    let new_den = respell(&crate::polynomials::poly_to_expr(&qd, &vars), spelling);
    canonicalize(&Expr::Div(Box::new(new_num), Box::new(new_den)))
}

/// Collect the free (non-constant) variable names of `e` into `out`.
fn collect_var_names(e: &Expr, out: &mut BTreeSet<String>) {
    if let Expr::Sym(s) = e {
        let name = s.name();
        if !crate::expr::sym::is_constant_symbol(&name) {
            out.insert(name);
        }
    }
    for c in e.children() {
        collect_var_names(c, out);
    }
}

/// Replace the constant symbols `pi` and `e` with their floating-point values
/// (`i` is left as the imaginary unit). Matches `me.constants_to_floats`.
pub fn constants_to_floats(e: &Expr) -> Expr {
    match e {
        Expr::Sym(s) => match s.name().as_str() {
            "pi" => Expr::Num(Number::from_f64(std::f64::consts::PI)),
            "e" => Expr::Num(Number::from_f64(std::f64::consts::E)),
            _ => e.clone(),
        },
        _ => map_children(e, constants_to_floats),
    }
}

/// Round every number in `e` to `decimals` decimal places (ties away from zero).
pub fn round_numbers_to_decimals(e: &Expr, decimals: i32) -> Expr {
    map_numbers(e, &|n| n.round_to_decimals(decimals))
}

/// `me.set_small_zero`: replace every number whose magnitude is `< tolerance`
/// with exact `0`. The float-noise cleanup applied after numeric evaluation
/// (default tolerance `1e-14` on the JS side — callers pass it explicitly).
pub fn set_small_zero(e: &Expr, tolerance: f64) -> Expr {
    let tol = tolerance.abs();
    map_numbers(e, &|n| {
        if n.to_f64().abs() < tol {
            Number::Int(0)
        } else {
            n.clone()
        }
    })
}

/// Round every number in `e` to `sig_figs` significant figures.
pub fn round_numbers_to_precision(e: &Expr, sig_figs: i32) -> Expr {
    map_numbers(e, &|n| {
        if sig_figs < 1 {
            return n.clone();
        }
        // Decimal place of the leading significant digit, then round so that
        // `sig_figs` digits survive. `magnitude_log10` is finite for every
        // nonzero value — including exact rationals outside f64 range like a
        // pasted `1e-400` or a 350-digit integer — and the i64 arithmetic +
        // saturating narrow avoid the i32 overflow those extremes caused.
        // (`round_to_decimals` clamps its argument again internally.)
        let Some(k) = n.magnitude_log10() else {
            return n.clone(); // zero / NaN
        };
        let d = (i64::from(sig_figs) - 1 - k).clamp(i64::from(i32::MIN), i64::from(i32::MAX));
        n.round_to_decimals(d as i32)
    })
}

/// Round every number to `digits` significant figures but never below
/// `decimals` decimal places — the port of
/// `me.round_numbers_to_precision_plus_decimals` (Doenet's display rounding:
/// "4 significant digits, at least 2 decimals"). Parameters are `f64` because
/// the JS callers pass `±Infinity` to disable one of the modes: `digits < 1`
/// (incl. `-Infinity`) → decimals-only; `digits > 15` (incl. `Infinity`) →
/// unchanged; non-finite `decimals` → precision-only.
pub fn round_numbers_to_precision_plus_decimals(e: &Expr, digits: f64, decimals: f64) -> Expr {
    let use_precision = digits >= 1.0;
    let sig_figs = digits.round();
    if use_precision && sig_figs > 15.0 {
        return e.clone();
    }
    let use_decimals = decimals.is_finite();
    // No need to go much beyond the limits of double precision (JS clamps ±330).
    let nd = decimals.round().clamp(-330.0, 330.0) as i64;

    match (use_precision, use_decimals) {
        (true, true) => map_numbers(e, &|n| {
            let Some(k) = n.magnitude_log10() else {
                return n.clone(); // zero / NaN
            };
            let d = (sig_figs as i64 - 1 - k)
                .max(nd)
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX));
            n.round_to_decimals(d as i32)
        }),
        (true, false) => round_numbers_to_precision(e, sig_figs as i32),
        (false, true) => round_numbers_to_decimals(e, nd as i32),
        (false, false) => e.clone(),
    }
}

/// Apply `f` to every `Num` leaf, recursing through the whole tree.
fn map_numbers(e: &Expr, f: &dyn Fn(&Number) -> Number) -> Expr {
    match e {
        Expr::Num(n) => Expr::Num(f(n)),
        _ => map_children(e, |c| map_numbers(c, f)),
    }
}
