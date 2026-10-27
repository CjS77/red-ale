//! Importing invoices and bills (`csvDocuments`), as GnuCash's Import Bills & Invoices does.
//!
//! Translated from `gnc_bi_import_fix_bis` and `gnc_bi_import_create_bis`
//! (`5.17:gnucash/import-export/bi-import/dialog-bi-import.c:275-951`, Copyright (C) 2009 Sebastian Held, Rob Laan,
//! Mike Evans) and the helpers in `dialog-bi-import-helper.c` (Copyright (C) 2009 Sebastian Held). All
//! GPL-2.0-or-later; this translation is GPL-3.0-or-later. Spec: `design/api/import.md`, "Business documents".
//!
//! **Grouping.** Consecutive rows with the same ID are one document, and an empty ID continues the document above
//! (`:510-524`); the first row of a document holds its own fields and its posting fields.
//!
//! **Checks** (`gnc_bi_import_fix_bis`, `:275-581`): an error in any row drops every row of its document. Fixes: an
//! invalid due date becomes the posting date, an invalid opening date today, an empty quantity 1, an invalid entry
//! date the opening date. Each fix is a `fixed` warning; GnuCash only counts them.
//!
//! **Creating** (`gnc_bi_import_create_bis`, `:605-951`): an existing document of the kind is found by ID; a new one
//! takes the owner's currency, the opening date (local midnight, `gnc_dmy2time64`), billing ID and notes; each row
//! becomes an entry; a document whose first row has a posting date is posted with `gncInvoicePostToAccount`, unless
//! an account is in another currency.
//!
//! Each document is worked out on a copy of the Book through the same code applying runs ([`apply_all`]), so what
//! red-ale's rules refuse shows before anything changes. Deviations from GnuCash, each where it is made and in
//! `design/api/import.md`: numbers that don't parse are errors ([`super::expr`]); one date parser ([`super::date`]);
//! an unknown tax table name is an error, where GnuCash leaves the entry without one; an entry date fixed to an
//! invalid opening date takes the fixed opening date (today), where GnuCash's entry keeps no date; an entry account
//! red-ale's rules refuse (a placeholder, A/R, A/P, bank, cash, equity, trading, or the wrong side) is an error, and a
//! posting they refuse leaves the document unposted with a `not-posted` warning; an existing credit note with the ID
//! gets a `duplicate-id` warning.

use std::collections::BTreeMap;

use red_ale_core::business::{AmountType, DiscountHow, Entry, Invoice, OwnerKind, OwnerRef};
use red_ale_core::ops::business::calc::document_totals;
use red_ale_core::ops::business::documents::{DocumentKind, add_entry, add_invoice, document_kind, entries_of};
use red_ale_core::ops::business::posting::{PostRequest, post};
use red_ale_core::ops::business::terms::tax_table_by_name;
use red_ale_core::{AccountType, Book, CommodityRef, Date, DomainError, DomainResult, Env, FixedEnv, Frame, Guid, Numeric, Time64};

use super::date::{DateContext, DateFormat, scan_date};
use super::expr::parse_expression;
use super::reader::{self, Row, Separator, un_escape};
use super::{AccountNames, Context, Created, RecordsApplied};
use crate::matcher::ImportAction;
use crate::pipeline::{IgnoredLine, RowError, Source, Warning};

/// The fields of a line, in GnuCash's order (`5.17:gnucash/import-export/bi-import/dialog-bi-import.h:41-47`).
pub const FIELDS: [&str; 22] = [
    "id",
    "date_opened",
    "owner_id",
    "billing_id",
    "notes",
    "date",
    "desc",
    "action",
    "account",
    "quantity",
    "price",
    "disc_type",
    "disc_how",
    "discount",
    "taxable",
    "taxincluded",
    "tax_table",
    "date_posted",
    "due_date",
    "account_posted",
    "memo_posted",
    "accu_splits",
];

/// The fields the quoted expressions allow `""` in (`dialog-bi-import-gui.c:332`, `:342`).
const ESCAPED: [&str; 2] = ["notes", "desc"];

const ID: usize = 0;
const DATE_OPENED: usize = 1;
const OWNER_ID: usize = 2;
const BILLING_ID: usize = 3;
const NOTES: usize = 4;
const DATE: usize = 5;
const DESC: usize = 6;
const ACTION: usize = 7;
const ACCOUNT: usize = 8;
const QUANTITY: usize = 9;
const PRICE: usize = 10;
const DISC_TYPE: usize = 11;
const DISC_HOW: usize = 12;
const DISCOUNT: usize = 13;
const TAXABLE: usize = 14;
const TAXINCLUDED: usize = 15;
const TAX_TABLE: usize = 16;
const DATE_POSTED: usize = 17;
const DUE_DATE: usize = 18;
const ACCOUNT_POSTED: usize = 19;
const MEMO_POSTED: usize = 20;
const ACCU_SPLITS: usize = 21;

