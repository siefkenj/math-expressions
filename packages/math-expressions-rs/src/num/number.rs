//! Tiered number type and its exact/float arithmetic.
//!
//! User-typed decimals parse to *exact* rationals (`Int`/`Rat`/`Big`), never
//! `Float` — see [`Number::from_decimal_str`](super::Number::from_decimal_str).
//! `Float` is reserved for numerical evaluation results. Decimal parsing and
//! rendering live in [`decimal`](super::decimal); GCD in [`gcd`](super::gcd).

use super::gcd::gcd_i64;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

/// f64 wrapper providing Eq + Hash by bit pattern (f64 itself implements
/// neither). Policy: NaN == NaN, +0.0 != -0.0. Numeric comparisons in
/// equality testing go through tolerances, not this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct F64(u64);

impl F64 {
    pub fn new(v: f64) -> Self {
        F64(v.to_bits())
    }
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// How a non-integer exact rational should be *written back out*.
///
/// The two are the same value and compare equal; this only decides spelling.
/// It has to be carried rather than derived because the value alone cannot
/// answer the question: decimals parse to exact rationals by design, so `0.5`
/// and `1/2` are both `Rat(1, 2)` and the distinction is gone by the time
/// anything reaches the serializer or a printer.
///
/// `Decimal` is contagious through arithmetic, the same way `Float` is: once a
/// decimal quantity is involved the result is a decimal quantity. That makes
/// `Fraction` the identity for [`join`](Spelling::join) and hence the right
/// default for integers, floats, and every exact value the engine *computes*.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Spelling {
    /// `n/d` — a fraction of integers, or anything derived from one.
    #[default]
    Fraction,
    /// A positional decimal, when the expansion terminates — a decimal literal
    /// the user typed, or a value rounded to a number of decimal places.
    Decimal,
}

impl Spelling {
    /// The spelling of a result computed from two operands: `Decimal` wins.
    pub fn join(self, other: Spelling) -> Spelling {
        if self == Spelling::Decimal || other == Spelling::Decimal {
            Spelling::Decimal
        } else {
            Spelling::Fraction
        }
    }
}

/// Note the hand-written `PartialEq`/`Hash` below: [`Spelling`] is *not* part
/// of a number's identity. `0.5 == 1/2` structurally, so canonical trees stay
/// comparable by `==` and hashable as keys, exactly as before this field
/// existed.
#[derive(Debug, Clone)]
pub enum Number {
    /// Integers that fit in i64. No allocation.
    Int(i64),
    /// Reduced fractions. Invariant: den > 0, gcd(|num|, den) == 1, den != 1.
    Rat(i64, i64, Spelling),
    /// Arbitrary precision fallback. Boxed to keep Number small.
    Big(Box<BigNumber>),
    /// Floating-point value — produced by numerical evaluation only. User
    /// input never parses to `Float` (decimals are exact rationals).
    Float(F64),
}

#[derive(Debug, Clone)]
pub enum BigNumber {
    Int(BigInt),
    Rat(BigRational, Spelling),
}

/// Value equality: the spelling is deliberately excluded (see [`Number`]).
impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Number::Int(a), Number::Int(b)) => a == b,
            (Number::Rat(a, b, _), Number::Rat(c, d, _)) => a == c && b == d,
            (Number::Float(a), Number::Float(b)) => a == b,
            (Number::Big(a), Number::Big(b)) => a == b,
            _ => false,
        }
    }
}
impl Eq for Number {}

impl std::hash::Hash for Number {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Number::Int(i) => i.hash(state),
            Number::Rat(n, d, _) => (n, d).hash(state),
            Number::Float(f) => f.hash(state),
            Number::Big(b) => b.hash(state),
        }
    }
}

impl PartialEq for BigNumber {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (BigNumber::Int(a), BigNumber::Int(b)) => a == b,
            (BigNumber::Rat(a, _), BigNumber::Rat(b, _)) => a == b,
            _ => false,
        }
    }
}
impl Eq for BigNumber {}

