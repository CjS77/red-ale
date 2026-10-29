<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/export.accounts
related: method/export.csvTree, method/book.create
---
Writes a new book file with the open book's account tree and commodities and no transactions: a chart of
accounts to start another book from. It never overwrites a file: a path that exists
is refused with `file-exists`. `.gnucash` is added when missing. The open book is not changed.

## Examples

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "export.accounts", "params": {"path": "chart"}}
{"result": {"path": "…", "accounts": 2, "commodities": 2}}
{"method": "export.accounts", "params": {"path": "chart"}}
{"error": {"code": 1007, "message": "…", "data": {"path": "…", "help": "error/file-exists"}}}
```
