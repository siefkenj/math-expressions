// End-to-end coverage for the PR-#84 follow-up items (DoenetML measurement):
// the vector/matrix shape pass, the `None` special, NaN/Infinity crossing, and
// the loud-failure passes. Runs against the real wasm build via the compat API.
import { describe, it, expect } from "vitest";
import me, { dopri, setWasmModule } from "../lib/math-expressions";

describe("item 1 — perform_vector_matrix_additions_scalar_multiplications", () => {
  const perform = (s: string) =>
    me.fromText(s).perform_vector_matrix_additions_scalar_multiplications().tree;

  it("moves the container outside a sum of vectors, componentwise", () => {
    // The grading precondition: top-level operator becomes the container.
    expect(perform("(1,2)+(3,4)")).toEqual(["tuple", ["+", 1, 3], ["+", 2, 4]]);
  });

  it("does not fold the components", () => {
    const tree = perform("(1,2)+(3,4)") as unknown[];
    expect(tree[0]).toBe("tuple");
    expect(Array.isArray(tree[1]) && (tree[1] as unknown[])[0]).toBe("+");
  });

  it("distributes a scalar into a vector, scalar on the right", () => {
    expect(perform("3(1,2)")).toEqual(["tuple", ["*", 1, 3], ["*", 2, 3]]);
  });

  it("leaves non-vectors and unpaired vectors untouched", () => {
    expect(perform("x+y")).toEqual(["+", "x", "y"]);
    expect((perform("(1,2)+3") as unknown[])[0]).toBe("+");
  });

  it("drops a literal 1 rather than distributing it", () => {
    // Legacy pops a `1` off the factor list without multiplying by it.
    expect(perform("1(1,2)")).toEqual(["tuple", 1, 2]);
    expect(perform("(x,y)*1")).toEqual(["tuple", "x", "y"]);
  });

  it("distributes into the container that can actually reach the scalar", () => {
    // The leading tuple is walled off from the 5; the altvector is not.
    expect(perform("(1,2)*⟨3,4⟩*5")).toEqual([
      "*",
      ["tuple", 1, 2],
      ["altvector", ["*", 3, 5], ["*", 4, 5]],
    ]);
  });

  it("is also reachable expression-first on the context", () => {
    expect(me.perform_vector_matrix_additions_scalar_multiplications(me.fromText("(1,2)+(3,4)")).tree)
      .toEqual(["tuple", ["+", 1, 3], ["+", 2, 4]]);
  });
});

describe("item 2 — the {\"$\":\"None\"} special round-trips", () => {
  it("revives a bare None instead of throwing", () => {
    expect(() => me.fromAst({ $: "None" })).not.toThrow();
    expect(me.fromAst({ $: "None" }).tree).toEqual({ $: "None" });
  });

  it("revives a None nested in a container", () => {
    const tree = ["tuple", { $: "None" }, 2];
    expect(me.fromAst(tree).tree).toEqual(tree);
  });
});

describe("item 3a — NaN / Infinity survive fromAst", () => {
  it("survives as the JS scalar, not as null and not as a tag", () => {
    // The tag is how these cross the wasm boundary — JSON cannot hold them —
    // but it is not what a caller sees. `.tree` untags, because `Infinity` is
    // what legacy returned and what `typeof x === "number"` consumers test.
    expect(me.fromAst(NaN).tree).toEqual(NaN);
    expect(me.fromAst(Infinity).tree).toEqual(Infinity);
    expect(me.fromAst(-Infinity).tree).toEqual(-Infinity);
  });

  it("preserves a NaN nested in a tree", () => {
    expect(me.fromAst(["tuple", NaN, 1]).tree).toEqual(["tuple", NaN, 1]);
  });

  it("is a fixpoint: a revived value revives to itself", () => {
    // The contract DOENET_INTEGRATION.md §4 documents — a value survives any
    // number of save/revive cycles unchanged. It holds at the *value* level:
    // `.tree` untags on the way out and the replacer re-tags on the way in, so
    // the wire stays tagged while the caller never sees a tag. `{$:"None"}` is
    // the exception in both directions, having no JS scalar to untag to.
    for (const v of [NaN, Infinity, -Infinity, { $: "None" }]) {
      const once = me.fromAst(v).tree;
      expect(me.fromAst(once).tree).toEqual(once);
    }
  });
});

