//! The Cash Flow (`standard/cash-flow.rhai`, translated from `5.17:gnucash/report/reports/standard/cash-flow.scm`,
//! cited as CF) and its calculation (`lib/cash-flow-calc`, from `5.17:gnucash/report/reports/cash-flow-calc.scm`,
//! cited as CFC) through `report.run` and `report.export`: the figures of GnuCash's own test
//! (`standard/test/test-cash-flow.scm`) on `fixtures/reports/cash-flow`, as XML and as its SQLite copy, read both from
//! the calculation's result items (through a test template) and from the report; and hand-worked cases for the
//! options and branches that test does not reach, on `test-data`, `charts-daily` and books built through the API.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::{Value, json};
use statements::{Setup, blocks, cell_text, displays, expected, rates, tables, test_settings};

const CASH_FLOW: &str = "Cash Flow";
const CALC: &str = "Cash flow calculation";

/// The money table read back: the "Selected Accounts" items, each direction's rows as `name amount`, and the three
/// totals.
#[derive(Debug, PartialEq, Eq)]
struct Flow {
    selected: Option<Vec<String>>,
    money_in: Vec<String>,
    money_out: Vec<String>,
    totals: Vec<String>,
}

fn flow(result: &Value) -> Flow {
    let lists: Vec<&Value> = blocks(result).into_iter().filter(|b| b["kind"] == "list").collect();
    let selected = lists.first().map(|l| {
        l["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                format!(
                    "{}{}",
                    i["text"].as_str().unwrap(),
                    if i["account"].is_string() { "" } else { " (no link)" }
                )
            })
            .collect()
    });
    let (mut money_in, mut money_out, mut totals) = (vec![], vec![], vec![]);
    let mut out = false;
    tables(result)[0]["rows"].as_array().unwrap().iter().for_each(|r| {
        let cells = r["cells"].as_array().unwrap();
        let text = format!("{} {}", cell_text(&cells[0]), displays(&cells[1]).join(" "));
        match r["role"].as_str().unwrap() {
            "heading" => out = cell_text(&cells[0]).starts_with("Money out"),
            "account" if out => money_out.push(text),
            "account" => money_in.push(text),
            "grandTotal" => totals.push(text),
            _ => {}
        }
    });
    Flow {
        selected,
        money_in,
        money_out,
        totals,
    }
}

fn strings(want: &[&str]) -> Vec<String> {
    want.iter().map(|s| (*s).to_owned()).collect()
}

/// The three totals as the table shows them.
fn totals(money_in: &str, money_out: &str, difference: &str) -> Vec<String> {
    vec![
        format!("Money In {money_in}"),
        format!("Money Out {money_out}"),
        format!("Difference {difference}"),
    ]
}

