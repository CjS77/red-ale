//! Reconciliation: a split's reconcile state, an account's reconcile information, and reconciling an account against a
//! statement.
//!
//! GnuCash keeps the reconcile window's ticks in memory until Finish or Postpone (`5.17:gnucash/gnome/window-reconcile.cpp`).
//! These operations are the window's steps as functions: [`prepare`] fills in the defaults, [`status`] lists the
//! splits and balances for a set of ticks, and [`finish`] and [`postpone`] change the book. The caller carries the
//! ticks between calls. See `design/api/reconcile.md` and `design/notes/engine-accounts-transactions.md` §5.7 and §8.

use std::sync::Arc;

use crate::account::slot_paths;
use crate::{Account, Book, Date, DomainError, DomainResult, Env, Guid, Numeric, ReconcileState, SlotValue, Split, Time64, Transaction};

/// `reconcile-info/include-children`: whether the last reconciliation included sub-accounts.
pub const INCLUDE_CHILDREN: &str = "reconcile-info/include-children";
/// `reconcile-info/postpone`: the frame holding a postponed reconciliation.
pub const POSTPONE: &str = "reconcile-info/postpone";

const SECONDS_PER_DAY: i64 = 86_400;

/// A fixed offset from UTC, in seconds, standing in for the local time zone of GnuCash's local-time rules (the
/// statement date is "the end of the chosen day, local time").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UtcOffset(pub i64);

impl UtcOffset {
    /// The local calendar date of an instant.
    pub fn local_date(self, t: Time64) -> Option<Date> {
        t.plus_seconds(self.0).utc_date()
    }

    /// The last second of a local day (`gnc_time64_get_day_end_gdate`).
    pub fn day_end(self, d: Date) -> Time64 {
        d.end_utc().plus_seconds(-self.0)
    }
}

/// The interval between reconciliations GnuCash remembers to propose the next statement date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    /// Whole months.
    pub months: i64,
    /// Days, used when `months` is zero.
    pub days: i64,
}

/// A postponed reconciliation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Postponed {
    /// The statement date.
    pub date: Time64,
    /// The statement's ending balance, raw sign (debits positive).
    pub balance: Numeric,
}

/// An account's reconcile information, from its `reconcile-info` slots (`5.17:libgnucash/engine/Account.cpp:4554-4668`).
#[derive(Debug, Clone, PartialEq)]
pub struct ReconcileInfo {
    /// When the account was last reconciled.
    pub last_date: Option<Time64>,
    /// The interval between the last two reconciliations; GnuCash reads it only when both members are present.
    pub last_interval: Option<Interval>,
    /// A postponed reconciliation's statement date.
    pub postpone_date: Option<Time64>,
    /// A postponed reconciliation's ending balance.
    pub postpone_balance: Option<Numeric>,
    /// Whether reconciliations include sub-accounts.
    pub include_children: bool,
}

fn slot_int(account: &Account, path: &str) -> Option<i64> {
    account.slots.get(path).and_then(SlotValue::as_int)
}

fn interval_path(member: &str) -> String {
    format!("{}/{member}", slot_paths::RECONCILE_LAST_INTERVAL)
}

/// `get_kvp_boolean_path`: a non-zero integer or the string `"true"` (`5.17:libgnucash/engine/Account.cpp:2564-2577`).
fn slot_bool(account: &Account, path: &str) -> bool {
    match account.slots.get(path) {
        Some(SlotValue::Integer(i)) => *i != 0,
        Some(SlotValue::String(s)) => s == "true",
        _ => false,
    }
}

/// An account's reconcile information.
pub fn reconcile_info(account: &Account) -> ReconcileInfo {
    let months = slot_int(account, &interval_path("months"));
    let days = slot_int(account, &interval_path("days"));
    ReconcileInfo {
        last_date: slot_int(account, slot_paths::RECONCILE_LAST_DATE).map(Time64),
        last_interval: months.zip(days).map(|(months, days)| Interval { months, days }),
        postpone_date: slot_int(account, slot_paths::RECONCILE_POSTPONE_DATE).map(Time64),
        postpone_balance: account.slots.get(slot_paths::RECONCILE_POSTPONE_BALANCE).and_then(SlotValue::as_numeric),
        include_children: slot_bool(account, INCLUDE_CHILDREN),
    }
}

