//! Business operations: posting a document to accounts, and unposting it.
//!
//! A port of `gncInvoicePostToAccount` and `gncInvoiceUnpost` (`5.17:libgnucash/engine/gncInvoice.c:1371-1858`) with
//! the tax-table and billing-term freezing they rely on (`gncTaxTableReturnChild`, `gncBillTermReturnChild`). See
//! `design/notes/engine-business.md` §2.2, §6 and §7. Unlike GnuCash, every input is checked before anything changes,
//! so a refused post leaves no partial state behind.

use std::sync::Arc;

use crate::book_options::num_action_targets;
use crate::business::{BillTerm, Entry, Invoice, OwnerKind, PaymentType, TaxTable};
use crate::lot::slot_paths as lot_slots;
use crate::ops::business::calc::{self, AccountValues, InvoiceType};
use crate::ops::business::{contacts, documents};
use crate::ops::business::payments::{self, LotWork, TXN_TYPE_INVOICE};
use crate::ops::commit::{account_scu, build_split, check_balanced, set_date_posted, sort_splits, NewSplit};
use crate::transaction::slot_paths as txn_slots;
use crate::{AccountType, Book, CommodityRef, DENOM_AUTO, Date, DenomMode, DomainError, DomainResult, Env, Frame, Guid, How, Lot, Numeric, Round, SlotValue, Time64, Transaction};

/// The transaction slot holding a posted document's due date (`5.17:libgnucash/engine/Transaction.cpp:182`).
pub const DATE_DUE: &str = "trans-date-due";
/// The read-only reason GnuCash writes on a posted transaction (`5.17:libgnucash/engine/gncInvoice.c:1713`).
pub const READ_ONLY_REASON: &str = "Generated from an invoice. Try unposting the invoice.";
/// The memo of the extra card charge on a voucher (`5.17:libgnucash/engine/gncInvoice.c:1673`).
pub const EXTRA_CHARGE_MEMO: &str = "Extra to Charge Card";

const EXACT_HALF_UP: How = How::new(DenomMode::Exact, Round::HalfUp);

/// A request to post a document.
#[derive(Debug, Clone, PartialEq)]
pub struct PostRequest {
    /// The document.
    pub invoice: Guid,
    /// The A/Receivable (customer) or A/Payable (vendor, employee) account to post to.
    pub account: Guid,
    /// The posting date.
    pub post_date: Date,
    /// The due date stored on the transaction. `None`: from the document's billing term, or the posting date.
    pub due: Option<Time64>,
    /// Memo of the A/R or A/P split and of accumulated splits.
    pub memo: String,
    /// One split per income or expense account instead of one per entry.
    pub accumulate: bool,
    /// Apply open pre-payments and opposite documents afterwards.
    pub autopay: bool,
    /// For entries posting to an account in another commodity: the price of one unit of that commodity in the
    /// document currency (GnuCash's temporary invoice price, `5.17:gnucash/gnome/dialog-invoice.c:1176-1258`).
    pub prices: Vec<(CommodityRef, Numeric)>,
}

/// What posting created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posted {
    /// The posted transaction.
    pub transaction: Guid,
    /// The document's lot.
    pub lot: Guid,
    /// The due date stored.
    pub due: Time64,
}

/// `gncInvoiceGetIsCreditNote`-aware document checks before posting.
struct Plan {
    inv: Arc<Invoice>,
    kind: InvoiceType,
    currency: CommodityRef,
    entries: Vec<Entry>,
    card: Option<Guid>,
}

fn plan(book: &Book, req: &PostRequest) -> DomainResult<Plan> {
    let inv = book.business.invoices.get(&req.invoice).cloned().ok_or_else(|| DomainError::not_found("invoice", &req.invoice))?;
    if inv.is_posted() {
        return Err(DomainError::conflict(format!("document {} is already posted", inv.id)));
    }
    let kind = calc::invoice_type(book, &inv)?;
    let currency = inv.currency.clone().ok_or_else(|| DomainError::invalid(format!("document {} has no currency", inv.id)))?;
    check_post_account(book, &kind, &currency, req.account)?;
    let entries: Vec<Entry> = documents::entries_of(book, inv.guid).into_iter().cloned().collect();
    if entries.is_empty() {
        return Err(DomainError::invalid(format!("document {} has no entries", inv.id)));
    }
    let card = card_account(book, &kind, &currency)?;
    Ok(Plan { inv, kind, currency, entries, card })
}

