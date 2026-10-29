<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/migrate-from-gnucash
title: Migrate from GnuCash
summary: Open a copy of your GnuCash file in red-ale, check that it opens as you expect, and bring over data from other tools with CSV and QIF imports.
related: method/book.open, method/book.info, method/book.save, method/import.preview, method/export.csvTransactions, method/export.accounts, guide/import-transactions, concept/coming-from-gnucash
from: guide/ch_basics.docbook#basics-migrate-settings
---
red-ale reads and writes GnuCash's own files, so there is nothing to convert. Your existing `.gnucash` file, XML (plain or
compressed) or SQLite, is the book. What changes is how you work with it: through JSON-RPC, MCP or the command line instead
of windows.

## Steps

1. **Work on a copy.** Close GnuCash, copy the file somewhere safe, and open the copy. Your original stays exactly as it
   was, and red-ale takes a backup before every save anyway ([Back up and recover](guide/back-up-and-recover)).
2. **Open it** with [book.open](method/book.open). red-ale will not open a book that GnuCash or another red-ale has locked
   for writing. Try `readOnly: true` first if you only want to look.
3. **Check the result.** `readOnlyReasons` says why a book opened read-only, for example because it uses a GnuCash feature
   red-ale does not implement. Such a book is never written, so you cannot damage it.
   [book.info](method/book.info) shows the format and the counts of accounts, transactions and prices.
4. **Look around.** [account.tree](method/account.tree) shows the chart, [transaction.list](method/transaction.list) a
   register, and the reports run as in GnuCash ([Run and export a report](guide/run-and-export-a-report)). For where GnuCash's
   menus have gone, search the manual with `help.search`.
5. **Carry settings over.** GnuCash's preferences are not read from its files. Set the equivalents with
   [config.set](method/config.set) ([Configure red-ale](guide/configure-red-ale)); book options are already in the book.
   Saved report configurations and style sheets from GnuCash's own files can be imported with
   [report.importConfigurations](method/report.importConfigurations) and [report.importStyleSheets](method/report.importStyleSheets).
6. **Save** with [book.save](method/book.save). GnuCash opens the result with nothing lost.
7. **Go back and forth.** The same file opens in both programs, one at a time. Close one before opening it in the other.

## Data from elsewhere

- **Bank files:** [Import transactions](guide/import-transactions) reads CSV and QIF.
- **Customers, vendors and invoices:** [Import business data](guide/import-business-data).
- **OFX, AqBanking and MT940** are not supported. Convert such files to CSV or QIF first.
- **A spreadsheet or another book's accounts:** [Export data](guide/export-data) produces CSV in GnuCash's layouts, and
  [export.accounts](method/export.accounts) starts a new book from your chart.
- **XML to SQLite or back:** create a new book in the other format with [book.create](method/book.create), and move the
  data with CSV exports and imports or with GnuCash's Save As. red-ale saves a book in the format it was opened in.

## Example

Open a GnuCash test book read-only and look at it.

```example fixture=gnucash/sample1 book=closed
{"method": "book.open", "params": {"path": "book.gnucash", "readOnly": true}}
{"result": {"readOnly": true, "readOnlyReasons": [{"code": "requested", "message": "opened read-only on request"}], "format": "xml", "…": "…"}}
{"method": "book.info", "params": {}}
{"result": {"readOnly": true, "dirty": false, "format": "xml", "counts": "…", "…": "…"}}
{"method": "account.tree", "params": {}}
{"result": "…"}
```
