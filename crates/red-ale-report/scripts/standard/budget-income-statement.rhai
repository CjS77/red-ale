// budget-income-statement.scm: income statement (a.k.a. Profit & Loss)
//
// Copyright (c) the following:
//
//  Forest Bond <forest@alittletooquiet.net>
//  David Montenegro <sunrise2000@comcast.net>
//
//  * BUGS:
//
//    Line & column alignments may still not conform with
//    textbook accounting practice (they're close though!).
//
//    Progress bar functionality is currently mostly broken.
//
//    The variables in this code could use more consistent naming.
//
//    See also all the "FIXME"s in the code.
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
// Translated from 5.17:gnucash/report/reports/standard/budget-income-statement.scm, cited below by line. Each
// account shows its own Budget amounts over the budget or the chosen periods, unrolled (:268-276). GnuCash builds a
// revenue table and an expense table, each with a row of spacer cells (:476-496), and lays them out one above the
// other or side by side in one layout table (:541-564); here they are two result tables, in a `group` when side by
// side, and the spacer rows are left out as layout.
//
// GnuCash computes each account's balance before it builds the account tables (:414-422), and their
// `get-balance-fn` only looks it up (:457-469); here the lookup is a map by GUID. "Report for range of budget periods"
// only makes the range options selectable in GnuCash's dialog (:139-158), so no callback is passed.

