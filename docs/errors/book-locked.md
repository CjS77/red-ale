<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/book-locked
related: method/book.open
summary: Another program holds the book's lock.
---
**Cause.** GnuCash, or another red-ale, has the book open for writing. An XML book is locked by a `.LCK` file beside
it (`data.path` is that file); an SQLite book by a row in its `gnclock` table (`data.path` is the book and
`data.holder` the row's `hostname` and `pid`). With no `holder`, another program was writing the SQLite book for
longer than red-ale's 5 second wait.

**Fix.** Close the other program and call again. Open the book with `readOnly: true` to look at it meanwhile; a
read-only open takes no lock. Delete a `.LCK` file only if you know the program that made it is gone, as a crash can
leave one behind, and never while GnuCash is running.
