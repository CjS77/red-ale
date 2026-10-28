//! Domain operations: lots, the lot viewer's information, capital gains and stock splits.
//!
//! Lots follow `5.17:libgnucash/engine/gnc-lot.cpp`; assignment, gains and repair follow `cap-gains.cpp`, `policy.cpp`
//! (FIFO, the only policy 5.17 implements), `Scrub2.cpp` and `Scrub3.cpp`; the viewer follows
//! `5.17:gnucash/gnome/dialog-lot-viewer.c`; stock splits follow `5.17:gnucash/gnome/assistant-stock-split.c`.
//! See `design/api/lots.md`.

mod gains;
mod ledger;
mod scrub;
mod stock_split;
#[cfg(test)]
mod tests;

use std::sync::Arc;

pub(crate) use gains::lookup_by_name;
pub use gains::{
    ORPHANED_GAINS, ORPHANED_GAINS_NOTES, REALIZED_GAIN_LOSS, balance_before, compute_lot_gains, compute_split_gains, free_split_gain,
    gains_account, recorded_gain,
};
pub use ledger::{Entry, Ledger};
pub use scrub::{Placed, assign_lots, assign_to_lot, has_trades, scrub_account_lots, scrub_lot, scrub_tree_lots, split_assign};
pub use stock_split::{PRICE_SOURCE_STOCK_SPLIT, PriceOutcome, STOCK_SPLIT_ACTION, StockSplit, StockSplitOutcome, stock_split};

use crate::lot::{account_slots, gains_source, is_stock_split, slot_paths};
use crate::transaction::slot_paths as txn_slots;
use crate::{Book, CommodityRef, DomainError, DomainResult, Env, Guid, Lot, Numeric, SlotValue, Split, Time64, Transaction};

/// Check & Repair's lot step for one account (`xaccAccountScrubLots`): assign unlotted splits to lots and recompute
/// capital gains. Called by `scrub::check_and_repair`.
pub fn scrub_lots(book: &mut Book, env: &mut dyn Env, account: Guid) -> DomainResult<()> {
    scrub_account_lots(book, env, account)
}

/// A split and the transaction holding it.
pub(crate) fn locate(book: &Book, split: Guid) -> Option<(Guid, &Split)> {
    book.transactions.values().find_map(|t| t.split(split).map(|s| (t.guid, s)))
}

/// Edit a transaction in place.
pub(crate) fn edit_txn<T>(book: &mut Book, txn: Guid, f: impl FnOnce(&mut Transaction) -> T) -> DomainResult<T> {
    let t = book
        .transactions
        .get_mut(&txn)
        .ok_or_else(|| DomainError::not_found("transaction", &txn))?;
    Ok(f(Arc::make_mut(t)))
}

/// Edit a split in place.
pub(crate) fn edit_split(book: &mut Book, split: Guid, f: impl FnOnce(&mut Split)) -> DomainResult<()> {
    let (txn, _) = locate(book, split).ok_or_else(|| DomainError::not_found("split", &split))?;
    edit_txn(book, txn, |t| t.split_mut(split).map(f))?.ok_or_else(|| DomainError::not_found("split", &split))
}

/// The posted invoice a transaction belongs to (`gncInvoiceGetInvoiceFromTxn`), if it exists.
pub(crate) fn invoice_of_txn(book: &Book, txn: Guid) -> Option<Guid> {
    let t = book.transaction(txn)?;
    t.slots
        .get(txn_slots::INVOICE)
        .and_then(SlotValue::as_guid)
        .filter(|g| book.business.invoices.contains_key(g))
}

/// The invoice a lot belongs to (`gncInvoiceGetInvoiceFromLot`), if it exists.
fn invoice_of_lot(book: &Book, lot: &Lot) -> Option<Guid> {
    lot.invoice().filter(|g| book.business.invoices.contains_key(g))
}

