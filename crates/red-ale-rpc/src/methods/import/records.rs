//! The records of the formats that import accounts and prices rather than transactions, as the Import proposal
//! returns them (`design/api/import.md`, "CSV accounts", "CSV prices").

use red_ale_import::csv::accounts::{AccountRow, FieldChange};
use red_ale_import::csv::prices::Existing;
use red_ale_import::pipeline::{ProposedAccount, ProposedPrice};
use schemars::JsonSchema;
use serde::Serialize;

use super::params::ActionWire;
use super::proposal::WarningWire;
use crate::wire::{Amount, CommodityId, Day, Id, Timestamp};

/// One row of a CSV account import.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountRecordWire {
    /// Its position: the key `import.apply` changes it by.
    pub index: usize,
    /// Its line in the file, from 1.
    pub line: usize,
    /// The account's full name.
    pub full_name: String,
    /// `add` (create it), `update` (change the fields in `changes`), or `skip` (it exists and the row changes nothing).
    /// The caller may turn an `add` or `update` into `skip`.
    pub action: ActionWire,
    /// The existing account an update changes; null for an add, or for an account an earlier row adds.
    pub account: Option<Id>,
    /// The row's cells, as read.
    pub fields: AccountFieldsWire,
    /// What an update changes: only code, description, notes and colour, from non-empty cells, as GnuCash does.
    pub changes: Vec<FieldChangeWire>,
    /// Warnings for this row: `color` (a colour not recognised, left as it was), `not-updated` (cells that differ
    /// from the account but that GnuCash's import never changes).
    pub warnings: Vec<WarningWire>,
}

/// The twelve cells of GnuCash's account tree export, as read.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountFieldsWire {
    /// The type, as GnuCash names it (`BANK`).
    #[serde(rename = "type")]
    pub account_type: String,
    /// The account's name.
    pub name: String,
    /// Account code.
    pub code: String,
    /// Description.
    pub description: String,
    /// Colour.
    pub color: String,
    /// Notes.
    pub notes: String,
    /// The commodity's mnemonic.
    pub symbol: String,
    /// The commodity's namespace.
    pub namespace: String,
    /// `T` when hidden.
    pub hidden: String,
    /// `T` when tax-related; GnuCash's import never uses it.
    pub tax_info: String,
    /// `T` when a placeholder.
    pub placeholder: String,
}

/// A field an update changes.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FieldChangeWire {
    /// `code`, `description`, `notes` or `color`.
    pub field: String,
    /// What the account holds, or null.
    pub from: Option<String>,
    /// What the row gives.
    pub to: String,
}

/// One price of a CSV price import.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PriceRecordWire {
    /// Its position: the key `import.apply` changes it by.
    pub index: usize,
    /// Its line in the file, from 1.
    pub line: usize,
    /// The commodity priced.
    pub commodity: CommodityId,
    /// The currency it is priced in.
    pub currency: CommodityId,
    /// The date.
    pub date: Day,
    /// The price's time: the date's day-neutral time.
    pub time: Timestamp,
    /// The amount as the file writes it.
    pub amount: Amount,
    /// The value stored: the amount at the currency's fraction × 10000, rounded half up, as GnuCash stores it.
    pub value: Amount,
    /// `add`; `update`, replacing the same-day price in `existing`; or `skip`, a duplicate of it. The caller may
    /// choose any of the three.
    pub action: ActionWire,
    /// The same-day price of the pair, in either direction, that the price meets: in the Book, or added by an earlier
    /// price of this proposal.
    pub existing: Option<ExistingWire>,
}

/// A same-day price: `{ "price": GUID }` in the Book, or `{ "index": n }` for the price of an earlier record.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ExistingWire {
    /// A price in the Book.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Id>,
    /// The index of the proposal's price that adds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

fn fields(row: &AccountRow) -> AccountFieldsWire {
    AccountFieldsWire {
        account_type: row.account_type.clone(),
        name: row.name.clone(),
        code: row.code.clone(),
        description: row.description.clone(),
        color: row.color.clone(),
        notes: row.notes.clone(),
        symbol: row.symbol.clone(),
        namespace: row.namespace.clone(),
        hidden: row.hidden.clone(),
        tax_info: row.tax.clone(),
        placeholder: row.placeholder.clone(),
    }
}

fn change(c: &FieldChange) -> FieldChangeWire {
    FieldChangeWire { field: c.field.to_owned(), from: c.from.clone(), to: c.to.clone() }
}

impl From<&ProposedAccount> for AccountRecordWire {
    fn from(a: &ProposedAccount) -> Self {
        Self {
            index: a.index,
            line: a.row.line,
            full_name: a.row.full_name.clone(),
            action: a.action.into(),
            account: a.account.map(Id),
            fields: fields(&a.row),
            changes: a.changes.iter().map(change).collect(),
            warnings: a.warnings.iter().map(WarningWire::from).collect(),
        }
    }
}

impl From<&ProposedPrice> for PriceRecordWire {
    fn from(p: &ProposedPrice) -> Self {
        let existing = p.existing.map(|e| match e {
            Existing::Book(g) => ExistingWire { price: Some(Id(g)), index: None },
            Existing::Index(i) => ExistingWire { price: None, index: Some(i) },
        });
        Self {
            index: p.index,
            line: p.line,
            commodity: CommodityId(p.commodity.clone()),
            currency: CommodityId(p.currency.clone()),
            date: Day(p.date),
            time: Timestamp(p.time),
            amount: p.amount.into(),
            value: p.value.into(),
            action: p.action.into(),
            existing,
        }
    }
}
