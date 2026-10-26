//! Running a template (`design/reports.md` §2, §6, §7): declaring options, setting them, reading them in `render`,
//! the run context, the finished result, and that a run reads the Book without changing it.

mod common;

use common::{GUID, Host, NOW, empty_book, fixture, registry};
use red_ale_core::time::{PeriodBounds, RelativeDate};
use red_ale_core::{Time64, TimeZone};
use red_ale_report::ReportError;
use red_ale_report::options::DateValue;
use red_ale_report::result::Block;
use serde_json::{Value, json};

const OPTIONS: &str = r##"
fn options(params, o, book) {
    o.boolean("Display", "Show", "a", "Show it.", true);
    o.string("General", "Report Title", "a", "Title for this report.", "Hello");
    o.text("General", "Notes", "aa", "", "multi\nline");
    o.multichoice("Display", "Levels", "b", "", 2, [[1, "One"], [2, "Two", "Two levels"], [3, "Three"]]);
    o.list("Display", "Kinds", "c", "", ["x"], [["x", "X"], ["y", "Y", "why"]]);
    o.number_range("Display", "Width", "d", "", "1.5", 0, 10, "0.1");
    o.plot_size("Display", "Height", "e", "", 400);
    o.color("Display", "Colour", "f", "", "#FF0000");
    o.end_date("General", "End Date", "b", "", true);
    o.date("General", "Fixed", "c", "", "today", #{ ui: "relative", relative: ["today", "start-this-month"] });
    o.account_list("Accounts", "Accounts", "a", "", [], #{ types: ["BANK", "ASSET"] });
    o.account_sel("Accounts", "Income", "b", "", (), #{ types: ["INCOME"] });
    o.currency("General", "Report's currency", "d", "", "USD");
    o.commodity("General", "Commodity", "e", "", "CURRENCY::GBP");
    o.internal("General", "Secret", "z", "", "s");
    o.set_default_section("Display");
}
"##;

/// Prints each option's type and value, one per log line.
const RENDER: &str = r#"
fn show(v) {
    if type_of(v) == "array" { `[${v.map(|x| show(x)).reduce(|a, b| a + "," + b, "")}]` }
    else if type_of(v) == "Account" { `Account:${v.fullName}` }
    else if type_of(v) == "Commodity" { `Commodity:${v.id}` }
    else { `${type_of(v)}:${v}` }
}
fn render(params, book, opts, ctx) {
    let names = [["Display", "Show"], ["General", "Report Title"], ["General", "Notes"], ["Display", "Levels"],
                 ["Display", "Kinds"], ["Display", "Width"], ["Display", "Height"], ["Display", "Colour"],
                 ["General", "End Date"], ["General", "Fixed"], ["Accounts", "Accounts"], ["Accounts", "Income"],
                 ["General", "Report's currency"], ["General", "Commodity"], ["General", "Secret"],
                 ["General", "Report name"], ["General", "Stylesheet"], ["General", "Add options summary"]];
    for n in names { print(`${n[1]}=${show(opts.get(n[0], n[1]))}`); }
    print(`raw=${opts.raw("General", "End Date")}`);
    #{ title: opts.get("General", "Report Title"), sections: [#{ blocks: [#{ kind: "text", text: "body" }] }] }
}
"#;

fn script() -> String {
    format!("fn templates() {{ [#{{ guid: \"{GUID}\", name: \"Options\" }}] }}\n{OPTIONS}\n{RENDER}")
}

fn run(host: &Host, options: &Value) -> Result<red_ale_report::ReportResult, ReportError> {
    host.run(&registry(&[("options.rhai", &script())]), GUID, options)
}

fn log(host: &Host, options: &Value) -> Vec<String> {
    run(host, options).unwrap().log.into_iter().map(|l| l.text).collect()
}

fn bounds() -> PeriodBounds {
    PeriodBounds { start: Time64(1_704_067_200), end: Time64(1_735_689_599) }
}

#[test]
fn render_reads_each_option_as_a_script_value() {
    let host = Host::new(fixture("balsheet-pnl.gnucash"));
    let today = DateValue::Relative(RelativeDate::Today).resolve(NOW, &TimeZone::utc(), bounds()).unwrap().0;
    let want = [
        "Show=bool:true".to_owned(),
        "Report Title=string:Hello".into(),
        "Notes=string:multi\nline".into(),
        "Levels=i64:2".into(),
        "Kinds=[,string:x]".into(),
        "Width=Rational:3/2".into(),
        "Height=map:#{\"unit\": \"pixels\", \"value\": 400}".into(),
        "Colour=string:FF0000".into(),
        "End Date=i64:1735689599".into(),
        format!("Fixed=i64:{today}"),
        "Accounts=[,Account:Root]".into(),
        "Income=Account:Root:Income".into(),
        "Report's currency=Commodity:CURRENCY::USD".into(),
        "Commodity=Commodity:CURRENCY::GBP".into(),
        "Secret=string:s".into(),
        "Report name=string:Options".into(),
        "Stylesheet=string:Default".into(),
        "Add options summary=string:never".into(),
        "raw=#{\"relative\": \"end-accounting-period\"}".into(),
    ];
    assert_eq!(log(&host, &json!({})), want);
}

#[test]
fn the_caller_sets_options_by_section_and_name() {
    let host = Host::new(fixture("balsheet-pnl.gnucash"));
    let options = json!({
        "Display": { "Levels": 3, "Kinds": ["y", "x"], "Width": "2.25", "Height": { "percent": 50 }, "Show": false },
        "General": { "End Date": { "absolute": "2024-01-31" }, "Report Title": "Set" },
        "Accounts": { "Accounts": ["Root:Asset:Bank1:Current", "Root:Asset:House"] }
    });
    let lines = log(&host, &options);
    assert_eq!(lines[0], "Show=bool:false");
    assert_eq!(lines[3], "Levels=i64:3");
    assert_eq!(lines[4], "Kinds=[,string:y,string:x]");
    assert_eq!(lines[5], "Width=Rational:9/4");
    assert_eq!(lines[6], "Height=map:#{\"unit\": \"percent\", \"value\": 50}");
    assert_eq!(lines[8], "End Date=i64:1706698740", "local 10:59:00 of the day");
    assert_eq!(lines[10], "Accounts=[,Account:Root:Asset:Bank1:Current,Account:Root:Asset:House]");
    let result = run(&host, &options).unwrap();
    assert_eq!(result.title, "Set");
    assert_eq!(result.options["Display"]["Levels"], json!(3));
    assert_eq!(result.options["General"]["End Date"], json!({ "absolute": "2024-01-31T10:59:00Z" }));
}

#[test]
fn an_invalid_option_value_is_refused_before_anything_runs() {
    let host = Host::new(fixture("balsheet-pnl.gnucash"));
    let cases = [
        (json!({ "Display": { "Nope": 1 } }), "Display", "Nope", "unknown option"),
        (json!({ "Display": { "Show": "yes" } }), "Display", "Show", "expected true or false"),
        (json!({ "Display": { "Levels": 7 } }), "Display", "Levels", "not one of the choices"),
        (json!({ "Display": { "Width": "10.5" } }), "Display", "Width", "outside"),
        (json!({ "Display": { "Width": 1.5 } }), "Display", "Width", "not a whole number"),
        (json!({ "Accounts": { "Income": "Root:Asset:House" } }), "Accounts", "Income", "not one this option allows"),
        (json!({ "Accounts": { "Accounts": ["No:Such"] } }), "Accounts", "Accounts", "unknown account"),
        (json!({ "General": { "Fixed": { "relative": "end-this-month" } } }), "General", "Fixed", "not offered"),
        (json!({ "General": { "Stylesheet": "Fancy" } }), "General", "Stylesheet", "not one of the choices"),
        (json!({ "General": { "Commodity": "XYZ::NONE" } }), "General", "Commodity", "unknown commodity"),
    ];
    cases.iter().for_each(|(options, section, name, reason)| {
        let e = run(&host, options).unwrap_err();
        let ReportError::Option(o) = &e else { panic!("{options}: {e:?}") };
        assert_eq!((o.section.as_str(), o.name.as_str()), (*section, *name), "{options}");
        assert!(o.reason.contains(reason), "{options}: {}", o.reason);
        assert_eq!(e.kind().code(), 3002);
    });
}

#[test]
fn every_template_gets_the_general_options() {
    let host = Host::new(empty_book());
    let reg = registry(&[("options.rhai", &script())]);
    let db = reg.options(reg.find(GUID).unwrap(), &host.ctx()).unwrap();
    let general: Vec<(String, String)> =
        db.describe().into_iter().filter(|o| o.section == "General").map(|o| (o.key.clone(), o.name.clone())).take(3).collect();
    let want = [("0a", "Report name"), ("0b", "Stylesheet"), ("0c", "Add options summary")].map(|(k, n)| (k.to_owned(), n.to_owned()));
    assert_eq!(general, want);
    let sheet = db.describe().into_iter().find(|o| o.name == "Stylesheet").unwrap();
    let keys: Vec<String> = sheet.choices.unwrap().into_iter().map(|c| c.key.to_string()).collect();
    assert_eq!(keys, ["Default", "Easy"]);
    assert_eq!(db.default_section(), Some("Display"));
    assert_eq!(db.params_schema()["properties"]["Display"]["properties"]["Levels"]["enum"], json!([1, 2, 3]));
}

#[test]
fn the_options_summary_lists_what_changed() {
    let host = Host::new(empty_book());
    let options = json!({ "General": { "Add options summary": "always", "Report Title": "Changed" } });
    let result = run(&host, &options).unwrap();
    let Some(Block::OptionsSummary(summary)) = result.sections.last().and_then(|s| s.blocks.first()) else { panic!("no summary") };
    let rows: Vec<(&str, &str)> = summary.rows.iter().map(|r| (r.name.as_str(), r.value.as_str())).collect();
    assert_eq!(rows, [("Report Title", "Changed")]);
    assert_eq!(run(&host, &json!({})).unwrap().sections.len(), 1, "no summary by default");
}

#[test]
fn running_leaves_the_book_unchanged() {
    let host = Host::new(fixture("balsheet-pnl.gnucash"));
    let before = host.book.clone();
    run(&host, &json!({})).unwrap();
    assert_eq!(host.book, before);
}

#[test]
fn the_same_book_options_and_now_give_the_same_bytes() {
    let host = Host::new(fixture("balsheet-pnl.gnucash"));
    let options = json!({ "Display": { "Kinds": ["y"] } });
    let bytes = || serde_json::to_vec(&run(&host, &options).unwrap()).unwrap();
    assert_eq!(bytes(), bytes());
    let result = run(&host, &options).unwrap();
    assert_eq!(serde_json::to_value(&result).unwrap()["now"], json!("2024-03-15T12:00:00Z"));
    assert_eq!(result.report.name, "Options");
}

#[test]
fn the_run_context_gives_time_in_the_local_zone() {
    let body = r#"
        let t = ctx.now;
        let l = ctx.local(t);
        let p = ctx.accounting_period();
        let lines = [
            `${t} ${ctx.today()} ${ctx.start_of_day(t)} ${ctx.end_of_day(t)}`,
            `${l.year}-${l.month}-${l.day} ${l.hour}:${l.minute}:${l.second} ${l.weekday} ${l.yearday}`,
            `${ctx.make_time(#{ year: 2024, month: 2, day: 31 })} ${ctx.make_time(#{ year: 2023, month: 14, day: 0, hour: 25 })}`,
            `${ctx.print_date(t)}|${ctx.print_date_interval(ctx.make_time(#{ year: 2024, month: 3, day: 1 }), t)}|${ctx.print_time_iso(t)}`,
            `${p.start} ${p.end} ${ctx.report_currency().id} ${ctx.week_start()}`,
        ];
        #{ title: lines.reduce(|a, b| a + "\n" + b, ""), sections: [] }
    "#;
    let title = common::render(body).unwrap().title;
    let want = "\n1710504000 1710504000 1710460800 1710547199\n2024-3-15 12:0:0 6 75\n1709337600 1706749200\n\
                03/15/24|03/01/24 to 03/15/24|2024-03-15 12:00:00\n1704067200 1735689599 CURRENCY::USD 1";
    assert_eq!(title, want);
}

#[test]
fn a_result_that_does_not_fit_the_schema_names_the_path() {
    let e = common::render("#{ title: \"t\", sections: [#{ blocks: [#{ kind: \"text\", text: 5 }] }] }").unwrap_err();
    let ReportError::Result(r) = &e else { panic!("{e:?}") };
    assert_eq!((r.report.as_str(), r.path.as_str()), (GUID, "sections[0].blocks[0].text"));
    let e = common::render("#{ title: \"t\", sections: [], extra: |x| x }").unwrap_err();
    assert!(matches!(&e, ReportError::Result(r) if r.path == "extra"), "{e:?}");
    let e = common::render("#{ title: \"t\", sections: [#{ blocks: [#{ kind: \"optionsSummary\", rows: [] }] }] }").unwrap_err();
    assert_eq!(e.kind().code(), 3003, "only the host adds the options summary");
}

#[test]
fn rationals_and_handles_become_their_wire_forms() {
    let body = r#"
        let bank = book.account("Root:Asset:Bank1:Current");
        let usd = book.commodity("USD");
        #{ title: "t", sections: [#{ blocks: [#{ kind: "table",
            columns: [#{ label: "A", role: "label" }, #{ label: "B", role: "amount" }],
            rows: [#{ account: bank, cells: [#{ text: bank.name, account: bank },
                                              #{ monetary: monetary(usd, rational(1, 3)) }] },
                   #{ cells: [#{ date: ctx.now }, #{ number: rational("57.125"), places: 2, suffix: " %" }] }] }] }] }
    "#;
    let host = Host::new(fixture("balsheet-pnl.gnucash"));
    let script = format!("fn templates() {{ [#{{ guid: \"{GUID}\", name: \"T\" }}] }}\nfn options(p, o, b) {{}}\nfn render(params, book, opts, ctx) {{ {body} }}");
    let result = host.run(&registry(&[("t.rhai", &script)]), GUID, &json!({})).unwrap();
    let json = serde_json::to_value(&result).unwrap();
    let rows = &json["sections"][0]["blocks"][0]["rows"];
    let guid = host.book.account_by_full_name("Root:Asset:Bank1:Current").unwrap().guid.to_string();
    assert_eq!(rows[0]["account"], json!(guid));
    assert_eq!(rows[0]["cells"][1]["monetary"], json!({ "amount": { "num": "1", "denom": "3" }, "commodity": "CURRENCY::USD" }));
    assert_eq!(rows[0]["cells"][1]["display"], json!("$0.33"));
    assert_eq!(rows[1]["cells"][0]["date"], json!("2024-03-15T12:00:00Z"));
    assert_eq!(rows[1]["cells"][1]["display"], json!("57.12 %"));
}
