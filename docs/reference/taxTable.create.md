<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/taxTable.create
related: method/taxTable.update, method/taxTable.list, method/invoice.addEntry
---
Creates a tax table: a named set of lines, each a percentage or a fixed amount posted to a liability account. Invoice
and bill lines refer to a table by GUID, and posting turns the tax into splits to those accounts.

A table needs at least one line (GnuCash deletes an empty table when it next loads the book), a name no other visible
table has, and percentages from -100 to 100. Lines are kept in GnuCash's order, by account full name and then amount,
whatever order you give them.

## Examples

```example fixture=synthetic/business-editable
{"method": "taxTable.create", "params": {"name": "VAT20", "entries": [{"account": "Liabilities:Tax", "amount": "20", "type": "percent"}]}}
{"result": {"guid": "…", "name": "VAT20", "refcount": 0, "entries": [{"accountName": "Liabilities:Tax", "amount": {"num": "20", "denom": "1"}, "type": "percent", "…": "…"}], "…": "…"}}
```
