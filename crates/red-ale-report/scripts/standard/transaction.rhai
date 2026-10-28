// transaction-report.scm : Report on all transactions in account(s)
//
// calls transaction report functions in trep-engine.scm
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
// Translated from 5.17:gnucash/report/reports/standard/transaction.scm, cited below by line: the engine as it is.
// GnuCash registers no export type for it (:31-36), so neither does red-ale; the engine's CSV rows serve the reports
// that do.

import "lib/trep-engine" as trep;

/// The template (:31-34).
fn templates() {
    [#{ guid: "2fe3b9833af044abb929a88d5a59620f", name: "Transaction Report", params: #{} }]
}

/// `gnc:trep-options-generator` (:35).
fn options(params, o, book) {
    trep::trep_options_generator(o, book);
}

/// `gnc:trep-renderer` (:36).
fn render(params, book, opts, ctx) {
    trep::trep_renderer(book, opts, ctx, #{})
}
