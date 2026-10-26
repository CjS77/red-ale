// standard/net-charts: a time series line or bar chart for either net worth or net profit.
//
// Translated from 5.17:gnucash/report/reports/standard/net-charts.scm, cited below by line. Its header:
//
// net-charts.scm : Display a time series line or bar chart for
// either net worth or net profit.
//
// By Robert Merkel <rgmerk@mira.net>
// and Christian Stimming <stimming@tu-harburg.de>
// and Mike Evans <mikee@saxicooa.co.uk>
// and Christopher Lam to combine with net-barchart.scm
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
// Four templates share one options generator and one renderer, told apart by `params`: `inc_exp` (Income and
// Expense rather than Assets and Liabilities) and `linechart` (:473-519). GnuCash's CSV export thunks are left out;
// CSV export comes with the rest of export in v0.3 (`design/reports.md` §12).

import "lib/accounts" as accounts;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utilities;
import "lib/options" as option_utils;

/// The four templates (:468-519). The Income/Expense Chart is "Income & Expense Barchart" in the menu (:488).
fn templates() {
    [
        #{ guid: "cbba1696c8c24744848062c7f1cf4a72", name: "Net Worth Barchart", menuPath: ["_Assets & Liabilities"],
           params: #{ inc_exp: false, linechart: false } },
        #{ guid: "80769921e87943adade887b9835a7685", name: "Income/Expense Chart", menuName: "Income & Expense Barchart",
           menuPath: ["_Income & Expense"], params: #{ inc_exp: true, linechart: false } },
        #{ guid: "d8b63264186b11e19038001558291366", name: "Net Worth Linechart", menuPath: ["_Assets & Liabilities"],
           params: #{ inc_exp: false, linechart: true } },
        #{ guid: "e533c998186b11e1b2e2001558291366", name: "Income & Expense Linechart",
           menuName: "Income & Expense Linechart", menuPath: ["_Income & Expense"], params: #{ inc_exp: true, linechart: true } },
    ]
}

