<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.create
related: method/transaction.post, method/account.balance
---
Adds an account to the open book, under the root or under the `parent` you name, as GnuCash's New Account dialog
does. Give it a `name`, a `type` (`BANK`, `CASH`, `ASSET`, `CREDIT`, `LIABILITY`, `STOCK`, `MUTUAL`, `INCOME`,
`EXPENSE`, `EQUITY`, …) and the `commodity` its amounts are in; a currency can be named by its code alone (`USD`).
Accounts are then named by their full name, levels joined with `:`, or by the GUID this method returns.

## Examples

A bank account under a new top-level asset account:

```example
{"method": "account.create", "params": {"name": "Assets", "type": "ASSET", "commodity": "USD", "placeholder": true}}
{"result": {"guid": "…", "name": "Assets", "fullName": "Assets", "placeholder": true, "…": "…"}}
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "parent": "Assets", "commodity": "USD"}}
{"result": {"guid": "…", "name": "Checking", "fullName": "Assets:Checking", "type": "BANK", "parent": "…", "commodity": "CURRENCY::USD", "scu": 100, "depth": 2, "…": "…"}}
```
