//! Moving the sign of a product into one of its factors — the rule that
//! decides between `−(1 − x)` and `x − 1`.
//!
//! Two operations are easy to conflate and only the first happens here: moving
//! the **sign** (`−2(1 − x)` → `2(x − 1)`, the `2` staying put) versus
//! distributing the **coefficient** (`2(1 − x)` → `2 − 2x`), which is `expand`'s
//! job. The criterion is "do not add minus signs": with `n` negated terms out
//! of `k`, the product costs `1 + n` signs as written and `k − n` with the sign
//! pushed in, so it fires exactly when `2n ≥ k`. Each assert below is one row
//! of that table, including the rows that must *not* move.

use math_expressions::{equals, expr, simplify, EqOptions, Expr, TextToAst};

fn t(s: &str) -> Expr {
    TextToAst::new(Default::default()).convert(s).unwrap()
}

fn simplified(s: &str) -> String {
    expr::serde::to_js(&simplify(&t(s))).to_string()
}

#[test]
fn a_sign_that_cancels_one_inside_moves_in() {
    // n = 1, k = 2: a tie on term count, but the outer sign goes away.
    assert_eq!(simplified("-(1-x)"), r#"["+","x",-1]"#);
    assert_eq!(simplified("-(-x+1)"), r#"["+","x",-1]"#);
    // n = 2, k = 2: strictly fewer signs.
    assert_eq!(simplified("-(-x-1)"), r#"["+","x",1]"#);
    // n = 2, k = 3.
    assert_eq!(simplified("-(x-y-z)"), r#"["+",["-","x"],"y","z"]"#);
}

#[test]
fn a_sign_that_would_multiply_stays_put() {
    // A bare `−1` over a sum is not this rule at all — `rule_distribute_neg_over_sum`
    // takes it first and distributes unconditionally, because negating a sum
    // adds no terms and no factors. So the "would multiply the signs" reasoning
    // below applies only once some *other* factor is present; with nothing but
    // the sign, the sum is simply negated termwise.
    //
    // These two rows used to assert the opposite. They were the only place this
    // crate disagreed with `math-expressions@2.0.0-alpha94`, the version
    // DoenetML pins, which gives `["+",["-","x"],["-","y"]]` here — and the
    // disagreement had a cost: a difference of two sums could never cancel, so
    // `(q + 12 - (q+2))/2` stayed unreduced where the JS library gives `5`.
    assert_eq!(simplified("-(x+y)"), r#"["+",["-","x"],["-","y"]]"#);
    assert_eq!(simplified("-(x+y-z)"), r#"["+",["-","x"],["-","y"],"z"]"#);
    // A *positive* coefficient has no sign to move in the first place.
    assert_eq!(simplified("2(1-x)"), r#"["*",2,["+",["-","x"],1]]"#);
    // Negative coefficient, but no factor worth giving the sign to.
    assert_eq!(simplified("-2(x+y)"), r#"["-",["*",2,["+","x","y"]]]"#);
}

#[test]
fn the_magnitude_stays_outside_when_the_sign_moves_in() {
    // The regression this rule was rewritten for: the sign is not the
    // coefficient, so a coefficient other than −1 is no reason to decline.
    assert_eq!(simplified("-2(1-x)"), r#"["*",2,["+","x",-1]]"#);
    assert_eq!(simplified("-0.5(1-x)"), r#"["*",0.5,["+","x",-1]]"#);
    assert_eq!(simplified("-(1-x)/2"), r#"["/",["+","x",-1],2]"#);
    assert_eq!(
        simplified("-2(x-y-z)"),
        r#"["*",2,["+",["-","x"],"y","z"]]"#
    );
}

#[test]
fn other_factors_alongside_do_not_block_it() {
    assert_eq!(simplified("-y(1-x)"), r#"["*","y",["+","x",-1]]"#);
    assert_eq!(simplified("-y z(1-x)"), r#"["*","y","z",["+","x",-1]]"#);
}

#[test]
fn exactly_one_factor_takes_the_sign() {
    // Into two it would cancel, so only the first sum flips.
    assert_eq!(
        simplified("-(1-x)(1-y)"),
        r#"["*",["+","x",-1],["+",["-","y"],1]]"#
    );
    // And it goes to the factor that actually shed signs, not to `x + y`.
    assert_eq!(
        simplified("-(x+y)(1-z)"),
        r#"["*",["+","z",-1],["+","x","y"]]"#
    );
}

#[test]
fn a_power_takes_a_sign_only_at_an_odd_integer_exponent() {
    // `(−b)^m = −(b^m)` for odd `m` …
    assert_eq!(simplified("-(1-x)^3"), r#"["^",["+","x",-1],3]"#);
    // … including `m = −1`, the reciprocal.
    assert_eq!(simplified("1/(-(1-x))"), r#"["/",1,["+","x",-1]]"#);
    // An even exponent must not: `−(1−x)²` is not `(x−1)²`.
    assert_eq!(simplified("-(1-x)^2"), r#"["-",["^",["+",["-","x"],1],2]]"#);
    // Nor a non-integer one, which keeps this away from `−√(1−x)`.
    assert_eq!(
        simplified("-(1-x)^(1/2)"),
        r#"["-",["^",["+",["-","x"],1],["/",1,2]]]"#
    );
}

#[test]
fn the_rewrite_reaches_a_fixpoint() {
    // The rewrite always leaves a positive coefficient, so it cannot fire on
    // its own output; a ping-pong here would spin the rewrite loop.
    for s in ["-(1-x)", "-2(1-x)", "-y(1-x)", "-(1-x)(1-y)", "-(1-x)^3"] {
        let once = simplify(&t(s));
        let twice = simplify(&once);
        assert_eq!(
            expr::serde::to_js(&once).to_string(),
            expr::serde::to_js(&twice).to_string(),
            "{s} did not settle"
        );
    }
}

#[test]
fn the_value_is_unchanged() {
    // Sign bookkeeping, not arithmetic: every rewritten form still equals what
    // it was written as. The `^2` and `^(1/2)` rows are the ones that would go
    // wrong if the exponent guard were dropped.
    for s in [
        "-(1-x)",
        "-(-x-1)",
        "-(x-y-z)",
        "-(x+y)",
        "-(x+y-z)",
        "-2(1-x)",
        "-0.5(1-x)",
        "-(1-x)/2",
        "-y(1-x)",
        "-y z(1-x)",
        "-(1-x)(1-y)",
        "-(x+y)(1-z)",
        "-(1-x)^3",
        "-(1-x)^2",
        "1/(-(1-x))",
        "-(1-x)^(1/2)",
    ] {
        assert!(
            equals(&t(s), &simplify(&t(s)), &EqOptions::default()),
            "{s} changed value under simplify"
        );
    }
}
