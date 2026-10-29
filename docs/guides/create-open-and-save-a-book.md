<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/create-open-and-save-a-book
title: Create, open and save a book
summary: Make a new book as XML or SQLite, open an existing one (read-only if you only want to look), save it with a backup, and close it so its lock is released.
related: method/book.create, method/book.open, method/book.info, method/book.save, method/book.close, error/unsaved-changes, error/file-exists, concept/books-and-files, concept/read-only-books
from: guide/ch_basics.docbook#basics-files1
---
A book is the file that holds all your accounts and transactions. red-ale keeps one book open at a time. You create it
or open it, change it in memory, save it, and close it. Opening takes the book's lock so that GnuCash and red-ale never
write the same file at once; closing releases it.

## Steps

1. **Create a book** with [book.create](method/book.create). The default is a gzip-compressed XML book, which is what
   GnuCash writes. Pass `compressed: false` for plain XML, or `format: "sqlite"` (or a `sqlite3://` path) for an SQLite
   book. The path must not exist yet: an existing file is refused with [file-exists](error/file-exists).
2. **Look at it** with [book.info](method/book.info): the format, whether it is read-only and why, whether there are
   unsaved changes (`dirty`), and counts of accounts, transactions and prices.
3. **Change it** with the methods in the other guides. Nothing reaches the file yet; `dirty` becomes `true`.
4. **Save it** with [book.save](method/book.save). Before overwriting, red-ale copies the previous file to
   `<book>.<timestamp>.gnucash` and returns that path as `backup`. A brand-new book has no previous file, so it has no
   backup.
5. **Close it** with [book.close](method/book.close). With unsaved changes it refuses
   ([unsaved-changes](error/unsaved-changes)); say `save: true` or `discard: true`.
6. **Open it again** with [book.open](method/book.open). To look without any risk, pass `readOnly: true`: the lock is
   not taken and nothing can be changed. A book can also open read-only because it uses a GnuCash feature that red-ale
   does not implement; `readOnlyReasons` says why.

From the command line, `red-ale exec --book book.gnucash <method> '<params>'` does steps 6, 3, 4 and 5 for one call, and
`--create` makes the book first.

## Example

Create a plain XML book, add an account, save, close and open it again.

```example book=none
{"method": "book.create", "params": {"path": "household.gnucash", "compressed": false}}
{"result": {"path": "…", "guid": "…", "rootAccount": "…", "readOnly": false, "format": "xml", "compressed": false, "…": "…"}}
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "book.info", "params": {}}
{"result": {"dirty": true, "readOnly": false, "format": "xml", "counts": "…", "…": "…"}}
{"method": "book.save", "params": {}}
{"result": "…"}
{"method": "book.close", "params": {}}
{"result": {"saved": false, "discarded": false}}
{"method": "book.open", "params": {"path": "household.gnucash", "readOnly": true}}
{"result": {"readOnly": true, "readOnlyReasons": [{"code": "requested", "message": "opened read-only on request"}], "…": "…"}}
```
