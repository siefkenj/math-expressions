//! Expression tree: the core [`Expr`] enum ([`tree`]) plus read-only
//! traversal and n-ary flattening ([`visit`]).

mod tree;
mod visit;

pub use tree::{Expr, MathConst, RelOp, SeqKind};
pub use visit::flatten;
