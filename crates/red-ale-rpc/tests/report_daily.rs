//! Income vs. Day of Week and Expenses vs. Day of Week (`example/daily-reports.rhai`, translated from
//! `5.17:gnucash/report/reports/example/daily-reports.scm`, cited as DR) through `report.run` and `report.export`.
//!
//! GnuCash's tests only render these (`standard/test/test-charts.scm:71-77` on an empty book, `:187-190` on book A),
//! so the figures are hand-worked from the Scheme, citing the lines they follow, on copies of
//! `fixtures/reports/charts-daily.gnucash` and `test-data.gnucash` (XML and their SQLite copies). In charts-daily,
//! day `idx` from 1970-01-01, a Thursday, moves $idx from Income to Bank (`test-charts.scm:195-200`), so Income's
//! total for a weekday is minus the sum of the indices that fall on it: 0 to 100 holds 15 indices of each residue 0
//! to 2 (Thursday, Friday, Saturday: 735, 750, 765) and 14 of 3 to 6 (Sunday to Wednesday: 679, 693, 707, 721).
//! Its 2024 transactions (`:177-185`) fall on 2024-01-11 (a Thursday), 02-11 (Sunday), 02-21 (void), 02-22
//! (Thursday), 03-01 (Friday), 05-10 (void), 07-10 (Wednesday) and 09-10 (Tuesday). The report settings are
//! GnuCash's out-of-the-box ones, the accounting period this calendar year, 2024 for the pinned now.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod chart_reports;

use chart_reports::{Charts, blocks, chart, check_read_only, escaped, values};
use serde_json::{Value, json};

const INCOME: &str = "5e2d129f28d14df881c3e47e3053f604";
const EXPENSE: &str = "dde49fed4ca940959ae7d01b72742530";

/// charts-daily's Income and Expenses (`fixtures/reports/charts-daily.expected.json`).
const DAILY_INCOME: &str = "a0000000000000000000000000000007";
/// test-data's Income and Income-GBP (`fixtures/reports/test-data.expected.json`).
const TD_INCOME: &str = "a0000000000000000000000000000006";
const TD_INCOME_GBP: &str = "a0000000000000000000000000000007";

/// The options of a run from `from` to `to` with `more` sections.
fn dated(from: &str, to: &str, more: Value) -> Value {
    let mut options = more;
    if options.is_null() {
        options = json!({});
    }
    let general = options.as_object_mut().unwrap().entry("General").or_insert_with(|| json!({}));
    general["Start Date"] = json!({ "absolute": from });
    general["End Date"] = json!({ "absolute": to });
    options
}

/// A pie as a hand-worked case expects it: its two title lines and each slice's label and value.
struct Pie<'a> {
    title: [&'a str; 2],
    slices: &'a [(&'a str, &'a str)],
}

/// Run `template` with `options` and check the result is the pie `want`: one series named after the template
/// (DR:269-273), a colour per slice from GnuCash's palette, the values exact, and the export showing every label.
fn check_pie(charts: &mut Charts, template: &str, options: Value, want: &Pie<'_>) -> Value {
    let result = charts.run(template, &options);
    let c = chart(&result);
    assert_eq!(c["type"], "pie");
    assert_eq!(c["title"], json!(want.title), "{options}");
    let labels: Vec<&str> = want.slices.iter().map(|(l, _)| *l).collect();
    assert_eq!(c["labels"], json!(labels), "{options}");
    let series = &c["series"][0];
    assert_eq!(series["label"], result["report"]["name"], "the series is named after the template");
    let want_values: Vec<&str> = want.slices.iter().map(|(_, v)| *v).collect();
    assert_eq!(values(series), want_values, "{options}");
    let palette = ["#ff4136", "#ff851b", "#ffdc00", "#2ecc40", "#0074d9", "#001f3f", "#85144b"];
    assert_eq!(series["colors"], json!(palette[..labels.len()]), "GnuCash's colours (HU:226-245)");
    let html = charts.export(template, &options);
    want.title
        .iter()
        .chain(&labels)
        .for_each(|t| assert!(html.contains(&escaped(t)), "{t} in the export"));
    result
}

/// The one warning block of a result: its title.
fn warning_title(result: &Value) -> String {
    let b = blocks(result);
    assert_eq!(b.len(), 1, "{result}");
    assert_eq!(b[0]["kind"], "warning", "{result}");
    b[0]["title"].as_str().unwrap().to_owned()
}

