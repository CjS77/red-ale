// reconcile-report.scm : Reconciliation report
//
// calls functions defined in trep-engine.scm with defaults suitable
// for a reconciliation report including alternative date filtering
// strategy
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
// Translated from 5.17:gnucash/report/reports/standard/reconcile-report.scm, cited below by line: the engine's
// options with other defaults, and its renderer with two calculated cells of its own and instructions shown when no
// account is selected.

import "lib/trep-engine" as trep;

/// The template (:106-114). GnuCash registers the name translated (`G_`); red-ale's is the English one.
fn templates() {
    [#{ guid: "e45218c6d76f11e7b5ef0800277ef320", name: "Reconciliation Report", params: #{} }]
}

/// `reconcile-report-options-generator` (:34-58): sorted by reconcile state, then date; dated by the reconcile date,
/// from the start of the previous quarter to today.
///
/// GnuCash also sets "Running Balance" #f in the Display section and makes it internal (:53-54, :57). The engine has
/// no option of that name, and `GncOption-set-default-value` and `make-internal` on an option that does not exist do
/// nothing (`5.17:bindings/guile/gnc-optiondb.i:1500-1503`; `5.17:libgnucash/engine/gnc-optiondb.cpp:266-272`).
/// red-ale refuses both on an unknown option, so those calls are left out (design/notes/report-transaction-engine.md,
/// fact 2 and §7 Q6).
fn options(params, o, book) {
    trep::trep_options_generator(o, book);
    o.set_default("Sorting", "Primary Key", "reconciled-status");
    o.set_default("Sorting", "Secondary Key", "date");
    o.set_default("Sorting", "Secondary Subtotal for Date Key", "none");
    o.set_default("General", "Start Date", "start-prev-quarter");
    o.set_default("General", "End Date", "today");
    o.set_default("General", "Date Filter", "reconciled");
    o.set_default("Display", "Reconciled Date", true);
    o.set_default("Display", "Memo", false);
}

/// `reconcile-report-instructions` (:60-67), with its two line breaks.
fn reconcile_report_instructions() {
    "The reconcile report is designed to be similar to the formal reconciliation tool. Please select the account " +
        "from Report Options. Please note the dates specified in the options will apply to the Reconciliation Date.\n\n"
}

/// `split-amount` (:71-74): a void transaction's former amount.
fn split_amount(s) {
    if s.transaction.isVoid { s.voidFormerAmount } else { s.amount }
}

/// `amount` (:76-77): the amount in the account's commodity, never converted.
fn split_monetary(s) {
    monetary(s.account.commodity, split_amount(s))
}

/// `reconcile-report-calculated-cells` (:69-98): "Funds In" and "Funds Out", subtotalled, never reversed, and not
/// a dual column, so subtotal rows show both (TE:1713-1765); Funds In merges with Funds Out in the Subtotal Table.
/// Their friendly heading is `(const "")`.
fn reconcile_report_calculated_cells(options) {
    [
        trep::calc_cell("Funds In", |s, tr| if split_amount(s) > 0 { split_monetary(s) }, false, true, false, "", true),
        trep::calc_cell("Funds Out", |s, tr| if !(split_amount(s) > 0) { -split_monetary(s) }, false, true, false, "", false),
    ]
}

/// `reconcile-report-renderer` (:100-104).
fn render(params, book, opts, ctx) {
    trep::trep_renderer(book, opts, ctx, #{
        custom_calculated_cells: |options| reconcile_report_calculated_cells(options),
        empty_report_message: reconcile_report_instructions(),
    })
}
