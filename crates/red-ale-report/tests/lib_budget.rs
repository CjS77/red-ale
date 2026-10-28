//! Budgets in reports: the query API's budget reads (`5.17:libgnucash/engine/gnc-budget.cpp`), `lib/budget`
//! (`5.17:gnucash/report/report-utilities.scm:727-856`, RU), the account table's `report_budget`
//! (`5.17:gnucash/report/html-acct-table.scm:749-756`, HAT) and the budget, complex boolean and multichoice-callback
//! options, on a book built here, in UTC.
//!
//! The book: Expenses (USD, no splits of its own) holds Food (USD), Rent (USD), Spare (USD) and Travel (EUR). Food
//! has $10.00 on 15 January 2024, $1.00 at 23:59:59 on 31 January and $2.00 at midnight on 1 February; Travel EUR 3.33
//! on 20 January; Bank opens with $100.00 on 15 December 2023. EUR is priced $1.10 on 10 January and $1.30 on 15
//! February. The budget "Plan" runs monthly from 1 January 2024 for 3 periods: Food 12.34 in period 0 (note
//! "January") and an explicit 0 in period 1, Travel EUR 5 and EUR 10 in periods 0 and 1, Rent 700 in period 2. A
//! second budget, "Other", has nothing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod libs;

use std::sync::Arc;

use common::{GUID, Host, registry_with};
use libs::{check, lines};
use red_ale_core::ops::accounts::{AccountEdit, create_account};
use red_ale_core::ops::budgets::{BudgetFields, Cell, create, set_amounts, set_default_budget, set_notes};
use red_ale_core::ops::commit::{NewSplit, NewTransaction, add_transaction};
use red_ale_core::{
    AccountType, Book, CommodityRef, Date, FixedEnv, Guid, Numeric, PeriodType, Price, Recurrence, Time64, TimeZone, WeekendAdjust,
};
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;
use serde_json::json;

fn day(y: i32, m: u32, d: u32) -> Date {
    Date::from_ymd(y, m, d).unwrap()
}

struct Built {
    book: Book,
    env: FixedEnv,
}

impl Built {
    fn account(&mut self, parent: Guid, name: &str, t: AccountType, currency: &str) -> Guid {
        let edit = AccountEdit {
            name: Some(name.into()),
            account_type: Some(t),
            commodity: Some(CommodityRef::currency(currency)),
            ..AccountEdit::default()
        };
        create_account(&mut self.book, &mut self.env, parent, &edit).unwrap()
    }

    /// `cents` from `from` to `to`, posted at `at` (seconds into the UTC day `d`).
    fn post(&mut self, currency: &str, to: Guid, from: Guid, cents: i64, d: Date, at: i64) {
        let mut t = NewTransaction::new(CommodityRef::currency(currency), d);
        t.splits = vec![NewSplit::new(to, Numeric::new(cents, 100)), NewSplit::new(from, Numeric::new(-cents, 100))];
        let g = add_transaction(&mut self.book, &mut self.env, t).unwrap();
        let posted = TimeZone::utc().day_start(d).0 + at;
        Arc::make_mut(self.book.transactions.get_mut(&g).unwrap()).date_posted = Time64(posted);
    }

    fn price(&mut self, k: u8, d: Date, value: Numeric) {
        let p = Price {
            guid: Guid::from_bytes([k; 16]),
            commodity: CommodityRef::currency("EUR"),
            currency: CommodityRef::currency("USD"),
            time: d.neutral_time(),
            source: None,
            price_type: None,
            value,
        };
        self.book.prices.insert(p.guid, Arc::new(p));
    }
}

