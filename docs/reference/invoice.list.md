<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.list
related: method/invoice.get, method/invoice.due
---
Lists documents of every kind, without their lines, in GUID order. Filter by `type` (`invoice`, `bill`,
`expenseVoucher`, `creditNote`), by owner (a customer's list includes its jobs' documents), by posted or unposted, by
active, or by exact number. Numbers are only unique within a counter, so `id` may match an invoice and a bill.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.list", "params": {"type": "expenseVoucher"}}
{"result": {"invoices": [{"id": "000001", "type": "expenseVoucher", "posted": true, "entryCount": 1, "…": "…"}]}}
{"method": "invoice.list", "params": {"posted": false}}
{"result": {"invoices": [{"guid": "71803cac5d8e4dceabfd0d56ec5e7521", "id": "000002", "posted": false, "…": "…"}]}}
```
