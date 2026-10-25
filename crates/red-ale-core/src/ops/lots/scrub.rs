//! Lot assignment and lot repair: FIFO assignment of unlotted splits, cutting splits that cross a lot boundary,
//! refilling and merging, then gains (`5.17:libgnucash/engine/cap-gains.cpp`, `Scrub2.cpp`, `Scrub3.cpp`,
//! `policy.cpp`).

use super::gains::compute_lot_gains_in;
use super::ledger::{Entry, Ledger, Touched};
use super::{account_lots, edit_split, edit_txn, invoice_of_txn, locate, make_default_lot};
use crate::lot::{gains_source, gains_split, is_peer_split, split_slots};
use crate::ops::commit::{account_scu, round_amount, round_value, sort_splits};
use crate::{Book, DomainError, DomainResult, Env, Frame, Guid, How, Numeric, ReconcileState, Round, SlotValue, Split};

/// True when an account can hold trades, so its splits belong in lots (`xaccAccountHasTrades`,
/// `5.17:libgnucash/engine/cap-gains.cpp:79-99`): a STOCK, MUTUAL or CURRENCY account, or one with a split whose
/// transaction currency is not the account's commodity. Gains splits are not counted.
pub fn has_trades(book: &Book, account: Guid) -> bool {
    let Some(acc) = book.account(account) else { return false };
    if matches!(acc.account_type, crate::AccountType::Stock | crate::AccountType::Mutual | crate::AccountType::Currency) {
        return true;
    }
    book.splits_of(account).any(|(t, s)| gains_source(s).is_none() && acc.commodity.as_ref() != Some(&t.currency))
}

/// What happened to a split given to a lot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placed {
    /// The split joined the lot whole.
    Whole,
    /// The split joined the lot after being cut; the new split holds the rest and is in no lot.
    Cut(Guid),
    /// The lot is closed; nothing changed.
    LotClosed,
    /// The split is already in a lot or has a zero amount; nothing changed.
    Skipped,
}

/// Put a split in a lot as `xaccSplitAssignToLot` does (`5.17:libgnucash/engine/cap-gains.cpp:218-427`): whole when
/// the lot is empty, the split enlarges it, or it fits; otherwise cut in two so the first piece closes the lot.
pub fn assign_to_lot(book: &mut Book, env: &mut dyn Env, split: Guid, lot: Guid) -> DomainResult<Placed> {
    assign_to_lot_in(book, env, &mut Touched::default(), split, lot)
}

fn assign_to_lot_in(book: &mut Book, env: &mut dyn Env, touched: &mut Touched, split: Guid, lot: Guid) -> DomainResult<Placed> {
    let (_, s) = locate(book, split).ok_or_else(|| DomainError::not_found("split", &split))?;
    if s.lot.is_some() || s.amount.is_zero() {
        return Ok(Placed::Skipped);
    }
    let account = book.lot(lot).map(|l| l.account).ok_or_else(|| DomainError::not_found("lot", &lot))?;
    let ledger = Ledger::build_touched(book, account, touched)?;
    if ledger.lot_is_closed(lot)? {
        return Ok(Placed::LotClosed);
    }
    let baln = ledger.lot_balance(lot)?;
    let adjusted = ledger.entry(split).map_or(s.amount, |e| e.adjusted);
    let fits = adjusted.abs().cmp_value(baln.abs()).is_le();
    if baln.is_zero() || baln.is_positive() == s.amount.is_positive() || fits {
        edit_split(book, split, |s| s.lot = Some(lot))?;
        return Ok(Placed::Whole);
    }
    let piece = cut(book, env, split, lot, baln, adjusted)?;
    touched.add(split);
    touched.add(piece);
    Ok(Placed::Cut(piece))
}

