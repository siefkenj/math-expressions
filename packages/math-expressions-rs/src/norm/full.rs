//! `full_simplify` — the aggressive, non-oracle simplifier (FULL_SIMPLIFY_PLAN).
//!
//! Distinct from [`simplify`](crate::simplify), which is held byte-compatible
//! with the JS differential corpus and therefore reproduces the JS library's
//! *absence* of rules like `exp(ln x) → x`. `full_simplify` is free to apply
//! the stronger — but still **sound** — rewrites the port has since built:
//! the S3 trig/exp/log special values (`exp(ln u) → u`, `sin(π/6) → 1/2`, …)
//! and S2 rational cancellation, run to a fixpoint.
//!
//! This is the staged form of the plan's S7 cost-directed driver: it composes
//! the already-landed S1–S4 passes to a fixpoint instead of doing beam search
//! over a complexity measure. Every pass is individually sound and
//! canonical-in/canonical-out, so the result is always *equal* to the input;
//! the remaining work (S5–S7) is answer *quality* (assumption-gated rules,
//! choosing expand-vs-factor), not correctness. `equals(full_simplify(e), e)`
//! holds by construction.

use crate::assumptions::Assumptions;
use crate::expr::Expr;

/// Aggressively simplify `e` with every sound reduction the port has (beyond
/// the oracle-compatible [`simplify`](crate::simplify)): special-value folding
/// (`exp(ln x) → x`, trig at the π/12 lattice, `ln 1`, `e^0`, …) and rational
/// cancellation, iterated to a fixpoint.
///
/// `a` is accepted for the forthcoming S5 assumption-gated rules
/// (`√(x²) → |x|` under `x ≥ 0`, …) and is not yet consulted.
pub fn full_simplify(e: &Expr, _a: &Assumptions) -> Expr {
    // Bound the fixpoint by the same §7f budget as `simplify`'s own rounds; in
    // practice this converges in 2–3 iterations.
    let max_rounds = crate::resource_limits::current()
        .max_simplify_rounds
        .max(1);
    let mut cur = crate::norm::simplify(e);
    for _ in 0..max_rounds {
        // Each pass is sound and canonical-in/out; re-`simplify` after them so
        // the next round sees a fully normalized tree.
        let folded = crate::norm::fold_special_values(&cur);
        let reduced = crate::ops::reduce_rational(&folded);
        let next = crate::norm::simplify(&reduced);
        if next == cur {
            break;
        }
        cur = next;
    }
    cur
}