/// The options of DR:68-106 for both templates, in GnuCash's order: "Include Sub-Accounts" before "Accounts", which
/// share the key "a"; the price source weighted-average; the accounts every account of the template's type.
#[test]
fn options_are_gnucash_s() {
    let mut charts = Charts::open("charts-daily.gnucash", "iso");
    [
        (INCOME, "Income vs. Day of Week", "a0000000000000000000000000000007"),
        (EXPENSE, "Expenses vs. Day of Week", "a0000000000000000000000000000009"),
    ]
    .iter()
    .for_each(|(template, name, account)| {
        let described = charts.call("report.options", json!({ "report": template }));
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
            "Accounts/Include Sub-Accounts \"a\" true".to_owned(),
            format!("Accounts/Accounts \"a\" [\"{account}\"]"),
            "Display/Show Totals \"b\" true".to_owned(),
            "Display/Plot Width \"da\" {\"percent\":100}".to_owned(),
            "Display/Plot Height \"db\" {\"percent\":100}".to_owned(),
            format!("General/Report name \"0a\" \"{name}\""),
            "General/Stylesheet \"0b\" \"Default\"".to_owned(),
            "General/Add options summary \"0c\" \"never\"".to_owned(),
            "General/Start Date \"aa\" {\"relative\":\"start-accounting-period\"}".to_owned(),
            "General/End Date \"ab\" {\"relative\":\"end-accounting-period\"}".to_owned(),
            "General/Report's currency \"b\" \"CURRENCY::USD\"".to_owned(),
            "General/Price Source \"c\" \"weighted-average\"".to_owned(),
        ];
        assert_eq!(got, want, "{name}");
    });
    let listed = charts.call("report.list", json!({}));
    let reports = listed["reports"].as_array().unwrap();
    [
        (INCOME, "Shows a piechart with the total income for each day of the week"),
        (EXPENSE, "Shows a piechart with the total expenses for each day of the week"),
    ]
    .iter()
    .for_each(|(guid, tip)| {
        let t = reports.iter().find(|r| r["guid"] == *guid).unwrap();
        assert_eq!((&t["menuPath"], &t["menuTip"]), (&json!(["E_xamples"]), &json!(tip)), "DR:291-310");
    });
}

/// GnuCash's null test (`test-charts.scm:71-77`): each template with its defaults on an empty book renders. No
/// accounts of the type means none selected, which gives the empty-data warning, not the no-account one (DR:283-286);
/// the result is titled with the "Report name" option, where GnuCash's document has none.
#[test]
fn the_null_test_renders_the_warning() {
    let mut charts = Charts::new("iso");
    let book = charts.path().join("book.gnucash");
    charts.call("book.create", json!({ "path": book.display().to_string() }));
    [(INCOME, "Income vs. Day of Week"), (EXPENSE, "Expenses vs. Day of Week")]
        .iter()
        .for_each(|(t, name)| {
            let result = charts.run(t, &json!({}));
            assert_eq!(result["title"], *name);
            assert_eq!(warning_title(&result), "No data");
        });
}

/// GnuCash's render test (`test-charts.scm:187-190`): book A with Start Date `start-cal-year` and End Date
/// `end-cal-year`. Its Accounts value (Bank and Liability) is refused by GnuCash's validation, as neither is of the
/// template's type, so the defaults run; red-ale refuses it outright (`report-option`). In 2024, Income has $29 on
/// a Sunday and $8 on a Tuesday, the voided $10 left out (DR:191-192); Expenses $8 and $27 on Thursdays, $15 on a
/// Friday and -$11 on a Wednesday. Days with no total are left out (DR:235-237), and income stays negative.
#[test]
fn the_render_test_on_book_a() {
    let mut charts = Charts::open("charts-daily.gnucash", "iso");
    let options = json!({ "General": { "Start Date": { "relative": "start-cal-year" }, "End Date": { "relative": "end-cal-year" } } });
    let income = Pie {
        title: ["Income vs. Day of Week", "2024-01-01 to 2024-12-31: -$37.00"],
        slices: &[("Sunday - -$29.00", "-29"), ("Tuesday - -$8.00", "-8")],
    };
    check_pie(&mut charts, INCOME, options.clone(), &income);
    let expense = Pie {
        title: ["Expenses vs. Day of Week", "2024-01-01 to 2024-12-31: $39.00"],
        slices: &[
            ("Wednesday - -$11.00", "-11"),
            ("Thursday - $35.00", "35"),
            ("Friday - $15.00", "15"),
        ],
    };
    check_pie(&mut charts, EXPENSE, options, &expense);
    let refused = charts.err(
        "report.run",
        json!({ "report": INCOME, "options": { "Accounts": { "Accounts": ["a0000000000000000000000000000006"] } } }),
    );
    assert_eq!(refused.code, 3002, "Bank is not an INCOME account");
    assert_eq!(
        charts.run(INCOME, &json!({}))["title"],
        "Income vs. Day of Week",
        "the defaults are this year"
    );
}

