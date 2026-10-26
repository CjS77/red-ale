// balance-sheet.scm: balance sheet
//
// By Robert Merkel <rgmerk@mira.net>
//
// Heavily modified and Frankensteined by David Montenegro
//   2004.06.12-2004.06.23 <sunrise2000@comcast.net>
//
//  * Removed from-date & Net Profit from the report.
//
//  * Updated to use the new gnc:html-acct-table utility object.
//    Added *lots* of new options.  The report can now probably
//    be coerced into the form that *you* want. <grin>
//
// Largely borrowed from pnl.scm by:
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
// Translated from 5.17:gnucash/report/reports/standard/balance-sheet.scm, cited below by line. GnuCash lays the
// report out as one HTML table (two side by side for the two-column form), with a row of 60-pixel spacer cells to
// keep the columns apart (:444-447); here it is one result table per column, its amounts in a column with one
// staircase level per level of the tree depth, and the spacer row is left out as layout.

import "lib/accounts" as accounts;
import "lib/acct-table" as acct_table;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:527-533).
fn templates() {
    [#{ guid: "c4173ac99b2b448289bf4d11c731af13", name: "Balance Sheet", menuPath: ["_Assets & Liabilities"],
        params: #{ reportname: "Balance Sheet" } }]
}

/// The account types the "Accounts" option selects by default (:164-171).
private fn default_account_types() {
    ["BANK", "CASH", "CREDIT", "ASSET", "LIABILITY", "STOCK", "MUTUAL", "CURRENCY", "PAYABLE", "RECEIVABLE", "EQUITY",
     "INCOME", "EXPENSE", "TRADING"]
}

/// `balance-sheet-options-generator` (:140-241).
fn options(params, o, book) {
    o.string("General", "Report Title", "a", "Title for this report.", params.reportname);
    opts_util::add_report_date(o, "General", "Balance Sheet Date", "c");
    o.boolean("General", "Single column Balance Sheet", "d",
              "Print liability/equity section in the same column under the assets section as opposed to a second column " +
              "right of the assets section.", true);
    o.boolean("General", "Use standard US layout", "dd",
              "Report section order is assets/liabilities/equity (rather than assets/equity/liabilities).", true);

    o.account_list("Accounts", "Accounts", "a", "Report on these accounts, if display depth allows.",
                   accounts::filter_accountlist_type(default_account_types(), book.root().descendants_sorted()), #{});
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
    o.boolean("Display", "Label the assets section", "g", "Whether or not to include a label for the assets section.", true);
    o.boolean("Display", "Include assets total", "h", "Whether or not to include a line indicating total assets.", true);
    o.boolean("Display", "Label the liabilities section", "i",
              "Whether or not to include a label for the liabilities section.", true);
    o.boolean("Display", "Include liabilities total", "j",
              "Whether or not to include a line indicating total liabilities.", true);
    o.boolean("Display", "Label the equity section", "k", "Whether or not to include a label for the equity section.", true);
    o.boolean("Display", "Include equity total", "l", "Whether or not to include a line indicating total equity.", true);

    o.set_default_section("Accounts");
}

/// The option values `balance-sheet-renderer` reads (:258-314), by its names.
private fn read_options(book, opts, ctx) {
    let depth_limit = opts.get("Accounts", "Levels of Subaccounts");
    let report_commodity = opts.get("Commodities", "Report's currency");
    let price_source = opts.get("Commodities", "Price Source");
    let reportdate = ctx.end_of_day(opts.get("General", "Balance Sheet Date"));
    #{
        report_title: opts.get("General", "Report Title"),
        reportdate: reportdate,
        report_form: opts.get("General", "Single column Balance Sheet"),
        standard_order: opts.get("General", "Use standard US layout"),
        use_trading_accts: book.uses_trading_accounts(),
        accounts: opts.get("Accounts", "Accounts"),
        // "this can occasionally put extra (blank) columns in our table ..." (:303-310)
        tree_depth: if depth_limit == "all" { accounts::get_current_account_tree_depth(book) } else { depth_limit },
        bottom_behavior: opts.get("Accounts", "Flatten list to depth limit"),
        report_commodity: report_commodity,
        show_fcur: opts.get("Commodities", "Show Foreign Currencies"),
        show_rates: opts.get("Commodities", "Show Exchange Rates"),
        parent_balance_mode: opts.get("Display", "Parent account balances"),
        // `(assq-ref '((t . #t) (f . #f)) ...)` (:276-278)
        parent_total_mode: opts.get("Display", "Parent account subtotals") == "t",
        show_zb_accts: opts.get("Display", "Include accounts with zero total balances"),
        omit_zb_bals: opts.get("Display", "Omit zero balance figures"),
        label_assets: opts.get("Display", "Label the assets section"),
        total_assets: opts.get("Display", "Include assets total"),
        label_liabilities: opts.get("Display", "Label the liabilities section"),
        total_liabilities: opts.get("Display", "Include liabilities total"),
        label_equity: opts.get("Display", "Label the equity section"),
        total_equity: opts.get("Display", "Include equity total"),
        use_links: opts.get("Display", "Display accounts as hyperlinks"),
        use_rules: opts.get("Display", "Show accounting-style rules"),
        price_fn: exchange::case_price_fn(book, price_source, report_commodity, reportdate),
        exchange_fn: exchange::case_exchange_fn(book, price_source, report_commodity, reportdate),
    }
}

/// `balance-sheet-renderer` (:250-525).
fn render(params, book, opts, ctx) {
    let o = read_options(book, opts, ctx);
    let title = `${book.company_name()} ${o.report_title} ${ctx.print_date(o.reportdate)}`;
    let blocks = if o.accounts.is_empty() {
        // "is this *really* necessary??" (:366-373)
        [html_utils::make_no_account_warning(params.reportname)]
    } else {
        statement_blocks(book, ctx, o)
    };
    #{ title: title, sections: [#{ blocks: blocks }] }
}

/// `account-list-balance` (:336-343): the balances of `acct_list` strictly before `reportdate`, in a collector.
fn account_list_balance(acct_list, reportdate) {
    let monetaries = [];
    for acc in acct_list {
        // The trading accounts' parents have no commodity when the book has no root currency
        // (`5.17:libgnucash/engine/Scrub.cpp:493-516`, :1507-1540). GnuCash adds their balance, always zero since
        // they hold no splits, under #f, which changes no figure; a Monetary needs a commodity, so they are left out.
        if acc.commodity != () {
            monetaries.push(monetary(acc.commodity, acc.balance_as_of(reportdate)));
        }
    }
    collectors::monetaries_add(monetaries)
}

/// `(gnc:accounts-get-comm-total-assets accts get-total-value-fn)` (:399-401, :359-360): the values of `accts`' splits
/// up to `reportdate`, by transaction currency, income and expense accounts left out, merged in order. This is
/// `accounts_get_comm_total_assets` of lib/balances with the value function inlined: a closure of this script that
/// calls lib/balances cannot be handed to lib/balances, since Rhai then cannot find the closures lib/collectors
/// makes inside it.
fn total_value(accts, reportdate) {
    let values = [];
    for acct in accts {
        if !accounts::account_is_inc_exp(acct) {
            values.push(balances::account_get_comm_value_at_date(acct, reportdate, false));
        }
    }
    collectors::collector_plus(values)
}

/// The balances the statement shows (:376-408), as commodity collectors: assets, liabilities, equity, retained
/// earnings, trading, unrealized gains, total equity, and liabilities plus equity. Exported for the property tests,
/// which check that assets equal liabilities plus equity.
fn statement_balances(accounts_list, reportdate, use_trading_accts) {
    let split_up = accounts::decompose_accountlist(accounts_list);
    let asset_accounts = split_up.ASSET;
    let liability_accounts = split_up.LIABILITY;
    let asset_balance = account_list_balance(asset_accounts, reportdate);
    let liability_balance = collectors::collector_minus([account_list_balance(liability_accounts, reportdate)]);
    let equity_balance = collectors::collector_minus([account_list_balance(split_up.EQUITY, reportdate)]);
    let retained_earnings = collectors::collector_minus([account_list_balance(split_up.INCOME + split_up.EXPENSE, reportdate)]);
    let trading_balance = if use_trading_accts {
        collectors::collector_minus([account_list_balance(split_up.TRADING, reportdate)])
    } else {
        collectors::collector_plus([])
    };
    let unrealized_gain_collector = if use_trading_accts {
        collectors::collector_plus([])
    } else {
        collectors::collector_minus([asset_balance, liability_balance, total_value(asset_accounts + liability_accounts, reportdate)])
    };
    let total_equity_balance = collectors::collector_plus([equity_balance, retained_earnings, unrealized_gain_collector, trading_balance]);
    #{
        asset: asset_balance,
        liability: liability_balance,
        equity: equity_balance,
        retained_earnings: retained_earnings,
        trading: trading_balance,
        unrealized_gain: unrealized_gain_collector,
        total_equity: total_equity_balance,
        liability_plus_equity: collectors::collector_plus([liability_balance, total_equity_balance]),
    }
}

