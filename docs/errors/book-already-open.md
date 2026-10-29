<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/book-already-open
related: method/book.close
summary: A book is already open in this server.
---
**Cause.** `book.open` or `book.create` was called while the server holds a book. One server holds one book.

**Fix.** Close the open one with [book.close](method/book.close) (with `save` or `discard` if it has changes), then
open the other. Or start a second server for the second book.
