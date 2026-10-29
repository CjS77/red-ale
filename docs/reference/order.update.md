<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/order.update
related: method/order.create, method/order.close
---
Changes an order's number, dates, notes, reference or active state. The owner can change only while the order has no
lines. To change its lines use [order.updateEntry](method/order.updateEntry).

## Examples

```example fixture=synthetic/business-editable
{"method": "order.create", "params": {"owner": "5b22073fa4e34411b07c14a77147ce6b"}}
{"result": "…"}
{"method": "order.update", "params": {"order": "00ed0000000000000000000000000001", "notes": "Rush"}}
{"result": {"notes": "Rush", "closed": false, "…": "…"}}
```
