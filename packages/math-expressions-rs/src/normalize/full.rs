//! `full_simplify` — the aggressive simplifier (FULL_SIMPLIFY_PLAN), and the
//! single engine behind the public [`simplify`](crate::simplify) /
//! [`simplify_with`](crate::simplify_with).
//!
//! It runs the base canonical simplify (which used to *be* `simplify`, held
//! byte-compatible with the JS differential corpus and therefore reproducing
//! the JS library's *absence* of rules like `exp(ln x) → x`) and then layers on
//! the stronger — but still **sound** — rewrites the port has since built: the
//! S3 trig/exp/log special values (`exp(ln u) → u`, `sin(π/6) → 1/2`, …) and S2
//! rational cancellation, run to a fixpoint. The base form is no longer
//! reachable from outside the crate; JS-corpus agreement is now advisory only
//! (`tests/simplify_corpus.rs`).
//!
//! This is the staged form of the plan's S7 cost-directed driver: it composes
//! the already-landed S1–S4 passes to a fixpoint instead of doing beam search
//! over a complexity measure. Every pass is individually sound and
//! canonical-in/canonical-out, so the result is always *equal* to the input;
//! the remaining work (S5–S7) is answer *quality* (choosing expand-vs-factor),
//! not correctness. `equals(full_simplify(e), e)` holds by construction.

use crate::assumptions::Assumptions;
use crate::expr::Expr;

/// Aggressively simplify `e` with every sound reduction the port has:
/// the base canonical simplify (including the assumption-aware rules when `a`
/// is non-empty), special-value folding (`exp(ln x) → x`, trig at the π/12
/// lattice, `ln 1`, `e^0`, …) and rational cancellation, iterated to a
/// fixpoint.
///
/// `full_simplify(e, &Assumptions::new())` is [`simplify`](crate::simplify) and
/// `full_simplify(e, a)` is [`simplify_with`](crate::simplify_with); this is
/// the implementation both delegate to, exported under its plan name.
pub fn full_simplify(e: &Expr, a: &Assumptions) -> Expr {
    // Bound the fixpoint by the same §7f budget as the base simplify's own
    // rounds; in practice this converges in 2–3 iterations.
    let max_rounds = crate::resource_limits::current()
        .max_simplify_rounds
        .max(1);
    let mut cur = crate::normalize::simplify_base_with(e, a);
    for _ in 0..max_rounds {
        // Each pass is sound and canonical-in/out; re-run the *base* simplify
        // after them so the next round sees a fully normalized tree. (Must be
        // the base, not the public `simplify`/`simplify_with`, which are this
        // function.)
        let folded = crate::normalize::fold_special_values(&cur);
        let reduced = crate::ops::reduce_rational(&folded);
        let next = crate::normalize::simplify_base_with(&reduced, a);
        if next == cur {
            break;
        }
        cur = next;
    }
    cur
}
