//! The business imports on the wire: the params of `csvDocuments` and `csvOwners` previews, their records in the
//! Import proposal, and what applying them did (`design/api/import.md`, "Business documents", "Customers and
//! vendors").

use red_ale_core::business::OwnerKind;
use red_ale_core::ops::business::documents::DocumentKind;
use red_ale_core::{Book, Guid};
use red_ale_import::business::{
    AccountNames, DateFormat, PostPlan, ProposedDocument, ProposedEntry, ProposedOwner, RecordsApplied, Separator, Totals,
};
use red_ale_import::business::owners::AddressFields;
use red_ale_service::import::{DocumentsParams, OwnersParams};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::params::{ActionWire, ImportFileParam};
use super::proposal::{SourceWire, WarningWire};
use crate::methods::business::contacts::{OwnerInfo, OwnerKindWire};
use crate::methods::business::documents::DiscountHowWire;
use crate::methods::business::terms::AmountTypeWire;
use crate::wire::{Amount, CommodityId, Day, Id};

/// How fields are separated: GnuCash's four choices.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BusinessSeparatorWire {
    /// Semicolons, no quotes (the default).
    #[default]
    Semicolon,
    /// Commas, no quotes.
    Comma,
    /// Semicolons, each field optionally in double quotes.
    SemicolonQuoted,
    /// Commas, each field optionally in double quotes.
    CommaQuoted,
}

impl From<BusinessSeparatorWire> for Separator {
    fn from(s: BusinessSeparatorWire) -> Self {
        match s {
            BusinessSeparatorWire::Semicolon => Self::Semicolon,
            BusinessSeparatorWire::Comma => Self::Comma,
            BusinessSeparatorWire::SemicolonQuoted => Self::SemicolonQuoted,
            BusinessSeparatorWire::CommaQuoted => Self::CommaQuoted,
        }
    }
}

/// Bills or invoices.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DocumentKindWire {
    /// Vendor bills (GnuCash's default).
    #[default]
    Bill,
    /// Customer invoices.
    Invoice,
}

/// Customers or vendors.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum OwnerImportKind {
    /// Customers (GnuCash's default).
    #[default]
    Customer,
    /// Vendors.
    Vendor,
}

/// GnuCash's date format preference.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DateFormatWire {
    /// `%m/%d/%Y`.
    Us,
    /// `%d/%m/%Y`.
    Uk,
    /// `%d.%m.%Y`.
    Ce,
    /// `%Y-%m-%d`.
    Iso,
    /// The Locale's date format.
    Locale,
}

impl From<DateFormatWire> for DateFormat {
    fn from(d: DateFormatWire) -> Self {
        match d {
            DateFormatWire::Us => Self::Us,
            DateFormatWire::Uk => Self::Uk,
            DateFormatWire::Ce => Self::Ce,
            DateFormatWire::Iso => Self::Iso,
            DateFormatWire::Locale => Self::Locale,
        }
    }
}

/// How the file names accounts: GnuCash's "show leaf account names" preference.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AccountNamesWire {
    /// Full names, `Expenses:Office` (the default).
    #[default]
    Full,
    /// Leaf names, the first found breadth first.
    Leaf,
}

/// `import.preview` with `format: "csvDocuments"`: GnuCash's Import Bills & Invoices.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DocumentsPreviewParam {
    /// The file, exactly one.
    pub files: Vec<ImportFileParam>,
    /// `bill` (default) or `invoice`.
    #[serde(default)]
    pub document_kind: DocumentKindWire,
    /// How fields are separated; default `semicolon`.
    #[serde(default)]
    pub separator: BusinessSeparatorWire,
    /// How dates are written; default the Configuration's `reports.dateFormat`.
    #[serde(default)]
    pub date_format: Option<DateFormatWire>,
    /// `full` (default) or `leaf` account names.
    #[serde(default)]
    pub account_names: AccountNamesWire,
    /// Append the rows to existing unposted documents with their IDs: GnuCash's question, answered up front. Default
    /// false, which skips them.
    #[serde(default)]
    pub update_existing: bool,
    /// Apply open payments to each document posted (GnuCash's auto-pay preference). Default false.
    #[serde(default)]
    pub auto_pay: bool,
}

