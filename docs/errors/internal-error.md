<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/internal-error
summary: A bug in red-ale.
---
**Cause.** red-ale hit a state it did not expect. The book is not changed by the failed call; a mutation applies
completely or not at all.

**Fix.** There is nothing to fix in the request. Repeat it once; if it fails again, keep the request and the book
(a copy, never your only one) and report the bug.
