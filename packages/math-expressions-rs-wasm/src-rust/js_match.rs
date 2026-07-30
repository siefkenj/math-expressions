//! Template matching on JS trees — the port of `me.utils.match`
//! (`lib/trees/basic.js` `match`) in its **default mode**, which is the only
//! mode Doenet uses (`match(tree, template)` with no params):
//!
//! - operators and numbers must match exactly;
//! - every variable (string leaf) appearing in the pattern is a wildcard
//!   bound to a subtree;
//! - repeated wildcards must bind syntactically equal subtrees;
//! - for associative operators (`+ * and or union intersect`) and
//!   tuple/vector shapes, tree operands are flattened and a pattern wildcard
//!   may absorb a *group* of consecutive operands (rewrapped in the
//!   operator), with the last pattern operand absorbing the remainder;
//! - a unary minus of a product matches a `*` pattern with the minus moved
//!   onto the first factor (the JS special case).
//!
//! Not ported (never used by Doenet, all opt-in params in JS):
//! `allow_permutations`, `allow_extended_match`, `allow_implicit_identities`,
//! regex/function wildcard conditions. Binding consistency uses structural
//! JSON equality where the JS uses its syntactic `equal` — stricter in
//! corner cases (e.g. `1` vs `1.0` differ only in JS number spelling, which
//! JSON round-tripping already collapses).
//!
//! Operates on `serde_json::Value` JS trees (not `Expr`): Doenet passes raw
//! ASTs and consumes raw subtree bindings, and converting through the
//! canonical layer would change the trees being matched.

use serde_json::{Map, Value};
use std::collections::HashSet;

/// Is this operator associative in the JS tree sense (`flatten.is_associative`)?
fn is_associative(op: &str) -> bool {
    matches!(op, "+" | "*" | "and" | "or" | "union" | "intersect")
}

/// May a wildcard absorb a group of operands under this operator?
/// (JS: associative operators plus tuple/vector shapes.)
fn allows_groups(op: &str) -> bool {
    is_associative(op) || matches!(op, "tuple" | "vector" | "altvector")
}

fn head(tree: &Value) -> Option<&str> {
    tree.as_array()?.first()?.as_str()
}

/// All operands of `tree` as though nested same-operator applications had
/// been flattened (JS `flatten.allChildren`).
fn all_children<'a>(tree: &'a Value, out: &mut Vec<&'a Value>) {
    let Some(arr) = tree.as_array() else { return };
    let Some(op) = arr.first().and_then(Value::as_str) else {
        return;
    };
    for operand in &arr[1..] {
        if is_associative(op) && head(operand) == Some(op) {
            all_children(operand, out);
        } else {
            out.push(operand);
        }
    }
}

/// Collect the wildcard names of a pattern: every distinct string leaf in
/// operand position (mirrors JS `variables_in(pattern)`, which drops
/// operators and `apply` heads).
fn pattern_variables(pattern: &Value, out: &mut HashSet<String>) {
    match pattern {
        Value::String(s) => {
            out.insert(s.clone());
        }
        Value::Array(arr) => {
            let is_apply = arr.first().and_then(Value::as_str) == Some("apply");
            for (i, operand) in arr.iter().enumerate().skip(1) {
                // The function name of an `apply` is not a variable.
                if is_apply && i == 1 && operand.is_string() {
                    continue;
                }
                pattern_variables(operand, out);
            }
        }
        _ => {}
    }
}

/// Attempt to match `tree` against `pattern` (default mode — see module
/// docs). `Some(bindings)` maps each pattern wildcard to the subtree it
/// bound; `None` means no match. An exact variable-free match yields an
/// empty map.
pub fn match_template(tree: &Value, pattern: &Value) -> Option<Map<String, Value>> {
    let mut wildcards = HashSet::new();
    pattern_variables(pattern, &mut wildcards);
    match_inner(tree, pattern, &wildcards)
}

