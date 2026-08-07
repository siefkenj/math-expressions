// Three grading paths DoenetML calls that the compat layer did not answer, or
// answered differently from the JS library. Each is checked at the boundary,
// which is where DoenetML meets them.
import { describe, it, expect } from "vitest";
import me from "../lib/math-expressions";

describe("sign-error grading (numSignErrorsMatched)", () => {
  // `me.equalSpecifiedSignErrors` was simply absent, so *any* award carrying
  // `numSignErrorsMatched` threw on submit and the answer never registered as
  // submitted at all.
  const target = me.fromText("x^2-2x+3");
  const eqf = (a: any, b: any) => a.equals(b);
  const one = (s: string) =>
    me.equalSpecifiedSignErrors(me.fromText(s), target, {
      equalityFunction: eqf,
      n_sign_errors: 1,
    });

  it("accepts a single flipped sign and rejects two", () => {
    expect(one("x^2+2x+3")).toBe(true); // the middle term's sign
    expect(one("x^2-2x-3")).toBe(true); // the constant's sign
    expect(one("x^2+2x-3")).toBe(false); // both — that is two errors
  });

  it("reports how many flips it took", () => {
    expect(
      me.equalWithSignErrors(me.fromText("x^2-2x+3"), target, {
        equalityFunction: eqf,
      }),
    ).toEqual({ matched: true, n_sign_errors: 0 });
    expect(
      me.equalWithSignErrors(me.fromText("x^2+2x+3"), target, {
        equalityFunction: eqf,
      }),
    ).toEqual({ matched: true, n_sign_errors: 1 });
  });
});

describe("default_order (simplify=normalizeOrder)", () => {
  // Was a no-op returning `this`, so an attribute whose entire job is to sort
  // did nothing and two orderings of one sum never matched.
  it("sorts without evaluating", () => {
    const a = me.fromText("1x^2+2-0x^2+3+x^2+3x^2+7+4").default_order();
    const b = me.fromText("4-0x^2 +7+ (x^2)1+3+x^2+2+(x^2)3").default_order();
    expect(a.equalsViaSyntax(b)).toBe(true);
    // Every term survives: the constants stay unfolded (7 and 4 are still two
    // terms, not 11) and the `0x^2` term is still there.
    const operands = a.tree.slice(1);
    expect(operands.filter((t: any) => t === 7 || t === 4).length).toBe(2);
    expect(JSON.stringify(operands)).toContain('["*",0,["^","x",2]]');
  });
});

describe("exp and e^ are one spelling", () => {
  it("normalize_function_names folds them together", () => {
    const a = me.fromText("-5e^(-t)").normalize_function_names().simplify();
    const b = me.fromText("-5exp(-t)").normalize_function_names().simplify();
    expect(a.equalsViaSyntax(b)).toBe(true);
    // ...and the reciprocal spellings land there too.
    const c = me.fromText("-5/e^t").normalize_function_names().simplify();
    const d = me.fromText("-5/exp(t)").normalize_function_names().simplify();
    expect(c.equalsViaSyntax(d)).toBe(true);
  });
});
