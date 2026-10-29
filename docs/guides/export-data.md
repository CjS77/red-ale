<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/export-data
title: Export data
summary: Get transactions out as CSV, in GnuCash's simple or complex layout, and the account tree as CSV or as a file of accounts and commodities.
related: method/export.csvTransactions, method/export.csvTree, method/export.accounts, method/report.exportCsv
---
To use your data in a spreadsheet, send it to an accountant, or load it into another book, export it. red-ale has three
exports. None changes the book.

| Export | Gives | Format |
|--------|-------|--------|
| [export.csvTransactions](method/export.csvTransactions) | Transactions of chosen accounts and dates | CSV text in the result |
| [export.csvTree](method/export.csvTree) | The account tree: type, name, code, commodity, flags | CSV text in the result |
| [export.accounts](method/export.accounts) | The accounts and their commodities, to be imported as a new book's chart | A file of a book with no transactions |

## Steps

1. **Transactions.** Call [export.csvTransactions](method/export.csvTransactions) with the `accounts` to export (a
   transaction appears under the first of them it has a split in), optionally `from` and `to`, and a `layout`: `simple`,
   one line per split with the other account in the line, or `complex` (the default), GnuCash's full layout with one line
   per split and its memo, action and price. `separator` and `quotes` shape the CSV; `locale` decides how dates and numbers
   are written. The CSV is in the result as `csv`. From the command line, `--output transactions.csv` writes it to a file.
2. **The account tree.** [export.csvTree](method/export.csvTree) gives one row per account in the layout that
   [Import transactions](guide/import-transactions) and GnuCash's account import read.
3. **A chart of accounts.** [export.accounts](method/export.accounts) writes the accounts and commodities, without the
   transactions, to a new book at `path`. It refuses a path that exists. Use it to start a new year's book with the same
   chart.
4. **A report's figures.** Reports with a CSV export give theirs through [report.exportCsv](method/report.exportCsv) (see
   [Run and export a report](guide/run-and-export-a-report)).

## Example

```example now=2024-07-01T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Food", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-06-03", "description": "Coffee", "splits": [{"account": "Food", "value": "3.50"}, {"account": "Checking", "value": "-3.50"}]}}
{"result": "…"}
{"method": "export.csvTransactions", "params": {"accounts": ["Checking"], "layout": "simple"}}
{"result": {"csv": "Date,Account Name,Number,Description,Full Category Path,Reconcile,Amount With Sym,Amount Num.,Value With Sym,Value Num.,Rate/Price\r\n06/03/24,Checking,,Coffee,Food,n,-$3.50,-3.50,-$3.50,-3.50,1.00\r\n", "transactions": 1, "lines": 1}}
{"method": "export.csvTree", "params": {}}
{"result": {"csv": "Type,Full Account Name,Account Name,Account Code,Description,Account Color,Notes,Symbol,Namespace,Hidden,Tax Info,Placeholder\r\nEXPENSE,Food,Food,,,,,USD,CURRENCY,F,F,F\r\nBANK,Checking,Checking,,,,,USD,CURRENCY,F,F,F\r\n", "rows": 2}}
{"method": "export.accounts", "params": {"path": "chart"}}
{"result": {"path": "…", "accounts": 3, "commodities": "…"}}
```
