<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/dashboard
related: report/multicolumn-view, report/expense-accounts, report/income-accounts, report/income-expense-chart, report/account-summary, method/report.run, method/report.saveConfiguration, method/report.list
---
GnuCash's Dashboard (Reports › Multicolumn › Dashboard): four reports on one page, in two columns. The expense
and income pie charts sit side by side, the Income/Expense Chart fills the next row and the Account Summary the
last, each with its own defaults. Use it for a quick look at where money comes from and goes, and what the
accounts hold.

The Dashboard is not a template but a **built-in saved configuration** of the
[Multicolumn View](report/multicolumn-view): "Report name" Dashboard, two columns, and the four children in
GnuCash's order and spans. [report.list](method/report.list) lists it with `kind: "saved"` and `readOnly: true`,
under GnuCash's menu path. Run it by its name or by its GUID, `ad80271c890b11dfa79f2dcedfd72085` (the GnuCash menu
item's).

## Changing it

`options` change the Dashboard for one run, as for any report. It can't be changed for good, renamed or deleted:
[report.updateConfiguration](method/report.updateConfiguration), `report.renameConfiguration` and
`report.deleteConfiguration` refuse it with [conflict](error/conflict) (`data.reason: "built-in"`). Copy it instead
with [report.saveConfiguration](method/report.saveConfiguration) under a name of your own, then change the copy.

Its name is not reserved. If you save or import a configuration also called "Dashboard" (GnuCash names a saved
dashboard after its "Report name"), the name matches both and is refused with `conflict`, its `data.matches` giving
both GUIDs; name each by its GUID.

## Key options

- **General › Number of columns**: 2. A row is closed once the column spans of its children reach it.
- **`__general` › `report-list`**: the four children, each a template GUID with its column and row spans: the
  Expense Accounts pie (1×1), the Income Accounts pie (1×1), the Income/Expense Chart (2×1) and the Account Summary
  (2×1). A child given no options runs on its template's defaults.

## Examples

The Dashboard of a fixture book: three rows, the two pies in the first.

```example fixture=reports/test-data
{"method": "report.run", "params": {"report": "Dashboard"}}
{"result": {"report": {"guid": "d8ba4a2e89e8479ca9f6eccdeb164588", "name": "Multicolumn View", "configuration": "ad80271c890b11dfa79f2dcedfd72085"}, "title": "", "sections": [{"blocks": [{"kind": "grid", "rows": [{"cells": [{"colspan": 1, "rowspan": 1, "result": {"title": "Expense Accounts", "…": "…"}, "…": "…"}, {"colspan": 1, "rowspan": 1, "result": {"title": "Income Accounts", "…": "…"}, "…": "…"}]}, {"cells": [{"colspan": 2, "rowspan": 1, "result": {"report": {"guid": "80769921e87943adade887b9835a7685", "name": "Income/Expense Chart", "configuration": null}, "…": "…"}, "…": "…"}]}, {"cells": [{"colspan": 2, "rowspan": 1, "result": {"report": {"guid": "3298541c236b494998b236dfad6ad752", "name": "Account Summary", "configuration": null}, "…": "…"}, "…": "…"}]}], "…": "…"}], "…": "…"}], "…": "…"}}
```

A copy to change, and the Dashboard itself refused:

```example fixture=reports/test-data
{"method": "report.saveConfiguration", "params": {"name": "My dashboard", "report": "Dashboard", "options": {"General": {"Number of columns": 3}}}}
{"result": {"name": "My dashboard", "kind": "saved", "template": "d8ba4a2e89e8479ca9f6eccdeb164588", "inMenu": false, "…": "…"}}
{"method": "report.updateConfiguration", "params": {"configuration": "Dashboard", "options": {"General": {"Number of columns": 3}}}}
{"error": {"code": 2003, "message": "…", "data": {"kind": "configuration", "reason": "built-in", "guid": "ad80271c890b11dfa79f2dcedfd72085", "name": "Dashboard", "help": "error/conflict"}}}
```