/// A test template that runs the calculation as `test-cash-flow.scm` calls it (`:42-48`): the selected accounts,
/// from 2024-03-14 23:59:59 to 2024-03-15 23:59:59 UTC, USD, trading accounts counted and identity conversions
/// (`:26-28`). It prints each item of the result: `in <account> <amount>` for each entry of the money-in alist in
/// its order, its collector summed in USD; the same for `out`; then `in-collector <amount>` and
/// `out-collector <amount>`.
const CALC_SCRIPT: &str = r#"
import "lib/cash-flow-calc" as cfc;
import "lib/collectors" as collectors;
fn templates() { [#{ guid: "0000000000000000000000000000cf01", name: "Cash flow calculation" }] }
fn options(params, o, book) {
    o.account_list("A", "Accounts", "a", "", [], #{});
    o.currency("A", "Currency", "b", "", o.default_report_currency());
}
fn render(params, book, opts, ctx) {
    let usd = opts.get("A", "Currency");
    let r = cfc::cash_flow_calc_money_in_out(#{
        book: book, accounts: opts.get("A", "Accounts"), from_date: 1710460799, to_date: 1710547199,
        report_currency: usd, include_trading_accounts: false, time_exchange_fn: |foreign, domestic, date| foreign,
    });
    for p in r.money_in_alist { print(`in ${p[0].guid} ${collectors::sum_collector_commodity(p[1], usd, |f, d| f).amount}`); }
    for p in r.money_out_alist { print(`out ${p[0].guid} ${collectors::sum_collector_commodity(p[1], usd, |f, d| f).amount}`); }
    print(`in-collector ${collectors::sum_collector_commodity(r.money_in_collector, usd, |f, d| f).amount}`);
    print(`out-collector ${collectors::sum_collector_commodity(r.money_out_collector, usd, |f, d| f).amount}`);
    #{ title: "", sections: [] }
}
"#;

/// The GUID of an account `cash-flow.expected.json` names as `group.full name`.
fn guid(e: &Value, name: &str) -> String {
    let (group, full) = name.split_once('.').unwrap();
    e["accounts"][group][full].as_str().unwrap().to_owned()
}

/// The log lines of a run.
fn log(result: &Value) -> Vec<String> {
    result["log"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["text"].as_str().unwrap().to_owned())
        .collect()
}

/// One check of the calculation against the lines the test template printed.
fn check_calc(e: &Value, lines: &[String], c: &Value, what: &str) {
    let read = c["read"].as_str().unwrap();
    let dir = if read.starts_with("money-in") { "in" } else { "out" };
    let alist: Vec<(&str, &str)> = lines
        .iter()
        .filter_map(|l| l.strip_prefix(&format!("{dir} ")))
        .map(|l| l.split_once(' ').unwrap())
        .collect();
    if read.ends_with("collector") {
        let line = lines.iter().find_map(|l| l.strip_prefix(&format!("{dir}-collector "))).unwrap();
        assert_eq!(line, c["expected"].as_str().unwrap(), "{what}");
    } else if let Some(account) = c["account"].as_str() {
        let entry = alist.iter().find(|(g, _)| *g == guid(e, account));
        let (_, amount) = entry.unwrap_or_else(|| panic!("{what}: {account} is in the alist {lines:?}"));
        if let Some(want) = c["expected"].as_str() {
            assert_eq!(*amount, want, "{what}");
        }
    } else {
        let want: Vec<String> = c["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| guid(e, a.as_str().unwrap()))
            .collect();
        assert_eq!(alist.iter().map(|(g, _)| (*g).to_owned()).collect::<Vec<_>>(), want, "{what}");
    }
}

/// The Cash Flow a run's `report` names: its accounts on 2024-03-15, USD. The file's full names use `.`, the
/// engine's default separator in GnuCash's test environment (`5.17:libgnucash/engine/Account.cpp:58`); red-ale's
/// is `:`, GnuCash's application default, so the names are compared with `:`.
fn report_check(s: &mut Setup, e: &Value, run: &Value, what: &str) {
    let accounts: Vec<String> = run["report"]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| guid(e, a.as_str().unwrap()))
        .collect();
    let options = json!({
        "General": { "Start Date": { "absolute": "2024-03-15" }, "End Date": { "absolute": "2024-03-15" },
                     "Report's currency": "CURRENCY::USD" },
        "Accounts": { "Account": accounts },
    });
    let result = s.run(CASH_FLOW, &options);
    let html = s.export(CASH_FLOW, &options);
    let rows = |key: &str| -> Vec<String> {
        let row = |r: &Value| format!("{} {}", r[0].as_str().unwrap().replace('.', ":"), r[1].as_str().unwrap());
        run["report"][key].as_array().unwrap().iter().map(row).collect()
    };
    let r = &run["report"];
    let got = flow(&result);
    assert_eq!(got.money_in, rows("in"), "{what}");
    assert_eq!(got.money_out, rows("out"), "{what}");
    let want = totals(
        r["Money In"].as_str().unwrap(),
        r["Money Out"].as_str().unwrap(),
        r["Difference"].as_str().unwrap(),
    );
    assert_eq!(got.totals, want, "{what}");
    ["Money In", "Money Out", "Difference"]
        .iter()
        .for_each(|k| assert!(html.contains(r[*k].as_str().unwrap()), "{what}: {k}"));
}

/// test-cash-flow.scm:30-146: every check of `cash-flow.expected.json`, on the calculation's result items and on the
/// Cash Flow's figures, as XML and as SQLite.
#[test]
fn expected_figures_of_gnucash_test() {
    let e = expected("cash-flow.expected.json");
    let settings = test_settings("locale");
    let books = [
        ("xml", Setup::fixture("cash-flow.gnucash", &settings)),
        ("sqlite", Setup::sqlite_fixture("cash-flow.gnucash", &settings)),
    ];
    let mut checked = 0;
    for (format, mut s) in books {
        s.script("calc.rhai", CALC_SCRIPT);
        e["runs"].as_array().unwrap().iter().for_each(|run| {
            let accounts: Vec<String> = run["calc"]["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| guid(&e, a.as_str().unwrap()))
                .collect();
            let lines = log(&s.run(CALC, &json!({ "A": { "Accounts": accounts, "Currency": "CURRENCY::USD" } })));
            run["checks"].as_array().unwrap().iter().for_each(|c| {
                check_calc(
                    &e,
                    &lines,
                    c,
                    &format!("{format} {}: {} (test line {})", run["id"], c["name"], c["line"]),
                );
                checked += 1;
            });
            report_check(&mut s, &e, run, &format!("{format} {}", run["id"]));
        });
    }
    assert_eq!(checked, 2 * 15);
}

/// The options of CF:57-106 in GnuCash's order, with the trading option's help saying it works backwards.
#[test]
fn options_are_gnucash_s() {
    let mut s = Setup::with_config(&test_settings("iso"));
    let described = s.call("report.options", json!({ "report": CASH_FLOW }));
    let got: Vec<String> = described["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}/{} {} {}",
                o["section"].as_str().unwrap(),
                o["name"].as_str().unwrap(),
                o["key"],
                o["default"]
            )
        })
        .collect();
    let want = [
        r#"Accounts/Account Display Depth "aa" 2"#,
        r#"Accounts/Always show sub-accounts "ab" false"#,
        r#"Accounts/Account "ac" []"#,
        r#"Accounts/Include Trading Accounts in report "b" false"#,
        r#"General/Report name "0a" "Cash Flow""#,
        r#"General/Stylesheet "0b" "Default""#,
        r#"General/Add options summary "0c" "never""#,
        r#"General/Start Date "aa" {"relative":"start-accounting-period"}"#,
        r#"General/End Date "ab" {"relative":"end-accounting-period"}"#,
        r#"General/Report's currency "b" "CURRENCY::USD""#,
        r#"General/Price Source "c" "pricedb-nearest""#,
        r#"General/Show Exchange Rates "d" false"#,
        r#"General/Show Full Account Names "e" true"#,
        r#"General/Show Selected Accounts "f" true"#,
    ];
    assert_eq!(got, want);
    let trading = described["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "Include Trading Accounts in report")
        .unwrap();
    assert!(trading["help"].as_str().unwrap().contains("works backwards"));
}

/// Test-data's accounts (`fixtures/reports/test-data.expected.json`).
const BANK: &str = "a0000000000000000000000000000003";
const GBP_BANK: &str = "a0000000000000000000000000000004";
const WALLET: &str = "a0000000000000000000000000000005";
const ASSET: &str = "a0000000000000000000000000000002";
const EQUITY: &str = "a000000000000000000000000000000a";

/// The Cash Flow on test-data for `accounts` from `from` to `to`, with `more` options.
fn test_data(accounts: &[&str], from: &str, to: &str, more: Value) -> Value {
    let mut s = Setup::fixture("test-data.gnucash", &test_settings("iso"));
    cash_flow(&mut s, accounts, from, to, more)
}

fn cash_flow(s: &mut Setup, accounts: &[&str], from: &str, to: &str, more: Value) -> Value {
    let mut options = json!({
        "General": { "Start Date": { "absolute": from }, "End Date": { "absolute": to } },
        "Accounts": { "Account": accounts },
    });
    more.as_object().unwrap().iter().for_each(|(section, values)| {
        values
            .as_object()
            .unwrap()
            .iter()
            .for_each(|(name, v)| options[section][name] = v.clone());
    });
    s.run(CASH_FLOW, &options)
}

/// No accounts: the title, then only the warning (CF:186-192, :329-334). An empty Account option reads as its
/// default, the book's BANK, CASH, ASSET, STOCK and MUTUAL accounts (CF:91-95), so the book has none.
#[test]
fn no_accounts_give_the_warning() {
    let mut s = Setup::new_book(&test_settings("iso"));
    s.account("Income", "INCOME", None, "CURRENCY::USD");
    let result = cash_flow(&mut s, &[], "2000-01-01", "2000-12-31", json!({}));
    assert_eq!(result["title"], "Cash Flow - 2000-01-01 to 2000-12-31");
    let all = blocks(&result);
    assert_eq!(all.len(), 1, "{all:#?}");
    assert_eq!(all[0]["kind"], "warning");
    assert_eq!(all[0]["title"], "No accounts selected");
}

/// charts-daily's 2024 (scripts/report-fixtures/charts.py:56-66) for Bank and Liability: in from Equity $3 (01-01),
/// Expenses $11 (07-10) and Income $29 + $8 (02-11, 09-10; the voided $10 of 02-21 is not found, RU:709-710); out to
/// Expenses $8 (01-11) + $27 (02-22, from Liability) + $15 (03-01); the voided $10 of 05-10 left out. Bank, at depth
/// 3, is below the default depth 2, so only Liability is listed.
#[test]
fn a_year_of_charts_daily() {
    let mut s = Setup::fixture("charts-daily.gnucash", &test_settings("iso"));
    let bank_and_liability = ["a0000000000000000000000000000006", "a0000000000000000000000000000008"];
    let got = flow(&cash_flow(&mut s, &bank_and_liability, "2024-01-01", "2024-12-31", json!({})));
    let want = Flow {
        selected: Some(strings(&["Root:Liability"])),
        money_in: strings(&["Root:Equity $3.00", "Root:Expenses $11.00", "Root:Income $37.00"]),
        money_out: strings(&["Root:Expenses $50.00"]),
        totals: totals("$51.00", "$50.00", "$1.00"),
    };
    assert_eq!(got, want);
}

/// Test-data's multi-split of 1971-02-14 (`5.17:bindings/guile/test/test-engine-extras.scm:447-453`): Bank -100,
/// Expenses +80, Wallet +20. Bank alone sends money to both; with Wallet selected too, the $20 stays inside.
#[test]
fn a_multi_split_with_two_selected_splits() {
    let bank = flow(&test_data(&[BANK], "1971-01-01", "1971-12-31", json!({})));
    assert_eq!(bank.money_in, Vec::<String>::new());
    assert_eq!(bank.money_out, strings(&["Root:Asset:Wallet $20.00", "Root:Expenses $80.00"]));
    assert_eq!(bank.totals, totals("$0.00", "$100.00", "-$100.00"));
    let both = flow(&test_data(&[BANK, WALLET], "1971-01-01", "1971-12-31", json!({})));
    assert_eq!(both.money_out, strings(&["Root:Expenses $80.00"]));
    assert_eq!(both.totals, totals("$0.00", "$80.00", "-$80.00"));
}

/// GBP Bank over 2000 (test-engine-extras.scm:463-467): on 01-15 GBP 10 out to Bank (a GBP transaction, its Bank
/// split +£10 at USD 14, adding the price USD = £10/14); on 02-15 USD 9 in from Bank (a USD transaction, adding
/// GBP = $9/6). The £10 converts at the posted date: $14.00 by the January price under the database's nearest and
/// before, by January's cost under average cost and weighted average; $15.00 by the latest price, February's $1.50.
#[test]
fn every_price_source_at_the_posted_date() {
    [
        ("pricedb-nearest", "$14.00", "-$5.00"),
        ("pricedb-before", "$14.00", "-$5.00"),
        ("average-cost", "$14.00", "-$5.00"),
        ("weighted-average", "$14.00", "-$5.00"),
        ("pricedb-latest", "$15.00", "-$6.00"),
    ]
    .iter()
    .for_each(|(source, out, difference)| {
        let got = flow(&test_data(
            &[GBP_BANK],
            "2000-01-01",
            "2000-12-31",
            json!({ "General": { "Price Source": source } }),
        ));
        assert_eq!(got.money_in, strings(&["Root:Asset:Bank $9.00"]), "{source}");
        assert_eq!(got.money_out, strings(&[&format!("Root:Asset:Bank {out}")]), "{source}");
        assert_eq!(got.totals, totals("$9.00", out, difference), "{source}");
    });
}

/// With USD Bank alone selected, the weighted average prices only the selected accounts' commodities, none but USD:
/// January's GBP transaction converts to 0, and GBP Bank still gets a row (CU:865-874).
#[test]
fn weighted_average_with_an_unheld_currency() {
    let options = |source: &str| json!({ "General": { "Price Source": source } });
    let weighted = flow(&test_data(&[BANK], "2000-01-01", "2000-12-31", options("weighted-average")));
    assert_eq!(weighted.money_in, strings(&["Root:Asset:GBP Bank $0.00"]));
    assert_eq!(weighted.money_out, strings(&["Root:Asset:GBP Bank $9.00"]));
    let nearest = flow(&test_data(&[BANK], "2000-01-01", "2000-12-31", options("pricedb-nearest")));
    assert_eq!(nearest.money_in, strings(&["Root:Asset:GBP Bank $14.00"]));
}

/// GBP as the report's currency: the £10 stays £10; the $9 converts by February's price, £6.
#[test]
fn a_foreign_report_currency() {
    let got = flow(&test_data(
        &[GBP_BANK],
        "2000-01-01",
        "2000-12-31",
        json!({ "General": { "Report's currency": "CURRENCY::GBP" } }),
    ));
    assert_eq!(got.money_in, strings(&["Root:Asset:Bank #6.00"]));
    assert_eq!(got.money_out, strings(&["Root:Asset:Bank #10.00"]));
    assert_eq!(got.totals, totals("#6.00", "#10.00", "-#4.00"));
}

/// The rates table of "Show Exchange Rates" (CF:323-327): GBP at the end of 2000 by the nearest price, $1.50.
#[test]
fn exchange_rates() {
    let result = test_data(
        &[GBP_BANK],
        "2000-01-01",
        "2000-12-31",
        json!({ "General": { "Show Exchange Rates": true } }),
    );
    assert_eq!(rates(&result), strings(&["#1.00", "$1.50"]));
    let without = test_data(&[GBP_BANK], "2000-01-01", "2000-12-31", json!({}));
    assert!(rates(&without).is_empty());
}

/// The Selected Accounts list (CF:194-215) for Root.Asset (depth 2, three children), every depth both ways, and the
/// list off: then the table opens on its heading, with no rule above it.
#[test]
fn the_selected_accounts_list() {
    let list = |depth: Value, subaccounts: bool, full: bool| {
        let more = json!({
            "Accounts": { "Account Display Depth": depth, "Always show sub-accounts": subaccounts },
            "General": { "Show Full Account Names": full },
        });
        flow(&test_data(&[ASSET], "1971-01-01", "1971-12-31", more)).selected.unwrap()
    };
    let with_children = ["Root:Asset", "Root:Asset:Bank", "Root:Asset:GBP Bank", "Root:Asset:Wallet"];
    assert_eq!(list(json!(2), true, true), strings(&["Root:Asset and subaccounts"]));
    assert_eq!(list(json!(2), false, true), strings(&["Root:Asset and selected subaccounts"]));
    assert_eq!(list(json!(2), false, false), strings(&["Asset and selected subaccounts"]));
    assert_eq!(list(json!(1), true, true), Vec::<String>::new());
    assert_eq!(list(json!(3), true, true), strings(&with_children));
    assert_eq!(list(json!("all"), true, true), strings(&with_children));
    assert_eq!(list(json!("all"), false, true), strings(&["Root:Asset"]));

    let off = test_data(
        &[ASSET],
        "1971-01-01",
        "1971-12-31",
        json!({ "General": { "Show Selected Accounts": false } }),
    );
    assert_eq!(flow(&off).selected, None);
    assert_eq!(blocks(&off).len(), 1, "only the table");
    assert_eq!(tables(&off)[0]["rows"][0]["role"], "heading");
    let on = test_data(&[ASSET], "1971-01-01", "1971-12-31", json!({}));
    assert_eq!(blocks(&on)[0]["text"], "Selected Accounts");
    assert_eq!(tables(&on)[0]["rows"][0]["role"], "rule");
}

/// "Always show sub-accounts" adds every descendant to the selection, so the 1971 multi-split's Bank and Wallet
/// splits are both inside, and full names off shows names alone.
#[test]
fn sub_accounts_join_the_selection() {
    let without = flow(&test_data(&[ASSET], "1971-01-01", "1971-12-31", json!({})));
    assert_eq!(without.totals, totals("$0.00", "$0.00", "$0.00"), "Root:Asset itself has no split");
    let more = json!({ "Accounts": { "Always show sub-accounts": true }, "General": { "Show Full Account Names": false } });
    let with = flow(&test_data(&[ASSET], "1971-01-01", "1971-12-31", more));
    assert_eq!(with.money_out, strings(&["Expenses $80.00"]));
}

/// The closing transaction of 1977-12-31 (test-engine-extras.scm:455-458) counts: nothing filters closing entries
/// (RU:692-725).
#[test]
fn a_closing_transaction_counts() {
    let got = flow(&test_data(&[EQUITY], "1977-01-01", "1977-12-31", json!({})));
    assert_eq!(got.money_in, strings(&["Root:Expenses $111.00"]));
    assert_eq!(got.totals, totals("$111.00", "$0.00", "$111.00"));
}

/// A book of Bank, Expenses, Income and Equity in USD.
fn small_book() -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    [
        ("Bank", "BANK"),
        ("Expenses", "EXPENSE"),
        ("Income", "INCOME"),
        ("Equity", "EQUITY"),
    ]
    .iter()
    .for_each(|(name, kind)| s.account(name, kind, None, "CURRENCY::USD"));
    s
}

fn guid_of(s: &mut Setup, name: &str) -> String {
    let accounts = s.call("account.list", json!({}));
    let account = accounts["accounts"].as_array().unwrap().iter().find(|a| a["name"] == name).unwrap();
    account["guid"].as_str().unwrap().to_owned()
}

/// A zero split gives no row (CFC:74-95), a voided transaction is not found, and one account can be both in and
/// out. Rows sort by full name, whatever order the alist has.
#[test]
fn zero_and_void_splits() {
    let mut s = small_book();
    let splits = json!([
        { "account": "Bank", "value": "10" }, { "account": "Income", "value": "-10" }, { "account": "Equity", "value": "0" },
    ]);
    s.call(
        "transaction.post",
        json!({ "datePosted": "2024-03-01", "currency": "CURRENCY::USD", "splits": splits }),
    );
    let splits = json!([{ "account": "Bank", "value": "7" }, { "account": "Income", "value": "-7" }]);
    let voided = s.call(
        "transaction.post",
        json!({ "datePosted": "2024-03-02", "currency": "CURRENCY::USD", "splits": splits }),
    );
    s.call("transaction.void", json!({ "guid": voided["guid"], "reason": "test" }));
    s.transfer("2024-03-03", "", "Bank", "Expenses", "4");
    s.transfer("2024-03-04", "", "Equity", "Bank", "2");
    let bank = guid_of(&mut s, "Bank");
    let got = flow(&cash_flow(&mut s, &[&bank], "2024-03-01", "2024-03-31", json!({})));
    assert_eq!(got.money_in, strings(&["Equity $2.00", "Income $10.00"]));
    assert_eq!(got.money_out, strings(&["Expenses $4.00"]));
    assert_eq!(got.totals, totals("$12.00", "$4.00", "$8.00"));
}

/// "Include Trading Accounts in report" works backwards (CFC:68-70): off, the default, counts the trading splits of
/// a USD transfer to a EUR account; on, leaves them out.
#[test]
fn the_trading_option_works_backwards() {
    let mut s = small_book();
    s.account("Euro", "BANK", None, "CURRENCY::EUR");
    s.account("Trading", "TRADING", None, "CURRENCY::USD");
    s.account("EUR", "TRADING", Some("Trading"), "CURRENCY::EUR");
    s.account("USD", "TRADING", Some("Trading"), "CURRENCY::USD");
    let splits = json!([
        { "account": "Bank", "value": "-10" },
        { "account": "Euro", "value": "10", "amount": "9" },
        { "account": "Trading:EUR", "value": "-10", "amount": "-9" },
        { "account": "Trading:USD", "value": "10" },
    ]);
    s.call(
        "transaction.post",
        json!({ "datePosted": "2024-03-01", "currency": "CURRENCY::USD", "splits": splits }),
    );
    let bank = guid_of(&mut s, "Bank");
    let off = flow(&cash_flow(&mut s, &[&bank], "2024-03-01", "2024-03-31", json!({})));
    assert_eq!(off.money_in, strings(&["Trading:EUR $10.00"]));
    assert_eq!(off.money_out, strings(&["Euro $10.00", "Trading:USD $10.00"]));
    assert_eq!(off.totals, totals("$10.00", "$20.00", "-$10.00"));
    let on_option = json!({ "Accounts": { "Include Trading Accounts in report": true } });
    let on = flow(&cash_flow(&mut s, &[&bank], "2024-03-01", "2024-03-31", on_option));
    assert_eq!(on.money_in, Vec::<String>::new());
    assert_eq!(on.money_out, strings(&["Euro $10.00"]));
}

/// A run leaves the Book, its revision, its dirty flag, the file and its directory as they were, on XML and SQLite;
/// two exports are the same bytes, with no script.
#[test]
fn a_run_changes_nothing() {
    let settings = test_settings("iso");
    let books = [
        ("xml", Setup::fixture("cash-flow.gnucash", &settings)),
        ("sqlite", Setup::sqlite_fixture("cash-flow.gnucash", &settings)),
    ];
    for (format, mut s) in books {
        let bytes = std::fs::read(s.book()).unwrap();
        let before = s.d.context().book().unwrap().clone();
        let revision = s.d.context().session().unwrap().revision();
        let files = s.files();
        let options = json!({
            "General": {
                "Start Date": { "absolute": "2024-03-15" }, "End Date": { "absolute": "2024-03-15" }, "Show Exchange Rates": true,
            },
            "Accounts": { "Account": ["a000000000000000000000000000000d"], "Always show sub-accounts": true },
        });
        s.run(CASH_FLOW, &options);
        let (a, b) = (s.export(CASH_FLOW, &options), s.export(CASH_FLOW, &options));
        assert_eq!(a, b, "{format}");
        assert!(!a.to_ascii_lowercase().contains("<script"), "{format}");
        assert_eq!(s.d.context().book().unwrap(), &before, "{format}");
        assert_eq!(s.d.context().session().unwrap().revision(), revision, "{format}");
        assert_eq!(s.call("book.info", json!({}))["dirty"], false, "{format}");
        assert_eq!(s.files(), files, "{format}: no journal or other file was written");
        assert_eq!(std::fs::read(s.book()).unwrap(), bytes, "{format}: the file is unchanged");
    }
}
