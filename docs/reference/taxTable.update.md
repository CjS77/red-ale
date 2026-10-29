<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/taxTable.update
related: method/taxTable.create, method/taxTable.get
---
Renames a tax table or replaces all its lines. Posted documents keep the frozen copy of the old lines, so changing a
table never changes a posted invoice or bill. Replacing the lines drops the table's cached frozen copy, so the next
posting makes a new one; renaming alone keeps it.

## Examples

```example fixture=synthetic/business-editable
{"method": "taxTable.update", "params": {"taxTable": "GST+Levy", "entries": [{"account": "Liabilities:Tax", "amount": "15", "type": "percent"}]}}
{"result": {"name": "GST+Levy", "entries": [{"accountName": "Liabilities:Tax", "amount": {"num": "15", "denom": "1"}, "…": "…"}], "…": "…"}}
```
