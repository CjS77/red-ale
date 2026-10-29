<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/commodity.namespaces
related: method/commodity.list, method/commodity.create
---
Lists the commodity namespaces with how many commodities each holds. `CURRENCY` holds the 227 built-in ISO 4217
currencies; other namespaces are exchanges or types you create securities in (`NASDAQ`, `FUND`). `CURRENCY` and
`template` are reserved: no security can be created in them. Note that GnuCash's old name for the currency namespace,
`ISO4217`, is accepted as `CURRENCY`.

## Examples

```example
{"method": "commodity.namespaces", "params": {}}
{"result": [{"name": "CURRENCY", "commodities": 227, "isCurrency": true, "reserved": true}, {"name": "template", "commodities": 1, "isCurrency": false, "reserved": true}]}
{"method": "commodity.create", "params": {"namespace": "NASDAQ", "mnemonic": "AAPL", "fullname": "Apple Inc"}}
{"result": "…"}
{"method": "commodity.namespaces", "params": {}}
{"result": [{"name": "CURRENCY", "…": "…"}, {"name": "NASDAQ", "commodities": 1, "isCurrency": false, "reserved": false}, "…"]}
```