/// The posting account must be A/R for customers and A/P otherwise (`5.17:libgnucash/engine/gncOwner.c:1443-1460`),
/// not a placeholder, and in the document currency: GnuCash's dialog offers only such accounts, and
/// `xaccSplitSetBaseValue` cannot set the amount of a split in another commodity.
fn check_post_account(book: &Book, kind: &InvoiceType, currency: &CommodityRef, account: Guid) -> DomainResult<()> {
    let acc = book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
    let wanted = if kind.is_customer() { AccountType::Receivable } else { AccountType::Payable };
    if acc.account_type != wanted {
        return Err(DomainError::invalid(format!("{} is not an {} account", acc.name, wanted.as_str())));
    }
    if acc.is_placeholder() {
        return Err(DomainError::invalid(format!("{} is a placeholder account", acc.name)));
    }
    if acc.commodity.as_ref() != Some(currency) {
        return Err(DomainError::invalid(format!("{} is not in the document currency {currency}", acc.name)));
    }
    Ok(())
}

/// An employee's company card account, which must be in the document currency.
fn card_account(book: &Book, kind: &InvoiceType, currency: &CommodityRef) -> DomainResult<Option<Guid>> {
    if kind.end_owner.kind != OwnerKind::Employee {
        return Ok(None);
    }
    let card = book.business.employees.get(&kind.end_owner.guid).and_then(|e| e.ccard);
    if let Some(card) = card {
        let acc = book.account(card).ok_or_else(|| DomainError::not_found("account", &card))?;
        if acc.commodity.as_ref() != Some(currency) {
            return Err(DomainError::invalid(format!("card account {} is not in the document currency {currency}", acc.name)));
        }
    }
    Ok(card)
}

/// The account an entry posts to on this kind of document.
fn entry_account(entry: &Entry, kind: &InvoiceType) -> DomainResult<Guid> {
    let account = if kind.is_customer() { entry.i_account } else { entry.b_account };
    account.ok_or_else(|| DomainError::invalid(format!("entry {:?} has no account", entry.description)))
}

/// The splits of the posted transaction, in GnuCash's append order (before the commit sort).
struct SplitPlan {
    splits: Vec<NewSplit>,
}

impl SplitPlan {
    fn push(&mut self, account: Guid, value: Numeric, memo: &str, action: &str) {
        let mut s = NewSplit::new(account, value).with_action(action);
        s.memo = (!memo.is_empty()).then(|| memo.to_owned());
        self.splits.push(s);
    }
}

/// Build the splits as `gncInvoicePostToAccount` does (`5.17:libgnucash/engine/gncInvoice.c:1516-1708`).
fn plan_splits(book: &Book, req: &PostRequest, p: &Plan) -> DomainResult<SplitPlan> {
    // Each split's action is the type word, or the document ID with the num field source on (`gnc_set_num_action`).
    let (_, type_str) = num_action_targets(book, &p.inv.id, p.kind.type_string());
    let scu = account_scu(book, req.account)?;
    let totals = calc::document_totals(book, &p.inv)?;
    let flip = |n: Numeric| if p.kind.negate_for_balance() { n.neg() } else { n };
    let mut total = flip(totals.total);
    let taxes: AccountValues = totals.taxes.iter().map(|(a, v)| (*a, flip(*v))).collect();
    let mut out = SplitPlan { splits: Vec::new() };
    let mut splitinfo = AccountValues::new();
    for entry in &p.entries {
        let account = entry_account(entry, &p.kind)?;
        let amounts = calc::entry_amounts(book, entry, p.kind.is_customer(), calc::entry_denom(book, entry))?;
        // gncEntryGetBalValue: the internal value, negated on customer documents.
        let value = if p.kind.is_customer() { amounts.value_rounded.neg() } else { amounts.value_rounded };
        if req.accumulate {
            calc::account_value_add(&mut splitinfo, account, value)?;
        } else {
            out.push(account, value, &entry.description, type_str);
        }
        if let Some(card) = p.card.filter(|_| entry.b_payment == PaymentType::Card) {
            total = total.sub(value, scu, EXACT_HALF_UP)?;
            out.push(card, value.neg(), &entry.description, type_str);
        }
    }
    calc::account_values_add_list(&mut splitinfo, &taxes)?;
    splitinfo.iter().for_each(|(a, v)| out.push(*a, *v, &req.memo, type_str));
    if let Some(card) = p.card.filter(|_| !p.inv.charge_amount.is_zero()) {
        let charge = if p.kind.credit_note { p.inv.charge_amount.neg() } else { p.inv.charge_amount };
        out.push(card, charge.neg(), EXTRA_CHARGE_MEMO, type_str);
        total = total.sub(charge, scu, EXACT_HALF_UP)?;
    }
    out.push(req.account, total.neg(), &req.memo, type_str);
    Ok(out)
}

