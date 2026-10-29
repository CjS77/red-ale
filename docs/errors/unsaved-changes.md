<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/unsaved-changes
related: method/book.close, method/book.save
summary: The book has changes not yet saved.
---
**Cause.** `book.close` was called on a book with unsaved changes and with neither `save` nor `discard`. The book
stays open.

**Fix.** Call [book.close](method/book.close) with `save: true` to write the changes, or `discard: true` to drop them.
