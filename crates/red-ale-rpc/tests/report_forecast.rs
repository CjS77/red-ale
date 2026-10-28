//! The two reports that read the scheduled transactions' cash flow, through `report.run` and `report.export`: the
//! Future Scheduled Transactions Summary (`standard/account-summary.rhai` with `sx`, from
//! `5.17:gnucash/report/reports/standard/account-summary.scm`, cited as AS) and the Balance Forecast (below).
//!
//! GnuCash has no test for either with scheduled transactions, and computing GnuCash's figures would need Guile, which
//! red-ale never runs, so every expected figure here is worked by hand from the 5.17 source:
//! `gnc_sx_get_num_occur_daterange` (`5.17:libgnucash/engine/SchedXaction.cpp:739-812`, cited as SX) counts each
//! scheduled transaction's occurrences in a range, both ends inclusive, from the one after its last occurrence;
//! `gnc_sx_all_instantiate_cashflow_all` (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1734-1976`, cited as SIM)
//! adds count × (debit − credit) to each template split's account, a numeric of 0 falling back to the formula with
//! every variable 0 (SIM:1087-1148), disabled ones adding nothing (SIM:1902-1907).
//!
//! The book is built through the API on the pinned now, Friday 2024-03-15, all in USD:
//!
//! - Assets: Checking (BANK) and Cash (CASH); Income: Salary; Expenses: Rent, Gym, Utilities; Equity: Opening.
//! - On 2024-01-02, Checking +$1,000 and Cash +$50 from Opening; on 2024-04-15, Checking −$100 to Utilities.
//! - **Rent**: monthly from 2024-01-01, $2,000 from Checking. Since Last Run on the pinned now ignores the 1 January
//!   and 1 February instances and postpones 1 March's, so its last occurrence is 2024-03-01 and it holds a Deferred
//!   instance, which no cash flow counts (SX:739-812 reads only the schedule's own state).
//! - **Salary**: every two weeks from Friday 2024-03-22, $900 to Checking: 22 March, 5 and 19 April, 3, 17 and
//!   31 May, 14 and 28 June. Never run.
//! - **Gym**: monthly from 2024-04-10, ending 2024-05-05, $20 from Cash: only 10 April, as 10 May is past the end
//!   (SX:793-794).
//! - **Utilities**: monthly from 2024-04-20, `power * 2 + 30` from Checking. The formula names a variable, so its
//!   numeric is 0 and the cash flow reads it with `power` at 0: $30.
//! - **Holiday**: monthly from 2024-04-01, $999 from Cash to Gym, disabled.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod report_runs;

use report_runs::{Fixture, only};
use serde_json::{Value, json};

const FSTS: &str = "Future Scheduled Transactions Summary";

fn account(f: &mut Fixture, name: &str, kind: &str, parent: Option<&str>) {
    let mut params = json!({ "name": name, "type": kind, "commodity": "USD" });
    if let Some(parent) = parent {
        params["parent"] = json!(parent);
    }
    f.call("account.create", &params);
}

fn transfer(f: &mut Fixture, date: &str, from: &str, to: &str, value: &str) {
    let splits = json!([{ "account": from, "value": format!("-{value}") }, { "account": to, "value": value }]);
    f.call(
        "transaction.post",
        &json!({ "datePosted": date, "description": "real", "splits": splits }),
    );
}

/// A scheduled transaction moving `formula` from `from` to `to` on `schedule`, with `extra` fields.
fn scheduled(f: &mut Fixture, name: &str, schedule: Value, from: &str, to: &str, formula: &str, extra: Value) {
    let mut params = json!({
        "name": name,
        "startDate": schedule["start"],
        "schedule": [schedule],
        "templateTransactions": [{ "description": name, "splits": [
            { "account": to, "debitFormula": formula },
            { "account": from, "creditFormula": formula },
        ] }],
    });
    params.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    f.call("scheduledTransaction.create", &params);
}

fn every(period: &str, mult: u32, start: &str) -> Value {
    json!({ "mult": mult, "periodType": period, "start": start })
}