impl DocumentsPreviewParam {
    /// The service's params.
    pub fn params(&self) -> DocumentsParams {
        DocumentsParams {
            kind: match self.document_kind {
                DocumentKindWire::Bill => DocumentKind::Bill,
                DocumentKindWire::Invoice => DocumentKind::Invoice,
            },
            separator: self.separator.into(),
            date_format: self.date_format.map(Into::into),
            account_names: match self.account_names {
                AccountNamesWire::Full => AccountNames::Full,
                AccountNamesWire::Leaf => AccountNames::Leaf,
            },
            update_existing: self.update_existing,
            auto_pay: self.auto_pay,
        }
    }
}

/// `import.preview` with `format: "csvOwners"`: GnuCash's Import Customers & Vendors.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OwnersPreviewParam {
    /// The file, exactly one.
    pub files: Vec<ImportFileParam>,
    /// `customer` (default) or `vendor`.
    #[serde(default)]
    pub owner_kind: OwnerImportKind,
    /// How fields are separated; default `semicolon`.
    #[serde(default)]
    pub separator: BusinessSeparatorWire,
}

impl OwnersPreviewParam {
    /// The service's params.
    pub fn params(&self) -> OwnersParams {
        let kind = match self.owner_kind {
            OwnerImportKind::Customer => OwnerKind::Customer,
            OwnerImportKind::Vendor => OwnerKind::Vendor,
        };
        OwnersParams { kind, separator: self.separator.into() }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The proposal.

/// One entry a document's row adds.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntryWire {
    /// The row's line.
    pub line: usize,
    /// The entry date.
    pub date: Day,
    /// Description.
    pub description: String,
    /// Action.
    pub action: String,
    /// Notes (invoices).
    pub notes: String,
    /// The income or expense account.
    pub account: Id,
    /// Its full name.
    pub account_name: Option<String>,
    /// Quantity.
    pub quantity: Amount,
    /// Unit price.
    pub price: Amount,
    /// Discount (invoices).
    pub discount: Amount,
    /// Discount type (invoices).
    pub discount_type: AmountTypeWire,
    /// When the discount applies (invoices).
    pub discount_how: DiscountHowWire,
    /// Taxable.
    pub taxable: bool,
    /// Tax included in the price.
    pub tax_included: bool,
    /// The tax table.
    pub tax_table: Option<Id>,
}

/// How a document will be posted, or why not.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostWire {
    /// True when applying posts it.
    pub will_post: bool,
    /// Why it won't be posted (also a `not-posted` warning).
    pub reason: Option<String>,
    /// The A/R or A/P account.
    pub account: Id,
    /// Its full name.
    pub account_name: Option<String>,
    /// The posting date.
    pub date: Day,
    /// The due date.
    pub due_date: Day,
    /// The memo.
    pub memo: String,
    /// One split per account rather than per entry.
    pub accumulate_splits: bool,
    /// Apply open payments afterwards.
    pub auto_pay: bool,
}

/// A document's totals once applied, signed as printed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TotalsWire {
    /// Σ entry values.
    pub subtotal: Amount,
    /// Σ taxes.
    pub tax: Amount,
    /// Subtotal and tax.
    pub total: Amount,
}

