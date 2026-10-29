<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/cannot-lock
related: method/book.open
summary: The lock could not be taken because the location is not writable.
---
**Cause.** red-ale could not create the `.LCK` file beside an XML book, or could not write the SQLite book or its
directory, so it cannot take the lock that makes writing safe. Nothing was opened.

**Fix.** Make the directory writable (or the file, for SQLite), or open the book with `readOnly: true`, which needs
no lock. A book on a read-only mount can only be opened read-only.