/// Set or remove the last reconcile date (`xaccAccountSetReconcileLastDate`, an int64 slot).
pub fn set_last_date(account: &mut Account, date: Option<Time64>) {
    match date {
        Some(d) => account.slots.set(slot_paths::RECONCILE_LAST_DATE, SlotValue::Integer(d.0)),
        None => drop(account.slots.remove(slot_paths::RECONCILE_LAST_DATE)),
    }
}

/// Set or remove the last interval (`xaccAccountSetReconcileLastInterval`, two int64 slots). Negative members are
/// refused, as GnuCash never stores them (`5.17:gnucash/gnome/window-reconcile.cpp:736-740`).
pub fn set_last_interval(account: &mut Account, interval: Option<Interval>) -> DomainResult<()> {
    match interval {
        Some(i) if i.months < 0 || i.days < 0 => Err(DomainError::invalid("a reconcile interval cannot be negative")),
        Some(i) => {
            account.slots.set(&interval_path("months"), SlotValue::Integer(i.months));
            account.slots.set(&interval_path("days"), SlotValue::Integer(i.days));
            Ok(())
        }
        None => {
            account.slots.remove(slot_paths::RECONCILE_LAST_INTERVAL);
            Ok(())
        }
    }
}

/// Record a postponed reconciliation, or clear it. Clearing deletes the whole `postpone` frame, as
/// `xaccAccountClearReconcilePostpone` does (`5.17:libgnucash/engine/Account.cpp:4662-4665`).
pub fn set_postponed(account: &mut Account, postponed: Option<Postponed>) {
    match postponed {
        Some(p) => {
            account.slots.set(slot_paths::RECONCILE_POSTPONE_DATE, SlotValue::Integer(p.date.0));
            account.slots.set(slot_paths::RECONCILE_POSTPONE_BALANCE, SlotValue::Numeric(p.balance));
        }
        None => drop(account.slots.remove(POSTPONE)),
    }
}

/// Record whether reconciliations include sub-accounts, as an int64 (`xaccAccountSetReconcileChildrenStatus`,
/// `5.17:libgnucash/engine/Account.cpp:4870-4878`).
pub fn set_include_children(account: &mut Account, include: bool) {
    account.slots.set(INCLUDE_CHILDREN, SlotValue::Integer(i64::from(include)));
}

/// The account, mutably.
pub fn account_mut(book: &mut Book, guid: Guid) -> DomainResult<&mut Account> {
    book.accounts.get_mut(&guid).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("account", &guid))
}

fn split_in_txn_mut(book: &mut Book, txn: Guid, split: Guid) -> DomainResult<&mut Split> {
    let t = book.transactions.get_mut(&txn).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("transaction", &txn))?;
    t.split_mut(split).ok_or_else(|| DomainError::not_found("split", &split))
}

/// Set a split's reconcile state.
///
/// GnuCash's `xaccSplitSetReconcile` accepts any of the five states and leaves the reconcile date alone
/// (`5.17:libgnucash/engine/Split.cpp:1818-1842`). red-ale adds what GnuCash's dialogs enforce: a voided transaction's
/// splits never change (the reconcile window lists only `n` and `c` splits, `5.17:gnucash/gnome/reconcile-view.c:381`,
/// and the register makes voided transactions read-only), and `v` comes only from voiding. Setting `y` stamps the
/// reconcile date (`date`, else now), so a reconciled split always says when; other states keep the stored date.
pub fn set_split_state(book: &mut Book, env: &mut dyn Env, split: Guid, state: ReconcileState, date: Option<Time64>) -> DomainResult<()> {
    let t = book.transaction_of_split(split).ok_or_else(|| DomainError::not_found("split", &split))?;
    if t.is_void() {
        return Err(DomainError::conflict(format!("split {split} belongs to a voided transaction; unvoid it first")));
    }
    if state == ReconcileState::Voided {
        return Err(DomainError::invalid("the void state comes only from voiding the transaction"));
    }
    if date.is_some() && state != ReconcileState::Reconciled {
        return Err(DomainError::invalid("a reconcile date can be given only with the reconciled state"));
    }
    let current = t.split(split).map(|s| s.reconcile_state);
    if current == Some(ReconcileState::Voided) {
        return Err(DomainError::conflict(format!("split {split} is void")));
    }
    if current == Some(state) && date.is_none() {
        return Ok(());
    }
    let stamp = if state == ReconcileState::Reconciled { Some(date.unwrap_or_else(|| env.now())) } else { None };
    let s = split_in_txn_mut(book, t.guid, split)?;
    s.reconcile_state = state;
    if stamp.is_some() {
        s.reconcile_date = stamp;
    }
    Ok(())
}

