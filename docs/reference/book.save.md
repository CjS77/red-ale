<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/book.save
related: method/book.close, method/book.info, error/backup-failed, error/save-verification
---
Writes the open book to its file. Changes made by other methods live in memory (and in the `.log` journal) until you
call this. Before overwriting, it copies the previous file to `<book>.<timestamp>.gnucash`, as GnuCash does, and
returns the copy's path as `backup`; it then reads the new file back to check that it holds the same book.

An XML book keeps its compression unless you pass `compressed`. An SQLite book is backed up before the session's
first save that writes, and `compressed` is refused for it.

## Examples

```example fixture=reports/test-data
{"method": "account.create", "params": {"name": "Savings", "type": "BANK", "parent": "Root:Asset", "commodity": "USD"}}
{"result": "…"}
{"method": "book.info", "params": {}}
{"result": {"dirty": true, "…": "…"}}
{"method": "book.save", "params": {}}
{"result": {"backup": "…", "removed": [], "compressed": false}}
{"method": "book.info", "params": {}}
{"result": {"dirty": false, "…": "…"}}
```
