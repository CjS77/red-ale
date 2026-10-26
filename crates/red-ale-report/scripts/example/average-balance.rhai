// average-balance.scm
// Report history of account balance and other info
//
// Author makes no implicit or explicit guarantee of accuracy of
//  these calculations and accepts no responsibility for direct
//  or indirect losses incurred as a result of using this software.
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
// Translated from 5.17:gnucash/report/reports/example/average-balance.scm, cited below by line. GnuCash files it as
// an example but loads it with the standard reports (5.17:gnucash/report/reports/reports.scm:51).
//
// For each step of the report period: the average, maximum and minimum of the selected accounts' daily balances, and
// the gains, losses and profit of their splits, as a table and a bar chart. The progress calls
// (`gnc:report-percent-done`) are left out.

import "lib/accounts" as accounts;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utilities;
import "lib/options" as opts_util;

/// The template (:488-494).
fn templates() {
    [#{ guid: "d5adcc61c62e4b8684dd8907448d7900", name: "Average Balance", menuPath: ["E_xamples"], params: #{} }]
}

/// `options-generator` (:54-127).
fn options(params, o, book) {
    // General tab
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", "a");
    opts_util::add_interval_choice(o, "General", "Step Size", "b", "MonthDelta");
    opts_util::add_currency(o, "General", "Report's currency", "c");
    opts_util::add_price_source(o, "General", "Price Source", "d", "weighted-average");

    // Account tab
    o.boolean("Accounts", "Include Sub-Accounts", "a", "Include sub-accounts of all selected accounts.", true);
    o.boolean("Accounts", "Exclude transactions between selected accounts", "b",
              "Exclude transactions that only involve two accounts, both of which are selected below. "
              + "This only affects the profit and loss columns of the table.",
              false);
    // The asset and liability accounts, in the order `gnc-account-list-from-types` walks the tree
    // (5.17:libgnucash/engine/gnc-optiondb.cpp:780-790).
    let types = ["BANK", "CASH", "CREDIT", "ASSET", "LIABILITY", "PAYABLE", "RECEIVABLE"];
    let default_accounts = book.root().descendants().filter(|a| types.contains(a.type));
    o.account_list("Accounts", "Accounts", "c", "Do transaction report on this account.", default_accounts, #{});

    // Display tab
    o.boolean("Display", "Show table", "a", "Display a table of the selected data.", false);
    o.boolean("Display", "Show plot", "b", "Display a graph of the selected data.", true);
    o.list("Display", "Plot Type", "c", "The type of graph to generate.", ["AvgBalPlot"], [
        ["AvgBalPlot", "Average"],
        ["GainPlot", "Profit"],
        ["GLPlot", "Gain/Loss"],
    ]);
    opts_util::add_plot_size(o, "Display", "Plot Width", "Plot Height", "d", #{ percent: 100 }, #{ percent: 100 });

    o.set_default_section("General");
}

/// The column headings (:133-138); the chart's series take their names from them.
fn columns() {
    ["Period start", "Period end", "Average", "Maximum", "Minimum", "Gain", "Loss", "Profit"]
}

/// The sum of `amounts`, `(apply + amounts)`.
fn sum(amounts) {
    amounts.reduce(|total, a| total + a, rational(0))
}

/// One row of results (:163-172, :179-189): the period's dates, then its average, maximum and minimum balance, its
/// gains, its losses (as a positive figure) and its profit. A period always has at least one daily balance, as
/// GnuCash's `(apply max ...)` needs.
fn interval_row(period_start, period_end, interval_bals, interval_amts) {
    let maximum = interval_bals.reduce(|m, b| max(m, b), interval_bals[0]);
    let minimum = interval_bals.reduce(|m, b| min(m, b), interval_bals[0]);
    [period_start, period_end, sum(interval_bals) / interval_bals.len(), maximum, minimum,
     sum(interval_amts.filter(|a| a > 0)), -sum(interval_amts.filter(|a| a < 0)), sum(interval_amts)]
}

/// `analyze-splits` (:141-254): walk the daily balances and the splits together, closing a row each time a daily
/// date crosses the end of a step. Within a step every split up to the step's end is taken before its daily
/// balances, as GnuCash's `cond` takes them. The first daily date and balance are skipped (`cdr`), so each balance
/// is the one at the start of the next day. GnuCash recurses; this loops over indices.
fn analyze_splits(ctx, splits, balances, daily_dates, interval_dates, internal_included, exchange_fn, report_currency) {
    let day_delta = dates::deltasym_to_delta("DayDelta");
    let results = [];
    let interval_bals = [];
    let interval_amts = [];
    let next_split = 0;
    let next_day = 1;
    let interval_start = interval_dates[0];
    let next_interval = 1;
    loop {
        let interval_end = interval_dates[next_interval];
        if next_day >= daily_dates.len() {
            // Daily dates finished: the last step ends on the report's end date.
            results.push(interval_row(interval_start, interval_end, interval_bals, interval_amts));
            return results;
        }
        if daily_dates[next_day] > interval_end {
            // Crossed a step boundary: the row ends the day before the next step starts.
            results.push(interval_row(interval_start, dates::decdate(ctx, interval_end, day_delta), interval_bals, interval_amts));
            interval_bals = [];
            interval_amts = [];
            interval_start = interval_end;
            next_interval += 1;
        } else if next_split >= splits.len() || splits[next_split].transaction.datePosted > interval_end {
            // No more splits in this step: add the day's balance.
            interval_bals.push(balances[next_day]);
            next_day += 1;
        } else if !internal_included && next_split + 1 < splits.len() && splits[next_split].transaction.splitCount == 2
            && splits[next_split].transaction == splits[next_split + 1].transaction {
            // A transaction of two splits, both in the selected accounts: skip both. GnuCash's comment: the option
            // is labelled as two splits only, so only those are skipped (:215-219).
            next_split += 2;
        } else {
            let split = splits[next_split];
            let foreign = monetary(split.account.commodity, split.amount);
            interval_amts.push(exchange::apply_exchange_time_fn(exchange_fn, foreign, report_currency, interval_end).amount);
            next_split += 1;
        }
    }
}

/// The balances of `accts` at each of `daily_dates`, summed in the report's currency at that date's exchange rates,
/// each commodity's total rounded to the currency's SCU (:351-373).
fn daily_balances(accts, daily_dates, exchange_fn, report_currency) {
    let accounts_balances = accts.map(|acc| balances::account_get_balances_at_dates(acc, daily_dates));
    let out = [];
    for (date, i) in daily_dates {
        let accounts_balance = accounts_balances.map(|account_balances| account_balances[i]);
        let at_date = |foreign, target_curr| exchange::apply_exchange_time_fn(exchange_fn, foreign, target_curr, date);
        out.push(collectors::sum_collector_commodity(collectors::monetaries_add(accounts_balance), report_currency, at_date).amount);
    }
    out
}

/// A plot size option's value as a result's plot size.
fn plot_size(value) {
    let size = #{};
    size[value.unit] = value.value;
    size
}

/// True when every value is zero, `(every zero? values)`.
fn all_zero(values) {
    values.all(|v| v == 0)
}

/// The chart series of column `col` of `data`, named by its heading.
fn series_of(data, col, color) {
    #{ label: columns()[col], color: color, values: data.map(|row| row[col]) }
}

/// The bar chart (:386-451), labelled by the periods' starts: the series the Plot Type option picks, each left out
/// when all its figures are zero, or the empty-data warning when that leaves none.
fn make_chart(ctx, data, opts, report_title, report_currency) {
    let plot_type = opts.get("Display", "Plot Type");
    let series = [];
    if plot_type.contains("AvgBalPlot") {
        series.push(series_of(data, 2, "#0074D9"));
    }
    if plot_type.contains("GainPlot") {
        series.push(series_of(data, 7, "#2ECC40"));
    }
    let series = series.filter(|s| !all_zero(s.values));
    // Gain and Loss go in together unless both are all zeros.
    if plot_type.contains("GLPlot") {
        let gain_loss = [series_of(data, 5, "#111111"), series_of(data, 6, "#FF4136")];
        if !gain_loss.all(|s| all_zero(s.values)) {
            series += gain_loss;
        }
    }
    if series.is_empty() {
        return html_utilities::make_empty_data_warning(report_title);
    }
    #{
        kind: "chart",
        type: "bar",
        width: plot_size(opts.get("Display", "Plot Width")),
        height: plot_size(opts.get("Display", "Plot Height")),
        yAxis: #{ label: "", format: "currency", commodity: report_currency },
        // The period starts, as `qof-print-date` prints them.
        labels: data.map(|row| ctx.print_date(row[0])),
        series: series,
    }
}

/// The table (:455-478): dates, then each figure in the report's currency. GnuCash rounds each figure to the
/// currency's SCU with `GNC-RND-ROUND` (:475); the cell keeps the exact figure and the host shows it rounded the same
/// way, to the SCU with banker's rounding.
fn make_table(data, report_currency) {
    let cols = columns();
    let columns = [#{ label: cols[0], role: "date" }, #{ label: cols[1], role: "date" }];
    columns += cols.extract(2).map(|label| #{ label: label, role: "amount" });
    let rows = data.map(|row| #{
        cells: [#{ date: row[0] }, #{ date: row[1] }] + row.extract(2).map(|amt| #{ monetary: monetary(report_currency, amt) }),
    });
    #{ kind: "table", columns: columns, rows: rows }
}

/// `renderer` (:260-486).
fn render(params, book, opts, ctx) {
    let report_title = opts.get("General", "Report name");
    let begindate = dates::time64_start_day_time(ctx, opts.get("General", "Start Date"));
    let enddate = dates::time64_end_day_time(ctx, opts.get("General", "End Date"));
    let stepsize = dates::deltasym_to_delta(opts.get("General", "Step Size"));
    let report_currency = opts.get("General", "Report's currency");
    let price_source = opts.get("General", "Price Source");
    let internal_included = !opts.get("Accounts", "Exclude transactions between selected accounts");
    let accts = opts.get("Accounts", "Accounts");
    if opts.get("Accounts", "Include Sub-Accounts") {
        accts = accounts::accounts_and_all_descendants(accts);
    }
    // GnuCash leaves the document untitled; red-ale titles it with the report's name, since its warnings leave out
    // the report-name heading GnuCash gives them (design/reports.md §14).
    let blocks = [];

    if accts.is_empty() {
        blocks.push(html_utilities::make_no_account_warning(report_title));
        return #{ title: report_title, sections: [#{ blocks: blocks }] };
    }

    let commodity_list = accounts::accounts_get_commodities(accts, report_currency);
    let exchange_fn = exchange::case_exchange_time_fn(book, ctx, price_source, report_currency, commodity_list, enddate);

    // The split query (:300-345): the accounts' non-void splits posted from the start of the first day to the end
    // of the last, in split order, whose same-day order follows the book's num field source
    // (5.17:libgnucash/engine/Split.cpp:1548-1557).
    let splits = book.split_query(#{ accounts: accts, from: begindate, to: enddate });
    let daily_dates = dates::make_date_list(ctx, begindate, enddate, dates::deltasym_to_delta("DayDelta"));
    let interval_dates = dates::make_date_list(ctx, begindate, enddate, stepsize);
    let balances = daily_balances(accts, daily_dates, exchange_fn, report_currency);

    let data = if splits.is_empty() {
        []
    } else {
        analyze_splits(ctx, splits, balances, daily_dates, interval_dates, internal_included, exchange_fn, report_currency)
    };

    // If both plot and table, plot comes first.
    if opts.get("Display", "Show plot") {
        blocks.push(make_chart(ctx, data, opts, report_title, report_currency));
    }
    if opts.get("Display", "Show table") {
        blocks.push(make_table(data, report_currency));
    }
    #{ title: report_title, sections: [#{ blocks: blocks }] }
}
