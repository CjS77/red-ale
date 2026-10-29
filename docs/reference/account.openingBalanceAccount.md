<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.openingBalanceAccount
related: method/account.create, method/account.update
---
Finds the account GnuCash would put a currency's opening balances in: the equity account flagged as the opening
balance account for that currency, else one named like GnuCash's `Opening Balances` account. It returns
`account: null` when there is none. GnuCash creates the account when it is missing; red-ale only looks, because a read
never writes. To make one, create an `EQUITY` account with `openingBalance: true`.

## Examples

```example
{"method": "account.openingBalanceAccount", "params": {"currency": "USD"}}
{"result": {"account": null, "foundBy": null, "featureUsed": false}}
{"method": "account.create", "params": {"name": "Opening Balances", "type": "EQUITY", "commodity": "USD", "openingBalance": true}}
{"result": "…"}
{"method": "account.openingBalanceAccount", "params": {"currency": "USD"}}
{"result": {"account": {"fullName": "Opening Balances", "openingBalance": true, "…": "…"}, "featureUsed": true, "…": "…"}}
```