describe("item 5 — render options are honored", () => {
  it("pads decimals and digits", () => {
    expect(me.fromText("1.5").toLatex({ padToDecimals: 4 })).toBe("1.5000");
    expect(me.fromText("1.5").toString({ padToDecimals: 4 })).toBe("1.5000");
    expect(me.fromText("5").toString({ padToDigits: 4 })).toBe("5.000");
    // No-arg render is unchanged.
    expect(me.fromText("1.5").toLatex()).toBe("1.5");
  });

  it("floors a non-integer pad count instead of dropping it", () => {
    // Legacy is `Math.floor(padToDigits)`; reading the option as an integer-only
    // JSON value silently meant "no padding at all".
    expect(me.fromText("1.5").toString({ padToDigits: 3.5 })).toBe("1.50");
    expect(me.fromText("1.5").toString({ padToDecimals: 3.9 })).toBe("1.500");
  });

  it("treats a non-positive pad count as no padding", () => {
    expect(me.fromText("1.5").toString({ padToDigits: 0 })).toBe("1.5");
    expect(me.fromText("1.5").toString({ padToDigits: -2 })).toBe("1.5");
  });

  it("clamps an absurd pad count instead of exhausting wasm memory", () => {
    // `"0".repeat(n)` with an unclamped n is an OOM abort, which under
    // `panic = "abort"` takes the worker down with it.
    const out = me.fromText("1.5").toString({ padToDecimals: 2 ** 31 });
    expect(out.length).toBeLessThan(2000);
    expect(out.startsWith("1.5")).toBe(true);
  });

  it("hides blanks with showBlanks:false and forces explicit multiplication", () => {
    // showBlanks hides the ＿ glyph (the surrounding structure stays).
    expect(me.fromAst("＿").toString()).toBe("＿");
    expect(me.fromAst("＿").toString({ showBlanks: false })).toBe("");
    expect(me.fromText("2x").toString({ explicitMultiplicationSymbols: true })).toBe("2*x");
  });
});

describe("item 4 — dopri peer export (numeric.dopri drop-in)", () => {
  it("solves a scalar ODE y'=y, y(0)=1 to y(1)=e", () => {
    const sol = dopri(0, 1, 1, (_x, y) => y as number);
    expect(sol.at(1) as number).toBeCloseTo(Math.E, 5);
    // Reachable as a peer on the context too.
    expect((me as unknown as { dopri: typeof dopri }).dopri).toBe(dopri);
  });

  it("solves a system (harmonic oscillator) with array states", () => {
    // y0'=y1, y1'=-y0, y(0)=[1,0] → y(π) ≈ [-1, 0]
    const sol = dopri(0, Math.PI, [1, 0], (_x, y) => {
      const [a, b] = y as number[];
      return [b, -a];
    });
    const yEnd = sol.at(Math.PI) as number[];
    expect(yEnd[0]).toBeCloseTo(-1, 4);
    expect(yEnd[1]).toBeCloseTo(0, 4);
  });
});

describe("§2 — fromAst accepts an Expression where a tree is expected", () => {
  it("unwraps a bare Expression instead of throwing", () => {
    // A math-valued DoenetML state variable *holds* an Expression, so code that
    // re-wraps one hands it straight back to fromAst.
    const e = me.fromText("3");
    expect(me.fromAst(e).tree).toEqual(3);
  });

  it("unwraps an Expression nested inside a tree under construction", () => {
    expect(me.fromAst(["+", me.fromText("3"), 2]).tree).toEqual(["+", 3, 2]);
    expect(me.fromAst(["tuple", me.fromText("x+1"), me.fromText("3")]).tree).toEqual([
      "tuple",
      ["+", "x", 1],
      3,
    ]);
  });

  it("also accepts the serialized envelope, as reviver does", () => {
    // A JSON.parse of a persisted expression that never went through reviver.
    const envelope = JSON.parse(JSON.stringify(me.fromText("3")));
    expect(envelope.objectType).toBe("math-expression");
    expect(me.fromAst(envelope).tree).toEqual(3);
  });
});

