// lib/options: the option declarations several reports share.
//
// Translated from 5.17:gnucash/report/options-utilities.scm (lines 44-234), cited below as OU, with
// `gnc:options-make-date-interval!` of 5.17:bindings/guile/options.scm (lines 330-334). Their headers:
//
// options-utilities.scm: Useful option helper functions.
//
// By Christian Stimming <stimming@tu-harburg.de>
//
// options.scm: Scheme code for supporting options
//
// Both carry this notice:
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
// Each function takes the option builder `o` of a template's `options()` where GnuCash takes the options object.
// Names, keys, help texts, choices and defaults are GnuCash's. Multichoice keys are GnuCash's symbols as strings
// ("pricedb-nearest"), and the account levels 1 to 6 integers beside "all".

/// `evaluate` (OU:44-45): a default may be given as a function, which is called.
private fn evaluate(arg) {
    if type_of(arg) == "Fn" { arg.call() } else { arg }
}

/// `gnc:options-add-report-date!` (OU:48-51): one end date of a report.
fn add_report_date(o, pagename, optname, sort_tag) {
    o.end_date(pagename, optname, sort_tag, "Select a date to report on.", true);
}

/// `gnc:options-add-date-interval!` (OU:54-59) through `gnc:options-make-date-interval!`: a start date with key
/// `sort_tag + "a"` and an end date with `sort_tag + "b"`.
fn add_date_interval(o, pagename, name_from, name_to, sort_tag) {
    o.start_date(pagename, name_from, sort_tag + "a", "Start of reporting period.", true);
    o.end_date(pagename, name_to, sort_tag + "b", "End of reporting period.", true);
}

/// `gnc:options-add-interval-choice!` (OU:62-74): the step between data points, a delta name of lib/dates.
fn add_interval_choice(o, pagename, optname, sort_tag, default_value) {
    o.multichoice(pagename, optname, sort_tag, "The amount of time between data points.", evaluate(default_value), [
        ["DayDelta", "One Day"],
        ["WeekDelta", "One Week"],
        ["TwoWeekDelta", "Two Weeks"],
        ["MonthDelta", "One Month"],
        ["QuarterDelta", "Quarter Year"],
        ["HalfYearDelta", "Half Year"],
        ["YearDelta", "One Year"],
    ]);
}

/// `gnc:options-add-account-levels!` (OU:79-93): how many levels of accounts to show, "all" or 1 to 6, with the
/// caller's help text.
fn add_account_levels(o, pagename, name_display_depth, sort_tag, help_string, default_depth) {
    o.multichoice(pagename, name_display_depth, sort_tag, help_string, evaluate(default_depth), [
        ["all", "All"],
        [1, "1"],
        [2, "2"],
        [3, "3"],
        [4, "4"],
        [5, "5"],
        [6, "6"],
    ]);
}

