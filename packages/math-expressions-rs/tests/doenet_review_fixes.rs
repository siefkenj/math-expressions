//! Regressions for the review of the Doenet-compatibility round (`c110a56..`).
//! Each test pins one defect the review found, so it cannot come back quietly.
//!
//! Three of them — the NaN aggregates, `0/∞`, and `nCr` on a float — produced
//! *wrong numbers* rather than errors, which is the failure shape
//! `active-plans/DOENET_INTEGRATION.md` argues is worst on a grading path: a
//! student sees a confident answer and nothing logs. The two cost tests exist
//! because student input is adversarial by construction.

use math_expressions::{simplify, Expr, MathConst, TextToAst};
use std::time::Instant;

fn p(s: &str) -> Expr {
    TextToAst::new(Default::default())
        .convert(s)
        .unwrap_or_else(|e| panic!("parse {s:?}: {e:?}"))
}

fn js(json: &str) -> Expr {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    math_expressions::expr::serde::try_from_js(&v).unwrap_or_else(|e| panic!("{json}: {e}"))
}

fn tree(e: &Expr) -> String {
    math_expressions::expr::serde::to_js(e).to_string()
}

// ---- annihilation: `0 · x` vs the indeterminate forms --------------------

/// `∞^(-1)` *is* `0`, so `0/∞` is a plain zero. The guard blocking `0·x → 0`
/// for non-finite factors read only the base and not the exponent, so the fix
/// that correctly made `0/0` indeterminate also made `0/∞` come back `NaN`.
#[test]
fn zero_over_infinity_is_zero() {
    for s in [
        r#"["/",0,{"$":"Inf"}]"#,
        r#"["/",0,{"$":"-Inf"}]"#,
        r#"["*",0,["^",{"$":"Inf"},-1]]"#,
        r#"["*",0,["^",{"$":"-Inf"},-2]]"#,
    ] {
        assert_eq!(tree(&simplify(&js(s))), "0", "{s} should annihilate to 0");
    }
}

/// The other half of the same guard: the genuinely indeterminate forms must
/// stay `NaN`. DoenetML computes an undefined slope as `0/0`, and reporting
/// that as `0` calls a degenerate line horizontal.
#[test]
fn indeterminate_products_stay_nan() {
    for s in [
        r#"["/",0,0]"#,
        r#"["*",0,{"$":"Inf"}]"#,
        r#"["*",0,{"$":"-Inf"}]"#,
        r#"["*",0,{"$":"NaN"}]"#,
        r#"["*",0,["^",{"$":"Inf"},2]]"#,
        // `{"$":"None"}` is DoenetML's "no value here". A product touching one
        // is undefined, not zero — plan item 3b, which is the whole reason
        // `evaluate_to_constant` guards on a `None` leaf before simplifying.
        r#"["*",0,{"$":"None"}]"#,
        r#"["*",0,["^",{"$":"None"},-1]]"#,
    ] {
        assert_eq!(
            simplify(&js(s)),
            Expr::Const(MathConst::NaN),
            "{s} is indeterminate and must be NaN"
        );
    }
}

/// A factor whose finiteness is merely *unknown* still annihilates — legacy's
/// third `undefined` state fell through to `0`, and narrowing that would break
/// the overwhelmingly common case.
#[test]
fn zero_times_an_unknown_factor_is_still_zero() {
    for s in ["0*x", "0*f(x)", "0/x", "0*x*y^2"] {
        assert_eq!(tree(&simplify(&p(s))), "0", "{s:?} should be 0");
    }
}

// ---- combinatorics on floats and on large arguments ----------------------

