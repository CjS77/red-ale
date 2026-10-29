<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.deleteConfiguration
related: method/report.saveConfiguration, method/report.list
---
Deletes a saved configuration and returns it as it was. Report templates cannot be deleted. It only changes the
Configuration, not the book.

## Examples

```example book=none
{"method": "report.saveConfiguration", "params": {"name": "March balance sheet", "report": "Balance Sheet"}}
{"result": "…"}
{"method": "report.deleteConfiguration", "params": {"configuration": "March balance sheet"}}
{"result": {"deleted": {"name": "March balance sheet", "kind": "saved", "…": "…"}}}
{"method": "report.deleteConfiguration", "params": {"configuration": "March balance sheet"}}
{"error": {"code": 2000, "message": "…", "data": {"kind": "configuration", "help": "error/not-found", "…": "…"}}}
```
