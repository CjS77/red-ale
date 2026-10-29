<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/employee.get
related: method/employee.list, method/employee.update
---
Returns one employee by GUID.

## Examples

```example fixture=synthetic/business-editable
{"method": "employee.get", "params": {"employee": "7f0d801fdccd48198abec7b197e55581"}}
{"result": {"guid": "7f0d801fdccd48198abec7b197e55581", "name": "Jane Doe", "username": "jdoe", "rate": {"num": "4250", "denom": "100"}, "…": "…"}}
```
