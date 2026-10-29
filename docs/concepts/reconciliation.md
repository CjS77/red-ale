<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/reconciliation
title: Reconciliation
summary: Checking an Account against a bank statement. Reconcile states, the statement, ticked Splits, the difference that must reach zero, finishing or postponing, and auto-clear. red-ale keeps no reconcile window - the caller carries the ticks.
related: concept/transactions-and-splits, method/reconcile.start, method/reconcile.status, method/reconcile.finish, method/reconcile.postpone, method/reconcile.autoClear, method/reconcile.setState
from: manual/ch_Account-Actions.docbook#acct-reconcile
---
**Reconciliation** double-checks your records and the bank's: it matches the Transactions in an Account with a
statement from the bank or other institution, and shows up differences and outstanding items such as cheques not yet
cashed. You can reconcile any Account that has statements: a bank account, a credit card, a loan. An expense account
has none, so there is nothing to reconcile.

## Reconcile states

Every Split has a state, shown in GnuCash's register as the R column:

| State | Meaning |
|-------|---------|
| `n` | New. The default |
| `c` | Cleared. The bank has processed it but it is not yet reconciled to a statement. Set by hand, or by an import |
| `y` | Reconciled. Only a reconciliation sets this. A reconciled Split warns you before you change it |
| `f` | Frozen. GnuCash does not implement it |
| `v` | Voided. Set by [`transaction.void`](method/transaction.void), never by hand |

[`reconcile.setState`](method/reconcile.setState) sets a Split to `n`, `c`, `y` or `f`. It refuses `v`, and any change to
a Split of a voided Transaction. Setting `y` records the reconcile date.

## The statement

A **statement** is one reconciliation: an Account, a statement date, the ending balance printed on the statement, and
whether the Account's sub-accounts are included. The starting balance is not asked for: it is the total of everything
already reconciled (`y`, `f`).

All reconcile figures use the raw sign: debits positive, credits negative ([Double entry](concept/double-entry)). For a
credit card, the amount you owe is a negative ending balance. GnuCash's window flips the sign for credit Accounts; the
methods do not.

## The steps

1. [`reconcile.start`](method/reconcile.start) fills in GnuCash's defaults (the statement date is a month after the last
   reconciliation, the ending balance the book's balance at that date) and lists the Splits you can tick: every new or
   cleared Split of the Account. Cleared Splits dated on or before the statement date arrive pre-ticked.
2. Compare the list with the statement. **Ticked splits** are the ones that appear on it. Call
   [`reconcile.status`](method/reconcile.status) with your ticks (`ticked`, adding with `mark` and `unmark`) to see the
   balances again: `reconciledBalance` is the starting balance plus the ticked Splits, and
   `difference = endingBalance - reconciledBalance`.
3. When `difference` is zero, [`reconcile.finish`](method/reconcile.finish) turns every ticked Split into `y` with the
   statement date as its reconcile date and records the statement date as the Account's last reconciliation. It refuses
   a non-zero difference unless you pass `force: true`.
4. If you cannot finish, [`reconcile.postpone`](method/reconcile.postpone) saves your place: the ticked Splits become `c`,
   un-ticked cleared ones on or before the date become `n`, and the statement date and balance are kept so the next
   `reconcile.start` resumes from them.

**The caller carries the ticks.** GnuCash holds them in its window until you press Finish or Postpone. red-ale has no
window and keeps nothing between calls: every call to `reconcile.status` and `reconcile.finish` names the ticks. Only
`finish` and `postpone` change the book, each in one step.

A missing Transaction or a wrong amount is fixed with the ordinary methods ([`transaction.post`](method/transaction.post),
[`transaction.update`](method/transaction.update)) between steps; interest or a fee on the statement is an ordinary
Transaction too. red-ale has no "Enter Interest Payment" button.

## Auto-clear

When you only have a figure, [`reconcile.findAutoClear`](method/reconcile.findAutoClear) looks for the one set of
uncleared Splits in an Account that brings its cleared balance to a given amount, and
[`reconcile.autoClear`](method/reconcile.autoClear) marks them cleared. It fails if no set does, or if more than one
set does: it will not guess.

## Last reconcile date

[`reconcile.info`](method/reconcile.info) gives an Account's last reconciliation and the interval used to suggest the
next date; [`reconcile.setInfo`](method/reconcile.setInfo) edits them.
