<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scrub.imbalance
related: method/scrub.checkAndRepair, method/scrub.orphans, error/unbalanced
---
Balances unbalanced transactions the way GnuCash does: it adds a split in an `Imbalance-<CUR>` account (made under the
root if missing) for the difference, after fixing orphan splits and split amounts. Scope it with `transaction`, or
`account` (and `subaccounts: true`); with neither it covers the whole book. Each change is listed in `repairs`.

red-ale never creates imbalances itself: it refuses an unbalanced transaction with [unbalanced](error/unbalanced). This
is for books that arrive unbalanced, from GnuCash or an import. GnuCash runs scrubs when it loads a book; red-ale keeps
a book as read and scrubs only on request. The account is named `Imbalance-<CUR>` in English even if your GnuCash runs
in another language.

## Examples

```example fixture=gnucash/taxreport
{"method": "scrub.imbalance", "params": {"transaction": "80f52bea76850f1d0563e43e87c1c109"}}
{"result": {"repairs": [{"kind": "accountCreated", "account": "…", "name": "Imbalance-USD", "commodity": "CURRENCY::USD"}, {"kind": "balanceSplitAdded", "transaction": "80f52bea76850f1d0563e43e87c1c109", "split": "…", "account": "…", "value": {"num": "2345", "denom": "100"}}]}}
```
