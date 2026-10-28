//! The Multicolumn Balance Sheet and Income Statement (`standard/balsheet-pnl.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/balsheet-pnl.scm`, cited as BP) through `report.run` and `report.export`:
//! every multicolumn check of GnuCash's own test on `fixtures/reports/balsheet-pnl.gnucash` and its SQLite copy, and
//! hand-worked cases on `balsheet-pnl-extras.gnucash` and `-trading`.
//!
//! GnuCash's test reads the text of HTML table cells (`5.17:gnucash/report/test/test-report-extras.scm:96-109`). The
//! result has no HTML table, so [`gnucash_rows`] rebuilds what GnuCash's table holds from the result's rows, as the
//! Scheme lays them out: a label cell and one cell per column (BP:346-369, full names and no amount indenting in every
//! run the test makes); the exchange-rate rows back into the one title row that holds them (BP:770-797), each rate
//! `"<one unit> <rate>"`; and each cell's texts in the order Guile's `sxpath` reads them, breadth first, so the amounts
//! shown (linked or not) come before the originals GnuCash prints in `<small>` with a trailing space (BP:404-410), and
//! a P&L header's `" to <end>"` is one text after its start (BP:1143-1147). The fixture's full names use `.`, the
//! engine's separator in GnuCash's test environment; red-ale's is `:`, so the expected names are compared with `:`.
//!
//! The extras books (`scripts/report-fixtures/balsheet_pnl_extras.py`, whose docstring gives every balance): Assets
//! (placeholder) holds Checking ($3,900 at 2024-03-15, $2,250 at the end of 2023), Savings (no splits), Euro Account
//! (EUR 500, bought for $550 on 2023-03-01; EUR prices 11/10 on 2023-03-01 and 6/5 on 2024-01-01) and Swiss Account
//! (CHF 800, never priced); Liabilities holds Credit Card (-$150, then -$80); Equity holds Opening Balances (-$1,000)
//! and Retained Earnings (-$1,650, from the closing transaction of 2023-12-31); Income holds Salary ($3,000 a year,
//! closed to zero at the end of 2023) and Consulting CHF (CHF 800, never closed); Expenses holds Rent ($1,200 a year)
//! and Groceries ($150 in 2023, $80 in 2024). All five parents are placeholders.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::{Value, json};
use statements::{Setup, blocks, displays, expected, options_param, tables, test_settings};

const BALSHEET: &str = "065d5d5a77ba11e8b31e83ada73c5eea";
const PNL: &str = "0e94fd0277ba11e8825d43e27232c9d4";

/// The extras books' accounts, by GUID (the docstring's creation order).
const ASSETS: &str = "a0000000000000000000000000000001";
const CHECKING: &str = "a0000000000000000000000000000002";
const EURO: &str = "a0000000000000000000000000000004";
const SWISS: &str = "a0000000000000000000000000000005";
const INCOME: &str = "a000000000000000000000000000000b";
const SALARY: &str = "a000000000000000000000000000000c";
const CONSULTING: &str = "a000000000000000000000000000000d";
const RENT: &str = "a000000000000000000000000000000f";
const GROCERIES: &str = "a0000000000000000000000000000010";

/// One row of GnuCash's table: the texts of each of its cells, from column 1; and beside each text, the display
/// strings of the result it is made from, which `report.export`'s HTML must hold.
#[derive(Debug, Default)]
struct GnuCashRow {
    cells: Vec<Vec<String>>,
    pieces: Vec<String>,
}

/// The texts of a data cell in the order `sxpath` reads them.
fn cell_texts(cell: &Value, pieces: &mut Vec<String>) -> Vec<String> {
    let s = |v: &Value| v.as_str().unwrap().to_owned();
    if cell.is_null() {
        return vec![];
    }
    if let Some(text) = cell["text"].as_str() {
        pieces.push(text.to_owned());
        return vec![text.to_owned()];
    }
    if let Some(lines) = cell["commodities"].as_array() {
        let shown = lines.iter().map(|l| if l["convertedDisplay"].is_string() { s(&l["convertedDisplay"]) } else { s(&l["display"]) });
        let originals = lines.iter().filter(|l| l["convertedDisplay"].is_string()).map(|l| format!("{} ", s(&l["display"])));
        let texts: Vec<String> = shown.chain(originals).collect();
        pieces.extend(texts.iter().map(|t| t.trim().to_owned()));
        return texts;
    }
    if cell["until"].is_string() {
        let display = s(&cell["display"]);
        let (from, until) = display.split_once(" to ").unwrap();
        pieces.extend([from.to_owned(), until.to_owned()]);
        return vec![from.to_owned(), format!(" to {until}")];
    }
    let display = s(&cell["display"]);
    pieces.push(display.clone());
    vec![display]
}

/// The rows of GnuCash's first table, rebuilt from the result's first table.
fn gnucash_rows(result: &Value) -> Vec<GnuCashRow> {
    let table = tables(result)[0];
    let mut out: Vec<GnuCashRow> = vec![];
    table["rows"].as_array().unwrap().iter().for_each(|row| {
        let cells = row["cells"].as_array().unwrap();
        let label = &cells[0];
        if label["monetary"].is_object() {
            // An exchange rate: its texts join the title row's cells, one "<unit> <rate>" per commodity.
            let title = out.last_mut().unwrap();
            let unit = label["display"].as_str().unwrap().to_owned();
            cells[1..].iter().enumerate().for_each(|(i, c)| {
                let rate = cell_texts(c, &mut title.pieces).join("");
                title.cells[i + 1].push(format!("{unit} {rate}"));
            });
            title.pieces.push(unit);
            return;
        }
        let mut r = GnuCashRow::default();
        let label_text: Vec<String> = label["text"].as_str().map(|t| vec![t.to_owned()]).unwrap_or_default();
        r.pieces.extend(label_text.clone());
        r.cells.push(label_text);
        let data: Vec<Vec<String>> = cells[1..].iter().map(|c| cell_texts(c, &mut r.pieces)).collect();
        r.cells.extend(data);
        out.push(r);
    });
    out
}

