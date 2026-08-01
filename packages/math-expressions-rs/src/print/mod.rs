//! Output formatters: `Expr` → text / LaTeX.
//!
//! These are clean precedence-based pretty-printers that walk `Expr`
//! directly, rather than transcriptions of the JS formatters (which decide
//! parenthesisation by regex-matching their own output over the ad-hoc JS
//! tree shape). Correctness is enforced by round-tripping through the parsers
//! (`tests/roundtrip.rs`), not by matching JS output byte-for-byte.

pub mod latex;
pub mod text;

use crate::expr::{Expr, SeqKind};
use crate::num::Number;

pub use latex::LatexOpts;
pub use text::TextOpts;

/// Precedence ladder (tighter binds higher), aligned with the parser grammars,
/// shared by both formatters so output round-trips with minimal parentheses.
pub(crate) mod prec {
    /// Sign-string symbols — parenthesise everywhere but the top level.
    pub const SIGN: u8 = 1;
    pub const LIST: u8 = 10;
    pub const COLONBAR: u8 = 15;
    pub const ARROW: u8 = 20;
    pub const OR: u8 = 30;
    pub const AND: u8 = 40;
    pub const NOT: u8 = 45;
    pub const REL: u8 = 50;
    pub const ADD: u8 = 60;
    pub const NEG: u8 = 65;
    /// A unit-bearing quantity (`x %`, `$ x`, `x°`) binds looser than
    /// multiplication, so it parenthesizes as a factor in a product
    /// (`\left(x \%\right) y`) but not standalone or in a sum.
    pub const UNIT: u8 = 66;
    pub const MUL: u8 = 70;
    pub const POW: u8 = 90;
    pub const INDEX: u8 = 95;
    pub const ATOM: u8 = 100;
}

/// Split a leading sign out of a sum term, structurally (never from a Mul,
/// which would not round-trip). Borrows where possible; only a negative
/// number needs an owned negation.
pub(crate) fn split_sign(e: &Expr) -> (bool, std::borrow::Cow<'_, Expr>) {
    use std::borrow::Cow;
    match e {
        Expr::Neg(x) => (true, Cow::Borrowed(&**x)),
        Expr::Num(n) if number_is_negative(n) => (true, Cow::Owned(Expr::Num(n.neg()))),
        _ => (false, Cow::Borrowed(e)),
    }
}

pub(crate) fn number_is_negative(n: &Number) -> bool {
    n.is_negative()
}

/// A Leibniz-notation variable entry is either `x` or `(x, n)`. A malformed
/// entry renders through the text printer — never `Debug`, whose Rust syntax
/// (`Num(Int(2))`) must not leak into user-facing output.
pub(crate) fn deriv_var(e: &Expr) -> (String, i64) {
    let render = |e: &Expr| text::convert(e, &Default::default());
    match e {
        Expr::Seq(SeqKind::Tuple, parts) if parts.len() == 2 => {
            let v = match &parts[0] {
                Expr::Sym(s) => s.name(),
                other => render(other),
            };
            let n = match &parts[1] {
                Expr::Num(Number::Int(i)) => *i,
                _ => 1,
            };
            (v, n)
        }
        Expr::Sym(s) => (s.name(), 1),
        other => (render(other), 1),
    }
}

pub(crate) fn pow_suffix(n: i64) -> String {
    if n > 1 {
        format!("^{}", n)
    } else {
        String::new()
    }
}

/// Render an expression as plain text (educational-math notation). Flattens
/// first, since parsing is now faithful (keeps raw grouping) but the formatters
/// assume flat n-ary operators; idempotent on already-canonical trees.
pub fn to_text(expr: &Expr, opts: &TextOpts) -> String {
    text::convert(&crate::expr::flatten(expr.clone()), opts)
}

/// Render an expression as LaTeX. Flattens first (see [`to_text`]).
pub fn to_latex(expr: &Expr, opts: &LatexOpts) -> String {
    latex::convert(&crate::expr::flatten(expr.clone()), opts)
}