/// An account's lots in GnuCash's lot-list order. GnuCash prepends each lot to the list, when loading and when making
/// one (`xaccAccountInsertLot`, `5.17:libgnucash/engine/Account.cpp:2150`), so the list is the Book's order reversed.
pub fn account_lots(book: &Book, account: Guid) -> Vec<Guid> {
    book.lots.values().rev().filter(|l| l.account == account).map(|l| l.guid).collect()
}

/// A lot's splits in GnuCash's list order (`gnc_lot_get_split_list`, `5.17:libgnucash/engine/gnc-lot.cpp:424-431`):
/// the order they joined it, each appended (`gnc_lot_add_split`, `:619`). Loading a book adds them in file order, so
/// red-ale takes Book order: transactions as the Book holds them, splits in their transaction's order.
pub fn lot_split_list(book: &Book, lot: Guid) -> Vec<(&Transaction, &Split)> {
    book.transactions
        .values()
        .flat_map(|t| t.splits.iter().map(move |s| (t.as_ref(), s)))
        .filter(|(_, s)| s.lot == Some(lot))
        .collect()
}

/// A new, empty lot titled `Lot <n>` from the account's `lot-mgmt/next-id` slot, which then counts up
/// (`gnc_lot_make_default`, `5.17:libgnucash/engine/gnc-lot.cpp:765-783`).
pub fn make_default_lot(book: &mut Book, env: &mut dyn Env, account: Guid) -> DomainResult<Guid> {
    if account == book.root {
        return Err(DomainError::invalid("the root account holds no lots"));
    }
    let acc = book
        .accounts
        .get_mut(&account)
        .ok_or_else(|| DomainError::not_found("account", &account))?;
    let path = format!("{}/{}", account_slots::LOT_MGMT, account_slots::NEXT_ID);
    let id = acc.slots.get(&path).and_then(SlotValue::as_int).unwrap_or(0);
    Arc::make_mut(acc).slots.set(&path, SlotValue::Integer(id + 1));
    let mut lot = Lot::new(env.new_guid(), account);
    lot.slots.set(slot_paths::TITLE, SlotValue::String(format!("Lot {id}")));
    let guid = lot.guid;
    book.lots.insert(guid, Arc::new(lot));
    Ok(guid)
}

/// Make an empty lot in an account, as the lot viewer's New Lot does, then set the title and notes given.
pub fn create_lot(book: &mut Book, env: &mut dyn Env, account: Guid, title: Option<&str>, notes: Option<&str>) -> DomainResult<Guid> {
    let guid = make_default_lot(book, env, account)?;
    if title.is_some() || notes.is_some() {
        update_lot(book, guid, title, notes)?;
    }
    Ok(guid)
}

/// Set a lot's title and notes; `None` leaves one unchanged and an empty string removes it.
pub fn update_lot(book: &mut Book, lot: Guid, title: Option<&str>, notes: Option<&str>) -> DomainResult<()> {
    let l = book.lots.get_mut(&lot).ok_or_else(|| DomainError::not_found("lot", &lot))?;
    let l = Arc::make_mut(l);
    if let Some(t) = title {
        l.slots.set_opt_str(slot_paths::TITLE, Some(t));
    }
    if let Some(n) = notes {
        l.slots.set_opt_str(slot_paths::NOTES, Some(n));
    }
    Ok(())
}

/// Delete a lot. An invoice's lot is refused, as the lot viewer refuses it (`5.17:gnucash/gnome/dialog-lot-viewer.c:795-806`);
/// a lot with splits is refused unless `release_splits`, which takes its splits out of it first. Returns the
/// released splits.
pub fn delete_lot(book: &mut Book, lot: Guid, release_splits: bool) -> DomainResult<Vec<Guid>> {
    let l = book.lot(lot).ok_or_else(|| DomainError::not_found("lot", &lot))?;
    if invoice_of_lot(book, l).is_some() {
        return Err(DomainError::conflict("the lot belongs to an invoice; unpost the invoice instead"));
    }
    let splits: Vec<Guid> = book.splits_in_lot(lot).map(|(_, s)| s.guid).collect();
    if !splits.is_empty() && !release_splits {
        return Err(DomainError::conflict(format!(
            "the lot holds {} splits; pass releaseSplits to take them out",
            splits.len()
        )));
    }
    splits.iter().try_for_each(|s| edit_split(book, *s, |s| s.lot = None))?;
    book.lots.shift_remove(&lot);
    Ok(splits)
}

