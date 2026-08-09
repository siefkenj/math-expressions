//! Presentation layer: canonical tree → display tree.
//!
//! `canonicalize`'s output is optimized for *equality testing*, not reading:
//! `Div` becomes `Mul·Pow⁻¹`, `Neg` becomes a `−1` coefficient, and
//! commutative operands sort by `normalize::order`'s variant-rank order, which
//! prints `x^2 + 2x + 1` as `1 + x^2 + 2 x`. This pass converts a canonical
//! tree into the equivalent faithful tree a calculus student would write:
//!
//! - negative exponents become division (`x^(−1) → 1/x`, `3 x^(−2) → 3/x²`),
//!   with rational coefficients joining the fraction (`(2/3)·x⁻¹ → 2/(3 x)`);
//! - negative leading coefficients become `Neg`, so sums render with `−`;
//! - `Add` terms sort like a polynomial: descending total degree, ties broken
//!   graded-lexicographically on the variables — so constants come last and
//!   `x² + x y + y²` reads in the conventional order;
//! - `Mul` factors sort coefficient first, then alphabetically by base.
//!
//! The pass is display-only and meaning-preserving: `canonicalize(present(e))
//! == e` for canonical `e`, and `present` is idempotent (it understands the
//! `Div`/`Neg` shapes it produces, so re-presenting is a no-op).

use crate::expr::Expr;
use crate::num::{BigNumber, Number};
use std::cmp::Ordering;

use crate::expr::map_children;

/// Convert a canonical tree to its display form (see module docs).
pub(crate) fn present(e: &Expr) -> Expr {
    match e {
        Expr::Add(ts) => present_add(ts),
        Expr::Mul(fs) => present_mul(fs),
        Expr::Pow(b, x) => present_pow(b, x),
        _ => map_children(e, present),
    }
}

/// Present an exponent. A fraction-spelled non-integer exponent displays as a
/// fraction — `x^(3/2)` — while a decimal-spelled one stays a plain number,
/// `x^1.5`. `split_number` applies that gate for every caller here.
fn present_exponent(x: &Expr) -> Expr {
    if let Expr::Num(n) = x {
        let (neg, num, den) = split_number(n);
        if !den.is_one() {
            let frac = Expr::Div(Box::new(Expr::Num(num)), Box::new(Expr::Num(den)));
            return if neg { Expr::Neg(Box::new(frac)) } else { frac };
        }
    }
    present(x)
}

/// `b^x` with a negative exponent displays as `1/b^(−x)`.
fn present_pow(b: &Expr, x: &Expr) -> Expr {
    // A matrix-valued base never moves under a fraction bar: `A^(-1)` is the
    // inverse, not the scalar `1/A`.
    if matches!(b, Expr::Matrix { .. }) {
        return Expr::Pow(Box::new(present(b)), Box::new(present_exponent(x)));
    }
    if let Some(pos) = negated_exponent(x) {
        return Expr::Div(
            Box::new(Expr::int(1)),
            Box::new(pow_display(present(b), present_exponent(&pos))),
        );
    }
    Expr::Pow(Box::new(present(b)), Box::new(present_exponent(x)))
}

/// `base^exp` for display, collapsing `base^1` (which arises when a `x^(−1)`
/// factor moves to a denominator) to `base`.
fn pow_display(base: Expr, exp: Expr) -> Expr {
    if matches!(&exp, Expr::Num(n) if n.is_one()) {
        base
    } else {
        Expr::Pow(Box::new(base), Box::new(exp))
    }
}

