//! Void, unvoid and reverse: `xaccTransVoid`, `xaccTransUnvoid` and `xaccTransReverse` with the checks GnuCash's
//! register applies around them. See `design/notes/engine-accounts-transactions.md` §6.

use std::sync::Arc;

use chrono::DateTime;

use super::{check_writable, get};
use crate::ops::{commit, trading};
use crate::transaction::slot_paths;
use crate::{Book, Date, DomainError, DomainResult, Env, Guid, Numeric, ReconcileState, SlotValue, Split, Time64, Transaction};

/// The notes a voided transaction gets (`_("Voided transaction")`, untranslated).
pub const VOIDED_NOTES: &str = "Voided transaction";
/// The read-only reason a voided transaction gets (`_("Transaction Voided")`, untranslated).
pub const VOIDED_READ_ONLY: &str = "Transaction Voided";

/// `gnc_time64_to_iso8601_buff`: `YYYY-MM-DD HH:MM:SS` in UTC, without a zone (`5.17:libgnucash/engine/gnc-date.cpp:1212-1234`,
/// `5.17:libgnucash/engine/gnc-datetime.cpp:631-636`).
pub fn void_time_string(t: Time64) -> String {
    DateTime::from_timestamp(t.0, 0).map_or_else(String::new, |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Refuse what GnuCash's register refuses before voiding (`5.17:gnucash/gnome/gnc-plugin-page-register.cpp:2453-2468`):
/// a transaction already voided, one with cleared or reconciled splits, and a read-only one. The engine also refuses
/// a read-only transaction (`5.17:libgnucash/engine/Transaction.cpp:2507-2514`).
fn check_voidable(t: &Transaction) -> DomainResult<()> {
    if t.splits.iter().any(|s| s.reconcile_state == ReconcileState::Voided) {
        return Err(DomainError::conflict(format!("transaction {} is already void", t.guid)));
    }
    if t.splits.iter().any(|s| matches!(s.reconcile_state, ReconcileState::Cleared | ReconcileState::Reconciled | ReconcileState::Frozen)) {
        return Err(DomainError::conflict("you cannot void a transaction with reconciled or cleared splits"));
    }
    check_writable(t)
}

/// `xaccSplitVoid`: keep the amount and value in slots, set both to zero at the account's SCU and the currency's
/// fraction, and mark the split `v`; the reconcile date is kept (`5.17:libgnucash/engine/Split.cpp:2206-2218`).
fn void_split(book: &Book, t_currency: &crate::CommodityRef, s: &mut Split) -> DomainResult<()> {
    s.slots.set(slot_paths::VOID_FORMER_AMOUNT, SlotValue::Numeric(s.amount));
    s.slots.set(slot_paths::VOID_FORMER_VALUE, SlotValue::Numeric(s.value));
    s.amount = match s.account {
        Some(a) => commit::round_amount(book, a, Numeric::ZERO)?,
        None => Numeric::ZERO,
    };
    s.value = commit::round_value(book, t_currency, Numeric::ZERO)?;
    s.reconcile_state = ReconcileState::Voided;
    Ok(())
}

/// Void a transaction (`xaccTransVoid`, `5.17:libgnucash/engine/Transaction.cpp:2502-2531`): former notes kept in
/// `void-former-notes`, notes set to "Voided transaction", `void-reason` set (an empty reason writes none),
/// `void-time` set to now, every split voided, and the transaction made read-only.
pub fn void(book: &mut Book, env: &mut dyn Env, guid: Guid, reason: &str) -> DomainResult<()> {
    let mut t = get(book, guid)?.clone();
    check_voidable(&t)?;
    if let Some(notes) = t.slots.get_str(slot_paths::NOTES).map(str::to_owned) {
        t.slots.set(slot_paths::VOID_FORMER_NOTES, SlotValue::String(notes));
    }
    t.slots.set_opt_str(slot_paths::NOTES, Some(VOIDED_NOTES));
    t.slots.set_opt_str(slot_paths::VOID_REASON, Some(reason));
    t.slots.set_opt_str(slot_paths::VOID_TIME, Some(&void_time_string(env.now())));
    let currency = t.currency.clone();
    t.splits.iter_mut().try_for_each(|s| void_split(book, &currency, s))?;
    t.slots.set_opt_str(slot_paths::READ_ONLY, Some(VOIDED_READ_ONLY));
    commit::commit(book, env, &mut t)?;
    book.transactions.insert(guid, Arc::new(t));
    Ok(())
}

/// `xaccSplitUnvoid`: restore amount and value from the slots (zero when missing), reset the state to `n` and delete
/// the slots (`5.17:libgnucash/engine/Split.cpp:2220-2231`).
fn unvoid_split(s: &mut Split) {
    let former = |s: &Split, path| s.slots.get(path).and_then(SlotValue::as_numeric).unwrap_or(Numeric::ZERO);
    s.amount = former(s, slot_paths::VOID_FORMER_AMOUNT);
    s.value = former(s, slot_paths::VOID_FORMER_VALUE);
    s.reconcile_state = ReconcileState::New;
    s.slots.remove(slot_paths::VOID_FORMER_AMOUNT);
    s.slots.remove(slot_paths::VOID_FORMER_VALUE);
}

/// Unvoid a transaction (`xaccTransUnvoid`, `5.17:libgnucash/engine/Transaction.cpp:2553-2573`): notes restored from
/// `void-former-notes` (deleted when there were none), the void slots deleted, splits restored and the read-only mark
/// cleared. A transaction that is not void is refused, where GnuCash silently does nothing.
pub fn unvoid(book: &mut Book, env: &mut dyn Env, guid: Guid) -> DomainResult<()> {
    let mut t = get(book, guid)?.clone();
    if !t.is_void() {
        return Err(DomainError::conflict(format!("transaction {guid} is not void")));
    }
    let former = t.slots.get_str(slot_paths::VOID_FORMER_NOTES).map(str::to_owned);
    t.slots.set_opt_str(slot_paths::NOTES, former.as_deref());
    [slot_paths::VOID_FORMER_NOTES, slot_paths::VOID_REASON, slot_paths::VOID_TIME, slot_paths::READ_ONLY].iter().for_each(|p| {
        t.slots.remove(p);
    });
    t.splits.iter_mut().for_each(unvoid_split);
    commit::commit(book, env, &mut t)?;
    book.transactions.insert(guid, Arc::new(t));
    Ok(())
}

/// `xaccSplitCloneNoKvp` and `xaccSplitCopyKvp` for a reversing split: a new GUID, the same account, lot, memo,
/// action and reconcile date, every slot but `online_id`; then amount and value negated and the state reset to `n`
/// (`5.17:libgnucash/engine/Split.cpp:597-636`, `5.17:libgnucash/engine/Transaction.cpp:2588-2594`).
fn reversed_split(env: &mut dyn Env, s: &Split) -> Split {
    let mut r = s.clone();
    r.guid = env.new_guid();
    r.slots.remove(slot_paths::ONLINE_ID);
    r.amount = s.amount.neg();
    r.value = s.value.neg();
    r.reconcile_state = ReconcileState::New;
    r
}

/// Reverse a transaction as GnuCash's Reverse Transaction command does (`xaccTransReverse`,
/// `5.17:libgnucash/engine/Transaction.cpp:2575-2610`, and its caller `5.17:gnucash/gnome/gnc-plugin-page-register.cpp:2560-2585`):
/// a clone with a new GUID and every slot, negated splits, the read-only mark cleared, posted on `date` and entered
/// now; the original gets `reversed-by`. Refused when a reversing transaction already exists, and, unlike GnuCash,
/// for a voided transaction, whose all-zero clone would itself read as void. As in GnuCash, the original is committed
/// first, so in a Book with trading accounts its trading splits are rebuilt if it is not balanced, and cloned.
pub fn reverse(book: &mut Book, env: &mut dyn Env, guid: Guid, date: Date) -> DomainResult<Guid> {
    let orig = get(book, guid)?;
    if let Some(by) = orig.reversed_by().filter(|g| book.transaction(*g).is_some()) {
        return Err(DomainError::conflict(format!("a reversing entry has already been created for this transaction: {by}")));
    }
    if orig.is_void() {
        return Err(DomainError::conflict(format!("transaction {guid} is void; unvoid it before reversing")));
    }
    let mut orig = orig.clone();
    if trading::commit(book, env, &mut orig)?.is_some() {
        commit::sort_splits(&mut orig);
    }
    let mut t = orig.clone();
    t.guid = env.new_guid();
    t.splits = orig.splits.iter().map(|s| reversed_split(env, s)).collect();
    t.slots.remove(slot_paths::READ_ONLY);
    commit::set_date_posted(&mut t, date, env.zone());
    t.date_entered = env.now();
    commit::commit(book, env, &mut t)?;
    let new_guid = t.guid;
    orig.slots.set(slot_paths::REVERSED_BY, SlotValue::Guid(new_guid));
    book.transactions.insert(guid, Arc::new(orig));
    book.transactions.insert(new_guid, Arc::new(t));
    Ok(new_guid)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{fixture, simple, usd};
    use super::*;

    #[test]
    fn void_and_unvoid_round_trip() {
        let mut f = fixture();
        let g = simple(&mut f, 700);
        let before = f.book.transaction(g).unwrap().clone();
        void(&mut f.book, &mut f.env, g, "wrong payee").unwrap();
        let t = f.book.transaction(g).unwrap();
        assert!(t.is_void());
        assert_eq!(t.notes(), Some(VOIDED_NOTES));
        assert_eq!(t.slots.get_str(slot_paths::VOID_REASON), Some("wrong payee"));
        assert_eq!(t.slots.get_str(slot_paths::VOID_TIME), Some("2024-07-01 12:00:00"));
        assert_eq!(t.read_only_reason(), Some(VOIDED_READ_ONLY));
        assert!(t.slots.get(slot_paths::VOID_FORMER_NOTES).is_none());
        let s = &t.splits[0];
        assert!(s.value.same_repr(usd(0)) && s.amount.same_repr(usd(0)));
        assert_eq!(s.slots.get(slot_paths::VOID_FORMER_VALUE), Some(&SlotValue::Numeric(usd(700))));
        assert!(matches!(void(&mut f.book, &mut f.env, g, "again"), Err(DomainError::Conflict(_))));
        unvoid(&mut f.book, &mut f.env, g).unwrap();
        assert_eq!(f.book.transaction(g).unwrap(), &before);
        assert!(matches!(unvoid(&mut f.book, &mut f.env, g), Err(DomainError::Conflict(_))));
    }

    #[test]
    fn void_with_an_empty_reason_is_still_void() {
        let mut f = fixture();
        let g = simple(&mut f, 700);
        void(&mut f.book, &mut f.env, g, "").unwrap();
        let t = f.book.transaction(g).unwrap();
        assert!(t.is_void() && t.slots.get(slot_paths::VOID_REASON).is_none());
    }

    #[test]
    fn reconciled_transactions_cannot_be_voided() {
        let mut f = fixture();
        let g = simple(&mut f, 700);
        Arc::make_mut(f.book.transactions.get_mut(&g).unwrap()).splits[0].reconcile_state = ReconcileState::Cleared;
        assert!(matches!(void(&mut f.book, &mut f.env, g, "x"), Err(DomainError::Conflict(_))));
    }

    #[test]
    fn reverse_negates_and_links() {
        let mut f = fixture();
        let g = simple(&mut f, 700);
        Arc::make_mut(f.book.transactions.get_mut(&g).unwrap()).splits[0].reconcile_state = ReconcileState::Reconciled;
        f.env.now = Time64(1_800_000_000);
        let r = reverse(&mut f.book, &mut f.env, g, Date::from_ymd(2024, 7, 2).unwrap()).unwrap();
        let t = f.book.transaction(r).unwrap();
        assert_eq!(f.book.transaction(g).unwrap().reversed_by(), Some(r));
        assert_eq!(t.date_entered, Time64(1_800_000_000));
        assert_eq!(t.date_posted.to_file_string(), "2024-07-02 10:59:00 +0000");
        assert!(t.splits.iter().all(|s| s.reconcile_state == ReconcileState::New));
        assert!(t.splits[0].value.same_repr(usd(700)) && t.splits[0].memo.is_none(), "the bank split is now the debit");
        assert_eq!(t.splits[1].memo.as_deref(), Some("lunch"));
        assert!(matches!(reverse(&mut f.book, &mut f.env, g, Date::from_ymd(2024, 7, 2).unwrap()), Err(DomainError::Conflict(_))));
    }
}
