<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/coming-from-gnucash
title: Coming from GnuCash
summary: What changes if you know GnuCash - no windows, registers or menus, only methods - and GnuCash's words mapped to red-ale's, how the two share books safely, accounts instead of categories, and where the From GnuCash pages are.
related: concept/books-and-files, concept/value-and-amount, concept/chart-of-accounts, concept/configuration, gnucash/file-menu, gnucash/not-in-red-ale, method/help.search, guide/migrate-from-gnucash
from: guide/ch_oview.docbook#chapter_oview
---
GnuCash is personal finance software made to keep track of anything from a checkbook to a business, with investments,
loans and many currencies. red-ale reimplements it *without a window*. It reads and writes GnuCash's own files, XML or
SQLite, and every operation is a JSON-RPC method, also an MCP tool for AI agents and a command of `red-ale exec`. There is
no assistant and no register. If you know GnuCash, this is what to expect.

## What replaces the windows

| In GnuCash | In red-ale |
|---|---|
| Menus and dialogs | Methods. Each [From GnuCash page](gnucash/file-menu) maps a menu to its methods |
| The Account tree window | [`account.tree`](method/account.tree), [`account.list`](method/account.list), [`account.balance`](method/account.balance) |
| The account register | [`transaction.list`](method/transaction.list) for an Account, [`transaction.post`](method/transaction.post) to enter |
| The Reconcile window | [`reconcile.start`](method/reconcile.start) and its siblings; the caller carries the ticks |
| Assistants (Stock Split, Loan, Close Book, Import) | One call, or a proposal call and an apply call, for each |
| Preferences | The [Configuration](concept/configuration) |
| Book options (File > Properties) | [`book.options`](method/book.options), [`book.setOptions`](method/book.setOptions) |
| Reports in a window | [`report.run`](method/report.run) and [`report.export`](method/report.export) |
| Help | [`help.search`](method/help.search), [`help.get`](method/help.get) and this Manual |

Nothing is shown or confirmed for you: a method does what it says, completely or not at all. Every change is atomic and
leaves every Transaction balanced. There is no "undo" button, so write to a copy while learning.

## Sharing a book with GnuCash

- **A file is a file.** Copy your `.gnucash` file, open the copy with [`book.open`](method/book.open), and GnuCash opens
  the result with nothing lost: anything red-ale does not understand is kept and written back unchanged. GUIDs, the order
  of entities and timestamps are kept as read.
- **One writer at a time.** XML books use a `.LCK` file and SQLite books the `gnclock` table. red-ale refuses a book
  GnuCash holds ([`book-locked`](error/book-locked)) and always releases its lock.
- **Backups and the journal** are made as GnuCash makes them ([Books and files](concept/books-and-files)).
- **Read-only when unsure.** A book that uses a feature red-ale lacks opens read-only ([Read-only books](concept/read-only-books)).
- **It does not repair on its own.** GnuCash scrubs a book as it opens it, and runs Since Last Run; red-ale changes
  nothing until you ask ([Imbalance, orphans and scrubs](concept/imbalance-and-scrubs)).
- **SQL.** SQLite books work. MySQL and PostgreSQL books are not supported: save a copy as SQLite or XML in GnuCash first.

## Words

red-ale keeps GnuCash's terms and says so where it uses a word differently.

| GnuCash says | red-ale says |
|---|---|
| Chart of accounts | **Account tree** ([The chart of accounts](concept/chart-of-accounts)) |
| Category (in other programs) | An `INCOME` or `EXPENSE` **Account** ([Double entry](concept/double-entry)) |
| Amount, quantity | **Amount** is a Split's size in its Account's Commodity; **value** is its size in the Transaction currency ([Value and amount](concept/value-and-amount)) |
| Preferences | The **Configuration**; a *Book option* is something else, kept in the book |
| Transaction log, `.log` | **Journal** |
| KVP, key-value pairs | **Slots** and **Frames** |
| SX | **Scheduled transaction** |
| Custom report | **Saved report configuration**; a *User report* is your own script |
| Invoice (bill, voucher) | **Business document**, or invoice, bill, expense voucher |
| Red line | The **auto read-only threshold** |
| Currency, security | Both are **Commodities**; a currency is a Commodity in the `CURRENCY` namespace |
| Close Book | `book.closeBooks`, not `book.close` ([Closing the books](concept/closing-the-books-and-accounting-period)) |
| Reverse balanced accounts | A display preference. Methods always use the **raw sign**: debits positive |

## Accounts versus categories

If you come from a program that uses categories, income and expenses are Accounts in GnuCash and in red-ale. Where you
would have named a category, name an income or expense Account; everything you could do with a category you can do. See
[Double entry](concept/double-entry).

## Not in red-ale

GUI windows, registers, autocompletion, the Tip of the Day, online banking, OFX import, Finance::Quote, Python bindings,
Guile scripting, the Account templates, the financial calculator, check printing, PDF export, most business reports and
the tax reports are not ported. [Not in red-ale](gnucash/not-in-red-ale) says what to do instead of each.

## Finding your way

Search the Manual in your own words, or by the name of a GnuCash menu item: `help.search` finds "Stock Split..." and
"Reconcile". The From GnuCash pages are [File menu](gnucash/file-menu), [Accounts and Actions](gnucash/account-menu),
[Transactions](gnucash/transaction-menu), [Business](gnucash/business-menu), [Tools](gnucash/tools-menu),
[Stock assistants](gnucash/stock-assistants), [Import and Export](gnucash/import-and-export),
[Reports](gnucash/reports-menu), [Preferences and book options](gnucash/preferences-and-book-options) and
[Windows and registers](gnucash/windows-and-registers).
