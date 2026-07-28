//! Integer GCD helpers for the [`Number`](super::Number) tier.

/// Greatest common divisor of two i64s (by magnitude). `gcd(x, 0) == |x|`.
pub(crate) fn gcd_i64(a: i64, b: i64) -> i64 {
    let mut a = a.unsigned_abs();
    let mut b = b.unsigned_abs();
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.min(i64::MAX as u64) as i64
}