/// The book of the module documentation.
fn book() -> Fixture {
    let mut f = Fixture::create("locale");
    account(&mut f, "Assets", "ASSET", None);
    account(&mut f, "Checking", "BANK", Some("Assets"));
    account(&mut f, "Cash", "CASH", Some("Assets"));
    account(&mut f, "Income", "INCOME", None);
    account(&mut f, "Salary", "INCOME", Some("Income"));
    account(&mut f, "Expenses", "EXPENSE", None);
    ["Rent", "Gym", "Utilities"]
        .iter()
        .for_each(|n| account(&mut f, n, "EXPENSE", Some("Expenses")));
    account(&mut f, "Equity", "EQUITY", None);
    account(&mut f, "Opening", "EQUITY", Some("Equity"));
    transfer(&mut f, "2024-01-02", "Equity:Opening", "Assets:Checking", "1000");
    transfer(&mut f, "2024-01-02", "Equity:Opening", "Assets:Cash", "50");
    transfer(&mut f, "2024-04-15", "Assets:Checking", "Expenses:Utilities", "100");
    scheduled(
        &mut f,
        "Rent",
        every("month", 1, "2024-01-01"),
        "Assets:Checking",
        "Expenses:Rent",
        "2000",
        json!({}),
    );
    scheduled(
        &mut f,
        "Salary",
        every("week", 2, "2024-03-22"),
        "Income:Salary",
        "Assets:Checking",
        "900",
        json!({}),
    );
    let gym_end = json!({ "end": { "date": "2024-05-05" } });
    scheduled(
        &mut f,
        "Gym",
        every("month", 1, "2024-04-10"),
        "Assets:Cash",
        "Expenses:Gym",
        "20",
        gym_end,
    );
    let utilities = every("month", 1, "2024-04-20");
    scheduled(
        &mut f,
        "Utilities",
        utilities,
        "Assets:Checking",
        "Expenses:Utilities",
        "power * 2 + 30",
        json!({}),
    );
    let holiday = every("month", 1, "2024-04-01");
    scheduled(
        &mut f,
        "Holiday",
        holiday,
        "Assets:Cash",
        "Expenses:Gym",
        "999",
        json!({ "enabled": false }),
    );
    run_rent_since_last_run(&mut f);
    f
}

/// Since Last Run on the pinned now proposes only Rent's three instances; ignore two and postpone the third.
fn run_rent_since_last_run(f: &mut Fixture) {
    let p = f.call("scheduledTransaction.sinceLastRun", &json!({}));
    let schedules = p["schedules"].as_array().unwrap();
    let dates: Vec<&Value> = schedules
        .iter()
        .flat_map(|s| s["instances"].as_array().unwrap())
        .map(|i| &i["date"])
        .collect();
    assert_eq!(dates, [&json!("2024-01-01"), &json!("2024-02-01"), &json!("2024-03-01")]);
    let choices = json!([{ "index": 0, "state": "ignored" }, { "index": 1, "state": "ignored" }, { "index": 2, "state": "postponed" }]);
    let applied = f.call("scheduledTransaction.apply", &json!({ "proposal": p["id"], "choices": choices }));
    let rent = &applied["schedules"][0];
    assert_eq!(
        json!([rent["lastOccurrence"], rent["deferredInstances"][0]["lastOccurrence"]]),
        json!(["2024-03-01", "2024-02-01"])
    );
}

/// The FSTS options for `from` to `to`, with `display` options.
fn range(from: &str, to: &str, display: Value) -> Value {
    json!({ "General": { "Start Date": { "absolute": from }, "End Date": { "absolute": to } }, "Display": display })
}

/// Each account row of the summary's table as `name: balance`, the balance as shown.
fn summary(result: &Value) -> Vec<String> {
    let table = only(result, "table");
    let columns = table["columns"].as_array().unwrap();
    let label = columns.iter().position(|c| c["role"] == "label").unwrap();
    let amount = columns.iter().position(|c| c["role"] == "amount").unwrap();
    table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let cells = &r["cells"];
            let shown = cells[amount]["display"]
                .as_str()
                .map_or_else(|| cells[amount].to_string(), str::to_owned);
            format!("{}: {shown}", cells[label]["text"].as_str().unwrap_or_default())
        })
        .collect()
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

