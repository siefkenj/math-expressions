//! The rigorous evaluator: [`exact_eval`] maps an expression to an [`Exact`]
//! value (or `None` outside the tower), and [`trig_special_value`] emits the
//! exact trig special values on the π/12 lattice.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::value::{spend, squarefree_part, Exact};
use crate::expr::{Expr, MathConst};

fn br_int(n: i64) -> BigRational {
    BigRational::from_integer(BigInt::from(n))
}

fn apply1<'a>(e: &'a Expr, name: &str) -> Option<&'a Expr> {
    if let Expr::Apply(head, args) = e {
        if let (Expr::Sym(s), [u]) = (&**head, args.as_slice()) {
            if s.name() == name {
                return Some(u);
            }
        }
    }
    None
}

fn is_e(e: &Expr) -> bool {
    matches!(e, Expr::Const(MathConst::E)) || matches!(e, Expr::Sym(s) if s.name() == "e")
}

fn is_pi(e: &Expr) -> bool {
    matches!(e, Expr::Const(MathConst::Pi)) || matches!(e, Expr::Sym(s) if s.name() == "pi")
}

/// Evaluate `e` to an [`Exact`] value, or `None` if it falls outside the tower.
pub fn exact_eval(e: &Expr) -> Option<Exact> {
    let mut budget = crate::resource_limits::current().max_exact_eval_ops;
    eval(e, &mut budget)
}

fn eval(e: &Expr, budget: &mut i64) -> Option<Exact> {
    spend(budget)?;
    Some(match e {
        Expr::Num(n) => Exact::rat(n.to_bigrational()?),
        _ if is_pi(e) => Exact::mono(BigRational::one(), 1, 0, BigInt::one()),
        _ if is_e(e) => Exact::mono(BigRational::one(), 0, 1, BigInt::one()),
        Expr::Add(ts) => {
            let mut acc = Exact::zero();
            for t in ts {
                acc = acc.add(&eval(t, budget)?);
            }
            acc
        }
        Expr::Mul(fs) => {
            let mut acc = Exact::rat(BigRational::one());
            for f in fs {
                let v = eval(f, budget)?;
                acc = acc.mul(&v, budget)?;
            }
            acc
        }
        Expr::Pow(b, k) => eval_pow(b, k, budget)?,
        Expr::Apply(..) => eval_apply(e, budget)?,
        _ => return None,
    })
}

fn eval_pow(b: &Expr, k: &Expr, budget: &mut i64) -> Option<Exact> {
    // e^x
    if is_e(b) {
        return eval_exp(k, budget);
    }
    let Expr::Num(n) = k else { return None };
    // Integer exponent.
    if let Some(i) = n.to_bigrational().and_then(|q| q.is_integer().then(|| q.to_integer())) {
        return eval(b, budget)?.pow_int(i.to_i64()?, budget);
    }
    // Half-integer exponent ⇒ (inverse) square root of a nonnegative rational.
    let q = n.to_bigrational()?;
    let two = BigRational::from_integer(BigInt::from(2));
    if q == BigRational::one() / &two {
        return eval_sqrt(b, budget);
    }
    if q == -BigRational::one() / &two {
        return eval_sqrt(b, budget)?.inverse();
    }
    None
}

fn eval_sqrt(arg: &Expr, budget: &mut i64) -> Option<Exact> {
    let q = eval(arg, budget)?.as_rational()?;
    if q.is_negative() {
        return None; // complex — out of the real tower
    }
    if q.is_zero() {
        return Some(Exact::zero());
    }
    // √(n/d) = √(n·d)/d.
    let n = q.numer().to_u128()?;
    let d = q.denom().to_u128()?;
    let (s, f) = squarefree_part(n.checked_mul(d)?)?;
    let coeff = BigRational::new(BigInt::from(s), BigInt::from(d));
    Some(Exact::surd(coeff, f))
}

fn eval_exp(arg: &Expr, budget: &mut i64) -> Option<Exact> {
    // e^{ln u} = u.
    if let Some(u) = apply1(arg, "log").or_else(|| apply1(arg, "ln")) {
        return eval(u, budget);
    }
    let v = eval(arg, budget)?;
    let q = v.as_rational()?;
    // e^0 = 1.
    if q.is_zero() {
        return Some(Exact::rat(BigRational::one()));
    }
    // e^k for a nonnegative integer k is the basis monomial e^k. A negative
    // power 1/e^k is not representable in the ring (`to_u32` rejects it) and a
    // non-integer exponent likewise falls through to None — both stay undecided,
    // never wrong.
    if q.is_integer() {
        if let Some(k) = q.to_integer().to_u32() {
            return Some(Exact::mono(BigRational::one(), 0, k, BigInt::one()));
        }
    }
    None
}

