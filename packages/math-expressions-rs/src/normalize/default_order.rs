//! `default_order`: **sort, do not evaluate** — a faithful port of the JS
//! `trees/default_order.js`.
//!
//! This is not [`canonicalize`](super::canonicalize), and the difference is the
//! whole point. Canonical form folds as it goes: `0·x²` disappears, `7+4`
//! becomes `11`, `1·x²` loses its coefficient. `default_order` touches nothing
//! but the *arrangement* — operands of the commutative operators are sorted,
//! comparison directions are flipped to point one way, and negatives are pulled
//! out of factors. Every term survives, spelled as it was written.
//!
//! That is what DoenetML's `simplify="normalizeOrder"` means, and what makes it
//! usable for grading: an author asking for it wants `1x²+2-0x²+3` to match the
//! same terms in any order and *not* to match `x²+5`, which is a different
//! answer written by a student who did more work than they were asked to.
//!
//! # The ordering is the JS one, deliberately
//!
//! [`super::order::cmp`] is this crate's canonical order and is a better
//! comparator — typed, allocation-free, stable across sessions. It is also a
//! *different* order, and the order here is observable: it decides the term
//! sequence a `normalizeOrder` expression prints in. Reproducing the JS key is
//! what keeps existing documents rendering as their authors saw them.
//!
//! The JS key is an array of mixed types compared with `<`, which in JavaScript
//! compares the arrays' *string* forms — so the key is really a comma-joined
//! string, and `[0,'number',10] < [0,'number',2]` because `"10" < "2"`. That is
//! reproduced literally: [`sort_key`] builds that string and comparison is
//! `str`'s. Do not "fix" the number ordering; it is the contract.

use crate::expr::{Expr, MathConst, RelOp, SeqKind};
use crate::num::Number;

/// Sort an expression into the JS library's default order.
pub fn default_order(e: &Expr) -> Expr {
    let t = normalize_negatives(&flatten(e));
    normalize_negatives(&sort_ast(&t))
}

/// Merge nested same-operator `Add`/`Mul`/`And`/`Or`/`Union`/`Intersect` nodes
/// into their parent, so the sort sees one flat operand list. The parsers
/// already produce flat trees; a tree built by hand through the AST boundary
/// need not be.
fn flatten(e: &Expr) -> Expr {
    fn flat_children(xs: &[Expr], same: impl Fn(&Expr) -> Option<Vec<Expr>>) -> Vec<Expr> {
        let mut out = Vec::with_capacity(xs.len());
        for x in xs {
            let fx = flatten(x);
            match same(&fx) {
                Some(inner) => out.extend(inner),
                None => out.push(fx),
            }
        }
        out
    }
    match e {
        Expr::Add(xs) => Expr::Add(flat_children(xs, |x| match x {
            Expr::Add(inner) => Some(inner.clone()),
            _ => None,
        })),
        Expr::Mul(xs) => Expr::Mul(flat_children(xs, |x| match x {
            Expr::Mul(inner) => Some(inner.clone()),
            _ => None,
        })),
        Expr::And(xs) => Expr::And(flat_children(xs, |x| match x {
            Expr::And(inner) => Some(inner.clone()),
            _ => None,
        })),
        Expr::Or(xs) => Expr::Or(flat_children(xs, |x| match x {
            Expr::Or(inner) => Some(inner.clone()),
            _ => None,
        })),
        Expr::Union(xs) => Expr::Union(flat_children(xs, |x| match x {
            Expr::Union(inner) => Some(inner.clone()),
            _ => None,
        })),
        Expr::Intersect(xs) => Expr::Intersect(flat_children(xs, |x| match x {
            Expr::Intersect(inner) => Some(inner.clone()),
            _ => None,
        })),
        other => crate::expr::map_children(other, flatten),
    }
}

/// Drop double negatives and pull a negative out of any factor, so a product's
/// sign is carried in one place. Run before *and* after sorting, as JS does:
/// the sort's own `-`-into-product rewrite can create a new inner negative.
fn normalize_negatives(e: &Expr) -> Expr {
    let e = remove_duplicate_negatives(e);
    let e = negatives_out_of_factors(&e);
    remove_duplicate_negatives(&e)
}

fn remove_duplicate_negatives(e: &Expr) -> Expr {
    if let Expr::Neg(inner) = e {
        if let Expr::Neg(inner2) = inner.as_ref() {
            return remove_duplicate_negatives(inner2);
        }
        // A negated *literal* folds into the literal. Not evaluation — it is
        // the same number, and the two spellings are only ever an accident of
        // where the value came from: `x-1` parses as `["+", x, -1]`, while the
        // same expression with the 1 substituted in from elsewhere arrives as
        // `["+", x, ["-", 1]]`. Left alone they carry different sort keys and
        // land in different positions, so the two spellings of one expression
        // stop matching — which is the whole job of this pass.
        //
        // Zero is the exception: negating it yields `NegZero`, a *different*
        // leaf that sorts elsewhere, so folding it would make the pass
        // non-idempotent — `-0x²` sorted to one place on the first run and
        // another on the second. A zero's sign carries no information here
        // anyway, so the `-0` spelling is left exactly as written.
        if let Expr::Num(n) = inner.as_ref() {
            if !n.is_zero() {
                return Expr::Num(n.neg());
            }
        }
    }
    crate::expr::map_children(e, remove_duplicate_negatives)
}

