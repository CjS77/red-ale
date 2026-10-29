<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/book.open
related: method/book.create, method/book.info, method/book.save, method/book.close, error/book-locked
---
Opens an existing GnuCash book, XML or SQLite, so the other methods can read and change it. Call it first, with the
book's path; a server holds one book at a time, so close the open one with `book.close` before opening another.
Unless you ask for `readOnly`, it takes the book's lock, as GnuCash does, and fails with `book-locked` while GnuCash or
another red-ale has the book open.

A book can also open read-only without being asked: `readOnly` is then true and `readOnlyReasons` says why, for
instance a Book feature red-ale does not implement. Read-only books are never locked and never written.

## Examples

```example fixture=reports/test-data book=closed
{"method": "book.open", "params": {"path": "book.gnucash"}}
{"result": {"path": "…", "guid": "…", "rootAccount": "…", "readOnly": false, "readOnlyReasons": [], "dirty": false, "compressed": "…", "format": "xml", "journal": "…", "features": "…", "counts": "…"}}
```

Opening it read-only leaves it free for GnuCash:

```example fixture=reports/test-data book=closed
{"method": "book.open", "params": {"path": "book.gnucash", "readOnly": true}}
{"result": {"readOnly": true, "readOnlyReasons": [{"code": "requested", "message": "opened read-only on request"}], "…": "…"}}
```
