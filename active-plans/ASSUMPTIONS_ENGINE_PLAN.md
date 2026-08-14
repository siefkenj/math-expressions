# Assumptions engine — completion plan

Goal: close the remaining feature gaps in the assumptions engine so
`packages/math-expressions-js-compat/spec/slow_assumptions.spec.ts` passes
*except* for the one test that cannot be passed soundly — see
"Accepted divergence" below. Zero is **not** the target and never was
reachable: legacy's expected answers there are partly false.

## Status

| stage | failing | note |
|-------|--------:|------|
| start of session | 565 / 845 | |
| after Phase 1 (done) | **234 / 845** | default-assumptions binding fix |
| now (2026-08-13) | **1 / 845** | 843 pass, 1 skipped (`define constants`) |
| target | 1 | `logical combinations`, accepted — see below |

## Accepted divergence — `logical combinations`

The one remaining failing test hides **six** failing assertions (vitest aborts
an `it` at its first failure; re-measure by converting that `it`'s `expect` to
`expect.soft`, then revert the scaffolding): spec lines 7357, 7415, 7417, 7418,
7419, 7420. On all six, legacy commits to an answer and this engine declines.
The engine is **incomplete here, never unsound**. Two root causes:

1. **Contradictory premises** (spec:7357). `x ∈ R and x ∉ R ⟹ is_real(x)`:
   legacy's `and` is `left || right`, so the first conjunct wins and it answers
   `true`. `Facts::and_meet` meets the two conflicting definite answers to
   `None`. Deliberate; see the doc comment on `and_meet` in
   `src/assumptions/facts.rs`.
2. **Non-realness does not propagate through an operator** (spec:7415,
   7417–7420). Under `x ∈ C, x ∉ R, y ∈ R`, legacy answers `false` for
   `is_real/nonpositive/nonnegative/positive/negative(x·y)`. Three of those
   five are **mathematically false**: `y = 0` is a model of the premises, and
   there `x·y = 0`, which *is* real, nonpositive and nonnegative. The other two
   (`positive`, `negative`) are sound, and we still answer `undefined` because
   `combine::mul` in `src/assumptions/infer/combine/mod.rs` never carries a
   `real: Some(false)` operand through the product.

### Known gap, deliberately not closed

Adding non-realness propagation rules to `combine::add` and `combine::mul` (and
their `combine::pow` sibling module) would close the sound half of cause 2 (two assertions), and
we are **not** doing it. Those rules turn facts that are `None` today into
`Some(false)`, and `simplify`'s rewrites are gated on exactly those facts — so
more definite answers means different rewrites, which on DoenetML's answer path
means different **grading**. Two assertions in a test that stays red either way
do not buy that risk. Anyone revisiting this must diff the whole compat suite
by (file, name, occurrence) and the `simplify` corpora before believing it is
inert.

## Phase 1 — default assumptions source (DONE, −331)

`lib/assumptions/element_of_sets.ts` built its predicates over a module-level
`EMPTY = new wasm.Assumptions()`, and `handleFor(undefined)` returned it. The spec
calls `is_real(me.fromText("x+y"))` with **no** second argument, expecting the
global store populated by `me.add_assumption(...)`. Every no-argument query
therefore answered "unknown".

Fix: `handleFor(undefined)` now returns `Context.assumptions` (the live handle).
The `EMPTY` fallback was additionally made lazy — as written it forced the wasm
load during module evaluation, the same hazard documented at
`lib/math-expressions.ts:1251`.

This proved the Rust reasoner was already correct for the bulk of these cases
(`is_real(x+y)` with `x,y ∈ R` returns `true` when handed the right store); the
failures were a binding defect, **not** a reasoning-depth gap.

## The 234 post-Phase-1 failures, in six groups (historical)

These groups are the breakdown of the **234** figure in the status table, not of
what is failing today — the suite is at 1 failing (see "Accepted divergence"
above). Kept as the record of what the work was.

### Group A — negated assumptions (16) · Rust
`variable_facts` in `assumptions/infer.rs` only reads `Expr::Relation`; it ignores
`Expr::Not`. Needed: negation-normalize each stored fact before interpreting —
`not(x>0)` ⇒ `x≤0`, `not(x≥0)` ⇒ `x<0`, `not(x≠0)` ⇒ `x=0`, `not(x=0)` ⇒ `x≠0`,
with double-negation elimination (`not(not(p))` ⇒ `p`).