/// The first GUID a preview's copy of the Book counts up from (as the transaction review's).
const PREVIEW_GUID_SEED: u128 = 0x7265_642d_616c_6520_6269_2d69_6d70_0000;

/// What the user chooses in GnuCash's dialog, and the preferences it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentOptions {
    /// Bills (GnuCash's default) or invoices.
    pub kind: DocumentKind,
    /// How fields are separated.
    pub separator: Separator,
    /// The date format preference.
    pub date_format: DateFormat,
    /// The "show leaf account names" preference.
    pub account_names: AccountNames,
    /// GnuCash's question whether to update existing documents, answered up front.
    pub update_existing: bool,
    /// The invoice or bill auto-pay preference.
    pub auto_pay: bool,
}

impl Default for DocumentOptions {
    fn default() -> Self {
        Self {
            kind: DocumentKind::Bill,
            separator: Separator::default(),
            date_format: DateFormat::default(),
            account_names: AccountNames::default(),
            update_existing: false,
            auto_pay: false,
        }
    }
}

/// One entry a row makes.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedEntry {
    /// The row's line.
    pub line: usize,
    /// The entry date.
    pub date: Date,
    /// Description, quotes un-escaped.
    pub description: String,
    /// Action.
    pub action: String,
    /// Notes (invoices only).
    pub notes: String,
    /// The income (invoices) or expense (bills) account.
    pub account: Guid,
    /// Quantity.
    pub quantity: Numeric,
    /// Unit price.
    pub price: Numeric,
    /// Discount (invoices only).
    pub discount: Numeric,
    /// Discount type (invoices only).
    pub discount_type: AmountType,
    /// When the discount applies (invoices only).
    pub discount_how: DiscountHow,
    /// Taxable.
    pub taxable: bool,
    /// Tax included in the price.
    pub tax_included: bool,
    /// The tax table.
    pub tax_table: Option<Guid>,
}

/// How a document will be posted, or why it won't be.
#[derive(Debug, Clone, PartialEq)]
pub struct PostPlan {
    /// The A/R or A/P account.
    pub account: Guid,
    /// The posting date.
    pub date: Date,
    /// The due date.
    pub due: Date,
    /// The memo.
    pub memo: String,
    /// Accumulate splits.
    pub accumulate: bool,
    /// Apply open payments afterwards.
    pub auto_pay: bool,
    /// Why it won't be posted, when it won't.
    pub not_posted: Option<String>,
}

/// A document's totals once its entries are added, signed as printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Totals {
    /// Σ entry values.
    pub subtotal: Numeric,
    /// Σ taxes.
    pub tax: Numeric,
    /// Subtotal and tax.
    pub total: Numeric,
}

/// One document of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedDocument {
    /// Its position in the proposal.
    pub index: usize,
    /// Its lines.
    pub source: Source,
    /// The line of each row.
    pub lines: Vec<usize>,
    /// Its ID.
    pub id: String,
    /// Invoice or bill.
    pub kind: DocumentKind,
    /// The customer or vendor.
    pub owner: OwnerRef,
    /// The owner's ID as the file gives it.
    pub owner_id: String,
    /// The document in the Book this updates (or would, when skipped).
    pub existing: Option<Guid>,
    /// The earlier record of this import whose document this updates (or would).
    pub same_as: Option<usize>,
    /// `add`, `update` or `skip`.
    pub action: ImportAction,
    /// The currency, the owner's.
    pub currency: CommodityRef,
    /// The opening date.
    pub opened: Date,
    /// Billing ID.
    pub billing_id: String,
    /// Notes, quotes un-escaped.
    pub notes: String,
    /// The entries the rows add.
    pub entries: Vec<ProposedEntry>,
    /// Posting, when the first row has a posting date.
    pub post: Option<PostPlan>,
    /// The totals once applied; `None` when skipped.
    pub totals: Option<Totals>,
    /// True when it will be posted before the read-only threshold.
    pub read_only_by_date: bool,
    /// Warnings about it.
    pub warnings: Vec<Warning>,
}

impl ProposedDocument {
    /// Whether applying posts it.
    pub fn will_post(&self) -> bool {
        self.post.as_ref().is_some_and(|p| p.not_posted.is_none())
    }
}

/// What a preview found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Proposed {
    /// The documents, by index.
    pub documents: Vec<ProposedDocument>,
    /// Rows that can't be imported.
    pub errors: Vec<RowError>,
    /// Lines that didn't match.
    pub ignored: Vec<IgnoredLine>,
    /// Warnings about the whole file.
    pub warnings: Vec<Warning>,
}

