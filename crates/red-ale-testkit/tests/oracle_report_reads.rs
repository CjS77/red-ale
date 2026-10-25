//! The Book reads red-ale's report query API stands on, against GnuCash 5.17 on the same book: balances as of a time,
//! in reconciled and noclosing totals, in a currency with sub-accounts, and their change over periods; every split's
//! running totals, with the num field source off and on; price rates and balance conversions (nearest, before,
//! latest; direct and through a third commodity) and the any-currency lookups behind them; and the reports' split
//! query.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use red_ale_core::ops::accounts::{AccountEdit, create_account};
use red_ale_core::ops::balances::{Total, balance_as_of, balance_as_of_in_commodity, balance_change, running_balances};
use red_ale_core::ops::commit::{NewSplit, NewTransaction, add_transaction};
use red_ale_core::ops::commodities::{NewSecurity, create_security};
use red_ale_core::ops::prices::convert::{PriceAt, convert_balance, latest_before_any_currency, nearest_any_currency, rate};
use red_ale_core::ops::transactions::find::{DescriptionPattern, PeriodQuery, period_splits};
use red_ale_core::transaction::slot_paths;
use red_ale_core::{AccountType, Book, CommodityRef, Date, DomainResult, Env, Guid, Numeric, ReconcileState, SlotValue, Time64};
use red_ale_testkit::{TempBook, call, create, diff, oracle};
use serde_json::{Value, json};

const COMMODITIES: [&str; 6] = ["CURRENCY::USD", "CURRENCY::EUR", "CURRENCY::GBP", "CURRENCY::JPY", "NASDAQ::ACME", "FUND::ZETA"];

/// Days probed, each at its posted time (the day-neutral 10:59 UTC) and one second later.
const DAYS: [&str; 14] = [
    "2024-01-01", "2024-01-05", "2024-01-06", "2024-01-20", "2024-02-10", "2024-02-15", "2024-03-01", "2024-03-02", "2024-03-15",
    "2024-04-01", "2024-05-01", "2024-06-30", "2024-07-01", "2099-06-01",
];

fn day(d: &str) -> Date {
    Date::parse(d).unwrap()
}

fn commodity(id: &str) -> CommodityRef {
    let (ns, mnemonic) = id.split_once("::").unwrap();
    CommodityRef::new(ns, mnemonic)
}

fn n(num: i64, denom: i64) -> Numeric {
    Numeric::new(num, denom)
}

fn times() -> Vec<Time64> {
    DAYS.iter().flat_map(|d| [day(d).neutral_time(), Time64(day(d).neutral_time().0 + 1)]).collect()
}

/// The accounts of the test book by full name.
type Accounts = BTreeMap<&'static str, Guid>;

fn account(book: &mut Book, env: &mut dyn Env, accounts: &mut Accounts, name: &'static str, t: AccountType, c: &str) -> DomainResult<()> {
    let (parent, leaf) = name.rsplit_once(':').map_or((book.root, name), |(p, l)| (accounts[p], l));
    let edit = AccountEdit { name: Some(leaf.into()), account_type: Some(t), commodity: Some(commodity(c)), ..AccountEdit::default() };
    accounts.insert(name, create_account(book, env, parent, &edit)?);
    Ok(())
}

struct Leg {
    account: &'static str,
    value: Numeric,
    amount: Option<Numeric>,
    action: &'static str,
    state: ReconcileState,
}

fn leg(account: &'static str, value: Numeric) -> Leg {
    Leg { account, value, amount: None, action: "", state: ReconcileState::New }
}

/// A transaction's date, num, description and currency.
type Header<'a> = (&'a str, &'a str, &'a str, &'a str);

fn post(book: &mut Book, env: &mut dyn Env, accounts: &Accounts, header: Header<'_>, legs: Vec<Leg>) -> DomainResult<Guid> {
    let (date, num, desc, currency) = header;
    let mut t = NewTransaction::new(commodity(currency), day(date));
    t.num = (!num.is_empty()).then(|| num.into());
    t.description = Some(desc.into());
    t.splits = legs
        .into_iter()
        .map(|l| {
            let mut s = NewSplit::new(accounts[l.account], l.value);
            s.action = (!l.action.is_empty()).then(|| l.action.into());
            s.amount = l.amount;
            s.reconcile_state = l.state;
            s
        })
        .collect();
    add_transaction(book, env, t)
}

