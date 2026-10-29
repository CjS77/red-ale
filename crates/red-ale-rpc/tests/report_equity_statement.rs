//! The Equity Statement (`standard/equity-statement.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/equity-statement.scm`, cited as ES) through `report.run` and `report.export`:
//! the figures of GnuCash's own test (`standard/test/test-equity-statement.scm`) on
//! `fixtures/reports/adjusting-entries.gnucash` and its SQLite copy, and hand-worked cases on books built through the
//! API for the options and branches that test does not reach.
//!
//! The hand-worked "drawings" book: Bank, Capital (EQUITY), Income and Expenses, in USD. The owner puts $1,000 in on
//! 2024-01-01, sales bring $300 on 2024-02-10, rent costs $500 on 2024-03-05, and the owner draws $200 on
//! 2024-04-01, so Bank ends 2024 at $600.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::{Value, json};
use statements::{Setup, blocks, displays, expected, expected_run, gnucash_text_count, options_param, row_texts, tables, test_settings};

const EQUITY: &str = "Equity Statement";
const FIXTURE: &str = "adjusting-entries.gnucash";

/// GnuCash's test period (test-equity-statement.scm:77-80).
fn test_period() -> Value {
    json!({ "Start Date": { "absolute": "1970-01-01" }, "End Date": { "absolute": "2005-01-01" } })
}

/// The fixture and its SQLite copy, each opened read-only, with the ISO dates of test-equity-statement.scm:63.
fn both_formats() -> [(&'static str, Setup); 2] {
    [
        ("xml", Setup::fixture(FIXTURE, &test_settings("iso"))),
        ("sqlite", Setup::sqlite_fixture(FIXTURE, &test_settings("iso"))),
    ]
}

/// The statement's rows, rule rows left out, each as GnuCash's test reads a row: the label, then the amount.
fn statement(result: &Value) -> Vec<Vec<String>> {
    let table = tables(result)[0];
    table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["role"] != "rule")
        .map(|r| row_texts(table, r))
        .collect()
}

fn rows(want: &[[&str; 2]]) -> Vec<Vec<String>> {
    want.iter().map(|r| r.iter().map(|s| (*s).to_owned()).collect()).collect()
}

