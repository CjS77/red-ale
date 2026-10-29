<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/books-and-files
title: Books and files
summary: A Book is one set of Accounts kept in one file, XML (plain or gzipped) or SQLite. Locks, the journal, backups and retention, Book features, and which storage to choose. Database-server books are not supported.
related: concept/read-only-books, method/book.create, method/book.open, method/book.info, method/book.save, method/book.close, method/book.replayJournal, guide/create-open-and-save-a-book, guide/back-up-and-recover
from: guide/ch_basics.docbook#basics-files1
---
GnuCash organises data on three levels: books, accounts and transactions. One **Book** holds many Accounts, and one
Account holds many Transactions. The Book is the top: one set of Accounts, with its commodities, prices, lots,
scheduled transactions, budgets and business objects. A **Book file** stores one Book. You need one book for each set
of accounts you keep; most people have one.

## Storage

red-ale reads and writes the same files as GnuCash, so a copy of a `.gnucash` file opens in red-ale, and a book red-ale
saved opens in GnuCash with nothing lost.

| Format | Notes |
|--------|-------|
| XML, gzip-compressed | GnuCash's default. A book is rewritten whole on each save |
| XML, plain | The same, readable in a text editor. A book keeps the compression it was read with; [`book.save`](method/book.save) can change it with `compressed` |
| SQLite | A single file holding a database. GnuCash writes each change as it is made; red-ale writes them at `book.save`, in one SQL transaction |

XML is the most established and is recommended for most people. SQLite has the benefits of SQL storage without a
database server. Neither is multi-user: only one program writes a book at a time.

**MySQL and PostgreSQL books are not supported.** A `mysql://` or `postgres://` path is refused with
[`sql-book`](error/sql-book). In GnuCash use File > Save As to write an XML or SQLite copy, and open that.

GnuCash can store an SQL book in places red-ale will not touch: if an SQLite book's schema is not GnuCash 5.17's, or it
holds tables red-ale does not know, it opens read-only (see [Read-only books](concept/read-only-books)) and red-ale
never upgrades a schema.

## Creating, opening, saving, closing

One book is open at a time.

- [`book.create`](method/book.create) makes a new, empty book (the root Account and nothing else) at a path that must
  be free. It is written at the first `book.save`.
- [`book.open`](method/book.open) opens an existing book. `book.info` describes what you opened.
- [`book.save`](method/book.save) writes it. Changes live in memory, and in the journal, until you save.
- [`book.close`](method/book.close) releases the lock. It refuses to drop unsaved changes unless you say `save` or
  `discard`.

`red-ale exec` opens, runs one method and saves for you.

## The files beside a book

| File | What it is |
|------|------------|
| `<book>.LCK` | The lock of an XML book: it stops two programs writing one book. GnuCash also writes a `.LNK`. SQLite books use a row of the `gnclock` table instead |
| `<book>.<YYYYMMDDhhmmss>.gnucash` | A **backup**: a complete copy of the previous file, made when a save replaces it. Restore one by opening it, then saving it under a name of your choice |
| `<book>.<YYYYMMDDhhmmss>.log` | The **journal**: every change to Transactions since the book was opened. It is not a backup of the book |

- **Locks.** red-ale takes the lock when it opens a book for writing, and always releases it, also on an error. A book
  GnuCash or another red-ale holds is refused with [`book-locked`](error/book-locked); open it read-only to look at it.
  A crash can leave a `.LCK` file behind: delete it only if you know the program that made it is gone.
- **Backups and the journal.** A save makes its backup first and reads the new file back to check it; a journal is
  started after each save. Backups and old journals are deleted after a retention period, 30 days by default
  (`--retention-days <n>`, or `--keep-backups-forever`, on the command that starts red-ale). `book.save` lists what
  it removed.
- **Recovering unsaved work.** After a crash, open the last saved book and replay its journal with
  [`book.replayJournal`](method/book.replayJournal). Replay restores Transactions only; changes to scheduled
  transactions, invoices or bills are not recovered, and Transactions created from them may no longer be tied to them,
  so check those.

There is no autosave: save when you want to keep your changes.

## Book features

A Book declares the capabilities it relies on in its `features` list (credit notes, natural-sign budgets, the
split-action number field...), so that a program lacking one refuses to edit it. **⚠** A *Book feature* is not a
"feature" of red-ale. red-ale keeps the list as read, adds a feature only when you use something that needs it (making a
credit note, say), and opens a book read-only if it declares a feature red-ale does not implement.

## Moving between GnuCash and red-ale

Use a copy of a book while you are learning, and never open the same book in both at once: whoever opens it second is
told it is locked. See [Coming from GnuCash](concept/coming-from-gnucash).
