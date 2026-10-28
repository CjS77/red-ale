// standard/budget-barchart: a chart per account of its budgeted and actual figures over a range of budget periods.
//
// Translated from 5.17:gnucash/report/reports/standard/budget-barchart.scm, cited below by line. Its header, kept
// from the net chart it was copied from:
//
// net-barchart.scm : Display a time series for either net worth or
// net profit.
//
// By Robert Merkel <rgmerk@mira.net>
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
// The range is always on: there is no switch, unlike the Budget Report's. "Range start" and "Range end" have
// callbacks that only make their exact periods selectable in GnuCash's dialog (:87-111), so none is passed. GnuCash's
// two `animationDuration` settings (:180-184) are Chart.js's and have no counterpart in the result.

import "lib/budget" as budget_util;
import "lib/accounts" as accounts;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:322-328); "B_udget" is `gnc:menuname-budget` (5.17:gnucash/report/report-core.scm:159).
fn templates() {
    [#{ guid: "415cd38d39054d9e9c4040455290c2b1", name: "Budget Chart", menuPath: ["B_udget"],
        params: #{ reportname: "Budget Chart" } }]
}

/// `period-options` (:64-70).
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

/// `options-generator` (:72-146).
fn options(params, o, book) {
    o.budget("General", "Budget", "a", "Budget to use.", book.default_budget());
    o.multichoice_callback("General", "Range start", "g1.1", "Select a budget period type that starts the reporting range.",
                           "current", period_options(), ());
    o.number_range("General", "Exact start period", "g1.2", "Select exact period that starts the reporting range.",
                   1, 1, 60, 1);
    o.multichoice_callback("General", "Range end", "g2.1", "Select a budget period type that ends the reporting range.",
                           "next", period_options(), ());
    o.number_range("General", "Exact end period", "g2.2", "Select exact period that ends the reporting range.",
                   1, 1, 60, 1);

    o.account_list("Accounts", "Accounts", "c", "Report on these accounts.",
                   accounts::filter_accountlist_type(["BANK", "ASSET", "LIABILITY"], book.root().descendants_sorted()), #{});
    opts_util::add_account_levels(o, "Accounts", "Levels of Subaccounts", "d",
                                  "Maximum number of levels in the account tree displayed.", 6);

    o.boolean("Display", "Running Sum", "a", "Calculate as running sum?", true);
    o.multichoice("Display", "Chart Type", "b", "Select which chart type to use.", "bars",
                  [["bars", "Bar Chart"], ["lines", "Line Chart"]]);
    opts_util::add_plot_size(o, "Display", "Plot Width", "Plot Height", "c", #{ percent: 100 }, #{ percent: 100 });

    o.set_default_section("General");
}

/// `net-renderer` (:231-319). GnuCash leaves the document untitled and each chart names its account; the warnings
/// here leave out the heading that names the report, so the title does (design/reports.md §14).
fn render(params, book, opts, ctx) {
    let budget = opts.get("General", "Budget");
    let accts = opts.get("Accounts", "Accounts");
    let report_title = opts.get("General", "Report name");
    if accts.is_empty() {
        return #{ title: report_title, sections: [#{ blocks: [html_utils::make_no_account_warning(report_title)] }] };
    }
    if budget == () {
        return #{ title: params.reportname, sections: [#{ blocks: [html_utils::make_generic_budget_warning(params.reportname)] }] };
    }
    // :302-303
    let env = #{
        book: book,
        budget: budget,
        running_sum: opts.get("Display", "Running Sum"),
        bars: opts.get("Display", "Chart Type") == "bars",
        width: plot_size(opts.get("Display", "Plot Width")),
        height: plot_size(opts.get("Display", "Plot Height")),
        period_start: calc_user_period(budget, ctx, opts.get("General", "Range start"),
                                       to_period_val(opts.get("General", "Exact start period"))),
        period_end: calc_user_period(budget, ctx, opts.get("General", "Range end"),
                                     to_period_val(opts.get("General", "Exact end period"))),
    };
    let depth_limit = opts.get("Accounts", "Levels of Subaccounts");
    let charts = [];
    for acct in accts {
        if shown(acct, depth_limit) {
            charts.push(chart_create_budget_actual(ctx, env, acct));
        }
    }
    #{ title: report_title, sections: [#{ blocks: charts }] }
}

/// Whether an account gets a chart (:304-312): a leaf within the depth limit, or any account at it. GnuCash's
/// `get-account-level` counts the account's ancestors, the root among them (:240-244), which is the query API's
/// `depth`.
private fn shown(acct, depth_limit) {
    let leaf = acct.descendants().is_empty();
    if depth_limit == "all" { leaf } else { (acct.depth <= depth_limit && leaf) || acct.depth == depth_limit }
}

/// `to-period-val` (:272-273): the number rounded to an integer, ties to even, where the Budget Report truncates.
private fn to_period_val(v) {
    parse_int(v.round().to_string())
}

/// `find-period-relative-to-current` (:246-261): the period holding now, moved by `offset` and kept within the
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

/// `calc-user-period` (:263-270): the period a range end names.
private fn calc_user_period(budget, ctx, period_type, period_exact_val) {
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

/// `gnc:chart-create-budget-actual` (:155-222): for every period of the budget the rolled-up Budget amount and the
/// actual, each added to the sum of the periods before it with "Running Sum" (:207-213), kept as a point at the
/// period's start when the period is in the range, both ends inclusive (:214-221). The sums run over every period,
/// those outside the range too, and with running sums the title gives their totals (:185-193).
private fn chart_create_budget_actual(ctx, env, acct) {
    let curr = acct.commodity;
    let bgt_sum = rational(0);
    let act_sum = rational(0);
    let bgt_vals = [];
    let act_vals = [];
    let dates_list = [];
    for period in 0..env.budget.numPeriods {
        let bgt = budget_util::get_account_period_rolledup_budget_value(env.book, env.budget, acct, period);
        let act = env.budget.account_period_actual_value(acct, period);
        bgt_sum = bgt + if env.running_sum { bgt_sum } else { rational(0) };
        act_sum = act + if env.running_sum { act_sum } else { rational(0) };
        if env.period_start <= period && period <= env.period_end {
            bgt_vals.push(bgt_sum);
            act_vals.push(act_sum);
            dates_list.push(ctx.print_date(env.budget.period_start_date(period)));
        }
    }
    // Translators' note in GnuCash: Bgt and Act refer to budgeted and actual total amounts (:188-190).
    let title = if env.running_sum {
        [acct.name, `Bgt: ${monetary(curr, bgt_sum).to_string()} Act: ${monetary(curr, act_sum).to_string()}`]
    } else {
        [acct.name]
    };
    #{
        kind: "chart",
        type: if env.bars { "bar" } else { "line" },
        title: title,
        width: env.width,
        height: env.height,
        yAxis: #{ label: curr.mnemonic, format: "currency", commodity: curr.id },
        labels: dates_list,
        series: [
            #{ label: "Actual", color: "#FF4136", values: act_vals, fill: env.bars },
            #{ label: "Budget", color: "#0074D9", values: bgt_vals, fill: env.bars },
        ],
    }
}

/// A plot size option's value as the chart takes it.
private fn plot_size(size) {
    let m = #{};
    m[size.unit] = size.value;
    m
}
