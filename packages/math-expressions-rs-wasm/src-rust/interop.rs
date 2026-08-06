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
///
/// A search that runs past `MAX_MATCH_STEPS` is an **error**, not `undefined`:
/// "we gave up" and "it does not match" grade differently.
#[wasm_bindgen]
pub fn match_template(tree_json: &str, pattern_json: &str) -> Result<Option<String>, JsError> {
    let Ok(tree) = serde_json::from_str::<serde_json::Value>(tree_json) else {
        return Ok(None);
    };
    let Ok(pattern) = serde_json::from_str::<serde_json::Value>(pattern_json) else {
        return Ok(None);
    };
    match crate::js_match::match_template(&tree, &pattern) {
        Ok(m) => Ok(m.map(|m| serde_json::Value::Object(m).to_string())),
        Err(_) => Err(JsError::new(
            "match: search budget exceeded — the tree is too large to match \
             against this pattern (try without allow_permutations)",
        )),
    }
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
///   operator's identity when the tree has no operand for them, or `true` for
///   every declared parameter.
///
/// Malformed options are an error rather than a silent fall-back to the
/// defaults, for the same reason they are on the equality entry points: a
/// match that silently ignored its parameter list produced confidently wrong
/// bindings. That covers the *shape* as well as the JSON syntax — an
/// ill-typed `variables` used to fall back to "every string leaf in the
/// pattern is a wildcard", which is the most permissive mode there is, so a
/// caller who misspelled the option got looser matching and no warning.
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

    // `null` and `{}` both mean "no options"; anything else that is not an
    // object is a caller mistake.
    let empty = serde_json::Map::new();
    let obj = match &v {
        serde_json::Value::Null => &empty,
        serde_json::Value::Object(o) => o,
        other => {
            return Err(JsError::new(&format!(
                "match: options must be an object or null, got {other}"
            )))
        }
    };
    if let Some(unknown) = obj.keys().find(|k| {
        !matches!(
            k.as_str(),
            "variables" | "allow_permutations" | "allow_implicit_identities"
        )
    }) {
        return Err(JsError::new(&format!(
            "match: unknown option {unknown:?} (expected \"variables\", \
             \"allow_permutations\" or \"allow_implicit_identities\")"
        )));
    }

    let mut opts = crate::js_match::MatchOptions::default();
    if let Some(raw) = obj.get("variables") {
        let vars = raw.as_object().ok_or_else(|| {
            JsError::new(&format!(
                "match: 'variables' must be an object mapping each parameter \
                 name to its kind, got {raw}"
            ))
        })?;
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
    if let Some(raw) = obj.get("allow_permutations") {
        opts.allow_permutations = raw.as_bool().ok_or_else(|| {
            JsError::new(&format!(
                "match: 'allow_permutations' must be a boolean, got {raw}"
            ))
        })?;
    }
    if let Some(raw) = obj.get("allow_implicit_identities") {
        match raw {
            // `true` means every declared parameter. Expanded here rather than
            // by the caller because the default wildcard set is the *pattern's*
            // string leaves, which only the matcher knows.
            serde_json::Value::Bool(all) => opts.implicit_identities_all = *all,
            serde_json::Value::Array(names) => {
                let mut set = std::collections::HashSet::new();
                for n in names {
                    let name = n.as_str().ok_or_else(|| {
                        JsError::new(&format!(
                            "match: 'allow_implicit_identities' entries must be \
                             parameter names, got {n}"
                        ))
                    })?;
                    set.insert(name.to_string());
                }
                opts.implicit_identities = set;
            }
            other => {
                return Err(JsError::new(&format!(
                    "match: 'allow_implicit_identities' must be an array of \
                     parameter names or true, got {other}"
                )))
            }
        }
    }

    match crate::js_match::match_template_with_options(&tree, &pattern, &opts) {
        Ok(m) => Ok(m.map(|m| serde_json::Value::Object(m).to_string())),
        Err(_) => Err(JsError::new(
            "match: search budget exceeded — the tree is too large to match \
             against this pattern (try without allow_permutations)",
        )),
    }
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
