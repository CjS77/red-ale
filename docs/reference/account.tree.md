<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.tree
related: method/account.list
---
Gives the account tree as nested nodes, `children` inside each node, for the whole book or for the subtree under
`under`. With `sorted` the siblings come in GnuCash's display order (by code, then type, then name) instead of the
order the book stores them. Pitfall: GnuCash 5.17's own type order puts BANK after TRADING, and red-ale reproduces it
so the two agree.

## Examples

```example fixture=reports/test-data
{"method": "account.tree", "params": {"under": "Root:Asset"}}
{"result": {"fullName": "Root:Asset", "type": "ASSET", "children": [{"fullName": "Root:Asset:Bank", "children": [], "…": "…"}, {"fullName": "Root:Asset:GBP Bank", "…": "…"}, {"fullName": "Root:Asset:Wallet", "…": "…"}], "…": "…"}}
```