/// The outcome of assigning a split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    /// Each piece of the split and the lot it joined.
    pub pieces: Vec<(Guid, Guid)>,
    /// When a split was cut to close an explicitly chosen lot, the new split holding the rest, in no lot.
    pub rest: Option<Guid>,
    /// Lots made for the assignment.
    pub new_lots: Vec<Guid>,
}

/// Assign a split to a lot. With a lot, the split joins it as `xaccSplitAssignToLot` decides, cut in two when it would
/// overfill it; without one, FIFO picks the lots and makes new ones as needed (`xaccSplitAssign`).
pub fn assign_split(book: &mut Book, env: &mut dyn Env, split: Guid, lot: Option<Guid>) -> DomainResult<Assignment> {
    let (_, s) = locate(book, split).ok_or_else(|| DomainError::not_found("split", &split))?;
    let account = s.account.ok_or_else(|| DomainError::invalid("the split has no account"))?;
    if let Some(current) = s.lot {
        return Err(DomainError::conflict(format!(
            "the split is already in lot {current}; remove it first"
        )));
    }
    if is_stock_split(s) {
        return Err(DomainError::invalid("stock-split splits are kept out of lots, as in GnuCash 5.17"));
    }
    if gains_source(s).is_some() || s.amount.is_zero() {
        return Err(DomainError::invalid("a split with a zero amount cannot be assigned to a lot"));
    }
    let before: Vec<Guid> = book.lots.keys().copied().collect();
    let (pieces, rest) = match lot {
        Some(lot) => assign_to_chosen(book, env, split, account, lot)?,
        None if has_trades(book, account) => (split_assign(book, env, split)?, None),
        None => return Err(DomainError::invalid("the account holds no trades, so FIFO has nothing to assign")),
    };
    let new_lots = book.lots.keys().filter(|k| !before.contains(k)).copied().collect();
    Ok(Assignment { pieces, rest, new_lots })
}

/// Pieces of a split with their lots, and the rest left out of any lot.
type Pieces = (Vec<(Guid, Guid)>, Option<Guid>);

fn assign_to_chosen(book: &mut Book, env: &mut dyn Env, split: Guid, account: Guid, lot: Guid) -> DomainResult<Pieces> {
    let l = book.lot(lot).ok_or_else(|| DomainError::not_found("lot", &lot))?;
    if l.account != account {
        return Err(DomainError::invalid("a split joins only a lot of its own account"));
    }
    match assign_to_lot(book, env, split, lot)? {
        Placed::Whole => Ok((vec![(split, lot)], None)),
        Placed::Cut(rest) => Ok((vec![(split, lot)], Some(rest))),
        Placed::LotClosed => Err(DomainError::conflict("the lot is closed")),
        Placed::Skipped => Err(DomainError::invalid("the split cannot join a lot")),
    }
}

