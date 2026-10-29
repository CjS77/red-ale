<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/printable-invoice
related: method/report.run, method/report.export, method/report.options, method/book.options, method/book.setOptions, method/invoice.get
---
Prints one business document, an invoice, bill, credit note or expense voucher, as GnuCash's Printable Invoice
(Reports › Business › Printable Invoice) does: its title and number, the document's dates, billing ID and terms, the
customer's or vendor's address, your company's details, one row per entry (date, description, action, quantity,
unit price, discount, taxable, total) and the net price, tax, total price, payments and amount due. Use it to print
or send a document; [report.export](method/report.export) gives the same report as one HTML page.

The document is the **General › Invoice Number** option: its GUID, as [invoice.list](method/invoice.list) and
[invoice.get](method/invoice.get) give it. Without one, or with a GUID that names no document, the report shows
GnuCash's warning "No valid invoice selected" and nothing else. Figures are exact: quantities, prices and discounts
are never rounded for display, and amounts are in the document's currency.

## Key options

- **General › Custom Title**: replaces "Invoice", "Bill" or "Expense Voucher" in the heading.
- **Display Columns**: which entry columns show. All but Tax Amount are on.
- **Display**: Due Date, References, Billing Terms, Billing ID, Payments, Invoice Notes, Job Details, the Extra Notes
  printed at the foot, the "Payable to" and "Company contact" lines, and the Minimum # of entries (blank rows are
  added up to it).
- **Layout › Row 1 to 3 Left and Right**: what each of the six header cells holds: the client, the company, the
  invoice details, today's date, the picture or nothing.
- **Layout › Picture Location**: a picture for the header, as a path or `file:///` URL. It goes into the page as a
  `file:///` URL, as GnuCash writes it; red-ale never reads the file, so the picture shows only where the page is
  opened on a machine that has it.
- **Layout › CSS**: the CSS of the page. Blocks and cells carry GnuCash's class names (`entries-table`,
  `invoice-title`, `client-name`, …), so the layout can be changed by class.

Your company's name, address, ID, contact, phone, fax, email, URL and tax number come from the Book's Business
options: set them with [book.setOptions](method/book.setOptions).

## Printing an invoice as GnuCash does

GnuCash's Print Invoice runs the Book's **Default Invoice Report** for the document. To do the same:

1. Read `defaultInvoiceReport` with [book.options](method/book.options).
2. If it names an invoice report red-ale has, run that report. An invoice report is a template whose General ›
   Invoice Number option has type `invoice` ([report.options](method/report.options) shows it), or a saved
   configuration of one; `report.list` lists them.
3. Otherwise (no default, or one naming a report red-ale doesn't have, such as GnuCash's Easy or Fancy Invoice) run
   the Printable Invoice, as GnuCash falls back to it.
4. Pass the document as `{"General": {"Invoice Number": "<guid>"}}` to [report.run](method/report.run) or
   [report.export](method/report.export).

## Examples

The first invoice of a fixture book: one entry of 2 at $3.00, so $6.00 due.

```example fixture=reports/transaction
{"method": "report.run", "params": {"report": "Printable Invoice", "options": {"General": {"Invoice Number": "30000000000000000000000000000000"}}}}
{"result": {"report": {"guid": "5123a759ceb9483abf2182d01c140e8d", "name": "Printable Invoice", "configuration": null}, "title": "", "sections": [{"blocks": [{"kind": "text", "text": "Invoice #0001", "role": "heading", "class": "invoice-title"}, "…", "…", "…", {"kind": "table", "class": "entries-table", "rows": [{"cells": [{"date": "2024-03-15T12:00:00Z", "…": "…"}, {"text": "entry-1-desc"}, {"text": "entry-1-action"}, {"number": {"num": "2", "denom": "1"}, "…": "…"}, {"price": {"num": "3", "denom": "1"}, "currency": "CURRENCY::USD", "…": "…"}, "…"], "…": "…"}, "…", "…", "…", {"role": "grandTotal", "cells": [{"text": "Amount Due"}, {"monetary": {"amount": {"num": "6", "denom": "1"}, "commodity": "CURRENCY::USD"}, "…": "…"}]}], "…": "…"}, "…"], "…": "…"}], "…": "…"}}
```

No document chosen gives GnuCash's warning:

```example fixture=reports/transaction
{"method": "report.run", "params": {"report": "Printable Invoice"}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "warning", "title": "Invoice:", "text": "No valid invoice selected. Click on the Options button and select the invoice to use."}], "…": "…"}], "…": "…"}}
```