/// The book of the module comment, with "Other" as the default budget when `other_default`.
fn book(other_default: bool) -> Book {
    let mut env = FixedEnv::new(Time64(1_710_504_000), 0);
    let book = Book::new_empty(&mut env);
    let mut b = Built { book, env };
    let root = b.book.root;
    let expenses = b.account(root, "Expenses", AccountType::Expense, "USD");
    let food = b.account(expenses, "Food", AccountType::Expense, "USD");
    let rent = b.account(expenses, "Rent", AccountType::Expense, "USD");
    b.account(expenses, "Spare", AccountType::Expense, "USD");
    let travel = b.account(expenses, "Travel", AccountType::Expense, "EUR");
    let bank = b.account(root, "Bank", AccountType::Bank, "USD");
    let eur_bank = b.account(root, "EurBank", AccountType::Bank, "EUR");
    let equity = b.account(root, "Equity", AccountType::Equity, "USD");
    b.post("USD", bank, equity, 10_000, day(2023, 12, 15), 39_540);
    b.post("USD", food, bank, 1000, day(2024, 1, 15), 39_540);
    b.post("USD", food, bank, 100, day(2024, 1, 31), 86_399);
    b.post("USD", food, bank, 200, day(2024, 2, 1), 0);
    b.post("EUR", travel, eur_bank, 333, day(2024, 1, 20), 39_540);
    b.price(1, day(2024, 1, 10), Numeric::new(110, 100));
    b.price(2, day(2024, 2, 15), Numeric::new(130, 100));
    let fields = BudgetFields {
        name: Some("Plan".into()),
        num_periods: Some(3),
        recurrence: Some(Recurrence::new(1, PeriodType::Month, day(2024, 1, 1), WeekendAdjust::None).unwrap()),
        ..BudgetFields::default()
    };
    let plan = create(&mut b.book, &mut b.env, fields).unwrap();
    let cell = |account, period| Cell { account, period };
    let amounts = [
        (cell(food, 0), Some(Numeric::new(1234, 100))),
        (cell(food, 1), Some(Numeric::new(0, 1))),
        (cell(travel, 0), Some(Numeric::new(5, 1))),
        (cell(travel, 1), Some(Numeric::new(10, 1))),
        (cell(rent, 2), Some(Numeric::new(700, 1))),
    ];
    set_amounts(&mut b.book, plan, &amounts).unwrap();
    set_notes(&mut b.book, plan, &[(cell(food, 0), Some("January".into()))]).unwrap();
    let other = BudgetFields {
        name: Some("Other".into()),
        ..BudgetFields::default()
    };
    let other = create(&mut b.book, &mut b.env, other).unwrap();
    if other_default {
        set_default_budget(&mut b.book, other).unwrap();
    }
    b.book
}

const PRELUDE: &str = r#"
import "lib/budget" as budget;
import "lib/collectors" as collectors;
let usd = book.commodity("USD");
let plan = book.budgets()[0];
let a = |name| book.account(name);
let iso = |t| ctx.print_time_iso(t);
"#;

fn run(host: &Host, body: &str) -> Vec<String> {
    lines(host, &format!("{PRELUDE}\n{body}"))
}

