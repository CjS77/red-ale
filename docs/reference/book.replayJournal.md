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
