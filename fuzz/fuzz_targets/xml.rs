//! The streaming XML book reader, on plain or gzipped input, and the save's read-back against an empty book.
//! Seed the corpus with `fixtures/` books (`design/testing.md`, "Fuzzing").

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_core::{Book, FixedEnv, Time64};
use red_ale_xml::{read_back, read_book_from};

fuzz_target!(|data: &[u8]| {
    let _ = read_book_from(data);
    let empty = Book::new_empty(&mut FixedEnv::new(Time64(0), 1));
    let _ = read_back(data, &empty);
});
