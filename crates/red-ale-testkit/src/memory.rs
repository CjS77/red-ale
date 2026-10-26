//! Measuring red-ale's heap through `book.open`, `transaction.post` and `book.save`.
//!
//! The figures are heap bytes as a counting global allocator sees them, so they are the same on every run and every
//! machine. The binary that measures declares the allocator (this crate forbids the `unsafe` one needs) and hands it
//! in as a [`HeapCounter`]. See `design/milestone3.md`, "Memory and scale".

use std::path::Path;

use serde_json::json;

use crate::{TempBook, call, dispatcher};

/// A global allocator that counts the bytes it has handed out.
pub trait HeapCounter {
    /// Bytes allocated now.
    fn current(&self) -> usize;
    /// The most bytes allocated at once since the last [`HeapCounter::reset_peak`].
    fn peak(&self) -> usize;
    /// Start a new peak from what is allocated now.
    fn reset_peak(&self);
}

/// Heap used by an open session, in bytes, above what was allocated before `book.open`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measurement {
    /// Size of the book's plain (uncompressed) file.
    pub file: usize,
    /// The most held at once while `book.open` ran.
    pub peak_open: usize,
    /// Held once `book.open` returned: the session at rest.
    pub at_rest: usize,
    /// The most held at once while one `transaction.post` ran.
    pub peak_post: usize,
    /// The most held at once while `book.save` ran.
    pub peak_save: usize,
    /// Held once `book.save` returned.
    pub after_save: usize,
}

impl Measurement {
    /// A figure as a multiple of the file's size, in hundredths, for budgets that scale with the book.
    pub fn per_file_size(&self, bytes: usize) -> usize {
        bytes.saturating_mul(100) / self.file.max(1)
    }
}

/// Open a copy of the book at `path` in a fresh dispatcher, post one transaction and save, measuring the heap at each
/// step. The book must be a synthetic one ([`crate::synthetic`]), whose account names the posting uses.
pub fn measure(path: &Path, heap: &dyn HeapCounter) -> Measurement {
    let book = TempBook::new_path();
    std::fs::copy(path, &book.path).unwrap_or_else(|e| panic!("copying {}: {e}", path.display()));
    let file = usize::try_from(std::fs::metadata(&book.path).map_or(0, |m| m.len())).unwrap_or(usize::MAX);
    warm_up();
    let mut d = dispatcher();
    let base = heap.current();
    let peak_open = step(heap, base, || call(&mut d, "book.open", &json!({ "path": book.path_str() })));
    let at_rest = heap.current().saturating_sub(base);
    let peak_post = step(heap, base, || call(&mut d, "transaction.post", &posting()));
    let peak_save = step(heap, base, || call(&mut d, "book.save", &json!({})));
    let after_save = heap.current().saturating_sub(base);
    call(&mut d, "book.close", &json!({ "save": false }));
    Measurement { file, peak_open, at_rest, peak_post, peak_save, after_save }
}

/// Heap held by the Book alone, read from the file at `path` with its Residue dropped.
pub fn book_alone(path: &Path, heap: &dyn HeapCounter) -> usize {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    let base = heap.current();
    let (book, residue) = red_ale_xml::read_book(&bytes).expect("the book reads");
    drop(residue);
    let held = heap.current().saturating_sub(base);
    drop(book);
    held
}

/// Run `f` and return the peak it reached above `base`.
fn step<T>(heap: &dyn HeapCounter, base: usize, f: impl FnOnce() -> T) -> usize {
    heap.reset_peak();
    drop(f());
    heap.peak().saturating_sub(base)
}

/// Open, post to and save a small book first, so allocations made once per process (tables, lazily built schemas)
/// are not counted against the book being measured.
fn warm_up() {
    let book = TempBook::new_path();
    std::fs::write(&book.path, crate::synthetic::synthetic_book_text(10)).expect("writing the warm-up book");
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    call(&mut d, "transaction.post", &posting());
    call(&mut d, "book.save", &json!({}));
    call(&mut d, "book.close", &json!({ "save": false }));
}

fn posting() -> serde_json::Value {
    json!({
        "datePosted": "2024-06-20", "description": "Measured",
        "splits": [
            { "account": "Assets:Assets 0000", "value": "-12.34" },
            { "account": "Expenses:Expenses 0000", "value": "12.34" }
        ]
    })
}

/// Format bytes as MB (10^6 bytes), as `design/roadmap.md` reports them.
pub fn megabytes(bytes: usize) -> String {
    let tenths = bytes.saturating_add(50_000) / 100_000;
    format!("{}.{} MB", tenths / 10, tenths % 10)
}
