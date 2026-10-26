// income-statement.scm: income statement (a.k.a. Profit & Loss)
//
// By David Montenegro <sunrise2000@comcast.net>
//  2004.07.13 - 2004.07.14
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
// Translated from 5.17:gnucash/report/reports/standard/income-statement.scm, cited below by line. The two templates,
// Income Statement and Profit & Loss, share one options generator and renderer and differ only in their names, which
// come in as `params.reportname` (:552-582). GnuCash lays the two-column form out as one row of nested tables
// (:483-502); here it is a group of tables side by side.

import "lib/accounts" as accounts;
import "lib/acct-table" as acct_table;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The templates (:566-582).
fn templates() {
    [#{ guid: "0b81a3bdfd504aff849ec2e8630524bc", name: "Income Statement", menuPath: ["_Income & Expense"],
        params: #{ reportname: "Income Statement" } },
     #{ guid: "8758ba23984c40dea5527f5f0ca2779e", name: "Profit & Loss", menuPath: ["_Income & Expense"],
        params: #{ reportname: "Profit & Loss" } }]
}

/// `income-statement-options-generator-internal` (:132-245).
fn options(params, o, book) {
    o.string("General", "Report Title", "a", "Title for this report.", params.reportname);
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", "c");

    // "select, by default, only income and expense accounts" (:149-152)
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
    o.boolean("Display", "Display accounts as hyperlinks", "e",
              "Shows each account in the table as a hyperlink to its register window.", true);
    o.boolean("Display", "Show accounting-style rules", "f",
              "Use rules beneath columns of added numbers like accountants do.", false);
    o.boolean("Display", "Label the revenue section", "g", "Whether or not to include a label for the revenue section.", true);
    o.boolean("Display", "Include revenue total", "h", "Whether or not to include a line indicating total revenue.", true);
    o.boolean("Display", "Label the trading accounts section", "h1",
              "Whether or not to include a label for the trading accounts section.", true);
    o.boolean("Display", "Include trading accounts total", "h2",
              "Whether or not to include a line indicating total trading accounts balance.", true);
    o.boolean("Display", "Label the expense section", "i", "Whether or not to include a label for the expense section.", true);
    o.boolean("Display", "Include expense total", "j", "Whether or not to include a line indicating total expense.", true);
    o.boolean("Display", "Display as a two column report", "k",
              "Divides the report into an income column and an expense column.", false);
    o.boolean("Display", "Display in standard, income first, order", "l",
              "Causes the report to display in the standard order, placing income before expenses.", true);

    // "transactions really should have a field where we can put transaction types like
    // "Adjusting/Closing/Correcting Entries"" (:228-231)
    o.string("Entries", "Closing Entries pattern", "a", "Any text in the Description column which identifies closing entries.",
             "Closing Entries");
    o.boolean("Entries", "Closing Entries pattern is case-sensitive", "b",
              "Causes the Closing Entries Pattern match to be case-sensitive.", false);
    o.boolean("Entries", "Closing Entries Pattern is regular expression", "c",
              "Causes the Closing Entries Pattern to be treated as a regular expression.", false);

    o.set_default_section("Accounts");
}

/// The option values `income-statement-renderer-internal` reads (:260-348), by its names.
private fn read_options(book, opts, ctx) {
    let depth_limit = opts.get("Accounts", "Levels of Subaccounts");
    let report_commodity = opts.get("Commodities", "Report's currency");
    let price_source = opts.get("Commodities", "Price Source");
    let end_date = ctx.end_of_day(opts.get("General", "End Date"));
    #{
        report_title: opts.get("General", "Report Title"),
        start_date_printable: opts.get("General", "Start Date"),
        start_date: ctx.start_of_day(opts.get("General", "Start Date")),
        end_date: end_date,
        accounts: opts.get("Accounts", "Accounts"),
        // "this can occasionally put extra (blank) columns in our table ..." (:337-344)
        tree_depth: if depth_limit == "all" { accounts::get_current_account_tree_depth(book) } else { depth_limit },
        bottom_behavior: opts.get("Accounts", "Flatten list to depth limit"),
        report_commodity: report_commodity,
        show_fcur: opts.get("Commodities", "Show Foreign Currencies"),
        show_rates: opts.get("Commodities", "Show Exchange Rates"),
        parent_balance_mode: opts.get("Display", "Parent account balances"),
        // `(assq-ref '((t . #t) (f . #f)) ...)` (:290-293)
        parent_total_mode: opts.get("Display", "Parent account subtotals") == "t",
        show_zb_accts: opts.get("Display", "Include accounts with zero total balances"),
        omit_zb_bals: opts.get("Display", "Omit zero balance figures"),
        label_revenue: opts.get("Display", "Label the revenue section"),
        total_revenue: opts.get("Display", "Include revenue total"),
        label_trading: opts.get("Display", "Label the trading accounts section"),
        total_trading: opts.get("Display", "Include trading accounts total"),
        label_expense: opts.get("Display", "Label the expense section"),
        total_expense: opts.get("Display", "Include expense total"),
        use_links: opts.get("Display", "Display accounts as hyperlinks"),
        use_rules: opts.get("Display", "Show accounting-style rules"),
        two_column: opts.get("Display", "Display as a two column report"),
        standard_order: opts.get("Display", "Display in standard, income first, order"),
        closing_pattern: #{
            str: opts.get("Entries", "Closing Entries pattern"),
            cased: opts.get("Entries", "Closing Entries pattern is case-sensitive"),
            regexp: opts.get("Entries", "Closing Entries Pattern is regular expression"),
            closing: true,
        },
        exchange_fn: exchange::case_exchange_fn(book, price_source, report_commodity, end_date),
        price_fn: exchange::case_price_fn(book, price_source, report_commodity, end_date),
    }
}

