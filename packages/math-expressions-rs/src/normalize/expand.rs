//! Expansion (`me.expand()`).
//!
//! Distributes multiplication, division-numerator, and negation over sums, and
//! multinomial-expands non-negative integer powers of sums — recursively, into
//! function arguments and everywhere else. Denominators and non-integer /
//! negative powers are left intact (`(x+1)/(x+2)` → `x/(x+2)+1/(x+2)`, and the
//! denominator is NOT itself expanded — a factored `1/((x+1)(x+2))` keeps its
//! factored denominator, matching mathjs). Matches mathjs `expand`, which
//! `me.expand()` delegates to.
//!
//! Built on the canonical smart constructors, so the result is a canonical,
//! like-terms-combined expanded form (`(x+1)(x+2)` → `x²+3x+2`).
//!
//! **Bounded on adversarial input**: like terms are combined after every
//! distributed factor (so `(a+b)^64` stays at ≤65 live terms instead of 2⁶⁴
//! clones), and a hard term-count cap makes genuinely huge expansions (many
//! distinct monomials, e.g. a product of 40 distinct binomials) bail out and
//! return the unexpanded product instead of exhausting memory.

use crate::expr::Expr;
use crate::num::Number;

use super::{add, contract_pair, mul, pow};
use crate::expr::map_children;

// Caps (resource_limits::current().max_expand_power / max_expand_terms): the exponent
// bound on multinomial expansion, and the raw term-count bound per
// distribution step beyond which the node is left unexpanded. Classroom
// polynomials are far below both; they exist so a pasted product of dozens of
// sums cannot exhaust memory (the bug that once froze this dev container).

/// Fully expand `e`, in display form (`normalize::present`). Internal callers
/// that pattern-match on canonical shapes use [`expand_core`].
pub fn expand(e: &Expr) -> Expr {
    super::present(&expand_core(e))
}

/// Contract the first matrix/vector product step inside a `Mul`, if there is one.
///
/// Multiplying matrices and vectors together is exactly the "multiply it out"
/// that `expand` is for, and it is the only place it happens for vectors:
/// canonicalization leaves such a product written as it stands, because a
/// `<math>` that asked for nothing should render what the author typed. (The
/// `mul` constructor already folds matrix·matrix; this covers everything a vector
/// touches.)
///
/// The *ordered* factors — the matrix- and vector-valued ones — are contracted
/// left to right through the shared [`contract_pair`] core, which treats a vector
/// as a row (left operand) or column (right operand): `M·v`→column, `v·M`→row,
/// `v·w`→row·column dot (a scalar). Scalar factors commute and ride along; they
/// are not part of the ordered segment, so `M·g·(e,f)` contracts exactly like
/// `g·M·(e,f)`.
///
/// Only the first two ordered factors are contracted here; the single result
/// goes back through [`expand_core`], which re-enters this function. That
/// re-entry is what folds a stranded scalar into the resulting components
/// (`distribute_over_vector` declines while a matrix is present) and lets a
/// longer chain contract one step at a time. Each pass replaces two ordered
/// factors with one, so it strictly shrinks and terminates.
fn contract_product(factors: &[Expr]) -> Option<Expr> {
    let ordered: Vec<usize> = factors
        .iter()
        .enumerate()
        .filter(|(_, f)| super::is_matrix_valued(f) || super::is_vector_valued(f))
        .map(|(i, _)| i)
        .collect();
    let [i, j, ..] = ordered[..] else {
        return None;
    };
    let contracted = contract_pair(&factors[i], &factors[j])?;
    // Rebuild: the contracted result takes the left factor's slot, the right
    // factor is dropped, and every other factor (scalars, further-right ordered
    // factors) stays where it was.
    let rest: Vec<Expr> = factors
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != j)
        .map(|(k, f)| if k == i { contracted.clone() } else { f.clone() })
        .collect();
    Some(expand_core(&mul(rest)))
}

