//! Integration (INTEGRATION_PLAN / DIVERGENCE_PLAN) and arbitrary-precision
//! evaluation of constant expressions.

use super::Expression;
use math_expressions::{Expr, Number};
use wasm_bindgen::prelude::*;

/// How `evaluate_to_precision` renders its digits (mirrors the core
/// `math_expressions::eval_numeric::certified_digits::DecimalFormat` across the JS boundary).
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum DecimalFormat {
    /// Plain decimal expansion (`"1.41421356…"`).
    Plain,
    /// Normalized scientific form (`"1.4142…e0"`).
    Scientific,
}

impl From<DecimalFormat> for math_expressions::eval_numeric::certified_digits::DecimalFormat {
    fn from(f: DecimalFormat) -> Self {
        match f {
            DecimalFormat::Plain => math_expressions::eval_numeric::certified_digits::DecimalFormat::Plain,
            DecimalFormat::Scientific => math_expressions::eval_numeric::certified_digits::DecimalFormat::Scientific,
        }
    }
}

#[wasm_bindgen]
impl Expression {
    /// Evaluate a constant expression to `digits` significant decimal digits
    /// (arbitrary precision). Renders per `format` — [`DecimalFormat::Plain`]
    /// (the default) or [`DecimalFormat::Scientific`]; `"re + im i"` for complex
    /// values, or `undefined` when not decidable within budget.
    pub fn evaluate_to_precision(
        &self,
        digits: usize,
        format: Option<DecimalFormat>,
    ) -> Option<String> {
        let fmt = format.unwrap_or(DecimalFormat::Plain).into();
        let p = math_expressions::eval_numeric::certified_digits::evaluate_to_precision(&self.0, digits);
        p.to_decimal_string_fmt(digits, fmt)
    }

    /// Indefinite integral in `var` (INTEGRATION_PLAN I1+I2), gate-verified
    /// by differentiation; `undefined` = no elementary form found.
    pub fn integrate(&self, var: &str) -> Option<Expression> {
        math_expressions::integrate(&self.0, var, &math_expressions::Assumptions::new())
            .map(|e| self.derive(e))
    }

    /// Certified definite integral over [a, b] to `digits` significant
    /// digits (guaranteed accuracy or `undefined` — never an estimate).
    pub fn integrate_to_precision(
        &self,
        var: &str,
        a: &Expression,
        b: &Expression,
        digits: usize,
    ) -> Option<String> {
        math_expressions::eval_numeric::certified_digits::integrate_to_precision(&self.0, var, &a.0, &b.0, digits)
            .to_decimal_string(digits)
    }

    /// Best-effort numeric definite integral over `[lower, upper]` — the port of
    /// the JS `integrateNumerically(var, lower, upper)`. Backed by the CERTIFIED
    /// `integrate_to_precision` reduced to an `f64`: returns the value when it
    /// can be certified, `undefined` when it cannot. This is the honest
    /// divergence from JS, which always returns a (possibly inaccurate) estimate
    /// — here a hard/divergent integrand yields `undefined` rather than a
    /// silently-wrong number.
    ///
    /// 10 significant digits (not the ≤13 certified max): ample for an f64
    /// estimate, with margin so near-cancellation cases — e.g. `∫₀^π sin`, which
    /// fails to certify at 13 — still return a value.
    pub fn integrate_numerically(&self, var: &str, lower: f64, upper: f64) -> Option<f64> {
        let a = Expr::Num(Number::from_f64(lower));
        let b = Expr::Num(Number::from_f64(upper));
        math_expressions::eval_numeric::certified_digits::integrate_to_precision(&self.0, var, &a, &b, 10).to_f64()
    }

    /// Three-way definite-integral analysis (DIVERGENCE_PLAN): JSON
    /// `{"status":"value","value":…}` |
    /// `{"status":"divergent","singularities":[{"location":…,"exact":…?}]}` |
    /// `{"status":"unknown","reason":…}`.
    pub fn integrate_analyzed(
        &self,
        var: &str,
        a: &Expression,
        b: &Expression,
        digits: usize,
    ) -> String {
        use math_expressions::eval_numeric::certified_digits::IntegralVerdict;
        let v = math_expressions::eval_numeric::certified_digits::integrate_analyzed(&self.0, var, &a.0, &b.0, digits);
        match v {
            IntegralVerdict::Value(p) => serde_json::json!({
                "status": "value",
                "value": p.to_f64(),
                "digits": p.to_decimal_string(digits),
            })
            .to_string(),
            IntegralVerdict::Divergent { at } => {
                let sing: Vec<serde_json::Value> = at
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "location": s.location,
                            "exact": s.exact.as_ref().map(|e| self.text_of(e)),
                        })
                    })
                    .collect();
                serde_json::json!({"status": "divergent", "singularities": sing}).to_string()
            }
            IntegralVerdict::Unknown(reason) => {
                serde_json::json!({"status": "unknown", "reason": reason}).to_string()
            }
        }
    }
}
