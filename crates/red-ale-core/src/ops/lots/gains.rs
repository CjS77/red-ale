//! Capital gains: the realised gain of each selling split, recorded in a gains transaction
//! (`5.17:libgnucash/engine/cap-gains.cpp`).

use std::sync::Arc;

use super::ledger::{Ledger, Touched, trans_order};
use super::{edit_split, edit_txn, locate};
use crate::account::slot_paths as acct_slots;
use crate::lot::{account_slots, gains_source, gains_split, is_stock_split, split_slots};
use crate::ops::commit::{commodity_fraction, round_amount, round_value, sort_splits};
use crate::{
    Account, AccountType, Book, CommodityRef, DomainResult, Env, Guid, How, Numeric, Round, SlotValue, Split, Time64, Transaction,
};

/// The description and memo of a gains transaction (`5.17:libgnucash/engine/cap-gains.cpp:800-806`).
pub const REALIZED_GAIN_LOSS: &str = "Realized Gain/Loss";
/// The name of the income account that takes gains when no gains account is set, before `-<mnemonic>`
/// (`5.17:libgnucash/engine/Account.cpp:4771`).
pub const ORPHANED_GAINS: &str = "Orphaned Gains";
/// The notes of a new orphaned-gains account (`5.17:libgnucash/engine/Account.cpp:4785-4788`).
pub const ORPHANED_GAINS_NOTES: &str = "Realized Gains or Losses from Commodity or Trading Accounts that haven't been recorded elsewhere.";

/// The gain a split realises against a lot holding `lot_amount` worth `lot_value` before it, or `None` where GnuCash
/// computes none: the lot is too thin or too fat for the split, or the arithmetic fails
/// (`5.17:libgnucash/engine/cap-gains.cpp:668-747`).
fn gain_against(lot_amount: Numeric, lot_value: Numeric, adjusted: Numeric, opening_value: Numeric, value: Numeric) -> Option<Numeric> {
    if lot_amount.abs().cmp_value(adjusted.abs()).is_lt() {
        return None;
    }
    if (lot_amount.is_negative() || adjusted.is_positive()) && (lot_amount.is_positive() || adjusted.is_negative()) {
        return None;
    }
    let frac = adjusted.div(lot_amount, crate::DENOM_AUTO, How::REDUCE_NEVER).ok()?;
    let basis = frac
        .mul(lot_value, opening_value.denom(), How::new(crate::DenomMode::Exact, Round::HalfUp))
        .ok()?;
    basis.sub(value, crate::DENOM_AUTO, How::FIXED_NEVER).ok()
}

/// The split a gains split records, if it still exists.
fn source_of(book: &Book, split: &Split) -> Option<Guid> {
    gains_source(split).filter(|g| locate(book, *g).is_some())
}

/// The amount and value a lot held before `split` (`gnc_lot_get_balance_before`,
/// `5.17:libgnucash/engine/gnc-lot.cpp:529-576`). Gains splits count at their source's place.
pub fn balance_before(book: &Book, ledger: &Ledger, lot: Guid, split: Guid) -> DomainResult<(Numeric, Numeric)> {
    let Some((_, s)) = locate(book, split) else {
        return Ok((Numeric::ZERO, Numeric::ZERO));
    };
    let target = source_of(book, s).unwrap_or(split);
    let Some((tb, _)) = locate(book, target) else {
        return Ok((Numeric::ZERO, Numeric::ZERO));
    };
    let tb = book.transaction(tb).cloned();
    let Some(tb) = tb else { return Ok((Numeric::ZERO, Numeric::ZERO)) };
    ledger
        .lot_entries(lot)
        .iter()
        .try_fold((Numeric::ZERO, Numeric::ZERO), |(amount, value), e| {
            let source = locate(book, e.split).and_then(|(_, s)| source_of(book, s)).unwrap_or(e.split);
            let ta = locate(book, source).and_then(|(t, _)| book.transaction(t));
            let before = ta.is_some_and(|ta| (ta.guid == tb.guid && source != target) || trans_order(ta, None, &tb, None).is_lt());
            if before {
                Ok((amount.add_fixed(e.adjusted)?, value.add_fixed(e.value)?))
            } else {
                Ok((amount, value))
            }
        })
}

/// What `xaccSplitComputeCapGains` needs to know about the split whose gain is computed.
struct Sale {
    split: Guid,
    lot: Guid,
    account: Guid,
    currency: CommodityRef,
    posted: Time64,
    gain: Numeric,
    gains_split: Option<Guid>,
}

