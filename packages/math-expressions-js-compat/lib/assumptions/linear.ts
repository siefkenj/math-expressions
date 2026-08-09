// Linear algebra over the assumption trees, backed by `grade::linear`: isolate
// a variable in a relation (`a + b < 1` filed under `a` becomes `a < 1 - b`)
// and read an expression as affine in its variables (`3a + 4b`), which is what
// lets `get_assumptions` restate a fact in terms of a compound expression.
import wasm from "../_wasm";
import { astToJson, jsonToAst } from "../converters/ast-json";

/**
 * Restate a relation with `variable` alone on the left. Returns undefined when
 * the relation is not linear in `variable`, or when the sign of the
 * coefficient — which decides whether an inequality flips — is unknown.
 */
export function solve_linear(tree: any, variable: any): any {
  if (typeof variable !== "string") return undefined;
  if (!Array.isArray(tree)) return undefined;

  const out = wasm.solve_linear_ast(astToJson(tree), variable);
  return out === undefined ? undefined : jsonToAst(out);
}

/**
 * Decompose `tree` as `b + Σ aᵢ·vᵢ` over `variables`, with every coefficient a
 * number. Returns undefined if it is not of that form.
 */
export function linear_decomposition(
  tree: any,
  variables: string[],
): { b: any; coefficients: Record<string, any> } | undefined {
  const out = wasm.linear_decomposition_ast(astToJson(tree), variables);
  if (out === undefined) return undefined;

  // The core answers positionally; the callers index by name.
  const { b, coefficients } = jsonToAst(out) as { b: any; coefficients: any[] };
  const by_name: Record<string, any> = {};
  variables.forEach((v, i) => {
    by_name[v] = coefficients[i];
  });
  return { b, coefficients: by_name };
}