/// 1970 on charts-daily: every weekday of Income's 101 daily transactions, Sunday first (DR:220-237), as the
/// module comment works them; the title's total is all seven, -$5,050.00 (DR:260-266). Labels and title show each
/// figure as `gnc:monetary->string` does.
#[test]
fn income_by_weekday_in_1970() {
    let mut charts = Charts::open("charts-daily.gnucash", "iso");
    let want = Pie {
        title: ["Income vs. Day of Week", "1970-01-01 to 1970-12-31: -$5,050.00"],
        slices: &[
            ("Sunday - -$679.00", "-679"),
            ("Monday - -$693.00", "-693"),
            ("Tuesday - -$707.00", "-707"),
            ("Wednesday - -$721.00", "-721"),
            ("Thursday - -$735.00", "-735"),
            ("Friday - -$750.00", "-750"),
            ("Saturday - -$765.00", "-765"),
        ],
    };
    let result = check_pie(&mut charts, INCOME, dated("1970-01-01", "1970-12-31", Value::Null), &want);
    assert_eq!(result["title"], "Income vs. Day of Week");
    let c = chart(&result);
    assert_eq!(c["yAxis"]["commodity"], "CURRENCY::USD");
    assert_eq!(c["grid"], false, "the axes are hidden (DR:275)");
    let mut sqlite = Charts::open_sqlite("charts-daily.gnucash", "iso");
    let from_sqlite = sqlite.run(INCOME, &dated("1970-01-01", "1970-12-31", Value::Null));
    assert_eq!(from_sqlite["sections"], result["sections"], "the SQLite copy gives the same pie");
}

/// "Show Totals" off: the day names alone and no total in the title (DR:238-247, :260-267). "Report name" titles
/// the chart and the result, while the series keeps the template's name (DR:255-257, :269-271).
#[test]
fn show_totals_off_and_a_report_name() {
    let mut charts = Charts::open("charts-daily.gnucash", "iso");
    let options = dated(
        "2024-01-01",
        "2024-12-31",
        json!({ "Display": { "Show Totals": false }, "General": { "Report name": "Days" } }),
    );
    let want = Pie {
        title: ["Days", "2024-01-01 to 2024-12-31"],
        slices: &[("Sunday", "-29"), ("Tuesday", "-8")],
    };
    let result = check_pie(&mut charts, INCOME, options, &want);
    assert_eq!(result["title"], "Days");
    assert_eq!(chart(&result)["series"][0]["label"], "Income vs. Day of Week");
}

/// Only zero left, or nothing found: the empty-data warning (DR:278-281), titled with the "Report name" option. On
/// 1970-01-01 Income's one transaction is day 0, $0.00; 1980 has none.
#[test]
fn zero_totals_and_no_splits_give_the_warning() {
    let mut charts = Charts::open("charts-daily.gnucash", "iso");
    let zero = charts.run(INCOME, &dated("1970-01-01", "1970-01-01", Value::Null));
    assert_eq!(
        (warning_title(&zero), zero["title"].as_str()),
        ("No data".to_owned(), Some("Income vs. Day of Week"))
    );
    let none = charts.run(
        EXPENSE,
        &dated("1980-01-01", "1980-12-31", json!({ "General": { "Report name": "Mine" } })),
    );
    assert_eq!((warning_title(&none), none["title"].as_str()), ("No data".to_owned(), Some("Mine")));
    let selected = json!({ "Accounts": { "Accounts": [DAILY_INCOME] } });
    let one = charts.run(INCOME, &dated("1970-01-01", "1970-01-01", selected));
    assert_eq!(warning_title(&one), "No data");
}

/// test-data, open for writing, with a child of Income that receives $7 on 1978-01-05, a Thursday. Income's own
/// January 1978 receipts are $103 on the 3rd (a Tuesday) and $109 on the 9th (a Monday) (TEE:470-477). With
/// "Include Sub-Accounts" (the default) the child counts; without, only the accounts selected (DR:196-202).
#[test]
fn include_sub_accounts() {
    let mut charts = Charts::open("test-data.gnucash", "iso");
    charts.call(
        "account.create",
        json!({ "name": "Child", "type": "INCOME", "parent": "Root:Income", "commodity": "CURRENCY::USD" }),
    );
    let splits = json!([{ "account": "Root:Asset:Bank", "value": "7" }, { "account": "Root:Income:Child", "value": "-7" }]);
    charts.call(
        "transaction.post",
        json!({ "datePosted": "1978-01-05", "currency": "CURRENCY::USD", "splits": splits }),
    );
    let only_income = |subs: bool| {
        dated(
            "1978-01-01",
            "1978-01-31",
            json!({ "Accounts": { "Accounts": [TD_INCOME], "Include Sub-Accounts": subs } }),
        )
    };
    let with = Pie {
        title: ["Income vs. Day of Week", "1978-01-01 to 1978-01-31: -$219.00"],
        slices: &[
            ("Monday - -$109.00", "-109"),
            ("Tuesday - -$103.00", "-103"),
            ("Thursday - -$7.00", "-7"),
        ],
    };
    check_pie(&mut charts, INCOME, only_income(true), &with);
    let without = Pie {
        title: ["Income vs. Day of Week", "1978-01-01 to 1978-01-31: -$212.00"],
        slices: &[("Monday - -$109.00", "-109"), ("Tuesday - -$103.00", "-103")],
    };
    check_pie(&mut charts, INCOME, only_income(false), &without);
}