fn match_inner(
    tree: &Value,
    pattern: &Value,
    wildcards: &HashSet<String>,
) -> Option<Map<String, Value>> {
    // A wildcard binds the whole tree.
    if let Value::String(name) = pattern {
        if wildcards.contains(name) {
            let mut m = Map::new();
            m.insert(name.clone(), tree.clone());
            return Some(m);
        }
    }

    // Non-array pattern with no binding: leaves must be identical.
    // (Numbers compare as JSON values; `1` vs `1.0` both parse to the same
    // f64 and serde_json preserves the distinction only in spelling.)
    let Value::Array(parr) = pattern else {
        return leaf_eq(tree, pattern).then(Map::new);
    };
    let op = parr.first()?.as_str()?;
    let pattern_operands = &parr[1..];

    let mut tree_operands: Vec<&Value> = Vec::new();
    let matches_shape = head(tree) == Some(op);
    if matches_shape {
        all_children(tree, &mut tree_operands);
    }

    // JS special case: a `*` pattern also matches `-(a·b·…)`, with the
    // minus moved onto the first factor.
    let mut neg_first: Option<Value> = None;
    if (!matches_shape || tree_operands.len() < pattern_operands.len()) && op == "*" {
        if let Some(arr) = tree.as_array() {
            if arr.len() == 2 && arr[0].as_str() == Some("-") && head(&arr[1]) == Some("*") {
                tree_operands.clear();
                all_children(&arr[1], &mut tree_operands);
                // A degenerate nullary product `["*"]` leaves no factors to
                // carry the minus; leave `neg_first` unset so the `None` path
                // below is taken instead of indexing an empty vec (an abort
                // under `panic = "abort"` — `match_template` is `pub` and runs
                // on raw caller-supplied JS trees).
                if !tree_operands.is_empty() {
                    neg_first = Some(Value::Array(vec![
                        Value::String("-".to_string()),
                        tree_operands[0].clone(),
                    ]));
                }
            }
        }
    }
    if neg_first.is_none() && (!matches_shape || tree_operands.len() < pattern_operands.len()) {
        return None;
    }
    let owned_first = neg_first;
    let operand_at = |i: usize| -> &Value {
        match (&owned_first, i) {
            (Some(v), 0) => v,
            _ => tree_operands[i],
        }
    };

    match_operands(op, &tree_operands, operand_at, pattern_operands, wildcards)
}

/// Sequential operand matching with grouping (the JS default path of
/// `matchOperands`): pattern operand `i` tries absorbing 1..=max_group
/// consecutive tree operands (max_group > 1 only for group-allowing
/// operators); the last pattern operand must absorb the remainder exactly.
fn match_operands<'a>(
    op: &str,
    tree_operands: &[&'a Value],
    operand_at: impl Fn(usize) -> &'a Value + Copy,
    pattern_operands: &[Value],
    wildcards: &HashSet<String>,
) -> Option<Map<String, Value>> {
    fn chunk<'a>(
        op: &str,
        operand_at: impl Fn(usize) -> &'a Value,
        start: usize,
        len: usize,
    ) -> Value {
        if len == 1 {
            operand_at(start).clone()
        } else {
            let mut arr = vec![Value::String(op.to_string())];
            arr.extend((start..start + len).map(|i| operand_at(i).clone()));
            Value::Array(arr)
        }
    }

    fn consistent(a: &Map<String, Value>, b: &Map<String, Value>) -> bool {
        a.iter().all(|(k, v)| b.get(k).is_none_or(|w| v == w))
    }

    #[allow(clippy::too_many_arguments)]
    fn go<'a>(
        op: &str,
        n_tree: usize,
        operand_at: impl Fn(usize) -> &'a Value + Copy,
        pattern_operands: &[Value],
        wildcards: &HashSet<String>,
        start: usize,
        pat_ind: usize,
        acc: &Map<String, Value>,
    ) -> Option<Map<String, Value>> {
        let n_pats = pattern_operands.len();
        let remaining = n_tree - start;
        if pat_ind == n_pats {
            return (remaining == 0).then(|| acc.clone());
        }
        let last = pat_ind == n_pats - 1;
        let max_group = if allows_groups(op) {
            remaining.saturating_sub(n_pats - pat_ind - 1)
        } else {
            1
        };
        // The last pattern operand must absorb everything left (JS: no
        // extended match). For non-group operators that means exactly one.
        let sizes: Vec<usize> = if last {
            (remaining == max_group.max(1) && remaining >= 1)
                .then_some(remaining)
                .into_iter()
                .collect()
        } else {
            (1..=max_group).collect()
        };
        for size in sizes {
            let piece = chunk(op, operand_at, start, size);
            let Some(m) = match_inner(&piece, &pattern_operands[pat_ind], wildcards) else {
                continue;
            };
            if !consistent(&m, acc) {
                continue;
            }
            let mut combined = acc.clone();
            combined.extend(m);
            if let Some(result) = go(
                op,
                n_tree,
                operand_at,
                pattern_operands,
                wildcards,
                start + size,
                pat_ind + 1,
                &combined,
            ) {
                return Some(result);
            }
        }
        None
    }

    go(
        op,
        tree_operands.len(),
        operand_at,
        pattern_operands,
        wildcards,
        0,
        0,
        &Map::new(),
    )
}

