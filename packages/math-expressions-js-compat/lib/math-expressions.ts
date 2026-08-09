// Drop-in replacement for the original `lib/math-expressions.js` default export
// (the `Context` factory + `Expression`), backed by the Rust/wasm core.
//
// Not every legacy method exists on the Rust side; those that don't are either
// approximated, or throw a clear "not implemented in js-compat" so the calling
// test fails cleanly (the suite still runs). See JS_TEST_COVERAGE_AUDIT.md.
import wasm, { onWasmModuleChange, setWasmModule } from "./_wasm";
import math from "./mathjs";
import {
  match,
  flatten,
  unflattenLeft,
  unflattenRight,
  normalizeMatchOptions,
} from "./trees/flatten";
import * as converters from "./converters/index";
import { jsonToAst, tagNonFinite } from "./converters/ast-json";
import * as assumptionStore from "./assumptions/store";
import { expression_to_polynomial } from "./polynomial/polynomial";
import { get_tree } from "./trees/util";
import { compileRustExpr } from "math-expressions-rs-wasm";
import type { WasmExpression } from "math-expressions-rs-wasm";
import type { MathJsInstance } from "mathjs";

/** The JS AST tree encoding (`["+", 1, "x", 3]`). */
export type Tree = number | string | boolean | Tree[];

/** Anything that can be coerced to an Expression. */
export type ExpressionLike = Expression | WasmExpression | Tree;

/** Legacy `.equals` grading options (snake_case or camelCase keys). */
export type EqualityOptions = Record<string, unknown>;

/** `.substitute` / `.evaluate` bindings. */
export type Bindings = Record<string, ExpressionLike>;

/** Type guard mirroring the original `isTree`. */
export function isTree(value: unknown): boolean {
  if (
    typeof value === "number" ||
    typeof value === "string" ||
    typeof value === "boolean"
  ) {
    return true;
  }
  if (
    Array.isArray(value) &&
    value.length > 0 &&
    typeof value[0] === "string"
  ) {
    return value.slice(1).every((item) => isTree(item));
  }
  return false;
}

function notImplemented(name: string): (...args: unknown[]) => never {
  return function () {
    throw new Error(`math-expressions-js-compat: ${name}() is not implemented`);
  };
}

/** Wrap a raw wasm Expression handle (or undefined) as a compat Expression. */
function wrap(
  handle: WasmExpression | undefined,
  context: Ctx,
): Expression | undefined {
  if (handle === undefined || handle === null) return undefined;
  return new Expression(handle, context);
}

/** Coerce a value (Expression | wasm handle | string | number | AST) → Expression. */
function toExpr(x: ExpressionLike, context?: Ctx): Expression {
  const ctx = context || Context;
  if (x instanceof Expression) return x;
  if (x && typeof (x as WasmExpression).tree_json === "function") {
    return new Expression(x as WasmExpression, ctx);
  }
  if (typeof x === "string") return ctx.fromText(x);
  return ctx.fromAst(x as Tree); // number or AST array
}

/**
 * A component index is either a bare index or a path of them — `get_component(2)`
 * and `get_component([2, 1, 2])` are both legal, the first being the one-element
 * path. Indices count operands of the tree spelling, 0-based.
 */
function componentPath(component: number | number[]): Uint32Array {
  const path = Array.isArray(component) ? component : [component];
  return Uint32Array.from(path, (i) => Number(i));
}

/**
 * `JSON.stringify` replacer that preserves the non-finite numbers JSON cannot
 * hold. `JSON.stringify(NaN) === "null"` and likewise for `±Infinity`, so a
 * `NaN` slope or an infinite bound would reach the Rust boundary as `null` and
 * be rejected — the tree is serialized here on the way in, and this maps those
 * three values to the `{"$":…}` specials the Rust `from_ast` already reads back.
 * An already-special `{"$":"NaN"}` object passes through untouched.
 *
 * The *wire* format is tagged in both directions, because JSON cannot hold
 * these three values in either one. The *values a caller sees* are not: `.tree`
 * untags them back to JS scalars (see `untagNonFinite`), because `Infinity` is
 * what legacy handed back and what `typeof x === "number"` and `x === -Infinity`
 * consumers test against. `fromAst(x).tree` is still a fixpoint — this replacer
 * re-tags on the way in — it just holds at the value level rather than the wire
 * level. `{"$":"None"}` is the exception in both directions: it has no JS scalar
 * to untag to, and DoenetML emits and reads it in that form already.
 */
function astReplacer(this: unknown, key: string, value: unknown): unknown {
  // An `Expression` standing where a tree is expected — `fromAst(expr)`, or an
  // `expr` nested inside one (`["+", someExpr, 2]`). A math-valued DoenetML
  // state variable *holds* an Expression, so code that re-wraps one hands it
  // straight back here; this makes that a no-op instead of a throw.
  //
  // Note this reads the *holder* rather than `value`: `JSON.stringify` calls
  // `toJSON()` before consulting the replacer, so by the time `value` arrives an
  // Expression has already become its `{objectType:"math-expression",tree:…}`
  // envelope and `value instanceof Expression` is always false. That envelope is
  // precisely the "object with no `$` key" the Rust side used to reject.
  //
  // Both unwrapped trees go back through `tagNonFinite`: `.tree` hands out the
  // *untagged* scalars, so an `Expression` holding `NaN` or `±Infinity` would
  // otherwise be returned as a bare JS non-finite and `JSON.stringify` would
  // write `null` for it — the "unexpected value null" the Rust side rejects.
  // (Nested ones are covered by the fall-through below, which `stringify`
  // reaches when it walks into the value returned here.)
  const held = (this as Record<string, unknown> | undefined)?.[key];
  if (held instanceof Expression) return tagNonFinite(held.tree);
  // The same envelope arriving as plain data — a `JSON.parse` of a persisted
  // expression that never got run through `Context.reviver`. Keyed on the shape
  // `reviver` itself recognizes.
  if (isSerializedExpression(value)) return tagNonFinite(value.tree);
  // Shared with the standalone converters, so the two cannot tag `Infinity`
  // differently (see `converters/ast-json.ts`).
  return tagNonFinite(value);
}

/** The `toJSON()` envelope shape, as `Context.reviver` recognizes it. */
function isSerializedExpression(v: unknown): v is { tree: unknown } {
  return (
    !!v &&
    typeof v === "object" &&
    (v as { objectType?: unknown }).objectType === "math-expression" &&
    (v as { tree?: unknown }).tree !== undefined
  );
}

/**
 * Whether a call carries options worth forwarding to the wasm
 * `*_with_options` entry points — render options (padToDigits, padToDecimals,
 * showBlanks, explicitMultiplicationSymbols, notation, unicode) or parser
 * options (splitSymbols, appliedFunctionSymbols, …). An empty/absent object
 * takes the cheaper no-options path.
 */
function hasOptions(opts: unknown): opts is Record<string, unknown> {
  return !!opts && typeof opts === "object" && Object.keys(opts).length > 0;
}

/** A variable argument may be a string name or an Expression of a symbol. */
function varName(v: string | Expression): string {
  if (typeof v === "string") return v;
  if (v instanceof Expression) return v.toString();
  return String(v);
}

/** The tree heads `get_component` will index — the JS library's set. */
const COMPONENT_CONTAINERS = new Set([
  "list",
  "tuple",
  "vector",
  "altvector",
  "array",
]);

