//! Every Example of the compiled Manual runs as a test (`design/manual.md`, "Examples are tests"): on a copy of its
//! fixture, or a new book, in an empty directory, with its fixed clock and zone, comparing each result or error
//! exactly or, where it says `"…"`, by shape (`red_ale_docs::example`). An Example that stops matching the code
//! fails here, so the Manual can't fall behind the API.
//!
//! Calls run in the directory holding the book, so an Example's paths are relative to it, as a user's would be to
//! where they run red-ale. The working directory is the process's, so this file holds one test, which changes it
//! for each Example in turn.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use red_ale_core::{FixedEnv, Time64, TimeZone};
use red_ale_docs::example::{BookState, Call, Example, Expected, mismatches};
use red_ale_rpc::Dispatcher;
use red_ale_service::Settings;
use serde_json::{Value, json};

/// The clock of an Example that sets none: 2024-07-01T12:00:00Z.
const DEFAULT_NOW: Time64 = Time64(1_719_835_200);

/// The book every Example works on, in its own directory.
const BOOK: &str = "book.gnucash";

#[test]
fn every_example_passes() {
    let manual = red_ale_rpc::manual::manual();
    assert!(!manual.examples().is_empty(), "the Manual has no Examples");
    let failures: Vec<String> = manual.examples().iter().flat_map(run).collect();
    assert!(failures.is_empty(), "{} Example failures:\n{}", failures.len(), failures.join("\n"));
}

/// Run one Example in a new directory; every way it fails.
fn run(example: &Example) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let mut d = dispatcher(example);
    let failures = match prepare(&mut d, example, dir.path()) {
        Ok(()) => example.calls.iter().enumerate().flat_map(|(i, c)| check(&mut d, i, c)).collect(),
        Err(e) => vec![e],
    };
    let _ = d.call("book.close", json!({ "discard": true }));
    std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).unwrap();
    failures.into_iter().map(|f| format!("{}: {f}", example.name())).collect()
}

/// A dispatcher whose sessions have the Example's clock and zone and sequential GUIDs, and whose Quote fetches can
/// reach nothing but loopback.
fn dispatcher(example: &Example) -> Dispatcher {
    let now = example.now.as_deref().map_or(DEFAULT_NOW, |n| {
        Time64(
            chrono::DateTime::parse_from_rfc3339(n)
                .unwrap_or_else(|e| panic!("{}: now={n}: {e}", example.name()))
                .timestamp(),
        )
    });
    let zone = example.zone.as_deref().map_or_else(TimeZone::utc, |z| {
        TimeZone::from_gnucash_posix(z).unwrap_or_else(|e| panic!("{}: zone={z}: {e}", example.name()))
    });
    let mut d = Dispatcher::new(Settings::default());
    d.restrict_quotes_to_loopback();
    let sessions = AtomicU32::new(0);
    d.set_env_factory(Box::new(move || {
        let seed = (0x00ed_u128 << 112) | (u128::from(sessions.fetch_add(1, Ordering::Relaxed)) << 80);
        Box::new(FixedEnv::new(now, seed).with_zone(zone.clone()))
    }));
    d
}

/// Put the book in the state the Example starts from.
fn prepare(d: &mut Dispatcher, example: &Example, dir: &Path) -> Result<(), String> {
    let path = json!({ "path": BOOK });
    let calls: Vec<(&str, Value)> = match (&example.fixture, example.book) {
        (_, BookState::None) => Vec::new(),
        (Some(fixture), state) => {
            copy_fixture(fixture, &dir.join(BOOK))?;
            if state == BookState::Open {
                vec![("book.open", path)]
            } else {
                Vec::new()
            }
        }
        (None, BookState::Open) => vec![("book.create", path)],
        (None, BookState::Closed) => vec![("book.create", path), ("book.close", json!({ "save": true }))],
    };
    calls
        .into_iter()
        .try_for_each(|(method, params)| d.call(method, params).map(drop).map_err(|e| format!("setting up, {method}: {e}")))
}

fn copy_fixture(name: &str, to: &Path) -> Result<(), String> {
    let file = if name.ends_with(".gnucash") {
        name.to_owned()
    } else {
        format!("{name}.gnucash")
    };
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(file);
    std::fs::copy(&from, to)
        .map(drop)
        .map_err(|e| format!("fixture {}: {e}", from.display()))
}

/// Make call number `i` and compare what it gives with what the Example says.
fn check(d: &mut Dispatcher, i: usize, call: &Call) -> Vec<String> {
    let outcome = d.call(&call.method, call.params.clone());
    let at = format!("call {} ({})", i + 1, call.method);
    match (&call.expected, outcome) {
        (Expected::Result(expected), Ok(actual)) => located(&at, mismatches(expected, &actual, "result")),
        (Expected::Error(expected), Err(e)) => located(&at, mismatches(expected, &serde_json::to_value(&e).unwrap(), "error")),
        (Expected::Result(_), Err(e)) => vec![format!("{at}: failed: {}", serde_json::to_value(&e).unwrap())],
        (Expected::Error(_), Ok(v)) => vec![format!("{at}: should fail, gave {v}")],
    }
}

fn located(at: &str, found: Vec<String>) -> Vec<String> {
    found.into_iter().map(|m| format!("{at}: {m}")).collect()
}
