<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.move
related: method/account.update, method/account.delete
---
Moves an account, with its sub-accounts, under another parent as that parent's last child. The new parent's type must
allow the account's type, the parent must not be inside the moved subtree, and no sibling there may have the same
name. Transactions are untouched: they follow the account.

## Examples

```example fixture=reports/test-data
{"method": "account.move", "params": {"account": "Root:Asset:Wallet", "parent": "Root:Asset:Bank"}}
{"result": {"name": "Wallet", "fullName": "Root:Asset:Bank:Wallet", "…": "…"}}
```
