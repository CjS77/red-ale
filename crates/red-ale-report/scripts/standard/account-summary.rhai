// account-summary.scm : account listing/chart of accounts
//
// Rewritten 2004.07.27 by David Montenegro <sunrise2000@comcast.net>
//   same license & restrictions apply
//
// Copyright 2001 Christian Stimming <stimming@tu-harburg.de>
// Copyright 2000-2001 Bill Gribble <grib@gnumatic.com>
//
// Even older original version by  Terry D. Boldt (tboldt@attglobal.net>
//   Author makes no implicit or explicit guarantee of accuracy of
//   these calculations and accepts no responsibility for direct
//   or indirect losses incurred as a result of using this software.
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
// Translated from 5.17:gnucash/report/reports/standard/account-summary.scm, cited below by line. The file's two
// templates share the options generator and the renderer, told apart by `sx?` (`params.sx`): the Account Summary
// (:423-428) shows balances at a date, the Future Scheduled Transactions Summary (:430-435) what the scheduled
// transactions add to each account over a date range, through `book.sx_cashflow` (v0.4).

import "lib/accounts" as accounts;
import "lib/acct-table" as acct_table;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The two templates (:423-435).
fn templates() {
    [
        #{ guid: "3298541c236b494998b236dfad6ad752", name: "Account Summary", params: #{ reportname: "Account Summary", sx: false } },
        #{ guid: "47f45d7d6d57b68518481c1fc8d4e4ba", name: "Future Scheduled Transactions Summary",
           params: #{ reportname: "Future Scheduled Transactions Summary", sx: true } },
    ]
}

/// The account types the "Accounts" option selects by default (:158-164).
private fn default_account_types() {
    ["BANK", "CASH", "CREDIT", "ASSET", "LIABILITY", "STOCK", "MUTUAL", "CURRENCY", "PAYABLE", "RECEIVABLE", "EQUITY",
     "INCOME", "EXPENSE"]
}

/// `accsum-options-generator` (:138-234).
fn options(params, o, book) {
    o.string("General", "Report Title", "a", "Title for this report.", params.reportname);
    // The date at which to report balances, or the range the scheduled transactions are counted over (:146-151).
    if params.sx {
        opts_util::add_date_interval(o, "General", "Start Date", "End Date", "c");
    } else {
        opts_util::add_report_date(o, "General", "Date", "c");
    }

    o.account_list("Accounts", "Accounts", "a", "Report on these accounts, if display depth allows.",
                   accounts::filter_accountlist_type(default_account_types(), book.root().descendants_sorted()), #{});
    opts_util::add_account_levels(o, "Accounts", "Levels of Subaccounts", "b",
                                  "Maximum number of levels in the account tree displayed.", 3);
    let behavior_help = ["How to treat accounts which exceed the specified depth limit (if any).",
                         "Show the total balance, including balances in subaccounts, of any account at the depth limit.",
                         "Raise accounts deeper than the depth limit to the depth limit.",
                         "Omit any accounts deeper than the depth limit."];
    o.multichoice("Accounts", "Depth limit behavior", "c", join(behavior_help, "\n* "), "summarize", [
        ["summarize", "Recursive Balance"],
        ["flatten", "Raise Accounts"],
        ["truncate", "Omit Accounts"],
    ]);

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
    o.boolean("Display", "Account Balance", "g", "Show an account's balance.", true);
    o.boolean("Display", "Account Code", "h", "Show an account's account code.", true);
    o.boolean("Display", "Account Description", "i", "Show an account's description.", false);
    o.boolean("Display", "Account Type", "j", "Show an account's account type.", false);
    o.boolean("Display", "Account Notes", "k", "Show an account's notes.", false);

    o.set_default_section("Display");
}

/// `parts` joined by `separator`: Guile's `string-join`.
private fn join(parts, separator) {
    let out = "";
    for (part, i) in parts {
        if i > 0 {
            out += separator;
        }
        out += part;
    }
    out
}

/// The option values `accsum-renderer` reads (:248-286), by its names.
private fn read_options(params, book, opts, ctx) {
    let depth_limit = opts.get("Accounts", "Levels of Subaccounts");
    let report_commodity = opts.get("Commodities", "Report's currency");
    let price_source = opts.get("Commodities", "Price Source");
    let to_date = ctx.end_of_day(opts.get("General", if params.sx { "End Date" } else { "Date" }));
    #{
        sx: params.sx,
        report_title: opts.get("General", "Report Title"),
        from_date: if params.sx { ctx.start_of_day(opts.get("General", "Start Date")) } else { () },
        to_date: to_date,
        accounts: opts.get("Accounts", "Accounts"),
        // "just in case we need this information..." (:280-283)
        tree_depth: if depth_limit == "all" { accounts::get_current_account_tree_depth(book) } else { depth_limit },
        bottom_behavior: opts.get("Accounts", "Depth limit behavior"),
        report_commodity: report_commodity,
        show_fcur: opts.get("Commodities", "Show Foreign Currencies"),
        show_rates: opts.get("Commodities", "Show Exchange Rates"),
        parent_mode: opts.get("Display", "Parent account balances"),
        // `(assq-ref '((t . #t) (f . #f)) ...)` (:266-268)
        parent_total_mode: opts.get("Display", "Parent account subtotals") == "t",
        show_zb_accts: opts.get("Display", "Include accounts with zero total balances"),
        omit_zb_bals: opts.get("Display", "Omit zero balance figures"),
        use_links: opts.get("Display", "Display accounts as hyperlinks"),
        use_rules: opts.get("Display", "Show accounting-style rules"),
        show_code: opts.get("Display", "Account Code"),
        show_type: opts.get("Display", "Account Type"),
        show_desc: opts.get("Display", "Account Description"),
        show_notes: opts.get("Display", "Account Notes"),
        show_bals: opts.get("Display", "Account Balance"),
        price_fn: exchange::case_price_fn(book, price_source, report_commodity, to_date),
        exchange_fn: exchange::case_exchange_fn(book, price_source, report_commodity, to_date),
    }
}

