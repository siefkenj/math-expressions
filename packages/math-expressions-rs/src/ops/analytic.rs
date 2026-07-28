//! `me.isAnalytic` (`lib/expression/analytic.js`): is every operator one of the
//! analytic structural operators and every function an analytic one?

use crate::expr::Expr;
use crate::ops::{functions, normalize_function_names, subscripts_to_strings};

/// Options for [`is_analytic`], mirroring the JS destructured defaults.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnalyticOpts {
    pub allow_abs: bool,
    pub allow_arg: bool,
    pub allow_relation: bool,
}

/// Port of `me.isAnalytic`: is every operator one of the analytic structural
/// operators (`+ - * / ^`, sequences, intervals) and every function an
/// analytic one (i.e. not `abs`/`sign`/`arg`, unless explicitly allowed)?
/// Logical/set operators and relations are non-analytic (relations pass only
/// under `allow_relation`, and only the order relations `= le ge < >`).
pub fn is_analytic(e: &Expr, opts: &AnalyticOpts) -> bool {
    // Every structural operator must be analytic (JS `analytic_operators`:
    // `+ - * / ^` plus the tuple/vector/altvector/list/array/interval/matrix/vec
    // constructors). We inspect each node's head directly rather than via
    // `operators()`, which only reports `+ - * / ^ and or not union intersect`
    // and silently drops the non-analytic tail operators (binom, pm, set, prime,
    // `_`, …) that JS's `operators_list` emits and its whitelist rejects.
    fn analytic_operators_only(e: &Expr) -> bool {
        use crate::expr::SeqKind;
        let head_ok = match e {
            // Leaves + function application carry no structural operator to
            // whitelist (JS filters `apply`; function names are checked below).
            Expr::Num(_)
            | Expr::Sym(_)
            | Expr::Const(_)
            | Expr::RootOf { .. }
            | Expr::Blank
            | Expr::Ldots
            | Expr::Apply(..) => true,
            // Analytic structural operators.
            Expr::Add(_)
            | Expr::Neg(_)
            | Expr::Mul(_)
            | Expr::Div(..)
            | Expr::Pow(..)
            | Expr::Interval { .. }
            | Expr::Matrix { .. } => true,
            // tuple/vector/altvector/list/array are analytic; `set` is not.
            Expr::Seq(kind, _) => !matches!(kind, SeqKind::Set),
            // Of the tail operators only `vec` is in the whitelist.
            Expr::OtherOp(name, _) => name.name() == "vec",
            // Relation operators are validated separately by `walk_rel` below.
            Expr::Relation { .. } => true,
            // Non-analytic: and / or / not / union / intersect / prime / `_`.
            _ => false,
        };
        head_ok && e.children().into_iter().all(analytic_operators_only)
    }
    if !analytic_operators_only(&subscripts_to_strings(e)) {
        return false;
    }
    // Non-analytic functions: abs, sign, arg (each gate-able).
    let normalized = normalize_function_names(e);
    for f in functions(&normalized) {
        let blocked = match f.as_str() {
            "abs" => !opts.allow_abs,
            "arg" => !opts.allow_arg,
            "sign" => true,
            _ => false,
        };
        if blocked {
            return false;
        }
    }
    // Relations: only the order relations, only when allowed.
    let mut ok = true;
    fn walk_rel(e: &Expr, allow: bool, ok: &mut bool) {
        if let Expr::Relation { ops, .. } = e {
            if !allow {
                *ok = false;
            } else {
                for op in ops {
                    use crate::expr::RelOp;
                    if !matches!(
                        op,
                        RelOp::Eq | RelOp::Le | RelOp::Ge | RelOp::Lt | RelOp::Gt
                    ) {
                        *ok = false;
                    }
                }
            }
        }
        for c in e.children() {
            walk_rel(c, allow, ok);
        }
    }
    walk_rel(e, opts.allow_relation, &mut ok);
    ok
}