/// GnuCash's 1-based index, negative from the end, into `len` items.
fn pick(n: i64, len: usize) -> usize {
    let len = i64::try_from(len).unwrap();
    usize::try_from(if n < 0 { len + n } else { n - 1 }).unwrap()
}

/// The texts `(sxml->table-row-col sxml 1 row col)` reads, `null` row or column meaning all; and the display strings
/// they come from.
fn select(rows: &[GnuCashRow], sel: &Value) -> (Vec<String>, Vec<String>) {
    let chosen: Vec<&GnuCashRow> = match sel["row"].as_i64() {
        Some(n) => vec![&rows[pick(n, rows.len())]],
        None => rows.iter().collect(),
    };
    let mut texts = vec![];
    let mut pieces = vec![];
    chosen.iter().for_each(|r| {
        let cols: Vec<usize> = match sel["column"].as_i64() {
            Some(n) => vec![pick(n, r.cells.len())],
            None => (0..r.cells.len()).collect(),
        };
        cols.iter().filter(|c| **c < r.cells.len()).for_each(|c| texts.extend(r.cells[*c].iter().cloned()));
        pieces.extend(r.pieces.iter().cloned());
    });
    (texts, pieces)
}

/// The expected strings of a check, account names with red-ale's separator.
fn want(check: &Value) -> Vec<String> {
    let names = check["select"]["column"] == 1;
    check["expected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .map(|v| if names && v.starts_with("Root") { v.replace('.', ":") } else { v.to_owned() })
        .collect()
}

/// test-balsheet-pnl.scm:493-650: every check of the multicolumn runs of `balsheet-pnl.expected.json`, through
/// `report.run` and `report.export`, on the XML book and its SQLite copy. The runs at :617-650 give the P&L's options
/// to the balance sheet's GUID, which GnuCash renders as a balance sheet (`5.17:gnucash/report/report-core.scm:389-409`).
#[test]
fn expected_figures_of_gnucash_test() {
    let e = expected("balsheet-pnl.expected.json");
    let settings = test_settings("iso");
    let books = [
        ("xml", Setup::fixture("balsheet-pnl.gnucash", &settings)),
        ("sqlite", Setup::sqlite_fixture("balsheet-pnl.gnucash", &settings)),
    ];
    let mut checked = 0;
    for (format, mut s) in books {
        e["runs"].as_array().unwrap().iter().filter(|r| r["id"].as_str().unwrap().starts_with("multicol")).for_each(|run| {
            let options = options_param(run);
            let template = run["templateGuid"].as_str().unwrap();
            let result = s.run(template, &options);
            let html = s.export(template, &options);
            let rows = gnucash_rows(&result);
            run["checks"].as_array().unwrap().iter().for_each(|c| {
                let what = format!("{format} {}: {} (test line {})", run["id"], c["name"], c["line"]);
                let (got, pieces) = select(&rows, &c["select"]);
                assert_eq!(got, want(c), "{what}");
                pieces.iter().filter(|p| !p.is_empty()).for_each(|p| assert!(html.contains(p.as_str()), "{what}: {p} is in the HTML"));
                checked += 1;
            });
        });
    }
    assert_eq!(checked, 2 * 16, "the 16 multicolumn checks, on both books");
}

// Hand-worked cases.

fn extras() -> Setup {
    Setup::fixture("balsheet-pnl-extras.gnucash", &test_settings("iso"))
}

/// The texts a cell shows: a text cell's text; a monetary, price or date cell's display; each line of a
/// `commodities` cell, its amount then its converted value.
fn texts(cell: &Value) -> Vec<String> {
    cell["text"].as_str().map_or_else(|| displays(cell), |t| vec![t.to_owned()])
}

/// The one row of `table` with `role` and label `label`.
fn row<'a>(table: &'a Value, role: &str, label: &str) -> &'a Value {
    let found: Vec<&Value> = table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["role"].as_str().unwrap_or("data") == role && texts(&r["cells"][0]) == [label])
        .collect();
    assert_eq!(found.len(), 1, "one {role} row {label:?} in {table}");
    found[0]
}

/// The texts of each data cell of the `role` row labelled `label`.
fn amounts(table: &Value, role: &str, label: &str) -> Vec<Vec<String>> {
    row(table, role, label)["cells"].as_array().unwrap()[1..].iter().map(texts).collect()
}

/// The labels of `table`'s rows, empty for a spacer.
fn labels(table: &Value) -> Vec<String> {
    table["rows"].as_array().unwrap().iter().map(|r| texts(&r["cells"][0]).concat()).collect()
}

/// `want` as owned strings, one cell.
fn one(want: &[&str]) -> Vec<Vec<String>> {
    vec![want.iter().map(|s| (*s).to_owned()).collect()]
}

/// `want` as owned strings, a cell each.
fn cols(want: &[&[&str]]) -> Vec<Vec<String>> {
    want.iter().map(|c| c.iter().map(|s| (*s).to_owned()).collect()).collect()
}

