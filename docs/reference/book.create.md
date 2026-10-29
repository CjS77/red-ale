<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/book.create
related: method/book.open, method/book.save, error/file-exists
---
Creates a new, empty book (a root account and nothing else) at `path` and opens it. By default it is an XML book,
gzip-compressed as GnuCash writes it; pass `compressed: false` for plain XML or `format: "sqlite"` for an SQLite
book (also chosen by a `sqlite3://` path). The file is written by the first [book.save](method/book.save).

The path must be free: an existing file is refused with [file-exists](error/file-exists), so a book is never
overwritten by accident.

## Examples

```example book=none
{"method": "book.create", "params": {"path": "new.gnucash", "compressed": false}}
{"result": {"path": "…", "guid": "…", "rootAccount": "…", "readOnly": false, "format": "xml", "compressed": false, "…": "…"}}
{"method": "account.list", "params": {}}
{"result": {"accounts": [], "…": "…"}}
```
