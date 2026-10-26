//! `lib/acct-table` (`5.17:gnucash/report/html-acct-table.scm:524-1071`, HAT) on
//! `fixtures/reports/balsheet-pnl.gnucash` at 1971-01-01, the Balance Sheet's date in
//! `standard/test/test-balsheet-pnl.scm:178-272`, with the asset accounts selected.
//!
//! The asset tree: Asset (no splits) holds Bank1 (Bonds $2,000, Current $2,609, Empty, Savings $100), Broker ($2,000,
//! with Funds, 30 FUNDS), ForeignBank (with ForeignSavings, 200 GBP) and House ($100,000); all under the top-level
//! account Root, which is not selected. At the nearest prices 30 FUNDS are $9,000.30 and 200 GBP $297.03, $116,006.33
//! in all (`test-balsheet-pnl.scm:180`).
//!
//! Rows print as `role depth level label amount`, `-` for no amount and `|` before an amount with a rule.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod libs;

use common::{GUID, Host, fixture, registry_with, template};
use libs::{check, lines, lines_with};
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;
use red_ale_report::settings::ReverseBalanced;
use serde_json::{Value, json};

const PRELUDE: &str = r#"
import "lib/acct-table" as hat;
import "lib/exchange" as exchange;
import "lib/collectors" as collectors;
let usd = book.commodity("USD");
let date = ctx.make_time(#{ year: 1971, month: 1, day: 1, hour: 23, minute: 59, second: 59 });
let asset = book.account("Root:Asset");
let assets = [asset] + asset.descendants_sorted();
let base = #{ end_date: date, report_commodity: usd, exchange_fn: exchange::case_exchange_fn(book, "pricedb-nearest", usd, date) };
let env_with = |m| { let e = base; for k in m.keys() { e[k] = m[k]; } e };
let amount_text = |c| if c == () { "-" } else {
    let rule = if "rule" in c && c.rule { "|" } else { "" };
    let shown = if "monetary" in c { c.monetary.to_string() }
                else if "commodities" in c {
                    c.commodities.map(|l| `${l.monetary}=${l.converted}`).reduce(|s, x| if s == "" { x } else { s + " " + x }, "")
                }
                else { `"${c.text}"` };
    rule + shown
};
let show = |rows| for r in rows { print(`${r.role} ${r.depth} ${r.level} ${r.cells[0].text} ${amount_text.call(r.cells[1])}`); };
let table = |env, accts, params| hat::add_account_balances(ctx, (), hat::make_html_acct_table_env_accts(book, ctx, env, accts), params);
"#;

fn run(host: &Host, body: &str) -> Vec<String> {
    lines(host, &format!("{PRELUDE}\n{body}"))
}

fn host() -> Host {
    Host::new(fixture("balsheet-pnl.gnucash"))
}

/// Depth 3, summarized: every asset account gets a row at its depth below Asset, the top selected account; Root, not
/// selected, gets none but is walked. Parents show their own balance ("immediate-bal"): $0 for each. The level, the
/// amount's place from the right, is the display depth (HAT:1045-1066).
#[test]
fn every_selected_account_at_its_depth() {
    let got = run(&host(), r#"
        show.call(table.call(env_with.call(#{ display_tree_depth: 3 }), assets, #{ parent_account_balance_mode: "immediate-bal" }));
    "#);
    check(&got, &[
        "account 0 0 Asset $0.00",
        "account 1 1 Bank1 $0.00",
        "account 2 2 Bonds $2,000.00",
        "account 2 2 Current $2,609.00",
        "account 2 2 Empty $0.00",
        "account 2 2 Savings $100.00",
        "account 1 1 Broker $2,000.00",
        "account 2 2 Funds $9,000.30",
        "account 1 1 ForeignBank $0.00",
        "account 2 2 ForeignSavings $297.03",
        "account 1 1 House $100,000.00",
    ]);
}

/// With subtotals, a parent whose children show gets a "Total" row after them with its recursive balance, each
/// commodity rounded to the cent before adding (CU:900-910); parents show no balance of their own by default
/// ("omit-bal"), and rule_mode puts a rule above subtotals.
#[test]
fn subtotals_follow_their_children() {
    let got = run(&host(), r"
        show.call(table.call(env_with.call(#{ display_tree_depth: 3, parent_account_subtotal_mode: true }), assets, #{ rule_mode: true }));
    ");
    check(&got, &[
        "account 0 0 Asset -",
        "account 1 1 Bank1 -",
        "account 2 2 Bonds $2,000.00",
        "account 2 2 Current $2,609.00",
        "account 2 2 Empty $0.00",
        "account 2 2 Savings $100.00",
        "subtotal 1 1 Total Bank1 |$4,709.00",
        "account 1 1 Broker -",
        "account 2 2 Funds $9,000.30",
        "subtotal 1 1 Total Broker |$11,000.30",
        "account 1 1 ForeignBank -",
        "account 2 2 ForeignSavings $297.03",
        "subtotal 1 1 Total ForeignBank |$297.03",
        "account 1 1 House $100,000.00",
        "subtotal 0 0 Total Asset |$116,006.33",
    ]);
}

/// At depth 2 the three behaviours differ (HAT:1018-1024): "summarize" gives the last level its subtree's balance,
/// "flatten" lifts the deeper accounts to the last level, and "truncate" drops them and shows the last level as any
/// parent.
#[test]
fn depth_limits_summarize_flatten_or_truncate() {
    let got = run(&host(), r#"
        let params = #{ parent_account_balance_mode: "immediate-bal" };
        for behavior in ["summarize", "flatten", "truncate"] {
            print(behavior);
            show.call(table.call(env_with.call(#{ display_tree_depth: 2, depth_limit_behavior: behavior }), assets, params));
        }
        print("flatten, no limit");
        let funds = book.account("Root:Asset:Broker:Funds");
        show.call(table.call(env_with.call(#{ depth_limit_behavior: "flatten" }), [asset, funds], params));
    "#);
    check(&got, &[
        "summarize",
        "account 0 0 Asset $0.00",
        "account 1 1 Bank1 $4,709.00",
        "account 1 1 Broker $11,000.30",
        "account 1 1 ForeignBank $297.03",
        "account 1 1 House $100,000.00",
        "flatten",
        "account 0 0 Asset $0.00",
        "account 1 1 Bank1 $0.00",
        "account 1 1 Bonds $2,000.00",
        "account 1 1 Current $2,609.00",
        "account 1 1 Empty $0.00",
        "account 1 1 Savings $100.00",
        "account 1 1 Broker $2,000.00",
        "account 1 1 Funds $9,000.30",
        "account 1 1 ForeignBank $0.00",
        "account 1 1 ForeignSavings $297.03",
        "account 1 1 House $100,000.00",
        "truncate",
        "account 0 0 Asset $0.00",
        "account 1 1 Bank1 $0.00",
        "account 1 1 Broker $2,000.00",
        "account 1 1 ForeignBank $0.00",
        "account 1 1 House $100,000.00",
        // Funds is the second selected account down its branch, so it is one level below Asset.
        "flatten, no limit",
        "account 0 0 Asset $0.00",
        "account 1 1 Funds $9,000.30",
    ]);
}

/// Zero balances (HAT:740-748, :1036-1043): "omit-leaf-acct" drops accounts whose subtree is all zero, here Empty;
/// "omit-balance" leaves the amount out. A multi-commodity balance becomes a commodity table in "table" mode, on
/// account rows only.
#[test]
fn zeros_and_commodity_tables() {
    let got = run(&host(), r#"
        let env = env_with.call(#{ display_tree_depth: 3, zero_balance_mode: "omit-leaf-acct", parent_account_subtotal_mode: true });
        let bank1 = book.account("Root:Asset:Bank1");
        show.call(table.call(env, [bank1] + bank1.children_sorted(), #{ zero_balance_display_mode: "omit-balance" }));
        print("table");
        let broker = book.account("Root:Asset:Broker");
        let env = env_with.call(#{ display_tree_depth: 1, parent_account_subtotal_mode: true });
        let params = #{ multicommodity_mode: "table", parent_account_balance_mode: "recursive-bal" };
        show.call(table.call(env, assets, params));
        env.exchange_fn = exchange::case_exchange_fn(book, "pricedb-latest", usd, date);
        show.call(table.call(env, assets, params));
        // Only the selected accounts' balances are counted (HAT:612-651): here Broker's own $2,000.
        show.call(table.call(env, [asset, broker], params));
    "#);
    check(&got, &[
        "account 0 0 Bank1 -",
        "account 1 1 Bonds $2,000.00",
        "account 1 1 Current $2,609.00",
        "account 1 1 Savings $100.00",
        "subtotal 0 0 Total Bank1 $4,709.00",
        "table",
        // Depth 1 summarizes Asset; the commodity table lists the collector's commodities in its order, the report
        // currency too, each with its value.
        "account 0 0 Asset #200.00=$297.03 30. FUNDS=$9,000.30 $106,709.00=$106,709.00",
        // At the latest prices, GnuCash's own figures for Asset (test-balsheet-pnl.scm:281).
        "account 0 0 Asset #200.00=$340.00 30. FUNDS=$14,424.52 $106,709.00=$106,709.00",
        "account 0 0 Asset $2,000.00",
    ]);
}

/// The table's environment and each row's (HAT:540-848): the walk's keys, the column counts of the post-pass, and
/// `children_displayed`, which a key of the environment overrides.
#[test]
fn row_environments() {
    let got = run(&host(), r#"
        let env = env_with.call(#{ display_tree_depth: 3, initial_indent: 2, account_label_mode: "name",
                                   parent_account_subtotal_mode: true });
        let t = hat::make_html_acct_table_env_accts(book, ctx, env, assets);
        print(`rows=${hat::num_rows(t)} past=[${hat::get_row_env(t, 99)}] before=[${hat::get_row_env(t, -1)}]`);
        let r = hat::get_row_env(t, 1);
        print(`${r.account_name} ${r.account_path} ${r.account_type} ${r.account_type_string} ${r.account_depth} ${r.logical_depth}`);
        print(`${r.row_type} display=${r.display_depth} indented=${r.indented_depth} children=${r.children_displayed} ` +
              `colspan=${r.account_colspan} label=${r.label_cols} account=${r.account_cols} logical=${r.logical_cols}`);
        print(`${r.account_label} ${r.account_anchor.text} ${r.account_children.map(|a| a.name)} ${r.account_parent.name}`);
        print(`${r.report_comm_recursive_bal} ${r.report_comm_account_bal} ${r.account_bal} ${r.recursive_bal.len()}`);
        let s = hat::get_row_env(t, 6);
        print(`${s.row_type} ${s.account_label.text} ${"children_displayed" in s}`);
        let leaf = hat::get_row_env(t, 2);
        print(`${leaf.account_name} children=${leaf.children_displayed}`);
        let forced_env = env_with.call(#{ display_tree_depth: 3, children_displayed: "env" });
        let forced = hat::make_html_acct_table_env_accts(book, ctx, forced_env, assets);
        print(`forced=${hat::get_row_env(forced, 0).children_displayed}`);
    "#);
    check(&got, &[
        "rows=15 past=[] before=[]",
        "Bank1 Root:Asset:Bank1 ASSET Asset 2 1",
        // Indented by initial_indent; three label columns (depths 0-2) and, the limit being 3, three account columns
        // plus the indent; logical columns are the deepest logical depth, 2.
        "account-row display=1 indented=3 children=true colspan=2 label=3 account=5 logical=2",
        "#{\"text\": \"Bank1\"} Bank1 [\"Bonds\", \"Current\", \"Empty\", \"Savings\"] Asset",
        "$4,709.00 $0.00 [] 1",
        // Only account rows record whether their children showed (HAT:771-774).
        "subtotal-row Total Bank1 false",
        "Bonds children=false",
        "forced=env",
    ]);
}

/// The sort closure orders each level (HAT:690); reversed balances follow the run's settings; a get_balance_fn
/// replaces the split sums (HAT:612-618).
#[test]
fn sorting_reversed_balances_and_balance_functions() {
    let mut credit = host();
    credit.settings.reverse_balanced_accounts = ReverseBalanced::Credit;
    let got = run(&credit, r#"
        let env = env_with.call(#{ display_tree_depth: 2, account_less_p: |a, b| a.name > b.name });
        let bank1 = book.account("Root:Asset:Bank1");
        show.call(table.call(env, [bank1] + bank1.children_sorted(), #{}));
        print("reversed");
        let liability = book.account("Root:Liability");
        let liabilities = [liability] + liability.descendants_sorted();
        show.call(table.call(env_with.call(#{ display_tree_depth: 3 }), liabilities, #{ parent_account_balance_mode: "recursive-bal" }));
        print("get_balance_fn");
        let one = |a, s, e| { let c = collectors::make_commodity_collector(); c.add(a.commodity, e - e + 1); c };
        show.call(table.call(env_with.call(#{ display_tree_depth: 2, get_balance_fn: one }), [bank1] + bank1.children_sorted(), #{}));
    "#);
    check(&got, &[
        "account 0 0 Bank1 -",
        "account 1 1 Savings $100.00",
        "account 1 1 Empty $0.00",
        "account 1 1 Current $2,609.00",
        "account 1 1 Bonds $2,000.00",
        "reversed",
        "account 0 0 Liability $9,500.00",
        "account 1 1 Bank2 $9,500.00",
        "account 2 2 CreditCard $500.00",
        "account 2 2 Loan $9,000.00",
        "get_balance_fn",
        "account 0 0 Bank1 -",
        "account 1 1 Bonds $1.00",
        "account 1 1 Current $1.00",
        "account 1 1 Empty $1.00",
        "account 1 1 Savings $1.00",
    ]);
}

/// "pre-closing" takes the closing transactions back out (HAT:635-641): on test-data.gnucash Expenses hold $264
/// after the closing entry of 1977 moved $111 to Equity, $375 before it.
#[test]
fn pre_closing_balances_leave_closing_entries_out() {
    let got = run(&Host::new(fixture("test-data.gnucash")), r#"
        let expenses = book.account("Root:Expenses");
        let fx = exchange::case_exchange_fn(book, "pricedb-nearest", usd, ctx.now);
        let env = #{ display_tree_depth: 1, report_commodity: usd, exchange_fn: fx };
        for mode in ["post-closing", "pre-closing"] {
            env.balance_mode = mode;
            show.call(table.call(env, [expenses], #{}));
        }
    "#);
    check(&got, &["account 0 0 Expenses $264.00", "account 0 0 Expenses $375.00"]);
}

/// What GnuCash cannot do is refused: no depth limit without "flatten" (`(< logi-depth #f)`), `account-less-p` #t
/// (an undefined function) and budgets.
#[test]
fn refusals() {
    let cases = [
        ("#{}", "display_tree_depth must be an integer"),
        ("#{ display_tree_depth: 2, account_less_p: true }", "gnc:account-code-less-p"),
        ("#{ display_tree_depth: 2, report_budget: 1 }", "report_budget"),
    ];
    cases.iter().for_each(|(env, want)| {
        let body = format!("{PRELUDE}\nhat::make_html_acct_table_env_accts(book, ctx, env_with.call({env}), assets);");
        let err = lines_with(&host(), "", &body).unwrap_err();
        assert!(format!("{err:?}").contains(want), "{env}: {err:?}");
    });
}

/// gnc:html-table-add-labeled-amount-line! (HAT:870-923) and gnc-commodity-table (HAT:925-948) on their own, and the
/// rows of an account table as a table block of a finished result.
#[test]
fn labeled_lines_and_a_finished_table() {
    let got = run(&host(), r#"
        let rows = hat::add_labeled_amount_line((), 4, "total", true, "Total Assets", 0, monetary(usd, 5), 3);
        rows = hat::add_labeled_amount_line(rows, (), (), false, "Defaults", (), (), ());
        rows = hat::add_labeled_amount_line(rows, (), (), true, #{ text: "Ruled" }, 2, (), ());
        show.call(rows);
        let c = collectors::make_commodity_collector(); c.add(book.commodity("GBP"), 10); c.add(usd, 1);
        let fx = exchange::make_exchange_function([[book.commodity("GBP"), rational(3, 2)]]);
        print(`${hat::gnc_commodity_table(c, usd, fx).commodities.map(|l| `${l.monetary} ${l.converted}`)}`);
    "#);
    check(&got, &[
        "total 0 0 Total Assets |$5.00",
        "data 0 0 Defaults -",
        "data 2 0 Ruled |\"\"",
        "[\"$1.00 $1.00\", \"#10.00 $15.00\"]",
    ]);

    let body = format!(
        "{PRELUDE}\nlet rows = table.call(env_with.call(#{{ display_tree_depth: 2, parent_account_subtotal_mode: true }}), assets, \
         #{{ multicommodity_mode: \"table\" }});\n\
         #{{ title: \"t\", sections: [#{{ blocks: [#{{ kind: \"table\", columns: [#{{ label: \"Account\", role: \"label\" }}, \
         #{{ label: \"\", role: \"amount\", levels: 2 }}], rows: rows }}] }}] }}"
    );
    let reg = registry_with(Library::builtin(), &[("test.rhai", &template(&body))], Limits::default());
    let result = host().run(&reg, GUID, &json!({})).unwrap_or_else(|e| panic!("{e:?}"));
    let table = serde_json::to_value(&result.sections[0].blocks[0]).unwrap();
    let rows = table["rows"].as_array().unwrap();
    // Asset, its four children (Bank1's at depth 2 are summarized, so it has no subtotal) and Total Asset.
    assert_eq!(rows.len(), 6, "{rows:#?}");
    let foreign = rows.iter().find(|r| r["cells"][0]["text"] == "ForeignBank").unwrap();
    assert_eq!(foreign["role"], "account");
    assert_eq!(foreign["level"], 1);
    assert_eq!(foreign["account"], foreign["cells"][0]["account"]);
    let lines: Vec<&Value> = foreign["cells"][1]["commodities"].as_array().unwrap().iter().collect();
    assert_eq!(lines.len(), 1, "{foreign:#?}");
    assert_eq!(lines[0]["display"], "#200.00");
    assert_eq!(lines[0]["convertedDisplay"], "$297.03");
}