/// The options of `report.options` for `template`, as `"Section/Name key default"`.
fn described(s: &mut Setup, template: &str) -> Vec<String> {
    let d = s.call("report.options", json!({ "report": template }));
    d["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| format!("{}/{} {} {}", o["section"].as_str().unwrap(), o["name"].as_str().unwrap(), o["key"], o["default"]))
        .collect()
}

/// The options of BP:128-277 in GnuCash's order: sections by name, options by key, "Enable chart" and "Period order
/// is most recent first" sharing c5 in declaration order; the P&L adds the overall period (BP:269-273).
#[test]
fn options_are_gnucash_s() {
    let mut s = Setup::with_config(&test_settings("iso"));
    let common = [
        r#"Accounts/Accounts "a" []"#,
        r#"Accounts/Levels of Subaccounts "b" "all""#,
        r#"Commodities/Common Currency "b" false"#,
        r#"Commodities/Report's currency "c" "CURRENCY::USD""#,
        r#"Commodities/Price Source "d" "pricedb-nearest""#,
        r#"Commodities/Show original currency amount "e" true"#,
        r#"Commodities/Show Exchange Rates "f" true"#,
        r#"Display/Include accounts with zero total balances "a" true"#,
        r#"Display/Omit zero balance figures "b" false"#,
        r#"Display/Parent account amounts include children "c" true"#,
        r#"Display/Display accounts as hyperlinks "e" true"#,
        r#"Display/Display amounts as hyperlinks "e5" true"#,
        r#"Display/Account full name instead of indenting "f" false"#,
        r#"Display/Label sections "g" true"#,
        r#"Display/Include totals "h" true"#,
    ];
    let general = |name: &str, pnl: bool| {
        let mut g = vec![
            format!(r#"General/Report name "0a" "{name}""#),
            r#"General/Stylesheet "0b" "Default""#.to_owned(),
            r#"General/Add options summary "0c" "never""#.to_owned(),
            r#"General/Period duration "c2" "disabled""#.to_owned(),
            r#"General/Disable amount indenting "c3" false"#.to_owned(),
            r#"General/Enable dual columns "c4" true"#.to_owned(),
            r#"General/Enable chart "c5" false"#.to_owned(),
            r#"General/Period order is most recent first "c5" true"#.to_owned(),
        ];
        if pnl {
            g.push(r#"General/If more than 1 period column, include overall period? "c6" false"#.to_owned());
        }
        g.extend([
            r#"General/Start Date "ca" {"relative":"start-accounting-period"}"#.to_owned(),
            r#"General/End Date "cb" {"relative":"end-accounting-period"}"#.to_owned(),
        ]);
        g
    };
    [(BALSHEET, "Balance Sheet (Multicolumn)", false), (PNL, "Income Statement (Multicolumn)", true)]
        .iter()
        .for_each(|(template, name, pnl)| {
            let want: Vec<String> = common.iter().map(|s| (*s).to_owned()).chain(general(name, *pnl)).collect();
            assert_eq!(described(&mut s, template), want, "{name}");
        });
    // BP:188-199: the default selection is every account of the listed types, in display order.
    let mut x = extras();
    let d = x.call("report.options", json!({ "report": BALSHEET }));
    let accounts = d["options"].as_array().unwrap().iter().find(|o| o["name"] == "Accounts").unwrap();
    assert_eq!(accounts["default"].as_array().unwrap().len(), 16, "every account of the extras book but the root");
}

/// No accounts: the title, then only the warning (BP:851-865). The balance sheet's title is its end date without a
/// period and the interval with one; the P&L's always the interval.
#[test]
fn no_accounts_give_the_warning_under_each_title() {
    let mut s = Setup::new_book(&test_settings("iso"));
    let dates = json!({ "Start Date": { "absolute": "2000-01-01" }, "End Date": { "absolute": "2000-12-31" } });
    let cases = [
        (BALSHEET, json!({ "General": dates }), "Balance Sheet (Multicolumn) 2000-12-31"),
        (BALSHEET, json!({ "General": { "Start Date": dates["Start Date"], "End Date": dates["End Date"], "Period duration": "MonthDelta" } }),
         "Balance Sheet (Multicolumn) 2000-01-01 to 2000-12-31"),
        (PNL, json!({ "General": dates }), "Income Statement (Multicolumn) 2000-01-01 to 2000-12-31"),
    ];
    cases.iter().for_each(|(template, options, title)| {
        let result = s.run(template, options);
        assert_eq!(result["title"], *title);
        let all = blocks(&result);
        assert_eq!(all.len(), 1, "{title}");
        assert_eq!(all[0]["kind"], "warning");
        assert_eq!(all[0]["title"], "No accounts selected");
    });
}

/// The balance sheet's defaults on the extras book at 2024-03-15 (BP:1022-1083): dual columns, a group of two tables
/// with the assets on the left; one column, so the amounts stand in a staircase of maxindent = 1 + the deepest depth
/// (2) = 3 levels, a parent's subtotal at level 1, an account at 2, titles and totals at 0 (BP:333, :346-369); names
/// indented by depth; placeholders unlinked (BP:432-436); account amounts linked to the account's last split, none
/// for Savings, which has no split (BP:881-886). Recursive mode: a parent's row sums it and its descendants
/// (BP:483-507), and the parents hold nothing of their own, so no second row follows.
///
/// Without a common currency nothing is converted: Assets holds $3,900, EUR 500 and CHF 800, its lines in the order
/// GnuCash's collector leaves them (each commodity where it last appears in the account and its descendants, in book
/// order: Checking, Savings, Euro, Swiss); the section total walks the accounts option's order (Checking, Euro, Savings,
/// Swiss), so EUR comes first. Retained earnings are the income and expense balances with closing, negated (BP:951-963):
/// Salary $3,000, Rent -$1,200, Groceries -$80 make $1,720, and Consulting CHF 800. Equity: $1,000 + $1,650 + $1,720
/// = $4,370; with the liabilities' $80, $4,450.
#[test]
fn dual_columns_and_the_amount_staircase() {
    let mut s = extras();
    let options = json!({ "General": { "End Date": { "absolute": "2024-03-15" } } });
    let result = s.run(BALSHEET, &options);
    assert_eq!(result["title"], "Balance Sheet (Multicolumn) 2024-03-15");
    let all = &result["sections"][0]["blocks"];
    assert_eq!(all.as_array().unwrap().len(), 1);
    assert_eq!(all[0]["kind"], "group");
    let (left, right) = (&all[0]["blocks"][0], &all[0]["blocks"][1]);
    assert_eq!(left["columns"], json!([{ "label": "", "role": "label" }, { "label": "", "role": "amount", "levels": 3 }]));
    assert_eq!(labels(left), ["Asset", "Assets", "Checking", "Euro Account", "Savings", "Swiss Account", "Total For Asset", ""]);
    let assets = row(left, "account", "Assets");
    assert_eq!((assets["depth"].clone(), assets["level"].clone(), assets["cells"][0].get("account")), (json!(1), json!(1), None));
    assert_eq!(amounts(left, "account", "Assets"), one(&["$3,900.00", "€500.00", "Fr.800.00"]));
    let checking = row(left, "account", "Checking");
    assert_eq!((checking["depth"].clone(), checking["level"].clone()), (json!(2), json!(2)));
    assert_eq!(checking["cells"][0]["account"], CHECKING);
    // The payment of 2024-02-25, the eleventh transaction: splits d…16 (Checking) and d…17 (Credit Card).
    assert_eq!(checking["cells"][1]["link"], json!({ "split": "d0000000000000000000000000000016" }));
    assert_eq!(amounts(left, "account", "Savings"), one(&["$0.00"]));
    assert!(row(left, "account", "Savings")["cells"][1].get("link").is_none());
    assert_eq!(row(left, "total", "Total For Asset")["level"], 0);
    assert_eq!(amounts(left, "total", "Total For Asset"), one(&["€500.00", "$3,900.00", "Fr.800.00"]));

    assert_eq!(
        labels(right),
        [
            "Liability", "Liabilities", "Credit Card", "Total For Liability", "", "Equity", "Equity", "Opening Balances",
            "Retained Earnings", "Retained Earnings", "Total For Equity", "", "Total For Liability and Equity", ""
        ]
    );
    assert_eq!(amounts(right, "account", "Credit Card"), one(&["$80.00"]));
    assert_eq!(amounts(right, "data", "Retained Earnings"), one(&["Fr.800.00", "$1,720.00"]));
    assert_eq!(amounts(right, "total", "Total For Equity"), one(&["Fr.800.00", "$4,370.00"]));
    assert_eq!(amounts(right, "grandTotal", "Total For Liability and Equity"), one(&["Fr.800.00", "$4,450.00"]));
    let html = s.export(BALSHEET, &options);
    ["$3,900.00", "€500.00", "Fr.800.00", "$4,450.00", "Total For Liability and Equity"].iter().for_each(|w| assert!(html.contains(w), "{w}"));
}

/// The P&L over 2023 and 2024 to 2024-03-15 (BP:1085-1226): income on the left with the Net Income, which needs both
/// income and expense accounts (BP:1207-1213), expenses on the right. Its cells are the no-closing balance at the end
/// less at the start (BP:1095-1107), so the closing transaction of 2023-12-31 is left out and Salary shows both years,
/// $6,000. Net Income: $6,000 - ($2,400 + $230) = $3,370, and CHF 800. Amounts link to the Transaction Report of
/// their accounts over the column (BP:1109-1124).
#[test]
fn the_pnl_leaves_closing_out_and_the_balance_sheet_keeps_it() {
    let mut s = extras();
    let both = json!({ "General": { "Start Date": { "absolute": "2023-01-01" }, "End Date": { "absolute": "2024-03-15" } } });
    let result = s.run(PNL, &both);
    assert_eq!(result["title"], "Income Statement (Multicolumn) 2023-01-01 to 2024-03-15");
    let t = tables(&result);
    assert_eq!(labels(t[0]), ["Income", "Income", "Consulting CHF", "Salary", "Total For Income", "", "Net Income", "Total For Net Income", ""]);
    assert_eq!(labels(t[1]), ["Expense", "Expenses", "Groceries", "Rent", "Total For Expense", ""]);
    assert_eq!(amounts(t[0], "account", "Salary"), one(&["$6,000.00"]));
    assert_eq!(amounts(t[0], "account", "Consulting CHF"), one(&["Fr.800.00"]));
    assert_eq!(amounts(t[1], "account", "Rent"), one(&["$2,400.00"]));
    assert_eq!(amounts(t[1], "account", "Groceries"), one(&["$230.00"]));
    assert_eq!(amounts(t[0], "grandTotal", "Total For Net Income"), one(&["Fr.800.00", "$3,370.00"]));
    // 2023 alone: the closing entries would zero Salary, Rent and Groceries; the P&L shows the year's figures.
    let y2023 = json!({ "General": { "Start Date": { "absolute": "2023-01-01" }, "End Date": { "absolute": "2023-12-31" } } });
    let t = s.run(PNL, &y2023);
    let t = tables(&t);
    assert_eq!(amounts(t[0], "account", "Salary"), one(&["$3,000.00"]));
    assert_eq!(amounts(t[1], "account", "Rent"), one(&["$1,200.00"]));
    // The balance sheet at the end of 2023 keeps it (BP:868-870): the income and expense balances with closing are $0
    // in USD, so the retained earnings row shows only Consulting's CHF 800; the closing's $1,650 is in the Retained
    // Earnings account.
    let bs = s.run(BALSHEET, &json!({ "General": { "End Date": { "absolute": "2023-12-31" } } }));
    let right = tables(&bs)[1];
    assert_eq!(amounts(right, "data", "Retained Earnings"), one(&["Fr.800.00", "$0.00"]));
    assert_eq!(amounts(right, "account", "Retained Earnings"), one(&["$1,650.00"]));
}

/// "Parent account amounts include children" off (BP:570-583): each parent is shown with its own amount, $0.00, one
/// step further left (level 2, as its children), and after its last child a "Total For <parent>" subtotal at the
/// parent's level, not linked (BP:483-507, :421-440). Two "Total For Equity" rows follow, the Equity account's and
/// the section's, as in GnuCash.
#[test]
fn multilevel_mode_closes_each_parent_with_its_total() {
    let mut s = extras();
    let options = json!({
        "General": { "End Date": { "absolute": "2024-03-15" }, "Enable dual columns": false },
        "Display": { "Parent account amounts include children": false },
    });
    let result = s.run(BALSHEET, &options);
    let t = tables(&result)[0];
    assert_eq!(
        labels(t)[..9],
        ["Asset", "Assets", "Checking", "Euro Account", "Savings", "Swiss Account", "Total For Assets", "Total For Asset", ""]
    );
    let assets = row(t, "account", "Assets");
    assert_eq!((assets["level"].clone(), texts(&assets["cells"][1])), (json!(2), vec!["$0.00".to_owned()]));
    let total = row(t, "subtotal", "Total For Assets");
    assert_eq!((total["depth"].clone(), total["level"].clone()), (json!(1), json!(1)));
    assert_eq!(amounts(t, "subtotal", "Total For Assets"), one(&["$3,900.00", "€500.00", "Fr.800.00"]));
    assert!(total["cells"][0].get("account").is_none() && total["cells"][1].get("link").is_none());
    assert_eq!(amounts(t, "subtotal", "Total For Equity"), one(&["$2,650.00"]));
    assert_eq!(amounts(t, "total", "Total For Equity"), one(&["Fr.800.00", "$4,370.00"]));
}

/// "Include accounts with zero total balances" off hides an account whose amounts are zero in every column
/// (BP:509-534; Savings), but not a multilevel parent (BP:570-572); "Omit zero balance figures" blanks every zero
/// monetary (BP:393-399), so the parent's $0.00 is an empty cell and the retained earnings of 2024-01-01 show only
/// CHF 800. Monthly from 2024-01-01, oldest first, with full names (no depth): four columns, 2024-01-01, 02-01, 03-01
/// and the end, 03-15 (DU:356-379).
#[test]
fn zero_balances_hidden_and_omitted_in_both_modes() {
    let mut s = extras();
    let options = json!({
        "General": { "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-03-15" }, "Period duration": "MonthDelta",
                     "Period order is most recent first": false },
        "Display": { "Include accounts with zero total balances": false, "Omit zero balance figures": true,
                     "Account full name instead of indenting": true },
    });
    let result = s.run(BALSHEET, &options);
    assert_eq!(result["title"], "Balance Sheet (Multicolumn) 2024-01-01 to 2024-03-15");
    let t = tables(&result)[0];
    assert_eq!(amounts(t, "heading", "Date"), cols(&[&["2024-01-01"], &["2024-02-01"], &["2024-03-01"], &["2024-03-15"]]));
    assert!(!labels(t).contains(&"Assets:Savings".to_owned()));
    assert!(row(t, "account", "Assets:Checking").get("depth").is_none());
    // Salary on 2024-01-31; Rent and Groceries in February.
    assert_eq!(
        amounts(t, "data", "Retained Earnings"),
        cols(&[&["Fr.800.00"], &["Fr.800.00", "$3,000.00"], &["Fr.800.00", "$1,720.00"], &["Fr.800.00", "$1,720.00"]])
    );
    let multilevel = s.run(BALSHEET, &json!({
        "General": { "End Date": { "absolute": "2024-03-15" }, "Enable dual columns": false },
        "Display": { "Include accounts with zero total balances": false, "Omit zero balance figures": true,
                     "Parent account amounts include children": false },
    }));
    let t = tables(&multilevel)[0];
    assert!(row(t, "account", "Assets")["cells"][1].is_null(), "the parent is shown, its $0.00 omitted");
    assert!(!labels(t).contains(&"Savings".to_owned()));
}

/// "Levels of Subaccounts" 1 hides the accounts below depth 1, their amounts kept in their parents' (BP:343-344);
/// "Label sections" off drops every title row, and with it the exchange rates, which live in the Exchange Rates title
/// row (BP:537-548, :1072); "Include totals" off drops every section total, the Liability and Equity one too
/// (BP:1011-1012, :1055-1070), but the P&L's Net Income is forced (BP:1207-1213). Accounts unlinked (BP:432-436).
#[test]
fn levels_labels_and_totals() {
    let mut s = extras();
    let result = s.run(BALSHEET, &json!({
        "General": { "End Date": { "absolute": "2024-03-15" }, "Enable dual columns": false },
        "Accounts": { "Levels of Subaccounts": 1 },
        "Display": { "Label sections": false, "Include totals": false, "Display accounts as hyperlinks": false },
        "Commodities": { "Common Currency": true, "Report's currency": "CURRENCY::USD" },
    }));
    let t = tables(&result)[0];
    // A spacer closes each of the five sections, the total and rates sections included (BP:607).
    assert_eq!(labels(t), ["Assets", "", "Liabilities", "", "Equity", "Unrealized Gains", "Retained Earnings", "", "", ""]);
    assert_eq!(amounts(t, "account", "Assets"), one(&["$4,500.00", "Fr.800.00"]));
    let pnl = s.run(PNL, &json!({
        "General": { "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-03-15" }, "Enable dual columns": false },
        "Display": { "Label sections": false, "Include totals": false },
    }));
    let t = tables(&pnl)[0];
    assert_eq!(labels(t), ["Income", "Consulting CHF", "Salary", "", "Expenses", "Groceries", "Rent", "", "Total For Net Income", ""]);
    assert_eq!(amounts(t, "grandTotal", "Total For Net Income"), one(&["Fr.0.00", "$1,720.00"]));
}

/// A common currency at the end of 2023 and at 2024-03-15, latest first, by quarter from 2023-12-31 (one step, then
/// the end). EUR 500 converts at the price nearest each date, 6/5 on 2024-01-01 in both: $600, the original kept
/// beside it (BP:400-411). CHF has no price, so it is never converted (`has-price?`, BP:662-676) and the rates row
/// says it is missing (BP:788-794). Unrealized gains (BP:933-946) are the asset and liability values less their
/// balances: at the end of 2023, by commodity, USD $2,650 - $2,100 = $550, EUR -500, CHF 0, so $550 - $600 = -$50,
/// shown negated. Retained earnings stay unconverted while an income commodity (CHF) has no price (BP:958-963). The
/// Barchart link precedes the table and runs the Net Worth Barchart over the period on the assets and liabilities,
/// in `append-reverse` order (BP:965-976, :1079-1083, :832).
#[test]
fn a_common_currency_converts_what_has_a_price() {
    let mut s = extras();
    let options = json!({
        "General": { "Start Date": { "absolute": "2023-12-31" }, "End Date": { "absolute": "2024-03-15" }, "Period duration": "QuarterDelta",
                     "Enable chart": true },
        "Commodities": { "Common Currency": true, "Report's currency": "CURRENCY::USD" },
    });
    let result = s.run(BALSHEET, &options);
    let all = blocks(&result);
    assert_eq!((all[0]["kind"].clone(), all[0]["text"].clone()), (json!("text"), json!("Barchart")));
    let link = &all[0]["link"]["report"];
    assert_eq!(link["template"], "cbba1696c8c24744848062c7f1cf4a72");
    let want_accounts = json!([SWISS, "a0000000000000000000000000000003", EURO, CHECKING, ASSETS, "a0000000000000000000000000000006",
                               "a0000000000000000000000000000007"]);
    assert_eq!(link["options"]["Accounts"]["Accounts"], want_accounts);
    assert_eq!(link["options"]["General"]["Step Size"], "QuarterDelta");
    assert_eq!(link["options"]["General"]["Price Source"], "pricedb-nearest");
    assert_eq!(link["options"]["General"]["Report's currency"], "CURRENCY::USD");
    assert_eq!(link["options"]["General"]["Start Date"], json!({ "absolute": "2023-12-31T23:59:59Z" }));
    assert_eq!(link["options"]["General"]["Report name"], "Balance Sheet (Multicolumn)", "linked reports take this report's name");
    let chart = s.run(link["template"].as_str().unwrap(), &link["options"]);
    assert_eq!(blocks(&chart)[0]["kind"], "chart", "the link runs");

    let t = all[1];
    assert_eq!(amounts(t, "heading", "Date"), cols(&[&["2024-03-15"], &["2023-12-31"]]));
    assert_eq!(amounts(t, "account", "Euro Account"), cols(&[&["€500.00", "$600.00"], &["€500.00", "$600.00"]]));
    assert_eq!(amounts(t, "account", "Swiss Account"), cols(&[&["Fr.800.00"], &["Fr.800.00"]]));
    assert_eq!(amounts(t, "data", "Unrealized Gains"), cols(&[&["$50.00"], &["$50.00"]]));
    assert_eq!(amounts(t, "data", "Retained Earnings"), cols(&[&["Fr.800.00", "$1,720.00"], &["Fr.800.00", "$0.00"]]));
    assert_eq!(amounts(t, "total", "Total For Asset"), cols(&[&["$4,500.00", "Fr.800.00"], &["$2,850.00", "Fr.800.00"]]));
    assert_eq!(
        amounts(t, "grandTotal", "Total For Liability and Equity"),
        cols(&[&["Fr.800.00", "$4,500.00"], &["Fr.800.00", "$2,850.00"]])
    );
    assert_eq!(amounts(t, "data", "Fr.1.00"), cols(&[&["$ missing"], &["$ missing"]]));
    assert_eq!(amounts(t, "data", "€1.00"), cols(&[&["$1.20"], &["$1.20"]]));
    let html = s.export(BALSHEET, &options);
    ["$ missing", "$1.20", "$600.00", "$50.00", "Barchart"].iter().for_each(|w| assert!(html.contains(w), "{w}"));

    // Without CHF the income has a price, and the retained earnings are one converted figure.
    let no_chf = json!({
        "General": { "End Date": { "absolute": "2024-03-15" }, "Enable dual columns": false },
        "Accounts": { "Accounts": [CHECKING, EURO, SALARY, RENT, GROCERIES] },
        "Commodities": { "Common Currency": true, "Report's currency": "CURRENCY::USD" },
    });
    let t = s.run(BALSHEET, &no_chf);
    assert_eq!(amounts(tables(&t)[0], "data", "Retained Earnings"), one(&["$1,720.00"]));
}

/// The Euro Account alone at the end of 2023 under each price source (OU:133-144, CU:769-887), its amount by the
/// time exchange function (BP:733-736, :755-763) and the rate by the price function (BP:779-781): nearest is 6/5 of
/// 2024-01-01, one second away ($600); before is 11/10 of 2023-03-01 ($550); latest is 6/5; average cost and the
/// weighted average are the one purchase, $550 for EUR 500. With "Show original currency amount" off, the account's
/// amount is converted with no original (BP:529).
#[test]
fn each_price_source() {
    let mut s = extras();
    [("pricedb-nearest", "$600.00", "$1.20"), ("pricedb-before", "$550.00", "$1.10"), ("pricedb-latest", "$600.00", "$1.20"),
     ("average-cost", "$550.00", "$1.10"), ("weighted-average", "$550.00", "$1.10")]
        .iter()
        .for_each(|(source, amount, rate)| {
            let options = |orig: bool| json!({
                "General": { "End Date": { "absolute": "2023-12-31" }, "Enable dual columns": false },
                "Accounts": { "Accounts": [EURO] },
                "Commodities": { "Common Currency": true, "Report's currency": "CURRENCY::USD", "Price Source": source,
                                 "Show original currency amount": orig },
            });
            let result = s.run(BALSHEET, &options(true));
            let t = tables(&result)[0];
            assert_eq!(amounts(t, "account", "Euro Account"), one(&["€500.00", amount]), "{source}");
            assert_eq!(amounts(t, "data", "€1.00"), one(&[rate]), "{source}");
            let t = s.run(BALSHEET, &options(false));
            assert_eq!(amounts(tables(&t)[0], "account", "Euro Account"), one(&[amount]), "{source}");
        });
}

/// Under "pricedb-latest" every P&L column's price date is now (BP:744), so each header ends today, 2024-03-15, and
/// so does each Transaction Report link (BP:1093, :1119): GnuCash's quirk, kept. With a common currency the link
/// shows original amounts in it (BP:1112-1122). The P&L's chart link needs no period: monthly then (BP:1126-1138).
#[test]
fn pricedb_latest_ends_every_pnl_column_today() {
    let mut s = extras();
    let options = json!({
        "General": { "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-03-15" }, "Period duration": "MonthDelta",
                     "Period order is most recent first": false },
        "Commodities": { "Common Currency": true, "Report's currency": "CURRENCY::USD", "Price Source": "pricedb-latest" },
    });
    let result = s.run(PNL, &options);
    let t = tables(&result)[0];
    let header = row(t, "heading", "Period");
    let until: Vec<&Value> = header["cells"].as_array().unwrap()[1..].iter().map(|c| &c["localUntil"]).collect();
    assert_eq!(until, [&json!("2024-03-15"); 3]);
    assert_eq!(amounts(t, "heading", "Period")[1], ["2024-02-01 to 2024-03-15"]);
    let link = &row(t, "account", "Salary")["cells"][1]["link"]["report"];
    assert_eq!(link["template"], "2fe3b9833af044abb929a88d5a59620f");
    assert_eq!(
        link["options"],
        json!({
            "General": { "Report name": "Income Statement (Multicolumn)", "Stylesheet": "Default", "Add options summary": "never",
                         "Start Date": { "absolute": "2024-01-01T00:00:00Z" }, "End Date": { "absolute": "2024-03-15T12:00:00Z" } },
            "Currency": { "Show original currency amount": true, "Common Currency": true, "Report's currency": "CURRENCY::USD" },
            "Display": { "Amount": "double" },
            "Accounts": { "Accounts": [SALARY] },
        })
    );
}

