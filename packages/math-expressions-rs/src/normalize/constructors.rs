//! The `add`/`mul`/`pow` smart constructors: they assume their children are
//! already canonical and re-establish the canonical invariants (flattened,
//! sorted, exactly folded, like terms/powers combined).

use super::{cmp, identity_matrix, is_matrix_valued, matmul_literal};
use crate::expr::{Expr, Mat, MathConst};
use crate::num::Number;
use std::cell::Cell;

thread_local! {
    /// See [`without_like_term_collection`]. Ambient rather than a parameter
    /// because `add` is reached from every level of `canonicalize`'s recursion
    /// and from `mul`/`pow`; the same reasoning (and the same `Cell`/restore
    /// shape) as [`crate::resource_limits`].
    static COLLECT_LIKE_TERMS: Cell<bool> = const { Cell::new(true) };
}

/// Run `f` with like-*term* collection in [`add`] switched off, so `x + 3x`
/// stays two summands instead of becoming `4x`.
///
/// This exists for one caller: [`evaluate_numbers`], which backs DoenetML's
/// `simplify="numbers"` — specified as "fold numeric constants, leave the
/// symbolic structure alone". Collecting like terms is a correct
/// simplification but not a *numeric* one, and doing it here made
/// `simplify="numbers"` indistinguishable from `simplify="full"`.
///
/// Only sums are affected. Like *powers* in [`mul`] still combine, because the
/// legacy oracle requires it: `i·i` must fold to `−1`, which is that same
/// merge (`i^1 · i^1 → i^2 → −1`).
///
/// Terms that cancel to zero still collapse even when this is off — `3x − 3x`
/// is `0`, not two surviving summands. That is the additive-inverse identity
/// rather than a shortening rewrite, and dropping it would make
/// `evaluate_numbers` unable to see a zero it is asked about (`(3x−3x)^0`).
///
/// [`evaluate_numbers`]: crate::evaluate_numbers
pub(crate) fn without_like_term_collection<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            COLLECT_LIKE_TERMS.with(|c| c.set(self.0));
        }
    }
    let _restore = Restore(COLLECT_LIKE_TERMS.with(|c| c.replace(false)));
    f()
}

/// Build a canonical sum from canonical terms: flatten, fold the numeric part
/// exactly, combine like terms (`3x + 2x → 5x`), drop zeros, sort. Literal
/// matrices of equal dimensions fold entrywise; mismatched dimensions (and
/// matrix + scalar) stay as separate unevaluated terms.
pub(crate) fn add(terms: Vec<Expr>) -> Expr {
    let mut flat = Vec::with_capacity(terms.len());
    for t in terms {
        match t {
            Expr::Add(xs) => flat.extend(xs),
            other => flat.push(other),
        }
    }

    let mut constant = Number::zero();
    // (rest, summed coefficient, the coefficients as written) for each distinct
    // non-constant term. The third field only matters under
    // `without_like_term_collection`, which re-emits the summands separately
    // unless they cancel — see there for why the sum is still tracked.
    let mut parts: Vec<(Expr, Number, Vec<Number>)> = Vec::new();
    // Entrywise accumulation per matrix dimension: (rows, cols, per-entry terms).
    let mut mats: Vec<(u32, u32, Vec<Vec<Expr>>)> = Vec::new();
    for t in flat {
        if let Expr::Matrix(m) = t {
            let (rows, cols) = (m.rows(), m.cols());
            let entries = m.into_entries();
            match mats.iter_mut().find(|(r, c, _)| *r == rows && *c == cols) {
                Some((_, _, acc)) => {
                    for (slot, e) in acc.iter_mut().zip(entries) {
                        slot.push(e);
                    }
                }
                None => mats.push((rows, cols, entries.into_iter().map(|e| vec![e]).collect())),
            }
            continue;
        }
        let (coeff, rest) = split_coeff(t);
        match rest {
            None => constant = constant.add(&coeff),
            // A term carrying an independent ± must never merge with another:
            // `±x + ±x` has value set {2x, 0, −2x} whereas `2·±x` has {2x, −2x},
            // so coalescing like terms would tie the two sign choices together.
            // Keep every pm-bearing term as its own summand (JS `noPmBase`).
            Some(r) if crate::ops::pm::contains_pm(&r) => {
                parts.push((r, coeff.clone(), vec![coeff]))
            }
            Some(r) => match parts.iter_mut().find(|(k, _, _)| *k == r) {
                Some(slot) => {
                    slot.1 = slot.1.add(&coeff);
                    slot.2.push(coeff);
                }
                None => parts.push((r, coeff.clone(), vec![coeff])),
            },
        }
    }

    let mut out = Vec::with_capacity(parts.len() + mats.len() + 1);
    for (rows, cols, acc) in mats {
        // `acc` carries exactly one accumulator per entry of a rows×cols
        // matrix (it was seeded from one and only ever `zip`ped), so this
        // always constructs.
        if let Some(m) = Mat::new(rows, cols, acc.into_iter().map(add).collect()) {
            out.push(Expr::Matrix(m));
        }
    }
    if !constant.is_zero() {
        out.push(Expr::Num(constant));
    }
    let collect = COLLECT_LIKE_TERMS.with(Cell::get);
    for (rest, total, coeffs) in parts {
        // A term whose coefficients cancel disappears either way.
        if total.is_zero() {
            continue;
        }
        if collect || coeffs.len() == 1 {
            out.push(mul(vec![Expr::Num(total), rest]));
            continue;
        }
        for c in coeffs {
            if c.is_zero() {
                continue; // `0·x²` was written but contributes nothing to read
            }
            out.push(mul(vec![Expr::Num(c), rest.clone()]));
        }
    }
    out.sort_by(cmp);
    match out.len() {
        0 => Expr::Num(Number::zero()),
        1 => out.pop().unwrap(),
        _ => Expr::Add(out),
    }
}

