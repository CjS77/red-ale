//! Closing the books: GnuCash's Close Book dialog as one operation. At a closing date, the balances of the income
//! accounts, and then of the expense accounts, move into equity in one closing transaction per currency
//! (`5.17:gnucash/gnome-utils/dialog-book-close.c:89-316`, GPL-2.0-or-later; the steps below follow that code).
//! See `design/api/close-book.md`.

use std::sync::Arc;

use crate::ops::accounts::{AccountEdit, balances, create_account};
use crate::ops::commit::{NewSplit, NewTransaction, add_transaction};
use crate::ops::lots::lookup_by_name;
use crate::transaction::slot_paths;
use crate::{AccountType, Book, CommodityRef, Date, DomainError, DomainResult, Env, Guid, Numeric, SlotValue, Time64};

/// What to close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseBooks {
    /// The closing date: the transactions are posted on it.
    pub date: Date,
    /// The last second of the closing date in local time. Balances count splits posted strictly before it
    /// (`gnc_time64_get_day_end`, `5.17:libgnucash/engine/gnc-date.cpp:1386-1394`).
    pub day_end: Time64,
    /// The equity account taking the income (the dialog's *Income Total*).
    pub income_total: Guid,
    /// The equity account taking the expense (the dialog's *Expense Total*).
    pub expense_total: Guid,
    /// The closing transactions' description.
    pub description: String,
}

/// One closing transaction made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosingTransaction {
    /// The transaction.
    pub guid: Guid,
    /// `Income` or `Expense`: the type of the accounts it closes.
    pub kind: AccountType,
    /// Its currency.
    pub currency: CommodityRef,
    /// The equity account its balancing split posts to.
    pub equity_account: Guid,
    /// The equity split's value: the sum of the closed balances.
    pub total: Numeric,
}

/// What closing the books did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CloseReport {
    /// The closing transactions, income first, then by currency in the order the accounts were met.
    pub transactions: Vec<ClosingTransaction>,
    /// Equity accounts made for a currency.
    pub created_accounts: Vec<Guid>,
}

/// The accounts of one type with a balance, grouped by currency in the order they are first met.
type Groups = Vec<(CommodityRef, Vec<(Guid, Numeric)>)>;

/// Close the books: income into `income_total`, then expense into `expense_total`
/// (`gnc_book_close_response_cb`, `5.17:gnucash/gnome-utils/dialog-book-close.c:284-308`).
pub fn close_books(book: &mut Book, env: &mut dyn Env, req: &CloseBooks) -> DomainResult<CloseReport> {
    [req.income_total, req.expense_total].iter().try_for_each(|g| check_equity(book, *g))?;
    let mut report = CloseReport::default();
    close_type(book, env, req, AccountType::Income, req.income_total, &mut report)?;
    close_type(book, env, req, AccountType::Expense, req.expense_total, &mut report)?;
    Ok(report)
}

/// The dialog offers only equity accounts (`5.17:gnucash/gnome-utils/dialog-book-close.c:350-365`).
fn check_equity(book: &Book, guid: Guid) -> DomainResult<()> {
    let acc = book.account(guid).ok_or_else(|| DomainError::not_found("account", &guid))?;
    if acc.account_type == AccountType::Equity {
        Ok(())
    } else {
        let name = book.full_name(guid).unwrap_or_default();
        Err(DomainError::invalid(format!("{name} is not an equity account; closing the books needs one")))
    }
}

/// `close_accounts_of_type` (`5.17:gnucash/gnome-utils/dialog-book-close.c:214-242`).
fn close_type(
    book: &mut Book,
    env: &mut dyn Env,
    req: &CloseBooks,
    kind: AccountType,
    base: Guid,
    report: &mut CloseReport,
) -> DomainResult<()> {
    let groups = balances_by_currency(book, kind, req.day_end)?;
    let single = groups.len() == 1;
    groups.into_iter().try_for_each(|(currency, accounts)| {
        let (equity_account, created) = equity_account(book, env, base, &currency, single)?;
        report.created_accounts.extend(created);
        let total = accounts.iter().try_fold(Numeric::ZERO, |sum, (_, bal)| sum.add_fixed(*bal))?;
        let guid = post_closing(book, env, req, &currency, &accounts, equity_account, total)?;
        report.transactions.push(ClosingTransaction { guid, kind, currency, equity_account, total });
        Ok(())
    })
}

/// Every account of exactly `kind`, depth first in tree order, with a non-zero balance at `day_end`, grouped by its
/// currency (`close_accounts_cb`, `5.17:gnucash/gnome-utils/dialog-book-close.c:123-157`).
fn balances_by_currency(book: &Book, kind: AccountType, day_end: Time64) -> DomainResult<Groups> {
    let mut groups: Groups = Vec::new();
    book.descendants(book.root).iter().filter(|a| a.account_type == kind).try_for_each(|a| -> DomainResult<()> {
        let bal = balances(book, a.guid, Some(day_end))?.balance;
        if bal.is_zero() {
            return Ok(());
        }
        let currency = a.commodity.clone().filter(CommodityRef::is_currency).ok_or_else(|| {
            let name = book.full_name(a.guid).unwrap_or_default();
            DomainError::invalid(format!("{name} has a balance in a commodity that is not a currency; it cannot be closed into equity"))
        })?;
        match groups.iter_mut().find(|(c, _)| *c == currency) {
            Some((_, accounts)) => accounts.push((a.guid, bal)),
            None => groups.push((currency, vec![(a.guid, bal)])),
        }
        Ok(())
    })?;
    Ok(groups)
}

