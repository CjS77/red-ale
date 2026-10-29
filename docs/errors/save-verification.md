<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/save-verification
related: method/book.save
summary: The file written did not read back as the same book, so it was removed.
---
**Cause.** After writing, red-ale reads the new file back and compares it with the book in memory. They differed, so
it removed the new file and kept the old one: the book on disk is unchanged. For an SQLite book the save's SQL
transaction was rolled back.

**Fix.** Your changes are still in memory. Try [book.save](method/book.save) again, perhaps to a better disk; if it
keeps failing, report it with the book, because red-ale found that it could not write something faithfully.