/// Income-GBP in 2000 on test-data, with #10 received on 2000-01-20 (a Thursday) and 2000-02-20 (a Sunday), each
/// amount converted at its own posted date (DR:147-149, :222-228). The book's GBP-USD prices are 7/5 from the
/// 2000-01-15 transfer (quoted USD in GBP, 10/14) and 3/2 from the 2000-02-15 one.
///
/// - `pricedb-nearest` (CU:708-721): the nearest price either way round, 7/5 for the first ($14.00) and 3/2 for the
///   second ($15.00).
/// - `weighted-average`, the default (CU:860-867): the running average of the book's GBP exchanges up to the end of
///   the split's day, the nearest entry to its date (CU:279-290): only the first exchange by 2000-01-20, 14/10;
///   both by 2000-02-20, (14 + 9) / (10 + 6) = 23/16, so #10 is $14.375, kept exact: the label prints it whole
///   and the value is 115/8.
/// - 1978, before any exchange: the weighted average has no price, so every total is 0 and the report gives the
///   warning (CU:752-756); the nearest price, 2000-01-15's, gives each #51 receipt $71.40.
#[test]
fn a_foreign_account_converted_at_each_date() {
    let mut charts = Charts::open("test-data.gnucash", "iso");
    ["2000-01-20", "2000-02-20"].iter().for_each(|date| {
        let splits = json!([{ "account": "Root:Asset:GBP Bank", "value": "10" }, { "account": "Root:Income-GBP", "value": "-10" }]);
        charts.call(
            "transaction.post",
            json!({ "datePosted": date, "currency": "CURRENCY::GBP", "splits": splits }),
        );
    });
    let gbp = |source: &str, from: &str, to: &str| {
        dated(
            from,
            to,
            json!({ "Accounts": { "Accounts": [TD_INCOME_GBP] }, "General": { "Price Source": source } }),
        )
    };
    let nearest = Pie {
        title: ["Income vs. Day of Week", "2000-01-01 to 2000-12-31: -$29.00"],
        slices: &[("Sunday - -$15.00", "-15"), ("Thursday - -$14.00", "-14")],
    };
    check_pie(&mut charts, INCOME, gbp("pricedb-nearest", "2000-01-01", "2000-12-31"), &nearest);
    let weighted = Pie {
        title: ["Income vs. Day of Week", "2000-01-01 to 2000-12-31: -$28.375"],
        slices: &[("Sunday - -$14.375", "-115/8"), ("Thursday - -$14.00", "-14")],
    };
    let result = check_pie(&mut charts, INCOME, gbp("weighted-average", "2000-01-01", "2000-12-31"), &weighted);
    assert_eq!(
        chart(&result)["series"][0]["displays"],
        json!(["-$14.38", "-$14.00"]),
        "the slice shown at the SCU"
    );

    let before = charts.run(INCOME, &gbp("weighted-average", "1978-01-01", "1978-12-31"));
    assert_eq!(warning_title(&before), "No data");
    let c = chart(&charts.run(INCOME, &gbp("pricedb-nearest", "1978-01-01", "1978-12-31")));
    // The 8th of each month of 1978: two Sundays, a Monday, a Tuesday, three Wednesdays, a Thursday, two Fridays and
    // two Saturdays.
    assert_eq!(
        values(&c["series"][0]),
        ["-714/5", "-357/5", "-357/5", "-1071/5", "-357/5", "-714/5", "-714/5"]
    );
}

/// Both templates read the Book and write nothing, on XML and SQLite.
#[test]
fn a_run_changes_nothing() {
    let options = dated("1970-01-01", "2024-12-31", Value::Null);
    check_read_only(&mut Charts::open("charts-daily.gnucash", "iso"), &[INCOME, EXPENSE], &options);
    check_read_only(
        &mut Charts::open_sqlite("charts-daily.gnucash", "iso"),
        &[INCOME, EXPENSE],
        &options,
    );
    check_read_only(&mut Charts::open_sqlite("test-data.gnucash", "iso"), &[INCOME, EXPENSE], &options);
}