/// The rows whose balance is not $0.00.
fn moved(result: &Value) -> Vec<String> {
    summary(result).into_iter().filter(|r| !r.ends_with(": $0.00")).collect()
}

/// April and May (AS:250-257, :310-311): Rent 1 April and 1 May, 2 × $2,000, its last occurrence (1 March) not counted
/// again (SX:756-760, :805-808); Salary 5 and 19 April, 3, 17 and 31 May, 5 × $900, the end date counted (SX:792);
/// Gym 10 April only; Utilities 20 April and 20 May at $30, `power` read as 0; Holiday nothing. Checking is
/// −$4,000 + $4,500 − $60 = $440. Parents show their own balance by default, and no template split names them, so
/// they get an empty collector (AS:332-336): $0.00, as does Opening. The title names the range (AS:290-295).
#[test]
fn april_and_may() {
    let mut f = book();
    let options = range("2024-04-01", "2024-05-31", json!({}));
    let result = f.run(FSTS, &options);
    assert_eq!(
        result["title"],
        " Future Scheduled Transactions Summary For Period Covering 04/01/24 to 05/31/24"
    );
    let expected = [
        "Assets: $0.00",
        "Cash: -$20.00",
        "Checking: $440.00",
        "Income: $0.00",
        "Salary: -$4,500.00",
        "Expenses: $0.00",
        "Gym: $20.00",
        "Rent: $4,000.00",
        "Utilities: $60.00",
        "Equity: $0.00",
        "Opening: $0.00",
    ];
    assert_eq!(summary(&result), strings(&expected));
    let html = f.export(FSTS, &options);
    assert!(
        html.contains("$4,000.00") && html.contains("-$4,500.00"),
        "the page shows the figures"
    );
}

/// With recursive parent balances the account table adds the children's cash flows to each parent (HAT:661-669):
/// Assets −$20 + $440 = $420, Expenses $20 + $4,000 + $60 = $4,080.
#[test]
fn parents_add_their_children() {
    let mut f = book();
    let result = f.run(
        FSTS,
        &range("2024-04-01", "2024-05-31", json!({ "Parent account balances": "recursive-bal" })),
    );
    let parents = ["Assets", "Income", "Expenses", "Equity"];
    let rows: Vec<String> = summary(&result)
        .into_iter()
        .filter(|r| parents.iter().any(|p| r.starts_with(&format!("{p}:"))))
        .collect();
    assert_eq!(
        rows,
        strings(&["Assets: $420.00", "Income: -$4,500.00", "Expenses: $4,080.00", "Equity: $0.00"])
    );
}

/// Both ends of the range count (SX:777, :792): 1 April holds Rent's occurrence, 31 May Salary's.
#[test]
fn both_range_ends_count() {
    let mut f = book();
    let first = f.run(FSTS, &range("2024-04-01", "2024-04-01", json!({})));
    assert_eq!(moved(&first), strings(&["Checking: -$2,000.00", "Rent: $2,000.00"]));
    let last = f.run(FSTS, &range("2024-05-31", "2024-05-31", json!({})));
    assert_eq!(moved(&last), strings(&["Checking: $900.00", "Salary: -$900.00"]));
}

/// March: Rent's last occurrence is 1 March and its Deferred instance was saved there; neither counts (SX:756-760,
/// :805-808), so Rent adds nothing and only Salary's 22 March occurrence is left.
#[test]
fn the_deferred_instance_and_the_last_occurrence_are_left_out() {
    let mut f = book();
    let march = f.run(FSTS, &range("2024-03-01", "2024-03-31", json!({})));
    assert_eq!(moved(&march), strings(&["Checking: $900.00", "Salary: -$900.00"]));
}