/// `n.re.round() as i64` saturates, so `nCr(1e20,3)` silently became
/// `nCr(i64::MAX,3)` — a confident answer ~1275× too small. The float path now
/// stays in f64, where `n - k` is at least correctly rounded.
#[test]
fn combinatorics_on_large_floats_are_not_saturated() {
    let near = |got: &Expr, want: f64, label: &str| {
        let Expr::Num(n) = got else {
            panic!("{label} did not fold: {got:?}")
        };
        let g = n.to_f64();
        assert!(
            (g / want - 1.0).abs() < 1e-9,
            "{label}: got {g:e}, want ~{want:e}"
        );
    };
    near(
        &simplify(&js(r#"["apply","nCr",["tuple",1e20,3]]"#)),
        1e60 / 6.0,
        "nCr(1e20,3)",
    );
    near(
        &simplify(&js(r#"["apply","nPr",["tuple",1e20,2]]"#)),
        1e40,
        "nPr(1e20,2)",
    );
    // Small float arguments are unaffected.
    near(
        &simplify(&js(r#"["apply","nCr",["tuple",5.0,3.0]]"#)),
        10.0,
        "nCr(5.0,3.0)",
    );
}

/// Bounding `r` alone left the *work* unbounded — a running product against a
/// 1661-bit `n` is quadratic in the result size. The value must be unchanged;
/// only the cost is. (A generous ceiling: this was ~4 s.)
#[test]
fn large_exact_combinatorics_are_fast() {
    for s in ["nCr(10^500,500)", "nPr(10^500,500)"] {
        let t = Instant::now();
        let got = simplify(&p(s));
        let dt = t.elapsed();
        assert!(
            matches!(got, Expr::Num(_)),
            "{s:?} should still fold exactly, got {got:?}"
        );
        assert!(dt.as_millis() < 1500, "{s:?} took {dt:?}");
    }
    // Still exactly right at a size that can be checked by hand.
    assert_eq!(tree(&simplify(&p("nCr(60,30)"))), "118264581564861424");
    assert_eq!(tree(&simplify(&p("nCr(5,3)"))), "10");
    assert_eq!(tree(&simplify(&p("nPr(5,3)"))), "60");
}

/// `integer_log` stripped one factor per iteration, which is quadratic in the
/// bit length: `log₂(2^200000)` — six characters of student input — took
/// seconds. Binary search on the exponent gives the same answers.
#[test]
fn exact_logarithms_are_fast_on_huge_powers() {
    let t = Instant::now();
    assert_eq!(tree(&simplify(&p("log_2(2^200000)"))), "200000");
    let dt = t.elapsed();
    assert!(dt.as_millis() < 1500, "log_2(2^200000) took {dt:?}");

    // The answers the binary search has to keep: exact powers fold, everything
    // else stays symbolic rather than becoming a float.
    assert_eq!(tree(&simplify(&p("log_10(1000)"))), "3");
    assert_eq!(tree(&simplify(&p("log10(100000)"))), "5");
    assert_eq!(tree(&simplify(&p("log_7(343)"))), "3");
    assert_eq!(tree(&simplify(&p("log_2(1)"))), "0");
    assert_eq!(tree(&simplify(&js(r#"["apply","log2",["/",1,8]]"#))), "-3");
    // Not an exact power: the value must stay symbolic rather than turn into a
    // float. `log10(3)` keeps its application; the *based* spellings hand off to
    // the change-of-base rewrite instead, which is still symbolic — and is what
    // decides that the based and unbased spellings are the same number.
    assert!(matches!(simplify(&p("log10(3)")), Expr::Apply(..)));
    for s in ["log_2(9)", "log_2(2^200000+1)"] {
        let simplified = simplify(&p(s));
        assert!(
            !matches!(simplified, Expr::Num(_)),
            "{s:?} is not an exact power and must not fold to a number"
        );
        let arg = s.trim_start_matches("log_2(").trim_end_matches(')');
        assert_eq!(
            simplified,
            simplify(&p(&format!("log({arg})/log(2)"))),
            "{s:?} should reduce by change of base"
        );
    }
}

// ---- `from_ast` diagnostics ----------------------------------------------

/// The message the DoenetML team lost a debugging cycle to: an object with no
/// `$` was reported as `unknown special None`, where that `None` was the
/// `Option` from the lookup — and `{"$":"None"}` is meanwhile a *legal* tree,
/// so it read as a valid input being rejected.
#[test]
fn from_ast_names_the_actual_problem() {
    let err = |json: &str| {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        math_expressions::expr::serde::try_from_js(&v).unwrap_err()
    };
    let missing = err("{}");
    assert!(
        !missing.contains("None"),
        "a missing `$` must not be reported with the word None: {missing}"
    );
    assert!(
        missing.contains('$'),
        "should name the missing key: {missing}"
    );

    let unknown = err(r#"{"$":"Bogus"}"#);
    assert!(
        unknown.contains("Bogus"),
        "should name the offending tag: {unknown}"
    );

    // And the tag that *is* legal still round-trips.
    assert_eq!(js(r#"{"$":"None"}"#), Expr::Const(MathConst::None));
}