/// One document of a `csvDocuments` proposal.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentWire {
    /// Its position: the key `import.apply` changes it by.
    pub index: usize,
    /// Where it came from.
    pub source: SourceWire,
    /// The line of each of its rows.
    pub lines: Vec<usize>,
    /// Its ID.
    pub id: String,
    /// `invoice` or `bill`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The customer or vendor.
    pub owner: OwnerInfo,
    /// The owner's ID as the file gives it.
    pub owner_id: String,
    /// The document in the Book it updates (or would, when skipped).
    pub existing: Option<Id>,
    /// The earlier record of this proposal that creates the document it updates.
    pub same_as: Option<usize>,
    /// `add`, `update` (append entries) or `skip`.
    pub action: ActionWire,
    /// The document currency: the owner's.
    pub currency: CommodityId,
    /// The opening date of a new document.
    pub date_opened: Day,
    /// Billing ID of a new document.
    pub billing_id: String,
    /// Notes of a new document.
    pub notes: String,
    /// The entries its rows add.
    pub entries: Vec<EntryWire>,
    /// Posting, when the file asks for it.
    pub post: Option<PostWire>,
    /// Totals once applied; null when skipped.
    pub totals: Option<TotalsWire>,
    /// True when it is posted before the read-only threshold.
    pub read_only_by_date: bool,
    /// Warnings about it: `fixed`, `exists`, `posted`, `not-posted`, `duplicate-id`, `duplicate-entry`, `read-only-by-date`.
    pub warnings: Vec<WarningWire>,
}

/// An address as a row sets it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AddressWire {
    /// Name.
    pub name: String,
    /// Line 1.
    pub addr1: String,
    /// Line 2.
    pub addr2: String,
    /// Line 3.
    pub addr3: String,
    /// Line 4.
    pub addr4: String,
    /// Phone.
    pub phone: String,
    /// Fax.
    pub fax: String,
    /// Email.
    pub email: String,
}

impl From<&AddressFields> for AddressWire {
    fn from(a: &AddressFields) -> Self {
        let [addr1, addr2, addr3, addr4] = a.lines.clone();
        Self { name: a.name.clone(), addr1, addr2, addr3, addr4, phone: a.phone.clone(), fax: a.fax.clone(), email: a.email.clone() }
    }
}

/// What a row sets on its Owner.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OwnerFieldsWire {
    /// The Owner's name (the company column, or the name column when that is empty).
    pub company: String,
    /// The address.
    pub address: AddressWire,
    /// Notes.
    pub notes: String,
    /// The shipping address; customers only.
    pub shipping: Option<AddressWire>,
}

/// One Owner of a `csvOwners` proposal.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnerWire {
    /// Its position: the key `import.apply` changes it by.
    pub index: usize,
    /// Its line.
    pub line: usize,
    /// `customer` or `vendor`.
    pub kind: OwnerKindWire,
    /// The ID: the file's, or the next number of the Owner's Counter when the file gives none.
    pub id: String,
    /// True when the ID comes from the Counter, which applying advances.
    pub id_from_counter: bool,
    /// The Owner in the Book it updates (or would, when skipped).
    pub existing: Option<Id>,
    /// The earlier record of this proposal that creates the Owner it updates.
    pub same_as: Option<usize>,
    /// `add`, `update` or `skip`.
    pub action: ActionWire,
    /// For a header row proposed as `skip`: the action GnuCash would take, which `import.apply` accepts.
    pub alternative: Option<ActionWire>,
    /// The fields the row sets, every one of them.
    pub fields: OwnerFieldsWire,
    /// The currency of a new Owner: `defaultCurrency`.
    pub currency: Option<CommodityId>,
    /// The fields an update changes (`company`, `notes`, `address.addr1`, `shipping.email`, `active`, …).
    pub changes: Vec<String>,
    /// True when an inactive Owner becomes active.
    pub reactivate: bool,
    /// Warnings about it: `fixed`, `header-row`, `unchanged`, `ignored-columns`, `id-in-use`.
    pub warnings: Vec<WarningWire>,
}

fn entry_wire(book: &Book, e: &ProposedEntry) -> EntryWire {
    EntryWire {
        line: e.line,
        date: Day(e.date),
        description: e.description.clone(),
        action: e.action.clone(),
        notes: e.notes.clone(),
        account: Id(e.account),
        account_name: book.full_name(e.account),
        quantity: e.quantity.into(),
        price: e.price.into(),
        discount: e.discount.into(),
        discount_type: e.discount_type.into(),
        discount_how: e.discount_how.into(),
        taxable: e.taxable,
        tax_included: e.tax_included,
        tax_table: e.tax_table.map(Id),
    }
}

