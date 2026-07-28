//! `full_simplify` — the aggressive (non-oracle) simplifier that exposes the
//! landed S1–S4 sound passes (FULL_SIMPLIFY_PLAN). Unlike `simplify`, it may
//! fold `exp(ln x) → x` and trig special values the JS corpus never had.

use math_expressions::{full_simplify, simplify, Assumptions, Expr, TextToAst};

fn p(s: &str) -> Expr {
    TextToAst::new(Default::default())
        .convert(s)
        .unwrap_or_else(|e| panic!("parse {s:?}: {e:?}"))
}

fn fs(s: &str) -> Expr {
    full_simplify(&p(s), &Assumptions::new())
}

/// `full_simplify(input)` equals the simplified `expected` form (structural,
/// since both are driven through `simplify`). Structural — not via `equals`,
/// which has a pre-existing false-negative on `cos(pi/3)` vs `1/2`.
fn assert_fs(input: &str, expected: &str) {
    assert_eq!(
        fs(input),
        simplify(&p(expected)),
        "full_simplify({input:?}) should be {expected:?}"
    );
}

#[test]
fn exp_log_inverses() {
    assert_fs("exp(ln(x))", "x"); // the motivating case
    assert_fs("exp(log(3))", "3");
    assert_fs("e^(ln(x))", "x");
    assert_fs("log(exp(5))", "5");
    assert_fs("ln(1)", "0");
    assert_fs("exp(0)", "1");
    assert_fs("exp(ln(x)) + 2*exp(ln(x))", "3*x");
}

#[test]
fn ln_of_exp_of_variable_is_conservatively_unfolded() {
    // ln(exp u) → u only when u is a decidable real (S5 will generalize).
    // For a free variable it must stay put — it is NOT identically x over ℂ.
    assert_eq!(fs("ln(exp(x))"), simplify(&p("log(exp(x))")));
}

#[test]
fn trig_special_values() {
    assert_fs("sin(pi/6)", "1/2");
    assert_fs("cos(pi/3)", "1/2");
    assert_fs("tan(pi/4)", "1");
    assert_fs("sin(2*pi)", "0");
    assert_fs("cos(pi/6)", "sqrt(3)/2");
}

#[test]
fn rational_cancellation() {
    assert_fs("(x^2 - 1)/(x - 1)", "x + 1");
    assert_fs("(x^2 - 4)/(x + 2)", "x - 2");
}

#[test]
fn idempotent() {
    for s in [
        "exp(ln(x))",
        "cos(pi/3)",
        "(x^2-1)/(x-1)",
        "sin(x)^2 + cos(x)^2",
        "exp(ln(x)) + 2*exp(ln(x))",
        "ln(exp(x))",
    ] {
        let once = fs(s);
        let twice = full_simplify(&once, &Assumptions::new());
        assert_eq!(once, twice, "full_simplify not idempotent on {s:?}");
    }
}

#[test]
fn meaning_preserving_on_reliable_inputs() {
    // Use `equals` only where it is reliable (polynomial/rational), since it
    // has a known false-negative on trig-vs-rational.
    use math_expressions::equals;
    for s in ["(x^2-1)/(x-1)", "2*x + 3*x", "(a+b)^2 - a^2 - 2*a*b"] {
        assert!(
            equals(&fs(s), &p(s), &Default::default()),
            "full_simplify changed the value of {s:?}"
        );
    }
}

#[test]
fn simplify_itself_is_unchanged_oracle() {
    // The whole point: `simplify` stays byte-compatible with the JS corpus and
    // does NOT fold exp(ln x) — only `full_simplify` does.
    assert_ne!(simplify(&p("exp(ln(x))")), p("x"));
    assert_eq!(fs("exp(ln(x))"), p("x"));
}