/// The Transaction Report links without a common currency (BP:1109-1124): no original amounts, "Common Currency"
/// false, and the report currency the book's main one, the currency most selected accounts are in (BP:804-812; USD).
/// A tie between CHF and USD, which GnuCash breaks in hash order, goes to the first in the accounts option's order
/// (`design/reports.md` §14); the P&L chart link (BP:1126-1138) takes it too and runs the Income/Expense Chart
/// monthly. A subtotal links its account and descendants; "Display amounts as hyperlinks" off links nothing, and
/// "Display accounts as hyperlinks" off leaves the names unlinked.
#[test]
fn transaction_report_and_chart_links() {
    let mut s = extras();
    let year = json!({ "Start Date": { "absolute": "2023-01-01" }, "End Date": { "absolute": "2023-12-31" }, "Enable dual columns": false });
    let result = s.run(PNL, &json!({ "General": year }));
    let t = tables(&result)[0];
    let link = &row(t, "account", "Income")["cells"][1]["link"]["report"]["options"];
    assert_eq!(link["Accounts"]["Accounts"], json!([INCOME, SALARY, CONSULTING]));
    assert_eq!(link["Currency"], json!({ "Show original currency amount": false, "Common Currency": false, "Report's currency": "CURRENCY::USD" }));
    assert_eq!(link["General"]["End Date"], json!({ "absolute": "2023-12-31T23:59:59Z" }));

    let tie = |first: &str, second: &str| {
        let mut general = year.clone();
        general["Enable chart"] = json!(true);
        json!({ "General": general, "Accounts": { "Accounts": [first, second] } })
    };
    [(CONSULTING, SALARY, "CURRENCY::CHF"), (SALARY, CONSULTING, "CURRENCY::USD")].iter().for_each(|(a, b, want)| {
        let result = s.run(PNL, &tie(a, b));
        let chart = &blocks(&result)[0]["link"]["report"];
        assert_eq!(chart["template"], "80769921e87943adade887b9835a7685");
        assert_eq!(chart["options"]["General"]["Report's currency"], *want);
        assert_eq!(chart["options"]["General"]["Step Size"], "MonthDelta");
        let run = s.run(chart["template"].as_str().unwrap(), &chart["options"]);
        assert_eq!(blocks(&run)[0]["kind"], "chart");
        let trep = &row(tables(&result)[0], "account", "Salary")["cells"][1]["link"]["report"]["options"];
        assert_eq!(trep["Currency"]["Report's currency"], *want);
    });

    let plain = s.run(PNL, &json!({
        "General": year,
        "Display": { "Display amounts as hyperlinks": false, "Display accounts as hyperlinks": false },
    }));
    let t = tables(&plain)[0];
    let salary = row(t, "account", "Salary");
    assert!(salary["cells"][0].get("account").is_none() && salary["cells"][1].get("link").is_none());
}

