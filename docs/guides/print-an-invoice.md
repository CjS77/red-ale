<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/print-an-invoice
title: Print an invoice
summary: Put your company's details in the book, run the Printable Invoice report for a document, and export it as a single HTML page to print or send.
related: report/printable-invoice, method/report.run, method/report.export, method/report.options, method/book.setOptions, method/book.options, method/invoice.list, method/invoice.get, gnucash/reports-menu, concept/report-scripts
from: guide/ch_reports.docbook#rpt_grp_business
---
red-ale has no print dialog. A document is printed by running the **Printable Invoice** report on it, which gives its
figures exactly, and exporting that as one HTML page. Open the page in a browser to print it, or
[turn it into a PDF](guide/report-to-pdf). The same report prints invoices, bills, credit notes and expense vouchers.

## Steps

1. **Set your company's details** once with [book.setOptions](method/book.setOptions): `companyName`, `companyAddress`,
   `companyContactPerson`, phone, fax, email, website, `companyId` and `taxNumber`. They fill the header of the printout.
2. **Find the document's GUID** with [invoice.list](method/invoice.list). The report is told which document to print by its
   GUID, not its number.
3. **Run the report** with [report.run](method/report.run), report `Printable Invoice`, and the option
   `{"General": {"Invoice Number": "<guid>"}}`. You get the report result: the entries, net price, tax, total, payments
   and amount due, as exact figures with their display forms. A document may be posted or not.
4. **Adjust the layout** if you like. [report.options](method/report.options) lists the options: which entry columns show
   (Display Columns), whether the due date, billing terms and payments print, a custom title, extra notes at the foot, and
   the CSS.
5. **Export it** with [report.export](method/report.export), with the same options. The result's `html` is a complete,
   self-contained page. From the command line, `red-ale exec --output invoice.html --book book.gnucash report.export '{...}'`
   writes it to a file instead of printing it.
6. **To print as GnuCash does,** read `defaultInvoiceReport` with [book.options](method/book.options). If the book names
   an invoice report that red-ale has, run that one; otherwise use the Printable Invoice, which is what GnuCash falls back
   to. The [Printable Invoice](report/printable-invoice) page has the details.

## Example

```example fixture=synthetic/business-editable now=2024-03-25T12:00:00Z
{"method": "book.setOptions", "params": {"companyName": "Acme Ltd", "companyAddress": "1 Main St\nSpringfield", "companyEmailAddress": "billing@acme.example"}}
{"result": {"companyName": "Acme Ltd", "…": "…"}}
{"method": "report.run", "params": {"report": "Printable Invoice", "options": {"General": {"Invoice Number": "aa210b7131de44c48b8513cd27c00e95"}}}}
{"result": {"report": {"name": "Printable Invoice", "…": "…"}, "sections": "…", "…": "…"}}
{"method": "report.export", "params": {"report": "Printable Invoice", "options": {"General": {"Invoice Number": "aa210b7131de44c48b8513cd27c00e95"}}}}
{"result": {"title": "…", "html": "…"}}
```