/// If `x` is a definitely-negative canonical exponent, return its negation:
/// a negative number, or a `Mul` whose leading numeric coefficient is
/// negative (`Mul(−1, n) → n`). `None` for anything sign-ambiguous.
fn negated_exponent(x: &Expr) -> Option<Expr> {
    match x {
        Expr::Num(n) if n.is_negative() => Some(Expr::Num(n.neg())),
        Expr::Mul(fs) => match fs.first() {
            Some(Expr::Num(n)) if n.is_negative() => {
                let m = n.neg();
                let rest = &fs[1..];
                if m.is_one() && rest.len() == 1 {
                    Some(rest[0].clone())
                } else if m.is_one() {
                    Some(Expr::Mul(rest.to_vec()))
                } else {
                    let mut out = vec![Expr::Num(m)];
                    out.extend(rest.iter().cloned());
                    Some(Expr::Mul(out))
                }
            }
            _ => None,
        },
        _ => None,
    }
}

/// Split a canonical `Mul` into sign, numerator, and denominator, and
/// reassemble as `[Neg] num`, `[Neg] num/den`. The numeric coefficient's
/// numerator/denominator split across the fraction bar (`(2/3)·x⁻¹ →
/// 2/(3 x)`, `(1/2)·x → x/2`).
fn present_mul(fs: &[Expr]) -> Expr {
    let mut negative = false;
    let mut coeff_num = Number::Int(1);
    let mut coeff_den = Number::Int(1);
    let mut num_factors: Vec<Expr> = Vec::new();
    let mut den_factors: Vec<Expr> = Vec::new();

    for f in fs {
        match f {
            Expr::Num(n) => {
                let (neg, num, den) = split_number(n);
                negative ^= neg;
                coeff_num = num;
                coeff_den = den;
            }
            Expr::Pow(b, x) => {
                let neg_exp = if matches!(**b, Expr::Matrix { .. }) {
                    None // A^(-1) is an inverse, not a fraction (MATRIX_PLAN §1a)
                } else {
                    negated_exponent(x)
                };
                if let Some(pos) = neg_exp {
                    den_factors.push(pow_display(present(b), present_exponent(&pos)));
                } else {
                    num_factors.push(present(f));
                }
            }
            _ => num_factors.push(present(f)),
        }
    }

    sort_factors(&mut num_factors);
    sort_factors(&mut den_factors);

    let num = assemble(coeff_num, num_factors);
    let out = if den_factors.is_empty() && coeff_den.is_one() {
        num
    } else {
        Expr::Div(Box::new(num), Box::new(assemble(coeff_den, den_factors)))
    };
    if negative {
        Expr::Neg(Box::new(out))
    } else {
        out
    }
}

/// One side of a fraction bar: the coefficient (dropped when it is a
/// redundant 1) followed by the factors.
fn assemble(coeff: Number, factors: Vec<Expr>) -> Expr {
    let mut items = Vec::with_capacity(factors.len() + 1);
    if !coeff.is_one() || factors.is_empty() {
        items.push(Expr::Num(coeff));
    }
    items.extend(factors);
    if items.len() == 1 {
        items.pop().unwrap()
    } else {
        Expr::Mul(items)
    }
}

/// `n` as (is_negative, |numerator|, denominator). Denominator 1 means "do not
/// put this under a fraction bar": floats, integers, and — the reason the
/// spelling is tracked at all — any rational that *displays* as a decimal.
///
/// Without that last case a decimal coefficient was silently rewritten into a
/// fraction: `0.5·x` presented as `Div(x, 2)`, which both read wrong and, once
/// the tree was re-canonicalized, left two plain integers behind with the
/// decimal origin gone for good. `0.5^2` came back as `1/4` for exactly that
/// reason, several passes downstream of anything that looked responsible.
pub(crate) fn split_number(n: &Number) -> (bool, Number, Number) {
    let neg = n.is_negative();
    let a = n.abs();
    if a.decimal_spelling().is_some() {
        return (neg, a, Number::Int(1));
    }
    match &a {
        Number::Rat(p, q, _) => (neg, Number::Int(*p), Number::Int(*q)),
        Number::Big(b) => match &**b {
            BigNumber::Rat(r, _) => (
                neg,
                Number::from_bigint(r.numer().clone()),
                Number::from_bigint(r.denom().clone()),
            ),
            BigNumber::Int(_) => (neg, a, Number::Int(1)),
        },
        _ => (neg, a, Number::Int(1)),
    }
}

