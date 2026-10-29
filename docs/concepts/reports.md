<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/reports
title: Reports
summary: Report templates and their options, the report result and its HTML export, style sheets, saved report configurations and the Multicolumn View - what red-ale ports from GnuCash, and what you write yourself as a User report.
related: concept/report-scripts, concept/budgets, method/report.list, method/report.options, method/report.run, method/report.export, method/report.exportCsv, method/report.saveConfiguration, method/report.styleSheets, guide/run-and-export-a-report
from: guide/ch_reports.docbook#rpt_concepts
---
A **report** is a read-only view of your book: balances, income and spending, the holdings of your investments, a
single invoice. Running a report never changes the book, never marks it dirty and works on a book that is open
read-only.

## Templates and options

A **report template** is a report definition with a GUID, a name and *options*. The built-in templates keep GnuCash 5.17's
names and GUIDs. **⚠** GnuCash also calls saved configurations "custom reports"; red-ale says *saved report
configuration* for those (below).

- [`report.list`](method/report.list) lists every template and saved configuration, with `kind` `builtin`, `user` or
  `saved`. It needs no book.
- [`report.options`](method/report.options) lists a report's *options*: each is named by a section and name (GnuCash's
  English strings, such as *General* / *Start Date*), has a type, a default and the current value, plus a JSON Schema for
  the `options` of `report.run`. Options come in GnuCash's order.
- Report options go in `options` as `{ "<section>": { "<name>": <value> } }`, changing only what differs from the defaults.
  Dates may be absolute or *relative* (`start-this-month`, `end-prev-quarter`). Accounts are given by GUID or full name.

Every report has GnuCash's three General options: *Report name*, *Stylesheet* and *Add options summary*.

## Running and the report result

[`report.run`](method/report.run) runs a report and returns the **report result**: a document of sections made of tables,
charts and text, with every figure exact (a `{num, denom}` rational and its display form) and roles instead of styles.
It is the published language for reports: an agent reads figures from it, a renderer draws it. Pass `now` to run the
report as of another time, and `locale` to format numbers and dates in another Locale; the same book, options, time and
settings give the same bytes.

Amounts in reports are exact rationals, and rounding to the Commodity's smallest unit happens only when a figure is
displayed. GnuCash computes many reports in floating point; red-ale does not, so a report may differ from GnuCash's in the
last cent where GnuCash's own sum drifts.

## Exporting

- [`report.export`](method/report.export) runs a report and returns it as a single HTML document, rendered through a style
  sheet: self-contained (no script, no outside resource), the same bytes for the same inputs. Nothing is written to disk:
  the CLI's `--output` saves it on your side.
- [`report.exportCsv`](method/report.exportCsv) returns the data of the net-worth and category charts as CSV text.
- **There is no PDF export.** Turn the exported HTML into PDF with a general tool such as pandoc or an HTML-to-PDF
  converter.

## Style sheets

A **style sheet** is a named set of presentation options, applied when a result is rendered as HTML. red-ale has GnuCash's
six predefined style sheets: *Default*, *CSS-based stylesheet (experimental)*, *Easy*, *Technicolor*, *Footer* and
*Head or Tail*. [`report.styleSheets`](method/report.styleSheets) lists them with their options;
[`report.createStyleSheet`](method/report.createStyleSheet), `report.updateStyleSheet` and `report.deleteStyleSheet` manage
your own. Changing one changes every report that uses it. Images in style sheets (a background picture, a banner or logo)
are not rendered.

## Saved report configurations

Once you have a report set up as you like it, save it: [`report.saveConfiguration`](method/report.saveConfiguration)
stores its options, only those that differ from the template's, under a new name and GUID in the Configuration
([The Configuration](concept/configuration)). A saved configuration runs and exports like any report;
[`report.updateConfiguration`](method/report.updateConfiguration), `report.renameConfiguration` and
`report.deleteConfiguration` change it. [`report.importConfigurations`](method/report.importConfigurations) and
`report.importStyleSheets` read GnuCash's own `saved-reports-2.8` and `stylesheets-2.0` files, which you give as text.

A **Multicolumn View** ([`report/multicolumn-view`](report/multicolumn-view)) puts several reports on one page; its children
are templates or saved configurations.

## What is built in

red-ale ports the reports GnuCash's own work relies on. They are in the groups the GnuCash Reports menu has.

- **Assets and liabilities**: [Account Summary](report/account-summary), [Balance Sheet](report/balance-sheet),
  [Multicolumn Balance Sheet](report/balance-sheet-multicolumn), [Advanced Portfolio](report/advanced-portfolio),
  [Investment Portfolio](report/investment-portfolio), [Average Balance](report/average-balance),
  [Net Worth Barchart](report/net-worth-barchart) and [Linechart](report/net-worth-linechart), [Assets](report/assets) and
  [Liabilities](report/liabilities) pie charts, [Securities](report/securities), [Price Scatterplot](report/price-scatterplot).
- **Income and expense**: [Income Statement](report/income-statement) (also called Profit & Loss), its
  [multicolumn](report/income-statement-multicolumn) form, [Cash Flow](report/cash-flow), [Equity Statement](report/equity-statement),
  [Trial Balance](report/trial-balance), [Income and Expense charts](report/income-expense-chart), [Income](report/income-accounts)
  and [Expense](report/expense-accounts) pies, the [over-time](report/income-over-time) and
  [day-of-week](report/income-vs-day-of-week) charts.
- **Transactions**: [Transaction Report](report/transaction-report), [General Journal](report/general-journal),
  [General Ledger](report/general-ledger), [Transaction Breakdown](report/transaction-breakdown-report),
  [Reconciliation Report](report/reconciliation-report).
- **Budget**: [Budget Report](report/budget-report), [Budget Flow](report/budget-flow), [Budget Chart](report/budget-chart),
  [Budget Income Statement](report/budget-income-statement) and [Budget Balance Sheet](report/budget-balance-sheet)
  ([Budgets](concept/budgets)).
- **Scheduled transactions**: [Balance Forecast](report/balance-forecast),
  [Future Scheduled Transactions Summary](report/future-scheduled-transactions-summary)
  ([Scheduled transactions](concept/scheduled-transactions)).
- **Investments**: [Investment Lots](report/investment-lots), [IFRS cost basis](report/ifrs-weighted-average-cost-basis-report)
  ([Lots and capital gains](concept/lots-and-capital-gains)).
- **Business**: the [Printable Invoice](report/printable-invoice) ([Business documents](concept/business-documents)).
- **Multicolumn**: the [Multicolumn View](report/multicolumn-view) and the [Dashboard](report/dashboard), a built-in saved
  configuration of it.

GnuCash's other business reports, its tax reports, the Easy and Fancy invoices, the Hello World and Sample Graphs examples
and the eguile Balance Sheet are not ported. The Balance Sheet above gives the figures of the last. Write anything else
you need as a [User report](concept/report-scripts). To find a GnuCash report here, see
[From GnuCash: Reports menu](gnucash/reports-menu).
