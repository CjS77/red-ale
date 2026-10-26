// standard/category-barchart: a bar or line chart of accounts' balances, or of their changes, over time.
//
// Translated from 5.17:gnucash/report/reports/standard/category-barchart.scm, cited below by line. Its header:
//
// category-barchart.scm: shows barchart of income/expense categories
//
// By Christian Stimming <stimming@tu-harburg.de>
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
// Four templates share one options generator and one renderer, told apart by `params` (:708-744): the account
// types, `do_intervals` (Income and Expense show each interval's change, Assets and Liabilities the balance at each
// date) and `reverse_bal` (Income and Liabilities negate balances). GnuCash's CSV export thunks are left out; CSV
// export comes with the rest of export in v0.3 (`design/reports.md` §12).

import "lib/accounts" as accounts;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utilities;
import "lib/options" as option_utils;

/// The four templates (:730-744), with the GUIDs of 5.17:gnucash/report/report.scm:63-66.
fn templates() {
    let assets = ["ASSET", "BANK", "CASH", "CHECKING", "SAVINGS", "MONEYMRKT", "RECEIVABLE", "STOCK", "MUTUAL", "CURRENCY"];
    let liabilities = ["LIABILITY", "PAYABLE", "CREDIT", "CREDITLINE"];
    [
        template("44f81bee049b4b3ea908f8dac9a9474e", "Income Over Time", ["INCOME"], true, "Income Chart",
                 "Shows a chart with the Income per interval developing over time", true),
        template("b1f15b2052c149df93e698fe85a81ea6", "Expense Over Time", ["EXPENSE"], true, "Expense Chart",
                 "Shows a chart with the Expenses per interval developing over time", false),
        template("e9cf815f79db44bcb637d0295093ae3d", "Assets Over Time", assets, false, "Asset Chart",
                 "Shows a chart with the Assets developing over time", false),
        template("faf410e8f8da481fbc09e4763da40bcc", "Liabilities Over Time", liabilities, false, "Liability Chart",
                 "Shows a chart with the Liabilities developing over time", true),
    ]
}

/// One template record (:709-729): income and expense charts go in the Income & Expense menu.
fn template(guid, name, account_types, inc_exp, menu_name, menu_tip, reverse_bal) {
    #{
        guid: guid,
        name: name,
        menuPath: [if inc_exp { "_Income & Expense" } else { "_Assets & Liabilities" }],
        menuName: menu_name,
        menuTip: menu_tip,
        params: #{ guid: guid, account_types: account_types, do_intervals: inc_exp, reverse_bal: reverse_bal },
    }
}