fn negatives_out_of_factors(e: &Expr) -> Expr {
    let e = crate::expr::map_children(e, negatives_out_of_factors);
    let (factors, rebuild): (Vec<Expr>, fn(Vec<Expr>) -> Expr) = match &e {
        Expr::Mul(xs) => (xs.clone(), |v| Expr::Mul(v)),
        Expr::Div(a, b) => (vec![a.as_ref().clone(), b.as_ref().clone()], |mut v| {
            Expr::Div(Box::new(v.remove(0)), Box::new(v.remove(0)))
        }),
        _ => return e,
    };
    let mut negative = false;
    let stripped: Vec<Expr> = factors
        .into_iter()
        .map(|f| match f {
            Expr::Neg(inner) => {
                negative = !negative;
                *inner
            }
            other => other,
        })
        .collect();
    let result = rebuild(stripped);
    if negative {
        Expr::Neg(Box::new(result))
    } else {
        result
    }
}

fn sort_ast(e: &Expr) -> Expr {
    let e = crate::expr::map_children(e, sort_ast);
    match e {
        // The commutative operators: sort operands by the JS key.
        Expr::Add(mut xs) => {
            sort_by_key(&mut xs);
            Expr::Add(xs)
        }
        Expr::Mul(mut xs) => {
            sort_by_key(&mut xs);
            Expr::Mul(xs)
        }
        Expr::And(mut xs) => {
            sort_by_key(&mut xs);
            Expr::And(xs)
        }
        Expr::Or(mut xs) => {
            sort_by_key(&mut xs);
            Expr::Or(xs)
        }
        Expr::Union(mut xs) => {
            sort_by_key(&mut xs);
            Expr::Union(xs)
        }
        Expr::Intersect(mut xs) => {
            sort_by_key(&mut xs);
            Expr::Intersect(xs)
        }
        // `=` and `ne` are commutative too; the ordered comparisons instead get
        // turned around so every one of them points the same way, and the
        // containment relations so the larger set is always on the right.
        Expr::Relation { mut operands, ops } => {
            if ops.iter().all(|o| matches!(o, RelOp::Eq))
                || ops.iter().all(|o| matches!(o, RelOp::Ne))
            {
                sort_by_key(&mut operands);
                return Expr::Relation { operands, ops };
            }
            if ops.iter().all(|o| {
                matches!(
                    o,
                    RelOp::Gt
                        | RelOp::Ge
                        | RelOp::Ni
                        | RelOp::NotNi
                        | RelOp::Superset
                        | RelOp::NotSuperset
                )
            }) {
                operands.reverse();
                let flipped: Vec<RelOp> = ops.iter().rev().map(|o| mirror(*o)).collect();
                return Expr::Relation {
                    operands,
                    ops: flipped,
                };
            }
            Expr::Relation { operands, ops }
        }
        // Negating a product with a leading numerical factor puts the sign on
        // that factor, so `-(2x)` and `(-2)x` reach the same tree.
        Expr::Neg(inner) => match *inner {
            Expr::Mul(mut xs) if !xs.is_empty() => {
                let first = xs.remove(0);
                xs.insert(0, Expr::Neg(Box::new(first)));
                Expr::Mul(xs)
            }
            other => Expr::Neg(Box::new(other)),
        },
        other => other,
    }
}

/// The same relation read right-to-left: `a > b` is `b < a`. Not
/// [`RelOp::negate`], which keeps the operand order and complements the
/// meaning; this keeps the meaning and reverses the operands.
fn mirror(op: RelOp) -> RelOp {
    match op {
        RelOp::Gt => RelOp::Lt,
        RelOp::Ge => RelOp::Le,
        RelOp::Lt => RelOp::Gt,
        RelOp::Le => RelOp::Ge,
        RelOp::Ni => RelOp::In,
        RelOp::NotNi => RelOp::NotIn,
        RelOp::In => RelOp::Ni,
        RelOp::NotIn => RelOp::NotNi,
        RelOp::Superset => RelOp::Subset,
        RelOp::NotSuperset => RelOp::NotSubset,
        RelOp::SupersetEq => RelOp::SubsetEq,
        RelOp::NotSupersetEq => RelOp::NotSubsetEq,
        RelOp::Subset => RelOp::Superset,
        RelOp::NotSubset => RelOp::NotSuperset,
        RelOp::SubsetEq => RelOp::SupersetEq,
        RelOp::NotSubsetEq => RelOp::NotSupersetEq,
        RelOp::Eq => RelOp::Eq,
        RelOp::Ne => RelOp::Ne,
    }
}

fn sort_by_key(xs: &mut [Expr]) {
    // A decorate-sort-undecorate: building the key is the expensive part, and
    // a comparison sort would rebuild it O(n log n) times per operand.
    let mut keyed: Vec<(String, Expr)> = xs.iter().map(|x| (sort_key(x), x.clone())).collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    for (slot, (_, e)) in xs.iter_mut().zip(keyed) {
        *slot = e;
    }
}

