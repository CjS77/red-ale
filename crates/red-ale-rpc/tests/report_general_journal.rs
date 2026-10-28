//! The General Journal (`standard/general-journal.rhai` on `lib/register`) through `report.run` and `report.export`.
//! GnuCash has no test of it, nor of the Register report's journal mode (`design/notes/report-transaction-engine.md`
//! §4), so every case is worked by hand from GJ (`5.17:gnucash/report/reports/standard/general-journal.scm`) and REG
//! (`5.17:gnucash/report/reports/standard/register.scm`), on `fixtures/reports/transaction.gnucash` and on test-data's
//! XML book and GnuCash's SQLite copy of it.
//!
//! The query (GJ:57-71) finds every split not in the void state, by date posted, then in split order, and keeps the
//! first of each transaction (REG:621-622); each transaction then takes a row, a notes row and a row per split
//! (REG:566-598). Within a transaction, splits are in GnuCash's order after a load (value 0 or more first), and the
//! query's split is the first by `xaccSplitOrder`: memo, action, reconcile state, then amount.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod report_runs;

use report_runs::{Fixture, blocks_of, described_options};
use serde_json::{Value, json};

const GENERAL_JOURNAL: &str = "25455562bd234dd0b048ecc5a8af9e43";

/// A cell as `display` or text, `-` when empty, with `<n>` after a cell spanning `n` columns.
fn cell(c: &Value) -> String {
    if c.is_null() {
        return "-".to_owned();
    }
    let s = c["display"].as_str().or_else(|| c["text"].as_str()).unwrap_or("").to_owned();
    c["span"].as_u64().map_or(s.clone(), |n| format!("{s}<{n}>"))
}

/// A table's headings joined by `|`, then each row: `~` first for an alternate row, `=` for a grand-total row, `_`
/// for a rule, then its cells joined by `|`.
fn shown(table: &Value) -> Vec<String> {
    let head = table["columns"].as_array().unwrap().iter().map(|c| c["label"].as_str().unwrap()).collect::<Vec<_>>().join("|");
    let row = |r: &Value| {
        let mark = match (r["role"].as_str(), r["alternate"].as_bool()) {
            (Some("grandTotal"), _) => "=",
            (Some("rule"), _) => "_",
            (_, Some(true)) => "~",
            _ => "",
        };
        format!("{mark}{}", r["cells"].as_array().unwrap().iter().map(cell).collect::<Vec<_>>().join("|"))
    };
    std::iter::once(head).chain(table["rows"].as_array().unwrap().iter().map(row)).collect()
}

/// The only table of a result.
fn the_table(result: &Value) -> &Value {
    let tables = blocks_of(result, "table");
    assert_eq!(tables.len(), 1, "{result:#}");
    tables[0]
}

/// The transaction book's first transactions as the General Journal's defaults show them (GJ:87-103): Date,
/// Description (the memo on split rows), Account (the full name) and two amount columns of the split's amount, with
/// a link to it (REG:243-254). trn2 and trn5 are void and left out (GJ:58-59); trn3's Income split sorts first by
/// amount, but rows follow the transaction's own split order. A notes row follows each transaction row (REG:292-321).
const FIRST_ROWS: [&str; 26] = [
    "Date|Description|Account|Debit|Credit",
    "1969-12-31|desc-3|-|-|-",
    "-|-|-|-|-",
    "~-|-|Root:Asset:Bank|$29.00|-",
    "~-|-|Root:Income|-|$29.00",
    "1970-01-01|desc-1|-|-|-",
    "-|-|-|-|-",
    "~-|memo-3|Root:Expenses|$5.00|-",
    "~-|memo-3|Root:Asset:Bank|-|$5.00",
    "1970-01-10|desc-6|-|-|-",
    "-|notes1<4>",
    "~-|-|Root:Expenses|$11.00|-",
    "~-|-|Root:Liabilities|-|$11.00",
    "1970-02-01|desc-4|-|-|-",
    "-|notes2<4>",
    "~-|memo-1|Root:Expenses|$15.00|-",
    "~-|memo-1|Root:Asset:Bank|-|$15.00",
    "1970-02-10|desc-7|-|-|-",
    "-|notes1<4>",
    "~-|memo-2|Root:Liabilities|$8.00|-",
    "~-|memo-2|Root:Asset:Bank|-|$8.00",
    "1971-02-14|$100bank -> $80expenses + $20wallet|-|-|-",
    "-|multisplit<4>",
    "~-|-|Root:Expenses|$80.00|-",
    "~-|-|Root:Asset:Wallet|$20.00|-",
    "~-|-|Root:Asset:Bank|-|$100.00",
];

