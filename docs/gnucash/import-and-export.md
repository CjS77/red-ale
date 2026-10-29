<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/import-and-export
title: Import and export
summary: GnuCash's File > Import and File > Export mapped to methods - QIF, CSV transactions, accounts, prices, bills and invoices, and customers and vendors through import.preview and import.apply; exports through the export methods. OFX, QFX, MT940 and the AqBanking formats are not supported.
related: concept/importing-and-matching, gnucash/file-menu, gnucash/not-in-red-ale, method/import.preview, method/import.apply, method/import.discard, method/export.csvTransactions, method/export.csvTree, method/export.accounts, method/book.replayJournal, guide/import-transactions
from: guide/ch_importing.docbook#chapter_importing
---
GnuCash imports through assistants that ask you questions as they go. red-ale always works in two steps: a **preview**
that reads the file and changes nothing, then an **apply** or a **discard**. The answer to the preview is the Import
proposal. See [Importing and matching](concept/importing-and-matching) for how it matches and what you can change.

## Import

| GnuCash | red-ale |
|---|---|
| File > Import > Import QIF... | [`import.preview`](method/import.preview) with `format` `qif`. Only QIF takes several files. Which Account each QIF category goes to is a Configuration setting, `import.qifAccountMap` |
| File > Import > Import Transactions from CSV... | `format` `csvTransactions`, with a `layout`: the separator, the date and number formats, and what each column is. A saved layout is named in the Configuration (`import.csvLayouts.*`). Only this format goes through the duplicate matcher |
| File > Import > Import Accounts from CSV... | `format` `csvAccounts` |
| File > Import > Import Prices from CSV... | `format` `csvPrices` |
| File > Import > Import Bills & Invoices... | `format` `csvDocuments` |
| File > Import > Import Customers & Vendors... | `format` `csvOwners` |
| The assistant's final page: Finish, or Cancel | [`import.apply`](method/import.apply) with your choices for each Transaction (`add`, `clear`, `update` or `skip`), or [`import.discard`](method/import.discard) |
| The Match Picker and the Import Map Editor | `matches` in the proposal, and [`importMap.list`](method/importMap.list), [`importMap.delete`](method/importMap.delete) ([Tools](gnucash/tools-menu)) |
| File > Import > Import Log Replayer... | [`book.replayJournal`](method/book.replayJournal), for the `.log` journal GnuCash writes beside a book |

The file goes in the call as text; there is no path to read. A proposal that does not balance is not refused, as in
GnuCash: applying it adds an `Imbalance-<CUR>` split, which is flagged in the proposal beforehand. Rows that cannot be
imported are listed in `errors` and left out of the apply.

## Export

| GnuCash | red-ale |
|---|---|
| File > Export > Export Transactions to CSV... | [`export.csvTransactions`](method/export.csvTransactions): Accounts, a date range, `layout` of `complex` (one line per Split) or `simple` (one line per Transaction). The text comes back in the result; no file is written |
| File > Export > Export Active Register to CSV... | The same with the one Account |
| File > Export > Export Account Tree to CSV... | [`export.csvTree`](method/export.csvTree) |
| File > Export > Export Accounts (a new book with the Account tree and no Transactions) | [`export.accounts`](method/export.accounts). It never overwrites a file |
| Report > Export Report... | [`report.export`](method/report.export) ([Reports menu](gnucash/reports-menu)) |

## Not supported

Open Financial Exchange (OFX and QFX), the AqBanking formats (MT940, MT942, MT535, DTAUS, CAMT 052, and the others),
online banking and the HBCI setup are not in red-ale. Convert such a file to QIF or CSV first, or see
[Not in red-ale](gnucash/not-in-red-ale).