/// Give splits to accounts in another commodity their amount: `value / price` (`gncInvoicePostAddSplit`,
/// `5.17:libgnucash/engine/gncInvoice.c:1408-1436`).
fn convert_foreign(book: &Book, currency: &CommodityRef, prices: &[(CommodityRef, Numeric)], s: &mut NewSplit) -> DomainResult<()> {
    let acc = book.account(s.account).ok_or_else(|| DomainError::not_found("account", &s.account))?;
    let Some(commodity) = acc.commodity.as_ref().filter(|c| *c != currency) else { return Ok(()) };
    let price = prices.iter().find(|(c, _)| c == commodity).map(|(_, p)| *p);
    let price = price.ok_or_else(|| DomainError::invalid(format!("account {} is in {commodity}: give a price for it", acc.name)))?;
    if price.is_zero() || price.is_negative() {
        return Err(DomainError::invalid(format!("the price of {commodity} must be positive")));
    }
    s.amount = Some(s.value.div(price, DENOM_AUTO, How::round(Round::HalfUp))?);
    Ok(())
}

/// The due date: as given, else from the billing term (the GUI's choice, `5.17:gnucash/gnome/dialog-date-close.c:94-98`),
/// else midnight at the start of the posting date, which the GUI's date field gives.
fn due_time(book: &Book, req: &PostRequest, inv: &Invoice) -> DomainResult<Time64> {
    if let Some(due) = req.due {
        return Ok(due);
    }
    match inv.terms.and_then(|g| book.business.bill_terms.get(&g)) {
        Some(term) => calc::due_date(Some(term), req.post_date.neutral_time()),
        None => Ok(req.post_date.start_utc()),
    }
}

/// `gncInvoicePostToAccount` (`5.17:libgnucash/engine/gncInvoice.c:1442-1731`).
pub fn post(book: &mut Book, env: &mut dyn Env, req: &PostRequest) -> DomainResult<Posted> {
    let p = plan(book, req)?;
    let due = due_time(book, req, &p.inv)?;
    let mut splits = plan_splits(book, req, &p)?;
    splits.splits.iter_mut().try_for_each(|s| convert_foreign(book, &p.currency, &req.prices, s))?;
    let lot = env.new_guid();
    let posted_split = splits.splits.len() - 1;
    splits.splits[posted_split].lot = Some(lot);
    let built = splits.splits.into_iter().map(|s| build_split(book, env, &p.currency, s)).collect::<DomainResult<Vec<_>>>()?;
    let txn = posted_transaction(book, env, req, &p, due, built)?;
    let mut lot_slots_frame = Frame::new();
    lot_slots_frame.set(lot_slots::INVOICE, SlotValue::Guid(p.inv.guid));
    lot_slots_frame.set(lot_slots::TITLE, SlotValue::String(format!("{} {}", p.kind.type_string(), p.inv.id)));
    book.lots.insert(lot, Arc::new(Lot { guid: lot, account: req.account, slots: lot_slots_frame }));
    let txn_guid = txn.guid;
    let date_posted = txn.date_posted;
    book.transactions.insert(txn_guid, Arc::new(txn));
    freeze(book, env, &p)?;
    let inv = book.business.invoices.get_mut(&req.invoice).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("invoice", &req.invoice))?;
    inv.post_txn = Some(txn_guid);
    inv.post_lot = Some(lot);
    inv.post_acc = Some(req.account);
    inv.posted = Some(date_posted);
    if req.autopay {
        payments::auto_apply_invoice(book, env, req.invoice)?;
    }
    Ok(Posted { transaction: txn_guid, lot, due })
}