/// The defaults on the transaction book: every non-void transaction of the Book, with no account or date option,
/// in date order; no totals, since Totals is off and there is no Value column (REG:534-550).
#[test]
fn general_journal_defaults() {
    let mut f = Fixture::open("transaction", "iso", true);
    let result = f.run(GENERAL_JOURNAL, &json!({}));
    assert_eq!(result["title"], "General Journal");
    assert_eq!(blocks_of(&result, "text").len(), 0);
    let table = the_table(&result);
    let rows = shown(table);
    assert_eq!(rows[..FIRST_ROWS.len()], FIRST_ROWS);
    // Split links on the amounts (REG:248, :254), not on the transaction rows.
    assert!(table["rows"][2]["cells"][3]["link"]["split"].is_string(), "{:#}", table["rows"][2]);
    assert!(table["rows"][0]["cells"].as_array().unwrap().iter().all(|c| c["link"].is_null()));
    // The 57 non-void transactions of the trep tree and the nine of the invoice tree, by date posted.
    let dates: Vec<&str> = table["rows"].as_array().unwrap().iter().filter_map(|r| r["cells"][0]["localDate"].as_str()).collect();
    assert_eq!(dates.len(), 66);
    assert!(dates.windows(2).all(|w| w[0] <= w[1]), "{dates:?}");
    assert!(rows.contains(&"1999-12-31|Closing|-|-|-".to_owned()), "closing transactions are journal entries too");
    assert!(rows.iter().all(|r| !r.contains("desc-2") && !r.contains("desc-5")));
    assert!(rows.last().unwrap().starts_with('~'), "no totals: {:?}", rows.last());
    let html = f.export(GENERAL_JOURNAL, &json!({}));
    report_runs::in_html(&html, &["General Journal", "Root:Asset:Bank", "$100bank -> $80expenses + $20wallet", "notes2"]);
}

