<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/customer.get
related: method/customer.list, method/customer.update
---
Returns one customer by GUID: its address, currency, default billing term and tax table, discount, credit limit and
whether it is active.

## Examples

```example fixture=synthetic/business-editable
{"method": "customer.get", "params": {"customer": "5b22073fa4e34411b07c14a77147ce6b"}}
{"result": {"guid": "5b22073fa4e34411b07c14a77147ce6b", "id": "000001", "name": "Acme & Sons <Ltd>", "currency": "CURRENCY::EUR", "taxIncluded": "no", "credit": {"num": "100000", "denom": "100"}, "active": true, "…": "…"}}
```
