//! red-ale's memory on an SQLite book stays within milestone 3's budgets (`design/milestone3.md`, "Memory and scale";
//! `design/sql-fidelity.md` §9, "Scale").
//!
//! The budgets are set on the 100,000-transaction synthetic book as multiples of its plain XML file, which an SQLite
//! book of the same Book shares: at rest within 2.41 times, and the peaks within 3.62 times. Here they are held, as in
//! `memory_budget.rs`, on the 5,000-transaction synthetic book, written as SQLite by red-ale's own writer, with the
//! limits in hundredths of the same book's plain XML file. The counting allocator sees red-ale's heap, not SQLite's own
//! page cache and statements (the example `memory` reports the peak resident set, which has both). This binary holds
//! one test, so nothing else allocates while it measures.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use peak_alloc::PeakAlloc;
use red_ale_testkit::TempBook;
use red_ale_testkit::memory::{HeapCounter, measure};
use red_ale_testkit::synthetic::{synthetic_book_text, write_synthetic_sqlite};

#[global_allocator]
static HEAP: PeakAlloc = PeakAlloc;

struct Heap;

impl HeapCounter for Heap {
    fn current(&self) -> usize {
        HEAP.current_usage()
    }

    fn peak(&self) -> usize {
        HEAP.peak_usage()
    }

    fn reset_peak(&self) {
        HEAP.reset_peak_usage();
    }
}

/// As in `memory_budget.rs`.
const TRANSACTIONS: usize = 5_000;

/// Limits in hundredths of the plain XML file's size, as `memory_budget.rs` sets them: the budgets are 241 at rest
/// and 362 for the peaks; the SQLite book reaches 112 at rest and 140 at the highest peak (the open).
const AT_REST: usize = 130;
const PEAK: usize = 200;

#[test]
fn memory_on_an_sqlite_book_stays_within_budget() {
    let xml = synthetic_book_text(TRANSACTIONS).len();
    let book = TempBook::new_path();
    write_synthetic_sqlite(TRANSACTIONS, &book.path);
    let m = measure(&book.path, &Heap);
    let figures = [
        ("at rest after open", m.at_rest, AT_REST),
        ("at rest after save", m.after_save, AT_REST),
        ("peak at post", m.peak_post, PEAK),
        ("peak at open", m.peak_open, PEAK),
        ("peak at save", m.peak_save, PEAK),
    ];
    let over: Vec<String> = figures
        .iter()
        .map(|(name, bytes, budget)| (name, bytes.saturating_mul(100) / xml, budget))
        .filter(|(_, used, budget)| used > budget)
        .map(|(name, used, budget)| format!("{name}: {used} hundredths of the XML file, budget {budget}"))
        .collect();
    assert!(over.is_empty(), "over budget for a {xml}-byte XML file ({}-byte SQLite):\n{}", m.file, over.join("\n"));
}
