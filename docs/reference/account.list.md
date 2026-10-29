<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.list
related: method/account.tree, method/account.get
---
Lists accounts depth first, each followed by its sub-accounts, in the order GnuCash keeps and writes them. Give
`under` to list one subtree (the account itself is left out) and `excludeHidden` to skip hidden accounts. Use
[account.tree](method/account.tree) when you want the nesting rather than a flat list.

`nameViolations` lists accounts whose names contain the separator `:`, which GnuCash warns about when it opens such a
book. red-ale refuses to create one.

## Examples

```example fixture=reports/test-data
{"method": "account.list", "params": {"under": "Root:Asset"}}
{"result": {"accounts": [{"fullName": "Root:Asset:Bank", "type": "ASSET", "…": "…"}, {"fullName": "Root:Asset:GBP Bank", "commodity": "CURRENCY::GBP", "…": "…"}, {"fullName": "Root:Asset:Wallet", "…": "…"}], "nameViolations": []}}
```
