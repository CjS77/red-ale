//! CSV transaction import after tokenizing: `TxImport::parse`, `verify`, `account_names` and `create_drafts`. The
//! first eight bytes pick the column types; the ninth the multi-split and skip flags; the rest is the file.

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_core::Date;
use red_ale_import::csv::{ColumnType, ParseContext, TxImport, TxLayout, tokenize};

fuzz_target!(|data: &[u8]| {
    if data.len() < 9 {
        return;
    }
    let (head, rest) = data.split_at(9);
    let columns = head[..8].iter().map(|b| ColumnType::ALL[usize::from(*b) % ColumnType::ALL.len()]).collect();
    let layout = TxLayout { multi_split: head[8] & 1 != 0, columns, ..TxLayout::default() };
    let skip = head[8] & 2 != 0;
    let Some(today) = Date::from_ymd(2026, 10, 26) else { return };
    let Ok(rows) = tokenize(&String::from_utf8_lossy(rest), &layout.csv) else { return };
    let import = TxImport::parse(rows, &layout, &ParseContext::new(today));
    let _ = import.verify(true, skip);
    let _ = import.account_names(skip);
    let _ = import.create_drafts(skip);
});
