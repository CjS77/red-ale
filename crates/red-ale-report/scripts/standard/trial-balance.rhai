// trial-balance.scm: trial balance and work sheet
// By David Montenegro <sunrise2000@comcast.net>
//
// Prepares a trial balance of your books.
// Optionally prepares a complete work sheet.
//
// N.B.: Since GnuCash ensures that all your debits and credits
// balance, preparing a Trial Balance isn't technically necessary for
// GnuCash users.  This report is included primarily for pedagogical
// and corroborative purposes.
//
// BUGS:
//
//    This code makes the assumption that you want your trial
//    balance to no more than daily resolution.
//
//    Progress bar functionality is currently mostly broken.
//
//    Unsure if the multi-currency support is correct.
//
//    The variables in this code could use more consistent naming.
//
//    See also any "FIXME"s in the code.
//
// Largely borrowed from balance-sheet.scm By Robert Merkel <rgmerk@mira.net>
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
// Translated from 5.17:gnucash/report/reports/standard/trial-balance.scm, cited below by line. GnuCash builds one
// HTML table: a row of 60-pixel spacer cells (:523-526), header rows of `th` cells (:528-573), then the accounts. Here
// the spacer row is left out as layout, "Account Name", "Debit" and "Credit" are the column labels, and the work
// sheet's five headings are a first `heading` row of cells two columns wide. Each figure goes in the column the
// Scheme names: GnuCash's `gnc:html-table-add-labeled-amount-line!` moves the work sheet's Net Income/Loss figure into
// the Adjustments column, and every amount left of its header when the account labels take two or more columns
// (design/reports.md §14). Left out as dead code: the unused "Include accounts with zero total balances" option
// (:264-267, fixed on at :321), `period-for` (:365) and `ws-col` (:565).

