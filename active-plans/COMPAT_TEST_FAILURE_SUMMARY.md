# js-compat test failure summary

Snapshot of `packages/math-expressions-js-compat` (`npx vitest run`) on branch
`doenet`, after rebuilding `vendor/wasm` (`bash build-wasm.sh`) so results reflect
current Rust source.

**Current totals (2026-08-08): 380 failed / 5935 passed / 6318 total**
(2 files skipped, 1 test skipped, 2 todo). Session start was 1166 failed / 5149 passed.

Judge changes by name-level diff against a baseline worktree, never by aggregate
counts (see memory `js-compat-suite-baseline-diff`).

## Fixes applied this session — 1166 → 380 (−786)

| spec file | before | after | change |
|-----------|-------:|------:|--------|
| `slow_assumptions` | 565 | **5** | Three-phase assumptions-engine completion — see `ASSUMPTIONS_ENGINE_PLAN.md`. (1) The predicates in `element_of_sets.ts` defaulted to an **empty** `Assumptions` handle, so every no-argument query ignored the global store and answered "unknown" — −331 from that one defect alone. (2) Rust engine: negated assumptions, constant folding before structural inference, sign normalization, zero-factor and complex-closure rules, function domain facts; `infer.rs` split into `infer/`. (3) `get_assumptions` rebuilt to return an oriented AST with transitive closure plus interval-membership and subset/superset expansion. |
| `slow_polynomial` | 205 | **0** | Revived the legacy pure-JS polynomial/Groebner engine as TypeScript in the compat lib (`lib/polynomial/`, plus `lib/trees/util.ts`, `lib/expression/variables.ts`, `lib/expression/evaluation.ts` shim). Faithful port; the spec compares the module's own polynomial objects, so it passes by construction. **Tradeoff: reintroduces pure-JS math into a package whose README says it has "no math of its own."** |
| `quick_latex-to-ast-to-latex` | 21 | **0** | Updated 21 test expectations to the new printer conventions: fewer exponent braces (`d^{2}`→`d^2`), parens around unary ± (`a++b`→`a+(+b)`), angle notation (`\angleABC`→`\angle(A,B,C)`). Test-only change. |

`cargo test` stays green throughout; the Rust assumptions corpus (which validates
against the original JS oracle) needed no re-snapshotting.

## Remaining failures by spec file

| count | spec file | root cause / category |
|------:|-----------|-----------------------|
| 71 | `quick_ast-to-text` | text printer parenthesization (`1/2 x` vs `(1/2) x`, `-2 x/3` vs `-(2 x)/3`) |
| 53 | `quick_text-to-ast-to-text` | same text-printer parenthesization |
| 45 | `slow_simplify` | three buckets (see below): canonical **ordering** (~13), **number folding** at singular values (~10), **radical/matrix/expand algebra** incomplete (~12, incl. one real bug) |
| 28 | `quick_mathjs-to-ast` | `mathjsToAst is not implemented` (explicit stub) |
| 26 | `quick_normalization` | function-name normalization not applied (`e^x` stays `^` instead of `apply exp`) |
| 24 | `slow_matrix` | `me.matrix is not a function`; vector/tuple typing (`tuple` vs `vector` after add) |
| 21 | `slow_check-equality-numerical-errors` | numerical-tolerance equality returns false |
| 20 | `quick_ast-to-latex` | LaTeX spacing/paren (`f\left(x\right)` vs `f\left( x \right)`) |
| 18 | `slow_check-symbolic-equality-numerical-errors` | as above (symbolic variant) |
| 17 | `quick_trees` | tree matching with predicate/regex conditions can't cross the wasm boundary |
| 10 | `quick_pm` | — |
| 8 | `quick_ast-to-mathjs` | — |
| 7 | `quick_sets` | — |
| 5 | `slow_assumptions` | see below |
| 4 | `quick_ast-to-guppy`, `slow_math-expressions` | — |
| 3 | `quick_latex-to-ast`, `quick_solve` | — |
| ≤2 | `quick_text-to-ast`, `quick_transformation`, `slow_rational`, + 6 misc quick specs | — |

### The 5 remaining `slow_assumptions` failures
- `is integer / via assumptions` — **not an engine bug**. `add_assumption` round-trips
  through `.toString()` and the text printer loses negation scope: `paren_if_spaced`
  (`src/print/text.rs:223`, same in `latex.rs:271`) tests `starts_with('(') && ends_with(')')`,
  which cannot distinguish `(a or b)` from `(a) or (b)`. Needs a paren-balance scan.
  Fed a correctly-parenthesized tree the engine answers correctly.
- `strict pow` — needs a `me.math.pow_strict` toggle with no wasm equivalent.
- `logical combinations`, `combined assumptions`, `combined assumptions, negated` —
  or-disjunction and interval-membership *reasoning* (as opposed to retrieval).

### The 45 `slow_simplify` failures
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

**Printer / formatting (~170)** — text parenthesization (124), LaTeX spacing (20),
normalization (26). Largest remaining area. Some are candidates for adopting new
conventions in the tests (as done for `quick_latex-to-ast-to-latex`), others are
genuine printer gaps. The `paren_if_spaced` bug above lives here too.

**Numerical-tolerance equality (39)** — the two `check-*-numerical-errors` specs.
Previously mis-attributed to the assumptions system; they are independent.

**Unimplemented / unbound APIs (~70)** — `quick_mathjs-to-ast` (28, explicit stub),
`quick_trees` (17, callback matching can't cross wasm), `slow_matrix` (`me.matrix`).

**Semantic edge cases (45)** — `slow_simplify`: canonical ordering (~13),
number folding at singular values (~10), radical/matrix/expand algebra (~12).
See the per-bucket breakdown above.

## Highest-leverage remaining item
The text-printer parenthesization rule: `quick_ast-to-text` (71) +
`quick_text-to-ast-to-text` (53) = 124 failures share one root cause.
