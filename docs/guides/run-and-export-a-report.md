<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/run-and-export-a-report
title: Run and export a report
summary: Find a report, read its options, run it for exact figures, export it as HTML or CSV, save a set of options under a name, and choose a style sheet.
related: method/report.list, method/report.options, method/report.run, method/report.export, method/report.exportCsv, method/report.saveConfiguration, method/report.styleSheets, report/balance-sheet
---
A report summarises the book: a balance sheet, an income statement, a transaction listing. In red-ale a report is a
*template* with *options*. You choose a template, set the options you care about, and run it. The result is data: every
figure is exact and comes with its display form, so a program can use it and a person can read it. To read it as a page,
export it as HTML.

## Steps

1. **Find the report.** [report.list](method/report.list) lists the report templates, each with its name and GUID, and any
   saved configurations. The reports are described in the manual's *Reports* section (for example
   [Balance Sheet](report/balance-sheet)). `problems` lists user scripts that did not load.
2. **Read its options.** [report.options](method/report.options) gives each option's section, name, type, default and help
   text, and a JSON Schema for the `options` param.
3. **Run it** with [report.run](method/report.run). Give `options` only for the ones you change, as
   `{"Section": {"Name": value}}`. Dates are `{"absolute": "2024-03-31"}` or `{"relative": "end-this-month"}`. "Today" is
   the session's clock unless you give `now`. A wrong option name or value is refused with a [report-option](error/report-option)
   error that names it.
4. **Export it.** [report.export](method/report.export) gives the same report as one self-contained HTML page, with no
   script and no outside resource. [report.exportCsv](method/report.exportCsv) gives CSV for the reports that have one. The
   CLI's `--output` writes either to a file on your side.
5. **Save the options** you used with [report.saveConfiguration](method/report.saveConfiguration), under a new name. This is
   GnuCash's saved report configuration. Run it again by name with [report.run](method/report.run). Saved configurations
   live in red-ale's configuration, not in the book.
6. **Choose how it looks.** The `General/Stylesheet` option names a style sheet; [report.styleSheets](method/report.styleSheets)
   lists them. Pass `styleSheet` to [report.export](method/report.export) to use another for one export.
7. **To print on paper,** open the HTML in a browser, or [turn it into a PDF](guide/report-to-pdf).

## Example

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.options", "params": {"report": "Balance Sheet"}}
{"result": {"report": {"name": "Balance Sheet", "…": "…"}, "options": "…", "…": "…"}}
{"method": "report.run", "params": {"report": "Balance Sheet", "options": {"General": {"Balance Sheet Date": {"absolute": "1978-12-31"}}}}}
{"result": {"title": " Balance Sheet 12/31/78", "report": {"name": "Balance Sheet", "…": "…"}, "sections": "…", "…": "…"}}
{"method": "report.export", "params": {"report": "Balance Sheet", "options": {"General": {"Balance Sheet Date": {"absolute": "1978-12-31"}}}}}
{"result": {"title": " Balance Sheet 12/31/78", "html": "…"}}
{"method": "report.saveConfiguration", "params": {"name": "Balance sheet 1978", "report": "Balance Sheet", "options": {"General": {"Balance Sheet Date": {"absolute": "1978-12-31"}}}}}
{"result": {"name": "Balance sheet 1978", "kind": "saved", "…": "…"}}
{"method": "report.run", "params": {"report": "Balance sheet 1978"}}
{"result": {"title": " Balance Sheet 12/31/78", "…": "…"}}
```

A report with CSV output gives its figures as text:

```example fixture=reports/category-daily now=2024-03-15T12:00:00Z
{"method": "report.exportCsv", "params": {"report": "Income Over Time", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}, "Step Size": "WeekDelta"}}}}
{"result": {"title": "", "csv": "\"Date\",\"account-3\"\n\"2024-03-01\",28.0\n\"2024-03-08\",77.0\n\"2024-03-15\",126.0\n\"2024-03-22\",175.0\n\"2024-03-29\",90.0"}}
```
