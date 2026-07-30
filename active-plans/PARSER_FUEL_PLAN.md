# Parser fuel + deterministic adversarial testing plan

## Status — IMPLEMENTED (Part A + Part B)

Done and green (`cargo test -p math-expressions` passes, no regressions):

- **Part A** — `max_parse_steps` in `resource_limits.rs`; `steps`/`max_steps` +
  `tick()` in the shared parser state (reset in `convert`); `self.tick()?` in
  **all 22 loops** across `shared_grammar.rs` (11), `text.rs` (5), `latex.rs` (6);
  and the `Tok::Eof` exit on the matrix loop. `\begin{bmatrix}` now returns
  `Err("Expecting \end{bmatrix}")` in ~1 ms (was an infinite loop); valid matrices
  still parse. Wasm rebuilt.
- **Part B** — `tests/parse_adversarial.rs` (curated corpus + the regression +
  deterministic budget-enforcement test) and `tests/parse_enumeration.rs`
  (truncation / bounded-exhaustive / single-edit sweeps, run under a scoped budget
  so the exhaustive pass stays ~1 s). Fully deterministic, no RNG.

**Second bug found by this suite (separate, pre-existing, NOT yet fixed):** a deep
superscript chain (`^^^^…`, N ≳ 4000) parses into an N-deep `Pow` tree because the
loop-based caret handler builds nesting without charging the recursion-depth budget
(`!`×N correctly errors at the cap; `^`×N does not). Later recursive processing of
that tree overflows the stack. This is a **recursion / AST-depth** issue, outside
the loop-fuel backstop's scope. Captured as the ignored test
`superscript_nesting_overflows_known_bug`. **Follow-up fix:** count loop-built
nesting against `MAX_PARSE_DEPTH` (or bound AST depth) so `^`×N errors like `!`×N.

## Context

The playground froze when a user typed `\begin{bmatrix}` in LaTeX mode. Investigation
showed **both** parsers (the canonical JS library _and_ the Rust port) infinite-loop
on an **opened-but-unclosed environment**. Reproduced against the Rust wasm:
`parse_latex("\\begin{bmatrix}")` and `parse_latex("\\begin{bmatrix} 1")` never return;
`\begin{bmatrix} 1 \end{bmatrix}` is fine. The playground has a stop-gap guard
(`guardLatex` in `packages/playground/src/engines.ts`) that refuses unbalanced
`\begin`/`\end` before calling either engine — necessary because the JS library is
external and unfixable — but the **Rust core still has the bug** and will hang any
other consumer.

### Root cause (the specific issue)

`matrix_environment` in `packages/math-expressions-rs/src/parse/latex.rs` (loop at
~L407):

```rust
while self.token.ttype != Tok::EndEnvironment {
    if Amp { … self.advance()?; }
    else if Linebreak { … self.advance()?; }
    else { row.push(self.statement(P::default())?); last_token = Tok::Space; }
}
```

At EOF the token is `Tok::Eof` (never `EndEnvironment`). It isn't `Amp`/`Linebreak`,
so the `else` branch runs `self.statement(...)`, which at EOF returns `Expr::Blank`
**without consuming input** (the lexer's `advance` returns `Tok::Eof` forever —
lexer.rs L1110). The loop condition never becomes false and no token is consumed →
**no forward progress → infinite loop**.

### Why the existing safeguards miss it

The parsers already bound **recursion depth** (`shared_grammar.rs` `enter()`/`leave()`

- `depth`, cap `MAX_PARSE_DEPTH = 64` in `common.rs`) — that catches nesting/prefix
  chains (`(((…`, `----x`). It does **not** bound **loop iterations**, and the matrix
  loop is a flat `while`, not recursion. `resource_limits.rs` is the crate's single
  source of truth for deterministic operation budgets (`max_expand_terms`,
  `max_integration_steps`, …) but has **no parse-step budget**. That is the gap.

### Goal

1. A **blanket fix — parse fuel**: a deterministic per-parse step budget so _any_
   loop that fails to make progress (this bug and any future one) aborts with a
   clean `ParseError` instead of hanging.
2. A **deterministic adversarial + enumeration test suite** that proves termination
   for pathological input and locks it in as a permanent regression guard.

Non-goal: fixing the JS library (external). The playground `guardLatex` stays as
the JS-side mitigation; this plan removes the hang at the Rust library level.

---

## Part A — Blanket solution: parse fuel

Fuel counts **loop iterations / units of parser work**, complementing the existing
depth budget. It is deterministic (operation count, never wall-clock — matches the
`resource_limits` doctrine), so verdicts are identical on every machine.

### A1. Budget lives in `resource_limits.rs`

Add to `ResourceLimits`:

