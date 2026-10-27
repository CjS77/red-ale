//! `lib/options` (`5.17:gnucash/report/options-utilities.scm:44-234`, OU): the options each helper declares, as
//! `report.options` lists them, with GnuCash's names, keys, help texts, choices and defaults.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{EMPTY_RESULT, GUID, Host, fixture, registry_with};
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;

/// Each option of section "Lib" that `body` declares in `options()`: key, name, type, default and help, then the
/// choice keys.
fn declared(body: &str) -> Vec<String> {
    let script = format!(
        "fn templates() {{ [#{{ guid: \"{GUID}\", name: \"Test\" }}] }}\n\
         fn options(params, o, book) {{\nimport \"lib/options\" as options;\n{body}\n}}\n\
         fn render(params, book, opts, ctx) {{ {EMPTY_RESULT} }}\n"
    );
    let reg = registry_with(Library::builtin(), &[("test.rhai", &script)], Limits::default());
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    let host = Host::new(fixture("balsheet-pnl.gnucash"));
    let db = reg
        .options(reg.find(GUID).unwrap(), &host.ctx())
        .unwrap_or_else(|e| panic!("{e:?}"));
    db.describe()
        .into_iter()
        .filter(|o| o.section == "Lib")
        .flat_map(|o| {
            let head = format!("{} {} {} default={} help={}", o.key, o.name, o.option_type, o.default, o.help);
            let choices = o
                .choices
                .map(|cs| cs.iter().map(|c| format!("{}={}", c.key, c.label)).collect::<Vec<_>>().join(", "));
            std::iter::once(head).chain(choices)
        })
        .collect()
}

fn check(got: &[String], want: &[&str]) {
    got.iter().zip(want).for_each(|(g, w)| assert_eq!(g, w, "{got:#?}"));
    assert_eq!(got.len(), want.len(), "{got:#?}");
}

/// OU:48-74; the date defaults are those of gnc-register-start-date-option and -end-date-option. A default may be a
/// closure, which is called (OU:44-45).
#[test]
fn dates_and_intervals() {
    let got = declared(
        r#"
        options::add_report_date(o, "Lib", "Date", "a");
        options::add_date_interval(o, "Lib", "Start Date", "End Date", "b");
        options::add_interval_choice(o, "Lib", "Step Size", "c", "MonthDelta");
        options::add_interval_choice(o, "Lib", "Step Size 2", "d", || "WeekDelta");
    "#,
    );
    let deltas = "DayDelta=One Day, WeekDelta=One Week, TwoWeekDelta=Two Weeks, MonthDelta=One Month, QuarterDelta=Quarter Year, \
                  HalfYearDelta=Half Year, YearDelta=One Year";
    check(
        &got,
        &[
            "a Date date default={\"relative\":\"end-accounting-period\"} help=Select a date to report on.",
            "ba Start Date date default={\"relative\":\"start-accounting-period\"} help=Start of reporting period.",
            "bb End Date date default={\"relative\":\"end-accounting-period\"} help=End of reporting period.",
            "c Step Size multichoice default=\"MonthDelta\" help=The amount of time between data points.",
            deltas,
            "d Step Size 2 multichoice default=\"WeekDelta\" help=The amount of time between data points.",
            deltas,
        ],
    );
}

/// OU:79-130: the levels take "all" or 1 to 6; the currency defaults to the report settings' (USD here).
#[test]
fn accounts_and_currency() {
    let got = declared(
        r#"
        options::add_account_levels(o, "Lib", "Levels", "a", "Some help.", "all");
        options::add_account_levels(o, "Lib", "Levels 2", "b", "", || 2);
        let current = book.account("Root:Asset:Bank1:Current");
        options::add_account_selection(o, "Lib", "Account Display Depth", "Always show sub-accounts", "Accounts", "c", 3,
                                       || [current], true);
        options::add_currency(o, "Lib", "Report's currency", "d");
    "#,
    );
    let levels = "all=All, 1=1, 2=2, 3=3, 4=4, 5=5, 6=6";
    check(
        &got,
        &[
            "a Levels multichoice default=\"all\" help=Some help.",
            levels,
            "b Levels 2 multichoice default=2 help=",
            levels,
            "ca Account Display Depth multichoice default=3 help=Show accounts to this depth, overriding any other option.",
            levels,
            "cb Always show sub-accounts boolean default=true help=Override account-selection and show sub-accounts of all selected accounts?",
            "cc Accounts accountList default=[\"a0000000000000000000000000000007\"] help=Report on these accounts, if display depth allows.",
            "d Report's currency currency default=\"CURRENCY::USD\" help=Select the currency to display the values of this report in.",
        ],
    );
}

/// OU:133-234: price sources, plot sizes (a `(percent . n)` default gives n), markers, sort methods and the
/// subtotal views, whose help joins three sentences with "\n* ".
#[test]
fn choices_plot_sizes_and_subtotal_views() {
    let got = declared(
        r#"
        options::add_price_source(o, "Lib", "Price Source", "a", "pricedb-nearest");
        options::add_plot_size(o, "Lib", "Plot Width", "Plot Height", "b", #{ percent: 100 }, || 400);
        // The unit is dropped: 50 is a percentage.
        options::add_plot_size(o, "Lib", "Width 2", "Height 2", "f", #{ pixels: 50 }, #{ unit: "pixels", value: 200 });
        options::add_marker_choice(o, "Lib", "Marker", "c", "diamond");
        options::add_sort_method(o, "Lib", "Sort Method", "d", "amount");
        options::add_subtotal_view(o, "Lib", "Parent account balances", "Parent account subtotals", "e");
    "#,
    );
    check(
        &got,
        &[
            "a Price Source multichoice default=\"pricedb-nearest\" help=The source of price information.",
            "average-cost=Average cost of purchases weighted by volume, weighted-average=Weighted average of all transactions in the \
         past, pricedb-before=Last up through report date, pricedb-nearest=Closest to report date, pricedb-latest=Most recent",
            "ba Plot Width plotSize default={\"percent\":100} help=Width of plot, 10 - 100 in percent, above in pixels.",
            "bb Plot Height plotSize default={\"pixels\":400} help=Height of plot, 10 - 100 in percent, above in pixels.",
            "c Marker multichoice default=\"diamond\" help=Choose the marker for each data point.",
            "diamond=Diamond, circle=Circle, square=Square, cross=Cross, plus=Plus, dash=Dash, filleddiamond=Filled diamond, \
         filledcircle=Filled circle, filledsquare=Filled square",
            "d Sort Method multichoice default=\"amount\" help=Choose the method for sorting accounts.",
            "acct-code=Alphabetical by account code, alphabetical=Alphabetical by account name, amount=Numerical by descending amount",
            "ea Parent account balances multichoice default=\"immediate-bal\" help=How to show the balances of parent accounts.\n* \
         Account Balance in the parent account, excluding any subaccounts.\n* Do not show any balances of parent accounts.",
            "immediate-bal=Account Balance, recursive-bal=Calculate Subtotal, omit-bal=Do not show",
            "eb Parent account subtotals multichoice default=\"f\" help=How to show account subtotals for parent accounts.\n* \
         Show subtotals for selected parent accounts which have subaccounts.\n* Do not show any subtotals for parent accounts.",
            "t=Show subtotals, f=Do not show",
            "fa Width 2 plotSize default={\"percent\":50} help=Width of plot, 10 - 100 in percent, above in pixels.",
            "fb Height 2 plotSize default={\"pixels\":200} help=Height of plot, 10 - 100 in percent, above in pixels.",
        ],
    );
}
