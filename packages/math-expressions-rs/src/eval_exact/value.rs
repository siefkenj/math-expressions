//! The exact-constant ring [`Exact`] and its arithmetic.
//!
//! A value is a ℚ-linear combination of the basis monomials `π^i · e^j · √r`
//! where `r` is a squarefree positive integer (`r = 1` ⇒ no surd). Those
//! monomials are linearly independent over ℚ: the surds `{√r : r squarefree}`
//! are ℚ-linearly independent (a standard result), and π, e are transcendental
//! (their algebraic independence from the surds — and, as every CAS assumes,
//! from each other — is taken as given). Hence the combination is zero **iff**
//! every coefficient is zero, which is exactly [`Exact::is_zero`].
//!
//! Multiplication stays in the ring because `√r · √s` re-normalizes to a
//! rational multiple of a single surd (`√12 = 2√3`); see [`mul_surd`].

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, ToPrimitive, Zero};

use crate::expr::Expr;
use crate::num::Number;

/// A basis monomial `π^pi · e^e · √rad`. `rad` is a squarefree integer ≥ 1
/// (`rad == 1` means no surd factor).
type Mono = (u32, u32, BigInt);

/// An element of ℚ[π, e] ⊗ (ℚ-span of surds), as a sparse map from basis
/// monomial to rational coefficient. Zero coefficients are pruned, so the
/// value is zero exactly when the map is empty.
#[derive(Clone, Debug, Default)]
pub struct Exact {
    terms: BTreeMap<Mono, BigRational>,
}

impl Exact {
    pub(super) fn zero() -> Exact {
        Exact::default()
    }

    /// The rational `q` (as `q · π^0 e^0 √1`).
    pub(super) fn rat(q: BigRational) -> Exact {
        let mut t = BTreeMap::new();
        if !q.is_zero() {
            t.insert((0, 0, BigInt::one()), q);
        }
        Exact { terms: t }
    }

    /// A single monomial `coeff · π^pi · e^e · √rad` (rad already squarefree).
    pub(super) fn mono(coeff: BigRational, pi: u32, e: u32, rad: BigInt) -> Exact {
        let mut t = BTreeMap::new();
        if !coeff.is_zero() {
            t.insert((pi, e, rad), coeff);
        }
        Exact { terms: t }
    }

    /// `coeff · √rad` for a rad that is **already squarefree** (the lattice
    /// tables pass literals 2/3/6; `eval_sqrt` passes the squarefree part it
    /// just computed). No factoring, no panic path — callers that hold a
    /// possibly-square-divisible radicand must go through [`squarefree_part`]
    /// first.
    pub(super) fn surd(coeff: BigRational, squarefree_rad: u128) -> Exact {
        Exact::mono(coeff, 0, 0, BigInt::from(squarefree_rad))
    }

    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    pub(super) fn add(&self, other: &Exact) -> Exact {
        let mut terms = self.terms.clone();
        for (k, v) in &other.terms {
            let e = terms.entry(k.clone()).or_insert_with(BigRational::zero);
            *e += v;
            if e.is_zero() {
                terms.remove(k);
            }
        }
        Exact { terms }
    }

    pub(super) fn neg(&self) -> Exact {
        Exact {
            terms: self.terms.iter().map(|(k, v)| (k.clone(), -v)).collect(),
        }
    }

    pub(super) fn mul(&self, other: &Exact, budget: &mut i64) -> Option<Exact> {
        let mut acc = Exact::zero();
        for ((p1, e1, r1), c1) in &self.terms {
            for ((p2, e2, r2), c2) in &other.terms {
                spend(budget)?;
                let (coeff, rad) = mul_surd(c1 * c2, r1, r2)?;
                let mono = Exact::mono(coeff, p1 + p2, e1 + e2, rad);
                acc = acc.add(&mono);
            }
        }
        Some(acc)
    }

    /// This value as a plain rational, if it has no π/e/surd part.
    pub(super) fn as_rational(&self) -> Option<BigRational> {
        match self.terms.len() {
            0 => Some(BigRational::zero()),
            1 => {
                let ((p, e, r), c) = self.terms.iter().next().unwrap();
                (*p == 0 && *e == 0 && r.is_one()).then(|| c.clone())
            }
            _ => None,
        }
    }

    /// The rational `p` such that `self == p · π` (only the `π^1` monomial),
    /// or `0` when `self` is zero. `None` if any other component is present.
    pub(super) fn as_pi_multiple(&self) -> Option<BigRational> {
        let mut p = BigRational::zero();
        for ((pi, e, r), c) in &self.terms {
            if *pi == 1 && *e == 0 && r.is_one() {
                p = c.clone();
            } else {
                return None;
            }
        }
        Some(p)
    }

