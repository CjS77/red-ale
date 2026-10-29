<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.list
related: method/lot.get, method/lot.create, method/lot.scrubAccount
---
Lists an account's lots as GnuCash's lot viewer shows them: opened earliest first, empty lots last. A lot groups the
splits of one account that buy and later sell the same holding; it is `closed` when it has splits and their amounts
sum to zero. `state` is `all` (the default), `open` or `closed`. Each lot shows its balance, its realized gains and
how many splits it holds.

A lot's `title` is null until it is set; lots made by red-ale are titled `Lot <n>`.

## Examples

```example fixture=reports/lots-baseline
{"method": "lot.list", "params": {"account": "Root:Assets:Broker:AAPL"}}
{"result": [{"guid": "10000000000000000000000000000000", "title": null, "closed": false, "openDate": "2020-01-01", "balance": {"num": "100", "denom": "1"}, "splitCount": 1, "…": "…"}]}
```
