<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scrub.checkAndRepair
related: method/scrub.imbalance, method/scrub.orphans, method/lot.scrubAccount
---
GnuCash's Check & Repair: the orphan scrub, then the imbalance scrub, over a `transaction`, an `account` (with
`subaccounts`) or the whole book. With `lots: true` it also runs the lot scrub on each account, which GnuCash 5.17 does
only when an environment variable is set, hence the default of false. Run it on a book you did not make before
trusting its balances; it can add `Imbalance` and `Orphan` accounts, so look at `repairs`, then move the amounts
to where they belong.

## Examples

```example fixture=gnucash/taxreport
{"method": "scrub.checkAndRepair", "params": {}}
{"result": {"repairs": [{"kind": "accountCreated", "name": "Imbalance-USD", "commodity": "CURRENCY::USD", "…": "…"}, {"kind": "balanceSplitAdded", "…": "…"}, "…"]}}
```
