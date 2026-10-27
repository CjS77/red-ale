//! The params of `import.preview` and `import.apply`, and their translation to the service's types.

use std::borrow::Cow;
use std::collections::BTreeMap;

use base64::Engine;
use red_ale_core::{Book, ReconcileState};
use red_ale_import::csv::accounts::Separator;
use red_ale_import::csv::{PriceLayout, TxLayout};
use red_ale_import::matcher::ImportAction;
use red_ale_import::pipeline::{ApplyChoices, Change};
use red_ale_import::qif::DateFormat;
use red_ale_import::qif::import::SecurityOverride;
use red_ale_service::config::schema::{csv_layout, csv_price_layout};
use red_ale_service::import::{
    AccountsParams, CsvParams, FileContent, ImportFile, LayoutChoice, PreviewParams, PriceLayoutChoice, PricesParams, QifParams,
};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use super::business::{DocumentsPreviewParam, OwnersPreviewParam};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, CommodityId, Id};

/// One import file (`ImportFile`): exactly one of `text` and `base64`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ImportFileParam {
    /// The file's name, shown in messages.
    #[serde(default)]
    pub name: Option<String>,
    /// The content as text, already Unicode.
    #[serde(default)]
    pub text: Option<String>,
    /// The content's bytes, base64-encoded (RFC 4648, with padding).
    #[serde(default)]
    pub base64: Option<String>,
    /// The encoding of `base64`'s bytes, a WHATWG label (`utf-8`, `iso-8859-1`, `windows-1252`, `utf-16le`, …).
    /// Default: the layout's, else UTF-8. Refused with `text`. A byte-order mark decides between UTF-8 and UTF-16.
    #[serde(default)]
    pub encoding: Option<String>,
}

impl ImportFileParam {
    fn into_file(self, index: usize) -> Result<ImportFile, RpcError> {
        let content = match (self.text, self.base64) {
            (Some(text), None) => FileContent::Text(text),
            (None, Some(b64)) => FileContent::Bytes(
                base64::engine::general_purpose::STANDARD
                    .decode(b64.trim())
                    .map_err(|e| RpcError::invalid_params(format!("files[{index}].base64 is not base64: {e}")))?,
            ),
            _ => {
                return Err(RpcError::invalid_params(format!(
                    "files[{index}] needs exactly one of text and base64"
                )));
            }
        };
        if matches!(content, FileContent::Text(_)) && self.encoding.is_some() {
            return Err(RpcError::invalid_params(format!(
                "files[{index}].encoding is for base64; text is already Unicode"
            )));
        }
        Ok(ImportFile {
            name: self.name,
            content,
            encoding: self.encoding,
        })
    }
}

/// A CSV import layout: by name (a built-in one or one in `import.csvLayouts`), or given whole.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LayoutParam {
    /// A layout's name.
    Name(String),
    /// A whole layout.
    Layout(Box<TxLayout>),
}

impl JsonSchema for LayoutParam {
    fn schema_name() -> Cow<'static, str> {
        "CsvImportLayoutChoice".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A CSV import layout: the name of a built-in one (No Settings, GnuCash Export Format, GnuCash Export Format (4.x and older)) or of one in the Configuration's import.csvLayouts, or a whole layout.",
            "oneOf": [{ "type": "string", "minLength": 1 }, csv_layout()],
        })
    }
}

/// The params of `import.preview`: the common ones and each format's own, chosen by `format`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "format")]
#[schemars(extend("type" = "object"))]
pub enum PreviewParam {
    /// GnuCash's Import Transactions from CSV.
    #[serde(rename = "csvTransactions")]
    CsvTransactions(CsvPreviewParam),
    /// GnuCash's Import Bills & Invoices.
    #[serde(rename = "csvDocuments")]
    CsvDocuments(DocumentsPreviewParam),
    /// GnuCash's Import Customers & Vendors.
    #[serde(rename = "csvOwners")]
    CsvOwners(OwnersPreviewParam),
    /// GnuCash's Import Accounts from CSV: its account tree export read back.
    #[serde(rename = "csvAccounts")]
    CsvAccounts(AccountsPreviewParam),
    /// GnuCash's Import Prices from CSV.
    #[serde(rename = "csvPrices")]
    CsvPrices(PricesPreviewParam),
    /// GnuCash's QIF import.
    #[serde(rename = "qif")]
    Qif(QifPreviewParam),
}

