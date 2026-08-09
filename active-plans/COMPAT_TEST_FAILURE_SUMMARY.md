# js-compat test failure summary

Snapshot of `packages/math-expressions-js-compat` (`npx vitest run`) on branch
`doenet`, after rebuilding `vendor/wasm` (`bash build-wasm.sh`) so results reflect
current Rust source.

**Current totals (2026-08-09): 97 failed / 6222 passed / 6319 total**
(2 files skipped, 1 test skipped, 2 todo). Previous snapshots: 380, then 162.

Caveat on this snapshot: the working tree also carried uncommitted in-progress
work on `evaluate_to_constant`, `fold_apply`, `units` and the matrix pipeline
that is not part of the session below. That work accounts for 8 of the fixes
(`slow_simplify`'s `evaluate_to_constant` and matrix cases) and for the single
regression noted at the end.

Judge changes by name-level diff against a baseline worktree, never by aggregate
counts (see memory `js-compat-suite-baseline-diff`). The AST session below was
verified that way against `d75c045`: **0 regressions** — no test present under the
same name in both runs went from passing to failing. 78 same-name tests went
failed → passed; the other 129 fixes changed name, because these specs derive the
test name from the expected string and 129 expectations were rewritten. Total test
count is unchanged at 6318.

Note when diffing: several spec files contain **duplicate test names**, so keying a
comparison by name alone silently drops results. Key by (file, name, occurrence).

## AST tail session — 130 → 97

The nine converter specs were already at zero; this pass took the AST-shaped
failures that live *outside* them. All of these buckets are now at zero:
`quick_pm`, `quick_sets`, `quick_transformation`, `quick_mml-to-latex`,
`quick_arithmetic`, `quick_rounding`, and the three `quick_doenet_*` rounding
specs.

### Bindings that were missing, not features that were missing

Four of these looked like unimplemented functionality and turned out to be
plumbing — the engine already did the work and nothing was wired to it:

| what | where it already lived |
| ---- | ---------------------- |
| `expand_relations` (2 tests) | `assumptions::expand::expand_relations`, used by the assumptions store all along; only the public binding was absent. |
| `create_discrete_infinite_set` (7) | `equality::discrete_infinite`, complete. Needed a `Context`-level factory (the prototype mirror ran `toExpr` over the `{offsets, periods}` config and rejected it), plus `min_index`/`max_index` on the wasm entry point. |
| `output_unicode` on `toString`/`toText`/`toLatex` (1) | `renderOptions` already translated the legacy key; these four methods bypassed it with a bare `JSON.stringify`, so the option was silently dropped. |
| `pm` helpers (7) | Nothing — a genuine port of `lib/expression/pm.js`. Kept in JS: `expand_pm_signs` returns up to 1024 trees and crossing the boundary per tree costs more than building them. |

### Real engine defects

- **`(±x)·(±x)` folded to `(±x)²`** (`normalize::constructors::mul`). Each `±`
  is an independent sign, so the product ranges over {x², −x²} and the power
  over {x²} only — the fold silently dropped half the value set. Factors
  carrying a `±` now stay written, the way coordinate vectors already did. A
  product with exactly one top-level `±` is unaffected: the scaling rule pulls
  that sign out earlier, which is sound precisely because there is no second
  sign to interact with.
- **`a.mod(b)` disagreed with the parser.** The builder made
  `OtherOp("mod",[a,b])` while `mod(a, b)` parses to
  `["apply","mod",["tuple",a,b]]`. Same operator, two spellings.
- **Discrete-infinite-set equality divided by a possibly-zero period.** The
  canonicalizer folded `2c/c` unconditionally, so `{a + kc}` and
  `{a, a+c} + 2kc` compared equal whether or not `c ≠ 0` was assumed — which
  made the assumption unobservable. `progression_contained` now requires
  `is_nonzero(period) == Some(true)`. This replaces an accepted divergence
  recorded in `tests/sets.rs`.
- **`mmlToLatex`** was a throwing stub; ported from legacy (309 lines), with
  `xml-parser`'s `parse()` inlined rather than adding an unmaintained CJS
  dependency — its unescaped-text behaviour is load-bearing for the entity table.

### Four stale expectations updated instead

Each of these asserted behaviour that a later, documented decision had already
superseded; the specs were simply never updated. Verified case by case rather
than assumed:

- **`1/3` under display rounding** (2 specs). These wanted "decimalize when the
  rounding changes the value". `ops::numbers::is_written_as_fraction` had
  deliberately replaced that rule with a spelling-based one two commits later,
  with the reasoning written down. I implemented the exactness rule first, found
  it broke the Rust mirror test, and reverted it — dates settled it (spec Aug 5,
  rule Aug 7/9).
- **`2.675` rounds to `2.68`, not `2.67`.** Checked against mathjs itself, which
  legacy rounded through: `format(2.675, {notation:"fixed", precision:2})` is
  `"2.68"`. Rounding reads the shortest decimal spelling, not the stored binary.
- **`round_numbers_to_decimals(100)` on an over-long literal.** The expected
  values were the f64 readings of the input, because in the JS library a decimal
  became a double at parse time. Decimals are exact rationals here, so a no-op
  rounding returns what was typed.
- **`default_order` is no longer a no-op.** Confirmed against the legacy
  `trees/default_order.js`, which also answers `["+","x",3]` for `3+x`.

## AST session — 369 → 162 (−207)

Every AST-shaped spec is now at zero: `quick_ast-to-text`, `quick_text-to-ast-to-text`,
`quick_ast-to-latex`, `quick_ast-to-mathjs`, `quick_mathjs-to-ast`, `quick_ast-to-guppy`,
`quick_latex-to-ast`, `quick_text-to-ast`, `quick_normalization`.

### Rust core fixes

| area                                                        | change                                                                                                                                                                                                                                                                                                                       |
| ----------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| text printer                                                | ASCII `±` now spells `plusminus`, the lexer keyword. `+- 3` re-lexed as `+(-3)`, so the old spelling did not round-trip.                                                                                                                                                                                                     |
| text printer                                                | Leibniz derivatives print tight (`dx/dt`, `d^3x/dsdt^2`) when every variable is one character; the separating space is kept only for multi-character names, where `dhello` would lex as one symbol. This made **10** established-output divergences disappear — the text printer now matches the JS oracle exactly on these. |
| LaTeX printer                                               | `\partial ^{2}x` → `\partial^{2}x`. The space belongs before a letter that would extend the control word, not before a superscript; a new `cat` helper decides that per join.                                                                                                                                                |
| LaTeX printer                                               | A multi-argument application is an application to a tuple, so it now pads its parentheses the way `render_seq` pads every other delimiter pair (`f\left( x, y, z \right)`). Removed **3** more divergences.                                                                                                                  |
| LaTeX printer                                               | `matrixEnvironment` is honored (`LatexOpts::matrix_environment`), restricted to the three environments the LaTeX _parser_ accepts so output keeps round-tripping and the option cannot inject LaTeX.                                                                                                                         |
| `normalize_applied_functions`, `normalize_negative_numbers` | Exported from `normalize::syntactic` (they were already there as passes 2 and 3 of `normalize_syntactic`) and bound through wasm. The compat layer had them as **no-ops returning `this`** — see the un-masking note below.                                                                                                  |
| `constants_to_floats`                                       | The base of `e^x` is now exempt. That `e` is half the spelling of the exponential function, not a constant standing in for a number; floating it left a tree nothing recognizes as `exp`.                                                                                                                                    |
| `subscripts_to_strings`                                     | Numeric bases collapse too (`2_2`, `3_y`), and legacy's `force` flag is implemented (via the text printer, as legacy did) and plumbed through all three layers.                                                                                                                                                              |
| `strings_to_subscripts`                                     | The base half is number-parsed, as the suffix already was.                                                                                                                                                                                                                                                                   |

### Compat-layer fixes

- **`Infinity` at the wasm boundary** — `text-to-ast.ts` / `latex-to-ast.ts` called
  bare `JSON.parse`, bypassing `jsonToAst`, so `{"$":"Inf"}` leaked into caller
  ASTs. The decoder already handled all four tags correctly.
- **`astToMathjs` set relations** — `tree-to-mathjs.ts` narrowed operands to arrays
  before testing for `interval`, so `["in","x","A"]` threw `Badly formed ast`
  instead of the intended "…not implemented…". Reordered.
- **`mathjsToAst` (28) and `astToGuppy` (4)** — ported from the legacy JS
  (`tmp/js-legacy/lib/converters/`) to TypeScript. Both legacy files are
  import-free tree→tree/tree→string converters, so the ports are faithful and the
  specs (verbatim copies) pass unmodified. Pure notation, no math.

### Test expectations updated to the Rust conventions

129 expectations across `quick_ast-to-text` (60), `quick_text-to-ast-to-text` (53)
and `quick_ast-to-latex` (16). Each spec carries a header comment listing the
conventions. They fall into five groups:

- delimiters no longer padded — `( 1, 2 )` → `(1, 2)`, `f( x, y )` → `f(x, y)`
- redundant parentheses dropped — `(1/2) x` → `1/2 x`, `x_(y_z)` → `x_y_z`
  (`_` is right-associative, so the parens said nothing)
- parentheses **added** where the old spelling did not re-parse — `x^a!` → `x^(a!)`,
  which the grammar reads as `(x^a)!`; likewise `C^+` → `C^(+)`
- `¬` for `not`, consistent with the `≠`/`∪`/`∀` the printer already emits, and
  without the trailing space that forced `(not B)` to be bracketed. ASCII output
  still says `not`.
- `∠(A, B, C)` for `∠ABC` — the spelling `quick_latex-to-ast-to-latex` adopted
  earlier, and the only one that survives non-atomic arguments (JS itself fell
  back to it for `∠( A + B, B D, x/y )`)

These were not accepted on looks. Every one was checked against the property the
round-trip spec only approximates: **re-parsing the rendered string yields the
same AST as the input**. That held for all 47 distinct round-trip inputs. The
twelve `ast-to-text` cases where it does not hold are pre-existing and unrelated
to the change — text notation cannot express a matrix or distinguish a vector
from a tuple, and the original library did not round-trip them either.

### One deliberate divergence kept

`normalize_function_names` folds `exp(x) → e^x` and leaves `sqrt` alone; the JS
folded both the other way. The six affected expectations were updated rather than
the code, because the direction is a documented design decision at
`ops::transforms::normalize_function_names`: this engine canonicalizes powers, so
powers are where the two spellings have to meet (`e^(-t)` simplifies to `1/e^t`
while `exp(-t)` stays applied, so folding the JS way left them apart).

### A vacuous pass un-masked

Making `normalize_negative_numbers` real exposed two `slow_simplify` tests that
had been passing only because it was a no-op. They compare `simplify()` output
against a hand-written expression normalized for sign placement — but normalized
on the _expected_ side only. Made the ten such comparisons in that file symmetric,
which is the comparison the author was reaching for; all four cbrt assertions then
agree exactly.

## Remaining failures by spec file

| count | spec file                                       | root cause / category                                                                                             |
| ----: | ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
|    26 | `slow_simplify`                                 | canonical **ordering**, **number folding** at singular values, **radical/expand algebra** incomplete — see below   |
|    21 | `slow_check-equality-numerical-errors`          | numerical-tolerance equality returns false                                                                        |
|    18 | `slow_check-symbolic-equality-numerical-errors` | as above (symbolic variant)                                                                                       |
|    17 | `quick_trees`                                   | tree matching with predicate/regex conditions can't cross the wasm boundary                                       |
|     5 | `slow_assumptions`                              | see below                                                                                                         |
|     4 | `slow_math-expressions`                         | —                                                                                                                 |
|     3 | `quick_solve`                                   | `solve_linear()` unimplemented                                                                                    |
|     2 | `slow_rational`                                 | —                                                                                                                 |
|     1 | `quick_doenet_compat_pr84`                      | `0*blank` → `NaN` where the spec wants `null`; from the in-flight `evaluate_to_constant` work, see the note below  |

`slow_matrix` (was 24) went to zero on the in-flight matrix-pipeline work, as did
8 of the `slow_simplify` cases.

### The one regression in this snapshot

`quick_doenet_compat_pr84 > "does not collapse 0*blank to a number"` expects
`me.fromText("0*_").evaluate_to_constant()` to be `null`; it now returns `NaN`.
The final line of the rewritten `evaluate_to_constant` is
`return freeVars.length > 0 ? null : NaN`, and a blank is not a variable, so
`0*_` has no free variables and takes the `NaN` branch. Two specs currently
disagree about blanks — `slow_simplify`'s "evaluate_to_constant with blanks
gives NaN" (now passing) and this one — so the fix is a decision about which
reading of "undefined" wins, not a typo.

### The 5 remaining `slow_assumptions` failures

- `is integer / via assumptions` — **not an engine bug**. `add_assumption` round-trips
  through `.toString()` and the text printer loses negation scope: `paren_if_spaced`
  (`src/print/text.rs:223`, same in `latex.rs:271`) tests `starts_with('(') && ends_with(')')`,
  which cannot distinguish `(a or b)` from `(a) or (b)`. Needs a paren-balance scan.
  Fed a correctly-parenthesized tree the engine answers correctly.
- `strict pow` — needs a `me.math.pow_strict` toggle with no wasm equivalent.
- `logical combinations`, `combined assumptions`, `combined assumptions, negated` —
  or-disjunction and interval-membership _reasoning_ (as opposed to retrieval).

### The 26 `slow_simplify` failures

Three root causes, not the single "Infinity/neg-zero" this row used to claim.
The counts below were taken at 42; the matrix/vector and `evaluate_to_constant`
cases have since closed, so ordering is now the bulk of what is left.

- **Canonical ordering (~13)** — sums are mathematically correct but ordered
  differently: numeric constants like `e`/`i` float to the front instead of
  sorting alphabetically, and mixed containers (tuple/vector/altvector/interval)
  group by type rather than interleaving by value. Tests: evaluate_numbers
  combination + the two "sorted the same" cases; collect "speed tests" / "with
  units" / "lone - or + signs"; every matrix/vector add-subtract and
  scalar-multiple case. Likely one comparator fix.
- **Number folding at singular values (~10)** — `Infinity`, `÷0`, negative zero,
  `x^0` (folded to 1 without an x≠0 assumption), like-unit combination, and
  `evaluate_to_constant` returning `null`/real where `NaN`/`Infinity`/complex is
  expected (blanks, det/trace, units, matrices, `Infinity*i`).
- **Radical / matrix / expand algebra incomplete (~12)** — pulling numeric
  factors out of roots (`sqrt(-16x^5)`), `abs` distribution into powers,
  distributing a scalar into matrix-product entries (`(M·N·g).expand()`), matrix
  powers, and `i^2 → -1` in expand. Includes the one genuine **wrong-output**
  bug: `int (x^2+x)dx` expand distributes the `dx` differential across the sum.

## Remaining buckets by theme

**Numerical-tolerance equality (39)** — the two `check-*-numerical-errors` specs.
Previously mis-attributed to the assumptions system; they are independent. Now
the largest single area.

**Semantic edge cases (26)** — `slow_simplify`: canonical ordering, number
folding at singular values, radical/expand algebra. See the per-bucket breakdown
above.

**Unimplemented / unbound APIs (20)** — `quick_trees` (17, predicate/regex
callback matching cannot cross the wasm boundary) and `quick_solve` (3,
`solve_linear`). `slow_matrix`'s missing `me.matrix` constructor closed.

**Printer / formatting (~1)** — was the largest bucket at ~170; the AST sessions
closed it. What is left is the `paren_if_spaced` bug noted under
`slow_assumptions`, which is a printer defect reported through another spec:
`src/print/text.rs` tests `starts_with('(') && ends_with(')')`, which cannot
distinguish `(a or b)` from `(a) or (b)`. It needs a paren-balance scan.

## Highest-leverage remaining item

The two `check-*-numerical-errors` specs (39) are now the biggest block and share
one root cause, so they are likely one fix. After that, the `slow_simplify`
canonical-ordering bucket is described above as "likely one comparator fix".

A note for whoever picks these up: four of the fixes in the tail session were
bindings for engine code that already existed and was already exercised
elsewhere. Before implementing anything that looks like a missing feature here,
grep the Rust core for it first.