/// Marks an account's split in a transaction as a stock split, as GnuCash's stock split assistant does.
fn mark_stock_split(book: &mut Book, txn: Guid, account: Guid) {
    let t = Arc::make_mut(book.transactions.get_mut(&txn).unwrap());
    let s = t.splits.iter_mut().find(|s| s.account == Some(account)).unwrap();
    s.slots.set("split-type", SlotValue::String("stock-split".into()));
    s.action = Some("Split".into());
}

fn build_accounts(book: &mut Book, env: &mut dyn Env) -> DomainResult<Accounts> {
    [("NASDAQ", "ACME", 1), ("FUND", "ZETA", 1000)].iter().try_for_each(|(ns, m, fraction)| {
        let names = ((*ns).into(), (*m).into(), (*m).into());
        let new = NewSecurity { namespace: names.0, mnemonic: names.1, fullname: names.2, fraction: *fraction, ..NewSecurity::default() };
        create_security(book, &new).map(|_| ())
    })?;
    let mut a = Accounts::new();
    [
        ("Assets", AccountType::Asset, "CURRENCY::USD"),
        ("Assets:Bank", AccountType::Bank, "CURRENCY::USD"),
        ("Assets:Euro Bank", AccountType::Bank, "CURRENCY::EUR"),
        ("Assets:Pound Bank", AccountType::Bank, "CURRENCY::GBP"),
        ("Assets:Brokerage", AccountType::Asset, "CURRENCY::USD"),
        ("Assets:Brokerage:ACME", AccountType::Stock, "NASDAQ::ACME"),
        ("Assets:Brokerage:Zeta", AccountType::Mutual, "FUND::ZETA"),
        ("Income", AccountType::Income, "CURRENCY::USD"),
        ("Income:Salary", AccountType::Income, "CURRENCY::USD"),
        ("Expenses", AccountType::Expense, "CURRENCY::USD"),
        ("Expenses:Food", AccountType::Expense, "CURRENCY::USD"),
        ("Expenses:Euro Food", AccountType::Expense, "CURRENCY::EUR"),
        ("Equity", AccountType::Equity, "CURRENCY::USD"),
    ]
    .iter()
    .try_for_each(|(name, t, c)| account(book, env, &mut a, name, *t, c))?;
    Ok(a)
}

/// Transactions on several accounts: three on one day whose nums and actions disagree, cross-currency transfers,
/// reconciled and cleared splits, a closing transaction on the same day as an ordinary one, and stock splits GnuCash
/// skips. Returns the void candidate.
fn build_transactions(book: &mut Book, env: &mut dyn Env, a: &Accounts) -> DomainResult<Guid> {
    let usd = |cents: i64| n(cents, 100);
    let with = |mut l: Leg, f: &dyn Fn(&mut Leg)| {
        f(&mut l);
        l
    };
    post(book, env, a, ("2024-01-05", "1", "Salary", "CURRENCY::USD"), vec![
        with(leg("Assets:Bank", usd(300_000)), &|l| l.action = "3"),
        with(leg("Income:Salary", usd(-300_000)), &|l| l.state = ReconcileState::Reconciled),
    ])?;
    post(book, env, a, ("2024-01-05", "10", "Groceries", "CURRENCY::USD"), vec![
        leg("Expenses:Food", usd(4550)),
        with(leg("Assets:Bank", usd(-4550)), &|l| l.action = "1"),
    ])?;
    post(book, env, a, ("2024-01-05", "2", "Lunch", "CURRENCY::USD"), vec![
        leg("Expenses:Food", usd(1225)),
        with(leg("Assets:Bank", usd(-1225)), &|l| {
            l.action = "2";
            l.state = ReconcileState::Cleared;
        }),
    ])?;
    post(book, env, a, ("2024-01-20", "", "To euros", "CURRENCY::USD"), vec![
        with(leg("Assets:Euro Bank", usd(110_000)), &|l| l.amount = Some(n(100_000, 100))),
        leg("Assets:Bank", usd(-110_000)),
    ])?;
    let paris = ("2024-02-03", "", "Groceries in Paris", "CURRENCY::EUR");
    post(book, env, a, paris, vec![leg("Expenses:Euro Food", usd(3333)), leg("Assets:Euro Bank", usd(-3333))])?;
    post(book, env, a, ("2024-02-10", "", "Buy ACME", "CURRENCY::USD"), vec![
        with(leg("Assets:Brokerage:ACME", usd(150_000)), &|l| l.amount = Some(n(10, 1))),
        leg("Assets:Bank", usd(-150_000)),
    ])?;
    post(book, env, a, ("2024-03-15", "", "Sell ACME", "CURRENCY::USD"), vec![
        with(leg("Assets:Brokerage:ACME", usd(-90_000)), &|l| l.amount = Some(n(-5, 1))),
        leg("Assets:Bank", usd(90_000)),
    ])?;
    post(book, env, a, ("2024-04-01", "", "To pounds", "CURRENCY::USD"), vec![
        with(leg("Assets:Pound Bank", usd(64_000)), &|l| l.amount = Some(n(50_000, 100))),
        leg("Assets:Bank", usd(-64_000)),
    ])?;
    let mistake = ("2024-05-01", "", "Mistake", "CURRENCY::USD");
    let mistake = post(book, env, a, mistake, vec![leg("Expenses:Food", usd(9900)), leg("Assets:Bank", usd(-9900))])?;
    let closing = ("2024-06-30", "", "Closing Entries", "CURRENCY::USD");
    let closing = post(book, env, a, closing, vec![leg("Income:Salary", usd(300_000)), leg("Equity", usd(-300_000))])?;
    Arc::make_mut(book.transactions.get_mut(&closing).unwrap()).slots.set(slot_paths::BOOK_CLOSING, SlotValue::Integer(1));
    post(book, env, a, ("2024-06-30", "", "Snack", "CURRENCY::USD"), vec![leg("Expenses:Food", usd(700)), leg("Assets:Bank", usd(-700))])?;
    zeta_stock_splits(book, env, a)?;
    Ok(mistake)
}