/// `options-generator` (:65-158).
fn options(params, o, book) {
    let inc_exp = params.inc_exp;
    option_utils::add_date_interval(o, "General", "Start Date", "End Date", "a");
    option_utils::add_interval_choice(o, "General", "Step Size", "b", "MonthDelta");
    option_utils::add_currency(o, "General", "Report's currency", "c");
    option_utils::add_price_source(o, "General", "Price Source", "d", "weighted-average");

    o.account_list("Accounts", "Accounts", "a", "Report on these accounts, if chosen account level allows.",
                   default_accounts(book, inc_exp), #{});

    o.boolean("Display", if inc_exp { "Show Income/Expense" } else { "Show Asset & Liability" }, "a",
              if inc_exp { "Show Income and Expenses?" } else { "Show the Asset and the Liability bars?" }, true);
    o.boolean("Display", if inc_exp { "Show Net Profit" } else { "Show Net Worth" }, "b",
              if inc_exp { "Show the net profit?" } else { "Show a Net Worth bar?" }, true);
    o.boolean("Display", "Show table", "c", "Display a table of the selected data.", false);
    option_utils::add_plot_size(o, "Display", "Plot Width", "Plot Height", "d", #{ percent: 100 }, #{ percent: 100 });
    if params.linechart {
        o.number_range("Display", "Line Width", "e", "Set line width in pixels.", "1.5", "0.5", 5, "0.1");
        o.boolean("Display", "Grid", "f", "Add grid lines.", true);
    }
    o.boolean("Display", "Show a grouped (indexed) tooltip", "t1",
              "Show all items, along the x-axis, in a grouped tooltip. Otherwise - only a single entry, as per coursor's nearest proximity.",
              true);
    o.boolean("Display", "Show only non-zero values in a tooltip", "t2", "Show only non-zero values in a tooltip.", true);
    o.set_default_section("General");
}

/// The Accounts default (:89-96): the income and expense accounts, or every account that is neither, in display
/// order.
fn default_accounts(book, inc_exp) {
    let all = book.root().descendants_sorted();
    if inc_exp {
        accounts::filter_accountlist_type(["INCOME", "EXPENSE"], all)
    } else {
        all.filter(|a| !accounts::account_is_inc_exp(a))
    }
}

/// `net-renderer` (:166-460).
fn render(params, book, opts, ctx) {
    let inc_exp = params.inc_exp;
    let to_date = dates::time64_end_day_time(ctx, opts.get("General", "End Date"));
    let from_date = dates::time64_start_day_time(ctx, opts.get("General", "Start Date"));
    let interval = opts.get("General", "Step Size");
    let report_currency = opts.get("General", "Report's currency");
    let price_source = opts.get("General", "Price Source");
    let accts = opts.get("Accounts", "Accounts");
    let report_title = opts.get("General", "Report name");
    let first_date = if inc_exp { dates::time64_start_day_time(ctx, from_date) } else { dates::time64_end_day_time(ctx, from_date) };
    let dates_list = dates::make_date_list(ctx, first_date, dates::time64_end_day_time(ctx, to_date),
                                           dates::deltasym_to_delta(interval));

    // :285-292
    let commodity_list = accounts::accounts_get_commodities(accounts::accounts_and_all_descendants(accts), report_currency);
    let exchange_fn = exchange::case_exchange_time_fn(book, ctx, price_source, report_currency, commodity_list, to_date);

    // GnuCash sets no document title: the chart carries the report's name.
    let blocks = if accts.is_empty() {
        [html_utilities::make_no_account_warning(report_title)]
    } else {
        net_blocks(params, opts, ctx, accts, dates_list, exchange_fn, report_currency, from_date, to_date)
    };
    #{ title: "", sections: [#{ blocks: blocks }] }
}

/// The chart and table once accounts are selected (:297-448), or the empty-data warning.
fn net_blocks(params, opts, ctx, accts, dates_list, exchange_fn, report_currency, from_date, to_date) {
    let inc_exp = params.inc_exp;
    let report_title = opts.get("General", "Report name");
    let classified = accounts::decompose_accountlist(accts);
    let account_balancelist = accts.map(|a| account_to_balancelist(a, dates_list));
    let left = if inc_exp { classified.INCOME } else { classified.ASSET };
    let right = if inc_exp { classified.EXPENSE } else { classified.LIABILITY };
    let minuend = process_datelist(account_balancelist, dates_list, left, inc_exp, exchange_fn, report_currency);
    let subtrahend = process_datelist(account_balancelist, dates_list, right, inc_exp, exchange_fn, report_currency);
    let difference = [];
    for i in 0..minuend.len() {
        difference.push(minuend[i] + subtrahend[i]);
    }
    // :308-312: income and expense are changes between dates, so the last date starts no interval.
    let shown_dates = if inc_exp { dates_list.extract(0, dates_list.len() - 1) } else { dates_list };
    let date_strings = shown_dates.map(|d| ctx.print_date(d));

    // :387-389
    if !collectors::not_all_zeros(minuend + subtrahend + difference) {
        return [html_utilities::make_empty_data_warning(report_title)];
    }
    let chart = make_chart(params, opts, ctx, report_currency, from_date, to_date, date_strings, minuend, subtrahend,
                           difference);
    let blocks = [chart];
    if opts.get("Display", "Show table") {
        blocks.push(make_table(params, opts, report_currency, date_strings, minuend, subtrahend, difference));
    }
    blocks
}

/// `account->balancelist` (:227-235): `[account, balances]`, the account's balance excluding closing transactions
/// (`xaccSplitGetNoclosingBalance`) at each date, 0 before its first split.
fn account_to_balancelist(account, dates_list) {
    let comm = account.commodity;
    let keys = #{ split_to_elt: "noclosingBalance", nosplit_elt: rational(0) };
    [account, balances::account_accumulate_at_dates(account, dates_list, keys).map(|b| monetary(comm, b))]
}

/// `process-datelist` (:240-282): for each date (each interval between dates for income and expense), the
/// selected accounts of `accountlist` summed in the report currency.
fn process_datelist(account_balances, dates_list, accountlist, inc_exp, exchange_fn, report_currency) {
    let filtered = account_balances.filter(|a| accountlist.contains(a[0]));
    let per_date = list_of_balances(filtered, dates_list.len());
    let result = [];
    if inc_exp {
        for i in 0..dates_list.len() - 1 {
            let change = collectors::collector_minus([per_date[i], per_date[i + 1]]);
            result.push(collector_to_report_currency_amount(change, dates_list[i + 1], exchange_fn, report_currency));
        }
    } else {
        for i in 0..dates_list.len() {
            result.push(collector_to_report_currency_amount(per_date[i], dates_list[i], exchange_fn, report_currency));
        }
    }
    result
}

/// `acc-balances->list-of-balances` (:257-265): a collector per date of every account's balance, or empty
/// collectors when there are no accounts.
fn list_of_balances(filtered, n) {
    let out = [];
    for i in 0..n {
        out.push(collectors::monetaries_add(filtered.map(|entry| entry[1][i])));
    }
    out
}

/// `collector->report-currency-amount` (:216-223): each commodity of `c` exchanged into the report currency at
/// `date`, rounded to its fraction with banker's rounding (`GNC-RND-ROUND`), and added.
fn collector_to_report_currency_amount(c, date, exchange_fn, report_currency) {
    let fraction = report_currency.fraction;
    let sum = rational(0);
    for pair in c {
        let exchanged = exchange::apply_exchange_time_fn(exchange_fn, monetary(pair[0], pair[1]), report_currency, date);
        sum += exchanged.amount.round_to(fraction, "round");
    }
    sum
}

/// The chart (:316-384).
fn make_chart(params, opts, ctx, report_currency, from_date, to_date, date_strings, minuend, subtrahend, difference) {
    let inc_exp = params.inc_exp;
    let linechart = params.linechart;
    let line_width = if linechart { opts.get("Display", "Line Width") } else { () };
    let y_grid = !linechart || opts.get("Display", "Grid");
    let series = [];
    if opts.get("Display", if inc_exp { "Show Income/Expense" } else { "Show Asset & Liability" }) {
        let left = if inc_exp { ["Income", "Income Chart", "44f81bee049b4b3ea908f8dac9a9474e", ["INCOME"]] }
                   else { ["Assets", "Asset Chart", "e9cf815f79db44bcb637d0295093ae3d", asset_types()] };
        series.push(make_series(left[0], minuend, "#0074D9", linechart, line_width,
                                category_link(opts, left[2], left[1], left[3])));
        let right = if inc_exp { ["Expense", "Expense Chart", "b1f15b2052c149df93e698fe85a81ea6", ["EXPENSE"]] }
                    else { ["Liabilities", "Liability Chart", "faf410e8f8da481fbc09e4763da40bcc", liability_types()] };
        series.push(make_series(right[0], subtrahend.map(|v| -v), "#FF4136", linechart, line_width,
                                category_link(opts, right[2], right[1], right[3])));
    }
    if opts.get("Display", if inc_exp { "Show Net Profit" } else { "Show Net Worth" }) {
        series.push(make_series(if inc_exp { "Net Profit" } else { "Net Worth" }, difference, "#2ECC40", linechart,
                                line_width, ()));
    }
    #{
        kind: "chart",
        type: if linechart { "line" } else { "bar" },
        title: [opts.get("General", "Report name"), ctx.print_date_interval(from_date, to_date)],
        width: plot_size(opts.get("Display", "Plot Width")),
        height: plot_size(opts.get("Display", "Plot Height")),
        grid: y_grid,
        yAxis: #{ label: report_currency.mnemonic, format: "currency", commodity: report_currency.id },
        labels: date_strings,
        series: series,
        tooltip: #{ grouped: opts.get("Display", "Show a grouped (indexed) tooltip"),
                    nonZeroOnly: opts.get("Display", "Show only non-zero values in a tooltip") },
    }
}

