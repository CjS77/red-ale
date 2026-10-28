// balance-forecast.scm
// Simulate future balance based on scheduled transactions.
//
// By Ryan Turner 2019-02-27 <zdbiohazard2@gmail.com>
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
// Translated from 5.17:gnucash/report/reports/standard/balance-forecast.scm, cited below by line.
//
// Each point is the selected accounts' real balance at the end of an interval plus what the scheduled transactions
// add from the accounts' earliest split to that interval's end, through `book.sx_cashflow`. GnuCash counts both ends
// of each cash-flow range (5.17:bindings/guile/expressions.i:27-32), so an occurrence on the start date is counted by
// the opening sum and again by the first interval; red-ale keeps that.
//
// The two amounts are number ranges, doubles in GnuCash; here they are exact, so the Target line is the reserve plus
// the target exactly. The complex-boolean callbacks only make "Reserve amount" and "Target amount above reserve"
// selectable in GnuCash's dialog (:104-120), so none is passed. GnuCash's colours are CSS's three-digit forms, written
// out in six digits.

import "lib/accounts" as accounts;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utilities;
import "lib/options" as option_utils;

/// The template (:300-306); "_Assets & Liabilities" is `gnc:menuname-asset-liability`
/// (5.17:gnucash/report/report-core.scm:157).
fn templates() {
    [#{ guid: "321d940d487d4ccbb4bd0467ffbadbf2", name: "Balance Forecast", menuPath: ["_Assets & Liabilities"] }]
}

/// `options-generator` (:73-126).
fn options(params, o, book) {
    o.account_list("Accounts", "Accounts", "a", "Report on these accounts.",
                   accounts::filter_accountlist_type(["BANK", "CASH"], book.root().descendants_sorted()), #{});
    option_utils::add_date_interval(o, "General", "Start Date", "End Date", "a");
    option_utils::add_interval_choice(o, "General", "Interval", "b", "DayDelta");
    option_utils::add_currency(o, "General", "Report's currency", "c");
    option_utils::add_price_source(o, "General", "Price Source", "d", "pricedb-nearest");

    option_utils::add_plot_size(o, "Display", "Plot Width", "Plot Height", "a", #{ percent: 100 }, #{ percent: 100 });
    o.boolean("Display", "Data markers?", "b", "Display a mark for each data point.", false);
    o.complex_boolean("Display", "Show reserve line", "c", "Show reserve line", false, ());
    o.number_range("Display", "Reserve amount", "d", "The reserve amount is set to a minimum balance desired",
                   0, -10_000_000_000, 10_000_000_000, "0.01");
    o.complex_boolean("Display", "Show target line", "e", "Show target line", false, ());
    o.number_range("Display", "Target amount above reserve", "f",
                   "The target is used to plan for a future large purchase, which will be added as a line above the reserve amount.",
                   0, -10_000_000_000, 10_000_000_000, "0.01");
    o.boolean("Display", "Show future minimum", "g",
              "The future minimum will add, for each date point, a projected minimum balance including scheduled transactions.",
              false);
    o.set_default_section("General");
}

/// `document-renderer` (:129-298). GnuCash sets no document title: the chart carries the report's name.
fn render(params, book, opts, ctx) {
    let report_title = opts.get("General", "Report name");
    let accts = opts.get("Accounts", "Accounts");
    let from_date = dates::time64_start_day_time(ctx, opts.get("General", "Start Date"));
    let to_date = dates::time64_end_day_time(ctx, opts.get("General", "End Date"));
    let blocks = if accts.is_empty() {
        // :178-182
        [html_utilities::make_no_account_warning(report_title)]
    } else {
        forecast_blocks(book, opts, ctx, accts, from_date, to_date)
    };
    #{ title: "", sections: [#{ blocks: blocks }] }
}

/// The chart (:190-297), or the empty-data warning when every real balance is zero, whatever the scheduled
/// transactions add (:184-188).
private fn forecast_blocks(book, opts, ctx, accts, from_date, to_date) {
    let currency = opts.get("General", "Report's currency");
    let intervals = dates::make_date_interval_list(ctx, from_date, to_date,
                                                   dates::deltasym_to_delta(opts.get("General", "Interval")));
    // :166-169
    let exchange_fn = exchange::case_exchange_time_fn(book, ctx, opts.get("General", "Price Source"), currency,
                                                      accounts::accounts_get_commodities(accts, ()), to_date);
    // :171-175: each account's balance at each interval's end.
    let ends = intervals.map(|interval| interval[1]);
    let balancelists = [];
    for acc in accts {
        balancelists.push(balances::account_get_balances_at_dates(acc, ends));
    }
    if balancelists.all(|list| list.all(|m| m.amount.is_zero())) {
        return [html_utilities::make_empty_data_warning(opts.get("General", "Report name"))];
    }
    let figures = forecast_figures(book, accts, balancelists, intervals, from_date, currency, exchange_fn);
    [make_chart(opts, ctx, currency, from_date, to_date, intervals, figures)]
}

/// The points (:211-232): for each interval, add its cash flow to the accumulator, which carries on to the next
/// interval, then take the real balances at the interval's end plus the accumulator.
private fn forecast_figures(book, accts, balancelists, intervals, from_date, currency, exchange_fn) {
    let accum = opening_accumulator(book, accts, from_date);
    let figures = [];
    for (interval, i) in intervals {
        let sx_value = book.sx_cashflow(interval[0], interval[1]);
        let balance = collectors::make_commodity_collector();
        for (acc, j) in accts {
            accum.add(acc.commodity, hash_ref(sx_value, acc.guid));
            let account_balance = balancelists[j][i];
            balance.add(account_balance.commodity, account_balance.amount);
        }
        balance.merge(accum);
        figures.push(sum_in_currency(balance, currency, exchange_fn, interval[1]));
    }
    figures
}

/// `(hash-ref hash guid 0)`.
private fn hash_ref(hash, guid) {
    if guid in hash { hash[guid] } else { 0 }
}

/// The scheduled-transaction accumulator before the first interval (:191-208): the cash flow from the earliest
/// posted date of any selected account's first split (`gnc-account-get-earliest-date`,
/// 5.17:libgnucash/engine/Account.cpp:5114-5120; accounts without splits are skipped) to the start date. With no
/// split in any account, nothing.
private fn opening_accumulator(book, accts, from_date) {
    let earliest = ();
    for acc in accts {
        let splits = acc.splits();
        if !splits.is_empty() {
            let date = splits[0].transaction.datePosted;
            earliest = if earliest == () || date < earliest { date } else { earliest };
        }
    }
    let sx_hash = if earliest != () { book.sx_cashflow(earliest, from_date) } else { #{} };
    let accum = collectors::make_commodity_collector();
    for acc in accts {
        accum.add(acc.commodity, hash_ref(sx_hash, acc.guid));
    }
    accum
}

/// A point's collector in the report currency (:227-231): each commodity exchanged at the interval's end and rounded
/// to the currency's fraction, then added (`gnc:sum-collector-commodity`,
/// 5.17:gnucash/report/commodity-utilities.scm:900-910).
private fn sum_in_currency(balance, currency, exchange_fn, end_date) {
    let fraction = currency.fraction;
    let sum = rational(0);
    for pair in balance {
        let exchanged = exchange::apply_exchange_time_fn(exchange_fn, monetary(pair[0], pair[1]), currency, end_date);
        sum += exchanged.amount.round_to(fraction, "round");
    }
    sum
}

/// The future minimum (:239-241): each point's minimum of itself and every later point.
private fn future_minimum(figures) {
    let out = [];
    for i in 0..figures.len() {
        let least = figures[i];
        for j in i..figures.len() {
            if figures[j] < least {
                least = figures[j];
            }
        }
        out.push(least);
    }
    out
}

/// A line of the chart (:236-271): unfilled, 1.5 wide, with points of radius 3 when "Data markers?" is on and 0
/// otherwise (:153-154).
private fn series(label, values, color, markers) {
    #{ label: label, color: color, values: values, fill: false, lineWidth: rational(3, 2), pointRadius: markers }
}

/// The chart (:234-293): the Minimum line under the Balance line, then the Target and Reserve lines, each as asked.
private fn make_chart(opts, ctx, currency, from_date, to_date, intervals, figures) {
    let markers = if opts.get("Display", "Data markers?") { rational(3) } else { rational(0) };
    let reserve = opts.get("Display", "Reserve amount");
    let lines = [];
    if opts.get("Display", "Show future minimum") {
        lines.push(series("Minimum", future_minimum(figures), "#00aaaa", markers));
    }
    lines.push(series("Balance", figures, "#00aa00", markers));
    if opts.get("Display", "Show target line") {
        let target = reserve + opts.get("Display", "Target amount above reserve");
        lines.push(series("Target", intervals.map(|interval| target), "#ffff00", markers));
    }
    if opts.get("Display", "Show reserve line") {
        lines.push(series("Reserve", intervals.map(|interval| reserve), "#ff0000", markers));
    }
    #{
        kind: "chart",
        type: "line",
        title: [opts.get("General", "Report name"), ctx.print_date_interval(from_date, to_date)],
        width: plot_size(opts.get("Display", "Plot Width")),
        height: plot_size(opts.get("Display", "Plot Height")),
        yAxis: #{ label: currency.mnemonic, format: "currency", commodity: currency.id },
        // :284-287: each interval's end in ISO form.
        labels: intervals.map(|interval| ctx.print_date_iso(interval[1])),
        series: lines,
    }
}

/// A plot size option's value as the chart takes it.
private fn plot_size(size) {
    let m = #{};
    m[size.unit] = size.value;
    m
}
