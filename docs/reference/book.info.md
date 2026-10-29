<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/book.info
related: method/book.open, method/book.save, method/book.options
---
Describes the open book: its path and format, whether it is `dirty` (has unsaved changes), whether it is read-only
and why, the Book features it declares, and how many accounts, transactions, prices and so on it holds. Call it
after [book.open](method/book.open) to see what you opened, or to check `dirty` before closing.

`counts` are the book's own totals; they include hidden accounts and read-only transactions.

## Examples

```example fixture=reports/test-data
{"method": "book.info", "params": {}}
{"result": {"path": "…", "readOnly": false, "dirty": false, "format": "xml", "features": [], "counts": {"commodities": 3, "accounts": 11, "transactions": 59, "prices": 2, "…": "…"}, "…": "…"}}
```
