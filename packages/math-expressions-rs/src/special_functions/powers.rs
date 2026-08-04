//! Root/magnitude functions. `sqrt`/`cbrt`/`nthroot` normalize to explicit
//! powers during canonicalization; they exist here for parsing and for the
//! derivative table (which runs on the faithful layer). `cbrt`/`nthroot`
//! are text-only: LaTeX `\sqrt[n]{…}` is grammar, not an applied symbol.

use super::{FnDef, DEFAULTS};
use crate::eval_numeric::certified_digits::kernels::{FixId, FnKernel};
use crate::expr::Expr;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Signed;
use crate::normalize::{mul, pow};
use crate::num::Number;
use num_complex::Complex64;

pub const SQRT: FnDef = FnDef {
    name: "sqrt",
    parse_text: &["sqrt"],
    parse_latex: &["sqrt"],
    derivative: Some("1/(2*sqrt(x))"),
    antiderivative: Some(|u| {
        mul(vec![
            Expr::Num(Number::rat(2, 3)),
            pow(u, Expr::Num(Number::rat(3, 2))),
        ])
    }),
    eval1: Some(|z| Some(z.sqrt())),
    latex_commands: &[("sqrt", "sqrt")],
    kernel: Some(&SQRT_KERNEL),
    ..DEFAULTS
};

pub(crate) const SQRT_KERNEL: FnKernel = FnKernel {
    f: f64::sqrt,
    df: |x| 0.5 / x.sqrt(),
    domain: |x| x >= 0.0,
    fix: Some(FixId::Sqrt),
    cf: |z| z.sqrt(),
    cdfm: |z| 0.5 / z.sqrt().norm().max(f64::MIN_POSITIVE),
};

pub const CBRT: FnDef = FnDef {
    name: "cbrt",
    parse_text: &["cbrt"],
    derivative: Some("1/(3*cbrt(x)^2)"),
    eval1: Some(|z| Some(z.powf(1.0 / 3.0))),
    ..DEFAULTS
};

pub const NTHROOT: FnDef = FnDef {
    name: "nthroot",
    parse_text: &["nthroot"],
    eval2: Some(|a, b| Some(a.powc(b.inv()))),
    ..DEFAULTS
};

pub const ABS: FnDef = FnDef {
    name: "abs",
    parse_text: &["abs"],
    parse_latex: &["abs"],
    derivative: Some("abs(x)/x"),
    eval1: Some(|z| Some(Complex64::new(z.norm(), 0.0))),
    fold_exact: Some(|xs| match xs {
        [v] => Some(v.abs()),
        _ => None,
    }),
    latex_commands: &[("abs", "abs")],
    kernel: Some(&ABS_KERNEL),
    ..DEFAULTS
};

pub(crate) const ABS_KERNEL: FnKernel = FnKernel {
    f: f64::abs,
    df: |x| x.signum(),
    domain: |_| true,
    fix: Some(FixId::Abs),
    cf: |z| Complex64::new(z.norm(), 0.0),
    cdfm: |_| 1.0,
};

pub const SIGN: FnDef = FnDef {
    name: "sign",
    parse_text: &["sign"],
    parse_latex: &["sign"],
    eval1: Some(|z| {
        Some(if z.norm() == 0.0 {
            Complex64::ZERO
        } else {
            z / z.norm()
        })
    }),
    fold_exact: Some(|xs| match xs {
        [v] => Some(BigRational::from(BigInt::from(match v.numer().sign() {
            num_bigint::Sign::Minus => -1,
            num_bigint::Sign::NoSign => 0,
            num_bigint::Sign::Plus => 1,
        }))),
        _ => None,
    }),
    latex_commands: &[("sign", "sign")],
    ..DEFAULTS
};
