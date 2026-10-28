// budget-balance-sheet.scm: balance sheet from budget projections
// Based on balance-sheet.scm.
//
// Copyright (c) the following:
//
//  Forest Bond <forest@alittletooquiet.net>
//  Robert Merkel <rgmerk@mira.net>
//  David Montenegro <sunrise2000@comcast.net>
//  Christian Stimming <stimming@tu-harburg.de>
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
// Translated from 5.17:gnucash/report/reports/standard/budget-balance-sheet.scm, cited below by line. The sheet is
// dated at the budget's start (:272): each account shows its balance then plus its own Budget amounts over the whole
// budget, unrolled (:226-262). As in balance-sheet.rhai, GnuCash's one HTML table (two side by side in the
// two-column form) is one result table per column, and its row of spacer cells (:613-616) is left out as layout.
//
// GnuCash computes each account's initial and budget balances before it builds the account tables (:423-473), and
// their `get-balance-fn` only looks them up (:244-248, :257-262); here the lookup is a map by GUID.

import "lib/accounts" as accounts;
import "lib/acct-table" as acct_table;
import "lib/balances" as balances;
import "lib/budget" as budget_util;
import "lib/collectors" as collectors;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:785-791); "B_udget" is `gnc:menuname-budget` (5.17:gnucash/report/report-core.scm:159).
fn templates() {
    [#{ guid: "ecc35ea9dbfa4e20ba389fc85d59cb69", name: "Budget Balance Sheet", menuPath: ["B_udget"],
        params: #{ reportname: "Budget Balance Sheet" } }]
}

/// The account types the "Accounts" option selects by default (:132-138): the Balance Sheet's without TRADING.
private fn default_account_types() {
    ["BANK", "CASH", "CREDIT", "ASSET", "LIABILITY", "STOCK", "MUTUAL", "CURRENCY", "PAYABLE", "RECEIVABLE", "EQUITY",
     "INCOME", "EXPENSE"]
}

/// `budget-balance-sheet-options-generator` (:112-212).
fn options(params, o, book) {
    o.string("General", "Report Title", "a", "Title for this report.", params.reportname);
    o.boolean("General", "Single column Balance Sheet", "c",
              "Print liability/equity section in the same column under the assets section as opposed to a second column " +
              "right of the assets section.", true);
    o.budget("General", "Budget", "d", "Budget to use.", book.default_budget());

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
    o.boolean("Display", "Display accounts as hyperlinks", "d",
              "Shows each account in the table as a hyperlink to its register window.", true);
    o.boolean("Display", "Show accounting-style rules", "e",
              "Use rules beneath columns of added numbers like accountants do.", false);
    o.boolean("Display", "Label the assets section", "f", "Whether or not to include a label for the assets section.", true);
    o.boolean("Display", "Include assets total", "g", "Whether or not to include a line indicating total assets.", true);
    o.boolean("Display", "Label the liabilities section", "h",
              "Whether or not to include a label for the liabilities section.", true);
    o.boolean("Display", "Include liabilities total", "i",
              "Whether or not to include a line indicating total liabilities.", true);
    o.boolean("Display", "Label the equity section", "j", "Whether or not to include a label for the equity section.", true);
    o.boolean("Display", "Include equity total", "k", "Whether or not to include a line indicating total equity.", true);
    o.boolean("Display", "Include new/existing totals", "l",
              "Whether or not to include lines indicating change in totals introduced by budget.", true);

    o.set_default_section("Accounts");
}

/// `budget-balance-sheet-renderer` (:221-783).
fn render(params, book, opts, ctx) {
    let reportname = params.reportname;
    let budget = opts.get("General", "Budget");
    // GnuCash titles the document only when it draws the sheet (:580-581); the warnings here leave out the heading
    // that names the report, so the title does.
    if opts.get("Accounts", "Accounts").is_empty() {
        return #{ title: reportname, sections: [#{ blocks: [html_utils::make_no_account_warning(reportname)] }] };
    }
    if budget == () {
        return #{ title: reportname, sections: [#{ blocks: [html_utils::make_generic_budget_warning(reportname)] }] };
    }
    let o = read_options(book, opts, budget);
    let bal = statement_balances(budget, o.accounts, o.date_t64);
    let title = `${book.company_name()} ${o.report_title} ${budget.name}`;
    #{ title: title, sections: [#{ blocks: statement_blocks(book, ctx, o, bal) }] }
}

/// The option values the renderer reads (:267-340), by its names; the date is the budget's start (:272).
private fn read_options(book, opts, budget) {
    let depth_limit = opts.get("Accounts", "Levels of Subaccounts");
    let report_commodity = opts.get("Commodities", "Report's currency");
    let price_source = opts.get("Commodities", "Price Source");
    let date_t64 = budget_util::budget_get_start_date(budget);
    #{
        report_title: opts.get("General", "Report Title"),
        date_t64: date_t64,
        report_form: opts.get("General", "Single column Balance Sheet"),
        accounts: opts.get("Accounts", "Accounts"),
        // "this can occasionally put extra (blank) columns in our table ..." (:327-335)
        tree_depth: if depth_limit == "all" { accounts::get_current_account_tree_depth(book) } else { depth_limit },
        bottom_behavior: opts.get("Accounts", "Flatten list to depth limit"),
        report_commodity: report_commodity,
        show_fcur: opts.get("Commodities", "Show Foreign Currencies"),
        show_rates: opts.get("Commodities", "Show Exchange Rates"),
        parent_balance_mode: opts.get("Display", "Parent account balances"),
        // `(assq-ref '((t . #t) (f . #f)) ...)` (:291-294)
        parent_total_mode: opts.get("Display", "Parent account subtotals") == "t",
        show_zb_accts: opts.get("Display", "Include accounts with zero total balances"),
        omit_zb_bals: opts.get("Display", "Omit zero balance figures"),
        label_assets: opts.get("Display", "Label the assets section"),
        total_assets: opts.get("Display", "Include assets total"),
        label_liabilities: opts.get("Display", "Label the liabilities section"),
        total_liabilities: opts.get("Display", "Include liabilities total"),
        label_equity: opts.get("Display", "Label the equity section"),
        total_equity: opts.get("Display", "Include equity total"),
        new_existing: opts.get("Display", "Include new/existing totals"),
        use_links: opts.get("Display", "Display accounts as hyperlinks"),
        use_rules: opts.get("Display", "Show accounting-style rules"),
        exchange_fn: exchange::case_exchange_fn(book, price_source, report_commodity, date_t64),
        price_fn: exchange::case_price_fn(book, price_source, report_commodity, date_t64),
    }
}

