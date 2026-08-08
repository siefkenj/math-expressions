// Ported from the legacy library (`lib/expression/variables.js`). The compat
// polynomial code needs `operators`; `variables`/`functions` are ported for
// completeness. The legacy default mathjs config defined e/pi/i as constants,
// so we mirror that here (they are treated as numbers, not free variables).
import math from "../mathjs";
import { get_tree } from "../trees/util";

const m = math as any;
if (m.define_e === undefined) m.define_e = true;
if (m.define_pi === undefined) m.define_pi = true;
if (m.define_i === undefined) m.define_i = true;

function leaves(tree: any, include_subscripts?: boolean): any[] {
  if (!Array.isArray(tree)) return [tree];

  var operator = tree[0];
  var operands = tree.slice(1);

  if (include_subscripts && operator === "_") {
    if (
      typeof operands[0] === "string" &&
      (typeof operands[1] === "string" || typeof operands[1] === "number")
    )
      return [operands[0] + "_" + operands[1]];
  }

  if (operator === "apply") {
    operands = tree.slice(2);
  }
  if (operands.length === 0) return [];

  return operands
    .map(function (v: any) {
      return leaves(v, include_subscripts);
    })
    .reduce(function (a: any[], b: any[]) {
      return a.concat(b);
    });
}

function variables(expr_or_tree: any, include_subscripts = false): any[] {
  var tree = get_tree(expr_or_tree);

  var result = leaves(tree, include_subscripts);

  result = result.filter(function (v: any) {
    return (
      typeof v === "string" &&
      (m.define_e || v !== "e") &&
      (m.define_pi || v !== "pi") &&
      (m.define_i || v !== "i")
    );
  });

  result = result.filter(function (itm: any, i: number) {
    return i === result.indexOf(itm);
  });

  return result;
}

function operators_list(tree: any): any[] {
  if (!Array.isArray(tree)) return [];

  var operator = tree[0];
  var operands = tree.slice(1);

  if (operator === "apply") {
    operands = tree.slice(2);
  }
  if (operands.length === 0) return [operator];

  return [operator].concat(
    operands
      .map(function (v: any) {
        return operators_list(v);
      })
      .reduce(function (a: any[], b: any[]) {
        return a.concat(b);
      }),
  );
}

function operators(expr_or_tree: any): any[] {
  var tree = get_tree(expr_or_tree);

  var result = operators_list(tree);

  result = result.filter(function (v: any) {
    return v !== "apply";
  });

  result = result.filter(function (itm: any, i: number) {
    return i === result.indexOf(itm);
  });

  return result;
}

function functions_list(tree: any): any[] {
  if (!Array.isArray(tree)) {
    return [];
  }

  var operator = tree[0];
  var operands = tree.slice(1);

  var functions: any[] = [];
  if (operator === "apply") {
    functions = [operands[0]];
    operands = tree.slice(2);
  }

  return functions.concat(
    operands
      .map(function (v: any) {
        return functions_list(v);
      })
      .reduce(function (a: any[], b: any[]) {
        return a.concat(b);
      }, []),
  );
}

function functions(expr_or_tree: any): any[] {
  var tree = get_tree(expr_or_tree);

  var result = functions_list(tree);

  result = result.filter(function (itm: any, i: number) {
    return i === result.indexOf(itm);
  });

  return result;
}

export { variables, operators, functions };