/// Take a split out of its lot (`gnc_lot_remove_split`). Refused for the posting split of an invoice's own lot
/// (`5.17:gnucash/gnome/dialog-lot-viewer.c:472-487`). A lot left empty leaves its account, so GnuCash no longer
/// writes it (`5.17:libgnucash/engine/gnc-lot.cpp:631-652`); red-ale deletes it. Returns whether it was deleted.
pub fn remove_split(book: &mut Book, split: Guid) -> DomainResult<bool> {
    let (txn, s) = locate(book, split).ok_or_else(|| DomainError::not_found("split", &split))?;
    let lot = s.lot.ok_or_else(|| DomainError::invalid("the split is in no lot"))?;
    let lot_invoice = book.lot(lot).and_then(|l| invoice_of_lot(book, l));
    if lot_invoice.is_some() && lot_invoice == invoice_of_txn(book, txn) {
        return Err(DomainError::conflict("the split posts the lot's invoice"));
    }
    edit_split(book, split, |s| s.lot = None)?;
    let empty = book.splits_in_lot(lot).next().is_none();
    if empty {
        book.lots.shift_remove(&lot);
    }
    Ok(empty)
}

/// What a split is, for the lot viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitKind {
    /// A buy, sell or other split.
    Normal,
    /// A gains transaction's split in the lot.
    Gains,
    /// The split of a stock split.
    StockSplit,
}

/// A lot as the lot viewer shows it (`gnc_lot_viewer_fill`, `5.17:gnucash/gnome/dialog-lot-viewer.c:333-430`).
#[derive(Debug, Clone, PartialEq)]
pub struct LotSummary {
    /// The lot.
    pub guid: Guid,
    /// Its account.
    pub account: Guid,
    /// Title.
    pub title: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// The invoice it belongs to.
    pub invoice: Option<Guid>,
    /// Closed: it has splits and its balance is zero.
    pub closed: bool,
    /// The posted date of its earliest split.
    pub opened: Option<Time64>,
    /// The posted date of its latest split, when closed.
    pub closed_at: Option<Time64>,
    /// The sum of its adjusted amounts.
    pub balance: Numeric,
    /// The currency of its gains.
    pub gains_currency: Option<CommodityRef>,
    /// Realised gains: the values of its zero-amount splits in that currency.
    pub gains: Numeric,
    /// How many splits it holds.
    pub split_count: usize,
}

/// One split, as the lot viewer lists it (`gnc_split_viewer_fill`, `5.17:gnucash/gnome/dialog-lot-viewer.c:490-581`).
#[derive(Debug, Clone, PartialEq)]
pub struct SplitView {
    /// The split.
    pub split: Guid,
    /// Its transaction.
    pub transaction: Guid,
    /// The transaction's posted date.
    pub date_posted: Time64,
    /// The transaction's number.
    pub num: Option<String>,
    /// The transaction's description.
    pub description: Option<String>,
    /// Memo.
    pub memo: Option<String>,
    /// Action.
    pub action: Option<String>,
    /// The split's Number, as the lot viewer's Num column shows it (`gnc_get_num_action`).
    pub number: Option<String>,
    /// Amount.
    pub amount: Numeric,
    /// Amount in units after later stock splits.
    pub adjusted_amount: Numeric,
    /// Value.
    pub value: Numeric,
    /// The transaction currency.
    pub currency: CommodityRef,
    /// The gain recorded for it (in a lot) or that it would realise against the chosen lot (free).
    pub gain: Option<Numeric>,
    /// What it is.
    pub kind: SplitKind,
}

/// The realised gains of a lot (`find_first_currency`, `get_realized_gains`,
/// `5.17:gnucash/gnome/dialog-lot-viewer.c:146-187`).
fn realized_gains(entries: &[&Entry]) -> (Option<CommodityRef>, Numeric) {
    let zero: Vec<&&Entry> = entries.iter().filter(|e| e.amount.is_zero()).collect();
    let Some(currency) = zero.first().map(|e| e.currency.clone()) else {
        return (None, Numeric::ZERO);
    };
    let gains = zero.iter().filter(|e| e.currency == currency).fold(Numeric::ZERO, |acc, e| {
        acc.add_fixed(e.value).or_else(|_| acc.add_exact(e.value)).unwrap_or(acc)
    });
    (Some(currency), gains)
}