/// The posted transaction with GnuCash's strings and slots (`5.17:libgnucash/engine/gncInvoice.c:1495-1515`, `:1705-1716`). Its num is the document
/// ID, or the type word with the num field source on (`gnc_set_num_action`, `:1504`).
fn posted_transaction(book: &Book, env: &mut dyn Env, req: &PostRequest, p: &Plan, due: Time64, splits: Vec<crate::Split>) -> DomainResult<Transaction> {
    let mut t = Transaction {
        guid: env.new_guid(),
        currency: p.currency.clone(),
        num: Some(num_action_targets(book, &p.inv.id, p.kind.type_string()).0).filter(|n| !n.is_empty()).map(str::to_owned),
        date_posted: req.post_date.neutral_time(),
        date_entered: env.now(),
        description: Some(contacts::owner_name(book, p.kind.end_owner).unwrap_or_default()),
        slots: Frame::new(),
        splits,
    };
    set_date_posted(&mut t, req.post_date);
    t.slots.set(txn_slots::INVOICE, SlotValue::Guid(p.inv.guid));
    t.slots.set(DATE_DUE, SlotValue::Timespec(due));
    t.slots.set(txn_slots::READ_ONLY, SlotValue::String(READ_ONLY_REASON.to_owned()));
    t.slots.set(txn_slots::TXN_TYPE, SlotValue::String(TXN_TYPE_INVOICE.to_owned()));
    sort_splits(&mut t);
    check_balanced(&t)?;
    Ok(t)
}

// ---------------------------------------------------------------------------------------------------------------
// Freezing tax tables and billing terms

/// Freeze the document's terms and each entry's tax table, and copy billable bill lines' net prices to their
/// invoice side (`5.17:libgnucash/engine/gncInvoice.c:1468-1472`, `:1544-1560`).
fn freeze(book: &mut Book, env: &mut dyn Env, p: &Plan) -> DomainResult<()> {
    if let Some(terms) = p.inv.terms {
        let child = term_return_child(book, env, terms)?;
        set_invoice_terms(book, p.inv.guid, Some(child))?;
    }
    p.entries.iter().try_for_each(|e| freeze_entry(book, env, &p.kind, e.guid))
}

fn freeze_entry(book: &mut Book, env: &mut dyn Env, kind: &InvoiceType, entry: Guid) -> DomainResult<()> {
    let e = book.business.entries.get(&entry).cloned().ok_or_else(|| DomainError::not_found("entry", &entry))?;
    let customer = kind.is_customer();
    let table = if customer { e.i_tax_table } else { e.b_tax_table };
    if let Some(t) = table {
        let child = table_return_child(book, env, t)?;
        set_entry_table(book, entry, customer, Some(child))?;
    }
    if !customer && e.billable {
        let current = book.business.entries.get(&entry).cloned().ok_or_else(|| DomainError::not_found("entry", &entry))?;
        let net = calc::entry_net_price(book, &current, false)?;
        let e = entry_mut(book, entry)?;
        e.i_price = net;
        e.i_tax_included = false;
    }
    Ok(())
}

fn entry_mut(book: &mut Book, entry: Guid) -> DomainResult<&mut Entry> {
    book.business.entries.get_mut(&entry).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("entry", &entry))
}

fn table_mut(book: &mut Book, table: Guid) -> DomainResult<&mut TaxTable> {
    book.business.tax_tables.get_mut(&table).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("tax table", &table))
}

fn term_mut(book: &mut Book, term: Guid) -> DomainResult<&mut BillTerm> {
    book.business.bill_terms.get_mut(&term).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("billing term", &term))
}

