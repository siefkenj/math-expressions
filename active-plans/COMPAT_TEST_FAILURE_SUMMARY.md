# js-compat test failure summary

Snapshot of `packages/math-expressions-js-compat` (`npx vitest run`) on branch
`doenet`, after rebuilding `vendor/wasm` (`bash build-wasm.sh`) so results reflect
current Rust source.

**Current totals (2026-08-10, at `f84577c` + the two equality fixes below):
61 failed / 6263 passed / 6327 total** (1 test skipped, 2 todo). Previous
snapshots: 380, 162, 97, 83, 82.

`slow_check-equality-numerical-errors` is now at **zero** (was 21).

This snapshot is reproducible from a clean tree: run `bash build-wasm.sh` then
`npx vitest run`, both from inside `packages/math-expressions-js-compat`. The
old (97) snapshot was **not** — it was taken with uncommitted work in the tree,
which is why its per-file counts drifted from anything checked out. Rebuild the
wasm first or the run measures the previous engine; that alone accounted for a
4-test discrepancy while an earlier snapshot was being taken, and `build-wasm.sh`
lives in the package, not at the repo root.

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
|    18 | `slow_check-symbolic-equality-numerical-errors` | a perturbed exponent reorders a sum; the fuzzy compare is pairwise — see below           |
|    17 | `quick_trees`                                   | tree matching with predicate/regex conditions can't cross the wasm boundary              |
|    12 | `slow_simplify`                                 | **sign placement**, **folding without assumptions**, **ordering**, algebra — see below   |
|     5 | `slow_assumptions`                              | see below                                                                                |
|     4 | `slow_math-expressions`                         | equality of containers/unions, an integer assumption, one derivative identity            |
|     3 | `quick_solve`                                   | `solve_linear()` unimplemented                                                           |
|     2 | `slow_rational`                                 | `e` is a polynomial *coefficient*, so `e+f` is not seen as a factor — see below          |

### The `*-numerical-errors` failures — 39 investigated, 21 fixed, 18 left

Investigated 2026-08-10 against the legacy library run as a live oracle
(`legacy-js-oracle-runnable`). **Every** failing block sets
`include_exponents: true` — the option that lets the number allowance reach into
an exponent. 38 of the 39 shared one root cause, and it was not the tolerance
arithmetic:

> The canonical form of a tree is decided by the *values* of the numbers in it,
> and the fuzzy comparison then blurs those same numbers at the leaves only. So a
> perturbation small enough to be forgiven at a leaf is still large enough to
> change the tree's shape, and `fuzzy_tree_eq` — which is structural and
> order-sensitive — never gets the chance to forgive it.

Two spellings of that, one per file. The first is fixed; the second is not.

#### FIXED — the root spelling (was 20 tests)

The engine keeps **two canonical forms for the same value** —
`canonicalize`/`simplify_canonical` leave `sqrt(q)` an `Apply` *and* leave
`q^(1/2)` and `q^0.5` as `Pow` (all three verified unchanged through `simplify`).
`sqrt(q).equals(q^0.5)` was `true` only because sampling rescued it. Add a
tolerance and the rescue disappears: the response's exponent is no longer exactly
½, so it is pinned to `Pow` while the key stays `Apply`.

The comparison then died on a variant tag, not on a number. Walking
`exp(0.01xy+1000sqrt(q))` against
`exp((0.01+.00002)xy+(1000+.00002)q^(0.5+.00002))`, `fuzzy_tree_eq` accepted
`0.01`/`0.01002` and `1000`/`1000.00002` (both 2e-5 against a 1e-4 allowance)
and then failed at `fuzzy.rs:40` on `discriminant(Apply) != discriminant(Pow)`.
Every number was inside tolerance; the shape was not, and shape is compared
exactly. Sampling could not recover either — `replace_numbers` matches
`Expr::Num`, and `sqrt(q)` contains none, so its tolerance had no
∂f/∂(exponent) term at all.

**The invariant it violated:** `equalsViaSyntax` — a deliberately *weaker* form
check — returned `true` on the exact pair where `equals` returned `false`. Form
equality should imply value equality. The two entry points normalize
differently: `equals_syntactic` runs `normalize_syntactic`, whose pass 1 rewrites
`sqrt(x) → x^(1/2)` (`syntactic.rs:44`); `equals` did not. Legacy has no such
gap — its `equals` calls `equalsViaSyntax` on a `normalize_function_names`-folded
pair, so it has one spelling for a root and reaches it before comparing.

**Fix** (`equality/api.rs`): under a number allowance only, if the fuzzy
structural compare fails, retry once on `canonicalize(normalize_syntactic(·))` of
both sides. That is the same reconciliation the JS chain performs, and the
re-canonicalize is needed because the pass emits `Div(1, n)` exponents for
`cbrt`/`nthroot` that must fold to a `Num` before a number-leaf comparison can
see them. Gated on the tolerance because without one, stage 3 already decides
these pairs correctly.

**Rejected alternative:** making `canonicalize` fold roots to powers. See the
"Highest-leverage" section — the oracle keeps the two spellings distinct too, and
`ops/transforms.rs:122` records that as a deliberate decision.