/// One check of test-equity-statement.scm: GnuCash's row `row` of table 1 holds `want`.
struct Ported {
    name: &'static str,
    line: u64,
    row: usize,
    want: [&'static str; 2],
}

fn ported() -> [Ported; 7] {
    let p = |name, line, row, want| Ported { name, line, row, want };
    [
        p("capital", 86, 2, ["Capital, 1970-01-01", "$29.00"]),
        p("income", 90, 3, ["Net income for Period", "$620.00"]),
        p("investments", 94, 4, ["Investments for Period", "$2,722.00"]),
        p("withdrawals", 98, 5, ["Withdrawals for Period", "$255.00"]),
        p("unrealized", 102, 6, ["Unrealized Losses for Period", "$1.00"]),
        p("inc/dec in capital", 106, 7, ["Increase in capital", "$3,086.00"]),
        p("capital end", 110, 8, ["Capital, 2005-01-01", "$3,115.00"]),
    ]
}

/// The text count of test-equity-statement.scm:82-84: "current table has 22 cells", though it expects 14, the seven
/// rows' label and amount.
const TEXT_COUNT: (&str, u64, usize) = ("current table has 22 cells", 82, 14);

/// Every check of test-equity-statement.scm:77-112, on the XML fixture and its SQLite copy, through `report.run` and
/// `report.export`. GnuCash's row N is the result's row N - 2: its rows count from 1 and its first row is two empty
/// spacer cells (ES:422-423), which the result leaves out.
#[test]
fn expected_figures_of_gnucash_test() {
    let e = expected("adjusting-entries.expected.json");
    let options = options_param(expected_run(&e, "equity-statement current"));
    for (format, mut s) in both_formats() {
        let result = s.run(EQUITY, &options);
        let html = s.export(EQUITY, &options);
        let got = statement(&result);
        let (name, line, count) = TEXT_COUNT;
        assert_eq!(
            gnucash_text_count(&tables(&result), 1),
            count,
            "{format}: {name} (test line {line})"
        );
        ported().iter().for_each(|c| {
            assert_eq!(got[c.row - 2], c.want, "{format}: {} (test line {}): {got:?}", c.name, c.line);
            c.want
                .iter()
                .for_each(|w| assert!(html.contains(w), "{format}: {} (test line {}): {w} is in the HTML", c.name, c.line));
        });
        assert_eq!(
            result["title"], " Equity Statement For Period Covering 1970-01-01 to 2005-01-01",
            "{format}"
        );
    }
}

/// The checks above are exactly the Equity Statement's checks of `adjusting-entries.expected.json`, with its names,
/// lines and strings.
#[test]
fn every_check_of_the_expected_figures_is_ported() {
    let e = expected("adjusting-entries.expected.json");
    let run = expected_run(&e, "equity-statement current");
    assert_eq!(run["template"], EQUITY);
    assert_eq!(options_param(run), json!({ "General": test_period() }));
    let checks = run["checks"].as_array().unwrap();
    let (name, line, count) = TEXT_COUNT;
    assert_eq!(
        json!([checks[0]["name"], checks[0]["line"], checks[0]["count"]]),
        json!([name, line, count])
    );
    let theirs: Vec<Value> = checks[1..]
        .iter()
        .map(|c| json!([c["name"], c["line"], c["select"]["row"], c["expected"]]))
        .collect();
    let ours: Vec<Value> = ported().iter().map(|c| json!([c.name, c.line, c.row, c.want])).collect();
    assert_eq!(ours, theirs);
    let others = e["runs"].as_array().unwrap().iter().filter(|r| r["template"] == EQUITY).count();
    assert_eq!(others, 1, "one Equity Statement run");
}

/// The options of ES:97-162 in GnuCash's order (sections by name, then keys), with "Show Foreign Currencies", which
/// nothing reads (ES:132-134), kept so saved configurations keep it.
#[test]
fn options_are_gnucash_s() {
    let mut s = Setup::with_config(&test_settings("iso"));
    let described = s.call("report.options", json!({ "report": EQUITY }));
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
        r#"Accounts/Accounts "a" []"#,
        r#"Commodities/Report's currency "a" "CURRENCY::USD""#,
        r#"Commodities/Price Source "b" "pricedb-nearest""#,
        r#"Commodities/Show Foreign Currencies "c" true"#,
        r#"Commodities/Show Exchange Rates "d" false"#,
        r#"Display/Show accounting-style rules "f" false"#,
        r#"Entries/Closing Entries pattern "a" "Closing Entries""#,
        r#"Entries/Closing Entries pattern is case-sensitive "b" false"#,
        r#"Entries/Closing Entries Pattern is regular expression "c" false"#,
        r#"General/Report name "0a" "Equity Statement""#,
        r#"General/Stylesheet "0b" "Default""#,
        r#"General/Add options summary "0c" "never""#,
        r#"General/Report Title "a" "Equity Statement""#,
        r#"General/Start Date "ca" {"relative":"start-accounting-period"}"#,
        r#"General/End Date "cb" {"relative":"end-accounting-period"}"#,
    ];
    assert_eq!(got, want);
}

/// "Show Foreign Currencies" changes nothing: every figure is one amount in the report's currency (ES:197-230 read
/// every other option).
#[test]
fn show_foreign_currencies_is_read_nowhere() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("iso"));
    let on = s.run(EQUITY, &json!({ "General": test_period() }));
    let off = s.run(
        EQUITY,
        &json!({ "General": test_period(), "Commodities": { "Show Foreign Currencies": false } }),
    );
    assert_eq!(on["sections"], off["sections"]);
}

/// The default accounts are every account of the types of ES:114-121, in display order; a book with none gives the
/// no-account warning and nothing else, under the usual title (ES:278-293).
#[test]
fn no_accounts_give_the_warning() {
    let mut s = Setup::new_book(&test_settings("iso"));
    let result = s.run(EQUITY, &json!({ "General": test_period() }));
    let b = blocks(&result);
    assert_eq!(b.len(), 1);
    assert_eq!(
        (b[0]["kind"].as_str(), b[0]["title"].as_str()),
        (Some("warning"), Some("No accounts selected"))
    );
    assert_eq!(result["title"], " Equity Statement For Period Covering 1970-01-01 to 2005-01-01");
}

