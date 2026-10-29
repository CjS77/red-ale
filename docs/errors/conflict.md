<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/conflict
summary: The entity exists but may not be changed this way.
---
**Cause.** The change clashes with what is already there: an account, commodity or price that already exists, a
transaction that is read-only or a split that is void, a document that is already posted, a transaction already
posted before the book's auto read-only threshold that `transaction.update`, `delete`, `void` or `unvoid` would
change (`data.reason` `auto-read-only`, with `data.date` and `data.threshold`), a built-in
saved report configuration such as the [Dashboard](report/dashboard) (`data.reason` `built-in`), or a report name
that several reports have (`data.matches`). Nothing was changed.

A date before the threshold that you ask for, as `transaction.post`'s `datePosted` or a new one in
`transaction.update`, is [invalid](error/invalid) instead, with the same `data`: the request is wrong, not the
transaction. GnuCash draws the same line: its register will not let you type such a date, and makes a transaction
already before the threshold read-only.

**Fix.** The message says which. Pick a different name, undo the state that blocks the change (unpost the
document, lower the threshold with [book.setOptions](method/book.setOptions)), copy a built-in configuration with
[report.saveConfiguration](method/report.saveConfiguration) and change the copy, name a report by its GUID, or leave
the record as it is.