/// The separator of an account file: GnuCash's radio buttons.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SeparatorWire {
    /// `,`, the default.
    #[default]
    Comma,
    /// `;`.
    Semicolon,
    /// `:`.
    Colon,
}

impl From<SeparatorWire> for Separator {
    fn from(s: SeparatorWire) -> Self {
        match s {
            SeparatorWire::Comma => Self::Comma,
            SeparatorWire::Semicolon => Self::Semicolon,
            SeparatorWire::Colon => Self::Colon,
        }
    }
}

/// `import.preview` with `format: "csvAccounts"`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AccountsPreviewParam {
    /// The file, exactly one: the twelve columns of GnuCash's account tree export (`export.csvTree`).
    pub files: Vec<ImportFileParam>,
    /// The separator between fields; default comma.
    #[serde(default)]
    pub separator: SeparatorWire,
    /// Header rows to leave out. Default: one when the first row's Type is `Type`, as GnuCash detects it.
    #[serde(default)]
    pub header_rows: Option<usize>,
}

/// A CSV price import layout: by name (the built-in `No Settings`, or one in `import.csvPriceLayouts`), or given
/// whole.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PriceLayoutParam {
    /// A layout's name.
    Name(String),
    /// A whole layout.
    Layout(Box<PriceLayout>),
}

impl JsonSchema for PriceLayoutParam {
    fn schema_name() -> Cow<'static, str> {
        "CsvPriceLayoutChoice".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A CSV price import layout: the name of the built-in one (No Settings) or of one in the Configuration's import.csvPriceLayouts, or a whole layout.",
            "oneOf": [{ "type": "string", "minLength": 1 }, csv_price_layout()],
        })
    }
}

/// `import.preview` with `format: "csvPrices"`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PricesPreviewParam {
    /// The file, exactly one.
    pub files: Vec<ImportFileParam>,
    /// The CSV price import layout.
    pub layout: PriceLayoutParam,
    /// GnuCash's "Allow existing prices to be over written": a same-day price of the pair, in either direction, is
    /// replaced (`update`) rather than kept (`skip`). Default false, as GnuCash's check box starts each time.
    #[serde(default)]
    pub overwrite: bool,
}

/// `import.preview` with `format: "csvTransactions"`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CsvPreviewParam {
    /// The file, exactly one.
    pub files: Vec<ImportFileParam>,
    /// The CSV import layout.
    pub layout: LayoutParam,
    /// Overrides the layout's base account.
    #[serde(default)]
    pub base_account: Option<AccountRef>,
    /// GnuCash's account match page: what each account name the file uses means. Stored in the `csv-account-map`
    /// Import map at apply.
    #[serde(default)]
    pub account_names: BTreeMap<String, AccountRef>,
}

/// One QIF file: an `ImportFile` with the account for its transactions read under no `!Account`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QifFileParam {
    /// The file's name, shown in messages; it gives the default account.
    #[serde(default)]
    pub name: Option<String>,
    /// The content as text, already Unicode.
    #[serde(default)]
    pub text: Option<String>,
    /// The content's bytes, base64-encoded (RFC 4648, with padding).
    #[serde(default)]
    pub base64: Option<String>,
    /// The encoding of `base64`'s bytes, a WHATWG label; default UTF-8. Refused with `text`.
    #[serde(default)]
    pub encoding: Option<String>,
    /// The account (full name) for the file's transactions read under no `!Account`: GnuCash's account page.
    /// Default: from the file's name, as GnuCash's (`qif-file:path-to-accountname`), else "QIF Import".
    #[serde(default)]
    pub default_account: Option<String>,
}

/// A QIF date format: the order of day, month and year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
pub enum QifDateFormatWire {
    /// Month, day, year.
    #[serde(rename = "m-d-y")]
    Mdy,
    /// Day, month, year.
    #[serde(rename = "d-m-y")]
    Dmy,
    /// Year, month, day.
    #[serde(rename = "y-m-d")]
    Ymd,
    /// Year, day, month.
    #[serde(rename = "y-d-m")]
    Ydm,
}

impl From<QifDateFormatWire> for DateFormat {
    fn from(f: QifDateFormatWire) -> Self {
        match f {
            QifDateFormatWire::Mdy => Self::Mdy,
            QifDateFormatWire::Dmy => Self::Dmy,
            QifDateFormatWire::Ymd => Self::Ymd,
            QifDateFormatWire::Ydm => Self::Ydm,
        }
    }
}

