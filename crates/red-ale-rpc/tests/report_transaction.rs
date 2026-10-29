//! The Transaction Report (`standard/transaction.rhai` on `lib/trep-engine`) through `report.run` and
//! `report.export`: every assertion of 5.17's `test-transaction.scm` (TT) on `fixtures/reports/transaction.gnucash`
//! (`transaction.expected.json`), then hand-worked cases for what the test does not reach
//! (`design/notes/report-transaction-engine.md` §4.6). TE stands for `5.17:gnucash/report/trep-engine.scm`.
//!
//! GnuCash's test reads the texts of HTML table cells (`test-report-extras.scm:96-109`): table N of the document, row
//! 0 the header, row N the Nth body row, column N the Nth `<td>`, empty cells giving no text. Here the same positions
//! of the result: the Nth table block, the Nth row, the Nth cell (a spanning cell is one `<td>`), its display string
//! or text, each anchor's text in an `anchors` cell. Every expected string must also be in the exported HTML.
//!
//! The CSV export checks (TT:1025-1050) run through the engine with the CSV export type, in red-ale-report's
//! `lib_trep_engine` tests: the Transaction Report declares no export, as in GnuCash (fact 7 of the note).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod report_runs;

use report_runs::{Fixture, blocks_of};
use serde_json::{Map, Value, json};

const TEMPLATE: &str = "2fe3b9833af044abb929a88d5a59620f";