### Group B — arithmetic reasoning gaps (~96) · Rust
`sum 8 · subtraction 10 · product 22 · quotient 14 · power 42`. Confirmed rules:
- **Zero factor**: any factor known `= 0` makes the product exactly zero
  (`nonneg`/`nonpos` true, `positive`/`negative`/`nonzero` false). `combine_mul`
  currently demands `real` before any sign reasoning and has no zero short-circuit.
- **Complex closure**: `x,y ∈ C` ⇒ `x+y`, `x·y`, `x^y` complex. `combine_pow`
  never sets `complex` from a complex base/exponent.
- **Sum with a known-zero term**: `x ∈ C, x≠0, y=0` ⇒ `x+y` nonzero and complex.
- **Powers**: `x ∈ R, y>0` ⇒ `x^y` complex; `x ∈ R, x≠0` ⇒ `x^y` nonzero.
  Power is the largest single bucket — expect several sub-rules.

### Group C — function domain facts (6) · Rust
`x ∈ C` ⇒ `sin(x)`, `sqrt(x)`, `exp(x)`, `abs(x)` are complex. `apply_facts`
currently derives facts only from a **real** argument, so a complex argument
yields nothing.

### Group D — literal & operator evaluation (8) · Rust
- `sin(0)` ⇒ integer/real (needs constant folding of the argument).
- `sqrt(-4)` ⇒ complex true, real false.
- `-2.2/(5-5)`, `(-6+6)/(5-5)` ⇒ division by zero: every predicate false.
- Non-numeric nodes — tuple `(5,2)`, relation `5=3` — ⇒ every predicate false
  (currently `Facts::unknown()` via the catch-all arm).

### Group E — `get_assumptions` structural rebuild (~103) · TypeScript
`interval containment 64 · element interval 16 · derived 5 · add/get misc 8 · misc 1`.
Today `Context.get_assumptions()` ignores its argument and returns a **Context**,
so the spec's `ordered_trees_equal(...)` is always false. Required semantics:
- Accept `"x"` or a nested-array form `[["x"]]` / `[["a","b"]]`; return an **AST**
  (or `undefined` when nothing is known).
- **Orientation**: facts are re-stated with the queried variable on the left
  (`x<a` stored ⇒ `get_assumptions("a")` yields `a>x`).
- **Transitive closure** over `=` and `<`/`≤` (`x<a, a<b, b<c` ⇒ for `x`:
  `x<a and x<b and x<c`), including the mixed strict/non-strict case.
- **Interval membership expansion**: `x ∈ (a,b)` ⇒ `x>a and x<b`; all four
  bracket forms, `containselement`, and the negated forms as `or`-disjunctions.
- **Subset/superset expansion**: `(a,b) subset (c,d)` ⇒ `a>=c and b<=d`, across
  all 64 bracket/negation combinations (the single largest sub-bucket).

Implement in the compat lib over the existing `_assumptionTexts` list rather than
widening the wasm ABI: the required shape is a JS-API concern, and keeping it in
TS avoids a rebuild cycle. The Rust store stays the source of truth for predicates.

### Group F — robustness (5) · TypeScript
The first row of each `sum/product/quotient/power` table has `input[0] === undefined`;
`me.add_assumption(me.from(undefined))` throws
`Cannot read properties of undefined (reading 'length')`. Adding an undefined or
empty assumption must be a no-op.

## Sequencing

Phases run **sequentially, not in parallel**: the Rust phases require rebuilding
`vendor/wasm`, which would change behaviour underneath a concurrently-measuring
TypeScript phase.

1. **Phase 2 — Rust engine** (Groups A–D, ~126 tests). Files: `src/assumptions/`
   (`infer.rs`, `facts.rs`, possibly a new negation helper). Rebuild via
   `packages/math-expressions-js-compat/build-wasm.sh`.
2. **Phase 3 — TypeScript** (Groups E–F, ~108 tests). Files:
   `lib/math-expressions.ts` (+ a new `lib/assumptions/` helper module).

## Verification & risk

- Per phase: `npx vitest run spec/slow_assumptions.spec.ts`.
- **Regression risk (Rust)**: `tests/assumptions.rs` and `tests/assumptions_corpus.rs`
  validate the engine against the JS oracle, and `infer.rs` deliberately mirrors
  several JS conservatisms (no interval arithmetic in sums; odd powers of
  negatives unsigned). Run `cargo test` for the whole crate after Phase 2 — a
  "smarter" rule that contradicts the oracle is a regression, not an improvement.
- **Regression risk (suite-wide)**: full `npx vitest run` after each phase.
  Baseline to beat: **940 failing** (see `COMPAT_TEST_FAILURE_SUMMARY.md`).
  Judge by name-level diff, never by aggregate counts.
- Per repo convention, split any file that grows past ~200 lines into a subfolder.
