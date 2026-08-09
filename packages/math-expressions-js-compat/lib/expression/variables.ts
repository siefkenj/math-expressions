// `me.variables` / `me.operators` on a raw tree, backed by the core's
// `ops::query`. Nothing is decided here: the walk that drops application heads
// and de-duplicates in first-appearance order is the Rust one, reached through
// an `Expression` handle because a tree is what the callers hold.
import wasm from "../_wasm";
import { astToJson } from "../converters/ast-json";
import { get_tree } from "../trees/util";

/**
 * Run a query method over a raw tree.
 *
 * A tree the core cannot read answers `[]`. These callers are normalization
 * passes over trees built by JS-side surgery, and every one of them treats an
 * empty answer as "nothing to do here" — which is the right outcome for a node
 * whose variables cannot be determined, and better than throwing out of a
 * normalization.
 */
function query(expr_or_tree: any, run: (src: any) => string[]): string[] {
  let src;
  try {
    src = wasm.from_ast(astToJson(get_tree(expr_or_tree)));
  } catch {
    return [];
  }
  try {
    return run(src);
  } finally {
    src.free(); // throwaway: parse source, never returned
  }
}

/**
 * The free variables, in first-appearance order. `include_subscripts` folds
 * `x_1` into the single name `x_1` instead of reporting the base `x`.
 */
export function variables(
  expr_or_tree: any,
  include_subscripts = false,
): string[] {
  return query(expr_or_tree, (src) => {
    if (!include_subscripts) return src.variables();
    const flat = src.subscripts_to_strings();
    try {
      return flat.variables();
    } finally {
      flat.free(); // throwaway: method result, never returned
    }
  });
}

/** The operator heads used, in first-appearance order. */
export function operators(expr_or_tree: any): string[] {
  return query(expr_or_tree, (src) => src.operators());
}
