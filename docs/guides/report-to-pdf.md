<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/report-to-pdf
title: Turn a report into a PDF
summary: red-ale exports reports as HTML only; write the HTML to a file with report.export, then convert it with pandoc, wkhtmltopdf or a browser's print-to-PDF.
related: method/report.export, method/report.run, guide/run-and-export-a-report
from: manual/ch_Reports.docbook#report-print
---
GnuCash prints a report to a PDF file. red-ale does not: it has no PDF export and none is planned. A report is exported as
one self-contained HTML page ([report.export](method/report.export)), and a PDF is made from that page by a program built
for the job. The page has no script and no outside resources, so any converter reads it the same way.

## Steps

1. **Export the report to a file.** With the CLI, `--output` writes the page on your side:

   ```
   red-ale exec --book book.gnucash --output report.html report.export '{"report": "Balance Sheet"}'
   ```

   Over JSON-RPC or MCP, take the result's `html` and write it to a file yourself.
2. **Convert it** with any of these (install them from your system's packages; red-ale does not include them):
   - pandoc, which needs a PDF engine such as WeasyPrint or a LaTeX system installed:
     `pandoc report.html -o report.pdf --pdf-engine=weasyprint`
   - wkhtmltopdf: `wkhtmltopdf report.html report.pdf`
   - a headless browser: `chromium --headless --print-to-pdf=report.pdf report.html`
3. **Or print from a browser.** Open `report.html` and choose Print, then Save as PDF.
4. **Check the result.** The tools differ in how they treat wide tables and page breaks, so look at the PDF before you
   send it.

An invoice for a customer is the usual case; see [Print an invoice](guide/print-an-invoice).

## Example

The export that step 1 starts from: the page is in the result's `html`.

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.export", "params": {"report": "Balance Sheet", "options": {"General": {"Balance Sheet Date": {"absolute": "1978-12-31"}}}}}
{"result": {"title": " Balance Sheet 12/31/78", "html": "…"}}
```
