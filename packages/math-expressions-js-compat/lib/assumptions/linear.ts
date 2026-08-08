// Linear algebra over the assumption trees: isolate a variable in a relation
// (`a + b < 1` filed under `a` becomes `a < 1 - b`) and recognize an expression
// that is affine in its variables (`3a + 4b`), which is what lets
// `get_assumptions` restate a fact in terms of a compound expression.
//
// The legacy library did both with `trees.match` against a pattern whose
// coefficient slots carried a `isNumber` *predicate*. The compat `match` runs
// in wasm and rejects predicate variables outright (they cannot cross the
// boundary), so the coefficients are recovered by evaluation instead: for a
// linear `f`, `f(0)` is the constant term and `f(e_v) - f(0)` is the
// coefficient of `v`. A final subtraction confirms the expression really was
// linear, which is the part the pattern match used to guarantee.

import { simplify } from "../expression/simplify";
import * as trees from "../trees/basic";

/** A coefficient slot only counts as filled if it came out a plain number. */
function is_number(tree: any): boolean {
  if (typeof tree === "number") return true;
  if (Array.isArray(tree) && tree[0] === "-" && typeof tree[1] === "number")
    return true;
  return false;
}

function numeric_value(tree: any): number | undefined {
  if (typeof tree === "number") return tree;
  if (Array.isArray(tree) && tree[0] === "-" && typeof tree[1] === "number")
    return -tree[1];
  return undefined;
}

function subst(tree: any, variable: string, value: any): any {
  return trees.substitute(tree, { [variable]: value });
}

/**
 * Split `tree` into `coefficient · variable + remainder`, or return undefined
 * if it is not linear in `variable`. The remainder may mention other
 * variables; only the coefficient has to come out constant.
 */
function split_off_variable(
  tree: any,
  variable: string,
): { a: any; b: any } | undefined {
  const b = simplify(subst(tree, variable, 0));
  const at_one = simplify(subst(tree, variable, 1));
  const a = simplify(["+", at_one, ["-", b]]);

  // `a` and `b` are free of `variable` by construction, but the reconstruction
  // is what rules out a nonlinear `tree` (`x^2` would give `a = 1`, and only
  // this check notices that `x^2 - x` is not zero).
  const residual = simplify(["+", tree, ["-", ["+", ["*", a, variable], b]]]);
  if (!trees.equal(residual, 0)) return undefined;

  return { a, b };
}

const FLIPPED = { "<": ">", le: "ge", ">": "<", ge: "le" };

/**
 * Restate a relation with `variable` alone on the left. Returns undefined when
 * the relation is not linear in `variable`, or when the sign of the
 * coefficient — which decides whether an inequality flips — is unknown.
 */
export function solve_linear(tree: any, variable: any): any {
  if (typeof variable !== "string") return undefined;
  if (!Array.isArray(tree)) return undefined;

  let operator = tree[0];
  const operands = tree.slice(1);

  if (!["=", "ne", "<", "le", ">", "ge"].includes(operator)) return undefined;

  const lhs = simplify(["+", operands[0], ["-", operands[1]]]);

  const split = split_off_variable(lhs, variable);
  if (split === undefined) return undefined;

  const coefficient = numeric_value(split.a);
  // An unknown-sign coefficient cannot be divided through an inequality. The
  // legacy code consulted the assumptions here; the store only ever needs the
  // numeric case, and guessing would produce a fact that is not entailed.
  if (coefficient === undefined || coefficient === 0) return undefined;

  const result = simplify(["/", ["-", split.b], split.a]);

  if (coefficient < 0 && FLIPPED[operator]) operator = FLIPPED[operator];

  return [operator, variable, result];
}

/**
 * Decompose `tree` as `b + Σ aᵢ·vᵢ` over `variables`, with every coefficient a
 * number. Returns undefined if it is not of that form.
 */
export function linear_decomposition(
  tree: any,
  variables: string[],
): { b: any; coefficients: Record<string, any> } | undefined {
  let remainder = tree;
  const coefficients: Record<string, any> = {};

  for (const variable of variables) {
    const split = split_off_variable(remainder, variable);
    if (split === undefined || !is_number(split.a)) return undefined;
    coefficients[variable] = split.a;
    remainder = split.b;
  }

  if (!is_number(remainder)) return undefined;

  return { b: remainder, coefficients };
}
