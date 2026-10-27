use std::sync::Arc;

use red_ale_core::time::RelativeDate;
use red_ale_core::{Account, AccountType, Book, CommodityRef, FixedEnv, Guid, Time64, TimeZone};
use serde_json::{Value, json};

use super::*;
use crate::number::Rational;
use crate::result::{DisplayContext, PlotSize};
use crate::settings::DateFormat;

fn guid(n: u8) -> Guid {
    Guid::from_bytes([n; 16])
}

/// Root → Assets (ASSET) → Bank (BANK) and Cash (CASH); Income (INCOME); Expenses (EXPENSE).
fn book() -> Book {
    let mut env = FixedEnv::new(Time64(0), 99);
    let mut book = Book::new_empty(&mut env);
    let usd = Some(CommodityRef::currency("USD"));
    let accounts = [
        (1, "Assets", AccountType::Asset, book.root),
        (2, "Bank", AccountType::Bank, guid(1)),
        (3, "Cash", AccountType::Cash, guid(1)),
        (4, "Income", AccountType::Income, book.root),
        (5, "Expenses", AccountType::Expense, book.root),
    ];
    accounts.into_iter().for_each(|(n, name, t, parent)| {
        book.accounts
            .insert(guid(n), Arc::new(Account::new(guid(n), name, t, usd.clone(), Some(parent))));
    });
    book
}

fn c(section: &str, name: &str, key: &str) -> Classifier {
    Classifier::new(section, name, key, format!("Help for {name}."))
}

fn choices() -> Vec<Choice> {
    vec![
        Choice::new("immediate-bal", "Account Balance", "Show only the balance."),
        Choice::new("omit-bal", "Omit Balance", ""),
    ]
}

/// A database with one option of each type, declared out of order.
fn db() -> OptionDb {
    let mut db = OptionDb::new();
    db.end_date(c("General", "End Date", "ab"), true).unwrap();
    db.start_date(c("General", "Start Date", "aa"), true).unwrap();
    db.boolean(c("Display", "Show Totals", "b"), true).unwrap();
    db.boolean(c("Display", "Show Percents", "b"), false).unwrap();
    db.string(c("General", "Report Title", "a"), "Balance Sheet").unwrap();
    db.color(c("Colors", "Background Color", "a"), "ffffff").unwrap();
    db.multichoice(c("Display", "Parent account balances", "c"), "immediate-bal".into(), choices())
        .unwrap();
    let levels = std::iter::once(Choice::new("all", "All", ""))
        .chain((1..=6).map(|n| Choice::new(n, n.to_string(), "")))
        .collect();
    db.multichoice(c("Accounts", "Levels of Subaccounts", "b"), "all".into(), levels)
        .unwrap();
    db.list(c("Display", "Columns", "d"), vec!["omit-bal".into()], choices()).unwrap();
    let n = |s: &str| Rational::parse(s).unwrap();
    db.number_range(c("Display", "Maximum Slices", "e"), n("7"), n("2"), n("255"), n("1"))
        .unwrap();
    db.number_range(c("Display", "Line Width", "f"), n("1.5"), n("0.5"), n("5"), n("0.1"))
        .unwrap();
    db.plot_size(c("Display", "Plot Width", "g"), PlotSize::Percent(100)).unwrap();
    db.account_list(
        c("Accounts", "Accounts", "a"),
        vec![],
        vec![AccountType::Bank, AccountType::Cash, AccountType::Asset],
        true,
    )
    .unwrap();
    db.account_sel(c("Accounts", "Income Account", "c"), None, vec![AccountType::Income])
        .unwrap();
    db.currency(c("General", "Report's currency", "c"), CommodityRef::currency("USD"))
        .unwrap();
    db.commodity(c("General", "Price of Commodity", "d"), CommodityRef::currency("USD"))
        .unwrap();
    db.internal(c("__internal", "flag", "a"), OptionValue::Bool(false)).unwrap();
    db.add_general_options("Balance Sheet", &["Default", "Easy", "Footer"]).unwrap();
    db
}

fn ctx_with<'a>(book: Option<&'a Book>, zone: &'a TimeZone) -> ValueContext<'a> {
    ValueContext { book, zone }
}

fn apply(db: &mut OptionDb, changes: &Value) -> Result<(), crate::error::OptionError> {
    let book = book();
    let zone = TimeZone::utc();
    db.apply(changes, &ctx_with(Some(&book), &zone))
}