describe("§4 — an indeterminate form is NaN, not 0", () => {
  it("does not annihilate 0/0 to zero", () => {
    // DoenetML computes an undefined slope this way; 0 would report a degenerate
    // line as horizontal — a wrong number on a grading path.
    expect(me.fromText("0/0").simplify().tree).toEqual(NaN);
    expect(me.fromText("0*Infinity").simplify().tree).toEqual(NaN);
    expect(me.fromText("0*(1/0)").simplify().tree).toEqual(NaN);
    // `NaN`, not `null`: `evaluate_to_constant` reports an indeterminate form
    // as the value it is. `null` crosses to JS and coerces to `0`, which would
    // report an undefined slope as a real point at the origin — the same wrong
    // number this test exists to rule out.
    expect(me.fromText("0/0").evaluate_to_constant()).toBeNaN();
  });

  it("still annihilates when the other factor is merely of unknown finiteness", () => {
    // Legacy's `is_nonzero` had a third "undefined" state that fell through to 0.
    expect(me.fromText("0*x").simplify().tree).toEqual(0);
    expect(me.fromText("1/0").simplify().tree).toEqual(Infinity);
  });
});

describe("§8 — the wasm load is deferred past module evaluation", () => {
  it("touches no wasm while the barrel's own body runs", async () => {
    // `_assumptionsHandle: new wasm.Assumptions()` in the Context literal used to
    // force the load during module evaluation, so a consumer importing
    // setWasmModule from the package root could never win the race.
    const { createRequire } = await import("node:module");
    const real = createRequire(import.meta.url)(
      "../vendor/wasm/math_expressions_wasm.js",
    ) as Record<string, unknown>;
    let touches = 0;
    const spy = new Proxy({} as never, {
      get: (_t, p) => {
        touches++;
        return real[p as string];
      },
      has: (_t, p) => p in real,
    });
    setWasmModule(spy);
    try {
      // Re-import with a cache-busting query so the module body runs again.
      await import("../lib/math-expressions?deferred-load-probe");
      expect(touches).toBe(0);
    } finally {
      setWasmModule(undefined as never);
    }
  });
});