/// Compute and record the capital gain of one split (`xaccSplitComputeCapGains`,
/// `5.17:libgnucash/engine/cap-gains.cpp:521-891`). A gains split computes its source's gain. `gain_account` is used
/// for a new gains transaction when its commodity is the transaction currency; otherwise the lot account's gains
/// account is used, made if needed.
pub fn compute_split_gains(book: &mut Book, env: &mut dyn Env, split: Guid, gain_account: Option<Guid>) -> DomainResult<()> {
    compute_split_gains_in(book, env, &Touched::default(), split, gain_account)
}

fn compute_split_gains_in(
    book: &mut Book,
    env: &mut dyn Env,
    touched: &Touched,
    split: Guid,
    gain_account: Option<Guid>,
) -> DomainResult<()> {
    let Some(sale) = sale(book, touched, split)? else { return Ok(()) };
    match sale.gains_split.filter(|g| locate(book, *g).is_some()) {
        None => create_gains_txn(book, env, &sale, gain_account),
        Some(lot_split) => update_gains_txn(book, env, &sale, lot_split),
    }
}

/// The sale whose gain `split` determines, or `None` where GnuCash records nothing.
fn sale(book: &Book, touched: &Touched, split: Guid) -> DomainResult<Option<Sale>> {
    let Some((txn, s)) = locate(book, split) else {
        return Err(crate::DomainError::not_found("split", &split));
    };
    let (Some(lot), Some(account)) = (s.lot, s.account) else {
        return Ok(None);
    };
    let Some(lot_account) = book.lot(lot).map(|l| l.account) else {
        return Ok(None);
    };
    let currency = book
        .transaction(txn)
        .map(|t| t.currency.clone())
        .ok_or_else(|| crate::DomainError::not_found("transaction", &txn))?;
    if book.account(account).and_then(|a| a.commodity.as_ref()) == Some(&currency) {
        return Ok(None);
    }
    let ledger = Ledger::build_touched(book, lot_account, touched)?;
    if ledger.earliest(lot).map(|e| e.split) == Some(split) || is_stock_split(s) {
        return Ok(None);
    }
    // A gains split stands for the split whose gain it records (cap-gains.cpp:586-612).
    let source = match gains_source(s) {
        None => split,
        Some(_) => match source_of(book, s).or_else(|| gains_split(s).filter(|g| locate(book, *g).is_some())) {
            Some(g) => g,
            None => return Ok(None),
        },
    };
    let Some((src_txn, src)) = locate(book, source) else {
        return Ok(None);
    };
    if src.amount.is_zero() {
        return Ok(None);
    }
    let adjusted = ledger.entry(source).map_or(src.amount, |e| e.adjusted);
    let (lot_amount, lot_value) = balance_before(book, &ledger, lot, source)?;
    let Some(opening) = ledger.earliest(lot) else { return Ok(None) };
    if opening.currency != currency {
        return Ok(None);
    }
    let Some(gain) = gain_against(lot_amount, lot_value, adjusted, opening.value, src.value).filter(|g| !g.is_zero()) else {
        return Ok(None);
    };
    let posted = book.transaction(src_txn).map_or(Time64(0), |t| t.date_posted);
    Ok(Some(Sale {
        split: source,
        lot,
        account: lot_account,
        currency,
        posted,
        gain,
        gains_split: gains_split(src),
    }))
}

/// A new gains transaction for `sale` (`5.17:libgnucash/engine/cap-gains.cpp:769-821`, `:862-887`).
fn create_gains_txn(book: &mut Book, env: &mut dyn Env, sale: &Sale, gain_account: Option<Guid>) -> DomainResult<()> {
    let gain_acc = match gain_account {
        Some(g) if book.account(g).and_then(|a| a.commodity.as_ref()) == Some(&sale.currency) => g,
        _ => gains_account(book, env, sale.account, &sale.currency)?,
    };
    let neg = sale.gain.neg();
    let mut lot_split = Split::new(
        env.new_guid(),
        sale.account,
        round_value(book, &sale.currency, sale.gain)?,
        round_amount(book, sale.account, Numeric::ZERO)?,
    );
    lot_split.memo = Some(REALIZED_GAIN_LOSS.to_owned());
    lot_split.lot = Some(sale.lot);
    lot_split.slots.set(split_slots::GAINS_SOURCE, SlotValue::Guid(sale.split));
    let mut gain_split = Split::new(
        env.new_guid(),
        gain_acc,
        round_value(book, &sale.currency, neg)?,
        round_amount(book, gain_acc, neg)?,
    );
    gain_split.memo = Some(REALIZED_GAIN_LOSS.to_owned());
    let lot_split_guid = lot_split.guid;
    let mut t = Transaction {
        guid: env.new_guid(),
        currency: sale.currency.clone(),
        num: None,
        date_posted: sale.posted,
        date_entered: env.now(),
        description: Some(REALIZED_GAIN_LOSS.to_owned()),
        slots: crate::Frame::new(),
        splits: vec![lot_split, gain_split],
    };
    sort_splits(&mut t);
    book.transactions.insert(t.guid, Arc::new(t));
    edit_split(book, sale.split, |s| {
        s.slots.set(split_slots::GAINS_SPLIT, SlotValue::Guid(lot_split_guid))
    })
}

