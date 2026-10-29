<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/sql-book
summary: The book is on a database server, which red-ale does not open.
---
**Cause.** The path is a `mysql://`, `postgres://` or `postgresql://` URI. red-ale reads XML and SQLite books only,
and refuses a server book before touching it.

**Fix.** Open a copy of the book as a file: in GnuCash, use File > Save As to write an XML or SQLite book, then open
that. Changes made to the copy are not reflected in the server's book.