fn post_wire(book: &Book, p: &PostPlan) -> PostWire {
    PostWire {
        will_post: p.not_posted.is_none(),
        reason: p.not_posted.clone(),
        account: Id(p.account),
        account_name: book.full_name(p.account),
        date: Day(p.date),
        due_date: Day(p.due),
        memo: p.memo.clone(),
        accumulate_splits: p.accumulate,
        auto_pay: p.auto_pay,
    }
}

fn totals_wire(t: &Totals) -> TotalsWire {
    TotalsWire { subtotal: t.subtotal.into(), tax: t.tax.into(), total: t.total.into() }
}

/// A document as the proposal shows it.
pub fn document_wire(book: &Book, d: &ProposedDocument) -> DocumentWire {
    DocumentWire {
        index: d.index,
        source: d.source.into(),
        lines: d.lines.clone(),
        id: d.id.clone(),
        kind: if d.kind == DocumentKind::Invoice { "invoice" } else { "bill" }.to_owned(),
        owner: d.owner.into(),
        owner_id: d.owner_id.clone(),
        existing: d.existing.map(Id),
        same_as: d.same_as,
        action: d.action.into(),
        currency: CommodityId(d.currency.clone()),
        date_opened: Day(d.opened),
        billing_id: d.billing_id.clone(),
        notes: d.notes.clone(),
        entries: d.entries.iter().map(|e| entry_wire(book, e)).collect(),
        post: d.post.as_ref().map(|p| post_wire(book, p)),
        totals: d.totals.as_ref().map(totals_wire),
        read_only_by_date: d.read_only_by_date,
        warnings: d.warnings.iter().map(WarningWire::from).collect(),
    }
}

/// An Owner as the proposal shows it.
pub fn owner_wire(o: &ProposedOwner) -> OwnerWire {
    let fields = &o.fields;
    OwnerWire {
        index: o.index,
        line: o.line,
        kind: o.kind.into(),
        id: o.id.clone(),
        id_from_counter: o.counter.is_some(),
        existing: o.existing.map(Id),
        same_as: o.same_as,
        action: o.action.into(),
        alternative: o.alternative.map(Into::into),
        fields: OwnerFieldsWire {
            company: fields.company.clone(),
            address: (&fields.address).into(),
            notes: fields.notes.clone(),
            shipping: (o.kind == OwnerKind::Customer).then(|| (&fields.shipping).into()),
        },
        currency: o.currency.clone().map(CommodityId),
        changes: o.changes.clone(),
        reactivate: o.reactivate,
        warnings: o.warnings.iter().map(WarningWire::from).collect(),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The result of `import.apply`.

/// A record applied, by its index in the proposal.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndexedRecord {
    /// Its index in the proposal.
    pub index: usize,
    /// The document or Owner.
    pub guid: Id,
}

/// Documents or Owners applying created, updated and posted.
#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct RecordsChanged {
    /// Created.
    pub created: Vec<IndexedRecord>,
    /// Existing ones updated.
    pub updated: Vec<IndexedRecord>,
    /// Documents posted; always empty for Owners.
    pub posted: Vec<IndexedRecord>,
    /// Indexes skipped.
    pub skipped: Vec<usize>,
}

fn indexed(list: Vec<(usize, Guid)>) -> Vec<IndexedRecord> {
    list.into_iter().map(|(index, g)| IndexedRecord { index, guid: Id(g) }).collect()
}

impl From<RecordsApplied> for RecordsChanged {
    fn from(r: RecordsApplied) -> Self {
        Self { created: indexed(r.created), updated: indexed(r.updated), posted: indexed(r.posted), skipped: r.skipped }
    }
}
