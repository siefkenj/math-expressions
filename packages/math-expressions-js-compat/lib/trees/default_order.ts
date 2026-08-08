// `default_order` was a standalone per-tree ordering pass. The Rust core folds
// ordering into `canonicalize`, with no separate tree-level entry point, so this
// is a compat stub: it returns the tree unchanged. Specs asserting a specific
// re-ordering will fail here (see JS_TEST_COVERAGE_AUDIT.md); the suite runs.
export function default_order(tree) {
  return tree;
}

export default default_order;

// ---------------------------------------------------------------------------
// `compare_function`: a total order on trees, ported from the legacy
// `lib/trees/default_order.js`. The compat polynomial module relies on it to
// order variables. This is additive — it does not affect `default_order`
// above. The legacy `sort_key` had a `unit` branch depending on units.js;
// polynomial variables never carry units, so that branch is omitted here.
function sort_key(tree: any, params: any = {}): any {
  if (typeof tree === "number") {
    if (params.ignore_negatives) return [0, "number", Math.abs(tree)];
    return [0, "number", tree];
  }
  if (typeof tree === "string") {
    if (tree === "-" || tree === "+") {
      return [8, "plus_minus_string", tree];
    }
    return [1, "symbol", tree];
  }
  if (typeof tree === "boolean") {
    return [1, "boolean", tree];
  }

  if (!Array.isArray(tree)) return [-1, "unknown", tree];

  var operator = tree[0];
  var operands = tree.slice(1);

  if (operator === "^" && operands.length === 2) {
    return [
      ...sort_key(operands[0], params),
      "power",
      ...sort_key(operands[1], params),
    ];
  }

  if (operator === "apply") {
    var key: any = [2, "function", operands[0]];
    if (
      operands[0] === "sqrt" ||
      operands[0] === "cbrt" ||
      operands[0] === "nthroot"
    ) {
      key = [
        5,
        "root",
        operands[0] === "sqrt"
          ? 2
          : operands[0] === "cbrt"
            ? 3
            : (operands[1][2] ?? 2),
      ];
    }

    var f_args = operands[1];

    var n_args = 1;

    var arg_keys: any[] = [];

    if (Array.isArray(f_args)) {
      f_args = f_args.slice(1); // remove vector operator

      n_args = f_args.length;

      arg_keys = f_args.map((x: any) => sort_key(x, params));
    } else {
      arg_keys = [sort_key(f_args, params)];
    }

    key.push([n_args, arg_keys]);

    return key;
  }

  var n_factors = operands.length;

  var factor_keys = operands.map((o: any) => sort_key(o, params));

  if (operator === "*") {
    return [4, "product", n_factors, factor_keys];
  }

  if (operator === "/") {
    return [4, "quotient", n_factors, factor_keys];
  }

  if (operator === "+") {
    return [5, "sum", n_factors, factor_keys];
  }

  if (operator === "-") {
    if (params.ignore_negatives) return factor_keys[0];
    return [6, "minus", n_factors, factor_keys];
  }

  if (operator === "pm") {
    if (params.ignore_negatives) return factor_keys[0];
    return [6, "pm", n_factors, factor_keys];
  }

  if (["tuple", "vector", "altvector"].includes(operator)) {
    return [7, n_factors, factor_keys];
  } else if (operator === "interval") {
    if (operands[1][1] === false) {
      if (operands[1][2] === false) {
        return [7, ...factor_keys[0].slice(1)];
      } else {
        return [8, n_factors, factor_keys];
      }
    } else if (operands[1][2] === false) {
      return [8, n_factors, factor_keys];
    } else {
      return [9, ...factor_keys[0].slice(1)];
    }
  } else if (operator === "array") {
    return [9, n_factors, factor_keys];
  }

  return [10, operator, n_factors, factor_keys];
}

function arrayCompare(a: any, b: any): number {
  if (Array.isArray(a)) {
    if (Array.isArray(b)) {
      let minLength = Math.min(a.length, b.length);
      for (let i = 0; i < minLength; i++) {
        let comp = arrayCompare(a[i], b[i]);
        if (comp !== 0) {
          return comp;
        }
      }
      return a.length < b.length ? -1 : a.length > b.length ? 1 : 0;
    } else {
      return 1;
    }
  } else {
    if (Array.isArray(b)) {
      return -1;
    } else {
      return a < b ? -1 : a > b ? 1 : 0;
    }
  }
}

export function compare_function(a: any, b: any, params: any = {}): number {
  var key_a = sort_key(a, params);
  var key_b = sort_key(b, params);

  return arrayCompare(key_a, key_b);
}

export { sort_key };
