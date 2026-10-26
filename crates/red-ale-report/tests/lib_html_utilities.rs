//! `lib/html-utilities` (`5.17:gnucash/report/html-utilities.scm:226-381`, HU): chart colours, the exchange-rates
//! table on `fixtures/reports/balsheet-pnl.gnucash`, and the warnings, checked as blocks of a finished result.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod libs;

use common::{GUID, Host, fixture, registry_with, template};
use libs::{check, lines};
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;
use serde_json::{Value, json};

const PRELUDE: &str = r#"
import "lib/html-utilities" as html;
import "lib/exchange" as exchange;
let usd = book.commodity("USD");
"#;

/// The blocks of the one section `body` returns as `blocks`, as the finished result has them.
fn blocks(body: &str) -> Value {
    let script = template(&format!("{PRELUDE}\n{body}\n#{{ title: \"t\", sections: [#{{ blocks: blocks }}] }}"));
    let reg = registry_with(Library::builtin(), &[("test.rhai", &script)], Limits::default());
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    let result = Host::new(fixture("balsheet-pnl.gnucash")).run(&reg, GUID, &json!({})).unwrap_or_else(|e| panic!("{e:?}"));
    serde_json::to_value(&result.sections[0].blocks).unwrap()
}

/// gnc:assign-colors (HU:226-245) starts the 18-colour palette again from its first colour.
#[test]
fn colors_cycle_through_the_palette() {
    let got = lines(&Host::new(fixture("balsheet-pnl.gnucash")), r#"
        import "lib/html-utilities" as html;
        let c = html::assign_colors(20);
        print(`${c.len()} ${c[0]} ${c[17]} ${c[18]} ${c[19]} [${html::assign_colors(0)}]`);
        print(`${html::assign_colors(3)}`);
    "#);
    check(&got, &["20 #FF4136 #d35400 #FF4136 #FF851B [[]]", "[\"#FF4136\", \"#FF851B\", \"#FFDC00\"]"]);
}

/// gnc:html-make-rates-table (HU:257-274) at the latest prices: GBP at $1.70 (1980-02-01) and FUNDS at the 1975
/// price, sorted CURRENCY before FUNDS; USD, the report currency, is left out.
#[test]
fn rates_table_lists_each_commodity_but_the_report_currency() {
    let got = blocks(r#"
        let accts = book.root().descendants_sorted();
        let pf = exchange::case_price_fn(book, "pricedb-latest", usd, 0);
        let funds = book.account("Root:Asset:Broker:Funds");
        let blocks = [html::make_rates_table(usd, pf, accts), html::make_rates_table(usd, pf, [funds]),
                      html::make_rates_table(usd, pf, [book.account("Root:Asset:House")])];
    "#);
    let cells = |b: &Value| -> Vec<Value> { b["rows"].as_array().unwrap().iter().map(|r| r["cells"].clone()).collect() };
    assert_eq!(got[0]["kind"], "table");
    assert_eq!(got[0]["columns"][0]["label"], "Exchange rates");
    assert_eq!(got[1]["columns"][0]["label"], "Exchange rate");
    assert_eq!(got[2]["columns"][0]["label"], "");
    assert_eq!(cells(&got[2]), Vec::<Value>::new());
    let rows = cells(&got[0]);
    let shown: Vec<[&str; 2]> = rows.iter().map(|r| [0, 1].map(|i| r[i]["display"].as_str().unwrap())).collect();
    // FUNDS has no decimal places; 50005/104 does not end in decimals, so it prints as GnuCash prints such a price.
    assert_eq!(shown, [["#1.00", "$1.70"], ["1. FUNDS", "$480 + 85/104"]], "{rows:#?}");
    let rational = |num: &str, denom: &str| json!({ "num": num, "denom": denom });
    assert_eq!(rows[0][0]["monetary"], json!({ "commodity": "CURRENCY::GBP", "amount": rational("1", "1") }));
    assert_eq!(rows[0][1]["price"], rational("17", "10"));
    assert_eq!(rows[0][1]["currency"], "CURRENCY::USD");
    assert_eq!(rows[1][1]["price"], rational("50005", "104"));
}

/// The warnings (HU:276-381): GnuCash's first heading, the report's title, is the title only when the warning has
/// none of its own.
#[test]
fn warnings() {
    let got = blocks(r#"
        let blocks = [html::make_no_account_warning("Balance Sheet"), html::make_empty_data_warning("Balance Sheet"),
                      html::make_generic_warning("Balance Sheet", "Odd", "Something odd."),
                      html::make_generic_options_warning("Budget Report"), html::make_generic_budget_warning("Budget Report"),
                      html::make_generic_simple_warning("Budget Report", "A message.")];
    "#);
    let want = [
        ("No accounts selected", "This report requires accounts to be selected in the report options."),
        ("No data", "The selected accounts contain no data/transactions (or only zeroes) for the selected time period"),
        ("Odd", "Something odd."),
        ("Budget Report:", "This report requires you to specify certain report options."),
        ("Budget Report:", "No budgets exist. You must create at least one budget."),
        ("Budget Report:", "A message."),
    ];
    let got: Vec<(String, String)> = got
        .as_array()
        .unwrap()
        .iter()
        .inspect(|b| assert_eq!(b["kind"], "warning"))
        .map(|b| (b["title"].as_str().unwrap().to_owned(), b["text"].as_str().unwrap().to_owned()))
        .collect();
    assert_eq!(got, want.map(|(t, x)| (t.to_owned(), x.to_owned())));
}