/** The Context (`me`) shape, used for the back-reference on each Expression. */
type Ctx = typeof Context;

// Legacy `.equals` options are snake_case; the wasm `equals_with_options` takes
// camelCase JSON keys. Map the ones the Rust side understands; drop the rest.
const EQ_OPTION_KEYS: Record<string, string> = {
  relative_tolerance: "relativeTolerance",
  absolute_tolerance: "absoluteTolerance",
  tolerance_for_zero: "toleranceForZero",
  allowed_error_in_numbers: "allowedErrorInNumbers",
  include_error_in_number_exponents: "includeErrorInNumberExponents",
  allowed_error_is_absolute: "allowedErrorIsAbsolute",
  allow_blanks: "allowBlanks",
  coerce_tuples_arrays: "coerceTuplesArrays",
  coerce_vectors: "coerceVectors",
};
function mapEqOptions(opts: EqualityOptions): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(opts)) {
    if (EQ_OPTION_KEYS[k]) out[EQ_OPTION_KEYS[k]] = v;
    else if (Object.values(EQ_OPTION_KEYS).includes(k)) out[k] = v; // already camelCase
  }
  return out;
}

/**
 * Handles for *recurring* atomic trees, and their (primitive) `.tree` readback.
 *
 * Evaluating a function over a domain drives `fromAst` in a tight loop, and
 * overwhelmingly on an atom: in one DoenetML `<evaluate>` test, 2.13M of 2.14M
 * calls were a bare number or the blank `"＿"` a domain miss returns, and
 * re-parsing those through `JSON.stringify` + `from_ast` was 40% of the run.
 * The handles are immutable, so a repeated atom can be built once and shared.
 *
 * The catch is which atoms actually repeat. Symbols, the blank, and small
 * integers do — 1.39M of those 2.13M calls were the single string `"＿"`. A
 * *sampled coordinate* does not: an interpolated function is evaluated at
 * millions of distinct floats, and caching those turns every call into a miss
 * plus table churn and holds a wasm handle per sample alive until the next
 * sweep. That is not merely a wash, it is a large loss: caching every atom
 * took the interpolated-function test from 77s to 153s while taking the
 * blank-driven one from 173s to 105s. Restricting the cache to strings and
 * small integers gives 78s and 4s — both faster than either. So an arbitrary
 * float goes straight to `from_ast`.
 *
 * `MAX_ATOMS` still bounds the table, since symbol names are unbounded over a
 * long session; on overflow it is dropped wholesale rather than evicted one at
 * a time, the working set being small and an atom cheap to re-parse.
 */
const ATOM_HANDLES = new Map<string, WasmExpression>();
const ATOM_TREES = new WeakMap<WasmExpression, unknown>();
/**
 * Handles the atom cache owns. A shared handle outlives any one wrapper, so
 * `free()` on a wrapper around one must not release it — see `free`.
 */
const ATOM_SHARED = new WeakSet<WasmExpression>();
/**
 * Live wrappers per shared handle, and the key each was cached under.
 *
 * Together these let `free()` release a handle the cache has since dropped
 * (`MAX_ATOMS` overflow, or a wasm-module swap) instead of leaving it to the
 * GC. Counting happens in the `Expression` constructor rather than in
 * `fromAst`, so a wrapper minted by any other route — `wrap`, the reviver, a
 * wasm call that hands back the same handle — is counted too; miss one and
 * `free()` would release a handle another live wrapper still points at.
 *
 * A wrapper that is garbage-collected without `free()` never decrements, which
 * only ever *inhibits* the release. The FinalizationRegistry is still the
 * backstop, so the failure direction is "freed late", never "freed early".
 */
const ATOM_REFS = new WeakMap<WasmExpression, number>();
const ATOM_KEYS = new WeakMap<WasmExpression, string>();
const MAX_ATOMS = 4096;

// Handles belong to the module that minted them, so a swap invalidates every
// cached one — passing a stale handle to the new module's `from_ast` fails with
// "expected instance of Expression". Dropping the table is enough: wrappers
// already handed out keep working against their own module, and the orphaned
// handles are released by `free()` or the GC as usual.
onWasmModuleChange(() => ATOM_HANDLES.clear());
/** Integers up to this magnitude are treated as recurring; see `atomKey`. */
const MAX_CACHED_INT = 1024;

/** Cache key for an atomic tree, or `undefined` if it is not worth caching. */
function atomKey(ast: unknown): string | undefined {
  if (typeof ast === "string") return "s" + ast;
  if (
    typeof ast === "number" &&
    Number.isInteger(ast) &&
    Math.abs(ast) <= MAX_CACHED_INT &&
    // `-0` and `0` are distinct expressions (see `Number::NegZero`); rather
    // than spell the sign into the key, leave `-0` uncached — it is rare, and
    // an uncached atom is correct, just not free.
    !Object.is(ast, -0)
  ) {
    return "n" + ast;
  }
  return undefined;
}

class Expression {
  _w: WasmExpression;
  context: Ctx;

  constructor(handle: WasmExpression, context?: Ctx) {
    this._w = handle;
    this.context = context || Context;
    if (ATOM_SHARED.has(handle)) {
      ATOM_REFS.set(handle, (ATOM_REFS.get(handle) ?? 0) + 1);
    }
  }

  // ---- inspection / rendering ----
  /**
   * The AST as plain JS data. `±Infinity` and `NaN` read back as the JS
   * scalars legacy handed out, not as their `{"$":…}` wire tags — see
   * `untagNonFinite`. `{"$":"None"}` stays tagged, having no scalar to become.
   */
  get tree() {
    // Memoized only when the tree is a primitive (a number, or a symbol/blank
    // string). A composite tree is handed out as a fresh array every read and
    // callers are free to mutate what they get back, so those must not be
    // shared; a primitive has nothing to mutate. Keyed on the wasm handle
    // rather than the wrapper because handles are immutable and are shared by
    // the atom cache below — `fromAst("＿")` is the single hottest call in
    // a function-evaluation loop, and this makes its `.tree` free after the
    // first read.
    const cached = ATOM_TREES.get(this._w);
    if (cached !== undefined) return cached;
    const tree = jsonToAst(this._w.tree_json());
    if (tree === null || typeof tree !== "object")
      ATOM_TREES.set(this._w, tree);
    return tree;
  }
  // Rendering honors the legacy render options (padToDigits, padToDecimals,
  // showBlanks, explicitMultiplicationSymbols, notation/unicode) by forwarding
  // a non-empty options object to the `*_with_options` wasm entry points. The
  // no-arg path stays on the cheap no-options render — `toString()` is what JS
  // coercion (`String(expr)`) calls.
  toString(opts?) {
    return hasOptions(opts)
      ? this._w.to_text_with_options(JSON.stringify(opts))
      : this._w.to_text();
  }
  toText(opts?) {
    return hasOptions(opts)
      ? this._w.to_text_with_options(JSON.stringify(opts))
      : this._w.to_text();
  }
  toLatex(opts?) {
    return hasOptions(opts)
      ? this._w.to_latex_with_options(JSON.stringify(opts))
      : this._w.to_latex();
  }
  tex(opts?) {
    return hasOptions(opts)
      ? this._w.to_latex_with_options(JSON.stringify(opts))
      : this._w.to_latex();
  }
  toJSON() {
    return JSON.parse(this._w.to_serialized());
  }
  variables() {
    return this._w.variables();
  }
  functions() {
    return this._w.functions();
  }
  /**
   * This expression read as a polynomial — `["polynomial", v, [[deg, coeff], …]]`
   * — or `false` when it is not one. See `lib/polynomial/polynomial`.
   */
  expression_to_polynomial() {
    return expression_to_polynomial(this.tree);
  }