/// What a zero numeric coefficient collapses the product to. `0·x` is `0`, but
/// `0·∞` and `0/0` are *indeterminate* and must be `NaN` — DoenetML computes an
/// undefined slope as `0/0`, so annihilating it to `0` reports a degenerate line
/// as horizontal: a wrong number on a grading path rather than a visible
/// failure. Legacy made the same distinction, and in the same order — its
/// `try_evaluate_quotient_of_numbers` tests `is_nonzero(denom) === false` before
/// the `numer === 0 → 0` shortcut, and its product fold checks `!isFinite`
/// before annihilating.
fn annihilate(coeff: &Number, indeterminate: bool) -> Expr {
    if indeterminate {
        Expr::Const(crate::expr::MathConst::NaN)
    } else {
        // Preserve the coefficient's *sign of zero* (`Int(0)` or `−0`), so a
        // product like `(−1)·0` collapses to `−0` and a later `1/(−0)` can fold
        // to `−∞`. `−0` reads as plain `0` everywhere that does not ask.
        Expr::Num(coeff.clone())
    }
}

/// Whether a `base^exp` factor blocks the annihilation above — because it is
/// *provably* non-finite, or because it is already undefined and must poison
/// the product. Only a literal pole (`0^negative`, i.e. `1/0`), the non-finite
/// constants, and `None`/`NaN` qualify. A factor whose finiteness is merely
/// *unknown* — a bare symbol, `1/x`, a function application — does not:
/// legacy's `is_nonzero` returned a third `undefined` state there and fell
/// through to `0`, which is why `0·x` stays `0` and only the provable cases
/// become `NaN`.
///
/// The exponent matters for the infinities as much as it does for `0`: `∞^(-1)`
/// *is* `0`, so `0/∞` is a plain `0` and not the indeterminate `0·∞`. Ignoring
/// it here reported `0/∞` as `NaN` — the same wrong-number failure this guard
/// exists to prevent, in the opposite direction. `NaN`/`None` are undefined at
/// every exponent (`NaN^0` included: the product is still meaningless), so they
/// poison unconditionally.
fn is_infinite_factor(base: &Expr, exp: &Expr) -> bool {
    use crate::expr::MathConst;
    let negative_exponent = matches!(exp, Expr::Num(x) if x.is_negative());
    if matches!(base, Expr::Num(n) if n.is_zero()) && negative_exponent {
        return true;
    }
    match base {
        // `∞^negative` → 0, which annihilates like any other zero.
        Expr::Const(MathConst::Inf | MathConst::NegInf) => !negative_exponent,
        Expr::Const(MathConst::NaN | MathConst::None) => true,
        _ => false,
    }
}

