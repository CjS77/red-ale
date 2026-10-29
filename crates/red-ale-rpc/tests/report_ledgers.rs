//! The General Ledger (`standard/general-ledger.rhai`) and the Reconciliation Report (`standard/reconcile-report.rhai`),
//! the two thin reports on `lib/trep-engine` besides the Transaction Report, through `report.run` and `report.export`:
//! every check of `fixtures/reports/reconcile.expected.json` (5.17's `test-transaction.scm`, TT:1107-1121) on the XML
//! book and GnuCash's SQLite copy of it, then hand-worked cases (`design/notes/report-transaction-engine.md` §4.6). GL
//! stands for `5.17:gnucash/report/reports/standard/general-ledger.scm`, RR for `…/reconcile-report.scm` and TE for
//! `5.17:gnucash/report/trep-engine.scm`.
//!
//! The reconcile book's Bank: trn2 (31 Dec 1969, +10 from Income, void), trn3 (31 Dec 1969, +29 from Income, cleared
//! 1 Mar 1970), trn1 (1 Jan 1970, -5 to Expenses), trn4 (1 Feb, -15 to Expenses), trn7 (10 Feb, -8 to Expenses,
//! reconciled 1 Mar 1970). Its Liabilities: trn5 (10 Jan, -10 to Expenses, void), trn6 (10 Jan, -11 to Expenses).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod report_runs;

use report_runs::{Fixture, blocks_of, described_options};
use serde_json::{Map, Value, json};

const GENERAL_LEDGER: &str = "2e22929e5c5b4b769f615a815ef0c20f";
const RECONCILIATION: &str = "e45218c6d76f11e7b5ef0800277ef320";

