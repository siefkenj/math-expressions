// Single-variable polynomial helpers, ported faithfully from the legacy
// `lib/polynomial/single-var-poly.js`. Used by the compat polynomial module's
// `pt_reduce_rational_expression` fast path for single-variable inputs.
import * as simplify from "../expression/simplify";

function single_var(poly: any): any {
  //takes polynomial in terms representation ["polynomial_terms", (highest term, monomial),...,(lowest term, monomial)] returns the variable if the polynomial has only one variable, false otherwise.
  if (!Array.isArray(poly) || poly[0] !== "polynomial_terms") return "_true"; //if polynomial is constant, it's single variable.

  let len = poly.length;
  let vars = new Set();
  let variable: any;
  for (let i = 1; i < len; i = i + 1) {
    if (Array.isArray(poly[i]) && poly[i][0] === "monomial") {
      if (poly[i][2].length > 1) return "_false";
      vars.add(poly[i][2][0][0]);
      variable = poly[i][2][0][0];
      if (vars.size > 1) return "_false";
    }
  }
  return variable;
}

function poly_to_sv(poly: any, variable: any): any {
  //takes a single variable polynomial in terms representation ["polynomial_terms", (highest term, monomial),...,(lowest term, monomial)] with variable given and converts to simpler single variable representation ["sv_poly", var, [[highest deg, coeff],...,[lowest deg, coeff]]]. Should only be called on single variable polynomials.
  if (!Array.isArray(poly) || poly[0] !== "polynomial_terms") return poly; //if polynomial is constant, don't need to convert.

  let len = poly.length;
  let sv: any = ["sv_poly", variable, []];
  for (let i = 1; i < len; i = i + 1) {
    if (Array.isArray(poly[i]) && poly[i][0] === "monomial")
      sv[2].push([poly[i][2][0][1], poly[i][1]]);
    else sv[2].push([0, poly[i]]);
  }
  return sv;
}

function sv_to_poly(f: any): any {
  //takes a single variable polynomial and converts to terms representation.

  if (!Array.isArray(f) || f[0] !== "sv_poly") return f; //if polynomial is constant, don't need to convert.

  let terms = f[2];
  let len = f[2].length;
  let poly: any = ["polynomial_terms"];
  for (let i = 0; i < len; i = i + 1) {
    poly.push(["monomial", terms[i][1], [[f[1], terms[i][0]]]]);
  }

  return poly;
}

function sv_deg(poly: any): any {
  //takes a single variable polynomial and returns the degree.
  if (!Array.isArray(poly) || poly[0] !== "sv_poly") return 0;

  return poly[2][0][0];
}

function sv_add(f: any, g: any): any {
  //takes two single variable polynomials in the same variable and returns their sum
  let coeff_sum: any = 0;

  if (!Array.isArray(g) || g[0] !== "sv_poly") {
    //if g is constant
    if (g === 0) {
      return f;
    }

    if (!Array.isArray(f) || f[0] !== "sv_poly") {
      //if f is also constant, return their sum as constants
      return simplify.simplify(["+", f, g]);
    }
    let sum: any = ["sv_poly", f[1], []];
    let len = f[2].length;
    for (let i = 0; i < len - 1; i = i + 1) {
      sum[2].push(f[2][i]);
    }
    let i = len - 1;
    if (f[2][i][0] === 0) {
      coeff_sum = simplify.simplify(["+", f[2][i][1], g]);
      if (coeff_sum !== 0) sum[2].push([0, coeff_sum]);
    } else {
      sum[2].push(f[2][i]);
      sum[2].push([0, g]);
    }
    return sum;
  }

  if (!Array.isArray(f) || f[0] !== "sv_poly") {
    //if f is constant
    if (f === 0) {
      return g;
    }

    let sum: any = ["sv_poly", g[1], []];
    let len = g[2].length;
    for (let i = 0; i < len - 1; i = i + 1) {
      sum[2].push(g[2][i]);
    }
    let i = len - 1;
    if (g[2][i][0] === 0) {
      coeff_sum = simplify.simplify(["+", f, g[2][i][1]]);
      if (coeff_sum !== 0) sum[2].push([0, coeff_sum]);
    } else {
      sum[2].push(g[2][i]);
      sum[2].push([0, f]);
    }
    return sum;
  }

  let sum: any = ["sv_poly", f[1], []];
  let len_f = f[2].length;
  let len_g = g[2].length;
  let i = 0;
  let j = 0;
  while (i < len_f && j < len_g) {
    if (f[2][i][0] > g[2][j][0]) {
      sum[2].push(f[2][i]);
      i = i + 1;
    } else if (f[2][i][0] < g[2][j][0]) {
      sum[2].push(g[2][j]);
      j = j + 1;
    } else {
      coeff_sum = simplify.simplify(["+", f[2][i][1], g[2][j][1]]);
      if (coeff_sum !== 0) sum[2].push([f[2][i][0], coeff_sum]);
      i = i + 1;
      j = j + 1;
    }
  }

  while (i < len_f) {
    sum[2].push(f[2][i]);
    i = i + 1;
  }

  while (j < len_g) {
    sum[2].push(g[2][j]);
    j = j + 1;
  }

  if (sum[2].length === 0) return 0;

  if (sum[2][0][0] === 0) return sum[2][0][1]; //if there's only a constant left, return it as a constant

  return sum;
}

