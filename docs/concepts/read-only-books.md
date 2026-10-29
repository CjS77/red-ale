<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/read-only-books
title: Read-only books
summary: Why a Book opens read-only - you asked, it declares a Book feature red-ale lacks, it holds content red-ale cannot safely edit, or its SQL schema is not GnuCash 5.17's - what readOnlyReasons says and what to do about each.
related: concept/books-and-files, method/book.open, method/book.info, error/read-only, error/book-locked, guide/create-open-and-save-a-book
from: guide/ch_basics.docbook#basics-files-storage-comparison
---
A **read-only session** never takes the lock and refuses every method that changes the Book (with
[`read-only`](error/read-only)). [`book.open`](method/book.open) and [`book.info`](method/book.info) say whether the
book is read-only and why, in `readOnlyReasons`: a list of `{ code, message }`.

red-ale prefers to refuse than to risk a book. If it cannot be sure that an edit will leave what GnuCash keeps intact,
it opens the book read-only and tells you.

| Code | Why | What to do |
|------|-----|-----------|
| `requested` | You opened it with `readOnly: true` | Close it and open it again without `readOnly` |
| `unsupported-features` | The book declares a Book feature that GnuCash implements and red-ale does not. Writing could drop what that feature means | Edit it in GnuCash. Reading, reports and exports all work |
| `unrecognised-content` | It holds something red-ale can keep but not edit safely: an unknown SQLite table or column, damaged slot rows, a failed integrity check | Edit it in GnuCash. Reading, reports and exports work |
| `schema-version` | An SQLite book whose tables are not at GnuCash 5.17's versions. red-ale never upgrades a schema | Open it in GnuCash 5.17 so that it upgrades the book, then reopen |

A read-only book is also how to look at a book that another program holds: a read-only open takes no lock, so it works
beside a running GnuCash. You get the book as it was last saved; edits made since are not there.

## What is not read-only

- A Book with the num field source option, the auto read-only threshold or trading accounts opens writable. The
  threshold makes only the *old Transactions* read-only: see [Transactions and Splits](concept/transactions-and-splits).
- Unknown XML elements and Slots in an XML book are not a reason in themselves: red-ale keeps them and writes them
  back unchanged ([Books and files](concept/books-and-files)).

## Checking before you change a book

Call `book.info` after opening, and read `readOnly`. An agent that is about to edit should stop and tell the user when
`readOnlyReasons` holds a reason other than `requested`: red-ale will not write the book however it asks.
