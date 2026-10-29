<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/taxTable.get
related: method/taxTable.list, method/taxTable.update
---
Returns one tax table, by GUID or by the name of a visible one, with its lines and its `refcount`, the number of
customers, vendors and document lines using it. `parent` and `child` link a table to its frozen copy: see
[taxTable.list](method/taxTable.list).

## Examples

```example fixture=synthetic/business-editable
{"method": "taxTable.get", "params": {"taxTable": "GST+Levy"}}
{"result": {"guid": "489200ce534b46379dd206e300012248", "name": "GST+Levy", "refcount": 1, "entries": [{"accountName": "Liabilities:Levy", "type": "value", "…": "…"}, {"accountName": "Liabilities:Tax", "type": "percent", "…": "…"}], "…": "…"}}
```