/// Build a canonical product from canonical factors: flatten, fold the numeric
/// coefficient exactly, annihilate on zero, combine like powers
/// (`x² · x³ → x⁵`), drop ones, sort.
///
/// Matrix factors split the product into a commutative scalar segment
/// (everything below) and an **order-preserving matrix
/// segment**: adjacent dimension-compatible literal matrices fold via matrix
/// multiplication, a fully-folded product absorbs the scalar part into its
/// entries, and anything unfoldable (dimension mismatch, unevaluated matrix
/// powers) stays as `Mul([scalars…, matrices-in-order…])`.
pub(crate) fn mul(factors: Vec<Expr>) -> Expr {
    let mut flat = Vec::with_capacity(factors.len());
    for f in factors {
        match f {
            Expr::Mul(xs) => flat.extend(xs),
            other => flat.push(other),
        }
    }

    if flat.iter().any(is_matrix_valued) {
        let (scalars, matrices): (Vec<Expr>, Vec<Expr>) =
            flat.into_iter().partition(|f| !is_matrix_valued(f));
        let scalar_part = mul(scalars); // no matrices: the commutative pipeline
                                        // Fold adjacent compatible literal matrices, left to right.
        let mut seq: Vec<Expr> = Vec::with_capacity(matrices.len());
        for m in matrices {
            match (seq.last(), &m) {
                (Some(Expr::Matrix(_)), Expr::Matrix(_)) => {
                    let prev = seq.pop().unwrap();
                    match matmul_literal(&prev, &m) {
                        Some(folded) => seq.push(folded),
                        None => {
                            seq.push(prev);
                            seq.push(m);
                        }
                    }
                }
                _ => seq.push(m),
            }
        }
        // Fully folded: the scalar part distributes into the entries.
        if seq.len() == 1 {
            if let Expr::Matrix(m) = &seq[0] {
                if !matches!(&scalar_part, Expr::Num(n) if n.is_one()) {
                    // Entrywise, so the shape carries over untouched.
                    return Expr::Matrix(m.map(|e| mul(vec![scalar_part.clone(), e.clone()])));
                }
                return seq.pop().unwrap();
            }
        }
        let mut out = match scalar_part {
            Expr::Num(n) if n.is_one() => Vec::new(),
            Expr::Mul(xs) => xs,
            other => vec![other],
        };
        out.extend(seq);
        return match out.len() {
            1 => out.pop().unwrap(),
            _ => Expr::Mul(out),
        };
    }

    // pm scaling: c · ±x → ±(c·x) when exactly one factor is a ± and every
    // other factor carries no ± of its own. A single value `c` scales across
    // the sign choice (the value set {cx, −cx} is unchanged); more than one ±
    // is left alone, since their signs are independent (JS simplify's `c · ±x`
    // rule, guarded by `c` containing no pm).
    if flat.len() > 1 {
        let pm_idx: Vec<usize> = flat
            .iter()
            .enumerate()
            .filter(|(_, f)| crate::ops::pm::is_pm(f))
            .map(|(i, _)| i)
            .collect();
        if pm_idx.len() == 1
            && flat
                .iter()
                .enumerate()
                .all(|(i, f)| i == pm_idx[0] || !crate::ops::pm::contains_pm(f))
        {
            let Expr::OtherOp(_, args) = flat.remove(pm_idx[0]) else {
                unreachable!()
            };
            let inner = args.into_iter().next().unwrap();
            flat.push(inner);
            let scaled = mul(flat);
            // If the scaled product is itself a ± (the pulled-in factor was a
            // nested ±), it already absorbs this one: ±(±y) = ±y.
            return if crate::ops::pm::is_pm(&scaled) {
                scaled
            } else {
                crate::ops::pm::make_pm(scaled)
            };
        }
    }

    let mut coeff = Number::one();
    // (base, summed exponent) for each distinct base.
    let mut parts: Vec<(Expr, Expr)> = Vec::new();
    for f in flat {
        if let Expr::Num(n) = &f {
            coeff = coeff.mul(n);
            continue;
        }
        let (base, exp) = split_pow(f);
        match parts.iter_mut().find(|(b, _)| *b == base) {
            Some(slot) => slot.1 = add(vec![std::mem::replace(&mut slot.1, Expr::Blank), exp]),
            None => parts.push((base, exp)),
        }
    }

    if coeff.is_zero() {
        return annihilate(&coeff, parts.iter().any(|(b, x)| is_infinite_factor(b, x)));
    }

    let mut out = Vec::with_capacity(parts.len() + 1);
    let mut refold = false;
    for (base, exp) in parts {
        match pow(base, exp) {
            // A folded power may collapse to a number (e.g. exponent 0 → 1).
            Expr::Num(n) => coeff = coeff.mul(&n),
            // A combined power may come back as a *product* (the integer
            // power-of-product rule: `(x·y)^(1/2)·(x·y)^(3/2)` combines to
            // `(x·y)^2`, which distributes to `x²·y²`). Its factors must merge
            // with the others — pushing it whole would nest a Mul inside a Mul
            // and break the flat canonical invariant.
            Expr::Mul(xs) => {
                refold = true;
                out.extend(xs);
            }
            other => out.push(other),
        }
    }
    if coeff.is_zero() {
        return annihilate(&coeff, out.iter().cloned().any(|f| {
            let (b, x) = split_pow(f);
            is_infinite_factor(&b, &x)
        }));
    }
    // Re-run the combining pass so distributed factors pair up with the rest
    // (e.g. an existing `x⁻²` cancels the distributed `x²`). Terminates: the
    // distribution only fires for Mul bases, and its output powers have
    // non-Mul bases, so nesting strictly decreases each round.
    if refold {
        if !coeff.is_one() {
            out.push(Expr::Num(coeff));
        }
        return mul(out);
    }
    out.sort_by(cmp);
    if out.is_empty() {
        return Expr::Num(coeff);
    }
    // The numeric coefficient sorts first (Num has the lowest rank).
    if !coeff.is_one() {
        out.insert(0, Expr::Num(coeff));
    }
    if out.len() == 1 {
        out.pop().unwrap()
    } else {
        Expr::Mul(out)
    }
}