/// Every column the Register's options can add, and the totals (REG:166-322, :426-550). Under Num, the transaction
/// row has the num and split rows the action; Lot names the split's lot; Price is value over amount in the
/// transaction's currency; Amount is the amount in the account's commodity and Value the value in the transaction's,
/// signed (single columns); the Balance on the transaction row is the running balance of the split the query found,
/// trn3's Income split (-$29) and trn1's Bank split ($29 - $5). Totals: debits and credits of amounts and values by
/// commodity, newest commodity first, and the change in value; with a single column, each in it as it stands.
#[test]
fn general_journal_every_column_and_the_totals() {
    let mut f = Fixture::open("transaction", "iso", true);
    let options = json!({ "Display": { "Num": true, "Shares": true, "Lot": true, "Price": true, "Value": true,
                                       "Running Balance": true, "Totals": true, "Amount": "single" } });
    let rows = shown(the_table(&f.run(GENERAL_JOURNAL, &options)));
    assert_eq!(rows[..9], [
        "Date|Num|Description|Account|Shares|Lot|Price|Amount|Value|Balance",
        "1969-12-31|trn3|desc-3|-|-|-|-|-|-|-$29.00",
        "-|-|-|-|-|-|-|-|-|-",
        "~-|-|-|Root:Asset:Bank|29.00|-|$1.00|$29.00|$29.00|-",
        "~-|-|-|Root:Income|-29.00|-|$1.00|-$29.00|-$29.00|-",
        "1970-01-01|trn1|desc-1|-|-|-|-|-|-|$24.00",
        "-|-|-|-|-|-|-|-|-|-",
        "~-|-|memo-3|Root:Expenses|5.00|-|$1.00|$5.00|$5.00|-",
        "~-|-|memo-3|Root:Asset:Bank|-5.00|-|$1.00|-$5.00|-$5.00|-",
    ]);
    let at = |prefix: &str| rows.iter().position(|r| r.starts_with(prefix)).unwrap_or_else(|| panic!("no row {prefix}"));
    let invoice = at("1980-09-01|");
    assert_eq!(rows[invoice..invoice + 4], [
        "1980-09-01|0001|cust-1-name|-|-|-|-|-|-|-$6.00",
        "-|-|-|-|-|-|-|-|-|-",
        "~-|Invoice|cust-invoice|Root:A/Receivable|6.00|Invoice 0001|$1.00|$6.00|$6.00|-",
        "~-|Invoice|cust-invoice|Root:Income|-6.00|-|$1.00|-$6.00|-$6.00|-",
    ]);
    // A USD transaction moving GBP: GBP Bank's amount is #6, its value $9, its price $1.50.
    let foreign = at("2000-02-15|");
    assert_eq!(rows[foreign..foreign + 4], [
        "2000-02-15|-|USD 9 to GBP 6|-|-|-|-|-|-|$5.00",
        "-|-|-|-|-|-|-|-|-|-",
        "~-|-|-|Root:Asset:GBP Bank|6.00|-|$1.50|#6.00|$9.00|-",
        "~-|-|-|Root:Asset:USD Bank|-9.00|-|$1.00|-$9.00|-$9.00|-",
    ]);
    // Credits in USD: the trep tree's 3,096 and the invoice tree's 35,000.36; GBP: GBP Bank's 10 and Income-GBP's
    // 12 × 51. Debits differ from credits by what crossed currencies: USD Bank's $14 and GBP Bank's #6 as amounts,
    // #10 and $9 as values.
    assert_eq!(rows[rows.len() - 15..], [
        "_-|-|-|-|-|-|-|-|-|-",
        "=Total Debits|-|-|-|-|-|-|#618.00|-|-",
        "=Total Debits|-|-|-|-|-|-|$38,101.36|-|-",
        "_-|-|-|-|-|-|-|-|-|-",
        "=Total Credits|-|-|-|-|-|-|-#622.00|-|-",
        "=Total Credits|-|-|-|-|-|-|-$38,096.36|-|-",
        "_-|-|-|-|-|-|-|-|-|-",
        "=Total Value Debits|-|-|-|-|-|-|-|#622.00|-",
        "=Total Value Debits|-|-|-|-|-|-|-|$38,096.36|-",
        "_-|-|-|-|-|-|-|-|-|-",
        "=Total Value Credits|-|-|-|-|-|-|-|-#622.00|-",
        "=Total Value Credits|-|-|-|-|-|-|-|-$38,096.36|-",
        "_-|-|-|-|-|-|-|-|-|-",
        "=Value Change|-|-|-|-|-|-|-|#0.00|-",
        "=Value Change|-|-|-|-|-|-|-|$0.00|-",
    ]);
}

/// Two columns put each total in the debit or credit column by its sign, as a positive figure (REG:441-454).
#[test]
fn general_journal_totals_in_two_columns() {
    let mut f = Fixture::open("transaction", "iso", true);
    let rows = shown(the_table(&f.run(GENERAL_JOURNAL, &json!({ "Display": { "Totals": true } }))));
    assert_eq!(rows[rows.len() - 6..], [
        "_-|-|-|-|-",
        "=Total Debits|-|-|#618.00|-",
        "=Total Debits|-|-|$38,101.36|-",
        "_-|-|-|-|-",
        "=Total Credits|-|-|-|#622.00",
        "=Total Credits|-|-|-|$38,096.36",
    ]);
}

/// A balance reverses with the account's sign (REG:286): Income's under the "credit" preference.
#[test]
fn general_journal_balance_follows_the_reversed_accounts() {
    let mut f = Fixture::open_reversing("transaction", "iso", true, "credit");
    let rows = shown(the_table(&f.run(GENERAL_JOURNAL, &json!({ "Display": { "Running Balance": true } }))));
    assert_eq!(rows[0], "Date|Description|Account|Debit|Credit|Balance");
    assert_eq!(rows[1], "1969-12-31|desc-3|-|-|-|$29.00");
}