  // ---- equality ----
  equals(other, options) {
    const o = toExpr(other, this.context);
    if (options && Object.keys(options).length > 0) {
      return this._w.equals_with_options(
        o._w,
        JSON.stringify(mapEqOptions(options)),
      );
    }
    return this._w.equals(o._w);
  }
  equalsViaReal(other) {
    return this._w.equals_via_real(toExpr(other, this.context)._w);
  }
  // via-complex is a numerical variant — the wasm `equals` (complex sampling).
  equalsViaComplex(other) {
    return this._w.equals(toExpr(other, this.context)._w);
  }
  // via-syntax is a *structural* comparison — NOT numerical. The wasm exposes it
  // through `structural_equality` with the `sameStructure` criterion, which
  // routes to Rust `equals_syntactic` (no sampling). This matches the original
  // `equalsViaSyntax` and never evaluates the expression at sample points.
  equalsViaSyntax(other, options?) {
    const o = toExpr(other, this.context)._w;
    if (hasOptions(options)) {
      return this._w.structural_equality_with_options(
        o,
        '"sameStructure"',
        JSON.stringify(mapEqOptions(options)),
      );
    }
    return this._w.structural_equality(o, '"sameStructure"');
  }
  is_zero() {
    return this._w.is_zero();
  }
  isAnalytic(opts) {
    const o = opts || {};
    return this._w.is_analytic(
      !!o.allow_abs,
      !!o.allow_arg,
      !!o.allow_relation,
    );
  }

  // ---- calculus ----
  derivative(v) {
    return wrap(this._w.derivative(varName(v)), this.context);
  }
  integrate(v) {
    return wrap(this._w.integrate(varName(v)), this.context);
  }
  // Best-effort numeric definite integral. Unlike the original JS (which always
  // returns an uncertified estimate), this is backed by the CERTIFIED
  // quadrature and returns `NaN` when the value cannot be certified — never a
  // silently-wrong number.
  integrateNumerically(v, lower, upper) {
    const r = this._w.integrate_numerically(
      varName(v),
      Number(lower),
      Number(upper),
    );
    return r === undefined ? NaN : r;
  }

  // ---- normalization / simplification ----
  simplify() {
    const a = this.context._assumptionTexts;
    return wrap(
      a && a.length ? this._w.simplify_with_assumptions(a) : this._w.simplify(),
      this.context,
    );
  }
  simplify_logical() {
    return wrap(this._w.simplify_logical(), this.context);
  }
  expand() {
    return wrap(this._w.expand(), this.context);
  }
  /**
   * Sort into the default order without evaluating — DoenetML's
   * `simplify="normalizeOrder"`. Unlike `simplify`, every term survives:
   * `0x^2` stays, `7+4` stays two terms, `1x^2` keeps its coefficient. The
   * ordering key is the JS library's, quirks included, because the term
   * sequence it produces is what gets displayed.
   */
  default_order() {
    return wrap(this._w.default_order(), this.context);
  }
  factor() {
    return wrap(this._w.factor(), this.context);
  }
  evaluate_numbers(opts) {
    // `skip_ordering` (DoenetML's `simplify="numberspreserveorder"`) selects a
    // genuinely different core pass: numbers fold only with *adjacent* numbers,
    // so `1+x+2` stays `1+x+2` where the ordering form gives `x+3`. It used to
    // throw here, which was worse than a missing feature — the Rust core calls
    // this mode and is built `panic = "abort"`, so the exception unwound into
    // it as a WASM trap and took the whole worker down.
    // `max_digits` is how many significant digits the caller is willing to
    // spend turning an exact value into a decimal. `Infinity` — spend as many
    // as it takes — folds `π` and `1/3` too, which is what makes
    // `2π + π + 6` comparable against a response typed as `15.42478`; it is
    // what DoenetML's grading path passes. A *finite* cap is not implemented:
    // it would have to decide per value whether the decimal fits the budget,
    // and silently ignoring it would round a student's value without saying so.
    const maxDigits = opts?.max_digits;
    if (maxDigits !== undefined && maxDigits !== Infinity) {
      throw new Error(
        `evaluate_numbers: 'max_digits' is only supported as Infinity (got ${maxDigits}). ` +
          "A finite digit budget is not implemented — pass Infinity to fold exact " +
          "values to floats, or omit it to keep them exact.",
      );
    }
    const skipOrdering = Boolean(opts?.skip_ordering);
    // `evaluate_functions` additionally folds a function applied to a numeric
    // argument (`sin(0)+2` → `2`), which is what `simplify="full"` needs.
    const evaluateFunctions = Boolean(opts?.evaluate_functions);
    if (maxDigits === Infinity) {
      return wrap(
        this._w.evaluate_numbers_to_floats(skipOrdering, evaluateFunctions),
        this.context,
      );
    }
    if (skipOrdering) {
      return wrap(this._w.evaluate_numbers_preserve_order(), this.context);
    }
    if (evaluateFunctions) {
      return wrap(this._w.evaluate_numbers_evaluate_functions(), this.context);
    }
    return wrap(this._w.evaluate_numbers(), this.context);
  }
  collect_like_terms_factors() {
    return wrap(this._w.collect_like_terms_factors(), this.context);
  }
  simplify_ratios() {
    return wrap(this._w.simplify_ratios(), this.context);
  }
  reduce_rational() {
    return wrap(this._w.reduce_rational(), this.context);
  }
  together() {
    return wrap(this._w.together(), this.context);
  }
  normalize_function_names() {
    return wrap(this._w.normalize_function_names(), this.context);
  }
  normalize_applied_functions() {
    return wrap(this._w.normalize_applied_functions(), this.context);
  }
  normalize_negative_numbers() {
    return wrap(this._w.normalize_negative_numbers(), this.context);
  }
  constants_to_floats() {
    return wrap(this._w.constants_to_floats(), this.context);
  }

  // ---- structural conversions ----
  tuples_to_vectors() {
    return wrap(this._w.tuples_to_vectors(), this.context);
  }
  altvectors_to_vectors() {
    return wrap(this._w.altvectors_to_vectors(), this.context);
  }
  to_intervals() {
    return wrap(this._w.to_intervals(), this.context);
  }
  // Move `+`/scalar-`*` inside vector & matrix containers so grading can slice
  // the result into components. Not arithmetic — it deliberately leaves `1+3`
  // rather than folding to `4` (`checkEquality` compares components under
  // tolerance). Mirrored onto `Context`, so `me.perform_…(expr)` works too.
  perform_vector_matrix_additions_scalar_multiplications() {
    return wrap(
      this._w.perform_vector_matrix_additions_scalar_multiplications(),
      this.context,
    );
  }
  // `force` also collapses a compound subscript, by its text spelling —
  // `(x^3)_2` becomes that seven-character symbol name.
  subscripts_to_strings(force = false) {
    return wrap(this._w.subscripts_to_strings(force), this.context);
  }
  strings_to_subscripts() {
    return wrap(this._w.strings_to_subscripts(), this.context);
  }
  copy() {
    return wrap(this._w.copy(), this.context);
  }