#### STILL OPEN — term order (all 18)
Every failure perturbs an exponent *downward* — `2*(1-.00009)` or `(2-.00009)`;
not one upward case fails. Terms are graded-lex by total degree, so `1000q^1.99991`
has degree 1.99991 and falls behind `0.01xy` at degree 2, while `1000q^2.00009`
stays in front. `fuzzy_tree_eq` compares children pairwise in order and does not
re-match, so the reordered sum mismatches. Controls: the same perturbation
compared bare (`1000q^2` vs `1000q^1.99991`) passes, and so does the same sum
with a degree-1 companion, where the flip cannot happen. Legacy's sort key does
not read the exponent's value — it keeps `1000q^…` first at 1.99991, 2, and
2.00009 alike.

### FIXED — the `-1` parameter bug (grading was too lenient)

The 39th failure in that group pointed the *other* way and had a different
cause. "at least one of … is incorrect" for `10 exp(7x^2/(3-sqrt(y)))` was a
**false accept**: all eight answers perturbed by 2e-4 against a 1e-4 allowance
graded as correct, where legacy rejects all eight.

Not the region search, as first guessed — the tolerance *value*. Instrumented at
the accepting sample point, ours was `2.38e123` where legacy's was `5.59e122`,
**4.26× larger**, with the true difference `1.13e123` falling between them.
The parameter lists explain it exactly:

```
legacy:  par1=10, par2=7, par3=3
ours:    par1=10, par2=7, par3=3, par4=-1     <-- spurious
         par1 exp(par2 x^2 (par3 + par4 sqrt(y))^(-1))
```

`canonicalize` spells `3 - sqrt(y)` as `3 + (-1)·sqrt(y)`, and `replace_numbers`
took that structural `-1` for a number the author had typed — so the tolerance
carried a `∂f/∂(-1)` term, "what if the minus sign were 0.01% more negative".
Here that term *dominated*. Legacy never had the problem: its `-` is a unary
node containing no number.

Fix in `equality/fuzzy.rs::replace_numbers`: a factor of exactly `-1` inside a
`Mul` is a sign, not a magnitude, and is not parameterized. (`-2x` still
parameterizes its `-2`.) This is JS parity, not a divergence.

**Generality — worth noting.** The bug inflated the grading tolerance for *any*
expression containing a subtraction, always in the accept-too-much direction.
Only one compat test happened to catch it. Verified: 0 regressions across the
6327-test suite, 77 cargo suites green, clippy clean, and the
`tolerance-known-failures.json` snapshot shrank by exactly this one entry.

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

These add to 61.

**Numerical-tolerance equality (18)** — all now in
`slow_check-symbolic-equality-numerical-errors`, one cause, broken down above.
Was 39; the root-spelling and `-1` fixes closed 21.

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


## Highest-leverage remaining item

`quick_trees` (17) is now the largest bucket, and it is a single binding
problem, not seventeen: the matcher takes JS predicate and `RegExp` conditions,
which cannot cross the wasm boundary. It needs either a callback bridge or a
Rust-side condition vocabulary.

Next after that is the remaining numerical-errors bucket:

1. **18 tests** — make the fuzzy comparison of a commutative `Add`/`Mul`
   order-insensitive (match children as a multiset under the same fuzzy
   predicate) instead of pairwise, gated on `allowed_error_in_numbers > 0` so
   `equals_syntactic`'s documented order-sensitivity survives at the default.
   Re-sorting under tolerance is not well-defined — `1.99991 < 2` is a true fact
   about the key — so the comparison, not the order, is the thing to fix.
   Bipartite matching, not greedy: fuzzy equality is not transitive.

### Rejected: folding roots to powers in `canonicalize`

Recorded so it is not relitigated. The 20 root-spelling failures were fixed in
`equals` instead. Changing the canonical spelling was scoped and rejected:

`ops/transforms.rs:122` records keeping `sqrt(x)` and `x^(1/2)` as distinct
canonical trees as a deliberate decision, and the oracle backs it — legacy also
keeps them distinct in `.tree` and in printed output, folding to a power only
inside the explicit `normalize_function_names` pass:

```
legacy  fromText("sqrt(q)").tree  ->  ["apply","sqrt","q"]    prints sqrt(q)
legacy  fromText("q^(1/2)").tree  ->  ["^","q",["/",1,2]]     prints q^(1/2)
```

Making `canonicalize` fold roots would break `q^(1/2)` round-tripping (the two
become one tree and must therefore print alike), move roots from the `Apply` to
the `Pow` rank in both comparators, cost `sqrt(8) → 2√2` unless
`rule_radical`'s numeric-base-only `Pow` arm is generalized, drop `sqrt` off its
dedicated `z.sqrt()` / `FixId::Sqrt` kernels onto generic `powc` (branch-cut
risk), and require regenerating ~130 fixture entries. The gap was in `equals`, so
it was fixed in `equals`.

A note for whoever picks these up: several past fixes turned out to be *bindings*
for engine code that already existed and was already exercised elsewhere. Before
implementing anything that looks like a missing feature here, grep the Rust core
for it first.
