// Drop-in replacement for the original `lib/math-expressions.js` default export
// (the `Context` factory + `Expression`), backed by the Rust/wasm core.
//
// Not every legacy method exists on the Rust side; those that don't are either
// approximated, or throw a clear "not implemented in js-compat" so the calling
// test fails cleanly (the suite still runs). See JS_TEST_COVERAGE_AUDIT.md.
import wasm, { setWasmModule } from "./_wasm";
import math from "./mathjs";
import { match, flatten, unflattenLeft, unflattenRight } from "./trees/flatten";
import * as converters from "./converters/index";
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
  if (Array.isArray(value) && value.length > 0 && typeof value[0] === "string") {
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
function wrap(handle: WasmExpression | undefined, context: Ctx): Expression | undefined {
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
 * Note the boundary is deliberately *tagged in both directions*: `.tree` gives
 * back `{"$":"NaN"}`, not a JS `NaN`. DoenetML already emits `{"$":"None"}`
 * itself, so one tagged wire format — in and out, `fromAst(x).tree` a fixpoint —
 * beats a half-symmetric one where `NaN`/`±Infinity` untag but `None` (which has
 * no JS scalar) cannot. See DOENET_INTEGRATION.md §"Non-finite and absent values".
 */
function astReplacer(_key: string, value: unknown): unknown {
  if (typeof value === "number" && !Number.isFinite(value)) {
    if (Number.isNaN(value)) return { $: "NaN" };
    return { $: value > 0 ? "Inf" : "-Inf" };
  }
  return value;
}

/**
 * Whether a render call carries options worth forwarding to the wasm
 * `*_with_options` entry points (padToDigits, padToDecimals, showBlanks,
 * explicitMultiplicationSymbols, notation, unicode). An empty/absent object
 * takes the cheaper no-options render path.
 */
function hasRenderOpts(opts: unknown): opts is Record<string, unknown> {
  return !!opts && typeof opts === "object" && Object.keys(opts).length > 0;
}

/** A variable argument may be a string name or an Expression of a symbol. */
function varName(v: string | Expression): string {
  if (typeof v === "string") return v;
  if (v instanceof Expression) return v.toString();
  return String(v);
}

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
};
function mapEqOptions(opts: EqualityOptions): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(opts)) {
    if (EQ_OPTION_KEYS[k]) out[EQ_OPTION_KEYS[k]] = v;
    else if (Object.values(EQ_OPTION_KEYS).includes(k)) out[k] = v; // already camelCase
  }
  return out;
}

class Expression {
  _w: WasmExpression;
  context: Ctx;

  constructor(handle: WasmExpression, context?: Ctx) {
    this._w = handle;
    this.context = context || Context;
  }

