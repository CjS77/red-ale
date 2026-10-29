<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/order.updateEntry
related: method/order.addEntry, method/invoice.updateEntry
---
Changes a line of an open order. It is the same method as [invoice.updateEntry](method/invoice.updateEntry); it is
refused once the order is closed.

## Examples

```example fixture=synthetic/business-editable
{"method": "order.create", "params": {"owner": "5b22073fa4e34411b07c14a77147ce6b"}}
{"result": "…"}
{"method": "order.addEntry", "params": {"order": "00ed0000000000000000000000000001", "description": "Widgets", "quantity": "10", "price": "2.50", "account": "Income"}}
{"result": "…"}
{"method": "order.updateEntry", "params": {"entry": "00ed0000000000000000000000000002", "quantity": "12"}}
{"result": {"quantity": {"num": "12", "denom": "1"}, "…": "…"}}
```
