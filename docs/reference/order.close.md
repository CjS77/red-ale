<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/order.close
related: method/order.create, method/order.addEntry
---
Closes an order that has at least one line. A closed order is frozen: no line can be added, changed or removed, and it
cannot be closed again. This is the only way to set its closing date, which defaults to now.

## Examples

```example fixture=synthetic/business-editable
{"method": "order.create", "params": {"owner": "5b22073fa4e34411b07c14a77147ce6b"}}
{"result": "…"}
{"method": "order.addEntry", "params": {"order": "00ed0000000000000000000000000001", "description": "Widgets", "quantity": "10", "price": "2.50", "account": "Income"}}
{"result": "…"}
{"method": "order.close", "params": {"order": "00ed0000000000000000000000000001", "date": "2024-07-02T09:00:00Z"}}
{"result": {"closed": true, "dateClosed": "2024-07-02T09:00:00Z", "entryCount": 1, "…": "…"}}
{"method": "order.addEntry", "params": {"order": "00ed0000000000000000000000000001", "description": "More"}}
{"error": {"code": 2003, "message": "order 000001 is closed", "data": {"help": "error/conflict"}}}
```