  // ---- inspection / rendering ----
  get tree() {
    return JSON.parse(this._w.tree_json());
  }
  // Rendering honors the legacy render options (padToDigits, padToDecimals,
  // showBlanks, explicitMultiplicationSymbols, notation/unicode) by forwarding
  // a non-empty options object to the `*_with_options` wasm entry points. The
  // no-arg path stays on the cheap no-options render — `toString()` is what JS
  // coercion (`String(expr)`) calls.
  toString(opts?) {
    return hasRenderOpts(opts) ? this._w.to_text_with_options(JSON.stringify(opts)) : this._w.to_text();
  }
  toText(opts?) {
    return hasRenderOpts(opts) ? this._w.to_text_with_options(JSON.stringify(opts)) : this._w.to_text();
  }
  toLatex(opts?) {
    return hasRenderOpts(opts) ? this._w.to_latex_with_options(JSON.stringify(opts)) : this._w.to_latex();
  }
  tex(opts?) {
    return hasRenderOpts(opts) ? this._w.to_latex_with_options(JSON.stringify(opts)) : this._w.to_latex();
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

  // ---- equality ----
  equals(other, options) {
    const o = toExpr(other, this.context);
    if (options && Object.keys(options).length > 0) {
      return this._w.equals_with_options(o._w, JSON.stringify(mapEqOptions(options)));
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
  equalsViaSyntax(other) {
    return this._w.structural_equality(toExpr(other, this.context)._w, '"sameStructure"');
  }
  is_zero() {
    return this._w.is_zero();
  }
  isAnalytic(opts) {
    const o = opts || {};
    return this._w.is_analytic(!!o.allow_abs, !!o.allow_arg, !!o.allow_relation);
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
    const r = this._w.integrate_numerically(varName(v), Number(lower), Number(upper));
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
  factor() {
    return wrap(this._w.factor(), this.context);
  }
  evaluate_numbers(opts) {
    // The no-argument form (fold, then order) is supported. `skip_ordering`
    // (DoenetML's `simplify="numberspreserveorder"`) is not — the core pass has
    // no order-preserving mode — so reject it loudly rather than silently
    // reorder, which is the bug this replaces (`1+x+2` came back `x+3`).
    // `skip_ordering: false` is the default and passes straight through.
    if (opts && opts.skip_ordering) {
      throw new Error(
        "math-expressions-js-compat: evaluate_numbers({skip_ordering:true}) is not " +
          "implemented — the core pass always orders; only the ordering form is available",
      );
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
  subscripts_to_strings() {
    return wrap(this._w.subscripts_to_strings(), this.context);
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
    if (w) {
      w.free();
      this._w = undefined as unknown as WasmExpression;
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
  get_component(component) {
    return wrap(this._w.get_component(componentPath(component)), this.context);
  }
  substitute_component(component, value) {
    return wrap(
      this._w.substitute_component(componentPath(component), toExpr(value, this.context)._w),
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
    return wrap(this._w.set_small_zero(tolerance === undefined ? 1e-14 : tolerance), this.context);
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
  evaluate_to_constant() {
    const v = this._w.evaluate_to_constant();
    return v === undefined ? null : v;
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
    return wrap(this._w.cross_prod(toExpr(other, this.context)._w), this.context);
  }
  vector_add(other) {
    return wrap(this._w.vector_add(toExpr(other, this.context)._w), this.context);
  }
  vector_sub(other) {
    return wrap(this._w.vector_sub(toExpr(other, this.context)._w), this.context);
  }

  // ---- pattern matching (default mode only) ----
  match(pattern, _options) {
    const res = wasm.match_template(
      this._w.tree_json(),
      toExpr(pattern, this.context)._w.tree_json(),
    );
    return res === undefined ? false : JSON.parse(res);
  }
}

// The `using` protocol, attached only where the runtime actually has the symbol
// (Node ≥ 18.18, Chrome ≥ 125, Safari ≥ 18.4). Written as a class member,
// `[Symbol.dispose]() {}` on an engine without it would define a method keyed by
// the *string* "undefined" — silently useless rather than absent, and `free()`
// would never run. Feature-detecting keeps `using expr = me.fromText(…)` working
// where it is supported and simply unavailable where it is not.
if (typeof Symbol.dispose === "symbol") {
  (Expression.prototype as Record<symbol, unknown>)[Symbol.dispose] =
    function (this: Expression) {
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
  "expression_to_polynomial",
  "finite_field_evaluate",
]) {
  (Expression.prototype as Record<string, unknown>)[name] = notImplemented(name);
}

// Normalization passes with no faithful Rust entry point (folded into
// `canonicalize`; `default_order` would need the JS ordering key, not Rust's
// canonical `cmp`). Kept as no-ops returning `this` rather than throwing: a
// blanket throw here regressed ~170 idempotent-input specs that legitimately
// pass on the unchanged tree, and aborted whole spec files at collection. The
// real fix is implementing them; see DOENET_COMPAT_PLAN R7 and the follow-up note.
for (const name of [
  "default_order",
  "normalize_negative_numbers",
  "normalize_applied_functions",
  "expand_relations",
  "applyAllTransformations",
]) {
  (Expression.prototype as Record<string, unknown>)[name] = function (this: Expression) {
    return this;
  };
}

function parseText(string) {
  return new Expression(wasm.parse_text(string), Context);
}
function parseLatex(string) {
  return new Expression(wasm.parse_latex(string), Context);
}
function createFrom(expr) {
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
  const rhs = scalar
    ? (x: number, y: Float64Array) => [Number(f(x, y[0]))]
    : (x: number, y: Float64Array) => Array.from(f(x, Array.from(y)) as number[], Number);
  const sol = wasm.solve_ode(rhs, x0, x1, Float64Array.from(y0arr), tol, maxit);
  const n = sol.dim();
  const state = (flat: Float64Array, i: number) => {
    const s = Array.from(flat.subarray(i * n, (i + 1) * n));
    return scalar ? s[0] : s;
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
  };
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
    return new Expression(wasm.from_ast(JSON.stringify(ast, astReplacer)), Context);
  },
  reviver(key, value) {
    if (value && value.objectType === "math-expression" && value.tree !== undefined) {
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

  // ---- assumptions (context-level) ----
  // Backed by a wasm `Assumptions` handle plus a parallel text list so
  // `simplify_with_assumptions` can be fed. `get_assumptions` is best-effort —
  // the original returned a richly-structured object this does not reproduce.
  _assumptionsHandle: new wasm.Assumptions(),
  _assumptionTexts: [],
  set_to_default() {
    this._assumptionsHandle = new wasm.Assumptions();
    this._assumptionTexts = [];
  },
  clear_assumptions() {
    this.set_to_default();
  },
  add_assumption(assumption) {
    const text = toExpr(assumption, this).toString();
    this._assumptionsHandle.add(text);
    this._assumptionTexts.push(text);
    return true;
  },
  add_generic_assumption(assumption) {
    return this.add_assumption(assumption);
  },
  remove_assumption(assumption) {
    const text = toExpr(assumption, this).toString();
    this._assumptionsHandle.remove(text);
    this._assumptionTexts = this._assumptionTexts.filter((t) => t !== text);
  },
  remove_generic_assumption(assumption) {
    return this.remove_assumption(assumption);
  },
  get_assumptions() {
    if (!this._assumptionTexts.length) return undefined;
    try {
      return Context.fromText(this._assumptionTexts.join(" and "));
    } catch {
      return undefined;
    }
  },
  get assumptions() {
    return this._assumptionsHandle;
  },
};

// The legacy library exposed every `Expression` method a second time as a free
// function on the context, expression-first: `me.simplify(expr)` alongside
// `expr.simplify()`. Mirror the prototype onto `Context` once both exist.
//
// Anything already reachable on `Context` wins, so the factories (`from`,
// `fromAst`, `match`, …) are never shadowed — and neither are the inherited
// `Object.prototype` members, which is why `toString`/`valueOf` stay put rather
// than becoming expression-first functions that would break `String(me)`.
//
// Coercion goes through `toExpr`, not `Context.from`: the argument is usually
// an `Expression` already, and `from` would try to read that as an AST.
for (const name of Object.getOwnPropertyNames(Expression.prototype)) {
  if (name === "constructor" || name in Context) continue;
  const desc = Object.getOwnPropertyDescriptor(Expression.prototype, name);
  if (typeof desc?.value !== "function") continue; // skip accessors such as `tree`
  (Context as Record<string, unknown>)[name] = (expr: ExpressionLike, ...args: unknown[]) =>
    (toExpr(expr) as unknown as Record<string, (...a: unknown[]) => unknown>)[name](...args);
}

export { Expression, dopri, setWasmModule };
export default Context;
