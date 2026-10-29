<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/file-exists
related: method/book.create
summary: The file to create already exists.
---
**Cause.** `book.create` was given a path where a file already exists. red-ale never overwrites a book on creation.

**Fix.** Choose another path, or move or delete the old file yourself if you really mean to replace it.
