<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.run
related: method/report.options, method/report.list, method/report.export, method/report.saveConfiguration
---
Runs a report on the open book and returns its result: tables, charts and text, with exact figures (every amount is a
rational as well as a display string). Name the report by name or GUID, and change options with `options`, as
`{"Section": {"Name": value}}`, only the ones you want; [report.options](method/report.options) lists them. It reads the
book only: it works on a read-only book and never makes one dirty.

"Today" and relative dates such as `end-this-month` follow the session's clock, or `now` if you give it. Numbers and dates
are formatted in the Configuration's locale, or `locale`. A saved configuration is run by its name or GUID, and `options`
are applied over its saved ones. For HTML use [report.export](method/report.export); each report has its own
[Page](report/balance-sheet) with an example.

A report that cannot show anything says so in a `warning` block instead of failing, such as `No accounts selected` or
`No data`. Unknown options, options of the wrong type and unknown reports are errors; a name that matches a template and
a saved configuration is a `conflict` that lists the matches.

## Examples

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Balance Sheet", "options": {"General": {"Balance Sheet Date": {"absolute": "1978-12-31"}}}}}
{"result": {"title": " Balance Sheet 12/31/78", "report": {"name": "Balance Sheet", "…": "…"}, "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", {"role": "total", "cells": [{"text": "Total Assets", "…": "…"}, {"display": "$3,057.80", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
{"method": "report.run", "params": {"report": "Transaction Report"}}
{"result": {"title": "Transaction Report", "sections": [{"blocks": [{"kind": "warning", "title": "No accounts selected", "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
{"method": "report.run", "params": {"report": "Balance Sheet", "options": {"General": {"Colour": "red"}}}}
{"error": {"code": 3002, "message": "option General/Colour: unknown option", "data": {"section": "General", "name": "Colour", "reason": "unknown option", "help": "error/report-option"}}}
{"method": "report.run", "params": {"report": "Balance Shet"}}
{"error": {"code": 2000, "message": "report Balance Shet not found", "data": {"kind": "report", "id": "Balance Shet", "help": "error/not-found"}}}
```
