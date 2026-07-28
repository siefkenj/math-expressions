//! f64 numerics replacing the parts of `me.math` (the re-exported mathjs
//! instance) that Doenet consumes — the drop-in for what Doenet uses
//! *numerically* today, deliberately separate from the exact/symbolic
//! counterparts (matrix algebra, `RootOf` eigenvalues, certified precision).
//!
//! - [`scalar`] — `mod`/`gcd`/`lcm` and statistics
//! - [`linalg`] — dense `lusolve` and `eigs`
//! - [`ode`] — the `dopri` ODE solver (Doenet's `ODESystem`)
//! - [`complex`] — complex-plane evaluation, the minimal slice the equality
//!   tester needs (re-exported crate-wide as `eval_numerical`)

mod linalg;
mod scalar;

pub mod complex;
pub mod ode;

pub use linalg::{eigs, lusolve, EigenPair};
pub use scalar::{gcd_f64, lcm_f64, math_mod, mean, median, quantile_seq, std_dev, variance};
