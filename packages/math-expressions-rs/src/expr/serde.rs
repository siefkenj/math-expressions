//! Converter to and from the JS `Tree` JSON shape. ALL the ad-hoc JS
//! encodings live here: parallel bool-tuples for chained inequalities, boolean
//! interval-closure leaves, the "＿" blank symbol, single-arg apply with tuple
//! wrapping.
//!
//! Infinity/NaN cannot be represented in JSON; they are encoded as
//! {"$": "Inf"} / {"$": "-Inf"} / {"$": "NaN"}, matching the fixture
//! extraction script (a JS Tree never contains plain objects, so this is
//! unambiguous).

use crate::expr::{Expr, MathConst, RelOp, SeqKind};
use crate::num::Number;
use serde_json::{json, Value};

// Recursion is deliberately not depth-capped here: the realistic input path is
// a JSON string deserialized by `serde_json`, whose own recursion limit (128)
// rejects deeply-nested input before a `Value` is built, so this never sees a
// tree deep enough to overflow. A hand-constructed `Value` could, but that is
// not a user-input vector.
/// Parse a JS `Tree` JSON value into an `Expr`. Inverse of [`to_js`] for the
/// tree shapes the parsers produce (Rat is not reconstructed — a `["/", a, b]`
/// node becomes `Div`, matching the parser). Malformed shapes return an `Err`
/// description and never panic (wasm builds abort on panic, so a bad tree from
/// JS must not unwind; trusted callers such as test fixtures just `.expect()`).
pub fn try_from_js(value: &Value) -> Result<Expr, String> {
    match value {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Expr::Num(Number::Int(i)))
            } else {
                Ok(Expr::Num(Number::from_f64(
                    n.as_f64().ok_or("non-finite JSON number")?,
                )))
            }
        }
        Value::Bool(b) => Ok(Expr::Bool(*b)),
        Value::String(s) => Ok(Expr::sym(s)),
        Value::Object(_) => match value.get("$").and_then(Value::as_str) {
            Some("Inf") => Ok(Expr::Const(MathConst::Inf)),
            Some("-Inf") => Ok(Expr::Const(MathConst::NegInf)),
            Some("NaN") => Ok(Expr::Const(MathConst::NaN)),
            other => Err(format!("unknown special {other:?}")),
        },
        Value::Array(arr) => from_js_array(arr),
        other => Err(format!("unexpected value {other}")),
    }
}