/// The JS sort key, as the string JavaScript's array comparison would have
/// produced. See the module note: the string form *is* the key, quirks and all.
fn sort_key(e: &Expr) -> String {
    let mut s = String::new();
    write_key(e, &mut s);
    s
}

fn write_key(e: &Expr, out: &mut String) {
    match e {
        Expr::Num(n) => {
            out.push_str("0,number,");
            out.push_str(&js_number(n));
        }
        // Constants are plain strings in the JS tree (`"pi"`), so they key as
        // symbols there and must here.
        Expr::Const(c) => {
            out.push_str("1,symbol,");
            out.push_str(const_name(*c));
        }
        Expr::Sym(s) => {
            out.push_str("1,symbol,");
            out.push_str(&s.name());
        }
        Expr::Blank => out.push_str("1,symbol,\u{ff3f}"),
        Expr::Bool(b) => {
            out.push_str("1,boolean,");
            out.push_str(if *b { "true" } else { "false" });
        }
        Expr::Apply(f, args) => {
            out.push_str("2,function,");
            write_key(f, out);
            out.push(',');
            out.push_str(&args.len().to_string());
            for a in args {
                out.push(',');
                write_key(a, out);
            }
        }
        Expr::Mul(xs) => write_op_key("4,product", xs, out),
        Expr::Div(a, b) => {
            write_op_key("4,quotient", &[a.as_ref().clone(), b.as_ref().clone()], out)
        }
        Expr::Add(xs) => write_op_key("5,sum", xs, out),
        Expr::Neg(x) => write_op_key("6,minus", std::slice::from_ref(x.as_ref()), out),
        other => {
            let mut head = String::from("7,");
            head.push_str(&legacy_operator(other));
            let children = key_children(other);
            write_op_key(&head, &children, out);
        }
    }
}

fn write_op_key(head: &str, xs: &[Expr], out: &mut String) {
    out.push_str(head);
    out.push(',');
    out.push_str(&xs.len().to_string());
    for x in xs {
        out.push(',');
        write_key(x, out);
    }
}

/// The operand list the JS tree would have had, for the nodes that key
/// generically. Metadata that JS carried as operands (interval closure,
/// relation ops) is spelled back out so the key sees what JS saw.
fn key_children(e: &Expr) -> Vec<Expr> {
    match e {
        Expr::Pow(b, x) => vec![b.as_ref().clone(), x.as_ref().clone()],
        Expr::Prime(f) => vec![f.as_ref().clone()],
        Expr::Index(x, i) => vec![x.as_ref().clone(), i.as_ref().clone()],
        Expr::Not(x) => vec![x.as_ref().clone()],
        Expr::Seq(_, xs) => xs.clone(),
        Expr::Interval { endpoints, closed } => vec![
            Expr::Seq(
                SeqKind::Tuple,
                vec![endpoints.0.clone(), endpoints.1.clone()],
            ),
            Expr::Seq(
                SeqKind::Tuple,
                vec![Expr::Bool(closed.0), Expr::Bool(closed.1)],
            ),
        ],
        Expr::Relation { operands, .. } => operands.clone(),
        Expr::Matrix(m) => m.entries().to_vec(),
        Expr::OtherOp(_, xs) => xs.clone(),
        _ => Vec::new(),
    }
}

fn legacy_operator(e: &Expr) -> String {
    match e {
        Expr::Pow(..) => "^".to_string(),
        Expr::Prime(_) => "prime".to_string(),
        Expr::Index(..) => "_".to_string(),
        Expr::Not(_) => "not".to_string(),
        Expr::And(_) => "and".to_string(),
        Expr::Or(_) => "or".to_string(),
        Expr::Union(_) => "union".to_string(),
        Expr::Intersect(_) => "intersect".to_string(),
        Expr::Seq(k, _) => k.js_name().to_string(),
        Expr::Interval { .. } => "interval".to_string(),
        // A chained relation keys under its first operator, as the JS tree's
        // head would have been for the two-operand case.
        Expr::Relation { ops, .. } => ops
            .first()
            .map(|o| o.js_name().to_string())
            .unwrap_or_else(|| "=".to_string()),
        Expr::Matrix(_) => "matrix".to_string(),
        Expr::OtherOp(s, _) => s.name(),
        Expr::RootOf { .. } => "rootof".to_string(),
        Expr::Ldots => "ldots".to_string(),
        _ => "unknown".to_string(),
    }
}

fn const_name(c: MathConst) -> &'static str {
    match c {
        MathConst::Pi => "pi",
        MathConst::E => "e",
        MathConst::I => "i",
        MathConst::Inf => "infinity",
        MathConst::NegInf => "-infinity",
        MathConst::NaN => "NaN",
        MathConst::None => "None",
    }
}

/// A number as JavaScript would have stringified it inside the key. The JS
/// trees held f64s, so an exact rational keys by its f64 projection — the same
/// value the JS tree would have carried.
fn js_number(n: &Number) -> String {
    n.js_string()
}