fn reason(changes: &Value) -> String {
    let mut db = db();
    let before = db.clone();
    let e = apply(&mut db, changes).unwrap_err();
    assert_eq!(db, before, "a refused change leaves every option as it was");
    e.reason
}

fn value_of(db: &OptionDb, section: &str, name: &str) -> Value {
    db.values()[section][name].clone()
}

#[test]
fn options_come_in_gnucash_order() {
    let db = db();
    let order: Vec<(&str, &str)> = db.options().iter().map(|o| (o.section(), o.name())).collect();
    assert_eq!(
        &order[..3],
        &[
            ("Accounts", "Accounts"),
            ("Accounts", "Levels of Subaccounts"),
            ("Accounts", "Income Account")
        ]
    );
    let display: Vec<&str> = order.iter().filter(|(s, _)| *s == "Display").map(|(_, n)| *n).collect();
    assert_eq!(
        &display[..2],
        &["Show Totals", "Show Percents"],
        "equal keys keep declaration order"
    );
    let general: Vec<&str> = order.iter().filter(|(s, _)| *s == GENERAL).map(|(_, n)| *n).collect();
    let want = [
        "Report name",
        "Stylesheet",
        "Add options summary",
        "Report Title",
        "Start Date",
        "End Date",
        "Report's currency",
    ];
    assert_eq!(general[..7], want);
    assert_eq!(general[7..], ["Price of Commodity"]);
    assert_eq!(order.last(), Some(&("__internal", "flag")));
}

#[test]
fn general_options_are_added_once() {
    let mut db = db();
    assert_eq!(value_of(&db, GENERAL, REPORT_NAME), json!("Balance Sheet"));
    assert_eq!(value_of(&db, GENERAL, STYLESHEET), json!("Default"));
    assert_eq!(value_of(&db, GENERAL, OPTIONS_SUMMARY), json!("never"));
    db.add_general_options("Other", &["Default"]).unwrap();
    assert_eq!(value_of(&db, GENERAL, REPORT_NAME), json!("Balance Sheet"));
    assert!(
        apply(&mut db, &json!({ "General": { "Stylesheet": "Missing" } }))
            .unwrap_err()
            .reason
            .contains("not one of the choices")
    );
}

#[test]
fn declarations_follow_gnucash_constructors() {
    let mut db = OptionDb::new();
    db.multichoice(c("A", "m", "a"), "nope".into(), choices()).unwrap();
    assert_eq!(
        db.get("A", "m").unwrap().value(),
        &OptionValue::Key("immediate-bal".into()),
        "an unknown default is the first choice"
    );
    db.number_range(
        c("A", "n", "b"),
        Rational::from_int(50),
        Rational::from_int(1),
        Rational::from_int(10),
        Rational::one(),
    )
    .unwrap();
    assert_eq!(
        db.get("A", "n").unwrap().value(),
        &OptionValue::Number(Rational::one()),
        "an out-of-range default is the minimum"
    );
    assert!(db.boolean(c("A", "m", "c"), true).is_err(), "declared twice");
    assert!(db.multichoice(c("A", "empty", "d"), "x".into(), vec![]).is_err());
    assert!(db.currency(c("A", "cur", "e"), CommodityRef::new("NASDAQ", "X")).is_err());
    assert!(db.list(c("A", "l", "f"), vec!["zzz".into()], choices()).is_err());
    assert!(
        db.date(c("A", "d", "g"), DateValue::Relative(RelativeDate::Today), DateUi::Absolute, vec![])
            .is_err()
    );
    assert!(
        db.date(
            c("A", "d", "g"),
            DateValue::Relative(RelativeDate::StartThisMonth),
            DateUi::Both,
            END_DATES.to_vec()
        )
        .is_err()
    );
    assert!(
        db.date(c("A", "d", "g"), DateValue::Absolute(Time64(0)), DateUi::Relative, vec![])
            .is_err()
    );
    assert!(db.account_list(c("A", "a", "h"), vec![guid(1), guid(2)], vec![], false).is_err());
    assert!(
        db.register(c("A", "bad", "i"), OptionKind::Boolean, OptionValue::Text("x".into()))
            .is_err()
    );
    db.color(c("A", "col", "j"), "#eexe8xaa").unwrap();
    assert_eq!(
        db.get("A", "col").unwrap().value(),
        &OptionValue::Text("eexe8xaa".into()),
        "an invalid default colour is kept"
    );
    db.set_default_section("A");
    assert_eq!(db.default_section(), Some("A"));
}

