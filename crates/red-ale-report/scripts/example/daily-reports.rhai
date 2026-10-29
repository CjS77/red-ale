// daily-reports.scm: reports based on the day of the week
//
// Copyright (C) 2003, Andy Wingo <wingo at pobox dot com>
//
// based on account-piecharts.scm by Robert Merkel (rgmerk@mira.net)
// and Christian Stimming <stimming@tu-harburg.de>
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
// Translated from 5.17:gnucash/report/reports/example/daily-reports.scm, cited below by line. The two templates
// share one options generator and one renderer; what tells them apart (the list of :305-310) comes in as `params`.
//
// Kept as GnuCash does: no sign is reversed, so income totals are negative, labels and title included
// ("Monday - -$109.00"), and a negative slice is sized by its absolute share (:220-231).
//
// Deliberate differences, each commented where it happens (design/reports.md §14):
// - the result's title is the "Report name" option, where GnuCash's document has none;
// - `gnc-accounts-and-all-descendants` lists the accounts in a fixed order, where GnuCash's is a hash set's (§12);
// - progress reporting (`gnc:report-percent-done`) and the debug log line (:165) are dropped (§12).

import "lib/accounts" as accounts;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html;
import "lib/options" as opts_util;

/// The two templates (:291-310): the report names (:49-50), menu names (:37-38) and tips (:41-44), and the account
/// types each reports on. `reportname` names the chart's series (:269-271).
fn templates() {
    [
        #{ guid: "5e2d129f28d14df881c3e47e3053f604", name: "Income vs. Day of Week", menuPath: ["E_xamples"],
           menuName: "Income vs. Day of Week", menuTip: "Shows a piechart with the total income for each day of the week",
           params: #{ reportname: "Income vs. Day of Week", account_types: ["INCOME"] } },
        #{ guid: "dde49fed4ca940959ae7d01b72742530", name: "Expenses vs. Day of Week", menuPath: ["E_xamples"],
           menuName: "Expenses vs. Day of Week", menuTip: "Shows a piechart with the total expenses for each day of the week",
           params: #{ reportname: "Expenses vs. Day of Week", account_types: ["EXPENSE"] } },
    ]
}

