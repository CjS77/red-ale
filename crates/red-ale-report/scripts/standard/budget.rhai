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
// Translated from 5.17:gnucash/report/reports/standard/budget.scm, cited below by line.
//
// GnuCash builds the account table's rows with `gnc:html-table-add-account-balances` and writes the budget columns
// over them (:737-745). The balances it computes there are always empty: the account table gets no exchange
// function, and `gnc:sum-collector-commodity` returns #f without one (5.17:gnucash/report/commodity-utilities.scm:
// 900-902). So each row here is the account's label followed by the budget columns. GnuCash's two header rows, the
// column titles and "Bgt", "Act" and "Diff", are heading rows; a period column's title spans its figures. A budget
// note becomes a footnote of the table, as GnuCash's footnote collector numbers and lists them (:240-259).
//
// Options' callbacks only make other options selectable in GnuCash's dialog, so none is passed: "Report for range of
// budget periods" enables the range and collapse options (:130-148), a "manual" range start or end enables its exact
// period (:150-174), "Show Budget" enables "Show Budget Notes" (:208-212) and "Show Actual" enables "Link to actual
// transactions" (:216-221).

import "lib/accounts" as accounts;
import "lib/acct-table" as acct_table;
import "lib/budget" as budget_util;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The Transaction Report's GUID, `trep-uuid`, which "Link to actual transactions" opens (:39).
private fn trep_uuid() {
    "2fe3b9833af044abb929a88d5a59620f"
}

