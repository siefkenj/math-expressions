# js-compat test failure summary

Snapshot of `packages/math-expressions-js-compat` (`npx vitest run`) on branch
`doenet`, after rebuilding `vendor/wasm` (`bash build-wasm.sh`) so results reflect
current Rust source.

**Current totals (2026-08-11, at `62c5f20` + uncommitted working-tree work):
11 failed / 6306 passed / 6329 total** (10 skipped, 2 todo).
Previous snapshots: 380, 162, 97, 83, 82, 61, 55, 54, 46, 43, 38, 20, 16, 12.

Both `*-numerical-errors` files are at **zero**, and so are `slow_simplify` and
`slow_rational`.

Nine of the ten skips are **wontfix, not pending** — the deprecated `match`
conditions under `quick_trees` below. Read the 54 → 46 step as a
reclassification, not nine defects fixed.

## How to measure

This snapshot is reproducible from a clean tree: run `bash build-wasm.sh` then
`npx vitest run`, both from inside `packages/math-expressions-js-compat`.
Rebuild the wasm first or the run measures the previous engine; that alone
accounted for a 4-test discrepancy while an earlier snapshot was being taken,
and `build-wasm.sh` lives in the package, not at the repo root.

Judge changes by name-level diff against a baseline worktree, never by aggregate
counts (see memory `js-compat-suite-baseline-diff`). When diffing, note that
several spec files contain **duplicate test names**, so keying a comparison by
name alone silently drops results — key by (file, name, occurrence). Some specs
also derive the test *name* from the expected string, so updating an expectation
renames its test; those show up as removed-plus-added rather than failed→passed,
and the honest check is that no name present in *both* runs went passing→failing.

**A matched pair means the two runs differ by the change set and nothing else.**
On a tree someone else is editing, a baseline goes stale in minutes; capture both
halves back to back or the diff is measuring the other person. Two past
attributions were confounded exactly this way — one over-claimed another change
set's fixes, and one made a resource guard look like a correctness fix.

A further caution: `slow_simplify`-style tests bundle 5–20 assertions each, so a
per-test bucket count is an upper bound on what any one fix buys. Attributing a
test to the first assertion that fails hides everything behind it.

## Remaining failures by spec file

| count | spec file                                       | root cause / category                                                                    |
| ----: | ----------------------------------------------- | ---------------------------------------------------------------------------------------- |
|     2 | `quick_trees`                                   | `allow_extended_match`, and one throw-vs-`false` (+9 skipped wontfix) — see below        |
|     5 | `slow_assumptions`                              | see below                                                                                |
|     4 | `slow_math-expressions`                         | equality of containers/unions, an integer assumption, one derivative identity            |

`quick_solve` is at zero, and its last failure was fixed at the root rather than
adopted as a divergence — see below.

### `simplify` now has one fixpoint per value, not one per spelling

`-3y-v <= 2xz+r` solved to `(-2xz-r-v)/3` where alpha94 gives `-((2xz+r+v)/3)`.
Same value, but *both* were fixpoints of `simplify`, so which one came out
depended only on how the input was spelled. Idempotence was never the issue
(`simplify∘simplify == simplify` held throughout, asserted in
`simplify_corpus.rs`); **confluence** was.

The cause was two rules that disagreed, keyed on something mathematically
irrelevant — the *magnitude* of the coefficient:

- `rule_distribute_neg_over_sum` fired only for a coefficient of exactly `−1`
  over a lone sum, and distributed **unconditionally**.
- `rule_distribute_sign` fired for any negative coefficient, but only when it
  reduced the sign count.

So `-(x+y)` distributed and `-2(x+y)` did not, and `-(a+b)/3` — whose canonical
coefficient is `−1/3` — took the second branch. Meanwhile `(-a-b)/3` has a
*positive* coefficient, so neither rule looked at it. Both stood still. More
generally: the engine only ever pushed signs **into** a sum and never pulled one
out, so any input already spelled the disfavoured way was a fixpoint by default.
A preference expressed by only one of the two rewrites is not a normal form.

**The fix** is the converse rewrite, `rule_factor_sign_out_of_sum`
(`normalize/simplify.rs`): a sum of `k` terms with `n` negated factors its `−1`
out iff `2n ≥ k + 2`. That complements `rule_distribute_sign`'s push-in
threshold exactly — with the tie there moved from "decline" to "push in", which
is load-bearing rather than cosmetic. Writing the factored spelling's negated
count as `m = k − n`:

- distributed is stable iff `2n ≤ k + 1`;
- factored is stable iff `2m ≤ k − 2`, i.e. `2n ≥ k + 2`.

