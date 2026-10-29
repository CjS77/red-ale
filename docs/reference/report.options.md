<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.options
related: method/report.run, method/report.list, method/report.saveConfiguration
---
Describes a report's options: each one's section, name, type, help text, default and current value, in GnuCash's order,
and the JSON Schema of the `options` param of [report.run](method/report.run). Look here for the names and types before
setting an option. A saved configuration shows its saved values.

Defaults that depend on the book, such as the account list, are worked out from the open book, or from an empty one if
none is open; with no book, accounts can only be given by GUID.

## Examples

```example book=none
{"method": "report.options", "params": {"report": "Net Worth Linechart"}}
{"result": {"report": {"name": "Net Worth Linechart", "kind": "builtin", "exportTypes": ["csv"], "…": "…"}, "options": [{"section": "Accounts", "name": "Accounts", "type": "accountList", "…": "…"}, "…"], "…": "…"}}
```
