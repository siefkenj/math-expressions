# js-compat test failure summary

Snapshot of `packages/math-expressions-js-compat` (`npx vitest run`) on branch
`doenet`, after rebuilding `vendor/wasm` (`bash build-wasm.sh`) so results reflect
current Rust source.

**Current totals (2026-08-11, at `62c5f20` + uncommitted working-tree work +
the `solve_linear` binding and the `=`/`≠` operand-order fix below):
12 failed / 6305 passed / 6329 total** (10 skipped, 2 todo).
Previous snapshots: 380, 162, 97, 83, 82, 61, 55, 54, 46, 43, 38, 20, 16, 13.

Both `*-numerical-errors` files are now at **zero**, and so are `slow_simplify`
and `slow_rational`.

**Two independent efforts landed in this tree at once** — the `solve_linear`
binding and the polynomial work (`polynomials/kernel.rs`, `ratform.rs`,
`multivariate.rs`, `ops/numbers.rs`). Treat 16 → 12 as their sum, never as
either one's. They fix **disjoint** tests and the arithmetic is additive: 16 with
neither, 14 with either alone, 12 with both.

Each is attributable only because it was measured as a matched pair — same tree,
same wasm build, one change set at a time, diffed by (file, name, occurrence) —
and none showed a test go passing→failing:

| change set | pair | attributable |
| --- | --- | --- |
| `solve_linear` binding | (of the 14 → 12 below) | `quick_solve` "nonlinear doesn't work" |
| `=`/`≠` operand order | (of the 14 → 12 below) | `quick_solve` "linear equation" |
| both of the above, together | 14 → 12 | those 2 `quick_solve` tests |
| polynomial / kernel work | 14 → 12 | both `slow_rational` |

The two `solve_linear` rows were split on a tree that was still moving, so read
the split as the weaker claim; the **14 → 12 for the pair of them** was measured
on a quiet tree by reverting exactly the six files that change set touches
(`lib/math-expressions.ts`, `src-js/wasm.ts`, `src-rust/assumptions.rs`,
`grade/linear.rs`, `normalize/canonicalize.rs`, `normalize/default_order.rs`),
rebuilding, and running both halves back to back: 2 fixed, 0 regressions, 0
renames. The 38 → 20 and 20 → 16 steps were measured the same way.

**A matched pair means the two runs differ by the change set and nothing else.**
The polynomial row was first measured against a baseline captured before the
`solve_linear` binding landed, and consequently over-claimed: it appeared to fix
two `quick_solve` tests that were in fact the binding's. A later A/B on
`MAX_INDETERMINATES` was confounded the same way, in the opposite direction —
edits arrived between the two wasm builds and made a resource guard look like a
correctness fix. Re-running with only the polynomial files set aside gave the 14
→ 12 above. On a tree someone else is editing, a baseline goes stale in minutes;
capture both halves back to back or the diff is measuring the other person.

That both change sets measure 14 → 12 is not a contradiction and not
double-counting — it is two different pairs sharing an endpoint. Each was run
against a baseline holding the *other* change set, so 14 is "everything but
mine" in one case and "everything but theirs" in the other, and the 12 is the
same tree both times. The check that they are genuinely disjoint is that neither
diff names a test the other's does.

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
|     2 | `quick_trees`                                   | `allow_extended_match`, and one throw-vs-`false` (+9 skipped wontfix) — see below        |
|     5 | `slow_assumptions`                              | see below                                                                                |
|     4 | `slow_math-expressions`                         | equality of containers/unions, an integer assumption, one derivative identity            |
|     1 | `quick_solve`                                   | `simplify` picks its `Neg`/`Div` sign placement from the input spelling — see below      |

### The `*-numerical-errors` failures — 39 investigated, 39 fixed

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

Two spellings of that, one per file. Both are now fixed.

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

#### FIXED — term order (was 18 tests)

Every failure perturbed an exponent *downward* — `2*(1-.00009)` or `(2-.00009)`;
not one upward case failed. That asymmetry is the tell: nothing in the tolerance
arithmetic is direction-dependent, so the sort had to be the culprit.

**Which sort, precisely** — an earlier draft of this section said "legacy's sort
key does not read the exponent's value". That is wrong, and the correction is the
whole diagnosis. There are two sum orderings in this tree:

