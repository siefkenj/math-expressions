// Logical normalization for stored assumptions: push `not` down to the
// comparisons and collapse nested `and`/`or`.
//
// These bind `normalize::push_not` / `normalize::flatten_logical` rather than
// `Expression.simplify_logical()`, which does the same rewriting but *also*
// simplifies and canonicalizes each relation, turning `x > a` into `a < x`. The
// assumptions store has to keep the shape of the relation it was handed until
// `clean_assumptions` orders it, so it needs the pushdown on its own.
import wasm from "../_wasm";
import { astToJson, jsonToAst } from "../converters/ast-json";

// A tree the core cannot read comes back unchanged: this is a normalization,
// and the store would rather file a fact as written than lose it.
function viaWasm(fn: (json: string) => string | undefined, tree: any): any {
  if (!Array.isArray(tree)) return tree;
  const out = fn(astToJson(tree));
  return out === undefined ? tree : jsonToAst(out);
}

/** Collapse `and`/`or` nested inside the same operator into one n-ary node. */
export function flatten_logical(tree: any): any {
  return viaWasm(wasm.flatten_logical_ast, tree);
}

/**
 * De Morgan plus double-negation elimination, driving `not` down to the
 * comparisons, where it becomes the complementary relation — stated on the
 * same operands (`not(x < a)` is `x >= a`, not `a <= x`), so the operand order
 * the caller wrote survives the rewrite.
 *
 * A `not` that cannot be pushed any further (over an operator with no
 * complement) is left in place rather than dropped — the store treats it as an
 * opaque fact.
 */
export function simplify_logical(tree: any): any {
  return viaWasm(wasm.push_not_ast, tree);
}

export default simplify_logical;
