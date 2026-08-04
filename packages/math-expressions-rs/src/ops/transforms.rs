//! Structural rewrites that map one faithful tree to another: symbol
//! substitution, subscript ⇄ flat-name conversion, tuple/vector reinterpretation,
//! interval coercion, and function-name canonicalization.

use crate::expr::map_children;
use crate::expr::Expr;
use std::collections::HashMap;

/// Simultaneously replace each `Sym(name)` with `subs[name]`. Substitution is
/// one-pass and simultaneous — a replacement is not itself re-substituted, so
/// `{x: y, y: x}` swaps — and does NOT simplify (`x^2` with `x → 2` gives
/// `2^2`, not `4`), matching `me.substitute`. Recurses into every subexpression,
/// including function arguments.
pub fn substitute(e: &Expr, subs: &HashMap<String, Expr>) -> Expr {
    match e {
        Expr::Sym(s) => match subs.get(&s.name()) {
            Some(rep) => rep.clone(),
            None => e.clone(),
        },
        _ => map_children(e, |c| substitute(c, subs)),
    }
}

/// Collapse simple subscripts into flat symbol names, port of
/// `me.subscripts_to_strings`: `x_1` (`Index(x, 1)`) → the symbol `x_1`.
/// Only bare-symbol bases with a number or bare-symbol index convert;
/// everything else is left structural.
pub fn subscripts_to_strings(e: &Expr) -> Expr {
    if let Expr::Index(base, idx) = e {
        if let Expr::Sym(b) = &**base {
            let suffix = match &**idx {
                Expr::Sym(s) => Some(s.name()),
                Expr::Num(n) => n.terminating_decimal(),
                _ => None,
            };
            if let Some(sfx) = suffix {
                return Expr::sym(&format!("{}_{}", b.name(), sfx));
            }
        }
    }
    map_children(e, subscripts_to_strings)
}

/// Inverse of [`subscripts_to_strings`]: a symbol containing `_` splits at the
/// first underscore into `Index(base, index)`, with a numeric suffix parsed as
/// a number (`x_1` → `Index(x, 1)`, `y_a` → `Index(y, a)`).
pub fn strings_to_subscripts(e: &Expr) -> Expr {
    if let Expr::Sym(s) = e {
        let name = s.name();
        if let Some(pos) = name.find('_') {
            let (base, sfx) = (&name[..pos], &name[pos + 1..]);
            if !base.is_empty() && !sfx.is_empty() {
                let idx = match sfx.parse::<i64>() {
                    Ok(n) => Expr::int(n),
                    Err(_) => Expr::sym(sfx),
                };
                return Expr::Index(Box::new(Expr::sym(base)), Box::new(idx));
            }
        }
        return e.clone();
    }
    map_children(e, strings_to_subscripts)
}

/// Convert 2-element tuples/arrays into interval notation, port of
/// `me.to_intervals`: `(1,2)` → the open interval, `[1,2]` → the closed one
/// (half-open forms already parse as intervals). Recurses everywhere; other
/// shapes are untouched.
pub fn to_intervals(e: &Expr) -> Expr {
    use crate::expr::SeqKind;
    if let Expr::Seq(kind, xs) = e {
        if xs.len() == 2 && matches!(kind, SeqKind::Tuple | SeqKind::Array) {
            let closed = matches!(kind, SeqKind::Array);
            return Expr::Interval {
                endpoints: Box::new((to_intervals(&xs[0]), to_intervals(&xs[1]))),
                closed: (closed, closed),
            };
        }
    }
    map_children(e, to_intervals)
}

/// `me.normalize_function_names`: fold alternate function spellings to their
/// canonical form (`arcsin` → `asin`, `ln` → `log`, …) via the function
/// registry's alias map. Only bare-symbol heads are rewritten.
pub fn normalize_function_names(e: &Expr) -> Expr {
    fn rename_head(h: &Expr) -> Expr {
        match h {
            Expr::Sym(s) => match crate::special_functions::canonical_name(&s.name()) {
                Some(canon) => Expr::sym(canon),
                None => h.clone(),
            },
            Expr::Pow(b, x) => Expr::Pow(Box::new(rename_head(b)), x.clone()),
            Expr::Prime(x) => Expr::Prime(Box::new(rename_head(x))),
            other => other.clone(),
        }
    }
    if let Expr::Apply(head, args) = e {
        return Expr::Apply(
            Box::new(rename_head(head)),
            args.iter().map(normalize_function_names).collect(),
        );
    }
    map_children(e, normalize_function_names)
}

/// `me.tuples_to_vectors`: reinterpret tuple sequences as vectors.
pub fn tuples_to_vectors(e: &Expr) -> Expr {
    use crate::expr::SeqKind;
    if let Expr::Seq(SeqKind::Tuple, xs) = e {
        return Expr::Seq(SeqKind::Vector, xs.iter().map(tuples_to_vectors).collect());
    }
    map_children(e, tuples_to_vectors)
}

/// `me.altvectors_to_vectors`: reinterpret `⟨…⟩` alt-vectors as vectors.
pub fn altvectors_to_vectors(e: &Expr) -> Expr {
    use crate::expr::SeqKind;
    if let Expr::Seq(SeqKind::AltVector, xs) = e {
        return Expr::Seq(
            SeqKind::Vector,
            xs.iter().map(altvectors_to_vectors).collect(),
        );
    }
    map_children(e, altvectors_to_vectors)
}