fn eval_log(arg: &Expr, budget: &mut i64) -> Option<Exact> {
    // ln(e^u) = u.
    if let Some(u) = apply1(arg, "exp") {
        return eval(u, budget);
    }
    if let Expr::Pow(b, x) = arg {
        if is_e(b) {
            return eval(x, budget);
        }
    }
    if is_e(arg) {
        return Some(Exact::rat(BigRational::one()));
    }
    let v = eval(arg, budget)?;
    (v.as_rational()? == BigRational::one()).then(Exact::zero)
}

fn eval_apply(e: &Expr, budget: &mut i64) -> Option<Exact> {
    let Expr::Apply(head, args) = e else { return None };
    let (Expr::Sym(s), [arg]) = (&**head, args.as_slice()) else {
        return None;
    };
    let name = s.name();
    match name.as_str() {
        "sin" | "cos" | "tan" => eval_trig(&name, arg, budget),
        "sqrt" => eval_sqrt(arg, budget),
        "exp" => eval_exp(arg, budget),
        "log" | "ln" => eval_log(arg, budget),
        "abs" => Some(Exact::rat(eval(arg, budget)?.as_rational()?.abs())),
        // These fold only at their known zero: sinh/tanh/asin/atan(0)=0.
        "sinh" | "tanh" | "asin" | "atan" => {
            (eval(arg, budget)?.as_rational()? == BigRational::zero()).then(Exact::zero)
        }
        "cosh" => (eval(arg, budget)?.as_rational()? == BigRational::zero())
            .then(|| Exact::rat(BigRational::one())),
        _ => None,
    }
}

/// sin/cos/tan at a rational multiple of π on the π/12 lattice (covers the
/// kπ/6 and kπ/4 lattices). Returns `None` for arguments off the lattice or,
/// for tan, at a pole.
fn eval_trig(name: &str, arg: &Expr, budget: &mut i64) -> Option<Exact> {
    let p = eval(arg, budget)?.as_pi_multiple()?;
    // Angle in units of π/12: t = 12·p, must be an integer.
    let t = p * BigRational::from_integer(BigInt::from(12));
    if !t.is_integer() {
        return None;
    }
    let ti = t.to_integer();
    let idx = |modulus: i64| -> usize {
        let m = BigInt::from(modulus);
        (((ti.clone() % &m) + &m) % &m).to_i64().unwrap() as usize
    };
    match name {
        "sin" => Some(sin_lattice(idx(24))),
        "cos" => Some(sin_lattice((idx(24) + 6) % 24)), // cos θ = sin(θ + 90°)
        "tan" => tan_lattice(idx(12)),
        _ => None,
    }
}

/// The exact value of `name(arg)` when `arg` is a rational multiple of π on the
/// π/12 lattice (`sin(pi/6) → 1/2`), as a canonical expression, or `None` off
/// the lattice, at a pole, or when the reciprocal value falls outside the
/// single-term inversion supported here.
pub(crate) fn trig_special_value(name: &str, arg: &Expr) -> Option<Expr> {
    let mut budget = crate::resource_limits::current().max_exact_eval_ops;
    Some(trig_exact(name, arg, &mut budget)?.to_expr())
}

/// All six trig functions on the lattice, as a ring element rather than an
/// expression — the shared core of [`trig_special_value`] and
/// [`inverse_trig_special_value`], which needs to *compare* values and so must
/// not go through a spelling.
fn trig_exact(name: &str, arg: &Expr, budget: &mut i64) -> Option<Exact> {
    Some(match name {
        "sin" | "cos" | "tan" => eval_trig(name, arg, budget)?,
        // cot θ = cos θ / sin θ, computed directly. The `1/tan θ` route returned
        // None at tan's poles (θ = π/2 + kπ) — precisely cot's *zeros*, where
        // cot is 0, not undefined.
        "cot" => {
            let cos = eval_trig("cos", arg, budget)?;
            let sin = eval_trig("sin", arg, budget)?;
            cos.mul(&sin.inverse()?, budget)?
        }
        "sec" => eval_trig("cos", arg, budget)?.inverse()?,
        "csc" => eval_trig("sin", arg, budget)?.inverse()?,
        _ => return None,
    })
}

