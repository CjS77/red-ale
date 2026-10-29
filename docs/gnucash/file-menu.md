<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/file-menu
title: File menu and the New File assistant
summary: GnuCash's File menu mapped to methods - File > New File is book.create, Open is book.open, Save is book.save, Revert is book.close with discard, Export is the export methods, Properties is book.options. There is no Save As, no Print and no Account templates.
related: concept/books-and-files, concept/read-only-books, concept/configuration, gnucash/import-and-export, gnucash/windows-and-registers, method/book.create, method/book.open, method/book.save, method/book.close, guide/set-up-accounts
from: manual/ch_GUIMenus.docbook#AccTree-file-menu
---
GnuCash's **File** menu opens, saves and exports books. In red-ale every item is a method, and there is no window to
start from.

| GnuCash | red-ale |
|---|---|
| File > New File | [`book.create`](method/book.create) makes an empty book (just the root Account) at a path that must be free: `format` is `xml` (the default, gzipped unless `compressed: false`) or `sqlite`. See below for the New Account Hierarchy assistant |
| File > Open... | [`book.open`](method/book.open). Pass `readOnly: true` to look without taking the lock. [`book.info`](method/book.info) says what you opened |
| File > Open > recently opened files | Not provided; keep your own list of paths |
| File > Import > ... | [From GnuCash: Import and Export](gnucash/import-and-export) |
| File > Import > Replay GnuCash .log file... | [`book.replayJournal`](method/book.replayJournal), to recover work from a journal after a crash |
| File > Save | [`book.save`](method/book.save): backs up the previous file, writes the book, reads it back to check, starts a new journal |
| File > Save As... | **Not provided.** `book.save` writes the book to the path it was opened from. To keep a copy, copy the file when the book is saved and closed. To change an XML book from gzipped to plain, pass `compressed` to `book.save`. To change between XML and SQLite use GnuCash |
| File > Revert | [`book.close`](method/book.close) with `discard: true`, then `book.open` again. Unsaved changes are lost |
| File > Print, Page Setup | Not provided. A report is exported as HTML ([`report.export`](method/report.export)) and printed from a browser, or turned into PDF with a converter ([Reports](concept/reports)) |
| File > Export > Export Account Tree to CSV | [`export.csvTree`](method/export.csvTree) |
| File > Export > Export Transactions to CSV | [`export.csvTransactions`](method/export.csvTransactions) |
| File > Export > Export Active Register to CSV... | [`export.csvTransactions`](method/export.csvTransactions) for that one Account |
| File > Export > Export Accounts | [`export.accounts`](method/export.accounts): a new book with the Account tree and Commodities and no Transactions |
| File > Properties | [`book.options`](method/book.options) and [`book.setOptions`](method/book.setOptions); numbering in [`counter.list`](method/counter.list) and [`counter.set`](method/counter.set). See [Preferences and book options](gnucash/preferences-and-book-options) |
| File > Close | [`book.close`](method/book.close). With unsaved changes it refuses unless you pass `save` or `discard` |
| File > Quit | Stop the process: `red-ale serve` and `red-ale mcp` end with the signal or the closed connection. Close the book first, so its lock is released |

## Starting a book

GnuCash's New File runs the **New Account Hierarchy Setup** assistant, which builds a starting tree of Accounts from a
localised template (a checkbook, a business...). **red-ale has no Account templates and no such assistant.** Build the
tree yourself with [`account.create`](method/account.create) ([The chart of accounts](concept/chart-of-accounts)), import
one from CSV ([Import and Export](gnucash/import-and-export)), or start from another book's tree with
[`export.accounts`](method/export.accounts). Opening balances go in with the Account:
[`account.create`](method/account.create) takes an `openingBalance`, and
[`account.openingBalanceAccount`](method/account.openingBalanceAccount) finds the equity Account that holds them.

## The first run

GnuCash's Welcome screen, Tip of the Day, tabs and toolbars have no counterpart. You do not "run" red-ale and see a
window: you call `book.create` or `book.open` and then the other methods. See
[Windows, registers and the Account tree](gnucash/windows-and-registers).

## Moving a book you have

Copy your `.gnucash` file, open the copy, and read or change it. GnuCash opens the result. GnuCash and red-ale never write
one book at once: the second to open is told it is locked ([`book-locked`](error/book-locked)). See
[Coming from GnuCash](concept/coming-from-gnucash).
