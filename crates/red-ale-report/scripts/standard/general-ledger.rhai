// general-ledger.scm: general ledger report
//
// By David Montenegro <sunrise2000@comcast.net> 2004.07.13
//
//  * BUGS:
//
//    See any "FIXME"s in the code.
//
// Largely borrowed from welcome-to-gnucash.scm by
//  Bill Gribble <grib@gnumatic.com>
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
// Translated from 5.17:gnucash/report/reports/standard/general-ledger.scm, cited below by line: the Transaction
// Report's options with other defaults, and its renderer with the title "General Ledger".

import "lib/trep-engine" as trep;
import "standard/transaction" as transaction_report;

/// The template (:39, :149-156).
fn templates() {
    [#{ guid: "2e22929e5c5b4b769f615a815ef0c20f", name: "General Ledger", menuPath: ["_Assets & Liabilities"], params: #{} }]
}

/// `general-ledger-options-generator` (:47-138): the engine's options, with the defaults the General Ledger changes.
///
/// GnuCash also sets "Running Balance" #t and "Totals" #f in the Display section (:82-83, :102-103). The engine has no
/// options of those names ("Account Balance" and "Grand Total" are its), and `GncOption-set-default-value` on an
/// option that does not exist does nothing (`5.17:bindings/guile/gnc-optiondb.i:1500-1503`), so the General Ledger
/// shows no balance column and does show a Grand Total. red-ale refuses `o.set_default` on an unknown option, so
/// those two calls are left out (design/notes/report-transaction-engine.md, fact 2 and §7 Q6).
fn options(params, o, book) {
    trep::trep_options_generator(o, book);

    // the accounts tab (:56-57)
    o.set_default("Accounts", "Filter Type", "none");
    o.set_default("Filter", "Void Transactions", "non-void-only");

    // the display tab (:60-107): the num field source decides the num option's name, and whether "Trans Number"
    // exists (:64-69, :89)
    let split_action = book.uses_split_action_for_num();
    let display = [["Date", true], ["Reconciled Date", false]];
    if split_action {
        display += [["Num/Action", false], ["Trans Number", false]];
    } else {
        display.push(["Num", false]);
    }
    display += [
        ["Description", true],
        ["Memo", false],
        ["Account Name", false],
        ["Use Full Account Name", false],
        ["Account Code", false],
        ["Other Account Name", false],
        ["Use Full Other Account Name", false],
        ["Other Account Code", false],
        ["Shares", false],
        ["Price", false],
        // note the "Amount" multichoice option here
        ["Amount", "double"],
        ["Sign Reverses", "credit-accounts"],
    ];
    for d in display { o.set_default("Display", d[0], d[1]); }

    // the general tab (:110-111)
    o.set_default("Display", "Detail Level", "single");

    // GnuCash can't set the Report name here (:114-116); it defaults to the template's name, "General Ledger".

    // the sorting tab (:119-135)
    let sorting = [
        ["Primary Key", "account-code"],
        ["Show Full Account Name", false],
        ["Show Account Code", true],
        ["Primary Subtotal", true],
        ["Primary Subtotal for Date Key", "none"],
        ["Primary Sort Order", "ascend"],
        ["Secondary Key", "register-order"],
        ["Secondary Subtotal", true],
        ["Secondary Subtotal for Date Key", "none"],
        ["Secondary Sort Order", "ascend"],
    ];
    for s in sorting { o.set_default("Sorting", s[0], s[1]); }
}

/// `general-ledger-renderer` (:142-147): delegate to the Transaction Report's renderer, then title the document,
/// warnings included.
fn render(params, book, opts, ctx) {
    let document = transaction_report::render(params, book, opts, ctx);
    document.title = "General Ledger";
    document
}
