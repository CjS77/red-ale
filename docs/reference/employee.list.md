<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/employee.list
related: method/employee.get, method/customer.list
---
Lists employees, in GUID order, optionally only the active ones, those whose name contains some text (any case) or those
with an exact ID.

## Examples

```example fixture=synthetic/business-editable
{"method": "employee.list", "params": {"activeOnly": true}}
{"result": {"employees": [{"guid": "7f0d801fdccd48198abec7b197e55581", "name": "Jane Doe", "…": "…"}]}}
```
