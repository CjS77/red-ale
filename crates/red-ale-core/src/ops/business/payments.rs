//! Business operations: payments, and the lot linking that applies payments and opposite documents to each other.
//!
//! A port of the payment half of `5.17:libgnucash/engine/gncOwner.c` (`gncOwnerCreatePaymentLotSecs`,
//! `gncOwnerFindOffsettingSplit`, `gncOwnerReduceSplitTo`, `gncOwnerCreateLotLink`, `gncOwnerOffsetLots`,
//! `gncOwnerAutoApplyPaymentsWithLots`, `gncOwnerApplyPaymentSecs`), of `gncInvoiceApplyPayment` and
//! `gncInvoiceAutoApplyPayments` (`5.17:libgnucash/engine/gncInvoice.c:1860-1980`), and of the due-documents query
//! behind GnuCash's "bills due" reminder. See `design/notes/engine-business.md` §8 and §9.
//!
//! GnuCash keeps each lot's splits in a list in the order they joined it; the choice of split to move depends on
//! that order. [`LotWork`] keeps the same lists during one operation, starting from book order: the order GnuCash
//! itself has after loading the file.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::book_options::num_action_targets;
use crate::business::{Invoice, OwnerKind, OwnerRef};
use crate::lot::slot_paths as lot_slots;
use crate::ops::business::calc::{self, InvoiceType};
use crate::ops::business::contacts;
use crate::ops::commit::{currency_fraction, round_amount, round_value, sort_splits};
use crate::transaction::slot_paths as txn_slots;
use crate::{
    Book, CommodityRef, DENOM_AUTO, DenomMode, DomainError, DomainResult, Env, Frame, Guid, How, Lot, Numeric, ReconcileState, Round, SlotValue, Split,
    Time64, Transaction,
};

/// The split action GnuCash gives payment splits (`_("Payment")`, `5.17:libgnucash/engine/gncOwner.c:832`, `:856`).
pub const PAYMENT_ACTION: &str = "Payment";
/// The split action of lot-link splits (`_("Lot Link")`, `5.17:libgnucash/engine/gncOwner.c:1124`).
pub const LOT_LINK_ACTION: &str = "Lot Link";
/// The memo prefix of lot-link splits (`5.17:libgnucash/engine/gncOwner.c:1012`).
pub const LOT_LINK_MEMO_PREFIX: &str = "Offset between business items: ";
/// Transaction type of an invoice's posted transaction.
pub const TXN_TYPE_INVOICE: &str = "I";
/// Transaction type of a payment.
pub const TXN_TYPE_PAYMENT: &str = "P";
/// Transaction type of a lot link.
pub const TXN_TYPE_LINK: &str = "L";

// ---------------------------------------------------------------------------------------------------------------
// Lots as GnuCash sees them

/// The split lists of the lots one operation touches, in GnuCash's list order.
#[derive(Debug, Default)]
pub struct LotWork {
    lists: BTreeMap<Guid, Vec<Guid>>,
}

impl LotWork {
    /// A lot's splits; on first use, in book order.
    pub fn splits(&mut self, book: &Book, lot: Guid) -> &mut Vec<Guid> {
        self.lists.entry(lot).or_insert_with(|| book.splits_in_lot(lot).map(|(_, s)| s.guid).collect())
    }

    /// How many splits a lot has.
    pub fn count(&mut self, book: &Book, lot: Guid) -> usize {
        self.splits(book, lot).len()
    }

    /// `gnc_lot_get_balance`: Σ split amounts (`5.17:libgnucash/engine/gnc-lot.cpp:487-523`).
    pub fn balance(&mut self, book: &Book, lot: Guid) -> DomainResult<Numeric> {
        let ids = self.splits(book, lot).clone();
        Ok(Numeric::sum_fixed(ids.iter().filter_map(|g| find_split(book, *g)).map(|(_, s)| s.amount))?)
    }

    /// `gnc_lot_is_closed`: a lot with splits and a zero balance. An empty lot is open.
    pub fn is_closed(&mut self, book: &Book, lot: Guid) -> DomainResult<bool> {
        Ok(self.count(book, lot) > 0 && self.balance(book, lot)?.is_zero())
    }

    /// `gnc_lot_add_split`: move a split into a lot, at the end of its list (`5.17:libgnucash/engine/gnc-lot.cpp:578-621`).
    pub fn add_split(&mut self, book: &mut Book, lot: Guid, split: Guid) -> DomainResult<()> {
        let (txn, current) = find_split(book, split).map(|(t, s)| (t.guid, s.lot)).ok_or_else(|| DomainError::not_found("split", &split))?;
        if current == Some(lot) {
            return Ok(());
        }
        if let Some(old) = current {
            self.splits(book, old).retain(|g| *g != split);
        }
        self.splits(book, lot).push(split);
        split_mut(book, txn, split)?.lot = Some(lot);
        Ok(())
    }

