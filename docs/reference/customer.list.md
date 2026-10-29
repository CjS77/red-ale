<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/customer.list
related: method/customer.get, method/vendor.list, method/job.list
---
Lists customers, in GUID order, optionally only the active ones or those whose name contains some text (any case) or
whose ID matches exactly. Use it to find a customer's GUID from what you know of it.

Deactivated customers are listed unless `activeOnly` is set.

## Examples

```example fixture=synthetic/business-editable
{"method": "customer.list", "params": {"name": "acme"}}
{"result": {"customers": [{"guid": "5b22073fa4e34411b07c14a77147ce6b", "name": "Acme & Sons <Ltd>", "…": "…"}]}}
{"method": "customer.list", "params": {"name": "globex"}}
{"result": {"customers": []}}
```