/// What the caller asks for when starting a reconciliation; anything absent gets GnuCash's default.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The account to reconcile.
    pub account: Guid,
    /// The statement date (a local day).
    pub date: Option<Date>,
    /// The statement's ending balance, raw sign.
    pub ending_balance: Option<Numeric>,
    /// Include sub-accounts.
    pub include_children: Option<bool>,
    /// The caller's time zone.
    pub zone: UtcOffset,
}

/// A statement being reconciled: everything Finish and Postpone need besides the ticks.
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    /// The account reconciled.
    pub account: Guid,
    /// Whether its sub-accounts are reconciled with it.
    pub include_children: bool,
    /// The statement date: the last second of the statement day.
    pub date: Time64,
    /// The ending balance, raw sign.
    pub ending_balance: Numeric,
    /// The caller's time zone, for the interval GnuCash remembers.
    pub zone: UtcOffset,
}

/// A statement with the defaults filled in, and where they came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    /// The statement.
    pub statement: Statement,
    /// The date and balance came from a postponed reconciliation.
    pub resumed_postponed: bool,
}

/// True when some descendant holds a different commodity, which rules out including sub-accounts
/// (`has_account_different_commodities`, `5.17:gnucash/gnome/window-reconcile.cpp:186-202`).
pub fn has_different_commodities(book: &Book, account: &Account) -> bool {
    book.descendants(account.guid).iter().any(|d| d.commodity != account.commodity)
}

/// The default statement date (`gnc_get_reconcile_info`, `5.17:gnucash/gnome/window-reconcile.cpp:1635-1675`): the
/// last reconcile date plus the last interval (one month by default, tracking month ends), no later than today; a
/// postponed date wins. GnuCash's preference to always reconcile to today is off by default and not modelled.
pub fn default_statement_date(info: &ReconcileInfo, now: Time64, zone: UtcOffset) -> Time64 {
    let today_end = zone.local_date(now).map_or(now, |d| zone.day_end(d));
    let from_last = info.last_date.and_then(|last| zone.local_date(last)).map(|last| {
        let interval = info.last_interval.unwrap_or(Interval { months: 1, days: 0 });
        let next = if interval.months == 0 {
            last.add_days(interval.days)
        } else {
            let moved = last.add_months(i32::try_from(interval.months).unwrap_or(1));
            if last == last.end_of_month() { moved.end_of_month() } else { moved }
        };
        zone.day_end(next).min(today_end)
    });
    info.postpone_date.or(from_last).unwrap_or(today_end)
}

/// The accounts a statement covers: the account, then its descendants if included.
pub fn covered_accounts(book: &Book, account: Guid, include_children: bool) -> Vec<Guid> {
    let children = if include_children { book.descendants(account).iter().map(|a| a.guid).collect() } else { Vec::new() };
    std::iter::once(account).chain(children).collect()
}

fn covered_splits<'a>(book: &'a Book, accounts: &'a [Guid]) -> impl Iterator<Item = (&'a Transaction, &'a Split)> {
    book.transactions
        .values()
        .flat_map(move |t| t.splits.iter().filter(move |s| s.account.is_some_and(|a| accounts.contains(&a))).map(move |s| (t.as_ref(), s)))
}

fn sum(mut amounts: impl Iterator<Item = Numeric>) -> DomainResult<Numeric> {
    amounts.try_fold(Numeric::ZERO, Numeric::add_exact).map_err(DomainError::from)
}

/// The balance of the accounts from splits posted strictly before `date` (`xaccAccountGetBalanceAsOfDate`,
/// `5.17:libgnucash/engine/Account.cpp:3505-3525`). Stock-split adjustments do not apply to the currency accounts
/// reconciled against statements.
pub fn balance_as_of(book: &Book, accounts: &[Guid], date: Time64) -> DomainResult<Numeric> {
    sum(covered_splits(book, accounts).filter(|(t, _)| t.date_posted < date).map(|(_, s)| s.amount))
}