/// `options-generator` (:98-199).
fn options(params, o, book) {
    option_utils::add_date_interval(o, "General", "Start Date", "End Date", "a");
    option_utils::add_interval_choice(o, "General", "Step Size", "b", "MonthDelta");
    option_utils::add_currency(o, "General", "Report's currency", "c");
    option_utils::add_price_source(o, "General", "Price Source", "d", "weighted-average");
    if params.do_intervals {
        o.multichoice("General", "Show Average", "e",
                      "Select whether the amounts should be shown over the full time period or rather as the average e.g. per month.",
                      "None", [["None", "No Averaging"], ["MonthDelta", "Monthly"], ["WeekDelta", "Weekly"], ["DayDelta", "Daily"]]);
    }

    o.account_list("Accounts", "Accounts", "a", "Report on these accounts, if chosen account level allows.",
                   book.accounts_of_types(params.account_types), #{ types: params.account_types });
    option_utils::add_account_levels(o, "Accounts", "Levels of Subaccounts", "c",
                                     "Maximum number of levels in the account tree displayed.", 2);

    o.boolean("Display", "Show long account names", "a", "Show the full account name in legend?", false);
    o.multichoice("Display", "Chart Type", "b", "Select which chart type to use.", "barchart",
                  [["barchart", "Bar Chart"], ["linechart", "Line Chart"]]);
    o.boolean("Display", "Use Stacked Charts", "c", "Show charts as stacked charts?", true);
    o.number_range("Display", "Maximum Bars", "d", "Maximum number of stacks in the chart.", 8, 2, 24, 1);
    o.boolean("Display", "Show table", "e", "Display a table of the selected data.", false);
    o.boolean("Display", "Percentage chart", "e1",
              "Display account contributions as a percentage of the total value for the period.", false);
    option_utils::add_plot_size(o, "Display", "Plot Width", "Plot Height", "f", #{ percent: 100 }, #{ percent: 100 });
    option_utils::add_sort_method(o, "Display", "Sort Method", "g", "amount");
    o.boolean("Display", "Show a grouped (indexed) tooltip", "t1",
              "Show all items, along the x-axis, in a grouped tooltip. " +
              "Otherwise - only a single entry, as per coursor's nearest proximity.",
              false);
    o.boolean("Display", "Show only non-zero values in a tooltip", "t2", "Show only non-zero values in a tooltip.", true);
    o.set_default_section("General");
}

/// `category-barchart-renderer` (:213-706).
fn render(params, book, opts, ctx) {
    let to_date = dates::time64_end_day_time(ctx, opts.get("General", "End Date"));
    let from_date = dates::time64_start_day_time(ctx, opts.get("General", "Start Date"));
    let report_title = opts.get("General", "Report name");
    let accts = opts.get("Accounts", "Accounts");
    // GnuCash sets no document title: the chart carries the report's name.
    let blocks = if accts.is_empty() {
        [html_utilities::make_no_account_warning(report_title)]
    } else if to_date <= from_date {
        [html_utilities::make_generic_warning(report_title, "Invalid dates", "Start date must be earlier than End date")]
    } else {
        chart_blocks(params, book, opts, ctx, from_date, to_date)
    };
    #{ title: "", sections: [#{ blocks: blocks }] }
}

/// What the helpers below share: the run's options and the values computed from them (:221-339).
fn make_env(params, book, opts, ctx, from_date, to_date) {
    let interval = opts.get("General", "Step Size");
    let report_currency = opts.get("General", "Report's currency");
    let averaging = if params.do_intervals { opts.get("General", "Show Average") } else { "None" };
    let accts = opts.get("Accounts", "Accounts");
    let account_levels = opts.get("Accounts", "Levels of Subaccounts");
    let tree_depth = if account_levels == "all" { accounts::get_current_account_tree_depth(book) } else { account_levels };
    // :292-298
    let commodity_list = accounts::accounts_get_commodities(accounts::accounts_and_all_descendants(accts), report_currency);
    let exchange_fn = exchange::case_exchange_time_fn(book, ctx, opts.get("General", "Price Source"), report_currency,
                                                      commodity_list, to_date);
    let first_date = if params.do_intervals { dates::time64_start_day_time(ctx, from_date) }
                     else { dates::time64_end_day_time(ctx, from_date) };
    let dates_list = dates::make_date_list(ctx, first_date, dates::time64_end_day_time(ctx, to_date),
                                           dates::deltasym_to_delta(interval));
    #{
        do_intervals: params.do_intervals,
        selected: accts,
        tree_depth: tree_depth,
        report_currency: report_currency,
        exchange_fn: exchange_fn,
        multiplier: averaging_multiplier(ctx, averaging, interval, from_date, to_date),
        dates_list: dates_list,
        alist: account_balances_alist(accounts::accounts_and_all_descendants(accts), dates_list, params.reverse_bal),
        fullname: opts.get("Display", "Show long account names"),
    }
}

/// The averaging multiplier (:299-320): the report period's length in steps over its length in averaging periods,
/// when the averaging period is the shorter, else 1. GnuCash computes the lengths in floating point for weeks and
/// days (5.17:bindings/guile/date-utilities.scm:244-259) and rounds the averaging length to 6 decimals; red-ale
/// computes them exactly and rounds the exact value to 6 decimals the same way, half to even as Scheme's `round`
/// (`design/reports.md` §12, "Floating point in GnuCash, exact in red-ale").
fn averaging_multiplier(ctx, averaging, interval, from_date, to_date) {
    let averaging_fraction_func = dates::date_get_fraction_func(ctx, averaging);
    if averaging_fraction_func == () {
        return rational(1);
    }
    let interval_fraction_func = dates::date_get_fraction_func(ctx, interval);
    let diff_avg = dates::apply_fraction_func(averaging_fraction_func, to_date + 1)
                   - dates::apply_fraction_func(averaging_fraction_func, from_date);
    let diff_avg_numeric = (diff_avg * 1000000).round() / 1000000;
    let diff_int = dates::apply_fraction_func(interval_fraction_func, to_date + 1)
                   - dates::apply_fraction_func(interval_fraction_func, from_date);
    if diff_avg > diff_int { diff_int / diff_avg_numeric } else { rational(1) }
}

/// The report title, extended when averaging (:323-328).
fn averaged_title(report_title, averaging) {
    switch averaging {
        "MonthDelta" => report_title + " Monthly Average",
        "WeekDelta" => report_title + " Weekly Average",
        "DayDelta" => report_title + " Daily Average",
        _ => report_title,
    }
}

/// `account-balances-alist` (:375-393): for every selected account and its descendants, `[account, balances]`, the
/// no-closing balance (`xaccSplitGetNoclosingBalance`) at each date, negated for the templates that reverse
/// balances, 0 before the first split.
fn account_balances_alist(accts, dates_list, reverse_bal) {
    let alist = [];
    for acc in accts {
        let comm = acc.commodity;
        let keys = #{ split_to_elt: "noclosingBalance", nosplit_elt: () };
        let balances = balances::account_accumulate_at_dates(acc, dates_list, keys)
            .map(|b| if b == () { monetary(comm, 0) } else if reverse_bal { monetary(comm, -b) } else { monetary(comm, b) });
        alist.push([acc, balances]);
    }
    alist
}

/// `collector->report-currency-amount` (:346-351): `c` in the report currency at `date` (`gnc:sum-collector-commodity`,
/// each commodity rounded to the currency's fraction), times the averaging multiplier.
fn collector_to_report_currency_amount(env, c, date) {
    let currency = env.report_currency;
    let sum = rational(0);
    for pair in c {
        let exchanged = exchange::apply_exchange_time_fn(env.exchange_fn, monetary(pair[0], pair[1]), currency, date);
        sum += exchanged.amount.round_to(currency.fraction, "round");
    }
    env.multiplier * sum
}

/// `account->balance-list` (:397-425): the account's amount at each date, or for each interval the change from
/// one date to the next, with its descendants' when `subacct` is true.
fn account_to_balance_list(env, account, subacct) {
    let accountslist = [account];
    if subacct {
        accountslist += account.descendants();
    }
    let selected = env.alist.filter(|entry| accountslist.contains(entry[0]));
    let dates_list = env.dates_list;
    let collectors_by_date = [];
    for i in 0..dates_list.len() {
        collectors_by_date.push(collectors::monetaries_add(selected.map(|entry| entry[1][i])));
    }
    let result = [];
    if env.do_intervals {
        for i in 0..dates_list.len() - 1 {
            let change = collectors::collector_minus([collectors_by_date[i + 1], collectors_by_date[i]]);
            result.push(collector_to_report_currency_amount(env, change, dates_list[i + 1]));
        }
    } else {
        for i in 0..dates_list.len() {
            result.push(collector_to_report_currency_amount(env, collectors_by_date[i], dates_list[i]));
        }
    }
    result
}

/// `traverse-accounts` (:453-477): `[account, amounts]` for the selected accounts down to the depth limit, an
/// account at the limit with its descendants' amounts folded in. GnuCash conses each account on the front of the
/// list and puts its children's results before it, so the list comes out deepest and latest first; the sort that
/// follows is stable, so that order decides ties.
fn traverse_accounts(env, current_depth, accts) {
    if current_depth < env.tree_depth {
        let res = [];
        for a in accts {
            if env.selected.contains(a) {
                res.insert(0, [a, account_to_balance_list(env, a, false)]);
            }
            res = traverse_accounts(env, current_depth + 1, a.children()) + res;
        }
        res
    } else {
        let res = [];
        for a in accts {
            if env.selected.contains(a) {
                res.push([a, account_to_balance_list(env, a, true)]);
            }
        }
        res
    }
}

/// `acct->name` (:353-354).
fn acct_to_name(env, acct) {
    if env.fullname { acct.fullName } else { acct.name }
}

/// The sum of an array of Rationals.
fn sum_of(amounts) {
    let total = rational(0);
    for a in amounts {
        total += a;
    }
    total
}

/// The accounts with any amount, sorted by the Sort Method (:482-498). Names and codes compare bytewise, where
/// GnuCash uses the locale's collation (`design/reports.md` §12).
fn sorted_data(env, data, sort_method) {
    let keyed = [];
    for entry in data {
        let total = sum_of(entry[1]);
        if total != 0 {
            let key = switch sort_method {
                "alphabetical" => acct_to_name(env, entry[0]),
                "acct-code" => entry[0].code,
                _ => -total,
            };
            keyed.push([key, entry]);
        }
    }
    keyed.sort(|a, b| if a[0] < b[0] { -1 } else if b[0] < a[0] { 1 } else { 0 });
    keyed.map(|k| k[1])
}

/// `get-negative-accounts` (:362-368): the names of the accounts with a negative amount, last first.
fn negative_accounts(env, data) {
    let names = [];
    for entry in data {
        if entry[1].some(|v| v < 0) {
            names.insert(0, acct_to_name(env, entry[0]));
        }
    }
    names
}

/// From the account balances to the chart and table, or a warning (:292-700).
fn chart_blocks(params, book, opts, ctx, from_date, to_date) {
    let env = make_env(params, book, opts, ctx, from_date, to_date);
    let averaging = if params.do_intervals { opts.get("General", "Show Average") } else { "None" };
    let report_title = averaged_title(opts.get("General", "Report name"), averaging);
    let topl_accounts = accounts::filter_accountlist_type(params.account_types, book.root().children_sorted());
    let all_data = sorted_data(env, traverse_accounts(env, 1, topl_accounts), opts.get("Display", "Sort Method"));
    let ratio_chart = opts.get("Display", "Percentage chart");

    // :500-518
    if all_data.is_empty() || !collectors::not_all_zeros(all_data.map(|entry| entry[1])) {
        return [html_utilities::make_empty_data_warning(report_title)];
    }
    let negative = if ratio_chart { negative_accounts(env, all_data) } else { [] };
    if !negative.is_empty() {
        let warning = html_utilities::make_generic_warning(report_title, "Negative amounts detected",
            "Charting ratios cannot occur on accounts with negative balances. Please deselect the accounts with negative balances.");
        return [warning, #{ kind: "text", text: negative.reduce(|out, name| if out == "" { name } else { out + "\n" + name }, "") }];
    }

    let dates_list = env.dates_list;
    let shown_dates = if params.do_intervals { dates_list.extract(0, dates_list.len() - 1) } else { dates_list };
    let date_strings = shown_dates.map(|d| ctx.print_date(d));

    // :554-574: past Maximum Bars, the rest are summed into "Other".
    let max_slices = whole_number(opts.get("Display", "Maximum Bars"));
    let series_data = all_data.map(|entry| [entry[0], entry[1]]);
    if series_data.len() > max_slices {
        let finish = series_data.extract(max_slices - 1);
        let other_sum = [];
        for i in 0..date_strings.len() {
            other_sum.push(sum_of(finish.map(|entry| entry[1][i])));
        }
        series_data = series_data.extract(0, max_slices - 1);
        series_data.push(["Other", other_sum, finish.map(|entry| entry[0])]);
    }

    // :576-577
    let list_of_rows = [];
    for i in 0..date_strings.len() {
        list_of_rows.push(series_data.map(|entry| entry[1][i]));
    }
    let row_totals = list_of_rows.map(|row| sum_of(row));

    let chart = make_chart(params, opts, ctx, env, report_title, from_date, to_date, date_strings, series_data, row_totals);
    let blocks = [chart];
    if opts.get("Display", "Show table") {
        blocks.push(make_table(env, ratio_chart, date_strings, series_data, list_of_rows, row_totals));
    }
    blocks
}

/// "Maximum Bars" as an integer: `inexact->exact` of the option (:247-248). GnuCash's `take` fails on a value that
/// is not whole, and so does this.
fn whole_number(r) {
    if !r.is_integer() {
        throw `Maximum Bars must be a whole number: ${r}`;
    }
    let n = 0;
    while n < r {
        n += 1;
    }
    n
}

/// `safe-/` (:38-39).
fn safe_div(x, y) {
    if y == 0 { rational(0) } else { x / y }
}

/// The chart (:529-626).
fn make_chart(params, opts, ctx, env, report_title, from_date, to_date, date_strings, series_data, row_totals) {
    let barchart = opts.get("Display", "Chart Type") == "barchart";
    let stacked = opts.get("Display", "Use Stacked Charts");
    let ratio_chart = opts.get("Display", "Percentage chart");
    let interval_text = ctx.print_date_interval(from_date, to_date);
    let colors = html_utilities::assign_colors(series_data.len());
    let series = [];
    for (entry, i) in series_data {
        let label = if type_of(entry[0]) == "string" { entry[0] } else { acct_to_name(env, entry[0]) };
        // GnuCash's chart shows a percent axis's values times 100, as Chart.js's percent style does; red-ale's
        // percent axis shows values as they are, so a ratio is given as a percentage (`design/reports.md` §14).
        let values = if ratio_chart {
            let v = [];
            for j in 0..row_totals.len() {
                v.push(safe_div(entry[1][j], row_totals[j]) * 100);
            }
            v
        } else {
            entry[1]
        };
        series.push(#{
            label: label,
            color: colors[i],
            values: values,
            stack: if stacked { "default" } else { `${i}` },
            fill: barchart,
            link: series_link(params, opts, env, entry),
        });
    }
    let y_axis = if ratio_chart {
        #{ label: "Ratio", format: "percent" }
    } else {
        #{ label: env.report_currency.mnemonic, format: "currency", commodity: env.report_currency.id }
    };
    #{
        kind: "chart",
        type: if barchart { "bar" } else { "line" },
        title: [report_title, if params.do_intervals { interval_text } else { "Balances " + interval_text }],
        width: plot_size(opts.get("Display", "Plot Width")),
        height: plot_size(opts.get("Display", "Plot Height")),
        stacked: stacked,
        yAxis: y_axis,
        labels: date_strings,
        series: series,
        tooltip: #{ grouped: opts.get("Display", "Show a grouped (indexed) tooltip"),
                    nonZeroOnly: opts.get("Display", "Show only non-zero values in a tooltip") },
    }
}