/// `test-account-summary.scm:67-80` (`fixtures/reports/test-data.expected.json`, run "fsts"): on `create-test-data`,
/// which has no scheduled transactions, the summary lists every account as the Account Summary does and every
/// balance is $0.00: the three GnuCash reads at column 3 are Bank's, GBP Bank's (the report currency only, as the
/// collector is empty) and Wallet's. The two date options replace "Date" (AS:146-151) and default to the
/// accounting period, today to today here.
#[test]
fn test_account_summary_fsts() {
    let mut f = Fixture::open("test-data", "locale", true);
    let result = f.run(FSTS, &json!({}));
    assert_eq!(
        result["title"],
        " Future Scheduled Transactions Summary For Period Covering 03/15/24 to 03/15/24"
    );
    let names = [
        "Root",
        "Asset",
        "Bank",
        "GBP Bank",
        "Wallet",
        "Liabilities",
        "Income",
        "Income-GBP",
        "Expenses",
        "Equity",
    ];
    assert_eq!(summary(&result), names.map(|n| format!("{n}: $0.00")), "fsts col 2 and col 3");
    let options = report_runs::described_options(FSTS);
    let dates: Vec<&String> = options
        .iter()
        .filter(|o| o.starts_with("General/") && o.contains(" Date "))
        .collect();
    assert_eq!(dates.len(), 2, "{options:#?}");
    assert!(
        dates[0].starts_with("General/Start Date \"ca\"") && dates[1].starts_with("General/End Date \"cb\""),
        "{dates:?}"
    );
}

// The Balance Forecast (`standard/balance-forecast.rhai`, from `5.17:gnucash/report/reports/standard/balance-forecast.scm`,
// cited as BF). Its accounts default to the BANK and CASH ones, Cash and Checking, whose real balances are $50 and
// $1,000 until 15 April and $50 and $900 after (`gnc:account-get-balances-at-dates`, BF:171-175). Every point adds
// what the scheduled transactions move from the earliest split, 2 January, to the interval's end (BF:194-232).

const FORECAST: &str = "Balance Forecast";

fn forecast(from: &str, to: &str, interval: &str, display: Value) -> Value {
    let general = json!({ "Start Date": { "absolute": from }, "End Date": { "absolute": to }, "Interval": interval });
    json!({ "General": general, "Display": display })
}

/// The chart's labels and each series as `label: values`.
fn lines(result: &Value) -> (Vec<String>, Vec<String>) {
    let chart = only(result, "chart");
    let labels = chart["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_owned())
        .collect();
    let fraction = |v: &Value| format!("{}/{}", v["num"].as_str().unwrap(), v["denom"].as_str().unwrap());
    let series = chart["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let values: Vec<String> = s["values"].as_array().unwrap().iter().map(fraction).collect();
            format!("{}: {}", s["label"].as_str().unwrap(), values.join(" "))
        })
        .collect();
    (labels, series)
}

