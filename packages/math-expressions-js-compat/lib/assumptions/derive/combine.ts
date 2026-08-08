// Composing two relations into one.
//
// The store knows `x < a` and, separately, `a < b`. Chaining them into `x < b`
// is a question about the *operators*: which pairs compose, into what, and
// which say nothing together (`x < a` with `a > b` bounds `x` from neither
// side). That table is this module.

import * as trees from "../../trees/basic";

// How `expr1 op1 expr2` composes with a relation on `expr2`. `undefined` means
// the two say nothing together (`x < a` with `a > b` bounds `x` from neither
// side); `new_as` means they interact but not in a way that can be stated about
// `expr1`.
const COMBINED_OPERATOR = {
  "<": { "<": "<", le: "<" },
  le: { "<": "<", le: "le" },
  ">": { ">": ">", ge: ">" },
  ge: { ">": ">", ge: "ge" },
  in: { subset: "in" },
  notin: { superset: "notin" },
  ni: { notin: "notsubset" },
  notni: { in: "notsuperset" },
  subset: { subset: "subset", notni: "notni", notsuperset: "notsuperset" },
  notsubset: { superset: "notsubset" },
  superset: { superset: "superset", ni: "ni", notsubset: "notsubset" },
  notsuperset: { subset: "notsuperset" },
};

// A composed relation is stated with `expr1` on the left, so the reversed
// operators are re-spelled with their operands swapped.
const REVERSED_FORM = {
  ">": "<",
  ge: "le",
  ni: "in",
  notni: "notin",
  superset: "subset",
  notsuperset: "notsubset",
};

/**
 * Given the assumption `expr1 op1 expr2` plus the assumptions `new_as` about
 * `expr2`, state what follows about `expr1`.
 *
 * Returns undefined when `new_as` says nothing about `expr1`, and `new_as`
 * itself when it bears on `expr1` but cannot be reduced to a relation on it.
 */
export function combine_assumptions(
  expr1: any,
  op1: string,
  expr2: any,
  new_as: any,
): any {
  if (
    ![
      "=",
      "ne",
      "<",
      "le",
      ">",
      "ge",
      "in",
      "notin",
      "ni",
      "notni",
      "subset",
      "notsubset",
      "superset",
      "notsuperset",
    ].includes(op1)
  )
    return new_as;

  if (!Array.isArray(new_as)) return undefined;

  const op2 = new_as[0];
  const operands2 = new_as.slice(1);

  if (op2 === "and" || op2 === "or") {
    const results = operands2
      .map((v: any) => combine_assumptions(expr1, op1, expr2, v))
      .filter((v: any) => v !== undefined);

    if (results.length === 0) return undefined;
    if (op2 === "or") {
      if (results.length === operands2.length) return ["or", ...results];
      return undefined;
    }
    if (results.length === 1) return results[0];
    return ["and", ...results];
  }

  if (
    !["=", "ne", "<", "le", "in", "notin", "subset", "notsubset"].includes(op2)
  )
    return new_as;

  let op2_eff = op2;
  let rhs;
  if (trees.equal(operands2[0], expr2)) {
    rhs = operands2[1];
  } else if (trees.equal(operands2[1], expr2)) {
    rhs = operands2[0];
    op2_eff =
      {
        "<": ">",
        le: "ge",
        in: "ni",
        notin: "notni",
        subset: "superset",
        notsubset: "notsuperset",
      }[op2] ?? op2;
  } else {
    return new_as;
  }

  let combined_op;
  if (op1 === "=") combined_op = op2_eff;
  else if (op2_eff === "=") combined_op = op1;
  else {
    const table = COMBINED_OPERATOR[op1];
    if (!table) return undefined;
    combined_op = table[op2_eff];
    if (!combined_op) {
      // A membership relation on the far side still constrains `expr1`, it
      // just cannot be folded into a single relation.
      if (
        ["<", "le", ">", "ge"].includes(op1) &&
        ["in", "notin"].includes(op2_eff)
      )
        return new_as;
      return undefined;
    }
  }

  const reversed = REVERSED_FORM[combined_op];
  if (reversed) return [reversed, rhs, expr1];
  return [combined_op, expr1, rhs];
}
