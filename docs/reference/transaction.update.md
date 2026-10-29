<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/transaction.update
related: method/transaction.post, method/transaction.void, error/unbalanced, error/conflict
---
Changes a transaction: its date, description, number, notes, currency or splits. Give `removeSplits` (GUIDs),
`changeSplits` (a split's `guid` and the fields to change) and `addSplits` (as in [transaction.post](method/transaction.post));
fields you leave out are kept. The steps run in that order and the result must balance and keep a split, or the whole
call is refused and nothing changes; use [transaction.delete](method/transaction.delete) to remove a transaction.

When a split's value changes in an account of the transaction's currency, its amount follows. Moving a split to an
account of another commodity needs a new `amount`. A read-only transaction (voided, or before the book's auto
read-only threshold) is refused with `conflict`. Changing the transaction's currency re-values the splits as GnuCash
does, except that a split in a third commodity is multiplied by the rate rather than divided, which keeps the
transaction balanced where GnuCash's result needs an `Imbalance` split.

## Examples

```example fixture=reports/test-data
{"method": "transaction.update", "params": {"guid": "c0000000000000000000000000000005", "description": "Fuel", "notes": "corrected"}}
{"result": {"guid": "c0000000000000000000000000000005", "description": "Fuel", "notes": "corrected", "balanced": true, "…": "…"}}
```

A change that unbalances the transaction is refused:

```example fixture=reports/test-data
{"method": "transaction.update", "params": {"guid": "c0000000000000000000000000000005", "changeSplits": [{"guid": "d000000000000000000000000000000b", "value": "12.00"}]}}
{"error": {"code": 2002, "message": "…", "data": {"imbalance": "…", "currency": "…", "help": "error/unbalanced"}}}
```