#[test]
fn values_are_set_from_their_wire_form() {
    let mut db = db();
    let cash = guid(3).to_string();
    let changes = json!({
        "General": { "Report Title": "Mine", "Start Date": { "absolute": "2024-01-01" }, "End Date": { "relative": "end-this-month" },
                     "Report's currency": "EUR", "Price of Commodity": "CURRENCY::GBP" },
        "Accounts": { "Levels of Subaccounts": 2, "Accounts": ["Assets:Bank", cash],
                      "Income Account": "Income" },
        "Display": { "Show Totals": false, "Parent account balances": "omit-bal", "Columns": ["omit-bal", "immediate-bal"],
                     "Maximum Slices": 12, "Line Width": "2.5", "Plot Width": 400 },
        "Colors": { "Background Color": "#FFEE00" }
    });
    apply(&mut db, &changes).unwrap();
    assert_eq!(
        value_of(&db, "General", "Start Date"),
        json!({ "absolute": "2024-01-01T10:59:00Z" })
    );
    assert_eq!(value_of(&db, "General", "End Date"), json!({ "relative": "end-this-month" }));
    assert_eq!(value_of(&db, "General", "Report's currency"), json!("CURRENCY::EUR"));
    assert_eq!(value_of(&db, "Accounts", "Levels of Subaccounts"), json!(2));
    assert_eq!(
        value_of(&db, "Accounts", "Accounts"),
        json!([guid(2).to_string(), guid(3).to_string()])
    );
    assert_eq!(value_of(&db, "Accounts", "Income Account"), json!(guid(4).to_string()));
    assert_eq!(value_of(&db, "Display", "Columns"), json!(["omit-bal", "immediate-bal"]));
    assert_eq!(value_of(&db, "Display", "Maximum Slices"), json!(12));
    assert_eq!(value_of(&db, "Display", "Line Width"), json!("2.5"));
    assert_eq!(value_of(&db, "Display", "Plot Width"), json!({ "pixels": 400 }));
    assert_eq!(value_of(&db, "Colors", "Background Color"), json!("ffee00"));
    apply(&mut db, &json!({ "Display": { "Show Totals": null, "Columns": [] } })).unwrap();
    assert_eq!(value_of(&db, "Display", "Show Totals"), json!(true), "null returns to the default");
    assert_eq!(
        value_of(&db, "Display", "Columns"),
        json!(["omit-bal"]),
        "an empty list reads as the default"
    );
}

#[test]
fn invalid_values_are_refused_with_a_reason() {
    let cases = [
        (json!({ "Nope": { "x": 1 } }), "unknown option"),
        (json!({ "General": { "Nope": 1 } }), "unknown option"),
        (json!({ "General": 5 }), "object by option name"),
        (json!({ "Display": { "Show Totals": "yes" } }), "true or false"),
        (
            json!({ "Display": { "Parent account balances": "recursive-bal" } }),
            "not one of the choices",
        ),
        (json!({ "Accounts": { "Levels of Subaccounts": "2" } }), "not one of the choices"),
        (json!({ "Display": { "Columns": ["omit-bal", "omit-bal"] } }), "chosen twice"),
        (json!({ "Display": { "Maximum Slices": 300 } }), "outside 2 to 255"),
        (json!({ "Display": { "Maximum Slices": 7.5 } }), "not a whole number"),
        (json!({ "Display": { "Line Width": "0.25" } }), "outside 0.5 to 5"),
        (json!({ "Display": { "Plot Width": { "percent": 150 } } }), "outside 10 to 100"),
        (json!({ "Display": { "Plot Width": { "pixels": 50 } } }), "outside 101 to 65535"),
        (json!({ "Display": { "Plot Width": 5 } }), "outside 10 to 100"),
        (json!({ "Colors": { "Background Color": "white" } }), "not a colour"),
        (
            json!({ "General": { "Start Date": { "relative": "end-this-month" } } }),
            "not offered",
        ),
        (
            json!({ "General": { "Start Date": { "relative": "someday" } } }),
            "not a relative date",
        ),
        (json!({ "General": { "Start Date": { "absolute": "01/02/2024" } } }), "not a date"),
        (
            json!({ "General": { "Start Date": { "absolute": "1300-01-01" } } }),
            "outside the supported range",
        ),
        (json!({ "General": { "Start Date": "2024-01-01" } }), "expected {\"absolute\""),
        (json!({ "General": { "Report's currency": "NASDAQ::ACME" } }), "not a currency"),
        (json!({ "General": { "Report's currency": "XXQ" } }), "unknown commodity"),
        (json!({ "Accounts": { "Accounts": ["Income"] } }), "not one this option allows"),
        (json!({ "Accounts": { "Accounts": ["Assets:Nope"] } }), "unknown account"),
        (json!({ "Accounts": { "Income Account": "Assets" } }), "not one this option allows"),
    ];
    cases.into_iter().for_each(|(changes, want)| {
        let got = reason(&changes);
        assert!(got.contains(want), "{changes}: {got}");
    });
    let mut db = db();
    let e = apply(&mut db, &json!({ "Display": { "Show Totals": false, "Maximum Slices": 1 } })).unwrap_err();
    assert_eq!((e.section.as_str(), e.name.as_str()), ("Display", "Maximum Slices"));
    assert_eq!(
        value_of(&db, "Display", "Show Totals"),
        json!(true),
        "nothing is set when one value fails"
    );
}

