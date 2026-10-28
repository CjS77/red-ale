//! Measure red-ale's memory on a synthetic book, for the table in `design/milestone3.md` ("Memory and scale") and the
//! figures in `design/milestone4.md` ("Done when").
//!
//! Generate the book in one process and measure in another, so the generator's memory stays out of the peak RSS:
//!
//! ```text
//! cargo run --release -p red-ale-testkit --example memory -- generate 100000 /tmp/synthetic-100k.gnucash
//! cargo run --release -p red-ale-testkit --example memory -- measure /tmp/synthetic-100k.gnucash
//! ```
//!
//! The same book as SQLite, written by red-ale's SQLite writer, is measured the same way (`measure` reads either
//! format); `rows` then saves one posted transaction in a copy and counts the rows that save changed, by table:
//!
//! ```text
//! cargo run --release -p red-ale-testkit --example memory -- generate-sqlite 100000 /tmp/synthetic-100k.sqlite.gnucash
//! cargo run --release -p red-ale-testkit --example memory -- measure /tmp/synthetic-100k.sqlite.gnucash
//! cargo run --release -p red-ale-testkit --example memory -- rows /tmp/synthetic-100k.sqlite.gnucash
//! ```
//!
//! A Since Last Run creating 5,000 transactions: add 10 daily scheduled transactions of 500 occurrences to a copy of
//! the book, then measure one `scheduledTransaction.sinceLastRun` and `scheduledTransaction.apply` on it:
//!
//! ```text
//! cargo run --release -p red-ale-testkit --example memory -- schedule 10 500 /tmp/synthetic-100k.gnucash /tmp/slr.gnucash
//! cargo run --release -p red-ale-testkit --example memory -- since-last-run /tmp/slr.gnucash
//! ```
//!
//! Heap figures come from a counting allocator, so they repeat exactly; for an SQLite book they leave out SQLite's own
//! allocations (its page cache and statements), which the peak RSS includes. Peak RSS is the kernel's `VmHWM` for the
//! whole process, and varies a little.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;

use peak_alloc::PeakAlloc;
use red_ale_testkit::memory::{HeapCounter, book_alone, measure, megabytes, one_transaction_save};
use red_ale_testkit::since_last_run::{add_daily_schedules, run};
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["generate", count, path] => generate(count.parse().expect("a transaction count"), Path::new(path)),
        ["generate-sqlite", count, path] => write_synthetic_sqlite(count.parse().expect("a transaction count"), Path::new(path)),
        ["measure", path] => report(Path::new(path)),
        ["rows", path] => rows(Path::new(path)),
        ["schedule", schedules, occurrences, from, to] => {
            std::fs::copy(from, to).unwrap_or_else(|e| panic!("copying {from}: {e}"));
            add_daily_schedules(
                Path::new(to),
                schedules.parse().expect("a count"),
                occurrences.parse().expect("a count"),
            );
        }
        ["since-last-run", path] => since_last_run(Path::new(path)),
        _ => panic!(
            "usage: memory generate <transactions> <path> | generate-sqlite <transactions> <path> | measure <path> | rows <path> \
             | schedule <schedules> <occurrences> <from> <to> | since-last-run <path>"
        ),
    }
}

fn generate(transactions: usize, path: &Path) {
    std::fs::write(path, synthetic_book_text(transactions)).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
}

fn report(path: &Path) {
    let m = measure(path, &Heap);
    let rss = peak_rss();
    let book = book_alone(path, &Heap);
    let ratio = |bytes: usize| {
        let h = m.per_file_size(bytes);
        format!("{}.{:02}x", h / 100, h % 100)
    };
    println!("file           {}", megabytes(m.file));
    println!("book alone     {} ({})", megabytes(book), ratio(book));
    println!("peak at open   {} ({})", megabytes(m.peak_open), ratio(m.peak_open));
    println!("open, at rest  {} ({})", megabytes(m.at_rest), ratio(m.at_rest));
    println!("peak at post   {} ({})", megabytes(m.peak_post), ratio(m.peak_post));
    println!("peak at save   {} ({})", megabytes(m.peak_save), ratio(m.peak_save));
    println!("after save     {} ({})", megabytes(m.after_save), ratio(m.after_save));
    println!("peak RSS       {}", rss.map_or_else(|| "unknown".to_owned(), megabytes));
}

fn rows(path: &Path) {
    let (posted, changed) = one_transaction_save(path);
    println!("posted {posted}");
    changed.iter().for_each(|(table, rows)| {
        rows.new.iter().for_each(|r| println!("{table:<16} + {r:?}"));
        rows.gone.iter().for_each(|r| println!("{table:<16} - {r:?}"));
    });
}

fn since_last_run(path: &Path) {
    let r = run(path, Some(&Heap));
    let rss = peak_rss();
    println!("to create      {}", r.to_create);
    println!("created        {}", r.created);
    println!("transactions   {} => {}", r.transactions.0, r.transactions.1);
    println!("book revision  {} => {}", r.revisions.0, r.revisions.1);
    println!(
        "journal        {} records naming {} transactions",
        r.journal_records, r.journal_transactions
    );
    println!("open, at rest  {}", megabytes(r.at_rest));
    println!("peak, propose  {} in {:.2?}", megabytes(r.peak_propose), r.propose);
    println!("peak, apply    {} in {:.2?}", megabytes(r.peak_apply), r.apply);
    println!("after apply    {}", megabytes(r.after_apply));
    println!("peak, save     {} in {:.2?}", megabytes(r.peak_save), r.save);
    println!("peak RSS       {}", rss.map_or_else(|| "unknown".to_owned(), megabytes));
}

/// The process's peak resident set (`VmHWM`), on Linux.
fn peak_rss() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: usize = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}