// ---------------------------------------------------------------------------------------------------------------
// Grouping and checking (gnc_bi_import_fix_bis).

/// Consecutive rows with one ID; an empty ID continues the group above (`dialog-bi-import.c:510-524`).
fn group(rows: &[Row]) -> Vec<(String, Vec<&Row>)> {
    rows.iter().fold(Vec::new(), |mut groups: Vec<(String, Vec<&Row>)>, row| {
        let own = &row.fields[ID];
        match groups.last_mut() {
            Some((id, members)) if own.is_empty() || own == id => members.push(row),
            _ => groups.push((own.clone(), vec![row])),
        }
        groups
    })
}

/// `text2bool` (`dialog-bi-import-helper.c:34-49`).
fn text2bool(text: &str) -> bool {
    let t = reader::strip(text);
    let first = t.chars().next().map(|c| c.to_ascii_lowercase());
    matches!(first, Some('y' | 't')) || t == "1" || t.eq_ignore_ascii_case("x")
}

/// `text2disc_type` (`dialog-bi-import-helper.c:69-83`): empty or `%` is a percentage.
fn text2disc_type(text: &str) -> AmountType {
    let t = reader::strip(text);
    if t.is_empty() || t == "%" {
        AmountType::Percent
    } else {
        AmountType::Value
    }
}

/// `text2disc_how` (`dialog-bi-import-helper.c:86-102`).
fn text2disc_how(text: &str) -> DiscountHow {
    match reader::strip(text) {
        "=" => DiscountHow::SameTime,
        ">" => DiscountHow::PostTax,
        _ => DiscountHow::PreTax,
    }
}

/// A group's own fields once checked.
struct Header {
    owner: OwnerRef,
    currency: CommodityRef,
    opened: Date,
    post: Option<PostPlan>,
}

/// Checks one group, collecting GnuCash's messages as errors and its fixes as warnings.
struct Checker<'a, 'c> {
    ctx: &'a Context<'c>,
    opts: &'a DocumentOptions,
    dates: DateContext<'c>,
    id: String,
    errors: Vec<RowError>,
    fixes: Vec<String>,
}

impl Checker<'_, '_> {
    fn error(&mut self, row: &Row, n: usize, column: usize, message: &str) {
        let message = format!("invoice {}/{n}: {message}", self.id);
        self.errors.push(RowError {
            file: self.ctx.file,
            line: row.line,
            column: Some(column + 1),
            message,
        });
    }

    fn fixed(&mut self, n: usize, message: &str) {
        self.fixes.push(format!("invoice {}/{n}: {message}", self.id));
    }

    fn date(&self, text: &str) -> Option<Date> {
        scan_date(text, &self.dates)
    }

    fn account(&self, name: &str) -> Option<Guid> {
        self.opts.account_names.lookup(self.ctx.book, name)
    }

    /// The owner, by exact ID (`gnc_search_vendor_on_id`, `gnc_search_customer_on_id`, `5.17:libgnucash/engine/gncIDSearch.c:46-162`).
    fn owner(&self, owner_id: &str) -> Option<(OwnerRef, Option<CommodityRef>)> {
        let b = &self.ctx.book.business;
        if self.opts.kind == DocumentKind::Invoice {
            let customer = b.customers.values().find(|c| c.id == owner_id)?;
            Some((
                OwnerRef {
                    kind: OwnerKind::Customer,
                    guid: customer.guid,
                },
                customer.currency.clone(),
            ))
        } else {
            let vendor = b.vendors.values().find(|v| v.id == owner_id)?;
            Some((
                OwnerRef {
                    kind: OwnerKind::Vendor,
                    guid: vendor.guid,
                },
                vendor.currency.clone(),
            ))
        }
    }

    /// The first row's own fields (`dialog-bi-import.c:313-453`).
    fn header(&mut self, row: &Row) -> Option<Header> {
        let f = &row.fields;
        if self.id.is_empty() {
            let message = "no invoice ID in first row of import file.".into();
            self.errors.push(RowError {
                file: self.ctx.file,
                line: row.line,
                column: Some(ID + 1),
                message,
            });
        }
        let owner = self.header_owner(row);
        let post = if f[DATE_POSTED].is_empty() { None } else { self.posting(row) };
        let opened = self.date(&f[DATE_OPENED]).unwrap_or_else(|| {
            self.fixed(1, &format!("{:?} is not a valid opening date; today is used.", f[DATE_OPENED]));
            self.ctx.today
        });
        let (owner, currency) = owner?;
        Some(Header {
            owner,
            currency,
            opened,
            post: if f[DATE_POSTED].is_empty() { None } else { Some(post?) },
        })
    }

