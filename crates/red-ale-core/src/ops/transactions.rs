//! Domain operations: transactions and their splits.
//!
//! Creating, editing and deleting transactions, changing their currency, and listing them in GnuCash's order. Void,
//! unvoid and reverse live in [`void`], find in [`find`], and transfers between commodities with the price they
//! record in [`transfer`]. Every operation ends in [`commit::commit`] or [`commit::add_transaction`], which round
//! values and amounts as GnuCash's setters do, sort the splits and refuse an unbalanced result.
//! See `design/notes/engine-accounts-transactions.md` §5.

pub mod find;
pub mod order;
pub mod transfer;
pub mod void;

use std::sync::Arc;

use crate::ops::commit::{self, NewSplit, NewTransaction};
use crate::{Book, CommodityRef, Date, DomainError, DomainResult, Env, Guid, How, Numeric, ReconcileState, Round, Time64, Transaction};

/// The transaction with this GUID, or `NotFound`.
pub fn get(book: &Book, guid: Guid) -> DomainResult<&Transaction> {
    book.transaction(guid).ok_or_else(|| DomainError::not_found("transaction", &guid))
}

/// Refuse an account that may not take splits: a missing account, the root, or a placeholder. The register refuses
/// placeholders ("The account %s does not allow transactions.", `5.17:gnucash/register/ledger-core/split-register.c:2102-2110`),
/// and so does the transfer dialog (`5.17:gnucash/gnome-utils/dialog-transfer.cpp:1414-1428`).
pub fn check_postable(book: &Book, account: Guid) -> DomainResult<()> {
    let acc = book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
    if account == book.root {
        return Err(DomainError::invalid("cannot post to the root account"));
    }
    if acc.is_placeholder() {
        let name = book.full_name(account).unwrap_or_default();
        return Err(DomainError::invalid(format!("the account {name} does not allow transactions")));
    }
    Ok(())
}

/// Refuse a change to a read-only transaction. GnuCash will not destroy one (`5.17:libgnucash/engine/Transaction.cpp:1363-1374`)
/// and its register will not edit one; voided and invoice-posted transactions are read-only.
pub fn check_writable(t: &Transaction) -> DomainResult<()> {
    match t.read_only_reason() {
        Some(reason) => Err(DomainError::conflict(format!("transaction {} is read-only: {reason}", t.guid))),
        None => Ok(()),
    }
}

/// Text GnuCash never writes when empty (num, memo, action) is held as `None`, so the file reads back as written.
fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.is_empty())
}

fn check_new_split(book: &Book, s: &NewSplit) -> DomainResult<()> {
    check_postable(book, s.account)?;
    if s.reconcile_state == ReconcileState::Voided {
        return Err(DomainError::invalid("a split cannot be created voided; void the transaction instead"));
    }
    if let Some(lot) = s.lot {
        let l = book.lot(lot).ok_or_else(|| DomainError::not_found("lot", &lot))?;
        if l.account != s.account {
            return Err(DomainError::invalid(format!("lot {lot} belongs to another account")));
        }
    }
    Ok(())
}

fn normalise_split(mut s: NewSplit) -> NewSplit {
    s.memo = non_empty(s.memo);
    s.action = non_empty(s.action);
    s.reconcile_date = s.reconcile_date.filter(|t| *t != Time64(0));
    s
}

/// Post a new transaction. Its accounts must take splits and it must balance. A new GnuCash transaction has an
/// empty description rather than none (`xaccInitTransaction`, `5.17:libgnucash/engine/Transaction.cpp:266-267`).
pub fn post(book: &mut Book, env: &mut dyn Env, mut new: NewTransaction) -> DomainResult<Guid> {
    new.splits.iter().try_for_each(|s| check_new_split(book, s))?;
    new.splits = new.splits.into_iter().map(normalise_split).collect();
    new.num = non_empty(new.num);
    new.description = Some(new.description.unwrap_or_default());
    commit::add_transaction(book, env, new)
}

/// A change to one existing split. Fields left `None` are kept.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SplitEdit {
    /// The split to change.
    pub guid: Guid,
    /// A new account.
    pub account: Option<Guid>,
    /// A new value, in the transaction currency.
    pub value: Option<Numeric>,
    /// A new amount, in the account's commodity.
    pub amount: Option<Numeric>,
    /// A new memo; empty clears it.
    pub memo: Option<String>,
    /// A new action; empty clears it.
    pub action: Option<String>,
    /// A new reconcile state. `v` is refused: void the transaction instead.
    pub reconcile_state: Option<ReconcileState>,
    /// A new reconcile date.
    pub reconcile_date: Option<Time64>,
}