/// One series (`gnc:html-chart-add-data-series!`, :337-384): filled bars, or lines of the chosen width.
fn make_series(label, values, color, linechart, line_width, link) {
    let s = #{ label: label, color: color, values: values, fill: !linechart };
    if line_width != () {
        s.lineWidth = line_width;
    }
    if link != () {
        s.link = link;
    }
    s
}

/// A plot size option's value as the chart takes it.
fn plot_size(size) {
    let m = #{};
    m[size.unit] = size.value;
    m
}

/// The account types of the Assets and Liabilities Over Time charts (5.17:gnucash/report/reports/standard/category-barchart.scm:735-743).
fn asset_types() {
    ["ASSET", "BANK", "CASH", "CHECKING", "SAVINGS", "MONEYMRKT", "RECEIVABLE", "STOCK", "MUTUAL", "CURRENCY"]
}

fn liability_types() {
    ["LIABILITY", "PAYABLE", "CREDIT", "CREDITLINE"]
}

/// `gnc:make-report-anchor` to the matching "Over Time" chart (:344-375): this report's option values copied to the
/// options that chart also has, as `gnc:options-copy-values` copies them (5.17:bindings/guile/options.scm:112-123,
/// 5.17:gnucash/report/html-utilities.scm:174-185), then stacked charts and the chart's menu name as its report
/// name. GnuCash's account-list option keeps its value when given an account of a type it does not allow
/// (5.17:libgnucash/engine/gnc-option-impl.cpp:385-407), so the accounts are copied only when all are allowed.
fn category_link(opts, template, report_name, allowed_types) {
    let common = [
        ["General", "Start Date"], ["General", "End Date"], ["General", "Step Size"], ["General", "Report's currency"],
        ["General", "Price Source"], ["General", "Stylesheet"], ["General", "Add options summary"],
        ["Display", "Show table"], ["Display", "Plot Width"], ["Display", "Plot Height"],
        ["Display", "Show a grouped (indexed) tooltip"], ["Display", "Show only non-zero values in a tooltip"],
    ];
    let options = #{ General: #{}, Accounts: #{}, Display: #{} };
    for entry in common {
        options[entry[0]][entry[1]] = opts.raw(entry[0], entry[1]);
    }
    let accts = opts.get("Accounts", "Accounts");
    if accts.all(|a| allowed_types.contains(a.type)) {
        options.Accounts["Accounts"] = accts.map(|a| a.guid);
    }
    options.Display["Use Stacked Charts"] = true;
    options.General["Report name"] = report_name;
    #{ report: #{ template: template, options: options } }
}

