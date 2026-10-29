<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.list
related: method/report.options, method/report.run
---
Lists every report template (GnuCash's, red-ale's translations of them, and your own scripts) and every saved report
configuration, sorted by name, and the user scripts that failed to load (`problems`). Use it to find the name or GUID of
a report to [run](method/report.run). It needs no book. A template's `menuPath` and `menuName` are GnuCash's menu
fields; `exportTypes` says whether `report.exportCsv` works on it.

## Examples

```example book=none
{"method": "report.list", "params": {}}
{"result": {"reports": [{"guid": "3298541c236b494998b236dfad6ad752", "name": "Account Summary", "kind": "builtin", "script": "standard/account-summary.rhai", "…": "…"}, "…"], "problems": []}}
```