/// The template (:763-769); "B_udget" is `gnc:menuname-budget` (5.17:gnucash/report/report-core.scm:159).
fn templates() {
    [#{ guid: "810ed4b25ef0486ea43bbd3dddb32b11", name: "Budget Report", menuPath: ["B_udget"],
        params: #{ reportname: "Budget Report" } }]
}

/// The choices of "Range start" and "Range end" (:110-116).
private fn period_options() {
    [
        ["first", "First budget period"],
        ["previous", "Previous budget period"],
        ["current", "Current budget period"],
        ["next", "Next budget period"],
        ["last", "Last budget period"],
        ["manual", "Manual period selection"],
    ]
}

/// `budget-report-options-generator` (:108-238).
fn options(params, o, book) {
    o.budget("General", "Budget", "a", "Budget to use.", book.default_budget());
    o.boolean("General", "Use accumulated amounts", "b", "Values are accumulated across periods.", false);
    o.complex_boolean("General", "Report for range of budget periods", "f",
                      "Create report for a budget period range instead of the entire budget.", false, ());
    o.multichoice_callback("General", "Range start", "g1.1", "Select a budget period type that starts the reporting range.",
                           "current", period_options(), ());
    // GnuCash's FIXME: the 60 periods of the budget dialog are not defined anywhere the options could use (:162-164).
    o.number_range("General", "Exact start period", "g1.2", "Select exact period that starts the reporting range.",
                   1, 1, 60, 1);
    o.multichoice_callback("General", "Range end", "g2.1", "Select a budget period type that ends the reporting range.",
                           "next", period_options(), ());
    o.number_range("General", "Exact end period", "g2.2", "Select exact period that ends the reporting range.",
                   1, 1, 60, 1);
    o.boolean("General", "Include collapsed periods before selected.", "g3",
              "Include in report previous periods as single collapsed column (one for all periods before starting)", true);
    o.boolean("General", "Include collapsed periods after selected.", "g4",
              "Include in report further periods as single collapsed column (one for all periods after ending and to the end of budget range)",
              true);

    opts_util::add_account_selection(o, "Accounts", "Account Display Depth", "Always show sub-accounts", "Account", "a", 2,
                                     accounts::filter_accountlist_type(["ASSET", "LIABILITY", "INCOME", "EXPENSE"],
                                                                       book.root().descendants_sorted()),
                                     false);
    o.boolean("Accounts", "Flatten list to depth limit", "c", "Displays accounts which exceed the depth limit at the depth limit.",
              false);

    o.complex_boolean("Display", "Show Budget", "s1", "Display a column for the budget values.", true, ());
    o.boolean("Display", "Show Budget Notes", "s15", "Display a column for the budget notes.", true);
    o.complex_boolean("Display", "Show Actual", "s2", "Display a column for the actual values.", true, ());
    o.boolean("Display", "Link to actual transactions", "s25", "Show the actual transactions for the budget period", false);
    o.boolean("Display", "Show Difference", "s3", "Display the difference as budget - actual.", false);
    o.boolean("Display", "Show Column with Totals", "s4", "Display a column with the row totals.", false);
    o.boolean("Display", "Include accounts with zero total balances and budget values", "s5",
              "Include accounts with zero total (recursive) balances and budget values in this report.", true);

    o.set_default_section("General");
}

/// `budget-renderer` (:630-761).
fn render(params, book, opts, ctx) {
    let reportname = params.reportname;
    let budget = opts.get("General", "Budget");
    let accts = opts.get("Accounts", "Account");
    if opts.get("Accounts", "Always show sub-accounts") {
        accts = accounts::accounts_and_all_descendants(accts);
    }
    // GnuCash leaves the document untitled when it only warns; the warnings here leave out the heading that names
    // the report, so the title does.
    if accts.is_empty() {
        return #{ title: reportname, sections: [#{ blocks: [html_utils::make_no_account_warning(reportname)] }] };
    }
    if budget == () {
        return #{ title: reportname, sections: [#{ blocks: [html_utils::make_generic_budget_warning(reportname)] }] };
    }
    let accumulate = opts.get("General", "Use accumulated amounts");
    let report_name = opts.get("General", "Report name");
    // :727-735
    let title = `${report_name}: ${budget.name} ${if accumulate { "using accumulated amounts" } else { "" }}`;
    #{ title: title, sections: [#{ blocks: [budget_table(book, ctx, opts, budget, accts)] }] }
}

/// The account table of the renderer's else branch (:673-758): the accounts sorted by full name, the table's
/// environment, then the budget columns.
private fn budget_table(book, ctx, opts, budget, accts) {
    let display_depth = opts.get("Accounts", "Account Display Depth");
    let tree_depth = if display_depth == "all" { accounts::accounts_get_children_depth(accts) } else { display_depth };
    let env = #{
        start_date: budget_util::budget_get_start_date(budget),
        end_date: budget_util::budget_get_end_date(budget),
        display_tree_depth: tree_depth,
        depth_limit_behavior: if opts.get("Accounts", "Flatten list to depth limit") { "flatten" } else { "summarize" },
        zero_balance_mode: if opts.get("Display", "Include accounts with zero total balances and budget values") {
            "show-leaf-acct"
        } else {
            "omit-leaf-acct"
        },
        report_budget: budget,
    };
    let sorted = accts;
    sorted.sort(|a, b| if a.fullName < b.fullName { -1 } else if b.fullName < a.fullName { 1 } else { 0 });
    let acct_tbl = acct_table::make_html_acct_table_env_accts(book, ctx, env, sorted);
    let label_rows = acct_table::add_account_balances(ctx, (), acct_tbl, #{});
    add_budget_values(book, ctx, opts, budget, acct_tbl, label_rows, budget_params(opts))
}

/// `paramsBudget` (:694-723).
private fn budget_params(opts) {
    let use_ranges = opts.get("General", "Report for range of budget periods");
    let show_budget = opts.get("Display", "Show Budget");
    #{
        show_actual: opts.get("Display", "Show Actual"),
        show_trep: opts.get("Display", "Link to actual transactions"),
        show_budget: show_budget,
        show_difference: opts.get("Display", "Show Difference"),
        show_note: show_budget && opts.get("Display", "Show Budget Notes"),
        use_envelope: opts.get("General", "Use accumulated amounts"),
        show_totalcol: opts.get("Display", "Show Column with Totals"),
        use_ranges: use_ranges,
        collapse_before: use_ranges && opts.get("General", "Include collapsed periods before selected."),
        collapse_after: use_ranges && opts.get("General", "Include collapsed periods after selected."),
        user_start_period: opts.get("General", "Range start"),
        user_end_period: opts.get("General", "Range end"),
        user_start_period_exact: to_period_val(opts.get("General", "Exact start period")),
        user_end_period_exact: to_period_val(opts.get("General", "Exact end period")),
        report_name: opts.raw("General", "Report name"),
        stylesheet: opts.raw("General", "Stylesheet"),
        options_summary: opts.raw("General", "Add options summary"),
    }
}