/// The equity account for one currency, and whether it was made (`finish_txn_cb`,
/// `5.17:gnucash/gnome-utils/dialog-book-close.c:172-200`): the chosen account when it is the only currency and the
/// account's own; else its descendant named after the currency; else a new child.
fn equity_account(
    book: &mut Book,
    env: &mut dyn Env,
    base: Guid,
    currency: &CommodityRef,
    single: bool,
) -> DomainResult<(Guid, Option<Guid>)> {
    let base_commodity = book.account(base).and_then(|a| a.commodity.clone());
    if single && base_commodity.as_ref() == Some(currency) {
        return Ok((base, None));
    }
    if let Some(found) = lookup_by_name(book, base, &currency.mnemonic) {
        let commodity = book.account(found).and_then(|a| a.commodity.as_ref());
        if commodity != Some(currency) {
            let name = book.full_name(found).unwrap_or_default();
            return Err(DomainError::invalid(format!("{name} should hold {currency} closing entries but is in another commodity")));
        }
        return Ok((found, None));
    }
    let edit = AccountEdit {
        name: Some(currency.mnemonic.clone()),
        account_type: Some(AccountType::Equity),
        commodity: Some(currency.clone()),
        description: Some(currency.mnemonic.clone()),
        ..AccountEdit::default()
    };
    let made = create_account(book, env, base, &edit)?;
    Ok((made, Some(made)))
}

