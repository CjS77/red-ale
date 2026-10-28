// cash-flow.scm: cash flow report
//
// By Herbert Thoma <herbie@hthoma.de>
//
// based on balance-sheet.scm by:
// Robert Merkel <rgmerk@mira.net>
// and pnl.scm by:
// Christian Stimming <stimming@tu-harburg.de>
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License as
// published by the Free Software Foundation; either version 2 of
// the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program; if not, contact:
//
// Free Software Foundation           Voice:  +1-617-542-5942
// 51 Franklin Street, Fifth Floor    Fax:    +1-617-542-2652
// Boston, MA  02110-1301,  USA       gnu@gnu.org
//
// This translation is part of red-ale and is distributed under the GNU General Public License, version 3 or (at your
// option) any later version (GPL-3.0-or-later); see LICENSE.
//
// Translated from 5.17:gnucash/report/reports/standard/cash-flow.scm, cited below by line. GnuCash's "Selected
// Accounts" is a `<ul>` of register links; here it is a list block whose items name their accounts. The table has no
// column headers, as GnuCash's has none.

import "lib/accounts" as accounts;
import "lib/cash-flow-calc" as cfc;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:340-346).
fn templates() {
    [#{ guid: "f8748b813fab4220ba26e743aedf38da", name: "Cash Flow", menuPath: ["_Income & Expense"], params: #{} }]
}

/// `cash-flow-options-generator` (:57-106).
fn options(params, o, book) {
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", "a");
    opts_util::add_currency(o, "General", "Report's currency", "b");
    opts_util::add_price_source(o, "General", "Price Source", "c", "pricedb-nearest");
    o.boolean("General", "Show Exchange Rates", "d", "Show the exchange rates used.", false);
    o.boolean("General", "Show Full Account Names", "e", "Show full account names (including parent accounts).", true);
    o.boolean("General", "Show Selected Accounts", "f", "Show the list of selected accounts on the top of this report.",
              true);

    let defaults = accounts::filter_accountlist_type(["BANK", "CASH", "ASSET", "STOCK", "MUTUAL"],
                                                     book.root().descendants_sorted());
    opts_util::add_account_selection(o, "Accounts", "Account Display Depth", "Always show sub-accounts", "Account", "a", 2,
                                     defaults, false);
    o.boolean("Accounts", "Include Trading Accounts in report", "b", trading_help(), false);

    o.set_default_section("General");
}

/// GnuCash's help, "Include transfers to and from Trading Accounts in the report." (:101), and what the option
/// does: since commit 2215dfc5e1 it works backwards (`5.17:gnucash/report/reports/cash-flow-calc.scm:68-70`).
/// red-ale keeps 5.17's figures and says so (`design/reports.md` §14).
fn trading_help() {
    "Include transfers to and from Trading Accounts in the report. As in GnuCash 5.17 this works backwards: off "
        + "counts transfers to and from Trading Accounts, on leaves them out."
}

/// The option values `cash-flow-renderer` reads (:121-161), the accounts with their sub-accounts when asked and
/// sorted by full name.
private fn read_options(book, opts, ctx) {
    let show_subaccts = opts.get("Accounts", "Always show sub-accounts");
    let accts = opts.get("Accounts", "Account");
    if show_subaccts {
        accts = accounts::accounts_and_all_descendants(accts);
    }
    accts.sort(|a, b| if accounts::account_full_name_lt(a, b) { -1 } else if accounts::account_full_name_lt(b, a) { 1 } else { 0 });
    let report_currency = opts.get("General", "Report's currency");
    let price_source = opts.get("General", "Price Source");
    let to_date = dates::time64_end_day_time(ctx, opts.get("General", "End Date"));
    #{
        display_depth: opts.get("Accounts", "Account Display Depth"),
        show_subaccts: show_subaccts,
        accounts: accts,
        include_trading_accounts: opts.get("Accounts", "Include Trading Accounts in report"),
        report_currency: report_currency,
        price_source: price_source,
        show_rates: opts.get("General", "Show Exchange Rates"),
        show_full_names: opts.get("General", "Show Full Account Names"),
        show_sel_acct_list: opts.get("General", "Show Selected Accounts"),
        from_date: dates::time64_start_day_time(ctx, opts.get("General", "Start Date")),
        to_date: to_date,
        exchange_fn: exchange::case_exchange_fn(book, price_source, report_currency, to_date),
        price_fn: exchange::case_price_fn(book, price_source, report_currency, to_date),
    }
}

