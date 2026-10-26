//! Measure red-ale's memory on a synthetic book, for the table in `design/milestone3.md` ("Memory and scale").
//!
//! Generate the book in one process and measure in another, so the generator's memory stays out of the peak RSS:
//!
//! ```text
//! cargo run --release -p red-ale-testkit --example memory -- generate 100000 /tmp/synthetic-100k.gnucash
//! cargo run --release -p red-ale-testkit --example memory -- measure /tmp/synthetic-100k.gnucash
//! ```
//!
//! Heap figures come from a counting allocator, so they repeat exactly; peak RSS is the kernel's `VmHWM` for the
//! whole process, and varies a little.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;

use peak_alloc::PeakAlloc;
use red_ale_testkit::memory::{HeapCounter, book_alone, measure, megabytes};
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["generate", count, path] => generate(count.parse().expect("a transaction count"), Path::new(path)),
        ["measure", path] => report(Path::new(path)),
        _ => panic!("usage: memory generate <transactions> <path> | memory measure <path>"),
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
    println!("file (plain)   {}", megabytes(m.file));
    println!("book alone     {} ({})", megabytes(book), ratio(book));
    println!("peak at open   {} ({})", megabytes(m.peak_open), ratio(m.peak_open));
    println!("open, at rest  {} ({})", megabytes(m.at_rest), ratio(m.at_rest));
    println!("peak at post   {} ({})", megabytes(m.peak_post), ratio(m.peak_post));
    println!("peak at save   {} ({})", megabytes(m.peak_save), ratio(m.peak_save));
    println!("after save     {} ({})", megabytes(m.after_save), ratio(m.after_save));
    println!("peak RSS       {}", rss.map_or_else(|| "unknown".to_owned(), megabytes));
}

/// The process's peak resident set (`VmHWM`), on Linux.
fn peak_rss() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: usize = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}
