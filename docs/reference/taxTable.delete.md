<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/taxTable.delete
related: method/taxTable.get, method/taxTable.list
---
Deletes a visible tax table that nothing uses. One with a `refcount` above zero (a customer, vendor or document line
refers to it) is refused with `conflict`. Its frozen copies stay in the book.

## Examples

```example fixture=synthetic/business-editable
{"method": "taxTable.delete", "params": {"taxTable": "GST+Levy"}}
{"error": {"code": 2003, "message": "tax table \"GST+Levy\" is in use and cannot be deleted", "data": {"help": "error/conflict"}}}
{"method": "taxTable.create", "params": {"name": "Spare", "entries": [{"account": "Liabilities:Tax", "amount": "5", "type": "percent"}]}}
{"result": "…"}
{"method": "taxTable.delete", "params": {"taxTable": "Spare"}}
{"result": {}}
```