fn read_json(path: &str) -> Value {
    let path = format!("{}/../../fixtures/reports/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The GUID of an account an `expected.json` names.
fn account(e: &Value, name: &str) -> Value {
    let guid = &e["accounts"][name];
    assert!(guid.is_string(), "no account {name}");
    guid.clone()
}

/// A run's options as `report.run` takes them: sections and names nested, accounts as GUIDs, absolute dates as local
/// (UTC) midnight.
fn options(e: &Value, run: &Value) -> Value {
    let mut out = Map::new();
    run["options"].as_object().unwrap().iter().for_each(|(key, value)| {
        let (section, name) = key.split_once('/').unwrap();
        let wire = match value {
            Value::Array(names) => names.iter().map(|n| account(e, n.as_str().unwrap())).collect(),
            Value::Object(m) if m.contains_key("absolute") => {
                json!({ "absolute": format!("{}T00:00:00Z", m["absolute"].as_str().unwrap()) })
            }
            v => v.clone(),
        };
        out.entry(section).or_insert_with(|| json!({}))[name] = wire;
    });
    Value::Object(out)
}

/// The text GnuCash reads from a cell: none for an empty one.
fn texts(cell: &Value) -> Vec<String> {
    let shown = cell["display"].as_str().or_else(|| cell["text"].as_str()).unwrap_or("");
    if shown.is_empty() { vec![] } else { vec![shown.to_owned()] }
}

/// The texts of body row `row` (from 1) of the `table`th table (from 1), or of one column of it.
fn select(result: &Value, s: &Value) -> Vec<String> {
    let tables = blocks_of(result, "table");
    let table = tables[usize::try_from(s["table"].as_u64().unwrap() - 1).unwrap()];
    let row = &table["rows"][usize::try_from(s["row"].as_u64().unwrap() - 1).unwrap()];
    let cells = row["cells"].as_array().unwrap();
    match s["column"].as_u64() {
        Some(c) => texts(&cells[usize::try_from(c - 1).unwrap()]),
        None => cells.iter().flat_map(texts).collect(),
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_owned()).collect()
}

/// Every check of `reconcile.expected.json` on `f`, each expected string also in the exported HTML; the number
/// checked.
fn check_reconcile(f: &mut Fixture, what: &str) -> usize {
    let e = read_json("reconcile.expected.json");
    let runs = e["runs"].as_array().unwrap();
    runs.iter()
        .map(|run| {
            assert_eq!(run["templateGuid"], RECONCILIATION);
            let options = options(&e, run);
            let result = f.run(RECONCILIATION, &options);
            assert_eq!(result["report"]["name"], run["template"]);
            let html = f.export(RECONCILIATION, &options);
            let checks = run["checks"].as_array().unwrap();
            checks.iter().for_each(|c| {
                let name = format!("{what}: \"{}\" (TT:{})", c["name"].as_str().unwrap(), c["line"]);
                let got = select(&result, &c["select"]);
                assert_eq!(got, strings(&c["expected"]), "{name}");
                got.iter()
                    .for_each(|t| assert!(html.contains(t.as_str()), "{name}: {t} is not in the HTML"));
            });
            checks.len()
        })
        .sum()
}

/// TT:1110-1121, on the XML book and on GnuCash's SQLite copy of it.
#[test]
fn reconcile_report_every_check_on_xml_and_sqlite() {
    let e = read_json("reconcile.expected.json");
    let all: usize = e["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["checks"].as_array().unwrap().len())
        .sum();
    assert_eq!(all, 3);
    assert_eq!(check_reconcile(&mut Fixture::open("reconcile", "iso", true), "XML"), all);
    assert_eq!(check_reconcile(&mut Fixture::open_sqlite("reconcile", "iso"), "SQLite"), all);
}

/// A table's headings joined by `|`, then each row's cells joined by `|`: display or text, `-` when empty.
fn shown(table: &Value) -> Vec<String> {
    let cell = |c: &Value| if c.is_null() { "-".to_owned() } else { texts(c).join(",") };
    let head = table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("|");
    let row = |r: &Value| r["cells"].as_array().unwrap().iter().map(cell).collect::<Vec<_>>().join("|");
    std::iter::once(head)
        .chain(table["rows"].as_array().unwrap().iter().map(row))
        .collect()
}

/// The only table of a result, shown.
fn the_table(result: &Value) -> Vec<String> {
    let tables = blocks_of(result, "table");
    assert_eq!(tables.len(), 1, "{result:#}");
    shown(tables[0])
}

/// Options on the reconcile book's Bank and Liabilities, with `more` merged in by section.
fn on_bank_and_liabilities(more: &Value) -> Value {
    let e = read_json("reconcile.expected.json");
    let mut o = json!({ "Accounts": { "Accounts": [account(&e, "Root.Asset.Bank"), account(&e, "Root.Liabilities")] } });
    merge(&mut o, more);
    o
}

fn merge(o: &mut Value, more: &Value) {
    more.as_object().unwrap().iter().for_each(|(section, names)| {
        if o.get(section).is_none() {
            o[section] = json!({});
        }
        names.as_object().unwrap().iter().for_each(|(n, v)| o[section][n] = v.clone());
    });
}

fn march_1970() -> Value {
    json!({ "General": { "Start Date": { "absolute": "1970-03-01T00:00:00Z" }, "End Date": { "absolute": "1970-03-31T00:00:00Z" } } })
}

/// The whole table of TT:1110-1121's run: grouped by reconcile state, voided first (TE:204-212), then by date with no
/// subtotal (RR:38-41); the reconcile date shown only for the reconciled split (TE:1127-1133); Funds In and Funds
/// Out (RR:69-98) each subtotalled on its own, not netted, as neither starts a dual column (TE:1713-1765).
#[test]
fn reconcile_report_table() {
    let mut f = Fixture::open("reconcile", "iso", true);
    let result = f.run(RECONCILIATION, &on_bank_and_liabilities(&march_1970()));
    assert_eq!(result["title"], "Reconciliation Report");
    assert_eq!(
        the_table(&result),
        [
            "|Date|Reconciled Date|Num|Description|Transfer from/to|Funds In|Funds Out",
            "Reconciled|-|-",
            "-|1970-02-10|1970-03-01|trn7|desc-7|Expenses|-|$8.00",
            "-|Total For Reconciled|-|$8.00",
            "Cleared|-|-",
            "-|1969-12-31|-|trn3|desc-3|Income|$29.00|-",
            "-|Total For Cleared|$29.00|-",
            "Unreconciled|-|-",
            "-|1970-01-01|-|trn1|desc-1|Expenses|-|$5.00",
            "-|1970-01-10|-|trn6|desc-6|Expenses|-|$11.00",
            "-|1970-02-01|-|trn4|desc-4|Expenses|-|$15.00",
            "-|Total For Unreconciled|-|$31.00",
            "-|-|-|-|-|-|-|-",
            "Grand Total|$29.00|$39.00",
        ]
    );
}

/// TT:1108-1109, the run on the template's dates, the start of the previous quarter to today. Quarters count from
/// the start of the accounting period, here today, the 15th, so the previous quarter starts on 2023-12-15
/// (`5.17:libgnucash/engine/gnc-option-date.cpp:487-512`, `:519-596`). Only a reconciled split is held to the dates
/// (TE:2566-2571), so trn7, reconciled in 1970, drops out, while the cleared trn3 and the unreconciled splits stay.
#[test]
fn reconcile_report_on_its_default_dates() {
    let mut f = Fixture::open("reconcile", "iso", true);
    let result = f.run(RECONCILIATION, &on_bank_and_liabilities(&json!({})));
    let blocks = blocks_of(&result, "text");
    assert_eq!(blocks[0]["text"], "2023-12-15 to 2024-03-15");
    let rows = the_table(&result);
    assert_eq!(
        rows.iter().filter(|r| !r.starts_with("-|19")).cloned().collect::<Vec<_>>(),
        [
            "|Date|Reconciled Date|Num|Description|Transfer from/to|Funds In|Funds Out",
            "Cleared|-|-",
            "-|Total For Cleared|$29.00|-",
            "Unreconciled|-|-",
            "-|Total For Unreconciled|-|$31.00",
            "-|-|-|-|-|-|-|-",
            "Grand Total|$29.00|$31.00",
        ]
    );
}

/// Void Transactions "Both": voided splits group first (TE:204-212) and show their former amounts (RR:71-74).
#[test]
fn reconcile_report_voided_splits_show_their_former_amounts() {
    let mut f = Fixture::open("reconcile", "iso", true);
    let mut more = march_1970();
    merge(&mut more, &json!({ "Filter": { "Void Transactions": "both" } }));
    let rows = the_table(&f.run(RECONCILIATION, &on_bank_and_liabilities(&more)));
    assert_eq!(
        rows[1..5],
        [
            "Voided|-|-",
            "-|1969-12-31|-|trn2|desc-2|Income|$10.00|-",
            "-|1970-01-10|-|trn5|desc-5|Expenses|-|$10.00",
            "-|Total For Voided|$10.00|$10.00",
        ]
    );
    assert_eq!(rows.last().unwrap(), "Grand Total|$39.00|$49.00");
}

/// Under Common Currency the calculated cells are RR's own: their headings name no currency and their amounts stay
/// in each account's commodity, here USD under a GBP report currency (RR:76-77, :85-98).
#[test]
fn reconcile_report_never_converts() {
    let mut f = Fixture::open("reconcile", "iso", true);
    let mut more = march_1970();
    merge(
        &mut more,
        &json!({ "Currency": { "Common Currency": true, "Report's currency": "CURRENCY::GBP" } }),
    );
    let rows = the_table(&f.run(RECONCILIATION, &on_bank_and_liabilities(&more)));
    assert_eq!(rows[0], "|Date|Reconciled Date|Num|Description|Transfer from/to|Funds In|Funds Out");
    assert_eq!(rows.last().unwrap(), "Grand Total|$29.00|$39.00");
}

/// With no account selected, the no-account warning, then RR's instructions (RR:60-67; TE:2526-2531), titled with
/// the report's name.
#[test]
fn reconcile_report_with_no_account_gives_its_instructions() {
    let mut f = Fixture::open("reconcile", "iso", true);
    let result = f.run(RECONCILIATION, &json!({}));
    assert_eq!(result["title"], "Reconciliation Report");
    let blocks = &result["sections"][0]["blocks"];
    assert_eq!(blocks[0]["kind"], "warning");
    assert_eq!(blocks[0]["title"], "No accounts selected");
    assert_eq!(blocks[1]["kind"], "text");
    let text = blocks[1]["text"].as_str().unwrap();
    assert!(
        text.starts_with("The reconcile report is designed to be similar to the formal reconciliation tool."),
        "{text}"
    );
    assert!(text.ends_with("will apply to the Reconciliation Date.\n\n"), "{text:?}");
    assert_eq!(blocks.as_array().unwrap().len(), 2);
}

/// The options whose defaults the report changes (RR:34-58); "Running Balance", which RR sets and makes internal,
/// does not exist (fact 2 of the note).
#[test]
fn reconcile_report_option_defaults() {
    let options = described_options(RECONCILIATION);
    let has = |line: &str| assert!(options.iter().any(|o| o == line), "{line} in {options:#?}");
    has(r#"Sorting/Primary Key "a" "multichoice" "reconciled-status""#);
    has(r#"Sorting/Secondary Key "f" "multichoice" "date""#);
    has(r#"Sorting/Secondary Subtotal for Date Key "i2" "multichoice" "none""#);
    has(r#"General/Start Date "aa" "date" {"relative":"start-prev-quarter"}"#);
    has(r#"General/End Date "ab" "date" {"relative":"today"}"#);
    has(r#"General/Date Filter "a5" "multichoice" "reconciled""#);
    has(r#"Display/Reconciled Date "a2" "boolean" true"#);
    has(r#"Display/Memo "d" "boolean" false"#);
    has(r#"Display/Account Balance "n" "boolean" false"#);
    assert!(!options.iter().any(|o| o.contains("Running Balance")), "{options:#?}");
}

// The General Ledger, on `transaction.gnucash`: the trep tree of TT. GBP Bank and USD Bank carry codes 01-GBP and
// 02-USD, Expenses EXP-001 (TT:147, :159-175); Income has none.

fn transaction_account(name: &str) -> Value {
    account(&read_json("transaction.expected.json"), name)
}

/// The General Ledger's options on `accounts` from `from` to `to`, with `more` merged in.
fn ledger_on(accounts: &[&str], from: &str, to: &str, more: &Value) -> Value {
    let mut o = json!({
        "Accounts": { "Accounts": accounts.iter().map(|a| transaction_account(a)).collect::<Vec<_>>() },
        "General": { "Start Date": { "absolute": format!("{from}T00:00:00Z") }, "End Date": { "absolute": format!("{to}T00:00:00Z") } },
    });
    merge(&mut o, more);
    o
}

/// The General Ledger's defaults (GL:47-138): grouped by account code with the code in each heading, by date within
/// a group; Date, Description and two amount columns; no balance column, since the "Running Balance" GL asks for is
/// no engine option, and a Grand Total, since neither is "Totals" (fact 2 of the note). The closing transaction
/// (Expenses, 1999-12-31) and the void trn5 are left out; the Grand Total has a line per commodity, USD first as its
/// debit collector met it last (TE:1688-1693).
#[test]
fn general_ledger_defaults() {
    let mut f = Fixture::open("transaction", "iso", true);
    let options = ledger_on(
        &["trep:Root.Asset.GBP Bank", "trep:Root.Asset.USD Bank", "trep:Root.Expenses"],
        "1970-01-01",
        "2000-12-31",
        &json!({}),
    );
    let result = f.run(GENERAL_LEDGER, &options);
    assert_eq!(result["title"], "General Ledger");
    let rows = the_table(&result);
    assert_eq!(
        rows,
        [
            "|Date|Description|Debit|Credit",
            "01-GBP GBP Bank|-|-",
            "-|2000-01-15|GBP 10 to USD 14|-|#10.00",
            "-|2000-02-15|USD 9 to GBP 6|#6.00|-",
            "-|Total For 01-GBP GBP Bank|-|#4.00",
            "02-USD USD Bank|-|-",
            "-|2000-01-15|GBP 10 to USD 14|$14.00|-",
            "-|2000-02-15|USD 9 to GBP 6|-|$9.00",
            "-|Total For 02-USD USD Bank|$5.00|-",
            "EXP-001 Expenses|-|-",
            "-|1970-01-01|desc-1|$5.00|-",
            "-|1970-01-10|desc-6|$11.00|-",
            "-|1970-02-01|desc-4|$15.00|-",
            "-|1971-02-14|$100bank -> $80expenses + $20wallet|$80.00|-",
            "-|Total For EXP-001 Expenses|$111.00|-",
            "-|-|-|-|-",
            "Grand Total|$116.00|-",
            "|-|#4.00",
        ]
    );
    let html = f.export(GENERAL_LEDGER, &options);
    report_runs::in_html(
        &html,
        &["General Ledger", "01-GBP GBP Bank", "Total For EXP-001 Expenses", "$116.00"],
    );
}

/// "Account Balance" on: with GL's sorting the balance is brought forward, so the column reads "Running Balance"
/// with each account's "Balance b/f" (TE:2380-2389, :1635-1648), and GL's Sign Reverses "Credit Accounts" shows
/// Income's -$29 as $29.00 (TE:420-435, :1072-1080); "Use Global Preference" follows the settings, which reverse none.
#[test]
fn general_ledger_running_balance_reverses_credit_accounts() {
    let mut f = Fixture::open("transaction", "iso", true);
    let run = |f: &mut Fixture, signs: Option<&str>| {
        let mut display = json!({ "Account Balance": true });
        if let Some(s) = signs {
            display["Sign Reverses"] = json!(s);
        }
        let options = ledger_on(
            &["trep:Root.Income", "trep:Root.Expenses"],
            "1969-01-01",
            "1970-12-31",
            &json!({ "Display": display }),
        );
        the_table(&f.run(GENERAL_LEDGER, &options))
    };
    assert_eq!(
        run(&mut f, None),
        [
            "|Date|Description|Debit|Credit|Running Balance",
            "Income: Balance b/f|-|-|$0.00",
            "-|1969-12-31|desc-3|-|$29.00|$29.00",
            "-|Total For Income|-|$29.00|-",
            "EXP-001 Expenses: Balance b/f|-|-|$0.00",
            "-|1970-01-01|desc-1|$5.00|-|$5.00",
            "-|1970-01-10|desc-6|$11.00|-|$16.00",
            "-|1970-02-01|desc-4|$15.00|-|$31.00",
            "-|Total For EXP-001 Expenses|$31.00|-|-",
            "-|-|-|-|-|-",
            "Grand Total|$2.00|-|-",
        ]
    );
    assert_eq!(run(&mut f, Some("global"))[2], "-|1969-12-31|desc-3|-|$29.00|-$29.00");
}

/// GL titles every document "General Ledger" (GL:146), whatever the Report name, warnings included, which the
/// engine leaves titled by the Report name.
#[test]
fn general_ledger_titles_its_warnings() {
    let mut f = Fixture::open("transaction", "iso", true);
    let result = f.run(GENERAL_LEDGER, &json!({ "General": { "Report name": "My Ledger" } }));
    assert_eq!(result["title"], "General Ledger");
    let warnings = blocks_of(&result, "warning");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["title"], "No accounts selected");
    assert!(blocks_of(&result, "table").is_empty());
}

/// The options whose defaults the report changes (GL:56-135), with no book: the num option is "Num" and there is
/// no "Trans Number"; "Running Balance" and "Totals" do not exist.
#[test]
fn general_ledger_option_defaults() {
    let options = described_options(GENERAL_LEDGER);
    let has = |line: &str| assert!(options.iter().any(|o| o == line), "{line} in {options:#?}");
    has(r#"Display/Num "b" "boolean" false"#);
    has(r#"Display/Memo "d" "boolean" false"#);
    has(r#"Display/Other Account Name "h5" "boolean" false"#);
    has(r#"Display/Use Full Account Name "f" "boolean" false"#);
    has(r#"Display/Amount "m" "multichoice" "double""#);
    has(r#"Display/Sign Reverses "m1" "multichoice" "credit-accounts""#);
    has(r#"Display/Account Balance "n" "boolean" false"#);
    has(r#"Display/Grand Total "o" "boolean" true"#);
    has(r#"Sorting/Primary Key "a" "multichoice" "account-code""#);
    has(r#"Sorting/Show Account Code "j2" "boolean" true"#);
    has(r#"Sorting/Primary Subtotal for Date Key "e2" "multichoice" "none""#);
    has(r#"Sorting/Secondary Subtotal "i5" "boolean" true"#);
    has(r#"Sorting/Secondary Subtotal for Date Key "i2" "multichoice" "none""#);
    assert!(!options.iter().any(|o| o.starts_with("Display/Trans Number")), "{options:#?}");
    assert!(
        !options
            .iter()
            .any(|o| o.contains("Running Balance") || o.starts_with("Display/Totals")),
        "{options:#?}"
    );
}

/// In a book that uses split actions for numbers, GL turns off "Num/Action" and "Trans Number" (GL:64-69).
#[test]
fn general_ledger_option_defaults_in_a_split_action_book() {
    let mut f = Fixture::open("transaction", "iso", false);
    f.call("book.setOptions", &json!({ "splitActionForNum": true }));
    let described = f.call("report.options", &json!({ "report": GENERAL_LEDGER }));
    let default = |name: &str| {
        let options = described["options"].as_array().unwrap();
        options
            .iter()
            .find(|o| o["section"] == "Display" && o["name"] == name)
            .unwrap_or_else(|| panic!("no {name}"))["default"]
            .clone()
    };
    assert_eq!(default("Num/Action"), json!(false));
    assert_eq!(default("Trans Number"), json!(false));
}

/// GnuCash's SQLite copy of test-data gives the same General Ledger and Reconciliation Report as the XML book.
#[test]
fn ledgers_on_sqlite_match_xml() {
    let accounts = json!([
        "a0000000000000000000000000000003",
        "a0000000000000000000000000000004",
        "a0000000000000000000000000000008"
    ]);
    let options = json!({ "Accounts": { "Accounts": accounts }, "General": { "Start Date": { "absolute": "1969-01-01T00:00:00Z" },
                                                                             "End Date": { "absolute": "2024-12-31T00:00:00Z" } },
                          "Display": { "Account Balance": true } });
    [GENERAL_LEDGER, RECONCILIATION].iter().for_each(|report| {
        let run = |sqlite: bool| {
            let mut f = if sqlite {
                Fixture::open_sqlite("test-data", "iso")
            } else {
                Fixture::open("test-data", "iso", true)
            };
            let mut result = f.run(report, &options);
            result["now"] = Value::Null;
            result
        };
        let xml = run(false);
        assert_eq!(blocks_of(&xml, "table").len(), 1, "{xml:#}");
        assert_eq!(run(true), xml, "{report}");
    });
}

/// A run of either report only reads: the Book stays clean and nothing is written beside it.
#[test]
fn ledgers_leave_the_book_unchanged() {
    let mut f = Fixture::open("reconcile", "iso", false);
    let files = f.files();
    f.run(RECONCILIATION, &on_bank_and_liabilities(&march_1970()));
    f.run(
        GENERAL_LEDGER,
        &on_bank_and_liabilities(&json!({ "Display": { "Account Balance": true } })),
    );
    f.export(RECONCILIATION, &json!({}));
    assert_eq!(f.call("book.info", &json!({}))["dirty"], false);
    assert_eq!(f.files(), files);
}
