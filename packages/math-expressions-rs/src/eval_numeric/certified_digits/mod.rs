//! Arbitrary-precision evaluation — "certified digits".
//!
//! Evaluates an expression to as many correct significant digits as requested,
//! or reports that it cannot within budget — it never returns an uncertified
//! digit. Pipeline: canonical tree → flat tape ([`tape`], iterative) → Tier R
//! (exact rational — the canonicalizer already folded it) → Tier 0 (f64 with
//! certified error bounds, [`float_bounds`]) → Tier 2 ([`fix`]'s `MpFix` fixed
//! point at a working precision chosen by a magnitude-informed backward
//! planning pass, escalated by a Ziv loop). Failures are values
//! (`Precise::Unknown`), never hangs or panics; every loop is operation-counted
//! under the configured resource limits.
//!
//! The pipeline orchestration lives in [`pipeline`]; the numeric kernels
//! (series, argument reduction, shared π/ln2/e caches) in [`kernels`];
//! complex-plane arithmetic in [`cfix`]; adaptive quadrature in [`quad`] and
//! its divergence analysis in [`diverge`].

pub mod cfix;
pub mod diverge;
pub mod fix;
pub mod float_bounds;
pub mod kernels;
pub mod quad;
pub mod tape;

mod pipeline;

pub use diverge::{integrate_analyzed, IntegralVerdict, SingularPoint};
pub use pipeline::{eval_tape, evaluate_to_precision, DecimalFormat, Precise};
pub use quad::integrate_to_precision;
pub use tape::{compile, CompileError};

// Consumed by `quad.rs` via `super::needed_bits`.
use pipeline::needed_bits;