import "lib/accounts" as accounts;
import "lib/acct-table" as acct_table;
import "lib/balances" as balances;
import "lib/budget" as budget_util;
import "lib/collectors" as collectors;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The templates (:591-609); "B_udget" is `gnc:menuname-budget` (5.17:gnucash/report/report-core.scm:159).
fn templates() {
    [#{ guid: "583c313fcc484efc974c4c844404f454", name: "Budget Income Statement", menuPath: ["B_udget"],
        params: #{ reportname: "Budget Income Statement" } },
     #{ guid: "e5fa5ce805e840ecbeca4dba3fa4ead9", name: "Budget Profit & Loss", menuPath: ["B_udget"],
        params: #{ reportname: "Budget Profit & Loss" } }]
}

/// `budget-income-statement-options-generator-internal` (:127-256).
fn options(params, o, book) {
    o.string("General", "Report Title", "a", "Title for this report.", params.reportname);
    o.budget("General", "Budget", "c", "Budget to use.", book.default_budget());
    o.complex_boolean("General", "Report for range of budget periods", "d",
                      "Create report for a budget period range instead of the entire budget.", false, ());
    // GnuCash's FIXME: the 60 periods of the budget dialog are not defined anywhere the options could use (:163-165).
    o.number_range("General", "Range start", "e", "Select a budget period that begins the reporting range.", 1, 1, 60, 1);
    o.number_range("General", "Range end", "f", "Select a budget period that ends the reporting range.", 1, 1, 60, 1);

    // "select, by default, only income and expense accounts" (:177-184)
    o.account_list("Accounts", "Accounts", "a", "Report on these accounts, if display depth allows.",
                   accounts::filter_accountlist_type(["INCOME", "EXPENSE"], book.root().descendants_sorted()), #{});
    opts_util::add_account_levels(o, "Accounts", "Levels of Subaccounts", "b",
                                  "Maximum number of levels in the account tree displayed.", 3);
    o.boolean("Accounts", "Flatten list to depth limit", "c",
              "Displays accounts which exceed the depth limit at the depth limit.", false);

    opts_util::add_currency(o, "Commodities", "Report's currency", "a");
    opts_util::add_price_source(o, "Commodities", "Price Source", "b", "pricedb-nearest");
    o.boolean("Commodities", "Show Foreign Currencies", "c", "Display any foreign currency amount in an account.", true);
    o.boolean("Commodities", "Show Exchange Rates", "d", "Show the exchange rates used.", false);

    o.boolean("Display", "Include accounts with zero total balances", "a",
              "Include accounts with zero total (recursive) balances in this report.", true);
    o.boolean("Display", "Omit zero balance figures", "b",
              "Show blank space in place of any zero balances which would be shown.", false);
    opts_util::add_subtotal_view(o, "Display", "Parent account balances", "Parent account subtotals", "c");
    o.boolean("Display", "Display accounts as hyperlinks", "d",
              "Shows each account in the table as a hyperlink to its register window.", true);
    o.boolean("Display", "Show accounting-style rules", "e",
              "Use rules beneath columns of added numbers like accountants do.", false);
    o.boolean("Display", "Label the revenue section", "f", "Whether or not to include a label for the revenue section.", true);
    o.boolean("Display", "Include revenue total", "g", "Whether or not to include a line indicating total revenue.", true);
    o.boolean("Display", "Label the expense section", "h", "Whether or not to include a label for the expense section.", true);
    o.boolean("Display", "Include expense total", "i", "Whether or not to include a line indicating total expense.", true);
    o.boolean("Display", "Display as a two column report", "j",
              "Divides the report into an income column and an expense column.", false);
    o.boolean("Display", "Display in standard, income first, order", "k",
              "Causes the report to display in the standard order, placing income before expenses.", true);

    o.set_default_section("Accounts");
}

/// The name the renderer passes for both templates (:582-588): the Budget Profit & Loss warns of missing accounts
/// as the Budget Income Statement.
private fn is_reportname() {
    "Budget Income Statement"
}

/// `(inexact->exact (truncate ...))` (:288-299): a period number truncated to an integer.
private fn to_period_val(v) {
    parse_int(v.round_to(1, "truncate").to_string())
}

/// A result of one warning block, titled as the warning's own heading would name the report.
private fn warning_result(title, block) {
    #{ title: title, sections: [#{ blocks: [block] }] }
}

/// `budget-income-statement-renderer-internal` (:263-575). GnuCash titles the document only when it draws the
/// statement (:531-532); the warnings here leave out the heading that names the report, so the title does.
fn render(params, book, opts, ctx) {
    let report_title = opts.get("General", "Report Title");
    let budget = opts.get("General", "Budget");
    let use_range = opts.get("General", "Report for range of budget periods");
    let user_start = if use_range { to_period_val(opts.get("General", "Range start")) } else { () };
    let user_end = if use_range { to_period_val(opts.get("General", "Range end")) } else { () };
    if opts.get("Accounts", "Accounts").is_empty() {
        return warning_result(is_reportname(), html_utils::make_no_account_warning(is_reportname()));
    }
    if budget == () {
        return warning_result(report_title, html_utils::make_generic_budget_warning(report_title));
    }
    if use_range && user_end < user_start {
        return warning_result(report_title, html_utils::make_generic_simple_warning(report_title,
            "Reporting range end period cannot be less than start period."));
    }
    if use_range && budget.numPeriods < user_end {
        return warning_result(report_title, html_utils::make_generic_simple_warning(report_title,
            "Reporting range end period cannot exceed number of periods in budget"));
    }
    let o = read_options(book, opts, budget, user_start, user_end);
    let title = `${book.company_name()} ${report_title} ${o.period_for}`;
    #{ title: title, sections: [#{ blocks: statement_blocks(book, ctx, o) }] }
}

/// "for Budget ~a", with the period or periods when ranged (:500-510).
private fn period_for(budget, user_start, user_end) {
    if user_start == () {
        `for Budget ${budget.name}`
    } else if user_start == user_end {
        `for Budget ${budget.name} Period ${user_start}`
    } else {
        `for Budget ${budget.name} Periods ${user_start} - ${user_end}`
    }
}

/// The option values the renderer reads (:281-369), by its names: periods `user_start - 1` (inclusive) to
/// `user_end` (exclusive), counted from 0, when ranged (:300-303); prices at the first reported period's start
/// (:304-309).
private fn read_options(book, opts, budget, user_start, user_end) {
    let depth_limit = opts.get("Accounts", "Levels of Subaccounts");
    let report_commodity = opts.get("Commodities", "Report's currency");
    let price_source = opts.get("Commodities", "Price Source");
    let period_start = if user_start == () { () } else { user_start - 1 };
    let date_t64 = budget.period_start_date(if user_start == () { 0 } else { period_start });
    #{
        budget: budget,
        period_start: period_start,
        period_end: user_end,
        period_for: period_for(budget, user_start, user_end),
        accounts: opts.get("Accounts", "Accounts"),
        // "this can occasionally put extra (blank) columns in our table ..." (:357-364)
        tree_depth: if depth_limit == "all" { accounts::get_current_account_tree_depth(book) } else { depth_limit },
        bottom_behavior: opts.get("Accounts", "Flatten list to depth limit"),
        report_commodity: report_commodity,
        show_fcur: opts.get("Commodities", "Show Foreign Currencies"),
        show_rates: opts.get("Commodities", "Show Exchange Rates"),
        parent_balance_mode: opts.get("Display", "Parent account balances"),
        // `(assq-ref '((t . #t) (f . #f)) ...)` (:326-329)
        parent_total_mode: opts.get("Display", "Parent account subtotals") == "t",
        show_zb_accts: opts.get("Display", "Include accounts with zero total balances"),
        omit_zb_bals: opts.get("Display", "Omit zero balance figures"),
        label_revenue: opts.get("Display", "Label the revenue section"),
        total_revenue: opts.get("Display", "Include revenue total"),
        label_expense: opts.get("Display", "Label the expense section"),
        total_expense: opts.get("Display", "Include expense total"),
        use_links: opts.get("Display", "Display accounts as hyperlinks"),
        use_rules: opts.get("Display", "Show accounting-style rules"),
        two_column: opts.get("Display", "Display as a two column report"),
        standard_order: opts.get("Display", "Display in standard, income first, order"),
        exchange_fn: exchange::case_exchange_fn(book, price_source, report_commodity, date_t64),
        price_fn: exchange::case_price_fn(book, price_source, report_commodity, date_t64),
    }
}

/// `get-assoc-account-balances-budget` (:268-272) with `get-budget-account-budget-balance` (:274-276), or with
/// `gnc:budget-account-get-net` itself when `signed` is false: each account's own Budget amounts over the periods,
/// negated for an account shown reversed when `signed`.
private fn budget_balances(ctx, o, accts, signed) {
    let out = [];
    for acct in accts {
        let bal = budget_util::budget_account_get_net(o.budget, acct, o.period_start, o.period_end);
        out.push([acct, if signed && acct.reverse_balance(ctx) { collectors::collector_minus([bal]) } else { bal }]);
    }
    out
}

/// The balances the account table shows (:457-474), by GUID: the revenue negated, the expenses as they are.
private fn by_guid(assoc, negate) {
    let out = #{};
    for acct_bal in assoc {
        out[acct_bal[0].guid] = if negate { collectors::collector_minus([acct_bal[1]]) } else { acct_bal[1] };
    }
    out
}

/// The totals of the statement (:414-435), as commodity collectors: the net income is the unsigned amounts summed
/// and negated, whatever the accounts' display signs.
private fn statement_totals(ctx, o, split_up) {
    let revenue = budget_balances(ctx, o, split_up.INCOME, true);
    let expense = budget_balances(ctx, o, split_up.EXPENSE, true);
    let unsigned = budget_balances(ctx, o, split_up.INCOME + split_up.EXPENSE, false);
    #{
        revenue_accounts: by_guid(revenue, true),
        expense_accounts: by_guid(expense, false),
        revenue: balances::get_assoc_account_balances_total(revenue),
        expense: balances::get_assoc_account_balances_total(expense),
        net_income: collectors::collector_minus([balances::get_assoc_account_balances_total(unsigned)]),
    }
}

