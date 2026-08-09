// The `me.converters` namespace. The text/LaTeX converters are backed by the
// Rust core; the guppy and mathjs ones are pure-notation converters ported
// directly to TypeScript (math.js nodes and Guppy XML never reach Rust). MathML
// parsing has no equivalent and is still a stub that throws when used.
import TextToAst from "./text-to-ast";
import LatexToAst from "./latex-to-ast";
import AstToText from "./ast-to-text";
import AstToLatex from "./ast-to-latex";
import AstToGuppy from "./ast-to-guppy";
import AstToMathjs from "./ast-to-mathjs";
import MathjsToAst from "./mathjs-to-ast";

export const textToAstObj = TextToAst;
export const latexToAstObj = LatexToAst;
export const astToTextObj = AstToText;
export const astToLatexObj = AstToLatex;
export const astToGuppyObj = AstToGuppy;
export const astToMathjsObj = AstToMathjs;
export const mathjsToAstObj = MathjsToAst;

// Present so `me.converters.mmlToAstObj` etc. exist; unsupported at runtime.
export class mmlToAstObj {
  convert() {
    throw new Error(
      "math-expressions-js-compat: MathML parsing is not implemented",
    );
  }
}

export {
  TextToAst,
  LatexToAst,
  AstToText,
  AstToLatex,
  AstToGuppy,
  AstToMathjs,
  MathjsToAst,
};