  // ---- lifetime ----
  // Every Expression owns a Rust/wasm handle that is otherwise only reclaimed by
  // the JS GC's FinalizationRegistry — too late for DoenetML's long-lived worker,
  // which mints a handle per state-variable eval and per state-JSON revive. `free`
  // releases it eagerly. Idempotent: the handle is nulled, so freeing twice is a
  // no-op rather than the wasm-memory corruption a double free would cause, and a
  // later method call fails on the null handle (a TypeError naming the method)
  // instead of reading through a dangling pointer.
  free() {
    const w = this._w as WasmExpression | undefined;
    if (!w) return;
    this._w = undefined as unknown as WasmExpression;
    if (!ATOM_SHARED.has(w)) {
      w.free();
      return;
    }
    // A handle from the atom cache is shared by every wrapper `fromAst` has
    // handed out for that atom, so releasing it on the first `free()` would
    // dangle the others. Release it only once this is the last live wrapper
    // *and* the cache itself has let go — after a `MAX_ATOMS` sweep or a wasm
    // swap the handle is an orphan nothing will hand out again, and leaving it
    // to the GC is what made `free()` a silent no-op for atoms. While the
    // handle is still cached it stays alive by design.
    const refs = (ATOM_REFS.get(w) ?? 0) - 1;
    ATOM_REFS.set(w, refs);
    const key = ATOM_KEYS.get(w);
    if (refs <= 0 && (key === undefined || ATOM_HANDLES.get(key) !== w)) {
      ATOM_SHARED.delete(w);
      w.free();
    }
  }
  // Aliases: `dispose()` and the `using`-statement protocol.
  dispose() {
    this.free();
  }

  // ---- component access ----
  // `component` is an operand index into the tree spelling, or a path of them
  // for nested components. A matrix is `["matrix", ["tuple", rows, cols],
  // ["tuple", <row-tuples>]]`, so an entry of one is `[1, row, col]`.
  /**
   * The `component`-th operand of a **container** — a list, tuple, vector,
   * altvector or array.
   *
   * **Throws** for anything else, which is the legacy contract and what
   * callers are written against: DoenetML wraps this in `try/catch` and reads
   * the throw as "not a container, use the value whole". Two things went wrong
   * without it. The wasm entry point indexes the operands of *any* operator
   * (its paths are over the flattened JS tree, which is right for what it is
   * used for internally), so `xyz` — a product — reported its first factor as
   * `.x`, and a scalar reported `undefined`, which read as a container holding
   * nothing.
   */
  get_component(component) {
    const t = this.tree;
    if (!Array.isArray(t) || !COMPONENT_CONTAINERS.has(t[0])) {
      throw Error(
        "Invalid get_component: expected list, tuple, vector, or array",
      );
    }
    const got = this._w.get_component(componentPath(component));
    if (got === undefined) {
      throw Error(
        "Invalid get_component: expected list, tuple, vector, or array",
      );
    }
    return wrap(got, this.context);
  }
  substitute_component(component, value) {
    return wrap(
      this._w.substitute_component(
        componentPath(component),
        toExpr(value, this.context)._w,
      ),
      this.context,
    );
  }

  // ---- numeric evaluator ----
  // The plotting / root-finding entry point: compile once through math.js, then
  // evaluate per sample. `compileRustExpr` normalizes function names Rust-side
  // and frees its own temporary handle; `this._w` is untouched.
  f() {
    // `./mathjs` re-exports either a created instance or the namespace itself,
    // so its static type is a union; the runtime value is always an instance.
    const compiled = compileRustExpr(math as MathJsInstance, this._w);
    return (bindings = {}) => compiled.evaluate(bindings);
  }

  /**
   * The critical points with respect to `variable` — the real solutions of
   * `d/dvariable = 0` — exactly, in increasing order.
   *
   * Three outcomes, and a caller that keeps a numerical fallback needs to tell
   * them apart: an array of points; an **empty** array, meaning there are
   * provably none; and `null`, meaning undecided — sample instead. Undecided
   * is a derivative that is not a rational function of `variable` (`cos(x)`,
   * which has infinitely many roots anyway), one carrying a free parameter
   * (`d/dx a·x²`, whose roots depend on `a`), or a constant-zero derivative,
   * where every point is critical and no finite list says so.
   *
   * Exact means exact: a rational root comes back as a number, an algebraic one
   * as the `rootof` form carrying its defining polynomial, and a repeated root
   * is listed once. Points where the derivative does not *exist* — the corner
   * of `|x|` — are not reported; they are critical in the textbook sense, but
   * finding them is not rational root-finding.
   */
  critical_points(variable) {
    const pts = this._w.critical_points(variable);
    return pts === undefined
      ? null
      : pts.map((p) => new Expression(p, this.context));
  }

  // ---- units ----
  remove_units(scaleBasedOnUnit) {
    return wrap(this._w.remove_units(!!scaleBasedOnUnit), this.context);
  }
  remove_scaling_units() {
    return wrap(this._w.remove_scaling_units(), this.context);
  }
  add_unit(unit) {
    return wrap(this._w.add_unit(unit), this.context);
  }
  set_small_zero(tolerance) {
    return wrap(
      this._w.set_small_zero(tolerance === undefined ? 1e-14 : tolerance),
      this.context,
    );
  }

  // ---- rounding ----
  round_numbers_to_precision(sigFigs) {
    return wrap(this._w.round_numbers_to_precision(sigFigs), this.context);
  }
  round_numbers_to_decimals(decimals) {
    return wrap(this._w.round_numbers_to_decimals(decimals), this.context);
  }
  round_numbers_to_precision_plus_decimals(digits, decimals) {
    return wrap(
      this._w.round_numbers_to_precision_plus_decimals(digits, decimals),
      this.context,
    );
  }

  // ---- evaluation ----
  // Legacy returned a plain number for a real value and a complex value for a
  // non-real one, so `fromText("i").evaluate_to_constant()` is `{re:0, im:1}`,
  // not null. The wasm entry point reports only the real case; the complex one
  // comes back through `evaluate_to_complex`, which applies the same
  // free-variable and undefined-leaf rules.
  //
  // The complex value is a math.js `Complex`, as legacy's was: callers pass it
  // straight into math.js functions (`divide(evaluate_to_constant(a), …)`),
  // which reject a plain object. A consumer that puts one into a *state
  // variable* should flatten it there — it is structured-cloned to the main
  // thread and arrives prototype-stripped either way.
  evaluate_to_constant() {
    const v = this._w.evaluate_to_constant();
    if (v !== undefined) return v;
    const c = this._w.evaluate_to_complex();
    return c === undefined ? null : math.complex(c[0], c[1]);
  }
  evaluate_to_complex() {
    const v = this._w.evaluate_to_complex();
    return v === undefined ? null : math.complex(v[0], v[1]);
  }
  evaluate(bindings) {
    const vars = Object.keys(bindings || {});
    const vals = Float64Array.from(vars.map((k) => Number(bindings[k])));
    const r = this._w.evaluate(vars, vals);
    return r === undefined ? NaN : r;
  }
  /**
   * Evaluate at many values of one variable in a single crossing.
   *
   * `evaluate` marshals the variable names on every call, which costs far more
   * than the arithmetic — measured at ~1.2µs a point against ~6ns of actual
   * work on `x²−3x+1`. Sampling a curve, scanning for extremum brackets or
   * hunting a root asks the same question thousands of times, and this pays
   * that overhead once.
   *
   * Any other variable is left unbound; `substitute` it first. The result is a
   * `Float64Array` the same length as `values`, with `NaN` wherever there is no
   * finite real value — a pole, a complex branch, an unbound variable — so it
   * lines up index-for-index with what was asked and the gaps carry the marker
   * consumers already test for.
   */
  evaluate_many(variable, values) {
    return this._w.evaluate_many(
      variable,
      values instanceof Float64Array ? values : Float64Array.from(values),
    );
  }
  /**
   * Replace variables by their bindings, one binding at a time.
   *
   * Sequential, as the JS library was, and deliberately: a replacement is
   * itself open to the bindings that follow it, which DoenetML depends on —
   * its `<math>` machinery substitutes generated *codes* whose values contain
   * further codes, and expects them to expand.
   *
   * The cost is capture: `sin(x+y)` with `{x: "10y", y: "-pi"}` gives
   * `sin(-10π − π)`, because the `y` the first binding introduced is still
   * there for the second. A caller replacing several *independent* variables
   * wants {@link substitute_all} instead.
   */
  substitute(bindings) {
    let cur = this._w;
    for (const k of Object.keys(bindings || {})) {
      const next = cur.substitute_var(k, toExpr(bindings[k], this.context)._w);
      // Free the prior intermediate handle (wrapper-owned); never `this._w`
      // (caller's own) and never the final handle we hand back via `wrap`.
      if (cur !== this._w) cur.free();
      cur = next;
    }
    return wrap(cur, this.context);
  }

