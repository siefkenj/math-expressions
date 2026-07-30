//! `full_simplify` — the aggressive simplifier that runs the landed S1–S4 sound
//! passes (FULL_SIMPLIFY_PLAN): it folds `exp(ln x) → x` and the trig special
//! values the JS corpus never had. It is now the engine behind the public
//! `simplify` (no assumptions) and `simplify_with` (with assumptions), so this
//! file also pins that the three agree.

use math_expressions::{full_simplify, simplify, simplify_with, Assumptions, Expr, TextToAst};

fn p(s: &str) -> Expr {
    TextToAst::new(Default::default())
        .convert(s)
        .unwrap_or_else(|e| panic!("parse {s:?}: {e:?}"))
}

fn fs(s: &str) -> Expr {
    full_simplify(&p(s), &Assumptions::new())
}

/// `full_simplify(input)` equals the simplified `expected` form. Structural
/// (both sides are driven through `simplify` first), not via `equals`: these
/// tests are about the *shape* `full_simplify` reduces to, and `equals` would
/// accept any mathematically-equal tree — including a completely unreduced one.
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
    // Use `equals` only where it is reliable (polynomial/rational). The new
    // certified-exact stage rescues *variable-free* trig-vs-rational pairs
    // (`cos(pi/3)` vs `1/2`), but with a free variable those fall back to
    // sampling, which still false-rejects a folded `sin(pi)*x → 0`.
    use math_expressions::equals;
    for s in ["(x^2-1)/(x-1)", "2*x + 3*x", "(a+b)^2 - a^2 - 2*a*b"] {
        assert!(
            equals(&fs(s), &p(s), &Default::default()),
            "full_simplify changed the value of {s:?}"
        );
    }
}

#[test]
fn simplify_now_folds_like_full_simplify() {
    // `simplify` is now the aggressive simplifier: it folds `exp(ln x) → x` and
    // the trig/exp/log special values (previously only `full_simplify` did), so
    // the two are equivalent. (The JS-corpus-compatible base survives only as
    // the crate-internal `simplify_base_with`.)
    assert_eq!(simplify(&p("exp(ln(x))")), p("x"));
    for s in ["exp(ln(x))", "sin(pi/6)", "ln(1) + e^0", "cos(0)*x"] {
        assert_eq!(simplify(&p(s)), fs(s), "simplify != full_simplify on {s:?}");
    }
}

#[test]
fn simplify_with_no_assumptions_equals_simplify() {
    // Adding assumptions must never make the simplifier *weaker*: `simplify_with`
    // runs the same aggressive pipeline, so with an empty set it is `simplify`.
    for s in [
        "exp(ln(x))",
        "cos(pi/3)",
        "(x^2-1)/(x-1)",
        "sin(x)^2 + cos(x)^2",
        "ln(1) + e^0",
    ] {
        assert_eq!(
            simplify_with(&p(s), &Assumptions::new()),
            simplify(&p(s)),
            "simplify_with(∅) != simplify on {s:?}"
        );
    }
}

#[test]
fn simplify_with_assumptions_keeps_the_aggressive_folds() {
    // The assumption-aware rules layer *on top of* the aggressive passes rather
    // than replacing them: `sqrt(x^2)` resolves by sign AND `exp(ln …)` folds.
    let mut a = Assumptions::new();
    a.add(&p("x > 0"));
    assert_eq!(simplify_with(&p("sqrt(x^2)"), &a), simplify(&p("x")));
    assert_eq!(
        simplify_with(&p("exp(ln(x)) + sqrt(x^2)"), &a),
        simplify(&p("2*x"))
    );
}