/// `income-statement-renderer-internal` (:252-550).
fn render(params, book, opts, ctx) {
    let o = read_options(book, opts, ctx);
    let start = ctx.print_date(o.start_date_printable);
    let title = `${book.company_name()} ${o.report_title} For Period Covering ${start} to ${ctx.print_date(o.end_date)}`;
    let blocks = if o.accounts.is_empty() {
        // "is this *really* necessary??" (:376-383)
        [html_utils::make_no_account_warning(params.reportname)]
    } else {
        statement_blocks(book, ctx, o)
    };
    #{ title: title, sections: [#{ blocks: blocks }] }
}

/// The totals the statement shows (:386-407), as commodity collectors. Revenue and expense leave out the closing
/// entries the pattern matches; trading does not. Exported for the tests.
fn statement_totals(book, accounts_list, closing_pattern, start_date, end_date) {
    let split_up = accounts::decompose_accountlist(accounts_list);
    let expense_accounts = split_up.EXPENSE;
    let revenue_accounts = split_up.INCOME;
    let expense_total = collectors::collector_minus([
        balances::accountlist_get_comm_balance_interval_with_closing(book, expense_accounts, start_date, end_date),
        balances::account_get_trans_type_balance_interval_with_closing(book, expense_accounts, closing_pattern, start_date, end_date),
    ]);
    let revenue_total = collectors::collector_minus([
        balances::account_get_trans_type_balance_interval_with_closing(book, revenue_accounts, closing_pattern, start_date, end_date),
        balances::accountlist_get_comm_balance_interval_with_closing(book, revenue_accounts, start_date, end_date),
    ]);
    let trading_total = balances::accountlist_get_comm_balance_interval_with_closing(book, split_up.TRADING, start_date, end_date);
    #{
        expense: expense_total,
        revenue: revenue_total,
        trading: trading_total,
        net_income: collectors::collector_plus([revenue_total, trading_total, collectors::collector_minus([expense_total])]),
    }
}

/// `add-subtotal-line` (:352-363): a total row at the rightmost level, labelled `neg_label` (and negated) when
/// `signed_balance` is negative in the report commodity and there is a `neg_label`; no amount when `signed_balance`
/// is `()`. GnuCash marks every such row "primary-subheading"; `role` says which kind of line it is.
fn add_subtotal_line(rows, o, role, pos_label, neg_label, signed_balance) {
    let neg = signed_balance != () && neg_label != ()
        && collectors::sum_collector_commodity(signed_balance, o.report_commodity, o.exchange_fn).amount < 0;
    let label = if neg { neg_label } else { pos_label };
    let balance = if neg { collectors::collector_minus([signed_balance]) } else { signed_balance };
    acct_table::add_labeled_amount_line(rows, o.tree_depth * 2, role, false, label, 0,
                                        collectors::sum_collector_commodity(balance, o.report_commodity, o.exchange_fn),
                                        o.tree_depth * 2 - 1)
}