- **Canonical** (`default_order.rs sort_sum_terms`) keys on a **per-variable
  exponent vector** over alphabetized names. It *does* read the exponent value,
  but `q`'s 1.99991 only ever competes against the other term's `q` exponent,
  which is 0. A perturbation cannot cross that. Faithful port of legacy, which
  has no other ordering — hence the oracle keeping `1000q^…` first at 1.99991,
  2, and 2.00009 alike.
- **Presentation** (`present.rs deg_key`) collapses to a single `total: f64`.
  Now `q`'s 1.99991 competes against `xy`'s 1+1 = 2, and 9e-5 is enough to cross.

`simplify()` returns `present(simplify_core(…))` (`simplify.rs:73`), so the
presented order is baked into the compared value, not just into printing. The
presentation layer is ours; legacy has none.

Controls, all consistent with total-degree and none with per-variable: the
perturbation compared bare passes; a degree-1 companion (`0.01x`) passes; a
degree-3 companion (`0.01xyz`) passes; only the exact degree-2 tie fails.

**Fix** (`equality/fuzzy.rs`): when the ordered child compare fails on an `Add`
under a nonzero `allowed_error_in_numbers`, re-match the terms as a bipartite
matching under the same fuzzy predicate. The rule it encodes: *a comparison that
forgives ε in a number must not depend on an ordering derived from that number.*

Three scope decisions, each load-bearing:

- **`Add` only, not `Mul`.** Multiplication is not commutative here — matrices —
  so re-matching factors would grade `AB` equal to `BA`. Addition is commutative
  unconditionally.
- **Only under a tolerance.** With none set the order is a function of numbers
  being compared exactly, so it cannot drift, and `equals_syntactic`'s documented
  order-sensitivity (`(x+y)+z` ≠ `z+x+y`) survives untouched on the default path.
- **Matching, not greedy first-fit.** Fuzzy number equality is not transitive, so
  greedy can fail on operands a perfect matching would pair. Kuhn's
  augmenting-path, capped at 32 terms.

Because it only ever runs *after* the ordered compare has failed, the change is
monotone — it can turn `false` into `true` and never the reverse. The only
regression it could produce is an expects-`false` test flipping; the name-level
diff shows none.

**Rejected alternative:** fixing the sort — giving `present.rs deg_key` the
per-variable vector canonical already uses. That is the parity-restoring change
and would keep `equals_syntactic` exactly as strict, but `present` exists on
purpose (`present.rs:363` records the `a·e + b·f` case that motivated the current
key) and its blast radius is every printed sum in the suite. The comparison was
the smaller and better-argued surface.

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

### FIXED — `reduce_rational` refused its own inputs (was 2 tests)

**Both pass.** Measured as a matched pair against the same tree and wasm build,
with only `polynomials/kernel.rs`, `polynomials/{mod,multivariate,ratform}.rs`,
`ops/numbers.rs` and `tests/reduce_rational.rs` set aside for the baseline:
14 → 12, exactly these two names, and **no** test going passing→failing.

Both build `(f1·f2).expand() / (f3·f2).expand()` and expect `f1/f3`. Isolated by
toggling the constant policy:

| `f3`         | `define_e: true` (default) | `define_e: false` |
| ------------ | -------------------------- | ----------------- |
| `e+f`        | **fail**                   | pass              |
| `e+atan(z)`  | fail                       | fail              |
| `g+h` (control) | pass                    | pass              |

Two causes, not one: `e` being a declared constant, and `atan(z)`/`sin(y)` not
being admitted as indeterminates either. Both were the same refusal.
`me.reduce_rational` runs on `polynomials::multivariate`, a recursive-dense ring
over ℚ in *named variables*; `expr_to_poly` rejects a constant symbol
(`multivariate.rs`, `Expr::Sym` arm) and returns `None` on `Expr::Apply`, and
`reduce_node` then bails and hands the input straight back. Nothing was
mis-cancelled — the pass simply declined to run.

#### The framing to avoid

An earlier reading of this had it as a *design* question — whether `e` should be
a coefficient (algebraically more correct) or an indeterminate (what the specs
were written against) — and pointed at `SPECIAL_CONSTANTS_FLAG.md`. That framing
is wrong twice over, and it is wrong in a way worth keeping on the record because
it is the natural way to see it:

- **`e` does not need to be a coefficient. It needs to be a variable.**
  Cancellation is a polynomial *identity* — the engine produces `g` with
  `num = g·qn` and `den = g·qd` — and identities survive specialization. So it
  makes no difference whether an indeterminate later stands for a number, a
  function, or anything else. `e` as a seventh variable is answered by the
  existing ℚ-coefficient GCD with nothing about the coefficient ring changed.