/// Add coordinate vectors of the same kind and length componentwise.
///
/// The counterpart of [`distribute_over_vector`] on the additive side, and
/// needed for the same reason: `simplify` combines `(a,b) + (c,d)` and `expand`
/// did not, so a scalar multiple distributed into two vectors stopped one step
/// short of `(am + cn, bm + dn)`. Canonicalization deliberately leaves the sum
/// alone — a `<math>` that asks for nothing renders what the author wrote — so
/// this lives here rather than in the `add` constructor.
///
/// Kinds combine by *class*, the rule `simplify` already uses: `(a,b)`, `⟨a,b⟩`
/// and the vector spelling are one object in three notations, while `[a,b]` is
/// its own class because `createIntervals` reads it as an interval. The first
/// term's spelling is the one the result keeps.
fn combine_vector_terms(terms: &[Expr]) -> Option<Expr> {
    let mut kind: Option<crate::expr::SeqKind> = None;
    let mut class = None;
    let mut len = 0;
    for t in terms {
        let Expr::Seq(k, comps) = t else { return None };
        let c = super::vector_class(*k)?;
        match class {
            None => {
                kind = Some(*k);
                class = Some(c);
                len = comps.len();
            }
            // Same class and arity: `⟨a,b⟩ + (c,d)` is one vector written two
            // ways, and the first term's spelling is the one that survives.
            Some(c0) if c0 == c && len == comps.len() => {}
            _ => return None,
        }
    }
    if terms.len() < 2 {
        return None;
    }
    let kind = kind?;
    let combined = (0..len)
        .map(|i| {
            add(terms
                .iter()
                .map(|t| match t {
                    Expr::Seq(_, comps) => comps[i].clone(),
                    _ => unreachable!("checked above"),
                })
                .collect())
        })
        .collect();
    Some(Expr::Seq(kind, combined))
}

/// Multiply a scalar factor into the components of a coordinate vector.
///
/// `simplify` already does this (`rule_seq_arith`), and `expand` — whose whole
/// job is multiplying out — did not, so `<math expand>m(a,b)</math>` rendered
/// `m(a,b)` where `<math simplify>` gave `(am, bm)`. One vector only: two of
/// them is a dot or cross product, which is not this rule's business.
fn distribute_over_vector(factors: &[Expr]) -> Option<Expr> {
    let mut seq_idx = None;
    for (i, f) in factors.iter().enumerate() {
        if super::is_vector_valued(f) {
            if seq_idx.is_some() {
                return None;
            }
            seq_idx = Some(i);
        }
    }
    let i = seq_idx?;
    let Expr::Seq(kind, comps) = &factors[i] else {
        return None;
    };
    // A matrix in the product means the vector is being multiplied, not scaled;
    // `contract_product` has already had its turn and declined (shapes do
    // not conform), so leaving the product alone is the honest answer.
    if factors.iter().any(super::is_matrix_valued) {
        return None;
    }
    let others: Vec<Expr> = factors
        .iter()
        .enumerate()
        .filter(|(j, _)| *j != i)
        .map(|(_, f)| f.clone())
        .collect();
    let scaled = comps
        .iter()
        .map(|c| {
            let mut fs = others.clone();
            fs.push(c.clone());
            expand_core(&mul(fs))
        })
        .collect();
    Some(Expr::Seq(*kind, scaled))
}

/// Re-expand the entries of a literal matrix or coordinate vector.
///
/// The `mul`/`pow` smart constructors contract `M·N`, raise `M^k`, and fold a
/// scalar factor into the entries — all *after* `expand_core` has already run on
/// the operands. The entries they build (`g·(ae+bf)` from `M·N·g`, or the nested
/// `a·(a²+bc)+b·(ac+cd)` of `M³`) are therefore themselves unexpanded products
/// over sums that were never handed back to `expand_core`. Do that here.
///
/// The `M·vector` path already re-enters `expand_core` (see
/// [`contract_product`]), so this closes the same gap for the two cases
/// that don't: a matrix·matrix product and a matrix power, both of which the
/// constructors collapse straight to an `Expr::Matrix`. Non-container results
/// pass through untouched, so this is a no-op on the scalar path.
fn expand_container_entries(e: Expr) -> Expr {
    match &e {
        Expr::Matrix(m) => Expr::Matrix(m.map(expand_core)),
        Expr::Seq(k, comps) if super::is_vector_valued(&e) => {
            Expr::Seq(*k, comps.iter().map(expand_core).collect())
        }
        _ => e,
    }
}

