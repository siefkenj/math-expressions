//! JavaScript interop — conversions between the crate's [`Expr`](crate::expr::Expr)
//! tree and the shapes JavaScript consumes.
//!
//! Currently just the [`tree`] codec (`Expr` ⇄ JS `Tree` JSON). Template
//! matching (`js_match`) lives in the wasm crate, since it is only used across
//! the JS boundary. New JS-boundary converters belong here.

pub mod tree;