/// The "drawings" book of the module notes.
fn drawings_book() -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    ["Bank:BANK", "Capital:EQUITY", "Income:INCOME", "Expenses:EXPENSE"]
        .iter()
        .for_each(|a| {
            let (name, kind) = a.split_once(':').unwrap();
            s.account(name, kind, None, "CURRENCY::USD");
        });
    s.transfer("2024-01-01", "Owner's investment", "Capital", "Bank", "1000.00");
    s.transfer("2024-02-10", "Sales", "Income", "Bank", "300.00");
    s.transfer("2024-03-05", "Rent", "Bank", "Expenses", "500.00");
    s.transfer("2024-04-01", "Drawings", "Bank", "Capital", "200.00");
    s
}

fn period(start: &str, end: &str) -> Value {
    json!({ "Start Date": { "absolute": start }, "End Date": { "absolute": end } })
}

fn year(start: &str, more: Value) -> Value {
    let mut options = json!({ "General": period(start, "2024-12-31") });
    more.as_object().unwrap().iter().for_each(|(k, v)| options[k] = v.clone());
    options
}

/// 2024 on the drawings book. Start capital 0 (ES:390-393); the income and expenses of the period, +$200 in GnuCash's
/// signs, make net income -$200, shown as a loss (ES:408-414, :432-436); equity ends at -$800, so the net investment is
/// 0 - (-800) = $800 (ES:365-371), the $200 drawn is a withdrawal (ES:378-379) and investments are $1,000
/// (ES:381-382); with no foreign commodity there are no unrealized gains and that row is left out (ES:448);
/// the increase is -200 + 1,000 + 0 - 200 = $600 (ES:384-388), the end capital $600 (ES:395-397), Bank's balance.
/// The capital rows are GnuCash's "primary-subheading" rows at the right (ES:427-430, :463-467); the period rows one
/// level in (column 0, ES:418).
#[test]
fn a_net_loss_drawings_and_no_unrealized_row() {
    let result = drawings_book().run(EQUITY, &year("2024-01-01", json!({})));
    assert_eq!(
        statement(&result),
        rows(&[
            ["Capital, 2024-01-01", "$0.00"],
            ["Net loss for Period", "$200.00"],
            ["Investments for Period", "$1,000.00"],
            ["Withdrawals for Period", "$200.00"],
            ["Increase in capital", "$600.00"],
            ["Capital, 2024-12-31", "$600.00"],
        ])
    );
    let table = tables(&result)[0];
    let shape: Vec<(String, u64)> = table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["role"].as_str().unwrap().to_owned(), r["level"].as_u64().unwrap()))
        .collect();
    let want = [("heading", 0), ("data", 1), ("data", 1), ("data", 1), ("data", 0), ("heading", 0)];
    assert_eq!(shape, want.map(|(r, l)| (r.to_owned(), l)));
    assert_eq!(table["columns"][1]["levels"], 2);
    assert!(table["rows"].as_array().unwrap().iter().all(|r| r["cells"][1]["rule"].is_null()));
}

/// From 2024-02-01 the opening capital is the $1,000 invested: 0 - (-1,000) - 0 (ES:390-393). Equity falls from -$1,000
/// to -$800, a net investment of -$200, and the $200 drawn makes investments 0 (ES:365-382); the increase is
/// -200 + 0 - 200 = -$400, shown as a decrease (ES:456-461).
#[test]
fn a_decrease_in_capital() {
    let result = drawings_book().run(EQUITY, &year("2024-02-01", json!({})));
    assert_eq!(
        statement(&result),
        rows(&[
            ["Capital, 2024-02-01", "$1,000.00"],
            ["Net loss for Period", "$200.00"],
            ["Investments for Period", "$0.00"],
            ["Withdrawals for Period", "$200.00"],
            ["Decrease in capital", "$400.00"],
            ["Capital, 2024-12-31", "$600.00"],
        ])
    );
}

