//! Text and LaTeX → [`Expr`](crate::expr::Expr): faithful ports of the JS
//! `text-to-ast.js` / `latex-to-ast.js` recursive-descent parsers.
//!
//! Both parsers share one engine: the hand-written [`lexer`] (a first-match
//! ordered regex table, text and LaTeX flavours), the pure state-free helpers
//! in [`common`], and the grammar skeleton in [`shared_grammar`] (statement /
//! relation / expression / term / factor productions). Only the flavour-
//! specific rules live in [`text`] and [`latex`]. Errors ([`error`]) carry a
//! byte offset and match the JS `ParseError` message text verbatim, since the
//! error fixtures assert on it.
//!
//! The parsers emit the **faithful** tree — raw associative grouping, `Div`
//! and `Neg` intact, nothing folded — which [`crate::normalize`] then
//! canonicalizes. (Parsers/lexers are the one place exempt from the
//! ~200-line split rule: the productions are a single grammar and read best
//! whole.)

pub mod common;
pub mod error;
pub mod latex;
pub mod lexer;
pub(crate) mod shared_grammar;
pub mod text;

pub use error::ParseError;