/// The reconcile state of a QIF transaction with no `C` line: GnuCash's `qif-import` preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
pub enum QifReconcileWire {
    /// Not reconciled or cleared (GnuCash's default).
    #[default]
    #[serde(rename = "n")]
    New,
    /// Cleared.
    #[serde(rename = "c")]
    Cleared,
    /// Reconciled.
    #[serde(rename = "y")]
    Reconciled,
}

impl From<QifReconcileWire> for ReconcileState {
    fn from(f: QifReconcileWire) -> Self {
        match f {
            QifReconcileWire::New => Self::New,
            QifReconcileWire::Cleared => Self::Cleared,
            QifReconcileWire::Reconciled => Self::Reconciled,
        }
    }
}

/// A new security as the caller wants it created: GnuCash's commodity page.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QifSecurityParam {
    /// Namespace (exchange or type), such as NASDAQ or FUND.
    pub namespace: String,
    /// Mnemonic (ticker).
    pub mnemonic: String,
    /// Full name.
    pub fullname: String,
}

/// `import.preview` with `format: "qif"`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QifPreviewParam {
    /// The files, one or more, converted together as GnuCash's assistant converts the files it loaded.
    pub files: Vec<QifFileParam>,
    /// The date format, when the dates are ambiguous: one of the proposal's `qif.dateFormats`.
    #[serde(default)]
    pub date_format: Option<QifDateFormatWire>,
    /// The default currency; default the Configuration's `defaultCurrency`.
    #[serde(default)]
    pub currency: Option<CommodityId>,
    /// The reconcile state of transactions with no `C` line; default `n`.
    #[serde(default)]
    pub default_reconcile_state: QifReconcileWire,
    /// QIF account name → account full name, existing or to create, over the guess and the saved map.
    #[serde(default)]
    pub accounts: BTreeMap<String, String>,
    /// QIF category → account full name.
    #[serde(default)]
    pub categories: BTreeMap<String, String>,
    /// QIF payee or memo → account full name.
    #[serde(default)]
    pub payees: BTreeMap<String, String>,
    /// QIF security name → the commodity to create for it.
    #[serde(default)]
    pub securities: BTreeMap<String, QifSecurityParam>,
}

impl QifPreviewParam {
    fn resolve(self) -> Result<(Vec<ImportFile>, PreviewParams), RpcError> {
        let default_accounts = self.files.iter().map(|f| f.default_account.clone()).collect();
        let files = files(
            self.files
                .into_iter()
                .map(|f| ImportFileParam {
                    name: f.name,
                    text: f.text,
                    base64: f.base64,
                    encoding: f.encoding,
                })
                .collect(),
        )?;
        let securities = self
            .securities
            .into_iter()
            .map(|(name, s)| {
                (
                    name,
                    SecurityOverride {
                        namespace: s.namespace,
                        mnemonic: s.mnemonic,
                        fullname: s.fullname,
                    },
                )
            })
            .collect();
        let params = QifParams {
            default_accounts,
            date_format: self.date_format.map(DateFormat::from),
            currency: self.currency.map(|c| c.0),
            status_pref: self.default_reconcile_state.into(),
            accounts: self.accounts,
            categories: self.categories,
            payees: self.payees,
            securities,
        };
        Ok((files, PreviewParams::Qif(params)))
    }
}

