//! The aggregates: variadic reducers over an argument list (`sum`, `mean`,
//! `max`, …), as opposed to every other family here, which has a fixed arity.
//!
//! Two things set this family apart:
//!
//! - **Arity.** They use [`FnDef::evaln`] and the whole-slice form of
//!   [`FnDef::fold_exact`], because `sum(1,2)` and `sum(1,2,3,4)` are the same
//!   function. Note the JS tree encoding is lossy here: `["apply","sum",
//!   ["tuple",1,2,3]]` and a genuine single tuple argument decode to the same
//!   `Apply(sum, [1,2,3])`, so a one-tuple call and a three-argument call
//!   cannot be told apart. That is inherited from the wire format, not chosen.
//! - **No parser spellings.** `parse_text`/`parse_latex` are deliberately
//!   empty, matching legacy: its `appliedFunctionSymbols` default has no
//!   `sum`/`mean`/`max` either, so `"sum(1,2,3)"` splits into `s·u·m·(1,2,3)`
//!   in both libraries. A caller that wants them applied passes
//!   `appliedFunctionSymbols` to the parser; building the tree directly
//!   (`fromAst`) always works. Adding them here would silently reinterpret
//!   `mean` and `max` in existing expressions that use them as variables.
//!
//! `variance` is the *sample* variance (divide by n−1), and `std` its square
//! root — matching mathjs, which is what legacy evaluated through.

use super::{FnDef, DEFAULTS};
use num_bigint::BigInt;
use num_complex::Complex64;
use num_rational::BigRational;
use num_traits::Zero;

pub const SUM: FnDef = FnDef {
    name: "sum",
    evaln: Some(|xs| Some(xs.iter().sum())),
    fold_exact: Some(|xs| Some(xs.iter().sum())),
    ..DEFAULTS
};

pub const PROD: FnDef = FnDef {
    name: "prod",
    evaln: Some(|xs| Some(xs.iter().product())),
    fold_exact: Some(|xs| Some(xs.iter().product())),
    ..DEFAULTS
};

pub const COUNT: FnDef = FnDef {
    name: "count",
    evaln: Some(|xs| Some(Complex64::new(xs.len() as f64, 0.0))),
    fold_exact: Some(|xs| Some(BigRational::from(BigInt::from(xs.len())))),
    ..DEFAULTS
};

pub const MEAN: FnDef = FnDef {
    name: "mean",
    evaln: Some(|xs| non_empty(xs).map(|_| xs.iter().sum::<Complex64>() / xs.len() as f64)),
    fold_exact: Some(mean_exact),
    ..DEFAULTS
};

pub const MEDIAN: FnDef = FnDef {
    name: "median",
    // Ordering is not defined on the complex plane, so the float path takes
    // the real parts — the same restriction `floor`/`ceil` accept.
    evaln: Some(|xs| real_sorted(xs).map(|s| middle(&s))),
    fold_exact: Some(|xs| {
        let mut s = xs.to_vec();
        s.sort();
        non_empty(&s)?;
        Some(middle_exact(&s))
    }),
    ..DEFAULTS
};

pub const MAX: FnDef = FnDef {
    name: "max",
    evaln: Some(|xs| real_sorted(xs).map(|s| Complex64::new(*s.last().unwrap(), 0.0))),
    fold_exact: Some(|xs| xs.iter().max().cloned()),
    ..DEFAULTS
};

pub const MIN: FnDef = FnDef {
    name: "min",
    evaln: Some(|xs| real_sorted(xs).map(|s| Complex64::new(s[0], 0.0))),
    fold_exact: Some(|xs| xs.iter().min().cloned()),
    ..DEFAULTS
};

pub const VARIANCE: FnDef = FnDef {
    name: "variance",
    evaln: Some(|xs| {
        let n = xs.len();
        if n < 2 {
            return None;
        }
        let m = xs.iter().sum::<Complex64>() / n as f64;
        let ss: Complex64 = xs.iter().map(|x| (x - m) * (x - m)).sum();
        Some(ss / (n - 1) as f64)
    }),
    fold_exact: Some(variance_exact),
    ..DEFAULTS
};

pub const STD: FnDef = FnDef {
    name: "std",
    evaln: Some(|xs| VARIANCE.evaln.unwrap()(xs).map(|v| v.sqrt())),
    // Exact only when the variance is a perfect rational square: `std(1,2,3)`
    // is `√1 = 1` and folds, `std(1,2,4)` is `√(7/3)` and does not.
    fold_exact: Some(|xs| exact_sqrt(&variance_exact(xs)?)),
    ..DEFAULTS
};

fn non_empty<T>(xs: &[T]) -> Option<()> {
    (!xs.is_empty()).then_some(())
}

fn mean_exact(xs: &[BigRational]) -> Option<BigRational> {
    non_empty(xs)?;
    Some(xs.iter().sum::<BigRational>() / BigRational::from(BigInt::from(xs.len())))
}

fn variance_exact(xs: &[BigRational]) -> Option<BigRational> {
    if xs.len() < 2 {
        return None;
    }
    let m = mean_exact(xs)?;
    let ss: BigRational = xs.iter().map(|x| (x - &m) * (x - &m)).sum();
    Some(ss / BigRational::from(BigInt::from(xs.len() - 1)))
}