/// The period starts after 23:59:59 of the day before the Start Date (ES:205-206, DU:495-496), so a Start Date on the
/// drawing's day takes the drawing in: the opening capital is Bank's $800 before it, and the $200 drawn is a decrease.
/// A day later the drawing is before the period: capital $600 throughout.
#[test]
fn the_start_date_takes_in_its_whole_day() {
    let mut s = drawings_book();
    let on = s.run(EQUITY, &year("2024-04-01", json!({})));
    assert_eq!(
        statement(&on),
        rows(&[
            ["Capital, 2024-04-01", "$800.00"],
            ["Net income for Period", "$0.00"],
            ["Investments for Period", "$0.00"],
            ["Withdrawals for Period", "$200.00"],
            ["Decrease in capital", "$200.00"],
            ["Capital, 2024-12-31", "$600.00"],
        ])
    );
    let after = s.run(EQUITY, &year("2024-04-02", json!({})));
    assert_eq!(
        statement(&after),
        rows(&[
            ["Capital, 2024-04-02", "$600.00"],
            ["Net income for Period", "$0.00"],
            ["Investments for Period", "$0.00"],
            ["Withdrawals for Period", "$0.00"],
            ["Increase in capital", "$0.00"],
            ["Capital, 2024-12-31", "$600.00"],
        ])
    );
}