/// Sort multiplicands alphabetically by their base symbol (`x² y`, not `y x²`);
/// factors with no symbol/constant base (functions, sums) keep their canonical
/// order at the end. `π`/`e`/`i` sort by *name* here, in either spelling, so
/// they read where an alphabetical reader expects them — unless
/// [`ConstantPolicy::sort_constants_first`] is on, which is the only thing that
/// pulls a declared constant to the front.
///
/// [`ConstantPolicy::sort_constants_first`]: crate::ConstantPolicy::sort_constants_first
fn sort_factors(factors: &mut [Expr]) {
    fn key(f: &Expr) -> (u8, String) {
        let base = if let Expr::Pow(b, _) = f { &**b } else { f };
        match atom_name(base) {
            // A bare `+`/`-` inside a `pm` reads as a sign on the product, so it
            // trails every real factor — `2 π (−)`, not `2 (−) π`. Legacy keys
            // it `[8, "plus_minus_string", …]`, last of all; same intent here.
            Some(name) if name == "-" || name == "+" => (ORDINARY_NAME + 2, name),
            Some(name) => (atom_rank(&name), name),
            // No symbol base: after every named factor, canonical order kept.
            None => (ORDINARY_NAME + 1, String::new()),
        }
    }
    factors.sort_by_cached_key(key); // stable: ties keep canonical order
}

/// The name a leaf sorts under, for either spelling of a named constant.
fn atom_name(e: &Expr) -> Option<String> {
    match e {
        Expr::Sym(s) => Some(s.name()),
        Expr::Const(_) => crate::constant_policy::constant_spelling(e),
        _ => None,
    }
}

/// Order `Add` terms like a polynomial: descending total degree, then
/// graded-lexicographic on the (alphabetized) variables. The sort is stable,
/// so equal keys keep their canonical order.
fn present_add(ts: &[Expr]) -> Expr {
    let mut items: Vec<(DegKey, Expr)> = ts.iter().map(|t| (deg_key(t), present(t))).collect();
    items.sort_by(|a, b| key_order(&a.0, &b.0));
    Expr::Add(items.into_iter().map(|p| p.1).collect())
}

/// A term's monomial signature: total degree plus per-atom exponents, sorted by
/// [`atom_rank`] then name. Non-monomial parts (function applications,
/// unexpanded powers of sums, symbolic exponents) contribute degree 0.
///
/// **Constants count.** `π`, `e` and `i` carry degree here exactly as a
/// variable does, which is what keeps `a·e + b·f` in the order it was written:
/// with constants excluded, `a·e` was degree 1 against `b·f`'s 2 and sorted
/// second.
struct DegKey {
    total: f64,
    vars: Vec<(u8, String, f64)>,
}

/// Which class an atom sorts in. By default there is only one class, so the
/// order is alphabetical and `π` sits between `n` and `r` like any other name.
///
/// That is deliberate, and it is what the JS oracle does under *every* setting
/// of its own `define_pi`/`define_e`/`define_i` — its `default_order` never
/// consulted them. A rank that promoted constants was tried and reverted: it
/// put `e` ahead of `π` in a product while `π` led in a sum, printed `2 i π`
/// for `2πi`, turned `d + e + f` into `e + d + f`, and broke the compat suite's
/// `3a+3b+3c+2d+2e+2f+…`. There is no ranking that serves both `2πr` and a
/// document whose points are `(e, f)` — which is why the question is *declared*
/// ([`crate::constant_policy`]) rather than guessed.
///
/// [`ConstantPolicy::sort_constants_first`] opts a document into the
/// conventional reading, and only then do declared constants take rank 0.
///
/// [`ConstantPolicy::sort_constants_first`]: crate::ConstantPolicy::sort_constants_first
fn atom_rank(name: &str) -> u8 {
    if !crate::constant_policy::current().sorts_first(name) {
        return ORDINARY_NAME;
    }
    // Within the promoted class the order is conventional, not alphabetical:
    // `2 π i`, never `2 e π` or `2 i π`.
    match name {
        "pi" => 0,
        "e" => 1,
        _ => 2,
    }
}