/// Under "weighted-average" the amounts use the running average to the end of the price date's day, its entry nearest
/// the date (CU:829-865, :741-761), and the rates row the book-wide average to the price date itself (CU:599-607,
/// :772-774): GnuCash's two weighted averages, kept. On `balsheet-pnl`, monthly from 1980-01-02, the first column's
/// price date is 00:00 of 1980-02-01 (BP:747-749). The ten 1970 trades of GBP 100 cost $1,499; the trade of
/// 1980-02-01 (GBP 100 for $170) falls on that day, after 00:00. So the rate is $1.499, while the amounts use the
/// trade's running average, $1,669 / GBP 1,100: Income-GBP's GBP 100 of income is $151.73.
#[test]
fn weighted_average_amounts_and_rates_differ() {
    let mut s = Setup::fixture("balsheet-pnl.gnucash", &test_settings("iso"));
    let result = s.run(PNL, &json!({
        "General": { "Start Date": { "absolute": "1980-01-02" }, "End Date": { "absolute": "1980-03-31" }, "Period duration": "MonthDelta",
                     "Period order is most recent first": false },
        "Accounts": { "Accounts": ["a0000000000000000000000000000012", "a0000000000000000000000000000013"] },
        "Commodities": { "Common Currency": true, "Report's currency": "CURRENCY::USD", "Price Source": "weighted-average" },
    }));
    let t = tables(&result)[0];
    assert_eq!(amounts(t, "heading", "Period")[0], ["1980-01-02 to 1980-02-01"]);
    assert_eq!(amounts(t, "account", "Income-GBP")[0], ["#100.00", "$151.73"]);
    assert_eq!(amounts(t, "data", "#1.00")[0], ["$1.499"]);
}