```rust
/// Total parser steps (loop iterations across a single parse) before the
/// input is refused. Far above any real expression; low enough that a
/// non-progressing loop aborts in microseconds.
pub max_parse_steps: usize,   // default e.g. 5_000_000 (usize: matches depth, wasm32-native)
```

Wire into `Default` and any preset constructors. Scopable via the existing
`resource_limits::with(...)` — tests set a tiny budget to exercise the abort path
deterministically.

### A2. Fuel counter in the shared parser state (`shared_grammar.rs`)

Alongside `depth`, add `steps: usize`. Reset in `convert()` (which already does
`self.depth = 0`). Read the cap once from `resource_limits::current().max_parse_steps`.

### A3. A `tick()` method (mirrors `enter()`)

```rust
fn tick(&mut self) -> R<()> {
    self.steps += 1;
    if self.steps > self.max_parse_steps {
        return Err(self.err("input too large — parser step budget exceeded"));
    }
    Ok(())
}
```

### A4. Call `self.tick()?` at the top of every unbounded loop

Both files, every `while` / `loop`. Inventory (from grep):

- `latex.rs`: matrix env L407 (**the bug**); prime L579; caret L583;
  `non_minus_factor` while-let L769; `loop {}` L886.
- `text.rs`: prime L368; caret L373; `non_minus_factor` L498; `loop {}` L615.
- `shared_grammar.rs`: `statement_list` comma loop L43.

(Bounded index loops like `while i + 1 < ops.len()` don't need it, but ticking them
is harmless and keeps the rule mechanical: _every loop ticks_.) Each iteration now
consumes fuel, so a non-advancing loop burns the budget and aborts.

### A5. Local hardening (defense-in-depth + good UX)

Fuel is the backstop; add the precise fix so the common case gives a good message,
not the generic budget error:

- Matrix loop: terminate on `Tok::Eof` too —
  `while !matches!(self.token.ttype, Tok::EndEnvironment | Tok::Eof)`. The existing
  post-loop check then yields `"Expecting \end{bmatrix}"`.
- Audit the other `while self.token == X` / `loop {}` sites for an EOF/`else`
  path that can spin without advancing; give each an explicit `Eof` exit.

### A6. (Optional) debug-only forward-progress assertion

In debug builds, record the lexer byte offset at each advance-driven loop head and
`debug_assert!` it strictly increases (or a structural counter did) — catches a
missing EOF exit _at its source_ during development, with the fuel budget as the
release-mode guarantee. Ship fuel; keep this behind `cfg!(debug_assertions)`.

**Why fuel is the right blanket:** it bounds total work regardless of _why_ a loop
won't stop (missing EOF exit, mis-lexed token, pathological-but-progressing input),
so it covers the whole class — including loops added in the future — in one place.

---

## Part B — Adversarial inputs (identify + lock in)

Build on the repo's precedent: `tests/rootof_adversarial.rs` (adversarial naming) and
`tests/autogenerated_fuzz_tests.rs` (already does `catch_unwind` + a hang-resistance
check).

**Everything here is fully deterministic — no randomness, no RNG, no seeded
generators.** Coverage comes from _systematic enumeration_ (truncation sweeps,
bounded exhaustive combinations, and deterministic single-edit mutations), not
sampling. `proptest` is deliberately **not** used. Every run parses exactly the same
inputs, so a failure is always reproducible and CI is never flaky.

### B1. Curated adversarial corpus — `tests/parse_adversarial.rs`

A table of hand-written pathological inputs, each asserted to return `Ok|Err`
(never panic, never hang) for **both** `parse_text` and `parse_latex`, under a small
`resource_limits::with` budget so termination is _proven by the budget_, not timing:

- **Unclosed / mismatched environments** — `\begin{bmatrix}`, `\begin{bmatrix} 1`,
  `\begin{bmatrix} 1 & 2`, `\begin{bmatrix}\end{pmatrix}`, nested
  `\begin{a}\begin{b}\end{b}`, stray `\end{bmatrix}`.
- **Deep nesting** — `((((((((…`, `\frac{\frac{\frac{…`, `\sqrt{\sqrt{…`.
- **Long unary chains** — `----x`, `!!!!x`, `x^^^^`, `x____`, `+++++x`.
- **Unbalanced delimiters** — `(1`, `[1`, `{1`, `|1`, `\left( 1`.
- **Bulk repetition** — 10⁵ of `(`, `!`, `^`, `&`, `\\`.
- **Truncations of valid input** — take known-good expressions and cut at each
  byte (the systematic generator for "unclosed construct" bugs — exactly this one).
- **Junk / unicode** — control chars, lone backslashes, `\begin{` with no name.

Includes the explicit regression: `parse_latex("\\begin{bmatrix}")` → `Err`
(pre-fix hangs; post-fix returns quickly).

### B2. Systematic enumeration — `tests/parse_enumeration.rs` (deterministic)

