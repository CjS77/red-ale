<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.renameConfiguration
related: method/report.saveConfiguration, method/report.deleteConfiguration
---
Renames a saved configuration. A name another saved configuration has is refused with `conflict`, as GnuCash's dialog
refuses it.

## Examples

```example book=none
{"method": "report.saveConfiguration", "params": {"name": "March balance sheet", "report": "Balance Sheet"}}
{"result": "…"}
{"method": "report.renameConfiguration", "params": {"configuration": "March balance sheet", "name": "Month-end balance sheet"}}
{"result": {"name": "Month-end balance sheet", "kind": "saved", "…": "…"}}
```