/// Periods (BP:680-685, :1164-1169; DU:356-379). Weekly through February 2024, oldest first, with the overall period
/// last: five weeks, the last one day, Rent's $1,200 in the first and Groceries' $80 in the third, $1,280 overall.
/// Two weeks with the dates swapped: the list is built from the earlier date, the last column runs to the End Date as
/// given (BP:747), before its start, so its header is text. By year with the overall period, latest first: the overall
/// column first. Monthly from a month's end, the steps keep month ends (DU:299-318): 2024-01-31, 02-29, then the end.
#[test]
fn periods_and_the_overall_column() {
    let mut s = extras();
    let expenses = json!([RENT, GROCERIES]);
    let weekly = s.run(PNL, &json!({
        "General": { "Start Date": { "absolute": "2024-02-01" }, "End Date": { "absolute": "2024-02-29" }, "Period duration": "WeekDelta",
                     "Period order is most recent first": false, "If more than 1 period column, include overall period?": true },
        "Accounts": { "Accounts": expenses },
    }));
    let t = tables(&weekly)[0];
    assert_eq!(
        amounts(t, "heading", "Period"),
        cols(&[&["2024-02-01 to 2024-02-07"], &["2024-02-08 to 2024-02-14"], &["2024-02-15 to 2024-02-21"], &["2024-02-22 to 2024-02-28"],
               &["2024-02-29 to 2024-02-29"], &["2024-02-01 to 2024-02-29"]])
    );
    assert_eq!(
        amounts(t, "total", "Total For Expense"),
        cols(&[&["$1,200.00"], &["$0.00"], &["$80.00"], &["$0.00"], &["$0.00"], &["$1,280.00"]])
    );

    let swapped = s.run(PNL, &json!({
        "General": { "Start Date": { "absolute": "2024-02-29" }, "End Date": { "absolute": "2024-02-01" }, "Period duration": "TwoWeekDelta",
                     "Period order is most recent first": false },
        "Accounts": { "Accounts": expenses },
    }));
    assert_eq!(swapped["title"], "Income Statement (Multicolumn) 2024-02-29 to 2024-02-01");
    let t = tables(&swapped)[0];
    let header = row(t, "heading", "Period");
    assert_eq!(header["cells"][2], json!({ "text": "2024-02-15 to 2024-02-01" }));
    assert_eq!(amounts(t, "total", "Total For Expense"), cols(&[&["$1,200.00"], &["$80.00"]]));

    let yearly = s.run(PNL, &json!({
        "General": { "Start Date": { "absolute": "2023-01-01" }, "End Date": { "absolute": "2024-03-15" }, "Period duration": "YearDelta",
                     "If more than 1 period column, include overall period?": true },
        "Accounts": { "Accounts": expenses },
    }));
    let t = tables(&yearly)[0];
    assert_eq!(
        amounts(t, "heading", "Period"),
        cols(&[&["2023-01-01 to 2024-03-15"], &["2024-01-01 to 2024-03-15"], &["2023-01-01 to 2023-12-31"]])
    );
    assert_eq!(amounts(t, "total", "Total For Expense"), cols(&[&["$2,630.00"], &["$1,280.00"], &["$1,350.00"]]));

    let month_ends = s.run(BALSHEET, &json!({
        "General": { "Start Date": { "absolute": "2024-01-31" }, "End Date": { "absolute": "2024-03-15" }, "Period duration": "MonthDelta",
                     "Period order is most recent first": false },
        "Accounts": { "Accounts": [CHECKING] },
    }));
    let t = tables(&month_ends)[0];
    assert_eq!(amounts(t, "heading", "Date"), cols(&[&["2024-01-31"], &["2024-02-29"], &["2024-03-15"]]));
    assert_eq!(amounts(t, "account", "Checking"), cols(&[&["$5,250.00"], &["$3,900.00"], &["$3,900.00"]]));
}