    /// Forget a split that is being destroyed.
    fn remove_split(&mut self, book: &Book, split: &Split) {
        if let Some(lot) = split.lot {
            self.splits(book, lot).retain(|g| *g != split.guid);
        }
    }

    /// `gnc_lot_get_latest_split`, which also sorts the lot's list by posted date as a side effect
    /// (`5.17:libgnucash/engine/gnc-lot.cpp:668-683`).
    fn latest_split_date(&mut self, book: &Book, lot: Guid) -> Time64 {
        let list = self.splits(book, lot);
        let date = |g: &Guid| find_split(book, *g).map_or(Time64(0), |(t, _)| t.date_posted);
        list.sort_by_key(date);
        list.last().map_or(Time64(0), date)
    }

    /// Remove every touched lot left without splits: GnuCash takes an emptied lot out of its account, so it is not
    /// saved (`5.17:libgnucash/engine/gnc-lot.cpp:636-641`).
    pub fn drop_empty_lots(self, book: &mut Book) {
        self.lists.iter().filter(|(_, splits)| splits.is_empty()).for_each(|(lot, _)| {
            book.lots.shift_remove(lot);
        });
    }
}

/// A split and the transaction holding it.
pub fn find_split(book: &Book, split: Guid) -> Option<(&Transaction, &Split)> {
    book.transactions.values().find_map(|t| t.split(split).map(|s| (t.as_ref(), s)))
}

fn txn_mut(book: &mut Book, txn: Guid) -> DomainResult<&mut Transaction> {
    book.transactions.get_mut(&txn).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("transaction", &txn))
}

fn split_mut(book: &mut Book, txn: Guid, split: Guid) -> DomainResult<&mut Split> {
    txn_mut(book, txn)?.split_mut(split).ok_or_else(|| DomainError::not_found("split", &split))
}

/// `gncInvoiceGetInvoiceFromLot`: the invoice a lot's `gncInvoice/invoice-guid` slot names, if it exists.
pub fn lot_invoice(book: &Book, lot: Guid) -> Option<&Invoice> {
    let guid = book.lot(lot)?.slots.get(lot_slots::INVOICE).and_then(SlotValue::as_guid)?;
    book.business.invoices.get(&guid).map(AsRef::as_ref)
}

/// `gncInvoiceGetInvoiceFromTxn`: the invoice a transaction's `gncInvoice/invoice-guid` slot names.
fn txn_invoice<'a>(book: &'a Book, txn: &Transaction) -> Option<&'a Invoice> {
    let guid = txn.slots.get(txn_slots::INVOICE).and_then(SlotValue::as_guid)?;
    book.business.invoices.get(&guid).map(AsRef::as_ref)
}

/// `gncOwnerGetOwnerFromLot`: the owner in a lot's `gncOwner` slots, if it exists (`5.17:libgnucash/engine/gncOwner.c:637-670`).
pub fn lot_slot_owner(book: &Book, lot: Guid) -> Option<OwnerRef> {
    let slots = &book.lot(lot)?.slots;
    let kind = OwnerKind::from_owner_type_number(slots.get(lot_slots::OWNER_TYPE).and_then(SlotValue::as_int)?)?;
    let owner = OwnerRef { kind, guid: slots.get(lot_slots::OWNER_GUID).and_then(SlotValue::as_guid)? };
    contacts::owner_exists(book, owner).then_some(owner)
}

/// The end owner of a document lot (through its invoice) or of a pre-payment lot (through its owner slots), as
/// `gncOwnerLotMatchOwnerFunc` finds it (`5.17:libgnucash/engine/gncOwner.c:705-722`).
fn lot_end_owner_invoice_first(book: &Book, lot: Guid) -> Option<OwnerRef> {
    match lot_invoice(book, lot) {
        Some(inv) => contacts::end_owner(book, inv.owner),
        None => lot_slot_owner(book, lot).and_then(|o| contacts::end_owner(book, o)),
    }
}

/// The same, owner slots first, as `gnc_lot_match_owner_balancing` looks (`5.17:libgnucash/engine/gncInvoice.c:1866-1890`).
fn lot_end_owner_slots_first(book: &Book, lot: Guid) -> Option<OwnerRef> {
    match lot_slot_owner(book, lot) {
        Some(o) => contacts::end_owner(book, o),
        None => lot_invoice(book, lot).and_then(|inv| contacts::end_owner(book, inv.owner)),
    }
}