/// A plot size option's value as the chart takes it.
fn plot_size(size) {
    let m = #{};
    m[size.unit] = size.value;
    m
}

/// Every option of these templates, to copy into a linked report as `gnc:options-copy-values` copies them
/// (5.17:bindings/guile/options.scm:112-123).
fn own_options(params) {
    let general = ["Start Date", "End Date", "Step Size", "Report's currency", "Price Source", "Report name", "Stylesheet",
                   "Add options summary"];
    if params.do_intervals {
        general.push("Show Average");
    }
    let display = ["Show long account names", "Chart Type", "Use Stacked Charts", "Maximum Bars", "Show table",
                   "Percentage chart", "Plot Width", "Plot Height", "Sort Method", "Show a grouped (indexed) tooltip",
                   "Show only non-zero values in a tooltip"];
    #{ General: general, Accounts: ["Accounts", "Levels of Subaccounts"], Display: display }
}

/// A link to this template with this run's options and `changes` (`gnc:make-report-anchor`,
/// 5.17:gnucash/report/html-utilities.scm:168-185). GnuCash's account-list option keeps the copied value when given
/// an account of a type it does not allow (5.17:libgnucash/engine/gnc-option-impl.cpp:385-407), so a list of
/// `accts` replaces it only when all are allowed.
fn report_link(params, opts, accts, changes) {
    let options = #{};
    let own = own_options(params);
    for section in own.keys() {
        options[section] = #{};
        for name in own[section] {
            options[section][name] = opts.raw(section, name);
        }
    }
    if accts.all(|a| params.account_types.contains(a.type)) {
        options.Accounts["Accounts"] = accts.map(|a| a.guid);
    }
    for section in changes.keys() {
        for name in changes[section].keys() {
            options[section][name] = changes[section][name];
        }
    }
    #{ report: #{ template: params.guid, options: options } }
}

