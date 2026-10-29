<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/order.create
related: method/order.addEntry, method/order.close, method/customer.create
---
Creates an open order for a customer, vendor or job: a list of lines promised to or by someone, which invoice lines can
join. It takes the next number from the `gncOrder` counter unless you give `id`. An order has no totals and no
posting.

Order lines use the invoice side whoever owns the order, because GnuCash's order screen is a customer-document screen.
Only a customer invoice's line can join an order.

## Examples

```example fixture=synthetic/business-editable
{"method": "order.create", "params": {"owner": "5b22073fa4e34411b07c14a77147ce6b", "reference": "PO-9"}}
{"result": {"guid": "…", "id": "000001", "owner": {"kind": "customer", "guid": "5b22073fa4e34411b07c14a77147ce6b"}, "closed": false, "reference": "PO-9", "entryCount": 0, "entries": [], "…": "…"}}
```