/// `to-period-val` (:677-680): the number truncated to an integer.
private fn to_period_val(v) {
    parse_int(v.round_to(1, "truncate").to_string())
}

/// `gnc:html-table-add-budget-values!` (:268-623): the table of the account labels in `label_rows` and the budget
/// columns `calc-periods` chooses, under the two header rows.
private fn add_budget_values(book, ctx, opts, budget, acct_tbl, label_rows, p) {
    let column_list = calc_periods(budget,
                                   calc_user_period(budget, ctx, p.use_ranges, p.user_start_period, p.user_start_period_exact),
                                   calc_user_period(budget, ctx, p.use_ranges, p.user_end_period, p.user_end_period_exact),
                                   p.collapse_before, p.collapse_after, p.show_totalcol);
    let sub_labels = [];
    if p.show_budget {
        sub_labels.push("Bgt");
    }
    if p.show_actual {
        sub_labels.push("Act");
    }
    if p.show_difference {
        sub_labels.push("Diff");
    }
    // A column's title spans its figures, at least one column (:449-450).
    let col_span = if sub_labels.is_empty() { 1 } else { sub_labels.len() };
    // Right-aligned, so a "." sits where a figure would, as GnuCash's number cells put it.
    let columns = [#{ label: "", role: "label" }];
    for i in 0..column_list.len() * col_span {
        columns.push(#{ label: "", role: "amount", align: "right" });
    }
    let rows = budget_headers(ctx, budget, column_list, col_span, sub_labels);
    let footnotes = [];
    for (label_row, i) in label_rows {
        let acct = acct_table::get_row_env(acct_tbl, i).account;
        let line = budget_line(book, ctx, budget, acct, column_list, p, footnotes.len());
        footnotes += line.notes;
        rows.push(#{ role: "account", account: acct, depth: label_row.depth, cells: [label_row.cells[0]] + line.cells });
    }
    let table = #{ kind: "table", columns: columns, rows: rows };
    if !footnotes.is_empty() {
        table.footnotes = footnotes;
    }
    table
}

/// `gnc:html-table-add-budget-headers!` (:446-504): a row of column titles, a period's start date, a collapsed
/// column's first and last periods' start dates (:467-470) or "Total", each spanning its figures; then "Bgt", "Act"
/// and "Diff" over them.
private fn budget_headers(ctx, budget, column_list, col_span, sub_labels) {
    let titles = [()];
    let subs = [()];
    for column in column_list {
        let title = if column == "total" {
            "Total"
        } else if type_of(column) == "array" {
            ctx.print_date_interval(budget.period_start_date(column[0]), budget.period_start_date(column[column.len() - 1]))
        } else {
            ctx.print_date(budget.period_start_date(column))
        };
        titles.push(if col_span > 1 { #{ text: title, span: col_span } } else { #{ text: title } });
        if sub_labels.is_empty() {
            subs.push(());
        } else {
            for label in sub_labels {
                subs.push(#{ text: label });
            }
        }
    }
    [#{ role: "heading", cells: titles }, #{ role: "heading", cells: subs }]
}

/// `gnc:html-table-add-budget-line!` (:336-436): the cells of `acct`'s row, one set per column of `column_list`, and
/// the notes its budget cells mark, numbered on from `notes_before`.
private fn budget_line(book, ctx, budget, acct, column_list, p, notes_before) {
    let comm = acct.commodity;
    let reverse_balance = acct.reverse_balance(ctx);
    let allperiods = [];
    for column in column_list {
        if type_of(column) == "array" {
            allperiods += column;
        } else if type_of(column) == "i64" {
            allperiods.push(column);
        }
    }
    let total_periods = if p.use_envelope && !allperiods.is_empty() { iota(max_of(allperiods) + 1) } else { allperiods };
    let cells = [];
    let notes = [];
    for column in column_list {
        let period_list = if column == "total" {
            total_periods
        } else if type_of(column) == "array" {
            column
        } else if p.use_envelope {
            iota(column + 1)
        } else {
            [column]
        };
        // `(car period-list)`: GnuCash fails here too when the total column has no periods.
        if period_list.is_empty() {
            throw "the total column has no budget periods";
        }
        let note = if column != "total" && period_list.len() == 1 { budget.account_period_note(acct, period_list[0]) } else { () };
        let bgt_val = maybe_negate(reverse_balance, periodlist_budget_value(book, budget, acct, period_list));
        let act_val = maybe_negate(reverse_balance, periodlist_actual_value(budget, acct, period_list));
        let first = budget.period_start_date(period_list[0]);
        // The total column's link ends with its last period, a period column's with its first (:413-414, :434-435).
        let last = budget.period_end_date(if column == "total" { period_list[period_list.len() - 1] } else { period_list[0] });
        cells += disp_cols(ctx, p, comm, acct, first, last, bgt_val, act_val, bgt_val - act_val, note, notes_before + notes.len());
        if p.show_note && note != () && note != "" {
            notes.push(note);
        }
    }
    #{ cells: cells, notes: notes }
}

/// `disp-cols` (:360-394): the budget ("." when zero), actual and difference ("." when budget and actual are both
/// zero) cells that are shown. A note marks the budget cell with the footnote number after `notes_before`.
private fn disp_cols(ctx, p, comm, acct, start_date, end_date, bgt_val, act_val, dif_val, note, notes_before) {
    let cells = [];
    if p.show_budget {
        let cell = if bgt_val.is_zero() { #{ text: "." } } else { #{ monetary: monetary(comm, bgt_val) } };
        if p.show_note && note != () && note != "" {
            cell.footnote = notes_before + 1;
        }
        cells.push(cell);
    }
    if p.show_actual {
        let cell = #{ monetary: monetary(comm, act_val) };
        if p.show_trep {
            cell.link = trep_link(ctx, p, acct, start_date, end_date);
        }
        cells.push(cell);
    }
    if p.show_difference {
        cells.push(if bgt_val.is_zero() && act_val.is_zero() { #{ text: "." } } else { #{ monetary: monetary(comm, dif_val) } });
    }
    // With nothing shown the column still has its title, over an empty cell.
    if cells.is_empty() {
        cells.push(());
    }
    cells
}

/// The Transaction Report on `acct` and its sub-accounts for the period, in the account's currency (:375-386).
/// `gnc:make-report-anchor` copies first the options both reports have, which are the General options every report
/// gets (5.17:gnucash/report/html-utilities.scm:167-185). A currency of none is left out, as setting the option to
/// #f leaves it unchanged.
private fn trep_link(ctx, p, acct, start_date, end_date) {
    let options = #{
        General: #{
            "Report name": p.report_name,
            "Stylesheet": p.stylesheet,
            "Add options summary": p.options_summary,
            "Start Date": #{ absolute: ctx.print_timestamp(start_date) },
            "End Date": #{ absolute: ctx.print_timestamp(end_date) },
        },
        Accounts: #{ "Accounts": accounts::accounts_and_all_descendants([acct]).map(|a| a.guid) },
        Currency: #{ "Common Currency": true },
    };
    let currency = account_get_currency_or_parent(acct);
    if currency != () {
        options.Currency["Report's currency"] = currency.id;
    }
    #{ report: #{ template: trep_uuid(), options: options } }
}

/// `gnc-account-get-currency-or-parent` (5.17:libgnucash/engine/Account.cpp:3403-3412): the account's commodity if
/// it is a currency, else its nearest ancestor's that is; `()` when none is.
private fn account_get_currency_or_parent(acct) {
    let acc = acct;
    while acc != () {
        let comm = acc.commodity;
        if comm != () && comm.isCurrency {
            return comm;
        }
        acc = acc.parent;
    }
    ()
}

/// `maybe-negate` (:341).
private fn maybe_negate(reverse_balance, amt) {
    if reverse_balance { -amt } else { amt }
}

/// `gnc:get-account-periodlist-budget-value` (:303-308): the rolled-up budget values of the periods, summed.
private fn periodlist_budget_value(book, budget, acct, periodlist) {
    let sum = rational(0);
    for period in periodlist {
        sum += budget_util::get_account_period_rolledup_budget_value(book, budget, acct, period);
    }
    sum
}

/// `gnc:get-account-periodlist-actual-value` (:321-325): the actuals of the periods, summed.
private fn periodlist_actual_value(budget, acct, periodlist) {
    let sum = rational(0);
    for period in periodlist {
        sum += budget.account_period_actual_value(acct, period);
    }
    sum
}

/// `find-period-relative-to-current` (:519-534): the period holding now, moved by `offset` and kept within the
/// budget. Before the budget it is 1 and after it the number of periods, neither adjusted; `()` when no period holds
/// now.
private fn find_period_relative_to_current(budget, ctx, offset) {
    let now = ctx.now;
    let total_periods = budget.numPeriods;
    let last_period = total_periods - 1;
    if now < budget.period_start_date(0) {
        return 1;
    }
    if now > budget.period_end_date(last_period) {
        return total_periods;
    }
    for period in 0..total_periods {
        if budget.period_start_date(period) <= now && now <= budget.period_end_date(period) {
            let adjusted = period + offset;
            let kept = if adjusted < last_period { adjusted } else { last_period };
            return if kept > 0 { kept } else { 0 };
        }
    }
    ()
}

/// `calc-user-period` (:537-545): the period a range end names, `()` without ranges.
private fn calc_user_period(budget, ctx, use_ranges, period_type, period_exact_val) {
    if !use_ranges {
        return ();
    }
    switch period_type {
        "first" => 0,
        "last" => budget.numPeriods - 1,
        "manual" => period_exact_val - 1,
        "previous" => find_period_relative_to_current(budget, ctx, -1),
        "current" => find_period_relative_to_current(budget, ctx, 0),
        "next" => find_period_relative_to_current(budget, ctx, 1),
        _ => (),
    }
}

/// `calc-periods` (:560-578): the columns, each a period, a list of periods collapsed into one column, or "total".
/// The periods before the range and those after it collapse when asked; a list of one period is that period.
private fn calc_periods(budget, user_start, user_end, collapse_before, collapse_after, show_total) {
    let num_periods = budget.numPeriods;
    let range_start = if user_start == () { 0 } else { user_start };
    let range_end = if user_end == () { num_periods } else { user_end + 1 };
    let fold_before_end = if collapse_before { range_start } else { 0 };
    let fold_after_start = if collapse_after { range_end } else { num_periods };
    let parts = [period_range(0, fold_before_end)];
    for period in period_range(range_start, range_end) {
        parts.push(period);
    }
    parts.push(period_range(fold_after_start, num_periods));
    if show_total {
        parts.push("total");
    }
    let columns = [];
    for x in parts {
        if type_of(x) != "array" {
            columns.push(x);
        } else if x.len() == 1 {
            columns.push(x[0]);
        } else if !x.is_empty() {
            columns.push(x);
        }
    }
    columns
}

/// `range` of `calc-periods` (:562-565): the periods from the smaller end, inclusive, to the larger, exclusive.
private fn period_range(start, end) {
    if start < end { iota_from(end - start, start) } else { iota_from(start - end, end) }
}

/// Scheme's `(iota n)`: 0 to n - 1.
private fn iota(n) {
    iota_from(n, 0)
}

/// Scheme's `(iota n start)`.
private fn iota_from(n, start) {
    let out = [];
    for i in 0..n {
        out.push(start + i);
    }
    out
}

/// `(apply max periods)`.
private fn max_of(periods) {
    let m = periods[0];
    for x in periods {
        if x > m {
            m = x;
        }
    }
    m
}