/// Fill in a statement's defaults as GnuCash's start dialog does (`startRecnWindow` and `recn_date_changed_cb`,
/// `5.17:gnucash/gnome/window-reconcile.cpp:449-512`, `:757-947`): the ending balance is the postponed balance when the
/// date was not changed, else the balance as of the statement date.
pub fn prepare(book: &Book, now: Time64, request: &Request) -> DomainResult<Prepared> {
    let account = book.account(request.account).ok_or_else(|| DomainError::not_found("account", &request.account))?;
    if account.guid == book.root {
        return Err(DomainError::invalid("the root account cannot be reconciled"));
    }
    let info = reconcile_info(account);
    let mixed = has_different_commodities(book, account);
    if request.include_children == Some(true) && mixed {
        return Err(DomainError::invalid("sub-accounts hold other commodities, so they cannot be reconciled with this account"));
    }
    let include_children = request.include_children.unwrap_or(info.include_children && !mixed);
    let date = request.date.map_or_else(|| default_statement_date(&info, now, request.zone), |d| request.zone.day_end(d));
    let use_postponed = request.date.is_none() && request.include_children.is_none_or(|i| i == (info.include_children && !mixed));
    let postponed = info.postpone_balance.filter(|_| use_postponed);
    let ending_balance = match request.ending_balance.or(postponed) {
        Some(b) => b,
        None => balance_as_of(book, &covered_accounts(book, account.guid, include_children), date)?,
    };
    let resumed_postponed = use_postponed && (info.postpone_balance.is_some() || info.postpone_date.is_some());
    Ok(Prepared { statement: Statement { account: account.guid, include_children, date, ending_balance, zone: request.zone }, resumed_postponed })
}

/// A split the reconciliation can tick: one of the covered accounts' splits that is not yet reconciled.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Its transaction.
    pub transaction: Guid,
    /// The split.
    pub split: Guid,
    /// Its account.
    pub account: Guid,
    /// When its transaction was posted.
    pub date_posted: Time64,
    /// Its amount, raw sign.
    pub amount: Numeric,
    /// `n` or `c`.
    pub state: ReconcileState,
    /// Ticked when the window opens: cleared and posted by the statement date.
    pub pre_ticked: bool,
}

/// The splits the reconcile window lists: every `n` or `c` split of the covered accounts, whatever its date
/// (`5.17:gnucash/gnome/reconcile-view.c:358-399`), ordered by posted date. Cleared splits posted by the statement date
/// start ticked (the default `check-cleared` preference).
pub fn candidates(book: &Book, statement: &Statement) -> Vec<Candidate> {
    let accounts = covered_accounts(book, statement.account, statement.include_children);
    let mut out: Vec<Candidate> = covered_splits(book, &accounts)
        .filter(|(_, s)| matches!(s.reconcile_state, ReconcileState::New | ReconcileState::Cleared))
        .map(|(t, s)| Candidate {
            transaction: t.guid,
            split: s.guid,
            account: s.account.unwrap_or(statement.account),
            date_posted: t.date_posted,
            amount: s.amount,
            state: s.reconcile_state,
            pre_ticked: s.reconcile_state == ReconcileState::Cleared && t.date_posted <= statement.date,
        })
        .collect();
    out.sort_by_key(|c| c.date_posted);
    out
}

/// The ticks the window starts with.
pub fn default_ticks(candidates: &[Candidate]) -> Vec<Guid> {
    candidates.iter().filter(|c| c.pre_ticked).map(|c| c.split).collect()
}

/// The reconcile window's figures (`recnRecalculateBalance`, `5.17:gnucash/gnome/window-reconcile.cpp:327-391`), all
/// raw sign.
#[derive(Debug, Clone, PartialEq)]
pub struct Balances {
    /// The reconciled balance before this statement: every `y` and `f` split, whatever its date.
    pub starting: Numeric,
    /// The cleared balance: every split that is not `n`.
    pub cleared: Numeric,
    /// The sum of the ticked splits.
    pub ticked: Numeric,
    /// `starting + ticked`.
    pub reconciled: Numeric,
    /// `ending − reconciled`; Finish needs it to be zero.
    pub difference: Numeric,
}

/// The ticked candidates, in candidate order. A tick that is not a candidate is refused.
pub fn ticked_candidates<'a>(candidates: &'a [Candidate], ticked: &[Guid]) -> DomainResult<Vec<&'a Candidate>> {
    if let Some(bad) = ticked.iter().find(|g| !candidates.iter().any(|c| c.split == **g)) {
        return Err(DomainError::invalid(format!("split {bad} cannot be ticked: it is not an unreconciled split of the accounts reconciled")));
    }
    Ok(candidates.iter().filter(|c| ticked.contains(&c.split)).collect())
}