fn summary(book: &Book, ledger: &Ledger, lot: &Lot) -> DomainResult<LotSummary> {
    let entries = ledger.lot_entries(lot.guid);
    let closed = ledger.lot_is_closed(lot.guid)?;
    let (gains_currency, gains) = realized_gains(&entries);
    Ok(LotSummary {
        guid: lot.guid,
        account: lot.account,
        title: lot.title().map(str::to_owned),
        notes: lot.notes().map(str::to_owned),
        invoice: invoice_of_lot(book, lot),
        closed,
        opened: entries.first().map(|e| e.posted),
        closed_at: if closed { entries.last().map(|e| e.posted) } else { None },
        balance: ledger.lot_balance(lot.guid)?,
        gains_currency,
        gains,
        split_count: entries.len(),
    })
}

/// One lot's summary.
pub fn lot_summary(book: &Book, lot: Guid) -> DomainResult<LotSummary> {
    let l = book.lot(lot).ok_or_else(|| DomainError::not_found("lot", &lot))?;
    summary(book, &Ledger::build(book, l.account)?, l)
}

/// An account's lots, opened earliest first (empty lots last, in GnuCash's lot-list order).
pub fn lot_summaries(book: &Book, account: Guid) -> DomainResult<Vec<LotSummary>> {
    book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
    let ledger = Ledger::build(book, account)?;
    let mut out = account_lots(book, account)
        .into_iter()
        .filter_map(|g| book.lot(g))
        .map(|l| summary(book, &ledger, l))
        .collect::<DomainResult<Vec<_>>>()?;
    out.sort_by_key(|s| (s.opened.is_none(), s.opened));
    Ok(out)
}

fn split_view(book: &Book, e: &Entry, gain: Option<Numeric>) -> Option<SplitView> {
    let t = book.transaction(e.txn)?;
    let s = t.split(e.split)?;
    let kind = if is_stock_split(s) {
        SplitKind::StockSplit
    } else if gains_source(s).is_some() {
        SplitKind::Gains
    } else {
        SplitKind::Normal
    };
    Some(SplitView {
        split: s.guid,
        transaction: t.guid,
        date_posted: t.date_posted,
        num: t.num.clone(),
        description: t.description.clone(),
        memo: s.memo.clone(),
        action: s.action.clone(),
        number: crate::book_options::number(book, t, s).map(str::to_owned),
        amount: s.amount,
        adjusted_amount: e.adjusted,
        value: s.value,
        currency: t.currency.clone(),
        gain: gain.or_else(|| recorded_gain(book, s)),
        kind,
    })
}

/// A lot's splits, earliest first, each with the gain recorded for it.
pub fn lot_splits(book: &Book, lot: Guid) -> DomainResult<Vec<SplitView>> {
    let l = book.lot(lot).ok_or_else(|| DomainError::not_found("lot", &lot))?;
    let ledger = Ledger::build(book, l.account)?;
    Ok(ledger
        .lot_entries(lot)
        .into_iter()
        .filter_map(|e| split_view(book, e, None))
        .collect())
}

/// An account's splits in no lot, in account order; with `lot`, each with the gain it would realise against that lot
/// (`xaccLotFreeSplitCapGain`).
pub fn free_splits(book: &Book, account: Guid, lot: Option<Guid>) -> DomainResult<Vec<SplitView>> {
    book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
    if let Some(l) = lot {
        let owner = book.lot(l).ok_or_else(|| DomainError::not_found("lot", &l))?.account;
        if owner != account {
            return Err(DomainError::invalid("the lot belongs to another account"));
        }
    }
    let ledger = Ledger::build(book, account)?;
    ledger
        .entries
        .iter()
        .filter(|e| e.lot.is_none())
        .map(|e| {
            let gain = lot.map(|l| free_split_gain(book, e.split, l)).transpose()?;
            Ok(split_view(book, e, gain))
        })
        .filter_map(Result::transpose)
        .collect()
}
