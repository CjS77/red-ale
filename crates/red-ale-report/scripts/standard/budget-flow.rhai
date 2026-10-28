// budget.scm: budget report
//
// (C) 2005 by Chris Shoemaker <c.shoemaker@cox.net>
//
// based on cash-flow.scm by:
// Herbert Thoma <herbie@hthoma.de>
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
// Translated from 5.17:gnucash/report/reports/standard/budget-flow.scm, cited below by line. (Its header names it
// budget.scm, as the file it was copied from.)
//
// One period of a budget: each selected account without children, its own Budget amount and its actual, by kind of
// account, each kind totalled in the report's currency; then the kinds' totals and a grand total. GnuCash's table
// has no header row; its three columns are the label, the budgeted figure and the actual one (:94-100).

import "lib/accounts" as accounts;
import "lib/collectors" as collectors;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:315-321); "B_udget" is `gnc:menuname-budget` (5.17:gnucash/report/report-core.scm:159).
fn templates() {
    [#{ guid: "e6e34fa3b6e748debde3cb3bc76d3e53", name: "Budget Flow", menuPath: ["B_udget"],
        params: #{ reportname: "Budget Flow" } }]
}

/// `budget-report-options-generator` (:48-88).
fn options(params, o, book) {
    o.budget("General", "Budget", "a", "Budget to use.", book.default_budget());
    // GnuCash's FIXMEs: the 60 periods are the budget dialog's, and the range is not the selected budget's (:60-66).
    o.number_range("General", "Period", "b", "Period number.", 1, 1, 60, 1);
    opts_util::add_currency(o, "General", "Report's currency", "d");
    opts_util::add_price_source(o, "General", "Price Source", "c", "pricedb-latest");
    o.account_list("Accounts", "Account", "ac", "Report on these accounts.", book.root().descendants_sorted(), #{});
    o.set_default_section("General");
}

/// `budget-renderer` (:247-313).
fn render(params, book, opts, ctx) {
    let reportname = params.reportname;
    let budget = opts.get("General", "Budget");
    let accts = opts.get("Accounts", "Account");
    // GnuCash leaves the document untitled when it only warns; the warnings here leave out the heading that names
    // the report, so the title does (design/reports.md §14).
    if accts.is_empty() {
        return #{ title: reportname, sections: [#{ blocks: [html_utils::make_no_account_warning(reportname)] }] };
    }
    if budget == () {
        return #{ title: reportname, sections: [#{ blocks: [html_utils::make_generic_budget_warning(reportname)] }] };
    }
    // `(inexact->exact period)` (:261-262): a period that is not whole reaches the C calls as a fraction, which
    // fails there too.
    let period_value = opts.get("General", "Period");
    if !period_value.is_integer() {
        throw `Period must be a whole number, not ${period_value}`;
    }
    let period = parse_int(period_value.to_string());
    let report_currency = opts.get("General", "Report's currency");
    // The exchange date is the end of the 1-based `period`, the period after the one reported (:286-288).
    let exchange_fn = exchange::case_exchange_fn(book, opts.get("General", "Price Source"), report_currency,
                                                 budget.period_end_date(period));
    let env = #{ budget: budget, period: period - 1, exchange_fn: exchange_fn, report_currency: report_currency };
    // :295-299
    let title = `${opts.get("General", "Report name")}: ${budget.name} - ${ctx.print_date(budget.period_start_date(period - 1))}`;
    let types = add_budget_types(env, accounts::decompose_accountlist(accts));
    let rows = types.rows + add_budget_totals(env, types.totals);
    let columns = [#{ label: "", role: "label" }, #{ label: "", role: "amount" }, #{ label: "", role: "amount" }];
    #{ title: title, sections: [#{ blocks: [#{ kind: "table", columns: columns, rows: rows }] }] }
}

/// `gnc:html-table-add-budget-row!` (:91-100): the label, then the budgeted and actual figures.
private fn budget_row(role, label, total1, total2) {
    #{ role: role, cells: [label, #{ monetary: total1 }, #{ monetary: total2 }] }
}

/// The kinds of `gnc:decompose-accountlist`, in its order (5.17:gnucash/report/report-utilities.scm:188-205).
private fn kinds() {
    ["ASSET", "LIABILITY", "EQUITY", "INCOME", "EXPENSE", "TRADING"]
}

/// `gnc:html-table-add-budget-types!` (:179-197): the rows of each kind, kinds without accounts too, and the kinds'
/// totals, `[kind, [budgeted, actual]]`. `assoc-set!` puts each new kind at the front (:190-194), so the totals come
/// last kind first.
private fn add_budget_types(env, split_up_accounts) {
    let rows = [];
    let accounts_totals = [];
    for kind in kinds() {
        let added = add_budget_accounts(env, split_up_accounts[kind]);
        rows += added.rows;
        accounts_totals.insert(0, [kind, added.totals]);
    }
    #{ rows: rows, totals: accounts_totals }
}

/// `gnc:html-table-add-budget-accounts!` (:110-171): a row for each account without children, its own Budget amount
/// (unset as 0, not rolled up) and its actual in the period, in its commodity (:124-152); then their totals in the
/// report's currency and a rule (:155-168). GnuCash's FIXME (:119-123): actuals include sub-accounts, so a parent's
/// would count its children twice.
private fn add_budget_accounts(env, acct_table) {
    let rows = [];
    let bgt_total = collectors::make_commodity_collector();
    let act_total = collectors::make_commodity_collector();
    for acct in acct_table.filter(|a| a.children().is_empty()) {
        let comm = acct.commodity;
        let bgt_numeric = env.budget.account_period_value(acct, env.period);
        let act_numeric = env.budget.account_period_actual_value(acct, env.period);
        bgt_total.add(comm, bgt_numeric);
        act_total.add(comm, act_numeric);
        let row = budget_row("account", #{ text: acct.fullName, account: acct }, monetary(comm, bgt_numeric),
                             monetary(comm, act_numeric));
        row.account = acct;
        rows.push(row);
    }
    let totals = [collectors::sum_collector_commodity(bgt_total, env.report_currency, env.exchange_fn),
                  collectors::sum_collector_commodity(act_total, env.report_currency, env.exchange_fn)];
    rows.push(budget_row("total", #{ text: "Total:" }, totals[0], totals[1]));
    rows.push(#{ role: "rule", cells: [(), (), ()] });
    #{ rows: rows, totals: totals }
}

/// `gnc:html-table-add-budget-totals!` (:203-240): a row for each kind's totals, labelled with its plural name, then
/// the grand total in the report's currency.
private fn add_budget_totals(env, type_totals) {
    let rows = [];
    let bgt_total_collector = collectors::make_commodity_collector();
    let act_total_collector = collectors::make_commodity_collector();
    for pair in type_totals {
        let bgt_total = pair[1][0];
        let act_total = pair[1][1];
        bgt_total_collector.add(bgt_total.commodity, bgt_total.amount);
        act_total_collector.add(act_total.commodity, act_total.amount);
        rows.push(budget_row("data", #{ text: accounts::account_get_type_string_plural(pair[0]) }, bgt_total, act_total));
    }
    rows.push(budget_row("grandTotal", #{ text: "Total:" },
                         collectors::sum_collector_commodity(bgt_total_collector, env.report_currency, env.exchange_fn),
                         collectors::sum_collector_commodity(act_total_collector, env.report_currency, env.exchange_fn)));
    rows
}
