<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.deleteStyleSheet
related: method/report.createStyleSheet, method/report.styleSheets
---
Deletes a style sheet you made. A predefined one cannot be deleted, so it goes back to its template's defaults instead
(`reset: true`), which is what GnuCash does on its next start. Saved configurations that name a deleted style sheet render
with the base look.

## Examples

```example book=none
{"method": "report.createStyleSheet", "params": {"name": "Roomy", "template": "Plain"}}
{"result": "…"}
{"method": "report.deleteStyleSheet", "params": {"name": "Roomy"}}
{"result": {"deleted": {"name": "Roomy", "…": "…"}, "reset": false}}
{"method": "report.deleteStyleSheet", "params": {"name": "Easy"}}
{"result": {"deleted": {"name": "Easy", "predefined": true, "…": "…"}, "reset": true}}
```