/// `options-generator` (:68-106). "Include Sub-Accounts" and "Accounts" share the key "a" (:82-93); red-ale keeps
/// declaration order (design/reports.md §6). "Levels of Subaccounts" is named (:58) but never registered.
fn options(params, o, book) {
    let account_types = params.account_types;
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", "a");
    opts_util::add_currency(o, "General", "Report's currency", "b");
    opts_util::add_price_source(o, "General", "Price Source", "c", "weighted-average");
    o.boolean("Accounts", "Include Sub-Accounts", "a", "Include sub-accounts of all selected accounts.", true);
    o.account_list("Accounts", "Accounts", "a", "Report on these accounts, if chosen account level allows.",
                   accounts::filter_accountlist_type(account_types, book.root().descendants_sorted()), #{ types: account_types });
    o.boolean("Display", "Show Totals", "b", "Show the total balance in legend?", true);
    opts_util::add_plot_size(o, "Display", "Plot Width", "Plot Height", "d", #{ percent: 100 }, #{ percent: 100 });
    o.set_default_section("General");
}

/// The day names (:161-163), Sunday first.
private fn days_of_week() {
    ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]
}

/// The total of each day of the week, Sunday first (:151-231): every non-void split of the accounts (with all their
/// descendants when `dosubs`) posted from `from_date` to `to_date`, its amount in its account's commodity converted
/// to the report currency at its posted date and added to the total of that date's local weekday, `(modulo (1-
/// (gnc:time64-get-week-day date)) 7)`. The exchange function prices the commodities of the accounts and all their
/// descendants, whatever `dosubs` says (:175-182). Totals are exact.
private fn daily_totals(book, ctx, env) {
    let accts = env.accounts;
    let commodity_list = accounts::accounts_get_commodities(accounts::accounts_and_all_descendants(accts), env.report_currency);
    let exchange_fn = exchange::case_exchange_time_fn(book, ctx, env.price_source, env.report_currency, commodity_list,
                                                      env.to_date);
    if env.dosubs {
        accts = accounts::accounts_and_all_descendants(accts);
    }
    // The query's sort (:207-210) does not change the sums.
    let splits = book.split_query(#{ accounts: accts, from: env.from_date, to: env.to_date,
                                     sort: [#{ key: "datePosted" }, #{ key: "default" }] });
    let totals = [];
    totals.pad(7, rational(0));
    for split in splits {
        let date = split.transaction.datePosted;
        let weekday = (ctx.local(date).weekday - 1) % 7;
        let foreign = monetary(split.account.commodity, split.amount);
        totals[weekday] += exchange::apply_exchange_time_fn(exchange_fn, foreign, env.report_currency, date).amount;
    }
    totals
}

/// The sum of `values`, `(apply + daily-totals)`.
private fn sum_of(values) {
    let sum = rational(0);
    for v in values {
        sum += v;
    }
    sum
}

/// A plot size option's value as the chart takes it.
private fn plot_size(size) {
    let out = #{};
    out[size.unit] = size.value;
    out
}

/// The pie (:249-276): the days whose total is not exactly zero, in week order (:235-237), each labelled with its
/// name and, with "Show Totals", ` - ` and the exact total (`gnc:monetary->string`, :238-247); the title the "Report
/// name" option, then the period and, with "Show Totals", `: ` and the sum of all seven totals (:255-267). One
/// series named after the template, the totals as they are (GnuCash does not round them), GnuCash's colours; the
/// axes are hidden (:275), so there is no grid.
private fn pie_chart(env, ctx, days, totals) {
    let labels = [];
    for i in 0..days.len() {
        let label = days[i];
        if env.show_total {
            label += " - " + monetary(env.report_currency, totals[i]).to_string();
        }
        labels.push(label);
    }
    let subtitle = ctx.print_date_interval(env.from_date, env.to_date);
    if env.show_total {
        subtitle += ": " + monetary(env.report_currency, env.total).to_string();
    }
    let colors = html::assign_colors(days.len());
    #{
        kind: "chart", type: "pie", title: [env.report_title, subtitle], width: env.width, height: env.height, grid: false,
        yAxis: #{ label: "", format: "currency", commodity: env.report_currency.id },
        labels: labels, series: [#{ label: env.reportname, color: colors[0], values: totals, colors: colors }],
    }
}

/// `piechart-renderer` (:113-289).
fn render(params, book, opts, ctx) {
    let report_title = opts.get("General", "Report name");
    let env = #{
        to_date: dates::time64_end_day_time(ctx, opts.get("General", "End Date")),
        from_date: dates::time64_start_day_time(ctx, opts.get("General", "Start Date")),
        accounts: opts.get("Accounts", "Accounts"), dosubs: opts.get("Accounts", "Include Sub-Accounts"),
        report_currency: opts.get("General", "Report's currency"), price_source: opts.get("General", "Price Source"),
        report_title: report_title, reportname: params.reportname, show_total: opts.get("Display", "Show Totals"),
        height: plot_size(opts.get("Display", "Plot Height")), width: plot_size(opts.get("Display", "Plot Width")),
        total: (),
    };

    // GnuCash's document has no title of its own: the chart's first title line and the warning's heading name the
    // report. The warning block leaves that heading out, so the result's title names the report instead.
    let block = if env.accounts.is_empty() {
        // No accounts is the empty-data warning, not the no-account one (:283-286).
        html::make_empty_data_warning(report_title)
    } else {
        let totals = daily_totals(book, ctx, env);
        env.total = sum_of(totals);
        let days = [];
        let shown = [];
        let names = days_of_week();
        for i in 0..7 {
            if !totals[i].is_zero() {
                days.push(names[i]);
                shown.push(totals[i]);
            }
        }
        if shown.is_empty() { html::make_empty_data_warning(report_title) } else { pie_chart(env, ctx, days, shown) }
    };
    #{ title: report_title, sections: [#{ blocks: [block] }] }
}
