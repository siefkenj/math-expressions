//! Scaling units (`%`, `deg`, `$`): the single source of truth for the
//! `["unit", …]` node layout, plus the equality-time desugaring pass.
//!
//! [`is_scaling_unit_symbol`] and [`unit_body`] own the operand-layout
//! knowledge (which operand is the symbol, which is the value); the `me.*`
//! stripping façade in [`crate::ops`] (`remove_units` / `add_unit`) delegates
//! here rather than re-deriving it. [`desugar_units`] is the equality-time
//! analogue of JS `remove_scaling_units` combined with numerical unit removal.

use crate::expr::Expr;

/// The scaling-unit spellings the parsers emit (see `lib/expression/units.js`):
/// `%`, `deg` (LaTeX spelling `circ`), and the prefix `$`. This is the *symbol*
/// set — a superset of what [`desugar_units`] rewrites (`circ` is recognized as
/// a unit but has no numeric desugaring rule, so it is stripped but not scaled).
pub(crate) fn is_scaling_unit_symbol(e: &Expr) -> bool {
    matches!(e, Expr::Sym(s) if matches!(s.name().as_str(), "%" | "$" | "deg" | "circ"))
}

/// Decode a two-operand `["unit", …]` node into `(symbol, value)`. The parsers
/// emit prefix `$` as `[unit, value]` and postfix `%`/`deg` as `[value, unit]`
/// (mirroring `get_unit_value_of_tree` in lib/expression/units.js); this takes
/// whichever operand *is* a scaling-unit symbol, so either order decodes for
/// either spelling — matching what `ops::remove_units` has always accepted.
/// If both operands are unit symbols the first one is treated as the unit.
fn unit_parts(args: &[Expr]) -> Option<(&Expr, &Expr)> {
    match args {
        [a, b] if is_scaling_unit_symbol(a) => Some((a, b)),
        [a, b] if is_scaling_unit_symbol(b) => Some((b, a)),
        _ => None,
    }
}

/// The value operand of a `["unit", …]` node (the operand that is not the unit
/// symbol). The shared layout primitive behind `me.remove_units`.
pub(crate) fn unit_body(args: &[Expr]) -> Option<&Expr> {
    unit_parts(args).map(|(_, value)| value)
}

/// The three scaling units from lib/expression/units.js.
enum Unit {
    /// `$` — a `prefix` unit that only marks its value (`scale: x => x`), so it
    /// survives desugaring as a free factor.
    Dollar,
    /// `%` — `only_scales`, `scale: x => x / 100`.
    Percent,
    /// `deg` — `only_scales`, `scale: x => x * pi / 180`.
    Deg,
}

/// Classify a `["unit", …]` node into its desugarable [`Unit`] and value.
/// `None` for a non-unit node or the `circ` spelling (recognized as a unit
/// symbol, but with no numeric scaling rule).
fn unit_value(args: &[Expr]) -> Option<(Unit, &Expr)> {
    let (symbol, value) = unit_parts(args)?;
    let Expr::Sym(s) = symbol else { return None };
    let unit = match s.name().as_str() {
        "$" => Unit::Dollar,
        "%" => Unit::Percent,
        "deg" => Unit::Deg,
        _ => return None,
    };
    Some((unit, value))
}

/// Rewrite scaling-unit nodes into plain arithmetic. This is the equality-time
/// analogue of JS `remove_scaling_units` (lib/expression/simplify.js) combined
/// with numerical unit removal:
///
/// - `n %`   → `n / 100`
/// - `n deg` → `n * pi / 180`
/// - `$ n`   → `$ * n`  (the `$` becomes an ordinary factor)
///
/// Making `$` a plain multiplication by the symbol `$` is what preserves the JS
/// semantics with no special-casing downstream: the like-term folding in
/// [`add`](super::add) then gives `$3 + $2 → $5`, while the numerical stage
/// samples `$` as a free variable, so `$5` never equals a bare `5`. It is
/// applied only in the full [`equals`](crate::equals) path — never in
/// `equalsViaSyntax` — so `50%` and `1/2` stay *syntactically* distinct even
/// though they are numerically equal.
pub fn desugar_units(e: &Expr) -> Expr {
    // One variant-specific rewrite; everything else is the blessed traversal
    // (`map_children`), so new `Expr` variants need no edit here.
    if let Expr::OtherOp(name, args) = e {
        if name.name() == "unit" {
            match unit_value(args) {
                Some((Unit::Dollar, v)) => {
                    return Expr::Mul(vec![Expr::sym("$"), desugar_units(v)])
                }
                Some((Unit::Percent, v)) => {
                    return Expr::Div(Box::new(desugar_units(v)), Box::new(Expr::int(100)))
                }
                Some((Unit::Deg, v)) => {
                    return Expr::Div(
                        Box::new(Expr::Mul(vec![
                            desugar_units(v),
                            // `Sym`, not `Const(Pi)`: the canonical spelling
                            // of π (matches the parsers; keeps `==`/tolerance
                            // paths on one representation).
                            Expr::sym("pi"),
                        ])),
                        Box::new(Expr::int(180)),
                    );
                }
                // An `OtherOp("unit", …)` that does not match a known unit
                // shape is left structurally intact (recurse into operands
                // via the shared traversal below).
                None => {}
            }
        }
    }
    crate::expr::map_children(e, desugar_units)
}