/// Leaf equality: strings by identity, numbers by numeric value, booleans by
/// value (JS `tree === pattern`).
fn leaf_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

// ---- JS-tree shape utilities (ports of `me.utils.flatten`/`unflatten*`) ----

/// Flatten nested associative operators: `["+", ["+", a, b], c] → ["+", a, b, c]`.
pub fn flatten_tree(tree: &Value) -> Value {
    let Some(arr) = tree.as_array() else {
        return tree.clone();
    };
    let Some(op) = arr.first().and_then(Value::as_str) else {
        return tree.clone();
    };
    if is_associative(op) {
        let mut operands = Vec::new();
        all_children(tree, &mut operands);
        let mut out = vec![Value::String(op.to_string())];
        out.extend(operands.iter().map(|o| flatten_tree(o)));
        Value::Array(out)
    } else {
        let mut out = vec![arr[0].clone()];
        out.extend(arr[1..].iter().map(flatten_tree));
        Value::Array(out)
    }
}

/// Left-associate an n-ary associative operator:
/// `["+", a, b, c] → ["+", ["+", a, b], c]`.
pub fn unflatten_left(tree: &Value) -> Value {
    unflatten(tree, true)
}

/// Right-associate: `["+", a, b, c] → ["+", a, ["+", b, c]]`.
pub fn unflatten_right(tree: &Value) -> Value {
    unflatten(tree, false)
}

fn unflatten(tree: &Value, left: bool) -> Value {
    let Some(arr) = tree.as_array() else {
        return tree.clone();
    };
    let Some(op) = arr.first().and_then(Value::as_str) else {
        return tree.clone();
    };
    let operands: Vec<Value> = arr[1..].iter().map(|o| unflatten(o, left)).collect();
    if !is_associative(op) || operands.len() <= 2 {
        let mut out = vec![arr[0].clone()];
        out.extend(operands);
        return Value::Array(out);
    }
    let wrap = |a: Value, b: Value| Value::Array(vec![Value::String(op.to_string()), a, b]);
    let mut iter = operands.into_iter();
    if left {
        let first = iter.next().unwrap();
        iter.fold(first, wrap)
    } else {
        let all: Vec<Value> = iter.collect();
        let mut rev = all.into_iter().rev();
        let last = rev.next().unwrap();
        rev.fold(last, |acc, x| wrap(x, acc))
    }
}

#[cfg(test)]
mod tests {
    //! The JS-tree utility surface Doenet uses via `me.utils`: default-mode
    //! template `match`, `flatten`/`unflatten{Left,Right}` (all `js_match`),
    //! plus `expr::serde::to_js` structural equality and the crate `substitute`
    //! (core-crate items, exercised here through the same JS-tree surface).
    //! Ported from `spec/quick_trees.spec.js`; only the **default** match mode
    //! is ported (opt-in JS params are deliberately unported — see this file's
    //! module docs and JS_TEST_COVERAGE_AUDIT.md).
    use super::{flatten_tree, match_template, unflatten_left, unflatten_right};
    use math_expressions::expr::serde::to_js;
    use math_expressions::{equals, substitute, EqOptions, Expr, TextToAst, TextToAstOptions};
    use serde_json::{json, Value};
    use std::collections::HashMap;

    fn parse(s: &str) -> Expr {
        TextToAst::new(TextToAstOptions::default())
            .convert(s)
            .unwrap_or_else(|e| panic!("parse {s:?}: {e}"))
    }

    /// The JS `TREE(s)` helper: parse text and take the raw JS tree.
    fn tree(s: &str) -> Value {
        to_js(&parse(s))
    }

    /// Structural tree equality (JS `trees.equal`) is JSON identity of the encoding.
    fn equal(a: &Value, b: &Value) -> bool {
        a == b
    }