/// A change to a transaction. Fields left `None` (and empty lists) are kept.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransactionEdit {
    /// A new currency; the existing splits are re-valued as `xaccTransSetCurrency` does (see [`set_currency`]).
    pub currency: Option<CommodityRef>,
    /// A new posted date.
    pub date_posted: Option<Date>,
    /// A new entered date.
    pub date_entered: Option<Time64>,
    /// A new num; empty clears it.
    pub num: Option<String>,
    /// A new description.
    pub description: Option<String>,
    /// New notes; empty deletes the slot.
    pub notes: Option<String>,
    /// Splits to remove.
    pub remove_splits: Vec<Guid>,
    /// Splits to change.
    pub change_splits: Vec<SplitEdit>,
    /// Splits to add.
    pub add_splits: Vec<NewSplit>,
}

/// Edit a transaction. The currency changes first, then splits are removed, changed and added, then the commit rules
/// apply: the result must balance. A read-only transaction is refused.
pub fn update(book: &mut Book, env: &mut dyn Env, guid: Guid, edit: TransactionEdit) -> DomainResult<()> {
    let mut t = get(book, guid)?.clone();
    check_writable(&t)?;
    if let Some(currency) = &edit.currency {
        set_currency(book, &mut t, currency)?;
    }
    remove_splits(&mut t, &edit.remove_splits)?;
    edit.change_splits.iter().try_for_each(|e| change_split(book, &mut t, e))?;
    edit.add_splits.iter().try_for_each(|s| check_new_split(book, s))?;
    let added = edit.add_splits.into_iter().map(|s| commit::build_split(book, env, &t.currency, normalise_split(s))).collect::<DomainResult<Vec<_>>>()?;
    t.splits.extend(added);
    if let Some(date) = edit.date_posted {
        commit::set_date_posted(&mut t, date, env.zone());
    }
    apply_fields(&mut t, edit.date_entered, edit.num, edit.description, edit.notes.as_deref());
    if t.splits.is_empty() {
        return Err(DomainError::invalid("a transaction needs at least one split; delete it instead"));
    }
    commit::commit(book, env, &mut t)?;
    book.transactions.insert(guid, Arc::new(t));
    Ok(())
}

fn apply_fields(t: &mut Transaction, entered: Option<Time64>, num: Option<String>, desc: Option<String>, notes: Option<&str>) {
    if let Some(entered) = entered {
        t.date_entered = entered;
    }
    if let Some(num) = num {
        t.num = non_empty(Some(num));
    }
    if let Some(desc) = desc {
        t.description = Some(desc);
    }
    if let Some(notes) = notes {
        commit::set_notes(t, Some(notes));
    }
}

fn remove_splits(t: &mut Transaction, guids: &[Guid]) -> DomainResult<()> {
    if let Some(missing) = guids.iter().find(|g| t.split(**g).is_none()) {
        return Err(DomainError::not_found("split", missing));
    }
    t.splits.retain(|s| !guids.contains(&s.guid));
    Ok(())
}

fn commodity_of(book: &Book, account: Option<Guid>) -> Option<CommodityRef> {
    account.and_then(|a| book.account(a)).and_then(|a| a.commodity.clone())
}

/// Apply one split change. When the value or account changes and the account holds the transaction currency, the
/// amount follows the value, as the register keeps them equal; when the account moves to another commodity, a new
/// amount must be given.
fn change_split(book: &Book, t: &mut Transaction, e: &SplitEdit) -> DomainResult<()> {
    let currency = t.currency.clone();
    let split = t.split_mut(e.guid).ok_or_else(|| DomainError::not_found("split", &e.guid))?;
    let old_commodity = commodity_of(book, split.account);
    if let Some(account) = e.account.filter(|a| split.account != Some(*a)) {
        check_postable(book, account)?;
        split.account = Some(account);
        if split.lot.is_some_and(|l| book.lot(l).is_some_and(|l| l.account != account)) {
            split.lot = None;
        }
    }
    let new_commodity = commodity_of(book, split.account);
    if let Some(value) = e.value {
        split.value = value;
    }
    match e.amount {
        Some(amount) => split.amount = amount,
        None if (e.value.is_some() || e.account.is_some()) && new_commodity.as_ref() == Some(&currency) => split.amount = split.value,
        None if new_commodity != old_commodity => {
            return Err(DomainError::invalid(format!("split {} moves to another commodity and needs an amount", e.guid)));
        }
        None => {}
    }
    if let Some(memo) = &e.memo {
        split.memo = non_empty(Some(memo.clone()));
    }
    if let Some(action) = &e.action {
        split.action = non_empty(Some(action.clone()));
    }
    match e.reconcile_state {
        Some(ReconcileState::Voided) => return Err(DomainError::invalid("a split cannot be set voided; void the transaction instead")),
        Some(state) => split.reconcile_state = state,
        None => {}
    }
    if let Some(date) = e.reconcile_date {
        split.reconcile_date = Some(date).filter(|t| *t != Time64(0));
    }
    Ok(())
}