    /// `1/self` when `self` is a nonzero pure rational or a single surd term
    /// `c·√r`; otherwise `None` (general field inversion is not implemented).
    pub(super) fn inverse(&self) -> Option<Exact> {
        if let Some(q) = self.as_rational() {
            return (!q.is_zero()).then(|| Exact::rat(BigRational::one() / q));
        }
        if self.terms.len() == 1 {
            let ((p, e, r), c) = self.terms.iter().next().unwrap();
            if *p == 0 && *e == 0 && !r.is_one() && !c.is_zero() {
                // 1/(c·√r) = √r / (c·r)
                let denom = c * BigRational::from_integer(r.clone());
                return Some(Exact::mono(BigRational::one() / denom, 0, 0, r.clone()));
            }
        }
        None
    }

    /// This value as a canonical expression — the inverse direction of
    /// [`exact_eval`](super::exact_eval), used to emit exact special values
    /// such as `sin(pi/6) → 1/2`, `sec(pi/4) → sqrt(2)`, etc.
    pub(crate) fn to_expr(&self) -> Expr {
        if self.terms.is_empty() {
            return Expr::int(0);
        }
        let mut terms = Vec::with_capacity(self.terms.len());
        for ((pi, e, rad), c) in &self.terms {
            let mut factors: Vec<Expr> = vec![Expr::Num(Number::from_bigrational(c.clone()))];
            // π/e are emitted as `Sym` — the canonical spelling. (`Const(Pi)`
            // exists as a variant, but the parsers only produce `Sym`, and
            // canonicalize unifies `Const(Pi/E/I)` → `Sym`; minting `Sym`
            // directly keeps this output canonical without a re-pass.)
            if *pi > 0 {
                factors.push(crate::normalize::pow(
                    Expr::sym("pi"),
                    Expr::int(i64::from(*pi)),
                ));
            }
            if *e > 0 {
                factors.push(crate::normalize::pow(
                    Expr::sym("e"),
                    Expr::int(i64::from(*e)),
                ));
            }
            if !rad.is_one() {
                let radn =
                    Expr::Num(Number::from_bigrational(BigRational::from_integer(rad.clone())));
                // `sqrt(rad)` (an `Apply`), not `rad^(1/2)` (a `Pow`): the
                // former is the canonical surd spelling the parsers/simplify
                // use, so `to_expr` output unifies with the rest of the system
                // (e.g. `full_simplify(cos(π/6))` == `sqrt(3)/2`).
                factors.push(Expr::Apply(Box::new(Expr::sym("sqrt")), vec![radn]));
            }
            terms.push(crate::normalize::mul(factors));
        }
        crate::normalize::canonicalize(&crate::normalize::add(terms))
    }

    pub(super) fn pow_int(&self, k: i64, budget: &mut i64) -> Option<Exact> {
        if k == 0 {
            return Some(Exact::rat(BigRational::one()));
        }
        let base = if k < 0 { self.inverse()? } else { self.clone() };
        let mut acc = Exact::rat(BigRational::one());
        for _ in 0..k.unsigned_abs() {
            spend(budget)?;
            acc = acc.mul(&base, budget)?;
        }
        Some(acc)
    }
}

/// `coeff · √r1 · √r2` re-normalized to `(coeff', r')` with `r'` squarefree.
fn mul_surd(coeff: BigRational, r1: &BigInt, r2: &BigInt) -> Option<(BigRational, BigInt)> {
    if r1.is_one() {
        return Some((coeff, r2.clone()));
    }
    if r2.is_one() {
        return Some((coeff, r1.clone()));
    }
    let prod = (r1 * r2).to_u128()?;
    let (s, f) = squarefree_part(prod)?;
    Some((coeff * BigRational::from_integer(BigInt::from(s)), BigInt::from(f)))
}

/// Write `m = s²·f` with `f` squarefree; return `(s, f)`. `None` if `m` is 0
/// or its square-factoring would exceed the trial-division budget.
pub(super) fn squarefree_part(mut m: u128) -> Option<(u128, u128)> {
    if m == 0 {
        return None;
    }
    let mut s: u128 = 1;
    let mut d: u128 = 2;
    while let Some(dd) = d.checked_mul(d) {
        if dd > m {
            break;
        }
        while m.is_multiple_of(dd) {
            m /= dd;
            s = s.checked_mul(d)?;
        }
        d += 1;
        // Cap trial division so a large near-prime radicand can't stall us —
        // §7f-governed like every other unpredictable-cost bound.
        if d > u128::from(crate::resource_limits::current().max_squarefree_trial_divisor) {
            return None;
        }
    }
    Some((s, m))
}

/// Decrement the shared operation budget; `None` once it is exhausted.
pub(super) fn spend(budget: &mut i64) -> Option<()> {
    *budget -= 1;
    (*budget >= 0).then_some(())
}