    fn header_owner(&mut self, row: &Row) -> Option<(OwnerRef, CommodityRef)> {
        let owner_id = row.fields[OWNER_ID].clone();
        if owner_id.is_empty() {
            self.error(row, 1, OWNER_ID, "owner not set.");
            return None;
        }
        let word = if self.opts.kind == DocumentKind::Invoice {
            "customer"
        } else {
            "vendor"
        };
        match self.owner(&owner_id) {
            Some((owner, Some(currency))) => Some((owner, currency)),
            Some((_, None)) => {
                self.error(row, 1, OWNER_ID, &format!("{word} {owner_id} has no currency."));
                None
            }
            None => {
                self.error(row, 1, OWNER_ID, &format!("{word} {owner_id} does not exist."));
                None
            }
        }
    }

    /// The posting fields (`dialog-bi-import.c:371-438`).
    fn posting(&mut self, row: &Row) -> Option<PostPlan> {
        let f = &row.fields;
        let date = self.date(&f[DATE_POSTED]);
        let due = self.date(&f[DUE_DATE]);
        if date.is_none() {
            self.error(row, 1, DATE_POSTED, &format!("{} is not a valid posting date.", f[DATE_POSTED]));
            if due.is_none() {
                self.error(row, 1, DUE_DATE, &format!("{} is not a valid due date.", f[DUE_DATE]));
            }
        } else if due.is_none() {
            self.fixed(1, &format!("{:?} is not a valid due date; the posting date is used.", f[DUE_DATE]));
        }
        let account = self.posting_account(row);
        Some(PostPlan {
            account: account?,
            date: date?,
            due: due.or(date)?,
            memo: f[MEMO_POSTED].clone(),
            accumulate: text2bool(&f[ACCU_SPLITS]),
            auto_pay: self.opts.auto_pay,
            not_posted: None,
        })
    }

    fn posting_account(&mut self, row: &Row) -> Option<Guid> {
        let name = row.fields[ACCOUNT_POSTED].clone();
        let Some(guid) = self.account(&name) else {
            self.error(row, 1, ACCOUNT_POSTED, &format!("account {name} does not exist."));
            return None;
        };
        let (wanted, words) = match self.opts.kind {
            DocumentKind::Invoice => (AccountType::Receivable, "Accounts Receivable"),
            _ => (AccountType::Payable, "Accounts Payable"),
        };
        if self.ctx.book.account(guid).is_some_and(|a| a.account_type == wanted) {
            Some(guid)
        } else {
            self.error(row, 1, ACCOUNT_POSTED, &format!("account {name} is not of type {words}."));
            None
        }
    }

    /// A number column: `gnc_exp_parser_parse`, refusing what GnuCash reads as 0.
    fn number(&mut self, row: &Row, n: usize, column: usize) -> Option<Numeric> {
        let text = &row.fields[column];
        match parse_expression(text, self.ctx.numbers()) {
            Ok(v) => Some(v),
            Err(e) => {
                let message = format!("{} {text:?} is not a number: {e}.", FIELDS[column]);
                self.error(row, n, column, &message);
                None
            }
        }
    }

    /// One row's entry (`dialog-bi-import.c:455-507`, `:777-827`).
    fn entry(&mut self, row: &Row, n: usize, opened: Date) -> Option<ProposedEntry> {
        let f = &row.fields;
        let price = if f[PRICE].is_empty() {
            self.error(row, n, PRICE, "price not set.");
            None
        } else {
            self.number(row, n, PRICE)
        };
        let account = self.account(&f[ACCOUNT]);
        if account.is_none() {
            self.error(row, n, ACCOUNT, &format!("account {} does not exist.", f[ACCOUNT]));
        }
        let quantity = if f[QUANTITY].is_empty() {
            self.fixed(n, "quantity not set; 1 is used.");
            Some(Numeric::ONE)
        } else {
            self.number(row, n, QUANTITY)
        };
        let invoice = self.opts.kind == DocumentKind::Invoice;
        let discount = if invoice && !f[DISCOUNT].is_empty() {
            self.number(row, n, DISCOUNT)
        } else {
            Some(Numeric::ZERO)
        };
        let tax_table = self.tax_table(row, n);
        let date = self.date(&f[DATE]).unwrap_or_else(|| {
            self.fixed(n, &format!("{:?} is not a valid date; the opening date is used.", f[DATE]));
            opened
        });
        Some(ProposedEntry {
            line: row.line,
            date,
            description: un_escape(&f[DESC]),
            action: f[ACTION].clone(),
            notes: if invoice { un_escape(&f[NOTES]) } else { String::new() },
            account: account?,
            quantity: quantity?,
            price: price?,
            discount: discount?,
            discount_type: if invoice {
                text2disc_type(&f[DISC_TYPE])
            } else {
                AmountType::Percent
            },
            discount_how: if invoice {
                text2disc_how(&f[DISC_HOW])
            } else {
                DiscountHow::PreTax
            },
            taxable: text2bool(&f[TAXABLE]),
            tax_included: text2bool(&f[TAXINCLUDED]),
            tax_table: tax_table.ok()?,
        })
    }

