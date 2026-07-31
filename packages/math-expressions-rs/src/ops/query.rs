//! Read-only inspection of an expression: the applied function names, operator
//! heads, and free variables it contains. Component access lives next door in
//! [`components`](super::components).

use crate::expr::Expr;
use std::collections::HashSet;

/// The applied function names in `e`, first-appearance order, de-duplicated —
/// the port of `me.functions` (`sin(x)+f(y)` → `["sin","f"]`). Only bare-Sym
/// application heads count (a `Pow` head like `sin^2` contributes its inner
/// name via the canonical faithful tree's head structure being Sym-rooted).
pub fn functions(e: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    fn walk(e: &Expr, out: &mut Vec<String>, seen: &mut HashSet<String>) {
        if let Expr::Apply(head, _) = e {
            // Dig a bare name out of the head (`sin`, and `sin` inside `sin^2`).
            fn head_name(h: &Expr) -> Option<String> {
                match h {
                    Expr::Sym(s) => Some(s.name()),
                    Expr::Pow(b, _) => head_name(b),
                    Expr::Prime(x) => head_name(x),
                    _ => None,
                }
            }
            if let Some(name) = head_name(head) {
                if seen.insert(name.clone()) {
                    out.push(name);
                }
            }
        }
        for c in e.children() {
            walk(c, out, seen);
        }
    }
    walk(e, &mut out, &mut seen);
    out
}

/// The operator heads used in `e`, first-appearance order, de-duplicated (JS
/// tree spelling: `+`, `-`, `*`, `/`, `^`, `apply`-less) — the port of
/// `me.operators` on the faithful tree.
pub fn operators(e: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    fn push(name: &str, out: &mut Vec<String>, seen: &mut HashSet<String>) {
        if seen.insert(name.to_string()) {
            out.push(name.to_string());
        }
    }
    fn walk(e: &Expr, out: &mut Vec<String>, seen: &mut HashSet<String>) {
        match e {
            Expr::Add(_) => push("+", out, seen),
            Expr::Neg(_) => push("-", out, seen),
            Expr::Mul(_) => push("*", out, seen),
            Expr::Div(..) => push("/", out, seen),
            Expr::Pow(..) => push("^", out, seen),
            Expr::And(_) => push("and", out, seen),
            Expr::Or(_) => push("or", out, seen),
            Expr::Not(_) => push("not", out, seen),
            Expr::Union(_) => push("union", out, seen),
            Expr::Intersect(_) => push("intersect", out, seen),
            _ => {}
        }
        for c in e.children() {
            walk(c, out, seen);
        }
    }
    walk(e, &mut out, &mut seen);
    out
}

/// The free variable names of `e`, in first-appearance order, de-duplicated.
/// Matches `me.variables`: the constant symbols `pi`/`e`/`i` ARE included (they
/// are ordinary symbols here), but a function-application head (`sin` in
/// `sin(x)`, `f` in `f(x)`) is NOT.
pub fn variables(e: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    collect(e, &mut out, &mut seen);
    out
}

fn collect(e: &Expr, out: &mut Vec<String>, seen: &mut HashSet<String>) {
    match e {
        Expr::Sym(s) => {
            let name = s.name();
            if seen.insert(name.clone()) {
                out.push(name);
            }
        }
        Expr::Num(_)
        | Expr::Const(_)
        | Expr::Bool(_)
        | Expr::RootOf { .. }
        | Expr::Blank
        | Expr::Ldots => {}

        // An application head is never a variable source — JS drops the head
        // wholesale (`tree.slice(2)` in lib/expression/variables.js), even a
        // compound one like `f'` or `sin^2` — so `f'(x)` has variables `[x]`,
        // not `[f, x]`.
        Expr::Apply(_, args) => {
            for a in args {
                collect(a, out, seen);
            }
        }

        Expr::Add(xs)
        | Expr::Mul(xs)
        | Expr::And(xs)
        | Expr::Or(xs)
        | Expr::Union(xs)
        | Expr::Intersect(xs)
        | Expr::Seq(_, xs)
        | Expr::OtherOp(_, xs) => {
            for c in xs {
                collect(c, out, seen);
            }
        }
        Expr::Div(a, b) | Expr::Pow(a, b) | Expr::Index(a, b) => {
            collect(a, out, seen);
            collect(b, out, seen);
        }
        Expr::Neg(x) | Expr::Not(x) | Expr::Prime(x) => collect(x, out, seen),
        Expr::Interval { endpoints, .. } => {
            collect(&endpoints.0, out, seen);
            collect(&endpoints.1, out, seen);
        }
        Expr::Relation { operands, .. } => {
            for c in operands {
                collect(c, out, seen);
            }
        }
        Expr::Matrix { entries, .. } => {
            for c in entries {
                collect(c, out, seen);
            }
        }
    }
}
