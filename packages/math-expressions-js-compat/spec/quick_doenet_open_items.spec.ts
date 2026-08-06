// The three DOENET_INTEGRATION items that were still open, checked at the
// library boundary — which is where DoenetML meets them, and where each was
// originally reported as a reproducible one-liner.
import { describe, it, expect } from "vitest";
import me from "../lib/math-expressions";

describe("evaluate_to_constant reports an infinite value", () => {
  // Item 1. An unbounded endpoint is ordinary — [-∞, ∞] is the default domain
  // of a function curve — and `null` is not a way to say so: it crashes
  // `fromAst` on the way back into a tree, and reads as 0 to `Math.max` where
  // it does not crash, silently turning an unbounded endpoint into a bounded
  // one.
  it("returns the infinity it was given", () => {
    expect(me.fromAst(-Infinity).evaluate_to_constant()).toBe(-Infinity);
    expect(me.fromAst(Infinity).evaluate_to_constant()).toBe(Infinity);
    expect(me.fromAst(-10).evaluate_to_constant()).toBe(-10); // unchanged
  });

  it("survives the round trip back into a tree", () => {
    const back = me.fromAst(me.fromAst(-Infinity).evaluate_to_constant());
    expect(back.tree).toEqual(-Infinity);
  });

  it("reaches infinity through arithmetic too", () => {
    expect(me.fromText("1/0").evaluate_to_constant()).toBe(Infinity);
    expect(me.fromText("Infinity+1").evaluate_to_constant()).toBe(Infinity);
    expect(me.fromText("1/Infinity").evaluate_to_constant()).toBe(0);
  });

  // `null` is reserved for what is genuinely *undecided* — a free variable.
  // An indeterminate form is decided: the answer is `NaN`, and reporting it as
  // `null` loses that, because `null` coerces to `0` on the JS side and would
  // present an undefined result as a real value.
  it("reports an indeterminate form as NaN, and only a free variable as null", () => {
    expect(me.fromText("Infinity-Infinity").evaluate_to_constant()).toBeNaN();
    expect(me.fromText("0/0").evaluate_to_constant()).toBeNaN();
    expect(me.fromText("x+1").evaluate_to_constant()).toBe(null);
  });

  // Legacy returned a math.js complex object for a non-real value rather than
  // discarding it. The wasm entry point reports only the real case, so the
  // wrapper falls back to `evaluate_to_complex`.
  it("keeps a complex value instead of dropping it", () => {
    const v = me.fromText("i").evaluate_to_constant();
    expect(v.re).toBe(0);
    expect(v.im).toBe(1);
    expect(me.fromText("2+3").evaluate_to_constant()).toBe(5); // still a number
  });
});

describe("scientific notation past the magnitude threshold", () => {
  // Item 3. Legacy had no threshold of its own — it called `toString()` and
  // switched whenever the result contained an `e` — so the switch is the
  // ECMAScript rule, which is what `avoidScientificNotation` is named against.
  it("switches where JavaScript's toString does", () => {
    expect(me.fromAst(1e20).toString()).toEqual("100000000000000000000");
    expect(me.fromAst(1e21).toString()).toEqual("1 * 10^21");
    expect(me.fromAst(1e-6).toString()).toEqual("0.000001");
    expect(me.fromAst(1e-7).toString()).toEqual("1 * 10^(-7)");
  });

  it("spells it per output format", () => {
    expect(me.fromAst(1.23e22).toString()).toEqual("1.23 * 10^22");
    expect(me.fromAst(1.23e22).toLatex()).toEqual("1.23 \\cdot 10^{22}");
    expect(me.fromAst(1.23e-11).toString()).toEqual("1.23 * 10^(-11)");
    expect(me.fromAst(1.23e-11).toLatex()).toEqual("1.23 \\cdot 10^{-11}");
  });

  it("honors avoidScientificNotation", () => {
    const opts = { avoidScientificNotation: true };
    expect(me.fromAst(1.23e30).toString(opts)).toEqual(
      "1230000000000000000000000000000",
    );
    expect(me.fromAst(1.23e-12).toLatex(opts)).toEqual("0.00000000000123");
  });

  // The rendered form is the product it is spelled as, so it parenthesises as
  // a power's base — and re-parses, which the bare `1.23e22` form would not.
  it("stays re-parseable", () => {
    const s = me.fromAst(1.23e-11).toString();
    expect(me.fromText(s).evaluate_to_constant()).toBeCloseTo(1.23e-11, 20);
  });
});

describe("a float survives the AST boundary unchanged", () => {
  // Not one of the three, but it blocked the item-3 output from being right:
  // serde_json's float parser is off by one ulp on some literals, so the tree
  // held a different number than was passed in and the mantissa printed as
  // 1.2300000000000001.
  it("round-trips the exact double", () => {
    for (const v of [1.23e-26, 1.23e-11, 0.1, 1.5e300, 4.35e-15]) {
      expect(me.fromAst(v).tree, String(v)).toBe(v);
    }
  });
});
