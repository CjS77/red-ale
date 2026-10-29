<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/book.replayJournal
related: method/book.save, error/file-format
---
Replays a GnuCash `.log` journal onto the open book: each record creates, updates or deletes a transaction by GUID.
Use it to recover work from a journal after a crash, by replaying the journal of the book you lost onto the last
saved file. `created`, `updated` and `deleted` list the GUIDs touched; `notFound` lists records whose transaction was
not there to update or delete.

Replaying is journaled like any other change, and it changes transactions only, not accounts: records that need an
account the book lacks are refused unless `skipInvalid` is true, which skips them and lists them in
`skippedRecords`. The file is read on the server, so `path` is relative to where it runs.

## Examples

The journal of an open book is the `journal` field of [book.info](method/book.info). A path that does not exist
fails with `io-error`, and nothing is changed:

```example fixture=reports/test-data
{"method": "book.replayJournal", "params": {"path": "missing.log"}}
{"error": {"code": 1008, "message": "…", "data": {"path": "…", "help": "error/io-error"}}}
```

A transaction posted and then lost, because the book was closed without saving, comes back from the journal. The
journal is named by the local time it was opened at, here `book.gnucash.20240701120000.log`:

```example now=2024-07-01T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "book.save", "params": {}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-01", "description": "Rent", "splits": [{"account": "Checking", "value": "-750.00"}, {"account": "Rent", "value": "750.00"}]}}
{"result": {"guid": "…", "…": "…"}}
{"method": "book.close", "params": {"discard": true}}
{"result": "…"}
{"method": "book.open", "params": {"path": "book.gnucash"}}
{"result": {"counts": {"transactions": 0, "…": "…"}, "…": "…"}}
{"method": "book.replayJournal", "params": {"path": "book.gnucash.20240701120000.log"}}
{"result": {"created": ["…"], "updated": [], "deleted": [], "notFound": [], "…": "…"}}
```