/// With trading accounts the equity section lists them instead of an unrealized gains row (BP:1040-1047): the EUR
/// trading account's -EUR 500, negated and converted at 6/5, is $600, the USD one's $550 shown -$550, together $50,
/// the gain the plain book shows as unrealized. Equity $1,000 + $1,650 + $50 + $1,720 = $4,420 balances the assets'
/// $4,500 with the liabilities' $80. maxindent is 1 + Trading.CURRENCY.EUR's depth 3 = 4 levels.
#[test]
fn trading_accounts_replace_unrealized_gains() {
    let mut s = Setup::fixture("balsheet-pnl-extras-trading.gnucash", &test_settings("iso"));
    let result = s.run(BALSHEET, &json!({
        "General": { "End Date": { "absolute": "2024-03-15" }, "Enable dual columns": false },
        "Commodities": { "Common Currency": true, "Report's currency": "CURRENCY::USD" },
    }));
    let t = tables(&result)[0];
    assert_eq!(t["columns"][1]["levels"], 4);
    assert!(!labels(t).contains(&"Unrealized Gains".to_owned()));
    assert_eq!(amounts(t, "account", "Trading"), one(&["$50.00"]));
    assert_eq!(row(t, "account", "EUR")["level"], 3);
    assert_eq!(amounts(t, "account", "EUR"), one(&["€500.00", "$600.00"]));
    assert_eq!(amounts(t, "account", "USD"), one(&["-$550.00"]));
    assert_eq!(amounts(t, "total", "Total For Equity"), one(&["Fr.800.00", "$4,420.00"]));
    assert_eq!(amounts(t, "grandTotal", "Total For Liability and Equity"), one(&["Fr.800.00", "$4,500.00"]));
    assert_eq!(amounts(t, "total", "Total For Asset"), one(&["$4,500.00", "Fr.800.00"]));
}

