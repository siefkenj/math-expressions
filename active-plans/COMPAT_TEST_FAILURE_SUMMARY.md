# js-compat test failure summary

Snapshot of `packages/math-expressions-js-compat` (`npx vitest run`) on branch
`doenet`, after rebuilding `vendor/wasm` (`bash build-wasm.sh`) so results reflect
current Rust source.

**Current totals (2026-08-10, at `f84577c` + the two equality fixes below + the
`trees/basic` port + the `match`-condition wontfix + the three presentation
fixes and five adopted divergences below): 38 failed / 6279 passed / 6329 total** (10 skipped, 2 todo).
Previous snapshots: 380, 162, 97, 83, 82, 61, 55, 54, 46, 43.

The immediately preceding baseline measured **46**, not the 45 recorded in an
earlier edit of this file; the 43 above is from a matched pair of runs (same
tree, same wasm build) diffed by name, so the −3 is the reliable figure whatever
the absolute count.

Nine of the ten skips are new, and are **wontfix, not pending** — the deprecated
`match` conditions under `quick_trees` below. Read 54 → 46 as a reclassification,
not nine defects fixed.

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
|     2 | `quick_trees`                                   | `allow_extended_match`, and one throw-vs-`false` (+9 skipped wontfix) — see below        |
|     4 | `slow_simplify`                                 | container ordering (2), `exp` not seen as a power, unit-group order — see below           |
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

### The 4 `slow_simplify` failures

Re-measured 2026-08-10 by dumping our tree against the legacy oracle for every
assertion (`legacy-js-oracle-runnable`). Three of the buckets were previously
misattributed. **A caution the measurement taught:** these tests bundle 5–20
assertions each, so a per-test bucket count is an upper bound on what any one
fix buys. Attributing a test to the first assertion that fails hides everything
behind it — the sign fix below, taken alone, moved three tests' failure point
*forward* into unrelated bugs without turning any of them green. It took all
three presentation fixes together to close them.

- **FIXED — sign placement.** `present_mul`
  ([`normalize/present.rs`](../packages/math-expressions-rs/src/normalize/present.rs))
  ended by wrapping a negative product in `Expr::Neg` unconditionally. The JS
  puts the sign on the leading numeric coefficient when there is one, and uses
  `Neg` only when there is nowhere else for it to go (`−x`):

  | input | was | now (= legacy) |
  | --- | --- | --- |
  | `4(x)(-2)` | `["-",["*",8,"x"]]` | `["*",-8,"x"]` |
  | `x-2uv` | `["-",["*",2,"u","v"]]` | `["*",-2,"u","v"]` |
  | `x-2/(uv)` | `["-",["/",2,["*","u","v"]]]` | `["/",-2,["*","u","v"]]` |
  | `-24x⁶` under `cbrt` | `["-",["*",2,x²,cbrt3]]` | `["*",-2,x²,cbrt3]` |

  The rule already existed — `normalize_negative_numbers` in
  [`normalize/default_order.rs`](../packages/math-expressions-rs/src/normalize/default_order.rs),
  which `default_order` runs and `present` did not. `present`'s operand is
  already presented, so it needs only the leading-coefficient case, not that
  function's recursion; `carry_sign`/`negate_leading_number` is that narrower
  form.

  Four Rust expectations were updated to the legacy shape, two of which had
  comments asserting the `Neg` form was correct — it was not; `-2(x+y)` gives
  `["*",-2,["+","x","y"]]` in alpha94, verified against the pinned library.

- **FIXED — powers of `i` not folded.** `i*i` gave `["^","i",2]` where legacy
  gives `-1`, and `i*i*i` gave `["^","i",3]` against `["-","i"]`.
  `fold_imaginary_power` already existed and was already correct; it was only
  reachable from `fold_special_values`, which `evaluate_numbers` does not run.
  `fold_i_powers_tree` in
  [`ops/numbers.rs`](../packages/math-expressions-rs/src/ops/numbers.rs) applies
  it bottom-up in the `evaluate_numbers` pipeline, re-canonicalizing afterwards
  so a surrounding product absorbs the `−1` (`2i·3i` = `6·i²` → `−6`).

  Folding `i^n` is arithmetic on a number, not a symbolic identity — the
  integer powers of the imaginary unit are exact and have no branch cut — so it
  belongs in a pass that claims to evaluate numbers.