/// `get-assoc-account-balances-budget` with `get-budget-account-initial-balance` (:229-238): each account's balance
/// at the budget's start, its sub-accounts left out.
private fn initial_balances(budget, accts) {
    let out = [];
    for acct in accts {
        out.push([acct, budget_util::budget_account_get_initial_balance(budget, acct)]);
    }
    out
}

/// `get-assoc-account-balances-budget` with `get-budget-account-budget-balance` (:226-238): each account's own Budget
/// amounts over the whole budget.
private fn budget_balances(budget, accts) {
    let out = [];
    for acct in accts {
        out.push([acct, budget_util::budget_account_get_net(budget, acct, (), ())]);
    }
    out
}

/// `(gnc:accounts-get-comm-total-assets accts get-total-value-fn)` (:540-548): the values of `accts`' splits up to
/// `date`, by transaction currency, income and expense accounts left out; as balance-sheet.rhai's `total_value`.
private fn total_value(accts, date) {
    let values = [];
    for acct in accts {
        if !accounts::account_is_inc_exp(acct) {
            values.push(balances::account_get_comm_value_at_date(acct, date, false));
        }
    }
    collectors::collector_plus(values)
}

/// The figures of the sheet (:374-576), as commodity collectors, with the per-account balances the account tables
/// show.
private fn statement_balances(budget, accounts_list, date_t64) {
    let split_up = accounts::decompose_accountlist(accounts_list);
    let asset_initial = initial_balances(budget, split_up.ASSET);
    let asset_budget = budget_balances(budget, split_up.ASSET);
    let liability_initial = initial_balances(budget, split_up.LIABILITY);
    let liability_budget = budget_balances(budget, split_up.LIABILITY);
    let equity_initial = initial_balances(budget, split_up.EQUITY);
    let equity_budget = budget_balances(budget, split_up.EQUITY);

    let existing_liabilities = balances::get_assoc_account_balances_total(liability_initial);
    let new_liabilities = balances::get_assoc_account_balances_total(liability_budget);
    // "existing retained earnings = initial income - initial expenses" (:492-498)
    let existing_retained_earnings = collectors::collector_minus([collectors::collector_plus([
        budget_util::budget_accountlist_get_initial_balance(budget, split_up.INCOME),
        budget_util::budget_accountlist_get_initial_balance(budget, split_up.EXPENSE),
    ])]);
    let new_retained_earnings = collectors::collector_minus([collectors::collector_plus([
        budget_util::budget_accountlist_get_net(budget, split_up.INCOME, (), ()),
        budget_util::budget_accountlist_get_net(budget, split_up.EXPENSE, (), ()),
    ])]);
    let existing_assets = balances::get_assoc_account_balances_total(asset_initial);
    let allocated_assets = balances::get_assoc_account_balances_total(asset_budget);
    // "unallocated-assets = new-retained-earnings - (allocated-assets + new-liabilities)" (:525-531)
    let unallocated_assets = collectors::collector_minus([new_retained_earnings,
                                                         collectors::collector_plus([allocated_assets, new_liabilities])]);
    // :539-553
    let unrealized_gain = collectors::collector_minus([
        collectors::collector_minus([existing_assets, total_value(split_up.ASSET, date_t64)]),
        collectors::collector_minus([existing_liabilities, total_value(split_up.LIABILITY, date_t64)]),
    ]);
    let existing_equity = collectors::collector_plus([
        collectors::commodity_collector_get_negated(balances::get_assoc_account_balances_total(equity_initial)),
        existing_retained_earnings,
        unrealized_gain,
    ]);
    let new_equity = collectors::collector_plus([balances::get_assoc_account_balances_total(equity_budget), new_retained_earnings]);
    let liability_balance = collectors::collector_plus([existing_liabilities, new_liabilities]);
    let equity_balance = collectors::collector_plus([existing_equity, new_equity]);
    #{
        split_up: split_up,
        asset_accounts: account_balances(asset_initial, asset_budget),
        liability_accounts: account_balances(liability_initial, liability_budget),
        equity_accounts: account_balances(equity_initial, equity_budget),
        existing_assets: existing_assets,
        allocated_assets: allocated_assets,
        unallocated_assets: unallocated_assets,
        asset: collectors::collector_plus([existing_assets, allocated_assets, unallocated_assets]),
        existing_liabilities: existing_liabilities,
        new_liabilities: new_liabilities,
        liability: liability_balance,
        existing_retained_earnings: existing_retained_earnings,
        new_retained_earnings: new_retained_earnings,
        retained_earnings: collectors::collector_plus([existing_retained_earnings, new_retained_earnings]),
        unrealized_gain: unrealized_gain,
        existing_equity: existing_equity,
        new_equity: new_equity,
        equity: equity_balance,
        // :574-576
        liability_plus_equity: collectors::collector_plus([collectors::collector_minus([liability_balance]), equity_balance]),
    }
}