describe("§3 — simplify folds numeric function applications", () => {
  const s = (tree: unknown) => me.fromAst(tree as never).simplify().tree;
  // The aggregates have no default parser spelling (legacy has none either),
  // so a caller opts in — exactly as DoenetML does.
  const AGGREGATES = {
    appliedFunctionSymbols: ["sum", "prod", "mean", "median", "variance", "std", "count", "max", "min", "log2"],
  };

  it("renders the report's student-visible case", () => {
    // <math simplify>sum(3,17,5-4)</math> rendered the unevaluated application.
    const e = me.fromText("sum(3,17,5-4)", AGGREGATES);
    expect(e.tree).toEqual(["apply", "sum", ["tuple", 3, 17, ["+", 5, -4]]]);
    expect(e.simplify().tree).toEqual(21);
  });

  it("folds rounding, magnitude and combinatoric functions", () => {
    expect(s(["apply", "floor", 55.33])).toEqual(55);
    expect(s(["apply", "ceil", 2.1])).toEqual(3);
    expect(s(["apply", "abs", -3])).toEqual(3);
    expect(s(["apply", "nCr", ["tuple", 5, 3]])).toEqual(10);
    expect(s(["apply", "nPr", ["tuple", 5, 3]])).toEqual(60);
  });

  it("folds the aggregates, exactly", () => {
    expect(s(["apply", "sum", ["tuple", 3, 17, 1]])).toEqual(21);
    expect(s(["apply", "prod", ["tuple", 2, 3, 4]])).toEqual(24);
    expect(s(["apply", "mean", ["tuple", 1, 2, 3]])).toEqual(2);
    expect(s(["apply", "mean", ["tuple", 1, 2, 4]])).toEqual(["/", 7, 3]);
    expect(s(["apply", "variance", ["tuple", 1, 2, 3]])).toEqual(1);
    expect(s(["apply", "std", ["tuple", 1, 2, 3]])).toEqual(1);
    expect(s(["apply", "count", ["tuple", 1, 2, 3]])).toEqual(3);
    expect(s(["apply", "max", ["tuple", 1, 5, 3]])).toEqual(5);
    expect(s(["apply", "min", ["tuple", 1, 5, 3]])).toEqual(1);
  });

  it("folds logarithms without the float noise legacy had", () => {
    expect(s(["apply", "log10", ["^", 10, 3]])).toEqual(3);
    expect(s(["apply", "log2", 8])).toEqual(3);
    // The based form: legacy answers 2.9999999999999996 here, because it
    // computes ln(1000)/ln(10) and then keeps the unrounded float.
    expect(me.fromText("log_10(1000)").simplify().tree).toEqual(3);
    expect(me.fromText("log_7(343)").simplify().tree).toEqual(3);
    expect(me.fromText("log10(1000)").evaluate_to_constant()).toBe(3);
  });

  it("leaves an irrational or symbolic value alone", () => {
    // The exactness gate: folding must never turn an exact value into a float.
    expect(s(["apply", "sqrt", 2])).toEqual(["apply", "sqrt", 2]);
    expect(s(["apply", "log10", 3])).toEqual(["apply", "log10", 3]);
    // `asin(1)` used to be the example here, but it is π/2 — an *exact* value,
    // so it now folds (DOENET_INTEGRATION §3) without violating the gate this
    // test is about. `asin(2)` is off the lattice entirely; `atan(1/3)` is on
    // the principal branch but is not a rational multiple of π.
    expect(s(["apply", "asin", 2])).toEqual(["apply", "asin", 2]);
    expect(s(["apply", "atan", ["/", 1, 3]])).toEqual(["apply", "atan", ["/", 1, 3]]);
    expect(s(["apply", "std", ["tuple", 1, 2, 4]])).toEqual(["apply", "std", ["tuple", 1, 2, 4]]);
    expect(s(["apply", "sum", ["tuple", "x", "y"]])).toEqual(["apply", "sum", ["tuple", "x", "y"]]);
  });

  it("evaluates aggregates numerically too", () => {
    // These returned null before: an unknown application was sampled as an
    // opaque variable rather than evaluated.
    expect(me.fromText("sum(1,2,3)", AGGREGATES).evaluate_to_constant()).toBe(6);
    expect(me.fromText("max(1,5,3)", AGGREGATES).evaluate_to_constant()).toBe(5);
    expect(me.fromText("std(1,2,3)", AGGREGATES).evaluate_to_constant()).toBe(1);
  });

  it("does not change the default parse", () => {
    // Adding the aggregates to the parser's defaults would silently
    // reinterpret `mean` and `max` where they are used as variables, so they
    // stay opt-in — matching legacy, which also splits this into letters.
    expect(me.fromText("sum(3,17,5-4)").tree).toEqual([
      "*", "s", "u", "m", ["tuple", 3, 17, ["+", 5, -4]],
    ]);
  });
});

describe("item 3b — undefined quantities evaluate to undefined, not 0", () => {
  it("does not collapse 0*blank to a number", () => {
    // evaluate_to_constant underlies DoenetML's numeric reads; a hole must stay
    // undefined rather than simplify to 0/1.
    expect(me.fromText("0*_").evaluate_to_constant()).toBeNull();
    expect(me.fromText("(_-_)/(_-_)").evaluate_to_constant()).toBeNull();
    expect(me.fromText("2+3").evaluate_to_constant()).toBe(5);
  });
});

