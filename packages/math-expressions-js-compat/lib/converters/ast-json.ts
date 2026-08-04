// Encoding an AST as the JSON the wasm `from_ast` reads.
//
// `JSON.stringify` has no representation for ±Infinity or NaN — it emits
// `null`, which `from_ast` rejects outright ("unexpected value null"). The
// wire format tags them instead, so anything handing a tree to wasm has to
// replace them on the way out. This lives on its own so the converters and
// `Expression.fromAst` tag them identically rather than one of them forgetting.

/** The tagged form of a non-finite number, or the value unchanged. */
export function tagNonFinite(value: unknown): unknown {
  if (typeof value === "number" && !Number.isFinite(value)) {
    if (Number.isNaN(value)) return { $: "NaN" };
    return { $: value > 0 ? "Inf" : "-Inf" };
  }
  return value;
}

/** `JSON.stringify` of a plain AST, with non-finite numbers tagged. */
export function astToJson(ast: unknown): string {
  return JSON.stringify(ast, (_key, value) => tagNonFinite(value));
}
