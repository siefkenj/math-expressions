//! JS-tree AST boundary (Doenet interop): construction from / serialization to
//! the array-AST and `toJSON` shapes, plus the `me.utils` match/flatten ports.

use super::Expression;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl Expression {
    /// Serialize in the JS library's `toJSON` shape:
    /// `{"objectType": "math-expression", "tree": ...}` — revive with
    /// [`from_serialized`] (or the JS `me.reviver`).
    pub fn to_serialized(&self) -> String {
        serde_json::json!({
            "objectType": "math-expression",
            "tree": math_expressions::expr::serde::to_js(&self.0),
        })
        .to_string()
    }
}

/// Build an `Expression` from a JS-tree AST (JSON) — the port of
/// `me.fromAst`. Accepts the array AST format Doenet manipulates directly.
#[wasm_bindgen]
pub fn from_ast(tree_json: &str) -> Result<Expression, JsError> {
    let value: serde_json::Value =
        serde_json::from_str(tree_json).map_err(|e| JsError::new(&e.to_string()))?;
    math_expressions::expr::serde::try_from_js(&value)
        .map(Expression::with_default_notation)
        .map_err(|e| JsError::new(&e))
}

/// Revive an expression serialized by [`Expression::to_serialized`] (or by
/// the JS library's `toJSON`) — the port of `me.reviver`'s object shape:
/// `{"objectType": "math-expression", "tree": ...}`.
#[wasm_bindgen]
pub fn from_serialized(json: &str) -> Result<Expression, JsError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| JsError::new(&e.to_string()))?;
    if value.get("objectType").and_then(serde_json::Value::as_str) != Some("math-expression") {
        return Err(JsError::new("not a serialized math-expression"));
    }
    let tree = value
        .get("tree")
        .ok_or_else(|| JsError::new("missing tree"))?;
    math_expressions::expr::serde::try_from_js(tree)
        .map(Expression::with_default_notation)
        .map_err(|e| JsError::new(&e))
}

/// Template match on JS-tree ASTs — the port of `me.utils.match` in its
/// default mode. Returns the bindings object as JSON (wildcard name →
/// subtree), or `undefined` if the tree does not match the pattern.
#[wasm_bindgen]
pub fn match_template(tree_json: &str, pattern_json: &str) -> Option<String> {
    let tree: serde_json::Value = serde_json::from_str(tree_json).ok()?;
    let pattern: serde_json::Value = serde_json::from_str(pattern_json).ok()?;
    crate::js_match::match_template(&tree, &pattern)
        .map(|m| serde_json::Value::Object(m).to_string())
}

/// [`match_template`] with the JS `match` options honored.
///
/// `options_json` keys, all optional:
/// - `variables`: object mapping each declared parameter to its kind —
///   `true`/`"any"`, `"number"`, or `"variable"`. **Present and empty means no
///   parameters**, so nothing binds; absent keeps the legacy default where
///   every string leaf in the pattern is a wildcard.
/// - `allow_permutations`: match `+`/`*` operands in any order.
/// - `allow_implicit_identities`: array of parameter names that may take the
///   operator's identity when the tree has no operand for them.
///
/// Malformed JSON is an error rather than a silent fall-back to the defaults,
/// for the same reason it is on the equality entry points: a match that
/// silently ignored its parameter list produced confidently wrong bindings.
#[wasm_bindgen]
pub fn match_template_with_options(
    tree_json: &str,
    pattern_json: &str,
    options_json: &str,
) -> Result<Option<String>, JsError> {
    let tree: serde_json::Value =
        serde_json::from_str(tree_json).map_err(|e| JsError::new(&e.to_string()))?;
    let pattern: serde_json::Value =
        serde_json::from_str(pattern_json).map_err(|e| JsError::new(&e.to_string()))?;
    let v: serde_json::Value =
        serde_json::from_str(options_json).map_err(|e| JsError::new(&e.to_string()))?;

    let mut opts = crate::js_match::MatchOptions::default();
    if let Some(vars) = v.get("variables").and_then(|x| x.as_object()) {
        let mut declared = std::collections::HashMap::new();
        for (name, kind) in vars {
            let kind = match kind {
                serde_json::Value::String(s) => match s.as_str() {
                    "number" => crate::js_match::VarKind::Number,
                    "variable" => crate::js_match::VarKind::Variable,
                    "any" => crate::js_match::VarKind::Any,
                    other => {
                        return Err(JsError::new(&format!(
                            "match: unknown parameter kind {other:?} for {name:?} \
                             (expected \"number\", \"variable\", \"any\", or true)"
                        )))
                    }
                },
                // `true` is the legacy "any subtree"; `false` declares the name
                // and then admits nothing, which is never what a caller means.
                serde_json::Value::Bool(true) => crate::js_match::VarKind::Any,
                other => {
                    return Err(JsError::new(&format!(
                        "match: invalid parameter kind {other} for {name:?}"
                    )))
                }
            };
            declared.insert(name.clone(), kind);
        }
        opts.variables = Some(declared);
    }
    if let Some(b) = v.get("allow_permutations").and_then(|x| x.as_bool()) {
        opts.allow_permutations = b;
    }
    if let Some(names) = v.get("allow_implicit_identities").and_then(|x| x.as_array()) {
        opts.implicit_identities = names
            .iter()
            .filter_map(|n| n.as_str().map(str::to_string))
            .collect();
    }

    Ok(
        crate::js_match::match_template_with_options(&tree, &pattern, &opts)
            .map(|m| serde_json::Value::Object(m).to_string()),
    )
}

/// `me.utils.flatten` on a JS-tree AST (JSON in, JSON out).
#[wasm_bindgen]
pub fn flatten_ast(tree_json: &str) -> Option<String> {
    let tree: serde_json::Value = serde_json::from_str(tree_json).ok()?;
    Some(crate::js_match::flatten_tree(&tree).to_string())
}

/// `me.utils.unflattenLeft`.
#[wasm_bindgen]
pub fn unflatten_left(tree_json: &str) -> Option<String> {
    let tree: serde_json::Value = serde_json::from_str(tree_json).ok()?;
    Some(crate::js_match::unflatten_left(&tree).to_string())
}

/// `me.utils.unflattenRight`.
#[wasm_bindgen]
pub fn unflatten_right(tree_json: &str) -> Option<String> {
    let tree: serde_json::Value = serde_json::from_str(tree_json).ok()?;
    Some(crate::js_match::unflatten_right(&tree).to_string())
}