#[test]
fn date_options_follow_their_ui_and_the_zone() {
    let mut db = OptionDb::new();
    db.date(c("A", "abs", "a"), DateValue::Absolute(Time64(0)), DateUi::Absolute, vec![])
        .unwrap();
    db.date(
        c("A", "rel", "b"),
        DateValue::Relative(RelativeDate::Today),
        DateUi::Relative,
        vec![],
    )
    .unwrap();
    let book = book();
    let plus_two = TimeZone::fixed(2 * 3600).unwrap();
    let ctx = ctx_with(Some(&book), &plus_two);
    db.apply(
        &json!({ "A": { "abs": { "absolute": "2024-06-01" }, "rel": { "relative": "start-prev-fin-year" } } }),
        &ctx,
    )
    .unwrap();
    assert_eq!(
        db.values()["A"]["abs"],
        json!({ "absolute": "2024-06-01T08:59:00Z" }),
        "local 10:59 that day"
    );
    assert_eq!(
        db.values()["A"]["rel"],
        json!({ "relative": "start-accounting-period" }),
        "the storage name is read as an alias"
    );
    let refused = |changes: Value| db.clone().apply(&changes, &ctx).unwrap_err().reason;
    assert!(refused(json!({ "A": { "abs": { "relative": "today" } } })).contains("only absolute"));
    assert!(refused(json!({ "A": { "rel": { "absolute": "2024-06-01" } } })).contains("only relative"));
    let rfc = db
        .clone()
        .apply(&json!({ "A": { "abs": { "absolute": "2024-06-01T23:30:00-01:00" } } }), &ctx);
    assert!(rfc.is_ok());
}

#[test]
fn accounts_without_a_book_need_guids() {
    let mut db = db();
    let zone = TimeZone::utc();
    let ctx = ctx_with(None, &zone);
    let g = guid(42).to_string();
    db.apply(&json!({ "Accounts": { "Accounts": [g] } }), &ctx).unwrap();
    assert_eq!(value_of(&db, "Accounts", "Accounts"), json!([g]));
    let e = db.apply(&json!({ "Accounts": { "Accounts": ["Assets:Bank"] } }), &ctx).unwrap_err();
    assert!(e.reason.contains("needs an open book"), "{}", e.reason);
    db.apply(&json!({ "General": { "Report's currency": "XXQ" } }), &ctx).unwrap();
}

#[test]
fn account_options_fall_back_to_the_first_allowed_account() {
    let mut db = db();
    let book = book();
    let accounts = db.get("Accounts", "Accounts").unwrap();
    assert_eq!(
        accounts.accounts(&book),
        vec![guid(1)],
        "Assets is the first allowed account in display order"
    );
    assert_eq!(db.get("Accounts", "Income Account").unwrap().accounts(&book), vec![guid(4)]);
    assert!(db.get("Display", "Show Totals").unwrap().accounts(&book).is_empty());
    apply(&mut db, &json!({ "Accounts": { "Accounts": ["Assets:Cash"] } })).unwrap();
    assert_eq!(db.get("Accounts", "Accounts").unwrap().accounts(&book), vec![guid(3)]);
    db.account_list(c("X", "single", "a"), vec![], vec![AccountType::Bank], false)
        .unwrap();
    assert!(reason_on(&mut db, &json!({ "X": { "single": ["Assets:Bank", "Assets:Bank"] } })).contains("one account"));
}

