// Filing and unfiling a single assumption.
//
// A fact is recorded once per variable it mentions, and it is recorded *solved
// for* that variable where possible — `a + b < 1` is filed under `a` as
// `a < 1 - b` — so that `get_assumptions("a")` can answer with a statement
// about `a` rather than a relation the caller has to rearrange. That is also
// why removal tries both spellings: `remove_assumption("5 < z")` has to find
// the `z > 5` that was filed.

import * as trees from "../trees/basic";
import { variables as variables_in } from "../expression/variables";
import { clean_assumptions, remove_from } from "./clean";
import { solve_linear } from "./linear";
import type { AssumptionStore } from "./store";

/** Sum the results of applying `sub` to each conjunct of an `and`. */
function over_conjuncts(tree: any, sub: (v: any) => number): number {
  return tree
    .slice(1)
    .map(sub)
    .reduce((a: number, b: number) => a + b, 0);
}

export function add_assumption_sub(
  store: AssumptionStore,
  tree: any,
  exclude_generic?: boolean,
): number {
  // Split an `and` so each conjunct is filed under its own variables.
  if (tree[0] === "and")
    return over_conjuncts(tree, (v) =>
      add_assumption_sub(store, v, exclude_generic),
    );

  const variables = variables_in(tree);

  if (variables.length === 0) return 0;

  let n_added = 0;

  if (!exclude_generic && store.generic.length > 0) {
    for (const v of variables) {
      if (store.byvar[v] === undefined) {
        // A variable named in the generic assumption itself is not one the
        // generic assumption speaks about (`x < y` says nothing about `y`).
        if (v === "x" || !variables_in(store.generic).includes(v)) {
          add_assumption_sub(
            store,
            trees.substitute(store.generic, { x: v }),
            true,
          );
          n_added += 1;
        }
      }
    }
  }

  for (const variable of variables) {
    const solved = solve_linear(tree, variable);

    let new_a = solved ? solved : tree;

    const current_a = store.byvar[variable];

    if (current_a !== undefined && current_a.length !== 0)
      new_a = ["and", current_a, new_a];

    new_a = clean_assumptions(new_a);

    if (!trees.equal(new_a, current_a)) {
      store.byvar[variable] = new_a;
      n_added += 1;
    }
  }

  return n_added;
}

export function add_generic_assumption_sub(
  store: AssumptionStore,
  tree: any,
): number {
  if (tree[0] === "and")
    return over_conjuncts(tree, (v) => add_generic_assumption_sub(store, v));

  if (!variables_in(tree).includes("x")) return 0;

  const solved = solve_linear(tree, "x");

  let new_a = solved ? solved : tree;

  const current_a = store.generic;

  if (current_a.length !== 0) new_a = ["and", current_a, new_a];

  new_a = clean_assumptions(new_a);

  if (trees.equal(new_a, current_a)) return 0;

  store.generic = new_a;

  return 1;
}

export function remove_assumption_sub(
  store: AssumptionStore,
  tree: any,
): number {
  if (tree[0] === "and")
    return over_conjuncts(tree, (v) => remove_assumption_sub(store, v));

  const variables = variables_in(tree);

  if (variables.length === 0) return 0;

  let n_removed = 0;

  for (const variable of variables) {
    const solved = solve_linear(tree, variable);

    const current = store.byvar[variable];

    if (!current || current.length === 0) continue;

    const result = remove_from(current, tree, solved);
    if (result === undefined) continue;

    n_removed += 1;
    store.byvar[variable] = result;
  }

  return n_removed;
}

export function remove_generic_assumption_sub(
  store: AssumptionStore,
  tree: any,
): number {
  if (tree[0] === "and")
    return over_conjuncts(tree, (v) => remove_generic_assumption_sub(store, v));

  if (!variables_in(tree).includes("x")) return 0;

  const current = store.generic;

  if (current.length === 0) return 0;

  const solved = solve_linear(tree, "x");

  const result = remove_from(current, tree, solved);
  if (result === undefined) return 0;

  store.generic = result;

  return 1;
}