/// The exact angle `θ` in `name(x) = θ`, when `x` is one of the finitely many
/// values the forward table takes on the π/12 lattice inside the function's
/// principal branch (`asin(1) → π/2`, `acos(√3/2) → π/6`, `atan(1) → π/4`).
/// `None` for everything else, including a genuinely transcendental angle.
///
/// Computed by *inverting the forward table* rather than tabulating the values
/// a second time: each admissible angle's forward value is built and compared
/// with the argument. One table, so the two directions cannot drift apart, and
/// the only thing this function has to get right is the index range — which is
/// exactly the principal branch. Each forward function is injective on its
/// range, so the first match is the only match.
///
/// The comparison happens **in the ring, not on trees**. [`Exact`] is a sparse
/// normal form that is zero exactly when the term map is empty, so
/// `value − arg` decides equality no matter how either side was written:
/// `asin(1/√2)`, `asin(√2/2)` and `asin(0.5·√2)` all fold. Comparing
/// canonicalized expressions instead made the fold depend on whether the
/// radical rules happened to have rationalized the argument first, so
/// `asin(√2/2)` folded and `asin(1/√2)` — the same number — did not.
pub(crate) fn inverse_trig_special_value(name: &str, arg: &Expr) -> Option<Expr> {
    // (forward function, inclusive angle range in units of π/12). Branches match
    // the numeric `eval1`s in `special_functions::trig_inverse`: [−π/2, π/2] for
    // asin/acsc, [0, π] for acos/asec, and the open (−π/2, π/2) for atan/acot,
    // whose reciprocal members are defined there as `a…(1/z)` on those same
    // ranges. Endpoints where the forward value does not exist — csc/cot at 0,
    // sec at π/2 — drop out on their own, since `trig_exact` declines at a pole.
    let (forward, lo, hi) = match name {
        "asin" => ("sin", -6, 6),
        "acsc" => ("csc", -6, 6),
        "acos" => ("cos", 0, 12),
        "asec" => ("sec", 0, 12),
        "atan" => ("tan", -5, 5),
        "acot" => ("cot", -5, 5),
        _ => return None,
    };
    let mut budget = crate::resource_limits::current().max_exact_eval_ops;
    // Outside the tower (a symbolic argument, a transcendental one) there is
    // nothing to compare and no reason to walk the lattice at all.
    let arg = eval(arg, &mut budget)?;
    (lo..=hi).find_map(|j| {
        let angle = twelfths_of_pi(j);
        let value = trig_exact(forward, &angle, &mut budget)?;
        value.add(&arg.neg()).is_zero().then_some(angle)
    })
}

/// The angle `j·π/12`, canonical.
fn twelfths_of_pi(j: i64) -> Expr {
    crate::normalize::canonicalize(&crate::normalize::mul(vec![
        Expr::Num(crate::num::Number::rat(j, 12)),
        Expr::Const(crate::expr::MathConst::Pi),
    ]))
}

/// sin at k·15°, k ∈ 0..24. Uses the 0..12 table and sin(θ+180°) = −sin θ.
fn sin_lattice(k: usize) -> Exact {
    if k >= 12 {
        return sin_lattice(k - 12).neg();
    }
    let q = |a, b| BigRational::new(BigInt::from(a), BigInt::from(b));
    // (√6 ± √2)/4
    let s6p2 = Exact::surd(q(1, 4), 6).add(&Exact::surd(q(1, 4), 2));
    let s6m2 = Exact::surd(q(1, 4), 6).add(&Exact::surd(q(-1, 4), 2));
    match k {
        0 => Exact::zero(),
        1 => s6m2,
        2 => Exact::rat(q(1, 2)),
        3 => Exact::surd(q(1, 2), 2),
        4 => Exact::surd(q(1, 2), 3),
        5 => s6p2,
        6 => Exact::rat(BigRational::one()),
        7 => s6p2,
        8 => Exact::surd(q(1, 2), 3),
        9 => Exact::surd(q(1, 2), 2),
        10 => Exact::rat(q(1, 2)),
        11 => s6m2,
        _ => unreachable!(),
    }
}

/// tan at k·15°, k ∈ 0..12 (tan has period π = 12 units). `None` at the pole.
fn tan_lattice(k: usize) -> Option<Exact> {
    let q = |a, b| BigRational::new(BigInt::from(a), BigInt::from(b));
    Some(match k {
        0 => Exact::zero(),
        1 => Exact::rat(br_int(2)).add(&Exact::surd(br_int(-1), 3)), // 2 − √3
        2 => Exact::surd(q(1, 3), 3),                                // √3/3
        3 => Exact::rat(BigRational::one()),
        4 => Exact::surd(BigRational::one(), 3), // √3
        5 => Exact::rat(br_int(2)).add(&Exact::surd(BigRational::one(), 3)), // 2 + √3
        6 => return None,                        // pole (90°)
        7 => Exact::rat(br_int(-2)).add(&Exact::surd(br_int(-1), 3)), // −(2 + √3)
        8 => Exact::surd(br_int(-1), 3),         // −√3
        9 => Exact::rat(br_int(-1)),
        10 => Exact::surd(q(-1, 3), 3), // −√3/3
        11 => Exact::rat(br_int(-2)).add(&Exact::surd(BigRational::one(), 3)), // −(2 − √3)
        _ => unreachable!(),
    })
}