/// `gnc-budget-get-*` (gnc-budget.cpp:587-669, :720-752): amounts read 0 when unset or past the count, an explicit 0 is
/// set, period dates go on past the count, and actuals follow `ops::budgets::actual`: Expenses' January is its
/// children's $10.00 plus EUR 3.33 at $1.10 before the period's end, $3.66 (the $1.00 at 23:59:59 is in no period).
#[test]
fn the_query_api_reads_budgets() {
    let got = run(
        &Host::new(book(false)),
        r#"
        print(`budgets=${book.budgets().map(|b| b.name)} default=${book.default_budget().name} periods=${plan.numPeriods}`);
        print(`lookup=${book.budget(plan.guid) == plan} ${book.budget("00000000000000000000000000000000")}`);
        print(`p0=${iso.call(plan.period_start_date(0))}..${iso.call(plan.period_end_date(0))} ` +
              `p3=${iso.call(plan.period_start_date(3))}..${iso.call(plan.period_end_date(3))}`);
        let food = a.call("Expenses:Food");
        print(`values=${[0, 1, 2, 5].map(|p| plan.account_period_value(food, p))}`);
        print(`set=${[0, 1, 2, 5].map(|p| plan.is_account_period_value_set(food, p))}`);
        print(`notes=${plan.account_period_note(food, 0)} [${plan.account_period_note(food, 1)}]`);
        print(`actuals=${[0, 1, 2].map(|p| plan.account_period_actual_value("Expenses", p))} food=${plan.account_period_actual_value(food, 0)}`);
    "#,
    );
    check(
        &got,
        &[
            "budgets=[\"Plan\", \"Other\"] default=Plan periods=3",
            "lookup=true ",
            "p0=2024-01-01 00:00:00..2024-01-31 23:59:59 p3=2024-04-01 00:00:00..2024-04-30 23:59:59",
            "values=[rational(\"617/50\"), rational(\"0\"), rational(\"0\"), rational(\"0\")]",
            "set=[true, true, false, false]",
            "notes=January []",
            "actuals=[rational(\"683/50\"), rational(\"2\"), rational(\"0\")] food=10",
        ],
    );
    let other = run(&Host::new(book(true)), "print(`${book.default_budget().name}`);");
    check(&other, &["Other"]);
}

/// RU:744-794: own amounts over `[start, end)`, not rolled up; balances at the budget's start, children excluded.
/// RU:806-856: an unset parent sums its children, each converted at the nearest price to the period's start: in
/// January 12.34 + EUR 5 at 1.10 = 17.84, in February 0 + EUR 10 at 1.30 (15 February is nearer 1 February than 10
/// January) = 13, in March Rent's 700; 730.84 over the budget, inclusive ends.
#[test]
fn lib_budget_sums_and_rolls_up() {
    let got = run(
        &Host::new(book(false)),
        r#"
        let food = a.call("Expenses:Food");
        let rent = a.call("Expenses:Rent");
        let expenses = a.call("Expenses");
        print(`dates=${iso.call(budget::budget_get_start_date(plan))}..${iso.call(budget::budget_get_end_date(plan))}`);
        print(`net=${budget::budget_account_get_net(plan, food, (), ()).getmonetary(usd, false)} ` +
              `${budget::budget_account_get_net(plan, food, 1, ()).getmonetary(usd, false)} ` +
              `list=${budget::budget_accountlist_get_net(plan, [food, rent], (), ()).getmonetary(usd, false)} ` +
              `parent=${budget::budget_account_get_net(plan, expenses, (), ()).getmonetary(usd, false)}`);
        print(`initial=${budget::budget_account_get_initial_balance(plan, a.call("Bank")).getmonetary(usd, false)} ` +
              `${budget::budget_accountlist_get_initial_balance(plan, [a.call("Bank"), food]).getmonetary(usd, false)}`);
        print(`rolled=${[0, 1, 2, 3].map(|p| budget::get_account_period_rolledup_budget_value(book, plan, expenses, p))}`);
        print(`net=${budget::budget_account_get_rolledup_net(book, plan, expenses, (), ())} ` +
              `${budget::budget_account_get_rolledup_net(book, plan, expenses, 1, 1)} ` +
              `${budget::budget_account_get_rolledup_net(book, plan, a.call("Expenses:Spare"), (), ())}`);
    "#,
    );
    check(
        &got,
        &[
            "dates=2024-01-01 00:00:00..2024-03-31 23:59:59",
            "net=$12.34 $0.00 list=$712.34 parent=$0.00",
            "initial=$100.00 $100.00",
            "rolled=[rational(\"446/25\"), rational(\"13\"), rational(\"700\"), rational(\"0\")]",
            "net=18271/25 13 0",
        ],
    );
}

