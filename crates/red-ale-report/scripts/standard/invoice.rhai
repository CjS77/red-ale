// -*-scheme-*-
// invoice.scm -- an Invoice Report, used to print a GncInvoice
//
// Created by:  Derek Atkins <warlord@MIT.EDU>
// Copyright (c) 2002, 2003 Derek Atkins <warlord@MIT.EDU>
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
// Translated from 5.17:gnucash/report/reports/standard/invoice.scm, cited below by line: one business document as a
// printable page. Only the Printable Invoice is translated (design/milestone6.md, "Decided", item 2). Its defaults
// are variant-list's 'invoice entry (:342-348), written here directly, so variant-list and keylist-get-info
// (:340-375) are left out: nothing else depends on the variant.
//
// Deliberate differences (design/reports.md §14):
// - the document has no title, as GnuCash's has none; the main table (:797-869) becomes its blocks in order: the
//   title as a heading of class invoice-title, a group per layout row, the entries table and the notes;
// - the entries table declares 1 + max(3, n) columns for n shown and pads shorter rows with null cells at the end,
//   so every row covers the table, as the report result requires (GnuCash leaves rows of different widths);
// - a non-job owner reads as having no job, where GnuCash prints a null job's ID (design/notes/report-invoice.md
//   §1.3);
// - the picture's address is HTML-escaped by the renderer, where GnuCash quotes it as a Scheme string.

import "lib/business" as business;
import "lib/html-utilities" as html;

/// The template (:873, :877-885).
fn templates() {
    [#{ guid: "5123a759ceb9483abf2182d01c140e8d", name: "Printable Invoice", menuPath: ["_Business"], params: #{} }]
}

/// `base-css` (:36-45): the Layout › CSS default, which styles the page by the class names below.
fn base_css() {
    "/* advanced users only */\n" +
    ".div-align-right { float: right; }\n" +
    ".div-align-right .maybe-align-right { text-align: right }\n" +
    ".entries-table * { border-width: 1px; border-style:solid; border-collapse: collapse}\n" +
    ".entries-table > table { width: 100% }\n" +
    ".company-table > table * { padding: 0px; }\n" +
    ".client-table > table * { padding: 0px; }\n" +
    ".invoice-details-table > table * { padding: 0px; text-indent: 0.2em; }\n" +
    "@media print { .main-table > table { width: 100%; }}\n"
}

/// `layout-key-list` (:314-338) as `keylist->vectorlist` gives it (:377-383): each key and its text, no help.
fn layout_choices() {
    [
        ["client", "Client or vendor name, address and ID"],
        ["company", "Company name, address and tax-ID"],
        ["invoice", "Invoice date, due date, billing ID, terms, job details"],
        ["today", "Today's date"],
        ["picture", "Picture"],
        ["none", "Empty space"],
    ]
}

/// The Display Columns options in `build-column-used`'s order (:73-85), each with its heading
/// (`make-heading-list`, :87-105), key, help, default and the role of its column.
fn column_specs() {
    [
        #{ name: "Date", heading: "Date", key: "b", help: "Display the date?", on: true, role: "date" },
        #{ name: "Description", heading: "Description", key: "d", help: "Display the description?", on: true, role: "text" },
        #{ name: "Action", heading: "Action", key: "g", help: "Display the action?", on: true, role: "text" },
        #{ name: "Quantity", heading: "Quantity", key: "ha", help: "Display the quantity of items?", on: true, role: "amount" },
        #{ name: "Price", heading: "Unit Price", key: "hb", help: "Display the price per item?", on: true, role: "amount" },
        #{ name: "Discount", heading: "Discount", key: "k", help: "Display the entry's discount?", on: true, role: "amount" },
        #{ name: "Taxable", heading: "Taxable", key: "l", help: "Display the entry's taxable status?", on: true, role: "text" },
        #{ name: "Tax Amount", heading: "Tax Amount", key: "m", help: "Display each entry's total total tax?", on: false,
           role: "amount" },
        #{ name: "Total", heading: "Total", key: "n", help: "Display the entry's value?", on: true, role: "amount" },
    ]
}