/// `accsum-renderer` (:241-421).
fn render(params, book, opts, ctx) {
    let o = read_options(params, book, opts, ctx);
    let title = if o.sx {
        `${book.company_name()} ${o.report_title} For Period Covering ${ctx.print_date(o.from_date)} to ${ctx.print_date(o.to_date)}`
    } else {
        `${book.company_name()} ${o.report_title} ${ctx.print_date(o.to_date)}`
    };
    let blocks = if o.accounts.is_empty() {
        // "is this *really* necessary??" (:303-307)
        [html_utils::make_no_account_warning(params.reportname)]
    } else {
        summary_blocks(book, ctx, o)
    };
    #{ title: title, sections: [#{ blocks: blocks }] }
}

/// The account table (:310-411) and, when asked, the exchange rates (:413-418).
private fn summary_blocks(book, ctx, o) {
    let table_env = #{
        start_date: o.from_date,
        end_date: o.to_date,
        display_tree_depth: o.tree_depth,
        depth_limit_behavior: o.bottom_behavior,
        report_commodity: o.report_commodity,
        exchange_fn: o.exchange_fn,
        parent_account_subtotal_mode: o.parent_total_mode,
        zero_balance_mode: if o.show_zb_accts { "show-leaf-acct" } else { "omit-leaf-acct" },
        account_label_mode: if o.use_links { "anchor" } else { "name" },
        get_balance_fn: if o.sx { sx_balance_fn(book.sx_cashflow(o.from_date, o.to_date)) } else { () },
    };
    let params = #{
        parent_account_balance_mode: o.parent_mode,
        zero_balance_display_mode: if o.omit_zb_bals { "omit-balance" } else { "show-balance" },
        multicommodity_mode: if o.show_fcur { "table" } else { false },
        rule_mode: o.use_rules,
    };
    let chart_table = acct_table::make_html_acct_table_env_accts(book, ctx, table_env, o.accounts);
    let hold_rows = acct_table::add_account_balances(ctx, (), chart_table, params);
    let blocks = [account_table(o, chart_table, hold_rows)];
    if o.show_rates {
        blocks.push(html_utils::make_rates_table(o.report_commodity, o.price_fn,
                                                 accounts::accounts_and_all_descendants(o.accounts)));
    }
    blocks
}

/// The `get-balance-fn` of the scheduled-transaction summary (:310-311, :327-336): an account's balance is what the
/// scheduled transactions add to it, in its own commodity, or an empty collector when they add nothing.
private fn sx_balance_fn(sx_value_hash) {
    |account, start_date, end_date| {
        if account.guid in sx_value_hash { [[account.commodity, sx_value_hash[account.guid]]] } else { [] }
    }
}

/// The table GnuCash assembles in `build-table` (:357-409): the code, type and description columns that are
/// shown, the account title, the balance and the notes. GnuCash copies the balance table's cells beside the
/// metadata cells; here each balance row gets its metadata cells, taken from the row's environment as `add-col`
/// takes them, so subtotal rows repeat their account's code, type, description and notes as GnuCash's do.
private fn account_table(o, chart_table, hold_rows) {
    let meta = [];
    if o.show_code {
        meta.push(["Account Code", "account_code"]);
    }
    if o.show_type {
        meta.push(["Type", "account_type_string"]);
    }
    if o.show_desc {
        meta.push(["Description", "account_description"]);
    }
    let columns = meta.map(|m| #{ label: m[0], role: "text" });
    columns.push(#{ label: "Account title", role: "label" });
    if o.show_bals {
        columns.push(#{ label: "Balance", role: "amount", levels: amount_levels(o.tree_depth) });
    }
    if o.show_notes {
        columns.push(#{ label: "Notes", role: "text" });
    }
    let rows = [];
    for (hold_row, i) in hold_rows {
        let env = acct_table::get_row_env(chart_table, i);
        let cells = meta.map(|m| #{ text: env[m[1]] });
        cells.push(hold_row.cells[0]);
        let row = #{ role: hold_row.role, account: hold_row.account, depth: hold_row.depth, cells: cells };
        if o.show_bals {
            row.cells.push(hold_row.cells[1]);
            row.level = hold_row.level;
        }
        if o.show_notes {
            row.cells.push(#{ text: env.account_notes });
        }
        rows.push(row);
    }
    #{ kind: "table", columns: columns, rows: rows }
}

/// The staircase positions of the balance column: one per level of the tree depth, which bounds every row's display
/// depth (HAT:688-690).
private fn amount_levels(tree_depth) {
    if tree_depth > 1 { tree_depth } else { 1 }
}
