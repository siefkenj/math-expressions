//! The round-8 DoenetML items: `evaluate_numbers`' digit budget, and odd
//! parity no longer firing when it has nothing to unlock.
//!
//! Each `assert` here is a row of an expected-behaviour table DoenetML filed,
//! including the neighbouring rows that must *not* change — those are the
//! point of the test, not padding.

use math_expressions::{
    evaluate_numbers, evaluate_numbers_evaluate_functions_with_digits,
    evaluate_numbers_preserve_order_with_digits, evaluate_numbers_with_digits, expr, simplify,
    Expr, MaxDigits, TextToAst,
};

fn t(s: &str) -> Expr {
    TextToAst::new(Default::default()).convert(s).unwrap()
}

fn js(e: &Expr) -> String {
    expr::serde::to_js(e).to_string()
}

fn simplified(s: &str) -> String {
    js(&simplify(&t(s)))
}

/// `evaluate_numbers` with the digit budget spent — `max_digits: Infinity` on
/// the JS side.
fn folded(s: &str) -> String {
    js(&evaluate_numbers_with_digits(&t(s), MaxDigits::Unlimited))
}

// ---- item 18: the digit budget -----------------------------------------

/// The reported case. DoenetML grades a response against a target by comparing
/// the two term by term with a tolerance, so a target that still holds `2π`
/// cannot match a response the student typed as `15.42478` unless the constants
/// are spent into floats first.
#[test]
fn unlimited_digits_folds_a_variable_free_subtree_to_one_number() {
    assert_eq!(folded("2pi+pi+6"), "15.42477796076938");
    assert_eq!(folded("pi"), "3.141592653589793");
    assert_eq!(folded("pi/2"), "1.5707963267948966");
    // An exact value that is already non-integral is spent; one that is not
    // stays put — see `to_floats` for why the budget stops there.
    assert_eq!(folded("0.5+1/4"), "0.75");
    // The whole point is that this lands beside a typed decimal: the two agree
    // to well inside the `0.001` a student is usually allowed.
    assert_eq!(
        folded("sin(2pi+1x+4x+pi+6)"),
        r#"["apply","sin",["+","x",["*",4,"x"],15.42477796076938]]"#
    );
}

