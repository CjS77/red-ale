<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/io-error
summary: A file operation failed.
---
**Cause.** Reading or writing a file failed at the operating system: the file is missing or unreadable, the disk is
full, or permissions forbid it. The message carries the system's reason and the path.

**Fix.** Fix the path or permissions named in the message and call again. For `book.save`, the book on disk is
unchanged and the book in memory still holds your changes.
