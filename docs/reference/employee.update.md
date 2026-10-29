<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/employee.update
related: method/employee.create, method/employee.get
---
Changes the fields of an employee you name; fields you leave out stay as they are, and `null` clears
`creditCardAccount`. Employees cannot be deleted; set `active: false`.

## Examples

```example fixture=synthetic/business-editable
{"method": "employee.update", "params": {"employee": "7f0d801fdccd48198abec7b197e55581", "rate": "45.00"}}
{"result": {"guid": "7f0d801fdccd48198abec7b197e55581", "rate": {"num": "4500", "denom": "100"}, "…": "…"}}
```
