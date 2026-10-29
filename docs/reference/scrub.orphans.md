<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scrub.orphans
related: method/scrub.checkAndRepair, method/scrub.imbalance
---
Puts splits that have no account (or an account the book does not contain) into an `Orphan-<CUR>` account, made if
missing, and balances each transaction it changed. Scope it with `transaction`, or `account` and `subaccounts`; with
neither it covers the whole book. A clean book gives an empty `repairs` list.

## Examples

```example fixture=gnucash/taxreport
{"method": "scrub.orphans", "params": {}}
{"result": {"repairs": []}}
```