impl PreviewParam {
    /// The service's params, account references resolved against the Book.
    pub fn resolve(self, book: &Book) -> Result<(Vec<ImportFile>, PreviewParams), RpcError> {
        match self {
            Self::CsvTransactions(p) => {
                let files = files(p.files)?;
                let layout = match p.layout {
                    LayoutParam::Name(name) => LayoutChoice::Named(name),
                    LayoutParam::Layout(layout) => LayoutChoice::Inline(layout),
                };
                let base_account = p.base_account.map(|a| a.resolve(book)).transpose()?;
                let account_names = p
                    .account_names
                    .into_iter()
                    .map(|(name, a)| Ok((name, a.resolve(book)?)))
                    .collect::<Result<_, RpcError>>()?;
                Ok((
                    files,
                    PreviewParams::CsvTransactions(CsvParams {
                        layout,
                        base_account,
                        account_names,
                    }),
                ))
            }
            Self::CsvDocuments(p) => {
                let params = p.params();
                Ok((files(p.files)?, PreviewParams::CsvDocuments(params)))
            }
            Self::CsvOwners(p) => {
                let params = p.params();
                Ok((files(p.files)?, PreviewParams::CsvOwners(params)))
            }
            Self::CsvAccounts(p) => {
                let params = AccountsParams {
                    separator: p.separator.into(),
                    header_rows: p.header_rows,
                };
                Ok((files(p.files)?, PreviewParams::CsvAccounts(params)))
            }
            Self::CsvPrices(p) => {
                let layout = match p.layout {
                    PriceLayoutParam::Name(name) => PriceLayoutChoice::Named(name),
                    PriceLayoutParam::Layout(layout) => PriceLayoutChoice::Inline(layout),
                };
                Ok((
                    files(p.files)?,
                    PreviewParams::CsvPrices(PricesParams {
                        layout,
                        overwrite: p.overwrite,
                    }),
                ))
            }
            Self::Qif(p) => p.resolve(),
        }
    }
}

fn files(files: Vec<ImportFileParam>) -> Result<Vec<ImportFile>, RpcError> {
    files.into_iter().enumerate().map(|(i, f)| f.into_file(i)).collect()
}

/// An Import action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ActionWire {
    /// Add it as a new transaction.
    Add,
    /// Mark the matched split cleared.
    Clear,
    /// Update the matched transaction from it, then clear it.
    Update,
    /// Leave it out.
    Skip,
}

impl From<ActionWire> for ImportAction {
    fn from(a: ActionWire) -> Self {
        match a {
            ActionWire::Add => Self::Add,
            ActionWire::Clear => Self::Clear,
            ActionWire::Update => Self::Update,
            ActionWire::Skip => Self::Skip,
        }
    }
}

impl From<ImportAction> for ActionWire {
    fn from(a: ImportAction) -> Self {
        match a {
            ImportAction::Add => Self::Add,
            ImportAction::Clear => Self::Clear,
            ImportAction::Update => Self::Update,
            ImportAction::Skip => Self::Skip,
        }
    }
}

/// The caller's change to one record (`ImportChange`): a transaction, or an account or price row, which take only
/// `action`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChangeParam {
    /// The record's `index` in the proposal.
    pub index: usize,
    /// The new Import action; `clear` and `update` need a match.
    #[serde(default)]
    pub action: Option<ActionWire>,
    /// The split GUID of one of the transaction's `matches`: GnuCash's match picker.
    #[serde(default, rename = "match")]
    pub match_split: Option<Id>,
    /// The balancing split's account on *add*: GnuCash's "select transfer account". Learned into the Import map.
    #[serde(default)]
    pub account: Option<AccountRef>,
    /// The balancing split's amount, when its account is not in the transaction currency.
    #[serde(default)]
    pub amount: Option<Amount>,
}

/// The params of `import.apply`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ApplyParams {
    /// The proposal's ID.
    pub proposal: String,
    /// The caller's changes, by index.
    #[serde(default)]
    pub changes: Vec<ChangeParam>,
    /// Apply although the proposal lists errors, leaving those rows out (GnuCash's "Skip errors").
    #[serde(default)]
    pub skip_errors: bool,
    /// For *update*: append the imported description and notes instead of replacing them. Default: the import
    /// account's `import-append-text` slot, as GnuCash's checkbox defaults.
    #[serde(default)]
    pub append_text: Option<bool>,
}

impl ApplyParams {
    /// The changes with their accounts resolved, and the choices.
    pub fn resolve(&self, book: &Book) -> Result<(Vec<Change>, ApplyChoices), RpcError> {
        let changes = self
            .changes
            .iter()
            .map(|c| {
                Ok(Change {
                    index: c.index,
                    action: c.action.map(ImportAction::from),
                    match_split: c.match_split.map(|m| m.0),
                    account: c.account.as_ref().map(|a| a.resolve(book)).transpose()?,
                    amount: c.amount.map(|a| a.0),
                })
            })
            .collect::<Result<Vec<_>, RpcError>>()?;
        Ok((
            changes,
            ApplyChoices {
                skip_errors: self.skip_errors,
                append_text: self.append_text,
            },
        ))
    }
}

/// The params of `import.discard`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DiscardParams {
    /// The proposal's ID.
    pub proposal: String,
}
