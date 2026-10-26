//! red-ale's memory stays within its budgets (`design/milestone3.md`, "Memory and scale").
//!
//! The budgets are set on the 100,000-transaction synthetic book, whose plain file is 155.6 MB, from GnuCash 5.17's
//! 188 MB: at rest within twice that (376 MB, 2.41 times the file), and the peaks within three times (564 MB, 3.62
//! times the file). Here they are held on a smaller synthetic book of the same shape, scaled by the size of its file,
//! with the heap counted by the allocator so every run gives the same figures. This binary holds one test, so nothing
//! else allocates while it measures.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use peak_alloc::PeakAlloc;
use red_ale_testkit::TempBook;
use red_ale_testkit::memory::{HeapCounter, measure};
use red_ale_testkit::synthetic::synthetic_book_text;

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

/// Large enough that the 2,000 accounts are a small part of the book, small enough for every `cargo test`.
const TRANSACTIONS: usize = 5_000;

/// Budgets in hundredths of the plain file's size.
const AT_REST: usize = 241;
const PEAK: usize = 362;

#[test]
fn memory_stays_within_budget() {
    let book = TempBook::new_path();
    std::fs::write(&book.path, synthetic_book_text(TRANSACTIONS)).unwrap();
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
        .map(|(name, bytes, budget)| (name, m.per_file_size(*bytes), budget))
        .filter(|(_, used, budget)| used > budget)
        .map(|(name, used, budget)| format!("{name}: {used} hundredths of the file, budget {budget}"))
        .collect();
    assert!(over.is_empty(), "over budget on a {}-byte file:\n{}", m.file, over.join("\n"));
}
