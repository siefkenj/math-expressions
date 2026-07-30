//! mathjs compatibility shims.
//!
//! These functions reimplement the parts of `me.math` — the mathjs instance
//! the JS `math-expressions` re-exports — that **Doenet consumes**. They exist
//! so the WASM boundary can offer the numeric operations Doenet calls today
//! (`mod`/`gcd`/`lcm`, statistics, dense `lusolve`, numeric `eigs`, the `dopri`
//! ODE integrator for `ODESystem`) without Doenet having to reach for mathjs.
//!
//! # This module is a way-station, not a home
//!
//! Everything here is "compat" only because the Rust CAS does not *yet* have a
//! first-class equivalent. Each function is expected to **graduate**: once the
//! native capability exists, the operation moves out of `mathjs_compat` to its
//! proper home as a core feature and stops being a compatibility shim. When you
//! add that capability, migrate the function rather than leaving a duplicate
//! here — this module should shrink over time, ideally to nothing.
//!
//! Concretely, where each shim is headed once the capability lands:
//!
//! - [`dense_f64::lusolve`] — belongs with a **certified numerical linear
//!   solver**. If that solver already exists, this should move *now*; it lives
//!   here only because the dense f64 Gaussian elimination predates it.
//! - [`dense_f64::eigs`] — the numeric eigendecomposition; graduates alongside the
//!   symbolic/certified eigen work (cf. `crate::matrix::eigenvalues`).
//! - [`ode`] — the `dopri` integrator; belongs in a first-class ODE/numerics
//!   subsystem (e.g. under `crate::calculus`) once one exists.
//! - [`scalar`] — `mod`/`gcd`/`lcm` and the statistics helpers; graduate to a
//!   general numeric-utilities core if/when the crate grows one.
//!
//! Nothing here is part of the CAS's symbolic contract: these are plain
//! double-precision numerics, deliberately separate from the exact/symbolic
//! counterparts (certified constants in [`crate::eval_exact`], arbitrary
//! precision in [`crate::eval_numeric::certified_digits`], symbolic matrix algebra in
//! [`crate::matrix`]).

pub mod ode;

mod dense_f64;
mod scalar;

pub use dense_f64::{eigs, lusolve, NumericEigenPair};
pub use scalar::{gcd_f64, lcm_f64, math_mod, mean, median, quantile_seq, std_dev, variance};
