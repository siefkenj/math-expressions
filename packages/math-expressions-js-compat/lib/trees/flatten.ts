// Raw JS-tree utilities (`me.utils.flatten` / `unflatten*` / `match`), backed by
// the wasm ports which take/return the JSON tree encoding.
import wasm from "../_wasm";
import { astToJson, jsonToAst } from "../converters/ast-json";

// `astToJson`/`jsonToAst` rather than bare `JSON.stringify`/`JSON.parse`: these
// take trees straight from `.tree`, which hands out real `Infinity`/`NaN`, and
// `JSON.stringify(Infinity)` is `null`. That turned `me.utils.flatten(e.tree)`
// into a silently wrong tree — no throw, just a `null` where a value was.
function viaWasm(fn, tree) {
  if (!Array.isArray(tree)) return tree;
  const out = fn(astToJson(tree));
  return out === undefined ? tree : jsonToAst(out);
}

export function flatten(tree) {
  return viaWasm(wasm.flatten_ast, tree);
}
export function unflattenLeft(tree) {
  return viaWasm(wasm.unflatten_left, tree);
}
export function unflattenRight(tree) {
  return viaWasm(wasm.unflatten_right, tree);
}

/** JS reimplementation of `flatten.allChildren` (flatten same-operator nests). */
export function allChildren(tree) {
  if (!Array.isArray(tree)) return tree;
  const op = tree[0];
  const associative = ["+", "*", "and", "or", "union", "intersect"].includes(op);
  const out = [];
  for (const operand of tree.slice(1)) {
    if (associative && Array.isArray(operand) && operand[0] === op) {
      out.push(...allChildren(operand));
    } else {
      out.push(operand);
    }
  }
  return out;
}

/**
 * Normalize the JS `match` params into the JSON the wasm option decoder takes.
 *
 * Lives here rather than in `math-expressions.ts` because that module imports
 * *this* one; both entry points share it so they cannot drift, which is what
 * let `me.utils.match` keep dropping its params after `expr.match` learned to
 * honor them.
 */
export function normalizeMatchOptions(options) {
  const opts: Record<string, unknown> = {};
  if (options.variables !== undefined) {
    const vars: Record<string, unknown> = {};
    for (const [name, kind] of Object.entries(options.variables)) {
      if (typeof kind === "function") {
        throw new Error(
          `match: 'variables.${name}' is a predicate function, which cannot cross ` +
            'the wasm boundary. Declare a kind instead: "number", "variable", ' +
            '"any" (or true).',
        );
      }
      vars[name] = kind;
    }
    opts.variables = vars;
  }
  if (options.allow_permutations !== undefined) {
    opts.allow_permutations = !!options.allow_permutations;
  }
  if (options.allow_implicit_identities !== undefined) {
    const ii = options.allow_implicit_identities;
    opts.allow_implicit_identities = Array.isArray(ii) ? ii : !!ii;
  }
  return opts;
}

/**
 * Template match; `false` when it does not match.
 *
 * `params` is honored rather than dropped. Ignoring it silently was the exact
 * "confidently wrong bindings" failure the option decoder exists to prevent:
 * with no params every string leaf in the pattern is a wildcard, so a caller
 * who declared two parameters got a match on three.
 */
export function match(tree, pattern, params?) {
  const hasParams =
    params !== null && typeof params === "object" && !Array.isArray(params);
  const res = hasParams
    ? wasm.match_template_with_options(
        astToJson(tree),
        astToJson(pattern),
        JSON.stringify(normalizeMatchOptions(params)),
      )
    : wasm.match_template(astToJson(tree), astToJson(pattern));
  return res === undefined ? false : jsonToAst(res);
}