/// Zeta: a stock split while it holds nothing, a purchase, a stock split that would empty it, and a purchase. GnuCash
/// skips both stock splits.
fn zeta_stock_splits(book: &mut Book, env: &mut dyn Env, a: &Accounts) -> DomainResult<()> {
    let zeta = a["Assets:Brokerage:Zeta"];
    let shares = |s: i64| Some(n(s, 1));
    let split = |date| (date, "", "Split", "CURRENCY::USD");
    let empty = post(book, env, a, split("2024-01-06"), vec![Leg { amount: shares(5), ..leg("Assets:Brokerage:Zeta", n(0, 100)) }])?;
    mark_stock_split(book, empty, zeta);
    post(book, env, a, ("2024-02-15", "", "Buy Zeta", "CURRENCY::USD"), vec![
        Leg { amount: shares(10), ..leg("Assets:Brokerage:Zeta", n(3000, 100)) },
        leg("Assets:Bank", n(-3000, 100)),
    ])?;
    let emptying = post(book, env, a, split("2024-03-02"), vec![Leg { amount: shares(-10), ..leg("Assets:Brokerage:Zeta", n(0, 100)) }])?;
    mark_stock_split(book, emptying, zeta);
    post(book, env, a, ("2024-04-01", "", "Buy Zeta", "CURRENCY::USD"), vec![
        Leg { amount: shares(2), ..leg("Assets:Brokerage:Zeta", n(600, 100)) },
        leg("Assets:Bank", n(-600, 100)),
    ])?;
    Ok(())
}

/// Prices: EUR in USD (one far in the future) and once the other way round; GBP in USD; ACME in USD; JPY only in EUR
/// and Zeta only in GBP, which only an indirect price reaches from USD.
const PRICES: [(&str, &str, &str, &str); 11] = [
    ("CURRENCY::EUR", "CURRENCY::USD", "2024-01-20", "11/10"),
    ("CURRENCY::EUR", "CURRENCY::USD", "2024-03-01", "27/25"),
    ("CURRENCY::EUR", "CURRENCY::USD", "2099-01-01", "3/2"),
    ("CURRENCY::USD", "CURRENCY::EUR", "2024-02-15", "23/25"),
    ("CURRENCY::GBP", "CURRENCY::USD", "2024-04-01", "32/25"),
    ("NASDAQ::ACME", "CURRENCY::USD", "2024-02-10", "150"),
    ("NASDAQ::ACME", "CURRENCY::USD", "2024-03-01", "75"),
    ("NASDAQ::ACME", "CURRENCY::USD", "2024-03-15", "180"),
    ("CURRENCY::JPY", "CURRENCY::EUR", "2024-02-01", "31/5000"),
    ("FUND::ZETA", "CURRENCY::GBP", "2024-03-03", "5/2"),
    ("FUND::ZETA", "CURRENCY::GBP", "2024-05-01", "11/4"),
];