/// The table of "Show table" (:392-430): the date, then the two series and the net as the Display options choose.
/// The second series is shown as computed, so a liability or an expense shows with its sign (:423).
fn make_table(params, opts, report_currency, date_strings, minuend, subtrahend, difference) {
    let inc_exp = params.inc_exp;
    let show_sep = opts.get("Display", if inc_exp { "Show Income/Expense" } else { "Show Asset & Liability" });
    let show_net = opts.get("Display", if inc_exp { "Show Net Profit" } else { "Show Net Worth" });
    let columns = [#{ label: "Date", role: "date" }];
    if show_sep {
        columns.push(#{ label: if inc_exp { "Income" } else { "Assets" }, role: "amount" });
        columns.push(#{ label: if inc_exp { "Expense" } else { "Liabilities" }, role: "amount" });
    }
    if show_net {
        columns.push(#{ label: if inc_exp { "Net Profit" } else { "Net Worth" }, role: "amount" });
    }
    let rows = [];
    for i in 0..date_strings.len() {
        let cells = [#{ text: date_strings[i] }];
        if show_sep {
            cells.push(#{ monetary: monetary(report_currency, minuend[i]) });
            cells.push(#{ monetary: monetary(report_currency, subtrahend[i]) });
        }
        if show_net {
            cells.push(#{ monetary: monetary(report_currency, difference[i]) });
        }
        rows.push(#{ cells: cells });
    }
    #{ kind: "table", columns: columns, rows: rows }
}