/// `add-subtotal-line` (:318-329): a total row at the rightmost level, labelled `neg_label` (and negated) when
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

/// `add-rule` (:333-334): a horizontal rule across the table.
fn add_rule(rows) {
    rows.push(#{ role: "rule", cells: [(), ()] });
    rows
}

/// `add-liability-block` (:346-357).
private fn add_liability_block(book, ctx, rows, o, table_env, liability_accounts, params, liability_balance) {
    let liability_table = acct_table::make_html_acct_table_env_accts(book, ctx, table_env, liability_accounts);
    if o.label_liabilities {
        rows = add_subtotal_line(rows, o, "heading", "Liabilities", (), ());
    }
    rows = acct_table::add_account_balances(ctx, rows, liability_table, params);
    if o.total_liabilities {
        rows = add_subtotal_line(rows, o, "total", "Total Liabilities", (), liability_balance);
    }
    add_rule(rows)
}

/// The statement (:376-519): the asset section, then the liabilities and equity, in one table or two side by side,
/// and the exchange rates when asked.
private fn statement_blocks(book, ctx, o) {
    let split_up = accounts::decompose_accountlist(o.accounts);
    let bal = statement_balances(o.accounts, o.reportdate, o.use_trading_accts);
    let table_env = #{
        start_date: (),
        end_date: o.reportdate,
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
    let asset_table = acct_table::make_html_acct_table_env_accts(book, ctx, table_env, split_up.ASSET);
    let equity_table = acct_table::make_html_acct_table_env_accts(book, ctx, table_env, split_up.EQUITY);

    let left = [];
    if o.label_assets {
        left = add_subtotal_line(left, o, "heading", "Assets", (), ());
    }
    left = acct_table::add_account_balances(ctx, left, asset_table, params);
    if o.total_assets {
        left = add_subtotal_line(left, o, "total", "Total Assets", (), bal.asset);
    }
    if o.report_form {
        left = add_rule(add_rule(left));
    }

    // The right-hand table is the left one in the single-column form (:413-415).
    let right = if o.report_form { left } else { [] };
    if o.standard_order {
        right = add_liability_block(book, ctx, right, o, table_env, split_up.LIABILITY, params, bal.liability);
    }
    if o.label_equity {
        right = add_subtotal_line(right, o, "heading", "Equity", (), ());
    }
    right = acct_table::add_account_balances(ctx, right, equity_table, params);
    // "we omit retained earnings & unrealized gains from the balance report, if zero, since they are not present on
    // normal balance sheets" (:470-472)
    if !collectors::commodity_collector_allzero(bal.retained_earnings) {
        right = add_subtotal_line(right, o, "subtotal", "Retained Earnings", "Retained Losses", bal.retained_earnings);
    }
    if !collectors::commodity_collector_allzero(bal.trading) {
        right = add_subtotal_line(right, o, "subtotal", "Trading Gains", "Trading Losses", bal.trading);
    }
    if !collectors::commodity_collector_allzero(bal.unrealized_gain) {
        right = add_subtotal_line(right, o, "subtotal", "Unrealized Gains", "Unrealized Losses", bal.unrealized_gain);
    }
    if o.total_equity {
        right = add_subtotal_line(right, o, "total", "Total Equity", (), bal.total_equity);
    }
    right = add_rule(right);
    if !o.standard_order {
        right = add_liability_block(book, ctx, right, o, table_env, split_up.LIABILITY, params, bal.liability);
    }
    right = add_subtotal_line(right, o, "grandTotal", "Total Liabilities & Equity", (), bal.liability_plus_equity);

    let blocks = if o.report_form {
        [statement_table(o, right)]
    } else {
        [#{ kind: "group", blocks: [statement_table(o, left), statement_table(o, right)] }]
    };
    if o.show_rates {
        blocks.push(html_utils::make_rates_table(o.report_commodity, o.price_fn, with_commodity(o.accounts)));
    }
    blocks
}

/// The accounts that have a commodity. GnuCash hands the rates table every account (BS:517-519), and sorting the
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