/// April to June by month (BF:163-164; DU:323-349 steps months from the start date, each interval ending a second
/// before the next). The opening sum, 2 January to 1 April, holds Rent's 1 April occurrence (its last, 1 March, is
/// not counted again, SX:805-808) and Salary's 22 March: Checking −$2,000 + $900. Then:
///
/// - April: Rent 1 April again (both cash-flow ranges include the start day), Salary 5 and 19 April, Utilities
///   $30 (`power` as 0), Gym $20 from Cash. Checking −$1,100 − $2,000 + $1,800 − $30 = −$1,330, Cash −$20; with the
///   real $900 and $50, −$400.
/// - May: Rent, Salary 3, 17 and 31 May, Utilities; Gym has ended. Checking −$1,330 + $670 = −$660: $270.
/// - June: Rent, Salary 14 and 28 June, Utilities: −$660 − $230 = −$890: $40.
///
/// The future minimum is each point's least of itself and the later ones (BF:239-241): −$400, $40, $40. The
/// Target is the reserve plus the target, $350, and the Reserve $100 (BF:254-271); the series come in that order
/// (BF:235-271), points labelled with the intervals' ends in ISO form (BF:284-287).
#[test]
fn april_to_june_by_month() {
    let mut f = book();
    let display = json!({ "Show future minimum": true, "Show reserve line": true, "Reserve amount": "100",
                          "Show target line": true, "Target amount above reserve": "250", "Data markers?": true });
    let options = forecast("2024-04-01", "2024-06-30", "MonthDelta", display);
    let result = f.run(FORECAST, &options);
    let (labels, series) = lines(&result);
    assert_eq!(labels, strings(&["2024-04-30", "2024-05-31", "2024-06-30"]));
    let expected = [
        "Minimum: -400/1 40/1 40/1",
        "Balance: -400/1 270/1 40/1",
        "Target: 350/1 350/1 350/1",
        "Reserve: 100/1 100/1 100/1",
    ];
    assert_eq!(series, strings(&expected));
    let chart = only(&result, "chart");
    assert_eq!(chart["type"], "line");
    assert_eq!(chart["title"], json!(["Balance Forecast", "04/01/24 to 06/30/24"]));
    assert_eq!(
        chart["yAxis"],
        json!({ "label": "USD", "format": "currency", "commodity": "CURRENCY::USD" })
    );
    let colors: Vec<&Value> = chart["series"].as_array().unwrap().iter().map(|s| &s["color"]).collect();
    assert_eq!(colors, [&json!("#00aaaa"), &json!("#00aa00"), &json!("#ffff00"), &json!("#ff0000")]);
    let balance = &chart["series"][1];
    let look = json!([balance["fill"], balance["lineWidth"], balance["pointRadius"]]);
    assert_eq!(look, json!([null, { "num": "3", "denom": "2" }, { "num": "3", "denom": "1" }]));
    assert_eq!(balance["displays"], json!(["-$400.00", "$270.00", "$40.00"]));
    let html = f.export(FORECAST, &options);
    assert!(html.contains("<svg ") && !html.contains("<g opacity=\"0\">"), "markers are drawn");
}

/// The defaults show the Balance line alone, without markers (BF:101-123): its points have radius 0.
#[test]
fn defaults_show_the_balance_alone() {
    let mut f = book();
    let options = forecast("2024-04-01", "2024-06-30", "MonthDelta", json!({}));
    let result = f.run(FORECAST, &options);
    assert_eq!(lines(&result).1, strings(&["Balance: -400/1 270/1 40/1"]));
    assert_eq!(
        only(&result, "chart")["series"][0]["pointRadius"],
        json!({ "num": "0", "denom": "1" })
    );
    let html = f.export(FORECAST, &options);
    assert!(html.contains("<g opacity=\"0\">"), "markers are hidden but keep their hover labels");
}

/// Weeks from Monday 1 April to Saturday 20 April: 1-7, 8-14 and a shorter last interval, 15-20 (DU:331-349). The
/// first adds Rent's 1 April and Salary's 5 April to the opening −$1,100: $1,050 − $2,200 = −$1,150; the second Gym's
/// 10 April from Cash: −$1,170; the third Salary's 19 April and Utilities' 20 April, with the real Checking down to
/// $900 on 15 April: $950 − $1,330 − $20 = −$400, as at the end of April by month.
#[test]
fn weeks_end_with_a_shorter_interval() {
    let mut f = book();
    let (labels, series) = lines(&f.run(FORECAST, &forecast("2024-04-01", "2024-04-20", "WeekDelta", json!({}))));
    assert_eq!(labels, strings(&["2024-04-07", "2024-04-14", "2024-04-20"]));
    assert_eq!(series, strings(&["Balance: -1150/1 -1170/1 -400/1"]));
}

/// An occurrence on the start date is counted twice, by the opening sum and by the first interval, as both of
/// GnuCash's cash-flow ranges include it (BF:202, :217-218; `5.17:bindings/guile/expressions.i:27-32`). From
/// 2 April, Rent's 1 April occurrence is in the opening sum only: the end of April shows $2,000 more than from
/// 1 April, $1,600.
#[test]
fn an_occurrence_on_the_start_date_counts_twice() {
    let mut f = book();
    let (_, series) = lines(&f.run(FORECAST, &forecast("2024-04-02", "2024-04-30", "MonthDelta", json!({}))));
    assert_eq!(series, strings(&["Balance: 1600/1"]));
}

