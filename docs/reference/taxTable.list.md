<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/taxTable.list
related: method/taxTable.get, method/taxTable.create
---
Lists the visible tax tables, by name. When a document is posted, GnuCash freezes a copy of each table its lines use,
so that later edits to the table do not change posted documents. These copies are invisible and are listed only with
`includeFrozen`; they have a `parent` and cannot be changed or deleted.

## Examples

```example fixture=synthetic/business-editable
{"method": "taxTable.list", "params": {}}
{"result": {"taxTables": [{"name": "GST+Levy", "invisible": false, "…": "…"}]}}
{"method": "taxTable.list", "params": {"includeFrozen": true}}
{"result": {"taxTables": [{"name": "GST+Levy", "invisible": false, "child": "dfe825b3f6ae4a6392c7eaeb0af670cd", "…": "…"}, {"guid": "dfe825b3f6ae4a6392c7eaeb0af670cd", "invisible": true, "parent": "489200ce534b46379dd206e300012248", "…": "…"}]}}
```