- **The real distinction is completeness, not soundness, and it is transcendence.**
  `π` and `e` are transcendental over ℚ, so `ℚ[e] ≅ ℚ[t]` and treating them as
  variables misses nothing at all. `i` is algebraic (`i²+1 = 0`), so as a free
  variable it misses `(x²+1)/(x+i) → x−i`. Missing a cancellation is the failure
  mode; inventing one is not reachable this way.

The same argument covers `cos(x)`: opaque generators are sound for identical
reasons, and the cost is only `sin²+cos² = 1` going unseen. Legacy lands in the
same place from the other end — `polynomial.js` falls through to *"return entire
tree as a polynomial variable"* for any operator it does not recognize.

#### What shipped

Not a second engine. `ratform.rs` **already** did exactly this, with the
soundness argument already written down, and `reduce_rational` just never called
it. Kernelization moved to `polynomials/kernel.rs` (also taking `ratform.rs` from
253 to 180 lines) and `reduce_node` now kernelizes before converting: each
distinct opaque subtree becomes a fresh indeterminate, the ℚ-GCD runs unchanged,
the kernels are substituted back. One `Kernels` spans numerator and denominator —
two would give the same `cos x` two names and the common factor would go unseen.

`polynomials/compat` — the sparse engine with expression-valued coefficients,
which is a faithful port of legacy's and gets both cases right on its own — is
*not* what fixed this. It was tempting: it holds `e` in the coefficient slot
exactly as legacy does. But its expression-valued coefficients are a contract
owed to the compat API's wire format (`["polynomial","f",[[0,"e"],[1,1]]]`, which
callers compare structurally), and `reduce_rational` does not care which slot `e`
lands in. Reaching for it would have been answering the shape question instead of
the algebra one.

#### Two things fell out

**A pre-existing sign defect, now fixed.** A gcd is defined only up to a unit and
`make_lc_positive` picks one by the *main* variable's sign, so when both sides
lead negatively there the leftover `−1` landed in the denominator:
`(y²−x²)/(y−x)` returned `−(−x − y)`. Right value, unacceptable spelling, and
entirely independent of kernels — rename `y`→`x` and it goes away, which is what
pinned it. `multivariate::normalize_fraction_sign` now moves that sign onto the
numerator. Kernels made it near-universal rather than occasional, since `$k…`
sorts ahead of every ordinary variable. It accounts for **no** compat test either
way; it is covered by `the_leftover_unit_does_not_land_in_the_denominator`.

**An unbounded path, now capped.** The dense model is exponential in indeterminate
count: `y/∏ᵏ(xᵢ+1)` costs 0.5 s at k=10, 4.8 s at 12, 15.7 s at 13, tripling per
variable, with the per-variable `MAX_DEGREE` cap not touching it. That hole is as
old as the pass and identical for plain variables — kernels do not make it worse
in kind, they remove the accidental shield that made it hard to reach (anything
with a `sin` in it was refused before it got this far). `reduce_node` now caps at
`MAX_INDETERMINATES = 10`, measured rather than principled: an order of magnitude
above the six a real rational function needs. `ratform` caps at 6 because
`together` multiplies denominators and starts from a worse place.

The cap is a resource guard and **nothing else**. An earlier note here credited it
with fixing `quick_solve > linear equation`; that A/B was confounded by concurrent
edits landing between the two wasm builds, and a native A/B on a fixed tree shows
the cap makes no difference to that result. See the measurement warning below.

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

### `slow_simplify` — now at 0 (74/74)

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

- **ADOPTED — container ordering, 2 tests.** The two "sorted the same" cases.
  Legacy's sort key is *(component count, then component values)*, with
  container type not in the key at all, so containers interleave:
  `[1,6] [1,9) [9,5) [9,8] [0,4,4]`. We sort by container type first, then by
  value within each type: `[0,4,4] [1,6] [9,8] [1,9) [9,5)`.

  Kept as ours and the expectations updated. Matching legacy would mean
  reworking `normalize/order.rs`'s `rank`/`seq_index` — the comparator that
  makes `==` on canonical trees mean equality, over trees that get persisted in
  DoenetML document state — and it could not be confined to `present`: the
  `union` case is ordered by `canonicalize` and never reaches `present_add`.
  Both tests' `skip_ordering` halves already matched exactly, so only the
  comparator was ever in question.