/// Delete a transaction. A read-only one is refused, as `xaccTransDestroy` refuses it
/// (`5.17:libgnucash/engine/Transaction.cpp:1363-1374`). Its splits leave their lots with it.
pub fn delete(book: &mut Book, guid: Guid) -> DomainResult<()> {
    check_writable(get(book, guid)?)?;
    book.transactions.shift_remove(&guid);
    Ok(())
}

/// Change a transaction's currency and re-value its splits as `xaccTransSetCurrency` does
/// (`5.17:libgnucash/engine/Transaction.cpp:1259-1335`):
///
/// - the rate is amount/value of the first split whose account holds the new currency (`find_new_rate`);
/// - a split in the new currency gets value = amount;
/// - a split in the old currency gets value = amount × rate, rounded half up to the new currency's fraction;
/// - any other split gets value = amount × (value/amount × rate), rounded the same way;
/// - with no split in the new currency, each value is kept and converted to the new fraction.
///
/// GnuCash divides by the rate in the third case (`:1293-1299`), which leaves the transaction unbalanced and makes
/// its commit add an `Imbalance-<CUR>` split (oracle: a GBP split worth 110 USD became 121 EUR at 100 EUR = 110 USD).
/// red-ale multiplies, so the split keeps its worth; see `design/api/transactions.md`.
pub fn set_currency(book: &Book, t: &mut Transaction, currency: &CommodityRef) -> DomainResult<()> {
    commit::currency_fraction(book, currency)?;
    if &t.currency == currency {
        return Ok(());
    }
    let old = std::mem::replace(&mut t.currency, currency.clone());
    let commodities: Vec<Option<CommodityRef>> = t.splits.iter().map(|s| commodity_of(book, s.account)).collect();
    let rate = t
        .splits
        .iter()
        .zip(&commodities)
        .find(|(_, c)| c.as_ref() == Some(currency))
        .map(|(s, _)| s.amount.div(s.value, crate::DENOM_AUTO, How::REDUCE_NEVER))
        .transpose()?
        .filter(|r| !r.is_zero());
    let Some(rate) = rate else {
        return t.splits.iter_mut().try_for_each(|s| commit::round_value(book, currency, s.value).map(|v| s.value = v));
    };
    let fraction = commit::currency_fraction(book, currency)?;
    let half_up = How::round(Round::HalfUp);
    t.splits.iter_mut().zip(commodities).try_for_each(|(s, c)| -> DomainResult<()> {
        s.value = match c {
            Some(c) if &c == currency => s.amount,
            Some(c) if c == old => s.amount.mul(rate, fraction, half_up)?,
            _ if s.amount.is_zero() => s.value,
            _ => {
                let old_rate = s.value.div(s.amount, crate::DENOM_AUTO, How::REDUCE_NEVER)?;
                s.amount.mul(old_rate.mul(rate, crate::DENOM_AUTO, How::REDUCE_NEVER)?, fraction, half_up)?
            }
        };
        Ok(())
    })
}

/// A filter for [`list`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListFilter {
    /// Only transactions with a split in this account.
    pub account: Option<Guid>,
    /// Only transactions posted on or after this date.
    pub from: Option<Date>,
    /// Only transactions posted on or before this date.
    pub to: Option<Date>,
}

/// The posted calendar date: the `date-posted` slot when present, else the UTC date of the posted time
/// (`xaccTransGetDatePostedGDate`, `5.17:libgnucash/engine/Transaction.cpp:2259-2289`).
pub fn posted_date(t: &Transaction) -> Option<Date> {
    t.slots.get(crate::transaction::slot_paths::DATE_POSTED).and_then(crate::SlotValue::as_date).or_else(|| t.date_posted.utc_date())
}