import "lib/accounts" as accounts;
import "lib/acct-table" as acct_table;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:868-874).
fn templates() {
    [#{ guid: "216cd0cf6931453ebcce85415aba7082", name: "Trial Balance", menuPath: ["_Income & Expense"], params: #{} }]
}

/// The account types the "Accounts" option selects by default (:197-204).
private fn default_account_types() {
    ["BANK", "CASH", "CREDIT", "ASSET", "LIABILITY", "STOCK", "MUTUAL", "CURRENCY", "PAYABLE", "RECEIVABLE", "EQUITY",
     "INCOME", "EXPENSE", "TRADING"]
}

/// `trial-balance-options-generator` (:170-277).
fn options(params, o, book) {
    o.string("General", "Report Title", "a", "Title for this report.", "Trial Balance");
    // "the period over which to collect adjusting/closing entries and date at which to report the balance" (:178-179)
    opts_util::add_date_interval(o, "General", "Start of Adjusting/Closing", "Date of Report", "c");
    o.multichoice("General", "Report variation", "d", "Kind of trial balance to generate.", "current", [
        ["current", "General journal exact balances"],
        ["pre-adj", "No adjusting/closing entries"],
        ["work-sheet", "Full end-of-period work sheet"],
    ]);

    o.account_list("Accounts", "Accounts", "a", "Report on these accounts.",
                   accounts::filter_accountlist_type(default_account_types(), book.root().descendants_sorted()), #{});
    opts_util::add_account_levels(o, "Accounts", "Levels of Subaccounts", "b",
                                  "Maximum number of levels in the account tree displayed.", 1);

    // "Here, it would be useful to have an inventory account type. Lacking that, just select no accounts by default."
    // (:214-215)
    o.account_list("Merchandising", "Gross adjustment accounts.", "c",
                   "Do not net, but show gross debit/credit adjustments to these accounts. "
                       + "Merchandising businesses will normally select their inventory accounts here.",
                   [], #{});
    o.account_list("Merchandising", "Income summary accounts", "d",
                   "Adjustments made to these accounts are gross adjusted (see above) in the Adjustments, Adjusted Trial Balance, "
                       + "and Income Statement columns. Mostly useful for merchandising businesses.",
                   [], #{});

    opts_util::add_currency(o, "Commodities", "Report's currency", "a");
    opts_util::add_price_source(o, "Commodities", "Price Source", "b", "average-cost");
    o.boolean("Commodities", "Show Foreign Currencies", "c", "Display any foreign currency amount in an account.", false);
    o.boolean("Commodities", "Show Exchange Rates", "d", "Show the exchange rates used.", false);

    // "transactions really should have a field where we can put transaction types like
    // "Adjusting/Closing/Correcting Entries"" (:243-244)
    o.string("Entries", "Adjusting Entries pattern", "a", "Any text in the Description column which identifies adjusting entries.",
             "Adjusting Entries");
    o.boolean("Entries", "Adjusting Entries pattern is case-sensitive", "b",
              "Causes the Adjusting Entries Pattern match to be case-sensitive.", false);
    o.boolean("Entries", "Adjusting Entries Pattern is regular expression", "c",
              "Causes the Adjusting Entries Pattern to be treated as a regular expression.", false);
    o.string("Entries", "Closing Entries pattern", "d", "Any text in the Description column which identifies closing entries.",
             "Closing Entries");
    o.boolean("Entries", "Closing Entries pattern is case-sensitive", "e",
              "Causes the Closing Entries Pattern match to be case-sensitive.", false);
    o.boolean("Entries", "Closing Entries Pattern is regular expression", "f",
              "Causes the Closing Entries Pattern to be treated as a regular expression.", false);

    o.boolean("Display", "Display accounts as hyperlinks", "e",
              "Shows each account in the table as a hyperlink to its register window.", true);

    o.set_default_section("Display");
}

/// The option values `trial-balance-renderer` reads (:294-364), by its names. "This code makes the assumption that
/// you want your trial balance to no more than daily resolution": the adjusting and closing entries are those after
/// 23:59:59 of the day before the start (:296-300).
private fn read_options(book, opts, ctx) {
    let start_date_printable = opts.get("General", "Start of Adjusting/Closing");
    let end_date = dates::time64_end_day_time(ctx, opts.get("General", "Date of Report"));
    let depth_limit = opts.get("Accounts", "Levels of Subaccounts");
    let report_commodity = opts.get("Commodities", "Report's currency");
    let price_source = opts.get("Commodities", "Price Source");
    #{
        report_title: opts.get("General", "Report Title"),
        start_date_printable: start_date_printable,
        start_date: dates::time64_end_day_time(ctx, dates::time64_previous_day(ctx, start_date_printable)),
        end_date: end_date,
        report_variant: opts.get("General", "Report variation"),
        accounts: opts.get("Accounts", "Accounts"),
        ga_accounts: opts.get("Merchandising", "Gross adjustment accounts."),
        is_accounts: opts.get("Merchandising", "Income summary accounts"),
        display_tree_depth: if type_of(depth_limit) == "i64" { depth_limit } else { () },
        adjusting_pattern: #{
            str: opts.get("Entries", "Adjusting Entries pattern"),
            cased: opts.get("Entries", "Adjusting Entries pattern is case-sensitive"),
            regexp: opts.get("Entries", "Adjusting Entries Pattern is regular expression"),
        },
        closing_pattern: #{
            str: opts.get("Entries", "Closing Entries pattern"),
            cased: opts.get("Entries", "Closing Entries pattern is case-sensitive"),
            regexp: opts.get("Entries", "Closing Entries Pattern is regular expression"),
            closing: true,
        },
        report_commodity: report_commodity,
        price_source: price_source,
        show_fcur: opts.get("Commodities", "Show Foreign Currencies"),
        show_rates: opts.get("Commodities", "Show Exchange Rates"),
        use_links: opts.get("Display", "Display accounts as hyperlinks"),
        exchange_fn: exchange::case_exchange_fn(book, price_source, report_commodity, end_date),
        price_fn: exchange::case_price_fn(book, price_source, report_commodity, end_date),
    }
}

/// `trial-balance-renderer` (:286-866).
fn render(params, book, opts, ctx) {
    let o = read_options(book, opts, ctx);
    let start = ctx.print_date(o.start_date_printable);
    let end = ctx.print_date(o.end_date);
    let title = if o.report_variant == "current" {
        `${book.company_name()} ${o.report_title} ${end}`
    } else {
        `${book.company_name()} ${o.report_title} For Period Covering ${start} to ${end}`
    };
    let blocks = if o.accounts.is_empty() {
        // "is this *really* necessary?? i'd be fine with an all-zero trial balance" (:380-389)
        [html_utils::make_no_account_warning("Trial Balance")]
    } else {
        [statement_table(book, ctx, o)]
    };
    if o.show_rates && !o.accounts.is_empty() {
        // Over the option's accounts, so a TRADING account's commodity is listed though it has no row (:857-861).
        blocks.push(html_utils::make_rates_table(o.report_commodity, o.price_fn, o.accounts.filter(|a| a.commodity != ())));
    }
    #{ title: title, sections: [#{ blocks: blocks }] }
}

/// `double-col` (:138-167): `signed_balance` in the report's currency, as the entry to show and whether it is a
/// credit. Negative is a credit, anything else a debit, and the entry is always the absolute value. It is blank when
/// the balance holds only the report's currency (or nothing) and comes to 0; with another commodity it is a commodity
/// table under "Show Foreign Currencies", else the converted sum, so a foreign balance of 0 shows as $0.00.
private fn double_col(signed_balance, o) {
    let amt = collectors::sum_collector_commodity(signed_balance, o.report_commodity, o.exchange_fn).amount;
    let neg = amt < 0;
    let bal = if neg { collectors::commodity_collector_get_negated(signed_balance) } else { signed_balance };
    let bal_sum = collectors::sum_collector_commodity(bal, o.report_commodity, o.exchange_fn);
    let balance = if collectors::uniform_commodity(bal, o.report_commodity) {
        if amt == 0 { () } else { #{ monetary: bal_sum } }
    } else if o.show_fcur {
        acct_table::gnc_commodity_table(bal, o.report_commodity, o.exchange_fn)
    } else {
        #{ monetary: bal_sum }
    };
    #{ entry: balance, credit: neg }
}

/// `tot-abs-amt-cell` (:446-459): the absolute value of `amt` in the report's currency.
private fn tot_abs_amt_cell(amt, o) {
    let rv = collectors::sum_collector_commodity(amt, o.report_commodity, o.exchange_fn);
    if rv.amount < 0 {
        rv = collectors::sum_collector_commodity(collectors::commodity_collector_get_negated(amt), o.report_commodity, o.exchange_fn);
    }
    #{ monetary: rv }
}

/// `totals` with `coll` merged into its column `i`, or minus-merged when `minus`: GnuCash's running totals, ten
/// collectors (the debit and credit of each column pair).
private fn tally(totals, i, coll, minus) {
    let total = totals[i];
    if minus {
        total.minusmerge(coll);
    } else {
        total.merge(coll);
    }
    totals[i] = total;
    totals
}

/// `sum-account-splits` (:598-608): the amounts of the splits of `account` among `splits`, only the positive ones
/// with `pos_filter`, under the account's commodity.
private fn sum_account_splits(account, splits, pos_filter) {
    let total = collectors::make_commodity_collector();
    let comm = account.commodity;
    for s in splits {
        let amt = s.amount;
        if s.account.guid == account.guid && (!pos_filter || amt > 0) {
            total.add(comm, amt);
        }
    }
    total
}

/// A set of the GUIDs of `split_up`'s asset, liability, equity, income and expense accounts (:342-358).
private fn guid_set(split_up) {
    let set = #{};
    for a in split_up.ASSET + split_up.LIABILITY + split_up.EQUITY + split_up.INCOME + split_up.EXPENSE {
        set[a.guid] = true;
    }
    set
}

/// The values of one account row (:610-660), by column pair: 0 the pre-adjustment balance, 1 the adjustments,
/// 2 the adjusted trial balance, 3 the income statement and 4 the balance sheet (one of them `()`), 5 the current
/// balance. A gross figure (gross adjustment and income summary accounts) is a map `#{ debit, credit }`.
private fn column_values(o, env, closing_splits, adjusting_splits, ga_set, is_set) {
    let acct = env.account;
    let curr_bal = env.account_bal;
    let closing = sum_account_splits(acct, closing_splits, false);
    let adjusting = sum_account_splits(acct, adjusting_splits, false);
    let is_acct = acct.guid in is_set;
    let ga_or_is = acct.guid in ga_set || is_acct;
    let pos_adjusting = if ga_or_is { sum_account_splits(acct, adjusting_splits, true) } else { () };
    let neg_adjusting = if ga_or_is { collectors::collector_minus([adjusting, pos_adjusting]) } else { () };
    let pre_closing_bal = collectors::collector_minus([curr_bal, closing]);
    let pre_adjusting_bal = collectors::collector_minus([pre_closing_bal, adjusting]);
    // "no explanation was offered in bug #150008" (:641-643)
    let atb = if !is_acct {
        pre_closing_bal
    } else if double_col(pre_adjusting_bal, o).credit {
        #{ debit: collectors::collector_plus([pos_adjusting]),
           credit: collectors::collector_plus([neg_adjusting, pre_adjusting_bal]) }
    } else {
        #{ debit: collectors::collector_plus([pos_adjusting, pre_adjusting_bal]),
           credit: collectors::collector_plus([neg_adjusting]) }
    };
    let in_is = accounts::account_is_inc_exp(acct) || is_acct;
    [pre_adjusting_bal, if ga_or_is { #{ debit: pos_adjusting, credit: neg_adjusting } } else { adjusting }, atb,
     if in_is { atb } else { () }, if in_is { () } else { atb }, curr_bal]
}

/// `collect-unrealized-gains` (:473-488): the book's balance strictly before the end, valued by the price source less
/// the same at average cost; nothing under average cost. Accounts with no commodity are left out, as the Balance
/// Sheet leaves them out.
private fn collect_unrealized_gains(book, o, all_accounts) {
    if o.price_source == "average-cost" {
        return collectors::make_commodity_collector();
    }
    let cost_fn = exchange::case_exchange_fn(book, "average-cost", o.report_commodity, o.end_date);
    let acct_balances = [];
    for acct in all_accounts {
        if acct.commodity != () {
            acct_balances.push(balances::account_get_comm_balance_at_date(acct, o.end_date, false));
        }
    }
    let book_balance = collectors::collector_plus(acct_balances);
    let value = collectors::sum_collector_commodity(book_balance, o.report_commodity, o.exchange_fn);
    let cost = collectors::sum_collector_commodity(book_balance, o.report_commodity, cost_fn);
    collectors::monetaries_add([value, -cost])
}

/// A row of `width` cells, the label (text, a cell or `()`) first, the rest empty.
private fn blank_row(role, label, width) {
    let cells = [if type_of(label) == "string" { #{ text: label } } else { label }];
    cells.pad(width, ());
    #{ role: role, cells: cells }
}

/// `add-line` (:413-427): `row` with `signed_balance` as a Trial Balance debit or credit, and the running totals.
/// Returns `[row, totals, credit]`.
private fn add_line(row, totals, signed_balance, o) {
    let d = double_col(signed_balance, o);
    row.cells[if d.credit { 2 } else { 1 }] = d.entry;
    [row, tally(totals, if d.credit { 1 } else { 0 }, signed_balance, d.credit), d]
}

/// The work sheet's cells of an account row (:723-778), from column pair 1 to 4: an unset value is blank and adds
/// nothing; a net one goes to its debit or credit side; a gross one shows each side, its debit merged into the debit
/// total and its credit minus-merged into the credit total. Returns `[row, totals]`.
private fn work_sheet_cells(row, totals, values, o) {
    for p in 1..5 {
        let bal = values[p];
        if bal == () {
            continue;
        }
        if type_of(bal) == "map" {
            row.cells[1 + 2 * p] = double_col(bal.debit, o).entry;
            row.cells[2 + 2 * p] = double_col(bal.credit, o).entry;
            totals = tally(totals, 2 * p + 1, bal.credit, true);
            totals = tally(totals, 2 * p, bal.debit, false);
        } else {
            let d = double_col(bal, o);
            row.cells[if d.credit { 2 + 2 * p } else { 1 + 2 * p }] = d.entry;
            totals = tally(totals, if d.credit { 2 * p + 1 } else { 2 * p }, bal, d.credit);
        }
    }
    [row, totals]
}

/// The statement (:391-853): a row per selected account but the TRADING ones, the unrealized gains, the totals and,
/// for the work sheet, the net income and the grand totals.
private fn statement_table(book, ctx, o) {
    let split_up = accounts::decompose_accountlist(o.accounts);
    // INCOME reversed onto EXPENSE (`append-reverse`, :330-332); the TRADING accounts are never used.
    let income = split_up.INCOME;
    income.reverse();
    let all_accounts = split_up.ASSET + split_up.LIABILITY + split_up.EQUITY + income + split_up.EXPENSE;
    if all_accounts.is_empty() {
        // GnuCash fails here, reading the account columns of a table with no row (:520-523).
        throw "Trial Balance: only TRADING accounts are selected, and the trial balance leaves TRADING accounts out";
    }
    let table_env = #{
        start_date: (),
        end_date: o.end_date,
        display_tree_depth: o.display_tree_depth,
        depth_limit_behavior: "flatten",
        report_commodity: o.report_commodity,
        exchange_fn: o.exchange_fn,
        parent_account_subtotal_mode: false,
        zero_balance_mode: "show-leaf-acct",
        account_label_mode: if o.use_links { "anchor" } else { "name" },
    };
    let table = acct_table::make_html_acct_table_env_accts(book, ctx, table_env, all_accounts);
    let closing_splits = balances::account_get_trans_type_splits_interval(book, all_accounts, o.closing_pattern, o.start_date, o.end_date);
    let adjusting_splits = balances::account_get_trans_type_splits_interval(book, all_accounts, o.adjusting_pattern, o.start_date,
                                                                             o.end_date);
    let ga_set = guid_set(accounts::decompose_accountlist(o.ga_accounts));
    let is_set = guid_set(accounts::decompose_accountlist(o.is_accounts));
    let work_sheet = o.report_variant == "work-sheet";
    let width = if work_sheet { 11 } else { 3 };
    let totals = [];
    totals.pad(10, []);
    let neg_unrealized_gain_collector = collectors::commodity_collector_get_negated(collect_unrealized_gains(book, o, all_accounts));

    let rows = [];
    let indented_depth = 0;
    for env in table.rows {
        let values = column_values(o, env, closing_splits, adjusting_splits, ga_set, is_set);
        let row = blank_row("account", env.account_label, width);
        row.account = env.account;
        row.depth = env.indented_depth;
        // "yeah, i know, global vars are devil... so deal with it" (:676-677)
        indented_depth = env.indented_depth;
        let added = add_line(row, totals, if o.report_variant == "current" { values[5] } else { values[0] }, o);
        row = added[0];
        totals = added[1];
        if work_sheet {
            let ws = work_sheet_cells(row, totals, values, o);
            row = ws[0];
            totals = ws[1];
        }
        rows.push(row);
    }
    // "we omit unrealized gains from the balance report, if zero, since they are not present on normal trial
    // balances" (:683-684). The row takes the last account row's indentation.
    if !collectors::commodity_collector_allzero(neg_unrealized_gain_collector) {
        let d = double_col(neg_unrealized_gain_collector, o);
        let row = blank_row("data", if d.credit { "Unrealized Gains" } else { "Unrealized Losses" }, width);
        row.depth = indented_depth;
        let added = add_line(row, totals, neg_unrealized_gain_collector, o);
        row = added[0];
        totals = added[1];
        if work_sheet {
            // In the adjusted trial balance and the balance sheet, on its side (:700-714).
            let side = if d.credit { 1 } else { 0 };
            for p in [2, 4] {
                row.cells[1 + 2 * p + side] = d.entry;
                totals = tally(totals, 2 * p + side, neg_unrealized_gain_collector, d.credit);
            }
        }
        rows.push(row);
    }
    let total_row = blank_row("total", (), width);
    for i in 0..(width - 1) {
        total_row.cells[i + 1] = tot_abs_amt_cell(totals[i], o);
    }
    rows.push(total_row);
    if work_sheet {
        rows += net_income_rows(totals, o);
    }
    let columns = [#{ label: "Account Name", role: "label" }];
    for i in 0..(width - 1) {
        columns.push(#{ label: if i % 2 == 0 { "Debit" } else { "Credit" }, role: "amount" });
    }
    let headings = [];
    if work_sheet {
        let heading = [()];
        for h in ["Trial Balance", "Adjustments", "Adjusted Trial Balance", "Income Statement", "Balance Sheet"] {
            heading.push(#{ text: h, span: 2 });
        }
        headings.push(#{ role: "heading", cells: heading });
    }
    #{ kind: "table", columns: columns, rows: headings + rows }
}

/// The work sheet's last two rows (:799-850): the net income or loss, then the grand totals of the income statement
/// and the balance sheet. Credits above debits in the income statement are a net income, shown as its debit;
/// otherwise a net loss, shown as its credit, blank when 0. The balance sheet's net goes to its debit when negative,
/// else to its credit, $0.00 when 0. GnuCash's HTML shows the income statement figure in the Adjustments column
/// (HAT:899-901); it goes in the Income Statement column the Scheme names (:821-826).
private fn net_income_rows(totals, o) {
    let net_is = collectors::collector_minus([totals[6], totals[7]]);
    let net_bs = collectors::collector_minus([totals[8], totals[9]]);
    let is_net = double_col(net_is, o);
    let bs_credit = double_col(net_bs, o).credit;
    let net_row = blank_row("total", if is_net.credit { "Net Income" } else { "Net Loss" }, 11);
    net_row.cells[if is_net.credit { 7 } else { 8 }] = is_net.entry;
    net_row.cells[if bs_credit { 9 } else { 10 }] = tot_abs_amt_cell(net_bs, o);
    // "now slap on the grand totals" (:833-841)
    let tot_is = collectors::make_commodity_collector();
    tot_is.merge(if is_net.credit { totals[6] } else { totals[7] });
    if is_net.credit { tot_is.minusmerge(net_is) } else { tot_is.merge(net_is) }
    let tot_bs = collectors::make_commodity_collector();
    tot_bs.merge(if bs_credit { totals[8] } else { totals[9] });
    if bs_credit { tot_bs.minusmerge(net_bs) } else { tot_bs.merge(net_bs) }
    let grand = blank_row("grandTotal", (), 11);
    grand.cells[7] = tot_abs_amt_cell(if is_net.credit { tot_is } else { totals[6] }, o);
    grand.cells[8] = tot_abs_amt_cell(if is_net.credit { totals[7] } else { tot_is }, o);
    grand.cells[9] = tot_abs_amt_cell(if bs_credit { tot_bs } else { totals[8] }, o);
    grand.cells[10] = tot_abs_amt_cell(if bs_credit { totals[9] } else { tot_bs }, o);
    [net_row, grand]
}