fn from_js_array(arr: &[Value]) -> Result<Expr, String> {
    let head = arr
        .first()
        .ok_or("empty array is not a tree")?
        .as_str()
        .ok_or("array head must be an operator string")?;
    let operands = &arr[1..];
    let each = || -> Result<Vec<Expr>, String> { operands.iter().map(try_from_js).collect() };
    let boxed = |i: usize| -> Result<Box<Expr>, String> {
        Ok(Box::new(try_from_js(operands.get(i).ok_or_else(
            || format!("operator {head:?} is missing operand {i}"),
        )?)?))
    };

    if let Some(kind) = seq_kind(head) {
        return Ok(Expr::Seq(kind, each()?));
    }
    if let Some(op) = rel_op(head) {
        // binary or chained-equality relation
        let operands = each()?;
        if operands.is_empty() {
            return Err(format!("relation {head:?} has no operands"));
        }
        let ops = vec![op; operands.len() - 1];
        return Ok(Expr::Relation { operands, ops });
    }

    Ok(match head {
        "+" => Expr::Add(each()?),
        "*" => Expr::Mul(each()?),
        "/" => Expr::Div(boxed(0)?, boxed(1)?),
        "^" => Expr::Pow(boxed(0)?, boxed(1)?),
        "-" => Expr::Neg(boxed(0)?),
        "and" => Expr::And(each()?),
        "or" => Expr::Or(each()?),
        "not" => Expr::Not(boxed(0)?),
        "union" => Expr::Union(each()?),
        "intersect" => Expr::Intersect(each()?),
        "prime" => Expr::Prime(boxed(0)?),
        "_" => Expr::Index(boxed(0)?, boxed(1)?),
        "ldots" => Expr::Ldots,
        "apply" => {
            let f = boxed(0)?;
            let arg = operands.get(1).ok_or("apply is missing its argument")?;
            let args = match arg.as_array() {
                Some(a) if a.first().and_then(Value::as_str) == Some("tuple") => {
                    a[1..].iter().map(try_from_js).collect::<Result<_, _>>()?
                }
                _ => vec![try_from_js(arg)?],
            };
            Expr::Apply(f, args)
        }
        "interval" => {
            let ep = tuple3(operands.first(), "interval endpoints")?;
            let cl = tuple3(operands.get(1), "interval closed")?;
            Expr::Interval {
                endpoints: Box::new((try_from_js(&ep[1])?, try_from_js(&ep[2])?)),
                closed: (
                    cl[1].as_bool().unwrap_or(false),
                    cl[2].as_bool().unwrap_or(false),
                ),
            }
        }
        "lts" | "gts" => {
            let args = operands
                .first()
                .and_then(Value::as_array)
                .ok_or("lts/gts args")?;
            let strict = operands
                .get(1)
                .and_then(Value::as_array)
                .ok_or("lts/gts strict")?;
            if args.len() < 2 || strict.len() != args.len() - 1 {
                return Err("lts/gts args/strict length mismatch".to_string());
            }
            let operands: Vec<Expr> = args[1..]
                .iter()
                .map(try_from_js)
                .collect::<Result<_, _>>()?;
            let ops = strict[1..]
                .iter()
                .map(|b| {
                    let s = b.as_bool().unwrap_or(false);
                    match (head, s) {
                        ("lts", true) => RelOp::Lt,
                        ("lts", false) => RelOp::Le,
                        (_, true) => RelOp::Gt,
                        (_, false) => RelOp::Ge,
                    }
                })
                .collect();
            Expr::Relation { operands, ops }
        }
        "matrix" => {
            let size = tuple3(operands.first(), "matrix size")?;
            let body = operands
                .get(1)
                .and_then(Value::as_array)
                .ok_or("matrix body")?;
            let rows = size[1].as_u64().ok_or("matrix rows")? as u32;
            let cols = size[2].as_u64().ok_or("matrix cols")? as u32;
            if rows.saturating_mul(cols) > 1_000_000 {
                return Err("matrix too large".to_string());
            }
            let mut entries = Vec::with_capacity((rows * cols) as usize);
            for r in 0..rows as usize {
                let row = body
                    .get(r + 1)
                    .and_then(Value::as_array)
                    .ok_or("matrix row")?;
                for c in 0..cols as usize {
                    entries.push(try_from_js(row.get(c + 1).ok_or("matrix entry")?)?);
                }
            }
            Expr::Matrix {
                rows,
                cols,
                entries,
            }
        }
        // everything else (unit, pm, angle, binom, vec, linesegment,
        // derivative_leibniz, forall, arrows, implies, iff, perp, ":", "|", d)
        other => Expr::OtherOp(crate::expr::sym::Sym::new(other), each()?),
    })
}

/// A `["tuple", a, b]`-shaped 3-element array (head + two entries).
fn tuple3<'a>(v: Option<&'a Value>, what: &str) -> Result<&'a Vec<Value>, String> {
    let arr = v.and_then(Value::as_array).ok_or_else(|| what.to_string())?;
    if arr.len() < 3 {
        return Err(format!("{what}: expected 3 elements"));
    }
    Ok(arr)
}

fn seq_kind(name: &str) -> Option<SeqKind> {
    Some(match name {
        "tuple" => SeqKind::Tuple,
        "array" => SeqKind::Array,
        "list" => SeqKind::List,
        "set" => SeqKind::Set,
        "vector" => SeqKind::Vector,
        "altvector" => SeqKind::AltVector,
        _ => return None,
    })
}

fn rel_op(name: &str) -> Option<RelOp> {
    Some(match name {
        "=" => RelOp::Eq,
        "ne" => RelOp::Ne,
        "<" => RelOp::Lt,
        ">" => RelOp::Gt,
        "le" => RelOp::Le,
        "ge" => RelOp::Ge,
        "in" => RelOp::In,
        "notin" => RelOp::NotIn,
        "ni" => RelOp::Ni,
        "notni" => RelOp::NotNi,
        "subset" => RelOp::Subset,
        "notsubset" => RelOp::NotSubset,
        "subseteq" => RelOp::SubsetEq,
        "notsubseteq" => RelOp::NotSubsetEq,
        "superset" => RelOp::Superset,
        "notsuperset" => RelOp::NotSuperset,
        "superseteq" => RelOp::SupersetEq,
        "notsuperseteq" => RelOp::NotSupersetEq,
        _ => return None,
    })
}