/// `add-rule` (:366-367): a horizontal rule across the table.
fn add_rule(rows) {
    rows.push(#{ role: "rule", cells: [(), ()] });
    rows
}

/// `add-report-line` (:445-457): the net income line, labelled "Net loss" (and negated) when negative. GnuCash
/// gives it no row markup (`row-style` #f); it is the statement's bottom line, so its role here is "grandTotal".
/// The amount goes `col + 1` columns in, which for the column GnuCash passes, `2 * (tree_depth - 1)`, is the
/// rightmost level.
private fn add_report_line(rows, o, pos_label, neg_label, amount, col) {
    let mon = collectors::sum_collector_commodity(amount, o.report_commodity, o.exchange_fn);
    let neg = amount != () && neg_label != () && mon.amount < 0;
    let label = if neg { neg_label } else { pos_label };
    let bal = if neg { -mon } else { mon };
    acct_table::add_labeled_amount_line(rows, 2 * o.tree_depth, "grandTotal", false, label, 0, bal, col + 1)
}

/// `add-revenue-table`, `add-expense-table` and `add-trading-table` (:459-481): a section's label, its accounts and
/// its total.
private fn add_section(ctx, rows, o, label_shown, label, acct_tbl, params, total_shown, total_label, total) {
    if label_shown {
        rows = add_subtotal_line(rows, o, "heading", label, (), ());
    }
    rows = acct_table::add_account_balances(ctx, rows, acct_tbl, params);
    if total_shown {
        rows = add_subtotal_line(rows, o, "total", total_label, (), total);
    }
    rows
}

/// The statement (:386-546): revenue, trading and expenses in one table or side by side, the net income, and the
/// exchange rates when asked.
private fn statement_blocks(book, ctx, o) {
    let split_up = accounts::decompose_accountlist(o.accounts);
    let totals = statement_totals(book, o.accounts, o.closing_pattern, o.start_date, o.end_date);
    let table_env = #{
        start_date: o.start_date,
        end_date: o.end_date,
        display_tree_depth: o.tree_depth,
        depth_limit_behavior: if o.bottom_behavior { "flatten" } else { "summarize" },
        report_commodity: o.report_commodity,
        exchange_fn: o.exchange_fn,
        parent_account_subtotal_mode: o.parent_total_mode,
        zero_balance_mode: if o.show_zb_accts { "show-leaf-acct" } else { "omit-leaf-acct" },
        account_label_mode: if o.use_links { "anchor" } else { "name" },
        // "we may, at some point, want to add an option to generate a pre-adjustment income statement..." (:422-424)
        balance_mode: "pre-closing",
        closing_pattern: o.closing_pattern,
    };
    let params = #{
        parent_account_balance_mode: o.parent_balance_mode,
        zero_balance_display_mode: if o.omit_zb_bals { "omit-balance" } else { "show-balance" },
        multicommodity_mode: if o.show_fcur { "table" } else { false },
        rule_mode: o.use_rules,
    };
    let revenue_table = acct_table::make_html_acct_table_env_accts(book, ctx, table_env, split_up.INCOME);
    let expense_table = acct_table::make_html_acct_table_env_accts(book, ctx, table_env, split_up.EXPENSE);
    let trading_table = acct_table::make_html_acct_table_env_accts(book, ctx, table_env, split_up.TRADING);
    let has_trading = !split_up.TRADING.is_empty();
    let net_col = 2 * (o.tree_depth - 1);

    let revenue = add_section(ctx, [], o, o.label_revenue, "Revenues", revenue_table, params, o.total_revenue,
                              "Total Revenue", totals.revenue);
    let expense = add_section(ctx, [], o, o.label_expense, "Expenses", expense_table, params, o.total_expense,
                              "Total Expenses", totals.expense);
    let trading = add_section(ctx, [], o, o.label_trading, "Trading", trading_table, params, o.total_trading,
                              "Total Trading", totals.trading);

    let blocks = if o.two_column {
        // The net income goes below the expenses in the standard order, else below the revenue (:494-499).
        if o.standard_order {
            expense = add_report_line(add_rule(expense), o, "Net income for Period", "Net loss for Period", totals.net_income, net_col);
        } else {
            revenue = add_report_line(add_rule(revenue), o, "Net income for Period", "Net loss for Period", totals.net_income, net_col);
        }
        // An empty cell stands for the trading table when no trading account is selected (:489-491).
        let trading_block = if has_trading { [statement_table(o, trading)] } else { [] };
        let columns = if o.standard_order {
            [statement_table(o, revenue)] + trading_block + [statement_table(o, expense)]
        } else {
            [statement_table(o, expense), statement_table(o, revenue)] + trading_block
        };
        [#{ kind: "group", blocks: columns }]
    } else {
        let first = if o.standard_order { revenue } else { expense };
        let last = if o.standard_order { expense } else { revenue };
        let rows = add_rule(first);
        if has_trading {
            rows = add_rule(rows + trading);
        }
        rows = add_rule(rows + last);
        rows = add_report_line(rows, o, "Net income for Period", "Net loss for Period", totals.net_income, net_col);
        [statement_table(o, rows)]
    };
    if o.show_rates {
        blocks.push(html_utils::make_rates_table(o.report_commodity, o.price_fn, with_commodity(o.accounts)));
    }
    blocks
}

/// The accounts that have a commodity. GnuCash hands the rates table every account (:542-545), and sorting the
/// commodities fails on the missing one of a trading parent made without a root currency
/// (`5.17:gnucash/report/report-utilities.scm:242-251`); red-ale leaves those accounts out, which lists the same
/// rates (`design/reports.md` §14).
fn with_commodity(accts) {
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
