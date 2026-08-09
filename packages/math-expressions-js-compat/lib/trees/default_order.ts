// `default_order` is a standalone per-tree ordering pass: it sorts the operands
// of the commutative operators and states each relation in one canonical
// direction (`x > a` becomes `a < x`), so that two spellings of the same fact
// compare equal.
//
// It used to be a stub returning the tree unchanged, on the reading that the
// Rust core folded ordering into `canonicalize` with no separate entry point.
// It does have one — `Expression.default_order()`, backed by
// `normalize::default_order` — and the assumptions store needs it: an
// assumption is filed with the variable it is about on the left, so `a > b` and
// `b < a` reach the store as different trees and only this pass reconciles
// them.
import wasm from "../_wasm";
import { astToJson, jsonToAst } from "../converters/ast-json";

export function default_order(tree) {
  // Leaves have nothing to order, and routing one through the core would cost
  // a wasm round trip to get it back unchanged.
  if (!Array.isArray(tree)) return tree;
  let src;
  try {
    src = wasm.from_ast(astToJson(tree));
  } catch {
    // Callers hand this arbitrary trees, including partial ones built by tree
    // surgery that the core cannot read back. Ordering is a normalization, so
    // an unreadable tree is returned as it came rather than throwing.
    return tree;
  }
  try {
    const out = src.default_order();
    try {
      return jsonToAst(out.tree_json());
    } finally {
      out.free(); // throwaway: method result, never returned
    }
  } finally {
    src.free(); // throwaway: parse source, never returned
  }
}

export default default_order;

// ---------------------------------------------------------------------------
/**
 * A total order on trees — the legacy `compare_function`, backed by the core's
 * `cmp_default_order`. The compat polynomial module orders its variables with
 * it, which is why it has to be the *legacy* sort key and not the core's own
 * canonical order: the chosen leading variable shows up in the polynomial ASTs
 * that API hands back.
 *
 * Additive; it does not affect `default_order` above. A tree the core cannot
 * read ties with everything, leaving the caller's array in the order it was.
 *
 * The legacy `params.ignore_negatives` is gone with the JS key it configured:
 * no caller ever set it.
 */
export function compare_function(a: any, b: any): number {
  return wasm.cmp_default_order(astToJson(a), astToJson(b)) ?? 0;
}
