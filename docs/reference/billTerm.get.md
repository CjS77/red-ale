<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/billTerm.get
related: method/billTerm.list, method/billTerm.update
---
Returns one billing term, by GUID or by the name of a visible one. `refcount` counts the customers, vendors and
documents using it; `parent` and `child` link it to its frozen copy (see [billTerm.list](method/billTerm.list)).

## Examples

```example fixture=synthetic/business-editable
{"method": "billTerm.get", "params": {"billTerm": "Prox15"}}
{"result": {"name": "Prox15", "type": "proximo", "dueDays": 15, "cutoff": 25, "…": "…"}}
```