/// Greek-letter (and a few symbol) name → unicode, shared by the formatters.
/// Mirrors JS `ast-to-text.js` `symbolConversions` exactly, including its
/// omissions: `chi` is deliberately absent (JS has no `chi` entry either), so
/// the `chi` symbol renders as ASCII `"chi"` in both engines. Do not add it —
/// that would emit `χ` where JS emits `chi`, a text-output parity break.
pub(crate) fn greek_unicode(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "Gamma" => "Γ",
        "gamma" => "γ",
        "Delta" => "Δ",
        "delta" => "δ",
        "epsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "Theta" => "ϴ",
        "theta" => "θ",
        "iota" => "ι",
        "kappa" => "κ",
        "Lambda" => "Λ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "Xi" => "Ξ",
        "xi" => "ξ",
        "Pi" => "Π",
        "pi" => "π",
        "rho" => "ρ",
        "Sigma" => "Σ",
        "sigma" => "σ",
        "tau" => "τ",
        "Upsilon" => "Υ",
        "upsilon" => "υ",
        "Phi" => "Φ",
        "phi" => "ϕ",
        "Psi" => "Ψ",
        "psi" => "ψ",
        "Omega" => "Ω",
        "omega" => "ω",
        "emptyset" => "∅",
        // Named glyphs the lexer accepts as single VARMULTICHAR tokens; their
        // ASCII names would re-split, so they must render as the glyph.
        "spade" => "♠",
        "heart" => "♡",
        "diamond" => "♢",
        "club" => "♣",
        "bigstar" => "★",
        "bigcirc" => "◯",
        "lozenge" => "◊",
        "bigtriangleup" => "△",
        "bigtriangledown" => "▽",
        "blacklozenge" => "⧫",
        "blacksquare" => "■",
        "blacktriangle" => "▲",
        "blacktriangledown" => "▼",
        "blacktriangleleft" => "◀",
        "blacktriangleright" => "▶",
        "Box" => "□",
        "circ" => "∘",
        "star" => "⋆",
        "perp" => "⟂",
        _ => return None,
    })
}

/// Render a float in positional decimal notation, never exponential, using
/// the shortest digit string that round-trips. Exponential forms cannot be
/// re-parsed reliably: the parsers' scientific literals are context-sensitive
/// (the exponent is spelled `E` and folds only before a delimiter) and a
/// lowercase `e` means Euler's number. Positional form parses unambiguously
/// anywhere, at worst verbosely (3e-12 → "0.000000000003").
pub(crate) fn f64_positional_string(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v == 0.0 {
        return "0".to_string();
    }
    if v < 0.0 {
        return format!("-{}", f64_positional_string(-v));
    }
    if v.is_infinite() {
        return "Infinity".to_string();
    }
    let (s, n) = crate::num::shortest_digits(v);
    let k = s.len() as i64;
    if k <= n {
        format!("{}{}", s, "0".repeat((n - k) as usize))
    } else if n > 0 {
        format!("{}.{}", &s[..n as usize], &s[n as usize..])
    } else {
        format!("0.{}{}", "0".repeat((-n) as usize), s)
    }
}

/// Append trailing zeros to a rendered positional number so it shows at least
/// `pad_to_digits` significant characters and/or `pad_to_decimals` fractional
/// places — port of the legacy `padNumberStringToDigitsAndDecimals`
/// (`converters/pad-numbers.js`). Pads only: never rounds, shortens, or moves
/// the point. A `None` or `0` bound is inactive. `s` is a positional magnitude,
/// possibly signed (`-1.5`), never exponential — matching the strings the
/// number printers produce.
///
/// Both bounds are clamped to [`MAX_PAD`]: padding is pure `"0".repeat(n)`, so
/// an unclamped bound turns a render option into an out-of-memory abort, which
/// on wasm (`panic = "abort"`) takes the whole worker with it. Legacy raised a
/// `RangeError` at the same wall; a clamp keeps the render alive instead.
pub(crate) fn pad_number(
    s: &str,
    pad_to_digits: Option<u32>,
    pad_to_decimals: Option<u32>,
) -> String {
    let bound = |d: Option<u32>| d.filter(|&d| d > 0).map(|d| (d as usize).min(MAX_PAD));
    let (digits, decimals) = (bound(pad_to_digits), bound(pad_to_decimals));
    match (digits, decimals) {
        (None, None) => s.to_string(),
        (None, Some(dec)) => pad_to_decimals_str(s, dec),
        (Some(dig), None) => pad_to_digits_str(s, dig),
        (Some(dig), Some(dec)) => pad_to_digits_and_decimals(s, dig, dec),
    }
}

/// The largest padding [`pad_number`] will honor. Well past any real display
/// use (a rendered number nobody reads is still a number nobody reads), and
/// small enough that the worst case is a few KB rather than an allocation
/// failure.
const MAX_PAD: usize = 1024;