/// `cash-flow-renderer` (:113-337).
fn render(params, book, opts, ctx) {
    let o = read_options(book, opts, ctx);
    let title = `${opts.get("General", "Report name")} - ${ctx.print_date_interval(o.from_date, o.to_date)}`;
    let blocks = if o.accounts.is_empty() {
        [html_utils::make_no_account_warning("Cash Flow")]
    } else {
        flow_blocks(book, ctx, o)
    };
    #{ title: title, sections: [#{ blocks: blocks }] }
}

/// An account's name as the report shows it (:176-178, :209-211).
private fn account_name(o, acct) {
    if o.show_full_names { acct.fullName } else { acct.name }
}

/// The "Selected Accounts" items (:194-215): the accounts down to the display depth, an account at that depth with
/// children said to stand for its sub-accounts too.
private fn selected_accounts_items(o) {
    let tree_depth = if o.display_depth == "all" { accounts::accounts_get_children_depth(o.accounts) } else { o.display_depth };
    let suffix = if o.show_subaccts { " and subaccounts" } else { " and selected subaccounts" };
    let items = [];
    for acct in o.accounts {
        if acct.depth <= tree_depth {
            let at_depth = acct.depth == tree_depth && !acct.children().is_empty();
            items.push(#{ text: account_name(o, acct) + if at_depth { suffix } else { "" }, account: acct });
        }
    }
    items
}

/// The selected accounts, the money table and the rates (:194-327).
private fn flow_blocks(book, ctx, o) {
    // :217-241
    let commodity_list = accounts::accounts_get_commodities(o.accounts, o.report_currency);
    let result = cfc::cash_flow_calc_money_in_out(#{
        book: book,
        accounts: o.accounts,
        from_date: o.from_date,
        to_date: o.to_date,
        report_currency: o.report_currency,
        include_trading_accounts: o.include_trading_accounts,
        time_exchange_fn: exchange::case_exchange_time_fn(book, ctx, o.price_source, o.report_currency, commodity_list,
                                                          o.to_date),
    });

    let blocks = [];
    let rows = [];
    // :253-264
    if o.show_sel_acct_list {
        blocks.push(#{ kind: "text", text: "Selected Accounts" });
        blocks.push(html_utils::html_markup_ul(selected_accounts_items(o)));
        rows.push(rule_row());
    }
    let money_in = collectors::sum_collector_commodity(result.money_in_collector, o.report_currency, o.exchange_fn);
    let money_out = collectors::sum_collector_commodity(result.money_out_collector, o.report_currency, o.exchange_fn);
    let difference = collectors::sum_collector_commodity(
        collectors::collector_minus([result.money_in_collector, result.money_out_collector]), o.report_currency, o.exchange_fn);
    // :266-317
    rows.push(#{ role: "heading", cells: [#{ text: "Money into selected accounts comes from" }, ()] });
    rows += flow_rows(o, result.money_in_accounts, result.money_in_alist);
    rows.push(total_row("Money In", money_in));
    rows.push(rule_row());
    rows.push(#{ role: "heading", cells: [#{ text: "Money out of selected accounts goes to" }, ()] });
    rows += flow_rows(o, result.money_out_accounts, result.money_out_alist);
    rows.push(total_row("Money Out", money_out));
    rows.push(rule_row());
    rows.push(total_row("Difference", difference));
    blocks.push(#{ kind: "table", columns: [#{ label: "", role: "label" }, #{ label: "", role: "amount" }], rows: rows });

    // :323-327
    if o.show_rates {
        blocks.push(html_utils::make_rates_table(o.report_currency, o.price_fn, o.accounts));
    }
    blocks
}

/// `add-accounts-flow` (:163-184): a row per account, sorted by full name (:242-249, a stable sort), with its
/// collector summed in the report currency.
private fn flow_rows(o, accts, alist) {
    let sorted = accts;
    sorted.sort(|a, b| if accounts::account_full_name_lt(a, b) { -1 } else if accounts::account_full_name_lt(b, a) { 1 } else { 0 });
    let rows = [];
    for acct in sorted {
        let i = alist.index_of(|p| p[0] == acct);
        let amount = collectors::sum_collector_commodity(alist[i][1], o.report_currency, o.exchange_fn);
        rows.push(#{ role: "account", cells: [#{ text: account_name(o, acct), account: acct }, #{ monetary: amount }] });
    }
    rows
}

/// A "grand-total" row of a label and an amount.
private fn total_row(label, amount) {
    #{ role: "grandTotal", cells: [#{ text: label }, #{ monetary: amount }] }
}

/// `gnc:html-table-append-ruler!` across both columns.
private fn rule_row() {
    #{ role: "rule", cells: [(), ()] }
}
