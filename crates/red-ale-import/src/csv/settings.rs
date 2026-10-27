//! CSV import layouts: everything GnuCash saves in a CSV import preset, and its built-in presets.
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/gnc-imp-settings-csv.cpp` (the settings shared by the
//! transaction and price importers), `gnc-imp-settings-csv-tx.cpp` (the transaction importer's own) and
//! `gnc-imp-settings-csv-price.cpp` (the price importer's, [`PriceLayout`]).
//! Copyright (C) 2014 Robert Fewell; (c) 2016 Geert Janssens. GPL-2.0-or-later; this translation is
//! GPL-3.0-or-later.
//!
//! GnuCash keeps presets in its per-book state file, one key-file group per preset, named
//! `Import csv,transaction - <name>`. red-ale keeps them as JSON in its Configuration under
//! `import.csvLayouts.<name>` (`design/api/import.md`, "CSV transactions"), with the field names that document
//! gives: every key of GnuCash's group, the date and currency formats by name rather than index, and the column
//! types by GnuCash's saved names. The name is the Configuration key, so it is not a field. Fields missing from the
//! JSON take GnuCash's defaults, as `CsvImportSettings::load` falls back on them for missing keys
//! (`gnc-imp-settings-csv.cpp:77-192`).

use serde::{Deserialize, Serialize};

use super::date::DateFormat;
use super::monetary::CurrencyFormat;
use super::prices::PriceColumn;
use super::props_tx::ColumnType;
use super::tokenizer::FileFormat;

/// The name of the empty built-in preset (`gnc-imp-settings-csv.cpp:46`).
pub const NO_SETTINGS: &str = "No Settings";
/// The built-in preset matching GnuCash's own CSV transaction export (`gnc-imp-settings-csv.cpp:47`).
pub const GNUCASH_EXPORT: &str = "GnuCash Export Format";
/// The built-in preset matching the export of GnuCash 4.x and older (`gnc-imp-settings-csv.cpp:48`).
pub const GNUCASH_EXPORT_4: &str = "GnuCash Export Format (4.x and older)";

/// Whether GnuCash refuses to save or delete a preset of this name (`preset_is_reserved_name`,
/// `gnc-imp-settings-csv.cpp:94-100`). As in GnuCash, the 4.x preset's name is not reserved; red-ale's
/// Configuration may still treat every built-in layout as read only.
pub fn is_reserved_name(name: &str) -> bool {
    name == NO_SETTINGS || name == GNUCASH_EXPORT
}

/// The settings every CSV importer shares: GnuCash's `CsvImportSettings` (`gnc-imp-settings-csv.hpp`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CsvLayout {
    /// Delimited or fixed width (GnuCash's `CsvFormat`, a boolean, true for CSV).
    pub format: FileFormat,
    /// The separator characters, any of which ends a field (`Separators`, default `,`). The assistant builds it from
    /// its check boxes in the order space, tab, comma, colon, semicolon, hyphen, then the custom entry
    /// (`assistant-csv-trans-import.cpp:1008-1022`).
    pub separators: String,
    /// Whether `\` escapes in fields (`EnableEscape`, default true).
    pub escape: bool,
    /// Fixed-width column widths in characters (`ColumnWidths`). GnuCash drops non-positive widths on load
    /// (`gnc-imp-settings-csv.cpp:182-186`); `u32` rules out negative ones and [`CsvLayout::sanitized`] drops zeros.
    pub column_widths: Vec<u32>,
    /// Lines to skip at the start, such as a header (`SkipStartLines`).
    pub skip_start_lines: u32,
    /// Lines to skip at the end (`SkipEndLines`).
    pub skip_end_lines: u32,
    /// Skip every second line after the start lines (`SkipAltLines`).
    pub skip_alt_lines: bool,
    /// How dates are written (`DateFormat`, an index into `GncDate::c_formats` in GnuCash).
    pub date_format: DateFormat,
    /// How amounts are written (`CurrencyFormat`, an index in GnuCash).
    pub currency_format: CurrencyFormat,
    /// The file's encoding, a name `encoding_rs` knows (`Encoding`, default `UTF-8`).
    pub encoding: String,
}

impl Default for CsvLayout {
    /// `CsvImportSettings::CsvImportSettings` (`gnc-imp-settings-csv.hpp`).
    fn default() -> Self {
        Self {
            format: FileFormat::Csv,
            separators: ",".to_owned(),
            escape: true,
            column_widths: Vec::new(),
            skip_start_lines: 0,
            skip_end_lines: 0,
            skip_alt_lines: false,
            date_format: DateFormat::default(),
            currency_format: CurrencyFormat::default(),
            encoding: "UTF-8".to_owned(),
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

/// The base account of a single-line import, as GnuCash saves it: by GUID (`BaseAccountGuid`, since 4.5) and by
/// full name (`BaseAccount`). GnuCash looks the GUID up first and falls back on the name
/// (`gnc-imp-settings-csv-tx.cpp:209-247`); resolving it is the import pipeline's job.
///
/// In JSON it is either the full name alone, as a string, or `{ "guid": …, "fullName": … }`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "BaseAccountJson", into = "BaseAccountJson")]
pub struct BaseAccount {
    /// The account's GUID, 32 hex digits.
    pub guid: Option<String>,
    /// The account's full name, with the book's separator.
    pub full_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum BaseAccountJson {
    Name(String),
    #[serde(rename_all = "camelCase")]
    Full {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guid: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        full_name: Option<String>,
    },
}

impl From<BaseAccountJson> for BaseAccount {
    fn from(json: BaseAccountJson) -> Self {
        match json {
            BaseAccountJson::Name(name) => Self {
                guid: None,
                full_name: Some(name),
            },
            BaseAccountJson::Full { guid, full_name } => Self { guid, full_name },
        }
    }
}

impl From<BaseAccount> for BaseAccountJson {
    fn from(account: BaseAccount) -> Self {
        match account {
            BaseAccount {
                guid: None,
                full_name: Some(name),
            } => Self::Name(name),
            BaseAccount { guid, full_name } => Self::Full { guid, full_name },
        }
    }
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
    /// The account every line's first split goes to, in single-line mode only (`BaseAccount`, `BaseAccountGuid`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_account: Option<BaseAccount>,
    /// The type of each column, in order (`ColumnTypes`).
    pub columns: Vec<ColumnType>,
}

impl TxLayout {
    /// GnuCash's built-in presets with their names, in the order the assistant lists them
    /// (`get_import_presets_trans`, `gnc-imp-settings-csv-tx.cpp:171-174`).
    pub fn presets() -> Vec<(&'static str, Self)> {
        vec![
            (NO_SETTINGS, Self::default()),
            (GNUCASH_EXPORT, Self::gnucash_export()),
            (GNUCASH_EXPORT_4, Self::gnucash_export_4()),
        ]
    }

    /// The built-in preset of this name.
    pub fn preset(name: &str) -> Option<Self> {
        Self::presets().into_iter().find(|(n, _)| *n == name).map(|(_, layout)| layout)
    }

    /// The layout of GnuCash's own transaction export (`create_int_gnc_exp_preset`, `gnc-imp-settings-csv-tx.cpp:67-101`).
    /// As in GnuCash, the date and currency formats are left at their defaults (`:74-79`).
    pub fn gnucash_export() -> Self {
        use ColumnType::{
            Account, Action, Amount, Commodity, Date, Description, Memo, None, Notes, Num, Price, RecDate, RecState, UniqueId, Value,
            VoidReason,
        };
        Self::export_preset(vec![
            Date,
            UniqueId,
            Num,
            Description,
            Notes,
            Commodity,
            VoidReason,
            Action,
            Memo,
            Account,
            None,
            None,
            Amount,
            None,
            Value,
            RecState,
            RecDate,
            Price,
        ])
    }

    /// The layout of the export of GnuCash 4.x and older (`create_int_gnc_exp_4_preset`,
    /// `gnc-imp-settings-csv-tx.cpp:103-135`).
    pub fn gnucash_export_4() -> Self {
        use ColumnType::{
            Account, Action, Amount, Commodity, Date, Description, Memo, None, Notes, Num, Price, RecDate, RecState, UniqueId, VoidReason,
        };
        Self::export_preset(vec![
            Date,
            UniqueId,
            Num,
            Description,
            Notes,
            Commodity,
            VoidReason,
            Action,
            Memo,
            Account,
            None,
            None,
            Amount,
            RecState,
            RecDate,
            Price,
        ])
    }

    fn export_preset(columns: Vec<ColumnType>) -> Self {
        Self {
            csv: CsvLayout {
                skip_start_lines: 1,
                ..CsvLayout::default()
            },
            multi_split: true,
            base_account: None,
            columns,
        }
    }

    /// The layout as GnuCash would load it back: the shared settings sanitized, and column types not allowed in the
    /// layout's mode replaced by `None` (`gnc-imp-settings-csv-tx.cpp:263-279`).
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.csv = self.csv.sanitized();
        let multi_split = self.multi_split;
        self.columns.iter_mut().for_each(|t| *t = t.sanitize(multi_split));
        self
    }
}

/// A CSV price import layout: GnuCash's `CsvPriceImpSettings` (`5.17:gnucash/import-export/csv-imp/gnc-imp-settings-csv-price.cpp`,
/// Copyright (C) 2017 Robert Fewell), saved in GnuCash's state file under `Import csv,price - <name>` and in red-ale's
/// Configuration under `import.csvPriceLayouts.<name>`.
///
/// GnuCash re-applies a preset's separators but not its `EnableEscape` (`gnc-import-price.cpp:313-314`), so a price
/// preset always reads with the tokenizer's default; red-ale honours `escape`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PriceLayout {
    /// The settings shared with the other CSV importers.
    #[serde(flatten)]
    pub csv: CsvLayout,
    /// The commodity every price is of when there are no From Symbol and From Namespace columns: GnuCash's
    /// "Commodity From", saved as its unique name `NAMESPACE::MNEMONIC` (`PriceFromCommodity`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_commodity: Option<String>,
    /// The currency every price is in when there is no Currency To column: GnuCash's "Currency To", saved as
    /// `CURRENCY::<code>` (`PriceToCurrency`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_currency: Option<String>,
    /// The type of each column, in order (`ColumnTypes`).
    pub columns: Vec<PriceColumn>,
}

impl PriceLayout {
    /// GnuCash's built-in price presets: `No Settings` only (`gnc-imp-settings-csv-price.cpp:101-102`).
    pub fn presets() -> Vec<(&'static str, Self)> {
        vec![(NO_SETTINGS, Self::default())]
    }

    /// The built-in preset of this name.
    pub fn preset(name: &str) -> Option<Self> {
        Self::presets().into_iter().find(|(n, _)| *n == name).map(|(_, layout)| layout)
    }

    /// The layout as GnuCash would load it back: the shared settings sanitized, and empty commodity names as none.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.csv = self.csv.sanitized();
        self.from_commodity = self.from_commodity.filter(|c| !c.is_empty());
        self.to_currency = self.to_currency.filter(|c| !c.is_empty());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_layouts() {
        let json = r#"{ "skipStartLines": 1, "dateFormat": "d-m-y", "toCurrency": "CURRENCY::EUR",
            "columns": ["Date", "From Symbol", "From Namespace", "Amount"] }"#;
        let layout: PriceLayout = serde_json::from_str(json).unwrap();
        assert_eq!(
            layout.columns,
            [
                PriceColumn::Date,
                PriceColumn::FromSymbol,
                PriceColumn::FromNamespace,
                PriceColumn::Amount
            ]
        );
        assert_eq!(
            (layout.to_currency.as_deref(), layout.from_commodity.as_deref()),
            (Some("CURRENCY::EUR"), None)
        );
        assert_eq!(layout.csv.skip_start_lines, 1);
        let back: PriceLayout = serde_json::from_value(serde_json::to_value(&layout).unwrap()).unwrap();
        assert_eq!(back, layout);
        assert_eq!(PriceLayout::preset(NO_SETTINGS), Some(PriceLayout::default()));
        assert!(serde_json::from_str::<PriceLayout>(r#"{ "columns": ["Price"] }"#).is_err());
    }

    #[test]
    fn defaults_match_gnucash() {
        let layout = TxLayout::default();
        assert_eq!(layout.csv.separators, ",");
        assert_eq!(layout.csv.encoding, "UTF-8");
        assert!(layout.csv.escape && !layout.multi_split && layout.columns.is_empty());
        assert_eq!(layout.csv.format, FileFormat::Csv);
    }

    #[test]
    fn presets() {
        let presets = TxLayout::presets();
        let names: Vec<_> = presets.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, [NO_SETTINGS, GNUCASH_EXPORT, GNUCASH_EXPORT_4]);
        assert_eq!(presets[1].1.columns.len(), 18);
        assert_eq!(presets[2].1.columns.len(), 16);
        assert!(presets[1].1.multi_split && presets[1].1.csv.skip_start_lines == 1);
        assert!(is_reserved_name(GNUCASH_EXPORT) && !is_reserved_name(GNUCASH_EXPORT_4));
        assert_eq!(TxLayout::preset(GNUCASH_EXPORT), Some(TxLayout::gnucash_export()));
    }

    /// The example layout of `design/api/import.md`, "CSV transactions".
    #[test]
    fn reads_the_import_md_example() {
        let json = r#"{ "format": "csv", "separators": ",", "escape": true, "columnWidths": [],
            "skipStartLines": 1, "skipEndLines": 0, "skipAltLines": false,
            "dateFormat": "d-m-y", "currencyFormat": "comma", "encoding": "windows-1252",
            "multiSplit": false, "baseAccount": "Assets:Current Assets:Checking",
            "columns": ["Date", "Description", "None", "Amount", "Amount (Negated)"] }"#;
        let layout: TxLayout = serde_json::from_str(json).unwrap();
        assert_eq!(layout.csv.date_format, DateFormat::Dmy);
        assert_eq!(layout.csv.currency_format, CurrencyFormat::Comma);
        assert_eq!(layout.csv.encoding, "windows-1252");
        assert_eq!(
            layout.base_account.as_ref().and_then(|b| b.full_name.as_deref()),
            Some("Assets:Current Assets:Checking")
        );
        assert_eq!(
            layout.columns,
            [
                ColumnType::Date,
                ColumnType::Description,
                ColumnType::None,
                ColumnType::Amount,
                ColumnType::AmountNeg
            ]
        );
        let back: TxLayout = serde_json::from_str(&serde_json::to_string(&layout).unwrap()).unwrap();
        assert_eq!(back, layout);
    }

    #[test]
    fn json_defaults_legacy_names_and_base_account_object() {
        let json = r#"{ "format": "fixedWidth", "columnWidths": [8, 3], "columns": ["Deposit", "Withdrawal", "Num"],
            "baseAccount": { "guid": "0123456789abcdef0123456789abcdef", "fullName": "Assets:Bank" } }"#;
        let layout: TxLayout = serde_json::from_str(json).unwrap();
        assert_eq!(layout.csv.separators, ",");
        assert_eq!(layout.csv.format, FileFormat::FixedWidth);
        assert_eq!(layout.columns, [ColumnType::Amount, ColumnType::AmountNeg, ColumnType::Num]);
        assert_eq!(
            layout.base_account.as_ref().and_then(|b| b.guid.as_deref()),
            Some("0123456789abcdef0123456789abcdef")
        );
        let out = serde_json::to_value(&layout).unwrap();
        assert_eq!(out["columns"], serde_json::json!(["Amount", "Amount (Negated)", "Number"]));
        assert_eq!(out["baseAccount"]["fullName"], "Assets:Bank");
        assert!(serde_json::from_str::<TxLayout>(r#"{ "columns": ["Bogus"] }"#).is_err());
    }

    #[test]
    fn sanitized_drops_blacklisted_columns() {
        let layout = TxLayout {
            multi_split: true,
            columns: vec![ColumnType::TransferAccount],
            ..TxLayout::default()
        };
        assert_eq!(layout.sanitized().columns, [ColumnType::None]);
        let layout = TxLayout {
            columns: vec![ColumnType::UniqueId],
            ..TxLayout::default()
        };
        assert_eq!(layout.sanitized().columns, [ColumnType::None]);
    }
}