/// `sum-prefetched-account-balances-for-account` (:244-248) for every account: its initial balance plus its Budget
/// amounts, by GUID.
private fn account_balances(initial, budgeted) {
    let out = #{};
    for i in 0..initial.len() {
        out[initial[i][0].guid] = collectors::collector_plus([initial[i][1], budgeted[i][1]]);
    }
    out
}

/// `make-sign-handler` (:250-254): negate the liability figures when the first liability account is shown reversed.
private fn liability_sign(ctx, liability_accounts, bal) {
    if !liability_accounts.is_empty() && liability_accounts[0].reverse_balance(ctx) {
        collectors::collector_minus([bal])
    } else {
        bal
    }
}

/// `add-subtotal-line` (:342-353): a total row at the rightmost level, labelled `neg_label` (and negated) when
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

/// `add-rule` (:357-359): a horizontal rule across the table.
fn add_rule(rows) {
    rows.push(#{ role: "rule", cells: [(), ()] });
    rows
}

/// The account table of one section (:620-629, :655-664, :686-694): its accounts with the balances of
/// `make-get-balance-fn` (:257-262), then their rows.
private fn add_accounts(book, ctx, rows, table_env, params, accts, by_guid) {
    let env = table_env;
    env.get_balance_fn = |acct, start_date, end_date| by_guid[acct.guid];
    let acct_tbl = acct_table::make_html_acct_table_env_accts(book, ctx, env, accts);
    acct_table::add_account_balances(ctx, rows, acct_tbl, params)
}

/// The asset section (:619-645).
private fn add_assets(book, ctx, rows, o, bal, table_env, params) {
    if o.label_assets {
        rows = add_subtotal_line(rows, o, "heading", "Assets", (), ());
    }
    rows = add_accounts(book, ctx, rows, table_env, params, bal.split_up.ASSET, bal.asset_accounts);
    if o.total_assets {
        if o.new_existing {
            rows = add_subtotal_line(rows, o, "subtotal", "Existing Assets", (), bal.existing_assets);
            rows = add_subtotal_line(rows, o, "subtotal", "Allocated Assets", (), bal.allocated_assets);
        }
        if !collectors::commodity_collector_allzero(bal.unallocated_assets) {
            rows = add_subtotal_line(rows, o, "subtotal", "Unallocated Assets", (), bal.unallocated_assets);
        }
        rows = add_subtotal_line(rows, o, "total", "Total Assets", (), bal.asset);
    }
    rows
}

/// The liability section and its rule (:653-680).
private fn add_liabilities(book, ctx, rows, o, bal, table_env, params) {
    let liabilities = bal.split_up.LIABILITY;
    if o.label_liabilities {
        rows = add_subtotal_line(rows, o, "heading", "Liabilities", (), ());
    }
    rows = add_accounts(book, ctx, rows, table_env, params, liabilities, bal.liability_accounts);
    if o.total_liabilities {
        if o.new_existing {
            rows = add_subtotal_line(rows, o, "subtotal", "Existing Liabilities", (),
                                     liability_sign(ctx, liabilities, bal.existing_liabilities));
            rows = add_subtotal_line(rows, o, "subtotal", "New Liabilities", (), liability_sign(ctx, liabilities, bal.new_liabilities));
        }
        rows = add_subtotal_line(rows, o, "total", "Total Liabilities", (), liability_sign(ctx, liabilities, bal.liability));
    }
    add_rule(rows)
}

/// The retained earnings (:699-720): left out when zero, "since they are not present on normal balance sheets";
/// the existing and new lines with "Include new/existing totals", else the total alone.
private fn add_retained_earnings(rows, o, bal) {
    if collectors::commodity_collector_allzero(bal.retained_earnings) {
        return rows;
    }
    if o.new_existing {
        rows = add_subtotal_line(rows, o, "subtotal", "Existing Retained Earnings", "Existing Retained Losses",
                                 bal.existing_retained_earnings);
        add_subtotal_line(rows, o, "subtotal", "New Retained Earnings", "New Retained Losses", bal.new_retained_earnings)
    } else {
        add_subtotal_line(rows, o, "subtotal", "Total Retained Earnings", "Total Retained Losses", bal.retained_earnings)
    }
}

/// The equity section and its rule (:683-743).
private fn add_equity(book, ctx, rows, o, bal, table_env, params) {
    if o.label_equity {
        rows = add_subtotal_line(rows, o, "heading", "Equity", (), ());
    }
    rows = add_accounts(book, ctx, rows, table_env, params, bal.split_up.EQUITY, bal.equity_accounts);
    rows = add_retained_earnings(rows, o, bal);
    if !collectors::commodity_collector_allzero(bal.unrealized_gain) {
        rows = add_subtotal_line(rows, o, "subtotal", "Unrealized Gains", "Unrealized Losses", bal.unrealized_gain);
    }
    if o.total_equity {
        if o.new_existing {
            rows = add_subtotal_line(rows, o, "subtotal", "Existing Equity", (), bal.existing_equity);
            rows = add_subtotal_line(rows, o, "subtotal", "New Equity", (), bal.new_equity);
        }
        rows = add_subtotal_line(rows, o, "total", "Total Equity", (), bal.equity);
    }
    add_rule(rows)
}

/// The sheet (:583-778): the assets, then the liabilities and equity, in one table or two side by side, and the
/// exchange rates when asked.
private fn statement_blocks(book, ctx, o, bal) {
    let table_env = #{
        start_date: (),
        end_date: (),
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
    let left = add_assets(book, ctx, [], o, bal, table_env, params);
    if o.report_form {
        left = add_rule(add_rule(left));
    }
    // The right-hand table is the left one in the single-column form (:413-414).
    let right = add_liabilities(book, ctx, if o.report_form { left } else { [] }, o, bal, table_env, params);
    right = add_equity(book, ctx, right, o, bal, table_env, params);
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

/// The accounts that have a commodity, for the rates table (:775-778), as balance-sheet.rhai's `with_commodity`
/// (`design/reports.md` §14).
fn with_commodity(accts) {
    let out = [];
    for a in accts {
        if a.commodity != () {
            out.push(a);
        }
    }
    out
}

/// A table of sheet rows: the labels, and the amounts in one staircase level per level of the tree depth.
private fn statement_table(o, rows) {
    let levels = if o.tree_depth > 1 { o.tree_depth } else { 1 };
    #{ kind: "table", columns: [#{ label: "", role: "label" }, #{ label: "", role: "amount", levels: levels }], rows: rows }
}
