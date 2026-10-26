//! CSV import layouts: everything GnuCash saves in a CSV import preset, and its built-in presets.
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/gnc-imp-settings-csv.cpp` (the settings shared by the
//! transaction and price importers) and `gnc-imp-settings-csv-tx.cpp` (the transaction importer's own).
//! Copyright (C) 2014 Robert Fewell; (c) 2016 Geert Janssens. GPL-2.0-or-later; this translation is
//! GPL-3.0-or-later.
//!
//! GnuCash keeps presets in its state file (`gnc_state_get_current`), one key-file group per preset. red-ale keeps
//! them as JSON in its Configuration, so the types here derive serde. Fields missing from the JSON take GnuCash's
//! defaults, as `CsvImportSettings::load` falls back on them for missing keys (`gnc-imp-settings-csv.cpp:77-192`).
//! `m_load_error` is load-time state, not a setting, and has no field.

use serde::{Deserialize, Serialize};

use super::date::DateFormat;
use super::monetary::CurrencyFormat;
use super::props_tx::ColumnType;
use super::tokenizer::FileFormat;

/// The name of the empty built-in preset (`gnc-imp-settings-csv.cpp:46`).
pub const NO_SETTINGS: &str = "No Settings";
/// The built-in preset matching GnuCash's own CSV transaction export (`gnc-imp-settings-csv.cpp:47`).
pub const GNUCASH_EXPORT: &str = "GnuCash Export Format";
/// The built-in preset matching the export of GnuCash 4.x and older (`gnc-imp-settings-csv.cpp:48`).
pub const GNUCASH_EXPORT_4: &str = "GnuCash Export Format (4.x and older)";

/// Whether a preset name belongs to a built-in preset, which is never saved or deleted
/// (`preset_is_reserved_name`, `gnc-imp-settings-csv.cpp:94-100`). As in GnuCash, the 4.x preset's name is not
/// reserved.
pub fn is_reserved_name(name: &str) -> bool {
    name == NO_SETTINGS || name == GNUCASH_EXPORT
}

/// The settings every CSV importer shares: GnuCash's `CsvImportSettings` (`gnc-imp-settings-csv.hpp`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CsvLayout {
    /// The preset's name (`Name`).
    pub name: String,
    /// Delimited or fixed-width (`CsvFormat`, saved by GnuCash as a boolean, true for CSV).
    pub file_format: FileFormat,
    /// The file's encoding, a name `encoding_rs` knows (`Encoding`, default `UTF-8`).
    pub encoding: String,
    /// How dates are written (`DateFormat`, saved by GnuCash as an index into `GncDate::c_formats`).
    pub date_format: DateFormat,
    /// How amounts are written (`CurrencyFormat`, saved by GnuCash as an index).
    pub currency_format: CurrencyFormat,
    /// Lines to skip at the start, such as a header (`SkipStartLines`).
    pub skip_start_lines: u32,
    /// Lines to skip at the end (`SkipEndLines`).
    pub skip_end_lines: u32,
    /// Skip every second line after the start lines (`SkipAltLines`).
    pub skip_alt_lines: bool,
    /// The separator characters, any of which ends a field (`Separators`, default `,`). The assistant builds it from
    /// its check boxes in the order space, tab, comma, colon, semicolon, hyphen, then the custom entry
    /// (`assistant-csv-trans-import.cpp:1008-1022`).
    pub separators: String,
    /// Whether `\` escapes in fields (`EnableEscape`, default true).
    pub enable_escape: bool,
    /// Fixed-width column widths in characters (`ColumnWidths`). GnuCash drops non-positive widths on load
    /// (`gnc-imp-settings-csv.cpp:182-186`); `u32` makes negative ones impossible, and [`CsvLayout::sanitized`] drops
    /// zeros.
    pub column_widths: Vec<u32>,
}

impl Default for CsvLayout {
    /// `CsvImportSettings::CsvImportSettings` (`gnc-imp-settings-csv.hpp`).
    fn default() -> Self {
        Self {
            name: String::new(),
            file_format: FileFormat::Csv,
            encoding: "UTF-8".to_owned(),
            date_format: DateFormat::default(),
            currency_format: CurrencyFormat::default(),
            skip_start_lines: 0,
            skip_end_lines: 0,
            skip_alt_lines: false,
            separators: ",".to_owned(),
            enable_escape: true,
            column_widths: Vec::new(),
        }
    }
}

impl CsvLayout {
    /// The layout as GnuCash would load it back: an empty encoding is `UTF-8`, empty separators keep the default
    /// `,`, and zero widths are dropped (`gnc-imp-settings-csv.cpp:147-186`).
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        if self.encoding.is_empty() {
            "UTF-8".clone_into(&mut self.encoding);
        }
        if self.separators.is_empty() {
            ",".clone_into(&mut self.separators);
        }
        self.column_widths.retain(|&w| w > 0);
        self
    }
}

/// The base account of a two-split import, as GnuCash saves it: by GUID (`BaseAccountGuid`, since 4.5) and by full
/// name (`BaseAccount`). GnuCash looks the GUID up first and falls back on the name
/// (`gnc-imp-settings-csv-tx.cpp:209-247`); resolving it is the import pipeline's job.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BaseAccount {
    /// The account's GUID, 32 hex digits.
    pub guid: Option<String>,
    /// The account's full name, with the book's separator.
    pub full_name: Option<String>,
}

/// A CSV transaction import layout: GnuCash's `CsvTransImpSettings` (`gnc-imp-settings-csv-tx.hpp`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TxLayout {
    /// The settings shared with the other CSV importers.
    #[serde(flatten)]
    pub csv: CsvLayout,
    /// Several lines make one transaction, one split per line (`MultiSplit`).
    pub multi_split: bool,
    /// The account every line's first split goes to, in two-split mode only.
    pub base_account: Option<BaseAccount>,
    /// The type of each column, in order (`ColumnTypes`).
    pub column_types: Vec<ColumnType>,
}

impl TxLayout {
    /// GnuCash's built-in presets, in the order the assistant lists them (`get_import_presets_trans`,
    /// `gnc-imp-settings-csv-tx.cpp:171-174`).
    pub fn presets() -> Vec<Self> {
        vec![Self::no_settings(), Self::gnucash_export(), Self::gnucash_export_4()]
    }

    /// "No Settings": the defaults (`create_int_no_preset`, `gnc-imp-settings-csv-tx.cpp:59-65`).
    pub fn no_settings() -> Self {
        Self { csv: CsvLayout { name: NO_SETTINGS.to_owned(), ..CsvLayout::default() }, ..Self::default() }
    }

    /// The layout of GnuCash's own transaction export (`create_int_gnc_exp_preset`, `gnc-imp-settings-csv-tx.cpp:67-101`).
    pub fn gnucash_export() -> Self {
        use ColumnType::{
            Account, Action, Amount, Commodity, Date, Description, Memo, None, Notes, Num, Price, RecDate, RecState,
            UniqueId, Value, VoidReason,
        };
        Self::export_preset(GNUCASH_EXPORT, vec![
            Date, UniqueId, Num, Description, Notes, Commodity, VoidReason, Action, Memo, Account, None, None, Amount,
            None, Value, RecState, RecDate, Price,
        ])
    }

    /// The layout of the export of GnuCash 4.x and older (`create_int_gnc_exp_4_preset`,
    /// `gnc-imp-settings-csv-tx.cpp:103-135`).
    pub fn gnucash_export_4() -> Self {
        use ColumnType::{
            Account, Action, Amount, Commodity, Date, Description, Memo, None, Notes, Num, Price, RecDate, RecState,
            UniqueId, VoidReason,
        };
        Self::export_preset(GNUCASH_EXPORT_4, vec![
            Date, UniqueId, Num, Description, Notes, Commodity, VoidReason, Action, Memo, Account, None, None, Amount,
            RecState, RecDate, Price,
        ])
    }

    fn export_preset(name: &str, column_types: Vec<ColumnType>) -> Self {
        Self {
            csv: CsvLayout { name: name.to_owned(), skip_start_lines: 1, ..CsvLayout::default() },
            multi_split: true,
            base_account: None,
            column_types,
        }
    }

    /// The layout as GnuCash would load it back: the shared settings sanitized, and column types not allowed in the
    /// layout's mode replaced by `None` (`gnc-imp-settings-csv-tx.cpp:263-279`).
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.csv = self.csv.sanitized();
        let multi_split = self.multi_split;
        self.column_types.iter_mut().for_each(|t| *t = t.sanitize(multi_split));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_gnucash() {
        let layout = TxLayout::default();
        assert_eq!(layout.csv.separators, ",");
        assert_eq!(layout.csv.encoding, "UTF-8");
        assert!(layout.csv.enable_escape && !layout.multi_split && layout.column_types.is_empty());
        assert_eq!(layout.csv.file_format, FileFormat::Csv);
    }

    #[test]
    fn presets() {
        let presets = TxLayout::presets();
        let names: Vec<_> = presets.iter().map(|p| p.csv.name.as_str()).collect();
        assert_eq!(names, [NO_SETTINGS, GNUCASH_EXPORT, GNUCASH_EXPORT_4]);
        assert_eq!(presets[1].column_types.len(), 18);
        assert_eq!(presets[2].column_types.len(), 16);
        assert!(presets[1].multi_split && presets[1].csv.skip_start_lines == 1);
        assert!(is_reserved_name(GNUCASH_EXPORT) && !is_reserved_name(GNUCASH_EXPORT_4));
    }

    #[test]
    fn json_round_trip_and_defaults() {
        let layout = TxLayout::gnucash_export();
        let json = serde_json::to_string(&layout).unwrap();
        assert_eq!(serde_json::from_str::<TxLayout>(&json).unwrap(), layout);
        let partial: TxLayout = serde_json::from_str(r#"{"name":"Bank","dateFormat":"d-m-y","columnTypes":["date","amountNeg"]}"#).unwrap();
        assert_eq!(partial.csv.separators, ",");
        assert_eq!(partial.csv.date_format, DateFormat::Dmy);
        assert_eq!(partial.column_types, [ColumnType::Date, ColumnType::AmountNeg]);
    }

    #[test]
    fn sanitized_drops_blacklisted_columns() {
        let layout = TxLayout { multi_split: true, column_types: vec![ColumnType::TransferAccount], ..TxLayout::default() };
        assert_eq!(layout.sanitized().column_types, [ColumnType::None]);
        let layout = TxLayout { column_types: vec![ColumnType::UniqueId], ..TxLayout::default() };
        assert_eq!(layout.sanitized().column_types, [ColumnType::None]);
    }
}