impl std::hash::Hash for BigNumber {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            BigNumber::Int(i) => i.hash(state),
            BigNumber::Rat(r, _) => r.hash(state),
        }
    }
}

impl Number {
    /// Number from an f64, demoting to Int when the value is integral —
    /// matches how JS number literals behave (JSON.stringify(3.0) == "3").
    /// The upper bound is exclusive: `i64::MAX as f64` rounds up to 2^63,
    /// which an `as` cast would silently saturate.
    pub fn from_f64(v: f64) -> Self {
        if v.fract() == 0.0 && v.is_finite() && v >= i64::MIN as f64 && v < i64::MAX as f64 {
            Number::Int(v as i64)
        } else {
            Number::Float(F64::new(v))
        }
    }

    /// Reduced rational from an i64 numerator/denominator, spelled as a
    /// fraction. Enforces the `Rat` invariants (den > 0, gcd == 1, den != 1)
    /// and demotes to `Int` when the denominator reduces to 1. Panics on a zero
    /// denominator.
    pub fn rat(num: i64, den: i64) -> Number {
        Number::rat_spelled(num, den, Spelling::Fraction)
    }

    /// [`rat`](Number::rat) with an explicit [`Spelling`].
    pub fn rat_spelled(mut num: i64, mut den: i64, spelling: Spelling) -> Number {
        assert!(den != 0, "rational with zero denominator");
        if den < 0 {
            num = -num;
            den = -den;
        }
        let g = gcd_i64(num, den);
        if g > 1 {
            num /= g;
            den /= g;
        }
        if den == 1 {
            Number::Int(num)
        } else {
            Number::Rat(num, den, spelling)
        }
    }

    /// How this value should be written back out. Integers and floats have only
    /// one spelling, and answer `Fraction` — the identity for
    /// [`Spelling::join`], so they never drag a sum or product either way.
    pub fn spelling(&self) -> Spelling {
        match self {
            Number::Rat(_, _, s) => *s,
            Number::Big(b) => match &**b {
                BigNumber::Rat(_, s) => *s,
                BigNumber::Int(_) => Spelling::Fraction,
            },
            Number::Int(_) | Number::Float(_) => Spelling::Fraction,
        }
    }

    /// This value with its spelling replaced. A no-op on integers and floats.
    pub fn with_spelling(&self, spelling: Spelling) -> Number {
        match self {
            Number::Rat(n, d, _) => Number::Rat(*n, *d, spelling),
            Number::Big(b) => match &**b {
                BigNumber::Rat(r, _) => Number::Big(Box::new(BigNumber::Rat(r.clone(), spelling))),
                BigNumber::Int(_) => self.clone(),
            },
            _ => self.clone(),
        }
    }

    /// Reduce and demote an arbitrary-precision integer to the smallest tier.
    pub fn from_bigint(v: BigInt) -> Number {
        match v.to_i64() {
            Some(i) => Number::Int(i),
            None => Number::Big(Box::new(BigNumber::Int(v))),
        }
    }

    /// Reduce and demote an arbitrary-precision rational, spelled as a
    /// fraction: to `Int` when integral and small, to `Rat` when numerator and
    /// denominator both fit i64, otherwise `Big`. `BigRational` keeps itself in
    /// lowest terms.
    pub fn from_bigrational(v: BigRational) -> Number {
        Number::from_bigrational_spelled(v, Spelling::Fraction)
    }

    /// [`from_bigrational`](Number::from_bigrational) with an explicit
    /// [`Spelling`].
    pub fn from_bigrational_spelled(v: BigRational, spelling: Spelling) -> Number {
        if v.is_integer() {
            return Number::from_bigint(v.to_integer());
        }
        if let (Some(n), Some(d)) = (v.numer().to_i64(), v.denom().to_i64()) {
            // Already reduced and non-integral, so den != 1 and den > 0.
            Number::Rat(n, d, spelling)
        } else {
            Number::Big(Box::new(BigNumber::Rat(v, spelling)))
        }
    }