/// Build a canonical power, applying the identities and constant folding that
/// hold without assumptions. `0` to a negative power is left unfolded (an
/// exact division by zero).
/// Is `e` one of the non-finite constants (`±∞`, `NaN`)?
fn is_nonfinite_const(e: &Expr) -> bool {
    matches!(
        e,
        Expr::Const(MathConst::Inf) | Expr::Const(MathConst::NegInf) | Expr::Const(MathConst::NaN)
    )
}

/// Bases for which `base^0` is an indeterminate form rather than 1: zero and
/// the non-finite constants.
///
/// `0^0` is the debatable one — combinatorics and power series take it as 1,
/// and IEEE `pow(0,0)` is 1. As a *limit* form it is indeterminate (`x^0 → 1`
/// but `0^x → 0`), which is the reading a mathematics course teaches and the
/// one this engine reports, alongside `∞ − ∞` and `0 · ∞`.
fn is_indeterminate_power_base(e: &Expr) -> bool {
    is_nonfinite_const(e) || matches!(e, Expr::Num(n) if n.is_zero())
}

pub(crate) fn pow(base: Expr, exp: Expr) -> Expr {
    // Matrix base (MATRIX_PLAN §1a): integer k ≥ 2 on a square matrix folds by
    // binary powering, k = 0 gives the identity, k = 1 the base. Everything
    // else (negative — inverse is Layer 2 —, symbolic, non-square) stays an
    // unevaluated Pow. Ordered before the scalar fast paths: `A^0` must be I,
    // not the scalar 1.
    if let Expr::Matrix(m) = &base {
        let (rows, cols) = (m.rows(), m.cols());
        match as_int(&exp) {
            Some(1) => return base,
            Some(0) if rows == cols => return identity_matrix(rows),
            Some(k)
                if rows == cols
                    && k >= 2
                    && k <= crate::resource_limits::current().max_expand_power =>
            {
                let mut acc = identity_matrix(rows);
                let mut sq = base.clone();
                let mut k = k as u64;
                loop {
                    if k & 1 == 1 {
                        match matmul_literal(&acc, &sq) {
                            Some(m) => acc = m,
                            None => return Expr::Pow(Box::new(base), Box::new(exp)),
                        }
                    }
                    k >>= 1;
                    if k == 0 {
                        return acc;
                    }
                    match matmul_literal(&sq, &sq) {
                        Some(m) => sq = m,
                        None => return Expr::Pow(Box::new(base), Box::new(exp)),
                    }
                }
            }
            // Negative integer power of an *invertible rational* matrix folds
            // through the exact inverse (MATRIX_PLAN §1b); symbolic or
            // singular matrices keep the unevaluated Pow (the assumption-
            // gated inverse is `matrix::matrix_inverse`).
            Some(k) if rows == cols && k < 0 && k > i64::MIN => {
                if let Some(inv) = crate::matrix::invert_rational_literal(&base) {
                    return pow(inv, Expr::int(-k));
                }
                return Expr::Pow(Box::new(base), Box::new(exp));
            }
            _ => return Expr::Pow(Box::new(base), Box::new(exp)),
        }
    }
    if let Expr::Num(e) = &exp {
        if e.is_zero() {
            // `x^0 = 1` requires `x` finite and nonzero. `0^0`, `(±∞)^0` and
            // `NaN^0` are indeterminate forms — folding them to 1 asserts a
            // limit that does not exist. This is also the path
            // `∞/∞` arrives on: it collects to `∞^(1−1)` = `∞^0`.
            if is_indeterminate_power_base(&base) {
                return Expr::Const(MathConst::NaN);
            }
            return Expr::Num(Number::one());
        }
        if e.is_one() {
            return base;
        }
    }
    // RootOf power reduction (MATRIX_PLAN §2d): an integer exponent ≥ deg p
    // (or negative) rewrites through t^n mod p, so polynomials in an abstract
    // root always stay below deg p — `p(RootOf(p,k)) = 0` falls out of this
    // plus like-term folding.
    if matches!(base, Expr::RootOf { .. }) {
        if let Some(k) = as_int(&exp) {
            if let Some(reduced) = crate::polynomials::rootof::power_reduced(&base, k) {
                return reduced;
            }
        }
    }
    // Flatten a nested power when the OUTER exponent is an integer:
    // `(b^a)^k = b^(a·k)` (repeated multiplication/division), valid for any base
    // and integer `k`. Restricting to integer `k` avoids the `(x^2)^(1/2) = |x|`
    // trap. This lets e.g. `x·(x^2)^(-1)` collapse to `x^(-1)` and removable
    // singularities like `d/dx((y/x)·x)` reduce to 0. (§7d nested-Pow flatten.)
    if let Expr::Pow(inner_base, inner_exp) = &base {
        if as_int(&exp).is_some() {
            let combined = mul(vec![(**inner_exp).clone(), exp]);
            return pow((**inner_base).clone(), combined);
        }
    }
    // Distribute an integer power over a product: `(a·b)^k = a^k·b^k` (valid for
    // any factors and integer `k`). Extracts numeric coefficients (`(2x)^(-1) =
    // x^(-1)/2`) and enables cancellations like `x·(2x)^(-1) → 1/2`.
    if let Expr::Mul(factors) = &base {
        // Not valid over a non-commutative (matrix) product: (A·B)² ≠ A²·B².
        if as_int(&exp).is_some() && !factors.iter().any(is_matrix_valued) {
            return mul(factors
                .iter()
                .map(|f| pow(f.clone(), exp.clone()))
                .collect());
        }
    }
    if let Expr::Num(b) = &base {
        if b.is_one() {
            // `1^x = 1` for every finite `x`, including a free variable — but
            // `1^∞` is the classic indeterminate form (it is the shape behind
            // `(1 + 1/n)^n → e`), so an infinite or NaN exponent must not fold.
            if is_nonfinite_const(&exp) {
                return Expr::Const(MathConst::NaN);
            }
            return Expr::Num(Number::one()); // 1^x = 1
        }
        // `as_int` matches only an integer exponent, the case we fold.
        if let Some(k) = as_int(&exp) {
            if let Some(v) = b.checked_pow_int(k) {
                return Expr::Num(v);
            }
            // 0^negative: fall through, stays a Pow node.
        }
    }
    Expr::Pow(Box::new(base), Box::new(exp))
}

/// A term split into (coefficient, remaining factor). `None` remainder means
/// the term is a pure number.
pub(crate) fn split_coeff(t: Expr) -> (Number, Option<Expr>) {
    match t {
        Expr::Num(n) => (n, None),
        Expr::Mul(xs) => {
            if let Some(Expr::Num(n)) = xs.first() {
                let n = n.clone();
                let rest = mul(xs[1..].to_vec());
                (n, Some(rest))
            } else {
                (Number::one(), Some(Expr::Mul(xs)))
            }
        }
        other => (Number::one(), Some(other)),
    }
}

/// A factor split into (base, exponent).
fn split_pow(f: Expr) -> (Expr, Expr) {
    match f {
        Expr::Pow(b, e) => (*b, *e),
        other => (other, Expr::Num(Number::one())),
    }
}

fn as_int(n: &Expr) -> Option<i64> {
    match n {
        Expr::Num(Number::Int(i)) => Some(*i),
        _ => None,
    }
}
