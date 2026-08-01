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
  it("maps NaN and ±Infinity to their specials instead of null", () => {
    expect(me.fromAst(NaN).tree).toEqual({ $: "NaN" });
    expect(me.fromAst(Infinity).tree).toEqual({ $: "Inf" });
    expect(me.fromAst(-Infinity).tree).toEqual({ $: "-Inf" });
  });

  it("preserves a NaN nested in a tree", () => {
    expect(me.fromAst(["tuple", NaN, 1]).tree).toEqual(["tuple", { $: "NaN" }, 1]);
  });

  it("is a fixpoint: the tagged form revives to itself", () => {
    // The contract DOENET_INTEGRATION.md §4 documents — `.tree` stays tagged, so
    // a value survives any number of save/revive cycles unchanged. Untagging on
    // the way out would not extend to `None`, which has no JS scalar.
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

  it("throws on evaluate_numbers({skip_ordering}) but honors the plain form", () => {
    // skip_ordering has no core support and silently reordered before; reject it
    // loudly. The plain form and a falsy flag pass through.
    expect(() => me.fromText("1+x+2").evaluate_numbers({ skip_ordering: true })).toThrow(
      /skip_ordering/,
    );
    expect(() => me.fromText("1+x+2").evaluate_numbers()).not.toThrow();
    expect(() => me.fromText("1+x+2").evaluate_numbers({ skip_ordering: false })).not.toThrow();
  });

  it("leaves the unimplemented normalization passes as no-ops (not throws)", () => {
    // A blanket throw here regressed ~170 idempotent-input specs, so these stay
    // no-ops returning the expression until they are properly implemented.
    expect(me.fromText("3+x").default_order().tree).toEqual(me.fromText("3+x").tree);
    expect(me.fromText("x").applyAllTransformations().tree).toEqual("x");
  });
});