- **FIXED — `Add` term order (coefficient tie-break).** `present_add` sorted by
  `DegKey` (descending total degree, then graded-lex) and is *stable*, so
  same-degree terms kept canonical order. Legacy breaks those ties by
  coefficient, **ascending, with symbolic coefficients last**:

  | input | was | now (= legacy) |
  | --- | --- | --- |
  | `1x²-3+0x²+4-2x²-3+5x²` | `x², -2x², 5x², -2` | `-2x², x², 5x², -2` |
  | `x² + f(t)x² + 2x²` | — | `x², 2x², f(t)x²` |
  | `$3 + 2` | — | `2, $3` |

  `coeff_key`/`coeff_order` in `present.rs`. A bare monomial counts as
  coefficient 1, which is what puts `x²` *between* `−2x²` and `5x²` instead of
  at one end. The tie-break only fires for terms with an identical monomial
  signature, so it never competes with the degree ordering.

  **Symbolic-last was measured, not assumed.** The instruction that prompted
  this work said symbolic coefficients should sort *first*; the oracle puts them
  last (`x²+f(t)x²+2x²` → `x², 2x², f(t)x²`, and `$3+2` → `2, $3`), and
  symbolic-first immediately broke `unlike_units_and_bare_scalars_never_combine`.
  Confirmed with the user before switching to the legacy order.