/// `gncTaxTableIncRef`/`DecRef`: no-ops on children and invisible tables; never below zero
/// (`5.17:libgnucash/engine/gncTaxTable.c:514-535`).
fn table_adjust_ref(book: &mut Book, table: Guid, delta: i64) -> DomainResult<()> {
    let t = table_mut(book, table)?;
    if t.parent.is_none() && !t.invisible && (delta > 0 || t.refcount > 0) {
        t.refcount += delta;
    }
    Ok(())
}

/// `gncBillTermIncRef`/`DecRef` (`5.17:libgnucash/engine/gncBillTerm.c:420-439`).
fn term_adjust_ref(book: &mut Book, term: Guid, delta: i64) -> DomainResult<()> {
    let t = term_mut(book, term)?;
    if t.parent.is_none() && !t.invisible && (delta > 0 || t.refcount > 0) {
        t.refcount += delta;
    }
    Ok(())
}

/// `gncEntrySetInvTaxTable` / `gncEntrySetBillTaxTable` (`5.17:libgnucash/engine/gncEntry.c:630-680`).
fn set_entry_table(book: &mut Book, entry: Guid, customer: bool, table: Option<Guid>) -> DomainResult<()> {
    let e = entry_mut(book, entry)?;
    let slot = if customer { &mut e.i_tax_table } else { &mut e.b_tax_table };
    if *slot == table {
        return Ok(());
    }
    let old = std::mem::replace(slot, table);
    old.map_or(Ok(()), |o| table_adjust_ref(book, o, -1))?;
    table.map_or(Ok(()), |t| table_adjust_ref(book, t, 1))
}

/// `gncInvoiceSetTerms` (`5.17:libgnucash/engine/gncInvoice.c:514-526`).
fn set_invoice_terms(book: &mut Book, invoice: Guid, terms: Option<Guid>) -> DomainResult<()> {
    let inv = book.business.invoices.get_mut(&invoice).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("invoice", &invoice))?;
    if inv.terms == terms {
        return Ok(());
    }
    let old = std::mem::replace(&mut inv.terms, terms);
    old.map_or(Ok(()), |o| term_adjust_ref(book, o, -1))?;
    terms.map_or(Ok(()), |t| term_adjust_ref(book, t, 1))
}

/// `gncTaxTableReturnChild(table, TRUE)`: the table's frozen copy, made on first use (`5.17:libgnucash/engine/gncTaxTable.c:749-762`).
/// The copy has a new GUID, the same name and lines, refcount 0, and is invisible.
fn table_return_child(book: &mut Book, env: &mut dyn Env, table: Guid) -> DomainResult<Guid> {
    let t = book.business.tax_tables.get(&table).cloned().ok_or_else(|| DomainError::not_found("tax table", &table))?;
    if let Some(child) = t.child {
        return Ok(child);
    }
    if t.parent.is_some() || t.invisible {
        return Ok(table);
    }
    let lines = calc::sorted_tax_lines(book, &t).into_iter().cloned().collect();
    let child = TaxTable { guid: env.new_guid(), name: t.name.clone(), refcount: 0, invisible: true, child: None, parent: Some(table), entries: lines, slots: Frame::new() };
    let g = child.guid;
    book.business.tax_tables.insert(g, Arc::new(child));
    table_mut(book, table)?.child = Some(g);
    Ok(g)
}

/// `gncBillTermReturnChild(term, TRUE)` (`5.17:libgnucash/engine/gncBillTerm.c:571-609`).
fn term_return_child(book: &mut Book, env: &mut dyn Env, term: Guid) -> DomainResult<Guid> {
    let t = book.business.bill_terms.get(&term).cloned().ok_or_else(|| DomainError::not_found("billing term", &term))?;
    if let Some(child) = t.child {
        return Ok(child);
    }
    if t.parent.is_some() || t.invisible {
        return Ok(term);
    }
    let child = BillTerm {
        guid: env.new_guid(),
        name: t.name.clone(),
        description: t.description.clone(),
        refcount: 0,
        invisible: true,
        slots: Frame::new(),
        child: None,
        parent: Some(term),
        kind: t.kind,
    };
    let g = child.guid;
    book.business.bill_terms.insert(g, Arc::new(child));
    term_mut(book, term)?.child = Some(g);
    Ok(g)
}

// ---------------------------------------------------------------------------------------------------------------
// Unpost

