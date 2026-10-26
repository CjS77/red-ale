// lib/html-utilities: chart colours, the exchange-rates table and the warnings reports show instead of figures.
//
// Translated from 5.17:gnucash/report/html-utilities.scm (lines 226-381), cited below as HU. Its header:
//
// html-utilities.scm: Useful functions when using the HTML generator.
//
// Modified slightly by David Montenegro 2004.06.18.
//
// Copyright 2001 Christian Stimming <stimming@tu-harburg.de>
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
// GnuCash builds HTML; these return blocks of the report result (`design/reports.md` §7). A warning block has one
// title and one text, so GnuCash's first heading, the report's name and a colon, is left out: the document's title
// names the report. So is the "Edit report options" link, which needs GnuCash's report ids.

import "lib/accounts" as accounts;
import "lib/exchange" as exchange;

/// `gnc:assign-colors` (HU:226-245): `num_colors` chart colours from GnuCash's 18-colour palette, starting again
/// from its first colour when they run out.
fn assign_colors(num_colors) {
    // From http://clrs.cc/ and flatuicolors.com, as GnuCash's comment says.
    let base_colors = ["#FF4136", "#FF851B", "#FFDC00", "#2ECC40", "#0074D9", "#001f3f", "#85144b", "#7FDBFF", "#F012BE",
                       "#3D9970", "#39CCCC", "#f39c12", "#e74c3c", "#e67e22", "#9b59b6", "#8e44ad", "#16a085", "#d35400"];
    let result = [];
    for i in 0..num_colors {
        result.push(base_colors[i % base_colors.len()]);
    }
    result
}

/// `gnc:html-make-rates-table` (HU:257-274): a table of the price in `currency` of one unit of each commodity the
/// `accounts` hold other than `currency`, sorted by namespace and symbol, by `price_fn` (lib/exchange's
/// `case_price_fn`). Headed "Exchange rate" or "Exchange rates" across both columns; with no commodity, an empty
/// table with no heading, as GnuCash's.
fn make_rates_table(currency, price_fn, accts) {
    let rows = [];
    for comm in accounts::accounts_get_commodities_sorted(accts, currency) {
        rows.push(#{ cells: [#{ monetary: monetary(comm, 1) },
                             #{ price: exchange::apply_price_fn(price_fn, comm), currency: currency }] });
    }
    let heading = switch rows.len() {
        0 => "",
        1 => "Exchange rate",
        _ => "Exchange rates",
    };
    #{ kind: "table", columns: [#{ label: heading, role: "amount" }, #{ label: "", role: "amount" }], rows: rows }
}

/// `gnc:html-make-generic-warning` (HU:350-357): a warning block titled `warning_title_string` saying
/// `warning_string`. When the title is empty, GnuCash's only other heading, `report_title_string` and a colon, takes
/// its place.
fn make_generic_warning(report_title_string, warning_title_string, warning_string) {
    let title = if warning_title_string == "" { report_title_string + ":" } else { warning_title_string };
    #{ kind: "warning", title: title, text: warning_string }
}

/// `gnc:html-make-generic-simple-warning` (HU:282-286): a warning saying `message`, titled by the report.
fn make_generic_simple_warning(report_title_string, message) {
    make_generic_warning(report_title_string, "", message)
}

/// `gnc:html-make-generic-budget-warning` (HU:276-279).
fn make_generic_budget_warning(report_title_string) {
    make_generic_simple_warning(report_title_string, "No budgets exist. You must create at least one budget.")
}

/// `gnc:html-make-generic-options-warning` (HU:359-365).
fn make_generic_options_warning(report_title_string) {
    make_generic_warning(report_title_string, "", "This report requires you to specify certain report options.")
}

/// `gnc:html-make-no-account-warning` (HU:367-373).
fn make_no_account_warning(report_title_string) {
    make_generic_warning(report_title_string, "No accounts selected",
                         "This report requires accounts to be selected in the report options.")
}

/// `gnc:html-make-empty-data-warning` (HU:375-381).
fn make_empty_data_warning(report_title_string) {
    make_generic_warning(report_title_string, "No data",
                         "The selected accounts contain no data/transactions (or only zeroes) for the selected time period")
}
