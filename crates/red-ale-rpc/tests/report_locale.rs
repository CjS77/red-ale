//! Reports under a Locale (`design/milestone3.md`, "Locales"): the `locale` param of `report.run` and
//! `report.export`, the Configuration's `locale` setting, and what changes from the C locale's output.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::{Value, json};
use statements::{Setup, test_settings};

const BALANCE_SHEET: &str = "Balance Sheet";

/// The fixture's figures in USD. Left unset, the report's currency is the Locale's: EUR under fr_FR.
fn options() -> Value {
    json!({ "General": { "Balance Sheet Date": { "absolute": "2026-12-31" } }, "Commodities": { "Report's currency": "USD" },
            "Accounts": { "Levels of Subaccounts": "all" } })
}

fn fixture(config: &Value) -> Setup {
    Setup::fixture("balsheet-pnl.gnucash", config)
}

fn run(s: &mut Setup, report: &str, options: &Value, locale: Option<&str>) -> Value {
    let mut params = json!({ "report": report, "options": options });
    if let Some(l) = locale {
        params["locale"] = json!(l);
    }
    s.call("report.run", params)
}

/// Every `display` in a result.
fn displays(v: &Value) -> Vec<String> {
    match v {
        Value::Object(map) => map
            .iter()
            .flat_map(|(k, v)| {
                if k == "display" {
                    v.as_str().map(str::to_owned).into_iter().collect()
                } else {
                    displays(v)
                }
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(displays).collect(),
        _ => Vec::new(),
    }
}

/// Every chart block in a result.
fn charts(v: &Value) -> Vec<Value> {
    match v {
        Value::Object(map) if map.get("kind") == Some(&json!("chart")) => vec![v.clone()],
        Value::Object(map) => map.values().flat_map(charts).collect(),
        Value::Array(items) => items.iter().flat_map(charts).collect(),
        _ => Vec::new(),
    }
}

/// `roadmap.md`'s example: under en_US the Balance Sheet's date is `12/31/2026` where the C locale prints `12/31/26`.
#[test]
fn the_locale_date_format_follows_d_fmt() {
    let mut s = fixture(&test_settings("locale"));
    assert_eq!(run(&mut s, BALANCE_SHEET, &options(), None)["title"], " Balance Sheet 12/31/26");
    assert_eq!(
        run(&mut s, BALANCE_SHEET, &options(), Some("C"))["title"],
        " Balance Sheet 12/31/26"
    );
    assert_eq!(
        run(&mut s, BALANCE_SHEET, &options(), Some("en_US"))["title"],
        " Balance Sheet 12/31/2026"
    );
    assert_eq!(
        run(&mut s, BALANCE_SHEET, &options(), Some("fr_FR.UTF-8"))["title"],
        " Balance Sheet 31/12/2026"
    );
    assert_eq!(
        run(&mut s, BALANCE_SHEET, &options(), Some("de_DE"))["title"],
        " Balance Sheet 31.12.2026"
    );
}

/// Under fr_FR amounts use `,` for the point and NARROW NO-BREAK SPACE between groups, with the symbol after the
/// amount; under C they are as GnuCash's own tests expect.
#[test]
fn amounts_follow_the_locale() {
    let mut s = fixture(&test_settings("locale"));
    let c = displays(&run(&mut s, BALANCE_SHEET, &options(), None));
    assert!(c.iter().any(|d| d == "$122,743.52"), "{c:?}");
    assert!(c.iter().any(|d| d == "-$103,600.00"), "{c:?}");
    assert!(c.iter().any(|d| d == "30. FUNDS"), "{c:?}");
    let fr = displays(&run(&mut s, BALANCE_SHEET, &options(), Some("fr_FR")));
    assert!(fr.iter().any(|d| d == "122\u{202f}743,52 $"), "{fr:?}");
    assert!(fr.iter().any(|d| d == "-103\u{202f}600,00 $"), "{fr:?}");
    assert!(fr.iter().any(|d| d == "30, FUNDS"), "{fr:?}");
    assert!(!fr.iter().any(|d| d == "$122,743.52"));
}

/// `gnc_default_report_currency`: with `reports.currency` unset, the report's currency is the Locale's.
#[test]
fn the_default_report_currency_is_the_locales() {
    let mut s = fixture(&test_settings("locale"));
    let currency = |s: &mut Setup, locale| {
        let options = s.call("report.run", json!({ "report": BALANCE_SHEET, "locale": locale }))["options"].clone();
        options["Commodities"]["Report's currency"].clone()
    };
    assert_eq!(currency(&mut s, "C"), "CURRENCY::USD");
    assert_eq!(currency(&mut s, "fr_FR"), "CURRENCY::EUR");
    assert_eq!(currency(&mut s, "en_GB"), "CURRENCY::GBP");
}

/// The Configuration's `locale` setting applies when the call names none; the param wins over it.
#[test]
fn the_setting_applies_unless_the_call_names_a_locale() {
    let mut config = test_settings("locale");
    config["locale"] = json!("en_GB");
    let mut s = fixture(&config);
    assert_eq!(run(&mut s, BALANCE_SHEET, &options(), None)["title"], " Balance Sheet 31/12/26");
    assert_eq!(
        run(&mut s, BALANCE_SHEET, &options(), Some("en_US"))["title"],
        " Balance Sheet 12/31/2026"
    );
    let html = s.call(
        "report.export",
        json!({ "report": BALANCE_SHEET, "options": options(), "locale": "fr_FR" }),
    )["html"]
        .clone();
    assert!(html.as_str().unwrap().contains("122\u{202f}743,52 $"));
}

/// The other date formats do not depend on the Locale.
#[test]
fn fixed_date_formats_ignore_the_locale() {
    let mut s = fixture(&test_settings("iso"));
    assert_eq!(
        run(&mut s, BALANCE_SHEET, &options(), Some("fr_FR"))["title"],
        " Balance Sheet 2026-12-31"
    );
}

/// Under the `locale` format a date range in a chart title takes ICU's interval form, except under C.
#[test]
fn intervals_in_titles_take_icus_form() {
    let mut s = fixture(&test_settings("locale"));
    let general =
        json!({ "Start Date": { "absolute": "1970-01-01" }, "End Date": { "absolute": "1971-12-31" }, "Report's currency": "USD" });
    let dates = json!({ "General": general });
    let title = |s: &mut Setup, locale| {
        let r = run(s, "Net Worth Barchart", &dates, locale);
        let chart = charts(&r).into_iter().next().unwrap_or_else(|| panic!("no chart: {r}"));
        chart["title"][1].as_str().unwrap().to_owned()
    };
    assert_eq!(title(&mut s, None), "01/01/70 to 12/31/71");
    assert_eq!(title(&mut s, Some("en_US")), "1/1/1970\u{2009}–\u{2009}12/31/1971");
    assert_eq!(title(&mut s, Some("de_DE")), "01.01.1970\u{2009}–\u{2009}31.12.1971");
}

#[test]
fn an_unsupported_locale_is_invalid_params() {
    let mut s = fixture(&test_settings("locale"));
    let e = s.err("report.run", json!({ "report": BALANCE_SHEET, "locale": "xx_YY" }));
    assert_eq!(e.code, -32602, "{e:?}");
    assert!(e.message.contains("xx_YY"), "{}", e.message);
}
