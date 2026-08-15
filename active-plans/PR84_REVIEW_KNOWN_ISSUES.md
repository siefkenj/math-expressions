# PR #84 review — known issues and durable findings

The durable ledger from the thirteen review passes over
[Doenet/math-expressions#84](https://github.com/Doenet/math-expressions/pull/84). The pass-by-pass
history lives in the git log (`Review cycle N:` commits) and the PR's edit history; this file keeps
only what still describes the code. Every entry below was re-verified against the pin it names or
carries a symbol anchor checked to exist at the head this file is committed at (`41b9cb4` when the
anchors were first swept at the eleventh pass, re-spot-checked at the thirteenth); the fifth pass
re-reproduced each then-open entry through the built compat package.

Conventions: "legacy" is `math-expressions@2.x` from npm. File paths are relative to
`packages/math-expressions-rs/src/` for `.rs` and `packages/math-expressions-js-compat/lib/` for
`.ts` unless said otherwise.

## Known issues, open

None of these block DoenetML (Doenet/DoenetML#1622); they are recorded for follow-up work.

### Rust crate

- **DP5(4) evaluates stage 7 twice per accepted step** (`mathjs_compat/ode.rs`, `solve_ode`): the
  FSAL stage loop already produced `f(t+h, ynew)` into `k[6]`. 7 RHS calls per step instead of 6,
  and through `solve_ode` each one is a JS boundary crossing. The `terminated_early` branch hanging
  off it is dead.
- **`max_steps` is off by one** (`mathjs_compat/ode.rs`): an integration converging in exactly
  `max_steps` steps reports `terminatedEarly`. The vanishing-step guard beside it also uses a
  different scale from the completion test, so a large-`t` run can reject its final sliver.
- **`fold_apply::is_variadic` tests "has an exact folder" rather than "is an aggregate"**, so a
  tuple argument spreads into fixed-arity heads: `["apply","mod",["tuple",7,3]]` folds to `1`.
- **`digits = +Infinity` disables the decimals mode** in
  `round_numbers_to_precision_plus_decimals` (`ops/numbers.rs`), asymmetrically with `-Infinity`.
- **`evaluate_many`'s scalar fallback skips canonicalization** while the tape path canonicalizes,
  giving intra-batch 1-ulp inconsistency.
- **`sort_key`'s `ignore_negatives` parameter is permanently `false`** at all three call sites,
  making several branches of `normalize/default_order.rs` unreachable. (Its doc no longer
  contradicts the code about whether nested keys propagate it — the `Pow`, `Apply` and unit
  branches do, the rest do not.)
- **Two-argument `log` does not fold** (`log(8,2)` stays an application), while the `log_10` /
  `log10` spellings do.
- **`mono_less_than` answers `true` in both directions** (`polynomials/compat/mono.rs`) for two
  distinct variables that `cmp_default_order` ranks `Equal` — the tie case `mono_gcd` already
  acknowledges.
- **`evaluate_to_constant` returns `Some(NaN)`** while its rustdoc mentions only `±∞`, and the
  `evaluate_to_complex` beside it rejects `NaN`. Undocumented asymmetry.
- **Non-realness does not propagate through `+`, `*` or `^`** — incompleteness, deliberately
  declined, with no consumer left that can be wrong about it. `real: Some(false)` is produced in
  only three places (an explicit `x ∉ R` assumption, the `i` literal, a constant with a nonzero
  imaginary part) and `combine::add`/`combine::mul` never carry it through an operator, so
  `is_real(x+1)` with `x ∉ R` answers _unknown_ where legacy answers `false`. Adding the rules
  would turn `None` into `Some(false)`, and `simplify`'s rewrites are gated on exactly those
  facts; the one rewrite that treated `None` as permission (the odd-root sign extraction in
  `normalize/simplify.rs`, `simplify_root`) now declines when any _part_ of the residual is
  provably non-real, which over-declines and moves no other rewrite.
- **`MAX_UNFLATTEN_OPERANDS = 1000`** (`math-expressions-rs-wasm/src-rust/js_match.rs`) **exceeds
  serde_json's 128-deep default**, so `unflatten_left` on a wide sum returns JSON that `from_ast`
  then refuses.
- **An exact integer past f64 range crosses to JS as `Infinity`.** `simplify` folds `2^2000`
  exactly and `to_text` prints all 613 digits, but `to_js` puts each part through an f64, so
  `.tree` answers `Infinity` and `2^2000` and `2^2001` have the same `.tree` while `equals` still
  tells them apart. Parity with legacy (which held everything in a JS number) — a limit of the AST
  wire format, not a regression — recorded because `max_pow_bits` deliberately permits results a
  thousand times past the f64 ceiling.
- **`1/(0^0)` stays written out** as `["/", 1, NaN]` rather than folding to `NaN`. Every other
  arithmetic combination with a `NaN` operand folds. (The `{"$":"NaN"}` envelope no longer reaches
  `.tree`; that half is fixed — see `engine-rust.ts` in DoenetML and the corresponding
  `MATH_EXPRESSIONS_UPSTREAM_REQUESTS.md` entry.)

### Compat layer

- **Handle leaks, systemic.** `evaluate_to_constant` creates intermediates via `remove_units` and
  `simplify` and frees neither; `Context.matrix` has the same shape; `equalSpecifiedSignErrors`
  mints a `fromAst` per sign variant per recursion level (the `numSignErrorsMatched` grading
  path); `trees/basic.ts`'s `evaluateNumbers` leaks two per rewrite per pattern per round inside
  `applyAllTransformations`; `evaluate_numbers`'s `set_small_zero` branch and
  `create_discrete_infinite_set` each discard intermediates. Systemically, every `toExpr(other, …)`
  in `equals`/`add`/`match`/… leaks whenever the argument is a tree or a string — `substitute` was
  the only method freeing carefully.
- **`astToJson` and `astReplacer` are not interchangeable** despite their shared file's claim:
  `astToJson` tags non-finites but does not unwrap an `Expression`, so the tree utils reject one
  where `fromAst` accepts it.
- **`extendedMatch` produces `_skipped` but never `_skipped_before`**, leaving the `addLeft` path
  in `trees/basic.ts` dead. (`_skipped` itself is live, set by `trees/flatten.ts`.)
- **`applyAllTransformations` folds numbers only after the extended-match splice**, where legacy
  folds before it as well. The pre-fold's only observable effect is on which branch the
  `result[0] === pattern[0]` test takes; noted in the code, to keep one `fromAst` round-trip per
  rewrite. Separately, the `applyAllTransformations` _method_ on `Context`
  (`math-expressions.ts`) is documented as a normalization pass folded into `canonicalize` and
  returns `this`, silently discarding the caller's transformation list — it is neither; the real
  pattern-rewriting driver lives in `trees/basic.ts`, which nothing re-exports (`Context.utils`
  carries only `{match, flatten, unflattenLeft, unflattenRight}`).
- **`substitute_component` validates nothing**, where legacy validated the container head at each
  level and the index range. `me.fromText("x*y").substitute_component(0, 5)` answers `5·y` instead
  of throwing, and an out-of-range index returns `undefined` rather than an `Expression`, so the
  caller fails a line later on `.tree`. `get_component` has the same shape one level down: its
  container check runs on the receiver only, and the rest of the path indexes the operands of any
  operator — `("(x*y, 3)").get_component([0,0])` answers `x`. The comment describing a matrix
  entry as `[1, row, col]` describes a call the code rejects (`"matrix"` is not in
  `COMPONENT_CONTAINERS`). DoenetML's `@doenet/math` `getComponent` wrapper restores the legacy
  throw for the one call site that used it as a type test.
- **`Expression#match` silently ignores `allow_extended_match`**, which `me.utils.match` honors,
  so the two entry points disagree on the same input despite the comment claiming they cannot
  drift. Legacy's `Expression.prototype.match` delegated to the shared implementation.
- **`ABSENT_EXPRESSION` snapshots the prototype before it is finished.** The `notImplemented`
  methods and `applyAllTransformations` are attached after the IIFE builds it, so
  `solve_linear(...).applyAllTransformations()` is a `TypeError` rather than the documented
  "returns the stand-in itself"; `toText()`/`tex()` hand back the stand-in _object_ rather than
  `""`, and `Symbol.dispose` is absent.
- **`me.from` never tries MathML** although `converters.MmlToAst` exists and works; legacy's
  `create_from_multiple` had that third fallback, and `Context.fromMml` is still `notImplemented`.
- **`Context.reviver` drops the `assumptions` field** legacy restored onto a revived expression,
  and `toJSON` no longer emits it — silent on both sides of a persist/revive round trip.
- **`evaluate_to_constant` does not read `nan_for_non_numeric`.** It always behaves as `false`
  (`null` for an unevaluable expression) where legacy defaulted to `true` (`NaN`). Deliberate, and
  DoenetML depends on it — but the option is part of the legacy signature and is accepted and
  ignored; stated in the code.
- **`evaluate_to_constant`'s blank-handling comments describe the wrong trees**
  (`math-expressions.ts`, near `treeHasBareBlank`): `_` in _text_ parses as a subscript node
  `["_","＿","＿"]`, and only the `head !== "_"` exception makes it `null`; written with the blank
  itself as an operand — which is what `fromAst("＿")` produces — `0·＿` and `＿/＿` are `NaN`,
  the opposite of what the paragraph promises.
- **`equalSpecifiedSignErrors` does not require _exactly_ `n_sign_errors`,** as its docstring
  says. `singleNegations` enumerates sign-invariant positions too, so negating `x` inside `x^2`
  folds back and a perfectly correct answer scores as "1 sign error" on DoenetML's
  `numSignErrorsMatched` path. Possibly legacy-faithful; the doc should not claim otherwise
  either way.
- **The render-option key list is enumerated in four places and each is different.**
  `converters/render-options.ts`'s `FORWARDED` is the authority; two lists in
  `math-expressions.ts` omit `avoidScientificNotation` and `matrixEnvironment`, `ast-to-text.ts`
  omits `notation` and `matrixEnvironment`, and
  `packages/math-expressions-rs-wasm/src-js/wasm.ts` (outside this file's `lib/` path convention)
  omits both and drops `unicode` from the LaTeX variant only.

## Standing invariants worth knowing

- **Nothing anywhere under `lib/` may dereference `wasm` at module scope.** `setWasmModule` is
  re-exported from the package root, so importing it evaluates the whole barrel; a module-scope
  `wasm` touch triggers the node fallback — throwing in a browser, and under node quietly pinning
  the node build so a later injection can never win. The invariant is written at its site in
  `lib/math-expressions.ts` (the `Context._assumptionsHandle` lazy accessors) and pinned by a spec
  that injects a counting proxy and asserts zero touches during import.
- **The 11 skipped compat tests** are 9 in `quick_trees.spec.ts` and 2 in
  `slow_assumptions.spec.ts`; all but one carry a `[wontfix: …]` tag in the test name saying why.
  None is an engine unsoundness: legacy's expected answers there are partly false, so the tests
  cannot be passed soundly. See `active-plans/ASSUMPTIONS_ENGINE_PLAN.md` ("Accepted divergence").
  The exception is `slow_assumptions.spec.ts`'s "define constants" (`:7292`), which carries a plain
  comment rather than a tag — worth tagging so the count stays self-explaining.
- **Aggregates have no default parser spelling**: `fromText("sum(3,17,5-4)")` parses as
  `s·u·m·(…)` unless `appliedFunctionSymbols` is passed. Deliberate, matches legacy.

## Fixed during review, kept for its contract

**Odd roots of negative reals read on the real branch on every numeric path** (eleventh pass).
The branch for `(negative)^(p/q)`, odd `q`, used to depend on whether the radicand was a perfect
power — `(-8)^(1/3)` folded to `-2` while `(-2)^(1/3)` evaluated to the principal
`0.6300 + 1.0911i` — so `equals` told the same number apart from itself and four DoenetML
`<answer>` cases regressed against legacy. Fixed at three sites: `rule_radical`'s `Pow` arm
(`normalize/simplify.rs`) pulls the sign out at simplify time, which is load-bearing because
`evaluate_to_constant` runs `simplify_core` and the certified-digits tape before any evaluator;
`eval_complex`'s `Pow` arm (`eval_numeric/complex.rs`, `odd_root_exponent`) takes the same branch
for sampling — matching the raw quotient-node exponent shape too, because that walk is
`evaluate_many`'s per-point fallback and gating on `Num(Rat)` alone diverges the batch and
single-point paths at 835 corpus points; and `CBRT::eval1`/`NTHROOT::eval2`
(`special_functions/powers.rs`) follow. Even roots, decimal exponents with even reduced
denominators (`(-8)^0.3333` = `3333/10000`), and complex bases stay principal. This is a
deliberate divergence from mathjs on the engine's *own* numeric paths (`x^(1/3)` at `x = -8` is
`-2` there, not `1 + i√3`), stated in `evaluate_fast_f64`'s rustdoc. **It does not extend to
`f()`**, which compiles the tree to math.js and so keeps mathjs's principal branch for a `Pow`
node: `f()` of `x^(1/3)` at `x = -8` is `1 + i√3`, while `cbrt` and `nthroot` — which map onto
math.js functions that take the real branch themselves — are `-2`. So `evaluate_many` and `f()`
disagree about the power spelling and agree about the root spellings, and a DoenetML
`<function>x^(1/3)</function>` still has a gap at negative inputs that `<answer>` grading does not.
That gap is unchanged from legacy (which also evaluated the power spelling principal through
`numericalf`), so it is a standing difference rather than a regression, and closing it would mean
mapping the odd-root `Pow` shape onto `nthRoot` in `tree-to-mathjs.ts`. Pinned in
`tests/odd_root_real_branch.rs` and `spec/quick_doenet_grading_gaps.spec.ts`, both verified to fail
against the unfixed engine.

**`f()` could not compile `nthroot`** (twelfth pass). `functionConversions` in
`packages/math-expressions-rs-wasm/src-js/tree-to-mathjs.ts` maps AST heads onto math.js names,
and math.js spells this one `nthRoot`. An unknown head is not a compile error — it becomes a
`FunctionNode` over an undefined symbol and throws `Undefined function nthroot` on the first
`evaluate` — so `nthroot(x, n)` was unevaluable through `f()` at _every_ input, not only at
negative ones. `f()` is the plotting and root-finding entry point, so a DoenetML
`<function>nthroot(x,3)</function>` drew nothing at all; legacy plotted it. Now mapped, which also
puts an odd root of a negative on the real branch (`nthRoot(-8, 3) === -2`), consistent with the
odd-root entry above and with `cbrt`. Pinned in `spec/quick_doenet_compat_pr84.spec.ts`, verified
to fail with the mapping removed.

**The sibling sweep that entry asked for, done** (thirteenth pass). Every spelling the Rust
registry can produce was diffed against `Object.keys(mathjs)` and against `functionConversions`,
and every author-typable spelling — the union of DoenetML's `appliedFunctionSymbolsDefault` and
`…Latex`, 69 of them — was then evaluated through `f()` at two in-domain points. `nthroot` was the
only head broken that way. One head, `rootof`, is deliberately unmapped: it is in neither of
DoenetML's applied lists, so it cannot be typed, and the `critical_points()` output that produces
it goes through `evaluate_to_constant`, never `f()`.

**`erf` had no evaluation kernel at all** (thirteenth pass) — the mirror image of `nthroot`, and
just as silent. `ERF` in `special_functions/misc.rs` carried parser spellings and LaTeX rendering
but no `eval1`, so `evaluate_to_constant("erf(0.5)")` was `None` and `evaluate_many` sampled `NaN`
at every point, while `f()` was right throughout because math.js *has* `erf`. A DoenetML
`<function>erf(x)</function>` therefore plotted a correct curve whose
`<number>$$f(0.5)</number>` read `NaN` and whose extrema search found nothing — and legacy
evaluated `erf` from all of those paths, so this was a regression. `eval1` is now a port of the
same W. J. Cody rational-Chebyshev approximation math.js uses, so the two paths agree to the last
bit rather than to a tolerance. Pinned in `tests/erf.rs` (which also asserts the three numeric
entry points agree) and `spec/quick_doenet_compat_pr84.spec.ts`, verified to fail with `eval1`
removed. The general lesson is the one the sweep confirms: a head can be missing from *either*
path, and neither absence produces a warning.

Everything else fixed during the review passes is described by its `Review cycle N:` commit and
its tests; the suite state at this head is `cargo test --workspace` 862 passed / 0 failed and the
compat suite 6,354 tests — 6,343 passing, 11 skipped, 0 failing — with `cargo fmt` and
`clippy -D warnings` clean.
