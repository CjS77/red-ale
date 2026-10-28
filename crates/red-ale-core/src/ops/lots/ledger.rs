//! An account's splits in GnuCash's account order, with the adjusted amounts and lot state that lot assignment and
//! capital gains read.
//!
//! GnuCash keeps these as runtime caches (`priv->splits`, `split->adjusted_amount`, `lot->is_closed`); red-ale
//! rebuilds them from the Book whenever an operation needs them, so they are never stale.

use crate::book_options::uses_split_action_for_num;
use crate::lot::is_stock_split;
use crate::ops::commit::account_scu;
use crate::ops::transactions::order::split_order;
use crate::{Book, CommodityRef, DomainResult, Guid, How, Numeric, Round, Split, Time64, Transaction};

/// One split of the account.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The transaction holding the split.
    pub txn: Guid,
    /// The split.
    pub split: Guid,
    /// Book order: the transaction's index in the Book, then the split's index in the transaction.
    pub pos: (usize, usize),
    /// The transaction's posted date.
    pub posted: Time64,
    /// The transaction currency.
    pub currency: CommodityRef,
    /// The split's amount.
    pub amount: Numeric,
    /// The amount restated in units after later stock splits.
    pub adjusted: Numeric,
    /// The split's value.
    pub value: Numeric,
    /// The split's lot.
    pub lot: Option<Guid>,
    /// The split records a stock split.
    pub stock_split: bool,
    /// The transaction is void.
    pub voided: bool,
}

/// Splits whose amount the current operation has set.
///
/// Setting a split's amount resets its adjusted amount to the amount (`xaccSplitSetAmount`,
/// `5.17:libgnucash/engine/Split.cpp:1260`); GnuCash restates it for stock splits only when it recomputes the
/// account's balances, which a scrub defers to the end of the account edit wrapping it
/// (`5.17:libgnucash/engine/Account.cpp:2268`). Until then, lot balances, FIFO and gains read the reset value. red-ale
/// reproduces this so its lots and gains match GnuCash's when a sale before a stock split is cut.
#[derive(Debug, Clone, Default)]
pub struct Touched(Vec<Guid>);

impl Touched {
    /// Record that a split's amount was set.
    pub fn add(&mut self, split: Guid) {
        if !self.0.contains(&split) {
            self.0.push(split);
        }
    }
}

/// An account's splits in account order.
#[derive(Debug, Clone)]
pub struct Ledger {
    /// The account.
    pub account: Guid,
    /// Its splits, in account order.
    pub entries: Vec<Entry>,
}

impl Ledger {
    /// Build the ledger of `account` from the Book.
    pub fn build(book: &Book, account: Guid) -> DomainResult<Self> {
        let scu = account_scu(book, account)?;
        let num_action = uses_split_action_for_num(book);
        let mut rows: Vec<(&Transaction, &Split, (usize, usize))> = book
            .transactions
            .values()
            .enumerate()
            .flat_map(|(i, t)| {
                t.splits
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.account == Some(account))
                    .map(move |(j, s)| (t.as_ref(), s, (i, j)))
            })
            .collect();
        rows.sort_by(|a, b| split_order(a.0, a.1, b.0, b.1, num_action));
        let mut entries: Vec<Entry> = rows
            .into_iter()
            .map(|(t, s, pos)| Entry {
                txn: t.guid,
                split: s.guid,
                pos,
                posted: t.date_posted,
                currency: t.currency.clone(),
                amount: s.amount,
                adjusted: s.amount,
                value: s.value,
                lot: s.lot,
                stock_split: is_stock_split(s),
                voided: t.is_void(),
            })
            .collect();
        adjust_for_stock_splits(&mut entries, scu)?;
        Ok(Self { account, entries })
    }

    /// Build the ledger as GnuCash sees it during an operation: splits in `touched` keep their amount as their
    /// adjusted amount.
    pub fn build_touched(book: &Book, account: Guid, touched: &Touched) -> DomainResult<Self> {
        let mut ledger = Self::build(book, account)?;
        ledger
            .entries
            .iter_mut()
            .filter(|e| touched.0.contains(&e.split))
            .for_each(|e| e.adjusted = e.amount);
        Ok(ledger)
    }

    /// The entry of a split.
    pub fn entry(&self, split: Guid) -> Option<&Entry> {
        self.entries.iter().find(|e| e.split == split)
    }

    /// A lot's splits in GnuCash's lot order: the order they joined the lot (approximated by Book order), sorted by
    /// posted date with ties kept (`gnc_lot_get_earliest_split` sorts with `xaccSplitOrderDateOnly`,
    /// `5.17:libgnucash/engine/gnc-lot.cpp:658-685`, `5.17:libgnucash/engine/Split.cpp:1600-1618`).
    pub fn lot_entries(&self, lot: Guid) -> Vec<&Entry> {
        let mut out: Vec<&Entry> = self.entries.iter().filter(|e| e.lot == Some(lot)).collect();
        out.sort_by_key(|e| e.pos);
        out.sort_by_key(|e| e.posted);
        out
    }

    /// A lot's balance: the sum of its adjusted amounts (`gnc_lot_get_balance`, `5.17:libgnucash/engine/gnc-lot.cpp:487-525`).
    pub fn lot_balance(&self, lot: Guid) -> DomainResult<Numeric> {
        Ok(Numeric::sum_fixed(self.lot_entries(lot).iter().map(|e| e.adjusted))?)
    }

    /// True when the lot has splits and its balance is zero (`gnc_lot_is_closed`).
    pub fn lot_is_closed(&self, lot: Guid) -> DomainResult<bool> {
        let entries = self.lot_entries(lot);
        Ok(!entries.is_empty() && Numeric::sum_fixed(entries.iter().map(|e| e.adjusted))?.is_zero())
    }

    /// The earliest split of a lot (`gnc_lot_get_earliest_split`): FIFO's opening split.
    pub fn earliest(&self, lot: Guid) -> Option<&Entry> {
        self.lot_entries(lot).first().copied()
    }

    /// The latest split of a lot (`gnc_lot_get_latest_split`).
    pub fn latest(&self, lot: Guid) -> Option<&Entry> {
        self.lot_entries(lot).last().copied()
    }
}

/// Restate amounts before each stock split in post-split units, as `xaccAccountRecomputeBalance` does
/// (`5.17:libgnucash/engine/Account.cpp:2263-2330`): a stock split with a zero balance before or after it is skipped.
fn adjust_for_stock_splits(entries: &mut [Entry], scu: i64) -> DomainResult<()> {
    let mut balance = Numeric::ZERO;
    (0..entries.len()).try_for_each(|i| -> DomainResult<()> {
        let amount = entries[i].amount;
        if !entries[i].stock_split {
            balance = balance.add_fixed(amount)?;
            return Ok(());
        }
        if balance.is_zero() {
            return Ok(());
        }
        let new_balance = balance.add_fixed(amount)?;
        if new_balance.is_zero() {
            return Ok(());
        }
        let ratio = new_balance.div(balance, crate::DENOM_AUTO, How::REDUCE_NEVER)?;
        entries[..i].iter_mut().try_for_each(|e| -> DomainResult<()> {
            e.adjusted = e.adjusted.mul(ratio, scu, How::round(Round::HalfUp))?;
            Ok(())
        })?;
        balance = new_balance;
        Ok(())
    })
}
