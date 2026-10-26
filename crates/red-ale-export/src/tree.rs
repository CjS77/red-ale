//! The account tree as CSV.
//!
//! Translated from `csv_tree_export` (`5.17:gnucash/import-export/csv-exp/csv-tree-export.cpp:51-101`),
//! GPL-2.0-or-later, Copyright (C) 2012 Robert Fewell.

use red_ale_core::ops::accounts::descendants_sorted;
use red_ale_core::{Account, Book};

use crate::CsvOptions;

/// The header line, in GnuCash's English msgids (`csv-tree-export.cpp:59-64`).
pub const HEADER: [&str; 12] = [
    "Type",
    "Full Account Name",
    "Account Name",
    "Account Code",
    "Description",
    "Account Color",
    "Notes",
    "Symbol",
    "Namespace",
    "Hidden",
    "Tax Info",
    "Placeholder",
];

/// The account tree as CSV text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeCsv {
    /// The text: the header, then one line per account, each ending in CRLF.
    pub csv: String,
    /// Account lines written, the header not counted.
    pub rows: usize,
}

/// Every account but the root, depth first with siblings in Display order, so parents come before children
/// (`gnc_account_get_descendants_sorted`, `csv-tree-export.cpp:72`). Siblings are compared bytewise where GnuCash
/// collates names by locale.
pub fn export_tree(book: &Book, options: &CsvOptions) -> TreeCsv {
    let accounts = descendants_sorted(book, book.root);
    let mut csv = String::new();
    options.add_line(&mut csv, &HEADER);
    accounts.iter().for_each(|a| options.add_line(&mut csv, &row(book, a)));
    TreeCsv { csv, rows: accounts.len() }
}

/// One account's fields (`csv-tree-export.cpp:82-95`): flags as `T` or `F` (`:74`), absent text as empty.
fn row(book: &Book, a: &Account) -> [String; 12] {
    let text = |s: Option<&str>| s.unwrap_or_default().to_owned();
    let flag = |on: bool| if on { "T" } else { "F" }.to_owned();
    let commodity = a.commodity.as_ref();
    [
        a.account_type.as_str().to_owned(),
        book.full_name(a.guid).unwrap_or_default(),
        a.name.clone(),
        text(a.code.as_deref()),
        text(a.description.as_deref()),
        text(a.stored_color()),
        text(a.notes()),
        commodity.map(|c| c.mnemonic.clone()).unwrap_or_default(),
        commodity.map(|c| c.namespace.clone()).unwrap_or_default(),
        flag(a.is_hidden()),
        flag(a.is_tax_related()),
        flag(a.is_placeholder()),
    ]
}
