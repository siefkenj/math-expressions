//! Expression utilities: small, self-contained ports of the corresponding
//! `me.*` methods, grouped by concern.
//!
//! - [`components`] — component paths into the JS tree operand lists
//! - [`numbers`] — numeric folding and display rounding
//! - [`preserve_order`] — the order-preserving (`skip_ordering`) numeric fold
//! - [`query`] — inspection (functions / operators / variables)
//! - [`transforms`] — structural rewrites (substitute, subscripts, intervals, …)
//! - [`vector_matrix`] — move `+`/scalar-`*` inside vector & matrix containers
//! - [`units`] — unit annotation stripping / adding
//! - [`analytic`] — the `isAnalytic` predicate
//! - [`evaluate`] — numeric evaluation at bindings
//! - [`pm`] — the plus-minus (`±`) operator helpers

mod analytic;
mod components;
mod evaluate;
mod numbers;
pub mod pm;
mod preserve_order;
mod query;
mod transforms;
mod units;
mod vector_matrix;

pub use analytic::{is_analytic, AnalyticOpts};
pub use components::{get_component, substitute_component};
pub use evaluate::{evaluate, evaluate_to_constant};
pub use numbers::{
    constants_to_floats, evaluate_numbers, evaluate_numbers_evaluate_functions, reduce_rational,
    round_numbers_to_decimals, round_numbers_to_precision,
    round_numbers_to_precision_plus_decimals, set_small_zero,
};
pub use preserve_order::evaluate_numbers_preserve_order;
pub use query::{functions, operators, variables};
pub use transforms::{
    altvectors_to_vectors, normalize_function_names, strings_to_subscripts, subscripts_to_strings,
    substitute, to_intervals, tuples_to_vectors,
};
pub use units::{add_unit, remove_scaling_units, remove_units};
pub use vector_matrix::perform_vector_matrix_additions_scalar_multiplications;
