// Logical normalization for stored assumptions: push `not` down to the
// comparisons and collapse nested `and`/`or`.
//
// The legacy library did this with `simplify.simplify_logical`, and the wasm
// core has an `Expression.simplify_logical()` that does the same rewriting —
// but it also canonicalizes each relation (`x > a` becomes `a < x`) and, worse,
// routes the tree through the parser. The assumptions store has to keep the
// *shape* of the relation it was handed until `clean_assumptions` orders it, so
// the not-pushdown is done here in JS instead.

/** Collapse `and`/`or` nested inside the same operator into one n-ary node. */
export function flatten_logical(tree: any): any {
  if (!Array.isArray(tree)) return tree;

  const operator = tree[0];
  const operands = tree.slice(1).map(flatten_logical);

  if (operator === "and" || operator === "or") {
    const out: any[] = [];
    for (const operand of operands) {
      if (Array.isArray(operand) && operand[0] === operator)
        out.push(...operand.slice(1));
      else out.push(operand);
    }
    if (out.length === 1) return out[0];
    return [operator, ...out];
  }

  return [operator, ...operands];
}

// Negating a comparison flips it to its complement. `not(x < a)` is `x >= a`,
// which is stated as `ge` on the same operands rather than `le` with them
// swapped, so the operand order the caller wrote survives the rewrite.
const NEGATED_RELATION = {
  "=": "ne",
  ne: "=",
  "<": "ge",
  ge: "<",
  ">": "le",
  le: ">",
  in: "notin",
  notin: "in",
  ni: "notni",
  notni: "ni",
  subset: "notsubset",
  notsubset: "subset",
  superset: "notsuperset",
  notsuperset: "superset",
};

/**
 * De Morgan plus double-negation elimination, driving `not` down to the
 * comparisons, where it becomes the complementary relation.
 *
 * A `not` that cannot be pushed any further (over an operator with no
 * complement) is left in place rather than dropped — the store treats it as an
 * opaque fact.
 */
export function simplify_logical(tree: any): any {
  if (!Array.isArray(tree)) return tree;

  const operator = tree[0];
  const operands = tree.slice(1).map(simplify_logical);

  if (operator === "not") {
    const operand = operands[0];
    if (!Array.isArray(operand)) return ["not", operand];

    const inner_operator = operand[0];
    const inner_operands = operand.slice(1);

    if (inner_operator === "not") return inner_operands[0];
    if (inner_operator === "and" || inner_operator === "or") {
      return flatten_logical([
        inner_operator === "and" ? "or" : "and",
        ...inner_operands.map((v: any) => simplify_logical(["not", v])),
      ]);
    }
    const negated = NEGATED_RELATION[inner_operator];
    if (negated) return [negated, ...inner_operands];

    return ["not", operand];
  }

  return flatten_logical([operator, ...operands]);
}

export default simplify_logical;