/// `fixtures/reports/transaction.expected.json`.
fn expected() -> Value {
    let path = format!("{}/../../fixtures/reports/transaction.expected.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The GUID of an account the file names as `tree:full name`.
fn account(e: &Value, name: &str) -> Value {
    let guid = &e["accounts"][name];
    assert!(guid.is_string(), "no account {name}");
    guid.clone()
}

/// A run's options as `report.run` takes them: sections and names nested, accounts as GUIDs, absolute dates as local
/// (UTC) midnight, currencies as commodity ids.
fn options(e: &Value, run: &Value) -> Value {
    let mut out = Map::new();
    run["options"].as_object().unwrap().iter().for_each(|(key, value)| {
        let (section, name) = key.split_once('/').unwrap();
        let wire = match value {
            Value::Array(names) => names.iter().map(|n| account(e, n.as_str().unwrap())).collect(),
            Value::Object(m) if m.contains_key("absolute") => {
                json!({ "absolute": format!("{}T00:00:00Z", m["absolute"].as_str().unwrap()) })
            }
            Value::String(s) if name == "Report's currency" => json!(format!("CURRENCY::{s}")),
            v => v.clone(),
        };
        out.entry(section).or_insert_with(|| json!({}))[name] = wire;
    });
    Value::Object(out)
}

/// The texts GnuCash reads from a cell: none for an empty one, each anchor's of an `anchors` cell.
fn texts(cell: &Value) -> Vec<String> {
    if let Some(anchors) = cell["anchors"].as_array() {
        return anchors.iter().map(|a| a["text"].as_str().unwrap().to_owned()).collect();
    }
    let shown = cell["display"].as_str().or_else(|| cell["text"].as_str()).unwrap_or("");
    if shown.is_empty() { vec![] } else { vec![shown.to_owned()] }
}

/// The element at a 1-based position, negative from the end.
fn nth<T>(items: &[T], n: i64) -> Option<&T> {
    let i = if n < 0 {
        usize::try_from(i64::try_from(items.len()).unwrap() + n).ok()?
    } else {
        usize::try_from(n - 1).ok()?
    };
    items.get(i)
}

/// The texts a `{ table, row, column, dropFirst }` selection reads.
fn select(tables: &[&Value], s: &Value) -> Vec<String> {
    let table = nth(tables, s["table"].as_i64().unwrap()).unwrap();
    let rows = table["rows"].as_array().unwrap();
    let column = s["column"].as_i64();
    let row_texts = |r: &Value| -> Vec<String> {
        let cells = r["cells"].as_array().unwrap();
        match column {
            Some(c) => nth(cells, c).map(texts).unwrap_or_default(),
            None => cells.iter().flat_map(texts).collect(),
        }
    };
    let got: Vec<String> = match s["row"].as_i64() {
        Some(0) => {
            let columns = table["columns"].as_array().unwrap();
            let labels: Vec<String> = columns.iter().map(|c| c["label"].as_str().unwrap().to_owned()).collect();
            match column {
                Some(c) => nth(&labels, c).into_iter().filter(|l| !l.is_empty()).cloned().collect(),
                None => labels.into_iter().filter(|l| !l.is_empty()).collect(),
            }
        }
        Some(n) => nth(rows, n).map(row_texts).unwrap_or_default(),
        None => rows.iter().flat_map(row_texts).collect(),
    };
    let drop = usize::from(s["dropFirst"].as_bool().unwrap_or(false));
    got.into_iter().skip(drop).collect()
}

/// The texts of the anchors in each row's last cell (`// tr // (td -1) // a // *text*`).
fn last_cell_links(table: &Value) -> Vec<String> {
    let rows = table["rows"].as_array().unwrap();
    rows.iter()
        .filter_map(|r| r["cells"].as_array().unwrap().last())
        .filter(|c| c["link"].is_object())
        .flat_map(texts)
        .collect()
}

/// The number of nodes an `xpath` selection names, or its texts.
fn xpath(tables: &[&Value], path: &str) -> Vec<String> {
    let table = || *tables.first().unwrap();
    let count = |n: usize| vec![String::new(); n];
    if path == "(// a // *text*)" {
        let linked = |c: &Value| {
            if c["link"].is_object() {
                texts(c)
            } else {
                c["anchors"].as_array().map_or_else(Vec::new, |_| texts(c))
            }
        };
        let cells = tables
            .iter()
            .flat_map(|t| t["rows"].as_array().unwrap())
            .flat_map(|r| r["cells"].as_array().unwrap());
        return cells.flat_map(linked).collect();
    }
    if path == "(// (table 1) // tr // (td -1) // a // *text*)" {
        return last_cell_links(table());
    }
    if path == "(// (table 1) // (tr 1) // th)" {
        return count(table()["columns"].as_array().unwrap().len());
    }
    let n: i64 = path
        .strip_prefix("(// (table 1) // (tr ")
        .and_then(|rest| rest.strip_suffix(") // td)"))
        .unwrap_or_else(|| panic!("unknown xpath {path}"))
        .parse()
        .unwrap();
    let rows = table()["rows"].as_array().unwrap();
    count(nth(rows, n).map_or(0, |r| r["cells"].as_array().unwrap().len()))
}

/// A figure read as GnuCash's test reads it, keeping only digits, `-` and (with `point`) `.`, scaled to cents.
fn cents(s: &str, point: bool) -> i64 {
    let kept: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-' || (point && *c == '.'))
        .collect();
    let (whole, frac) = kept.split_once('.').unwrap_or((&kept, ""));
    let negative = whole.starts_with('-');
    let magnitude = whole.trim_start_matches('-').parse::<i64>().unwrap_or(0) * 100 + format!("{frac:0<2}")[..2].parse::<i64>().unwrap();
    if negative { -magnitude } else { magnitude }
}

/// Expected strings. The file's full names use `.`, the engine's default separator in GnuCash's test environment
/// (`5.17:libgnucash/engine/Account.cpp:58`); red-ale's is `:`, GnuCash's application default, so they are compared
/// with `:`.
fn strings(v: &Value) -> Vec<String> {
    let colons = |s: &str| s.replace("Root.Asset.", "Root:Asset:").replace("Root.", "Root:");
    v.as_array().unwrap().iter().map(|s| colons(s.as_str().unwrap())).collect()
}

/// Check a relation GnuCash states in Scheme rather than as strings.
fn relation(what: &str, c: &Value, got: &[Vec<String>]) {
    let rel = c["relation"].as_str().unwrap();
    let flat: Vec<String> = got.concat();
    if rel.starts_with("Each text, trimmed, is qof-print-date of 1970-03-01") {
        assert!(
            !flat.is_empty() && flat.iter().all(|t| t.trim() == "1970-03-01"),
            "{what}: {flat:?}"
        );
    } else if rel.starts_with("All texts of column") {
        assert!(flat.len() > 1 && flat.iter().all(|t| *t == flat[0]), "{what}: {flat:?}");
    } else if rel.contains("sum to 0") {
        assert_eq!(flat.iter().map(|t| cents(t, true)).sum::<i64>(), 0, "{what}: {flat:?}");
    } else if rel.starts_with("For each text, whether it contains no '-'") {
        let want: Vec<bool> = c["expected"].as_array().unwrap().iter().map(|b| b.as_bool().unwrap()).collect();
        assert_eq!(flat.iter().map(|t| !t.contains('-')).collect::<Vec<_>>(), want, "{what}: {flat:?}");
        assert_eq!(flat, strings(&c["implied"]), "{what}");
    } else if rel.starts_with("Read keeping only digits, '-' and '.'") {
        assert_eq!(flat, strings(&c["implied"]), "{what}");
    } else if rel == "The two selections are equal" {
        assert!(got[0].len() == 1 && got[0] == got[1], "{what}: {got:?}");
        if c["implied"].is_array() {
            assert_eq!(got[0], strings(&c["implied"]), "{what}");
        }
    } else if rel.contains("first + second = third") {
        let n: Vec<i64> = got.iter().map(|g| cents(&g.concat(), false)).collect();
        assert!(got.iter().all(|g| g.len() == 1) && n[0] + n[1] == n[2], "{what}: {got:?}");
    } else if rel == "The texts are in order by string<?" {
        assert!(flat.len() > 1 && flat.windows(2).all(|w| w[0] <= w[1]), "{what}: {flat:?}");
    } else {
        panic!("{what}: unknown relation {rel}");
    }
}

/// Run one run of the file through `report.run` and `report.export` and check each of its assertions; the number
/// checked. The CSV run is left to the engine's tests.
fn check_run(f: &mut Fixture, e: &Value, run: &Value) -> usize {
    let id = run["id"].as_str().unwrap();
    if run["exportType"] == "csv" {
        return 0;
    }
    let options = options(e, run);
    let result = f.run(TEMPLATE, &options);
    assert_eq!(result["report"]["name"], run["template"], "{id}");
    let html = f.export(TEMPLATE, &options);
    let tables = blocks_of(&result, "table");
    let checks = run["checks"].as_array().unwrap();
    checks.iter().for_each(|c| {
        let what = format!("{id}: \"{}\" (TT:{})", c["name"].as_str().unwrap(), c["line"]);
        let s = &c["select"];
        if c["expected"] == json!([]) {
            assert!(tables.is_empty(), "{what}: no table");
            assert_eq!(blocks_of(&result, "warning").len(), 1, "{what}: a warning");
            return;
        }
        let got: Vec<Vec<String>> = if let Some(cells) = s["cells"].as_array() {
            cells.iter().map(|sel| select(&tables, sel)).collect()
        } else if let Some(path) = s["xpath"].as_str() {
            vec![xpath(&tables, path)]
        } else {
            vec![select(&tables, s)]
        };
        if let Some(n) = c["count"].as_u64() {
            assert_eq!(got[0].len() as u64, n, "{what}: {got:?}");
        } else if let Some(n) = c["countAtLeast"].as_u64() {
            assert!(got[0].len() as u64 >= n, "{what}: {got:?}");
        } else if c["relation"].is_string() {
            relation(&what, c, &got);
        } else {
            assert_eq!(got[0], strings(&c["expected"]), "{what}");
            got[0]
                .iter()
                .for_each(|w| assert!(html.contains(&escaped(w)), "{what}: {w} is not in the HTML"));
        }
    });
    checks.len()
}

fn escaped(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Every run of `transaction.expected.json` but the CSV one, each assertion a named check.
#[test]
fn test_transaction_every_assertion() {
    let e = expected();
    let mut f = Fixture::open("transaction", "iso", true);
    let runs = e["runs"].as_array().unwrap();
    let checked: usize = runs.iter().map(|run| check_run(&mut f, &e, run)).sum();
    let all: usize = runs.iter().map(|r| r["checks"].as_array().unwrap().len()).sum();
    let csv = runs.iter().find(|r| r["exportType"] == "csv").unwrap()["checks"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(checked + csv, all);
}

/// TT:99-102: the template's defaults select no account, so the no-account warning, titled with the report's name.
#[test]
fn test_transaction_defaults_warn_of_no_account() {
    let mut f = Fixture::open("transaction", "iso", true);
    let result = f.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], "Transaction Report");
    let warning = blocks_of(&result, "warning");
    assert_eq!(warning.len(), 1);
    assert_eq!(warning[0]["title"], "No accounts selected");
    assert!(blocks_of(&result, "table").is_empty());
}

// Hand-worked cases (§4.6 of the note). The splits, filters, sort keys and date sources at split level are
// red-ale-report's `lib_trep_engine` tests; these read the cells.

/// A table's headings joined by `|`, then each row's cells: display or text, anchors joined by `,`, `-` when empty.
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
    let rows = table["rows"].as_array().unwrap().iter().map(row);
    std::iter::once(head).chain(rows).collect()
}

/// The last table of a run, shown.
fn run_shown(f: &mut Fixture, options: &Value) -> Vec<String> {
    let result = f.run(TEMPLATE, options);
    shown(blocks_of(&result, "table").last().unwrap())
}

/// The account of `transaction.gnucash` named `tree:full name`, as a GUID.
fn acct(name: &str) -> Value {
    account(&expected(), name)
}

/// `options` on `accounts` from `from` to `to`, with Table for Exporting so labels do not span.
fn on(accounts: &[&str], from: &str, to: &str, more: &Value) -> Value {
    let mut o = json!({
        "Accounts": { "Accounts": accounts.iter().map(|a| acct(a)).collect::<Vec<_>>() },
        "General": { "Start Date": { "absolute": format!("{from}T00:00:00Z") }, "End Date": { "absolute": format!("{to}T00:00:00Z") },
                     "Table for Exporting": true },
    });
    more.as_object().unwrap().iter().for_each(|(section, names)| {
        names.as_object().unwrap().iter().for_each(|(n, v)| o[section][n] = v.clone());
    });
    o
}

/// The GUID of the transaction whose description is `desc`.
fn transaction(f: &mut Fixture, desc: &str) -> String {
    let found = f.call(
        "transaction.find",
        &json!({ "query": { "description": { "text": desc, "op": "equals" } } }),
    );
    found["matches"][0]["transaction"].as_str().unwrap().to_owned()
}

/// The left columns TT never shows (TE:1103-1244), on GBP Bank in 2000: Date Entered and Reconciled Date (empty: no
/// split is reconciled), Shares (the amount, two places) and Price (value over amount in the transaction's currency:
/// 1 in the GBP transaction, $9 / #6 in the USD one), and the other account's name.
#[test]
fn test_transaction_entered_date_shares_and_price() {
    let mut f = Fixture::open("transaction", "iso", true);
    let display = json!({ "Display": { "Date Entered": true, "Reconciled Date": true, "Num": false, "Memo": false, "Shares": true,
                                       "Price": true, "Amount": "single", "Grand Total": false },
                          "Sorting": { "Primary Subtotal": false } });
    let rows = run_shown(&mut f, &on(&["trep:Root.Asset.GBP Bank"], "2000-01-01", "2000-12-31", &display));
    assert_eq!(
        rows[0],
        "Date|Date Entered|Reconciled Date|Description|Transfer from/to|Shares|Price|Amount"
    );
    // Entered at the fixture's pinned now.
    assert_eq!(
        rows[1..],
        [
            "2000-01-15|2024-03-15|-|GBP 10 to USD 14|USD Bank|-10.00|#1.00|-#10.00",
            "2000-02-15|2024-03-15|-|USD 9 to GBP 6|USD Bank|6.00|$1.50|#6.00"
        ]
    );
}

/// The Link column (TE:1220-1234): "L" where the transaction has a document, linked to it under Enable Links; the
/// export shows the text only, with no path.
#[test]
fn test_transaction_document_links() {
    let mut f = Fixture::open("transaction", "iso", false);
    let trn1 = transaction(&mut f, "desc-1");
    f.call(
        "transaction.update",
        &json!({ "guid": trn1, "docLink": "file:///home/user/receipt.pdf" }),
    );
    let links = |f: &mut Fixture, enable: bool| {
        let options = on(
            &["trep:Root.Asset.Bank"],
            "1970-01-01",
            "1970-01-31",
            &json!({ "Display": { "Link": true, "Enable Links": enable, "Amount": "single", "Grand Total": false },
                                  "Sorting": { "Primary Subtotal": false } }),
        );
        let result = f.run(TEMPLATE, &options);
        let table = blocks_of(&result, "table")[0].clone();
        let html = f.export(TEMPLATE, &options);
        (shown(&table), table["rows"][0]["cells"][5].clone(), html)
    };
    let (rows, cell, html) = links(&mut f, true);
    assert_eq!(
        rows,
        [
            "Date|Num|Description|Memo/Notes|Transfer from/to||Amount",
            "1970-01-01|trn1|desc-1|memo-3|Expenses|L|-$5.00"
        ]
    );
    assert_eq!(cell["link"], json!({ "docLink": "file:///home/user/receipt.pdf" }));
    assert!(!html.contains("receipt.pdf"), "the page carries no path");
    let (rows, cell, _) = links(&mut f, false);
    assert_eq!(rows[1], "1970-01-01|trn1|desc-1|memo-3|Expenses|L|-$5.00");
    assert!(cell["link"].is_null(), "{cell}");
}

/// In a book that uses split actions for numbers, the Num cell is the split's action, and with Trans Number
/// "<action>/<num>" under "Num/T-Num" (TE:1139-1155; `gnc-get-num-action`).
#[test]
fn test_transaction_split_action_numbers() {
    let mut f = Fixture::open("transaction", "iso", false);
    f.call("book.setOptions", &json!({ "splitActionForNum": true }));
    let trn1 = transaction(&mut f, "desc-1");
    let tx = f.call("transaction.get", &json!({ "guid": trn1 }));
    let bank = acct("trep:Root.Asset.Bank");
    let split = tx["splits"].as_array().unwrap().iter().find(|s| s["account"] == bank).unwrap()["guid"].clone();
    f.call(
        "transaction.update",
        &json!({ "guid": trn1, "changeSplits": [{ "guid": split, "action": "A1" }] }),
    );
    let options = |more: &Value| {
        on(
            &["trep:Root.Asset.Bank"],
            "1969-12-31",
            "1970-01-31",
            &json!({
                "Display": { "Num/Action": true, "Trans Number": more["tn"], "Description": false, "Memo": false, "Amount": "single",
                             "Other Account Name": false, "Grand Total": false },
                "Sorting": { "Primary Subtotal": false },
            }),
        )
    };
    let rows = run_shown(&mut f, &options(&json!({ "tn": true })));
    assert_eq!(
        rows,
        ["Date|Num/T-Num|Amount", "1969-12-31|/trn3|$29.00", "1970-01-01|A1/trn1|-$5.00"]
    );
    let rows = run_shown(&mut f, &options(&json!({ "tn": false })));
    assert_eq!(rows, ["Date|Num|Amount", "1969-12-31|-|$29.00", "1970-01-01|A1|-$5.00"]);
}

/// The Invoice cell (TE:1036-1049) of a payment that pays two invoices at once: its A/R splits sit in the two
/// invoices' lots, and each new invoice goes in front, so the cell lists them in reverse order of the splits.
#[test]
fn test_transaction_two_invoices_in_one_cell() {
    let mut f = Fixture::open("transaction", "iso", false);
    let invoices = f.call("invoice.list", &json!({ "type": "invoice", "posted": true }))["invoices"].clone();
    let pick = |id: &str| invoices.as_array().unwrap().iter().find(|i| i["id"] == id).unwrap().clone();
    let (first, second) = (pick("0001"), pick("0002"));
    let owner = json!({ "type": first["owner"]["kind"], "id": first["owner"]["guid"] });
    let paid = f.call(
        "payment.process",
        &json!({
            "owner": owner, "account": acct("invoice:Root.A/Receivable"), "transferAccount": acct("invoice:Root.Asset.Bank"),
            "amount": "12", "date": "1980-09-20", "applyTo": [first["guid"], second["guid"]],
        }),
    );
    // The payment's A/R splits in split order, each named by the invoice whose lot holds it.
    let tx = f.call("transaction.get", &json!({ "guid": paid["transaction"] }));
    let by_lot = |lot: &Value| {
        [&first, &second]
            .into_iter()
            .find(|i| i["postedLot"] == *lot)
            .map(|i| i["id"].as_str().unwrap().to_owned())
    };
    let in_split_order: Vec<String> = tx["splits"].as_array().unwrap().iter().filter_map(|s| by_lot(&s["lot"])).collect();
    assert_eq!(in_split_order, ["0001", "0002"]);
    let options = on(
        &["invoice:Root.Asset.Bank"],
        "1980-09-20",
        "1980-09-20",
        &json!({ "Display": { "Invoice": true, "Amount": "single", "Grand Total": false, "Memo": false },
                              "Sorting": { "Primary Subtotal": false } }),
    );
    let result = f.run(TEMPLATE, &options);
    let table = blocks_of(&result, "table")[0].clone();
    let rows = shown(&table);
    assert_eq!(rows[0], "Date|Num|Description|Invoice|Transfer from/to|Amount");
    assert_eq!(rows[1], "1980-09-20|-|cust-1-name|0002,0001|Split Transaction|$12.00");
    let anchors = &table["rows"][0]["cells"][3]["anchors"];
    assert_eq!(anchors[0]["link"], json!({ "invoice": second["guid"] }));
}

/// "Balance b/f" under Common Currency (TE:1635-1648): GBP Bank's -#10 before February 2000, at the price nearest
/// local noon of its first split's day (2000-02-15: $9 for #6), so -$15.00; the running balance of that split, -#4,
/// is -$6.00, and in the original currency -#4.00.
#[test]
fn test_transaction_balance_brought_forward_in_common_currency() {
    let mut f = Fixture::open("transaction", "iso", true);
    let options = on(
        &["trep:Root.Asset.GBP Bank"],
        "2000-02-01",
        "2000-12-31",
        &json!({
            "Display": { "Account Balance": true, "Amount": "single", "Grand Total": false, "Num": false, "Memo": false,
                         "Description": false, "Other Account Name": false },
            "Currency": { "Common Currency": true, "Show original currency amount": true },
        }),
    );
    let rows = run_shown(&mut f, &options);
    // An indenting column comes first; the subtotal's second line, in GBP, has no label.
    assert_eq!(
        rows,
        [
            "|Date|Amount (USD)|Running Balance (USD)|Amount|Running Balance",
            "GBP Bank: Balance b/f|-|-|-$15.00|-|-#10.00",
            "-|2000-02-15|$9.00|-$6.00|#6.00|-#4.00",
            "-|Total For GBP Bank|$9.00|-|-|-",
            "-||-|-|#6.00|-",
        ]
    );
}

/// `bal-bf` (TE:2380-2389): each condition it needs, broken alone, turns "Running Balance" back into "Account
/// Balance" with no "Balance b/f".
#[test]
fn test_transaction_balance_brought_forward_conditions() {
    let mut f = Fixture::open("transaction", "iso", true);
    let heading = |f: &mut Fixture, more: &Value| {
        let mut m = json!({ "Display": { "Account Balance": true, "Amount": "single" } });
        more.as_object().unwrap().iter().for_each(|(s, names)| {
            names.as_object().unwrap().iter().for_each(|(n, v)| m[s][n] = v.clone());
        });
        let rows = run_shown(f, &on(&["trep:Root.Asset.Bank"], "2024-01-01", "2024-12-31", &m));
        (rows[0].rsplit('|').next().unwrap().to_owned(), rows[1].contains("Balance b/f"))
    };
    assert_eq!(heading(&mut f, &json!({})), ("Running Balance".to_owned(), true));
    let breakers = [
        json!({ "Display": { "Detail Level": "multi-line" } }),
        json!({ "General": { "Date Filter": "entered" } }),
        json!({ "Filter": { "Transaction Filter": "income" } }),
        json!({ "Filter": { "Reconciled Status": "unreconciled" } }),
        json!({ "Filter": { "Void Transactions": "both" } }),
        json!({ "Sorting": { "Primary Key": "date" } }),
        json!({ "Sorting": { "Secondary Key": "description" } }),
        json!({ "Sorting": { "Primary Subtotal": false } }),
        json!({ "Sorting": { "Secondary Sort Order": "descend" } }),
    ];
    breakers
        .iter()
        .for_each(|b| assert_eq!(heading(&mut f, b), ("Account Balance".to_owned(), false), "{b}"));
}

/// A run only reads: the Book stays clean and unchanged, nothing is written beside it, two exports are the same
/// bytes and hold no script.
#[test]
fn test_transaction_leaves_the_book_unchanged() {
    let mut f = Fixture::open("transaction", "iso", false);
    let files = f.files();
    let options = on(
        &["trep:Root.Asset.Bank", "trep:Root.Asset.GBP Bank"],
        "1969-01-01",
        "2024-12-31",
        &json!({ "Display": { "Subtotal Table": true, "Account Balance": true },
                              "Currency": { "Common Currency": true, "Show original currency amount": true } }),
    );
    f.run(TEMPLATE, &options);
    let (a, b) = (f.export(TEMPLATE, &options), f.export(TEMPLATE, &options));
    assert_eq!(a, b);
    assert!(!a.to_ascii_lowercase().contains("<script"));
    assert_eq!(f.call("book.info", &json!({}))["dirty"], false);
    assert_eq!(f.files(), files);
}

/// The SQLite path: GnuCash's SQLite copy of test-data (`fixtures/sqlite/reports/test-data.gnucash`; the transaction
/// book has none, GnuCash cannot save it as SQLite) gives the same result as the XML book, for the base options and
/// with every display column, the Subtotal Table and running totals.
#[test]
fn test_transaction_on_sqlite_matches_xml() {
    let accounts = json!([
        "a0000000000000000000000000000003",
        "a0000000000000000000000000000004",
        "a0000000000000000000000000000008"
    ]);
    let base = json!({ "Accounts": { "Accounts": accounts }, "General": { "Start Date": { "absolute": "1969-01-01T00:00:00Z" },
                                                                          "End Date": { "absolute": "2024-12-31T00:00:00Z" } } });
    let mut all = base.clone();
    all["Display"] = json!({ "Reconciled Date": true, "Date Entered": true, "Account Name": true, "Shares": true, "Price": true,
                             "Account Balance": true, "Subtotal Table": true, "Running Totals": "all" });
    all["Sorting"] = json!({ "Secondary Key": "date", "Secondary Subtotal for Date Key": "quarterly" });
    let run = |sqlite: bool, options: &Value| {
        let mut f = if sqlite {
            Fixture::open_sqlite("test-data", "iso")
        } else {
            Fixture::open("test-data", "iso", true)
        };
        let mut result = f.run(TEMPLATE, options);
        result["now"] = Value::Null;
        result
    };
    [base, all].iter().for_each(|options| {
        let xml = run(false, options);
        assert!(!blocks_of(&xml, "table").is_empty(), "{xml:#}");
        assert_eq!(run(true, options), xml);
    });
}