Exact complements, so precisely one of the two spellings is stable for every
sum: never both (two fixpoints — the bug) and never neither (a ping-pong).
Moving either threshold by one re-opens one of those. The tie cases (`k` odd,
`2n = k+1`, e.g. `-x-y+z` vs `-(x+y-z)`) are what a paper argument gets wrong
first; they are pinned in `tests/simplify_sign_fixpoint.rs`.

The old unconditional distribution survives, narrowed, as
`rule_flatten_negated_sum_term`: a negated sum is spliced into its parent only
when it is a *term of a larger sum*, which is the only position where terms can
meet and cancel. That preserves `(q + 12 - (q+2))/2 → 5`, the `<lineSegment>`
midpoint shape that motivated the original rule.

**Cost: one row.** `-(x+y)` now stays factored where alpha94 distributes it to
`-x-y`. That row is unavoidable — `-(x+y)` costs one sign and `-x-y` costs two,
so every sign-counting rule prefers the factored form, and alpha94 prefers the
other only because it never consults a count. Keeping alpha94's answer would
require the distributed spelling to stay a fixpoint, which is precisely the bug.
Recorded as `DIVERGENCE (adopted):` in `tests/doenet_sign_distribution.rs`.

**Measured: 0 net js-compat change** (11 failures, identical by name), 77 cargo
suites green, clippy clean. The `quick_solve` expectation reverted to alpha94's
spelling and now passes with no divergence note, and `-(x+y-z)` — a tie —
still matches alpha94 because ties push in.

Two things fell out. `(-a-b)/(-c-d)` now reduces to `(a+b)/(c+d)`; it did not
before, in this engine **or** alpha94, because with no numeric coefficient
anywhere neither sign rule could see it. And the cost table at
`simplify.rs`'s sign cluster was wrong about `-(x+y)` — it claimed "unchanged"
while the unconditional rule preempted it and distributed. Both are now correct
and covered.

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

### `quick_trees` — 2 left, 9 wontfix

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

## Remaining buckets by theme

**Semantic edge cases (4)** — all `slow_math-expressions`. No single root cause;
these are a scatter of individual normalization decisions rather than one bucket.

**Unimplemented / unbound APIs (2)** — `quick_trees`, both above.

**Assumption reasoning (5)** — all `slow_assumptions`, listed above. Includes the
`paren_if_spaced` printer defect surfacing through an assumptions spec:
`src/print/text.rs` tests `starts_with('(') && ends_with(')')`, which cannot
distinguish `(a or b)` from `(a) or (b)`. It needs a paren-balance scan.

## Highest-leverage remaining item

**`slow_assumptions` (5)**, now the largest single bucket. Nothing left is a
single-cause bucket the way `*-numerical-errors` and `quick_trees` were. The
remaining 11 span three files and at least seven distinct causes, so from here
the work is per-item rather than per-bucket.

Two things worth doing that no failing test covers:

- **`equality/fuzzy.rs` is ~330 lines** and has an obvious seam (structural
  equality vs. the sensitivity tolerance) that the file's own module doc already
  names. Over the ~200-line split guideline.
- **`ops/numbers.rs` is ~640 lines** and holds two unrelated passes: numeric
  folding (`evaluate_numbers` and the rounding family) and the polynomial-GCD
  fraction cancellation (`reduce_rational`/`reduce_node`). The second reaches
  into `polynomials::kernel` and owns its own resource cap, which makes the seam
  wide. Splitting `reduce_rational` into its own module under `ops/` would leave
  both halves under the guideline.

## Rejected: folding roots to powers in `canonicalize`

Recorded so it is not relitigated. `ops/transforms.rs:122` records keeping
`sqrt(x)` and `x^(1/2)` as distinct canonical trees as a deliberate decision, and
the oracle backs it — legacy also keeps them distinct in `.tree` and in printed
output, folding to a power only inside the explicit `normalize_function_names`
pass:

```
legacy  fromText("sqrt(q)").tree  ->  ["apply","sqrt","q"]    prints sqrt(q)
legacy  fromText("q^(1/2)").tree  ->  ["^","q",["/",1,2]]     prints q^(1/2)
```

Making `canonicalize` fold roots would break `q^(1/2)` round-tripping (the two
become one tree and must therefore print alike), move roots from the `Apply` to
the `Pow` rank in both comparators, cost `sqrt(8) → 2√2` unless
`rule_radical`'s numeric-base-only `Pow` arm is generalized, drop `sqrt` off its
dedicated `z.sqrt()` / `FixId::Sqrt` kernels onto generic `powc` (branch-cut
risk), and require regenerating ~130 fixture entries.

A note for whoever picks these up: several past fixes turned out to be *bindings*
for engine code that already existed and was already exercised elsewhere. Before
implementing anything that looks like a missing feature here, grep the Rust core
for it first.