    /// Round to `d` decimal places, ties away from zero. Negative `d` rounds to
    /// tens / hundreds / … Exact for rational values (so `2.345` → `2.35`, no
    /// float ambiguity); f64-based for `Float`.
    ///
    /// Extreme `d` is resolved *semantically* rather than computed: the scale
    /// 10^|d| is materialized as a `BigInt`, so a hostile `d` (e.g. from the
    /// wasm boundary) must neither allocate gigabytes nor grind debug-mode
    /// bignum arithmetic. Beyond ±4000 decimal places: a very positive `d`
    /// returns the value unchanged (no classroom value has finer structure);
    /// a very negative `d` compares the rounding unit to the value's magnitude
    /// (smaller → 0, larger → unchanged, with tie behaviour at those
    /// astronomical scales deliberately approximate).
    pub fn round_to_decimals(&self, d: i32) -> Number {
        let max_scale = crate::resource_limits::current().max_round_decimals;
        let d64 = i64::from(d);
        if d64 > max_scale {
            return self.clone();
        }
        if d64 < -max_scale {
            return match self.magnitude_log10() {
                // |value| far below the rounding unit → rounds to zero.
                Some(k) if k < -d64 => Number::Int(0),
                // Coarse rounding of an even more astronomical value: leading
                // digits dominate; unchanged is the bounded approximation.
                Some(_) => self.clone(),
                None => self.clone(), // zero / NaN
            };
        }
        // Fast path: an integer value is unchanged by rounding to ≥ 0 decimals.
        let is_int_value = matches!(self, Number::Int(_))
            || matches!(self, Number::Big(b) if matches!(&**b, BigNumber::Int(_)));
        if d >= 0 && is_int_value {
            return self.clone();
        }
        match self.to_bigrational() {
            Some(r) => {
                let pow10 = BigInt::from(10).pow(d.unsigned_abs());
                let scale = if d >= 0 {
                    BigRational::from_integer(pow10)
                } else {
                    BigRational::new(BigInt::one(), pow10)
                };
                let rounded = (&r * &scale).round(); // half away from zero
                // Rounding *to decimal places* produces a decimal, whatever
                // went in: `round_numbers_to_decimals(1/3, 2)` is `0.33`, not
                // `33/100`.
                Number::from_bigrational_spelled(rounded / scale, Spelling::Decimal)
            }
            None => {
                let f = 10f64.powi(d);
                Number::from_f64((self.to_f64() * f).round() / f)
            }
        }
    }

    /// `⌊log10 |self|⌋` — the decimal place of the leading significant digit —
    /// or `None` for zero/NaN. Uses f64 when the magnitude is in f64 range;
    /// for `Big` values beyond it (where `to_f64()` is ±∞ or underflows to 0),
    /// falls back to bit lengths (accuracy ±1, which only shifts a
    /// significant-figures boundary by one digit at ≳10³⁰⁸ magnitudes — the
    /// point is a sane finite result, not an unbounded/overflowing one).
    pub fn magnitude_log10(&self) -> Option<i64> {
        if self.is_zero() {
            return None;
        }
        let f = self.to_f64().abs();
        if f.is_finite() && f > 0.0 {
            return Some(f.log10().floor() as i64);
        }
        if f.is_nan() {
            return None;
        }
        // Exact value outside f64 range: approximate from bit lengths.
        let (num_bits, den_bits) = match self {
            Number::Big(b) => match &**b {
                BigNumber::Int(i) => (i.bits() as i64, 0i64),
                BigNumber::Rat(r, _) => (r.numer().bits() as i64, r.denom().bits() as i64),
            },
            // Small variants always fit f64; unreachable in practice.
            _ => return None,
        };
        Some(((num_bits - den_bits) as f64 * std::f64::consts::LOG10_2).floor() as i64)
    }