/// [`expand`] without the final presentation pass: the result is canonical.
pub(crate) fn expand_core(e: &Expr) -> Expr {
    match e {
        // Sum: expand each term (the smart `add` flattens and combines).
        Expr::Add(ts) => {
            let terms: Vec<Expr> = ts.iter().map(expand_core).collect();
            combine_vector_terms(&terms).unwrap_or_else(|| add(terms))
        }

        // Negation is multiplication by −1, so it distributes over a sum.
        // (Two factors, one of them a constant: cannot hit the cap on its own.)
        //
        // It distributes over a *vector* for the same reason, and by the same
        // rule the `Mul` arm below uses — `−1` is a scalar like any other. This
        // is what lets `combine_vector_terms` see a subtraction: it matches on
        // `Expr::Seq`, so a term left as `Neg(Seq)` made the whole sum decline
        // and `(a,b) − (c,d)` came out of `expand` uncombined while `simplify`
        // combined it — the one gap that rule exists to close.
        Expr::Neg(a) => {
            let factors = vec![Expr::int(-1), expand_core(a)];
            if let Some(distributed) = distribute_over_vector(&factors) {
                return distributed;
            }
            let fallback = mul(factors.clone());
            let result = distribute_guarded(try_distribute(&factors), fallback);
            expand_container_entries(result)
        }

        // Product: distribute the (already-expanded) factors; on cap overflow
        // fall back to the unexpanded (canonical) product.
        Expr::Mul(fs) => {
            let factors: Vec<Expr> = fs.iter().map(expand_core).collect();
            if let Some(contracted) = contract_product(&factors) {
                return contracted;
            }
            if let Some(distributed) = distribute_over_vector(&factors) {
                return distributed;
            }
            let fallback = mul(factors.clone());
            let result = distribute_guarded(try_distribute(&factors), fallback);
            expand_container_entries(result)
        }

        // Division distributes its numerator over the denominator, which is left
        // as-is (neither expanded nor distributed into): (a+b)/d → a/d + b/d.
        Expr::Div(a, b) => {
            let num = expand_core(a);
            let inv = pow((**b).clone(), Expr::int(-1));
            let factors = vec![num, inv];
            let fallback = mul(factors.clone());
            distribute_guarded(try_distribute(&factors), fallback)
        }

        Expr::Pow(base, exp) => {
            let base = expand_core(base);
            let exp = expand_core(exp);
            // Multinomial-expand a non-negative integer power of a sum.
            if let Expr::Num(Number::Int(n)) = &exp {
                let is_sum = matches!(&base, Expr::Add(ts) if ts.len() > 1);
                if (1..=crate::resource_limits::current().max_expand_power).contains(n) && is_sum {
                    let factors = vec![base.clone(); *n as usize];
                    let fallback = pow(base.clone(), exp.clone());
                    return distribute_guarded(try_distribute(&factors), fallback);
                }
            }
            // A matrix power (`M^k`) collapses to a literal matrix here, with
            // entries the `matmul` chain built but never expanded.
            expand_container_entries(pow(base, exp))
        }

        // Everything else (function applications, sequences, relations, leaves):
        // recurse into children but do not distribute across this node.
        _ => map_children(e, expand_core),
    }
}

/// Accept a distributed expansion only when it does not increase the number of
/// `±` operators. Distributing a factor that carries a `±` across a sum (or
/// multinomial-expanding a sum that contains one) would clone a single sign
/// choice into several independent ones — changing the value set. In that case,
/// and on cap overflow (`None`), keep the unexpanded canonical `fallback`.
fn distribute_guarded(distributed: Option<Expr>, fallback: Expr) -> Expr {
    match distributed {
        Some(d) if crate::ops::pm::count_pm(&d) <= crate::ops::pm::count_pm(&fallback) => d,
        _ => fallback,
    }
}

/// The additive terms of `e`: the operands of an `Add`, or `e` itself.
fn terms_of(e: Expr) -> Vec<Expr> {
    match e {
        Expr::Add(ts) => ts,
        other => vec![other],
    }
}

/// Multiply out a list of (already-expanded) factors into a single expanded
/// sum. Like terms are combined after each factor (via the smart `add`), so the
/// live term count tracks the *combined* size, not the raw Cartesian product.
/// Returns `None` when a step would exceed the raw-term cap
/// (`resource_limits::current().max_expand_terms`) — the
/// caller keeps the node unexpanded.
fn try_distribute(factors: &[Expr]) -> Option<Expr> {
    let mut acc = vec![Expr::int(1)];
    for f in factors {
        let f_terms = terms_of(f.clone());
        if acc.len().saturating_mul(f_terms.len())
            > crate::resource_limits::current().max_expand_terms
        {
            return None;
        }
        let mut next = Vec::with_capacity(acc.len() * f_terms.len());
        for a in &acc {
            for b in &f_terms {
                next.push(mul(vec![a.clone(), b.clone()]));
            }
        }
        // Combine like terms now: keeps acc multinomially bounded for powers
        // of the same sum ((a+b)^n stays at n+1 terms, not 2^n).
        acc = terms_of(add(next));
    }
    Some(add(acc))
}
