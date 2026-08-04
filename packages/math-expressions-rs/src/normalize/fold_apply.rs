//! Fold numeric function applications: `floor(55.33) → 55`,
//! `sum(3, 17, 5−4) → 21`, `log10(10³) → 3`.
//!
//! Like [`fold_special_values`](super::fold_special_values), this is a pass in
//! its own right rather than one of the base rewrite clusters in `simplify`:
//! the base rounds never run it, and the public `simplify` reaches it only
//! through the `full_simplify` fixpoint driver. That keeps `equals` — which
//! goes through `simplify_canonical` — byte-stable.
//!
//! # Only exact folds
//!
//! An application folds only when every argument is an exact rational *and*
//! the result is one too; [`FnDef::fold_exact`] decides, returning `None` to
//! leave the node alone. So `floor(55.33)` and `mean(1,2,4)` fold (to `55` and
//! `7/3`) while `sqrt(2)`, `log10(3)` and `asin(1)` stay symbolic.
//!
//! The legacy library reached similar answers by a different route: evaluate
//! in floating point, then try to *recover* a fraction from the result. That
//! is why its `log(1000, 10)` is `2.9999999999999996` while its `log10(1000)`
//! is `3` — the difference being only that V8 has a dedicated `Math.log10`.
//! Deciding exactness instead of estimating it removes that whole class of
//! answer, and is why `log_10(1000)` folds here too.
//!
//! [`FnDef::fold_exact`]: crate::special_functions::FnDef::fold_exact

use crate::expr::{Expr, map_children};
use crate::num::Number;
use crate::special_functions::fold_exact;
use num_rational::BigRational;

/// Fold every numeric application in `e` that has an exact value. Bottom-up,
/// so an inner fold feeds the one above it (`abs(floor(-2.5)) → 3`).
///
/// The input is canonicalized first and the output is canonical, matching
/// [`fold_special_values`](super::fold_special_values). That is not just
/// tidiness: the exactness gate below only recognizes a `Num` leaf, so
/// `sum(3, 17, 5−4)` and `log₂(1/8)` fold only once `5−4` and `1/8` have
/// become single numbers.
pub fn fold_numeric_applications(e: &Expr) -> Expr {
    fold_nodes(&super::canonicalize(e))
}

fn fold_nodes(e: &Expr) -> Expr {
    let e = map_children(e, fold_nodes);
    let Expr::Apply(head, args) = &e else {
        return e;
    };
    fold_application(head, args).map_or(e, Expr::Num)
}

fn fold_application(head: &Expr, args: &[Expr]) -> Option<Number> {
    // Nothing folds unless every argument is already a number.
    let numbers: Vec<&Number> = args
        .iter()
        .map(|a| match a {
            Expr::Num(n) => Some(n),
            _ => None,
        })
        .collect::<Option<_>>()?;

    // `to_bigrational` is the exactness gate: it returns `None` for
    // `Number::Float`. When every argument clears it we are in exact
    // territory and only an exact result is acceptable.
    match numbers.iter().map(|n| n.to_bigrational()).collect() {
        Some(rationals) => fold_exactly(head, rationals),
        // An argument that is *already* a float — `floor(55.33)` arriving
        // through the JSON tree, where a non-integer literal is an f64. The
        // value is inexact before we touch it, so folding cannot lose
        // exactness and the float evaluator decides. This is legacy's
        // "contains a decimal" escape hatch, and it is why `asin(0.5)` folds
        // to a number while `asin(1)` stays symbolic in both libraries.
        None => fold_approximately(head, args),
    }
}