  /**
   * Replace variables by their bindings **simultaneously** — no binding sees
   * another's replacement.
   *
   * This is what evaluating a multi-variable function at given arguments
   * needs: `f(x,y) = sin(x+y)` at `(10y, -π)` is `sin(10y − π)`, and
   * {@link substitute}'s left-to-right pass would turn the freshly-substituted
   * `y` into `-π` and answer `sin(-11π)`.
   */
  substitute_all(bindings) {
    const keys = Object.keys(bindings || {});
    if (keys.length === 0) return this;
    const map = {};
    for (const k of keys) map[k] = bindings[k];
    return wrap(
      this._w.substitute_map(JSON.stringify(map, astReplacer)),
      this.context,
    );
  }

  // ---- arithmetic ----
  add(other) {
    return wrap(this._w.add(toExpr(other, this.context)._w), this.context);
  }
  subtract(other) {
    return wrap(this._w.subtract(toExpr(other, this.context)._w), this.context);
  }
  multiply(other) {
    return wrap(this._w.multiply(toExpr(other, this.context)._w), this.context);
  }
  divide(other) {
    return wrap(this._w.divide(toExpr(other, this.context)._w), this.context);
  }
  pow(other) {
    return wrap(this._w.pow(toExpr(other, this.context)._w), this.context);
  }
  mod(other) {
    return wrap(this._w.mod(toExpr(other, this.context)._w), this.context);
  }

  // ---- matrices / vectors ----
  determinant() {
    return wrap(this._w.determinant(), this.context);
  }
  transpose() {
    return wrap(this._w.transpose(), this.context);
  }
  trace() {
    return wrap(this._w.trace(), this.context);
  }
  matrix_inverse() {
    return wrap(this._w.matrix_inverse(), this.context);
  }
  rref() {
    return wrap(this._w.rref(), this.context);
  }
  rank() {
    return this._w.rank();
  }
  matmul(other) {
    return wrap(this._w.matmul(toExpr(other, this.context)._w), this.context);
  }
  dot_prod(other) {
    return wrap(this._w.dot_prod(toExpr(other, this.context)._w), this.context);
  }
  cross_prod(other) {
    return wrap(
      this._w.cross_prod(toExpr(other, this.context)._w),
      this.context,
    );
  }
  vector_add(other) {
    return wrap(
      this._w.vector_add(toExpr(other, this.context)._w),
      this.context,
    );
  }
  vector_sub(other) {
    return wrap(
      this._w.vector_sub(toExpr(other, this.context)._w),
      this.context,
    );
  }

  // ---- pattern matching (default mode only) ----
  /**
   * Template match against `pattern`. Options:
   *
   * - `variables` — the declared parameters, as `{name: kind}` where kind is
   *   `true`/`"any"`, `"number"` or `"variable"`. Present-and-empty declares
   *   *no* parameters, so only an exact match succeeds; omitting the option
   *   keeps the legacy default where every string leaf in the pattern binds.
   * - `allow_permutations` — match `+`/`*` operands in any order.
   * - `allow_implicit_identities` — array of parameter names that may take the
   *   operator's identity, so `a x + b` matches `x` with `a = 1`, `b = 0`.
   *
   * The kinds replace the JS predicates the legacy API took: a function cannot
   * cross the wasm boundary, and these three are what the predicates expressed.
   * A predicate is therefore rejected rather than ignored — silently treating
   * one as "any" is what made `requireNumericMatches` a no-op.
   */
  match(pattern, options?) {
    const tree = this._w.tree_json();
    const pat = toExpr(pattern, this.context)._w.tree_json();
    // Bindings come back through `jsonToAst`, not bare `JSON.parse`: they are
    // subtrees, and `.tree` hands subtrees out untagged, so returning
    // `{a: {$: "Inf"}}` here would contradict the convention the rest of the
    // surface follows — and break the `typeof m.a === "number"` consumers
    // legacy supported.
    if (!hasOptions(options)) {
      const res = wasm.match_template(tree, pat);
      return res === undefined ? false : jsonToAst(res);
    }
    // Shared with `me.utils.match` so the two entry points cannot drift; the
    // `true` spelling of `allow_implicit_identities` is expanded by the
    // matcher, which is the only side that knows the default parameter set.
    const res = wasm.match_template_with_options(
      tree,
      pat,
      JSON.stringify(normalizeMatchOptions(options)),
    );
    return res === undefined ? false : jsonToAst(res);
  }
}

// The `using` protocol, attached only where the runtime actually has the symbol
// (Node ≥ 18.18, Chrome ≥ 125, Safari ≥ 18.4). Written as a class member,
// `[Symbol.dispose]() {}` on an engine without it would define a method keyed by
// the *string* "undefined" — silently useless rather than absent, and `free()`
// would never run. Feature-detecting keeps `using expr = me.fromText(…)` working
// where it is supported and simply unavailable where it is not.
if (typeof Symbol.dispose === "symbol") {
  (Expression.prototype as Record<symbol, unknown>)[Symbol.dispose] = function (
    this: Expression,
  ) {
    this.free();
  };
}

// Legacy methods with no Rust backing — defined so calls fail loudly, not as
// "undefined is not a function" surprises. Tests using them fail; suite runs.
for (const name of [
  "derivative_with_story",
  "derivative_story",
  "derivativeStory",
  "toXML",
  "toGLSL",
  "toMathjs",
  "solve_linear",
  "create_discrete_infinite_set",
  "finite_field_evaluate",
]) {
  (Expression.prototype as Record<string, unknown>)[name] =
    notImplemented(name);
}