    fn eq_expr(a: &Expr, b: &Expr) -> bool {
        equals(a, b, &EqOptions::default())
    }

    // ---- tree basics ----

    #[test]
    fn structural_equality_is_exact_and_order_sensitive() {
        assert!(equal(&tree("cos x"), &tree("cos x")));
        assert!(!equal(&tree("cos x"), &tree("cos y")));
        // Structural equality does NOT allow order changes (that is `equals`).
        assert!(!equal(&tree("x+y"), &tree("y+x")));
    }

    #[test]
    fn flatten_and_unflatten() {
        // unflattenRight: ["+",1,2,3] -> ["+",1,["+",2,3]]
        assert_eq!(unflatten_right(&json!(["+", 1, 2, 3])), json!(["+", 1, ["+", 2, 3]]));
        // unflattenLeft: ["+",1,2,3] -> ["+",["+",1,2],3]
        assert_eq!(unflatten_left(&json!(["+", 1, 2, 3])), json!(["+", ["+", 1, 2], 3]));
        // flatten both nestings back to the n-ary form.
        assert_eq!(flatten_tree(&json!(["+", 1, ["+", 2, 3]])), json!(["+", 1, 2, 3]));
        assert_eq!(flatten_tree(&json!(["+", ["+", 1, 2], 3])), json!(["+", 1, 2, 3]));
    }

    #[test]
    fn substitute_symbols() {
        let sub = |e: &str, pairs: &[(&str, Expr)]| {
            let map: HashMap<String, Expr> =
                pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
            substitute(&parse(e), &map)
        };

        // x+y becomes 1+2 when x:=1 and y:=2
        assert!(eq_expr(
            &sub("x+y", &[("x", parse("1")), ("y", parse("2"))]),
            &parse("1+2")
        ));
        // simultaneous swap: x := y^2 and y := x^2
        assert!(eq_expr(
            &sub("x+y", &[("x", parse("y^2")), ("y", parse("x^2"))]),
            &parse("y^2 + x^2")
        ));
        // recurses through apply / div
        assert!(eq_expr(
            &sub("cos(x+y)/sin(x*y)", &[("x", parse("1")), ("y", parse("2"))]),
            &parse("cos(1+2)/sin(1*2)")
        ));
        // recurses through relations (chained inequality)
        assert!(eq_expr(
            &sub("x < y < z", &[("x", parse("a")), ("y", parse("b")), ("z", parse("c"))]),
            &parse("a < b < c")
        ));
        assert!(eq_expr(
            &sub("x < y <= z", &[("x", parse("a")), ("y", parse("b")), ("z", parse("c"))]),
            &parse("a < b <= c")
        ));
    }

    // ---- default-mode template matching ----

    #[test]
    fn match_binds_wildcards() {
        let m = match_template(&tree("x+y"), &tree("a+b")).expect("x+y matches a+b");
        assert_eq!(m.get("a"), Some(&json!("x")));
        assert_eq!(m.get("b"), Some(&json!("y")));
    }

    #[test]
    fn match_requires_same_operator_and_whole_tree() {
        // x+y does not match a*b
        assert!(match_template(&tree("x+y"), &tree("a*b")).is_none());
        // a wildcard match must cover the entire tree
        assert!(match_template(&tree("x+y/z"), &tree("a/b")).is_none());
    }

    #[test]
    fn match_must_be_consistent() {
        // x+y/z matches a+b/c (all distinct) ...
        assert!(match_template(&tree("x+y/z"), &tree("a+b/c")).is_some());
        // ... but not a+b/a (would need y/z's numerator == denominator)
        assert!(match_template(&tree("x+y/z"), &tree("a+b/a")).is_none());
        // x+y/x DOES match a+b/a (x bound consistently)
        assert!(match_template(&tree("x+y/x"), &tree("a+b/a")).is_some());
    }

    #[test]
    fn match_multichar_placeholders_and_exact_numbers() {
        // multi-character pattern leaves are still wildcards by default
        assert!(match_template(&json!(["+", "x", "y"]), &json!(["+", "a", "bc"])).is_some());
        assert!(match_template(&json!(["+", "x", "bc"]), &json!(["+", "a", "bc"])).is_some());
        // numbers must match exactly
        assert!(match_template(&tree("3x+5"), &tree("ab+5")).is_some());
        assert!(match_template(&tree("3x+5"), &tree("ab+6")).is_none());
    }