/// The test book, saved; the num field source on or off.
fn report_book(num_source: bool) -> (TempBook, Accounts) {
    let (book, mut d) = create();
    let (accounts, mistake) = d
        .context()
        .mutate(|book, env| {
            let a = build_accounts(book, env)?;
            let mistake = build_transactions(book, env, &a)?;
            red_ale_core::book_options::set_split_action_for_num(book, num_source);
            Ok((a, mistake))
        })
        .unwrap();
    call(&mut d, "stock.split", &json!({ "account": "Assets:Brokerage:ACME", "date": "2024-03-01", "shares": "10" }));
    call(&mut d, "transaction.void", &json!({ "guid": mistake.to_string(), "reason": "typo" }));
    PRICES.iter().for_each(|(c, cur, date, value)| {
        call(&mut d, "price.add", &json!({ "commodity": c, "currency": cur, "date": date, "value": value }));
    });
    call(&mut d, "book.close", &json!({ "save": true }));
    (book, accounts)
}

fn frac(n: Numeric) -> String {
    let r = n.reduce();
    format!("{}/{}", r.num(), r.denom())
}

fn fracs(v: impl IntoIterator<Item = DomainResult<Numeric>>) -> Vec<String> {
    v.into_iter().map(|x| x.map_or_else(|_| "error".to_owned(), frac)).collect()
}

fn now() -> Time64 {
    Time64(i64::try_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()).unwrap())
}

