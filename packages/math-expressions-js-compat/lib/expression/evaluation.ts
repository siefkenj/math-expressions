// Lean compat shim for the legacy `evaluate_to_constant`. The full legacy
// implementation pulled in a large graph (ast-to-finite-field, standard_form,
// units, ...). The polynomial code only ever inspects the result with
// `Number.isFinite(...)`, so we delegate to the wasm-backed expression method
// and collapse anything that is not a finite real number to NaN.
import me from "../math-expressions";

function evaluate_to_constant(tree: any, _opts?: any): any {
  let v: any;
  try {
    v = (me as any).fromAst(tree).evaluate_to_constant();
  } catch (e) {
    return NaN;
  }
  if (typeof v === "number") return v;
  // null (free variables / undefined) or a complex value: not a finite real
  return NaN;
}

export { evaluate_to_constant };
