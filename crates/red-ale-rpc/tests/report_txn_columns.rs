//! The Transaction Breakdown Report (`standard/txn-columns.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/txn-columns.scm`, cited as TC) through `report.run` and `report.export`.
//!
//! GnuCash has no test of this report; `test/test-stress-options.scm` only renders it. Every figure here is
//! hand-worked from the Scheme, citing the lines it follows, on copies of `fixtures/reports/test-data.gnucash` (XML
//! and its SQLite copy) and on books built through the API. test-data is `create-test-data`
//! (`5.17:bindings/guile/test/test-engine-extras.scm:389-480`): every account sits below the top-level ASSET account
//! Root, so Bank is at depth 3 and Income, Expenses, Liabilities and Equity at depth 2; GBP's symbol is `#`; the
//! date format is `locale`, `%m/%d/%y` under C. `env-transfer` gives its first account `-amount` and its second
//! `+amount` (`:199-223`), so "bank expense 5" moves $5 from Bank to Expenses. Split GUIDs are `d` and the number of
//! splits made before, the first account named first (`scripts/report-fixtures/engine_extras.py`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::{Value, json};
use statements::{Setup, blocks, tables, test_settings};

const BREAKDOWN: &str = "03603f0443494a6fa790b417f35d90d4";

/// test-data's accounts (`fixtures/reports/test-data.expected.json`).
const BANK: &str = "a0000000000000000000000000000003";
const LIABILITIES: &str = "a0000000000000000000000000000009";
const EXPENSES: &str = "a0000000000000000000000000000008";

/// The split GUID made `n`th in test-data.
fn split(n: u32) -> String {
    format!("d{n:031x}")
}

fn test_data() -> Setup {
    Setup::fixture("test-data.gnucash", &test_settings("locale"))
}

/// The options of a run on `account` from `from` to `to`, with `more` General options.
fn options(account: &str, from: &str, to: &str, more: Value) -> Value {
    let mut general = json!({ "Account": account, "Start Date": { "absolute": from }, "End Date": { "absolute": to } });
    more.as_object().into_iter().flatten().for_each(|(k, v)| general[k] = v.clone());
    json!({ "General": general })
}

/// A cell as GnuCash's text shows it: a date or monetary cell's display, a `commodities` cell as
/// `[<converted>] <amount>` (TC:232-235), text as it is, an empty cell as `""`.
fn cell_view(cell: &Value) -> String {
    if cell.is_null() {
        return String::new();
    }
    if let Some(lines) = cell["commodities"].as_array() {
        assert_eq!(lines.len(), 1, "one line: {cell}");
        let line = &lines[0];
        return format!("[{}] {}", line["convertedDisplay"].as_str().unwrap(), line["display"].as_str().unwrap());
    }
    cell["display"].as_str().or_else(|| cell["text"].as_str()).unwrap().to_owned()
}

/// The table read back: each column's label and header lines, each row's cells as GnuCash's text shows them, and
/// each row's split links (`""` for a cell with none).
#[derive(Debug)]
struct Breakdown {
    labels: Vec<String>,
    lines: Vec<Vec<String>>,
    rows: Vec<Vec<String>>,
    links: Vec<Vec<String>>,
    table: Value,
}

