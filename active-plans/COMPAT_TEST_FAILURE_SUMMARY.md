# js-compat test failure summary

Snapshot of `packages/math-expressions-js-compat` (`npx vitest run`) on branch
`doenet`, after rebuilding `vendor/wasm` (`bash build-wasm.sh`) so results reflect
current Rust source.

**Current totals (2026-08-10, at `0155902`): 83 failed / 6241 passed / 6327 total**
(1 test skipped, 2 todo). Previous snapshots: 380, 162, 97.

This snapshot is reproducible from a clean tree at `0155902`: run
`bash build-wasm.sh` then `npx vitest run` from inside
`packages/math-expressions-js-compat`. The previous (97) snapshot was **not** —
it was taken with uncommitted matrix-pipeline and `evaluate_to_constant` work in
the tree, which is why its per-file counts drifted from anything checked out.
Rebuild the wasm first or the run measures the previous engine; that alone
accounted for a 4-test discrepancy while this snapshot was being taken.

Judge changes by name-level diff against a baseline worktree, never by aggregate
counts (see memory `js-compat-suite-baseline-diff`). When diffing, note that
several spec files contain **duplicate test names**, so keying a comparison by
name alone silently drops results — key by (file, name, occurrence). Some specs
also derive the test *name* from the expected string, so updating an expectation
renames its test; those show up as removed-plus-added rather than failed→passed,
and the honest check is that no name present in *both* runs went passing→failing.

## Remaining failures by spec file

| count | spec file                                       | root cause / category                                                                    |
| ----: | ----------------------------------------------- | ---------------------------------------------------------------------------------------- |
|    21 | `slow_check-equality-numerical-errors`          | canonical form keys on a number the tolerance is supposed to blur — see below            |
|    18 | `slow_check-symbolic-equality-numerical-errors` | as above (ordering variant)                                                              |
|    17 | `quick_trees`                                   | tree matching with predicate/regex conditions can't cross the wasm boundary              |
|    12 | `slow_simplify`                                 | **sign placement**, **folding without assumptions**, **ordering**, algebra — see below   |
|     5 | `slow_assumptions`                              | see below                                                                                |
|     4 | `slow_math-expressions`                         | equality of containers/unions, an integer assumption, one derivative identity            |
|     3 | `quick_solve`                                   | `solve_linear()` unimplemented                                                           |
|     2 | `slow_rational`                                 | `e` is a polynomial *coefficient*, so `e+f` is not seen as a factor — see below          |
|     1 | `quick_doenet_printer_and_rounding`             | `sqrt(-2)` is not folded to `i·sqrt(2)` — see below                                      |

### The 39 `*-numerical-errors` failures

Investigated 2026-08-10, against the legacy library run as a live oracle
(`legacy-js-oracle-runnable`). **Every** failing block sets
`include_exponents: true` — the option that lets the number allowance reach into
an exponent. 38 of the 39 share one root cause, and it is not the tolerance
arithmetic:

> The canonical form of a tree is decided by the *values* of the numbers in it,
> and the fuzzy comparison then blurs those same numbers at the leaves only. So a
> perturbation small enough to be forgiven at a leaf is still large enough to
> change the tree's shape, and `fuzzy_tree_eq` — which is structural and
> order-sensitive — never gets the chance to forgive it.

Two spellings of that, one per file.

**`slow_check-equality-numerical-errors` (20 of 21): the root spelling.**
`normalize::canonicalize` writes an exact ½ power as `sqrt(x)` (`simplify.rs:1273`)
but leaves `q^0.50002` a `Pow`, so the pair is structurally unequal for a
difference of 2e-5 against an allowance of 1e-4. Measured: `sqrt(q)` vs
`q^0.50002` is `false` under `equals` and `true` under `equalsViaSyntax`;
respelling the original as `q^0.5` makes the whole failing block pass. Once the
structural stage misses, sampling cannot recover it either — `replace_numbers`
sees no number inside `sqrt(q)`, so the tolerance it builds has no
∂f/∂(exponent) term at all.

Legacy has no such gap: its `normalize_function_names` folds `sqrt(q)` → `q^0.5`
(**verified by running it** — the reverse of what `ARCHITECTURE`-level notes here
implied), and `equals` calls `equalsViaSyntax` on that normalized pair. Our
`equals` uses `canonicalize`/`simplify_canonical` instead, which prefer the
`sqrt` spelling. `equals_syntactic` already normalizes the way legacy does, which
is exactly why it answers correctly on the same inputs.

