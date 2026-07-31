//! Normalization: a pure faithful-layer → canonical-layer transform. Canonical
//! form eliminates the display-only variants (`Div`, `Neg`), flattens and sorts
//! commutative operators, folds constants *exactly*, and combines like terms
//! and like powers — so two equal canonical expressions are identical trees and
//! structural equality is tree comparison.
//!
//! `canonicalize` is confluent, cheap, and assumption-free. Heuristic
//! simplification (root pulling, trig/log identities) is a separate, deferred
//! layer that needs the assumptions system.
//!
//! Barrel module. The canonical layer's core is factored into:
//!
//! - [`canonicalize`] — the bottom-up dispatch and application/relation canon
//! - [`constructors`] — the `add`/`mul`/`pow` smart constructors
//! - [`matrix_ops`]   — literal-matrix helpers used by the constructors
//! - [`units`]        — scaling-unit desugaring (`%`, `deg`, `$`)
//! - [`full`]         — the aggressive `full_simplify` fixpoint driver that the
//!   public `simplify` / `simplify_with` delegate to
//!
//! plus the `expand`, `order`, `present`, `simplify` (the base rewrite
//! clusters), `special_values`, and `syntactic` passes.

mod canonicalize;
mod constructors;
mod full;
mod matrix_ops;
mod units;

pub(crate) mod expand;
pub(crate) mod order;
pub(crate) mod present;
pub(crate) mod simplify;
pub(crate) mod special_values;
pub(crate) mod syntactic;

pub(crate) use expand::expand_core;
pub(crate) use order::cmp;
pub(crate) use present::present;
pub(crate) use simplify::{simplify_base_with, simplify_canonical, simplify_core};
pub use expand::expand;
pub use simplify::{simplify, simplify_logical, simplify_with};
pub use special_values::fold_special_values;
pub use syntactic::normalize_syntactic;

pub use canonicalize::canonicalize;
pub(crate) use full::full_simplify;
pub use units::desugar_units;
pub(crate) use constructors::{add, mul, pow, split_coeff};
pub(crate) use units::unit_body;
pub(crate) use matrix_ops::{identity_matrix, is_matrix_valued, matmul_literal};
