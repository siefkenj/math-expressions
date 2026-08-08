// Barrel for the assumptions machinery: the three-valued predicates that read
// the wasm store, and the JS-side store behind `me.add_assumption` /
// `me.get_assumptions`.
export * from "./element_of_sets";
export { AssumptionStore } from "./store";
export { clean_assumptions, normalize } from "./clean";
export { expand_relations } from "./expand_relations";
export { simplify_logical, flatten_logical } from "./logical";
export { solve_linear, linear_decomposition } from "./linear";
export {
  calculate_derived_assumptions,
  get_assumptions_for_expr,
  combine_assumptions,
} from "./derive";