/// `gncInvoiceUnpost(invoice, reset_tax_tables)` (`5.17:libgnucash/engine/gncInvoice.c:1735-1858`): delete the posted
/// transaction; keep the lot as a pre-payment lot of the document's owner if payments remain in it; delete lot
/// links to other documents and re-apply what they linked; clear the posted state; with `reset_tax_tables`, point
/// entries back at the visible parent tax tables. The frozen billing term stays.
pub fn unpost(book: &mut Book, env: &mut dyn Env, invoice: Guid, reset_tax_tables: bool) -> DomainResult<()> {
    let inv = book.business.invoices.get(&invoice).cloned().ok_or_else(|| DomainError::not_found("invoice", &invoice))?;
    let (Some(txn), Some(lot)) = (inv.post_txn, inv.post_lot) else {
        return Err(DomainError::conflict(format!("document {} is not posted", inv.id)));
    };
    if book.lot(lot).is_none() {
        return Err(DomainError::invalid(format!("the lot of document {} is missing", inv.id)));
    }
    let mut work = LotWork::default();
    payments::destroy_transaction(book, &mut work, txn);
    let l = book.lots.get_mut(&lot).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("lot", &lot))?;
    l.slots.remove(lot_slots::INVOICE);
    payments::attach_owner(&mut l.slots, inv.owner);
    relink_remaining(book, env, &mut work, lot, &inv)?;
    if work.count(book, lot) == 0 {
        book.lots.shift_remove(&lot);
    }
    work.drop_empty_lots(book);
    let i = book.business.invoices.get_mut(&invoice).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("invoice", &invoice))?;
    i.post_acc = None;
    i.post_txn = None;
    i.post_lot = None;
    i.posted = None;
    if reset_tax_tables {
        reset_tables(book, &inv)?;
    }
    Ok(())
}

/// Remove each lot-link transaction left in the lot and re-apply the lots it linked to each other
/// (`5.17:libgnucash/engine/gncInvoice.c:1775-1828`).
fn relink_remaining(book: &mut Book, env: &mut dyn Env, work: &mut LotWork, lot: Guid, inv: &Invoice) -> DomainResult<()> {
    for split in work.splits(book, lot).clone() {
        let Some((t, _)) = payments::find_split(book, split) else { continue };
        if t.txn_type() != Some('L') {
            continue;
        }
        let txn = t.guid;
        let mut others: Vec<Option<Guid>> = t.splits.iter().filter(|s| s.lot != Some(lot)).map(|s| s.lot).collect();
        payments::destroy_transaction(book, work, txn);
        payments::auto_apply_with_lots(book, env, work, inv.owner, &mut others)?;
        others.into_iter().flatten().filter(|l| work.count(book, *l) == 0).collect::<Vec<_>>().into_iter().for_each(|l| {
            book.lots.shift_remove(&l);
        });
    }
    Ok(())
}