/// The budget stops at values that are exactly integral, which is narrower than
/// legacy: `x/3` stayed `x/3` here where legacy gave `0.3333333333333333 x`.
/// The difference is deliberate and measured — floating integers costs 6 to 25
/// tests depending on how far it is taken, and buys none.
#[test]
fn integral_values_stay_exact_even_under_the_budget() {
    assert_eq!(folded("x/3"), r#"["/","x",3]"#);
    assert_eq!(folded("2^x"), r#"["^",2,"x"]"#);
    assert_eq!(folded("x^2"), r#"["^","x",2]"#);
}

/// Without the budget nothing changes, which is what keeps `simplify="numbers"`
/// an *exact* pass: this is the default and the one every other caller gets.
#[test]
fn the_default_budget_leaves_exact_values_exact() {
    let exact = |s: &str| js(&evaluate_numbers(&t(s)));
    // Constant-bearing terms lead: `π` and `2π` carry degree where the bare
    // `6` does not (see `present::atom_rank`).
    assert_eq!(exact("2pi+pi+6"), r#"["+","pi",["*",2,"pi"],6]"#);
    assert_eq!(exact("pi"), "\"pi\"");
    assert_eq!(exact("pi/2"), r#"["/","pi",2]"#);
    assert_eq!(exact("1/2+1/3"), r#"["/",5,6]"#);
    assert_eq!(exact("x/3"), r#"["/","x",3]"#);
    // Folding numbers with numbers never needed a budget.
    assert_eq!(exact("0.5 7"), "3.5");
    assert_eq!(exact("4+x-2"), r#"["+","x",2]"#);
}

/// The budget composes with the other two `evaluate_numbers` forms, because
/// DoenetML reaches all three of them from `simplifyOnCompare`.
#[test]
fn the_budget_composes_with_the_other_forms() {
    // `numberspreserveorder`: numbers fold only with *adjacent* numbers, so the
    // two constants on either side of the `x` terms stay apart.
    assert_eq!(
        js(&evaluate_numbers_preserve_order_with_digits(
            &t("sin(2pi+1x+4x+pi+6)"),
            MaxDigits::Unlimited
        )),
        r#"["apply","sin",["+",6.283185307179586,"x",["*",4,"x"],9.141592653589793]]"#
    );
    // `full`: functions of numeric arguments evaluate as well.
    assert_eq!(
        js(&evaluate_numbers_evaluate_functions_with_digits(
            &t("cos(2pi)+1"),
            MaxDigits::Unlimited
        )),
        "2"
    );
}

/// `i` has no real value to be spent into, and a complex expression must not
/// come out of the pass as nonsense.
#[test]
fn the_imaginary_unit_survives_the_budget() {
    assert_eq!(folded("0.5i+0.75"), r#"["+",["*",0.5,"i"],0.75]"#);
}

// ---- item 19: parity that pays for itself ------------------------------

/// Odd parity moves a sign from inside an application to outside it, which is
/// the same tree in two spellings. DoenetML reads `.tree` in a dozen places and
/// wants the shape the author wrote, so the move now has to unlock something.
#[test]
fn odd_parity_alone_leaves_the_expression_alone() {
    assert_eq!(simplified("sin(-2)"), r#"["apply","sin",-2]"#);
    assert_eq!(simplified("sin(-x)"), r#"["apply","sin",["-","x"]]"#);
    assert_eq!(simplified("tan(-x)"), r#"["apply","tan",["-","x"]]"#);
    assert_eq!(simplified("csc(-x)"), r#"["apply","csc",["-","x"]]"#);
}

/// Even parity is a different trade — the sign *disappears* rather than moving
/// — so it still applies with nothing else in hand.
#[test]
fn even_parity_still_drops_the_sign() {
    assert_eq!(simplified("cos(-x)"), r#"["apply","cos","x"]"#);
    assert_eq!(simplified("sec(-x)"), r#"["apply","sec","x"]"#);
    assert_eq!(simplified("cos(-2)"), r#"["apply","cos",2]"#);
}

/// When the sign unlocks a lattice value it is pulled out as before: this is
/// the case the rule exists for.
#[test]
fn parity_still_fires_when_it_reaches_a_value() {
    assert_eq!(simplified("sin(-pi/6)"), r#"["/",-1,2]"#);
    assert_eq!(simplified("tan(-pi/4)"), "-1");
    assert_eq!(simplified("cos(-pi/3)"), r#"["/",1,2]"#);
    assert_eq!(simplified("sin(3pi/2)"), "-1");
}

/// And when a multiple of π drops out, the sign that drop leaves behind is
/// still resolved — including the case where the remainder is itself negative,
/// which would otherwise come out as the double negative `−sin(−x)`.
#[test]
fn periodicity_still_reduces_and_settles_its_own_sign() {
    assert_eq!(simplified("sin(x+2pi)"), r#"["apply","sin","x"]"#);
    assert_eq!(simplified("sin(x+pi)"), r#"["-",["apply","sin","x"]]"#);
    assert_eq!(simplified("sin(pi-x)"), r#"["apply","sin","x"]"#);
    assert_eq!(simplified("sin(2pi-x)"), r#"["-",["apply","sin","x"]]"#);
    assert_eq!(simplified("cos(pi-x)"), r#"["-",["apply","cos","x"]]"#);
}

/// Inverse trig is a separate rule with a separate justification (`asin(-x)` →
/// `-asin(x)` is how the inverse is *defined* on its branch, not a
/// rearrangement), and is deliberately unchanged.
#[test]
fn inverse_trig_parity_is_untouched() {
    assert_eq!(simplified("asin(-x)"), r#"["-",["apply","asin","x"]]"#);
    assert_eq!(simplified("atan(-x)"), r#"["-",["apply","atan","x"]]"#);
    assert_eq!(
        simplified("acos(-x)"),
        r#"["+","pi",["-",["apply","acos","x"]]]"#
    );
}

/// Numeric agreement is what actually matters downstream, and it is unchanged:
/// the two spellings are equal whichever way the tree is written.
#[test]
fn the_two_spellings_still_compare_equal() {
    let eq = |a: &str, b: &str| {
        math_expressions::equals(&t(a), &t(b), &math_expressions::EqOptions::default())
    };
    assert!(eq("sin(-x)", "-sin(x)"));
    assert!(eq("sin(-2)", "-sin(2)"));
    assert!(eq("cos(-x)", "cos(x)"));
}
