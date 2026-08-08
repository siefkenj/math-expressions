//! The long tail: complex-part accessors, combinatorics, rounding, matrix
//! reducers, and other applied symbols with no family of their own. `re`/`im`
//! illustrate per-parser spellings: lowercase in text, capitalized in LaTeX.

use super::{real_only, FnDef, DEFAULTS};
use num_bigint::BigInt;
use num_complex::Complex64;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

/// Apply an exact unary rational rule, or `None` for any other arity.
fn unary(xs: &[BigRational], f: fn(&BigRational) -> Option<BigRational>) -> Option<BigRational> {
    match xs {
        [v] => f(v),
        _ => None,
    }
}

/// Nudge a value that is within a hair of an integer onto it, before rounding.
///
/// `floor(3.999999999999999)` is `3` exactly and `4` to anyone reading it, and
/// the DoenetML tests that pin these functions say so in as many words
/// ("Allow for slight roundoff error"). The number all but always got there by
/// arithmetic — a float sum, a division, a numerically evaluated function —
/// and flooring the shortfall away turns an accumulated 1e-15 into an answer
/// off by one, which is the largest error a rounding function can make.
///
/// The window is relative (1e-14 of the magnitude, never less than 1e-14
/// absolute), so it stays a *roundoff* allowance rather than a rounding rule of
/// its own: `floor(3.99)` is still `3`.
fn snap_to_integer(v: &BigRational) -> BigRational {
    use num_traits::{One, Signed};
    let nearest = v.round();
    let diff = (v - &nearest).abs();
    let scale = {
        let mag = v.abs();
        if mag > BigRational::one() {
            mag
        } else {
            BigRational::one()
        }
    };
    // 1e-14, as an exact rational.
    let window = scale * BigRational::new(1.into(), 100_000_000_000_000u64.into());
    if diff <= window {
        nearest
    } else {
        v.clone()
    }
}

pub const MOD: FnDef = FnDef {
    name: "mod",
    parse_text: &["mod"],
    eval2: Some(|a, b| {
        // mathjs `mod`: floored division — the result takes the sign of the
        // *divisor* (`mod(5,-3) = -1`, unlike `rem_euclid`, which is always
        // non-negative), and `mod(x, 0) = x` (mathjs short-circuits, not NaN).
        // Real-only, matching mathjs (which rejects complex operands).
        let (x, y) = (a.re, b.re);
        let r = if y == 0.0 { x } else { x - y * (x / y).floor() };
        Some(Complex64::new(r, 0.0))
    }),
    fold_exact: Some(|xs| match xs {
        // Floored division, as in the float rule above: the result takes the
        // sign of the divisor, and `mod(x, 0)` is `x`.
        [x, y] if y.is_zero() => Some(x.clone()),
        [x, y] => Some(x - y * (x / y).floor()),
        _ => None,
    }),
    ..DEFAULTS
};

pub const ERF: FnDef = FnDef {
    name: "erf",
    parse_text: &["erf"],
    parse_latex: &["erf"],
    latex_commands: &[("erf", "erf")],
    ..DEFAULTS
};

pub const ARG: FnDef = FnDef {
    name: "arg",
    parse_text: &["arg"],
    parse_latex: &["arg"],
    eval1: Some(|z| Some(Complex64::new(z.arg(), 0.0))),
    latex_commands: &[("arg", "arg")],
    ..DEFAULTS
};

pub const CONJ: FnDef = FnDef {
    name: "conj",
    parse_text: &["conj"],
    parse_latex: &["conj"],
    eval1: Some(|z| Some(z.conj())),
    ..DEFAULTS
};

pub const RE: FnDef = FnDef {
    name: "re",
    parse_text: &["re"],
    parse_latex: &["Re"],
    eval1: Some(|z| Some(Complex64::new(z.re, 0.0))),
    latex_commands: &[("Re", "Re")],
    latex_head: Some("\\Re"),
    ..DEFAULTS
};

pub const IM: FnDef = FnDef {
    name: "im",
    parse_text: &["im"],
    parse_latex: &["Im"],
    eval1: Some(|z| Some(Complex64::new(z.im, 0.0))),
    latex_commands: &[("Im", "Im")],
    latex_head: Some("\\Im"),
    ..DEFAULTS
};

