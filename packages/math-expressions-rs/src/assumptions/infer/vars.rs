//! Facts about a bare variable, read off its stored assumptions.
//!
//! Stored facts are canonical relations, optionally wrapped in `not(...)`.
//! JS peels those wrappers itself (a `while (assume_operator === "not")` loop
//! that toggles a `negate_assumptions` flag) and then reads the relation with
//! the sense flipped; [`relation_parts`] does the same, but pushes the flip
//! into the operator so the reader below only ever sees the four shapes
//! canonicalization produces.

use super::super::facts::Facts;
use super::super::Assumptions;
use crate::expr::{Expr, RelOp};

/// Facts about a bare variable, derived from its stored assumptions.
pub(super) fn variable_facts(name: &str, a: &Assumptions) -> Facts {
    let mut out = Facts::unknown();
    for fact in &a.facts_for(name) {
        let Some((lhs, rhs, op)) = relation_parts(fact) else {
            continue;
        };
        // Which side is the variable, which the bound? (Canonicalization
        // rewrites `>`/`≥` to `<`/`≤` with swapped operands, and sorts `=`.)
        let (var_on_left, other) = match (lhs, rhs) {
            (Expr::Sym(s), o) if s.name() == name => (true, o),
            (o, Expr::Sym(s)) if s.name() == name => (false, o),
            _ => continue, // compound fact (e.g. x + y < 2): no direct bound
        };

        match op {
            RelOp::Eq => {
                // Known value: adopt the literal's own facts wholesale.
                if let Expr::Num(n) = other {
                    return Facts::of_number(n);
                }
            }
            RelOp::Ne => {
                if matches!(other, Expr::Num(n) if n.is_zero()) {
                    out.nonzero = Some(true);
                }
            }
            RelOp::Lt | RelOp::Le => {
                let strict = matches!(op, RelOp::Lt);
                let Expr::Num(n) = other else { continue };
                let c = n.to_f64();
                // A one-sided real bound implies realness.
                out.real = Some(true);
                out.complex = Some(true);
                if var_on_left {
                    // name < c (or ≤): upper bound.
                    if c < 0.0 || (c == 0.0 && strict) {
                        out.negative = Some(true);
                        out.nonpos = Some(true);
                        out.nonzero = Some(true);
                        out.positive = Some(false);
                        out.nonneg = Some(false);
                    } else if c == 0.0 {
                        out.nonpos = Some(true);
                        out.positive = Some(false);
                    }
                } else {
                    // c < name (or ≤): lower bound.
                    if c > 0.0 || (c == 0.0 && strict) {
                        out.positive = Some(true);
                        out.nonneg = Some(true);
                        out.nonzero = Some(true);
                        out.negative = Some(false);
                        out.nonpos = Some(false);
                    } else if c == 0.0 {
                        out.nonneg = Some(true);
                        out.negative = Some(false);
                    }
                }
            }
            // `name ∈ Z / Q / R / C` and its negation (the JS set names).
            RelOp::In | RelOp::NotIn if var_on_left => {
                let Expr::Sym(set) = other else { continue };
                let member = matches!(op, RelOp::In);
                match (set.name().as_str(), member) {
                    ("Z", true) => {
                        out.integer = Some(true);
                        out.real = Some(true);
                        out.complex = Some(true);
                    }
                    ("Q" | "R", true) => {
                        out.real = Some(true);
                        out.complex = Some(true);
                    }
                    ("C", true) => out.complex = Some(true),
                    ("Z", false) => out.integer = Some(false),
                    // Not real (not complex) says nothing about the wider set:
                    // JS leaves `is_complex(x)` unknown for `x notin R`.
                    ("R", false) => out.real = Some(false),
                    ("C", false) => out.complex = Some(false),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

/// The two-sided relation a stored fact constrains, with any `not(...)`
/// wrappers folded in: an odd number of them flips the operator, and the
/// flipped `≥`/`>` results are re-stated as `≤`/`<` with the operands swapped
/// so they match the canonical orientation. `None` for anything that is not a
/// simple two-operand relation (a chained inequality, say).
fn relation_parts(fact: &Expr) -> Option<(&Expr, &Expr, RelOp)> {
    let mut rel = fact;
    let mut negated = false;
    while let Expr::Not(inner) = rel {
        negated = !negated;
        rel = inner;
    }
    let Expr::Relation { operands, ops } = rel else {
        return None;
    };
    let ([lhs, rhs], [op]) = (operands.as_slice(), ops.as_slice()) else {
        return None;
    };
    if !negated {
        return Some((lhs, rhs, *op));
    }
    Some(match op {
        RelOp::Eq => (lhs, rhs, RelOp::Ne),
        RelOp::Ne => (lhs, rhs, RelOp::Eq),
        RelOp::Lt => (rhs, lhs, RelOp::Le), // ¬(a < b) ⇔ b ≤ a
        RelOp::Le => (rhs, lhs, RelOp::Lt), // ¬(a ≤ b) ⇔ b < a
        // Canonicalization does not emit these, but a hand-built tree can.
        RelOp::Gt => (lhs, rhs, RelOp::Le),
        RelOp::Ge => (lhs, rhs, RelOp::Lt),
        RelOp::In => (lhs, rhs, RelOp::NotIn),
        RelOp::NotIn => (lhs, rhs, RelOp::In),
        _ => return None,
    })
}
