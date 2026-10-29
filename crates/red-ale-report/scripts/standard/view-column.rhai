// view-column.scm : simple multi-column table view.
// Copyright 2001 Bill Gribble <grib@gnumatic.com>
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
// Translated from 5.17:gnucash/report/reports/standard/view-column.scm, cited below by line: other reports laid out
// in a table of rows and columns.
//
// Deliberate differences (design/reports.md §14):
// - the script lays out the cells; the host runs each child and fills its cell (a script cannot run reports);
// - a child is a template or saved configuration by GUID, not a report instance of the session;
// - the options-changed and options-cleanup callbacks (:161-177) have nothing to do: there are no instances to mark
//   dirty, and no callback slot to clear.

/// The template (:180-189).
fn templates() {
    [#{ guid: "d8ba4a2e89e8479ca9f6eccdeb164588", name: "Multicolumn View", menuName: "Custom Multicolumn Report",
        menuPath: ["_Multicolumn"], params: #{} }]
}

/// `make-options` (:38-49): the report-list, edited on its own page in GnuCash, and the number of columns.
fn options(params, o, book) {
    o.report_placement("__general", "report-list");
    o.number_range("General", "Number of columns", "a", "Number of columns before wrapping to a new row.", 1, 0, 20, 1);
}

/// `render-view` (:51-159): one cell per child, in order. A row is closed once its width reaches "Number of
/// columns", checked only after a child joins it; the next row starts as wide as the room children above take in it
/// with their row spans (`column-allocs`, :99-104, :135-146). The view's document has no title.
fn render(params, book, opts, ctx) {
    let table_width = opts.get("General", "Number of columns");
    let allocs = [];
    let rows = [];
    let current_row = [];
    let current_width = 0;
    let current_row_num = 0;
    for (info, index) in opts.get("__general", "report-list") {
        let colspan = info.colspan;
        // Every row the child spans, at least its own (:100-104).
        let row = current_row_num;
        loop {
            while allocs.len() <= row { allocs.push(0); }
            allocs[row] += colspan;
            if 1 + row - current_row_num < info.rowspan { row += 1; } else { break; }
        }
        current_row.push(#{ rowspan: info.rowspan, colspan: colspan, child: index });
        current_width += colspan;
        if current_width >= table_width {
            rows.push(#{ cells: current_row });
            current_row_num += 1;
            current_width = if current_row_num < allocs.len() { allocs[current_row_num] } else { 0 };
            current_row = [];
        }
    }
    if current_row.len() > 0 { rows.push(#{ cells: current_row }); }
    #{ title: "", sections: [#{ blocks: [#{ kind: "grid", width: table_width, rows: rows }] }] }
}