fn reason_on(db: &mut OptionDb, changes: &Value) -> String {
    apply(db, changes).unwrap_err().reason
}

#[test]
fn changed_keeps_only_what_differs_from_the_defaults() {
    let mut db = db();
    assert!(db.changed().is_empty());
    apply(
        &mut db,
        &json!({ "Accounts": { "Levels of Subaccounts": 2 }, "Display": { "Show Totals": true } }),
    )
    .unwrap();
    assert_eq!(Value::Object(db.changed()), json!({ "Accounts": { "Levels of Subaccounts": 2 } }));
    let mut again = self::db();
    apply(&mut again, &Value::Object(db.changed())).unwrap();
    assert_eq!(again.values(), db.values(), "a saved diff restores the same values");
    let keys: Vec<String> = db.values().keys().cloned().collect();
    assert_eq!(keys, ["Accounts", "Colors", "Display", "General", "__internal"]);
}

#[test]
fn describe_lists_each_option_as_report_options_does() {
    let db = db();
    let info = db.describe();
    let start = info.iter().find(|i| i.name == "Start Date").unwrap();
    assert_eq!(
        serde_json::to_value(start).unwrap(),
        json!({ "section": "General", "name": "Start Date", "key": "aa", "type": "date", "help": "Help for Start Date.",
                "default": { "relative": "start-accounting-period" }, "value": { "relative": "start-accounting-period" },
                "ui": "both",
                "relative": ["today", "start-this-month", "start-prev-month", "start-current-quarter", "start-prev-quarter",
                             "start-cal-year", "start-prev-year", "start-accounting-period"] })
    );
    let width = serde_json::to_value(info.iter().find(|i| i.name == "Line Width").unwrap()).unwrap();
    assert_eq!(
        (width["min"].clone(), width["step"].clone()),
        (json!({ "num": "1", "denom": "2" }), json!({ "num": "1", "denom": "10" }))
    );
    let accounts = serde_json::to_value(info.iter().find(|i| i.name == "Accounts").unwrap()).unwrap();
    assert_eq!(
        (accounts["accountTypes"].clone(), accounts["multiple"].clone()),
        (json!(["BANK", "CASH", "ASSET"]), json!(true))
    );
    let parent = serde_json::to_value(info.iter().find(|i| i.name == "Parent account balances").unwrap()).unwrap();
    assert_eq!(
        parent["choices"][0],
        json!({ "key": "immediate-bal", "label": "Account Balance", "help": "Show only the balance." })
    );
}

#[test]
fn the_params_schema_follows_the_declarations() {
    let s = db().params_schema();
    assert_eq!(s["$schema"], json!("https://json-schema.org/draft/2020-12/schema"));
    assert_eq!(s["additionalProperties"], json!(false));
    let p = |section: &str, name: &str| s["properties"][section]["properties"][name].clone();
    assert_eq!(p("Accounts", "Levels of Subaccounts")["enum"], json!(["all", 1, 2, 3, 4, 5, 6]));
    assert_eq!(
        p("Display", "Maximum Slices")["oneOf"][0],
        json!({ "type": "integer", "minimum": 2, "maximum": 255 })
    );
    assert_eq!(
        p("Display", "Line Width")["oneOf"][0],
        json!({ "type": "integer", "minimum": 1, "maximum": 5 })
    );
    assert_eq!(
        p("General", "Start Date")["oneOf"][1]["properties"]["relative"]["enum"][7],
        json!("start-accounting-period")
    );
    assert_eq!(p("General", "End Date")["default"], json!({ "relative": "end-accounting-period" }));
    assert_eq!(
        p("Display", "Show Totals"),
        json!({ "type": "boolean", "description": "Help for Show Totals.", "default": true })
    );
    assert_eq!(p("Accounts", "Accounts")["type"], json!("array"));
    assert_eq!(p("__internal", "flag")["type"], json!("boolean"));
    assert_eq!(s["properties"]["Display"]["additionalProperties"], json!(false));
}

