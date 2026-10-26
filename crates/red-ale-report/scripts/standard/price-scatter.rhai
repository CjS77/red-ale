// price-scatter.scm: A scatter plot report about some price.
//
// By Christian Stimming <stimming@tuhh.de>
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
// Translated from 5.17:gnucash/report/reports/standard/price-scatter.scm, cited below by line.
//
// The prices of one commodity in the report's currency over the report period, from the transactions of the share
// and currency accounts or from the price database, as a scatter chart whose x axis counts steps since the start
// date. GnuCash's unused `dates-list` (:141-144) and the tooltip padding it takes from a preference (:189-191),
// which only places the hover label, are left out.

import "lib/accounts" as accounts;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/options" as opts_util;

/// The template (:281-288).
fn templates() {
    [#{ guid: "1d241609fd4644caad765c95be20ff4c", name: "Price Scatterplot", menuPath: ["_Assets & Liabilities"],
        menuName: "Price Scatterplot", params: #{} }]
}

/// `options-generator` (:50-96).
fn options(params, o, book) {
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", "a");
    opts_util::add_interval_choice(o, "General", "Step Size", "b", "MonthDelta");

    opts_util::add_currency(o, "Price", "Report's currency", "d");
    // `gnc-locale-default-iso-currency-code`: USD under the C locale red-ale stands in for (design/reports.md §6).
    o.commodity("Price", "Price of Commodity", "e", "Calculate the price of this commodity.", "CURRENCY::USD");
    o.multichoice("Price", "Price Source", "f", "The source of price information.", "actual-transactions", [
        ["weighted-average", "Weighted Average"],
        ["actual-transactions", "Actual Transactions"],
        ["pricedb", "Price Database"],
    ]);
    o.boolean("Price", "Invert prices", "g", "Plot commodity per currency rather than currency per commodity.", false);

    opts_util::add_plot_size(o, "Display", "Plot Width", "Plot Height", "c", #{ percent: 100 }, #{ percent: 100 });
    o.color("Display", "Marker Color", "b", "Color of the marker.", "b22222");

    o.set_default_section("General");
}

/// The x axis's label and the seconds in one step, by Step Size (:106-113).
fn interval_of(interval) {
    switch interval {
        "DayDelta" => ["Days", 86400],
        "WeekDelta" => ["Weeks", 604800],
        "TwoWeekDelta" => ["Double-Weeks", 1209600],
        "MonthDelta" => ["Months", 2628000],
        "QuarterDelta" => ["Quarters", rational(31536000, 4)],
        "HalfYearDelta" => ["Half Years", rational(31536000, 2)],
        "YearDelta" => ["Years", 31536000],
        _ => throw `not a step size: ${interval}`,
    }
}

/// The `[time, price]` pairs of `price_source` (:202-216). The transaction sources read the splits up to `to_date`
/// of the share and currency accounts; the price database gives its forward prices of `price_commodity` in
/// `report_currency`, newest first (`gnc-pricedb-get-prices`).
fn price_data(book, price_source, currency_accounts, to_date, price_commodity, report_currency) {
    if currency_accounts.is_empty() && price_source != "pricedb" {
        return [];
    }
    switch price_source {
        "actual-transactions" =>
            exchange::get_commodity_inst_prices(book, currency_accounts, to_date, price_commodity, report_currency),
        "weighted-average" =>
            exchange::get_commodity_totalavg_prices(book, currency_accounts, to_date, price_commodity, report_currency),
        "pricedb" => book.prices(price_commodity, report_currency).map(|p| [p.time, p.value]),
        _ => [],
    }
}

/// True when every element of `values` is equal, `(apply equal? values)`.
fn all_equal(values) {
    let first = values[0];
    values.all(|v| v == first)
}

/// `make-warning` (:159-164): GnuCash's heading and paragraph, a warning block.
fn make_warning(title, text) {
    #{ kind: "warning", title: title, text: text }
}

/// The warning GnuCash shows in place of the chart, `()` when there is data to plot (:231-257).
fn data_warning(data) {
    let straight_line = "This would result in a plot with one straight line. Unfortunately, the plotting tool can't handle that.";
    if data.is_empty() {
        make_warning("No data", "There is no price information available for the selected commodities in the selected time period.")
    } else if data.len() <= 1 {
        let text = "There was only one single price found for the selected commodities in the selected time period. "
            + "This doesn't give a useful plot.";
        make_warning("Only one price", text)
    } else if all_equal(data.map(|point| point[1])) {
        make_warning("All Prices equal", "All the prices found are equal. " + straight_line)
    } else if all_equal(data.map(|point| point[0])) {
        make_warning("All Prices at the same date", "All the prices found are from the same date. " + straight_line)
    } else {
        ()
    }
}

/// The document: one block. GnuCash leaves it untitled; red-ale titles it with the report's name, as the Average
/// Balance (design/reports.md §14).
fn document(report_title, block) {
    #{ title: report_title, sections: [#{ blocks: [block] }] }
}

/// A plot size option's value as a result's plot size.
fn plot_size(value) {
    let size = #{};
    size[value.unit] = value.value;
    size
}

/// `renderer` (:100-278).
fn render(params, book, opts, ctx) {
    let to_date = dates::time64_end_day_time(ctx, opts.get("General", "End Date"));
    let from_date = dates::time64_start_day_time(ctx, opts.get("General", "Start Date"));
    let interval = interval_of(opts.get("General", "Step Size"));
    let int_label = interval[0];
    let int_secs = interval[1];
    let report_title = opts.get("General", "Report name");
    // `gnc:color-option->hex-string` gives the stored text; a colour of the result is `#rrggbb`, so an alpha byte
    // (`rrggbbaa`) is dropped, as `gnc:color->html` drops it (5.17:bindings/guile/options.scm:72-77).
    let mcolor = "#" + opts.get("Display", "Marker Color").sub_string(0, 6);
    let report_currency = opts.get("Price", "Report's currency");
    let price_commodity = opts.get("Price", "Price of Commodity");
    let price_source = opts.get("Price", "Price Source");
    let currency_accounts = book.root().descendants_sorted().filter(|a| accounts::account_has_shares(a));
    let invert = opts.get("Price", "Invert prices");
    let amount_commodity = if invert { price_commodity } else { report_currency };
    let base_commodity = if invert { report_currency } else { price_commodity };

    if report_currency == price_commodity {
        let text = "Your selected commodity and the currency of the report are identical. "
            + "It doesn't make sense to show prices for identical commodities.";
        return document(report_title, make_warning("Identical commodities", text));
    }

    // The prices within the report's dates, each at its number of steps since the start date, inverted if asked
    // (:218-229). Both are exact; GnuCash's x is an exact Scheme rational too.
    let data = [];
    for point in price_data(book, price_source, currency_accounts, to_date, price_commodity, report_currency) {
        let date = point[0];
        if from_date <= date && date <= to_date {
            data.push([rational(date - from_date) / int_secs, if invert { 1 / point[1] } else { point[1] }]);
        }
    }

    let warning = data_warning(data);
    if warning != () {
        return document(report_title, warning);
    }

    // Each point's hover title (:260-267): GnuCash's `~2,2F` of x, a double; red-ale's `fixed` rounds the exact x
    // half-up to two places (design/reports.md §12, "Floating point in GnuCash, exact in red-ale").
    let points = data.map(|point| #{
        x: point[0],
        y: point[1],
        label: `${fixed(point[0], 2, 2)} ${int_label}  =  ${monetary(amount_commodity, point[1]).to_string()}`,
    });
    let chart = #{
        kind: "chart",
        type: "scatter",
        title: [report_title, `${base_commodity.mnemonic} - ${ctx.print_date_interval(from_date, to_date)}`],
        width: plot_size(opts.get("Display", "Plot Width")),
        height: plot_size(opts.get("Display", "Plot Height")),
        xAxis: #{ label: int_label, scale: "linear" },
        yAxis: #{ label: amount_commodity.mnemonic, format: "currency", commodity: amount_commodity },
        // Points in the marker colour, joined by a line in GnuCash's #4bb2c5 (:269-274); Chart.js's default point
        // is a circle.
        series: [#{ label: "Price", color: mcolor, points: points, line: "#4bb2c5", marker: "circle" }],
    };
    document(report_title, chart)
}