// Normalization passes with no faithful Rust entry point (folded into
// `canonicalize`). Kept as no-ops returning `this` rather than throwing: a
// blanket throw here regressed ~170 idempotent-input specs that legitimately
// pass on the unchanged tree, and aborted whole spec files at collection. The
// real fix is implementing them; see DOENET_COMPAT_PLAN R7 and the follow-up note.
// `default_order` graduated out of this list — it has a real implementation
// now (`normalize::default_order`), carrying the JS ordering key rather than
// the Rust canonical `cmp`, because the order it produces is displayed. So did
// `normalize_negative_numbers` and `normalize_applied_functions`: the passes
// they name were already in the Rust core as `normalize_syntactic`'s second and
// third steps, and are now exported individually.
for (const name of ["expand_relations", "applyAllTransformations"]) {
  (Expression.prototype as Record<string, unknown>)[name] = function (
    this: Expression,
  ) {
    return this;
  };
}

// The parser options object is the legacy second argument (`splitSymbols`,
// `appliedFunctionSymbols`, `functionSymbols`, `operatorSymbols`, …). It was
// being dropped on the floor here, which mattered most for
// `appliedFunctionSymbols`: without it there is no way to get `sum(1,2,3)` to
// parse as an application rather than as `s·u·m·(1,2,3)`, since neither this
// library nor the legacy one lists the aggregates by default.
/**
 * The legacy library threw a `ParseError` — an `Error` subclass whose `name`
 * said so — and callers narrow on that name to tell "you typed something I
 * cannot read", which is worth showing a student, from any other failure, which
 * is not. `wasm-bindgen` throws a plain `Error`, so that name was lost and the
 * narrowing silently stopped matching: DoenetML's `<mathInput showPreview>` has
 * a slot for the parser's complaint and had been rendering nothing in it.
 *
 * The message is the engine's own and is already the useful part
 * (`Expecting } (at 7)`, `Invalid symbol '@' (at 0)`); only the label was
 * missing. `cause` keeps the original for anyone who wants the stack.
 */
function asParseError(e: unknown) {
  if (e instanceof Error && e.name === "Error") {
    e.name = "ParseError";
    return e;
  }
  if (e instanceof Error) {
    return e;
  }
  // wasm-bindgen can reject with a bare string.
  const wrapped = new Error(String(e), { cause: e });
  wrapped.name = "ParseError";
  return wrapped;
}

function parseText(string, opts?) {
  try {
    return new Expression(
      hasOptions(opts)
        ? wasm.parse_text_with_options(string, JSON.stringify(opts))
        : wasm.parse_text(string),
      Context,
    );
  } catch (e) {
    throw asParseError(e);
  }
}
function parseLatex(string, opts?) {
  try {
    return new Expression(
      hasOptions(opts)
        ? wasm.parse_latex_with_options(string, JSON.stringify(opts))
        : wasm.parse_latex(string),
      Context,
    );
  } catch (e) {
    throw asParseError(e);
  }
}
function createFrom(expr) {
  // "Nothing" converts to nothing. `fromAst(undefined)` reaches the core as a
  // literal `undefined` string and dies inside the parser with a
  // `Cannot read properties of undefined` — but callers do write
  // `me.from(value)` over a table whose empty rows mean "no expression", and
  // the legacy library handed those back an expression with an undefined tree
  // that every consumer treated as absent.
  if (expr === undefined || expr === null) return undefined;
  if (typeof expr === "string") {
    try {
      return parseText(expr);
    } catch (e_text) {
      try {
        return parseLatex(expr);
      } catch (e_latex) {
        if (expr.indexOf("\\") !== -1) throw e_latex;
        throw e_text;
      }
    }
  }
  return Context.fromAst(expr); // number or AST
}

/**
 * `numeric.dopri` drop-in — the Dormand-Prince ODE integrator DoenetML reached
 * through the old bundled math.js (`me.math.dopri`). Since DoenetML is dropping
 * mathjs, this is exported as a peer compat function (`me.dopri` / a named
 * export) rather than under `me.math`; the call contract is unchanged:
 *
 *   dopri(x0, x1, y0, f, tol?, maxit?)
 *
 * `f(x, y)` returns the derivative; `y0`, the states, and `f`'s return are
 * arrays for a system or plain numbers for a scalar ODE. The result exposes
 * `.at(x)` dense interpolation (a scalar/array x), and the `.x`/`.y` step
 * arrays. Backed by the Rust `solve_ode` integrator (one boundary crossing per
 * RK stage). numeric.js's `event` argument is not supported.
 */
function dopri(
  x0: number,
  x1: number,
  y0: number | ArrayLike<number>,
  f: (x: number, y: number | number[]) => number | number[],
  tol = 1e-6,
  maxit = 1000,
) {
  const scalar = typeof y0 === "number";
  const y0arr = scalar ? [y0 as number] : Array.from(y0 as ArrayLike<number>);
  const dim = y0arr.length;
  // `f` is called from inside the integrator, across the wasm boundary, where
  // an exception must not unwind — `panic = "abort"` makes that a module crash,
  // so the Rust side treats a throwing stage as a failed step and stops early.
  // Correct, but on its own it hands the caller a short, entirely
  // plausible-looking trajectory with only `terminatedEarly` to hint at why:
  // `dopri(0,1,1,()=>{throw …}).at(1)` returned the initial condition. Capture
  // the first failure and rethrow it on this side once the integrator is done.
  // A wrong-length derivative is caught here for the same reason — silently
  // integrating one component of a two-component system is a wrong answer.
  let failure: { error: unknown } | undefined;
  const zeros = () => new Array<number>(dim).fill(0);
  const rhs = (x: number, y: Float64Array): number[] => {
    if (failure) return zeros(); // already doomed; just let the solver wind down
    let out: number | number[];
    try {
      out = f(x, scalar ? y[0] : Array.from(y));
    } catch (error) {
      failure = { error };
      return zeros();
    }
    const arr =
      typeof out === "number"
        ? [out]
        : Array.from(out as ArrayLike<number>, Number);
    if (arr.length !== dim) {
      failure = {
        error: new TypeError(
          `dopri: the derivative returned ${arr.length} component(s) for a ${dim}-component state`,
        ),
      };
      return zeros();
    }
    return arr;
  };
  const sol = wasm.solve_ode(rhs, x0, x1, Float64Array.from(y0arr), tol, maxit);
  if (failure) {
    sol.free(); // nothing will read this solution; do not leak its handle
    throw failure.error;
  }
  const n = sol.dim();
  const state = (flat: Float64Array, i: number) => {
    const s = Array.from(flat.subarray(i * n, (i + 1) * n));
    return scalar ? s[0] : s;
  };
  // Guarded so a second `free()` is a no-op rather than the "null pointer
  // passed to rust" that a wasm-bindgen double free raises — same reasoning as
  // `Expression.free`.
  let freed = false;
  const freeSolution = () => {
    if (freed) return;
    freed = true;
    sol.free();
  };
  return {
    /** Dense output: interpolated state at `x` (or one per element of an `x` array). */
    at(x: number | number[]): number | number[] | (number | number[])[] {
      if (Array.isArray(x)) {
        const flat = sol.at_many(Float64Array.from(x));
        return x.map((_, i) => state(flat, i));
      }
      const s = Array.from(sol.at(x));
      return scalar ? s[0] : s;
    },
    /** Accepted step abscissas. */
    get x(): number[] {
      return Array.from(sol.times());
    },
    /** States at each step abscissa. */
    get y(): (number | number[])[] {
      const ts = sol.times();
      const flat = sol.at_many(ts);
      return Array.from(ts, (_v, i) => state(flat, i));
    },
    /** True when integration stopped before `x1` (blow-up / step budget). */
    get terminatedEarly(): boolean {
      return sol.terminated_early();
    },
    // Same contract as `Expression.free`/`dispose`: the solution owns a wasm
    // handle, and a worker that integrates in a loop leaks one per call
    // otherwise. numeric.js had nothing to release, so this is additive —
    // callers that never free behave exactly as before.
    /** Release the underlying wasm handle. Idempotent. */
    free() {
      freeSolution();
    },
    /** Alias of `free()`, and the `using`-statement protocol where supported. */
    dispose() {
      freeSolution();
    },
    ...(typeof Symbol.dispose === "symbol"
      ? { [Symbol.dispose]: freeSolution }
      : {}),
  };
}

