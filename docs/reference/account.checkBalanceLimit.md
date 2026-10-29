<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.checkBalanceLimit
related: method/account.update, method/account.balance
---
Compares an account's balance with its higher and lower balance limits, as GnuCash's account tree does to show a
warning icon. `state` is `above`, `below`, `shouldBeZero` or null when no limit is crossed. The balance is at the end
of `asOf` (default today); with `includeSubaccountBalances` set on the account, sub-accounts are converted at their
latest price, and the ones without a price are listed in `unconverted` and counted as zero.

Set limits with [account.update](method/account.update) (`higherBalanceLimit`, `lowerBalanceLimit`).

## Examples

```example fixture=reports/test-data now=2024-07-01T12:00:00Z
{"method": "account.update", "params": {"account": "Root:Asset:Bank", "higherBalanceLimit": "1000.00"}}
{"result": "…"}
{"method": "account.checkBalanceLimit", "params": {"account": "Root:Asset:Bank"}}
{"result": {"asOf": "2024-07-01", "balance": {"num": "218600", "denom": "100"}, "higherLimit": {"num": "100000", "denom": "100"}, "lowerLimit": null, "state": "above", "…": "…"}}
```