- **FIXED — `exp` not treated as a power of `e`, 1 test.** "treat exp like
  power". All four `e^`-spelled assertions passed; all four `exp(...)`-spelled
  ones were left completely uncollected, because the `Apply` form was never
  recognised as a power.

  `exp(u)` now combines exactly as the `e^u` it spells, in the two places the
  `e^u` rules live: an accumulator in `mul` sums the arguments of `exp` factors
  (`exp(3)·exp(5) → exp(8)`), and `pow` flattens `exp(u)^k → exp(u·k)` for
  integer `k` — needed because `exp(3)/exp(5)` canonicalizes its divisor to
  `exp(5)^(−1)`. `present` gained the `exp` twin of `b^(−x) → 1/b^x`, so
  `−5exp(−t)` reads `−5/exp(t)`.

  The argument sum is kept in its own accumulator rather than folded into
  `parts` under base `e`, so the author's spelling survives: `evaluate_numbers`
  hands `exp(8)` back as `exp(8)`, and a product mixing spellings (`e^3·exp(5)`)
  leaves both alone — which is what alpha94 does too. Our version is *stronger*
  than legacy on the symbolic cases (`exp(x)exp(y) → exp(x+y)`, which legacy
  declines), matching how we already treat `e^x·e^y`. Legacy is erratic here in
  ways not worth copying: it returns `exp(5)^2` for `exp(3)exp(5)exp(2)`.

- **FIXED — unit-group ordering, 1 test.** "with units". Our
  `collect_like_terms_factors` emitted `$, deg, %`; legacy emits `$, %, deg` —
  and so did our *own* `default_order()`, so this was an inconsistency between
  two of our paths, not a missing comparator.

  A `unit` node has no degree and no coefficient, so all three groups tied both
  `present_add` keys, and the stable sort left them in **canonical** order —
  which compares the enclosed sums with the `Neg` term sorted last (`e, z, −c`
  before `f, y, −a`) while the display prints the `Neg` first. The order was
  decided by a form the reader of the output cannot see. `present_add` now ends
  with a tie-break on the *presented* subtrees, putting it in step with
  `default_order`.

## Remaining buckets by theme

**Numerical-tolerance equality (0)** — was 39, now closed out across three fixes
(root spelling, the `-1` parameter, term order), all broken down above. Both
`*-numerical-errors` files are at zero.

**Unimplemented / unbound APIs (2)** — `quick_trees` (2, below). `quick_solve`
left this bucket entirely; see below.

### The `quick_solve` failures — 3 investigated, 2 fixed, 1 left

"`solve_linear()` unimplemented" was wrong. The port has been in
`grade/linear.rs` all along and is *correct*: called directly, it returns exactly
what the legacy oracle returns on all eleven inputs the spec uses, the two
assumption-dependent ones included. All three tests failed on their first line
because `solve_linear` sat in the `notImplemented` list in
`lib/math-expressions.ts` — so 3 tests, not 3 defects, and 12 assertions that
never ran.

Three separate things had to be true to bind it:

- **A JS method.** Dropped from `notImplemented`, added as
  `Expression.solve_linear`. On no answer it returns `ABSENT_EXPRESSION`, not
  `undefined`: legacy funnelled tree-returning helpers through
  `context.fromAst(...)`, so "unsolvable" arrived as an `Expression` with
  `.tree === undefined`, and the specs read `.tree` off the result without
  checking. The stand-in's shape is what the live oracle actually hands out,
  checked case by case.
- **An assumption-aware entry point.** The existing free `solve_linear_ast` is
  deliberately assumption-*blind* — the store calls it while deciding what to
  file, so a conclusion drawn from facts already on file would depend on
  insertion order. Two of the twelve assertions need the store (`v < 0` to make
  `2v-3` nonzero; `u < 0` to flip an inequality), so the binding is a new
  `Assumptions::solve_linear` method next to `equals_expressions`, for the same
  reason that one lives there: the assumptions are the argument that matters.
  `solve_linear_ast` is untouched.
- **The `=`/`≠` operand order** — two of the assertions, and *not* a
  `solve_linear` problem at all. Detailed in the next section.

### `=`/`≠` operand order — an exact rational is a *tree* in JS

`x = -2/3` came out of `evaluate_numbers`/`simplify` as `-2/3 = x`, where the
oracle leaves the variable on the left. Both `default_order()` on its own and
`solve_linear`'s hand-built relation had it right, so the engine was holding two
spellings of one equation and the spec compared one against the other.

