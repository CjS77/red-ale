<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/backup-failed
related: method/book.save
summary: The backup could not be made, so nothing was written.
---
**Cause.** red-ale copies the previous file to `<book>.<timestamp>.gnucash` before it overwrites it. That copy failed
(no space, no permission), and it will not write without a backup. Nothing was written.

**Fix.** Free space or fix permissions in the book's directory and save again.
