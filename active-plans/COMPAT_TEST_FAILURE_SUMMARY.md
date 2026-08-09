# js-compat test failure summary

Snapshot of `packages/math-expressions-js-compat` (`npx vitest run`) on branch
`doenet`, after rebuilding `vendor/wasm` (`bash build-wasm.sh`) so results reflect
current Rust source.

**Current totals (2026-08-09): 162 failed / 6153 passed / 6318 total**
(2 files skipped, 1 test skipped, 2 todo). Previous snapshot was 380 failed.

Judge changes by name-level diff against a baseline worktree, never by aggregate
counts (see memory `js-compat-suite-baseline-diff`). The AST session below was
verified that way against `d75c045`: **0 regressions** — no test present under the
same name in both runs went from passing to failing. 78 same-name tests went
failed → passed; the other 129 fixes changed name, because these specs derive the
test name from the expected string and 129 expectations were rewritten. Total test
count is unchanged at 6318.

Note when diffing: several spec files contain **duplicate test names**, so keying a
comparison by name alone silently drops results. Key by (file, name, occurrence).

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

| count | spec file                                                                              | root cause / category                                                                                                                                                        |
| ----: | -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|    42 | `slow_simplify`                                                                        | three buckets (see below): canonical **ordering** (~13), **number folding** at singular values (~10), **radical/matrix/expand algebra** incomplete (~12, incl. one real bug) |
|    24 | `slow_matrix`                                                                          | `me.matrix is not a function`; vector/tuple typing (`tuple` vs `vector` after add)                                                                                           |
|    21 | `slow_check-equality-numerical-errors`                                                 | numerical-tolerance equality returns false                                                                                                                                   |
|    18 | `slow_check-symbolic-equality-numerical-errors`                                        | as above (symbolic variant)                                                                                                                                                  |
|    17 | `quick_trees`                                                                          | tree matching with predicate/regex conditions can't cross the wasm boundary                                                                                                  |
|    10 | `quick_pm`                                                                             | —                                                                                                                                                                            |
|     7 | `quick_sets`                                                                           | —                                                                                                                                                                            |
|     5 | `slow_assumptions`                                                                     | see below                                                                                                                                                                    |
|     4 | `slow_math-expressions`                                                                | —                                                                                                                                                                            |
|     3 | `quick_solve`                                                                          | —                                                                                                                                                                            |
|     2 | `quick_transformation`, `slow_rational`                                                | —                                                                                                                                                                            |
|     1 | `quick_arithmetic`, `quick_rounding`, `quick_mml-to-latex`, + 4 `quick_doenet_*` specs | —                                                                                                                                                                            |

### The 5 remaining `slow_assumptions` failures

- `is integer / via assumptions` — **not an engine bug**. `add_assumption` round-trips
  through `.toString()` and the text printer loses negation scope: `paren_if_spaced`
  (`src/print/text.rs:223`, same in `latex.rs:271`) tests `starts_with('(') && ends_with(')')`,
  which cannot distinguish `(a or b)` from `(a) or (b)`. Needs a paren-balance scan.
  Fed a correctly-parenthesized tree the engine answers correctly.
- `strict pow` — needs a `me.math.pow_strict` toggle with no wasm equivalent.
- `logical combinations`, `combined assumptions`, `combined assumptions, negated` —
  or-disjunction and interval-membership _reasoning_ (as opposed to retrieval).

### The 42 `slow_simplify` failures

Three root causes, not the single "Infinity/neg-zero" this row used to claim:

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

**Semantic edge cases (42)** — `slow_simplify`: canonical ordering (~13), number
folding at singular values (~10), radical/matrix/expand algebra (~12). See the
per-bucket breakdown above. Now the largest area.

**Numerical-tolerance equality (39)** — the two `check-*-numerical-errors` specs.
Previously mis-attributed to the assumptions system; they are independent.

**Unimplemented / unbound APIs (41)** — `slow_matrix` (24, `me.matrix` is not a
function) and `quick_trees` (17, predicate/regex callback matching cannot cross
the wasm boundary).

**Printer / formatting (~1)** — was the largest bucket at ~170; the AST session
closed it. What is left is the `paren_if_spaced` bug noted under
`slow_assumptions`, which is a printer defect reported through another spec:
`src/print/text.rs` tests `starts_with('(') && ends_with(')')`, which cannot
distinguish `(a or b)` from `(a) or (b)`. It needs a paren-balance scan.

## Highest-leverage remaining item

The `slow_simplify` canonical-ordering bucket (~13) is described above as "likely
one comparator fix" — the best ratio of tests to change. After that,
`slow_matrix`'s missing `me.matrix` constructor (24) is a binding, not math.