/// Cut `split` so its first piece brings the lot balance `baln` to zero; the second piece is a new split in the same
/// transaction and account (`5.17:libgnucash/engine/cap-gains.cpp:321-426`).
fn cut(book: &mut Book, env: &mut dyn Env, split: Guid, lot: Guid, baln: Numeric, adjusted: Numeric) -> DomainResult<Guid> {
    let (txn, s) = locate(book, split).ok_or_else(|| DomainError::not_found("split", &split))?;
    let s = s.clone();
    let account = s.account.ok_or_else(|| DomainError::invalid("split has no account"))?;
    let currency = book.transaction(txn).map(|t| t.currency.clone()).ok_or_else(|| DomainError::not_found("transaction", &txn))?;
    // Restate the lot balance in the split's own units when a later stock split adjusts it.
    let baln = if s.amount == adjusted {
        baln
    } else {
        let frac = s.amount.div(adjusted, crate::DENOM_AUTO, How::REDUCE_NEVER)?;
        baln.mul(frac, account_scu(book, account)?, How::round(Round::HalfUp))?
    };
    let amt_a = baln.neg();
    let amt_b = s.amount.sub_fixed(amt_a)?;
    let frac = amt_a.div(s.amount, crate::DENOM_AUTO, How::REDUCE_NEVER)?;
    let val_a = frac.mul(s.value, s.value.denom(), How::new(crate::DenomMode::Exact, Round::HalfUp))?;
    let val_b = s.value.sub_fixed(val_a)?;
    if amt_a.is_zero() || amt_b.is_zero() {
        return Err(DomainError::invalid("cannot cut the split in two"));
    }
    let (amt_a, val_a) = (round_amount(book, account, amt_a)?, round_value(book, &currency, val_a)?);
    let (amt_b, val_b) = (round_amount(book, account, amt_b)?, round_value(book, &currency, val_b)?);
    let mut piece = Split::new(env.new_guid(), account, val_b, amt_b);
    piece.memo.clone_from(&s.memo);
    piece.action.clone_from(&s.action);
    piece.reconcile_state = s.reconcile_state;
    piece.reconcile_date = s.reconcile_date;
    let now = env.now();
    piece.slots.set(split_slots::LOT_SPLIT, peer_frame(split, now));
    let piece_guid = piece.guid;
    edit_txn(book, txn, |t| {
        if let Some(first) = t.split_mut(split) {
            first.amount = amt_a;
            first.value = val_a;
            first.lot = Some(lot);
            first.slots.set(split_slots::LOT_SPLIT, peer_frame(piece_guid, now));
        }
        t.splits.push(piece);
        sort_splits(t);
    })?;
    Ok(piece_guid)
}

/// The `lot-split` frame naming a peer (`qof_instance_kvp_add_guid`, `5.17:libgnucash/engine/qofinstance.cpp:1130-1141`).
fn peer_frame(peer: Guid, now: crate::Time64) -> SlotValue {
    let mut f = Frame::new();
    f.insert_key(split_slots::PEER_GUID, SlotValue::Guid(peer));
    f.insert_key(split_slots::DATE, SlotValue::Timespec(now));
    SlotValue::Frame(f)
}

/// FIFO's lot for a split (`xaccAccountFindEarliestOpenLot`, `5.17:libgnucash/engine/cap-gains.cpp:124-200`): of the
/// open lots whose opening split has the opposite sign to the split, are not overfull and share its transaction
/// currency, the one opened first; ties go to the first in the account's lot list.
pub fn fifo_lot(book: &Book, ledger: &Ledger, split: &Entry) -> DomainResult<Option<Guid>> {
    let want_negative_opening = split.adjusted.is_positive();
    let mut best: Option<(Guid, crate::Time64)> = None;
    account_lots(book, ledger.account).into_iter().try_for_each(|lot| -> DomainResult<()> {
        if ledger.lot_is_closed(lot)? {
            return Ok(());
        }
        let Some(e) = ledger.earliest(lot) else { return Ok(()) };
        let opposite = if want_negative_opening { e.amount.is_negative() } else { e.amount.is_positive() };
        if !opposite || e.amount.is_positive() != ledger.lot_balance(lot)?.is_positive() || e.currency != split.currency {
            return Ok(());
        }
        if best.is_none_or(|(_, t)| t > e.posted) {
            best = Some((lot, e.posted));
        }
        Ok(())
    })?;
    Ok(best.map(|(lot, _)| lot))
}

