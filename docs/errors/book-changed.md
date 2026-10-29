<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/book-changed
related: method/book.save
summary: Another program changed the SQLite book since red-ale read it.
---
**Cause.** Another program wrote the SQLite book, or broke its lock, after red-ale opened it or last saved it.
`data.path` is the book and `data.detail` what differed. red-ale writes nothing, so it cannot overwrite the other
program's changes.

**Fix.** Close the book discarding your changes and open it again; make your edits again on the new state. Keep
GnuCash and red-ale from holding the same book at once.