/// One closing transaction (`find_or_create_txn`, `5.17:gnucash/gnome-utils/dialog-book-close.c:89-115`): each
/// account's split takes minus its balance, the equity split the total; the transaction is flagged `book_closing`
/// (`xaccTransSetIsClosingTxn`, `5.17:libgnucash/engine/Transaction.cpp:2049-2055`). The splits go to the accounts
/// as they are, placeholders included, as the dialog does.
fn post_closing(
    book: &mut Book,
    env: &mut dyn Env,
    req: &CloseBooks,
    currency: &CommodityRef,
    accounts: &[(Guid, Numeric)],
    equity: Guid,
    total: Numeric,
) -> DomainResult<Guid> {
    let mut new = NewTransaction::new(currency.clone(), req.date);
    new.description = Some(req.description.clone());
    new.splits = accounts.iter().map(|(a, bal)| NewSplit::new(*a, bal.neg()).with_amount(bal.neg())).collect();
    new.splits.push(NewSplit::new(equity, total).with_amount(total));
    let guid = add_transaction(book, env, new)?;
    if let Some(t) = book.transactions.get_mut(&guid) {
        Arc::make_mut(t).slots.set(slot_paths::BOOK_CLOSING, SlotValue::Integer(1));
    }
    Ok(guid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Account, FixedEnv};

    struct Fixture {
        book: Book,
        env: FixedEnv,
    }

    fn usd() -> CommodityRef {
        CommodityRef::currency("USD")
    }

    impl Fixture {
        fn new() -> Self {
            let mut env = FixedEnv::new(Time64(1_719_835_200), 0);
            let book = Book::new_empty(&mut env);
            Self { book, env }
        }

        fn add(&mut self, name: &str, ty: AccountType, commodity: &CommodityRef, parent: Option<Guid>) -> Guid {
            let a = Account::new(self.env.new_guid(), name, ty, Some(commodity.clone()), Some(parent.unwrap_or(self.book.root)));
            let g = a.guid;
            self.book.accounts.insert(g, Arc::new(a));
            g
        }

        fn post(&mut self, day: (u32, u32), a: Guid, b: Guid, value: &str) {
            let currency = self.book.account(a).and_then(|x| x.commodity.clone()).unwrap();
            let mut t = NewTransaction::new(currency, Date::from_ymd(2024, day.0, day.1).unwrap());
            let v = Numeric::parse_any(value).unwrap();
            t.splits = vec![NewSplit::new(a, v), NewSplit::new(b, v.neg())];
            add_transaction(&mut self.book, &mut self.env, t).unwrap();
        }

        fn request(income: Guid, expense: Guid) -> CloseBooks {
            let date = Date::from_ymd(2024, 6, 30).unwrap();
            CloseBooks { date, day_end: date.end_utc(), income_total: income, expense_total: expense, description: "Closing".into() }
        }

        fn balance(&self, a: Guid, at: Time64) -> Numeric {
            balances(&self.book, a, Some(at)).unwrap().balance
        }
    }

    fn n(s: &str) -> Numeric {
        Numeric::parse_any(s).unwrap()
    }

    #[test]
    fn closes_one_currency_into_the_chosen_accounts() {
        let mut f = Fixture::new();
        let bank = f.add("Bank", AccountType::Bank, &usd(), None);
        let income = f.add("Income", AccountType::Income, &usd(), None);
        let salary = f.add("Salary", AccountType::Income, &usd(), Some(income));
        let food = f.add("Food", AccountType::Expense, &usd(), None);
        let equity = f.add("Equity", AccountType::Equity, &usd(), None);
        let retained = f.add("Retained", AccountType::Equity, &usd(), Some(equity));
        f.post((1, 5), bank, salary, "1000.00");
        f.post((2, 5), bank, income, "50.25");
        f.post((3, 5), food, bank, "120.10");
        f.post((7, 1), bank, salary, "999.00");
        let req = Fixture::request(retained, retained);
        let report = close_books(&mut f.book, &mut f.env, &req).unwrap();
        assert!(report.created_accounts.is_empty());
        let totals: Vec<(AccountType, Numeric)> = report.transactions.iter().map(|t| (t.kind, t.total)).collect();
        assert_eq!(totals, [(AccountType::Income, n("-1050.25")), (AccountType::Expense, n("120.10"))]);
        let income_txn = f.book.transaction(report.transactions[0].guid).unwrap();
        let splits: Vec<(Option<Guid>, Numeric)> = income_txn.splits.iter().map(|s| (s.account, s.value)).collect();
        let expected = [(Some(income), n("50.25")), (Some(salary), n("1000.00")), (Some(retained), n("-1050.25"))];
        assert_eq!(splits, expected, "debits first, tree order");
        assert_eq!(income_txn.slots.get(slot_paths::BOOK_CLOSING), Some(&SlotValue::Integer(1)));
        assert_eq!(income_txn.description.as_deref(), Some("Closing"));
        assert_eq!(income_txn.date_posted, req.date.neutral_time());
        [income, salary, food].iter().for_each(|a| assert!(f.balance(*a, req.day_end).is_zero()));
        assert_eq!(f.balance(salary, Date::from_ymd(2024, 7, 2).unwrap().end_utc()), n("-999.00"), "later income stays");
        assert_eq!(f.balance(retained, req.day_end), n("-930.15"));
    }

    #[test]
    fn several_currencies_get_equity_accounts_named_after_them() {
        let mut f = Fixture::new();
        let eur = CommodityRef::currency("EUR");
        let bank = f.add("Bank", AccountType::Bank, &usd(), None);
        let konto = f.add("Konto", AccountType::Bank, &eur, None);
        let sales = f.add("Sales", AccountType::Income, &usd(), None);
        let verkauf = f.add("Verkauf", AccountType::Income, &eur, None);
        let equity = f.add("Equity", AccountType::Equity, &usd(), None);
        let existing = f.add("EUR", AccountType::Equity, &eur, Some(equity));
        f.post((1, 5), bank, sales, "10.00");
        f.post((1, 6), konto, verkauf, "20.00");
        let req = Fixture::request(equity, equity);
        let report = close_books(&mut f.book, &mut f.env, &req).unwrap();
        assert_eq!(report.transactions.len(), 2);
        assert_eq!(report.transactions[1].equity_account, existing, "found by name");
        let made = report.transactions[0].equity_account;
        assert_eq!(report.created_accounts, [made]);
        let acc = f.book.account(made).unwrap();
        let got = (acc.name.as_str(), acc.account_type, acc.description.as_deref(), acc.parent);
        assert_eq!(got, ("USD", AccountType::Equity, Some("USD"), Some(equity)));
        assert_eq!(f.balance(made, req.day_end), n("-10.00"));
    }

    #[test]
    fn nothing_to_close_makes_nothing_and_bad_accounts_are_refused() {
        let mut f = Fixture::new();
        let equity = f.add("Equity", AccountType::Equity, &usd(), None);
        let bank = f.add("Bank", AccountType::Bank, &usd(), None);
        assert_eq!(close_books(&mut f.book, &mut f.env, &Fixture::request(equity, equity)).unwrap(), CloseReport::default());
        assert!(matches!(close_books(&mut f.book, &mut f.env, &Fixture::request(bank, equity)), Err(DomainError::Invalid(_))));

        let shares = CommodityRef::new("NASDAQ", "ACME");
        let odd = f.add("Odd", AccountType::Income, &shares, None);
        let holding = f.add("Holding", AccountType::Stock, &shares, None);
        f.book.commodities.insert(shares.clone(), Arc::new(crate::Commodity::new(shares.clone(), None, 1)));
        let mut t = NewTransaction::new(usd(), Date::from_ymd(2024, 1, 1).unwrap());
        t.splits = vec![NewSplit::new(holding, n("0")).with_amount(n("5")), NewSplit::new(odd, n("0")).with_amount(n("-5"))];
        add_transaction(&mut f.book, &mut f.env, t).unwrap();
        assert!(matches!(close_books(&mut f.book, &mut f.env, &Fixture::request(equity, equity)), Err(DomainError::Invalid(_))));
    }
}