/// The window's figures for a set of ticks.
pub fn balances(book: &Book, statement: &Statement, candidates: &[Candidate], ticked: &[Guid]) -> DomainResult<Balances> {
    let accounts = covered_accounts(book, statement.account, statement.include_children);
    let reconciled_state = |s: &Split| matches!(s.reconcile_state, ReconcileState::Reconciled | ReconcileState::Frozen);
    let starting = sum(covered_splits(book, &accounts).filter(|(_, s)| reconciled_state(s)).map(|(_, s)| s.amount))?;
    let cleared = sum(covered_splits(book, &accounts).filter(|(_, s)| s.reconcile_state != ReconcileState::New).map(|(_, s)| s.amount))?;
    let ticked = sum(ticked_candidates(candidates, ticked)?.into_iter().map(|c| c.amount))?;
    let reconciled = starting.add_exact(ticked)?;
    let difference = statement.ending_balance.sub_exact(reconciled)?;
    Ok(Balances { starting, cleared, ticked, reconciled, difference })
}

/// The interval GnuCash remembers when a reconciliation starts (`gnc_save_reconcile_interval`,
/// `5.17:gnucash/gnome/window-reconcile.cpp:693-740`): whole days, except that 28 days stays one month if it was, and
/// more than 28 days counts calendar months. `None` when there is no earlier reconciliation or the result is negative.
pub fn next_interval(info: &ReconcileInfo, statement_date: Time64, zone: UtcOffset) -> Option<Interval> {
    let last = info.last_date?;
    let days = (statement_date.0 - last.0) / SECONDS_PER_DAY;
    let interval = match days {
        28 if info.last_interval.is_none_or(|i| i.months == 1) => Interval { months: 1, days: 0 },
        d if d > 28 => {
            let (now, then) = (zone.local_date(statement_date)?, zone.local_date(last)?);
            let months = i64::from(now.year() - then.year()) * 12 + i64::from(now.month()) - i64::from(then.month());
            Interval { months, days: 0 }
        }
        d => Interval { months: 0, days: d },
    };
    (interval.months >= 0 && interval.days >= 0).then_some(interval)
}

/// What starting a reconciliation records on the account: whether sub-accounts are included and the interval
/// (`5.17:gnucash/gnome/window-reconcile.cpp:929-939`). red-ale records them at Finish or Postpone, its only writes.
fn record_start(book: &mut Book, statement: &Statement) -> DomainResult<Option<Interval>> {
    let account = account_mut(book, statement.account)?;
    let interval = next_interval(&reconcile_info(account), statement.date, statement.zone);
    set_include_children(account, statement.include_children);
    if interval.is_some() {
        set_last_interval(account, interval)?;
    }
    Ok(interval)
}

/// A split whose reconcile state changed.
#[derive(Debug, Clone, PartialEq)]
pub struct StateChange {
    /// Its transaction.
    pub transaction: Guid,
    /// The split.
    pub split: Guid,
    /// The state before.
    pub from: ReconcileState,
    /// The state after.
    pub to: ReconcileState,
}

/// What Finish did.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    /// The splits now reconciled.
    pub reconciled: Vec<StateChange>,
    /// The interval recorded, if any.
    pub interval: Option<Interval>,
    /// The figures at Finish.
    pub balances: Balances,
}

