//! Shared fixture-test harness: the parser fixture runner, plus [`caught`],
//! the differential corpora's "run this and tolerate a panic" wrapper.

// Each integration-test binary compiles its own copy of this module and uses
// only the part it needs, so unused-item warnings here are structural.
#![allow(dead_code)]

use math_expressions::expr::serde::to_js;
use math_expressions::{Expr, ParseError};
use serde_json::Value;
use std::cell::Cell;

thread_local! {
    /// Set for the duration of a [`caught`] call, read by the panic hook.
    static INSIDE_CAUGHT: Cell<bool> = const { Cell::new(false) };
}

/// Run `f`, turning a panic into `None` and suppressing the panic *message* —
/// but only for panics raised inside `f`.
///
/// The differential corpora deliberately feed the engine thousands of hostile
/// inputs and accept that some panic, so the default hook's output would bury
/// the real result. The obvious way to quiet it, `panic::set_hook(Box::new(|_|
/// {}))`, is a trap: libtest prints a failing test's panic text **from the
/// hook**, not from the unwind payload it re-catches, so a process-wide no-op
/// hook also erases the message of every genuine `assert!` in the file — the
/// suite reports a bare `test … FAILED` with nothing to debug. This wrapper
/// therefore installs a *filtering* hook (once per process) that delegates to
/// the previous hook unless the panic came from inside `caught`.
pub fn caught<T>(f: impl FnOnce() -> T) -> Option<T> {
    install_filtering_hook();
    INSIDE_CAUGHT.with(|c| c.set(true));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok();
    INSIDE_CAUGHT.with(|c| c.set(false));
    out
}

fn install_filtering_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !INSIDE_CAUGHT.with(|c| c.get()) {
                previous(info);
            }
        }));
    });
}

#[derive(serde::Deserialize)]
pub struct TreeCase {
    pub input: String,
    pub tree: Value,
}

#[derive(serde::Deserialize)]
pub struct ErrorCase {
    pub input: String,
    pub error: String,
}

/// Run every tree case through `parse`, comparing the js::tree encoding of the
/// result with the fixture. Panics with a report of all failures.
pub fn run_tree_cases(fixture_json: &str, mut parse: impl FnMut(&str) -> Result<Expr, ParseError>) {
    let cases: Vec<TreeCase> = serde_json::from_str(fixture_json).unwrap();
    assert!(!cases.is_empty(), "fixture file is empty");
    let mut failures = vec![];

    for case in &cases {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parse(&case.input))) {
            Ok(Ok(expr)) => {
                let got = to_js(&expr);
                if got != case.tree {
                    failures.push(format!(
                        "{:?}\n  expected: {}\n  got:      {}",
                        case.input, case.tree, got
                    ));
                }
            }
            Ok(Err(e)) => failures.push(format!("{:?}\n  parse error: {}", case.input, e)),
            Err(_) => failures.push(format!("{:?}\n  PANICKED", case.input)),
        }
    }

    if !failures.is_empty() {
        panic!(
            "{}/{} fixture cases failed:\n\n{}",
            failures.len(),
            cases.len(),
            failures
                .iter()
                .take(40)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Run every error case through `parse`, expecting a ParseError whose message
/// contains the fixture text (vitest's toThrow(msg) checks containment).
pub fn run_error_cases(
    fixture_json: &str,
    mut parse: impl FnMut(&str) -> Result<Expr, ParseError>,
) {
    let cases: Vec<ErrorCase> = serde_json::from_str(fixture_json).unwrap();
    assert!(!cases.is_empty(), "fixture file is empty");
    let mut failures = vec![];

    for case in &cases {
        match parse(&case.input) {
            Ok(expr) => failures.push(format!(
                "{:?}: expected error {:?}, parsed as {}",
                case.input,
                case.error,
                to_js(&expr)
            )),
            Err(e) => {
                if !e.message.contains(&case.error) {
                    failures.push(format!(
                        "{:?}: expected error containing {:?}, got {:?}",
                        case.input, case.error, e.message
                    ));
                }
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{}/{} error cases failed:\n{}",
            failures.len(),
            cases.len(),
            failures.join("\n")
        );
    }
}
