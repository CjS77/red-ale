<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.createStyleSheet
related: method/report.styleSheets, method/report.updateStyleSheet, method/report.export
---
Creates a style sheet from a style sheet template, with the options you give. It lives in red-ale's Configuration. A name
already used is a `conflict`; an unknown template is `not-found`. Option values are checked: CSS text that could pull in
anything outside the document (`@import`, `url(`, `</style`) is refused with `report-option`. Image options are kept in
the Configuration but not rendered.

## Examples

```example book=none
{"method": "report.createStyleSheet", "params": {"name": "Roomy", "template": "Plain", "options": {"Tables": {"Table cell padding": 6}}}}
{"result": {"name": "Roomy", "template": "Plain", "predefined": false, "options": ["…"]}}
{"method": "report.createStyleSheet", "params": {"name": "Roomy", "template": "Plain"}}
{"error": {"code": 2003, "message": "a style sheet is already called \"Roomy\"", "data": {"kind": "styleSheet", "name": "Roomy", "help": "error/conflict"}}}
```