pub const DET: FnDef = FnDef {
    name: "det",
    parse_text: &["det"],
    parse_latex: &["det"],
    latex_commands: &[("det", "det")],
    ..DEFAULTS
};

pub const TRACE: FnDef = FnDef {
    name: "trace",
    parse_text: &["trace"],
    parse_latex: &["trace"],
    eval1: Some(Some),
    ..DEFAULTS
};

pub const NPR: FnDef = FnDef {
    name: "nPr",
    parse_text: &["nPr"],
    parse_latex: &["nPr"],
    eval2: Some(|n, r| combinatorial(n, r, true)),
    fold_exact: Some(|xs| combinatorial_exact(xs, true)),
    ..DEFAULTS
};

pub const NCR: FnDef = FnDef {
    name: "nCr",
    parse_text: &["nCr"],
    parse_latex: &["nCr"],
    eval2: Some(|n, r| combinatorial(n, r, false)),
    fold_exact: Some(|xs| combinatorial_exact(xs, false)),
    ..DEFAULTS
};

pub const FLOOR: FnDef = FnDef {
    name: "floor",
    parse_text: &["floor"],
    parse_latex: &["floor"],
    eval1: Some(|z| real_only(z, f64::floor)),
    fold_exact: Some(|xs| unary(xs, |v| Some(snap_to_integer(v).floor()))),
    ..DEFAULTS
};

pub const CEIL: FnDef = FnDef {
    name: "ceil",
    parse_text: &["ceil"],
    parse_latex: &["ceil"],
    eval1: Some(|z| real_only(z, f64::ceil)),
    fold_exact: Some(|xs| unary(xs, |v| Some(snap_to_integer(v).ceil()))),
    ..DEFAULTS
};

pub const ROUND: FnDef = FnDef {
    name: "round",
    parse_text: &["round"],
    parse_latex: &["round"],
    eval1: Some(|z| real_only(z, f64::round)),
    // `BigRational::round` breaks ties away from zero, the same rule as the
    // `f64::round` above — so the exact and float paths agree. (Both differ
    // from JS `Math.round`, which breaks ties toward +∞: `round(-2.5)` is -3
    // here and -2 there. That divergence predates this facet.)
    fold_exact: Some(|xs| unary(xs, |v| Some(v.round()))),
    ..DEFAULTS
};

pub const ROOTOF: FnDef = FnDef {
    name: "rootof",
    parse_text: &["rootof"],
    ..DEFAULTS
};

/// Not a parseable applied symbol — `n!` postfix notation produces it — but
/// a real function for evaluation: `n! = Γ(n+1)`, as a complex function so
/// identities like `(n+1)·n! = (n+1)!` hold at sampled points.
pub const FACTORIAL: FnDef = FnDef {
    name: "factorial",
    eval1: Some(|z| Some(gamma(z + 1.0))),
    ..DEFAULTS
};

/// Complex gamma function via the Lanczos approximation (g = 7, 9
/// coefficients), with the reflection formula for the left half-plane.
/// Accurate to ~1e-13, so the recurrence `Γ(z+1) = z·Γ(z)` holds well within
/// the equality tolerance — which is what lets `(n+1)·n! = (n+1)!` and
/// `n/n! = 1/(n-1)!` pass.
fn gamma(z: Complex64) -> Complex64 {
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    let pi = std::f64::consts::PI;
    if z.re < 0.5 {
        // Reflection: Γ(z)·Γ(1-z) = π / sin(πz).
        Complex64::new(pi, 0.0)
            / ((Complex64::new(pi, 0.0) * z).sin() * gamma(Complex64::new(1.0, 0.0) - z))
    } else {
        let z = z - 1.0;
        let mut x = Complex64::new(C[0], 0.0);
        for (i, &c) in C.iter().enumerate().skip(1) {
            x += c / (z + i as f64);
        }
        let t = z + (G + 0.5);
        let sqrt_2pi = (2.0 * pi).sqrt();
        Complex64::new(sqrt_2pi, 0.0) * t.powc(z + 0.5) * (-t).exp() * x
    }
}

