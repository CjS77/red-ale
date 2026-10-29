<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/securities
related: method/report.run, method/report.options, report/investment-portfolio, report/assets
---
A pie chart of the value of each security held at a date, largest first. Use it to see how a portfolio is spread. In
GnuCash: Reports ▸ Assets & Liabilities ▸ Security Piechart.

The options that matter are `Accounts/Accounts`, `General/End Date`, `General/Price Source` and `Display/Maximum
Slices`.

## Examples

```example fixture=reports/stock-test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Securities", "options": {"General": {"End Date": {"absolute": "1981-04-01"}}}}}
{"result": {"title": "Securities", "sections": [{"blocks": [{"kind": "chart", "type": "pie", "title": ["Securities", "Balance at 04/01/81: $9,999.00"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
