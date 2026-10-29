<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/vendor.create
related: method/vendor.update, method/vendor.list, method/billTerm.create
---
Creates a vendor: someone you receive bills from. It needs a company `name` and a `currency`, the currency of its bills.
Unlike a customer, a vendor has one address, where you pay it, and no discount or credit limit.

Leave `id` out and the vendor is numbered from the book's `gncVendor` counter. `terms` and `taxTable` take a GUID or the
name of a visible billing term or tax table.

## Examples

```example fixture=synthetic/business-editable
{"method": "vendor.create", "params": {"name": "Ink Co", "currency": "EUR", "id": "V-100"}}
{"result": {"guid": "…", "id": "V-100", "name": "Ink Co", "currency": "CURRENCY::EUR", "terms": null, "taxIncluded": "useGlobal", "active": true, "…": "…"}}
```
