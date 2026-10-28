//! Since Last Run at scale: daily scheduled transactions added to a synthetic book, then one
//! `scheduledTransaction.sinceLastRun` and one `scheduledTransaction.apply` through the dispatcher, with what they
//! created, the Book revision and journal around them, and their time and heap (`design/milestone4.md`, "Done when").

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::memory::HeapCounter;
use crate::{TempBook, call, dispatcher};

/// The first occurrence of every schedule [`add_daily_schedules`] adds, so that up to 1,643 daily occurrences fall
/// due by [`crate::TEST_NOW`] (2024-07-01).
const FIRST_OCCURRENCE: &str = "2020-01-01";

/// Leaf accounts under each synthetic top-level account ([`crate::synthetic::ACCOUNTS`] less the root and the five,
/// spread over five).
const LEAVES: usize = 398;

/// Add `schedules` scheduled transactions to the synthetic book at `path`, each daily from 2020-01-01 for
/// `occurrences` occurrences, moving 12.34 from a synthetic asset account to an expense account, and save the book.
/// None is auto-create and none reminds in advance, so a Since Last Run at [`crate::TEST_NOW`] proposes every
/// occurrence to create: `schedules * occurrences` of them, for `occurrences` up to 1,643.
pub fn add_daily_schedules(path: &Path, schedules: usize, occurrences: usize) {
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": path.display().to_string() }));
    (0..schedules).for_each(|i| drop(call(&mut d, "scheduledTransaction.create", &schedule(i, occurrences))));
    call(&mut d, "book.close", &json!({ "save": true }));
}

fn schedule(i: usize, occurrences: usize) -> Value {
    let leaf = i % LEAVES;
    json!({
        "name": format!("Daily {i:04}"),
        "startDate": FIRST_OCCURRENCE,
        "schedule": [{ "mult": 1, "periodType": "day" }],
        "end": { "occurrences": occurrences },
        "templateTransactions": [{
            "description": format!("Scheduled {i:04}"),
            "splits": [
                { "account": format!("Expenses:Expenses {leaf:04}"), "debitFormula": "12.34" },
                { "account": format!("Assets:Assets {leaf:04}"), "creditFormula": "12.34" },
            ],
        }],
    })
}

/// What one Since Last Run did and cost. Heap figures are bytes above what was allocated before `book.open`, as in
/// [`crate::memory::Measurement`]; they are zero without a [`HeapCounter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// Instances the proposal held to create.
    pub to_create: usize,
    /// Transactions the apply's result lists as created.
    pub created: usize,
    /// Transactions in the Book before the apply and after it.
    pub transactions: (u64, u64),
    /// The Book revision the proposal was made at, and the one a second proposal, made after the apply, was made at.
    pub revisions: (u64, u64),
    /// Records the journal holds once the apply returns (none before: a session's journal starts with its first
    /// change), and the distinct transactions they name.
    pub journal_records: usize,
    /// Distinct transactions the journal's records name.
    pub journal_transactions: usize,
    /// Held once `book.open` returned.
    pub at_rest: usize,
    /// The most held at once while `scheduledTransaction.sinceLastRun` ran.
    pub peak_propose: usize,
    /// The most held at once while `scheduledTransaction.apply` ran.
    pub peak_apply: usize,
    /// Held once the apply returned and the proposal was dropped.
    pub after_apply: usize,
    /// The most held at once while the `book.save` after the apply ran.
    pub peak_save: usize,
    /// Wall-clock time of the proposal.
    pub propose: Duration,
    /// Wall-clock time of the apply.
    pub apply: Duration,
    /// Wall-clock time of the save that writes what the apply created.
    pub save: Duration,
}

/// Open a copy of the book at `path`, propose a Since Last Run, apply it with no choices (every instance created as
/// proposed), propose again and save.
pub fn run(path: &Path, heap: Option<&dyn HeapCounter>) -> Run {
    let book = TempBook::new_path();
    std::fs::copy(path, &book.path).unwrap_or_else(|e| panic!("copying {}: {e}", path.display()));
    let mut d = dispatcher();
    let base = heap.map_or(0, HeapCounter::current);
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    let at_rest = held(heap, base);
    let before = transactions(&mut d);
    let ((proposal, propose), peak_propose) = step(heap, base, || {
        timed(|| call(&mut d, "scheduledTransaction.sinceLastRun", &json!({})))
    });
    let to_create = proposal["summary"]["toCreate"]
        .as_u64()
        .map_or(0, |n| usize::try_from(n).unwrap_or(0));
    let params = json!({ "proposal": proposal["id"], "choices": [] });
    let first_revision = proposal["bookRevision"].as_u64().unwrap_or(u64::MAX);
    drop(proposal);
    let ((applied, apply), peak_apply) = step(heap, base, || timed(|| call(&mut d, "scheduledTransaction.apply", &params)));
    let created = applied["created"]
        .as_array()
        .map_or(0, |c| c.iter().map(|i| i["transactions"].as_array().map_or(0, Vec::len)).sum());
    drop(applied);
    let after_apply = held(heap, base);
    let after = transactions(&mut d);
    let journal = call(&mut d, "book.info", &json!({}))["journal"].as_str().map(str::to_owned);
    let (journal_records, journal_transactions) = journal.map_or((0, 0), |p| journal_counts(Path::new(&p)));
    let second_revision = call(&mut d, "scheduledTransaction.sinceLastRun", &json!({}))["bookRevision"]
        .as_u64()
        .unwrap_or(u64::MAX);
    let ((_, save), peak_save) = step(heap, base, || timed(|| call(&mut d, "book.save", &json!({}))));
    call(&mut d, "book.close", &json!({ "save": false }));
    Run {
        to_create,
        created,
        transactions: (before, after),
        revisions: (first_revision, second_revision),
        journal_records,
        journal_transactions,
        at_rest,
        peak_propose,
        peak_apply,
        after_apply,
        peak_save,
        propose,
        apply,
        save,
    }
}

fn transactions(d: &mut red_ale_rpc::Dispatcher) -> u64 {
    call(d, "book.info", &json!({}))["counts"]["transactions"].as_u64().unwrap_or(0)
}

fn held(heap: Option<&dyn HeapCounter>, base: usize) -> usize {
    heap.map_or(0, |h| h.current().saturating_sub(base))
}

/// Run `f` and return its result with the peak it reached above `base`.
fn step<T>(heap: Option<&dyn HeapCounter>, base: usize, f: impl FnOnce() -> T) -> (T, usize) {
    heap.inspect(|h| h.reset_peak());
    let out = f();
    (out, heap.map_or(0, |h| h.peak().saturating_sub(base)))
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let out = f();
    (out, start.elapsed())
}

/// The journal's records, and the distinct transactions their lines name (`trans_guid`, the second field).
fn journal_counts(path: &Path) -> (usize, usize) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    let records = text.lines().filter(|l| *l == "===== START").count();
    let guids: BTreeSet<&str> = text
        .lines()
        .skip(2)
        .filter(|l| !l.starts_with("====="))
        .filter_map(|l| l.split('\t').nth(1))
        .collect();
    (records, guids.len())
}