/// `nCr`/`nPr` on non-negative integer arguments.
fn combinatorial(n: Complex64, r: Complex64, ordered: bool) -> Option<Complex64> {
    let is_int = |z: Complex64| z.im.abs() < 1e-9 && (z.re.round() - z.re).abs() < 1e-9;
    if !is_int(n) || !is_int(r) {
        return None;
    }
    // Both stay f64. `as i64` saturates, so `n = 1e20` silently became
    // `i64::MAX` and the product loop returned a confidently wrong number
    // (`nCr(1e20,3)` came back ~1275× low). Only `r` bounds the loop, and it is
    // capped below, so `n` never needs to be an integer type at all — past 2^53
    // an f64 is not a faithful integer anyway, and `n - k` correctly evaluates
    // to `n` there.
    let (n, r) = (n.re.round(), r.re.round());
    // The r-length product loop must stay bounded on any input; past ~10^4
    // the f64 result is astronomically large/imprecise anyway.
    if n < 0.0 || r < 0.0 || r > n || r > 10_000.0 {
        return None;
    }
    let r = r as i64;
    // P(n,r) = n·(n-1)···(n-r+1); C(n,r) = P(n,r)/r!.
    let mut num = 1.0f64;
    for k in 0..r {
        num *= n - k as f64;
    }
    if ordered {
        return Some(Complex64::new(num, 0.0));
    }
    let mut den = 1.0f64;
    for k in 1..=r {
        den *= k as f64;
    }
    Some(Complex64::new(num / den, 0.0))
}

/// Exact `nPr`/`nCr` on non-negative integers. `None` for anything else —
/// including a huge `r`, where the product loop must stay bounded; the float
/// rule refuses at the same point.
fn combinatorial_exact(xs: &[BigRational], ordered: bool) -> Option<BigRational> {
    let [n, r] = xs else { return None };
    if !n.is_integer() || !r.is_integer() || n.is_negative() || r.is_negative() || r > n {
        return None;
    }
    let r = r.numer().to_u64()?;
    if r > 10_000 {
        return None;
    }
    let n = n.numer();
    // Bounding `r` alone leaves the *size* unbounded: the numerator runs to
    // about `r · bits(n)` bits and `n` may be as large as the parser will
    // build. Charge the result against the same budget `pow` charges its own
    // against — student input is adversarial by construction, and declining
    // leaves the application symbolic rather than wrong.
    if r.saturating_mul(n.bits()) > crate::resource_limits::current().max_pow_bits {
        return None;
    }
    // P(n,r) = n·(n−1)···(n−r+1);  C(n,r) = P(n,r)/r!.
    let num = balanced_product((0..r).map(|k| n - BigInt::from(k)));
    if ordered {
        return Some(BigRational::from(num));
    }
    // Exact division, not `BigRational::new`: `C(n,r)` is an integer whenever
    // `n ≥ r ≥ 0` are, so there is no fraction to reduce and reducing one would
    // mean a GCD over operands as large as the result.
    Some(BigRational::from(
        num / balanced_product((1..=r).map(BigInt::from)),
    ))
}

/// The product of `xs`, paired up by halves rather than accumulated
/// left-to-right.
///
/// The size bound above caps the *result*, but a running product multiplies an
/// ever-growing accumulator by one small factor at a time, which costs about
/// `r ·` (result size) — quadratic, and measurably so: `nCr(10^500, 500)` sat
/// inside the budget and still took ~4 s. Halving keeps both operands the same
/// size at every level, which is where num-bigint's subquadratic multiplication
/// actually engages. Same value, same accepted inputs, ~30 ms.
fn balanced_product(xs: impl IntoIterator<Item = BigInt>) -> BigInt {
    fn go(xs: &[BigInt]) -> BigInt {
        match xs.len() {
            0 => BigInt::one(),
            1 => xs[0].clone(),
            // Depth is log2(r) ≤ 14 given `r ≤ 10_000`, so this cannot run deep.
            n => go(&xs[..n / 2]) * go(&xs[n / 2..]),
        }
    }
    go(&xs.into_iter().collect::<Vec<_>>())
}