    /// `gncTaxTableLookupByName`; an unknown name is an error, where GnuCash leaves the entry without a table.
    fn tax_table(&mut self, row: &Row, n: usize) -> Result<Option<Guid>, ()> {
        let name = &row.fields[TAX_TABLE];
        if name.is_empty() {
            return Ok(None);
        }
        let Some(t) = tax_table_by_name(self.ctx.book, name) else {
            self.error(row, n, TAX_TABLE, &format!("tax table {name} does not exist."));
            return Err(());
        };
        Ok(Some(t.guid))
    }
}

/// A group that passed the checks.
struct Checked {
    id: String,
    lines: Vec<usize>,
    owner_id: String,
    header: Header,
    billing_id: String,
    notes: String,
    entries: Vec<ProposedEntry>,
    fixes: Vec<String>,
}

/// Check one group: everything it holds, or every error, as GnuCash drops the whole document.
fn check(ctx: &Context<'_>, opts: DocumentOptions, id: &str, rows: &[&Row]) -> Result<Checked, Vec<RowError>> {
    let dates = DateContext::new(opts.date_format, ctx.locale, ctx.today);
    let mut c = Checker {
        ctx,
        opts: &opts,
        dates,
        id: id.to_owned(),
        errors: Vec::new(),
        fixes: Vec::new(),
    };
    let first = rows.first().copied();
    let header = first.and_then(|r| c.header(r));
    let opened = header.as_ref().map_or(ctx.today, |h| h.opened);
    let entries: Vec<Option<ProposedEntry>> = rows.iter().enumerate().map(|(i, r)| c.entry(r, i + 1, opened)).collect();
    let (Some(header), Some(first), true) = (header, first, c.errors.is_empty()) else {
        let mut errors = c.errors;
        let what = if id.is_empty() {
            "invoice without id".to_owned()
        } else {
            format!("invoice {id}")
        };
        let line = first.map_or(0, |r| r.line);
        let message = format!("Error(s) in {what}, all rows of this invoice ignored.");
        errors.push(RowError {
            file: ctx.file,
            line,
            column: None,
            message,
        });
        return Err(errors);
    };
    Ok(Checked {
        id: id.to_owned(),
        lines: rows.iter().map(|r| r.line).collect(),
        owner_id: first.fields[OWNER_ID].clone(),
        header,
        billing_id: first.fields[BILLING_ID].clone(),
        notes: un_escape(&first.fields[NOTES]),
        entries: entries.into_iter().flatten().collect(),
        fixes: c.fixes,
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Creating and posting (gnc_bi_import_create_bis), on a copy for the preview and on the Book for apply.

/// An existing document of the kind with this ID, not a credit note (`gnc_search_invoice_on_id`,
/// `gnc_search_bill_on_id`, `5.17:libgnucash/engine/gncIDSearch.c:55-72`, `:140-151`).
fn find_document<'b>(book: &'b Book, kind: DocumentKind, id: &str, credit_note: bool) -> Option<&'b Invoice> {
    book.business
        .invoices
        .values()
        .map(AsRef::as_ref)
        .find(|i| i.id == id && i.is_credit_note() == credit_note && document_kind(book, i) == Some(kind))
}

/// Why a document can't be posted, as GnuCash checks it before posting (`dialog-bi-import.c:867-901`): an entry or
/// tax account in another commodity (`gncInvoiceGetForeignCurrencies`, `5.17:libgnucash/engine/gncInvoice.c:1308-1370`),
/// or a posting account not in the document's currency.
fn cannot_post(book: &Book, doc: &ProposedDocument, target: Guid) -> DomainResult<Option<String>> {
    let Some(plan) = &doc.post else { return Ok(None) };
    let inv = book
        .business
        .invoices
        .get(&target)
        .ok_or_else(|| DomainError::not_found("invoice", &target))?;
    let customer = doc.kind == DocumentKind::Invoice;
    let currency = Some(&doc.currency);
    let entry_accounts: Vec<Guid> = entries_of(book, target)
        .into_iter()
        .filter_map(|e| if customer { e.i_account } else { e.b_account })
        .collect();
    let tax_accounts = document_totals(book, inv)?.taxes.into_iter().map(|(a, _)| a);
    let foreign = entry_accounts
        .into_iter()
        .chain(tax_accounts)
        .any(|a| book.account(a).is_some_and(|a| a.commodity.as_ref() != currency));
    if foreign {
        return Ok(Some(format!(
            "Invoice {} NOT posted because it requires currency conversion.",
            doc.id
        )));
    }
    if book.account(plan.account).and_then(|a| a.commodity.as_ref()) != currency {
        return Ok(Some(format!("Invoice {} NOT posted because currencies don't match.", doc.id)));
    }
    Ok(None)
}

/// A new document (`dialog-bi-import.c:695-725`): the owner's currency, opened at local midnight, active, and no
/// billing term or credit-note slot, as `gncInvoiceCreate` leaves them.
fn new_document(env: &mut dyn Env, doc: &ProposedDocument) -> Invoice {
    Invoice {
        guid: env.new_guid(),
        id: doc.id.clone(),
        owner: doc.owner,
        opened: env.zone().day_start(doc.opened),
        posted: None,
        terms: None,
        billing_id: doc.billing_id.clone(),
        notes: doc.notes.clone(),
        active: true,
        post_txn: None,
        post_lot: None,
        post_acc: None,
        currency: Some(doc.currency.clone()),
        bill_to: None,
        charge_amount: Numeric::ZERO,
        slots: Frame::new(),
    }
}

/// One row's entry (`dialog-bi-import.c:777-827`): dated at local noon (`gncEntrySetDateGDate`,
/// `5.17:libgnucash/engine/gncEntry.c:504-515`), entered now.
fn new_entry(env: &mut dyn Env, kind: DocumentKind, target: Guid, p: &ProposedEntry) -> Entry {
    let date = env.zone().noon(p.date);
    let now = env.now();
    let mut e = Entry::new(env.new_guid(), date, now);
    e.description.clone_from(&p.description);
    e.action.clone_from(&p.action);
    e.quantity = p.quantity;
    if kind == DocumentKind::Invoice {
        e.notes.clone_from(&p.notes);
        (e.i_account, e.i_price, e.i_taxable) = (Some(p.account), p.price, p.taxable);
        (e.i_tax_included, e.i_tax_table) = (p.tax_included, p.tax_table);
        (e.i_discount, e.i_disc_type, e.i_disc_how) = (p.discount, p.discount_type, p.discount_how);
        e.invoice = Some(target);
    } else {
        (e.b_account, e.b_price, e.b_taxable) = (Some(p.account), p.price, p.taxable);
        (e.b_tax_included, e.b_tax_table) = (p.tax_included, p.tax_table);
        e.bill = Some(target);
    }
    e
}

/// A failure with the line it belongs to.
struct Failed {
    line: Option<usize>,
    error: DomainError,
}

impl From<DomainError> for Failed {
    fn from(error: DomainError) -> Self {
        Self { line: None, error }
    }
}

/// Create the document or find the one it updates, then add its entries. Returns the document.
fn create_and_fill(book: &mut Book, env: &mut dyn Env, doc: &ProposedDocument, created: &Created) -> Result<Guid, Failed> {
    let target = match doc.action {
        ImportAction::Add => add_invoice(book, new_document(env, doc))?,
        _ => doc
            .existing
            .or_else(|| doc.same_as.and_then(|i| created.get(&i).copied()))
            .ok_or_else(|| DomainError::invalid(format!("document {} has nothing to update", doc.id)))?,
    };
    doc.entries.iter().try_for_each(|p| {
        let e = new_entry(env, doc.kind, target, p);
        add_entry(book, e).map(|_| ()).map_err(|error| Failed { line: Some(p.line), error })
    })?;
    Ok(target)
}

/// Post as the first row asks (`dialog-bi-import.c:879-886`): due at local midnight of the due date
/// (`gnc_dmy2time64`).
fn post_document(book: &mut Book, env: &mut dyn Env, plan: &PostPlan, target: Guid) -> DomainResult<Guid> {
    let due = env.zone().day_start(plan.due);
    let req = PostRequest {
        invoice: target,
        account: plan.account,
        post_date: plan.date,
        due: Some(due),
        memo: plan.memo.clone(),
        accumulate: plan.accumulate,
        autopay: plan.auto_pay,
        prices: Vec::new(),
    };
    post(book, env, &req).map(|p| p.transaction)
}

/// Apply every document not skipped, in order, as `gnc_bi_import_create_bis` processes them.
pub(super) fn apply_all(
    book: &mut Book,
    env: &mut dyn Env,
    docs: &[ProposedDocument],
    actions: &[ImportAction],
    threshold: Option<Date>,
) -> DomainResult<(RecordsApplied, Vec<Warning>)> {
    let mut out = RecordsApplied::default();
    let mut warnings = Vec::new();
    let mut created = Created::new();
    docs.iter().zip(actions).try_for_each(|(doc, action)| -> DomainResult<()> {
        if *action == ImportAction::Skip {
            out.skipped.push(doc.index);
            return Ok(());
        }
        let target = create_and_fill(book, env, doc, &created).map_err(|f| f.error)?;
        if doc.action == ImportAction::Add {
            created.insert(doc.index, target);
            out.created.push((doc.index, target));
        } else {
            out.updated.push((doc.index, target));
        }
        if let Some(plan) = doc.post.as_ref().filter(|_| doc.will_post()) {
            post_document(book, env, plan, target)?;
            out.posted.push((doc.index, target));
            if threshold.is_some_and(|t| plan.date < t) {
                let message = format!("posted although dated before the read-only threshold {}", plan.date);
                warnings.push(Warning::about(doc.index, "read-only-by-date", message));
            }
        }
        Ok(())
    })?;
    Ok((out, warnings))
}

// ---------------------------------------------------------------------------------------------------------------
// The preview.

/// What finding a document by ID decided: the action, the document it updates and the warnings.
struct Found {
    action: ImportAction,
    existing: Option<Guid>,
    same_as: Option<usize>,
    notes: Vec<(&'static str, String)>,
}

/// Works each checked document out on a copy of the Book, in file order.
struct Planner<'a> {
    ctx: &'a Context<'a>,
    opts: &'a DocumentOptions,
    copy: Book,
    env: FixedEnv,
    created: Created,
    by_guid: BTreeMap<Guid, usize>,
    out: Proposed,
}

impl Planner<'_> {
    /// The action GnuCash takes for a document with this ID (`dialog-bi-import.c:683-775`), and its warnings.
    fn action(&self, c: &Checked) -> Found {
        let kind = self.opts.kind;
        let Some(found) = find_document(&self.copy, kind, &c.id, false) else {
            let note = find_document(&self.copy, kind, &c.id, true).map(|_| {
                (
                    "duplicate-id",
                    format!(
                        "a credit note already has the ID {}; GnuCash creates a second document with it",
                        c.id
                    ),
                )
            });
            return Found {
                action: ImportAction::Add,
                existing: None,
                same_as: None,
                notes: note.into_iter().collect(),
            };
        };
        let (existing, same_as) = match self.by_guid.get(&found.guid) {
            Some(i) => (None, Some(*i)),
            None => (Some(found.guid), None),
        };
        if !self.opts.update_existing {
            let message = format!("Invoice {} not updated because it already exists.", c.id);
            return Found {
                action: ImportAction::Skip,
                existing,
                same_as,
                notes: vec![("exists", message)],
            };
        }
        if found.is_posted() {
            let message = format!("Invoice {} not updated because it is already posted.", c.id);
            return Found {
                action: ImportAction::Skip,
                existing,
                same_as,
                notes: vec![("posted", message)],
            };
        }
        Found {
            action: ImportAction::Update,
            existing,
            same_as,
            notes: Vec::new(),
        }
    }

    fn document(&self, c: Checked, index: usize) -> ProposedDocument {
        let Found {
            action,
            existing,
            same_as,
            notes,
        } = self.action(&c);
        let mut warnings: Vec<Warning> = c.fixes.iter().map(|m| Warning::about(index, "fixed", m.clone())).collect();
        warnings.extend(notes.into_iter().map(|(code, m)| Warning::about(index, code, m)));
        let (first_line, last_line) = (c.lines.first().copied().unwrap_or(0), c.lines.last().copied().unwrap_or(0));
        let source = Source {
            file: self.ctx.file,
            first_line,
            last_line,
        };
        ProposedDocument {
            index,
            source,
            lines: c.lines,
            id: c.id,
            kind: self.opts.kind,
            owner: c.header.owner,
            owner_id: c.owner_id,
            existing,
            same_as,
            action,
            currency: c.header.currency,
            opened: c.header.opened,
            billing_id: c.billing_id,
            notes: c.notes,
            entries: c.entries,
            post: c.header.post,
            totals: None,
            read_only_by_date: false,
            warnings,
        }
    }

    /// Run a document on a copy of the copy; on success the copy takes its result.
    fn trial(&mut self, doc: &mut ProposedDocument) -> Result<Option<Guid>, Failed> {
        if doc.action == ImportAction::Skip {
            return Ok(None);
        }
        let mut trial = self.copy.clone();
        let mut env = self.env.clone();
        let target = create_and_fill(&mut trial, &mut env, doc, &self.created)?;
        let mut not_posted = cannot_post(&trial, doc, target)?;
        if let Some(plan) = doc.post.as_ref().filter(|_| not_posted.is_none()) {
            let mut posted = trial.clone();
            let mut posting_env = env.clone();
            match post_document(&mut posted, &mut posting_env, plan, target) {
                Ok(_) => (trial, env) = (posted, posting_env),
                Err(e) => not_posted = Some(format!("Invoice {} NOT posted: {e}", doc.id)),
            }
        }
        if let Some(plan) = doc.post.as_mut() {
            plan.not_posted = not_posted;
        }
        let inv = trial
            .business
            .invoices
            .get(&target)
            .ok_or_else(|| DomainError::not_found("invoice", &target))?;
        let t = document_totals(&trial, inv)?;
        doc.totals = Some(Totals {
            subtotal: t.subtotal,
            tax: t.tax,
            total: t.total,
        });
        self.copy = trial;
        self.env = env;
        Ok(Some(target))
    }

    /// Warnings that follow from the trial: not posted, read-only by date, entries the document already has.
    fn review(&self, doc: &mut ProposedDocument, before: &Book) {
        let index = doc.index;
        if let Some(reason) = doc
            .post
            .as_ref()
            .and_then(|p| p.not_posted.clone())
            .filter(|_| doc.action != ImportAction::Skip)
        {
            doc.warnings.push(Warning::about(index, "not-posted", reason));
        }
        let posting = doc.post.as_ref().filter(|_| doc.will_post() && doc.action != ImportAction::Skip);
        if let (Some(t), Some(plan)) = (self.ctx.threshold, posting) {
            doc.read_only_by_date = plan.date < t;
            if doc.read_only_by_date {
                doc.warnings.push(Warning::about(
                    index,
                    "read-only-by-date",
                    format!("posted before the read-only threshold {t}"),
                ));
            }
        }
        if let Some(target) = doc.existing.filter(|_| doc.action == ImportAction::Update) {
            let had = entries_of(before, target);
            let customer = doc.kind == DocumentKind::Invoice;
            let zone = self.ctx.zone;
            doc.entries
                .iter()
                .filter(|p| had.iter().any(|e| same_entry(e, p, customer, zone.noon(p.date))))
                .for_each(|p| {
                    let message = format!(
                        "line {}: the document already has this entry; it is added again, as GnuCash does",
                        p.line
                    );
                    doc.warnings.push(Warning::about(index, "duplicate-entry", message));
                });
        }
    }

    fn plan(&mut self, c: Checked) {
        let index = self.out.documents.len();
        let mut doc = self.document(c, index);
        let before = self.copy.clone();
        match self.trial(&mut doc) {
            Ok(target) => {
                if let Some(target) = target.filter(|_| doc.action == ImportAction::Add) {
                    self.created.insert(index, target);
                    self.by_guid.insert(target, index);
                }
                self.review(&mut doc, &before);
                self.out.documents.push(doc);
            }
            Err(f) => {
                let line = f.line.unwrap_or(doc.source.first_line);
                let message = format!("invoice {}: {}", doc.id, f.error);
                self.out.errors.push(RowError {
                    file: self.ctx.file,
                    line,
                    column: None,
                    message,
                });
                let summary = format!("Error(s) in invoice {}, all rows of this invoice ignored.", doc.id);
                self.out.errors.push(RowError {
                    file: self.ctx.file,
                    line: doc.source.first_line,
                    column: None,
                    message: summary,
                });
            }
        }
    }
}

