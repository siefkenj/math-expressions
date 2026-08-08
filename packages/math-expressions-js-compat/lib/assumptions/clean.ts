// Putting an assumption tree into the store's canonical form, and taking it
// apart again.
//
// Everything the store holds passes through `clean_assumptions` on the way in
// and on the way out, which is what makes `trees.equal` a usable test between
// two facts: without it `a > b` and `b < a` are different trees and the store
// would file, and hand back, both.

import * as trees from "../trees/basic";
import { simplify } from "../expression/simplify";
import { variables as variables_in } from "../expression/variables";
import { default_order } from "../trees/default_order";
import { expand_relations } from "./expand_relations";
import { flatten_logical, simplify_logical } from "./logical";

/**
 * Normalize an incoming assumption before it is filed. The legacy library
 * simplified with the assumptions in hand; the compat `simplify` has no such
 * parameter, and its value here is the arithmetic normalization (`5-5` to `0`)
 * plus a consistent relation orientation. An assumption the core cannot parse
 * back is filed as written rather than dropped.
 */
export function normalize(tree: any): any {
  try {
    return simplify(tree);
  } catch {
    return tree;
  }
}

/**
 * Canonical form: relations expanded into comparisons, `not` pushed down,
 * operands ordered, and duplicates — including anything already in `known` —
 * dropped.
 *
 * Returns undefined when nothing is left after the `known` facts are removed.
 */
export function clean_assumptions(tree: any, known?: any): any {
  if (!Array.isArray(tree) || tree.length === 0) return tree;

  tree = flatten_logical(
    default_order(simplify_logical(expand_relations(tree))),
  );

  const operator = tree[0];
  let operands = tree.slice(1);

  if (operator === "and" || operator === "or") {
    operands = operands.reduce(function (a: any[], b: any) {
      if (a.every((v) => !trees.equal(v, b))) a.push(b);
      return a;
    }, []);

    if (operator === "and" && known && Array.isArray(known)) {
      const known_operands = known[0] === "and" ? known.slice(1) : [known];
      operands = operands.filter((v: any) =>
        known_operands.every((u: any) => !trees.equal(u, v)),
      );
    }

    if (operands.length === 1) tree = operands[0];
    else tree = [operator, ...operands];
  }

  // A single fact that is already known adds nothing.
  if (operator !== "and" && known && Array.isArray(known)) {
    const known_operands = known[0] === "and" ? known.slice(1) : [known];
    if (!known_operands.every((u: any) => !trees.equal(u, tree)))
      return undefined;
  }

  return tree;
}

/**
 * The part of a fact that stays clear of `exclude_variables`, or undefined if
 * none of it does. Used to answer a query about one variable without dragging
 * in the variable that query came from.
 */
export function filter_assumptions_from_tree(
  tree: any,
  exclude_variables: any,
): any {
  if (!Array.isArray(tree) || tree.length === 0) return undefined;

  if (!Array.isArray(exclude_variables))
    exclude_variables = [exclude_variables];

  if (tree[0] === "and") {
    const kept = tree
      .slice(1)
      .map((v: any) => filter_assumptions_from_tree(v, exclude_variables))
      .filter((v: any) => v !== undefined);

    if (kept.length === 0) return undefined;
    if (kept.length === 1) return kept[0];
    return ["and", ...kept];
  }

  const tree_variables = variables_in(tree);
  const contains_excluded = exclude_variables.some((v: string) =>
    tree_variables.includes(v),
  );

  return contains_excluded ? undefined : tree;
}

/**
 * Drop `tree` (or its solved-for-a-variable spelling) from a fact. Returns
 * undefined when there was nothing to remove, `[]` when nothing is left.
 */
export function remove_from(current: any, tree: any, solved: any): any {
  const matches = (v: any) => trees.equal(v, tree) || trees.equal(v, solved);

  if (current[0] === "and") {
    const operands = current.slice(1);
    const kept = operands.filter((v: any) => !matches(v));

    if (kept.length === 0) return [];
    if (kept.length === 1) return kept[0];
    if (kept.length < operands.length) return ["and", ...kept];
    return undefined;
  }

  if (matches(current)) return [];
  return undefined;
}
