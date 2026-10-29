<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/order.get
related: method/order.list, method/order.update
---
Returns one order with its lines.

## Examples

```example fixture=synthetic/business-editable
{"method": "order.create", "params": {"owner": "5b22073fa4e34411b07c14a77147ce6b"}}
{"result": "…"}
{"method": "order.get", "params": {"order": "00ed0000000000000000000000000001"}}
{"result": {"id": "000001", "closed": false, "dateClosed": null, "entryCount": 0, "entries": [], "…": "…"}}
```
