//! Numeric evaluation of expression trees, at two precisions:
//!
//! - [`complex`] — the f64 complex-plane sampler (`eval_complex`), the minimal
//!   slice the equality tester (and the assumptions/analytic checks) need.
//! - [`certified_digits`] — arbitrary-precision evaluation that returns as many
//!   *certified* correct significant digits as requested, or reports it cannot.
//!
//! Both are CAS-internal infrastructure, *not* mathjs shims: the Doenet-facing
//! `me.math` replacements (`mod`/`gcd`/`lcm`, statistics, `lusolve`, `eigs`,
//! `dopri`) live in [`crate::mathjs_compat`].

pub mod certified_digits;
pub mod complex;