/**
 * Every tree obtainable from `tree` by negating exactly one of its nodes,
 * itself included. Each node is negated once across the whole enumeration, so
 * an n-node tree yields n variants.
 */
function* singleNegations(tree: Tree): Generator<Tree> {
  yield ["-", tree] as Tree;
  if (Array.isArray(tree)) {
    for (let i = 1; i < tree.length; i++) {
      for (const variant of singleNegations(tree[i])) {
        const copy = tree.slice() as Tree[];
        copy[i] = variant;
        yield copy as Tree;
      }
    }
  }
}

/**
 * Does `expr` equal `other` once **exactly** `n_sign_errors` of its parts have
 * their sign flipped? Grading for "you had the right idea but dropped a minus
 * sign" — DoenetML's `numSignErrorsMatched`.
 *
 * Port of the JS `equalSpecifiedSignErrors`. That version negated nodes in
 * place, in the caller's tree, and relied on restoring them afterwards; this
 * one enumerates variants instead, since a wasm-backed `Expression` has no
 * mutable tree. Callers no longer need the defensive deep copy the old
 * contract forced on them, though making one is harmless.
 *
 * `equalityFunction` receives the *negated* expression first, matching the JS
 * argument order — DoenetML's normalizes that side before comparing.
 */
function equalSpecifiedSignErrors(
  expr: ExpressionLike,
  other: ExpressionLike,
  {
    equalityFunction,
    n_sign_errors = 1,
  }: {
    equalityFunction?: (a: Expression, b: Expression) => boolean;
    n_sign_errors?: number;
  } = {},
): boolean {
  const e = toExpr(expr, Context);
  const o = toExpr(other, Context);
  const baseEquality =
    equalityFunction ?? ((a: Expression, b: Expression) => a.equals(b));

  if (n_sign_errors === 0) {
    return baseEquality(e, o);
  }
  if (!(Number.isInteger(n_sign_errors) && n_sign_errors > 0)) {
    throw Error(
      `Have not implemented equality check with ${n_sign_errors} sign errors.`,
    );
  }

  // More than one error: each variant is then checked for the remaining ones,
  // so the negations compose without this function needing to enumerate
  // combinations itself.
  const compare =
    n_sign_errors === 1
      ? baseEquality
      : (a: Expression, b: Expression) =>
          equalSpecifiedSignErrors(a, b, {
            equalityFunction: baseEquality,
            n_sign_errors: n_sign_errors - 1,
          });

  const ctx = (e.context || Context) as Ctx;
  for (const variant of singleNegations(e.tree as Tree)) {
    if (compare(ctx.fromAst(variant) as Expression, o)) return true;
  }
  return false;
}

/**
 * Equal outright, or after up to `max_sign_errors` sign flips — reporting how
 * many it took. Port of the JS `equalWithSignErrors`.
 */
function equalWithSignErrors(
  expr: ExpressionLike,
  other: ExpressionLike,
  {
    equalityFunction,
    max_sign_errors = 1,
  }: {
    equalityFunction?: (a: Expression, b: Expression) => boolean;
    max_sign_errors?: number;
  } = {},
): { matched: boolean; n_sign_errors?: number } {
  const e = toExpr(expr, Context);
  const o = toExpr(other, Context);
  const compare =
    equalityFunction ?? ((a: Expression, b: Expression) => a.equals(b));

  if (compare(e, o)) return { matched: true, n_sign_errors: 0 };

  for (let i = 1; i <= max_sign_errors; i++) {
    if (
      equalSpecifiedSignErrors(e, o, {
        equalityFunction: compare,
        n_sign_errors: i,
      })
    ) {
      return { matched: true, n_sign_errors: i };
    }
  }
  return { matched: false };
}

const Context = {
  dopri,
  from: createFrom,
  fromText: parseText,
  parse: parseText,
  fromLatex: parseLatex,
  fromLaTeX: parseLatex,
  fromTeX: parseLatex,
  fromTex: parseLatex,
  parse_tex: parseLatex,
  fromMml: notImplemented("fromMml"),
  fromAst(ast) {
    const key = atomKey(ast);
    if (key === undefined) {
      // A bare number skips JSON entirely. This is the sampled-coordinate
      // case, which `atomKey` deliberately declines to cache (see
      // `ATOM_HANDLES`: caching arbitrary floats is a measured loss), so it
      // arrives here on every call — once per sample point when a function is
      // evaluated over a domain. Going through `from_ast` meant a
      // `JSON.stringify` here and a full JSON parse in wasm to move one f64.
      //
      // Finite only. The JSON path routes a non-finite through `astReplacer`'s
      // `{"$":"Inf"}` / `{"$":"NaN"}` tags, which `from_ast` revives as the
      // infinity *constant* — a different expression from a float that happens
      // to be infinite, and one that `equals`, `simplify` and the interval
      // endpoints all treat differently. Taking the shortcut there made
      // `fromAst(Infinity)` stop comparing equal to `fromText("infinity")`.
      if (typeof ast === "number" && Number.isFinite(ast)) {
        return new Expression(wasm.from_number(ast), Context);
      }
      return new Expression(
        wasm.from_ast(JSON.stringify(ast, astReplacer)),
        Context,
      );
    }
    let handle = ATOM_HANDLES.get(key);
    if (handle === undefined) {
      handle = wasm.from_ast(JSON.stringify(ast, astReplacer));
      if (ATOM_HANDLES.size >= MAX_ATOMS) ATOM_HANDLES.clear();
      ATOM_HANDLES.set(key, handle);
      ATOM_SHARED.add(handle);
      ATOM_KEYS.set(handle, key);
    }
    // A fresh wrapper per call: the handle is immutable and safe to share, but
    // the `Expression` around it carries a `context` and is what callers hold.
    return new Expression(handle, Context);
  },
  reviver(key, value) {
    if (
      value &&
      value.objectType === "math-expression" &&
      value.tree !== undefined
    ) {
      return Context.fromAst(value.tree);
    }
    return value;
  },
  /**
   * Distinct symbol names interned this session — a memory gauge for the
   * long-lived worker. Append-only (see item 8); use it to measure symbol
   * growth over a session.
   */
  interner_size(): number {
    return wasm.interner_size();
  },
  isTree,
  math,
  converters,
  utils: { match, flatten, unflattenLeft, unflattenRight },
  class: Expression,

  // ---- sign-error grading (`lib/expression/sign_error.js`) ----
  equalSpecifiedSignErrors,
  equalWithSignErrors,

  // ---- assumptions (context-level) ----
  // One handle, fed twice. The wasm `Assumptions` handle answers the predicates
  // (`is_real`, `is_positive`, …) from the text spelling of every assumption,
  // and holds the same facts as trees — filed per variable, so that
  // `get_assumptions` can hand a fact *back*. The parallel text list is what
  // `simplify_with_assumptions` takes.
  //
  // The handle is constructed lazily, and that is load-bearing. As a plain `new
  // wasm.Assumptions()` in this literal it ran while *this module's body* was
  // still evaluating, so any consumer importing `setWasmModule` from the package
  // root forced the wasm load before it had a chance to inject — the injection
  // could never win, and silently fell through to the node loader. Nothing here
  // may touch `wasm` until someone actually calls a method.
  _assumptionsHandleCache: undefined,
  get _assumptionsHandle() {
    return (this._assumptionsHandleCache ??= new wasm.Assumptions());
  },
  set _assumptionsHandle(h) {
    this._assumptionsHandleCache = h;
  },
  _assumptionTexts: [],
  set_to_default() {
    // A fresh handle is the reset: it carries the per-variable facts too.
    this._assumptionsHandle = new wasm.Assumptions();
    this._assumptionTexts = [];
  },
  clear_assumptions() {
    this.set_to_default();
  },
  add_assumption(assumption, exclude_generic?) {
    const tree = syncAssumptionText(this, assumption, "add");
    if (tree === undefined) return 0;
    return assumptionStore.add_assumption(
      this._assumptionsHandle,
      tree,
      exclude_generic,
    );
  },
  add_generic_assumption(assumption) {
    // A generic assumption is stated in terms of `x` and stands for every
    // variable, which the wasm store cannot express; it gets the `x` spelling,
    // which is at least right for `x` itself.
    const tree = syncAssumptionText(this, assumption, "add");
    if (tree === undefined) return 0;
    return assumptionStore.add_generic_assumption(
      this._assumptionsHandle,
      tree,
    );
  },
  remove_assumption(assumption) {
    const tree = syncAssumptionText(this, assumption, "remove");
    if (tree === undefined) return 0;
    return assumptionStore.remove_assumption(this._assumptionsHandle, tree);
  },
  remove_generic_assumption(assumption) {
    const tree = syncAssumptionText(this, assumption, "remove");
    if (tree === undefined) return 0;
    return assumptionStore.remove_generic_assumption(
      this._assumptionsHandle,
      tree,
    );
  },
  get_assumptions(variables_or_expr, params?) {
    return assumptionStore.get_assumptions(
      this._assumptionsHandle,
      variables_or_expr,
      params,
    );
  },
  // `me.assumptions` was the assumptions object itself, carrying the same
  // add/get methods as the context. This port also has to keep answering the
  // wasm predicates through it, since `lib/assumptions/element_of_sets` reads
  // `Context.assumptions` as its default source — so the facade forwards those
  // to the handle rather than replacing it.
  _assumptionsFacadeCache: undefined,
  get assumptions() {
    return (this._assumptionsFacadeCache ??= makeAssumptionsFacade());
  },
};

