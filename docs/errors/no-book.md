<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/no-book
related: method/book.open, method/book.create
summary: No book is open.
---
**Cause.** The method works on a book and the server has none open. A server holds one book at a time, and starts
with none unless it was given `--book`.

**Fix.** Call [book.open](method/book.open) with the path of an existing book, or [book.create](method/book.create)
for a new one. `red-ale exec --book <file>` opens it for you. Methods that need no book (`help.*`, `config.*`,
`period.*`) never fail this way.
