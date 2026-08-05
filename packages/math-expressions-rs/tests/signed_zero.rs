//! Signed-zero (`Number::NegZero`) behaviour: a division-by-zero pole reports a
//! *signed* infinity, with the sign flowing through products and negation, while
//! a bare `−0` still reads as plain `0` everywhere else.

use math_expressions::{expr, simplify, TextToAst};

/// Parse `s`, canonicalise/simplify, and spell the result as its JS tree.
fn run(s: &str) -> String {
    let e = TextToAst::new(Default::default()).convert(s).unwrap();
    expr::serde::to_js(&simplify(&e)).to_string()
}

#[test]
fn pole_sign_from_literal_negative_zero() {
    // 1/(−0) is −∞; 1/0 stays +∞.
    assert_eq!(run("1/0"), r#"{"$":"Inf"}"#);
    assert_eq!(run("1/-0"), r#"{"$":"-Inf"}"#);
    // The numerator's own sign composes with the pole's.
    assert_eq!(run("6/-0"), r#"{"$":"-Inf"}"#);
    assert_eq!(run("-6/-0"), r#"{"$":"Inf"}"#);
}

#[test]
fn sign_flows_through_a_product_into_the_zero() {
    // The zero acquires its sign from a negative factor before the reciprocal.
    assert_eq!(run("1/((-1)*0)"), r#"{"$":"-Inf"}"#);
    assert_eq!(run("1/((-1)(0))"), r#"{"$":"-Inf"}"#);
    // Two negatives cancel: the zero is +0 again.
    assert_eq!(run("1/((-1)*(-1)*0)"), r#"{"$":"Inf"}"#);
    assert_eq!(run("1/(2*(-3)*0)"), r#"{"$":"-Inf"}"#);
}

#[test]
fn bare_negative_zero_reads_as_plain_zero() {
    // `−0` is value-equal to `0`: it prints as `0` and annihilates as usual.
    assert_eq!(run("-0"), "0");
    assert_eq!(run("(-1)*0"), "0");
    assert_eq!(run("(-1)*0+x"), r#""x""#);
    // `0/0` is still the indeterminate NaN, sign or no sign.
    assert_eq!(run("0/0"), r#"{"$":"NaN"}"#);
}
