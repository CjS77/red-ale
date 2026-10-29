<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.updateStyleSheet
related: method/report.createStyleSheet, method/report.styleSheets
---
Changes options of a style sheet; an option given as `null` goes back to its default. It works on the predefined style
sheets too, as GnuCash lets you edit `Default`. Image options already in the Configuration entry survive.

## Examples

```example book=none
{"method": "report.updateStyleSheet", "params": {"name": "Default", "options": {"Tables": {"Table cell padding": 6}}}}
{"result": {"name": "Default", "template": "Plain", "predefined": true, "options": ["…"]}}
```