/// `gncOwnerAttachToLot`: write the owner slots (`5.17:libgnucash/engine/gncOwner.c:621-633`).
pub fn attach_owner(slots: &mut Frame, owner: OwnerRef) {
    slots.set(lot_slots::OWNER_TYPE, SlotValue::Integer(owner.kind.owner_type_number()));
    slots.set(lot_slots::OWNER_GUID, SlotValue::Guid(owner.guid));
}

/// `xaccAccountFindOpenLots` without a sort: the account's open lots in the order they joined it
/// (`5.17:libgnucash/engine/Account.cpp:3970-4001`), filtered by `keep`.
fn open_lots(book: &Book, work: &mut LotWork, account: Guid, keep: impl Fn(&mut LotWork, Guid) -> DomainResult<bool>) -> DomainResult<Vec<Guid>> {
    let lots: Vec<Guid> = book.lots.values().filter(|l| l.account == account).map(|l| l.guid).collect();
    lots.into_iter()
        .filter_map(|l| match work.is_closed(book, l) {
            Ok(true) => None,
            Ok(false) => match keep(work, l) {
                Ok(true) => Some(Ok(l)),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            },
            Err(e) => Some(Err(e)),
        })
        .collect()
}

/// The end owner's name for descriptions (`5.17:libgnucash/engine/gncOwner.c:771`).
fn end_owner_name(book: &Book, owner: OwnerRef) -> Option<String> {
    contacts::end_owner(book, owner).and_then(|o| contacts::owner_name(book, o))
}

/// "Type ID" as GnuCash titles a document (`5.17:libgnucash/engine/gncInvoice.c:1491`).
pub fn document_title(book: &Book, invoice: &Invoice) -> DomainResult<String> {
    Ok(format!("{} {}", calc::invoice_type(book, invoice)?.type_string(), invoice.id))
}

// ---------------------------------------------------------------------------------------------------------------
// Payment transaction

/// A payment to record.
#[derive(Debug, Clone, PartialEq)]
pub struct Payment {
    /// Who pays or is paid.
    pub owner: OwnerRef,
    /// The A/R or A/P account the documents are posted to.
    pub posted_account: Guid,
    /// Where the money goes or comes from. Needed unless `amount` is zero.
    pub transfer_account: Option<Guid>,
    /// The engine amount, in the posted account's commodity: positive is money into the transfer account. The
    /// A/R or A/P split gets the negation.
    pub amount: Numeric,
    /// Units of the transfer account's commodity per unit of the posted account's commodity.
    pub exchange_rate: Numeric,
    /// The transaction's posted time (not normalised: no `date-posted` slot is written).
    pub date: Time64,
    /// Memo of both splits.
    pub memo: String,
    /// Transaction number.
    pub num: String,
}

/// What a payment created.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PaymentResult {
    /// The payment transaction, when the amount was not zero.
    pub transaction: Option<Guid>,
    /// The payment lot, if it still holds a split (an over-payment).
    pub payment_lot: Option<Guid>,
}

fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_owned())
}

/// Validate the accounts of a payment, returning (posted commodity, transfer account, transfer commodity).
fn payment_accounts(book: &Book, p: &Payment) -> DomainResult<(CommodityRef, Guid, CommodityRef)> {
    let commodity = |g: Guid| {
        let acc = book.account(g).ok_or_else(|| DomainError::not_found("account", &g))?;
        acc.commodity.clone().ok_or_else(|| DomainError::invalid(format!("account {} has no commodity", acc.name)))
    };
    let post_comm = commodity(p.posted_account)?;
    let xfer = p.transfer_account.ok_or_else(|| DomainError::invalid("a payment with an amount needs a transfer account"))?;
    let xfer_comm = commodity(xfer)?;
    if !xfer_comm.is_currency() {
        return Err(DomainError::invalid(format!("the transfer account's commodity {xfer_comm} is not a currency")));
    }
    if post_comm != xfer_comm && (p.exchange_rate.is_zero() || p.exchange_rate.is_negative()) {
        return Err(DomainError::invalid("the accounts' commodities differ, so a positive exchange rate is needed"));
    }
    Ok((post_comm, xfer, xfer_comm))
}