/// Chars in the leading `0.0*` run (the JS `/^0\.0*/` match). Only called when
/// `s` begins `0.`.
fn leading_zero_run(s: &str) -> usize {
    2 + s[2..].chars().take_while(|&c| c == '0').count()
}

/// Fractional-digit count — chars after the `.` (0 if none).
fn decimal_count(s: &str) -> usize {
    s.split_once('.').map_or(0, |(_, frac)| frac.len())
}

fn pad_to_digits_str(s: &str, n_digits: usize) -> String {
    let mut s = s.to_string();
    let mut n_chars = n_digits;
    if s.contains('.') {
        // A non-leading-zero head (including a `-` sign) costs one char for the
        // point; a `0.00…` head costs the whole run — mirrors the JS branch.
        n_chars += if s.starts_with('0') {
            leading_zero_run(&s)
        } else {
            1
        };
        if s.len() < n_chars {
            s.push_str(&"0".repeat(n_chars - s.len()));
        }
    } else if s.len() < n_chars {
        let n_pad = n_chars - s.len();
        s.push('.');
        s.push_str(&"0".repeat(n_pad));
    }
    s
}

fn pad_to_decimals_str(s: &str, n_decimals: usize) -> String {
    let mut s = s.to_string();
    if s.contains('.') {
        let current = decimal_count(&s);
        if current < n_decimals {
            s.push_str(&"0".repeat(n_decimals - current));
        }
    } else {
        s.push('.');
        s.push_str(&"0".repeat(n_decimals));
    }
    s
}

fn pad_to_digits_and_decimals(s: &str, n_digits: usize, n_decimals: usize) -> String {
    let mut s = s.to_string();
    if s.contains('.') {
        let mut n_chars = n_digits;
        n_chars += if s.starts_with('0') {
            leading_zero_run(&s)
        } else {
            1
        };
        let mut n_pad = n_chars.saturating_sub(s.len());
        let current = decimal_count(&s);
        if current < n_decimals {
            n_pad = n_pad.max(n_decimals - current);
        }
        if n_pad > 0 {
            s.push_str(&"0".repeat(n_pad));
        }
    } else {
        let n_pad = n_digits.saturating_sub(s.len()).max(n_decimals);
        s.push('.');
        s.push_str(&"0".repeat(n_pad));
    }
    s
}

#[cfg(test)]
mod pad_tests {
    use super::pad_number;
    fn d(s: &str, dec: u32) -> String {
        pad_number(s, None, Some(dec))
    }
    fn g(s: &str, dig: u32) -> String {
        pad_number(s, Some(dig), None)
    }

    #[test]
    fn pads_decimals() {
        assert_eq!(d("1.5", 4), "1.5000");
        assert_eq!(d("2", 3), "2.000");
        assert_eq!(d("1.5", 1), "1.5"); // already enough
        assert_eq!(d("-0.75", 4), "-0.7500");
    }

    #[test]
    fn pads_digits() {
        assert_eq!(g("5", 4), "5.000");
        assert_eq!(g("1.5", 4), "1.500"); // 1,5,0,0 significant chars + point
        assert_eq!(g("0.005", 2), "0.0050"); // leading-zero run counted
    }

    #[test]
    fn pads_to_the_larger_of_both() {
        assert_eq!(pad_number("1.5", Some(6), Some(2)), "1.50000"); // 6 sig chars win
        assert_eq!(pad_number("1.5", Some(2), Some(4)), "1.5000"); // decimals win
        assert_eq!(pad_number("3", Some(4), Some(2)), "3.000"); // digits win, integer
    }

    #[test]
    fn zero_and_none_bounds_are_inactive() {
        assert_eq!(pad_number("1.5", None, None), "1.5");
        assert_eq!(pad_number("1.5", Some(0), Some(0)), "1.5");
    }

    /// An absurd bound clamps instead of trying to allocate 4 GB of zeros.
    #[test]
    fn an_absurd_bound_clamps_instead_of_exhausting_memory() {
        // `1.` plus MAX_PAD fractional zeros.
        assert_eq!(d("1.5", u32::MAX).len(), 2 + super::MAX_PAD);
        // MAX_PAD significant characters plus the decimal point.
        assert_eq!(g("5", u32::MAX).len(), super::MAX_PAD + 1);
    }
}
