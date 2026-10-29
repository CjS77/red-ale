<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.cascade
related: method/account.update
---
Sets the colour, the placeholder flag or the hidden flag on an account and all its sub-accounts at once, as the
account tree's "cascade" action does, and returns the GUIDs of the accounts that changed. A colour is `#rgb`,
`#rrggbb` or `rgb(r,g,b)`; it is stored as `rgb(r,g,b)`. By default sub-accounts that already have a colour keep
it; pass `replaceColors: true` to overwrite them. The account must have sub-accounts (`invalid` otherwise).

## Examples

```example fixture=reports/test-data
{"method": "account.cascade", "params": {"account": "Root:Asset", "color": "#336699", "hidden": false}}
{"result": {"changed": ["…", "…", "…", "…"]}}
{"method": "account.get", "params": {"account": "Root:Asset:Bank"}}
{"result": {"color": "rgb(51,102,153)", "…": "…"}}
```
