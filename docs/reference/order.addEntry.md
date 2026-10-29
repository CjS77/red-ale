<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/order.addEntry
related: method/order.updateEntry, method/order.removeEntry, method/invoice.addEntry
---
Adds a line to an open order. The line uses the invoice side and gets only the engine's defaults, not the owner's
discount or tax table. To put a line on an order and a customer invoice at once, use
[invoice.addEntry](method/invoice.addEntry) with `order`.

## Examples

```example fixture=synthetic/business-editable
{"method": "order.create", "params": {"owner": "5b22073fa4e34411b07c14a77147ce6b"}}
{"result": "…"}
{"method": "order.addEntry", "params": {"order": "00ed0000000000000000000000000001", "description": "Widgets", "quantity": "10", "price": "2.50", "account": "Income"}}
{"result": {"guid": "…", "description": "Widgets", "quantity": {"num": "10", "denom": "1"}, "invoicePrice": {"num": "250", "denom": "100"}, "invoice": null, "order": "00ed0000000000000000000000000001", "…": "…"}}
```