fn breakdown(result: &Value) -> Breakdown {
    let t = tables(result);
    assert_eq!(t.len(), 1, "{result}");
    let table = t[0].clone();
    let columns = table["columns"].as_array().unwrap();
    let strings = |v: &Value| v.as_array().map(|a| a.iter().map(|s| s.as_str().unwrap().to_owned()).collect()).unwrap_or_default();
    let labels = columns.iter().map(|c| c["label"].as_str().unwrap().to_owned()).collect();
    let header_lines = columns.iter().map(|c| strings(&c["labelLines"])).collect();
    let rows_of = |f: &dyn Fn(&Value) -> String| -> Vec<Vec<String>> {
        let rows = table["rows"].as_array().unwrap();
        rows.iter().map(|r| r["cells"].as_array().unwrap().iter().map(f).collect()).collect()
    };
    let rows = rows_of(&cell_view);
    let split_links = rows_of(&|c: &Value| c["link"]["split"].as_str().unwrap_or("").to_owned());
    Breakdown { labels, lines: header_lines, rows, links: split_links, table }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// The one warning block of a result, as `(title, text)`.
fn warning(result: &Value) -> (String, String) {
    let b = blocks(result);
    assert_eq!(b.len(), 1, "{result}");
    assert_eq!(b[0]["kind"], "warning", "{result}");
    (b[0]["title"].as_str().unwrap().to_owned(), b[0]["text"].as_str().unwrap().to_owned())
}

/// The options of TC:41-56, in GnuCash's order, and the three General options every report has.
#[test]
fn options_are_gnucash_s() {
    let mut s = Setup::with_config(&test_settings("locale"));
    let described = s.call("report.options", json!({ "report": BREAKDOWN }));
    let got: Vec<String> = described["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| format!("{}/{} {} {} {}", o["section"].as_str().unwrap(), o["name"].as_str().unwrap(), o["key"], o["type"], o["default"]))
        .collect();
    let want = [
        r#"General/Report name "0a" "string" "Transaction Breakdown Report""#,
        r#"General/Stylesheet "0b" "multichoice" "Default""#,
        r#"General/Add options summary "0c" "multichoice" "never""#,
        r#"General/Start Date "aa" "date" {"relative":"start-accounting-period"}"#,
        r#"General/End Date "ab" "date" {"relative":"end-accounting-period"}"#,
        r#"General/Account "b" "accountSel" null"#,
        r#"General/Description Filter "c" "string" """#,
        r#"General/Limit for number of columns "d" "numberRange" 10"#,
    ];
    assert_eq!(got, want);
    let template = &s.call("report.list", json!({}))["reports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["guid"] == BREAKDOWN)
        .unwrap()
        .clone();
    assert_eq!(template["menuPath"], json!(["_Experimental"]), "TC:258");
}

/// The Account option has no default (TC:47-48), so the default run finds no transactions and gives the empty-data
/// warning (TC:143-147), as `test-stress-options.scm` renders it. The result is titled with the "Report name"
/// option, where GnuCash's document has no title.
#[test]
fn the_default_run_is_the_warning() {
    let mut s = test_data();
    let result = s.run(BREAKDOWN, &json!({}));
    assert_eq!(result["title"], "Transaction Breakdown Report");
    assert_eq!(warning(&result).0, "No data");
    let named = s.run(BREAKDOWN, &json!({ "General": { "Report name": "Mine" } }));
    assert_eq!(named["title"], "Mine");
    assert_eq!(warning(&named).0, "No data");
    let mut empty = Setup::new_book(&test_settings("locale"));
    assert_eq!(warning(&empty.run(BREAKDOWN, &json!({}))).0, "No data");
}

/// Bank from 1970 to 1971: the four transactions with a Bank split, in posted order (TC:101-114), the 1969 ones
/// outside. Their accounts are Bank, Expenses, Liabilities and Wallet; sorted by `xaccAccountOrder` (all have no
/// code; Bank and Wallet are ASSET, by name) and then by fundamental type, ASSET, LIABILITY, EQUITY, EXPENSE, INCOME
/// (TC:116-139). Bank is three deep, so the header has three rows (TC:160-174). Each cell is the sum of the
/// transaction's splits there (TC:207-244), linked to the first; the total row sums each column's amounts
/// (TC:179-191). The title is the template's name (TC:246).
#[test]
fn bank_in_1970_and_1971() {
    let mut s = test_data();
    let opts = options(BANK, "1970-01-01", "1971-12-31", json!({}));
    let result = s.run(BREAKDOWN, &opts);
    assert_eq!(result["title"], "Transaction Breakdown Report");
    let b = breakdown(&result);
    assert_eq!(b.labels, strings(&["Date", "Num", "Desc", "Root:Asset:Bank", "Root:Asset:Wallet", "Root:Liabilities", "Root:Expenses"]));
    let want_lines: Vec<Vec<String>> = vec![
        vec![],
        vec![],
        vec![],
        strings(&["Root", "Asset", "Bank"]),
        strings(&["Root", "Asset", "Wallet"]),
        strings(&["Root", "Liabilities"]),
        strings(&["Root", "Expenses"]),
    ];
    assert_eq!(b.lines, want_lines);
    let want = [
        ["01/01/70", "trn1", "desc-1", "-$5.00", "", "", "$5.00"],
        ["02/01/70", "trn4", "desc-4", "-$15.00", "", "", "$15.00"],
        ["02/10/70", "trn7", "desc-7", "-$8.00", "", "$8.00", ""],
        ["02/14/71", "", "$100bank -> $80expenses + $20wallet", "-$100.00", "$20.00", "", "$80.00"],
    ];
    assert_eq!(b.rows[..4], want.map(|r| strings(&r)));
    assert_eq!(b.rows[4], strings(&["Total For 01/01/70 to 12/31/71", "-$128.00", "$20.00", "$8.00", "$100.00"]));
    let total = &b.table["rows"][4];
    assert_eq!((total["role"].as_str(), total["cells"][0]["span"].as_u64()), (Some("total"), Some(3)));
    // env-transfer's splits are made in its order: txn 1's Bank split is the first split of the book.
    assert_eq!(b.links[0], strings(&["", "", "", &split(0), "", "", &split(1)]));
    assert_eq!(b.links[3][3..], strings(&[&split(14), &split(16), "", &split(15)]));
    assert!(b.links[4].iter().all(String::is_empty), "the totals have no links");

    let html = s.export(BREAKDOWN, &opts);
    ["Root", "Asset", "Liabilities", "Total For 01/01/70 to 12/31/71", "$100bank -&gt; $80expenses + $20wallet", "-$128.00"]
        .iter()
        .for_each(|t| assert!(html.contains(t), "{t} in the export"));
    assert_eq!(html.matches("<thead>").count(), 1);
    let head = &html[html.find("<thead>").unwrap()..html.find("</thead>").unwrap()];
    assert_eq!(head.matches("<tr>").count(), 3, "three header rows: {head}");
}

/// The 2000 transfers between Bank (USD) and GBP Bank (GBP), `env-transfer-foreign` (TEE:170-197): the first in
/// GBP gives Bank $14 for #10, the second in USD gives GBP Bank #6 for $9. A split whose account's commodity is not
/// the transaction's currency shows `[value] amount` (TC:232-235), here a `commodities` cell; the totals add the
/// amounts in each account's commodity (TC:187-190, :237).
#[test]
fn foreign_rows() {
    let mut s = test_data();
    let b = breakdown(&s.run(BREAKDOWN, &options(BANK, "2000-01-01", "2000-12-31", json!({}))));
    assert_eq!(b.labels[3..], strings(&["Root:Asset:Bank", "Root:Asset:GBP Bank"]));
    assert_eq!(
        b.rows,
        vec![
            strings(&["01/15/00", "", "GBP 10 to USD 14", "[#10.00] $14.00", "-#10.00"]),
            strings(&["02/15/00", "", "USD 9 to GBP 6", "-$9.00", "[$9.00] #6.00"]),
            strings(&["Total For 01/01/00 to 12/31/00", "$5.00", "-#4.00"]),
        ]
    );
    let cell = &b.table["rows"][0]["cells"][3];
    assert_eq!(cell["commodities"][0]["monetary"]["commodity"], "CURRENCY::USD");
    assert_eq!(cell["commodities"][0]["converted"]["commodity"], "CURRENCY::GBP");
    assert!(!cell["link"]["split"].as_str().unwrap().is_empty(), "the amount is linked (TC:233-235)");
}

/// Reverse-balanced accounts are negated, value and amount, totals too (TC:210-212): under `credit` Liabilities
/// shows -$8.00 for the 1970 transfer, and Income's January 1978 receipts are positive.
#[test]
fn reversed_accounts() {
    let mut config = test_settings("locale");
    config["reports"]["reverseBalancedAccounts"] = json!("credit");
    let mut s = Setup::fixture("test-data.gnucash", &config);
    let b = breakdown(&s.run(BREAKDOWN, &options(BANK, "1970-01-01", "1971-12-31", json!({}))));
    assert_eq!(b.rows[2][5], "-$8.00");
    assert_eq!(b.rows[4][3], "-$8.00");
    let b = breakdown(&s.run(BREAKDOWN, &options(BANK, "1978-01-01", "1978-01-31", json!({}))));
    assert_eq!(b.labels[3..], strings(&["Root:Asset:Bank", "Root:Expenses", "Root:Income"]));
    assert_eq!(
        b.rows,
        vec![
            strings(&["01/03/78", "", "$103 income", "$103.00", "", "$103.00"]),
            strings(&["01/09/78", "", "$109 income", "$109.00", "", "$109.00"]),
            strings(&["01/15/78", "", "$22 expense", "-$22.00", "$22.00", ""]),
            strings(&["Total For 01/01/78 to 01/31/78", "$190.00", "$22.00", "$212.00"]),
        ]
    );
}

/// The description filter keeps transactions whose description contains it, case and all (`string-contains`,
/// TC:98-99): "desc" keeps 1970's three and drops the multi-split, and Wallet with it; "Desc" keeps none (the
/// warning); "USD 9" keeps one of 2000's transfers.
#[test]
fn the_description_filter() {
    let mut s = test_data();
    let filtered = |s: &mut Setup, from: &str, to: &str, filter: &str| {
        s.run(BREAKDOWN, &options(BANK, from, to, json!({ "Description Filter": filter })))
    };
    let b = breakdown(&filtered(&mut s, "1970-01-01", "1971-12-31", "desc"));
    assert_eq!(b.labels[3..], strings(&["Root:Asset:Bank", "Root:Liabilities", "Root:Expenses"]));
    let descriptions: Vec<&str> = b.rows[..b.rows.len() - 1].iter().map(|r| r[2].as_str()).collect();
    assert_eq!(descriptions, ["desc-1", "desc-4", "desc-7"]);
    assert_eq!(warning(&filtered(&mut s, "1970-01-01", "1971-12-31", "Desc")).0, "No data");
    let b = breakdown(&filtered(&mut s, "2000-01-01", "2000-12-31", "USD 9"));
    assert_eq!(b.rows.len(), 2);
    assert_eq!(b.rows[0][2], "USD 9 to GBP 6");
}

/// "Limit for number of columns" counts account columns (TC:141, :149-156): Bank in 1970-1971 has four, so a limit
/// of 4 gives the table and 3 the warning, titled with the "Report name" option.
#[test]
fn the_column_limit() {
    let mut s = test_data();
    let limited = |s: &mut Setup, limit: u32| {
        s.run(BREAKDOWN, &options(BANK, "1970-01-01", "1971-12-31", json!({ "Limit for number of columns": limit })))
    };
    assert_eq!(breakdown(&limited(&mut s, 4)).labels.len(), 7);
    let over = limited(&mut s, 3);
    assert_eq!(over["title"], "Transaction Breakdown Report");
    assert_eq!(
        warning(&over),
        (
            "Too many accounts".to_owned(),
            "The number of accounts linked by the transactions found exceeds the limit. Select a different subset of \
             transactions, or increase the limit in the options."
                .to_owned()
        )
    );
    assert_eq!(warning(&limited(&mut s, 1)).0, "Too many accounts");
}

/// Expenses in 1977: the closing transaction (TEE:455-458), which nothing leaves out. Equity sorts before Expenses
/// by fundamental type; both are two deep, so the header has two rows.
#[test]
fn a_two_row_header() {
    let mut s = test_data();
    let opts = options(EXPENSES, "1977-01-01", "1977-12-31", json!({}));
    let b = breakdown(&s.run(BREAKDOWN, &opts));
    assert_eq!(b.lines[3..], [strings(&["Root", "Equity"]), strings(&["Root", "Expenses"])]);
    assert_eq!(b.rows[0], strings(&["12/31/77", "", "Closing", "$111.00", "-$111.00"]));
    let html = s.export(BREAKDOWN, &opts);
    let head = &html[html.find("<thead>").unwrap()..html.find("</thead>").unwrap()];
    assert_eq!(head.matches("<tr>").count(), 2, "{head}");
}

/// A voided transaction is not found (TC:106-107): of the two 1970-01-10 transfers from Liabilities, desc-5 is void.
#[test]
fn a_void_transaction_is_left_out() {
    let mut s = test_data();
    let b = breakdown(&s.run(BREAKDOWN, &options(LIABILITIES, "1970-01-01", "1970-01-31", json!({}))));
    assert_eq!(b.labels[3..], strings(&["Root:Liabilities", "Root:Expenses"]));
    assert_eq!(b.rows[0], strings(&["01/10/70", "trn6", "desc-6", "-$11.00", "$11.00"]));
    assert_eq!(b.rows.len(), 2);
}

/// A book with Bank, Expenses:Auto:Tax, Expenses:Groceries, Expenses:Auto, a EUR account and trading accounts, built
/// through the API.
fn api_book() -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    s.account("Bank", "BANK", None, "CURRENCY::USD");
    s.account("Expenses", "EXPENSE", None, "CURRENCY::USD");
    s.account("Auto", "EXPENSE", Some("Expenses"), "CURRENCY::USD");
    s.account("Tax", "EXPENSE", Some("Expenses:Auto"), "CURRENCY::USD");
    s.account("Groceries", "EXPENSE", Some("Expenses"), "CURRENCY::USD");
    s.account("Euro", "BANK", None, "CURRENCY::EUR");
    s.account("Trading", "TRADING", None, "CURRENCY::USD");
    s.account("EUR", "TRADING", Some("Trading"), "CURRENCY::EUR");
    s.account("USD", "TRADING", Some("Trading"), "CURRENCY::USD");
    s
}

fn guid_of(s: &mut Setup, full_name: &str) -> String {
    s.call("account.get", json!({ "account": full_name }))["guid"].as_str().unwrap().to_owned()
}

/// Two Bank splits in one transaction are summed and linked to the first in the transaction's split order
/// (TC:239-243); the columns sort by `xaccAccountOrder` across the tree, by name (Auto, Groceries, Tax), not depth
/// first (Auto, Tax, Groceries) (TC:116-117, :130). Tax is three deep: three header rows. The num is the
/// transaction's, not the split action, even when the book takes nums from split actions (TC:203).
#[test]
fn summed_splits_and_the_column_order() {
    let mut s = api_book();
    s.call("book.setOptions", json!({ "splitActionForNum": true }));
    let splits = json!([
        { "account": "Bank", "value": "-6", "action": "act" }, { "account": "Bank", "value": "-4" },
        { "account": "Expenses:Auto:Tax", "value": "3" }, { "account": "Expenses:Groceries", "value": "4" },
        { "account": "Expenses:Auto", "value": "3" },
    ]);
    let posted = s.call(
        "transaction.post",
        json!({ "datePosted": "2024-03-01", "currency": "CURRENCY::USD", "num": "42", "description": "shop", "splits": splits }),
    );
    let txn = s.call("transaction.get", json!({ "guid": posted["guid"] }));
    let bank = guid_of(&mut s, "Bank");
    let first_bank = txn["splits"].as_array().unwrap().iter().find(|sp| sp["account"] == bank.as_str()).unwrap()["guid"].clone();
    let b = breakdown(&s.run(BREAKDOWN, &options(&bank, "2024-03-01", "2024-03-01", json!({}))));
    assert_eq!(b.labels[3..], strings(&["Bank", "Expenses:Auto", "Expenses:Groceries", "Expenses:Auto:Tax"]));
    assert_eq!(b.lines[6], strings(&["Expenses", "Auto", "Tax"]));
    assert_eq!(b.rows[0], strings(&["2024-03-01", "42", "shop", "-$10.00", "$3.00", "$4.00", "$3.00"]));
    assert_eq!(b.links[0][3], first_bank.as_str().unwrap());
    assert_eq!(b.rows[1][0], "Total For 2024-03-01 to 2024-03-01");
}

/// TRADING accounts get no column (TC:136-138): a USD transfer to the EUR account with its trading splits shows Bank
/// and Euro only, Euro's €9 bought for $10 as `[$10.00] €9.00`.
#[test]
fn trading_accounts_get_no_column() {
    let mut s = api_book();
    let splits = json!([
        { "account": "Bank", "value": "-10" },
        { "account": "Euro", "value": "10", "amount": "9" },
        { "account": "Trading:EUR", "value": "-10", "amount": "-9" },
        { "account": "Trading:USD", "value": "10" },
    ]);
    s.call("transaction.post", json!({ "datePosted": "2024-03-01", "currency": "CURRENCY::USD", "splits": splits }));
    let bank = guid_of(&mut s, "Bank");
    let b = breakdown(&s.run(BREAKDOWN, &options(&bank, "2024-03-01", "2024-03-31", json!({}))));
    assert_eq!(b.labels[3..], strings(&["Bank", "Euro"]));
    assert_eq!(b.rows[0][3..], strings(&["-$10.00", "[$10.00] €9.00"]));
    assert_eq!(b.rows[1][1..], strings(&["-$10.00", "€9.00"]));
}

/// A copy of `fixtures/<dir>/test-data.gnucash` open for writing.
fn writable(dir: &str) -> Setup {
    let mut s = Setup::with_config(&test_settings("locale"));
    std::fs::copy(format!("{}/../../fixtures/{dir}/test-data.gnucash", env!("CARGO_MANIFEST_DIR")), s.book()).unwrap();
    let book = s.book().display().to_string();
    s.call("book.open", json!({ "path": book }));
    s
}

/// The same run on test-data's XML book and its SQLite copy gives the same result, and a run leaves the Book
/// unchanged, not dirty, with nothing written beside it; two exports are the same bytes.
#[test]
fn xml_and_sqlite_agree_and_a_run_changes_nothing() {
    let opts = options(BANK, "1970-01-01", "2001-12-31", json!({}));
    let mut xml = writable("reports");
    let mut sqlite = writable("sqlite/reports");
    let from_xml = xml.run(BREAKDOWN, &opts);
    assert_eq!(from_xml["sections"], sqlite.run(BREAKDOWN, &opts)["sections"]);
    let b = breakdown(&from_xml);
    let want = ["Root:Asset:Bank", "Root:Asset:GBP Bank", "Root:Asset:Wallet", "Root:Liabilities", "Root:Expenses", "Root:Income"];
    assert_eq!(b.labels[3..], strings(&want));
    assert_eq!(b.rows.len(), 4 + 36 + 2 + 1, "1970-71, 1978's three Bank transfers a month, 2000's two, the total");

    [&mut xml, &mut sqlite].into_iter().for_each(|s| {
        let before = s.d.context().book().unwrap().clone();
        let files = s.files();
        s.run(BREAKDOWN, &opts);
        let (a, b) = (s.export(BREAKDOWN, &opts), s.export(BREAKDOWN, &opts));
        assert_eq!(a, b, "two exports are the same bytes");
        assert_eq!(*s.d.context().book().unwrap(), before, "the Book is unchanged");
        assert_eq!(s.call("book.info", json!({}))["dirty"], false);
        assert_eq!(s.files(), files, "nothing is written beside the book");
    });
}
