// Derived assumptions: the facts that follow from combining the stored ones.
//
// The store files each fact under the variables it mentions, so `x < a` lands
// under `x` and under `a`. Chaining them is this module's job: with `x < a` and
// `a < b` on file, `x < b` holds and has to come back from
// `get_assumptions("x")` even though nobody stated it.
//
// Ported from the legacy `lib/assumptions/assumptions.js`.

import { variables as variables_in } from "../../expression/variables";
import { clean_assumptions } from "../clean";
import { combine_assumptions } from "./combine";
import { get_assumptions_for_expr } from "./for_expr";
import type { AssumptionStore } from "../store";

/**
 * Every assumption on the variables of `tree` that follows from `tree`, keyed
 * by variable, with anything already recorded on that variable filtered out.
 *
 * With `tree` undefined, works from every stored assumption at once — which is
 * how the store recomputes its derived facts after each change.
 */
export function calculate_derived_assumptions(
  store: AssumptionStore,
  tree?: any,
): Record<string, any> {
  if (tree === undefined) {
    let collected: any[] = [];
    for (const v in store.byvar) {
      const a = store.byvar[v];
      if (a && a.length > 0) collected.push(a);
    }
    if (collected.length === 0) return {};

    tree = collected.length === 1 ? collected[0] : ["and", ...collected];
    tree = clean_assumptions(tree);
  }

  if (!Array.isArray(tree) || tree.length === 0) return {};

  const operator = tree[0];
  const operands = tree.slice(1);

  if (operator === "and" || operator === "or") {
    const results = operands.map((v: any) =>
      calculate_derived_assumptions(store, v),
    );

    const allvars: string[] = [
      ...new Set(
        results.reduce<string[]>((a, b) => [...a, ...Object.keys(b)], []),
      ),
    ];

    const derived: Record<string, any> = {};

    for (const v of allvars) {
      const res = results.reduce<any[]>((a, b) => {
        if (b[v] !== undefined) a.push(b[v]);
        return a;
      }, []);

      // An `or` only entails something about `v` if every branch does.
      if (operator === "and" || res.length === results.length) {
        let new_derived = derived[v];
        if (new_derived === undefined) {
          new_derived = res.length > 1 ? [operator, ...res] : res[0];
        } else {
          new_derived =
            res.length > 1
              ? ["and", new_derived, [operator, ...res]]
              : ["and", new_derived, res[0]];
        }

        derived[v] = clean_assumptions(
          new_derived,
          store.get_assumptions(v, { omit_derived: true }),
        );
      }
    }

    return derived;
  }

  const derived: Record<string, any> = {};

  if (
    ["=", "ne", "<", "le", "in", "subset", "notin", "notsubset"].includes(
      operator,
    )
  ) {
    let addressed_assumption = false;

    // Only a side that *is* a variable can carry a derived fact about it.
    for (let ind = 0; ind < 2; ind++) {
      const v = operands[ind];
      const other = operands[1 - ind];
      const other_var = variables_in(other);
      if (
        typeof v !== "string" ||
        other_var.length === 0 ||
        other_var.includes(v)
      )
        continue;

      addressed_assumption = true;

      // Reading the relation from the right-hand side reverses it.
      let adjusted_op = operator;
      if (ind === 1) {
        if (operator === "<") adjusted_op = ">";
        else if (operator === "le") adjusted_op = "ge";
        else if (operator === "in") adjusted_op = "ni";
        else if (operator === "subset") adjusted_op = "superset";
        else if (operator === "notin") adjusted_op = "notni";
        else if (operator === "notsubset") adjusted_op = "notsuperset";
      }

      let result = get_assumptions_for_expr(store, other, [v]);
      result = combine_assumptions(v, adjusted_op, other, result);

      if (result !== undefined) {
        let new_derived = derived[v];
        new_derived =
          new_derived === undefined ? result : ["and", new_derived, result];

        derived[v] = clean_assumptions(
          new_derived,
          store.get_assumptions(v, { omit_derived: true }),
        );
      }
    }
    if (addressed_assumption) return derived;
  }

  // Nothing could be combined, so carry over whatever is known about the
  // operands unchanged.
  let collected: any[] = [];

  for (const op of operands) {
    const res = get_assumptions_for_expr(store, op, []);
    if (res !== undefined) collected.push(res);
  }

  if (collected.length === 0) return {};

  const results = collected.length === 1 ? collected[0] : ["and", ...collected];

  for (const v of variables_in(tree)) {
    derived[v] = clean_assumptions(
      results,
      store.get_assumptions(v, { omit_derived: true }),
    );
  }

  return derived;
}
