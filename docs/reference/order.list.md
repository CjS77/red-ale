<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/order.list
related: method/order.get, method/order.close
---
Lists orders, in GUID order and without their lines, optionally those of one owner (a customer's list includes its jobs'
orders), only the open or only the closed, or only the active.

## Examples

```example fixture=synthetic/business-editable
{"method": "order.list", "params": {}}
{"result": {"orders": []}}
```