**`slow_check-symbolic-equality-numerical-errors` (all 18): term order.**
Every failure perturbs an exponent *downward* — `2*(1-.00009)` or `(2-.00009)`;
not one upward case fails. Terms are graded-lex by total degree, so `1000q^1.99991`
has degree 1.99991 and falls behind `0.01xy` at degree 2, while `1000q^2.00009`
stays in front. `fuzzy_tree_eq` compares children pairwise in order and does not
re-match, so the reordered sum mismatches. Controls: the same perturbation
compared bare (`1000q^2` vs `1000q^1.99991`) passes, and so does the same sum
with a degree-1 companion, where the flip cannot happen. Legacy's sort key does
not read the exponent's value — it keeps `1000q^…` first at 1.99991, 2, and
2.00009 alike.

**The 21st failure is unrelated and points the other way.** "at least one of …
is incorrect" for `10 exp(7x^2/(3-sqrt(y)))` is a *false accept*: we return
`true` for all eight answers perturbed by 2e-4 against a 1e-4 allowance, where
legacy returns `false` for all eight. The syntax stage rejects them correctly, so
this is the sampling stage being too lenient — plausibly the two documented
`equals_numerical` divergences (a fixed `NEIGHBORHOOD_RADIUS` of 0.01 instead of
`scale/100`, and never advancing the binding scale). Not yet isolated.

### The `quick_doenet_printer_and_rounding` failure

`sqrt(-1)` folds to `i` and `sqrt(-4)` to `2i`, but `sqrt(-2)` stays written —
the spec wants `i·sqrt(2)`. So the negative-radicand path only fires when the
radicand is a perfect square; pulling `-1` out of a non-square is missing.

This file was at zero in the 97-snapshot, and the spec itself has not changed
since `264be80`, so this is either newly surfaced or was masked there by the
uncommitted work that snapshot carried. **Not bisected** — do that before
treating it as a regression.

### The 2 `slow_rational` failures — a constant-policy divergence

Both build `(f1·f2).expand() / (f3·f2).expand()` and expect it to reduce to
`f1/f3`. Isolated by toggling the policy:

| `f3`         | `define_e: true` (default) | `define_e: false` |
| ------------ | -------------------------- | ----------------- |
| `e+f`        | **fail**                   | pass              |
| `e+atan(z)`  | fail                       | fail              |
| `g+h` (control) | pass                    | pass              |

So the first failure is caused *entirely* by `e` being declared a constant.
`polynomials/compat/convert.rs` maps a declared constant to `Poly::Coeff`, so
`e+f` reads as a linear polynomial in `f` rather than a product-able factor in
two indeterminates, and the GCD never finds it. Rename `e` to `g` and it passes.

**This diverges from alpha94 on purpose but possibly wrongly.** Legacy's
`variables()` filter is `(math.define_e || v !== "e")`, which *keeps* `e` in the
variable list when `define_e` is on — so legacy's polynomial code treats `e` as
an indeterminate and factors `e+f` fine. Whether the Rust reader should follow it
is a real decision (a coefficient `e` is more correct algebraically; an
indeterminate `e` is what the specs were written against), not a bug to patch
blindly. See `SPECIAL_CONSTANTS_FLAG.md`.

The second failure is independent of the policy: with `e` undeclared the
transcendental case still fails, so `sin(y)`/`atan(z)` are not being admitted as
indeterminates either. Two causes, not one.

### The 5 remaining `slow_assumptions` failures

- `is integer / via assumptions` — **not an engine bug**. `add_assumption` round-trips
  through `.toString()` and the text printer loses negation scope: `paren_if_spaced`
  (`src/print/text.rs:223`, same in `latex.rs:271`) tests `starts_with('(') && ends_with(')')`,
  which cannot distinguish `(a or b)` from `(a) or (b)`. Needs a paren-balance scan.
  Fed a correctly-parenthesized tree the engine answers correctly.
- `strict pow` — needs a `me.math.pow_strict` toggle with no wasm equivalent.
- `logical combinations`, `combined assumptions`, `combined assumptions, negated` —
  or-disjunction and interval-membership _reasoning_ (as opposed to retrieval).