    pub fn to_f64(&self) -> f64 {
        match self {
            Number::Int(i) => *i as f64,
            Number::Rat(n, d, _) => *n as f64 / *d as f64,
            Number::Float(f) => f.get(),
            Number::Big(b) => match &**b {
                BigNumber::Int(i) => i.to_f64().unwrap_or(f64::NAN),
                BigNumber::Rat(r, _) => r.to_f64().unwrap_or(f64::NAN),
            },
        }
    }

    pub fn is_positive(&self) -> bool {
        match self {
            Number::Int(i) => *i > 0,
            Number::Rat(n, ..) => *n > 0,
            Number::Float(f) => f.get() > 0.0,
            Number::Big(b) => match &**b {
                BigNumber::Int(i) => i.is_positive(),
                BigNumber::Rat(r, _) => r.is_positive(),
            },
        }
    }

    pub fn is_negative(&self) -> bool {
        match self {
            Number::Int(i) => *i < 0,
            Number::Rat(n, ..) => *n < 0,
            Number::Float(f) => f.get() < 0.0,
            Number::Big(b) => match &**b {
                BigNumber::Int(i) => i.is_negative(),
                BigNumber::Rat(r, _) => r.is_negative(),
            },
        }
    }

    /// Numerator and denominator as strings, for a non-integral rational
    /// (`Rat` or big rational); `None` for integers, floats, and big
    /// integers. Used by formatters for the `a/b` / `\frac` fallback when the
    /// fraction does not terminate as a decimal.
    pub fn rational_parts(&self) -> Option<(String, String)> {
        match self {
            Number::Rat(n, d, _) => Some((n.to_string(), d.to_string())),
            Number::Big(b) => match &**b {
                BigNumber::Rat(r, _) => Some((r.numer().to_string(), r.denom().to_string())),
                BigNumber::Int(_) => None,
            },
            _ => None,
        }
    }

    pub fn neg(&self) -> Number {
        match self {
            Number::Int(i) => Number::Int(-i),
            Number::Rat(n, d, s) => Number::Rat(-n, *d, *s),
            Number::Float(f) => Number::Float(F64::new(-f.get())),
            Number::Big(b) => match &**b {
                BigNumber::Int(i) => Number::from_bigint(-i),
                BigNumber::Rat(r, s) => Number::from_bigrational_spelled(-r, *s),
            },
        }
    }

