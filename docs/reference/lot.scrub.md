<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.scrub
related: method/lot.scrubAccount, method/lot.computeGains
---
Repairs one lot, as GnuCash's lot scrub does: merges split pieces that were cut from one split, refills an overfull
lot, and recomputes its capital gains, recording them in gains transactions in the gains account (an income account
named `Orphaned Gains-<CUR>` unless the account names another). The result lists what was created, changed and
deleted. It changes nothing in a lot that is already right.

## Examples

```example fixture=reports/lots-baseline
{"method": "lot.scrub", "params": {"lot": "10000000000000000000000000000000"}}
{"result": {"lotsCreated": [], "lotsDeleted": [], "transactionsCreated": [], "transactionsChanged": [], "transactionsDeleted": [], "accountsCreated": []}}
```
