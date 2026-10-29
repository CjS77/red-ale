<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/not-in-red-ale
title: Not in red-ale
summary: What GnuCash does that red-ale does not, and what to do instead - the GUI, online banking, OFX and the AqBanking formats, Finance::Quote, PDF export, MySQL and PostgreSQL books, the Python and Guile interfaces, and the business and tax reports that are not ported.
related: concept/report-scripts, concept/coming-from-gnucash, concept/importing-and-matching, gnucash/stock-assistants, gnucash/reports-menu, gnucash/import-and-export, error/sql-book, method/report.export, method/price.fetchQuotes, guide/set-up-accounts, guide/record-prices
from: guide/ch_python_bindings.docbook#ch_python_bindings
---
red-ale aims at GnuCash's behaviour on your file, driven by a program. Anything that is a window, needs a bank or a
service outside red-ale, or is too narrow to carry is left out. Your file is not affected: anything in the book that
red-ale does not use is kept and written back unchanged. This page lists what is left out and what to use instead.

## The graphical interface

| GnuCash | Instead |
|---|---|
| Windows, tabs, toolbars, summary bars | The methods ([Windows and registers](gnucash/windows-and-registers)) |
| Account registers and their styles, autocompletion | [`transaction.list`](method/transaction.list), [`transaction.post`](method/transaction.post) ([Transaction](gnucash/transaction-menu)) |
| Window state file (`.gcm`), Tip of the Day, Reset Warnings | None |
| Check printing and check formats | None. Export the data and print cheques with another tool |
| New Account Hierarchy assistant, Account templates | Build the tree with `account.create`, or import it from CSV ([File menu](gnucash/file-menu)) |
| Financial Calculator, Loan Repayment Calculator | [Loans and interest](concept/loans-and-interest) gives the formula; [`scheduledTransaction.loan`](method/scheduledTransaction.loan) works out a loan's payment |
| Stock Transaction assistant | Each of its transactions is a [`transaction.post`](method/transaction.post) ([Stock assistants](gnucash/stock-assistants)) |
| Autosave | None. Call [`book.save`](method/book.save) when you want the file written |

## Banks and files from banks

Online banking through AqBanking (HBCI, FinTS, SEPA, Get Balance, Get Transactions), the Online Banking preferences and
setup, OFX and QFX import, and the AqBanking file formats (MT940, MT942, MT535, DTAUS, CAMT 052, Q45, Swiss YellowNet,
Olympic ERI) are not supported. Convert a statement to QIF or CSV with another tool, then
[import it](gnucash/import-and-export).

## Online quotes

GnuCash fetches quotes through Finance::Quote, a Perl library. red-ale does not use it. It has its own quote services,
off until you turn on `quotes.network`, and [`price.fetchQuotes`](method/price.fetchQuotes) fetches into the price
database ([Prices and exchange rates](concept/prices-and-exchange-rates)). A Commodity's quote source in the book is kept.

## Reports

- **PDF export.** [`report.export`](method/report.export) writes HTML. Turn it into PDF with `pandoc` or any
  HTML-to-PDF converter.
- **Business reports.** The Customer, Vendor, Employee, Owner and Job reports, Receivable and Payable Aging, the Customer
  Summary, the Easy, Fancy and Tax invoices and the Receipt are not ported. The Printable Invoice is
  ([Business](gnucash/business-menu)).
- **Tax and regional reports.** The US income tax report and TXF export, the German income tax report and the Income and
  GST Statement.
- **Sample and other reports.** Hello World, Sample Graphs, the Lot Viewer, the Register report, and the eguile Balance
  Sheet (the [Balance Sheet](report/balance-sheet) gives its figures).

Where you need one of these, write it as a User report: a short script in red-ale's report language, run with
[`report.run`](method/report.run) like the built-in ones. See [Report scripts](concept/report-scripts).

## Books on a server

MySQL, MariaDB and PostgreSQL books are not opened: [`book.open`](method/book.open) refuses them with
[`sql-book`](error/sql-book) before it touches anything. In GnuCash, use **File > Save As...** to save the book as SQLite
or XML, then open that file. XML and SQLite books are supported.

## Programming interfaces

| GnuCash | Instead |
|---|---|
| Python bindings and the REST API example built on them | JSON-RPC, MCP, or `red-ale exec` ([Coming from GnuCash](concept/coming-from-gnucash)) |
| Guile scripting and Scheme reports | Reports are red-ale's own scripts; red-ale never runs Guile ([Report scripts](concept/report-scripts)) |
| `gnucash-cli`: quotes, reports from the command line | `red-ale exec` with `price.fetchQuotes`, `report.run` and the other methods |

## Book currency

GnuCash has a book currency flag that upstream never implemented. red-ale does not implement it either.
