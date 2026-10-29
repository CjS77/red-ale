<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/conflict
summary: The entity exists but may not be changed this way.
---
**Cause.** The change clashes with what is already there: an account, commodity or price that already exists, a
transaction that is read-only or a split that is void, a document that is already posted, or a date before the
book's auto read-only threshold (`data.reason` `auto-read-only`, with `data.date` and `data.threshold`). Nothing was
changed.

**Fix.** The message says which. Pick a different name, undo the state that blocks the change (unpost the
document, lower the threshold with [book.setOptions](method/book.setOptions)), or leave the record as it is.