describe("item 7 — wasm loader injection seam", () => {
  it("resolves the node fallback with no injection", () => {
    expect(typeof setWasmModule).toBe("function");
    expect(me.fromText("1+1").toString()).toBe("1 + 1");
  });

  it("actually routes calls through an injected module, then back", async () => {
    // Exercise the seam rather than just asserting the export exists: inject a
    // module that wraps the real one and counts calls, and confirm the compat
    // layer went through it. This is the path a browser host takes with its
    // `--target web` build after `initSync(bytes)`.
    // Load the vendored build directly, not through `../lib/_wasm`'s default
    // export — that one is the forwarding Proxy, and delegating to it once the
    // spy is installed would just re-enter the spy.
    const { createRequire } = await import("node:module");
    const real = createRequire(import.meta.url)(
      "../vendor/wasm/math_expressions_wasm.js",
    ) as Record<string, unknown>;
    let parseCalls = 0;
    const spy = new Proxy({} as never, {
      get: (_t, prop) => {
        if (prop === "parse_text") {
          return (...args: unknown[]) => {
            parseCalls++;
            return (real[prop] as (...a: unknown[]) => unknown)(...args);
          };
        }
        return real[prop as string];
      },
    });

    setWasmModule(spy);
    try {
      expect(me.fromText("2+2").toString()).toBe("2 + 2");
      expect(parseCalls).toBeGreaterThan(0);
    } finally {
      // Clear the injection so the rest of the suite uses the node fallback.
      setWasmModule(undefined as never);
    }
    expect(me.fromText("3+3").toString()).toBe("3 + 3");
  });

  it("does not import node:module, so a browser bundle can resolve it", async () => {
    // A static `import ... from "node:module"` is evaluated by a browser bundle
    // even when setWasmModule is called first, and marking it external does not
    // help — the browser still cannot resolve the specifier. The node builtin is
    // reached through `process.getBuiltinModule` instead, which leaves nothing
    // for a bundler to resolve.
    const { readFile } = await import("node:fs/promises");
    const src = await readFile(new URL("../lib/_wasm.ts", import.meta.url), "utf8");
    expect(src).not.toMatch(/^\s*import\s[^;]*["']node:/m);
  });
});

describe("item 8 — handle lifetime & interner gauge", () => {
  it("free() releases the handle and is idempotent", () => {
    const e = me.fromText("x+1");
    expect(e.toString()).toBe("x + 1");
    e.free();
    expect(() => e.free()).not.toThrow(); // idempotent, no double-free
    e.dispose(); // alias, also safe
  });

  it("exposes an interner size that only grows", () => {
    const before = me.interner_size();
    me.fromText("aUniqueSymbolName_zzz1 + anotherUnique_zzz2");
    const after = me.interner_size();
    expect(typeof after).toBe("number");
    expect(after).toBeGreaterThanOrEqual(before);
  });
});

describe("item 6 — evaluate_numbers / passes", () => {
  const skip = (s: string) => me.fromText(s).evaluate_numbers({ skip_ordering: true }).tree;

  it("folds without reordering under skip_ordering", () => {
    // DoenetML's simplify="numberspreserveorder". A constant merges with an
    // adjacent constant but never hops over a symbolic term. Expectations taken
    // from the legacy library running the same option.
    expect(skip("1+x+2")).toEqual(["+", 1, "x", 2]);
    expect(skip("1+2+x")).toEqual(["+", 3, "x"]);
    expect(skip("x+1+2")).toEqual(["+", "x", 3]);
    expect(skip("1+x+2+3+y+4")).toEqual(["+", 1, "x", 5, "y", 4]);
    expect(skip("2*x*3")).toEqual(["*", 2, "x", 3]);
  });

  it("no longer throws — the throw was a hard crash for the Rust core", () => {
    // The core calls this mode and is built `panic = "abort"`, so an exception
    // unwinding into it was a WASM trap that killed the worker, not a catchable
    // missing feature.
    expect(() => me.fromText("1+x+2").evaluate_numbers({ skip_ordering: true })).not.toThrow();
    expect(() => me.fromText("1+x+2").evaluate_numbers()).not.toThrow();
    expect(() => me.fromText("1+x+2").evaluate_numbers({ skip_ordering: false })).not.toThrow();
  });

  it("stays distinguishable from the ordering form", () => {
    // If these ever coincide, the mode has silently stopped preserving order.
    expect(skip("1+x+2")).not.toEqual(me.fromText("1+x+2").evaluate_numbers().tree);
    expect(me.fromText("1+x+2").evaluate_numbers().tree).toEqual(["+", "x", 3]);
    // Like symbolic terms are not collected either, unlike the ordering form.
    expect(skip("x+x")).toEqual(["+", "x", "x"]);
  });

  it("leaves the still-unimplemented normalization passes as no-ops (not throws)", () => {
    // A blanket throw here regressed ~170 idempotent-input specs, so what is
    // left unimplemented stays a no-op returning the expression.
    expect(me.fromText("x").applyAllTransformations().tree).toEqual("x");
  });

  it("has graduated default_order out of the no-op list", () => {
    // This asserted `3+x` came back untouched, which was true only while the
    // pass did nothing. It is implemented now and carries the JS ordering key:
    // the legacy `trees/default_order.js` also answers `["+","x",3]` here.
    expect(me.fromText("3+x").default_order().tree).toEqual(["+", "x", 3]);
    expect(me.fromText("x+3").default_order().tree).toEqual(["+", "x", 3]);
  });
});

describe("review cycle 3 — legacy contracts that had quietly lapsed", () => {
  it("compiles an expression whose tree holds ±Infinity or NaN", () => {
    // `tree_json()` spells a non-finite as `{"$":"Inf"}`/`{"$":"NaN"}`, and the
    // mathjs converter takes all three as *numbers*. The compile path parsed
    // the JSON without decoding the tag, so it was handed a plain object and
    // rejected it as `Invalid ast`. Everything DoenetML plots or searches for
    // extrema goes through `f()`, and this PR deliberately folds `0/0` to NaN.
    expect(me.fromText("x+infinity").f()({ x: 1 })).toBe(Infinity);
    expect(me.fromText("x-infinity").f()({ x: 1 })).toBe(-Infinity);
    expect(me.fromText("0/0").simplify().f()({})).toBeNaN();
    expect(me.fromAst(["+", "x", NaN]).f()({ x: 1 })).toBeNaN();
    // The ordinary case is unaffected.
    expect(me.fromText("x^2").f()({ x: 3 })).toBe(9);
  });

  it("honors variables(include_subscripts)", () => {
    // The argument was dropped, so a caller matching against a subscripted
    // name — `Line.js`, deciding whether a coefficient mentions the line's own
    // variables — never found one.
    expect(me.fromText("x_1+y").variables(true)).toEqual(["x_1", "y"]);
    expect(me.fromText("x_1+y").variables()).toEqual(["x", "y"]);
    expect(me.fromText("x_1+y").variables(false)).toEqual(["x", "y"]);
  });

  it("accepts an Expression where a variable name is wanted", () => {
    // These two took the argument raw into a `&str` binding, where wasm-bindgen
    // reads a length off it and copies that many bytes — an out-of-bounds
    // access in a crate built `panic = "abort"`. Every other variable-taking
    // method already went through `varName`.
    const x = me.fromText("x");
    expect(
      me
        .fromText("x^2")
        .critical_points(x)!
        .map((p) => p.tree),
    ).toEqual([0]);
    expect(Array.from(me.fromText("x^2").evaluate_many(x, [1, 2, 3]))).toEqual([
      1, 4, 9,
    ]);
  });
});
