<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/multicolumn-view
related: method/report.run, method/report.options, report/balance-sheet, report/income-statement
---
A page that holds other reports side by side, in a grid. Each child is a report template or a saved configuration,
with its own options and the number of columns and rows it spans. Use it to put a balance sheet and an income
statement on one page; [report.export](method/report.export) renders them together through one style sheet. In
GnuCash: Reports ▸ Multicolumn ▸ Custom Multicolumn Report.

The children are the option `__general/report-list`, an array of `{report, options, colspan, rowspan}` where `report`
is the GUID of a template or saved configuration (not a name) and `options` only goes with a template's GUID. A child
that fails, or that no longer exists, shows a warning in its cell instead of stopping the page. Views can nest to
four levels and a run can have 100 children.

## Examples

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Multicolumn View", "options": {"__general": {"report-list": [{"report": "c4173ac99b2b448289bf4d11c731af13"}, {"report": "0b81a3bdfd504aff849ec2e8630524bc"}]}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "grid", "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