    #[test]
    fn match_addition_matches_subtraction_not_vice_versa() {
        // x-y is ["+","x",["-","y"]]; a wildcard b absorbs the negated term.
        assert!(match_template(&tree("x-y"), &tree("a+b")).is_some());
        // but x+y cannot match a-b (the second operand must be a negation)
        assert!(match_template(&tree("x+y"), &tree("a-b")).is_none());
    }

    #[test]
    fn match_template_default_mode() {
        // ["+", ["*", 2, "x"], 3] against ["+", ["*", "a", "x"], "b"]:
        // wildcards a, x, b (all pattern variables).
        let tree = json!(["+", ["*", 2, "x"], 3]);
        let pat = json!(["+", ["*", "a", "y"], "b"]);
        let m = match_template(&tree, &pat).unwrap();
        assert_eq!(m.get("a").unwrap(), &json!(2));
        assert_eq!(m.get("y").unwrap(), &json!("x"));
        assert_eq!(m.get("b").unwrap(), &json!(3));

        // Grouping: last wildcard absorbs the rest of an associative operator.
        let tree = json!(["+", 1, 2, 3]);
        let m = match_template(&tree, &json!(["+", "u", "v"])).unwrap();
        assert_eq!(m.get("u").unwrap(), &json!(1));
        assert_eq!(m.get("v").unwrap(), &json!(["+", 2, 3]));

        // Repeated wildcard must bind equal subtrees.
        assert!(match_template(&json!(["+", "x", "x"]), &json!(["+", "u", "u"])).is_some());
        assert!(match_template(&json!(["+", "x", "y"]), &json!(["+", "u", "u"])).is_none());

        // Unary minus of product matches a * pattern.
        let tree = json!(["-", ["*", "x", "y"]]);
        let m = match_template(&tree, &json!(["*", "a", "b"])).unwrap();
        assert_eq!(m.get("a").unwrap(), &json!(["-", "x"]));
        assert_eq!(m.get("b").unwrap(), &json!("y"));

        // Operators must match exactly; no match across operators.
        assert!(match_template(&json!(["*", 1, 2]), &json!(["+", "u", "v"])).is_none());
        // Exact variable-free match -> empty bindings.
        assert_eq!(
            match_template(&json!(["+", 1, 2]), &json!(["+", 1, 2]))
                .unwrap()
                .len(),
            0
        );
    }

    /// `match_template` is `pub` and runs on raw caller-supplied JS trees. A
    /// degenerate `["-", ["*"]]` (unary minus of a nullary product) against any
    /// `["*", …]` pattern used to index an empty operand vec → abort under
    /// `panic = "abort"`. It must now cleanly return `None`.
    #[test]
    fn match_template_nullary_product_minus_does_not_abort() {
        assert_eq!(
            match_template(&json!(["-", ["*"]]), &json!(["*", "a"])),
            None
        );
    }

    /// Differential corpus generated from the JS oracle (`me.utils.match`) by
    /// `scripts/generate-numeric-corpus.mjs`. The fixture is shared with the
    /// core crate's numeric corpus and lives there; read it across the crate
    /// boundary (the `match` slice is the only part `js_match` owns).
    fn match_corpus() -> Value {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../math-expressions-rs/tests/fixtures/numeric-corpus.json"
        ))
        .expect("run scripts/generate-numeric-corpus.mjs first");
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn match_agrees_with_js_default_mode() {
        for case in match_corpus()["match"].as_array().unwrap() {
            let got = match_template(&case["tree"], &case["pattern"]);
            match (&case["bindings"], got) {
                (Value::Null, None) => {}
                (Value::Null, Some(m)) => panic!(
                    "JS found no match but we bound {:?} in {case}",
                    Value::Object(m)
                ),
                (expected, None) => panic!("JS bound {expected} but we found no match in {case}"),
                (expected, Some(m)) => {
                    let exp = expected.as_object().unwrap();
                    assert_eq!(
                        exp.len(),
                        m.len(),
                        "binding sets differ in {case}: JS {expected}, ours {:?}",
                        Value::Object(m.clone())
                    );
                    for (k, v) in exp {
                        assert_eq!(
                            m.get(k),
                            Some(v),
                            "binding {k} differs in {case}: ours {:?}",
                            Value::Object(m.clone())
                        );
                    }
                }
            }
        }
    }
}
