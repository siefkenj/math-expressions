//! Exact polynomial algebra.
//!
//! - [`multivariate`] — recursive dense (SymPy DMP) GCD over ℚ, backing
//!   `reduce_rational` / `cancel`.
//! - [`univariate`] — dense ℚ[t] utilities for the `RootOf` pipeline and
//!   quotient-ring elimination.
//! - [`factor`] — univariate factorization over ℚ.
//! - [`rootof`] — the `RootOf` leaf: construction, power reduction, numeric eval.
//! - [`ratform`] — rational-function normal form (`together` / `cancel`).

mod multivariate;

pub mod factor;
pub mod ratform;
pub(crate) mod rootof;
pub(crate) mod univariate;

pub(crate) use multivariate::*;
