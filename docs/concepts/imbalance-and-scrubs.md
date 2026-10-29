<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/imbalance-and-scrubs
title: Imbalance, orphans and scrubs
summary: What the Imbalance-CUR and Orphan-CUR Accounts are, why red-ale refuses an unbalanced Transaction instead of creating them, and how scrub.checkAndRepair puts right a book that already has them.
related: concept/double-entry, concept/transactions-and-splits, method/scrub.checkAndRepair, method/scrub.imbalance, method/scrub.orphans, method/scrub.currency, error/unbalanced
---
A Transaction's Split values should sum to exactly zero ([Double entry](concept/double-entry)). GnuCash enforces that
by repairing: when a Transaction does not balance it adds a Split in an Account named `Imbalance-USD` (one per
currency) for the difference. A Split with no Account at all is moved to `Orphan-USD`. These repairs are called
**scrubs**.

## What red-ale does differently

- **It refuses, rather than repairs.** [`transaction.post`](method/transaction.post) and the other methods that change
  Transactions refuse one that does not balance, with [`unbalanced`](error/unbalanced) naming the difference, and change
  nothing. An `Imbalance-USD` Account never appears because of something you did through red-ale.
- **It never scrubs on its own.** GnuCash scrubs a book as it loads it. red-ale opens a book exactly as it is, and a book
  it opens can still hold an orphan Split, a Split whose amount disagrees with its value, or an unbalanced Transaction
  that GnuCash would quietly fix. Nothing is changed unless you ask.

## Check and repair

Run a scrub when a book comes from elsewhere (an import, an old GnuCash book) and you want it clean:

| Method | Does |
|--------|------|
| [`scrub.orphans`](method/scrub.orphans) | Moves Splits with no Account to `Orphan-<CUR>` |
| [`scrub.currency`](method/scrub.currency) | Gives a Transaction whose currency is not a currency the common currency of its Splits |
| [`scrub.imbalance`](method/scrub.imbalance) | Balances unbalanced Transactions with an `Imbalance-<CUR>` Split, after fixing orphans and Split amounts |
| [`scrub.checkAndRepair`](method/scrub.checkAndRepair) | GnuCash's Check & Repair: orphans, then imbalances, and with `lots: true` the [Lot](concept/lots-and-capital-gains) scrub |

Each takes one Transaction, or one Account (with `subaccounts: true` for its descendants), or neither for the whole
book. Each is atomic: it repairs everything it reports or nothing. The result lists every change made, in order, as
`repairs`: an Account created, a Split assigned, a balancing Split added, and so on. All four are marked destructive
because a repair can remove a Split.

## After a repair

`Imbalance-USD` is a real Account, of type `BANK`, under the root. It holds what was out of balance; it is not an
answer. Open each Transaction that has a Split in it, work out where the money went, and move the Split to the right
Account with [`transaction.update`](method/transaction.update). When the Account is empty, delete it
([`account.delete`](method/account.delete)). Do the same for `Orphan-USD`.

An import can also leave an imbalance when a file's Transactions do not balance; the import proposal reports it before
you apply anything ([Importing and matching](concept/importing-and-matching)).