Three enumerators, no randomness. Each asserts _"parse returns (Ok|Err) without
panic under the fuel budget"_ for both parsers on **every** generated input:

1. **Truncation sweep** — the highest-yield method for unclosed-construct bugs, and
   fully deterministic. Take a fixed list of known-good expressions (text + LaTeX,
   including matrices, `\frac`, `\sqrt`, nested delimiters) and parse **every prefix**
   `s[..i]` for `i` in `0..=s.len()` (respecting char boundaries). This mechanically
   produces every half-open construct — `\begin{bmatrix}`, `\begin{bmatrix}1&`,
   `\frac{`, `\sqrt{2` — which is exactly the bug family.
2. **Bounded exhaustive combinations** — a small curated alphabet of _dangerous
   tokens_ (`\begin{bmatrix}`, `\end{bmatrix}`, `&`, `\\`, `^`, `_`, `!`, `(`, `{`,
   `[`, `|`, `1`, `x`) and enumerate **all** sequences up to length `k` (e.g. k=4 →
   ≤ 13⁴ ≈ 28k cases, a fixed, exhaustive set). Catches ordering-dependent loops
   (e.g. `\begin{bmatrix}` followed only by `&`/`\\` and then EOF).
3. **Deterministic single-edit mutations** — for each corpus/known-good input, apply
   _every_ single-token deletion, duplication, and dangerous-token insertion at
   _every_ position (a full, ordered sweep — no random choice). Surfaces "valid input
   minus one delimiter" hangs.

All counts are fixed and enumerated in source order; the exact same inputs run every
time. Sizes are chosen so the whole suite stays well under a second.

### B3. Test-harness safety net (rollout only)

So a _latent_ loop lacking a `tick()` fails the suite instead of hanging CI, run each
enumerated parse on a worker thread with a join timeout (native tests only) and fail
on timeout. This is a fixed per-input wall-clock _ceiling in the harness_ used only to
convert a hang into a failure — it does not make the inputs or verdicts random, and
the library itself stays wall-clock-free. Once every loop ticks it never fires; it
guards loops added later without fuel.

---

## Part C — CI

The new tests run under the existing `rust-test` job (`.github/workflows/ci.yml`
already runs `cargo test`). The enumeration suite is a fixed, bounded set that runs in
well under a second, so it belongs in the normal `cargo test` run — no separate or
scheduled job, and no flakiness. To widen coverage later, raise the enumeration bounds
(prefix set, alphabet, length `k`) in source — still fully deterministic.

---

## Files touched

- `src/resource_limits.rs` — add `max_parse_steps` (+ default, presets).
- `src/parse/shared_grammar.rs` — `steps` field, `tick()`, reset in `convert()`.
- `src/parse/latex.rs`, `src/parse/text.rs` — `self.tick()?` in every loop; explicit
  `Eof` exit on the matrix loop (and any sibling that can spin at EOF).
- `src/parse/common.rs` — a `MAX_PARSE_STEPS` default constant if not carried solely
  in `ResourceLimits`.
- `tests/parse_adversarial.rs` — curated corpus + the `\begin{bmatrix}` regression.
- `tests/parse_enumeration.rs` — deterministic truncation / bounded-exhaustive /
  single-edit-mutation sweeps + the termination property (no `proptest`, no RNG).
- `packages/playground/src/engines.ts` — **unchanged**; `guardLatex` stays as the
  JS-engine mitigation (JS is external and still hangs).

## Verification

1. **Reproduce → fix**: a test asserting `parse_latex("\\begin{bmatrix}")` returns
   `Err` quickly. Before A4/A5 it hangs (run it under the harness timeout to prove
   the hang); after, it returns `Err`.
2. `cargo test` green, including the new adversarial + enumeration suites (no panics, no
   timeouts).
3. Deterministic abort: with `resource_limits::with(tiny_budget, …)`, a large input
   returns the budget `Err` at the same step count on every run.
4. Rebuild wasm (`packages/playground/rebuild-wasm.sh`) and confirm the playground no
   longer hangs even with `guardLatex` removed on the **Rust** path (keep it for JS).
5. No perf regression: `tick()` is an increment+compare; benchmark a large valid
   parse to confirm negligible overhead.

## Open decisions

- **Budget value** for `max_parse_steps` (default). Pick from the largest realistic
  input × safety factor; validate the biggest existing corpus/fixture parses well
  under it.
- **One budget vs two**: a single `max_parse_steps` for both parsers (simple), or a
  separate lexer-token cap. Recommendation: one parser-step budget; the lexer already
  terminates (it emits `Eof` and stops).
- **Error taxonomy**: reuse the generic `ParseError` with a distinct message, or add
  a `ParseError::BudgetExceeded` variant for callers that want to distinguish "too
  large" from "malformed". Recommendation: distinct message now, variant if a caller
  needs it.