/// `gncOwnerCreatePaymentLotSecs` for a new transaction (`5.17:libgnucash/engine/gncOwner.c:749-880`): the
/// transfer split and the A/R or A/P split, the latter in a new lot carrying the owner.
fn create_payment_lot(book: &mut Book, env: &mut dyn Env, work: &mut LotWork, p: &Payment) -> DomainResult<(Guid, Guid)> {
    let (post_comm, xfer, xfer_comm) = payment_accounts(book, p)?;
    // A new transaction has no currency, so it always takes the transfer account's commodity.
    let currency = xfer_comm.clone();
    let converted = || p.amount.mul(p.exchange_rate, DENOM_AUTO, How::round(Round::HalfUp));
    let xfer_amount = if post_comm == xfer_comm { p.amount } else { converted()? };
    let value = if post_comm == currency { p.amount } else { converted()? };
    let split = |book: &Book, env: &mut dyn Env, account: Guid, amount: Numeric, value: Numeric, action: &str| -> DomainResult<Split> {
        let mut s = Split::new(env.new_guid(), account, round_value(book, &currency, value)?, round_amount(book, account, amount)?);
        s.memo = non_empty(&p.memo);
        s.action = non_empty(action);
        Ok(s)
    };
    // The transfer split's action is the payment's num with the num field source on (`gncOwner.c:836`); the A/R or
    // A/P split's is always "Payment" (`:859`).
    let xfer_action = num_action_targets(book, &p.num, PAYMENT_ACTION).1.to_owned();
    let xfer_split = split(book, env, xfer, xfer_amount, value, &xfer_action)?;
    let post_split = split(book, env, p.posted_account, p.amount.neg(), value.neg(), PAYMENT_ACTION)?;
    let lot_guid = env.new_guid();
    let mut lot_frame = Frame::new();
    attach_owner(&mut lot_frame, p.owner);
    book.lots.insert(lot_guid, Arc::new(Lot { guid: lot_guid, account: p.posted_account, slots: lot_frame }));
    let post_guid = post_split.guid;
    let mut t = Transaction {
        guid: env.new_guid(),
        currency,
        num: non_empty(&p.num),
        date_posted: p.date,
        date_entered: env.now(),
        description: Some(end_owner_name(book, p.owner).unwrap_or_default()),
        slots: Frame::new(),
        splits: vec![xfer_split, post_split],
    };
    t.slots.set(txn_slots::TXN_TYPE, SlotValue::String(TXN_TYPE_PAYMENT.to_owned()));
    sort_splits(&mut t);
    let txn = t.guid;
    book.transactions.insert(txn, Arc::new(t));
    work.add_split(book, lot_guid, post_guid)?;
    Ok((txn, lot_guid))
}

// ---------------------------------------------------------------------------------------------------------------
// Offsetting

/// `gncOwnerFindOffsettingSplit` (`5.17:libgnucash/engine/gncOwner.c:896-951`). Taken literally: a later split with
/// better flags but a smaller amount does not replace the best so far.
fn find_offsetting_split(book: &Book, work: &mut LotWork, lot: Guid, target: Numeric) -> Option<Guid> {
    let mut best: Option<Guid> = None;
    let (mut best_amt, mut best_flags) = (Numeric::ZERO, 0);
    for g in work.splits(book, lot).clone() {
        let Some((txn, split)) = find_split(book, g) else { continue };
        if target.is_positive() == split.amount.is_positive() {
            continue;
        }
        let flags = match split.amount.abs().cmp_value(target.abs()) {
            std::cmp::Ordering::Equal => 8,
            std::cmp::Ordering::Greater => 4,
            std::cmp::Ordering::Less => 2,
        } + i32::from(txn.txn_type() != Some('L'));
        if flags >= best_flags && split.amount.abs().cmp_value(best_amt.abs()).is_gt() {
            (best, best_flags, best_amt) = (Some(g), flags, split.amount);
        }
    }
    best
}

/// `gncOwnerReduceSplitTo`: shrink a split to `target` and put the rest in a new split in the same transaction and
/// lot (`5.17:libgnucash/engine/gncOwner.c:958-1005`).
fn reduce_split_to(book: &mut Book, env: &mut dyn Env, work: &mut LotWork, split: Guid, target: Numeric) -> DomainResult<()> {
    let (txn, s) = find_split(book, split).map(|(t, s)| (t.guid, s.clone())).ok_or_else(|| DomainError::not_found("split", &split))?;
    if s.amount.is_positive() != target.is_positive() || s.amount.same_value(target) || s.amount.is_zero() {
        return Ok(());
    }
    let currency = book.transaction(txn).map(|t| t.currency.clone()).ok_or_else(|| DomainError::not_found("transaction", &txn))?;
    let account = s.account.ok_or_else(|| DomainError::invalid("a payment split has no account"))?;
    let exch = s.value.div(s.amount, DENOM_AUTO, How::new(DenomMode::Lcd, Round::Truncate))?;
    let target_val = target.mul(exch, currency_fraction(book, &currency)?, How::round(Round::HalfUp))?;
    let rem_amt = s.amount.sub_fixed(target)?;
    let rem_val = s.value.sub_fixed(target_val)?;
    let (amount, value) = (round_amount(book, account, target)?, round_value(book, &currency, target_val)?);
    let mut rem = Split::new(env.new_guid(), account, round_value(book, &currency, rem_val)?, round_amount(book, account, rem_amt)?);
    rem.memo.clone_from(&s.memo);
    rem.action.clone_from(&s.action);
    let rem_guid = rem.guid;
    let t = txn_mut(book, txn)?;
    if let Some(reduced) = t.split_mut(split) {
        reduced.amount = amount;
        reduced.value = value;
    }
    t.splits.push(rem);
    sort_splits(t);
    match s.lot {
        Some(lot) => work.add_split(book, lot, rem_guid),
        None => Ok(()),
    }
}

