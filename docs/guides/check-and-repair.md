<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/check-and-repair
title: Check and repair a book
summary: Find and fix splits with no account, unbalanced transactions and broken lots with scrub.checkAndRepair, and know what each repair changed.
related: method/scrub.checkAndRepair, method/scrub.imbalance, method/scrub.orphans, method/scrub.currency, method/lot.scrub, method/lot.scrubAccount, method/book.save
---
A book that came from another program, an import with mistakes, or an old file can contain transactions that do not
balance, or splits that point at an account that no longer exists. red-ale refuses to create these, but it can open a
book that has them. A *scrub* finds them and repairs them in the way GnuCash's Check & Repair does, so GnuCash agrees
with the result.

Every repair is reported, so you can see what was changed before you save.

## What the scrubs do

- **Orphans.** A split with no account is moved to an account called `Orphan-<currency>`, which is created if needed.
  ([scrub.orphans](method/scrub.orphans))
- **Imbalances.** A transaction whose values do not sum to zero gets one more split, in an account called
  `Imbalance-<currency>`, for the difference. The books then balance, and the Imbalance account shows what still needs a
  real home. ([scrub.imbalance](method/scrub.imbalance))
- **Lots.** Splits of a security account are assigned to lots and each lot's capital gains recomputed.
  ([lot.scrub](method/lot.scrub), [lot.scrubAccount](method/lot.scrubAccount)) See
  [Capital gains with lots](guide/capital-gains-with-lots).

## Steps

1. **Back up first.** A repair changes the book in memory, but you will want a copy of the file you started from. Save
   only after you have looked at the result.
2. **Run it** with [scrub.checkAndRepair](method/scrub.checkAndRepair). With no parameters it checks every transaction.
   Narrow it with `account` (and `subaccounts`) or a single `transaction`. Add `lots: true` to repair lots too, which
   GnuCash does not do by default.
3. **Read the `repairs`.** Each has a `kind` (`accountCreated`, `balanceSplitAdded`, and so on) and the transaction,
   split and account involved. An empty list means the book was clean.
4. **Finish the job by hand.** An `Imbalance-USD` account is a to-do list, not an answer. For each split in it, decide
   what the money really was, move the split to the right account with
   [transaction.update](method/transaction.update), and delete the Imbalance account when it is empty
   ([account.delete](method/account.delete)).
5. **Run it again.** A second run reports no repairs.
6. **Save** with [book.save](method/book.save).

To repair just one transaction, call [scrub.imbalance](method/scrub.imbalance) with its `transaction` GUID.

## Example

This GnuCash test book has an unbalanced transaction. The first run adds an Imbalance account and a balancing split; the
second finds nothing to do.

```example fixture=gnucash/taxreport
{"method": "scrub.checkAndRepair", "params": {}}
{"result": {"repairs": [{"kind": "accountCreated", "name": "Imbalance-USD", "commodity": "CURRENCY::USD", "…": "…"}, {"kind": "balanceSplitAdded", "…": "…"}, "…"]}}
{"method": "scrub.checkAndRepair", "params": {}}
{"result": {"repairs": []}}
```
