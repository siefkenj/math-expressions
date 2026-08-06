//! Batched sampling (`evaluate_many`) and exact critical points
//! (`critical_points`).
//!
//! Both exist because sampling a function point-by-point across the JS boundary
//! costs ~200× the arithmetic it performs: `evaluate_many` amortizes that, and
//! `critical_points` removes the need to sample at all wherever the derivative
//! is rational.

use math_expressions::{
    critical_points, derivative, evaluate, evaluate_many, evaluate_to_constant, expr, Expr,
    TextToAst,
};
use std::collections::HashMap;

fn t(s: &str) -> Expr {
    TextToAst::new(Default::default()).convert(s).unwrap()
}

fn js(e: &Expr) -> String {
    expr::serde::to_js(e).to_string()
}

/// The reported points as JS trees, or `None` when undecided.
fn points(s: &str) -> Option<Vec<String>> {
    critical_points(&t(s), "x").map(|v| v.iter().map(js).collect())
}

/// The numeric value of a reported point.
fn value_of(e: &Expr) -> f64 {
    evaluate_to_constant(e)
        .unwrap_or_else(|| panic!("critical point {} is not numeric", js(e)))
        .re
}

// ============================ evaluate_many ============================

#[test]
fn batched_sampling_agrees_with_point_by_point() {
    let e = t("x^2-3x+1");
    let xs: Vec<f64> = (-50..=50).map(|i| i as f64 / 7.0).collect();
    let batch = evaluate_many(&e, "x", &xs);
    assert_eq!(batch.len(), xs.len(), "one result per point asked about");
    for (i, x) in xs.iter().enumerate() {
        let one = evaluate(&e, &HashMap::from([("x".to_string(), *x)])).unwrap();
        assert_eq!(batch[i], one.re, "disagreement at x = {x}");
    }
}

#[test]
fn a_point_with_no_finite_real_value_is_nan() {
    // A pole, and the point beside it that is fine.
    let r = evaluate_many(&t("1/x"), "x", &[0.0, 2.0]);
    assert!(r[0].is_nan(), "1/0 should be NaN, got {}", r[0]);
    assert_eq!(r[1], 0.5);
    // A complex branch is not a real value.
    let r = evaluate_many(&t("sqrt(x)"), "x", &[-1.0, 4.0]);
    assert!(r[0].is_nan(), "sqrt(-1) is not real, got {}", r[0]);
    assert_eq!(r[1], 2.0);
    // An unbound variable yields no value, rather than silently binding to 0.
    let r = evaluate_many(&t("y+1"), "x", &[1.0]);
    assert!(r[0].is_nan(), "unbound y should be NaN, got {}", r[0]);
}

#[test]
fn sampling_holds_its_shape_at_the_edges() {
    // No points asked, no points returned — not an error.
    assert!(evaluate_many(&t("x^2"), "x", &[]).is_empty());
    // A constant expression evaluates everywhere without mentioning x.
    assert_eq!(evaluate_many(&t("7"), "x", &[1.0, 2.0]), vec![7.0, 7.0]);
    // Non-finite inputs are points like any other.
    let r = evaluate_many(&t("1/x"), "x", &[f64::INFINITY, f64::NAN]);
    assert!(r.iter().all(|v| v.is_nan() || *v == 0.0));
}

// ============================ critical_points ============================

#[test]
fn a_polynomial_gives_its_roots_exactly() {
    assert_eq!(points("x^2-3x+1"), Some(vec![r#"["/",3,2]"#.into()]));
    assert_eq!(points("x^3-3x"), Some(vec!["-1".into(), "1".into()]));
    assert_eq!(
        points("x^3-6x^2+9x+1"),
        Some(vec!["1".into(), "3".into()]),
        "ascending order"
    );
}

#[test]
fn a_repeated_root_is_reported_once() {
    // f' = 3x², a double root at 0 — one critical point, not two.
    assert_eq!(points("x^3"), Some(vec!["0".into()]));
}

#[test]
fn an_irrational_root_comes_back_as_an_exact_algebraic_number() {
    // f' = 4x³ − 10x = 2x(2x² − 5): 0 and ±√(5/2), the irrational pair carried
    // as `rootof` with its defining polynomial rather than as a float.
    let pts = points("x^4-5x^2+4").expect("decidable");
    assert_eq!(pts.len(), 3);
    assert_eq!(pts[1], "0");
    assert!(pts[0].contains("rootof"), "got {}", pts[0]);
    assert!(pts[2].contains("rootof"), "got {}", pts[2]);
}

#[test]
fn a_pole_is_not_a_critical_point() {
    // f = x²/(x−1): f' = x(x−2)/(x−1)². The root shared with the denominator is
    // cancelled before root-finding, so x = 1 must not appear.
    assert_eq!(points("x^2/(x-1)"), Some(vec!["0".into(), "2".into()]));
    assert_eq!(points("x+1/x"), Some(vec!["-1".into(), "1".into()]));
}

#[test]
fn provably_none_is_not_the_same_as_undecided() {
    // A non-zero constant slope never vanishes: an empty list, decisively.
    assert_eq!(points("x"), Some(vec![]));
    assert_eq!(points("2x+3"), Some(vec![]));
    assert_eq!(points("pi x"), Some(vec![]));
    assert_eq!(points("1/x"), Some(vec![]));
    // Undecided, each for its own reason: not rational in x; a free parameter
    // the answer would depend on; and a constant function, every point of which
    // is critical.
    assert_eq!(points("sin(x)"), None);
    assert_eq!(points("exp(x)"), None);
    assert_eq!(points("a x^2"), None);
    assert_eq!(points("5"), None);
}

#[test]
fn every_reported_point_zeroes_the_derivative() {
    for s in [
        "x^2-3x+1",
        "x^3",
        "x^3-3x",
        "(x^2-1)/(x^2+1)",
        "x^4-5x^2+4",
        "x^5-x",
        "x+1/x",
        "x^2/(x-1)",
        "(x-1)^2(x+2)",
        "x^6-3x^2",
    ] {
        let d = derivative(&t(s), "x");
        for p in critical_points(&t(s), "x").expect(s) {
            let x = value_of(&p);
            let at = evaluate_many(&d, "x", &[x])[0];
            assert!(at.abs() < 1e-9, "{s}: f'({x}) = {at}, expected 0");
        }
    }
}

#[test]
fn no_real_root_of_the_derivative_is_missed() {
    // Soundness is only half of it: a list that silently drops a root is worse
    // than no list. Scan densely and require every sign change of f' to sit
    // beside a reported point.
    for s in [
        "x^2-3x+1",
        "x^3-3x",
        "x^4-5x^2+4",
        "x^5-x",
        "x+1/x",
        "x^2/(x-1)",
        "x^6-3x^2",
        "x^3-6x^2+9x+1",
    ] {
        let d = derivative(&t(s), "x");
        let pts: Vec<f64> = critical_points(&t(s), "x")
            .expect(s)
            .iter()
            .map(value_of)
            .collect();
        let xs: Vec<f64> = (-4000..=4000).map(|i| i as f64 / 500.0).collect();
        let vals = evaluate_many(&d, "x", &xs);
        for w in 0..xs.len() - 1 {
            let (a, b) = (vals[w], vals[w + 1]);
            if !a.is_finite() || !b.is_finite() || a == 0.0 || (a > 0.0) == (b > 0.0) {
                continue;
            }
            assert!(
                pts.iter()
                    .any(|p| *p >= xs[w] - 0.01 && *p <= xs[w + 1] + 0.01),
                "{s}: f' changes sign near {} with no critical point reported",
                xs[w]
            );
        }
    }
}
