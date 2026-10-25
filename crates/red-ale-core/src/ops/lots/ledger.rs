//! An account's splits in GnuCash's account order, with the adjusted amounts and lot state that lot assignment and
//! capital gains read.
//!
//! GnuCash keeps these as runtime caches (`priv->splits`, `split->adjusted_amount`, `lot->is_closed`); red-ale
//! rebuilds them from the Book whenever an operation needs them, so they are never stale.

use std::cmp::Ordering;

use crate::book_options::uses_split_action_for_num;
use crate::lot::is_stock_split;
use crate::ops::commit::account_scu;
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
/// (`5.17:libgnucash/engine/Account.cpp:2266`). Until then, lot balances, FIFO and gains read the reset value. red-ale
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
            .flat_map(|(i, t)| t.splits.iter().enumerate().filter(|(_, s)| s.account == Some(account)).map(move |(j, s)| (t.as_ref(), s, (i, j))))
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
        ledger.entries.iter_mut().filter(|e| touched.0.contains(&e.split)).for_each(|e| e.adjusted = e.amount);
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

/// True when the transaction closes the books (`xaccTransGetIsClosingTxn`, slot `book_closing`,
/// `5.17:libgnucash/engine/Transaction.cpp:2228-2236`).
fn is_closing(t: &Transaction) -> bool {
    t.slots.get_key("book_closing").and_then(crate::SlotValue::as_int).is_some_and(|v| v != 0)
}

/// The leading unsigned integer of a string, as `strtoull` reads it; 0 when there is none.
fn leading_number(s: &str) -> (u64, &str) {
    let trimmed = s.trim_start();
    let digits = trimmed.strip_prefix('+').unwrap_or(trimmed);
    let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
    match digits[..end].parse::<u64>() {
        Ok(n) => (n, &digits[end..]),
        Err(_) if end > 0 => (u64::MAX, &digits[end..]),
        Err(_) => (0, s),
    }
}

/// `order_by_int64_or_string` (`5.17:libgnucash/engine/Transaction.cpp:1773-1791`). GnuCash collates text with
/// `g_utf8_collate`, which depends on the locale; red-ale compares bytes.
fn order_num(a: &str, b: &str) -> Ordering {
    let ((na, ra), (nb, rb)) = (leading_number(a), leading_number(b));
    if na != 0 && nb != 0 {
        return na.cmp(&nb).then_with(|| ra.cmp(rb));
    }
    a.cmp(b)
}

/// `xaccTransOrder_num_action` (`5.17:libgnucash/engine/Transaction.cpp:1794-1838`): posted date, closing
/// transactions last, number (or split action), date entered, description, GUID.
pub fn trans_order(ta: &Transaction, num_a: Option<&str>, tb: &Transaction, num_b: Option<&str>) -> Ordering {
    if ta.guid == tb.guid {
        return Ordering::Equal;
    }
    let num = |t: &Transaction, n: Option<&str>| n.map_or_else(|| t.num.clone().unwrap_or_default(), str::to_owned);
    ta.date_posted
        .cmp(&tb.date_posted)
        .then_with(|| is_closing(ta).cmp(&is_closing(tb)))
        .then_with(|| order_num(&num(ta, num_a), &num(tb, num_b)))
        .then_with(|| ta.date_entered.cmp(&tb.date_entered))
        .then_with(|| ta.description.as_deref().unwrap_or_default().cmp(tb.description.as_deref().unwrap_or_default()))
        .then_with(|| ta.guid.cmp(&tb.guid))
}

/// `xaccSplitOrder` (`5.17:libgnucash/engine/Split.cpp:1536-1597`): the order of an account's splits.
fn split_order(ta: &Transaction, sa: &Split, tb: &Transaction, sb: &Split, num_action: bool) -> Ordering {
    if ta.guid == tb.guid && sa.guid == sb.guid {
        return Ordering::Equal;
    }
    let by_txn = if num_action {
        trans_order(ta, Some(sa.action.as_deref().unwrap_or_default()), tb, Some(sb.action.as_deref().unwrap_or_default()))
    } else {
        trans_order(ta, None, tb, None)
    };
    by_txn
        .then_with(|| sa.memo.as_deref().unwrap_or_default().cmp(sb.memo.as_deref().unwrap_or_default()))
        .then_with(|| sa.action.as_deref().unwrap_or_default().cmp(sb.action.as_deref().unwrap_or_default()))
        .then_with(|| sa.reconcile_state.as_char().cmp(&sb.reconcile_state.as_char()))
        .then_with(|| sa.amount.cmp_value(sb.amount))
        .then_with(|| sa.value.cmp_value(sb.value))
        .then_with(|| sa.reconcile_date.unwrap_or_default().cmp(&sb.reconcile_date.unwrap_or_default()))
        .then_with(|| sa.guid.cmp(&sb.guid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_order_numerically_then_as_text() {
        assert_eq!(order_num("9", "10"), Ordering::Less);
        assert_eq!(order_num("10a", "10b"), Ordering::Less);
        assert_eq!(order_num("", "1"), Ordering::Less);
        assert_eq!(order_num("abc", "abd"), Ordering::Less);
    }
}