There is also one **skipped** test here, `define constants`, which the legacy
suite skipped with the note "although this passes, skip test as setting
`define_i`, etc., no longer changes mathjs". The Rust port *does* thread that
declaration all the way through (`src/constant_policy.rs`, surfaced as
`me.setConstantPolicy`), so this test is nearly enableable: 13 of its 14
assertions pass as written. The one that does not is `is_real(i·x)` given
`x ∈ ℝ`, which answers `undefined` where the spec wants `false` — a missing
inference (a product of a nonzero real and the imaginary unit is not real), not
a declaration problem. Fix that and the test can be un-skipped.

### The 12 `slow_simplify` failures

- **Sign placement (3)** — `evaluate_numbers` "multiplication", "unary minus of
  product", "unary minus of quotient". We emit a negation node wrapping the
  product, `["-", ["*", 8, "x"]]`, where the JS library folds the sign into the
  coefficient, `["*", -8, "x"]`. Mathematically equal, structurally different;
  one normalization rule, and the largest single bucket left in this file.
- **Folding without an assumption (2)** — `0/x → 0` and `x^0 → 1` fire with no
  `x ≠ 0` in hand. The spec wants both left written. Same class as the
  documented `x/x → 1` divergence.
- **Container ordering (3)** — `evaluate_numbers` "combination" and the two
  "sorted the same" cases: mixed tuple/vector/altvector/interval and
  array/interval unions group by container type rather than interleaving by
  value. Distinct from atom ordering, which is now correct.
- **Fraction vs decimal spelling (1)** — "to decimals" expects `1/3` to become
  `0.333…`; `is_written_as_fraction` now keeps it a fraction. This is a
  deliberate divergence adopted for DoenetML open item 8, so the spec is the
  thing that is out of date, not the engine.
- **Collect-like-terms algebra (2)** — "like factors with assumptions" and
  "treat exp like power".
- **Unit-group ordering (1)** — "with units". The atoms inside each unit group
  are correct; the groups are ordered differently. Separate from the container
  ordering bucket above.

## Remaining buckets by theme

These add to 83.

**Numerical-tolerance equality (39)** — the two `check-*-numerical-errors` specs,
broken down above. Not a tolerance-arithmetic bucket: 38 of them are canonical
form keying on the numbers the tolerance is meant to blur. The largest single
area by a wide margin.

**Unimplemented / unbound APIs (20)** — `quick_trees` (17, predicate/regex
callback matching cannot cross the wasm boundary) and `quick_solve` (3,
`solve_linear`).

**Semantic edge cases (18)** — `slow_simplify` (12, broken down above),
`slow_math-expressions` (4), `slow_rational` (2). No single root cause; these are
a scatter of individual normalization decisions rather than one bucket.

**Assumption reasoning (5)** — all `slow_assumptions`, listed above. Includes the
`paren_if_spaced` printer defect surfacing through an assumptions spec:
`src/print/text.rs` tests `starts_with('(') && ends_with(')')`, which cannot
distinguish `(a or b)` from `(a) or (b)`. It needs a paren-balance scan.

**Radical folding (1)** — `sqrt(-2)`, described above.

## Highest-leverage remaining item

The two `check-*-numerical-errors` specs (39) are **47% of all remaining
failures**, and the investigation above reduces them to one idea in two places:
under a number allowance, the comparison must not be made against a canonical
form that those same numbers selected. Concretely, two independent changes,
either of which can land alone:

1. **20 tests** — give `equals` the normalization legacy gives it, so both
   spellings of a root meet. `equals_syntactic` already does this and already
   answers correctly; the cheapest version is to route the fuzzy structural stage
   through the same `normalize_syntactic` rather than to change what
   `canonicalize` considers canonical. (Changing the canonical spelling of roots
   would fix it too, and is the wrong lever — wide blast radius, and the `sqrt`
   spelling is load-bearing elsewhere.)
2. **18 tests** — make the fuzzy comparison of a commutative `Add`/`Mul`
   order-insensitive (match children as a multiset under the same fuzzy
   predicate) instead of pairwise. Re-sorting under tolerance is not well-defined
   — `1.99991 < 2` is a true fact about the key — so the comparison, not the
   order, is the thing to fix.

The remaining 1 is a genuine over-acceptance in the sampling stage and is
independent of both.

After that, `quick_trees` (17) is a single binding problem, not seventeen: the
matcher takes JS predicate and `RegExp` conditions, which cannot cross the wasm
boundary. It needs either a callback bridge or a Rust-side condition vocabulary.

A note for whoever picks these up: several past fixes turned out to be *bindings*
for engine code that already existed and was already exercised elsewhere. Before
implementing anything that looks like a missing feature here, grep the Rust core
for it first.
