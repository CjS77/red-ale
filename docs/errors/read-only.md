<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/read-only
related: method/book.info
summary: The book is open read-only, so it cannot be changed.
---
**Cause.** A method that changes the book was called on a book that is open read-only. `data.reason` says why: it
was opened with `readOnly: true`, or red-ale opened it read-only on its own because it declares a Book feature
red-ale does not implement. [book.info](method/book.info) lists the same `readOnlyReasons`.

**Fix.** If you asked for read-only, close and reopen the book without `readOnly`. If red-ale chose it, the book
stays read-only: writing it could lose data GnuCash would keep, so use GnuCash for those changes. A
change dated before the book's auto read-only threshold is refused separately, as `conflict` or `invalid` with
`data.reason` `auto-read-only` (see [book.setOptions](method/book.setOptions)).
