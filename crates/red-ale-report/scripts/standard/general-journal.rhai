// general-journal.scm: general journal report
//
// By David Montenegro <sunrise2000@comcast.net> 2004.07.14
//
//  * BUGS:
//
//    See any "FIXME"s in the code.
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
// Translated from 5.17:gnucash/report/reports/standard/general-journal.scm, cited below by line: the Register
// report's options and renderer (lib/register) in journal mode, over every non-void transaction of the Book.

import "lib/register" as register;

/// The template (:36, :116-123).
fn templates() {
    [#{ guid: "25455562bd234dd0b048ecc5a8af9e43", name: "General Journal", menuPath: ["_Assets & Liabilities"], params: #{} }]
}

/// `general-journal-options-generator` (:43-107): the Register report's options with the General Journal's
/// defaults.
///
/// GnuCash also stores its query as the default of the Register's `__reg/query` option (:57-71, :79); red-ale has no
/// query option, so `render` builds it. GnuCash sets the Report name to "General Journal" (:105) because the options
/// came from the Register template, whose name they carry; red-ale adds the Report name after this function, with
/// this template's name, so that call is left out.
fn options(params, o, book) {
    register::options(o, book);

    // the "__reg" options required by the Register Report (:74-83)
    o.set_default("__reg", "journal", true);
    o.set_default("__reg", "double", true);
    o.set_default("__reg", "debit-string", "Debit");
    o.set_default("__reg", "credit-string", "Credit");

    // the display tab (:87-103)
    let num = if book.uses_split_action_for_num() { "Num/Action" } else { "Num" };
    let display = [
        ["Date", true],
        [num, false],
        ["Description", true],
        ["Account", true],
        ["Shares", false],
        ["Price", false],
        // note the "Amount" multichoice option here
        ["Amount", "double"],
        ["Running Balance", false],
        ["Totals", false],
    ];
    for d in display { o.set_default("Display", d[0], d[1]); }

    o.set_default("General", "Title", "General Journal");
}

/// The query of :46, :57-71: the splits of the Book not in the void state, sorted by date posted, then in split
/// order. GnuCash also leaves out the splits of the scheduled transactions' template accounts (:66-71); those
/// accounts are not in red-ale's account tree, so the Book's accounts are every account but them.
fn general_journal_query(book) {
    #{
        accounts: book.accounts(),
        reconcile: ["n", "c", "y", "f"],
        sort: [#{ key: "datePosted", ascending: true }, #{ key: "default", ascending: true }],
    }
}

/// `general-journal-renderer` (:111-114): delegate to the Register report's renderer.
fn render(params, book, opts, ctx) {
    register::reg_renderer(book, opts, ctx, general_journal_query(book))
}