/// Bring an existing gains transaction up to date (`5.17:libgnucash/engine/cap-gains.cpp:822-887`). A gains
/// transaction edited to hold other than two splits is left alone. Unlike GnuCash, which rewrites every gains
/// transaction the first time it meets it in a session, red-ale writes only when something differs.
fn update_gains_txn(book: &mut Book, env: &mut dyn Env, sale: &Sale, lot_split: Guid) -> DomainResult<()> {
    let Some((txn, ls)) = locate(book, lot_split) else { return Ok(()) };
    let Some(t) = book.transaction(txn) else { return Ok(()) };
    let Some(gs_guid) = crate::ops::transactions::other_split(book, t, lot_split).map(|s| s.guid) else {
        return Ok(());
    };
    let Some(gs) = t.split(gs_guid) else { return Ok(()) };
    let neg = sale.gain.neg();
    let correct = gains_source(ls) == Some(sale.split)
        && ls.value == sale.gain
        && ls.amount.is_zero()
        && gs.value == neg
        && gs.amount == neg
        && t.date_posted == sale.posted
        && t.currency == sale.currency
        && ls.lot == Some(sale.lot);
    if correct {
        return Ok(());
    }
    let gs_account = gs.account;
    let ls_value = round_value(book, &sale.currency, sale.gain)?;
    let ls_amount = round_amount(book, sale.account, Numeric::ZERO)?;
    let gs_value = round_value(book, &sale.currency, neg)?;
    let gs_amount = match gs_account {
        Some(a) => round_amount(book, a, neg)?,
        None => neg,
    };
    let now = env.now();
    edit_txn(book, txn, |t| {
        t.currency = sale.currency.clone();
        t.date_posted = sale.posted;
        t.date_entered = now;
        t.splits.iter_mut().for_each(|s| {
            if s.guid == lot_split {
                s.amount = ls_amount;
                s.value = ls_value;
                s.lot = Some(sale.lot);
            } else if s.guid == gs_guid {
                s.amount = gs_amount;
                s.value = gs_value;
            }
        });
        sort_splits(t);
    })
}

/// Compute the gains of every split in a lot (`xaccLotComputeCapGains`, `5.17:libgnucash/engine/cap-gains.cpp:1070-1112`).
pub fn compute_lot_gains(book: &mut Book, env: &mut dyn Env, lot: Guid, gain_account: Option<Guid>) -> DomainResult<()> {
    compute_lot_gains_in(book, env, &Touched::default(), lot, gain_account)
}

/// [`compute_lot_gains`] within a larger operation that has set the amounts of `touched`.
pub(super) fn compute_lot_gains_in(
    book: &mut Book,
    env: &mut dyn Env,
    touched: &Touched,
    lot: Guid,
    gain_account: Option<Guid>,
) -> DomainResult<()> {
    let account = book
        .lot(lot)
        .map(|l| l.account)
        .ok_or_else(|| crate::DomainError::not_found("lot", &lot))?;
    let splits: Vec<Guid> = Ledger::build_touched(book, account, touched)?
        .lot_entries(lot)
        .iter()
        .map(|e| e.split)
        .collect();
    splits.into_iter().try_for_each(|s| {
        if locate(book, s).is_some() {
            compute_split_gains_in(book, env, touched, s, gain_account)
        } else {
            Ok(())
        }
    })
}

