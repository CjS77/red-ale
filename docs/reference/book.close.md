<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/book.close
related: method/book.save, method/book.open, error/unsaved-changes
---
Closes the open book and releases its lock, so another program (or another [book.open](method/book.open)) can use
it. If the book has unsaved changes the call is refused with [unsaved-changes](error/unsaved-changes) and the book
stays open; pass `save: true` to write them first or `discard: true` to drop them.

Not to be confused with [book.closeBooks](method/book.closeBooks), which is GnuCash's Close Book: it zeroes income
and expense into equity and leaves the book open.

## Examples

An edited book will not close until you say what to do with the edit:

```example fixture=reports/test-data
{"method": "account.create", "params": {"name": "Savings", "type": "BANK", "parent": "Root:Asset", "commodity": "USD"}}
{"result": "…"}
{"method": "book.close", "params": {}}
{"error": {"code": 1012, "message": "…", "data": {"help": "error/unsaved-changes"}}}
{"method": "book.close", "params": {"save": true}}
{"result": {"saved": true, "discarded": false}}
```