/// An existing entry with the same date, description, account, quantity and price.
fn same_entry(e: &Entry, p: &ProposedEntry, customer: bool, date: Time64) -> bool {
    let (account, price) = if customer {
        (e.i_account, e.i_price)
    } else {
        (e.b_account, e.b_price)
    };
    e.date == date
        && e.description == p.description
        && account == Some(p.account)
        && e.quantity.same_value(p.quantity)
        && price.same_value(p.price)
}

/// Read a `csvDocuments` file and work out what importing it would do.
pub fn propose(text: &str, ctx: &Context<'_>, opts: &DocumentOptions) -> Result<Proposed, regex::Error> {
    let pattern = reader::line_pattern(&FIELDS, &ESCAPED, opts.separator)?;
    let read = reader::read(text, ctx.file, &FIELDS, &pattern, opts.separator);
    let env = FixedEnv::new(ctx.now, PREVIEW_GUID_SEED).with_zone(ctx.zone.clone());
    let copy = ctx.book.clone();
    let mut planner = Planner {
        ctx,
        opts,
        copy,
        env,
        created: Created::new(),
        by_guid: BTreeMap::new(),
        out: Proposed::default(),
    };
    planner.out.ignored = read.ignored;
    planner.out.warnings = stripped_warnings(&read.stripped);
    group(&read.rows)
        .into_iter()
        .for_each(|(id, rows)| match check(ctx, *opts, &id, &rows) {
            Ok(c) => planner.plan(c),
            Err(errors) => planner.out.errors.extend(errors),
        });
    Ok(planner.out)
}

/// A warning for each line characters were removed from.
pub(super) fn stripped_warnings(lines: &[usize]) -> Vec<Warning> {
    lines
        .iter()
        .map(|l| {
            let message = format!("line {l}: control characters were removed, as GnuCash removes them");
            Warning {
                code: "stripped-characters",
                message,
                index: None,
            }
        })
        .collect()
}
