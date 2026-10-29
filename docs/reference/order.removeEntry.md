<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/order.removeEntry
related: method/order.addEntry, method/invoice.removeEntry
---
Deletes a line of an open order. It is the same method as [invoice.removeEntry](method/invoice.removeEntry); it is
refused once the order is closed.

## Examples

```example fixture=synthetic/business-editable
{"method": "order.create", "params": {"owner": "5b22073fa4e34411b07c14a77147ce6b"}}
{"result": "…"}
{"method": "order.addEntry", "params": {"order": "00ed0000000000000000000000000001", "description": "Widgets", "quantity": "10", "price": "2.50", "account": "Income"}}
{"result": "…"}
{"method": "order.removeEntry", "params": {"entry": "00ed0000000000000000000000000002"}}
{"result": {}}
```
