<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/file-format
summary: The file is not a readable GnuCash XML book or journal.
---
**Cause.** The file does not parse as a GnuCash 2.x-or-later XML book (plain or gzipped), as an SQLite book, or, for
`book.replayJournal`, as a GnuCash `.log` journal. GnuCash version 1 files are in this group too.

**Fix.** Check that you named the right file. Open a very old file in GnuCash and save it again, which upgrades it.
A file damaged by a failed copy needs its backup (`<book>.<timestamp>.gnucash`).