/// Two transactions on one day keep split order: a transaction entered later with num trn0 comes before trn1
/// (`xaccTransOrder`: the date posted, then the num). Its rows read as any other's.
#[test]
fn general_journal_orders_a_day_by_split_order() {
    let mut f = Fixture::open("transaction", "iso", false);
    let (wallet, expenses) = ("a0000000000000000000000000000006", "a0000000000000000000000000000009");
    f.call("transaction.post", &json!({
        "datePosted": "1970-01-01", "num": "trn0", "description": "desc-0",
        "splits": [{ "account": expenses, "value": "2", "memo": "memo-0" }, { "account": wallet, "value": "-2", "memo": "memo-0" }],
    }));
    let rows = shown(the_table(&f.run(GENERAL_JOURNAL, &json!({ "Display": { "Num": true } }))));
    let day: Vec<&str> = rows.iter().filter(|r| r.starts_with("1970-01-01")).map(String::as_str).collect();
    assert_eq!(day, ["1970-01-01|trn0|desc-0|-|-|-", "1970-01-01|trn1|desc-1|-|-|-"]);
    let at = rows.iter().position(|r| r.starts_with("1970-01-01|trn0")).unwrap();
    assert_eq!(rows[at + 1..at + 4], ["-|-|-|-|-|-", "~-|-|memo-0|Root:Expenses|$2.00|-", "~-|-|memo-0|Root:Asset:Wallet|-|$2.00"]);
}

/// In a book that uses split actions for numbers, the heading is "Num/Action"; the transaction row shows the action
/// of the split the query found, the notes row the transaction's num, and split rows their actions (REG:130-135,
/// :185-193, :301-309). trn1's Expenses split, with action A1, sorts before Bank's A2.
#[test]
fn general_journal_in_a_split_action_book() {
    let mut f = Fixture::open("transaction", "iso", false);
    f.call("book.setOptions", &json!({ "splitActionForNum": true }));
    let found = f.call("transaction.find", &json!({ "query": { "description": { "text": "desc-1", "op": "equals" } } }));
    let trn1 = found["matches"][0]["transaction"].clone();
    let tx = f.call("transaction.get", &json!({ "guid": trn1 }));
    let changes: Vec<Value> = tx["splits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let action = if s["account"] == "a0000000000000000000000000000003" { "A2" } else { "A1" };
            json!({ "guid": s["guid"], "action": action })
        })
        .collect();
    f.call("transaction.update", &json!({ "guid": trn1, "changeSplits": changes }));
    let rows = shown(the_table(&f.run(GENERAL_JOURNAL, &json!({ "Display": { "Num/Action": true } }))));
    assert_eq!(rows[0], "Date|Num/Action|Description|Account|Debit|Credit");
    let at = rows.iter().position(|r| r.starts_with("1970-01-01")).unwrap();
    assert_eq!(rows[at..at + 4], [
        "1970-01-01|A1|desc-1|-|-|-",
        "-|trn1|-|-|-|-",
        "~-|A1|memo-3|Root:Expenses|$5.00|-",
        "~-|A2|memo-3|Root:Asset:Bank|-|$5.00",
    ]);
}