/// `add-subtotal-line` (:371-382): a total row at the rightmost level; no amount when `signed_balance` is `()`.
/// GnuCash marks every such row "primary-subheading"; `role` says which kind of line it is. The statement passes no
/// negative label, so the line is never relabelled.
private fn add_subtotal_line(rows, o, role, label, signed_balance) {
    acct_table::add_labeled_amount_line(rows, o.tree_depth * 2, role, false, label, 0,
                                        collectors::sum_collector_commodity(signed_balance, o.report_commodity, o.exchange_fn),
                                        o.tree_depth * 2 - 1)
}

/// `report-line` (:513-527): the net income line, labelled "Net loss" (and negated) when negative, its amount
/// `col + 1` columns in, which for the column GnuCash passes, `2 * (tree_depth - 1)`, is the rightmost level.
/// GnuCash gives it no row markup; it is the statement's bottom line, so its role here is "grandTotal".
private fn add_report_line(rows, o, pos_label, neg_label, amount, col) {
    let mon = collectors::sum_collector_commodity(amount, o.report_commodity, o.exchange_fn);
    let neg = mon.amount < 0;
    let label = if neg { neg_label } else { pos_label };
    acct_table::add_labeled_amount_line(rows, 2 * o.tree_depth, "grandTotal", false, label, 0, if neg { -mon } else { mon },
                                        col + 1)
}