The cause is representational. Legacy has no exact-rational leaf: `-2/3` is the
tree `["/", -2, 3]`, and `sort_key` reaches it through the two-operand branch as
`[4, "quotient", …]` — *behind* every symbol, which keys `[1, "symbol", …]`. Our
`Number::Rat` is a leaf, so it keyed `[0, "number", …]` and sorted ahead of a
symbol. `default_order()` looked correct only by accident: it runs on the parsed
tree, where the fraction is still a `Div` and never became a `Num`.

Two changes, both needed — verified by reverting each with the other in place:

- `default_order.rs sort_key` keys a `Num` that crosses to JS as a *tree* as
  that tree. The test is the JS spelling, not the `Number` variant, which is what
  makes it exact: a decimal-spelled rational (`19.9` is `Rat(199, 10)`) crosses
  as a plain number and keeps the number key — the same split `number_to_js`
  makes.
- `canonicalize.rs canon_relation` sorts `=`/`≠` operands by
  `cmp_default_order` with the canonical `cmp` as tie-break, instead of `cmp`
  alone. Which order canonical form picks is free — equality only needs both
  sides sorted alike — but the order is displayed. `cmp` stays as the tie-break
  because the JS key is not a total order, and a stable sort would otherwise
  leave canonical form dependent on the order operands were authored in, which
  is exactly what would break `equals`.

Measured as a matched pair against the same tree and wasm build, diffed by
(file, name, occurrence): **1 fixed, 0 regressions**, and the Rust suite stays
clean.

#### LEFT — `simplify` picks its sign placement from the input spelling (1 test)

`-3y - v <= 2xz + r` solves to `(-2xz-r-v)/3` where the oracle gives
`-((2xz+r+v)/3)`. One value — `equals` says so — but two *fixpoints* of
`simplify`, and which one you land on depends on the spelling handed in:
`(2xz+r+v)/(-3)` simplifies to the pulled-out sign, while `(-(-2xz-r-v))/(-3)`
distributes it into the numerator. The `Div` sign rule fires before a `Neg` of a
sum of negations folds.

Not fixable in `solve_linear`, and it was tried: folding the numerator first
(`Div(simplify(Neg(b)), a)`) does produce the oracle's form here, but then spells
`2uv-v = 3u+q` as `(-q-v)/(3-2v)` instead of `(q+v)/(2v-3)` — one sign placement
traded for another, net zero. The fix belongs in `simplify`'s `Div`/`Neg`
handling, and being a normal-form change to displayed output it needs its own
baseline diff. `grade/linear.rs` carries a note so the shortcut is not
re-attempted.

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

**Semantic edge cases (5)** — `slow_math-expressions` (4), plus the `quick_solve`
sign-placement fixpoint (1). `slow_rational`'s 2 now pass.
No single root cause; these are a scatter of individual normalization decisions
rather than one bucket. (`slow_simplify` was the rest of this bucket and is now
at zero.)

**Assumption reasoning (5)** — all `slow_assumptions`, listed above. Includes the
`paren_if_spaced` printer defect surfacing through an assumptions spec:
`src/print/text.rs` tests `starts_with('(') && ends_with(')')`, which cannot
distinguish `(a or b)` from `(a) or (b)`. It needs a paren-balance scan.


## Highest-leverage remaining item

**`slow_assumptions` (5)**, now the largest single bucket. The two previous
holders are closed: `*-numerical-errors` is at zero, and `quick_trees` is down to
2 small items — "fail gracefully" (one arm in `interop.rs`) and
`allow_extended_match`.

Nothing left is a single-cause bucket the way those two were. The remaining 12
span five files and at least eight distinct causes, so from here the work is
per-item rather than per-bucket.

Two things worth doing that no failing test covers:

- **`equality/fuzzy.rs` is ~330 lines** and has an obvious seam (structural
  equality vs. the sensitivity tolerance) that the file's own module doc already
  names. Over the ~200-line split guideline, and over it before that change too.
- **`ops/numbers.rs` is ~640 lines** and holds two unrelated passes: numeric
  folding (`evaluate_numbers` and the rounding family) and the polynomial-GCD
  fraction cancellation (`reduce_rational`/`reduce_node`). The second now reaches
  into `polynomials::kernel` and owns its own resource cap, which makes the seam
  wider than it was. Splitting `reduce_rational` into its own module under
  `ops/` would leave both halves under the guideline.

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
