//! Small shared helpers for the integrator: literal/apply constructors, the
//! `x`-dependence test, division-by-`b`, and the linear-argument coefficient
//! extractor (`u = a + b·x`) that every elementary-table row keys on.

use crate::expr::Expr;
use crate::normalize::{add, mul, pow};
use crate::num::Number;

pub(super) fn int(i: i64) -> Expr {
    Expr::Num(Number::Int(i))
}

pub(super) fn apply(name: &str, arg: Expr) -> Expr {
    Expr::Apply(Box::new(Expr::sym(name)), vec![arg])
}

pub(super) fn depends_on(e: &Expr, x: &str) -> bool {
    crate::ops::variables(e).iter().any(|v| v == x)
}

pub(super) fn over(e: Expr, b: &Expr) -> Expr {
    if matches!(b, Expr::Num(n) if n.is_one()) {
        e
    } else {
        mul(vec![e, pow(b.clone(), int(-1))])
    }
}

/// `u = a + b·x` with x-free `b` (returned). `None` if `u` is not linear
/// in `x`. The constant part is never needed by the rules — only `b`.
pub(super) fn linear_coeff(u: &Expr, x: &str) -> Option<Expr> {
    fn term_coeff(t: &Expr, x: &str) -> Option<Expr> {
        // A canonical term that is exactly c·x (or x).
        match t {
            Expr::Sym(s) if s.name() == x => Some(int(1)),
            Expr::Mul(fs) => {
                let mut coeff = Vec::new();
                let mut seen_x = false;
                for f in fs {
                    match f {
                        Expr::Sym(s) if s.name() == x => {
                            if seen_x {
                                return None;
                            }
                            seen_x = true;
                        }
                        f if !depends_on(f, x) => coeff.push(f.clone()),
                        _ => return None,
                    }
                }
                seen_x.then(|| mul(coeff))
            }
            _ => None,
        }
    }
    match u {
        _ if !depends_on(u, x) => None,
        Expr::Add(ts) => {
            let mut b: Option<Expr> = None;
            for t in ts {
                if !depends_on(t, x) {
                    continue;
                }
                let c = term_coeff(t, x)?;
                b = Some(match b {
                    None => c,
                    Some(prev) => add(vec![prev, c]),
                });
            }
            b.filter(|b| !matches!(b, Expr::Num(n) if n.is_zero()))
        }
        other => term_coeff(other, x),
    }
}
