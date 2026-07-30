# Numeric integration — saturate f64 precision

> **STATUS: PLAN ONLY (not started).** Design for lifting the certified
> quadrature's precision ceiling so numeric definite integration reliably
> returns a result good to full f64 precision (~15–16 significant digits)
> instead of refusing near the current ~13-digit cap.

## Context

`math_expressions::eval_numeric::certified_digits::integrate_to_precision(f, x, a, b, digits)`
(`src/eval_numeric/certified_digits/quad.rs`) is a **certified** definite-integral routine: it returns
a value only with a _proven_ worst-case error bound at or below the requested
accuracy, or an honest `Precise::Unknown` — never a heuristic estimate. The wasm
binding `Expression::integrate_numerically(var, lower, upper)` (added
2026-07-28) wraps it (requesting 10 digits) to provide the JS
`integrateNumerically` shim in the js-compat drop-in and the playground.

**The ceiling.** The algorithm is adaptive **composite Simpson**:

- node values come from `eval_f64` — the Tier-0 certified **f64** evaluator
  (value + a proven ~f64-ε error bound per abscissa);
- the Simpson remainder `(w⁵/2880)·sup|f⁗|` is bounded rigorously via **f64
  interval arithmetic** (`Iv`) over the symbolically-differentiated 4th
  derivative;
- adaptive bisection splits the worst segment until the certified total
  (node error + remainder + summation rounding) meets the target, bounded by
  `max_quadrature_segments`; `package()` refuses if it doesn't.

The binding constraint is the **per-node f64 error (~2⁻⁵³) accumulated in f64
summation**: the smallest certifiable total is ~10⁻¹³ of the magnitude, so the
hard guard `if digits > 13 { Unknown("certified through f64 nodes (≤13 digits)") }`.
Worse, integrands with a root at an endpoint or heavy cancellation lose the top
digit even _at_ 13 — e.g. `∫₀^π sin` certifies `2.00000000000` at 12 digits but
**refuses at 13** (the true value is 2 to ~30 digits; the _certified_ bound is
just over the 13-digit threshold). There is no symbolic-antiderivative
shortcut — it goes straight to quadrature.

**Principle.** To certify N digits, internal working precision must exceed N.
f64 nodes ⇒ sub-f64 certification. Saturating f64 requires computing _above_
f64 internally.

## Goal

Numeric definite integration returns a value certified to full f64 precision
(~15–16 sig figs) for every well-behaved integrand, instead of refusing near
~13 digits. Preserve the "certified or honest refusal, never an estimate"
contract; keep worst-case cost bounded by the existing resource limits.

## Approach — three levers (do in order; each stands alone)

### Lever 1 — symbolic-antiderivative fast path (biggest win, cheapest) — PHASE 1

Before quadrature, try the crate's own symbolic `integrate(f, var)`
(`INTEGRATION_PLAN` I1–I2, already shipped). If it yields an elementary
antiderivative `F`, the definite integral is `F(b) − F(a)`, evaluated with the
**existing arbitrary-precision** `eval_tape` / `evaluate_to_precision` (Tier-2
`MpFix`, Ziv-escalated). That is _exact to any requested digits_ — it bypasses
the 13-cap entirely for the whole elementary class (polynomials, exp/log, trig,
rational, …). `∫₀^π sin` → `−cos(π)+cos(0) = 2`, exact.

- **Coverage:** essentially everything realistic (and everything the playground
  sees); only genuinely non-elementary integrands (`e^{−x²}`, `sin(x)/x`,
  elliptic, …) fall through to quadrature.
- **Reuses:** `integrate` + `evaluate_to_precision` — both proven.
- **Care:** `F` must be continuous on `[a,b]` — reuse the divergence classifier
  (`super::diverge::is_certified_divergent`) and reject/split when `F` has a
  branch cut or singularity inside the interval (else FTC is invalid). Handle
  the `a > b` sign and `a == b` cases as now.
- **Cost:** small; a fast path in `integrate_to_precision` (or a new
  `integrate_symbolic_first` that `integrate_numerically` and the wasm
  `integrate_to_precision` call).

### Lever 2 — extended-precision certified nodes (for the non-elementary tail) — PHASE 2

Swap the quadrature's `eval_f64` node calls for the higher-precision certified
evaluator (`eval_tape` at a fixed ~20–25-digit / double-double working
precision). Per-node certified error then sits far below 10⁻¹⁶, so certifying
full f64 becomes routine even for non-elementary integrands.

- **Requires:** an extended-precision _interval_ type for both node values and
  the f⁗ remainder bound — the current `Iv` is f64. Either a double-double
  interval or an `MpFix` interval. (Node error is the binding constraint, so
  extended-precision _nodes_ are the key; the remainder can also be driven small
  by more segments / higher order.)
