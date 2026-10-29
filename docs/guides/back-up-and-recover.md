<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/back-up-and-recover
title: Back up and recover a book
summary: Every save backs up the previous file; a .log journal records changes between saves, and book.replayJournal puts them back after a crash.
related: method/book.save, method/book.info, method/book.replayJournal, error/io-error, error/file-format, concept/books-and-files
from: guide/ch_basics.docbook#basics-backup1
---
red-ale protects your book the way GnuCash does, with two safety nets. Each save first copies the old file to a
timestamped backup. Between saves, every change is also written to a `.log` journal beside the book. If a session ends
before you save, the journal still has the changes, and you can replay them onto the last saved file.

## Steps

1. **Save as usual** with [book.save](method/book.save). The result's `backup` is the path of the copy of the previous
   file (`book.gnucash` becomes `book.gnucash.<timestamp>.gnucash`), and `removed` lists old backups and journals that
   passed their retention. `red-ale exec --retention-days 30` sets how long they are kept (30 by default);
   `--keep-backups-forever` keeps them all.
2. **Find the journal.** [book.info](method/book.info) gives its path as `journal`: a file named
   `<book>.<timestamp>.log`, written while the book is open.
3. **After a crash**, open the last saved book (not a backup, unless you mean to go back in time) and call
   [book.replayJournal](method/book.replayJournal) with the journal's `path`. It creates, updates or deletes each
   transaction by GUID. `created`, `updated` and `deleted` list what it touched, and `notFound` lists records whose
   transaction was missing.
4. **Check and save.** Look at the result and at [book.info](method/book.info), then call
   [book.save](method/book.save) to keep the recovered work.
5. **To go back to an older state**, close the book and copy a backup over it, or open the backup directly. A backup is
   an ordinary book file.

Replay changes transactions only, not accounts. If a journal record needs an account the book no longer has, the replay
refuses that record unless you pass `skipInvalid: true`, which skips and lists it.

## Example

Saving after a change returns the path of the backup; replaying a journal that does not exist is refused cleanly.

```example fixture=reports/test-data
{"method": "account.create", "params": {"name": "Savings", "type": "BANK", "parent": "Root:Asset", "commodity": "USD"}}
{"result": "…"}
{"method": "book.save", "params": {}}
{"result": {"backup": "…", "removed": [], "compressed": false}}
{"method": "book.replayJournal", "params": {"path": "lost-session.log"}}
{"error": {"code": 1008, "message": "…", "data": {"path": "…", "help": "error/io-error"}}}
```