/// The split queries asked of both: accounts by full name, from, to, description pattern, closing.
type QuerySpec = (&'static [&'static str], Option<&'static str>, Option<&'static str>, Option<(&'static str, bool, bool)>, bool);

const QUERIES: [QuerySpec; 7] = [
    (&["Expenses:Food", "Expenses:Euro Food", "Income:Salary"], None, None, None, false),
    (&["Assets:Bank"], Some("2024-01-05"), Some("2024-03-15"), None, false),
    (&["Assets:Bank", "Assets:Brokerage:ACME"], None, Some("2024-01-20"), None, false),
    (&["Income:Salary", "Equity"], None, None, None, true),
    (&["Income:Salary", "Expenses:Food"], None, None, Some(("closing", false, false)), false),
    (&["Expenses:Food", "Income:Salary"], None, None, Some(("^Gro", true, true)), true),
    (&["Expenses:Food"], None, None, Some(("", true, false)), false),
];

fn query_json(q: &QuerySpec, a: &Accounts) -> Value {
    let (accounts, from, to, pattern, closing) = q;
    let t = |d: Option<&str>| d.map(|d| day(d).neutral_time().0);
    json!({
        "accounts": accounts.iter().map(|x| a[x].to_string()).collect::<Vec<_>>(),
        "from": t(*from), "to": t(*to),
        "str": pattern.map(|p| p.0), "cased": pattern.is_some_and(|p| p.1), "regexp": pattern.is_some_and(|p| p.2),
        "closing": closing,
    })
}

fn query(q: &QuerySpec, a: &Accounts) -> PeriodQuery {
    let (accounts, from, to, pattern, closing) = q;
    PeriodQuery {
        accounts: accounts.iter().map(|x| a[x]).collect(),
        from: from.map(|d| day(d).neutral_time()),
        to: to.map(|d| day(d).neutral_time()),
        description: pattern.map(|(text, cased, regexp)| DescriptionPattern { text: text.into(), cased, regexp }),
        closing: *closing,
    }
}

fn gnucash(book: &TempBook, a: &Accounts) -> Value {
    oracle::check_version().unwrap();
    let spec = json!({
        "times": times().iter().map(|t| t.0).collect::<Vec<_>>(),
        "currency": "CURRENCY::USD",
        "commodities": COMMODITIES,
        "amount": [123_456, 100],
        "queries": QUERIES.iter().map(|q| query_json(q, a)).collect::<Vec<_>>(),
    });
    let out = oracle::run_script(&oracle::scripts_dir().join("report_reads.py"), &[&book.path_str(), &spec.to_string()])
        .unwrap_or_else(|e| panic!("oracle: {e}"));
    serde_json::from_str(&out).unwrap()
}

fn account_reads(book: &Book, g: Guid) -> Value {
    let ts = times();
    let usd = commodity("CURRENCY::USD");
    let each = |f: &dyn Fn(Time64) -> DomainResult<Numeric>| fracs(ts.iter().map(|t| f(*t)));
    let periods = |total| fracs(ts.windows(2).map(|w| balance_change(book, g, w[0], w[1], total, true)));
    let splits: Vec<Value> = running_balances(book, g)
        .unwrap()
        .iter()
        .map(|s| {
            let r = s.running;
            json!([s.split.guid.to_string(), frac(r.balance), frac(r.noclosing), frac(r.cleared), frac(r.reconciled)])
        })
        .collect();
    json!({
        "asOf": each(&|t| balance_as_of(book, g, t, Total::Balance)),
        "reconciledAsOf": each(&|t| balance_as_of(book, g, t, Total::Reconciled)),
        "noclosingAsOf": each(&|t| balance_as_of_in_commodity(book, g, t, Total::Noclosing, None, false)),
        "inCurrency": each(&|t| balance_as_of_in_commodity(book, g, t, Total::Balance, Some(&usd), true)),
        "noclosingInCurrency": each(&|t| balance_as_of_in_commodity(book, g, t, Total::Noclosing, Some(&usd), true)),
        "change": periods(Total::Balance),
        "noclosingChange": periods(Total::Noclosing),
        "splits": splits,
    })
}

fn price_reads(book: &Book, from: &CommodityRef, to: &CommodityRef, now: Time64) -> Value {
    let ts = times();
    let amount = n(123_456, 100);
    let rates = |at: &dyn Fn(Time64) -> PriceAt| fracs(ts.iter().map(|t| Ok(rate(book, from, to, at(*t))?)));
    let converted = |at: &dyn Fn(Time64) -> PriceAt| fracs(ts.iter().map(|t| convert_balance(book, amount, from, to, at(*t))));
    let latest = PriceAt::Latest { now };
    json!({
        "nearest": rates(&PriceAt::Nearest),
        "before": rates(&PriceAt::Before),
        "latest": fracs([Ok(rate(book, from, to, latest).unwrap())]).remove(0),
        "convertNearest": converted(&PriceAt::Nearest),
        "convertBefore": converted(&PriceAt::Before),
        "convertLatest": fracs([convert_balance(book, amount, from, to, latest)]).remove(0),
    })
}

fn red_ale(book: &TempBook, a: &Accounts) -> Value {
    let bytes = std::fs::read(&book.path).unwrap();
    let (ours, _) = red_ale_xml::read_book(&bytes).unwrap();
    let now = now();
    let accounts: serde_json::Map<String, Value> =
        ours.accounts.keys().filter(|g| **g != ours.root).map(|g| (g.to_string(), account_reads(&ours, *g))).collect();
    let ids: Vec<CommodityRef> = COMMODITIES.iter().map(|c| commodity(c)).collect();
    let prices: serde_json::Map<String, Value> = COMMODITIES
        .iter()
        .zip(&ids)
        .flat_map(|(f, fc)| COMMODITIES.iter().zip(&ids).map(move |(t, tc)| (f, fc, t, tc)))
        .map(|(f, fc, t, tc)| (format!("{f}>{t}"), price_reads(&ours, fc, tc, now)))
        .collect();
    let guids = |ps: Vec<&red_ale_core::Price>| ps.iter().map(|p| p.guid.to_string()).collect::<Vec<_>>();
    let any: serde_json::Map<String, Value> = COMMODITIES
        .iter()
        .zip(&ids)
        .map(|(c, id)| {
            let nearest: Vec<Vec<String>> = times().iter().map(|t| guids(nearest_any_currency(&ours, id, *t))).collect();
            let before: Vec<Vec<String>> = times().iter().map(|t| guids(latest_before_any_currency(&ours, id, *t))).collect();
            ((*c).to_owned(), json!({ "nearest": nearest, "before": before }))
        })
        .collect();
    let queries: Vec<Vec<String>> =
        QUERIES.iter().map(|q| period_splits(&ours, &query(q, a)).unwrap().iter().map(|(_, s)| s.guid.to_string()).collect()).collect();
    json!({ "accounts": accounts, "prices": prices, "anyCurrency": any, "queries": queries })
}

fn compare(num_source: bool) {
    let (book, accounts) = report_book(num_source);
    let theirs = gnucash(&book, &accounts);
    let ours = red_ale(&book, &accounts);
    assert_eq!(theirs["accounts"].as_object().map(serde_json::Map::len), Some(13), "every account but the root");
    let diffs = diff(&theirs, &ours, &[]);
    assert!(diffs.is_empty(), "num source {num_source}: {} differences:\n{}", diffs.len(), diffs.join("\n"));
}

#[test]
fn report_reads_match_gnucash() {
    compare(false);
}

#[test]
fn report_reads_match_gnucash_with_the_num_field_source() {
    compare(true);
}
