<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/billTerm.delete
related: method/billTerm.list, method/billTerm.get
---
Deletes a visible billing term that nothing uses. One that a customer, vendor or document refers to is refused with
`conflict`. Frozen copies stay in the book.

## Examples

```example fixture=synthetic/business-editable
{"method": "billTerm.delete", "params": {"billTerm": "Net30"}}
{"error": {"code": 2003, "message": "billing term \"Net30\" is in use and cannot be deleted", "data": {"help": "error/conflict"}}}
{"method": "billTerm.create", "params": {"name": "Net14", "type": "days", "dueDays": 14}}
{"result": "…"}
{"method": "billTerm.delete", "params": {"billTerm": "Net14"}}
{"result": {}}
```
