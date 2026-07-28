//! Unit annotations: stripping and adding `["unit", …]` wrappers, port of
//! `me.remove_units` / `me.remove_scaling_units` / `me.add_unit`.

use crate::expr::Expr;
use crate::normalize::syntactic::map_children;

/// The known scaling-unit spellings the parsers emit (see
/// `lib/expression/units.js`): `%`, `deg` (and its LaTeX spelling `circ`),
/// and the prefix `$`.
fn is_unit_symbol(e: &Expr) -> bool {
    matches!(e, Expr::Sym(s) if matches!(s.name().as_str(), "%" | "$" | "deg" | "circ"))
}

/// The value operand of a `["unit", …]` node — the operand that is not the
/// unit symbol itself (prefix `$` puts the symbol first, postfix units last).
fn unit_body(args: &[Expr]) -> Option<&Expr> {
    match args {
        [a, b] if is_unit_symbol(a) => Some(b),
        [a, b] if is_unit_symbol(b) => Some(a),
        _ => None,
    }
}

/// `me.remove_units`: strip unit annotations. With `scale_based_on_unit`, the
/// scaling units are applied (`50%` → `1/2`, `90 deg` → `pi/2`) via
/// [`crate::normalize::desugar_units`]; without it, the bare value is kept
/// (`50%` → `50`).
pub fn remove_units(e: &Expr, scale_based_on_unit: bool) -> Expr {
    if scale_based_on_unit {
        return crate::normalize::desugar_units(e);
    }
    if let Expr::OtherOp(name, args) = e {
        if name.name() == "unit" {
            if let Some(body) = unit_body(args) {
                return remove_units(body, false);
            }
        }
    }
    map_children(e, |c| remove_units(c, false))
}

/// `me.remove_scaling_units`: drop only the *scaling* units (`%`, `deg`, `$`),
/// rewriting them into plain arithmetic. Identical to the equality-time
/// [`crate::normalize::desugar_units`] pass.
pub fn remove_scaling_units(e: &Expr) -> Expr {
    crate::normalize::desugar_units(e)
}

/// `me.add_unit`: wrap `e` in the given unit. `$` is a prefix unit
/// (`unit($, e)`); everything else is postfix (`unit(e, name)`).
pub fn add_unit(e: &Expr, unit: &str) -> Expr {
    let args = if unit == "$" {
        vec![Expr::sym("$"), e.clone()]
    } else {
        vec![e.clone(), Expr::sym(unit)]
    };
    Expr::OtherOp(crate::sym::Sym::new("unit"), args)
}