/// A run leaves the Book, its revision, its dirty flag, the file and its directory as they were, on both extras books
/// as XML and as SQLite; two exports are the same bytes, with no script.
#[test]
fn a_run_changes_nothing() {
    let settings = test_settings("iso");
    let books = ["balsheet-pnl-extras.gnucash", "balsheet-pnl-extras-trading.gnucash"]
        .iter()
        .flat_map(|b| [("xml", Setup::fixture(b, &settings)), ("sqlite", Setup::sqlite_fixture(b, &settings))]);
    for (format, mut s) in books {
        let bytes = std::fs::read(s.book()).unwrap();
        let before = s.d.context().book().unwrap().clone();
        let revision = s.d.context().session().unwrap().revision();
        let files = s.files();
        let options = json!({
            "General": { "Start Date": { "absolute": "2023-01-01" }, "End Date": { "absolute": "2024-03-15" }, "Period duration": "QuarterDelta",
                         "Enable chart": true },
            "Commodities": { "Common Currency": true, "Report's currency": "CURRENCY::USD", "Price Source": "average-cost" },
        });
        [BALSHEET, PNL].iter().for_each(|template| {
            s.run(template, &options);
            let (a, b) = (s.export(template, &options), s.export(template, &options));
            assert_eq!(a, b, "{format}");
            assert!(!a.to_ascii_lowercase().contains("<script"), "{format}");
        });
        assert_eq!(s.d.context().book().unwrap(), &before, "{format}");
        assert_eq!(s.d.context().session().unwrap().revision(), revision, "{format}");
        assert_eq!(s.call("book.info", json!({}))["dirty"], false, "{format}");
        assert_eq!(s.files(), files, "{format}: no journal or other file was written");
        assert_eq!(std::fs::read(s.book()).unwrap(), bytes, "{format}: the file is unchanged");
    }
}