- **Cost:** `MpFix`/dd evaluation is ~10–50× slower per node than f64, and
  Simpson uses many nodes — so pair with Lever 3.
- **Then, and only then:** raise the hard `digits > 13` cap toward 15–16. Do
  **not** bump the constant without the node precision to back it — that guard
  is what keeps the output honest.

### Lever 3 — fewer nodes: higher-order / doubly-exponential rule — PHASE 3 (optional)

Simpson is 4th-order (many nodes ⇒ more accumulation and more expensive
extended-precision evals). Options, in rough order of certification difficulty:

- **Compensated summation** (Kahan/Neumaier) or double-double accumulation —
  cheap, rule-agnostic; removes the summation-rounding term and pushes the
  ceiling from ~13 toward ~15 even with f64 nodes. Low-risk first step of Lever 3.
- **Clenshaw–Curtis** (Chebyshev nodes) — spectral for smooth `f`; rigorous
  error via Chebyshev-coefficient tail bounds.
- **Gauss–Kronrod** (G7–K15) — cheap embedded error estimate, but _rigorous_
  certification needs bounds on high (2n-th) derivatives, harder to get via
  interval extension than f⁗.
- **tanh–sinh (double-exponential)** — doubly-exponential convergence, tiny node
  counts, excellent for endpoint singularities; certification is subtler.

Simpson's virtue (why it's the current choice): a dead-simple, cheap, rigorous
f⁗ remainder. Any replacement must carry an equally rigorous certified bound.

## Recommended sequencing

1. **Phase 1 (Lever 1)** almost certainly delivers "saturate f64 for everything
   realistic": makes `∫₀^π sin` exact and lifts the cap for all elementary
   integrands, at low cost. Ship this first; re-point `integrate_numerically`
   to request full f64 (or return the exact FTC value) when the fast path fires.
2. **Phase 2 (Lever 2 + compensated summation)** for robust f64 saturation on
   _true_ non-elementary integrands; raise the 13 cap afterward.
3. **Phase 3 (Lever 3 rule swap)** only if Phase 2 quadrature proves too slow
   under `max_quadrature_segments` — it's the largest and most research-heavy.

## Files (anticipated)

- `src/eval_numeric/certified_digits/quad.rs` — Lever 1 fast path in `integrate_to_precision`;
  Lever 2 extended-precision node evals + interval type; Lever 3 rule.
- `src/eval_numeric/certified_digits/fix.rs` / a new `iv_dd.rs` — extended-precision interval type
  (Lever 2).
- `src/eval_numeric/certified_digits/pipeline.rs` — reuse `eval_tape` at a fixed working precision for
  nodes (Lever 2).
- `src/calculus/integrate/…` + `src/calculus/diff.rs` — reused read-only by
  Lever 1 (symbolic antiderivative), not modified.
- `packages/math-expressions-rs-wasm/src-rust/calculus.rs` — once Phase 1/2
  land, `integrate_numerically` can request full f64 (drop the 10-digit hedge);
  optionally expose the raised `integrate_to_precision` cap.
- `src/resource_limits.rs` — possibly a working-precision knob for quadrature
  nodes (Lever 2), analogous to `max_eval_precision_bits`.

## Caveats / non-goals

- **"Always" is bounded.** Even with all three levers, a pathological integrand
  (highly oscillatory, near-singular, non-elementary with slow node-error decay)
  can still exhaust `max_quadrature_segments` / `max_eval_precision_bits` and
  refuse — that is the certified-or-refuse contract, not a bug.
- **No uncertified estimates.** This plan never returns a best-effort number to
  raise the "always succeeds" rate; the whole point is certified saturation.
- Relates to: `INTEGRATION_PLAN.md` (symbolic `integrate`, reused by Lever 1),
  `DONE_ARBITRARY_PERCISION_PLAN.md` (`MpFix`/`eval_tape`, reused by Lever 2).

## Verification (when implemented)

- Regression: `∫₀^π sin`, `∫₀^π cos`, `∫ e^x`, `∫ x^k`, `∫ 1/(1+x²)` all certify
  to ≥15 digits; compare against exact closed forms.
- Non-elementary: `∫₀^1 e^{−x²}`, `∫₀^1 sin(x)/x` certify to ≥15 digits (Phase 2).
- Honest refusal preserved: a divergent (`∫₀^1 1/x`) and a deliberately
  over-budget oscillatory integrand still return `Unknown`.
- Determinism: identical certified digits across runs (operation-count budgets,
  no wall-clock).