/// A series' link (:589-609): "Other" to this report on the accounts it sums; an account without children, or when
/// the depth is already 6 or more, to the account; any other to this report on the account and its children, one
/// level deeper and named after it.
fn series_link(params, opts, env, entry) {
    let acct = entry[0];
    if type_of(acct) == "string" {
        return report_link(params, opts, entry[2], #{});
    }
    let children = acct.children();
    if children.is_empty() || env.tree_depth >= 6 {
        return #{ account: acct.guid };
    }
    let changes = #{ Accounts: #{ "Levels of Subaccounts": env.tree_depth + 1 }, General: #{ "Report name": acct_to_name(env, acct) } };
    report_link(params, opts, [acct] + children, changes)
}

/// The table of "Show table" (:628-672): the date, each series, and the Grand Total of each row when there is more
/// than one series. A percentage chart shows each series' share of the row (:636-637) and the total as an amount.
fn make_table(env, ratio_chart, date_strings, series_data, list_of_rows, row_totals) {
    let currency = env.report_currency;
    let cols_gt_1 = series_data.len() > 1;
    let columns = [#{ label: "Date", role: "date" }];
    for entry in series_data {
        let label = if type_of(entry[0]) == "string" { entry[0] } else { acct_to_name(env, entry[0]) };
        columns.push(#{ label: label, role: "amount" });
    }
    if cols_gt_1 {
        columns.push(#{ label: "Grand Total", role: "amount" });
    }
    let rows = [];
    for i in 0..date_strings.len() {
        let cells = [#{ text: date_strings[i] }];
        for amt in list_of_rows[i] {
            if ratio_chart {
                cells.push(#{ number: safe_div(amt, row_totals[i]) * 100, suffix: " %" });
            } else {
                cells.push(#{ monetary: monetary(currency, amt) });
            }
        }
        if cols_gt_1 {
            cells.push(#{ monetary: monetary(currency, row_totals[i]) });
        }
        rows.push(#{ cells: cells });
    }
    #{ kind: "table", columns: columns, rows: rows }
}