/// `gncOwnerOffsetLots`: move (part of) a split from a payment lot to close `to` (`5.17:libgnucash/engine/gncOwner.c:1221-1251`).
fn offset_lots(book: &mut Book, env: &mut dyn Env, work: &mut LotWork, from: Guid, to: Guid) -> DomainResult<()> {
    if lot_invoice(book, from).is_some() {
        return Ok(());
    }
    let target = work.balance(book, to)?;
    if target.is_zero() {
        return Ok(());
    }
    let Some(split) = find_offsetting_split(book, work, from, target) else { return Ok(()) };
    let amount = find_split(book, split).map_or(Numeric::ZERO, |(_, s)| s.amount);
    if amount.abs().cmp_value(target.abs()).is_gt() {
        reduce_split_to(book, env, work, split, target.neg())?;
    }
    work.add_split(book, to, split)
}

// ---------------------------------------------------------------------------------------------------------------
// Lot links

/// `get_ll_transaction_from_lot`, including its test of the wrong lot, which makes any lot-link transaction in the
/// lot that reaches another lot qualify (`5.17:libgnucash/engine/gncOwner.c:1077-1117`).
fn existing_link(book: &Book, work: &mut LotWork, lot: Guid) -> Option<Guid> {
    lot_invoice(book, lot)?;
    work.splits(book, lot).clone().into_iter().find_map(|g| {
        let (txn, _) = find_split(book, g)?;
        (txn.txn_type() == Some('L') && txn.splits.iter().any(|s| s.lot.is_some_and(|l| l != lot))).then_some(txn.guid)
    })
}

/// A lot-link split: base value `value` in the account's commodity, which is also the transaction currency.
fn link_split(book: &Book, env: &mut dyn Env, account: Guid, currency: &CommodityRef, value: Numeric) -> DomainResult<Split> {
    let mut s = Split::new(env.new_guid(), account, round_value(book, currency, value)?, round_amount(book, account, value)?);
    s.action = Some(LOT_LINK_ACTION.to_owned());
    Ok(s)
}

/// `gncOwnerCreateLotLink` (`5.17:libgnucash/engine/gncOwner.c:1121-1218`): a transaction moving the smaller of two
/// opposite document balances between their lots.
fn create_lot_link(book: &mut Book, env: &mut dyn Env, work: &mut LotWork, from: Guid, to: Guid, owner_name: Option<&str>) -> DomainResult<()> {
    if lot_invoice(book, from).is_none() || lot_invoice(book, to).is_none() {
        return Ok(());
    }
    let posted = work.latest_split_date(book, from).max(work.latest_split_date(book, to));
    let (mut from_bal, mut to_bal) = (work.balance(book, from)?, work.balance(book, to)?);
    if from_bal.abs().cmp_value(to_bal.abs()).is_gt() {
        from_bal = to_bal.neg();
    } else {
        to_bal = from_bal.neg();
    }
    let account = book.lot(from).map(|l| l.account).ok_or_else(|| DomainError::not_found("lot", &from))?;
    let currency = book.account(account).and_then(|a| a.commodity.clone()).ok_or_else(|| DomainError::invalid("the A/R or A/P account has no commodity"))?;
    let existing = existing_link(book, work, from).or_else(|| existing_link(book, work, to));
    let txn = match existing {
        Some(g) => {
            let t = txn_mut(book, g)?;
            t.date_posted = t.date_posted.max(posted);
            g
        }
        None => new_link_txn(book, env, currency.clone(), posted, owner_name),
    };
    let from_split = link_split(book, env, account, &currency, from_bal.neg())?;
    let to_split = link_split(book, env, account, &currency, to_bal.neg())?;
    let (fg, tg) = (from_split.guid, to_split.guid);
    let t = txn_mut(book, txn)?;
    t.splits.extend([from_split, to_split]);
    sort_splits(t);
    work.add_split(book, from, fg)?;
    work.add_split(book, to, tg)?;
    merge_lot_subsplits(book, work, to)?;
    merge_lot_subsplits(book, work, from)?;
    set_lot_link_memo(book, txn)
}