/// Transactions matching `filter`, in GnuCash's transaction order (`xaccTransOrder`, see [`order`]).
pub fn list<'a>(book: &'a Book, filter: &ListFilter) -> Vec<&'a Transaction> {
    let in_range = |t: &Transaction| {
        let date = posted_date(t);
        filter.from.is_none_or(|f| date.is_some_and(|d| d >= f)) && filter.to.is_none_or(|to| date.is_some_and(|d| d <= to))
    };
    let mut out: Vec<&Transaction> = book
        .transactions
        .values()
        .map(AsRef::as_ref)
        .filter(|t| filter.account.is_none_or(|a| t.splits.iter().any(|s| s.account == Some(a))))
        .filter(|t| in_range(t))
        .collect();
    out.sort_by(|a, b| order::trans_order(a, b));
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Account, AccountType, FixedEnv, SlotValue};

    /// A book with USD bank and expense accounts, a EUR bank account and a GBP expense account, and a placeholder.
    pub(crate) struct Fixture {
        pub book: Book,
        pub env: FixedEnv,
        pub bank: Guid,
        pub food: Guid,
        pub eur: Guid,
        pub gbp: Guid,
        pub parent: Guid,
    }

    pub(crate) fn fixture() -> Fixture {
        let mut env = FixedEnv::new(Time64(1_719_835_200), 0);
        let mut book = Book::new_empty(&mut env);
        let mut add = |name: &str, ty, cur: &str, placeholder: bool| {
            let mut a = Account::new(env.new_guid(), name, ty, Some(CommodityRef::currency(cur)), Some(book.root));
            a.set_placeholder(placeholder);
            let g = a.guid;
            book.accounts.insert(g, Arc::new(a));
            g
        };
        let bank = add("Bank", AccountType::Bank, "USD", false);
        let food = add("Food", AccountType::Expense, "USD", false);
        let eur = add("Euro", AccountType::Bank, "EUR", false);
        let gbp = add("Pounds", AccountType::Expense, "GBP", false);
        let parent = add("Parent", AccountType::Asset, "USD", true);
        Fixture { book, env, bank, food, eur, gbp, parent }
    }

    pub(crate) fn usd(n: i64) -> Numeric {
        Numeric::new(n, 100)
    }

    pub(crate) fn simple(f: &mut Fixture, cents: i64) -> Guid {
        let mut new = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, 6, 1).unwrap());
        new.splits = vec![NewSplit::new(f.bank, usd(-cents)), NewSplit::new(f.food, usd(cents)).with_memo("lunch")];
        post(&mut f.book, &mut f.env, new).unwrap()
    }

    #[test]
    fn post_normalises_text_and_refuses_placeholders() {
        let mut f = fixture();
        let g = simple(&mut f, 1234);
        let t = f.book.transaction(g).unwrap();
        assert_eq!(t.description.as_deref(), Some(""));
        assert_eq!(t.splits[0].account, Some(f.food), "debit first");
        let mut new = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, 6, 1).unwrap());
        new.splits = vec![NewSplit::new(f.parent, usd(-1)), NewSplit::new(f.food, usd(1))];
        assert!(matches!(post(&mut f.book, &mut f.env, new), Err(DomainError::Invalid(m)) if m.contains("does not allow")));
    }

    #[test]
    fn update_changes_adds_and_removes_splits() {
        let mut f = fixture();
        let g = simple(&mut f, 1000);
        let (food_split, bank_split) = {
            let t = f.book.transaction(g).unwrap();
            (t.splits[0].guid, t.splits[1].guid)
        };
        let edit = TransactionEdit {
            description: Some("Lunch".into()),
            notes: Some("with Bob".into()),
            change_splits: vec![SplitEdit { guid: food_split, value: Some(usd(600)), memo: Some(String::new()), ..SplitEdit::default() }],
            add_splits: vec![NewSplit::new(f.food, usd(400)).with_memo("tip")],
            ..TransactionEdit::default()
        };
        update(&mut f.book, &mut f.env, g, edit).unwrap();
        let t = f.book.transaction(g).unwrap();
        assert_eq!(t.splits.len(), 3);
        let food = t.split(food_split).unwrap();
        assert!(food.amount.same_repr(usd(600)) && food.memo.is_none(), "amount follows value in the currency");
        assert_eq!(t.notes(), Some("with Bob"));
        let unbalanced = TransactionEdit { remove_splits: vec![bank_split], ..TransactionEdit::default() };
        assert!(matches!(update(&mut f.book, &mut f.env, g, unbalanced), Err(DomainError::Unbalanced { .. })));
    }

    #[test]
    fn read_only_transactions_are_refused() {
        let mut f = fixture();
        let g = simple(&mut f, 100);
        Arc::make_mut(f.book.transactions.get_mut(&g).unwrap()).slots.set("trans-read-only", SlotValue::String("Generated".into()));
        assert!(matches!(delete(&mut f.book, g), Err(DomainError::Conflict(_))));
        let edit = TransactionEdit { description: Some("x".into()), ..TransactionEdit::default() };
        assert!(matches!(update(&mut f.book, &mut f.env, g, edit), Err(DomainError::Conflict(_))));
    }

    #[test]
    fn currency_change_revalues_as_gnucash_with_the_rate_multiplied() {
        let mut f = fixture();
        let mut new = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, 6, 1).unwrap());
        new.splits = vec![
            NewSplit::new(f.eur, usd(11000)).with_amount(usd(10000)),
            NewSplit::new(f.bank, usd(-2000)),
            NewSplit::new(f.gbp, usd(-9000)).with_amount(usd(-7000)),
        ];
        let g = post(&mut f.book, &mut f.env, new).unwrap();
        let mut t = f.book.transaction(g).unwrap().clone();
        set_currency(&f.book, &mut t, &CommodityRef::currency("EUR")).unwrap();
        let values: Vec<i64> = t.splits.iter().map(|s| s.value.num()).collect();
        // GnuCash 5.17 gives 10000, -1818 and -9900 plus an Imbalance-EUR split of 1718 (oracle).
        assert_eq!(values, [10000, -1818, -8182]);
        assert!(t.is_balanced());
    }

    #[test]
    fn currency_change_without_a_split_in_the_new_currency_keeps_values() {
        let mut f = fixture();
        let g = simple(&mut f, 1234);
        let mut t = f.book.transaction(g).unwrap().clone();
        set_currency(&f.book, &mut t, &CommodityRef::currency("JPY")).unwrap();
        assert_eq!(t.splits.iter().map(|s| (s.value.num(), s.value.denom())).collect::<Vec<_>>(), [(12, 1), (-12, 1)]);
    }

    #[test]
    fn list_filters_by_account_and_date_in_order() {
        let mut f = fixture();
        let a = simple(&mut f, 100);
        let b = simple(&mut f, 200);
        let filter = ListFilter { account: Some(f.food), from: Date::from_ymd(2024, 6, 1), to: Date::from_ymd(2024, 6, 1) };
        let found: Vec<Guid> = list(&f.book, &filter).iter().map(|t| t.guid).collect();
        assert_eq!(found.len(), 2);
        assert!(found.contains(&a) && found.contains(&b));
        assert!(list(&f.book, &ListFilter { from: Date::from_ymd(2024, 6, 2), ..ListFilter::default() }).is_empty());
    }

    proptest::proptest! {
        /// Posting balanced splits keeps every value exactly; void then unvoid restores the transaction; a reversal
        /// cancels the original split by split.
        #[test]
        fn post_void_unvoid_reverse_keep_exact_balance(cents in proptest::collection::vec(-1_000_000_i64..1_000_000, 1..6)) {
            let mut f = fixture();
            let total: i64 = cents.iter().sum();
            let mut new = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, 6, 1).unwrap());
            new.splits = cents.iter().map(|c| NewSplit::new(f.food, usd(*c))).chain(std::iter::once(NewSplit::new(f.bank, usd(-total)))).collect();
            let g = post(&mut f.book, &mut f.env, new).unwrap();
            let posted = f.book.transaction(g).unwrap().clone();
            proptest::prop_assert!(posted.is_balanced());
            let mut sorted: Vec<i64> = posted.splits.iter().map(|s| s.value.num()).collect();
            let mut expected: Vec<i64> = cents.iter().copied().chain(std::iter::once(-total)).collect();
            sorted.sort_unstable();
            expected.sort_unstable();
            proptest::prop_assert_eq!(sorted, expected);
            void::void(&mut f.book, &mut f.env, g, "test").unwrap();
            proptest::prop_assert!(f.book.transaction(g).unwrap().splits.iter().all(|s| s.value.is_zero()));
            void::unvoid(&mut f.book, &mut f.env, g).unwrap();
            proptest::prop_assert_eq!(f.book.transaction(g).unwrap(), &posted);
            let r = void::reverse(&mut f.book, &mut f.env, g, Date::from_ymd(2024, 6, 2).unwrap()).unwrap();
            let net: i64 = [g, r].iter().flat_map(|t| f.book.transaction(*t).unwrap().splits.iter().map(|s| s.value.num())).sum();
            proptest::prop_assert_eq!(net, 0);
            proptest::prop_assert!(f.book.transaction(r).unwrap().is_balanced());
        }
    }
}