/// `inc-table` and `exp-table` (:478-496): a section's label, its accounts with the balances of the get-balance
/// function (:457-474), and its total.
private fn section(book, ctx, o, env, params, label_shown, label, accts, balances_by_guid, total_shown, total_label, total) {
    let rows = [];
    if label_shown {
        rows = add_subtotal_line(rows, o, "heading", label, ());
    }
    env.get_balance_fn = |acct, start_date, end_date| balances_by_guid[acct.guid];
    let acct_tbl = acct_table::make_html_acct_table_env_accts(book, ctx, env, accts);
    rows = acct_table::add_account_balances(ctx, rows, acct_tbl, params);
    if total_shown {
        rows = add_subtotal_line(rows, o, "total", total_label, total);
    }
    rows
}

/// The statement (:412-571): the revenue and expense tables, the net income below the expenses in the standard
/// order and below the revenue otherwise, one above the other or side by side, and the exchange rates when asked.
private fn statement_blocks(book, ctx, o) {
    let split_up = accounts::decompose_accountlist(o.accounts);
    let totals = statement_totals(ctx, o, split_up);
    let env = #{
        display_tree_depth: o.tree_depth,
        depth_limit_behavior: if o.bottom_behavior { "flatten" } else { "summarize" },
        report_commodity: o.report_commodity,
        exchange_fn: o.exchange_fn,
        parent_account_subtotal_mode: o.parent_total_mode,
        zero_balance_mode: if o.show_zb_accts { "show-leaf-acct" } else { "omit-leaf-acct" },
        account_label_mode: if o.use_links { "anchor" } else { "name" },
    };
    let params = #{
        parent_account_balance_mode: o.parent_balance_mode,
        zero_balance_display_mode: if o.omit_zb_bals { "omit-balance" } else { "show-balance" },
        multicommodity_mode: if o.show_fcur { "table" } else { false },
        rule_mode: o.use_rules,
    };
    let inc = section(book, ctx, o, env, params, o.label_revenue, "Revenues", split_up.INCOME, totals.revenue_accounts,
                      o.total_revenue, "Total Revenue", totals.revenue);
    let exp = section(book, ctx, o, env, params, o.label_expense, "Expenses", split_up.EXPENSE, totals.expense_accounts,
                      o.total_expense, "Total Expenses", totals.expense);
    let net_col = 2 * (o.tree_depth - 1);
    let net_label = `Net income ${o.period_for}`;
    let loss_label = `Net loss ${o.period_for}`;
    if o.standard_order {
        exp = add_report_line(exp, o, net_label, loss_label, totals.net_income, net_col);
    } else {
        inc = add_report_line(inc, o, net_label, loss_label, totals.net_income, net_col);
    }
    let tables = if o.standard_order {
        [statement_table(o, inc), statement_table(o, exp)]
    } else {
        [statement_table(o, exp), statement_table(o, inc)]
    };
    let blocks = if o.two_column { [#{ kind: "group", blocks: tables }] } else { tables };
    if o.show_rates {
        blocks.push(html_utils::make_rates_table(o.report_commodity, o.price_fn, with_commodity(o.accounts)));
    }
    blocks
}

/// The accounts that have a commodity, for the rates table (:568-570), as balance-sheet.rhai's `with_commodity`
/// (`design/reports.md` §14).
private fn with_commodity(accts) {
    let out = [];
    for a in accts {
        if a.commodity != () {
            out.push(a);
        }
    }
    out
}

/// A table of statement rows: the labels, and the amounts in one staircase level per level of the tree depth.
private fn statement_table(o, rows) {
    let levels = if o.tree_depth > 1 { o.tree_depth } else { 1 };
    #{ kind: "table", columns: [#{ label: "", role: "label" }, #{ label: "", role: "amount", levels: levels }], rows: rows }
}