fn new_link_txn(book: &mut Book, env: &mut dyn Env, currency: CommodityRef, posted: Time64, owner_name: Option<&str>) -> Guid {
    let mut t = Transaction {
        guid: env.new_guid(),
        currency,
        num: None,
        date_posted: posted,
        date_entered: env.now(),
        description: Some(owner_name.unwrap_or("(Unknown)").to_owned()),
        slots: Frame::new(),
        splits: Vec::new(),
    };
    t.slots.set(txn_slots::TXN_TYPE, SlotValue::String(TXN_TYPE_LINK.to_owned()));
    let g = t.guid;
    book.transactions.insert(g, Arc::new(t));
    g
}

/// `xaccScrubMergeLotSubSplits(lot, FALSE)`: merge splits of one transaction that sit in the same lot
/// (`5.17:libgnucash/engine/Scrub2.cpp:329-409`).
fn merge_lot_subsplits(book: &mut Book, work: &mut LotWork, lot: Guid) -> DomainResult<()> {
    while let Some(split) = work.splits(book, lot).clone().into_iter().find(|g| mergeable_partner(book, *g).is_some()) {
        while let Some(other) = mergeable_partner(book, split) {
            merge_splits(book, work, split, other)?;
        }
    }
    Ok(())
}

/// Another split of the same transaction in the same lot, unless the transaction belongs to an invoice.
fn mergeable_partner(book: &Book, split: Guid) -> Option<Guid> {
    let (txn, s) = find_split(book, split)?;
    if txn_invoice(book, txn).is_some() {
        return None;
    }
    txn.splits.iter().find(|o| o.guid != split && o.lot == s.lot && s.lot.is_some()).map(|o| o.guid)
}

/// `merge_splits`: add `sb` into `sa` and destroy `sb` (`5.17:libgnucash/engine/Scrub2.cpp:270-327`).
fn merge_splits(book: &mut Book, work: &mut LotWork, sa: Guid, sb: Guid) -> DomainResult<()> {
    let (txn, b) = find_split(book, sb).map(|(t, s)| (t.guid, s.clone())).ok_or_else(|| DomainError::not_found("split", &sb))?;
    let a = find_split(book, sa).map(|(_, s)| s.clone()).ok_or_else(|| DomainError::not_found("split", &sa))?;
    let account = a.account.ok_or_else(|| DomainError::invalid("a lot split has no account"))?;
    let currency = book.transaction(txn).map(|t| t.currency.clone()).ok_or_else(|| DomainError::not_found("transaction", &txn))?;
    let amount = round_amount(book, account, a.amount.add_fixed(b.amount)?)?;
    let value = round_value(book, &currency, a.value.add_fixed(b.value)?)?;
    work.remove_split(book, &b);
    let t = txn_mut(book, txn)?;
    t.splits.retain(|s| s.guid != sb);
    if let Some(merged) = t.split_mut(sa) {
        merged.amount = amount;
        merged.value = value;
        merged.reconcile_state = ReconcileState::New;
    }
    sort_splits(t);
    Ok(())
}

