//! The display-rounding passes on `Float` values — DoenetML upstream request 08,
//! "display rounding loses precision at large magnitudes".
//!
//! `<number>` defaults to `displayDigits = 3, displayDecimals = 2`, so
//! `round_numbers_to_precision_plus_decimals(v, 3, 2)` is the path every number
//! a student sees goes through. It asked for 2 decimal places of `2e21` — a
//! no-op on a value that is already an integer — and returned
//! `1.9999999999999997e21`, because the implementation computed
//! `(v · 10^d).round() / 10^d` and neither `2e23` nor the quotient is
//! representable. Rounding now goes through the float's exact binary value.

use math_expressions::num::Number;
use math_expressions::{
    round_numbers_to_decimals, round_numbers_to_precision,
    round_numbers_to_precision_plus_decimals, Expr, TextOpts,
};

fn f(v: f64) -> Expr {
    Expr::Num(Number::from_f64(v))
}

fn show(e: &Expr) -> String {
    math_expressions::to_text(e, &TextOpts::default())
}

/// The reported case, and the neighbouring magnitudes on both sides of the
/// point where `v · 100` stops being representable (`2^53 / 100 ≈ 9e13`).
#[test]
fn rounding_a_large_float_does_not_perturb_it() {
    for v in [2e21, 2e14, 1.5e16, 9.87e30, 1e21, 6.02e23, -2e21] {
        assert_eq!(
            show(&round_numbers_to_precision_plus_decimals(&f(v), 3.0, 2.0)),
            show(&f(v)),
            "{v:e} is an integer; rounding it to 2 decimals must be identity"
        );
        assert_eq!(show(&round_numbers_to_decimals(&f(v), 2)), show(&f(v)));
    }
}

/// Asking for *more* digits used to be the workaround — it returned the exact
/// answer where 3 did not. Both must agree now, and with the precision-only
/// pass, which was never affected because its decimal count came out negative.
#[test]
fn every_way_of_asking_agrees() {
    let v = f(2e21);
    let expected = "2 * 10^21";
    assert_eq!(show(&round_numbers_to_precision(&v, 3)), expected);
    assert_eq!(
        show(&round_numbers_to_precision_plus_decimals(&v, 3.0, 2.0)),
        expected
    );
    assert_eq!(
        show(&round_numbers_to_precision_plus_decimals(&v, 15.0, 2.0)),
        expected
    );
    assert_eq!(show(&round_numbers_to_decimals(&v, 2)), expected);
}

/// Rounding that has something to do still does it, at the significant figure
/// and at the decimal place, with the larger of the two winning.
#[test]
fn rounding_that_should_change_the_value_still_does() {
    assert_eq!(show(&round_numbers_to_decimals(&f(2.345), 2)), "2.35");
    assert_eq!(show(&round_numbers_to_decimals(&f(-2.345), 2)), "-2.35");
    assert_eq!(
        show(&round_numbers_to_precision(&f(12345.6789), 3)),
        "12300"
    );
    // digits alone would give 12300; decimals raises it to the full value.
    assert_eq!(
        show(&round_numbers_to_precision_plus_decimals(
            &f(12345.6789),
            3.0,
            2.0
        )),
        "12345.68"
    );
    // Below 1, the significant-figure count is the one that bites: 2 decimals
    // of 0.00123456 would be 0, and the point of the pass is that it is not.
    assert_eq!(
        show(&round_numbers_to_precision_plus_decimals(
            &f(0.00123456),
            3.0,
            2.0
        )),
        "0.00123"
    );
}

/// Exact rounding is rounding of the value the float *actually holds*, not of
/// its shortest decimal spelling. `2.675` is stored as `2.67499999999999982…`,
/// so two decimals is `2.67` — the classic result, and the one legacy's
/// `parseFloat(toFixed(v, n))` produced. A value that is exactly a half rounds
/// away from zero.
#[test]
fn ties_are_resolved_against_the_stored_value() {
    assert_eq!(show(&round_numbers_to_decimals(&f(2.675), 2)), "2.67");
    assert_eq!(show(&round_numbers_to_decimals(&f(1.005), 2)), "1");
    // Exactly representable halves: away from zero, both signs.
    assert_eq!(show(&round_numbers_to_decimals(&f(0.125), 2)), "0.13");
    assert_eq!(show(&round_numbers_to_decimals(&f(-0.125), 2)), "-0.13");
    assert_eq!(show(&round_numbers_to_decimals(&f(2.5), 0)), "3");
    assert_eq!(show(&round_numbers_to_decimals(&f(-2.5), 0)), "-3");
}

/// A non-finite float has no decimal expansion to round; it comes back
/// untouched rather than as `NaN` or a trap.
#[test]
fn non_finite_floats_pass_through() {
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let rounded = round_numbers_to_precision_plus_decimals(&f(v), 3.0, 2.0);
        assert_eq!(show(&rounded), show(&f(v)), "{v} must be unchanged");
    }
}

/// Exact values are rounded exactly, as before — the float path is the only
/// one that changed.
#[test]
fn exact_values_are_unaffected() {
    use math_expressions::TextToAst;
    let p = |s: &str| TextToAst::new(Default::default()).convert(s).unwrap();
    assert_eq!(show(&round_numbers_to_decimals(&p("2.345"), 2)), "2.35");
    // Simplified first: rounding maps over *numbers*, so an unevaluated `1/3`
    // is two integers and each is already whole.
    let third = math_expressions::simplify(&p("1/3"));
    assert_eq!(show(&round_numbers_to_decimals(&third, 2)), "0.33");
    assert_eq!(
        show(&round_numbers_to_precision_plus_decimals(
            &p("2000000000000000000000"),
            3.0,
            2.0
        )),
        "2000000000000000000000"
    );
}
