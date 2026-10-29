<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.types
related: method/account.create, method/account.update
---
Lists GnuCash's account types, with each one's label, its fundamental type (asset, liability, income, expense or
equity), the kind of commodity it holds, the types it can sit under (`parentTypes`) and the types it can be changed
to once it has splits (`changesWithSplits`). Call it to find out what `type` values [account.create](method/account.create)
accepts. A type with `choosable: false` is deprecated and cannot be chosen.

`STOCK` and `MUTUAL` hold securities; the others hold a currency. Receivable and Payable accounts cannot change type
once they have splits.

## Examples

```example
{"method": "account.types", "params": {}}
{"result": [{"name": "BANK", "label": "Bank", "fundamental": "ASSET", "choosable": true, "commodity": "currency", "…": "…"}, "…"]}
```