/// FIFO's next split to fill a lot (`DirectionPolicyGetSplit`, `5.17:libgnucash/engine/policy.cpp:67-114`): the first
/// unlotted split in account order, posted no earlier than the lot's latest split, in the lot's currency, with a
/// non-zero amount of the sign that shrinks the lot.
fn fifo_split(ledger: &Ledger, lot: Guid) -> DomainResult<Option<Guid>> {
    if ledger.lot_entries(lot).is_empty() || ledger.lot_is_closed(lot)? {
        return Ok(None);
    }
    let want_positive = ledger.lot_balance(lot)?.is_negative();
    let (Some(first), Some(latest)) = (ledger.earliest(lot), ledger.latest(lot)) else { return Ok(None) };
    Ok(ledger
        .entries
        .iter()
        .find(|e| {
            e.lot.is_none() && e.posted >= latest.posted && e.currency == first.currency && !e.amount.is_zero() && e.amount.is_positive() == want_positive
        })
        .map(|e| e.split))
}

/// Assign a split to lots by FIFO, cutting it across lots as needed (`xaccSplitAssign`,
/// `5.17:libgnucash/engine/cap-gains.cpp:435-485`). Returns every piece with its lot, and whether the split was cut.
pub fn split_assign(book: &mut Book, env: &mut dyn Env, split: Guid) -> DomainResult<Vec<(Guid, Guid)>> {
    split_assign_in(book, env, &mut Touched::default(), split)
}

fn split_assign_in(book: &mut Book, env: &mut dyn Env, touched: &mut Touched, split: Guid) -> DomainResult<Vec<(Guid, Guid)>> {
    let (_, s) = locate(book, split).ok_or_else(|| DomainError::not_found("split", &split))?;
    let Some(account) = s.account else { return Ok(Vec::new()) };
    if s.lot.is_some() || gains_source(s).is_some() || !has_trades(book, account) || s.amount.is_zero() {
        return Ok(Vec::new());
    }
    let mut pieces = Vec::new();
    let mut current = Some(split);
    while let Some(piece) = current {
        let ledger = Ledger::build_touched(book, account, touched)?;
        let entry = ledger.entry(piece).cloned().ok_or_else(|| DomainError::not_found("split", &piece))?;
        let lot = match fifo_lot(book, &ledger, &entry)? {
            Some(lot) => lot,
            None => make_default_lot(book, env, account)?,
        };
        current = match assign_to_lot_in(book, env, touched, piece, lot)? {
            Placed::Whole => {
                pieces.push((piece, lot));
                None
            }
            Placed::Cut(rest) => {
                pieces.push((piece, lot));
                Some(rest)
            }
            Placed::LotClosed | Placed::Skipped => None,
        };
    }
    Ok(pieces)
}

/// Put every unlotted split of an account into lots (`xaccAccountAssignLots`, `5.17:libgnucash/engine/Scrub2.cpp:58-83`).
/// Stock-split splits and zeroed splits of void transactions stay out.
pub fn assign_lots(book: &mut Book, env: &mut dyn Env, account: Guid) -> DomainResult<()> {
    assign_lots_in(book, env, &mut Touched::default(), account)
}

fn assign_lots_in(book: &mut Book, env: &mut dyn Env, touched: &mut Touched, account: Guid) -> DomainResult<()> {
    let mut done: Vec<Guid> = Vec::new();
    loop {
        let ledger = Ledger::build_touched(book, account, touched)?;
        let next = ledger
            .entries
            .iter()
            .find(|e| e.lot.is_none() && !e.stock_split && (!e.amount.is_zero() || !e.voided) && !done.contains(&e.split))
            .map(|e| e.split);
        let Some(split) = next else { return Ok(()) };
        done.push(split);
        split_assign_in(book, env, touched, split)?;
    }
}

/// Fill a lot from unlotted splits until it closes (`xaccLotFill`, `5.17:libgnucash/engine/Scrub2.cpp:94-166`).
fn lot_fill(book: &mut Book, env: &mut dyn Env, touched: &mut Touched, account: Guid, lot: Guid) -> DomainResult<()> {
    let ledger = Ledger::build_touched(book, account, touched)?;
    let Some(first) = fifo_split(&ledger, lot)? else { return Ok(()) };
    let entry = ledger.entry(first).cloned();
    if entry.as_ref().is_some_and(|e| e.stock_split || (e.amount.is_zero() && e.voided)) {
        return Ok(());
    }
    let mut split = first;
    loop {
        if assign_to_lot_in(book, env, touched, split, lot)? == Placed::LotClosed {
            return Ok(());
        }
        let ledger = Ledger::build_touched(book, account, touched)?;
        if ledger.lot_is_closed(lot)? {
            return Ok(());
        }
        match fifo_split(&ledger, lot)? {
            Some(next) if next != split => split = next,
            _ => return Ok(()),
        }
    }
}

