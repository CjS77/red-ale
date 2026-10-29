<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.scrubAccount
related: method/lot.scrub, method/lot.assignSplit, method/scrub.checkAndRepair
---
Assigns an account's splits to lots by FIFO and repairs every lot, as GnuCash's Scrub Account does for lots; with
`subaccounts: true` it does the whole subtree. Only accounts with trades are scrubbed (stock and mutual fund
accounts, and any account with a split in another commodity than its own). Use it once on an account that has never
had lots; run again, it changes nothing.

## Examples

```example fixture=reports/investment-lots
{"method": "lot.scrubAccount", "params": {"account": "Root:Assets:Broker:AAPL"}}
{"result": {"lotsCreated": ["…", "…"], "lotsDeleted": [], "transactionsChanged": ["…", "…", "…"], "…": "…"}}
{"method": "lot.list", "params": {"account": "Root:Assets:Broker:AAPL"}}
{"result": [{"title": "Lot 0", "balance": {"num": "40", "denom": "1"}, "splitCount": 2, "…": "…"}, {"title": "Lot 1", "balance": {"num": "50", "denom": "1"}, "splitCount": 1, "…": "…"}]}
```
