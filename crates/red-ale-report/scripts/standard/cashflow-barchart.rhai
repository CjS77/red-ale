// cashflow-barchart.scm: cash flow barchart report
//
// By Jose Marino <jmarino@users.noreply.github.com>
//
// based on cash-flow.scm by:
// Herbert Thoma <herbie@hthoma.de>
// and net-barchart by:
// Robert Merkel <rgmerk@mira.net>
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
// Translated from 5.17:gnucash/report/reports/standard/cashflow-barchart.scm, cited below by line.

import "lib/accounts" as accounts;
import "lib/cash-flow-calc" as cfc;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:339-350).
fn templates() {
    [#{ guid: "5426e4d987f6444387fe70880e5b28a0", name: "Cash Flow Barchart", menuPath: ["_Income & Expense"],
        menuTip: "Shows a barchart with cash flow over time", params: #{} }]
}

/// `cashflow-barchart-options-generator` (:63-119).
fn options(params, o, book) {
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", "a");
    opts_util::add_interval_choice(o, "General", "Step Size", "b", "MonthDelta");
    opts_util::add_currency(o, "General", "Report's currency", "c");
    opts_util::add_price_source(o, "General", "Price Source", "d", "pricedb-nearest");

    o.account_list("Accounts", "Accounts", "a", "Report on these accounts.",
                   accounts::filter_accountlist_type(["BANK", "CASH", "ASSET", "STOCK", "MUTUAL"],
                                                     book.root().descendants_sorted()), #{});
    // GnuCash's help (:92); the option works backwards, as the Cash Flow's (standard/cash-flow.rhai's trading_help).
    o.boolean("Accounts", "Include Trading Accounts in report", "b",
              "Include transfers to and from Trading Accounts in the report. As in GnuCash 5.17 this works backwards: off "
              + "counts transfers to and from Trading Accounts, on leaves them out.", false);

    o.boolean("Display", "Show Money In", "a", "Show money in?", true);
    o.boolean("Display", "Show Money Out", "b", "Show money out?", true);
    o.boolean("Display", "Show Net Flow", "c", "Show net money flow?", true);
    o.boolean("Display", "Show Table", "d", "Display a table of the selected data.", false);
    opts_util::add_plot_size(o, "Display", "Plot Width", "Plot Height", "e", #{ percent: 100 }, #{ percent: 100 });

    o.set_default_section("General");
}

/// `cashflow-barchart-renderer` (:126-336). GnuCash sets no document title: the chart carries the report's name.
fn render(params, book, opts, ctx) {
    let accts = opts.get("Accounts", "Accounts");
    let blocks = if accts.is_empty() {
        [html_utils::make_no_account_warning("Cash Flow Barchart")]
    } else {
        chart_blocks(book, opts, ctx, accts)
    };
    #{ title: "", sections: [#{ blocks: blocks }] }
}

/// The option values the renderer reads (:134-170).
private fn read_options(book, opts, ctx, accts) {
    let report_currency = opts.get("General", "Report's currency");
    let price_source = opts.get("General", "Price Source");
    let from_date = dates::time64_start_day_time(ctx, opts.get("General", "Start Date"));
    let to_date = dates::time64_end_day_time(ctx, opts.get("General", "End Date"));
    let commodity_list = accounts::accounts_get_commodities(accts, report_currency);
    #{
        accounts: accts,
        include_trading_accounts: opts.get("Accounts", "Include Trading Accounts in report"),
        report_currency: report_currency,
        from_date: from_date,
        to_date: to_date,
        exchange_fn: exchange::case_exchange_fn(book, price_source, report_currency, to_date),
        time_exchange_fn: exchange::case_exchange_time_fn(book, ctx, price_source, report_currency, commodity_list, to_date),
        dates_list: dates::make_date_interval_list(ctx, from_date, to_date, dates::deltasym_to_delta(opts.get("General", "Step Size"))),
        show_in: opts.get("Display", "Show Money In"),
        show_out: opts.get("Display", "Show Money Out"),
        show_net: opts.get("Display", "Show Net Flow"),
        report_title: opts.get("General", "Report name"),
    }
}

/// The money in, out and net of each interval (:219-241), each summed and rounded per interval: `[in, out, net]`
/// of Rationals in the report currency.
private fn interval_flows(book, o) {
    let in_list = [];
    let out_list = [];
    let net_list = [];
    for date_pair in o.dates_list {
        let result = cfc::cash_flow_calc_money_in_out(#{
            book: book,
            accounts: o.accounts,
            from_date: date_pair[0],
            to_date: date_pair[1],
            report_currency: o.report_currency,
            include_trading_accounts: o.include_trading_accounts,
            time_exchange_fn: o.time_exchange_fn,
        });
        let money_in = collectors::sum_collector_commodity(result.money_in_collector, o.report_currency, o.exchange_fn).amount;
        let money_out = collectors::sum_collector_commodity(result.money_out_collector, o.report_currency, o.exchange_fn).amount;
        in_list.push(money_in);
        out_list.push(money_out);
        net_list.push(money_in - money_out);
    }
    [in_list, out_list, net_list]
}

/// The chart and table, or the empty-data warning (:176-326).
private fn chart_blocks(book, opts, ctx, accts) {
    let o = read_options(book, opts, ctx, accts);
    let flows = interval_flows(book, o);
    // :289-299: zero test over all three series, shown or not.
    if !collectors::not_all_zeros(flows[0] + flows[1] + flows[2]) {
        return [html_utils::make_empty_data_warning(o.report_title)];
    }
    let date_strings = o.dates_list.map(|d| ctx.print_date(d[0]));
    let links = [];
    for date_pair in o.dates_list {
        links.push(cashflow_url(opts, ctx, accts, date_pair));
    }
    let shown = [];
    if o.show_in {
        shown.push(["Money In", flows[0], "#0074D9"]);
    }
    if o.show_out {
        shown.push(["Money Out", flows[1], "#FF4136"]);
    }
    if o.show_net {
        shown.push(["Net Flow", flows[2], "#2ECC40"]);
    }
    let blocks = [make_chart(opts, ctx, o, date_strings, shown, links)];
    if opts.get("Display", "Show Table") {
        blocks.push(#{ kind: "text", text: "Overview:", role: "heading" });
        blocks.push(make_table(o, date_strings, shown));
    }
    blocks
}

/// `make-cashflow-url` (:207-214): the Cash Flow over one interval on these accounts, with this report's values of
/// the options the Cash Flow also has (so its "Report name" too).
private fn cashflow_url(opts, ctx, accts, date_pair) {
    let both_have = [["General", "Start Date"], ["General", "End Date"], ["General", "Report's currency"],
                  ["General", "Price Source"], ["Accounts", "Include Trading Accounts in report"]];
    html_utils::make_report_anchor("f8748b813fab4220ba26e743aedf38da", opts, both_have, [
        ["General", "Start Date", #{ absolute: ctx.print_timestamp(date_pair[0]) }],
        ["General", "End Date", #{ absolute: ctx.print_timestamp(date_pair[1]) }],
        ["Accounts", "Account", accts.map(|a| a.guid)],
    ])
}

/// The chart (:256-285): bars, one series per shown figure, each value linked to its interval's Cash Flow.
private fn make_chart(opts, ctx, o, date_strings, shown, links) {
    let size = |s| { let m = #{}; m[s.unit] = s.value; m };
    #{
        kind: "chart",
        type: "bar",
        title: [o.report_title, ctx.print_date_interval(o.from_date, o.to_date)],
        width: size.call(opts.get("Display", "Plot Width")),
        height: size.call(opts.get("Display", "Plot Height")),
        yAxis: #{ label: o.report_currency.mnemonic, format: "currency", commodity: o.report_currency.id },
        labels: date_strings,
        series: shown.map(|s| #{ label: s[0], color: s[2], values: s[1], fill: true, links: links }),
    }
}

/// The table of "Show Table" (:301-326): the date and each shown figure per interval, then the totals, the sums of
/// the rounded interval figures (:244-252).
private fn make_table(o, date_strings, shown) {
    let currency = o.report_currency;
    let columns = [#{ label: "Date", role: "date" }] + shown.map(|s| #{ label: s[0], role: "amount" });
    let rows = [];
    for i in 0..date_strings.len() {
        rows.push(#{ cells: [#{ text: date_strings[i] }] + shown.map(|s| #{ monetary: monetary(currency, s[1][i]) }) });
    }
    let totals = shown.map(|s| #{ monetary: monetary(currency, s[1].reduce(|sum, v| sum + v, rational(0))) });
    rows.push(#{ cells: [#{ text: "Total" }] + totals });
    #{ kind: "table", columns: columns, rows: rows }
}