    pub const fn zero() -> Number {
        Number::Int(0)
    }
    pub const fn one() -> Number {
        Number::Int(1)
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Number::Int(i) => *i == 0,
            Number::Rat(n, ..) => *n == 0,
            Number::Float(f) => f.get() == 0.0,
            Number::Big(b) => match &**b {
                BigNumber::Int(i) => i.is_zero(),
                BigNumber::Rat(r, _) => r.is_zero(),
            },
        }
    }

    pub fn is_one(&self) -> bool {
        matches!(self, Number::Int(1))
    }

    pub fn abs(&self) -> Number {
        if self.is_negative() {
            self.neg()
        } else {
            self.clone()
        }
    }

    /// A finite exact rational as a `BigRational`; `None` for `Float`. The
    /// common currency for exact arithmetic across the tiers.
    pub(crate) fn to_bigrational(&self) -> Option<BigRational> {
        match self {
            Number::Int(i) => Some(BigRational::from_integer(BigInt::from(*i))),
            Number::Rat(n, d, _) => Some(BigRational::new(BigInt::from(*n), BigInt::from(*d))),
            Number::Big(b) => Some(match &**b {
                BigNumber::Int(i) => BigRational::from_integer(i.clone()),
                BigNumber::Rat(r, _) => r.clone(),
            }),
            Number::Float(_) => None,
        }
    }

    fn is_float(&self) -> bool {
        matches!(self, Number::Float(_))
    }

    /// Exact binary op on two exact operands, or f64 arithmetic if either is a
    /// `Float` (float-ness is contagious — a `Float` operand marks an inexact
    /// evaluation result). The `Int op Int` fast path stays allocation-free.
    fn binop(
        &self,
        other: &Number,
        int_checked: impl Fn(i64, i64) -> Option<i64>,
        exact: impl Fn(BigRational, BigRational) -> BigRational,
        float: impl Fn(f64, f64) -> f64,
    ) -> Number {
        if let (Number::Int(a), Number::Int(b)) = (self, other) {
            if let Some(v) = int_checked(*a, *b) {
                return Number::Int(v);
            }
        }
        if self.is_float() || other.is_float() {
            return Number::Float(F64::new(float(self.to_f64(), other.to_f64())));
        }
        Number::from_bigrational_spelled(
            exact(
                self.to_bigrational().unwrap(),
                other.to_bigrational().unwrap(),
            ),
            // Decimal is contagious, so `0.5 + 1/4` reads back as `0.75` while
            // `1/2 + 1/4` reads back as `3/4`. See `Spelling`.
            self.spelling().join(other.spelling()),
        )
    }

    pub fn add(&self, other: &Number) -> Number {
        self.binop(other, i64::checked_add, |a, b| a + b, |a, b| a + b)
    }
    pub fn sub(&self, other: &Number) -> Number {
        self.binop(other, i64::checked_sub, |a, b| a - b, |a, b| a - b)
    }
    pub fn mul(&self, other: &Number) -> Number {
        self.binop(other, i64::checked_mul, |a, b| a * b, |a, b| a * b)
    }

    /// Division, or `None` when dividing by (exact) zero — the caller leaves
    /// the expression unfolded rather than fabricating an infinity. Float ÷ 0.0
    /// follows IEEE (±∞/NaN), matching JS.
    pub fn checked_div(&self, other: &Number) -> Option<Number> {
        if other.is_zero() && !self.is_float() && !other.is_float() {
            return None;
        }
        Some(self.binop(
            other,
            |_, _| None, // never take the i64 path: division is not closed on i64
            |a, b| a / b,
            |a, b| a / b,
        ))
    }

    /// Raise to an integer power. `None` for `0` to a negative power (an
    /// exact division by zero), and for exponents so large the exact result
    /// would be astronomically big — the caller leaves the node unfolded
    /// either way. `0^0 == 1`, matching JS `Math.pow`.
    pub fn checked_pow_int(&self, exp: i64) -> Option<Number> {
        if let Number::Float(f) = self {
            // powi takes i32; saturate rather than wrap for absurd exponents.
            let e = exp.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
            return Some(Number::Float(F64::new(f.get().powi(e))));
        }
        if exp == 0 {
            return Some(Number::one());
        }
        if self.is_zero() {
            return if exp < 0 { None } else { Some(Number::zero()) };
        }
        let base = self.to_bigrational().unwrap();
        // Refuse exact results beyond ~10^6 bits (canonicalization must stay
        // cheap on any input; `2^(10^12)` is not a number to materialize).
        // |±1| is exempt: its powers stay one digit.
        let base_bits = base.numer().bits().max(base.denom().bits());
        if base_bits > 1
            && exp.unsigned_abs().saturating_mul(base_bits) > crate::resource_limits::current().max_pow_bits
        {
            return None;
        }
        let mag = bigrat_powu(base, exp.unsigned_abs());
        let result = if exp < 0 { mag.recip() } else { mag };
        // A power of a decimal is a decimal (`0.5^2` is `0.25`); a power of an
        // integer or a fraction is a fraction (`2^(-2)` is `1/4`). That second
        // case is the one origin of a fraction spelling that is not a literal
        // `a/b`: canonicalization turns every division into a negative power.
        Some(Number::from_bigrational_spelled(result, self.spelling()))
    }

}

/// `base^n` by exponentiation-by-squaring (n unsigned; caller handles sign).
fn bigrat_powu(base: BigRational, mut n: u64) -> BigRational {
    let mut result = BigRational::one();
    let mut b = base;
    while n > 0 {
        if n & 1 == 1 {
            result *= &b;
        }
        n >>= 1;
        if n > 0 {
            b = &b * &b;
        }
    }
    result
}