/**
 * Mirror an assumption into the wasm handle and the `simplify_with_assumptions`
 * text list, returning its tree for the JS store to file — or undefined when
 * there is no assumption at all.
 *
 * An empty assumption is a no-op rather than an error: the spec tables drive
 * `me.add_assumption(me.from(input))` over rows whose input is undefined,
 * meaning "no assumptions for this row".
 */
function syncAssumptionText(
  context: Ctx,
  assumption: ExpressionLike,
  action: "add" | "remove",
): Tree | undefined {
  const tree = get_tree(assumption);
  if (!Array.isArray(tree)) return undefined;

  const text = toExpr(assumption, context).toString();
  if (action === "add") {
    context._assumptionsHandle.add(text);
    context._assumptionTexts.push(text);
  } else {
    context._assumptionsHandle.remove(text);
    context._assumptionTexts = context._assumptionTexts.filter(
      (t) => t !== text,
    );
  }
  return tree;
}

/**
 * `me.assumptions`: the JS assumption API plus the wasm predicates, both
 * pointing at the live context state (never a snapshot — the spec clears and
 * re-adds assumptions between calls while holding the same object).
 */
function makeAssumptionsFacade() {
  const facade: Record<string, unknown> = {
    get _assumptionsHandle() {
      return Context._assumptionsHandle;
    },
    get byvar() {
      return assumptionStore.byvar(Context._assumptionsHandle);
    },
    get derived() {
      return assumptionStore.derived(Context._assumptionsHandle);
    },
    get generic() {
      return assumptionStore.generic(Context._assumptionsHandle);
    },
  };
  for (const name of [
    "get_assumptions",
    "add_assumption",
    "add_generic_assumption",
    "remove_assumption",
    "remove_generic_assumption",
    "clear_assumptions",
    "set_to_default",
  ]) {
    facade[name] = (...args: unknown[]) => Context[name](...args);
  }
  // The three-valued predicates and the raw relation add/remove live on the
  // wasm handle; keep them reachable so a caller holding `me.assumptions` can
  // still use it as one.
  for (const name of [
    "is_integer",
    "is_real",
    "is_complex",
    "is_nonzero",
    "is_nonnegative",
    "is_nonpositive",
    "is_positive",
    "is_negative",
    "add",
    "remove",
  ]) {
    facade[name] = (...args: unknown[]) =>
      Context._assumptionsHandle[name](...args);
  }
  return facade;
}

// The legacy library exposed every `Expression` method a second time as a free
// function on the context, expression-first: `me.simplify(expr)` alongside
// `expr.simplify()`. Mirror the prototype onto `Context` once both exist.
//
// Anything already reachable on `Context` wins, so the factories (`from`,
// `fromAst`, `fromText`, …) are never shadowed — and neither are the inherited
// `Object.prototype` members, which is why `toString`/`valueOf` stay put rather
// than becoming expression-first functions that would break `String(me)`.
//
// `NOT_EXPRESSION_FIRST` covers what that `in Context` test misses. A protocol
// method the *runtime* calls is not a candidate for the expression-first
// treatment, because the runtime supplies its own argument: `JSON.stringify`
// invokes `toJSON(key)`, so mirroring it made the property key the "expression",
// and `JSON.stringify({me})` emitted a `{objectType:"math-expression"}` envelope
// that `Context.reviver` would then revive the whole library context from —
// while `JSON.stringify({"(": me})` *threw* a parse error out of a plain
// stringify. `toJSON` is not on `Object.prototype`, so only naming it works.
// `free`/`dispose` are excluded for a milder reason: they manage this port's
// wasm handles, which legacy had no concept of, so there is no expression-first
// spelling of them to be compatible with — and `me.dispose()` reads like "tear
// down the context", which it would not do.
//
// Coercion goes through `toExpr`, not `Context.from`: the argument is usually
// an `Expression` already, and `from` would try to read that as an AST.
const NOT_EXPRESSION_FIRST = new Set([
  "constructor",
  "toJSON",
  "free",
  "dispose",
]);
for (const name of Object.getOwnPropertyNames(Expression.prototype)) {
  if (NOT_EXPRESSION_FIRST.has(name) || name in Context) continue;
  const desc = Object.getOwnPropertyDescriptor(Expression.prototype, name);
  if (typeof desc?.value !== "function") continue; // skip accessors such as `tree`
  (Context as Record<string, unknown>)[name] = (
    expr: ExpressionLike,
    ...args: unknown[]
  ) =>
    (toExpr(expr) as unknown as Record<string, (...a: unknown[]) => unknown>)[
      name
    ](...args);
}

export { Expression, dopri, setWasmModule };
export default Context;
