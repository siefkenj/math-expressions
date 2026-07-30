//! Elementary f64 numerics — the mathjs scalar/statistics functions Doenet
//! consumes (`mod`/`gcd`/`lcm`, mean/median/variance/std/quantile).
//!
//! All are plain double-precision; failures are `NaN`, never panics (this is
//! wasm-boundary-facing).

/// mathjs `mod`: `x − y·floor(x/y)`, result has the sign of `y`; `y = 0`
/// returns `x` (mathjs convention).
pub fn math_mod(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return x;
    }
    x - y * (x / y).floor()
}

/// mathjs `gcd` for numbers: defined on integers only (NaN otherwise —
/// where mathjs throws, the wasm boundary reports NaN). `gcd(0,0) = 0`.
pub fn gcd_f64(x: f64, y: f64) -> f64 {
    if x.fract() != 0.0 || y.fract() != 0.0 || !x.is_finite() || !y.is_finite() {
        return f64::NAN;
    }
    let (mut a, mut b) = (x.abs(), y.abs());
    // Euclid on exactly-representable integers; f64 keeps exactness ≤ 2^53.
    while b > 0.0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

/// mathjs `lcm` on integers (NaN otherwise); `lcm(0, _) = 0`.
pub fn lcm_f64(x: f64, y: f64) -> f64 {
    let g = gcd_f64(x, y);
    if g.is_nan() {
        return f64::NAN;
    }
    if g == 0.0 {
        return 0.0;
    }
    (x / g * y).abs()
}

pub fn mean(data: &[f64]) -> f64 {
    if data.is_empty() {
        return f64::NAN;
    }
    data.iter().sum::<f64>() / data.len() as f64
}

pub fn median(data: &[f64]) -> f64 {
    quantile_seq(data, 0.5)
}

/// mathjs `variance` default: unbiased (divide by n − 1).
pub fn variance(data: &[f64]) -> f64 {
    if data.len() < 2 {
        return f64::NAN;
    }
    let m = mean(data);
    data.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (data.len() - 1) as f64
}

pub fn std_dev(data: &[f64]) -> f64 {
    variance(data).sqrt()
}

/// mathjs `quantileSeq` with default linear interpolation:
/// `h = (n−1)p`, result `= a⌊h⌋ + (h − ⌊h⌋)(a⌊h⌋₊₁ − a⌊h⌋)` on sorted data.
pub fn quantile_seq(data: &[f64], prob: f64) -> f64 {
    if data.is_empty() || !(0.0..=1.0).contains(&prob) {
        return f64::NAN;
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let h = (sorted.len() - 1) as f64 * prob;
    let lo = h.floor() as usize;
    let frac = h - h.floor();
    if lo + 1 >= sorted.len() || frac == 0.0 {
        return sorted[lo.min(sorted.len() - 1)];
    }
    sorted[lo] + frac * (sorted[lo + 1] - sorted[lo])
}