/// Point each entry's tax table at its parent (`5.17:libgnucash/engine/gncInvoice.c:1841-1855`).
fn reset_tables(book: &mut Book, inv: &Invoice) -> DomainResult<()> {
    let kind = calc::invoice_type(book, inv)?;
    let entries: Vec<(Guid, Option<Guid>)> = documents::entries_of(book, inv.guid)
        .into_iter()
        .map(|e| (e.guid, if kind.is_customer() { e.i_tax_table } else { e.b_tax_table }))
        .collect();
    entries.into_iter().try_for_each(|(entry, table)| {
        let parent = table.and_then(|t| book.business.tax_tables.get(&t)).and_then(|t| t.parent);
        set_entry_table(book, entry, kind.is_customer(), parent)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::{AmountType, Customer, OwnerRef, TaxIncluded, TaxTableEntry};
    use crate::ops::business::payments::{Payment, apply_owner_payment};
    use crate::{Account, FixedEnv};

    struct Fixture {
        book: Book,
        env: FixedEnv,
        ar: Guid,
        bank: Guid,
        sales: Guid,
        gst: Guid,
        table: Guid,
        customer: OwnerRef,
    }

    fn fixture() -> Fixture {
        let mut env = FixedEnv::new(Time64(1_700_000_000), 0);
        let mut book = Book::new_empty(&mut env);
        let usd = CommodityRef::currency("USD");
        let root = book.root;
        let mut add = |book: &mut Book, name: &str, ty| {
            let a = Account::new(env.new_guid(), name, ty, Some(usd.clone()), Some(root));
            let g = a.guid;
            book.accounts.insert(g, Arc::new(a));
            g
        };
        let ar = add(&mut book, "AR", AccountType::Receivable);
        let bank = add(&mut book, "Bank", AccountType::Bank);
        let sales = add(&mut book, "Sales", AccountType::Income);
        let gst = add(&mut book, "GST", AccountType::Liability);
        let table = TaxTable {
            guid: env.new_guid(),
            name: "GST".into(),
            refcount: 0,
            invisible: false,
            child: None,
            parent: None,
            entries: vec![TaxTableEntry { account: Some(gst), amount: Numeric::from_int(5), kind: AmountType::Percent }],
            slots: Frame::new(),
        };
        let table_guid = table.guid;
        book.business.tax_tables.insert(table_guid, Arc::new(table));
        let customer = Customer {
            guid: env.new_guid(),
            name: "Acme".into(),
            id: "C1".into(),
            addr: crate::business::Address::default(),
            ship_addr: crate::business::Address::default(),
            notes: String::new(),
            terms: None,
            tax_included: TaxIncluded::UseGlobal,
            active: true,
            discount: Numeric::ZERO,
            credit: Numeric::ZERO,
            currency: Some(usd),
            use_tax_table: false,
            tax_table: None,
            slots: Frame::new(),
        };
        let owner = OwnerRef { kind: OwnerKind::Customer, guid: customer.guid };
        book.business.customers.insert(customer.guid, Arc::new(customer));
        Fixture { book, env, ar, bank, sales, gst, table: table_guid, customer: owner }
    }

    /// An invoice with one entry per (quantity, price) pair, each taxed with the 5 % table.
    fn invoice(f: &mut Fixture, id: &str, lines: &[(Numeric, Numeric)]) -> Guid {
        let inv = Invoice {
            guid: f.env.new_guid(),
            id: id.into(),
            owner: f.customer,
            opened: Time64(0),
            posted: None,
            terms: None,
            billing_id: String::new(),
            notes: String::new(),
            active: true,
            post_txn: None,
            post_lot: None,
            post_acc: None,
            currency: Some(CommodityRef::currency("USD")),
            bill_to: None,
            charge_amount: Numeric::ZERO,
            slots: Frame::new(),
        };
        let g = inv.guid;
        f.book.business.invoices.insert(g, Arc::new(inv));
        lines.iter().enumerate().for_each(|(i, (q, p))| {
            let mut e = Entry::new(f.env.new_guid(), Time64(i64::try_from(i).unwrap_or(0)), Time64(0));
            e.invoice = Some(g);
            e.quantity = *q;
            e.i_price = *p;
            e.i_account = Some(f.sales);
            e.i_tax_table = Some(f.table);
            f.book.business.entries.insert(e.guid, Arc::new(e));
            Arc::make_mut(f.book.business.tax_tables.get_mut(&f.table).unwrap()).refcount += 1;
        });
        g
    }

    fn request(f: &Fixture, invoice: Guid) -> PostRequest {
        PostRequest {
            invoice,
            account: f.ar,
            post_date: Date::from_ymd(2024, 3, 5).unwrap(),
            due: None,
            memo: String::new(),
            accumulate: true,
            autopay: false,
            prices: Vec::new(),
        }
    }

    fn lot_balance(book: &Book, lot: Guid) -> Numeric {
        book.splits_in_lot(lot).fold(Numeric::ZERO, |a, (_, s)| a.add_exact(s.amount).unwrap())
    }

    #[test]
    fn post_freezes_the_tax_table_and_unpost_resets_it() {
        let mut f = fixture();
        let inv = invoice(&mut f, "I1", &[(Numeric::from_int(2), Numeric::new(1050, 100)), (Numeric::from_int(1), Numeric::new(333, 100))]);
        let req = request(&f, inv);
        let posted = post(&mut f.book, &mut f.env, &req).unwrap();
        let t = f.book.transaction(posted.transaction).unwrap();
        let accounts: Vec<Guid> = t.splits.iter().filter_map(|s| s.account).collect();
        assert_eq!(accounts, vec![f.ar, f.gst, f.sales], "A/R first, then tax, then income");
        assert_eq!(lot_balance(&f.book, posted.lot), Numeric::new(2555, 100), "24.33 + 5 % (1.2165 → 1.22)");
        let parent = &f.book.business.tax_tables[&f.table];
        let child = parent.child.unwrap();
        assert_eq!(parent.refcount, 0, "each entry dropped its reference to the parent");
        assert!(f.book.business.tax_tables[&child].invisible);
        assert!(f.book.business.entries.values().all(|e| e.i_tax_table == Some(child)));
        assert_eq!(posted.due, Date::from_ymd(2024, 3, 5).unwrap().start_utc());
        assert!(matches!(post(&mut f.book, &mut f.env, &req), Err(DomainError::Conflict(_))));

        unpost(&mut f.book, &mut f.env, inv, true).unwrap();
        assert!(f.book.transaction(posted.transaction).is_none());
        assert!(f.book.lot(posted.lot).is_none(), "an empty lot is removed");
        assert_eq!(f.book.business.tax_tables[&f.table].refcount, 2);
        assert!(f.book.business.entries.values().all(|e| e.i_tax_table == Some(f.table)));
        let again = post(&mut f.book, &mut f.env, &req).unwrap();
        assert_eq!(f.book.business.tax_tables[&f.table].child, Some(child), "the frozen copy is reused");
        assert_eq!(lot_balance(&f.book, again.lot), Numeric::new(2555, 100));
    }

    #[test]
    fn an_over_payment_leaves_a_prepayment_that_autopay_consumes() {
        let mut f = fixture();
        let first = invoice(&mut f, "I1", &[(Numeric::ONE, Numeric::from_int(100))]);
        let second = invoice(&mut f, "I2", &[(Numeric::ONE, Numeric::from_int(20))]);
        let req = request(&f, first);
        let posted = post(&mut f.book, &mut f.env, &req).unwrap();
        let pay = Payment {
            owner: f.customer,
            posted_account: f.ar,
            transfer_account: Some(f.bank),
            amount: Numeric::from_int(150),
            exchange_rate: Numeric::ONE,
            date: Time64(1_710_000_000),
            memo: String::new(),
            num: "1".into(),
        };
        let paid = apply_owner_payment(&mut f.book, &mut f.env, &pay, &[], true).unwrap();
        assert!(lot_balance(&f.book, posted.lot).is_zero());
        let prepay = paid.payment_lot.unwrap();
        assert_eq!(lot_balance(&f.book, prepay), Numeric::from_int(-45), "150 − 105");
        let mut second_req = request(&f, second);
        second_req.autopay = true;
        let posted2 = post(&mut f.book, &mut f.env, &second_req).unwrap();
        assert!(lot_balance(&f.book, posted2.lot).is_zero());
        assert_eq!(lot_balance(&f.book, prepay), Numeric::from_int(-24));
        assert!(f.book.transactions.values().all(|t| t.is_balanced()));
    }

    proptest::proptest! {
        /// Whatever the lines, the posted transaction balances and its A/R split is the document total.
        #[test]
        fn posting_balances_and_carries_the_total(lines in proptest::collection::vec((1_i64..1_000, 1_i64..100_000), 1..6), accumulate: bool) {
            let mut f = fixture();
            let lines: Vec<(Numeric, Numeric)> = lines.iter().map(|(q, p)| (Numeric::new(*q, 10), Numeric::new(*p, 100))).collect();
            let inv = invoice(&mut f, "P", &lines);
            let total = calc::document_totals(&f.book, &f.book.business.invoices[&inv]).unwrap().total;
            let mut req = request(&f, inv);
            req.accumulate = accumulate;
            let posted = post(&mut f.book, &mut f.env, &req).unwrap();
            proptest::prop_assert!(f.book.transaction(posted.transaction).unwrap().is_balanced());
            proptest::prop_assert_eq!(lot_balance(&f.book, posted.lot), total);
        }
    }
}