/// Rank of a factor that is neither a promoted constant nor one of the trailing
/// classes in [`sort_factors`]. Ordinary names sort among themselves by name.
const ORDINARY_NAME: u8 = 3;

fn deg_key(t: &Expr) -> DegKey {
    let mut vars: Vec<(u8, String, f64)> = Vec::new();
    collect_deg(t, 1.0, &mut vars);
    vars.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    // Merge duplicate names (e.g. from a presented `Div` with x on both sides).
    vars.dedup_by(|next, prev| {
        if prev.1 == next.1 {
            prev.2 += next.2;
            true
        } else {
            false
        }
    });
    vars.retain(|(_, _, d)| *d != 0.0);
    let total = vars.iter().map(|(_, _, d)| d).sum();
    DegKey { total, vars }
}

fn collect_deg(t: &Expr, mult: f64, vars: &mut Vec<(u8, String, f64)>) {
    match t {
        Expr::Sym(s) => {
            let name = s.name().to_string();
            vars.push((atom_rank(&name), name, mult));
        }
        // `π`, `e` and `i` reach here as constants rather than symbols
        // depending on how the expression was built; both spellings must land
        // in the same class. The non-finite specials are not atoms of a
        // monomial and are left out.
        Expr::Const(_) => {
            if let Some(name) = crate::constant_policy::constant_spelling(t) {
                vars.push((atom_rank(&name), name, mult));
            }
        }
        Expr::Pow(b, x) => {
            if let Expr::Num(n) = &**x {
                collect_deg(b, mult * n.to_f64(), vars);
            }
        }
        Expr::Mul(fs) => {
            for f in fs {
                collect_deg(f, mult, vars);
            }
        }
        // Presented shapes, so re-presenting (idempotence) sees the same keys.
        Expr::Neg(a) => collect_deg(a, mult, vars),
        Expr::Div(a, b) => {
            collect_deg(a, mult, vars);
            collect_deg(b, -mult, vars);
        }
        _ => {}
    }
}

/// `Less` ⇔ `a` displays before `b`: higher total degree first, then the
/// first alphabetical variable where the exponents differ, higher first.
fn key_order(a: &DegKey, b: &DegKey) -> Ordering {
    match b.total.partial_cmp(&a.total) {
        Some(Ordering::Equal) | None => {}
        Some(o) => return o,
    }
    let (mut i, mut j) = (0, 0);
    while i < a.vars.len() || j < b.vars.len() {
        let (ar, an, ad) = a
            .vars
            .get(i)
            .map(|(r, n, d)| (*r, n.as_str(), *d))
            .unwrap_or((u8::MAX, "\u{10FFFF}", 0.0));
        let (br, bn, bd) = b
            .vars
            .get(j)
            .map(|(r, n, d)| (*r, n.as_str(), *d))
            .unwrap_or((u8::MAX, "\u{10FFFF}", 0.0));
        match ar.cmp(&br).then_with(|| an.cmp(bn)) {
            Ordering::Equal => {
                match bd.partial_cmp(&ad) {
                    Some(Ordering::Equal) | None => {}
                    Some(o) => return o,
                }
                i += 1;
                j += 1;
            }
            // One term has an (alphabetically earlier) variable the other
            // lacks: the term that has it displays first (x·y before z²).
            Ordering::Less => return Ordering::Less,
            Ordering::Greater => return Ordering::Greater,
        }
    }
    Ordering::Equal
}
