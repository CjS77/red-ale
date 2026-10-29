<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.create
related: method/lot.update, method/lot.assignSplit, method/lot.delete
---
Makes an empty lot in an account. Its title is `Lot <n>` from the account's counter unless you give `title`, and
you can give `notes`. Put splits in it with [lot.assignSplit](method/lot.assignSplit). You rarely need this:
assigning a split without naming a lot opens one for you.

## Examples

```example fixture=reports/investment-lots
{"method": "lot.create", "params": {"account": "Root:Assets:Broker:AAPL"}}
{"result": {"guid": "…", "title": "Lot 0", "closed": false, "balance": {"num": "0", "denom": "1"}, "splitCount": 0, "…": "…"}}
```
