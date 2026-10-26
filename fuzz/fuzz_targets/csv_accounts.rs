//! CSV account import: GnuCash's account-export reader with each separator, then the proposal against an empty
//! book.

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_core::{Book, FixedEnv, Time64};
use red_ale_import::csv::accounts::propose;
use red_ale_import::csv::accounts::read::{Separator, read};

fuzz_target!(|data: &[u8]| {
    let Some((first, rest)) = data.split_first() else { return };
    let text = String::from_utf8_lossy(rest);
    let separator = [Separator::Comma, Separator::Semicolon, Separator::Colon][usize::from(first % 3)];
    let header_rows = (first & 4 != 0).then_some(usize::from(first >> 4));
    let file = read(&text, separator, header_rows, 0);
    let book = Book::new_empty(&mut FixedEnv::new(Time64(1_711_000_000), 1));
    let _ = propose(&book, &file.rows, 0);
});