/// Merge the pieces of cut splits that ended up in the same lot and transaction (`xaccScrubMergeLotSubSplits`,
/// `5.17:libgnucash/engine/Scrub2.cpp:390-409`). Returns whether anything merged.
fn merge_lot_sub_splits(book: &mut Book, touched: &mut Touched, account: Guid, lot: Guid) -> DomainResult<bool> {
    let mut merged = false;
    'restart: loop {
        let splits: Vec<Guid> = Ledger::build(book, account)?.lot_entries(lot).iter().map(|e| e.split).collect();
        for split in splits {
            if merge_sub_splits(book, touched, split, lot)? {
                merged = true;
                continue 'restart;
            }
        }
        return Ok(merged);
    }
}

/// Merge into `split` the other splits of its transaction in the same lot that it names as peers
/// (`xaccScrubMergeSubSplits`, `5.17:libgnucash/engine/Scrub2.cpp:325-388`). Invoice transactions are left to the
/// business code.
fn merge_sub_splits(book: &mut Book, touched: &mut Touched, split: Guid, lot: Guid) -> DomainResult<bool> {
    let Some((txn, s)) = locate(book, split) else { return Ok(false) };
    if s.slots.get_key(split_slots::LOT_SPLIT).is_none() || invoice_of_txn(book, txn).is_some() {
        return Ok(false);
    }
    let Some(t) = book.transaction(txn) else { return Ok(false) };
    let peer = t.splits.iter().find(|o| o.guid != split && o.lot == Some(lot) && is_peer_split(s, o.guid)).map(|o| o.guid);
    let Some(peer) = peer else { return Ok(false) };
    merge_splits(book, txn, split, peer)?;
    touched.add(split);
    Ok(true)
}

/// Fold `sb` into `sa` and delete it (`merge_splits`, `5.17:libgnucash/engine/Scrub2.cpp:261-322`). `sb`'s gains
/// transaction is deleted too. GnuCash 5.17 deletes it only when it has already looked at `sb`'s gains in the same
/// session, so after a fresh open it leaves the gains transaction behind and counts the gain twice; red-ale deletes it
/// always.
fn merge_splits(book: &mut Book, txn: crate::Guid, sa: Guid, sb: Guid) -> DomainResult<()> {
    let t = book.transaction(txn).ok_or_else(|| DomainError::not_found("transaction", &txn))?;
    let (a, b) = (t.split(sa).cloned(), t.split(sb).cloned());
    let (Some(a), Some(b)) = (a, b) else { return Ok(()) };
    let account = a.account.ok_or_else(|| DomainError::invalid("split has no account"))?;
    let amount = round_amount(book, account, a.amount.add_fixed(b.amount)?)?;
    let value = round_value(book, &t.currency, a.value.add_fixed(b.value)?)?;
    let gains_txn = gains_split(&b).and_then(|g| locate(book, g)).filter(|(_, g)| gains_source(g).is_some()).map(|(t, _)| t);
    edit_txn(book, txn, |t| {
        let b_lot_split = t.split(sb).and_then(|s| s.slots.get_key(split_slots::LOT_SPLIT).cloned());
        if let Some(s) = t.split_mut(sa) {
            if is_peer_split(s, sb) {
                s.slots.remove(split_slots::LOT_SPLIT);
            }
            // The peer's links move over only when `sa` has none (qof_instance_kvp_merge_guids,
            // 5.17:libgnucash/engine/qofinstance.cpp:1236-1262, drops the merged list it builds otherwise).
            let b_keeps = b_lot_split.filter(|v| !matches_peer(v, sa));
            if let (None, Some(v)) = (s.slots.get_key(split_slots::LOT_SPLIT), b_keeps) {
                s.slots.insert_key(split_slots::LOT_SPLIT, v);
            }
            s.amount = amount;
            s.value = value;
            s.reconcile_state = ReconcileState::New;
        }
        t.splits.retain(|s| s.guid != sb);
        sort_splits(t);
    })?;
    if let Some(g) = gains_txn.filter(|g| *g != txn) {
        book.transactions.shift_remove(&g);
    }
    Ok(())
}