**Net for the three fixes together: 46 → 43, three tests green** ("unary minus
of product", "multiplication", "combination"), **0 regressions**, verified by a
name-level diff over a matched pair of runs.

#### Adopted divergences — our behaviour is canonical (5 tests)

Decided 2026-08-10. These are engine policy this project has chosen, not
defects; the spec expectations were updated to our output with the reason
recorded inline at each site (`DIVERGENCE (adopted):` in
`spec/slow_simplify.spec.ts`). **5 tests green, 0 regressions.** Each was
confirmed against the oracle first, so what was adopted is a known difference,
not an unexamined one.

| test | ours (adopted) | alpha94 |
| --- | --- | --- |
| "unary minus of quotient" | `x-2u/v` → `x, (-2u)/v` | `(-2u)/v, x` |
| "division" | `0/x` → `0` | `["/",0,"x"]` |
| "power" | `x^0` → `1` | `["^","x",0]` |
| "like factors with assumptions" | `y/y/y²` → `1/y²` | `y/y³` |
| "to decimals" | `(1/2)i` → `["/","i",2]` | `["*","i",["/",1,2]]` |

Two policies cover all five:

- **Sum order is the polynomial reading.** Terms sort by descending total
  degree, so `x` (degree 1) precedes `-2u/v` (degree 0: `u¹v⁻¹`). Legacy's
  comparator puts the quotient first, which no visible rule in its source
  explains; matching it would mean special-casing a quotient to outrank a
  higher-degree term. The same key orders every other sum in the suite.
- **Fold on the generic branch.** `0/x`, `x^0` and `y/y` all reduce without a
  `≠ 0` assumption in hand — the removable singularity is not carried through
  every later pass. Already the documented behaviour for `x/x → 1`; adopting
  these three makes the policy uniform instead of applying to one case. The
  post-assumption expectations in those tests are unchanged, so the two engines
  still agree wherever alpha94 has the assumption.

The fifth is the rational-coefficient split: `(1/2)i → i/2` is the same rule
that gives `(2/3)x⁻¹ → 2/(3x)`, and exempting a numerator of 1 would make the
presentation depend on the coefficient's value. The decimal-spelled sibling
(`0.5i`) is unaffected — a decimal never moves under a bar.

- **Container ordering — 2 tests.** The two "sorted the same" cases. Legacy's
  sort key is *(component count, then component values)*, with container type
  not in the key at all, so containers interleave:
  `[1,6] [1,9) [9,5) [9,8] [0,4,4]`. We sort by container type first, then by
  value within each type: `[0,4,4] [1,6] [9,8] [1,9) [9,5)`.

- **`exp` not treated as a power of `e` — 1 test.** "treat exp like power".
  `collect_like_terms_factors` handles `e^3·e^5 → e^8` but leaves
  `exp(3)·exp(5)` and `exp(3)/exp(5)` completely uncollected — the `Apply` form
  is never recognised as a power. (This test also trips the sign bug above.)

- **Unit-group ordering — 1 test.** "with units". Our
  `collect_like_terms_factors` emits `$, deg, %`; legacy emits `$, %, deg`.
  Note our *own* `default_order()` produces the legacy order on the same
  expression, so this is an internal inconsistency between the two paths, not a
  missing comparator.

## Remaining buckets by theme

**Numerical-tolerance equality (18)** — all now in
`slow_check-symbolic-equality-numerical-errors`, one cause, broken down above.
Was 39; the root-spelling and `-1` fixes closed 21.

**Unimplemented / unbound APIs (5)** — `quick_trees` (2, below) and
`quick_solve` (3, `solve_linear`).

### The `quick_trees` failures — 17 investigated, 6 fixed, 9 wontfix, 2 left

An earlier note called this "a single binding problem, not seventeen". That was
wrong; it was three problems, and only nine were the binding one. The other
eight were **missing functions**, now ported:

- `replaceSubtree`, `traverse`, `transform`, `applyAllTransformations`,
  `applyTransformationEachSubtree`, `patternTransformer`,
  `equalAfterTransformations` — legacy `trees/basic.js:603-828`, absent from the
  port's 26-line `lib/trees/basic.ts`. Pure tree rewriting over the raw ASTs,
  nothing to do with the wasm boundary. **6 tests fixed.**
- `Expression.collapse_unary_minus()` — legacy `expression/simplify.js:34`.
  Ported and checked against the live oracle on 11 inputs, identical on all of
  them. Its own test is skipped, for the predicate reason below.

None of the eight has a single call site anywhere in DoenetML (grepped
`tmp/DoenetML`), so this is spec-compat only.

#### WONTFIX — arbitrary per-parameter `match` conditions (9 tests, now skipped)

Legacy let `variables` map a parameter to a **predicate function** (8 tests) or
a **`RegExp`** (1). [`VarKind`] is the closed replacement and the open forms are
deprecated; the specs are `it.skip`ped with a `[wontfix: …]` name prefix and a
block comment, so they stay as the record of what legacy accepted rather than
sitting in the failing bucket forever.

Two reasons, and the second is the real one:

1. A predicate would be called back into JS once per **candidate** binding —
   the matcher backtracks, so the call count is a function of the search, not of
   the input. It would stop being a pure Rust search.
2. **Nobody needs it.** DoenetML's `<matchesPattern>`
   ([`MatchesPattern.js:259-274`](../tmp/DoenetML/packages/doenetml-worker-javascript/src/components/MatchesPattern.js#L259-L274))
   is the only real consumer and passes exactly two closures —
   `(m) => !Number.isNaN(me.fromAst(m).evaluate_to_constant())` under
   `requireNumericMatches`, `(m) => typeof m === "string"` under
   `requireVariableMatches`. Those *are* `VarKind::Number` and
   `VarKind::Variable`. No `RegExp` condition appears anywhere in DoenetML.

The declarative form is also sharper: `Number` means "evaluates to a real
numeric constant", where the legacy specs' hand-written `typeof s === "number"`
quietly rejected `π`.

Cost of the skip, and how it was covered: every legacy test exercising
`allow_permutations` and `allow_implicit_identities` *also* declared its
parameters with predicates, and those two options are supported and are what
Doenet passes. Skipping blind would have left both with **zero** coverage in the
suite. Two replacement tests ("… with parameters declared by kind") re-express
the same scenarios with declared kinds, including a negative control confirming
the kind is what fails the match (`e^(0.3s^2+3s+q)` matches under `true`, not
under `"number"`).

#### Still open — 2 tests

- **Throwing where legacy returned `false` — 1 test**, named "invalid matching
  conditions fail gracefully". Not part of the wontfix: the condition here is
  `{a: false, b: "h"}`, neither a predicate nor a regex. Legacy's
  `else { return false }` makes any unrecognized condition simply never match;
  [`interop.rs:172`](../packages/math-expressions-rs-wasm/src-rust/interop.rs#L172)
  reasons that `false` "is never what a caller means" and raises instead. The
  spec name states the contract. Cheapest item here.
- **`allow_extended_match` — 1 test**, "trig transformation". A gap, not a
  decision: unported in `js_match.rs`, and the matcher never emits the
  `_skipped`/`_skipped_before` bindings that go with it. (Two more tests need it,
  but they need predicates too and are skipped above.) `applyAllTransformations`
  already carries the splice logic for those bindings, so only the Rust side is
  missing.

**Semantic edge cases (10)** — `slow_simplify` (4, broken down above),
`slow_math-expressions` (4), `slow_rational` (2). No single root cause; these are
a scatter of individual normalization decisions rather than one bucket.

**Assumption reasoning (5)** — all `slow_assumptions`, listed above. Includes the
`paren_if_spaced` printer defect surfacing through an assumptions spec:
`src/print/text.rs` tests `starts_with('(') && ends_with(')')`, which cannot
distinguish `(a or b)` from `(a) or (b)`. It needs a paren-balance scan.


## Highest-leverage remaining item

The `*-numerical-errors` bucket. `quick_trees` is closed out: 6 fixed, 9 marked
wontfix, and the 2 left are small — "fail gracefully" (one arm in `interop.rs`)
and `allow_extended_match`.

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