/// A euro account's points are exchanged into the report currency at each interval's end (BF:166-169, :227-231),
/// here at the one price, $1.10: €100 since 2 January and €10 on the 15th of each month from April by schedule give
/// €110, $121.00, at the end of April and €120, $132.00, at the end of May.
#[test]
fn foreign_accounts_are_exchanged_at_each_interval_end() {
    let mut f = book();
    let euro =
        |f: &mut Fixture, name: &str, kind: &str| f.call("account.create", &json!({ "name": name, "type": kind, "commodity": "EUR" }));
    euro(&mut f, "Euro Cash", "CASH");
    euro(&mut f, "Euro Opening", "EQUITY");
    transfer(&mut f, "2024-01-02", "Euro Opening", "Euro Cash", "100");
    scheduled(
        &mut f,
        "Top-up",
        every("month", 1, "2024-04-15"),
        "Euro Opening",
        "Euro Cash",
        "10",
        json!({}),
    );
    let price = json!({ "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "value": "1.10", "date": "2024-04-01" });
    f.call("price.add", &price);
    let mut options = forecast("2024-04-01", "2024-05-31", "MonthDelta", json!({}));
    options["Accounts"] = json!({ "Accounts": ["Euro Cash"] });
    assert_eq!(lines(&f.run(FORECAST, &options)).1, strings(&["Balance: 121/1 132/1"]));
}

/// The warnings (BF:177-188): no accounts, here a new book with no BANK or CASH account to default to; and accounts
/// whose real balances are all zero, whatever the scheduled transactions add to them: Salary never has a split,
/// though Salary's schedule moves $900 out of it every two weeks.
#[test]
fn warnings_for_no_accounts_and_no_real_balances() {
    let mut empty = Fixture::create("locale");
    let options = forecast("2024-04-01", "2024-06-30", "MonthDelta", json!({}));
    assert_eq!(only(&empty.run(FORECAST, &options), "warning")["title"], "No accounts selected");
    let mut f = book();
    let mut salary = options.clone();
    salary["Accounts"] = json!({ "Accounts": ["Income:Salary"] });
    let result = f.run(FORECAST, &salary);
    assert_eq!(only(&result, "warning")["title"], "No data");
    assert!(report_runs::blocks_of(&result, "chart").is_empty());
}

/// Every option with GnuCash's section, name, key and default (BF:73-126), after the three General options every
/// report has.
#[test]
fn options_keep_gnucash_keys_and_defaults() {
    let expected = [
        r#"Accounts/Accounts "a" "accountList" []"#,
        r#"Display/Plot Width "aa" "plotSize" {"percent":100}"#,
        r#"Display/Plot Height "ab" "plotSize" {"percent":100}"#,
        r#"Display/Data markers? "b" "boolean" false"#,
        r#"Display/Show reserve line "c" "boolean" false"#,
        r#"Display/Reserve amount "d" "numberRange" 0"#,
        r#"Display/Show target line "e" "boolean" false"#,
        r#"Display/Target amount above reserve "f" "numberRange" 0"#,
        r#"Display/Show future minimum "g" "boolean" false"#,
        r#"General/Report name "0a" "string" "Balance Forecast""#,
        r#"General/Stylesheet "0b" "multichoice" "Default""#,
        r#"General/Add options summary "0c" "multichoice" "never""#,
        r#"General/Start Date "aa" "date" {"relative":"start-accounting-period"}"#,
        r#"General/End Date "ab" "date" {"relative":"end-accounting-period"}"#,
        r#"General/Interval "b" "multichoice" "DayDelta""#,
        r#"General/Report's currency "c" "currency" "CURRENCY::USD""#,
        r#"General/Price Source "d" "multichoice" "pricedb-nearest""#,
    ];
    assert_eq!(report_runs::described_options(FORECAST), strings(&expected));
}