/// "Show accounting-style rules" puts a rule over the change in capital only (ES:456-461).
#[test]
fn accounting_style_rules_rule_the_change_in_capital() {
    let result = drawings_book().run(
        EQUITY,
        &year("2024-01-01", json!({ "Display": { "Show accounting-style rules": true } })),
    );
    let ruled: Vec<String> = tables(&result)[0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["cells"][1]["rule"] == true)
        .map(|r| r["cells"][0]["text"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ruled, ["Increase in capital"]);
}

/// The drawings book closed on 2024-12-30 by two transactions described "Closing Entries": Income's $300 into
/// Capital, then Expenses' $500 out of it. Neither is flagged as closing.
fn closed_by_description() -> Setup {
    let mut s = drawings_book();
    s.transfer("2024-12-30", "Closing Entries", "Capital", "Income", "300.00");
    s.transfer("2024-12-30", "Closing Entries", "Expenses", "Capital", "500.00");
    s
}

fn entries(pattern: &str, cased: bool, regexp: bool) -> Value {
    year(
        "2024-01-01",
        json!({ "Entries": {
            "Closing Entries pattern": pattern,
            "Closing Entries pattern is case-sensitive": cased,
            "Closing Entries Pattern is regular expression": regexp,
        } }),
    )
}

/// Matched closing entries: the income and expense ones are the net income, -$200, since the period's change with
/// them is 0 (ES:320-328); the equity ones, +$200, come off the end equity of -$600 (ES:361-367), so the net
/// investment is $800. GnuCash counts every debit to equity as a withdrawal (ES:373-379), so the $500 that closes the
/// expenses is one, beside the $200 drawn: withdrawals $700, investments $1,500, increase -200 + 1,500 - 700 = $600.
const MATCHED: [[&str; 2]; 6] = [
    ["Capital, 2024-01-01", "$0.00"],
    ["Net loss for Period", "$200.00"],
    ["Investments for Period", "$1,500.00"],
    ["Withdrawals for Period", "$700.00"],
    ["Increase in capital", "$600.00"],
    ["Capital, 2024-12-31", "$600.00"],
];

/// Unmatched: no closing entries, so the net income is the change of 0 (ES:324-328); the net investment is 0 - (-600)
/// = $600 and investments $1,300, the increase again $600.
const UNMATCHED: [[&str; 2]; 6] = [
    ["Capital, 2024-01-01", "$0.00"],
    ["Net income for Period", "$0.00"],
    ["Investments for Period", "$1,300.00"],
    ["Withdrawals for Period", "$700.00"],
    ["Increase in capital", "$600.00"],
    ["Capital, 2024-12-31", "$600.00"],
];

/// The closing pattern matches the description (ES:225-250 into
/// `5.17:gnucash/report/report-utilities.scm:692-725`): plainly and ignoring case by default, case-sensitive when asked,
/// and as a regular expression when asked; a pattern that is not matched leaves the entries as ordinary ones.
#[test]
fn the_closing_pattern_matches_descriptions() {
    let mut s = closed_by_description();
    let cases = [
        ("Closing Entries", false, false, MATCHED),
        ("closing entries", false, false, MATCHED),
        ("closing entries", true, false, UNMATCHED),
        ("^Clos.*ries$", true, true, MATCHED),
        ("^Clos.*ries$", false, false, UNMATCHED),
    ];
    cases.iter().for_each(|(pattern, cased, regexp, want)| {
        let result = s.run(EQUITY, &entries(pattern, *cased, *regexp));
        assert_eq!(statement(&result), rows(want), "{pattern:?} cased {cased} regexp {regexp}");
    });
}

/// Transactions that close the books are closing entries whatever their description (the `closing` key, ES:250):
/// GnuCash's Close Book on 2024-12-30 makes the same two entries as above, and a pattern matching nothing gives the
/// matched figures.
#[test]
fn closing_transactions_match_whatever_their_description() {
    let mut s = drawings_book();
    let close =
        json!({ "date": "2024-12-30", "incomeTotalAccount": "Capital", "expenseTotalAccount": "Capital", "description": "Year end" });
    s.call("book.closeBooks", close);
    let result = s.run(EQUITY, &entries("Nothing", false, false));
    assert_eq!(statement(&result), rows(&MATCHED));
}

/// A book with a euro account: $1,000 invested on 2024-01-10, €100 bought for $110 on 2024-02-01; EUR is $1.05 on
/// 2024-01-15 and `end_rate` on 2024-12-01.
fn euro_book(end_rate: &str) -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    s.account("Bank", "BANK", None, "CURRENCY::USD");
    s.account("Euro", "BANK", None, "CURRENCY::EUR");
    s.account("Capital", "EQUITY", None, "CURRENCY::USD");
    s.transfer("2024-01-10", "Owner's investment", "Capital", "Bank", "1000.00");
    let splits = json!([{ "account": "Euro", "value": "110.00", "amount": "100.00" }, { "account": "Bank", "value": "-110.00" }]);
    s.call(
        "transaction.post",
        json!({ "datePosted": "2024-02-01", "currency": "CURRENCY::USD", "splits": splits }),
    );
    [("2024-01-15", "1.05"), ("2024-12-01", end_rate)].iter().for_each(|(date, value)| {
        let price = json!({ "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "value": value, "date": date });
        s.call("price.add", price);
    });
    s
}

/// The book ends 2024 at -$110 + €100 (ES:344-348). At $1.20 that is +$10 and at its average cost, $1.10 a euro
/// (`5.17:gnucash/report/commodity-utilities.scm:497-540`), 0, so the unrealized gains are $10 (ES:264-270,
/// :355-359), shown under their own label (ES:448-454); at $1.00 they are a $10 loss. The increase is
/// 0 + 1,000 + 10 - 0 (ES:384-388).
#[test]
fn unrealized_gains_and_losses() {
    let gains = euro_book("1.20").run(EQUITY, &year("2024-01-01", json!({})));
    assert_eq!(
        statement(&gains),
        rows(&[
            ["Capital, 2024-01-01", "$0.00"],
            ["Net income for Period", "$0.00"],
            ["Investments for Period", "$1,000.00"],
            ["Withdrawals for Period", "$0.00"],
            ["Unrealized Gains for Period", "$10.00"],
            ["Increase in capital", "$1,010.00"],
            ["Capital, 2024-12-31", "$1,010.00"],
        ])
    );
    let losses = euro_book("1.00").run(EQUITY, &year("2024-01-01", json!({})));
    let got = statement(&losses);
    assert_eq!(got[4], ["Unrealized Losses for Period", "$10.00"]);
    assert_eq!(
        got[5..],
        rows(&[["Increase in capital", "$990.00"], ["Capital, 2024-12-31", "$990.00"]])
    );
}

/// "Show Exchange Rates" (ES:473-491): a rule ends the statement's table, as GnuCash appends its ruler to the table
/// it has already added, and the rates follow side by side, at the start and at the end, each under its date: the
/// nearest prices to 2023-12-31 23:59:59 and to 2024-12-31 23:59:59 (ES:261-262).
#[test]
fn exchange_rates_at_both_dates() {
    let mut s = euro_book("1.20");
    let result = s.run(
        EQUITY,
        &year("2024-01-01", json!({ "Commodities": { "Show Exchange Rates": true } })),
    );
    let b = blocks(&result);
    assert_eq!(b.len(), 3);
    let last = tables(&result)[0]["rows"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["role"], "rule");
    assert_eq!(result["sections"][0]["blocks"][1]["kind"], "group");
    let rates: Vec<(String, Vec<String>)> = b[1..]
        .iter()
        .map(|t| {
            let cells = t["rows"][0]["cells"].as_array().unwrap().iter().flat_map(displays).collect();
            (t["title"].as_str().unwrap().to_owned(), cells)
        })
        .collect();
    let want = [("2024-01-01", ["€1.00", "$1.05"]), ("2024-12-31", ["€1.00", "$1.20"])];
    assert_eq!(rates, want.map(|(d, c)| (d.to_owned(), c.map(str::to_owned).to_vec())));
    assert!(b[1..].iter().all(|t| t["columns"][0]["label"] == "Exchange rate"));
    let html = s.export(
        EQUITY,
        &year("2024-01-01", json!({ "Commodities": { "Show Exchange Rates": true } })),
    );
    ["$1.05", "$1.20", "2024-12-31"]
        .iter()
        .for_each(|w| assert!(html.contains(w), "{w} is in the HTML"));
    let plain = s.run(EQUITY, &year("2024-01-01", json!({})));
    assert_eq!(blocks(&plain).len(), 1);
}

/// Each price source on GnuCash's test book and period, where the book ends at $5 - #4 and the net income is
/// -$298 + #612 (ES:254-259, :264-270). GBP is #1 = $1.50 at the 2000-02-15 price, nearest, before or latest; $23/16
/// at the weighted average of the two exchanges ($14 for #10, $9 for #6; `report/commodity-utilities.scm:599-607`);
/// $5/4 at average cost ((14 - 9) / (10 - 6)), which values the book at cost, so its unrealized gains are 0 and the
/// row is left out (ES:448). The net investment is $2,467 throughout.
#[test]
fn each_price_source() {
    let at_three_halves: [[&str; 2]; 7] = [
        ["Capital, 1970-01-01", "$29.00"],
        ["Net income for Period", "$620.00"],
        ["Investments for Period", "$2,722.00"],
        ["Withdrawals for Period", "$255.00"],
        ["Unrealized Losses for Period", "$1.00"],
        ["Increase in capital", "$3,086.00"],
        ["Capital, 2005-01-01", "$3,115.00"],
    ];
    let cases: [(&str, Vec<[&str; 2]>); 4] = [
        ("pricedb-before", at_three_halves.to_vec()),
        ("pricedb-latest", at_three_halves.to_vec()),
        (
            "weighted-average",
            vec![
                ["Capital, 1970-01-01", "$29.00"],
                ["Net income for Period", "$581.75"],
                ["Investments for Period", "$2,722.00"],
                ["Withdrawals for Period", "$255.00"],
                ["Unrealized Losses for Period", "$0.75"],
                ["Increase in capital", "$3,048.00"],
                ["Capital, 2005-01-01", "$3,077.00"],
            ],
        ),
        (
            "average-cost",
            vec![
                ["Capital, 1970-01-01", "$29.00"],
                ["Net income for Period", "$467.00"],
                ["Investments for Period", "$2,722.00"],
                ["Withdrawals for Period", "$255.00"],
                ["Increase in capital", "$2,934.00"],
                ["Capital, 2005-01-01", "$2,963.00"],
            ],
        ),
    ];
    let mut s = Setup::fixture(FIXTURE, &test_settings("iso"));
    cases.iter().for_each(|(source, want)| {
        let options = json!({ "General": test_period(), "Commodities": { "Price Source": source } });
        assert_eq!(statement(&s.run(EQUITY, &options)), rows(want), "{source}");
    });
}

/// Withdrawals add an equity account's amount under the transaction's currency (ES:177): €100 drawn from a euro
/// equity account in a dollar transaction worth $110 counts as $100. Kept, as GnuCash does.
#[test]
fn withdrawals_take_the_transaction_currency() {
    let mut s = euro_book("1.20");
    s.account("Euro Capital", "EQUITY", None, "CURRENCY::EUR");
    let splits = json!([{ "account": "Euro Capital", "value": "110.00", "amount": "100.00" }, { "account": "Bank", "value": "-110.00" }]);
    s.call(
        "transaction.post",
        json!({ "datePosted": "2024-06-01", "currency": "CURRENCY::USD", "splits": splits }),
    );
    let got = statement(&s.run(EQUITY, &year("2024-01-01", json!({}))));
    assert_eq!(got[3], ["Withdrawals for Period", "$100.00"]);
}

/// The title is the company name, the "Report Title" option and the period (ES:278-283): GnuCash's book option
/// `options/Business/Company Name` (`5.17:bindings/guile/options.scm:409-411`), written into the fixture's copy.
#[test]
fn the_title_names_the_company() {
    let mut s = Setup::with_config(&test_settings("iso"));
    let text = std::fs::read_to_string(format!("{}/../../fixtures/reports/{FIXTURE}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let id = "<book:id type=\"guid\">b0000000000000000000000000000000</book:id>\n";
    let slots = "<book:slots>\n  <slot>\n    <slot:key>options</slot:key>\n    <slot:value type=\"frame\">\n      <slot>\n        \
                 <slot:key>Business</slot:key>\n        <slot:value type=\"frame\">\n          <slot>\n            \
                 <slot:key>Company Name</slot:key>\n            <slot:value type=\"string\">Acme Ltd</slot:value>\n          \
                 </slot>\n        </slot:value>\n      </slot>\n    </slot:value>\n  </slot>\n</book:slots>\n";
    assert!(text.contains(id));
    std::fs::write(s.book(), text.replacen(id, &format!("{id}{slots}"), 1)).unwrap();
    let book = s.book().display().to_string();
    s.call("book.open", json!({ "path": book, "readOnly": true }));
    let options = json!({ "General": { "Report Title": "Statement of Owner's Equity", "Start Date": { "absolute": "1970-01-01" },
                                       "End Date": { "absolute": "2005-01-01" } } });
    let result = s.run(EQUITY, &options);
    assert_eq!(
        result["title"],
        "Acme Ltd Statement of Owner's Equity For Period Covering 1970-01-01 to 2005-01-01"
    );
}

/// Running the statement, with every option that changes what it reads, leaves the Book, its revision and its file as
/// they were and writes nothing beside it, on XML and SQLite; two exports are the same bytes, with no script.
#[test]
fn a_run_changes_nothing() {
    for (format, mut s) in both_formats() {
        let bytes = std::fs::read(s.book()).unwrap();
        let before = s.d.context().book().unwrap().clone();
        let revision = s.d.context().session().unwrap().revision();
        let files = s.files();
        let options = json!({
            "General": test_period(),
            "Commodities": { "Show Exchange Rates": true, "Price Source": "average-cost" },
            "Display": { "Show accounting-style rules": true },
            "Entries": { "Closing Entries pattern": "^Clos", "Closing Entries Pattern is regular expression": true },
        });
        s.run(EQUITY, &options);
        let (a, b) = (s.export(EQUITY, &options), s.export(EQUITY, &options));
        assert_eq!(a, b, "{format}");
        assert!(!a.to_ascii_lowercase().contains("<script"), "{format}");
        assert_eq!(s.d.context().book().unwrap(), &before, "{format}");
        assert_eq!(s.d.context().session().unwrap().revision(), revision, "{format}");
        assert_eq!(s.call("book.info", json!({}))["dirty"], false, "{format}");
        assert_eq!(s.files(), files, "{format}: no journal or other file was written");
        assert_eq!(std::fs::read(s.book()).unwrap(), bytes, "{format}: the file is unchanged");
    }
}