fn fold_exactly(head: &Expr, rationals: Vec<BigRational>) -> Option<Number> {
    let value = match head {
        Expr::Sym(s) => fold_exact(&s.name())?(&rationals)?,
        // `log_b(x)` — the parsers spell a based logarithm as an `Index` head
        // rather than a two-argument apply.
        Expr::Index(f, base) => {
            let (Expr::Sym(f), Expr::Num(base)) = (&**f, &**base) else {
                return None;
            };
            let (name, [x]) = (f.name(), rationals.as_slice()) else {
                return None;
            };
            if name != "log" {
                return None;
            }
            crate::special_functions::exp_log::exact_log(x, &base.to_bigrational()?)?
        }
        _ => return None,
    };
    Some(Number::from_bigrational(value))
}

/// Fold through the float evaluator, for an application that already holds an
/// inexact argument. Only a finite *real* value is accepted — a complex result
/// (`sqrt(-4.5)`) leaves the application as written rather than silently
/// dropping an imaginary part.
fn fold_approximately(head: &Expr, args: &[Expr]) -> Option<Number> {
    let node = Expr::Apply(Box::new(head.clone()), args.to_vec());
    let z = crate::eval_numeric::complex::eval_complex(&node, &Default::default())?;
    (z.im == 0.0 && z.re.is_finite()).then(|| Number::from_f64(z.re))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextToAst;

    /// Fold `s` and print the JS tree spelling, so expectations read the way
    /// the legacy oracle reports them.
    fn run(s: &str) -> String {
        let e = TextToAst::new(Default::default()).convert(s).unwrap();
        crate::expr::serde::to_js(&fold_numeric_applications(&e)).to_string()
    }

    /// Folding a tree built directly, which is how the aggregates arrive —
    /// they have no parser spelling (see `special_functions::aggregate`).
    fn run_js(json: &str) -> String {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        let e = crate::expr::serde::try_from_js(&v).unwrap();
        crate::expr::serde::to_js(&fold_numeric_applications(&e)).to_string()
    }

    #[test]
    fn rounding_and_magnitude_functions_fold_exactly() {
        // 55.33 is an exact rational (5533/100), never a float, so this is
        // decided rather than rounded.
        assert_eq!(run("floor(55.33)"), "55");
        assert_eq!(run("ceil(2.1)"), "3");
        assert_eq!(run("floor(-2.5)"), "-3");
        assert_eq!(run("ceil(-2.5)"), "-2");
        assert_eq!(run("abs(-3)"), "3");
        assert_eq!(run("abs(-3.5)"), "3.5");
        assert_eq!(run("sign(-4)"), "-1");
        assert_eq!(run("mod(7,3)"), "1");
    }

    #[test]
    fn logarithms_fold_only_on_exact_powers_of_the_base() {
        assert_eq!(run("log10(1000)"), "3");
        assert_eq!(run("log10(100000)"), "5");
        assert_eq!(run_js(r#"["apply","log2",8]"#), "3");
        assert_eq!(run_js(r#"["apply","log2",1024]"#), "10");
        // Negative exponents work the same way.
        assert_eq!(run_js(r#"["apply","log2",["/",1,8]]"#), "-3");
        // A based logarithm reaches the same helper through an `Index` head —
        // the shape where legacy returned 2.9999999999999996.
        assert_eq!(run("log_10(1000)"), "3");
        assert_eq!(run("log_2(8)"), "3");
        assert_eq!(run("log_7(343)"), "3");
        // Not a power of the base: left symbolic, not turned into a float.
        assert_eq!(run("log10(3)"), r#"["apply","log10",3]"#);
        assert_eq!(run("log_2(9)"), r#"["apply",["_","log",2],9]"#);
    }

    #[test]
    fn combinatorics_fold_exactly() {
        assert_eq!(run("nCr(5,3)"), "10");
        assert_eq!(run("nPr(5,3)"), "60");
        // Well past f64's exact-integer range: the float rule would lose
        // digits here, the exact one does not.
        assert_eq!(run("nCr(60,30)"), "118264581564861424");
    }

    #[test]
    fn aggregates_fold_over_their_whole_argument_list() {
        assert_eq!(run_js(r#"["apply","sum",["tuple",3,17,["+",5,-4]]]"#), "21");
        assert_eq!(run_js(r#"["apply","prod",["tuple",2,3,4]]"#), "24");
        assert_eq!(run_js(r#"["apply","mean",["tuple",1,2,3]]"#), "2");
        assert_eq!(run_js(r#"["apply","mean",["tuple",1,2,4]]"#), r#"["/",7,3]"#);
        // `5/2` prints as the terminating decimal `2.5` — the engine-wide
        // convention (legacy spells it `["/",5,2]`), not a property of this
        // fold. `7/3` above stays a fraction because it has no finite decimal.
        assert_eq!(run_js(r#"["apply","median",["tuple",1,2,3,4]]"#), "2.5");
        assert_eq!(run_js(r#"["apply","variance",["tuple",1,2,3]]"#), "1");
        assert_eq!(run_js(r#"["apply","std",["tuple",1,2,3]]"#), "1");
        assert_eq!(run_js(r#"["apply","count",["tuple",1,2,3]]"#), "3");
        assert_eq!(run_js(r#"["apply","max",["tuple",1,5,3]]"#), "5");
        assert_eq!(run_js(r#"["apply","min",["tuple",1,5,3]]"#), "1");
        assert_eq!(run_js(r#"["apply","sum",3]"#), "3");
    }

    /// The whole point of the exactness gate: an irrational value keeps its
    /// symbolic form rather than collapsing to a float.
    #[test]
    fn irrational_and_symbolic_applications_are_left_alone() {
        assert_eq!(run("log10(3)"), r#"["apply","log10",3]"#);
        assert_eq!(run("asin(1)"), r#"["apply","asin",1]"#);
        assert_eq!(run("floor(x)"), r#"["apply","floor","x"]"#);
        assert_eq!(
            run_js(r#"["apply","std",["tuple",1,2,4]]"#),
            r#"["apply","std",["tuple",1,2,4]]"#
        );
        assert_eq!(
            run_js(r#"["apply","sum",["tuple","x","y"]]"#),
            r#"["apply","sum",["tuple","x","y"]]"#
        );
        // An unknown name has no folder and is untouched.
        assert_eq!(run("g(2)"), r#"["apply","g",2]"#);
    }

    /// An argument that is already an inexact float cannot be made *less*
    /// exact by folding, so the float evaluator decides there. This is the
    /// only route by which a fold introduces a non-rational number, and it
    /// mirrors legacy's "input contained a decimal" escape hatch.
    #[test]
    fn an_already_inexact_argument_folds_through_floats() {
        // 55.33 is an exact rational from the text parser but an f64 when it
        // arrives as a JSON literal — both reach 55.
        assert_eq!(run("floor(55.33)"), "55");
        assert_eq!(run_js(r#"["apply","floor",55.33]"#), "55");
        assert_eq!(run_js(r#"["apply","ceil",2.1]"#), "3");
        assert_eq!(run_js(r#"["apply","abs",-3.5]"#), "3.5");
        assert_eq!(run_js(r#"["apply","max",["tuple",1.5,2.5]]"#), "2.5");
        // Exact arguments still refuse: this is what keeps `asin(1)` symbolic
        // while `asin(0.5)` becomes a number, in both libraries.
        assert_eq!(run("asin(1)"), r#"["apply","asin",1]"#);
        assert!(run_js(r#"["apply","asin",0.5]"#).starts_with("0.523"));
        // A complex result is refused rather than silently losing its
        // imaginary part.
        assert_eq!(
            run_js(r#"["apply","sqrt",-4.5]"#),
            r#"["apply","sqrt",-4.5]"#
        );
    }

    /// Bottom-up, so a fold feeds the application above it.
    #[test]
    fn nested_applications_fold_inside_out() {
        assert_eq!(run("abs(floor(-2.5))"), "3");
        assert_eq!(run_js(r#"["apply","sum",["tuple",["apply","abs",-2],3]]"#), "5");
    }
}
