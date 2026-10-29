<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/customer.update
related: method/customer.create, method/customer.get
---
Changes the fields of a customer you name; fields you leave out stay as they are. `null` clears a field that can be
empty (`terms`, `taxTable`), and `addr` and `shipAddr` merge field by field, so `{"addr": {"phone": "555-9999"}}`
changes the phone number alone.

There is no way to delete a customer, as there is none in GnuCash: its documents would be left without an owner.
Set `active: false` instead. A deactivated customer is still in the book and keeps its documents.

## Examples

```example fixture=synthetic/business-editable
{"method": "customer.update", "params": {"customer": "5b22073fa4e34411b07c14a77147ce6b", "active": false, "addr": {"phone": "555-9999"}}}
{"result": {"guid": "5b22073fa4e34411b07c14a77147ce6b", "addr": {"name": "Acme Billing", "addr1": "1 Main St", "phone": "555-9999", "…": "…"}, "active": false, "…": "…"}}
{"method": "customer.list", "params": {"activeOnly": true}}
{"result": {"customers": []}}
```