/// `options-generator` (:385-555) with the `'invoice` variant's defaults.
fn options(params, o, book) {
    o.invoice("General", "Invoice Number", "x", "", ());
    o.string("General", "Custom Title", "z", "A custom string to replace Invoice, Bill or Expense Voucher.", "");
    o.text("Layout", "CSS", "zz",
           "CSS code. This field specifies the CSS code for styling the invoice. Please see the exported report for the CSS class names.",
           base_css());
    o.image("Layout", "Picture Location", "zy", "Location for Picture", "");
    for c in column_specs() {
        o.boolean("Display Columns", c.name, c.key, c.help, c.on);
    }
    o.boolean("Display", "Due Date", "c", "Display due date?", true);
    o.boolean("Display", "Subtotal", "d", "Display the subtotals?", true);
    // The callbacks only make the phrases selectable in GnuCash's dialog (:453-458, :465-469).
    o.complex_boolean("Display", "Payable to", "ua1", "Display the Payable to: information.", false, ());
    o.text("Display", "Payable to string", "ua2", "The phrase for specifying to whom payments should be made.",
           "Please make all checks payable to");
    o.complex_boolean("Display", "Company contact", "ub1", "Display the Company contact information.", false, ());
    o.text("Display", "Company contact string", "ub2", "The phrase used to introduce the company contact.",
           "Please direct all enquiries to");
    o.number_range("Display", "Minimum # of entries", "zz", "The minimum number of invoice entries to display.", 1, 0, 23, 1);
    o.boolean("Display", "Use Detailed Tax Summary", "o",
              "Display all tax categories separately (one per line) instead of one single tax line?", false);
    o.boolean("Display", "References", "s", "Display the invoice references?", true);
    o.boolean("Display", "Billing Terms", "t", "Display the invoice billing terms?", true);
    o.boolean("Display", "Billing ID", "ta", "Display the billing id?", true);
    o.boolean("Display", "Invoice owner ID", "tam", "Display the customer/vendor id?", false);
    o.boolean("Display", "Invoice Notes", "tb", "Display the invoice notes?", false);
    o.boolean("Display", "Payments", "tc", "Display the payments applied to this invoice?", true);
    o.boolean("Display", "Job Details", "td", "Display the job name for this invoice?", false);
    o.text("Display", "Extra Notes", "u", "Extra notes to put on the invoice.", "Thank you for your patronage!");
    let layout = [
        ["Row 1 Left", "1a", "1st row, left", "none"],
        ["Row 1 Right", "1b", "1st row, right", "invoice"],
        ["Row 2 Left", "2a", "2nd row, left", "client"],
        ["Row 2 Right", "2b", "2nd row, right", "company"],
        ["Row 3 Left", "3a", "3rd row, left", "none"],
        ["Row 3 Right", "3b", "3rd row, right", "today"],
    ];
    for l in layout {
        o.multichoice("Layout", l[0], l[1], l[2], l[3], layout_choices());
    }
    o.set_default_section("General");
}

/// A row of a layout table, which GnuCash gives no row style.
fn plain_row(cells) {
    #{ alternate: false, cells: cells }
}

