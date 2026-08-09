// Barrel for the assumptions machinery: the three-valued predicates that read
// the wasm store, and the marshalling behind `me.add_assumption` /
// `me.get_assumptions`.
export * from "./element_of_sets";
export * as store from "./store";
export { simplify_logical, flatten_logical } from "./logical";
export { solve_linear, linear_decomposition } from "./linear";