/// `gnc:options-add-account-selection!` (OU:96-120): the account levels (key `sort_tag + "a"`), whether to show
/// the sub-accounts of all selected accounts (`sort_tag + "b"`) and the accounts to report on (`sort_tag + "c"`).
/// An account shows if the depth allows and it is selected, or a parent is selected and sub-accounts are shown.
fn add_account_selection(o, pagename, name_display_depth, name_show_subaccounts, name_accounts, sort_tag, default_depth,
                         default_accounts, default_show_subaccounts) {
    add_account_levels(o, pagename, name_display_depth, sort_tag + "a",
                       "Show accounts to this depth, overriding any other option.", default_depth);
    o.boolean(pagename, name_show_subaccounts, sort_tag + "b",
              "Override account-selection and show sub-accounts of all selected accounts?", default_show_subaccounts);
    o.account_list(pagename, name_accounts, sort_tag + "c", "Report on these accounts, if display depth allows.",
                   evaluate(default_accounts), #{});
}

/// `gnc:options-add-currency!` (OU:124-130): the report's currency, defaulting to `gnc-default-report-currency`,
/// the report settings' currency.
fn add_currency(o, pagename, name_report_currency, sort_tag) {
    o.currency(pagename, name_report_currency, sort_tag, "Select the currency to display the values of this report in.",
               o.default_report_currency());
}

/// `gnc:options-add-price-source!` (OU:133-144): where prices come from, a source of lib/exchange's case functions.
fn add_price_source(o, pagename, optname, sort_tag, default_value) {
    o.multichoice(pagename, optname, sort_tag, "The source of price information.", evaluate(default_value), [
        ["average-cost", "Average cost of purchases weighted by volume"],
        ["weighted-average", "Weighted average of all transactions in the past"],
        ["pricedb-before", "Last up through report date"],
        ["pricedb-nearest", "Closest to report date"],
        ["pricedb-latest", "Most recent"],
    ]);
}

/// The number of a plot-size default: GnuCash takes the `cdr` of a `(percent . n)` or `(pixels . n)` pair, dropping
/// the unit (OU:150-152). So does red-ale for `#{ percent: n }`, `#{ pixels: n }` and the `#{ unit, value }` that
/// `opts.get` returns; the number alone says the unit, percent up to 100.
private fn plot_size_number(v) {
    if type_of(v) != "map" {
        v
    } else if "value" in v {
        v.value
    } else if "percent" in v {
        v.percent
    } else if "pixels" in v {
        v.pixels
    } else {
        v
    }
}

/// `gnc:options-add-plot-size!` (OU:147-165): the width (key `sort_tag + "a"`) and height (`sort_tag + "b"`) of a
/// chart, up to 100 in percent and above in pixels.
fn add_plot_size(o, pagename, name_width, name_height, sort_tag, default_width, default_height) {
    let width = plot_size_number(evaluate(default_width));
    let height = plot_size_number(evaluate(default_height));
    o.plot_size(pagename, name_width, sort_tag + "a", "Width of plot, 10 - 100 in percent, above in pixels.", width);
    o.plot_size(pagename, name_height, sort_tag + "b", "Height of plot, 10 - 100 in percent, above in pixels.", height);
}

/// `gnc:options-add-marker-choice!` (OU:168-183): the marker of each data point of a scatter plot.
fn add_marker_choice(o, pagename, optname, sort_tag, default_value) {
    o.multichoice(pagename, optname, sort_tag, "Choose the marker for each data point.", evaluate(default_value), [
        ["diamond", "Diamond"],
        ["circle", "Circle"],
        ["square", "Square"],
        ["cross", "Cross"],
        ["plus", "Plus"],
        ["dash", "Dash"],
        ["filleddiamond", "Filled diamond"],
        ["filledcircle", "Filled circle"],
        ["filledsquare", "Filled square"],
    ]);
}

/// `gnc:options-add-sort-method!` (OU:186-196): how to sort accounts.
fn add_sort_method(o, pagename, optname, sort_tag, default_value) {
    o.multichoice(pagename, optname, sort_tag, "Choose the method for sorting accounts.", evaluate(default_value), [
        ["acct-code", "Alphabetical by account code"],
        ["alphabetical", "Alphabetical by account name"],
        ["amount", "Numerical by descending amount"],
    ]);
}

/// `gnc:options-add-subtotal-view!` (OU:200-234): what parent accounts show (key `sort_tag + "a"`, default
/// "immediate-bal") and whether they get subtotals (`sort_tag + "b"`, "t" or "f", default "f").
fn add_subtotal_view(o, pagename, optname_parent_balance_mode, optname_parent_total_mode, sort_tag) {
    let balance_help = ["How to show the balances of parent accounts.",
                        "Account Balance in the parent account, excluding any subaccounts.",
                        "Do not show any balances of parent accounts."];
    o.multichoice(pagename, optname_parent_balance_mode, sort_tag + "a", join(balance_help, "\n* "), "immediate-bal", [
        ["immediate-bal", "Account Balance"],
        ["recursive-bal", "Calculate Subtotal"],
        ["omit-bal", "Do not show"],
    ]);
    let total_help = ["How to show account subtotals for parent accounts.",
                      "Show subtotals for selected parent accounts which have subaccounts.",
                      "Do not show any subtotals for parent accounts."];
    o.multichoice(pagename, optname_parent_total_mode, sort_tag + "b", join(total_help, "\n* "), "f", [
        ["t", "Show subtotals"],
        ["f", "Do not show"],
    ]);
}

/// `string-join`: the strings of `parts` with `separator` between them.
private fn join(parts, separator) {
    let out = "";
    for (part, i) in parts {
        if i > 0 {
            out += separator;
        }
        out += part;
    }
    out
}
