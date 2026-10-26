//! CSV transaction import: file text to draft transactions, as GnuCash's CSV importer reads it.
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/` (GPL-2.0-or-later; copyright lines and line citations are
//! in each module). The GUI (`assistant-csv-trans-import.cpp`) is not translated: what the user chooses there is a
//! [`TxLayout`] (saved as a preset) or a parameter (`skip_errors`, today's date, the locale).
//!
//! | Module | GnuCash source |
//! |--------|----------------|
//! | [`tokenizer`] | `gnc-tokenizer.cpp` |
//! | `tokenizer_csv` | `gnc-tokenizer-csv.cpp` |
//! | `tokenizer_fw` | `gnc-tokenizer-fw.cpp` |
//! | [`settings`] | `gnc-imp-settings-csv.cpp`, `gnc-imp-settings-csv-tx.cpp` |
//! | [`date`] | `GncDate` string parsing, `libgnucash/engine/gnc-datetime.cpp` |
//! | [`monetary`] | `parse_monetary`, and `xaccParseAmount*` from `libgnucash/app-utils/gnc-ui-util.cpp` |
//! | [`props_tx`] | `gnc-imp-props-tx.cpp` |
//! | [`import_tx`] | `gnc-import-tx.cpp` |
//!
//! The tokenizer and the shared settings ([`CsvLayout`], [`FileFormat`], [`DateFormat`], [`CurrencyFormat`],
//! [`parse_monetary`], [`parse_date`]) are what the price importer (`gnc-import-price.cpp`) uses too.
//!
//! The layout is the JSON object `design/api/import.md` specifies for `import.csvLayouts.<name>` in the
//! Configuration; [`TxLayout::presets`] are GnuCash's built-in ones.
//!
//! # Deviations from GnuCash
//!
//! Only those `design/api/import.md` lists ("Deviations from GnuCash"), each documented where it is made:
//!
//! - a split GnuCash would drop silently (reconciled with no reconcile date) is a line error instead
//!   ([`TxImport::parse`]);
//! - every draft's void reason applies, the file's last included ([`DraftTransaction::void_reason`]);
//! - an amount whose digits overflow 64 bits is an error, where GnuCash saturates or wraps ([`monetary`]);
//! - an unknown column type name in a layout is refused, where GnuCash reads it as `None` ([`ColumnType`]).
//!
//! # Use
//!
//! ```
//! # use red_ale_core::Date;
//! # use red_ale_import::csv::{ColumnType, ParseContext, TxImport, TxLayout, tokenize};
//! let mut layout = TxLayout::default();
//! layout.csv.skip_start_lines = 1;
//! layout.columns = vec![ColumnType::Date, ColumnType::Description, ColumnType::Account, ColumnType::Amount];
//! let rows = tokenize("Date,Payee,Account,Amount\n2024-03-01,Rent,Expenses:Rent,950.00\n", &layout.csv).unwrap();
//! let import = TxImport::parse(rows, &layout, &ParseContext::new(Date::from_ymd(2024, 3, 15).unwrap()));
//! let drafts = import.create_drafts(false).unwrap();
//! assert_eq!(drafts[0].description.as_deref(), Some("Rent"));
//! ```
//!
//! # Where GnuCash goes next, for the import pipeline
//!
//! GnuCash parses with the Book at hand; this module stops before it. The pipeline continues with:
//!
//! 1. **Resolve accounts.** For each line's Account and Transfer Account text: the `csv-account-map` import map
//!    (`gnc_account_imap_find_any`), then the full name (`gnc_account_lookup_by_full_name`), recording
//!    "Account value can't be mapped back to an account." (or "Transfer account value …") on failure with
//!    [`PreSplit::add_error`] (`gnc-imp-props-tx.cpp:448-469`). [`TxImport::account_names`] lists the texts the
//!    assistant asks the user to match; a match the user makes is stored in the `csv-account-map` map
//!    (`assistant-csv-trans-import.cpp:1914-1919`). [`SplitAccount::Base`] is the layout's [`BaseAccount`], by GUID
//!    then full name.
//! 2. **Resolve the Transaction Commodity** text with `parse_commodity`'s order (`gnc-imp-props-tx.cpp:188-226`),
//!    recording "Value can't be parsed into a valid commodity." with [`PreTrans::add_error`]. GnuCash compares the
//!    resolved commodity when grouping multi-split lines; this module compares the text.
//! 3. **Check multi-currency.** With the accounts' commodities, GnuCash's `is_multi_currency` and the extra checks
//!    of `verify_column_selections` (`gnc-import-tx.cpp:542-556`) and `GncPreSplit::verify_essentials`
//!    (`gnc-imp-props-tx.cpp:654-670`); a split failing the latter is not created.
//! 4. **Verify and draft.** [`TxImport::verify`] then [`TxImport::create_drafts`].
//! 5. **Build the transactions** as `GncPreTrans::create_trans` and `trans_add_split` do: the transaction currency
//!    (see [`DraftTransaction::commodity`]), posted at neutral time with the `date-posted` slot, num with
//!    `xaccTransSetNum` whatever the book's num-field source;
//!    each split's value with [`DraftSplit::split_value`]; in two-split mode the transfer split with
//!    [`DraftSplit::transfer_amount`] when the transfer account resolves, else the transfer columns and price go to
//!    the matcher as `GNCImportLastSplitInfo` (`assistant-csv-trans-import.cpp:2192-2210`); and `xaccTransVoid`
//!    for every draft with a [`DraftTransaction::void_reason`].
//! 6. **Match**: each transaction goes to the generic import matcher (`gnc_gen_trans_list_add_trans_with_split_data`).

pub mod date;
pub mod import_tx;
pub mod monetary;
pub mod props_tx;
pub mod settings;
pub mod tokenizer;
mod tokenizer_csv;
mod tokenizer_fw;
mod unicode;

pub use date::{DateContext, DateError, DateFormat, parse_date};
pub use import_tx::{CreateError, DraftSplit, DraftTransaction, DraftTransfer, LineState, NearestPrice, ParsedLine, TxImport};
pub use monetary::{CurrencyFormat, MonetaryError, MonetaryLocale, parse_monetary};
pub use props_tx::{ColumnType, ErrMap, ParseContext, PreSplit, PreTrans, SplitAccount};
pub use settings::{BaseAccount, CsvLayout, TxLayout};
pub use tokenizer::{FileFormat, Row, TokenizeError, tokenize};

#[cfg(test)]
mod tests;