/// With "omit-leaf-acct", Rent and Spare balance to zero over the budget and get no row; with `report_budget`, Rent's
/// rolled-up Budget amounts (700) are not zero, so it keeps its row (HAT:749-756).
#[test]
fn the_account_table_keeps_budgeted_zero_accounts() {
    let got = run(
        &Host::new(book(false)),
        r#"
        import "lib/acct-table" as hat;
        import "lib/exchange" as exchange;
        let end = budget::budget_get_end_date(plan);
        let expenses = a.call("Expenses");
        let env = #{ display_tree_depth: 3, zero_balance_mode: "omit-leaf-acct", start_date: budget::budget_get_start_date(plan),
                     end_date: end, report_commodity: usd, exchange_fn: exchange::case_exchange_fn(book, "pricedb-nearest", usd, end) };
        let names = |e| hat::make_html_acct_table_env_accts(book, ctx, e, [expenses] + expenses.descendants_sorted()).rows.map(|r| r.account_name);
        print(`plain=${names.call(env)}`);
        env.report_budget = plan;
        print(`budget=${names.call(env)}`);
    "#,
    );
    check(
        &got,
        &[
            "plain=[\"Expenses\", \"Food\", \"Travel\"]",
            "budget=[\"Expenses\", \"Food\", \"Rent\", \"Travel\"]",
        ],
    );
}

/// The options of the budget reports (`standard/budget.scm:121-238`): a budget option defaulting to the default
/// budget, set by GUID and read as a `Budget`; a complex boolean and a multichoice with callbacks, which are plain
/// options whose callbacks are never run.
const OPTIONS_SCRIPT: &str = r#"
fn templates() { [#{ guid: "00000000000000000000000000000abc", name: "Test" }] }
fn options(params, o, book) {
    o.budget("General", "Budget", "a", "Budget to use.", book.default_budget());
    o.complex_boolean("General", "Report for range of budget periods", "f", "Ranges.", false, |v| throw "never run");
    o.multichoice_callback("General", "Range start", "g1.1", "Start.", "current",
                           [["first", "First budget period"], ["current", "Current budget period"]], |v| throw "never run");
}
fn render(params, book, opts, ctx) {
    let b = opts.get("General", "Budget");
    let name = if b == () { "none" } else { b.name };
    #{ title: `${name} ${opts.get("General", "Report for range of budget periods")} ${opts.get("General", "Range start")}`, sections: [] }
}
"#;

#[test]
fn budget_and_callback_options() {
    let reg = registry_with(Library::builtin(), &[("test.rhai", OPTIONS_SCRIPT)], Limits::default());
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    let host = Host::new(book(false));
    let template = reg.find(GUID).unwrap();
    let db = reg.options(template, &host.ctx()).unwrap();
    let types: Vec<(String, String)> = db.describe().into_iter().skip(3).map(|i| (i.name, i.option_type)).collect();
    let want = [("Budget", "budget"), ("Report for range of budget periods", "boolean"), ("Range start", "multichoice")];
    assert_eq!(types, want.map(|(n, t)| (n.to_owned(), t.to_owned())));
    assert_eq!(host.run(&reg, GUID, &json!({})).unwrap().title, "Plan false current");
    let other = book(false).budgets.keys().nth(1).copied().unwrap();
    let options = json!({ "General": { "Budget": other.to_string(), "Range start": "first" } });
    assert_eq!(host.run(&reg, GUID, &options).unwrap().title, "Other false first");
    let unknown = json!({ "General": { "Budget": "00000000000000000000000000000001" } });
    assert_eq!(host.run(&reg, GUID, &unknown).unwrap().title, "none false current", "a deleted budget reads as none");
    let empty = Host::new(common::empty_book());
    assert_eq!(empty.run(&reg, GUID, &json!({})).unwrap().title, "none false current");

    let bad = OPTIONS_SCRIPT.replace("false, |v| throw \"never run\"", "false, \"not a function\"");
    let reg = registry_with(Library::builtin(), &[("test.rhai", &bad)], Limits::default());
    let err = reg.options(reg.find(GUID).unwrap(), &host.ctx()).unwrap_err();
    assert!(format!("{err:?}").contains("callback is a function"), "{err:?}");
}
