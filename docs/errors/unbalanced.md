<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/unbalanced
summary: A transaction would not balance, so nothing was changed.
related: method/transaction.post
---
**Cause.** A transaction's split values must sum to exactly zero in its currency, and the call would have left one
that does not. `data.imbalance` is how far off it is, the sum of the values, and `data.currency` the transaction's
currency. The book is unchanged: no part of the call was applied.

**Fix.** Make the values sum to zero: correct the amount that is wrong, or add a split for the difference. GnuCash
would put the difference in an `Imbalance-<currency>` account; red-ale refuses instead, so nothing lands there
unnoticed.