/// Serialize an `Expr` to the JS `Tree` JSON shape. Flattens first: parsing is
/// now faithful (keeps raw associative grouping), but the JS reference AST is
/// flat (`["+", a, b, c]`, not `["+", ["+", a, b], c]`), so consumers such as
/// the wasm `tree_json` stay JS-compatible. Idempotent on already-flat trees.
pub fn to_js(expr: &Expr) -> Value {
    to_js_rec(&crate::expr::flatten(expr.clone()))
}

fn to_js_rec(expr: &Expr) -> Value {
    match expr {
        Expr::Num(n) => number_to_js(n),
        // Serialized as its `rootof(p(t), k)` application; deserialization
        // re-canonicalizes that back into the leaf.
        Expr::RootOf { poly, index } => to_js_rec(&crate::polynomials::rootof::as_apply(poly, *index)),
        Expr::Sym(s) => Value::String(s.name()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Blank => Value::String("\u{ff3f}".to_string()),
        Expr::Ldots => json!(["ldots"]),
        Expr::Const(c) => match c {
            crate::expr::MathConst::Inf => json!({"$": "Inf"}),
            crate::expr::MathConst::NegInf => json!({"$": "-Inf"}),
            crate::expr::MathConst::NaN => json!({"$": "NaN"}),
            crate::expr::MathConst::Pi => Value::String("pi".to_string()),
            crate::expr::MathConst::E => Value::String("e".to_string()),
            crate::expr::MathConst::I => Value::String("i".to_string()),
        },

        Expr::Add(args) => op("+", args),
        Expr::Mul(args) => op("*", args),
        Expr::Div(a, b) => json!(["/", to_js_rec(a), to_js_rec(b)]),
        Expr::Pow(a, b) => json!(["^", to_js_rec(a), to_js_rec(b)]),
        Expr::Neg(a) => json!(["-", to_js_rec(a)]),

        Expr::And(args) => op("and", args),
        Expr::Or(args) => op("or", args),
        Expr::Not(a) => json!(["not", to_js_rec(a)]),
        Expr::Union(args) => op("union", args),
        Expr::Intersect(args) => op("intersect", args),

        Expr::Apply(head, args) => {
            // JS applies take exactly one argument; multiple args are a tuple.
            let arg = if args.len() == 1 {
                to_js_rec(&args[0])
            } else {
                op("tuple", args)
            };
            json!(["apply", to_js_rec(head), arg])
        }

        Expr::Prime(a) => json!(["prime", to_js_rec(a)]),
        Expr::Index(a, b) => json!(["_", to_js_rec(a), to_js_rec(b)]),

        Expr::Seq(kind, args) => op(kind.js_name(), args),

        Expr::Interval { endpoints, closed } => json!([
            "interval",
            ["tuple", to_js_rec(&endpoints.0), to_js_rec(&endpoints.1)],
            ["tuple", closed.0, closed.1]
        ]),

        Expr::Relation { operands, ops } => relation_to_js(operands, ops),

        Expr::Matrix {
            rows,
            cols,
            entries,
        } => {
            // ["matrix", ["tuple", rows, cols], ["tuple", <row-tuples>]]
            let ncols = *cols as usize;
            let mut body = vec![Value::String("tuple".to_string())];
            for r in 0..*rows as usize {
                let mut row = vec![Value::String("tuple".to_string())];
                for c in 0..ncols {
                    row.push(to_js_rec(&entries[r * ncols + c]));
                }
                body.push(Value::Array(row));
            }
            json!(["matrix", ["tuple", rows, cols], Value::Array(body)])
        }

        Expr::OtherOp(name, args) => {
            let mut v = vec![Value::String(name.name())];
            v.extend(args.iter().map(to_js_rec));
            Value::Array(v)
        }
    }
}

fn op(name: &str, args: &[Expr]) -> Value {
    let mut v = vec![Value::String(name.to_string())];
    v.extend(args.iter().map(to_js_rec));
    Value::Array(v)
}

fn number_to_js(n: &Number) -> Value {
    match n {
        Number::Int(i) => json!(i),
        Number::Float(_) => f64_to_js(n.to_f64()),
        // Exact rationals split on whether their decimal expansion terminates.
        //
        // A *terminating* one (denominator 2^a·5^b) keeps its positional
        // spelling, because it is indistinguishable from a decimal literal:
        // user-typed decimals parse to exact rationals, so `0.5` and `1/2` are
        // the same `Number::Rat(1, 2)`. Emitting `["/", …]` here would turn
        // `19.9` into `["/", 199, 10]` — the fraction/decimal distinction is
        // already gone by this point and cannot be recovered at the boundary.
        //
        // A *non*-terminating one (`1/3`, `5/6`) has no such ambiguity: it can
        // never have come from a decimal literal, and the f64 projection loses
        // it irreversibly (`0.3333333333333333` does not come back). The JS
        // trees spell these `["/", 1, 3]`, so this is also the faithful shape.
        Number::Rat(..) | Number::Big(_) => match exact_ratio(n) {
            Some((num, den)) => json!(["/", num, den]),
            None => f64_to_js(n.to_f64()),
        },
    }
}

/// The largest integer a JS number holds exactly (2^53 − 1). Past it a
/// `["/", num, den]` pair is no more recoverable on the JS side than the f64
/// projection is, so there is nothing to gain by emitting it.
const JS_MAX_SAFE_INT: u64 = 9_007_199_254_740_991;

/// Numerator/denominator for a rational that must *not* be decimalized.
/// `None` when the value terminates as a decimal (it keeps the positional
/// spelling) or when the parts exceed JS's exact-integer range.
///
/// The `Rat` normal form puts the sign on the numerator with `den > 0`, so
/// negatives come out as `["/", -2, 3]` — the spelling the JS fixtures use.
fn exact_ratio(n: &Number) -> Option<(i64, i64)> {
    if n.terminating_decimal().is_some() {
        return None;
    }
    let (num, den) = n.rational_parts()?;
    let num: i64 = num.parse().ok()?;
    let den: i64 = den.parse().ok()?;
    (num.unsigned_abs() <= JS_MAX_SAFE_INT && den.unsigned_abs() <= JS_MAX_SAFE_INT)
        .then_some((num, den))
}

/// Serialise an f64 the way a JS `Tree` holds a number: integral values as
/// ints (`JSON.stringify(3.0) === "3"`), non-finite as the `{"$": ...}`
/// specials the fixture extraction uses (JSON has no infinity/NaN).
fn f64_to_js(v: f64) -> Value {
    if v.is_nan() {
        return json!({ "$": "NaN" });
    }
    if v.is_infinite() {
        return json!({ "$": if v > 0.0 { "Inf" } else { "-Inf" } });
    }
    if v.fract() == 0.0 && v.abs() < 9e15 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

fn relation_to_js(operands: &[Expr], ops: &[RelOp]) -> Value {
    if ops.len() == 1 {
        return json!([ops[0].js_name(), to_js_rec(&operands[0]), to_js_rec(&operands[1])]);
    }
    if ops.iter().all(|o| *o == RelOp::Eq) {
        // Chained equality: ["=", a, b, c, ...]
        let mut v = vec![Value::String("=".to_string())];
        v.extend(operands.iter().map(to_js_rec));
        return Value::Array(v);
    }
    // Chained inequalities: ["lts"/"gts", ["tuple", ...operands],
    // ["tuple", ...strict-flags]] where strict means < or > (not <=/>=).
    let (head, strict_op) = if ops.iter().all(|o| matches!(o, RelOp::Lt | RelOp::Le)) {
        ("lts", RelOp::Lt)
    } else if ops.iter().all(|o| matches!(o, RelOp::Gt | RelOp::Ge)) {
        ("gts", RelOp::Gt)
    } else {
        unreachable!("parser nests mixed-direction relation chains");
    };
    let mut args = vec![Value::String("tuple".to_string())];
    args.extend(operands.iter().map(to_js_rec));
    let mut strict = vec![Value::String("tuple".to_string())];
    strict.extend(ops.iter().map(|o| Value::Bool(*o == strict_op)));
    json!([head, args, strict])
}

#[cfg(test)]
mod tests {
    use super::*;

    // A chained inequality `["gts"/"lts", ["tuple", ...operands],
    // ["tuple", ...strict-flags]]` has one more operand than strict-flag, so
    // the tuple-with-head arrays satisfy `strict.len() == args.len() - 1`.
    // Regression: an off-by-one in that check used to reject every chained
    // inequality, panicking `from_js` (the ast-to-{latex,text} formatter path).
    #[test]
    fn chained_inequality_from_js_round_trips() {
        for head in ["gts", "lts"] {
            let tree = json!([
                head,
                ["tuple", "x", "y", "z"],
                ["tuple", true, false]
            ]);
            let expr = try_from_js(&tree).expect("chained inequality should parse");
            let Expr::Relation { operands, ops } = &expr else {
                panic!("expected Relation, got {expr:?}");
            };
            assert_eq!(operands.len(), 3);
            assert_eq!(ops.len(), 2);
            // to_js is the inverse for this shape.
            assert_eq!(to_js_rec(&expr), tree);
        }
    }

    /// A rational whose decimal expansion does not terminate crosses to JS as
    /// `["/", num, den]`, not as a truncated f64. `1/3` used to go out as
    /// `0.3333333333333333`, which nothing on the JS side can turn back into a
    /// third — an irreversible loss on every state save/load, not merely a
    /// display defect.
    #[test]
    fn non_terminating_rationals_cross_as_exact_fractions() {
        for (num, den) in [(1, 3), (5, 6), (-2, 3), (-1, 7), (22, 7)] {
            let n = Number::rat(num, den);
            assert_eq!(
                number_to_js(&n),
                json!(["/", num, den]),
                "{num}/{den} must not decimalize"
            );
        }
    }

    /// The other half of the same rule, and the reason the naive "emit every
    /// `Rat` as a fraction" version is wrong: user-typed decimals parse to
    /// exact rationals, so `19.9` *is* `Rat(199, 10)`. Terminating rationals
    /// keep the positional spelling the JS trees use, or `19.9` would go out as
    /// `["/", 199, 10]`.
    #[test]
    fn terminating_rationals_keep_their_decimal_spelling() {
        for (num, den, expected) in [(1, 2, 0.5), (199, 10, 19.9), (-3, 4, -0.75)] {
            assert_eq!(
                number_to_js(&Number::rat(num, den)),
                json!(expected),
                "{num}/{den} must stay positional"
            );
        }
    }

    /// Past JS's exact-integer range a fraction is no more recoverable than the
    /// f64 projection, so there is nothing to gain by emitting one — and the
    /// pair must not be silently truncated into a *wrong* fraction.
    #[test]
    fn out_of_range_rationals_fall_back_to_the_float_projection() {
        use num_bigint::BigInt;
        use num_rational::BigRational;
        let huge = BigRational::new(BigInt::from(1), BigInt::from(3u8).pow(60));
        let n = Number::from_bigrational(huge);
        assert!(
            number_to_js(&n).is_f64(),
            "an out-of-range denominator should project to a float"
        );
    }

    /// `Tree = number | string | boolean | Tree[]`, so a boolean leaf is a
    /// legal tree. It used to fall through to `Err("unexpected value …")`,
    /// making `["and", true, false]` unconstructible — the whole boolean
    /// algebra existed (`And`/`Or`/`Not`) with no values to put in it.
    #[test]
    fn boolean_leaves_round_trip_through_the_js_tree() {
        for tree in [
            json!(true),
            json!(false),
            json!(["and", true, false]),
            json!(["not", true]),
            json!(["or", ["and", true, "x"], false]),
        ] {
            let expr = try_from_js(&tree).expect("a boolean leaf is a legal tree");
            assert_eq!(to_js_rec(&expr), tree, "round trip of {tree}");
        }
    }

    /// The distinction the whole variant exists for: a boolean must come back
    /// as a JSON boolean, not as the *string* `"true"`. Mapping booleans onto
    /// symbols (or onto `MathConst`, whose members all serialize to strings)
    /// would type-check and still lose the type on every round trip.
    #[test]
    fn a_boolean_is_not_the_symbol_of_the_same_name() {
        assert_eq!(try_from_js(&json!(true)).unwrap(), Expr::Bool(true));
        assert_ne!(try_from_js(&json!(true)).unwrap(), Expr::sym("true"));
        assert_eq!(to_js_rec(&Expr::Bool(true)), json!(true));
        assert_eq!(to_js_rec(&Expr::sym("true")), json!("true"));
    }

    /// Interval closures and chained-inequality strictness are metadata on
    /// `Expr::Interval`/`Expr::Relation`, not `Expr::Bool` children. Adding the
    /// boolean leaf must not divert those flag tuples into it — the flags carry
    /// more than a bool (`("lts", false)` is `Le`, `("gts", false)` is `Ge`),
    /// and the metadata is what makes `operands.len() == ops.len() + 1`
    /// structural rather than a runtime check.
    #[test]
    fn flag_tuples_stay_metadata_and_do_not_become_boolean_children() {
        let interval = try_from_js(&json!(["interval", ["tuple", 0, 1], ["tuple", true, false]]))
            .expect("interval should parse");
        assert!(
            matches!(&interval, Expr::Interval { closed, .. } if *closed == (true, false)),
            "closure belongs in the `closed` field, got {interval:?}"
        );
        assert!(
            !interval.any_subexpr(&|e| matches!(e, Expr::Bool(_))),
            "no boolean child should appear anywhere in {interval:?}"
        );
    }
}