/// `gncOwnerSetLotLinkMemo` (`5.17:libgnucash/engine/gncOwner.c:1009-1071`).
fn set_lot_link_memo(book: &mut Book, txn: Guid) -> DomainResult<()> {
    let t = book.transaction(txn).ok_or_else(|| DomainError::not_found("transaction", &txn))?;
    if t.txn_type() != Some('L') {
        return Ok(());
    }
    let docs: Vec<(Guid, String)> = t
        .splits
        .iter()
        .filter_map(|s| s.lot.and_then(|l| lot_invoice(book, l)).map(|inv| document_title(book, inv).map(|title| (s.guid, title))))
        .collect::<DomainResult<_>>()?;
    if docs.is_empty() {
        return Ok(());
    }
    let mut titles: Vec<&str> = docs.iter().map(|(_, t)| t.as_str()).collect();
    titles.sort_unstable();
    let memo = format!("{LOT_LINK_MEMO_PREFIX}{}", titles.join(" - "));
    let ids: Vec<Guid> = docs.iter().map(|(g, _)| *g).collect();
    let t = txn_mut(book, txn)?;
    t.splits.iter_mut().filter(|s| ids.contains(&s.guid)).for_each(|s| s.memo = Some(memo.clone()));
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Applying

/// `gncOwnerAutoApplyPaymentsWithLots` (`5.17:libgnucash/engine/gncOwner.c:1256-1394`). Each lot's balance is read
/// once, before its inner loop, as GnuCash does.
pub fn auto_apply_with_lots(book: &mut Book, env: &mut dyn Env, work: &mut LotWork, owner: OwnerRef, lots: &mut [Option<Guid>]) -> DomainResult<()> {
    let name = end_owner_name(book, owner);
    for i in 0..lots.len() {
        let Some(left) = usable_lot(book, work, lots, i)? else { continue };
        let account = book.lot(left).map(|l| l.account);
        let left_bal = work.balance(book, left)?;
        let left_doc = lot_invoice(book, left).is_some();
        for j in i + 1..lots.len() {
            let Some(right) = usable_lot(book, work, lots, j)? else { continue };
            if book.lot(right).map(|l| l.account) != account {
                continue;
            }
            let right_bal = work.balance(book, right)?;
            if left_bal.is_positive() == right_bal.is_positive() {
                continue;
            }
            match (left_doc, lot_invoice(book, right).is_some()) {
                (true, true) => create_lot_link(book, env, work, left, right, name.as_deref())?,
                // Two payment lots: the bigger one pays the smaller; otherwise the payment lot pays the document.
                (false, false) if left_bal.abs().cmp_value(right_bal.abs()).is_ge() => offset_lots(book, env, work, left, right)?,
                (false, true) => offset_lots(book, env, work, left, right)?,
                (false | true, false) => offset_lots(book, env, work, right, left)?,
            }
        }
    }
    Ok(())
}

/// The lot at `i` when it is open; an empty lot is destroyed and dropped from the list.
fn usable_lot(book: &mut Book, work: &mut LotWork, lots: &mut [Option<Guid>], i: usize) -> DomainResult<Option<Guid>> {
    let Some(lot) = lots[i] else { return Ok(None) };
    if work.count(book, lot) == 0 {
        book.lots.shift_remove(&lot);
        lots[i] = None;
        return Ok(None);
    }
    Ok((!work.is_closed(book, lot)?).then_some(lot))
}

/// `gncOwnerApplyPaymentSecs` (`5.17:libgnucash/engine/gncOwner.c:1404-1440`): record a payment and apply it, with
/// the given lots or, if none and `auto_pay`, every open lot of the owner in the posted account.
pub fn apply_owner_payment(book: &mut Book, env: &mut dyn Env, p: &Payment, lots: &[Guid], auto_pay: bool) -> DomainResult<PaymentResult> {
    check_owner_and_account(book, p)?;
    let mut work = LotWork::default();
    let mut selected: Vec<Option<Guid>> = if !lots.is_empty() {
        lots.iter().map(|l| Some(*l)).collect()
    } else if auto_pay {
        open_lots(book, &mut work, p.posted_account, |_, l| Ok(lot_end_owner_invoice_first(book, l) == Some(p.owner)))?.into_iter().map(Some).collect()
    } else {
        Vec::new()
    };
    let created = (!p.amount.is_zero()).then(|| create_payment_lot(book, env, &mut work, p)).transpose()?;
    if let Some((_, lot)) = created {
        selected.insert(0, Some(lot));
    }
    auto_apply_with_lots(book, env, &mut work, p.owner, &mut selected)?;
    let payment_lot = created.map(|(_, l)| l).filter(|l| work.count(book, *l) > 0);
    work.drop_empty_lots(book);
    Ok(PaymentResult { transaction: created.map(|(t, _)| t), payment_lot })
}

/// Refuse a payment whose owner or posted account does not exist.
fn check_owner_and_account(book: &Book, p: &Payment) -> DomainResult<()> {
    if !contacts::owner_exists(book, p.owner) {
        return Err(DomainError::not_found(p.owner.kind.type_name(), &p.owner.guid));
    }
    let acc = book.account(p.posted_account).ok_or_else(|| DomainError::not_found("account", &p.posted_account))?;
    if !acc.account_type.is_ap_ar() {
        return Err(DomainError::invalid(format!("{} is not an A/Receivable or A/Payable account", acc.name)));
    }
    Ok(())
}

/// `gncInvoiceApplyPayment`: pay one posted document (`5.17:libgnucash/engine/gncInvoice.c:1950-1979`).
pub fn apply_invoice_payment(book: &mut Book, env: &mut dyn Env, invoice: Guid, mut p: Payment) -> DomainResult<PaymentResult> {
    let inv = book.business.invoices.get(&invoice).cloned().ok_or_else(|| DomainError::not_found("invoice", &invoice))?;
    let (Some(lot), Some(account)) = (inv.post_lot, inv.post_acc) else {
        return Err(DomainError::conflict(format!("document {} is not posted", inv.id)));
    };
    p.owner = calc::require_end_owner(book, inv.owner)?;
    p.posted_account = account;
    if p.transfer_account.is_none() {
        return Err(DomainError::invalid("a payment needs a transfer account"));
    }
    let mut work = LotWork::default();
    let (txn, pay_lot) = create_payment_lot(book, env, &mut work, &p)?;
    let mut lots = [Some(pay_lot), Some(lot)];
    auto_apply_with_lots(book, env, &mut work, p.owner, &mut lots)?;
    let payment_lot = Some(pay_lot).filter(|l| work.count(book, *l) > 0);
    work.drop_empty_lots(book);
    Ok(PaymentResult { transaction: Some(txn), payment_lot })
}

/// `gncInvoiceAutoApplyPayments`: apply open pre-payments and opposite documents of the same owner to a posted
/// document (`5.17:libgnucash/engine/gncInvoice.c:1892-1934`).
pub fn auto_apply_invoice(book: &mut Book, env: &mut dyn Env, invoice: Guid) -> DomainResult<()> {
    let inv = book.business.invoices.get(&invoice).cloned().ok_or_else(|| DomainError::not_found("invoice", &invoice))?;
    let (Some(lot), Some(account)) = (inv.post_lot, inv.post_acc) else {
        return Err(DomainError::conflict(format!("document {} is not posted", inv.id)));
    };
    let owner = calc::require_end_owner(book, inv.owner)?;
    let mut work = LotWork::default();
    let positive = work.balance(book, lot)?.is_positive();
    let candidates = open_lots(book, &mut work, account, |w, l| {
        Ok(positive != w.balance(book, l)?.is_positive() && lot_end_owner_slots_first(book, l) == Some(owner))
    })?;
    let mut lots: Vec<Option<Guid>> = std::iter::once(lot).chain(candidates).map(Some).collect();
    auto_apply_with_lots(book, env, &mut work, owner, &mut lots)?;
    work.drop_empty_lots(book);
    Ok(())
}

/// Destroy a transaction: its splits leave their lots.
pub fn destroy_transaction(book: &mut Book, work: &mut LotWork, txn: Guid) {
    if let Some(t) = book.transactions.shift_remove(&txn) {
        t.splits.iter().for_each(|s| work.remove_split(book, s));
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Due reminders

/// Which documents a reminder lists (`DUE_FOR_VENDOR`, `DUE_FOR_CUSTOMER`, `5.17:gnucash/gnome/dialog-invoice.c:3780-3880`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueFor {
    /// Bills and expense vouchers, with their credit notes.
    Vendors,
    /// Customer invoices and credit notes.
    Customers,
}

/// A posted, unpaid document in a reminder.
#[derive(Debug, Clone, PartialEq)]
pub struct DueDocument {
    /// The document.
    pub invoice: Guid,
    /// When it is due (`trans-date-due` of its posted transaction).
    pub due: Time64,
    /// Its kind.
    pub kind: InvoiceType,
    /// The open balance of its lot, signed as on the A/R or A/P account.
    pub balance: Numeric,
}

/// `gnc_invoice_show_docs_due`: posted documents whose lot is open and which fall due by `now + days × 86400`,
/// overdue ones included, sorted by due date (`5.17:gnucash/gnome/dialog-invoice.c:3780-3895`). A negative `days`
/// counts as zero.
pub fn documents_due(book: &Book, which: DueFor, days: i64, now: Time64) -> DomainResult<Vec<DueDocument>> {
    let limit = now.plus_seconds(days.max(0).saturating_mul(86_400));
    let mut work = LotWork::default();
    let mut out = Vec::new();
    for inv in book.business.invoices.values() {
        let (Some(txn), Some(lot)) = (inv.post_txn.and_then(|g| book.transaction(g)), inv.post_lot) else { continue };
        let Ok(kind) = calc::invoice_type(book, inv) else { continue };
        let wanted = match which {
            DueFor::Customers => kind.is_customer(),
            DueFor::Vendors => !kind.is_customer(),
        };
        let due = txn.slots.get(crate::ops::business::posting::DATE_DUE).and_then(SlotValue::as_time).unwrap_or(txn.date_posted);
        if wanted && due <= limit && book.lot(lot).is_some() && !work.is_closed(book, lot)? {
            out.push(DueDocument { invoice: inv.guid, due, kind, balance: work.balance(book, lot)? });
        }
    }
    out.sort_by_key(|d| d.due);
    Ok(out)
}
