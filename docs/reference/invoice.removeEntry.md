<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.removeEntry
related: method/invoice.addEntry, method/invoice.delete
---
Deletes a line from the documents and the order it is on, and destroys it. The document must be unposted and the order
open. The tax tables and billing term the line used lose a reference.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.removeEntry", "params": {"entry": "5eff8018de334b2084ef00f90330cd57"}}
{"result": {}}
{"method": "invoice.get", "params": {"invoice": "71803cac5d8e4dceabfd0d56ec5e7521"}}
{"result": {"entryCount": 0, "entries": [], "…": "…"}}
```
