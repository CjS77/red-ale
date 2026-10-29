<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/commodity.list
related: method/commodity.get, method/commodity.namespaces
---
Lists commodities, sorted by namespace and then mnemonic: the book's own and every built-in currency it does not
define. `namespace` limits the list to one namespace; `usedOnly` keeps only those something in the book refers to
(an account, a transaction currency, a price, a business object). Each entry says how much of the book uses it.

## Examples

```example fixture=reports/test-data
{"method": "commodity.list", "params": {"usedOnly": true}}
{"result": [{"id": "CURRENCY::GBP", "fullname": "Pound Sterling", "fraction": 100, "isCurrency": true, "usage": {"accounts": 2, "transactions": 13, "prices": 2, "business": 0}, "…": "…"}, {"id": "CURRENCY::USD", "fullname": "US Dollar", "fraction": 100, "usage": {"accounts": 8, "transactions": 46, "prices": 2, "business": 0}, "…": "…"}]}
```
