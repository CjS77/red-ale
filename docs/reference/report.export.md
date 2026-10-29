<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.export
related: method/report.run, method/report.styleSheets, method/report.exportCsv
---
Runs a report as [report.run](method/report.run) does and renders it as one self-contained HTML document through a style
sheet: `styleSheet` by name, or the report's own `General/Stylesheet` option. The HTML has no script and no external
resource, and is the same bytes for the same book, options, `now` and settings. Nothing is written to disk: the server
never writes to a path you name. The CLI's `--output` writes the `html` on your side.

An unknown `styleSheet` is a `report-option` error. A saved configuration that names a style sheet that has since been
deleted renders with the base look.

## Examples

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.export", "params": {"report": "Balance Sheet", "options": {"General": {"Balance Sheet Date": {"absolute": "1978-12-31"}}}}}
{"result": {"title": " Balance Sheet 12/31/78", "html": "…"}}
```
