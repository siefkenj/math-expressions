# Certified Numeric Eigenvalue Fast-Path

**Status:** ⬜ PROPOSED (feasibility assessment; not yet scheduled)

## Motivation

We currently have two eigenvalue paths that trade off precision against speed:

- `matrix::eigenvalues` — **symbolic / exact**. Closed forms where they exist,
  `RootOf` otherwise, certified to arbitrary precision. Correct but **500–5,600×
  slower** than f64 QR, dominated by global `RootOf` isolation (Sturm bisection
  over the whole real line to *prove* root separation) plus arbitrary-precision
  ordering.
- `mathjs_compat::eigs` — **f64 QR** (Householder → shifted QR). ~0.02 ms, ~13–15
  correct digits for well-conditioned eigenvalues, but **unverified**: no bound
  on the error, no guarantee the returned count/multiplicity is right.

The gap: a rigorous answer good to **~10 digits** without paying the full
symbolic cost. This is the classic verified-numerics ("trust but verify")
pattern — use the fast unverified QR to *locate*, then use exact arithmetic to
*validate* locally. Verifying is fundamentally far cheaper than deriving.

## Key insight

The expensive part of the symbolic path is **global** root isolation. QR hands
us the root locations for free, so certification replaces global isolation with
cheap **local** validation:

1. f64 QR → approximate eigenvalues λ̃ᵢ (0.02 ms).
2. Exact characteristic polynomial via `charpoly_rational` (already computed by
   the symbolic path; `matrix/eigen.rs`).
3. Per root: a **single Sturm count** on a tiny interval `[λ̃ᵢ−δ, λ̃ᵢ+δ]` to
   certify "exactly one root here" (a couple of exact polynomial evaluations —
   *not* recursive bisection), then `refine_real` to shrink to 10 digits.

"Only 10 digits" matters mostly because it gives comfortable margin over f64
(~15–16 digits), so the certificate usually *succeeds* cheaply and only fails on
exactly the ill-conditioned cases we'd want flagged. The dominant saving,
though, is skipping global isolation — not the reduced digit count.

## Reusable primitives (already in the crate)

- `matrix/eigen.rs::charpoly_rational` — exact characteristic polynomial.
- `polynomials/univariate.rs`:
  - `isolate_real_roots` (Sturm) — its per-interval Sturm-count core is the
    "exactly one root in (a,b]" certificate.
  - `refine_real` — shrink an isolating interval to target precision (this *is*
    the per-root validation, already certified).
  - `eval_rat`, `derivative`, `eval_c64`, `sturm_chain`.
- `eval_numeric/certified_digits/fix.rs::MpFix` (`excludes_zero()`),
  `complex.rs::CFix` — certified fixed-point real / complex arithmetic for
  interval-Newton and residual bounds.

## Difficulty breakdown (honest)

| Case | Difficulty | Notes |
| --- | --- | --- |
| Real, well-separated (all symmetric/Hermitian) | **Easy (~1–2 days)** | Sturm certification is exactly the tool. Mostly plumbing over existing functions. Bauer–Fike residual bound is an even cheaper O(n²) alternative for symmetric matrices. |
| Complex eigenvalues (general non-symmetric) | **Moderate (~1 week)** | Sturm is real-only. Need argument-principle / winding-number count around a complex box, or Rump-style verified eigenpairs (interval-Newton on `(A−λI)x=0`). `CFix` exists but this is new interval-linear-algebra code. |
| Clustered / near-defective / highly non-normal | **Intrinsically hard** | If two eigenvalues agree to ~10 digits, or κ(V) ≫ 10¹⁰, the eigenvalue is genuinely uncertain at that precision in f64. No cheap certificate exists — the honest behavior is to return "cannot certify to 10 digits" and escalate precision or fall back to the exact path. A real math limit, not an implementation gap. |
| Eigen**vectors** | Separate follow-on | Residual bound `‖A x̃ − λ x̃‖` in exact/interval arithmetic (Bauer–Fike / Rump). Feasible, more code, conditioning-sensitive for non-normal matrices. |

## Proposed phasing

- **C1 — real-eigenvalue certificate (easy, high value).** Symmetric/Hermitian
  and any real spectrum. QR locate → `charpoly_rational` → Sturm-count local
  validation → `refine_real` to 10 digits. Verdict includes an honest
  "uncertifiable at 10 digits" for clusters (Sturm count ≠ 1 in the trial
  interval). Covers a large share of real use (covariance, Laplacians, Gram).
- **C2 — general complex certificate (moderate).** Rump verified eigenpairs /
  interval-Newton on `(A−λI)x=0`, needing interval linear algebra on `MpFix`/
  `CFix`. Handles non-symmetric spectra with complex eigenvalues.
- **C3 — eigenvector certificates (follow-on).** Residual/Bauer–Fike bounds.

## API sketch (C1)

```rust
// Returns certified eigenvalues good to `digits`, or the indices that could
// not be certified at that precision (clusters / ill-conditioning).
fn certified_eigenvalues_real(
    a: &[BigRational], n: usize, digits: u32,
) -> Result<Vec<CertifiedReal>, Uncertifiable>;
```

Certification must **never lie**: it either returns an interval provably
containing exactly one eigenvalue to the requested precision, or reports that it
could not certify (never a silent wrong count).