/// The gain a split not in `lot` would realise against it, for the lot viewer; nothing is written
/// (`xaccLotFreeSplitCapGain`, `5.17:libgnucash/engine/cap-gains.cpp:895-1033`).
pub fn free_split_gain(book: &Book, split: Guid, lot: Guid) -> DomainResult<Numeric> {
    let Some((txn, s)) = locate(book, split) else {
        return Err(crate::DomainError::not_found("split", &split));
    };
    let account = book
        .lot(lot)
        .map(|l| l.account)
        .ok_or_else(|| crate::DomainError::not_found("lot", &lot))?;
    let Some(t) = book.transaction(txn) else { return Ok(Numeric::ZERO) };
    if s.lot.is_some() || s.account.and_then(|a| book.account(a)).and_then(|a| a.commodity.as_ref()) == Some(&t.currency) {
        return Ok(Numeric::ZERO);
    }
    let ledger = Ledger::build(book, account)?;
    let Some(opening) = ledger.earliest(lot) else {
        return Ok(Numeric::ZERO);
    };
    if opening.posted >= t.date_posted || is_stock_split(s) || s.amount.is_zero() || opening.currency != t.currency {
        return Ok(Numeric::ZERO);
    }
    let adjusted = ledger.entry(split).map_or(s.amount, |e| e.adjusted);
    let (lot_amount, lot_value) = balance_before(book, &ledger, lot, split)?;
    Ok(gain_against(lot_amount, lot_value, adjusted, opening.value, s.value).unwrap_or(Numeric::ZERO))
}

/// The gain recorded for a split, without recomputing it: a gains split's own value, or the value of the gains split
/// a selling split names (`xaccSplitGetCapGains` on clean splits, `5.17:libgnucash/engine/cap-gains.cpp:1037-1065`).
pub fn recorded_gain(book: &Book, split: &Split) -> Option<Numeric> {
    if gains_source(split).is_some() {
        return Some(split.value);
    }
    gains_split(split).and_then(|g| locate(book, g)).map(|(_, s)| s.value)
}

/// The account that takes `lot_account`'s gains in `currency` (`xaccAccountGainsAccount`,
/// `5.17:libgnucash/engine/Account.cpp:4805-4817`): the one its `lot-mgmt/gains-acct/<currency>` slot names, else an
/// income account `Orphaned Gains-<mnemonic>` found by name or made under the root, which the slot then names.
pub fn gains_account(book: &mut Book, env: &mut dyn Env, lot_account: Guid, currency: &CommodityRef) -> DomainResult<Guid> {
    let key = currency.to_string();
    let account = book
        .account(lot_account)
        .ok_or_else(|| crate::DomainError::not_found("account", &lot_account))?;
    let named = account
        .slots
        .get_key(account_slots::LOT_MGMT)
        .and_then(SlotValue::as_frame)
        .and_then(|f| f.get_key(account_slots::GAINS_ACCT))
        .and_then(SlotValue::as_frame)
        .and_then(|f| f.get_key(&key))
        .and_then(SlotValue::as_guid)
        .filter(|g| book.account(*g).is_some());
    if let Some(g) = named {
        return Ok(g);
    }
    let name = format!("{ORPHANED_GAINS}-{}", currency.mnemonic);
    let gains = match lookup_by_name(book, book.root, &name) {
        Some(g) => g,
        None => make_orphan_account(book, env, &name, currency)?,
    };
    let path = format!("{}/{}/{key}", account_slots::LOT_MGMT, account_slots::GAINS_ACCT);
    let acc = book
        .accounts
        .get_mut(&lot_account)
        .ok_or_else(|| crate::DomainError::not_found("account", &lot_account))?;
    Arc::make_mut(acc).slots.set(&path, SlotValue::Guid(gains));
    Ok(gains)
}

/// `gnc_account_lookup_by_name`: children first, then each child's subtree (`5.17:libgnucash/engine/Account.cpp:3081`).
pub(crate) fn lookup_by_name(book: &Book, parent: Guid, name: &str) -> Option<Guid> {
    let children: Vec<&Account> = book.children(parent).collect();
    children
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.guid)
        .or_else(|| children.iter().find_map(|c| lookup_by_name(book, c.guid, name)))
}

/// `GetOrMakeOrphanAccount`'s new account (`5.17:libgnucash/engine/Account.cpp:4760-4800`).
fn make_orphan_account(book: &mut Book, env: &mut dyn Env, name: &str, currency: &CommodityRef) -> DomainResult<Guid> {
    let fraction = commodity_fraction(book, currency).ok_or_else(|| crate::DomainError::not_found("commodity", currency))?;
    let mut account = Account::new(env.new_guid(), name, AccountType::Income, Some(currency.clone()), Some(book.root));
    account.commodity_scu = Some(fraction);
    account.description = Some(REALIZED_GAIN_LOSS.to_owned());
    account
        .slots
        .set(acct_slots::NOTES, SlotValue::String(ORPHANED_GAINS_NOTES.to_owned()));
    let guid = account.guid;
    book.accounts.insert(guid, Arc::new(account));
    Ok(guid)
}