function sv_neg(f: any): any {
  //takes a single variable polynomial and returns its negation

  if (!Array.isArray(f) || f[0] !== "sv_poly")
    return simplify.simplify(["-", f]);

  let neg_f: any = ["sv_poly", f[1], []];
  let len = f[2].length;
  for (let i = 0; i < len; i = i + 1) {
    neg_f[2].push([f[2][i][0], simplify.simplify(["-", f[2][i][1]])]);
  }

  return neg_f;
}

function sv_sub(f: any, g: any): any {
  //takes a single variable polynomial and returns the difference f-g

  return sv_add(f, sv_neg(g));
}

function sv_mul(f: any, g: any): any {
  //takes two single variable polynomials and returns their product

  if (!Array.isArray(f) || f[0] !== "sv_poly") {
    if (f === 1) {
      return g;
    }

    if (f === 0) {
      return 0;
    }

    if (!Array.isArray(g) || g[0] !== "sv_poly") {
      return simplify.simplify(["*", f, g]);
    }
    let prod: any = ["sv_poly", g[1], []];
    let len = g[2].length;
    for (let i = 0; i < len; i = i + 1) {
      prod[2].push([g[2][i][0], simplify.simplify(["*", g[2][i][1], f])]);
    }
    return prod;
  }

  if (!Array.isArray(g) || g[0] !== "sv_poly") {
    if (g === 1) {
      return f;
    }

    if (g === 0) {
      return 0;
    }

    let prod: any = ["sv_poly", f[1], []];
    let len = f[2].length;
    for (let i = 0; i < len; i = i + 1) {
      prod[2].push([f[2][i][0], simplify.simplify(["*", f[2][i][1], g])]);
    }
    return prod;
  }

  let terms: any[] = [];
  let len_f = f[2].length;
  let len_g = g[2].length;
  for (let i = 0; i < len_f; i = i + 1) {
    for (let j = 0; j < len_g; j = j + 1) {
      terms.push([
        f[2][i][0] + g[2][j][0],
        simplify.simplify(["*", f[2][i][1], g[2][j][1]]),
      ]);
    }
  }

  terms.sort(function (a, b) {
    return b[0] - a[0];
  });

  let combined: any[] = [terms[0]];
  let end = 0;
  let coeff_sum: any = 0;
  let len = terms.length;
  for (let i = 1; i < len; i = i + 1) {
    end = combined.length - 1;
    if (terms[i][0] === combined[end][0]) {
      coeff_sum = simplify.simplify(["+", combined[end][1], terms[i][1]]);
      if (coeff_sum !== 0) combined[end][1] = coeff_sum;
      else combined.pop();
    } else combined.push(terms[i]);
  }

  return ["sv_poly", f[1], combined];
}

function sv_leading(f: any): any {
  if (!Array.isArray(f) || f[0] !== "sv_poly") return f;
  return ["sv_poly", f[1], [f[2][0]]];
}

function sv_div_lt(term1: any, term2: any): any {
  if (!Array.isArray(term2) || term2[0] !== "sv_poly") {
    if (!Array.isArray(term1) || term1[0] !== "sv_poly")
      return simplify.simplify(["/", term1, term2]);
    let coeff_ratio = simplify.simplify(["/", term1[2][0][1], term2]);
    return ["sv_poly", term1[1], [[term1[2][0][0], coeff_ratio]]];
  }

  if (!Array.isArray(term1) || term1[0] !== "sv_poly") return undefined;

  if (sv_deg(term1) < sv_deg(term2)) return undefined;

  let coeff_ratio = simplify.simplify(["/", term1[2][0][1], term2[2][0][1]]);
  let deg = term1[2][0][0] - term2[2][0][0];

  if (deg === 0) return coeff_ratio;

  return ["sv_poly", term1[1], [[deg, coeff_ratio]]];
}

function sv_div(f: any, g: any): any {
  //takes single variable polynomials f,g and returns quotient and remainder [q,r] such that f = gq+r, and deg(r) < deg(g) or r=0.

  let q: any = 0;
  let r: any = f;
  let div_lt: any = 0;
  while (r !== 0 && sv_deg(g) <= sv_deg(r)) {
    div_lt = sv_div_lt(sv_leading(r), sv_leading(g));
    q = sv_add(q, div_lt);
    r = sv_sub(r, sv_mul(div_lt, g));
  }

  return [q, r];
}

function sv_gcd(f: any, g: any): any {
  //takes single variable polynomials f,g and returns their gcd as a single variable polynomial.

  let h: any = f;
  let s: any = g;
  let rem: any = 0;

  while (s !== 0) {
    rem = sv_div(h, s)[1];
    h = s;
    s = rem;
  }

  if (!Array.isArray(h) || h[0] !== "sv_poly") return 1; //if gcd is constant, return 1

  let leading_coeff = h[2][0][1];
  return sv_div(h, leading_coeff)[0];
}

export {
  single_var,
  poly_to_sv,
  sv_to_poly,
  sv_add,
  sv_neg,
  sv_sub,
  sv_mul,
  sv_leading,
  sv_div_lt,
  sv_div,
  sv_gcd,
};