/// True when a `lot-split` value names `peer` as a single frame.
fn matches_peer(v: &SlotValue, peer: Guid) -> bool {
    v.as_frame().and_then(|f| f.get_key(split_slots::PEER_GUID)).and_then(SlotValue::as_guid) == Some(peer)
}

/// Repair one lot (`xaccScrubLot`, `5.17:libgnucash/engine/Scrub3.cpp:84-154`): merge cut pieces; if the balance is
/// not zero, empty an overfull lot of all but its opening split and refill it; then, when gains are possible,
/// compute them.
pub fn scrub_lot(book: &mut Book, env: &mut dyn Env, lot: Guid) -> DomainResult<()> {
    scrub_lot_in(book, env, &mut Touched::default(), lot)
}

fn scrub_lot_in(book: &mut Book, env: &mut dyn Env, touched: &mut Touched, lot: Guid) -> DomainResult<()> {
    let account = book.lot(lot).map(|l| l.account).ok_or_else(|| DomainError::not_found("lot", &lot))?;
    merge_lot_sub_splits(book, touched, account, lot)?;
    let ledger = Ledger::build_touched(book, account, touched)?;
    let balance = ledger.lot_balance(lot)?;
    if !balance.is_zero() {
        let opening = ledger.earliest(lot).map(|e| (e.split, e.adjusted));
        if let Some((opening, amount)) = opening
            && amount.is_positive() != balance.is_positive()
        {
            let others: Vec<Guid> = ledger.lot_entries(lot).iter().map(|e| e.split).filter(|s| *s != opening).collect();
            others.into_iter().try_for_each(|s| edit_split(book, s, |s| s.lot = None))?;
        }
        lot_fill(book, env, touched, account, lot)?;
        merge_lot_sub_splits(book, touched, account, lot)?;
    }
    if gains_possible(book, account, lot)? {
        compute_lot_gains_in(book, env, touched, lot, None)?;
    }
    Ok(())
}

/// Gains are possible when the lot's first split is in a transaction whose currency is not the account's commodity
/// (`gains_possible`, `5.17:libgnucash/engine/Scrub3.cpp:58-76`).
fn gains_possible(book: &Book, account: Guid, lot: Guid) -> DomainResult<bool> {
    let ledger = Ledger::build(book, account)?;
    let commodity = book.account(account).and_then(|a| a.commodity.clone());
    Ok(ledger.earliest(lot).is_some_and(|e| commodity.as_ref() != Some(&e.currency)))
}

/// Assign an account's unlotted splits to lots and repair each of its lots (`xaccAccountScrubLots`,
/// `5.17:libgnucash/engine/Scrub3.cpp:158-178`). Accounts without trades are left alone.
pub fn scrub_account_lots(book: &mut Book, env: &mut dyn Env, account: Guid) -> DomainResult<()> {
    if !has_trades(book, account) {
        return Ok(());
    }
    let mut touched = Touched::default();
    assign_lots_in(book, env, &mut touched, account)?;
    account_lots(book, account)
        .into_iter()
        .try_for_each(|lot| if book.lots.contains_key(&lot) { scrub_lot_in(book, env, &mut touched, lot) } else { Ok(()) })
}

/// Scrub the lots of every account under `account`, then of `account` itself (`xaccAccountTreeScrubLots`,
/// `5.17:libgnucash/engine/Scrub3.cpp:190-196`).
pub fn scrub_tree_lots(book: &mut Book, env: &mut dyn Env, account: Guid) -> DomainResult<()> {
    let accounts: Vec<Guid> = book.descendants(account).iter().map(|a| a.guid).chain(std::iter::once(account)).collect();
    accounts.into_iter().try_for_each(|a| scrub_account_lots(book, env, a))
}