#[test]
fn the_summary_lists_changed_options_as_gnucash_shows_them() {
    let mut db = db();
    apply(
        &mut db,
        &json!({ "General": { "Add options summary": "always", "Start Date": { "absolute": "2024-01-01" },
                             "End Date": { "relative": "today" }, "Report's currency": "EUR" },
                "Accounts": { "Accounts": ["Assets:Bank", "Assets:Cash"], "Levels of Subaccounts": 2 },
                "Display": { "Show Totals": false, "Maximum Slices": 12, "Line Width": "2.5", "Plot Width": 400 },
                "__internal": { "flag": true } }),
    )
    .unwrap();
    let book = book();
    let zone = TimeZone::utc();
    let rows = db.summary(&DisplayContext {
        book: &book,
        zone: &zone,
        date_format: DateFormat::Iso,
        locale: red_ale_locale::Locale::c(),
    });
    let shown: Vec<(String, String)> = rows.into_iter().map(|r| (format!("{} / {}", r.section, r.name), r.value)).collect();
    let want = [
        ("Accounts / Accounts", "Assets:Bank, Assets:Cash"),
        ("Accounts / Levels of Subaccounts", "2"),
        ("Display / Show Totals", "Disabled"),
        ("Display / Maximum Slices", "12.0"),
        ("Display / Line Width", "2.5"),
        ("Display / Plot Width", "pixels . 400"),
        ("General / Start Date", "absolute . 2024-01-01"),
        ("General / End Date", "relative . today"),
        ("General / Report's currency", "EUR"),
    ];
    let want: Vec<(String, String)> = want.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect();
    assert_eq!(shown, want);
}

/// `book()` with the budgets "Plan" (`guid(7)`) and "Spare" (`guid(8)`).
fn book_with_budgets() -> Book {
    let mut book = book();
    let start = red_ale_core::Date::from_ymd(2024, 1, 1).unwrap();
    let recurrence = red_ale_core::Recurrence::new(1, red_ale_core::PeriodType::Month, start, red_ale_core::WeekendAdjust::None).unwrap();
    [(7, "Plan"), (8, "Spare")].into_iter().for_each(|(n, name)| {
        let budget = red_ale_core::Budget {
            guid: guid(n),
            name: name.into(),
            description: String::new(),
            num_periods: 12,
            recurrence,
            slots: red_ale_core::Frame::new(),
        };
        book.budgets.insert(guid(n), Arc::new(budget));
    });
    book
}

/// A budget option holds a GUID, as GnuCash saves it (`5.17:bindings/guile/options.scm:150-160`): a saved
/// configuration keeps it only when it differs from the default, and reapplying what was kept gives the same options.
/// The summary shows the budget's name (`gnc-budget-get-name`, `5.17:gnucash/report/html-utilities.scm:320`).
#[test]
fn budget_options_round_trip_by_guid() {
    let (book, zone) = (book_with_budgets(), TimeZone::utc());
    let mut db = OptionDb::new();
    db.budget(c("General", "Budget", "a"), Some(guid(7))).unwrap();
    assert_eq!(db.changed(), serde_json::Map::new(), "the default is not saved");
    assert_eq!(value_of(&db, "General", "Budget"), json!(guid(7).to_string()));
    let spare = json!({ "General": { "Budget": guid(8).to_string() } });
    db.apply(&spare, &ctx_with(Some(&book), &zone)).unwrap();
    let saved = Value::Object(db.changed());
    assert_eq!(saved, spare);
    let mut again = OptionDb::new();
    again.budget(c("General", "Budget", "a"), Some(guid(7))).unwrap();
    again.apply(&saved, &ctx_with(None, &zone)).unwrap();
    assert_eq!(again, db);
    let unknown = json!({ "General": { "Budget": guid(9).to_string() } });
    let refused = db.apply(&unknown, &ctx_with(Some(&book), &zone)).unwrap_err();
    assert_eq!(refused.reason, format!("unknown budget {}", guid(9)));
    assert!(db.apply(&unknown, &ctx_with(None, &zone)).is_ok(), "without a book a GUID is kept unchecked");
    db.apply(&spare, &ctx_with(Some(&book), &zone)).unwrap();
    let info = &db.describe()[0];
    assert_eq!((info.option_type.as_str(), &info.default), ("budget", &json!(guid(7).to_string())));
    assert_eq!(db.params_schema()["properties"]["General"]["properties"]["Budget"]["type"], json!("string"));
    let rows = db.summary(&DisplayContext {
        book: &book,
        zone: &zone,
        date_format: DateFormat::Iso,
        locale: red_ale_locale::Locale::c(),
    });
    assert_eq!(rows[0].value, "Spare");
    let mut none = OptionDb::new();
    none.budget(c("General", "Budget", "a"), None).unwrap();
    assert_eq!(value_of(&none, "General", "Budget"), Value::Null, "no budget to default to");
}
