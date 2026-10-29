<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/customer.create
related: method/customer.update, method/customer.list, method/billTerm.create, method/taxTable.create, method/job.create
---
Creates a customer: someone you send invoices to. It needs a company `name` and a `currency`, the currency of all its
documents; everything else (address, default billing term, tax table, discount, credit limit) is optional.

Leave `id` out and the customer is numbered from the book's `gncCustomer` counter (`000001`, `000002`, …); see
[counter.set](method/counter.set) to change the counter or its format. As in GnuCash, IDs are labels, not keys: nothing
stops two customers having the same one, and the GUID is what identifies a customer. `terms` and `taxTable` take a
GUID or the name of a visible billing term or tax table.

## Examples

```example fixture=synthetic/business-editable
{"method": "customer.create", "params": {"name": "Globex", "currency": "EUR", "terms": "Net30", "discount": "10"}}
{"result": {"guid": "…", "id": "000002", "name": "Globex", "currency": "CURRENCY::EUR", "terms": "3416d941790749378beeb338b8357a72", "discount": {"num": "10", "denom": "1"}, "active": true, "…": "…"}}
```
