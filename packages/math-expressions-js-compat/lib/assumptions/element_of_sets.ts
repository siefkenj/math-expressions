// The `is_integer` / `is_real` / … predicates. Each takes an Expression (and an
// optional assumptions source) and returns true / false / undefined, mapping to
// the wasm `Assumptions` three-valued predicates.
import wasm from "../_wasm";
import Context from "../math-expressions";

// Constructed lazily, like `Context._assumptionsHandle` and for the same
// reason: a `new wasm.Assumptions()` evaluated in this module's body would
// force the wasm load before a host had any chance to `setWasmModule`.
let emptyCache;
const empty = () => (emptyCache ??= new wasm.Assumptions());

function handleFor(assumptions) {
  // No explicit source: consult the context's live global assumptions, so
  // `is_real(me.fromText("x+y"))` sees `me.add_assumption(...)` state. The
  // original JS predicates defaulted to the global store this way; falling
  // back to an empty one made every no-argument query answer "unknown".
  if (!assumptions) return Context.assumptions ?? empty();
  // Our Context exposes its live handle as `.assumptions`.
  if (assumptions._assumptionsHandle) return assumptions._assumptionsHandle;
  if (typeof assumptions.is_integer === "function") return assumptions; // a raw handle
  return empty();
}

function rawExpr(expression) {
  if (expression && expression._w) return expression._w;
  return expression; // already a raw wasm handle
}

function predicate(name) {
  return function (expression, assumptions) {
    return handleFor(assumptions)[name](rawExpr(expression));
  };
}

export const is_integer = predicate("is_integer");
export const is_real = predicate("is_real");
export const is_complex = predicate("is_complex");
export const is_nonzero = predicate("is_nonzero");
export const is_nonnegative = predicate("is_nonnegative");
export const is_nonpositive = predicate("is_nonpositive");
export const is_positive = predicate("is_positive");
export const is_negative = predicate("is_negative");