/// The Register's options with GJ's defaults (REG:324-404; GJ:74-106), with no query option. The `__reg` options
/// are internal, listed as the engine's `__trep` option is; the non-journal mode they could ask for is not translated,
/// so setting `__reg/journal` false fails as a script error.
#[test]
fn general_journal_options() {
    let options = described_options(GENERAL_JOURNAL);
    assert_eq!(options, [
        r#"Display/Date "b" "boolean" true"#,
        r#"Display/Num "c" "boolean" false"#,
        r#"Display/Description "d" "boolean" true"#,
        r#"Display/Memo "e" "boolean" true"#,
        r#"Display/Account "g" "boolean" true"#,
        r#"Display/Shares "ha" "boolean" false"#,
        r#"Display/Lot "hb" "boolean" false"#,
        r#"Display/Price "hc" "boolean" false"#,
        r#"Display/Amount "ia" "multichoice" "double""#,
        r#"Display/Value "ib" "boolean" false"#,
        r#"Display/Running Balance "k" "boolean" false"#,
        r#"Display/Totals "l" "boolean" false"#,
        r#"General/Report name "0a" "string" "General Journal""#,
        r#"General/Stylesheet "0b" "multichoice" "Default""#,
        r#"General/Add options summary "0c" "multichoice" "never""#,
        r#"General/Title "a" "string" "General Journal""#,
        r#"__reg/journal "" "internal" true"#,
        r#"__reg/double "" "internal" true"#,
        r#"__reg/debit-string "" "internal" "Debit""#,
        r#"__reg/credit-string "" "internal" "Credit""#,
    ]);
    let mut f = Fixture::open("transaction", "iso", true);
    let refused = f.d.call("report.run", json!({ "report": GENERAL_JOURNAL, "options": { "__reg": { "journal": false } } }));
    let e = refused.expect_err("non-journal mode");
    assert_eq!(e.code, 3000, "{e}");
    assert!(e.data.unwrap()["message"].as_str().unwrap().contains("non-journal mode is not translated"));
}

/// Other `__reg` options take effect: without `double` no notes row follows a transaction row (REG:292); the debit
/// and credit strings head the amount columns (REG:152-155).
#[test]
fn general_journal_without_notes_rows_and_with_other_headings() {
    let mut f = Fixture::open("transaction", "iso", true);
    let options = json!({ "__reg": { "double": false, "debit-string": "Dr", "credit-string": "Cr" } });
    let rows = shown(the_table(&f.run(GENERAL_JOURNAL, &options)));
    assert_eq!(rows[..4], ["Date|Description|Account|Dr|Cr", "1969-12-31|desc-3|-|-|-", "~-|-|Root:Asset:Bank|$29.00|-", "~-|-|Root:Income|-|$29.00"]);
}

/// test-data, a smaller book, as XML and as GnuCash's SQLite copy: the same journal, its 57 non-void transactions
/// with the closing entry of 1977, found on Expenses, whose balance it brings to zero.
#[test]
fn general_journal_on_sqlite_matches_xml() {
    let options = json!({ "Display": { "Num": true, "Totals": true, "Value": true, "Running Balance": true } });
    let run = |sqlite: bool| {
        let mut f = if sqlite { Fixture::open_sqlite("test-data", "iso") } else { Fixture::open("test-data", "iso", true) };
        let mut result = f.run(GENERAL_JOURNAL, &options);
        result["now"] = Value::Null;
        result
    };
    let xml = run(false);
    let table = the_table(&xml);
    let dates = table["rows"].as_array().unwrap().iter().filter(|r| r["cells"][0]["localDate"].is_string()).count();
    assert_eq!(dates, 57);
    let rows = shown(table);
    let closing = rows.iter().position(|r| r.starts_with("1977-12-31")).unwrap();
    assert_eq!(rows[closing..closing + 4], [
        "1977-12-31|-|Closing|-|-|-|-|-|$0.00",
        "-|-|-|-|-|-|-|-|-",
        "~-|-|-|Root:Equity|$111.00|-|$111.00|-|-",
        "~-|-|-|Root:Expenses|-|$111.00|-|$111.00|-",
    ]);
    assert_eq!(run(true), xml);
}

/// A run only reads: the Book stays clean, nothing is written beside it, two exports are the same bytes and hold no
/// script.
#[test]
fn general_journal_leaves_the_book_unchanged() {
    let mut f = Fixture::open("transaction", "iso", false);
    let files = f.files();
    let options = json!({ "Display": { "Totals": true, "Value": true, "Lot": true } });
    f.run(GENERAL_JOURNAL, &options);
    let (a, b) = (f.export(GENERAL_JOURNAL, &options), f.export(GENERAL_JOURNAL, &options));
    assert_eq!(a, b);
    assert!(!a.to_ascii_lowercase().contains("<script"));
    assert_eq!(f.call("book.info", &json!({}))["dirty"], false);
    assert_eq!(f.files(), files);
}