/// Finish a reconciliation (`recnFinishCB`, `5.17:gnucash/gnome/window-reconcile.cpp:2419-2475`): every ticked split
/// becomes `y` with the statement date as its reconcile date (`5.17:gnucash/gnome/reconcile-view.c:931-937`), the
/// postponed reconciliation is cleared and the statement date becomes the last reconcile date. Refused while the
/// difference is not zero unless `force`, GnuCash's "finish anyway" confirmation.
pub fn finish(book: &mut Book, statement: &Statement, ticked: &[Guid], force: bool) -> DomainResult<Finished> {
    let candidates = candidates(book, statement);
    let balances = balances(book, statement, &candidates, ticked)?;
    if !balances.difference.is_zero() && !force {
        return Err(DomainError::invalid(format!(
            "the account does not balance: the difference is {}; tick more splits, correct the ending balance, or pass force",
            balances.difference.to_decimal_string()
        )));
    }
    let interval = record_start(book, statement)?;
    let changes = ticked_candidates(&candidates, ticked)?
        .into_iter()
        .map(|c| StateChange { transaction: c.transaction, split: c.split, from: c.state, to: ReconcileState::Reconciled })
        .collect::<Vec<_>>();
    changes.iter().try_for_each(|c| -> DomainResult<()> {
        let s = split_in_txn_mut(book, c.transaction, c.split)?;
        s.reconcile_state = ReconcileState::Reconciled;
        s.reconcile_date = Some(statement.date);
        Ok(())
    })?;
    let account = account_mut(book, statement.account)?;
    set_postponed(account, None);
    set_last_date(account, Some(statement.date));
    Ok(Finished { reconciled: changes, interval, balances })
}

/// What Postpone did.
#[derive(Debug, Clone, PartialEq)]
pub struct PostponedResult {
    /// Splits whose state changed.
    pub changed: Vec<StateChange>,
    /// The interval recorded, if any.
    pub interval: Option<Interval>,
}

