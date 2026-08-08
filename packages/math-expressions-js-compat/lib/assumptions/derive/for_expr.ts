// Restating what is known about variables as a statement about an expression
// built from them.
//
// `get_assumptions(me.from("q-x"))` has to answer `q - x > 0` from a stored
// `q > x`. That works whenever the expression is linear in its variables: each
// stored relation on a variable is substituted back into the expression, which
// turns a fact about the variable into a fact about the whole.

import * as trees from "../../trees/basic";
import { simplify } from "../../expression/simplify";
import { variables as variables_in } from "../../expression/variables";
import { clean_assumptions } from "../clean";
import { linear_decomposition } from "../linear";
import { combine_assumptions } from "./combine";
import type { AssumptionStore } from "../store";

/**
 * Assumptions that can be stated about `expr` itself, skipping anything that
 * mentions `exclude_variables`.
 *
 * When `expr` is linear in its variables, a fact about one of them can be
 * rewritten as a fact about `expr`: with `q > x` on file, `q - x` is `> 0`.
 * Otherwise the assumptions on the individual variables are returned as they
 * stand.
 */
export function get_assumptions_for_expr(
  store: AssumptionStore,
  expr: any,
  exclude_variables: string[],
): any {
  let variables = variables_in(expr).filter(
    (v: string) => !exclude_variables.includes(v),
  );

  if (variables.length === 0) return undefined;

  const decomposition = linear_decomposition(expr, variables);

  if (!decomposition) {
    // Not linear: fall back to the assumptions on each variable.
    const results: any[] = [];
    for (const v of variables_in(expr)) {
      const res = get_assumptions_for_expr(store, v, exclude_variables);
      if (res !== undefined) results.push(res);
    }
    if (results.length === 0) return undefined;
    if (results.length === 1) return results[0];
    return ["and", ...results];
  }

  const coefficients = decomposition.coefficients;

  // `expr` is the variable itself, so a containment (`x elementof A`) can be
  // carried over verbatim; for any other linear combination it could not.
  const identity =
    trees.equal(decomposition.b, 0) &&
    trees.equal(coefficients[variables[0]], 1) &&
    variables.length === 1;

  const new_assumptions = store.get_assumptions([variables], {
    exclude_variables: exclude_variables,
  });

  if (new_assumptions === undefined) return undefined;

  return clean_assumptions(process_additional_assumptions(new_assumptions));

  function process_additional_assumptions(new_as: any): any {
    if (!Array.isArray(new_as)) return undefined;

    const operator = new_as[0];
    const operands = new_as.slice(1);

    if (operator === "and" || operator === "or") {
      const results = operands
        .map(process_additional_assumptions)
        .filter((v: any) => v !== undefined);

      if (results.length === 0) return undefined;
      // An `or` survives only if every branch does; a partial disjunction
      // would claim more than is known.
      if (operator === "or") {
        if (results.length === operands.length) return ["or", ...results];
        return undefined;
      }
      if (results.length === 1) return results[0];
      return ["and", ...results];
    }

    if (
      !(
        ["=", "ne", "<", "le"].includes(operator) ||
        (["in", "notin", "subset", "notsubset"].includes(operator) && identity)
      )
    ) {
      return with_assumptions_on_other_variables(new_as);
    }

    const results: any[] = [];

    for (let ind = 0; ind <= 1; ind++) {
      const next_var = operands[ind];
      const next_rhs = operands[1 - ind];

      if (typeof next_var === "string" && variables.includes(next_var)) {
        const new_expr = simplify(
          trees.substitute(expr, { [next_var]: next_rhs }),
        );

        // Two things can reverse the relation: a negative coefficient in
        // `expr`, and reading the stored relation from its right-hand side.
        // Both at once cancel out.
        let flip = false;
        let operator_eff = operator;
        const coefficient = numeric_coefficient(coefficients[next_var]);
        if (
          coefficient !== undefined &&
          ((ind === 1 && coefficient > 0) || (ind === 0 && coefficient < 0))
        ) {
          const reversed = {
            "<": ">",
            le: "ge",
            in: "ni",
            subset: "superset",
            notin: "notni",
            notsubset: "notsuperset",
          }[operator];
          if (reversed) {
            flip = true;
            operator_eff = reversed;
          }
        }

        if (flip) results.push([operator, new_expr, expr]);
        else results.push([operator, expr, new_expr]);

        // Chase whatever is known about the substituted expression.
        const new_exclude = exclude_variables.concat([next_var]);
        let res = get_assumptions_for_expr(store, new_expr, new_exclude);
        res = combine_assumptions(expr, operator_eff, new_expr, res);

        if (res !== undefined) results.push(res);
      }
    }

    if (results.length === 1) return results[0];
    if (results.length > 1) return ["and", ...results];

    return with_assumptions_on_other_variables(new_as);
  }

  function with_assumptions_on_other_variables(new_as: any): any {
    const new_exclude = exclude_variables.concat(variables_in(expr));
    const results: any[] = [];
    for (const v of variables_in(new_as)) {
      if (new_exclude.includes(v)) continue;
      const res = get_assumptions_for_expr(store, v, new_exclude);
      if (res !== undefined) results.push(res);
    }
    if (results.length === 0) return new_as;
    if (results.length === 1) return ["and", new_as, results[0]];
    return ["and", new_as, ...results];
  }
}

function numeric_coefficient(tree: any): number | undefined {
  if (typeof tree === "number") return tree;
  if (Array.isArray(tree) && tree[0] === "-" && typeof tree[1] === "number")
    return -tree[1];
  return undefined;
}
