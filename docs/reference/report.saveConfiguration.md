<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.saveConfiguration
related: method/report.updateConfiguration, method/report.renameConfiguration, method/report.run
---
Saves a report's options under a name, as GnuCash's Save Report Configuration does, so that you can run it again by that
name. Only the options that differ from the template's defaults are stored. Saved configurations live in red-ale's
Configuration, not in the book, and this works with no book open; an account you give by full name needs an open book
to resolve, a GUID does not.

A name already taken is refused with `conflict`, where GnuCash appends a number. Run a saved configuration with
[report.run](method/report.run) using its name or GUID.

## Examples

```example book=none
{"method": "report.saveConfiguration", "params": {"name": "March balance sheet", "report": "Balance Sheet", "options": {"General": {"Balance Sheet Date": {"absolute": "2024-03-31"}}}}}
{"result": {"guid": "…", "name": "March balance sheet", "kind": "saved", "template": "c4173ac99b2b448289bf4d11c731af13", "inMenu": false, "…": "…"}}
{"method": "report.saveConfiguration", "params": {"name": "March balance sheet", "report": "Balance Sheet"}}
{"error": {"code": 2003, "message": "a saved report configuration is already called \"March balance sheet\"", "data": {"kind": "configuration", "name": "March balance sheet", "help": "error/conflict"}}}
```