/// A layout table: one or two unlabelled text columns, so no header, inside a `div` of `class`.
fn layout_table(class, width, rows) {
    let columns = [];
    for i in 0..width {
        columns.push(#{ label: "", role: "text" });
    }
    #{ kind: "table", class: class, columns: columns, rows: rows }
}

/// The orders of the document's entries, each once, in the reverse of the order their first entries appear: GnuCash
/// folds `cons` over the entries (:113-118). An entry with no order adds `()`.
fn orders_of(invoice) {
    let seen = [];
    let orders = [];
    for e in invoice.entries() {
        let order = e.order;
        let key = if type_of(order) == "()" { "" } else { order.guid };
        if !seen.contains(key) {
            seen.push(key);
            orders.insert(0, order);
        }
    }
    orders
}

/// `make-client-table` (:111-161): the owner's name and address, its ID when asked, and a `REF` row per order with a
/// reference.
fn make_client_table(invoice, opts) {
    let owner = invoice.owner;
    let rows = [
        plain_row([#{ text: business::owner_get_name_dep(owner), class: "maybe-align-right client-name" }]),
        plain_row([#{ text: business::owner_get_address_dep(owner), class: "maybe-align-right client-address" }]),
    ];
    if opts.get("Display", "Invoice owner ID") {
        rows.push(plain_row([#{ text: business::owner_get_owner_id(owner), class: "maybe-align-right client-id" }]));
    }
    if opts.get("Display", "References") {
        for order in orders_of(invoice) {
            if type_of(order) != "()" && order.reference != "" {
                rows.push(plain_row([#{ text: "REF " + order.reference }]));
            }
        }
    }
    layout_table("client-table", 1, rows)
}

/// `make-company-table` (:163-220): one row per company detail the Book has, in this order.
fn make_company_table(book) {
    let details = [
        [business::company_info(book, "Company Name"), "company-name"],
        [business::company_info(book, "Company Address"), "company-address"],
        [business::company_info(book, "Company Phone Number"), "company-phone"],
        [business::company_info(book, "Company Fax Number"), "company-fax"],
        [business::company_info(book, "Company Email Address"), "company-email"],
        [business::company_info(book, "Company Website URL"), "company-url"],
        [business::company_info(book, "Company ID"), "company-tax-id"],
        [business::book_string(book, "Tax/Tax Number"), "company-tax-nr"],
    ];
    let rows = details
        .filter(|d| d[0] != "")
        .map(|d| plain_row([#{ text: d[0], class: "maybe-align-right " + d[1] }]));
    layout_table("company-table", 1, rows)
}

/// `make-date-row` (:222-228): the label and the date in the Book's Fancy Date Format (`gnc:options-fancy-date`,
/// `5.17:bindings/guile/options.scm:403-407`), or as `qof-print-date` prints it when the Book has none.
fn make_date_row(label, date, book, ctx) {
    let format = book.option("Business/Fancy Date Format/custom");
    let text = if type_of(format) == "string" { ctx.print_time(date, format) } else { ctx.print_date(date) };
    plain_row([#{ text: label + ":" }, #{ text: text, class: "div-align-right" }])
}

/// `make-invoice-details-table` (:230-298): the dates or "Invoice in progress…", the billing ID, the terms and the
/// job.
fn make_invoice_details_table(invoice, book, opts, ctx) {
    let rows = [];
    if invoice.posted {
        rows.push(make_date_row("Date", invoice.datePosted, book, ctx));
        if opts.get("Display", "Due Date") {
            rows.push(make_date_row("Due Date", invoice.dateDue, book, ctx));
        }
    } else {
        rows.push(plain_row([#{ text: "Invoice in progress…", span: 2, class: "invoice-in-progress" }]));
    }
    let billing_id = invoice.billingId;
    if opts.get("Display", "Billing ID") && billing_id != "" {
        rows.push(plain_row([#{ text: "Reference:" }, #{ text: billing_id, class: "div-align-right" }]));
        // GnuCash appends an empty row after it (:270).
        rows.push(#{ role: "spacer", cells: [(), ()] });
    }
    let term = invoice.terms;
    let terms = if type_of(term) == "()" { "" } else { term.description };
    if opts.get("Display", "Billing Terms") && terms != "" {
        rows.push(plain_row([#{ text: "Terms:" }, #{ text: terms, class: "div-align-right" }]));
    }
    // Only a job's document has a job (`gncOwnerGetJob`, `5.17:libgnucash/engine/gncOwner.c:376-381`).
    let owner = invoice.owner;
    if opts.get("Display", "Job Details") && owner.kind == "job" && owner.id != "" {
        rows.push(plain_row([#{ text: "Job number:" }, #{ text: owner.id, class: "div-align-right" }]));
        rows.push(plain_row([#{ text: "Job name:" }, #{ text: owner.name, class: "div-align-right" }]));
    }
    layout_table("invoice-details-table", 2, rows)
}

/// `make-file-url` (`5.17:gnucash/report/report-utilities.scm:1028-1031`): `file:///` before anything that does not
/// already start with it.
fn make_file_url(url) {
    if url.starts_with("file:///") { url } else { "file:///" + url }
}

/// The block a Layout choice renders (`layout-key-list`, :314-338): `none` is an empty text block.
fn layout_block(key, invoice, book, opts, ctx) {
    switch key {
        "client" => make_client_table(invoice, opts),
        "company" => make_company_table(book),
        "invoice" => make_invoice_details_table(invoice, book, opts, ctx),
        // `make-today` (:310-312).
        "today" => #{ kind: "text", text: ctx.print_date(ctx.now), class: "invoice-print-date" },
        // `make-picture` (:300-308).
        "picture" => #{ kind: "image", src: make_file_url(opts.get("Layout", "Picture Location")), class: "picture" },
        _ => #{ kind: "text", text: "" },
    }
}

/// A row of the main table (:804-820): the left choice, then the right one inside `div.div-align-right`.
fn layout_row(row, invoice, book, opts, ctx) {
    let left = layout_block(opts.get("Layout", `Row ${row} Left`), invoice, book, opts, ctx);
    let right = layout_block(opts.get("Layout", `Row ${row} Right`), invoice, book, opts, ctx);
    right.class = if "class" in right { "div-align-right " + right.class } else { "div-align-right" };
    #{ kind: "group", blocks: [left, right] }
}

/// A figure of a total row: `display-subtotal` (:570-578) prints it positive when the Total column is hidden.
fn display_subtotal(amount, show_value) {
    if !show_value && amount < 0 { -amount } else { amount }
}

/// The cells of one entry (`add-entry-row`, :612-668) in the shown columns.
fn entry_cells(e, used, currency, cust_doc) {
    let cells = [];
    if used[0] { cells.push(#{ date: e.date }); }
    if used[1] { cells.push(#{ text: e.description }); }
    if used[2] { cells.push(#{ text: e.action }); }
    // `xaccPrintAmount` with `gnc-default-print-info #f` (:629-631).
    if used[3] { cells.push(#{ number: e.documentQuantity, numberFormat: "exact" }); }
    if used[4] { cells.push(#{ price: if cust_doc { e.invoicePrice } else { e.billPrice }, currency: currency }); }
    if used[5] {
        // `monetary-or-percent` (:106-109); a bill's discount is left empty.
        cells.push(if !cust_doc {
            #{ text: "" }
        } else if e.discountType == "percent" {
            #{ number: e.discount, numberFormat: "exact", suffix: " %" }
        } else {
            #{ monetary: monetary(currency, e.discount) }
        });
    }
    // Only the taxable flag decides (design/notes/report-invoice.md, "Facts that surprise", 4).
    if used[6] { cells.push(#{ text: if (if cust_doc { e.invoiceTaxable } else { e.billTaxable }) { "T" } else { "" } }); }
    if used[7] { cells.push(#{ monetary: monetary(currency, e.documentTaxValue) }); }
    if used[8] { cells.push(#{ monetary: monetary(currency, e.documentValue) }); }
    cells
}

/// `cells` padded with empty cells to `width` columns.
fn padded(cells, width) {
    while cells.len() < width {
        cells.push(());
    }
    cells
}

/// The payments of the posted lot (:723-736): its splits in the order of their transactions (`xaccTransOrder`, a
/// stable sort), less the posting transaction's, each amount negated unless the document's amount is positive.
fn payments_of(invoice) {
    let lot = invoice.postedLot;
    if type_of(lot) == "()" {
        return [];
    }
    let posting = invoice.postedTransaction.guid;
    // `gncInvoiceAmountPositive` (`5.17:libgnucash/engine/gncInvoice.c:1287-1306`): a customer invoice, or a vendor or
    // employee credit note.
    let positive = invoice.isCustomerDocument != invoice.isCreditNote;
    let splits = lot.splits();
    splits.sort(|a, b| a.transaction.order(b.transaction));
    splits
        .filter(|s| s.transaction.guid != posting)
        .map(|s| #{ date: s.transaction.datePosted, amount: if positive { s.amount } else { -s.amount } })
}

/// `make-entry-table` (:558-750): the entries, padding rows up to "Minimum # of entries", the totals, the payments
/// and the amount due.
fn make_entry_table(invoice, opts, cust_doc) {
    let used = column_specs().map(|c| opts.get("Display Columns", c.name));
    let shown = used.filter(|u| u).len();
    let span = if shown > 3 { shown } else { 3 };
    let width = 1 + span;
    let currency = invoice.currency;
    let columns = [];
    for (c, i) in column_specs() {
        if used[i] { columns.push(#{ label: c.heading, role: c.role }); }
    }
    while columns.len() < width {
        columns.push(#{ label: "", role: "text" });
    }

    let rows = [];
    let alternate = false;
    for e in invoice.entries() {
        rows.push(#{ alternate: alternate, cells: padded(entry_cells(e, used, currency, cust_doc), width) });
        alternate = !alternate;
    }
    // The minimum number of entries (:692-699): a fractional minimum pads to its ceiling.
    let left = opts.get("Display", "Minimum # of entries") - rows.len();
    while left > 0 {
        rows.push(#{ alternate: alternate, cells: padded([], width) });
        alternate = !alternate;
        left -= 1;
    }

    let show_value = used[8];
    let total_row = |label, amount| #{ role: "grandTotal", cells: [
        #{ text: label }, #{ monetary: monetary(currency, display_subtotal(amount, show_value)), span: span }] };
    let totals = invoice.totals();
    if opts.get("Display", "Subtotal") {
        rows.push(total_row.call("Net Price", totals.subtotal));
    }
    if opts.get("Display", "Use Detailed Tax Summary") {
        for t in totals.taxes {
            rows.push(total_row.call(t.account.name, t.value));
        }
    } else {
        rows.push(total_row.call("Tax", totals.tax));
    }
    rows.push(total_row.call("Total Price", totals.total));

    let due = totals.total;
    if opts.get("Display", "Payments") {
        for p in payments_of(invoice) {
            due += p.amount;
            // `add-payment-row` (:580-606): one column short of the totals, padded at the end.
            let cells = [];
            if used[0] { cells.push(#{ date: p.date }); }
            if used[1] { cells.push(#{ text: "Payment, thank you!" }); }
            cells.push(#{ monetary: monetary(currency, display_subtotal(p.amount, show_value)), span: span - cells.len() });
            cells.push(());
            rows.push(#{ role: "grandTotal", cells: cells });
        }
    }
    rows.push(total_row.call("Amount Due", due));
    #{ kind: "table", class: "entries-table", columns: columns, rows: rows }
}

/// A text block of the main table inside a `div` of `class`.
fn note(text, class) {
    #{ kind: "text", text: text, class: class }
}

/// `reg-renderer` (:753-871): the no-invoice warning, or the main table's rows as blocks.
fn render(params, book, opts, ctx) {
    let invoice = opts.get("General", "Invoice Number");
    if type_of(invoice) == "()" {
        let warning = html::make_generic_warning("Invoice", "",
            "No valid invoice selected. Click on the Options button and select the invoice to use.");
        return #{ title: "", sections: [#{ blocks: [warning] }] };
    }
    let default_title = switch invoice.type {
        "bill" => "Bill",
        "expenseVoucher" => "Expense Voucher",
        "creditNote" => "Credit Note",
        _ => "Invoice",
    };
    let custom_title = opts.get("General", "Custom Title");
    let title = if custom_title == "" { default_title } else { custom_title };

    let blocks = [#{ kind: "text", role: "heading", text: `${title} #${invoice.id}`, class: "invoice-title" }];
    for row in [1, 2, 3] {
        blocks.push(layout_row(row, invoice, book, opts, ctx));
    }
    blocks.push(make_entry_table(invoice, opts, invoice.isCustomerDocument));
    if opts.get("Display", "Invoice Notes") {
        blocks.push(note(invoice.notes, "invoice-notes"));
    }
    let name = business::company_info(book, "Company Name");
    if opts.get("Display", "Payable to") && name != "" {
        blocks.push(note(`${opts.get("Display", "Payable to string")}: ${name}`, "invoice-footer-payable-to"));
    }
    let contact = business::company_info(book, "Company Contact Person");
    if opts.get("Display", "Company contact") && contact != "" {
        blocks.push(note(`${opts.get("Display", "Company contact string")}: ${contact}`, "invoice-footer-company-contact"));
    }
    blocks.push(note(opts.get("Display", "Extra Notes"), "invoice-notes"));
    #{ title: "", css: opts.get("Layout", "CSS"), sections: [#{ blocks: blocks }] }
}
