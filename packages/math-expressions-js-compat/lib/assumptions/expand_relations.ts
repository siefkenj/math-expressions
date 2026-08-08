// `expand_relations`: rewrite the relations that carry more than one fact into
// an explicit `and`/`or` of two-sided comparisons. Ported from the legacy
// `lib/expression/transformation.js`.
//
// The assumptions store keeps one fact per variable, so a chained inequality
// (`a < b < c`), an interval membership (`x elementof (a,b]`) or an interval
// containment (`(a,b) subset [c,d)`) has to be broken into the comparisons it
// stands for before it can be filed under a variable. That expansion is also
// what `get_assumptions` hands back, which is why the bracket kinds have to map
// onto strict/non-strict exactly (an endpoint that is closed on the small side
// and open on the big side forces a *strict* comparison).
//
// The compat `Expression.expand_relations()` is a no-op stub, so this is the
// only working implementation in the package.

/**
 * A tuple/array of two entries spells an interval when it sits on either side
 * of a containment: `(a,b)` is open, `[a,b]` closed. Half-open forms already
 * parse to an explicit `interval` node, so they arrive here unchanged.
 *
 * Only the top level is converted (the legacy `to_intervals` recursed): the
 * endpoints of an interval are scalars, and a nested tuple inside one is not an
 * interval of its own.
 */
function to_interval(tree: any): any {
  if (!Array.isArray(tree)) return tree;
  if (tree[0] === "tuple" && tree.length === 3) {
    return ["interval", ["tuple", tree[1], tree[2]], ["tuple", false, false]];
  }
  if (tree[0] === "array" && tree.length === 3) {
    return ["interval", ["tuple", tree[1], tree[2]], ["tuple", true, true]];
  }
  return tree;
}

export function expand_relations(tree: any): any {
  if (!Array.isArray(tree)) return tree;

  // Bottom-up, like the legacy transform: operands are expanded first so a
  // relation nested inside an `and` is already in its expanded form.
  const operator = tree[0];
  const operands = tree.slice(1).map(expand_relations);

  if (operator === "=") {
    if (operands.length <= 2) return [operator, ...operands];
    const result: any[] = ["and"];
    for (let i = 0; i < operands.length - 1; i++) {
      result.push(["=", operands[i], operands[i + 1]]);
    }
    return result;
  }

  if (operator === "gts" || operator === "lts") {
    const args = operands[0];
    const strict = operands[1];

    if (args[0] !== "tuple" || strict[0] !== "tuple")
      throw new Error("Badly formed ast");

    const comparisons: any[] = [];
    for (let i = 1; i < args.length - 1; i++) {
      let new_operator;
      if (strict[i]) new_operator = operator === "lts" ? "<" : ">";
      else new_operator = operator === "lts" ? "le" : "ge";
      comparisons.push([new_operator, args[i], args[i + 1]]);
    }

    let result: any = ["and", comparisons[0], comparisons[1]];
    for (let i = 2; i < comparisons.length; i++)
      result = ["and", result, comparisons[i]];
    return result;
  }

  if (["in", "notin", "ni", "notni"].includes(operator)) {
    const negate = operator === "notin" || operator === "notni";

    let x, interval;
    if (operator === "in" || operator === "notin") {
      x = operands[0];
      interval = operands[1];
    } else {
      x = operands[1];
      interval = operands[0];
    }

    interval = to_interval(interval);

    // Membership in a set that is not an interval (`x elementof R`) stays as it
    // is — there is nothing to expand it into.
    if (!Array.isArray(interval) || interval[0] !== "interval")
      return [operator, ...operands];

    const args = interval[1];
    const closed = interval[2];
    if (args[0] !== "tuple" || closed[0] !== "tuple")
      throw new Error("Badly formed ast");

    const a = args[1];
    const b = args[2];

    const comparisons: any[] = [];
    if (closed[1]) comparisons.push(negate ? ["<", x, a] : ["ge", x, a]);
    else comparisons.push(negate ? ["le", x, a] : [">", x, a]);
    if (closed[2]) comparisons.push(negate ? [">", x, b] : ["le", x, b]);
    else comparisons.push(negate ? ["ge", x, b] : ["<", x, b]);

    // Negating a conjunction gives a disjunction: `x ∉ (a,b)` is
    // `x ≤ a or x ≥ b`, not an `and`.
    return [negate ? "or" : "and", ...comparisons];
  }

  if (["subset", "notsubset", "superset", "notsuperset"].includes(operator)) {
    const negate = operator === "notsubset" || operator === "notsuperset";

    let small, big;
    if (operator === "subset" || operator === "notsubset") {
      small = operands[0];
      big = operands[1];
    } else {
      small = operands[1];
      big = operands[0];
    }

    small = to_interval(small);
    big = to_interval(big);

    // Containment between things that are not both intervals (`A subset B`)
    // carries no comparison to expand.
    if (
      !Array.isArray(small) ||
      !Array.isArray(big) ||
      small[0] !== "interval" ||
      big[0] !== "interval"
    )
      return [operator, ...operands];

    const small_args = small[1];
    const small_closed = small[2];
    const big_args = big[1];
    const big_closed = big[2];
    if (
      small_args[0] !== "tuple" ||
      small_closed[0] !== "tuple" ||
      big_args[0] !== "tuple" ||
      big_closed[0] !== "tuple"
    )
      throw new Error("Badly formed ast");

    const small_a = small_args[1];
    const small_b = small_args[2];
    const big_a = big_args[1];
    const big_b = big_args[2];

    const comparisons: any[] = [];
    // A closed small end inside an open big end is the one case that needs a
    // strict comparison: `[a,b] ⊂ (c,d)` requires `a > c`, while every other
    // combination is satisfied by `a ≥ c`.
    if (small_closed[1] && !big_closed[1])
      comparisons.push(negate ? ["le", small_a, big_a] : [">", small_a, big_a]);
    else
      comparisons.push(negate ? ["<", small_a, big_a] : ["ge", small_a, big_a]);
    if (small_closed[2] && !big_closed[2])
      comparisons.push(negate ? ["ge", small_b, big_b] : ["<", small_b, big_b]);
    else
      comparisons.push(negate ? [">", small_b, big_b] : ["le", small_b, big_b]);

    return [negate ? "or" : "and", ...comparisons];
  }

  return [operator, ...operands];
}

export default expand_relations;