/// Postpone a reconciliation (`recnPostponeCB`, `5.17:gnucash/gnome/window-reconcile.cpp:2482-2510`): every listed split
/// posted by the statement date, or ticked, becomes `c` if ticked and `n` if not (`gnc_reconcile_view_postpone`,
/// `5.17:gnucash/gnome/reconcile-view.c:961-998`), and the statement date and ending balance are kept for next time.
pub fn postpone(book: &mut Book, statement: &Statement, ticked: &[Guid]) -> DomainResult<PostponedResult> {
    let candidates = candidates(book, statement);
    ticked_candidates(&candidates, ticked)?;
    let interval = record_start(book, statement)?;
    let changed: Vec<StateChange> = candidates
        .iter()
        .filter(|c| c.date_posted <= statement.date || ticked.contains(&c.split))
        .map(|c| {
            let to = if ticked.contains(&c.split) { ReconcileState::Cleared } else { ReconcileState::New };
            StateChange { transaction: c.transaction, split: c.split, from: c.state, to }
        })
        .filter(|c| c.from != c.to)
        .collect();
    changed.iter().try_for_each(|c| split_in_txn_mut(book, c.transaction, c.split).map(|s| s.reconcile_state = c.to))?;
    set_postponed(account_mut(book, statement.account)?, Some(Postponed { date: statement.date, balance: statement.ending_balance }));
    Ok(PostponedResult { changed, interval })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::ops::commit::{NewSplit, NewTransaction, add_transaction};
    use crate::{AccountType, CommodityRef, FixedEnv, transaction::slot_paths as txn_paths};

    const NOW: Time64 = Time64(1_719_835_200); // 2024-07-01 12:00 UTC

    struct Fixture {
        book: Book,
        env: FixedEnv,
        bank: Guid,
        food: Guid,
    }

    fn fixture() -> Fixture {
        let mut env = FixedEnv::new(NOW, 0);
        let mut book = Book::new_empty(&mut env);
        let usd = CommodityRef::currency("USD");
        let mut add = |name: &str, ty| {
            let a = Account::new(env.new_guid(), name, ty, Some(usd.clone()), Some(book.root));
            let g = a.guid;
            book.accounts.insert(g, Arc::new(a));
            g
        };
        let bank = add("Bank", AccountType::Bank);
        let food = add("Food", AccountType::Expense);
        Fixture { book, env, bank, food }
    }

    fn day(y: i32, m: u32, d: u32) -> Date {
        Date::from_ymd(y, m, d).unwrap()
    }

    /// A payment from the bank on `date`; returns the bank split.
    fn pay(f: &mut Fixture, date: Date, cents: i64) -> Guid {
        let mut new = NewTransaction::new(CommodityRef::currency("USD"), date);
        new.splits = vec![NewSplit::new(f.bank, Numeric::new(-cents, 100)), NewSplit::new(f.food, Numeric::new(cents, 100))];
        let t = add_transaction(&mut f.book, &mut f.env, new).unwrap();
        f.book.transaction(t).unwrap().splits.iter().find(|s| s.account == Some(f.bank)).unwrap().guid
    }

    fn state(book: &Book, split: Guid) -> (ReconcileState, Option<Time64>) {
        let s = book.transaction_of_split(split).unwrap().split(split).unwrap();
        (s.reconcile_state, s.reconcile_date)
    }

    fn statement(f: &Fixture, date: Date, ending_cents: i64) -> Statement {
        let request = Request {
            account: f.bank,
            date: Some(date),
            ending_balance: Some(Numeric::new(ending_cents, 100)),
            include_children: None,
            zone: UtcOffset::default(),
        };
        prepare(&f.book, NOW, &request).unwrap().statement
    }

    #[test]
    fn reconciling_stamps_the_date_and_void_is_refused() {
        let mut f = fixture();
        let split = pay(&mut f, day(2024, 6, 3), 500);
        set_split_state(&mut f.book, &mut f.env, split, ReconcileState::Cleared, None).unwrap();
        assert_eq!(state(&f.book, split), (ReconcileState::Cleared, None));
        set_split_state(&mut f.book, &mut f.env, split, ReconcileState::Reconciled, None).unwrap();
        assert_eq!(state(&f.book, split), (ReconcileState::Reconciled, Some(NOW)));
        set_split_state(&mut f.book, &mut f.env, split, ReconcileState::New, None).unwrap();
        assert_eq!(state(&f.book, split), (ReconcileState::New, Some(NOW)), "the date is kept, as xaccSplitSetReconcile does");
        assert!(set_split_state(&mut f.book, &mut f.env, split, ReconcileState::Voided, None).is_err());
        assert!(set_split_state(&mut f.book, &mut f.env, split, ReconcileState::Cleared, Some(NOW)).is_err());
        let t = f.book.transaction_of_split(split).unwrap().guid;
        Arc::make_mut(f.book.transactions.get_mut(&t).unwrap()).slots.set(txn_paths::VOID_REASON, SlotValue::String("x".into()));
        let err = set_split_state(&mut f.book, &mut f.env, split, ReconcileState::Cleared, None).unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)));
    }

    #[test]
    fn default_statement_date_follows_the_interval() {
        let zone = UtcOffset::default();
        let mut info = reconcile_info(&fixture().book.accounts[1]);
        assert_eq!(default_statement_date(&info, NOW, zone), day(2024, 7, 1).end_utc(), "no history: today");
        info.last_date = Some(day(2024, 1, 31).end_utc());
        assert_eq!(default_statement_date(&info, NOW, zone), day(2024, 2, 29).end_utc(), "month ends track month ends");
        info.last_date = Some(day(2024, 6, 15).end_utc());
        assert_eq!(default_statement_date(&info, NOW, zone), day(2024, 7, 1).end_utc(), "clamped to today");
        info.last_interval = Some(Interval { months: 0, days: 7 });
        assert_eq!(default_statement_date(&info, NOW, zone), day(2024, 6, 22).end_utc());
        info.postpone_date = Some(Time64(5));
        assert_eq!(default_statement_date(&info, NOW, zone), Time64(5), "a postponed date wins");
        let east = UtcOffset(2 * 3600);
        assert_eq!(east.day_end(day(2024, 6, 30)), Time64(day(2024, 6, 30).end_utc().0 - 7200));
    }

    #[test]
    fn interval_rules() {
        let zone = UtcOffset::default();
        let mut info = reconcile_info(&fixture().book.accounts[1]);
        assert_eq!(next_interval(&info, NOW, zone), None);
        info.last_date = Some(day(2024, 2, 1).end_utc());
        assert_eq!(next_interval(&info, day(2024, 2, 29).end_utc(), zone), Some(Interval { months: 1, days: 0 }));
        info.last_interval = Some(Interval { months: 0, days: 14 });
        assert_eq!(next_interval(&info, day(2024, 2, 29).end_utc(), zone), Some(Interval { months: 0, days: 28 }));
        assert_eq!(next_interval(&info, day(2024, 4, 30).end_utc(), zone), Some(Interval { months: 2, days: 0 }));
        assert_eq!(next_interval(&info, day(2024, 2, 8).end_utc(), zone), Some(Interval { months: 0, days: 7 }));
        assert_eq!(next_interval(&info, day(2024, 1, 8).end_utc(), zone), None, "negative intervals are not stored");
    }

    #[test]
    fn finish_and_postpone() {
        let mut f = fixture();
        let a = pay(&mut f, day(2024, 6, 3), 500);
        let b = pay(&mut f, day(2024, 6, 20), 250);
        let late = pay(&mut f, day(2024, 7, 20), 100);
        set_split_state(&mut f.book, &mut f.env, b, ReconcileState::Cleared, None).unwrap();
        let st = statement(&f, day(2024, 6, 30), -750);
        let cands = candidates(&f.book, &st);
        assert_eq!(cands.iter().map(|c| c.split).collect::<Vec<_>>(), [a, b, late], "no date filter");
        assert_eq!(default_ticks(&cands), [b]);
        let bal = balances(&f.book, &st, &cands, &[b]).unwrap();
        assert_eq!(bal.difference, Numeric::new(-500, 100));
        assert!(finish(&mut f.book.clone(), &st, &[b], false).is_err());

        let p = postpone(&mut f.book, &st, &[a]).unwrap();
        let moves: Vec<_> = p.changed.iter().map(|c| (c.split, c.to)).collect();
        assert_eq!(moves, [(a, ReconcileState::Cleared), (b, ReconcileState::New)], "late splits are left alone");
        let info = reconcile_info(f.book.account(f.bank).unwrap());
        assert_eq!((info.postpone_date, info.postpone_balance), (Some(st.date), Some(Numeric::new(-750, 100))));
        assert_eq!(f.book.account(f.bank).unwrap().slots.get(INCLUDE_CHILDREN), Some(&SlotValue::Integer(0)));

        let resumed = prepare(&f.book, NOW, &Request { account: f.bank, date: None, ending_balance: None, include_children: None, zone: UtcOffset(0) })
            .unwrap();
        assert!(resumed.resumed_postponed);
        assert_eq!((resumed.statement.date, resumed.statement.ending_balance), (st.date, Numeric::new(-750, 100)));

        let done = finish(&mut f.book, &st, &[a, b], false).unwrap();
        assert_eq!(done.reconciled.len(), 2);
        assert_eq!(state(&f.book, a), (ReconcileState::Reconciled, Some(st.date)));
        assert_eq!(state(&f.book, late).0, ReconcileState::New);
        let info = reconcile_info(f.book.account(f.bank).unwrap());
        assert_eq!((info.last_date, info.postpone_date, info.postpone_balance), (Some(st.date), None, None));
        assert!(f.book.account(f.bank).unwrap().slots.get(POSTPONE).is_none());
        assert!(finish(&mut f.book, &st, &[a], false).is_err(), "a reconciled split is no longer a candidate");
    }

    #[test]
    fn default_ending_balance_is_the_balance_before_the_statement_end() {
        let mut f = fixture();
        pay(&mut f, day(2024, 6, 3), 500);
        pay(&mut f, day(2024, 7, 2), 100);
        let p = prepare(&f.book, NOW, &Request { account: f.bank, date: Some(day(2024, 6, 30)), ending_balance: None, include_children: None, zone: UtcOffset(0) })
            .unwrap();
        assert_eq!(p.statement.ending_balance, Numeric::new(-500, 100));
        assert!(!p.resumed_postponed);
    }

    proptest! {
        /// Finishing with the reconciled balance as the ending balance always succeeds, and moves exactly the ticked
        /// amounts into the reconciled balance.
        #[test]
        fn finish_balances_exactly(amounts in proptest::collection::vec(1i64..1_000_000, 1..8), mask in proptest::collection::vec(any::<bool>(), 8)) {
            let mut f = fixture();
            let splits: Vec<Guid> = amounts.iter().map(|c| pay(&mut f, day(2024, 6, 3), *c)).collect();
            let ticked: Vec<Guid> = splits.iter().zip(&mask).filter(|(_, m)| **m).map(|(s, _)| *s).collect();
            let expected: i64 = amounts.iter().zip(&mask).filter(|(_, m)| **m).map(|(a, _)| -a).sum();
            let st = statement(&f, day(2024, 6, 30), expected);
            let cands = candidates(&f.book, &st);
            prop_assert!(balances(&f.book, &st, &cands, &ticked).unwrap().difference.is_zero());
            finish(&mut f.book, &st, &ticked, false).unwrap();
            let after = statement(&f, day(2024, 6, 30), 0);
            let bal = balances(&f.book, &after, &candidates(&f.book, &after), &[]).unwrap();
            prop_assert_eq!(bal.starting, Numeric::new(expected, 100));
        }
    }
}
