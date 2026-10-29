<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/reports-menu
title: Reports menu and the report window
summary: GnuCash's Reports menu mapped to report names for report.run, and the report window's File and Edit menus mapped to report.saveConfiguration, report.export and report.styleSheets. The business and tax reports that are not ported are written as User reports.
related: concept/reports, concept/report-scripts, gnucash/business-menu, gnucash/not-in-red-ale, method/report.run, method/report.export, method/report.saveConfiguration, method/report.styleSheets, report/dashboard
from: manual/ch_GUIMenus.docbook#AccTree-reports-menu
---
Every GnuCash report is a menu entry that opens a window. In red-ale a report is a name and some options given to
[`report.run`](method/report.run), which returns the figures. [`report.list`](method/report.list) lists the names and
[`report.options`](method/report.options) the options of one. See [Reports](concept/reports).

## Assets and liabilities, income and expense

| GnuCash | red-ale |
|---|---|
| Assets & Liabilities > Balance Sheet | [Balance Sheet](report/balance-sheet) |
| Assets & Liabilities > Balance Sheet (Multicolumn) | [Balance Sheet (Multicolumn)](report/balance-sheet-multicolumn) |
| Assets & Liabilities > Net Worth Barchart, Net Worth Linechart | [Net Worth Barchart](report/net-worth-barchart), [Net Worth Linechart](report/net-worth-linechart) |
| Assets & Liabilities > Asset Chart, Asset Piechart | [Assets over time](report/assets-over-time), [Assets](report/assets) |
| Assets & Liabilities > Liability Chart, Liability Piechart | [Liabilities over time](report/liabilities-over-time), [Liabilities](report/liabilities) |
| Assets & Liabilities > Advanced Portfolio | [Advanced Portfolio](report/advanced-portfolio) |
| Assets & Liabilities > Investment Portfolio | [Investment Portfolio](report/investment-portfolio) |
| Assets & Liabilities > Investment Lots | [Investment Lots](report/investment-lots) |
| Assets & Liabilities > IFRS Weighted Average Cost Basis | [IFRS weighted average cost basis](report/ifrs-weighted-average-cost-basis-report) |
| Assets & Liabilities > Average Balance | [Average Balance](report/average-balance) |
| Assets & Liabilities > Price Scatterplot | [Price Scatterplot](report/price-scatterplot) |
| Assets & Liabilities > Security Piechart | [Securities](report/securities) |
| Income & Expense > Profit & Loss | [Profit & Loss](report/profit-loss) |
| Income & Expense > Income Statement | [Income Statement](report/income-statement) |
| Income & Expense > Income Statement (Multicolumn) | [Income Statement (Multicolumn)](report/income-statement-multicolumn) |
| Income & Expense > Cash Flow, Cash Flow Barchart | [Cash Flow](report/cash-flow), [Cash Flow Barchart](report/cash-flow-barchart) |
| Income & Expense > Income Chart, Income Piechart | [Income over time](report/income-over-time), [Income accounts](report/income-accounts) |
| Income & Expense > Expense Chart, Expense Piechart | [Expense over time](report/expense-over-time), [Expense accounts](report/expense-accounts) |
| Income & Expense > Income & Expense Barchart, Linechart | [Income & Expense chart](report/income-expense-chart), [Income & Expense linechart](report/income-expense-linechart) |
| Income & Expense > Income vs. Day of Week, Expenses vs. Day of Week | [Income vs day of week](report/income-vs-day-of-week), [Expenses vs day of week](report/expenses-vs-day-of-week) |

## Budget, transaction and other reports

| GnuCash | red-ale |
|---|---|
| Budget > Budget Report | [Budget Report](report/budget-report) |
| Budget > Budget Balance Sheet, Budget Income Statement | [Budget Balance Sheet](report/budget-balance-sheet), [Budget Income Statement](report/budget-income-statement) |
| Budget > Budget Profit & Loss | [Budget Profit & Loss](report/budget-profit-loss) |
| Budget > Budget Chart, Budget Flow | [Budget chart](report/budget-chart), [Budget flow](report/budget-flow) |
| Transaction Report | [Transaction Report](report/transaction-report) |
| Transaction Report > Transaction Breakdown | [Transaction Breakdown](report/transaction-breakdown-report) |
| General Journal | [General Journal](report/general-journal) |
| General Ledger | [General Ledger](report/general-ledger) |
| Trial Balance | [Trial Balance](report/trial-balance) |
| Equity Statement | [Equity Statement](report/equity-statement) |
| Account Summary | [Account Summary](report/account-summary) |
| Reconciliation Report | [Reconciliation Report](report/reconciliation-report) |
| Balance Forecast | [Balance Forecast](report/balance-forecast) |
| Future Scheduled Transactions Summary | [Future Scheduled Transactions Summary](report/future-scheduled-transactions-summary) |
| Custom Multicolumn Report | [Multicolumn View](report/multicolumn-view) |
| Dashboard | [Dashboard](report/dashboard), a saved configuration built in |

## Business reports

| GnuCash | red-ale |
|---|---|
| Print Invoice (File menu of an invoice) | [`report.run`](method/report.run) with the Book's **Default Invoice Report**, else the [Printable Invoice](report/printable-invoice). Set its **Invoice Number** option to the document's GUID: `{"General": {"Invoice Number": "<guid>"}}`. Details in [Business](gnucash/business-menu) |
| Business > Customer Report, Vendor Report, Employee Report, Job Report | Not ported. Write them as a [User report](concept/report-scripts) |
| Business > Receivable Aging, Payable Aging | Not ported: a [User report](concept/report-scripts) |
| Business > Customer Summary | Not ported: a [User report](concept/report-scripts) |
| Easy Invoice, Fancy Invoice, Tax Invoice, Australian Tax Invoice, Receipt | Not ported. Use the Printable Invoice, or a [User report](concept/report-scripts) |

## Tax and other reports not ported

The Tax Schedule Report and TXF Export, the US and German income tax reports, and the Income and GST Statement are not
in red-ale. The tax flags on Accounts are kept in the book (see [Accounts](gnucash/account-menu)), so GnuCash still has
them. The Lot Viewer, Hello World, Sample Graphs and Welcome reports are not ported either. The Register report
(Account Report) is the [Transaction Report](report/transaction-report) with one Account. Write the rest as
[User reports](concept/report-scripts); see [Not in red-ale](gnucash/not-in-red-ale).

## The report window

| GnuCash | red-ale |
|---|---|
| Options... | The `options` of [`report.run`](method/report.run), by option page and name, as [`report.options`](method/report.options) lists them |
| File > Save Report Configuration, Save Report Configuration As | [`report.saveConfiguration`](method/report.saveConfiguration); change one with [`report.updateConfiguration`](method/report.updateConfiguration), rename with [`report.renameConfiguration`](method/report.renameConfiguration), delete with [`report.deleteConfiguration`](method/report.deleteConfiguration) |
| File > Export Report... | [`report.export`](method/report.export) writes HTML; [`report.exportCsv`](method/report.exportCsv) gives CSV |
| File > Print Report, Page Setup | Not provided. Export the HTML and print it from a browser |
| Edit > Style Sheets... | [`report.styleSheets`](method/report.styleSheets), with [`report.createStyleSheet`](method/report.createStyleSheet), [`report.updateStyleSheet`](method/report.updateStyleSheet) and [`report.deleteStyleSheet`](method/report.deleteStyleSheet) |
| Reload, Stop, Back, Forward, Refresh | Not needed. Call `report.run` again |