/// The middle of an already-sorted list — the mean of the two middle entries
/// when the count is even, so `median(1,2,3,4)` is `5/2`.
fn middle_exact(sorted: &[BigRational]) -> BigRational {
    let n = sorted.len();
    if n % 2 == 1 {
        return sorted[n / 2].clone();
    }
    (&sorted[n / 2 - 1] + &sorted[n / 2]) / BigRational::from(BigInt::from(2))
}

/// The exact square root of a non-negative rational, when numerator and
/// denominator are both perfect squares. `None` otherwise — an irrational root
/// has no exact form here and must leave the application unfolded.
fn exact_sqrt(v: &BigRational) -> Option<BigRational> {
    if v.is_zero() {
        return Some(BigRational::zero());
    }
    if v < &BigRational::zero() {
        return None; // not real; the caller leaves the node symbolic
    }
    let (n, d) = (v.numer().sqrt(), v.denom().sqrt());
    let root = BigRational::new(n, d);
    (&root * &root == *v).then_some(root)
}

fn real_sorted(xs: &[Complex64]) -> Option<Vec<f64>> {
    non_empty(xs)?;
    let mut s: Vec<f64> = Vec::with_capacity(xs.len());
    for z in xs {
        if z.im != 0.0 {
            return None;
        }
        s.push(z.re);
    }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(s)
}

fn middle(sorted: &[f64]) -> Complex64 {
    let n = sorted.len();
    let v = if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    };
    Complex64::new(v, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(n: i64) -> BigRational {
        BigRational::from(BigInt::from(n))
    }
    fn q(n: i64, d: i64) -> BigRational {
        BigRational::new(BigInt::from(n), BigInt::from(d))
    }

    #[test]
    fn exact_aggregates_match_the_legacy_values() {
        let f = |d: &FnDef, xs: &[BigRational]| d.fold_exact.unwrap()(xs);
        assert_eq!(f(&SUM, &[r(3), r(17), r(1)]), Some(r(21)));
        assert_eq!(f(&PROD, &[r(2), r(3), r(4)]), Some(r(24)));
        assert_eq!(f(&MEAN, &[r(1), r(2), r(3)]), Some(r(2)));
        assert_eq!(f(&MEAN, &[r(1), r(2), r(4)]), Some(q(7, 3)));
        assert_eq!(f(&MEDIAN, &[r(1), r(2), r(3)]), Some(r(2)));
        assert_eq!(f(&MEDIAN, &[r(1), r(2), r(3), r(4)]), Some(q(5, 2)));
        assert_eq!(f(&COUNT, &[r(1), r(2), r(3)]), Some(r(3)));
        assert_eq!(f(&MAX, &[r(1), r(5), r(3)]), Some(r(5)));
        assert_eq!(f(&MIN, &[r(1), r(5), r(3)]), Some(r(1)));
        // Sample variance (n−1), matching mathjs.
        assert_eq!(f(&VARIANCE, &[r(1), r(2), r(3)]), Some(r(1)));
        assert_eq!(f(&VARIANCE, &[r(1), r(2), r(4)]), Some(q(7, 3)));
        assert_eq!(f(&VARIANCE, &[r(1), r(2), r(3), r(4)]), Some(q(5, 3)));
    }

    /// `std` folds only when the variance is a perfect square — the point of
    /// exact folding is that an irrational stays symbolic.
    #[test]
    fn std_folds_only_on_a_perfect_square_variance() {
        let f = |xs: &[BigRational]| STD.fold_exact.unwrap()(xs);
        assert_eq!(f(&[r(1), r(2), r(3)]), Some(r(1))); // √1
        assert_eq!(f(&[r(1), r(2), r(4)]), None); // √(7/3)
        assert_eq!(f(&[r(1), r(2), r(3), r(4)]), None); // √(5/3)
        // Variance 1/4, a perfect rational square.
        assert_eq!(f(&[r(0), q(1, 2), r(1)]), Some(q(1, 2)));
        // No spread at all.
        assert_eq!(f(&[r(3), r(3)]), Some(r(0)));
        // Two points can only give `√((x−y)²/2)`, which is irrational unless
        // the points coincide — worth pinning, it is an easy thing to get wrong.
        assert_eq!(f(&[q(1, 2), r(1)]), None);
    }

    /// Degenerate arities return `None` rather than a wrong number, so the
    /// application is left as written.
    #[test]
    fn degenerate_arities_do_not_fold() {
        assert_eq!(MEAN.fold_exact.unwrap()(&[]), None);
        assert_eq!(MEDIAN.fold_exact.unwrap()(&[]), None);
        assert_eq!(VARIANCE.fold_exact.unwrap()(&[r(1)]), None);
        assert_eq!(STD.fold_exact.unwrap()(&[r(1)]), None);
        assert_eq!(MAX.fold_exact.unwrap()(&[]), None);
        // A one-element sum is still that element.
        assert_eq!(SUM.fold_exact.unwrap()(&[r(3)]), Some(r(3)));
        assert_eq!(COUNT.fold_exact.unwrap()(&[]), Some(BigRational::zero()));
    }

    #[test]
    fn exact_sqrt_is_exact() {
        assert_eq!(exact_sqrt(&r(4)), Some(r(2)));
        assert_eq!(exact_sqrt(&q(9, 16)), Some(q(3, 4)));
        assert_eq!(exact_sqrt(&r(2)), None);
        assert_eq!(exact_sqrt(&r(0)), Some(r(0)));
        assert_eq!(exact_sqrt(&r(-1)), None);
    }
}
