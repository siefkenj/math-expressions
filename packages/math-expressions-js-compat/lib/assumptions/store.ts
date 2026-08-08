// The assumption store: the JS-shaped side of `me.add_assumption(...)` /
// `me.get_assumptions(...)`.
//
// The wasm `Assumptions` handle answers *predicates* (`is_real(x+y)`), and it
// stays the source of truth for those. It has no notion of handing a fact back
// as a tree, though, which is what `get_assumptions` is for: it returns an AST
// stating everything known about a variable, with that variable on the left.
// That is a JS-API shape rather than a reasoning question, so it is rebuilt
// here over the assumption trees instead of by widening the wasm ABI.
//
// Ported from the legacy `lib/assumptions/assumptions.js`. Facts are filed per
// variable (`byvar`, see `./mutate`), the consequences of combining them are
// recomputed after every change (`derived`, see `./derive`), and a `generic`
// assumption written in terms of `x` stands in for any variable with no facts
// of its own.

import { get_tree } from "../trees/util";
import * as trees from "../trees/basic";
import { variables as variables_in } from "../expression/variables";
import {
  clean_assumptions,
  filter_assumptions_from_tree,
  normalize,
} from "./clean";
import {
  add_assumption_sub,
  add_generic_assumption_sub,
  remove_assumption_sub,
  remove_generic_assumption_sub,
} from "./mutate";
import {
  calculate_derived_assumptions,
  get_assumptions_for_expr,
} from "./derive";

/** Assumptions on a variable, or `[]` for "nothing known". */
type Facts = any;

export class AssumptionStore {
  byvar: Record<string, Facts> = {};
  derived: Record<string, Facts> = {};
  generic: Facts = [];

  clear() {
    this.byvar = {};
    this.derived = {};
    this.generic = [];
  }

  /**
   * The assumptions on a variable, a list of variables, or an expression.
   *
   * - a string or `[["a","b"]]` (an array holding an array) names variables,
   *   answered from `byvar`/`derived`, or from `generic` for a variable with no
   *   facts of its own;
   * - anything else is an expression, answered by restating the facts on its
   *   variables in terms of the expression itself.
   *
   * Returns undefined when nothing is known.
   */
  get_assumptions(variables_or_expr: any, params: any = {}): any {
    let exclude_variables = params.exclude_variables;
    if (exclude_variables === undefined) exclude_variables = [];
    else if (!Array.isArray(exclude_variables))
      exclude_variables = [exclude_variables];

    const tree = get_tree(variables_or_expr);

    let variables;
    if (typeof tree === "string") variables = [tree];
    else if (!Array.isArray(tree)) return undefined;
    else if (Array.isArray(tree[0])) variables = tree[0];

    if (variables)
      return this.facts_for_variables(
        variables,
        exclude_variables,
        params.omit_derived,
      );
    return get_assumptions_for_expr(this, tree, exclude_variables);
  }

  /**
   * File an assumption. Unless `exclude_generic`, any variable meeting the
   * store for the first time also picks up the generic assumption.
   *
   * Returns the number of facts recorded — 0 for an empty or non-tree
   * assumption, which is a no-op rather than an error.
   */
  add_assumption(expr_or_tree: any, exclude_generic?: boolean): number {
    return this.apply(expr_or_tree, (tree) =>
      add_assumption_sub(this, tree, exclude_generic),
    );
  }

  /**
   * File a generic assumption: one written in terms of `x`, standing for any
   * variable that has no assumptions of its own.
   */
  add_generic_assumption(expr_or_tree: any): number {
    return this.apply(expr_or_tree, (tree) =>
      add_generic_assumption_sub(this, tree),
    );
  }

  remove_assumption(expr_or_tree: any): number {
    return this.apply(expr_or_tree, (tree) =>
      remove_assumption_sub(this, tree),
    );
  }

  remove_generic_assumption(expr_or_tree: any): number {
    return this.apply(expr_or_tree, (tree) =>
      remove_generic_assumption_sub(this, tree),
    );
  }

  /**
   * The shape every mutation shares: normalize the assumption, hand it to
   * `sub`, and recompute the derived facts if anything moved. The derived
   * facts are a function of the whole store, so any change invalidates all of
   * them.
   */
  private apply(expr_or_tree: any, sub: (tree: any) => number): number {
    const tree = get_tree(expr_or_tree);

    if (!Array.isArray(tree)) return 0;

    const cleaned = clean_assumptions(normalize(tree));
    if (!Array.isArray(cleaned) || cleaned.length === 0) return 0;

    const n = sub(cleaned);

    if (n) this.derived = calculate_derived_assumptions(this);

    return n;
  }

  private facts_for_variables(
    variables: any,
    exclude_variables: string[],
    omit_derived?: boolean,
  ): any {
    if (!Array.isArray(variables)) variables = [variables];

    const collected: any[] = [];

    for (const v of variables) {
      if (this.byvar[v] || this.derived[v]) {
        if (this.byvar[v] && this.byvar[v].length > 0) {
          const byvar = filter_assumptions_from_tree(
            this.byvar[v],
            exclude_variables,
          );
          if (byvar !== undefined) collected.push(byvar);
        }
        if (this.derived[v] && this.derived[v].length > 0 && !omit_derived) {
          const da = filter_assumptions_from_tree(
            this.derived[v],
            exclude_variables,
          );
          if (da !== undefined) collected.push(da);
        }
      } else if (this.generic.length > 0) {
        // The generic assumption is written in terms of `x`. Substituting a
        // different variable into it would be wrong if that variable is named
        // in the generic assumption itself (`x < y` says nothing about `y`).
        if (v === "x" || !variables_in(this.generic).includes(v))
          collected.push(trees.substitute(this.generic, { x: v }));
      }
    }

    let a: any;
    if (collected.length === 1) a = collected[0];
    else if (collected.length > 1) a = ["and", ...collected];
    else a = [];

    if (a.length > 0) return clean_assumptions(a);
    return undefined;
  }
}

export default AssumptionStore;
