//! Investment Lots (`standard/investment-lots.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/investment-lots.scm`, cited as IL) through `report.run` and `report.export`:
//! the checks of GnuCash's own test (`5.17:gnucash/report/reports/standard/test/test-investment-lots.scm`, cited as
//! TIL) on the five books it builds (`fixtures/reports/investment-lots.gnucash` and the four `lots-*.gnucash`, with
//! their SQLite copies), and hand-worked cases for what that test does not reach, on books built through the API.
//! Figures are worked from IL by hand (`design/notes/report-investments.md` §4, §9.4).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::{Value, json};
use statements::{Setup, blocks, expected, options_param, tables, test_settings};

const LOTS: &str = "Investment Lots";
/// Book A, and books B to E in GnuCash's test order.
const BOOK_A: &str = "investment-lots.gnucash";
const BOOK_B: &str = "lots-reverse-split.gnucash";
const BOOK_C: &str = "lots-baseline.gnucash";
const BOOK_D: &str = "lots-reverse-split-sale.gnucash";
const BOOK_E: &str = "lots-postsplit-baseline.gnucash";
/// The lot of books B to E, and its account.
const LOT: &str = "10000000000000000000000000000000";
const AAPL: &str = "a0000000000000000000000000000005";

/// `fixture` and its SQLite copy, each opened read-only, with GnuCash's default date format (TIL sets none).
fn both(fixture: &str) -> [(&'static str, Setup); 2] {
    [
        ("xml", Setup::fixture(fixture, &test_settings("locale"))),
        ("sqlite", Setup::sqlite_fixture(fixture, &test_settings("locale"))),
    ]
}

/// A cell's text: a text cell's text, else its display form; `""` for an empty cell.
fn text(cell: &Value) -> String {
    cell["text"].as_str().or_else(|| cell["display"].as_str()).unwrap_or("").to_owned()
}

/// The table, the report's only one.
fn table(result: &Value) -> &Value {
    let t = tables(result);
    assert_eq!(t.len(), 1);
    t[0]
}

/// The rows of the table.
fn table_rows(result: &Value) -> &Vec<Value> {
    table(result)["rows"].as_array().unwrap()
}

/// A row's role, `data` when it has none.
fn role(row: &Value) -> &str {
    row["role"].as_str().unwrap_or("data")
}

/// Every row of the table as `role: texts`; a rule or spacer row as its role alone.
fn rows(result: &Value) -> Vec<String> {
    table_rows(result)
        .iter()
        .map(|r| match role(r) {
            "rule" | "spacer" => role(r).to_owned(),
            other => {
                let texts: Vec<String> = r["cells"].as_array().unwrap().iter().map(text).collect();
                format!("{other}: {}", texts.join(" | "))
            }
        })
        .collect()
}

/// The rows with `role`, as [`rows`] gives them.
fn rows_of(result: &Value, role: &str) -> Vec<String> {
    rows(result).into_iter().filter(|r| r.starts_with(&format!("{role}: "))).collect()
}

/// The column labels.
fn labels(result: &Value) -> Vec<String> {
    table(result)["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap().to_owned())
        .collect()
}

/// A lot's row: a data row at the top level with a cell per column (the price row is one cell across the table).
fn is_lot_row(result: &Value, row: &Value) -> bool {
    role(row) == "data" && row["depth"].as_u64().unwrap_or(0) == 0 && row["cells"].as_array().unwrap().len() == labels(result).len()
}

/// The non-empty texts of the `n`th cell (from 0) of every row that has one, leaving out heading, rule and spacer
/// rows: what `sxml->table-row-col` reads of the `<td>`s at that position
/// (`5.17:gnucash/report/test/test-report-extras.scm:96-109`).
fn td_texts(result: &Value, n: usize) -> Vec<String> {
    table_rows(result)
        .iter()
        .filter(|r| !matches!(role(r), "heading" | "rule" | "spacer"))
        .filter_map(|r| r["cells"].as_array().unwrap().get(n).map(text))
        .filter(|t| !t.is_empty())
        .collect()
}

/// TIL's selection of the column headed `label` (`find-col-index`, TIL:371-376), read by label: the column's texts,
/// or none when no column has the label.
fn column_texts(result: &Value, label: &str) -> Vec<String> {
    labels(result)
        .iter()
        .position(|l| l == label)
        .map_or_else(Vec::new, |i| td_texts(result, i))
}

/// The same selection as GnuCash makes it: the plain style sheet writes the account's name as a `<th>`
/// (`5.17:gnucash/report/stylesheets/plain.scm:105-112`), first among the header texts, so the position found is one
/// past the column and the `<td>`s read are those of the column to its right.
fn gnucash_column_texts(result: &Value, label: &str) -> Vec<String> {
    labels(result)
        .iter()
        .position(|l| l == label)
        .map_or_else(Vec::new, |i| td_texts(result, i + 1))
}

/// The first lot row's cell under the column headed `label`.
fn lot_cell(result: &Value, label: &str) -> String {
    let i = labels(result)
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("no column {label}"));
    let row = table_rows(result).iter().find(|r| is_lot_row(result, r)).unwrap();
    text(&row["cells"][i])
}

/// The cells of the first lot row under `labels`.
fn lot_cells(result: &Value, labels: &[&str]) -> Vec<String> {
    labels.iter().map(|l| lot_cell(result, l)).collect()
}

/// The options of a window from `from` to `to`, and `more`.
fn window(from: &str, to: &str, more: Value) -> Value {
    let mut options = json!({ "General": { "Start date": { "absolute": from }, "End date": { "absolute": to } } });
    more.as_object().unwrap().iter().for_each(|(section, values)| {
        if options.get(section).is_none() {
            options[section] = json!({});
        }
        values
            .as_object()
            .unwrap()
            .iter()
            .for_each(|(k, v)| options[section][k] = v.clone());
    });
    options
}

/// The window of the note's worked case on book D (`design/notes/report-investments.md` §9.4).
fn year_2020(more: Value) -> Value {
    window("2019-12-31", "2020-12-31", more)
}

// GnuCash's checks.

/// TIL:392-499 renders book A under eleven option sets and asserts only that each renders. Book A has no lots, so
/// with "Include accounts with no lots" off (IL:2150-2152) no account is shown: the table is the grand total's
/// header and row.
#[test]
fn gnucash_s_rendering_runs() {
    let e = expected("investment-lots.expected.json");
    let runs = e["renderOnly"].as_array().unwrap();
    assert_eq!(runs.len(), 11);
    for (format, mut s) in both(BOOK_A) {
        runs.iter().for_each(|r| {
            let options = options_param(r);
            let result = s.run(LOTS, &options);
            assert_eq!(result["title"], "Investment Lots, 03/15/24 to 03/15/24", "{format} {}", r["name"]);
            assert_eq!(
                rows_of(&result, "grandTotal").len(),
                1,
                "{format} {}: {:#?}",
                r["name"],
                rows(&result)
            );
            assert!(s.export(LOTS, &options).contains("Grand Total"), "{format} {}", r["name"]);
        });
    }
}

/// TIL:448-475 sets ten options, one after another on one set, and passes when each exists. All of them at once
/// are accepted, and the result echoes them.
#[test]
fn gnucash_s_option_checks() {
    let e = expected("investment-lots.expected.json");
    let checks = e["optionChecks"]["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 10);
    let mut options = json!({});
    checks.iter().for_each(|c| {
        let (section, name) = c["option"].as_str().unwrap().split_once('/').unwrap();
        if options.get(section).is_none() {
            options[section] = json!({});
        }
        options[section][name] = c["value"].clone();
    });
    let mut s = Setup::fixture(BOOK_A, &test_settings("locale"));
    let result = s.run(LOTS, &options);
    checks.iter().for_each(|c| {
        let (section, name) = c["option"].as_str().unwrap().split_once('/').unwrap();
        assert_eq!(result["options"][section][name], c["value"], "{} (TIL:{})", c["name"], c["line"]);
    });
}

/// The `is-long-term?` cases (TIL:623-646) under the report's own rule, `long-term?` (IL:779-789): held strictly
/// more than the threshold by `gnc:date-year-delta`. 2020-01-01 to 2021-01-01 is exactly one year, so short term
/// there, where the test-only helper says long term (the note's Q8).
fn report_rule(name: &str) -> bool {
    match name {
        "one-year-is-lt" | "364-days-not-lt" | "same-day-not-lt" => false,
        "two-years-is-lt" | "five-years-is-lt" => true,
        _ => panic!("no case {name}"),
    }
}

/// A Rhai expression for an argument of a function check: `()` for `false`, a Rational for a number.
fn rhai_arg(v: &Value) -> String {
    if v == &json!(false) {
        "()".to_owned()
    } else {
        format!("rational({v})")
    }
}

/// The split type of a classifier check: book B's marked reverse split's own for the engine cases (TIL:651-670),
/// else the string.
fn split_type_arg(t: &Value) -> String {
    let t = t.as_str().unwrap();
    if t.starts_with("stock-split (") || t.starts_with("xaccSplitGetType") {
        "type_of_split(rs)".to_owned()
    } else {
        format!("{t:?}")
    }
}

/// The Rhai test of one function check of `investment-lots.expected.json`, or `None` for `calculate-gain`, which
/// red-ale does not translate (IL:466-469, used only by TIL; the note's Q8).
fn rhai_check(c: &Value) -> Option<String> {
    let a = &c["args"];
    let name = c["name"].as_str().unwrap();
    let want = |x: &str| match &c["expected"] {
        Value::Bool(false) => format!("is_undefined({x})"),
        Value::Object(r) => format!("in_range({x}, \"{}\", \"{}\")", r["min"], r["max"]),
        other => panic!("{name}: {other}"),
    };
    let test = match c["function"].as_str().unwrap() {
        "calculate-cagr" => want(&format!(
            "calculate_cagr({}, {}, {})",
            rhai_arg(&a["basis"]),
            rhai_arg(&a["endValue"]),
            rhai_arg(&a["years"])
        )),
        "calculate-roi" => want(&format!("il::calculate_roi({}, {})", rhai_arg(&a["basis"]), rhai_arg(&a["gain"]))),
        "calculate-gain" => return None,
        "is-long-term?" => format!(
            "il::held_long_term(ctx, day(ctx, {}), day(ctx, {}), {}) == {}",
            a["buy"],
            a["sell"],
            rhai_arg(&a["years"]),
            report_rule(name)
        ),
        "stock-split-event?" => {
            format!(
                "il::stock_split_event({}, {}) == {}",
                split_type_arg(&a["splitType"]),
                a["splitAction"],
                c["expected"]
            )
        }
        "non-realizing-share-reduction?" => format!(
            "il::non_realizing_share_reduction({}, {}, {}, {}) == {}",
            split_type_arg(&a["splitType"]),
            a["splitAction"],
            rhai_arg(&a["amount"]),
            rhai_arg(&a["value"]),
            c["expected"]
        ),
        other => panic!("{name}: no function {other}"),
    };
    Some(format!("    out.push(result_row({name:?}, {test}));"))
}

/// A user report that runs `checks` against IL's helpers (imported from the built-in script, as TIL loads the
/// module, TIL:18-25) and the engine's `calculate_cagr`, a row each. `rs` is book B's reverse split, which
/// `xaccSplitMakeStockSplit` marked; the checks TIL makes on such a split take its type from it.
fn function_check_script(checks: &[String]) -> String {
    format!(
        r#"import "standard/investment-lots" as il;
fn templates() {{ [#{{ guid: "0123456789abcdef0123456789abcdef", name: "Lot helper checks" }}] }}
fn options(params, o, book) {{ }}
fn is_undefined(x) {{ type_of(x) == "()" }}
fn in_range(x, min, max) {{ !is_undefined(x) && x >= rational(min) && x <= rational(max) }}
fn day(ctx, text) {{
    let p = text.split("-");
    ctx.make_time(#{{ year: parse_int(p[0]), month: parse_int(p[1]), day: parse_int(p[2]), hour: 0 }})
}}
fn type_of_split(s) {{ if s.isStockSplit {{ "stock-split" }} else {{ "normal" }} }}
fn result_row(name, pass) {{ #{{ cells: [#{{ text: name }}, #{{ text: if pass {{ "pass" }} else {{ "FAIL" }} }}] }} }}
fn render(params, book, opts, ctx) {{
    let rs = book.account("{AAPL}").splits().filter(|s| s.value == 0)[0];
    let out = [];
{}
    #{{ title: "checks", sections: [#{{ blocks: [#{{ kind: "table", columns: [#{{ label: "check", role: "label" }},
        #{{ label: "result", role: "label" }}], rows: out }}] }}] }}
}}
"#,
        checks.join("\n")
    )
}

/// TIL:505-730 calls the report's helpers directly. Each check runs on red-ale's: `calculate-cagr` is the engine's
/// `calculate_cagr` (to 10⁻¹²) within GnuCash's ranges; `calculate-roi`, the classifier and `long-term?` are the
/// script's. `calculate-gain` is not translated.
#[test]
fn gnucash_s_function_checks() {
    let e = expected("investment-lots.expected.json");
    let all = e["functionChecks"]["checks"].as_array().unwrap();
    let checks: Vec<String> = all.iter().filter_map(rhai_check).collect();
    let skipped: Vec<&str> = all
        .iter()
        .filter(|c| c["function"] == "calculate-gain")
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        skipped,
        [
            "positive-gain",
            "fifty-dollar-gain",
            "negative-loss",
            "twenty-dollar-loss",
            "zero-change"
        ]
    );
    assert_eq!(checks.len(), all.len() - skipped.len());
    let mut s = Setup::fixture(BOOK_B, &test_settings("locale"));
    s.script("lot-checks.rhai", &function_check_script(&checks));
    let result = s.run("Lot helper checks", &json!({}));
    let want: Vec<String> = all
        .iter()
        .filter(|c| c["function"] != "calculate-gain")
        .map(|c| format!("data: {} | pass", c["name"].as_str().unwrap()))
        .collect();
    assert_eq!(rows(&result), want);
}

/// TIL:733-760 on book B, its reverse split in the lot and outside the default window: the header has "Sold
/// Splits", that column has no "1", and the "Realized Gain" column has no "(".
fn reverse_split_regression(format: &str, result: &Value, html: &str) {
    assert!(
        labels(result).contains(&"Sold Splits".to_owned()),
        "{format}: found-sold-splits-column (TIL:741)"
    );
    assert!(
        rows_of(result, "heading").iter().any(|h| h.contains("| Sold Splits |")),
        "{format}: the account's header"
    );
    assert!(html.contains(">Sold Splits<"), "{format}");
    for texts in [column_texts(result, "Sold Splits"), gnucash_column_texts(result, "Sold Splits")] {
        assert!(
            !texts.iter().any(|t| t == "1"),
            "{format}: no-sold-split-count-one (TIL:749): {texts:?}"
        );
    }
    for texts in [column_texts(result, "Realized Gain"), gnucash_column_texts(result, "Realized Gain")] {
        assert!(
            !texts.iter().any(|t| t.contains('(')),
            "{format}: no-phantom-negative-realized-loss (TIL:755)"
        );
    }
}

/// TIL's two equality checks between books (TIL:792-794, :831-833): the "Sold Splits" and "Realized Gain"
/// selections of two runs are equal, read by label and as GnuCash reads them.
fn assert_same_selections(format: &str, what: &str, a: &Value, b: &Value) {
    for label in ["Sold Splits", "Realized Gain"] {
        assert_eq!(column_texts(a, label), column_texts(b, label), "{format} {what}: {label}");
        assert_eq!(
            gnucash_column_texts(a, label),
            gnucash_column_texts(b, label),
            "{format} {what}: {label}, GnuCash's way"
        );
    }
}

/// The checks of TIL:733-834 on books B to E with the default options, XML and SQLite. With the default window
/// (today) nothing is sold in it and the default "gains-only" has no "Realized Gain" column, so as in GnuCash the
/// comparisons hold trivially; `windowed_comparisons_test_something` repeats them over 2020.
#[test]
fn gnucash_s_reverse_split_checks() {
    for (format, mut b) in both(BOOK_B) {
        let result = b.run(LOTS, &json!({}));
        reverse_split_regression(format, &result, &b.export(LOTS, &json!({})));
        let base = Setup::fixture(BOOK_C, &test_settings("locale")).run(LOTS, &json!({}));
        assert_same_selections(format, "C vs B", &base, &result);
    }
    for (format, mut d) in both(BOOK_D) {
        let result = d.run(LOTS, &json!({}));
        assert!(labels(&result).contains(&"Sold Splits".to_owned()), "{format}: TIL:829");
        assert!(d.export(LOTS, &json!({})).contains(">Sold Splits<"), "{format}");
        let base = Setup::fixture(BOOK_E, &test_settings("locale")).run(LOTS, &json!({}));
        assert_same_selections(format, "E vs D", &base, &result);
        assert!(
            column_texts(&result, "Realized Gain").is_empty(),
            "gains-only has no Realized Gain column"
        );
    }
}

/// The ported checks, by run, name and line, are exactly those of the four expected files.
#[test]
fn every_check_of_the_lot_books_is_ported() {
    let names = |file: &str| -> Vec<String> {
        let e = expected(file);
        e["runs"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| {
                let id = r["id"].as_str().unwrap().to_owned();
                r["checks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(move |c| format!("{id}/{}:{}", c["name"].as_str().unwrap(), c["line"]))
            })
            .collect()
    };
    assert_eq!(
        names("lots-reverse-split.expected.json"),
        [
            "reverse-split-regression/found-sold-splits-column:741",
            "reverse-split-regression/realized-gain-column-optional-when-no-sales:742",
            "reverse-split-regression/no-sold-split-count-one:749",
            "reverse-split-regression/no-phantom-negative-realized-loss:755",
            "reverse-split-vs-baseline/sold-splits-equal-baseline-vs-reverse:792",
            "reverse-split-vs-baseline/realized-gain-equal-baseline-vs-reverse:794",
        ]
    );
    assert_eq!(
        names("lots-reverse-split-sale.expected.json"),
        [
            "reverse-split-with-sale/reverse-split-sale-has-sold-splits-column:829",
            "reverse-split-with-sale/reverse-split-sale-realized-gain-column-optional:830",
            "reverse-split-with-sale/sold-splits-equal-equivalent-baseline:831",
            "reverse-split-with-sale/realized-gain-equal-equivalent-baseline:833",
        ]
    );
    assert!(names("lots-baseline.expected.json").is_empty(), "C is read only to compare with B");
    assert!(
        names("lots-postsplit-baseline.expected.json").is_empty(),
        "E is read only to compare with D"
    );
}

/// TIL's comparisons over 2020 with "Group gains by age" off, so that the window holds the trades and a "Realized
/// Gain" column exists. D and E each sell 25 of their lot's shares for $3,000 against a $2,500 basis: one sale and
/// a $500.00 gain on the lot, account and grand-total rows. B and C sell nothing: the totals show their zeros.
#[test]
fn windowed_comparisons_test_something() {
    let options = year_2020(json!({ "Columns": { "Group gains by age (short term and long term)": "no" } }));
    for (format, mut d) in both(BOOK_D) {
        let rd = d.run(LOTS, &options);
        let re = Setup::fixture(BOOK_E, &test_settings("locale")).run(LOTS, &options);
        assert_same_selections(format, "E vs D over 2020", &re, &rd);
        assert_eq!(column_texts(&rd, "Sold Splits"), ["1", "1", "1"], "{format}");
        assert_eq!(column_texts(&rd, "Realized Gain"), ["$500.00", "$500.00", "$500.00"], "{format}");
        assert_eq!(
            gnucash_column_texts(&rd, "Sold Splits"),
            ["25. AAPL", "25. AAPL"],
            "{format}: Sold Amount"
        );
    }
    for (format, mut b) in both(BOOK_B) {
        let rb = b.run(LOTS, &options);
        let rc = Setup::fixture(BOOK_C, &test_settings("locale")).run(LOTS, &options);
        assert_same_selections(format, "C vs B over 2020", &rc, &rb);
        assert_eq!(column_texts(&rb, "Sold Splits"), ["0", "0"], "{format}: total rows show zeros");
        assert_eq!(column_texts(&rb, "Realized Gain"), ["$0.00", "$0.00"], "{format}");
    }
}

/// The figures `workedFromCode` gives for the default runs of books B to E (`design/notes/report-investments.md`
/// §4.6): the lot's end amount and basis, no price so an end value of 0 and the whole basis lost, long term since
/// 2020, a CAGR of −100 %.
#[test]
fn worked_figures_of_the_default_runs() {
    for (file, fixture) in [
        ("lots-reverse-split.expected.json", BOOK_B),
        ("lots-baseline.expected.json", BOOK_C),
        ("lots-reverse-split-sale.expected.json", BOOK_D),
        ("lots-postsplit-baseline.expected.json", BOOK_E),
    ] {
        let e = expected(file);
        let worked = &e["workedFromCode"];
        let figures = worked.get("figures").unwrap_or(&worked["runs"][0]["figures"]);
        for (format, mut s) in both(fixture) {
            let result = s.run(LOTS, &json!({}));
            figures.as_object().unwrap().iter().for_each(|(label, want)| {
                let got = if label == "price line" {
                    text(&table_rows(&result)[4]["cells"][0])
                } else {
                    lot_cell(&result, label)
                };
                assert_eq!(got, want.as_str().unwrap(), "{format} {fixture}: {label}");
            });
        }
    }
}

/// The GUID of AAPL's split posted on `date` in books B to E.
fn split_of(s: &mut Setup, date: &str) -> String {
    let list = s.call("transaction.list", json!({ "account": AAPL }));
    let t = list["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["datePosted"].as_str().unwrap().starts_with(date))
        .unwrap();
    t["splits"].as_array().unwrap().iter().find(|sp| sp["account"] == AAPL).unwrap()["guid"]
        .as_str()
        .unwrap()
        .to_owned()
}

const D_HEADER: &str = "Lot Title | Opened | Closed | Bought Amount | Bought Value (Basis) | Bought Average Price | Sold Splits | \
                        Sold Amount | Sold Basis | Sold Value | Sold Average Price | End Amount | End Basis | End Value | \
                        ST Realized Gain | LT Realized Gain | Realized CAGR | ST Unrealized Gain | LT Unrealized Gain | \
                        Unrealized CAGR";

/// Book D's rows over 2020 (the note's §9.4).
fn book_d_rows() -> Vec<String> {
    let warning = "Warning: Above lot's computed gain $500.00 is not equal to the \"Realized Gain/Loss\" split(s) sum $0.00. \
                   Difference: $500.00";
    let spacers = || vec!["spacer".to_owned(); 3];
    [
        spacers(),
        vec![
            "heading: Root:Assets:Broker:AAPL".to_owned(),
            "data: No price found".to_owned(),
            format!("heading: {D_HEADER}"),
            "data:  | 01/01/20 |  | 100. AAPL | $5,000.00 | $50.00 | 1 | 25. AAPL | $2,500.00 | $3,000.00 | $120.00 | \
             25. AAPL | $2,500.00 |  | $500.00 |  | 204.1% | -$2,500.00 |  | -100.0%"
                .to_owned(),
            format!("warning: {warning}"),
            "rule".to_owned(),
            "grandTotal: Account Lots Total |  |  | 100. AAPL | $5,000.00 | $50.00 | 1 | 25. AAPL | $2,500.00 | $3,000.00 | \
             $120.00 | 25. AAPL | $2,500.00 | $0.00 | $500.00 | $0.00 | 204.1% | -$2,500.00 | $0.00 | -100.0%"
                .to_owned(),
        ],
        spacers(),
        vec![
            "rule".to_owned(),
            "rule".to_owned(),
            "heading:  |  |  |  | Bought Value (Basis) |  | Sold Splits |  | Sold Basis | Sold Value |  |  | End Basis | \
             End Value | ST Realized Gain | LT Realized Gain |  | ST Unrealized Gain | LT Unrealized Gain | "
                .to_owned(),
            "rule".to_owned(),
            "grandTotal: Grand Total |  |  |  | $5,000.00 |  | 1 |  | $2,500.00 | $3,000.00 |  |  | $2,500.00 | $0.00 | \
             $500.00 | $0.00 |  | -$2,500.00 | $0.00 | "
                .to_owned(),
        ],
        spacers(),
    ]
    .concat()
}

/// Book D over 2020 (`workedFromCode`'s second run; the note's §9.4): the purchase of 100 shares for $5,000, the
/// reverse split to 50, the sale of 25 for $3,000 against half the basis, $2,500, a short-term gain of $500 over 60
/// days of a leap year, (3000/2500)^(366/60) − 1 = 204.1 %; 25 shares left with a $2,500 basis and no price. No
/// "Realized Gain/Loss" split records the gain, so the lot warns (IL:1586-1600).
#[test]
fn book_d_over_2020() {
    let e = expected("lots-reverse-split-sale.expected.json");
    let worked = &e["workedFromCode"]["runs"][1];
    let options = options_param(worked);
    for (format, mut s) in both(BOOK_D) {
        let result = s.run(LOTS, &options);
        worked["figures"].as_object().unwrap().iter().for_each(|(label, want)| {
            assert_eq!(lot_cell(&result, label), want.as_str().unwrap(), "{format}: {label}");
        });
        assert_eq!(rows(&result), book_d_rows(), "{format}");
        assert_eq!(result["title"], "Investment Lots, 12/31/19 to 12/31/20");
        let t = table_rows(&result);
        let lot_row = &t[6];
        assert_eq!(
            lot_row["cells"][1]["link"]["split"],
            json!(split_of(&mut s, "2020-01-01")),
            "Opened links the purchase"
        );
        assert_eq!(
            (&lot_row["cells"][16]["rounding"], &lot_row["cells"][16]["places"]),
            (&json!("halfUp"), &json!(1))
        );
        assert_eq!(t[3]["cells"][0]["account"], AAPL);
        let html = s.export(LOTS, &options);
        assert!(html.contains("204.1%"), "{format}");
    }
}

/// Book D's split rows over 2020 (IL:1262-1312), under the lot row, indented: the purchase with its date under
/// Opened and the running end amount and basis; the sale with its date under Closed, its figures in the short-term
/// columns (long term reads 0, blank) and its CAGR. The reverse split has no row.
#[test]
fn split_rows() {
    let mut s = Setup::fixture(BOOK_D, &test_settings("locale"));
    let result = s.run(LOTS, &year_2020(json!({ "Display": { "Show lot split rows": true } })));
    let data = rows_of(&result, "data");
    assert_eq!(
        &data[2..],
        [
            "data: split | 01/01/20 |  | 100. AAPL | $5,000.00 | $50.00 |  |  |  |  |  | 100. AAPL | $5,000.00 |  |  |  |  |  |  | ",
            "data: split |  | 03/01/20 |  |  |  |  | 25. AAPL | $2,500.00 | $3,000.00 | $120.00 | 25. AAPL | $2,500.00 |  | $500.00 |  | \
         204.1% |  |  | ",
        ]
    );
    let t = table_rows(&result);
    assert_eq!(t[7]["depth"], 1);
    assert_eq!(t[7]["cells"][1]["link"]["split"], json!(split_of(&mut s, "2020-01-01")));
    assert_eq!(t[8]["cells"][2]["link"]["split"], json!(split_of(&mut s, "2020-03-01")));
}

/// Book D over 2020 with the other column options: "gains-and-sales" splits the sold columns by term too; ROI is the
/// gain over its basis, 500/2500 and −2500/2500 (IL:475-481, :499-513); the GUID column names the lot; "no" sums
/// the gains, and with "Show blanks instead of zeros" off the lot row shows its zeros; with every column off only the
/// title is left.
#[test]
fn column_options() {
    let mut s = Setup::fixture(BOOK_D, &test_settings("locale"));
    let result = s.run(
        LOTS,
        &year_2020(json!({ "Columns": {
        "Group gains by age (short term and long term)": "gains-and-sales", "Show ROI columns": true,
        "Show lot GUID column": true } })),
    );
    assert_eq!(&labels(&result)[..3], ["Lot Title", "GUID", "Opened"]);
    assert_eq!(lot_cell(&result, "GUID"), LOT);
    assert_eq!(
        lot_cells(
            &result,
            &[
                "ST Sold Amount",
                "ST Sold Basis",
                "ST Sold Value",
                "LT Sold Amount",
                "Sold Average Price"
            ]
        ),
        ["25. AAPL", "$2,500.00", "$3,000.00", "", "$120.00",]
    );
    assert_eq!(
        lot_cells(&result, &["Realized ROI", "Unrealized ROI", "Realized CAGR"]),
        ["20.0%", "-100.0%", "204.1%"]
    );
    let result = s.run(
        LOTS,
        &year_2020(json!({
        "Columns": { "Group gains by age (short term and long term)": "no", "Show CAGR columns": false },
        "Display": { "Show blanks instead of zeros in table cells": false } })),
    );
    assert_eq!(
        labels(&result)[14..],
        ["Realized Gain", "Unrealized Gain"],
        "no ST and LT columns, no CAGR"
    );
    assert_eq!(
        lot_cells(&result, &["Closed", "End Value", "Realized Gain", "Unrealized Gain"]),
        ["", "$0.00", "$500.00", "-$2,500.00"]
    );
    let result = s.run(
        LOTS,
        &year_2020(json!({ "Columns": {
        "Show date columns": false, "Show bought columns": false, "Show sold columns": false, "Show end columns": false,
        "Show realized gain columns": false, "Show unrealized gain columns": false } })),
    );
    assert_eq!(labels(&result), ["Lot Title"]);
}

/// The window starts at the *end* of the Start date's day (IL:667-672), so a trade on that day is before it: from
/// 2020-01-01 book D's purchase is not bought in the window, but still opens the lot and makes its basis; from
/// 2020-03-01 the sale is not in it either.
#[test]
fn the_start_date_s_own_trades_are_outside() {
    let mut s = Setup::fixture(BOOK_D, &test_settings("locale"));
    let result = s.run(LOTS, &window("2020-01-01", "2020-12-31", json!({})));
    assert_eq!(
        lot_cells(
            &result,
            &["Opened", "Bought Amount", "Sold Basis", "ST Realized Gain", "Realized CAGR"]
        ),
        ["01/01/20", "", "$2,500.00", "$500.00", "204.1%",]
    );
    let result = s.run(LOTS, &window("2020-03-01", "2020-12-31", json!({})));
    assert_eq!(lot_cells(&result, &["Sold Splits", "End Basis"]), ["", "$2,500.00"]);
}

// Books built through the API: Bank (BANK) and Broker (ASSET) in USD, and NYSE stocks of SCU 1 under Broker.

/// A new book with Bank, Broker and the `stocks`.
fn book_with(stocks: &[&str]) -> Setup {
    let mut s = Setup::new_book(&test_settings("locale"));
    s.account("Bank", "BANK", None, "CURRENCY::USD");
    s.account("Broker", "ASSET", None, "CURRENCY::USD");
    stocks.iter().for_each(|m| {
        s.call(
            "commodity.create",
            json!({ "namespace": "NYSE", "mnemonic": m, "fullname": m, "fraction": 1 }),
        );
        s.account(m, "STOCK", Some("Broker"), &format!("NYSE::{m}"));
    });
    s
}

/// A new lot of `stock` titled `title`.
fn lot(s: &mut Setup, stock: &str, title: &str) -> String {
    s.call("lot.create", json!({ "account": format!("Broker:{stock}"), "title": title }))["guid"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// `shares` of `stock` for `value` dollars on `date`, against Bank, in `lot` (or none), with `action`; a zero value
/// is a split alone. Gives the stock split's GUID.
fn trade(s: &mut Setup, date: &str, stock: &str, lot: Option<&str>, shares: &str, value: &str, action: &str) -> String {
    let account = format!("Broker:{stock}");
    let mut split = json!({ "account": account, "value": value, "amount": shares, "action": action });
    if let Some(l) = lot {
        split["lot"] = json!(l);
    }
    let mut splits = vec![split];
    if value != "0" {
        let other = value.strip_prefix('-').map_or_else(|| format!("-{value}"), str::to_owned);
        splits.push(json!({ "account": "Bank", "value": other }));
    }
    let t = s.call(
        "transaction.post",
        json!({ "datePosted": date, "currency": "CURRENCY::USD", "splits": splits }),
    );
    let id = s.call("account.get", json!({ "account": account }))["guid"].clone();
    t["splits"].as_array().unwrap().iter().find(|sp| sp["account"] == id).unwrap()["guid"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Options for `stocks` over 2018-01-01 to 2024-03-15, and `more`.
fn options_for(stocks: &[&str], more: Value) -> Value {
    let accounts: Vec<String> = stocks.iter().map(|m| format!("Broker:{m}")).collect();
    let mut options = window("2018-01-01", "2024-03-15", more);
    if options.get("Accounts").is_none() {
        options["Accounts"] = json!({});
    }
    options["Accounts"]["Accounts"] = json!(accounts);
    options
}

/// The ST and LT realized gains of each lot row, as `title: ST | LT`.
fn gains_by_term(result: &Value) -> Vec<String> {
    let l = labels(result);
    let (st, lt) = (
        l.iter().position(|l| l == "ST Realized Gain").unwrap(),
        l.iter().position(|l| l == "LT Realized Gain").unwrap(),
    );
    table_rows(result)
        .iter()
        .filter(|r| is_lot_row(result, r) && role(r) == "data")
        .map(|r| format!("{}: {} | {}", text(&r["cells"][0]), text(&r["cells"][st]), text(&r["cells"][lt])))
        .collect()
}

/// Lots each bought for $100 and sold for $150, to place holding periods around a threshold.
fn holding_book(trades: &[(&str, &str, &str)]) -> Setup {
    let mut s = book_with(&["XYZ"]);
    trades.iter().for_each(|(title, bought, sold)| {
        let l = lot(&mut s, "XYZ", title);
        trade(&mut s, bought, "XYZ", Some(&l), "10", "100", "");
        trade(&mut s, sold, "XYZ", Some(&l), "-10", "-150", "");
    });
    s
}

/// `long-term?` (IL:779-789) is `gnc:date-year-delta` strictly over the threshold, from the latest purchase. Days
/// count as 366ths of a year and a non-leap year's days from 28 February on move up one
/// (`5.17:bindings/guile/date-utilities.scm:201-217`): 2019-02-28 to 2020-02-29 is exactly a year, short term; to
/// 2020-03-01 a day more. A year to the day is short term, a year and a day long term. With a threshold of 0 a sale
/// on the purchase day is short term and the next day long term; with 0.5, 2020-01-01 to 2020-07-02 is 183/366,
/// exactly half, short term, and a day later long term.
#[test]
fn the_long_term_threshold() {
    let mut s = holding_book(&[
        ("leap-equal", "2019-02-28", "2020-02-29"),
        ("leap-more", "2019-02-28", "2020-03-01"),
        ("year", "2020-01-01", "2021-01-01"),
        ("year-and-day", "2020-01-01", "2021-01-02"),
        ("same-day", "2022-01-03", "2022-01-03"),
        ("next-day", "2022-01-03", "2022-01-04"),
        ("half", "2020-01-01", "2020-07-02"),
        ("over-half", "2020-01-01", "2020-07-03"),
    ]);
    let mut run = |years: Value, titles: &[&str]| -> Vec<String> {
        let all = gains_by_term(&s.run(
            LOTS,
            &options_for(&["XYZ"], json!({ "Columns": { "Long term gains age (years)": years } })),
        ));
        titles
            .iter()
            .map(|t| all.iter().find(|g| g.starts_with(&format!("{t}: "))).unwrap().clone())
            .collect()
    };
    assert_eq!(
        run(json!(1), &["leap-equal", "leap-more", "year", "year-and-day"]),
        [
            "leap-equal: $50.00 | ",
            "leap-more:  | $50.00",
            "year: $50.00 | ",
            "year-and-day:  | $50.00"
        ]
    );
    assert_eq!(
        run(json!(0), &["same-day", "next-day"]),
        ["same-day: $50.00 | ", "next-day:  | $50.00"]
    );
    assert_eq!(
        run(json!("0.5"), &["half", "over-half"]),
        ["half: $50.00 | ", "over-half:  | $50.00"]
    );
}

/// A lot of two purchases, $100 for 10 on 2020-01-01 and $200 for 10 on 2020-06-01, sold 10 for $500 on 2021-03-01
/// (IL:1824-1925): the sale takes half the $300 basis, a $350 gain, short term since it counts from the latest
/// purchase: `gnc:date-year-delta` counts 2021-03-01 as a leap year's 61st day, so 274/366 of a year, and
/// (500/150)^(366/274) − 1 = 399.4 %. Opened is the earliest purchase, and the lot warns of its
/// two bought splits (IL:1558-1565).
#[test]
fn several_purchases() {
    let mut s = book_with(&["XYZ"]);
    let l = lot(&mut s, "XYZ", "Two buys");
    trade(&mut s, "2020-01-01", "XYZ", Some(&l), "10", "100", "");
    trade(&mut s, "2020-06-01", "XYZ", Some(&l), "10", "200", "");
    trade(&mut s, "2021-03-01", "XYZ", Some(&l), "-10", "-500", "");
    let result = s.run(LOTS, &options_for(&["XYZ"], json!({})));
    let labels = [
        "Lot Title",
        "Opened",
        "Bought Amount",
        "Sold Basis",
        "ST Realized Gain",
        "LT Realized Gain",
        "Realized CAGR",
        "End Amount",
        "End Basis",
    ];
    assert_eq!(
        lot_cells(&result, &labels),
        [
            "Two buys", "01/01/20", "20. XYZ", "$150.00", "$350.00", "", "399.4%", "10. XYZ", "$150.00"
        ]
    );
    assert_eq!(
        rows_of(&result, "warning"),
        [
            "warning: Warning: Above lot has 2 bought splits. Consider separating them into their own lots.",
            "warning: Warning: Above lot's computed gain $350.00 is not equal to the \"Realized Gain/Loss\" split(s) sum $0.00. \
         Difference: $350.00",
        ]
    );
}

/// A forward stock split is a purchase (IL:524-536): 10 bought for $100, 10 more for nothing (action "Split"), then
/// 10 sold for $80 against half the $100 basis, a $30 gain over 29/366 of a year from the split. The split counts
/// as a second bought split. A CAGR of 37,582.1 % is grouped, where GnuCash's `~,1f` is not (`design/reports.md`
/// §14).
#[test]
fn a_forward_split_is_a_purchase() {
    let mut s = book_with(&["XYZ"]);
    let l = lot(&mut s, "XYZ", "Split");
    trade(&mut s, "2020-01-01", "XYZ", Some(&l), "10", "100", "");
    trade(&mut s, "2020-02-01", "XYZ", Some(&l), "10", "0", "Split");
    trade(&mut s, "2020-03-01", "XYZ", Some(&l), "-10", "-80", "");
    let result = s.run(LOTS, &options_for(&["XYZ"], json!({})));
    let labels = [
        "Bought Amount",
        "Bought Value (Basis)",
        "Bought Average Price",
        "Sold Basis",
        "ST Realized Gain",
        "Realized CAGR",
        "End Amount",
        "End Basis",
    ];
    assert_eq!(
        lot_cells(&result, &labels),
        ["20. XYZ", "$100.00", "$5.00", "$50.00", "$30.00", "37,582.1%", "10. XYZ", "$50.00"]
    );
    assert!(rows(&result).iter().any(|r| r.contains("2 bought splits")));
}

/// A sale of more than the lot holds makes its balance negative on the sale's date (IL:1866-1868, :1567-1575): 15 of
/// 10 shares take 15/10 of the $100 basis, $150, leaving −$50, a $150 gain over 60 days of a leap year,
/// 2^(366/60) − 1 = 6,759.4 %, and an unrealized gain of $50 on no price. A sale from a lot with nothing in it divides by zero, so its basis and gain are undefined and blank, and so is
/// everything they reach, the account's sums too; with no purchase the CAGR is blank, where GnuCash's report fails
/// (the note's Q6). The undefined gain also trips the gains check, printed empty as `xaccPrintAmount` prints an
/// error numeric.
#[test]
fn negative_balances_and_a_sale_from_nothing() {
    let mut s = book_with(&["XYZ"]);
    let over = lot(&mut s, "XYZ", "Oversold");
    trade(&mut s, "2020-01-01", "XYZ", Some(&over), "10", "100", "");
    trade(&mut s, "2020-03-01", "XYZ", Some(&over), "-15", "-300", "");
    let empty = lot(&mut s, "XYZ", "Empty");
    trade(&mut s, "2020-04-01", "XYZ", Some(&empty), "-5", "-50", "");
    let result = s.run(
        LOTS,
        &options_for(
            &["XYZ"],
            json!({ "Validation": {
        "Warn if the 'Realized Gain/Loss' split(s) sum does not match the computed gains": false } }),
        ),
    );
    let r = rows(&result);
    let negative = |date: &str| {
        format!(
            "warning: Warning: Above lot's balance is negative on {date}. Consider removing the responsible sale split from the lot \
                 and then scrubbing."
        )
    };
    assert_eq!(
        r[6],
        "data: Oversold | 01/01/20 |  | 10. XYZ | $100.00 | $10.00 | 1 | 15. XYZ | $150.00 | $300.00 | $20.00 | \
                      -5. XYZ | -$50.00 |  | $150.00 |  | 6,759.4% |  | $50.00 | "
    );
    assert_eq!(r[7], negative("03/01/20"));
    assert_eq!(
        r[8],
        "data: Empty |  |  |  |  |  | 1 | 5. XYZ |  | $50.00 | $10.00 | -5. XYZ |  |  |  |  |  |  |  | "
    );
    assert_eq!(r[9], negative("04/01/20"));
    let total = &r[11];
    assert!(
        total.starts_with("grandTotal: Account Lots Total |  |  | 10. XYZ | $100.00 | $10.00 | 2 | 20. XYZ |  | $350.00 |"),
        "{total}"
    );
    let result = s.run(LOTS, &options_for(&["XYZ"], json!({})));
    let mismatch = "warning: Warning: Above lot's computed gain  is not equal to the \"Realized Gain/Loss\" split(s) sum $0.00. \
                    Difference: ";
    assert!(rows(&result).contains(&mismatch.to_owned()), "{:#?}", rows(&result));
}

/// The gains check (IL:1586-1600): a "Realized Gain/Loss" split in the lot (no shares, the gain as its value) in the
/// window is summed and compared with the computed gain. $50 matches; $40 is $10 short.
#[test]
fn gains_splits_match_or_not() {
    let mut s = book_with(&["XYZ", "ABC"]);
    s.account("Gains", "INCOME", None, "CURRENCY::USD");
    for (stock, gain) in [("XYZ", "50"), ("ABC", "40")] {
        let l = lot(&mut s, stock, "Lot");
        trade(&mut s, "2020-01-01", stock, Some(&l), "10", "100", "");
        trade(&mut s, "2020-03-01", stock, Some(&l), "-10", "-150", "");
        let splits = json!([{ "account": format!("Broker:{stock}"), "value": gain, "amount": "0", "lot": l },
                            { "account": "Gains", "value": format!("-{gain}") }]);
        let t = json!({ "datePosted": "2020-03-01", "currency": "CURRENCY::USD", "description": "Realized Gain/Loss", "splits": splits });
        s.call("transaction.post", t);
    }
    let warnings = rows_of(&s.run(LOTS, &options_for(&["XYZ", "ABC"], json!({}))), "warning");
    assert_eq!(
        warnings,
        [
            "warning: Warning: Above lot's computed gain $50.00 is not equal to the \"Realized Gain/Loss\" split(s) sum $40.00. Difference: \
         $10.00"
        ]
    );
}

/// Splits in no lot (IL:1605-1640) and a balance the lots do not add up to (IL:1642-1660): an account with a lot of
/// 10 and 5 more shares bought outside it. "count" counts them; "list" lists each, newest first, linked and indented,
/// with its amount and value; "no" says nothing of them. The lots' end amount is 5 short of the balance.
#[test]
fn unassigned_splits_and_the_balance() {
    let mut s = book_with(&["XYZ"]);
    let l = lot(&mut s, "XYZ", "Lot");
    trade(&mut s, "2020-01-01", "XYZ", Some(&l), "10", "100", "");
    let first = trade(&mut s, "2020-02-01", "XYZ", None, "2", "30", "");
    let second = trade(&mut s, "2020-03-01", "XYZ", None, "3", "50", "");
    let mismatch = "warning: Warning: End amount 10. XYZ is not equal to actual account balance 15. XYZ. Difference: -5. XYZ. Do lots \
                    need to be scrubbed?";
    let mut warnings = |kind: &str| {
        let result = s.run(
            LOTS,
            &options_for(
                &["XYZ"],
                json!({ "Validation": { "Warn if a split is not assigned to a lot": kind } }),
            ),
        );
        (rows_of(&result, "warning"), result)
    };
    assert_eq!(
        warnings("count").0,
        [
            "warning: Warning: 2 split(s) are not assigned to a lot. Do lots need to be scrubbed?",
            mismatch
        ]
    );
    let (listed, result) = warnings("list");
    assert_eq!(
        listed,
        [
            "warning: Warning: The following 2 split(s) are not assigned to a lot. Do lots need to be scrubbed?",
            "warning: 03/01/20: amount 3. XYZ, value $50.00",
            "warning: 02/01/20: amount 2. XYZ, value $30.00",
            mismatch,
        ]
    );
    let items: Vec<&Value> = table_rows(&result).iter().filter(|r| r["depth"] == 1).collect();
    assert_eq!(items.len(), 2);
    assert_eq!(
        (&items[0]["cells"][0]["link"]["split"], &items[1]["cells"][0]["link"]["split"]),
        (&json!(second), &json!(first))
    );
    assert_eq!(warnings("no").0, [mismatch]);
}

/// "Include only accounts with warnings" (IL:2253-2256): of two accounts, only the one whose lot warns is shown, and
/// only its figures reach the grand total. A blank title (spaces) warns with "Warn if a lot has a blank title".
#[test]
fn only_accounts_with_warnings() {
    let mut s = book_with(&["XYZ", "ABC"]);
    let l = lot(&mut s, "XYZ", "  ");
    trade(&mut s, "2020-01-01", "XYZ", Some(&l), "10", "100", "");
    let l = lot(&mut s, "ABC", "Fine");
    trade(&mut s, "2020-01-01", "ABC", Some(&l), "10", "200", "");
    let options = options_for(
        &["XYZ", "ABC"],
        json!({ "Validation": {
        "Include only accounts with warnings": true, "Warn if a lot has a blank title": true } }),
    );
    let result = s.run(LOTS, &options);
    let headings = rows_of(&result, "heading");
    assert_eq!(headings[0], "heading: Broker:XYZ");
    assert_eq!(headings.len(), 3, "XYZ, its header and the grand header");
    assert_eq!(rows_of(&result, "warning"), ["warning: Warning: Above lot's title is blank."]);
    let grand = rows_of(&result, "grandTotal").pop().unwrap();
    assert!(grand.starts_with("grandTotal: Grand Total |  |  |  | $100.00 |"), "{grand}");
    let options = options_for(&["XYZ", "ABC"], json!({}));
    assert_eq!(
        rows_of(&s.run(LOTS, &options), "heading").len(),
        5,
        "both accounts without the option"
    );
}

/// The include options (IL:2135-2152, :2203-2206): an account whose shares were all sold is shown, but left out
/// without "Include accounts with no shares"; an account with no lots is shown with "Include accounts with no lots",
/// its split counted as unassigned; a closed lot is left out without "Include closed lots", and otherwise closed on
/// its last sale (IL:1320-1326).
#[test]
fn the_include_options() {
    let mut s = book_with(&["XYZ", "ABC"]);
    let l = lot(&mut s, "XYZ", "Closed");
    trade(&mut s, "2020-01-01", "XYZ", Some(&l), "10", "100", "");
    trade(&mut s, "2020-03-01", "XYZ", Some(&l), "-10", "-150", "");
    trade(&mut s, "2020-01-01", "ABC", None, "10", "100", "");
    let mut shown = |more: Value| -> Vec<String> {
        rows_of(&s.run(LOTS, &options_for(&["XYZ", "ABC"], more)), "heading")
            .into_iter()
            .filter(|h| h.contains("Broker:"))
            .collect()
    };
    assert_eq!(shown(json!({})), ["heading: Broker:XYZ"]);
    assert!(shown(json!({ "Accounts": { "Include accounts with no shares": false } })).is_empty());
    assert_eq!(
        shown(json!({ "Accounts": { "Include accounts with no lots": true } })),
        ["heading: Broker:XYZ", "heading: Broker:ABC"]
    );
    let result = s.run(
        LOTS,
        &options_for(&["ABC"], json!({ "Accounts": { "Include accounts with no lots": true } })),
    );
    let warnings = rows_of(&result, "warning");
    assert_eq!(
        warnings[0],
        "warning: Warning: 1 split(s) are not assigned to a lot. Do lots need to be scrubbed?"
    );
    assert_eq!(warnings.len(), 2, "{warnings:#?}");
    let result = s.run(LOTS, &options_for(&["XYZ"], json!({ "Display": { "Include closed lots": false } })));
    assert!(!rows(&result).iter().any(|r| r.starts_with("data: Closed")), "{:#?}", rows(&result));
    let result = s.run(LOTS, &options_for(&["XYZ"], json!({})));
    assert_eq!(lot_cell(&result, "Closed"), "03/01/20");
}

/// Adds a price and gives its GUID.
fn price(s: &mut Setup, commodity: &str, currency: &str, value: &str, date: &str) -> String {
    let p = s.call(
        "price.add",
        json!({ "commodity": commodity, "currency": currency, "value": value, "date": date }),
    );
    p["price"]["guid"].as_str().unwrap().to_owned()
}

/// The row after each account's name: its price.
fn price_rows(result: &Value) -> Vec<String> {
    let r = rows(result);
    r.iter()
        .enumerate()
        .filter(|(_, x)| x.starts_with("heading: Broker:"))
        .map(|(i, _)| r[i + 1].clone())
        .collect()
}

/// Prices (IL:620-636, :732-753, :944-962), each stock holding 10 shares bought for $100 in 2020: A has $12 on
/// 2020-06-01, $15 on 2020-12-01 and $20 on 2025-01-01; B only a price of the dollar in B, 0.1, inverted to $10 and
/// linked to the stored price; C only €8, shown in euros and in dollars at $1.25 a euro; D none. With End date
/// 2020-09-30: "before" takes A's $12, "nearest" its $15, "latest" its newest at or before now, $15, not the future
/// $20.
#[test]
fn prices() {
    let mut s = book_with(&["A", "B", "C", "D"]);
    ["A", "B", "C", "D"].iter().for_each(|m| {
        let l = lot(&mut s, m, m);
        trade(&mut s, "2020-01-01", m, Some(&l), "10", "100", "");
    });
    price(&mut s, "NYSE::A", "CURRENCY::USD", "12", "2020-06-01");
    price(&mut s, "NYSE::A", "CURRENCY::USD", "15", "2020-12-01");
    price(&mut s, "NYSE::A", "CURRENCY::USD", "20", "2025-01-01");
    let inverted = price(&mut s, "CURRENCY::USD", "NYSE::B", "0.1", "2020-06-01");
    price(&mut s, "NYSE::C", "CURRENCY::EUR", "8", "2020-06-01");
    price(&mut s, "CURRENCY::EUR", "CURRENCY::USD", "1.25", "2020-06-01");
    let mut run = |source: &str| {
        let mut options = options_for(&["A", "B", "C", "D"], json!({ "General": { "Price source": source } }));
        options["General"]["End date"] = json!({ "absolute": "2020-09-30" });
        s.run(LOTS, &options)
    };
    let result = run("pricedb-before");
    assert_eq!(
        price_rows(&result),
        [
            "data:   End price: $12.00 on 06/01/20",
            "data:   End price: $10.00 on 06/01/20",
            "data:   End price: €8.00 [$10.00] on 06/01/20",
            "data: No price found",
        ]
    );
    let end_values: Vec<String> = table_rows(&result)
        .iter()
        .filter(|r| is_lot_row(&result, r))
        .map(|r| text(&r["cells"][13]))
        .collect();
    assert_eq!(end_values, ["$120.00", "$100.00", "$100.00", ""]);
    let price_cell = table_rows(&result)
        .iter()
        .find(|r| text(&r["cells"][0]).contains("$10.00 on"))
        .unwrap();
    assert_eq!(
        price_cell["cells"][0]["link"]["price"],
        json!(inverted),
        "the inverted price links the price stored"
    );
    assert_eq!(price_rows(&run("pricedb-nearest"))[0], "data:   End price: $15.00 on 12/01/20");
    assert_eq!(price_rows(&run("pricedb-latest"))[0], "data:   End price: $15.00 on 12/01/20");
}

/// The Display options: "Show mnemonic in amounts" off prints shares without the symbol (IL:806-812); "Show long
/// account names" off names the account alone (IL:791-795).
#[test]
fn display_options() {
    let mut s = book_with(&["XYZ"]);
    let l = lot(&mut s, "XYZ", "Lot");
    trade(&mut s, "2020-01-01", "XYZ", Some(&l), "10", "100", "");
    let result = s.run(
        LOTS,
        &options_for(
            &["XYZ"],
            json!({ "Display": {
        "Show mnemonic in amounts": false, "Show long account names": false } }),
        ),
    );
    assert_eq!(rows_of(&result, "heading")[0], "heading: XYZ");
    assert_eq!(lot_cell(&result, "End Amount"), "10.");
}

/// Two sales of one lot on one date (IL:588-600): GnuCash conses the lot's splits off its list, reversing it, and its
/// stable sort keeps them so, so the sale entered second is taken first. Three shares for $10.00: the first sale
/// taken gets 1/3 of $10.00, $3.33, the other 1/2 of $6.67, $3.335, rounded to even, $3.34.
#[test]
fn same_day_sales_come_in_reverse_book_order() {
    let mut s = book_with(&["XYZ"]);
    let l = lot(&mut s, "XYZ", "Lot");
    trade(&mut s, "2020-01-01", "XYZ", Some(&l), "3", "10", "");
    trade(&mut s, "2020-03-01", "XYZ", Some(&l), "-1", "-4", "first");
    trade(&mut s, "2020-03-01", "XYZ", Some(&l), "-1", "-5", "second");
    let result = s.run(LOTS, &options_for(&["XYZ"], json!({ "Display": { "Show lot split rows": true } })));
    let sales: Vec<String> = rows_of(&result, "data")
        .iter()
        .filter(|r| r.starts_with("data: split |  |"))
        .map(|r| r.split(" | ").skip(7).take(3).collect::<Vec<_>>().join(" "))
        .collect();
    assert_eq!(sales, ["1. XYZ $3.33 $5.00", "1. XYZ $3.34 $4.00"]);
}

/// The chart (IL:2112-2145, :2266-2316): by default a stacked bar chart above the table, "Account Lot Gains" and the
/// interval, a bar per gain column and a series per shown account; "bar" does not stack; "bottom" puts it after the
/// table; with "Group gains by age" off the gains are summed; off, there is no chart, and the table ends with the
/// grand total.
#[test]
fn the_chart() {
    let mut s = Setup::fixture(BOOK_D, &test_settings("locale"));
    let result = s.run(LOTS, &year_2020(json!({})));
    let b = blocks(&result);
    assert_eq!((b.len(), b[0]["kind"].as_str()), (2, Some("chart")));
    let chart = b[0];
    assert_eq!(chart["title"], json!(["Account Lot Gains", "12/31/19 to 12/31/20"]));
    assert_eq!((&chart["type"], &chart["stacked"]), (&json!("bar"), &json!(true)));
    assert_eq!(
        chart["labels"],
        json!(["ST Realized Gain", "LT Realized Gain", "ST Unrealized Gain", "LT Unrealized Gain"])
    );
    let series = chart["series"].as_array().unwrap();
    assert_eq!(series.len(), 1);
    assert_eq!(series[0]["label"], "Root:Assets:Broker:AAPL");
    let n = |v: &str| json!({ "num": v, "denom": "1" });
    assert_eq!(series[0]["values"], json!([n("500"), n("0"), n("-2500"), n("0")]));
    let result = s.run(
        LOTS,
        &year_2020(json!({ "Chart": { "Chart type": "bar", "Chart location": "bottom" },
        "Columns": { "Group gains by age (short term and long term)": "no" } })),
    );
    let b = blocks(&result);
    assert_eq!((b[0]["kind"].as_str(), b[1]["kind"].as_str()), (Some("table"), Some("chart")));
    assert_eq!(
        (&b[1]["stacked"], &b[1]["labels"]),
        (&json!(false), &json!(["Realized Gain", "Unrealized Gain"]))
    );
    assert_eq!(b[1]["series"][0]["values"], json!([n("500"), n("-2500")]));
    let result = s.run(LOTS, &year_2020(json!({ "Chart": { "Show Chart": false } })));
    assert_eq!(blocks(&result).len(), 1);
    assert!(rows(&result).last().unwrap().starts_with("grandTotal: Grand Total"));
}

/// No accounts selected (IL:2266-2298): the title, a table of the chart's padding alone, and no chart, where GnuCash
/// adds one it never set up.
#[test]
fn no_accounts() {
    let mut s = Setup::new_book(&test_settings("locale"));
    let result = s.run(LOTS, &json!({}));
    assert_eq!(result["title"], "Investment Lots, 03/15/24 to 03/15/24");
    assert_eq!(blocks(&result).len(), 1);
    assert_eq!(rows(&result), ["spacer", "spacer", "spacer"]);
    assert!(rows(&s.run(LOTS, &json!({ "Chart": { "Show Chart": false } }))).is_empty());
    assert!(s.export(LOTS, &json!({})).contains("Investment Lots"));
}

/// Running the report, with split rows, every warning and each price source, leaves the Book, its revision and its
/// file as they were and writes nothing beside it, on XML and SQLite; two exports are the same bytes, with no script.
#[test]
fn a_run_changes_nothing() {
    for (format, mut s) in both(BOOK_D) {
        let bytes = std::fs::read(s.book()).unwrap();
        let info = s.call("book.info", json!({}));
        let files = s.files();
        ["pricedb-before", "pricedb-nearest", "pricedb-latest"].iter().for_each(|source| {
            let options = year_2020(
                json!({ "General": { "Price source": source }, "Display": { "Show lot split rows": true },
                "Validation": { "Warn if a split is not assigned to a lot": "list", "Warn if a lot has a blank title": true } }),
            );
            s.run(LOTS, &options);
            let (a, b) = (s.export(LOTS, &options), s.export(LOTS, &options));
            assert_eq!(a, b, "{format} {source}");
            assert!(!a.to_ascii_lowercase().contains("<script"), "{format}");
        });
        assert_eq!(s.call("book.info", json!({})), info, "{format}");
        assert_eq!(info["dirty"], false, "{format}");
        assert_eq!(s.files(), files, "{format}: no journal or other file was written");
        assert_eq!(std::fs::read(s.book()).unwrap(), bytes, "{format}: the file is unchanged");
    }
}

/// The options of IL:141-439 in GnuCash's order, with the General options every template gets.
#[test]
fn options_are_gnucash_s() {
    let mut s = Setup::with_config(&test_settings("locale"));
    let described = s.call("report.options", json!({ "report": LOTS }));
    let got: Vec<String> = described["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}/{} {} {} {}",
                o["section"].as_str().unwrap(),
                o["name"].as_str().unwrap(),
                o["key"],
                o["type"],
                o["default"]
            )
        })
        .collect();
    let want = [
        r#"Accounts/Accounts "a" "accountList" []"#,
        r#"Accounts/Include accounts with no shares "b" "boolean" true"#,
        r#"Accounts/Include accounts with no lots "c" "boolean" false"#,
        r#"Chart/Show Chart "a" "boolean" true"#,
        r#"Chart/Chart type "b" "multichoice" "bar-stacked""#,
        r#"Chart/Chart location "c" "multichoice" "top""#,
        r#"Chart/Plot width "da" "plotSize" {"percent":100}"#,
        r#"Chart/Plot height "db" "plotSize" {"percent":50}"#,
        r#"Columns/Show lot GUID column "a" "boolean" false"#,
        r#"Columns/Show date columns "b" "boolean" true"#,
        r#"Columns/Show bought columns "c" "boolean" true"#,
        r#"Columns/Show sold columns "d" "boolean" true"#,
        r#"Columns/Show end columns "e" "boolean" true"#,
        r#"Columns/Show realized gain columns "f" "boolean" true"#,
        r#"Columns/Show unrealized gain columns "g" "boolean" true"#,
        r#"Columns/Group gains by age (short term and long term) "h" "multichoice" "gains-only""#,
        r#"Columns/Long term gains age (years) "i" "numberRange" 1"#,
        r#"Columns/Show ROI columns "j" "boolean" false"#,
        r#"Columns/Show CAGR columns "k" "boolean" true"#,
        r#"Display/Show long account names "a" "boolean" true"#,
        r#"Display/Show mnemonic in amounts "b" "boolean" true"#,
        r#"Display/Include closed lots "c" "boolean" true"#,
        r#"Display/Show blanks instead of zeros in table cells "d" "boolean" true"#,
        r#"Display/Show lot split rows "e" "boolean" false"#,
        r#"General/Report name "0a" "string" "Investment Lots""#,
        r#"General/Stylesheet "0b" "multichoice" "Default""#,
        r#"General/Add options summary "0c" "multichoice" "never""#,
        r#"General/Start date "aa" "date" {"relative":"start-accounting-period"}"#,
        r#"General/End date "ab" "date" {"relative":"today"}"#,
        r#"General/Report's currency "b" "currency" "CURRENCY::USD""#,
        r#"General/Price source "c" "multichoice" "pricedb-before""#,
        r#"Validation/Include only accounts with warnings "a" "boolean" false"#,
        r#"Validation/Warn if a lot has more than one bought split "b" "boolean" true"#,
        r#"Validation/Warn if a lot's balance drops below zero "c" "boolean" true"#,
        r#"Validation/Warn if a lot has a blank title "d" "boolean" false"#,
        r#"Validation/Warn if the 'Realized Gain/Loss' split(s) sum does not match the computed gains "e" "boolean" true"#,
        r#"Validation/Warn if a split is not assigned to a lot "f" "multichoice" "count""#,
        r#"Validation/Warn if the account balance does not match the computed lots' end balance "g" "boolean" true"#,
    ];
    assert_eq!(got, want);
}
